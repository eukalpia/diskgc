//! Applies a plan. Measures what was actually freed rather than trusting estimates, refuses to
//! touch paths outside the allowed prefixes, and reports progress per action.

use std::path::{Path, PathBuf};
use std::time::{Instant, SystemTime};

use crate::config::{Config, DeleteMode};
use crate::fs::{self, days};
use crate::model::{Action, ActionKind, ActionResult, Plan, Progress, RunSummary};
use crate::paths;
use crate::platform::{self, CompressOutcome};
use crate::rules;

/// External cleanup commands must not stall a run: `uv cache clean`, for one, blocks for as long
/// as another uv process holds the cache lock.
const COMMAND_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(90);

/// Like `Command::output`, but kills the child and returns an error after `timeout`.
fn run_with_timeout(
    argv: &[String],
    timeout: std::time::Duration,
) -> std::io::Result<std::process::Output> {
    use std::io::Read;
    let mut child = std::process::Command::new(&argv[0])
        .args(&argv[1..])
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::piped())
        .spawn()?;
    let start = Instant::now();
    loop {
        if let Some(status) = child.try_wait()? {
            let mut stderr = Vec::new();
            if let Some(mut e) = child.stderr.take() {
                let _ = e.read_to_end(&mut stderr);
            }
            return Ok(std::process::Output {
                status,
                stdout: Vec::new(),
                stderr,
            });
        }
        if start.elapsed() > timeout {
            let _ = child.kill();
            let _ = child.wait();
            return Err(std::io::Error::new(
                std::io::ErrorKind::TimedOut,
                format!("timed out after {} s", timeout.as_secs()),
            ));
        }
        std::thread::sleep(std::time::Duration::from_millis(200));
    }
}

pub struct Executor<'a> {
    cfg: &'a Config,
    allowed: Vec<PathBuf>,
}

impl<'a> Executor<'a> {
    pub fn new(cfg: &'a Config) -> Self {
        let mut allowed: Vec<PathBuf> = cfg
            .safety
            .allowed_prefixes
            .iter()
            .map(|p| paths::expand(p))
            .collect();
        if allowed.is_empty() {
            allowed.push(paths::home());
            allowed.push(platform::system_tmp_dir());
            allowed.push(std::env::temp_dir());
            if let Some(c) = platform::chrome_clone_dir() {
                allowed.push(c);
            }
            if let Some(s) = platform::claude_scratch_root() {
                allowed.push(s);
            }
        }
        let allowed = allowed
            .into_iter()
            .map(|p| p.canonicalize().unwrap_or(p))
            .collect();
        Self { cfg, allowed }
    }

    fn allowed(&self, p: &Path) -> bool {
        let c = p.canonicalize().unwrap_or_else(|_| p.to_path_buf());
        self.allowed.iter().any(|a| c.starts_with(a))
    }

    /// Execute `plan` (or only the actions whose ids are in `selection`). With `dry_run` nothing is
    /// touched and estimates are reported as results.
    pub fn run(
        &self,
        plan: &Plan,
        selection: Option<&[u32]>,
        dry_run: bool,
        mut progress: impl FnMut(Progress),
    ) -> RunSummary {
        let started = Instant::now();
        let mut summary = RunSummary {
            level: plan.level,
            dry_run,
            started_at: crate::history::now(),
            ..Default::default()
        };
        let chosen: Vec<&Action> = plan
            .actions
            .iter()
            .filter(|a| selection.map(|s| s.contains(&a.id)).unwrap_or(true))
            .collect();
        summary.action_count = chosen.len();
        progress(Progress::Planned {
            actions: chosen.len(),
            estimated_bytes: chosen
                .iter()
                .filter(|a| a.kind != ActionKind::Compress)
                .map(|a| a.bytes)
                .sum(),
        });
        for (i, a) in chosen.iter().enumerate() {
            progress(Progress::Executing {
                index: i + 1,
                total: chosen.len(),
                action: (*a).clone(),
            });
            let result = if dry_run {
                ActionResult {
                    id: a.id,
                    freed: if a.kind == ActionKind::Compress {
                        0
                    } else {
                        a.bytes
                    },
                    ok: true,
                    error: None,
                }
            } else {
                self.apply(a)
            };
            summary.freed_bytes += result.freed;
            progress(Progress::Executed {
                result: result.clone(),
            });
            summary.results.push(result);
        }
        if let Some(d) = &plan.docker {
            if d.available && (selection.is_none() || !chosen.is_empty()) && !dry_run {
                let eff = self
                    .cfg
                    .effective(plan.level.unwrap_or(crate::model::Level::Normal));
                let out = crate::docker::clean(self.cfg, &eff);
                summary.docker_freed_bytes = out.freed;
                for l in &out.lines {
                    progress(Progress::Docker { line: l.clone() });
                }
                summary.docker_lines = out.lines;
            }
        }
        summary.duration_secs = started.elapsed().as_secs_f64();
        summary.free_after = platform::disk_space(&paths::watched_volume())
            .map(|t| t.2)
            .unwrap_or(0);
        progress(Progress::Finished {
            summary: summary.clone(),
        });
        summary
    }

