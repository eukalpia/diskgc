//! The facade every front-end uses: CLI, daemon, IPC server, tests.

use std::path::PathBuf;

use anyhow::Result;
use serde::{Deserialize, Serialize};

use crate::config::Config;
use crate::executor::Executor;
use crate::forecast::Forecast;
use crate::history::{ActionRec, History, Stats};
use crate::model::{ActionKind, DiskStatus, Level, Plan, Progress, RunSummary};
use crate::platform::{self, AutostartStatus};
use crate::report::{self, Report};
use crate::{bytes, mem, paths, rules};

pub struct Engine {
    cfg: Config,
    config_path: PathBuf,
    history: History,
    /// The last full report and when it was computed. A report walks every cache and project
    /// tree, so callers that can live with slightly stale numbers (the dashboard) reuse it.
    report_cache: std::sync::Mutex<Option<(std::time::Instant, Report)>>,
}

/// How long a cached report stays good for `report_cached`.
pub const REPORT_CACHE_TTL: std::time::Duration = std::time::Duration::from_secs(15 * 60);

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Status {
    pub version: String,
    pub disk: DiskStatus,
    pub thresholds_gb: (f64, f64, f64),
    pub autostart: AutostartStatus,
    pub forecast: Option<Forecast>,
    pub last_run: Option<crate::history::Record>,
    pub config_path: PathBuf,
    pub log_path: PathBuf,
    pub compression_supported: bool,
}

impl Engine {
    pub fn load() -> Result<Self> {
        Self::load_from(paths::config_file())
    }

    pub fn load_from(config_path: PathBuf) -> Result<Self> {
        let cfg = Config::load_from(&config_path)?;
        Ok(Self {
            cfg,
            config_path,
            history: History::default(),
            report_cache: std::sync::Mutex::new(None),
        })
    }

    pub fn with_config(cfg: Config) -> Self {
        Self {
            cfg,
            config_path: paths::config_file(),
            history: History::default(),
            report_cache: std::sync::Mutex::new(None),
        }
    }

    pub fn config(&self) -> &Config {
        &self.cfg
    }

    pub fn config_path(&self) -> &PathBuf {
        &self.config_path
    }

    pub fn history(&self) -> &History {
        &self.history
    }

    /// Re-read the config file; keeps the old config if the new one does not parse.
    pub fn reload(&mut self) -> Result<()> {
        self.cfg = Config::load_from(&self.config_path)?;
        Ok(())
    }

    pub fn replace_config(&mut self, toml_text: &str) -> Result<()> {
        let cfg = Config::parse(toml_text)?;
        cfg.save_to(&self.config_path)?;
        self.cfg = cfg;
        Ok(())
    }

    pub fn config_toml(&self) -> Result<String> {
        Ok(std::fs::read_to_string(&self.config_path)?)
    }

    pub fn disk(&self) -> DiskStatus {
        let volume = paths::watched_volume();
        let (total, used, available) = platform::disk_space(&volume).unwrap_or((0, 0, 0));
        DiskStatus {
            volume,
            total,
            used,
            available,
            level: self.cfg.level_for(available),
        }
    }

    pub fn auto_level(&self) -> Level {
        self.disk().level
    }

    pub fn plan(&self, level: Level, mut progress: impl FnMut(Progress)) -> Plan {
        rules::build_plan(
            &self.cfg,
            level,
            rules::PlanHints {
                skip_commands: self.history.fruitless_commands(7),
            },
            |rule| {
                progress(Progress::Scanning {
                    rule: rule.to_string(),
                })
            },
        )
    }

    /// Execute a plan and record it. `selection` restricts execution to the given action ids.
    pub fn execute(
        &self,
        plan: &Plan,
        selection: Option<&[u32]>,
        dry_run: bool,
        progress: impl FnMut(Progress),
    ) -> RunSummary {
        let summary = Executor::new(&self.cfg).run(plan, selection, dry_run, progress);
        let top: Vec<ActionRec> = plan
            .actions
            .iter()
            .filter(|a| {
                a.kind != ActionKind::Compress
                    && selection.map(|s| s.contains(&a.id)).unwrap_or(true)
            })
            .take(15)
            .map(|a| ActionRec {
                path: a.path.display().to_string(),
                bytes: a.bytes,
                reason: a.reason.clone(),
                group: a.group.name().into(),
                freed: if dry_run {
                    None
                } else {
                    summary
                        .results
                        .iter()
                        .find(|r| r.id == a.id)
                        // A failed action freed an unknown amount: don't let it read as
                        // "freed nothing" and put its command on cooldown.
                        .and_then(|r| r.ok.then_some(r.freed))
                },
                command: match &a.kind {
                    ActionKind::Command { argv, .. } => Some(argv.join(" ")),
                    _ => None,
                },
            })
            .collect();
        if let Err(e) = self.history.record_run(&summary, top) {
            tracing::warn!("could not record run: {e}");
        }
        summary
    }

    pub fn clean(
        &self,
        level: Level,
        dry_run: bool,
        mut progress: impl FnMut(Progress),
    ) -> (Plan, RunSummary) {
        let plan = self.plan(level, &mut progress);
        let summary = self.execute(&plan, None, dry_run, &mut progress);
        (plan, summary)
    }

    pub fn report(&self, on_step: impl FnMut(&str)) -> Report {
        let r = report::collect(&self.cfg, on_step);
        if let Err(e) = self.history.record_categories(r.categories.clone()) {
            tracing::warn!("could not record categories: {e}");
        }
        *self.report_cache.lock().unwrap() = Some((std::time::Instant::now(), r.clone()));
        r
    }

    /// The last report if it is younger than [`REPORT_CACHE_TTL`], otherwise a fresh one.
    pub fn report_cached(&self, on_step: impl FnMut(&str)) -> Report {
        if let Some((at, r)) = self.report_cache.lock().unwrap().as_ref() {
            if at.elapsed() < REPORT_CACHE_TTL {
                return r.clone();
            }
        }
        self.report(on_step)
    }

    pub fn snapshot(&self) -> DiskStatus {
        let d = self.disk();
        if let Err(e) = self.history.record_snapshot(d.used, d.available, d.level) {
            tracing::warn!("could not record snapshot: {e}");
        }
        d
    }

    pub fn forecast(&self) -> Option<Forecast> {
        let d = self.disk();
        self.history.forecast(
            6.0,
            d.available,
            bytes::from_gib(self.cfg.thresholds.critical_gb),
        )
    }

    pub fn stats(&self) -> Stats {
        self.history.stats()
    }

    pub fn status(&self) -> Status {
        let t = &self.cfg.thresholds;
        Status {
            version: env!("CARGO_PKG_VERSION").to_string(),
            disk: self.disk(),
            thresholds_gb: (t.comfortable_gb, t.low_gb, t.critical_gb),
            autostart: platform::autostart_status(),
            forecast: self.forecast(),
            last_run: self.history.runs(1).pop(),
            config_path: self.config_path.clone(),
            log_path: paths::log_file(),
            compression_supported: platform::compression_supported(),
        }
    }

    pub fn memory(&self) -> mem::MemReport {
        mem::report(self.cfg.docker.enable)
    }

    pub fn kill_orphans(&self) -> Vec<u32> {
        mem::kill_orphans()
    }

    pub fn kill_automation_browsers(&self, max_hours: f64) -> Vec<u32> {
        mem::kill_automation_browsers(max_hours)
    }
}
