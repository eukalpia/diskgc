//! The daemon loop. Wakes every `tick_minutes`, records a snapshot, picks a level from free space
//! and the forecast, and cleans when due. Everything it does is reported through `events`.

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, RwLock};
use std::time::{Duration, Instant};

use crate::bytes::{from_gib, gib, human};
use crate::engine::Engine;
use crate::model::{Level, Progress};
use crate::protocol::Event;

pub type EventSink = Arc<dyn Fn(Event) + Send + Sync>;

pub struct Daemon {
    pub engine: Arc<RwLock<Engine>>,
    /// Held while any run (plan, execute, clean, report) is in progress, by the loop or by IPC clients.
    pub run_lock: Arc<Mutex<()>>,
    pub events: EventSink,
    pub stop: Arc<AtomicBool>,
    /// Set by the IPC server while a client-initiated run is in progress, so the loop waits.
    pub busy: Arc<AtomicBool>,
}

impl Daemon {
    fn log(&self, level: &str, message: String) {
        match level {
            "warn" => tracing::warn!("{message}"),
            _ => tracing::info!("{message}"),
        }
        (self.events)(Event::Log {
            level: level.into(),
            message,
        });
    }

    pub fn run(&self) {
        self.log("info", "daemon started".into());
        let mut last_clean: Option<Instant> = None;
        let mut last_level = Level::Normal;
        let mut first = true;
        while !self.stop.load(Ordering::Relaxed) {
            let tick = {
                {
                    let mut eng = self.engine.write().unwrap();
                    if let Err(e) = eng.reload() {
                        self.log(
                            "warn",
                            format!("config not reloaded, keeping the previous one: {e}"),
                        );
                    }
                }
                let eng = self.engine.read().unwrap();
                let cfg = eng.config().clone();
                let disk = eng.snapshot();
                (self.events)(Event::Snapshot {
                    used: disk.used,
                    available: disk.available,
                    level: disk.level,
                });
                let mut level = disk.level;
                let t = &cfg.thresholds;
                if let Some(f) = eng.forecast() {
                    let predicted = if f.available_in_24h < from_gib(t.critical_gb) {
                        Level::Low
                    } else if f.available_in_24h < from_gib(t.low_gb) {
                        Level::Normal
                    } else {
                        level
                    };
                    if predicted > level {
                        self.log(
                            "info",
                            format!(
                                "forecast: {}/h growth, ~{} free in 24 h; raising level to {}",
                                human(f.rate_bytes_per_hour as u64),
                                human(f.available_in_24h),
                                predicted.name()
                            ),
                        );
                        level = predicted;
                    }
                }
                let min_gap = match level {
                    Level::Critical => Duration::from_secs(10 * 60),
                    Level::Low => Duration::from_secs(30 * 60),
                    Level::Normal if gib(disk.available) < t.comfortable_gb => {
                        Duration::from_secs(60 * 60)
                    }
                    Level::Normal => Duration::from_secs(cfg.daemon.hygiene_every_hours * 3600),
                };
                let due = match last_clean {
                    None => first || gib(disk.available) < t.comfortable_gb,
                    Some(at) => at.elapsed() >= min_gap || level > last_level,
                };
                if due && !self.busy.load(Ordering::Relaxed) {
                    // Never run two cleans at once; an IPC client may be mid-run.
                    let _run = self.run_lock.lock().unwrap();
                    self.log(
                        "info",
                        format!(
                            "{} free, running a {} clean",
                            human(disk.available),
                            level.name()
                        ),
                    );
                    let events = self.events.clone();
                    let (_, summary) = eng.clean(level, false, |p| {
                        if let Progress::Finished { .. } = &p {
                            return;
                        }
                        events(Event::Progress(p));
                    });
                    (self.events)(Event::RunFinished(summary.clone()));
                    let freed = summary.freed_bytes + summary.docker_freed_bytes;
                    self.log(
                        "info",
                        format!(
                            "run finished: {} freed, {} actions, {} free now",
                            human(freed),
                            summary.action_count,
                            human(summary.free_after)
                        ),
                    );
                    if cfg.daemon.notify && (freed > 512 * 1024 * 1024 || level == Level::Critical)
                    {
                        crate::platform::notify(
                            "diskgc",
                            &format!(
                                "{}: freed {}, {} free",
                                level.name(),
                                human(freed),
                                human(summary.free_after)
                            ),
                        );
                    }
                    let _ = eng.report(|_| {});
                    last_clean = Some(Instant::now());
                    last_level = level;
                }
                if cfg.mem.automation_browser_max_hours > 0 {
                    let killed =
                        eng.kill_automation_browsers(cfg.mem.automation_browser_max_hours as f64);
                    if !killed.is_empty() {
                        self.log(
                            "info",
                            format!(
                                "terminated {} automation browser(s) older than {} h: {:?}",
                                killed.len(),
                                cfg.mem.automation_browser_max_hours,
                                killed
                            ),
                        );
                    }
                }
                first = false;
                Duration::from_secs(cfg.daemon.tick_minutes.max(1) * 60)
            };
            // Sleep in small steps so `stop` is honoured quickly.
            let until = Instant::now() + tick;
            while Instant::now() < until && !self.stop.load(Ordering::Relaxed) {
                std::thread::sleep(Duration::from_secs(1));
            }
        }
        self.log("info", "daemon stopped".into());
    }
}
