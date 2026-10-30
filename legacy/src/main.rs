//! diskgc — умная фоновая уборка диска для dev-машины на macOS.
//!
//! Что убирает: артефакты сборки (Rust target/, node_modules, .dart_tool, ...), временные каталоги
//! AI-агентов, сирот Docker от тестов, старые версии Codex, кэши. Что сжимает (прозрачно, APFS):
//! холодные логи и истории сессий. Исходники, .git, базы, историю чатов — не трогает.

mod compress;
mod config;
mod daemon;
mod docker;
mod fsutil;
mod history;
mod launchd;
mod mem;
mod report;
mod rules;

use anyhow::Result;
use clap::{Parser, Subcommand};
use std::io::Write;
use std::path::PathBuf;

use config::{Config, Level};

#[derive(Parser)]
#[command(name = "diskgc", version, about = "Умная уборка диска: report / clean / daemon")]
struct Cli {
    #[command(subcommand)]
    cmd: Option<Cmd>,
}

#[derive(Subcommand)]
enum Cmd {
    /// Что и сколько занимает (категории + топ виновников)
    Report,
    /// Убрать мусор по правилам конфига
    Clean {
        /// Только показать план, ничего не удалять
        #[arg(short = 'n', long)]
        dry_run: bool,
        /// Уровень агрессивности: normal | low | critical (по умолчанию — от свободного места)
        #[arg(long)]
        level: Option<String>,
    },
    /// Фоновый режим: следить за местом и чистить по необходимости (запускается launchd)
    Daemon,
    /// Состояние демона, свободное место, последняя чистка
    Status,
    /// Как копится место по дням и категориям
    Stats,
    /// Последние чистки: что и почему удалялось
    History,
    /// Установить демон в автозапуск (launchd, KeepAlive)
    Install,
    /// Убрать демон из автозапуска
    Uninstall,
    /// Путь к конфигу (и открыть его)
    Config,
    /// RAM: кто ест память, процессы-сироты от старых сессий
    Mem {
        /// Завершить (SIGTERM) сирот старше 12 ч
        #[arg(long)]
        kill_orphans: bool,
    },
}

pub fn log_path() -> PathBuf {
    fsutil::home().join("Library/Logs/diskgc.log")
}

/// В stdout и в лог-файл.
pub fn log(line: &str) {
    println!("{line}");
    if let Ok(mut f) = std::fs::OpenOptions::new().create(true).append(true).open(log_path()) {
        let _ = writeln!(f, "{} {}", fsutil::now_string(), line);
    }
}

pub mod clean {
    use super::*;

    pub struct Summary {
        pub freed: u64,
        pub actions: usize,
    }

