//! Configuration: `config.toml`, created with defaults on first run and re-read by the daemon on
//! every tick, so edits apply without a restart.

use std::path::Path;

use anyhow::{Context, Result};
use globset::{Glob, GlobSet, GlobSetBuilder};
use serde::{Deserialize, Serialize};

use crate::model::Level;
use crate::paths;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
#[derive(Default)]
pub struct Config {
    pub scan: Scan,
    pub thresholds: Thresholds,
    pub daemon: Daemon,
    pub safety: Safety,
    pub rust: Rust,
    pub node: Node,
    pub build_dirs: BuildDirs,
    pub scratch: Scratch,
    pub ai_tools: AiTools,
    pub docker: Docker,
    pub caches: Caches,
    pub compress: Compress,
    pub mem: Mem,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct Scan {
    /// Where your projects live. `~` and environment variables are expanded.
    pub roots: Vec<String>,
    /// Extra places to look for Rust `target/` directories (agents like to put them in temp dirs).
    pub extra_target_roots: Vec<String>,
    /// Directory depth below each root to search.
    pub depth: usize,
    /// Glob patterns (matched against the full path) that no rule may touch.
    pub exclude: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct Thresholds {
    /// More free space than this: only scheduled hygiene.
    pub comfortable_gb: f64,
    /// Less than this: clean right away with normal rules.
    pub low_gb: f64,
    /// Less than this: critical level, staleness windows quartered.
    pub critical_gb: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct Daemon {
    pub tick_minutes: u64,
    pub hygiene_every_hours: u64,
    /// Desktop notification after a run that freed something. Off by default on macOS: a
    /// notification from a command-line tool makes the system ask for permission, and the
    /// grant does not survive a rebuild of an ad-hoc-signed binary.
    pub notify: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct Safety {
    /// `permanent` or `trash`.
    pub delete_mode: DeleteMode,
    /// Refuse to remove anything outside these prefixes (expanded). Empty = home + temp dirs.
    pub allowed_prefixes: Vec<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum DeleteMode {
    Permanent,
    Trash,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct Rust {
    /// Delete artifacts nobody touched for this many days.
    pub sweep_days: u64,
    /// Above this size: drop incremental caches and fat test binaries first, then, if the
    /// directory is idle for `idle_hours`, remove it entirely.
    pub target_max_gb: f64,
    pub idle_hours: u64,
    /// Budget for all `target/` directories together. Above it, whole targets go in least
    /// recently built order once idle for `idle_hours`: a dozen agent worktrees of 5–20 GB each
    /// never trip the per-directory cap, yet add up to more than the disk can spare.
    pub total_max_gb: f64,
    /// The target of a linked git worktree whose branch is already merged into the main branch
    /// goes once nothing was built there for this long.
    pub merged_idle_hours: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct Node {
    /// `node_modules` goes when nothing in the project changed for this many days.
    pub stale_days: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct BuildDirs {
    pub names: Vec<String>,
    pub stale_days: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct Scratch {
    /// Claude Code scratch directories: removed when the session was silent this long.
    pub claude_stale_days: u64,
    /// Your own entries in the system temp directory: removed when untouched this long.
    pub tmp_stale_days: u64,
    /// Entries in the per-user `$TMPDIR` (macOS): removed when untouched this long. Builds
    /// (`go test`, Next/Vite SSR bundles, Playwright profiles) leave gigabytes there a day.
    pub user_tmp_stale_hours: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct AiTools {
    /// Old Codex application versions to keep besides the current one.
    pub codex_keep_releases: usize,
    /// Delete Codex session logs older than N days (0 = never delete, only compress).
    pub codex_sessions_delete_days: u64,
    /// Delete Claude Code session logs older than N days (0 = never delete, only compress).
    pub claude_sessions_delete_days: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct Docker {
    pub enable: bool,
    /// testcontainers containers older than this are orphans of killed test runs.
    pub testcontainers_max_hours: u64,
    pub stopped_hours: u64,
    pub build_cache_days: u64,
    /// Also remove images not used by any container (they are re-pulled on demand).
    pub image_prune_all: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct Caches {
    /// Chrome leaves code-signing clones of itself behind after updates (macOS).
    pub chrome_clone_hours: u64,
    pub go_cache_max_gb: f64,
    pub npm_cache_max_gb: f64,
    pub uv_cache_max_gb: f64,
    /// sccache's local cache above this size loses entries older than a week; all of it when
    /// nothing used it for a week. sccache itself only caps it at 10 GB.
    pub sccache_max_gb: f64,
    /// Metal shader caches the system keeps per application (macOS): above this size for one
    /// application, and when it compiled nothing for a day, they go and are rebuilt on demand.
    pub shader_cache_max_gb: f64,
    pub brew_prune_days: u64,
    pub xcode_derived_days: u64,
    pub vscode_shipit: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct Compress {
    /// Filesystem-level transparent compression of cold files (APFS, NTFS).
    pub enable: bool,
    pub min_age_days: u64,
    pub min_size_kb: u64,
    pub dirs: Vec<String>,
    pub extensions: Vec<String>,
}

fn strings(v: &[&str]) -> Vec<String> {
    v.iter().map(|s| s.to_string()).collect()
}

impl Default for Scan {
    fn default() -> Self {
        let mut roots = strings(&[
            "~/Projects",
            "~/Developer",
            "~/dev",
            "~/src",
            "~/code",
            "~/work",
        ]);
        // ~/Documents is guarded by macOS privacy protection (TCC): scanning it from a background
        // daemon triggers a permission dialog, and the grant is tied to the binary's code hash, so
        // every rebuild asks again. Users who keep projects there can add it after granting
        // Full Disk Access to the daemon.
        if !cfg!(target_os = "macos") {
            roots.push("~/Documents".into());
        }
        if cfg!(windows) {
            roots.push("%USERPROFILE%\\source".into());
        }
        Self {
            roots,
            extra_target_roots: if cfg!(windows) {
                strings(&["%TEMP%"])
            } else {
                strings(&["/tmp"])
            },
            depth: 7,
            exclude: vec![],
        }
    }
}
impl Default for Thresholds {
    fn default() -> Self {
        Self {
            comfortable_gb: 150.0,
            low_gb: 100.0,
            critical_gb: 60.0,
        }
    }
}
impl Default for Daemon {
    fn default() -> Self {
        Self {
            tick_minutes: 10,
            hygiene_every_hours: 6,
            notify: !cfg!(target_os = "macos"),
        }
    }
}
impl Default for Safety {
    fn default() -> Self {
        Self {
            delete_mode: DeleteMode::Permanent,
            allowed_prefixes: vec![],
        }
    }
}
impl Default for Rust {
    fn default() -> Self {
        Self {
            sweep_days: 7,
            target_max_gb: 20.0,
            idle_hours: 12,
            total_max_gb: 40.0,
            merged_idle_hours: 6,
        }
    }
}
impl Default for Node {
    fn default() -> Self {
        Self { stale_days: 30 }
    }
}
impl Default for BuildDirs {
    fn default() -> Self {
        Self {
            names: strings(&[
                ".dart_tool",
                "build",
                "_build",
                ".next",
                ".turbo",
                ".nuxt",
                ".svelte-kit",
                ".parcel-cache",
                "__pycache__",
                ".pytest_cache",
                ".mypy_cache",
                ".gradle",
                "cmake-build-debug",
                "cmake-build-release",
            ]),
            stale_days: 30,
        }
    }
}
impl Default for Scratch {
    fn default() -> Self {
        Self {
            claude_stale_days: 3,
            tmp_stale_days: 7,
            user_tmp_stale_hours: 24,
        }
    }
}
impl Default for AiTools {
    fn default() -> Self {
        Self {
            codex_keep_releases: 1,
            codex_sessions_delete_days: 0,
            claude_sessions_delete_days: 0,
        }
    }
}
impl Default for Docker {
    fn default() -> Self {
        Self {
            enable: true,
            testcontainers_max_hours: 2,
            stopped_hours: 24,
            build_cache_days: 7,
            image_prune_all: false,
        }
    }
}
impl Default for Caches {
    fn default() -> Self {
        Self {
            chrome_clone_hours: 6,
            go_cache_max_gb: 2.0,
            npm_cache_max_gb: 1.0,
            uv_cache_max_gb: 1.0,
            sccache_max_gb: 2.0,
            shader_cache_max_gb: 1.0,
            brew_prune_days: 7,
            xcode_derived_days: 14,
            vscode_shipit: true,
        }
    }
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
#[derive(Default)]
pub struct Mem {
    /// Browsers started by automation tools (Playwright, Puppeteer, headless shells) that have
    /// been running longer than this many hours are terminated by the daemon. They keep a live
    /// parent, so the orphan rule never catches them, yet each holds 1–2 GB. `0` = report only.
    pub automation_browser_max_hours: u64,
}

impl Default for Compress {
    fn default() -> Self {
        Self {
            enable: true,
            min_age_days: 7,
            min_size_kb: 64,
            dirs: strings(&["~/.codex/sessions", "~/.claude/projects", "~/.codex/logs"]),
            extensions: strings(&["jsonl", "log", "json", "txt", "md"]),
        }
    }
}

/// Concrete windows and caps after applying a [`Level`].
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Effective {
    pub level: Level,
    pub rust_sweep_days: u64,
    pub rust_target_max_gb: f64,
    pub rust_idle_hours: u64,
    pub rust_total_max_gb: f64,
    pub rust_merged_idle_hours: u64,
    pub node_stale_days: u64,
    pub build_stale_days: u64,
    pub claude_scratch_days: u64,
    pub tmp_stale_days: u64,
    pub user_tmp_stale_hours: u64,
    pub docker_stopped_hours: u64,
    pub testcontainers_hours: u64,
    /// Build cache unused for this long is pruned; `0` = all unused build cache.
    pub docker_build_cache_hours: u64,
    /// Age after which entries of size-capped tool caches (sccache) are dropped.
    pub cache_sweep_days: u64,
}

fn shrink(v: u64, div: u64) -> u64 {
    (v / div).max(1)
}

impl Config {
    /// Load `config.toml`, writing the defaults first if it does not exist.
    pub fn load() -> Result<Config> {
        Self::load_from(&paths::config_file())
    }

    pub fn load_from(path: &Path) -> Result<Config> {
        if !path.exists() {
            let cfg = Config::default();
            cfg.save_to(path)?;
            return Ok(cfg);
        }
        let text =
            std::fs::read_to_string(path).with_context(|| format!("reading {}", path.display()))?;
        Self::parse(&text).with_context(|| format!("parsing {}", path.display()))
    }

    pub fn parse(text: &str) -> Result<Config> {
        let cfg: Config = toml::from_str(text)?;
        cfg.validate()?;
        Ok(cfg)
    }

    pub fn save(&self) -> Result<()> {
        self.save_to(&paths::config_file())
    }

    pub fn save_to(&self, path: &Path) -> Result<()> {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let text = format!("{}\n{}", HEADER, toml::to_string_pretty(self)?);
        std::fs::write(path, text).with_context(|| format!("writing {}", path.display()))
    }

    pub fn validate(&self) -> Result<()> {
        let t = &self.thresholds;
        anyhow::ensure!(
            t.critical_gb <= t.low_gb && t.low_gb <= t.comfortable_gb,
            "thresholds must satisfy critical_gb <= low_gb <= comfortable_gb"
        );
        anyhow::ensure!(
            self.daemon.tick_minutes >= 1,
            "daemon.tick_minutes must be at least 1"
        );
        anyhow::ensure!(
            self.scan.depth >= 1 && self.scan.depth <= 32,
            "scan.depth must be between 1 and 32"
        );
        self.exclude_set()?;
        Ok(())
    }

    pub fn effective(&self, level: Level) -> Effective {
        let div = match level {
            Level::Normal => 1,
            Level::Low => 2,
            Level::Critical => 4,
        };
        Effective {
            level,
            rust_sweep_days: shrink(self.rust.sweep_days, div),
            rust_target_max_gb: if level == Level::Critical {
                self.rust.target_max_gb / 2.0
            } else {
                self.rust.target_max_gb
            },
            rust_idle_hours: shrink(self.rust.idle_hours, div),
            rust_total_max_gb: if level == Level::Critical {
                self.rust.total_max_gb / 2.0
            } else {
                self.rust.total_max_gb
            },
            rust_merged_idle_hours: shrink(self.rust.merged_idle_hours, div),
            node_stale_days: shrink(self.node.stale_days, div),
            build_stale_days: shrink(self.build_dirs.stale_days, div),
            claude_scratch_days: shrink(self.scratch.claude_stale_days, div),
            tmp_stale_days: shrink(self.scratch.tmp_stale_days, div),
            user_tmp_stale_hours: shrink(self.scratch.user_tmp_stale_hours, div),
            docker_stopped_hours: shrink(self.docker.stopped_hours, div),
            testcontainers_hours: shrink(self.docker.testcontainers_max_hours, div),
            docker_build_cache_hours: if level == Level::Critical {
                0
            } else {
                shrink(self.docker.build_cache_days * 24, div)
            },
            cache_sweep_days: shrink(7, div),
        }
    }

    pub fn exclude_set(&self) -> Result<GlobSet> {
        let mut b = GlobSetBuilder::new();
        for pat in &self.scan.exclude {
            if pat.trim().is_empty() {
                continue;
            }
            // A bare path prefix is the most common intent: treat "foo/bar" as "foo/bar/**" too.
            b.add(Glob::new(pat).with_context(|| format!("bad exclude pattern `{pat}`"))?);
            if !pat.contains('*') {
                b.add(Glob::new(&format!("{}/**", pat.trim_end_matches('/')))?);
            }
        }
        Ok(b.build()?)
    }

    pub fn level_for(&self, available_bytes: u64) -> Level {
        let free = crate::bytes::gib(available_bytes);
        if free < self.thresholds.critical_gb {
            Level::Critical
        } else if free < self.thresholds.low_gb {
            Level::Low
        } else {
            Level::Normal
        }
    }
}

const HEADER: &str =
    "# diskgc configuration. Edit freely; the daemon re-reads this file on every tick.
# Documentation for every key: https://github.com/eukalpia/diskgc/blob/main/docs/CONFIG.md
";

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn defaults_roundtrip() {
        let cfg = Config::default();
        let text = toml::to_string_pretty(&cfg).unwrap();
        let back = Config::parse(&text).unwrap();
        assert_eq!(back.rust.sweep_days, 7);
        assert_eq!(back.thresholds.low_gb, 100.0);
    }

    #[test]
    fn unknown_keys_are_rejected() {
        assert!(Config::parse("[rust]\nsweep_dayz = 3\n").is_err());
    }

    #[test]
    fn levels_shrink_windows() {
        let cfg = Config::default();
        assert_eq!(cfg.effective(Level::Normal).node_stale_days, 30);
        assert_eq!(cfg.effective(Level::Low).node_stale_days, 15);
        assert_eq!(cfg.effective(Level::Critical).node_stale_days, 7);
        assert_eq!(cfg.effective(Level::Critical).rust_target_max_gb, 10.0);
        assert_eq!(cfg.effective(Level::Critical).rust_total_max_gb, 20.0);
        assert_eq!(cfg.effective(Level::Normal).docker_build_cache_hours, 168);
        assert_eq!(cfg.effective(Level::Low).docker_build_cache_hours, 84);
        assert_eq!(cfg.effective(Level::Critical).docker_build_cache_hours, 0);
    }

    #[test]
    fn config_without_new_keys_still_loads() {
        let cfg =
            Config::parse("[rust]\nsweep_days = 3\n[caches]\ngo_cache_max_gb = 1.0\n").unwrap();
        assert_eq!(cfg.rust.total_max_gb, 40.0);
        assert_eq!(cfg.caches.sccache_max_gb, 2.0);
    }

    #[test]
    fn exclude_prefix_matches_children() {
        let mut cfg = Config::default();
        cfg.scan.exclude = vec!["/home/me/keep".into()];
        let set = cfg.exclude_set().unwrap();
        assert!(set.is_match("/home/me/keep"));
        assert!(set.is_match("/home/me/keep/target"));
        assert!(!set.is_match("/home/me/other"));
    }

    #[test]
    fn bad_thresholds_fail_validation() {
        let mut cfg = Config::default();
        cfg.thresholds.low_gb = 500.0;
        assert!(cfg.validate().is_err());
    }
}
