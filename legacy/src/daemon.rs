//! Демон: живёт постоянно, каждые tick минут смотрит на свободное место и решает,
//! что делать. Много места — только плановая гигиена раз в N часов. Мало — чистит сразу,
//! чем меньше места, тем короче сроки «свежести» (уровни normal / low / critical).

use std::time::{Duration, Instant};

use crate::config::{Config, Level};
use crate::fsutil::{free_bytes, gb, GB};
use crate::{clean, log};

pub fn run() -> ! {
    log("daemon: старт");
    let mut last_clean: Option<Instant> = None;
    let mut last_level = Level::Normal;
    let mut first = true;
    loop {
        let cfg = match Config::load() {
            Ok(c) => c,
            Err(e) => {
                log(&format!("daemon: конфиг не читается ({e}), использую дефолт"));
                Config::default()
            }
        };
        let free = free_bytes();
        let free_gb = gb(free);
        let t = &cfg.thresholds;
        let mut level = if free_gb < t.critical_gb {
            Level::Critical
        } else if free_gb < t.low_gb {
            Level::Low
        } else {
            Level::Normal
        };
        // прогноз: если при текущей скорости роста за сутки упрёмся в порог — чистим заранее
        if let Some(rate) = crate::history::growth_rate_gb_per_hour(6.0) {
            if rate > 0.05 {
                let in_24h = free_gb - rate * 24.0;
                let predicted = if in_24h < t.critical_gb { Level::Low } else if in_24h < t.low_gb { Level::Normal } else { level };
                if predicted > level {
                    log(&format!("daemon: прогноз {:+.2} ГБ/ч → через 24 ч ~{:.0} ГБ свободно, поднимаю уровень до {}", rate, in_24h, predicted.name()));
                    level = predicted;
                }
            }
        }
        // как часто можно чистить на этом уровне
        let min_gap = match level {
            Level::Critical => Duration::from_secs(10 * 60),
            Level::Low => Duration::from_secs(30 * 60),
            Level::Normal => {
                if free_gb < t.comfortable_gb {
                    Duration::from_secs(60 * 60)
                } else {
                    Duration::from_secs(cfg.daemon.hygiene_every_hours * 3600)
                }
            }
        };
        let due = match last_clean {
            None => first || free_gb < t.comfortable_gb,
            Some(at) => at.elapsed() >= min_gap || level > last_level,
        };
        let _ = crate::history::append(&crate::history::Record {
            ts: crate::fsutil::now_string(),
            kind: "snapshot".into(),
            free_gb,
            used_gb: used_gb(),
            level: Some(level.name().into()),
            dry: None,
            freed_gb: None,
            action_count: None,
            categories: None,
            top_actions: None,
        });
        if due {
            log(&format!("daemon: свободно {:.0} ГБ → чистка уровня {}", free_gb, level.name()));
            let summary = clean::run(&cfg, level, false, true);
            if cfg.daemon.notify && (summary.freed as f64 > 0.5 * GB || level == Level::Critical) {
                notify(&format!(
                    "{}: освобождено {:.1} ГБ, свободно {:.0} ГБ",
                    level.name(),
                    gb(summary.freed),
                    gb(free_bytes())
                ));
            }
            last_clean = Some(Instant::now());
            last_level = level;
        }
        first = false;
        rotate_log();
        std::thread::sleep(Duration::from_secs(cfg.daemon.tick_minutes.max(1) * 60));
    }
}

fn used_gb() -> f64 {
    crate::fsutil::disk_space(std::path::Path::new(crate::fsutil::DATA_VOLUME)).map(|t| gb(t.1)).unwrap_or(0.0)
}

pub fn notify(msg: &str) {
    let script = format!("display notification \"{}\" with title \"diskgc\"", msg.replace('"', "'"));
    let _ = std::process::Command::new("osascript").arg("-e").arg(script).output();
}

fn rotate_log() {
    let p = crate::log_path();
    if let Ok(m) = std::fs::metadata(&p) {
        if m.len() > 20 * 1024 * 1024 {
            let _ = std::fs::rename(&p, p.with_extension("log.1"));
        }
    }
}