    /// Собрать план → показать/выполнить → Docker → записать в историю.
    pub fn run(cfg: &Config, level: Level, dry: bool, from_daemon: bool) -> Summary {
        let eff = cfg.effective(level);
        let started = std::time::Instant::now();
        log(&format!(
            "=== clean [{}{}] свободно {:.0} ГБ ===",
            level.name(),
            if dry { ", dry-run" } else { "" },
            fsutil::gb(fsutil::free_bytes())
        ));
        let mut plan = rules::build_plan(cfg, &eff);
        plan.actions.sort_by(|a, b| b.bytes.cmp(&a.bytes));
        let out = rules::execute(&plan, dry);
        for l in &out.lines {
            log(l);
        }
        let d = docker::clean(cfg, &eff, dry);
        for l in &d.lines {
            log(l);
        }
        let freed = out.freed + d.freed;
        let free_after = fsutil::gb(fsutil::free_bytes());
        log(&format!(
            "=== итог: {} {:.1} ГБ, действий {}, свободно {:.0} ГБ, {:.0} с ===",
            if dry { "можно освободить" } else { "освобождено" },
            fsutil::gb(freed),
            plan.actions.len(),
            free_after,
            started.elapsed().as_secs_f64()
        ));
        let top: Vec<history::ActionRec> = plan
            .actions
            .iter()
            .filter(|a| !matches!(a.kind, rules::Kind::Compress))
            .take(15)
            .map(|a| history::ActionRec { path: a.path.display().to_string(), gb: fsutil::gb(a.bytes), reason: a.reason.clone() })
            .collect();
        let used = fsutil::disk_space(std::path::Path::new(fsutil::DATA_VOLUME)).map(|t| fsutil::gb(t.1)).unwrap_or(0.0);
        let _ = history::append(&history::Record {
            ts: fsutil::now_string(),
            kind: "clean".into(),
            free_gb: free_after,
            used_gb: used,
            level: Some(level.name().into()),
            dry: Some(dry),
            freed_gb: Some(fsutil::gb(freed)),
            action_count: Some(plan.actions.len()),
            categories: None,
            top_actions: Some(top),
        });
        // размеры категорий — для статистики «как копится»; из демона и из обычной чистки
        if !dry || from_daemon {
            let cats = report::collect(cfg);
            let _ = history::append(&history::Record {
                ts: fsutil::now_string(),
                kind: "categories".into(),
                free_gb: fsutil::gb(fsutil::free_bytes()),
                used_gb: used,
                level: None,
                dry: None,
                freed_gb: None,
                action_count: None,
                categories: Some(cats.sizes),
                top_actions: None,
            });
        }
        Summary { freed, actions: plan.actions.len() }
    }
}

fn auto_level(cfg: &Config) -> Level {
    let free = fsutil::gb(fsutil::free_bytes());
    if free < cfg.thresholds.critical_gb {
        Level::Critical
    } else if free < cfg.thresholds.low_gb {
        Level::Low
    } else {
        Level::Normal
    }
}

fn main() -> Result<()> {
    // `diskgc report | head` не должен паниковать по SIGPIPE
    unsafe {
        libc::signal(libc::SIGPIPE, libc::SIG_DFL);
    }
    let cli = Cli::parse();
    let cfg = Config::load()?;
    match cli.cmd.unwrap_or(Cmd::Report) {
        Cmd::Report => report::print(&cfg),
        Cmd::Clean { dry_run, level } => {
            let level = match level {
                Some(s) => Level::parse(&s).ok_or_else(|| anyhow::anyhow!("level: normal | low | critical"))?,
                None => auto_level(&cfg),
            };
            clean::run(&cfg, level, dry_run, false);
        }
        Cmd::Daemon => daemon::run(),
        Cmd::Status => {
            launchd::status();
            if let Ok((total, used, avail)) = fsutil::disk_space(std::path::Path::new(fsutil::DATA_VOLUME)) {
                println!(
                    "Диск: всего {:.0} ГБ, занято {:.0} ГБ, свободно {:.0} ГБ (уровень: {})",
                    fsutil::gb(total),
                    fsutil::gb(used),
                    fsutil::gb(avail),
                    auto_level(&cfg).name()
                );
            }
            println!(
                "Пороги: comfortable {:.0} / low {:.0} / critical {:.0} ГБ; тик {} мин; гигиена каждые {} ч",
                cfg.thresholds.comfortable_gb,
                cfg.thresholds.low_gb,
                cfg.thresholds.critical_gb,
                cfg.daemon.tick_minutes,
                cfg.daemon.hygiene_every_hours
            );
            if let Some(f) = history::forecast_line(fsutil::gb(fsutil::free_bytes()), cfg.thresholds.critical_gb) {
                println!("Прогноз: {}", f);
            }
            history::print_history(1);
        }
        Cmd::Mem { kill_orphans } => mem::report(kill_orphans),
        Cmd::Stats => history::print_stats(),
        Cmd::History => history::print_history(10),
        Cmd::Install => launchd::install()?,
        Cmd::Uninstall => launchd::uninstall()?,
        Cmd::Config => {
            let p = config::config_path();
            println!("{}", p.display());
            let _ = std::process::Command::new("open").arg("-t").arg(&p).status();
        }
    }
    Ok(())
}