    fn apply(&self, a: &Action) -> ActionResult {
        let fail = |e: String| ActionResult {
            id: a.id,
            freed: 0,
            ok: false,
            error: Some(e),
        };
        if !matches!(a.kind, ActionKind::Command { .. }) && !self.allowed(&a.path) {
            return fail(format!(
                "refused: {} is outside the allowed prefixes",
                a.path.display()
            ));
        }
        match &a.kind {
            ActionKind::Remove => {
                let before = fs::disk_usage(&a.path);
                match self.remove(&a.path) {
                    Ok(()) => ActionResult {
                        id: a.id,
                        freed: before,
                        ok: true,
                        error: None,
                    },
                    Err(e) => fail(e.to_string()),
                }
            }
            ActionKind::SweepOlderThan { days: d } => {
                let cutoff = SystemTime::now() - days(*d);
                let freed = fs::sweep(&a.path, |_, m| {
                    m.modified().map(|t| t < cutoff).unwrap_or(false)
                });
                ActionResult {
                    id: a.id,
                    freed,
                    ok: true,
                    error: None,
                }
            }
            ActionKind::SweepBigBinaries => {
                let freed = fs::sweep(&a.path, rules::is_big_binary);
                ActionResult {
                    id: a.id,
                    freed,
                    ok: true,
                    error: None,
                }
            }
            ActionKind::Compress => match platform::compress_file(&a.path) {
                Ok(CompressOutcome::Compressed { before, after }) => ActionResult {
                    id: a.id,
                    freed: before.saturating_sub(after),
                    ok: true,
                    error: None,
                },
                Ok(CompressOutcome::NotWorthIt) => {
                    platform::set_nocompress_marker(&a.path);
                    ActionResult {
                        id: a.id,
                        freed: 0,
                        ok: true,
                        error: None,
                    }
                }
                Ok(CompressOutcome::Unsupported) => {
                    fail("compression not supported on this platform".into())
                }
                Err(e) => fail(e.to_string()),
            },
            ActionKind::Command { argv, measure } => {
                let before = measure.as_ref().map(|m| fs::disk_usage(m)).unwrap_or(0);
                let out = run_with_timeout(argv, COMMAND_TIMEOUT);
                match out {
                    Ok(o) if o.status.success() => {
                        let after = measure.as_ref().map(|m| fs::disk_usage(m)).unwrap_or(0);
                        ActionResult {
                            id: a.id,
                            freed: before.saturating_sub(after),
                            ok: true,
                            error: None,
                        }
                    }
                    Ok(o) => fail(format!(
                        "{} exited with {}: {}",
                        argv[0],
                        o.status,
                        String::from_utf8_lossy(&o.stderr).trim()
                    )),
                    Err(e) => fail(format!("{}: {e}", argv[0])),
                }
            }
        }
    }

    fn remove(&self, p: &Path) -> anyhow::Result<()> {
        match self.cfg.safety.delete_mode {
            DeleteMode::Trash => move_to_trash(p),
            DeleteMode::Permanent => {
                if p.is_dir() {
                    std::fs::remove_dir_all(p)?;
                } else {
                    std::fs::remove_file(p)?;
                }
                Ok(())
            }
        }
    }
}

/// Move to the user's trash folder. Cheap on the same volume (a rename); falls back to permanent
/// deletion across volumes.
fn move_to_trash(p: &Path) -> anyhow::Result<()> {
    let trash = if cfg!(target_os = "macos") {
        paths::home().join(".Trash")
    } else if cfg!(windows) {
        anyhow::bail!(
            "trash mode is not supported on Windows yet; use delete_mode = \"permanent\""
        );
    } else {
        paths::home().join(".local/share/Trash/files")
    };
    std::fs::create_dir_all(&trash)?;
    let name = fs::file_name(p);
    let mut dest = trash.join(&name);
    let mut n = 1;
    while dest.exists() {
        dest = trash.join(format!("{name} {n}"));
        n += 1;
    }
    if std::fs::rename(p, &dest).is_err() {
        if p.is_dir() {
            std::fs::remove_dir_all(p)?;
        } else {
            std::fs::remove_file(p)?;
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::{Group, Plan};

    #[test]
    fn refuses_paths_outside_allowed_prefixes() {
        let d = tempfile::tempdir().unwrap();
        let victim = d.path().join("victim");
        std::fs::create_dir_all(&victim).unwrap();
        let mut cfg = Config::default();
        cfg.safety.allowed_prefixes = vec![d.path().join("elsewhere").display().to_string()];
        let ex = Executor::new(&cfg);
        let mut plan = Plan::default();
        plan.push(Group::Tmp, ActionKind::Remove, victim.clone(), 1, "test");
        let s = ex.run(&plan, None, false, |_| {});
        assert!(!s.results[0].ok);
        assert!(victim.exists());
    }

    #[test]
    fn dry_run_touches_nothing_and_reports_estimates() {
        let d = tempfile::tempdir().unwrap();
        let victim = d.path().join("v");
        std::fs::write(&victim, b"data").unwrap();
        let mut cfg = Config::default();
        cfg.safety.allowed_prefixes = vec![d.path().display().to_string()];
        let ex = Executor::new(&cfg);
        let mut plan = Plan::default();
        plan.push(Group::Tmp, ActionKind::Remove, victim.clone(), 4, "test");
        let s = ex.run(&plan, None, true, |_| {});
        assert_eq!(s.freed_bytes, 4);
        assert!(victim.exists());
        let s = ex.run(&plan, Some(&[1]), false, |_| {});
        assert!(s.results[0].ok);
        assert!(!victim.exists());
    }
}
