//! Конфиг: ~/.config/diskgc/config.toml. Создаётся с дефолтами при первом запуске,
//! перечитывается демоном на каждом тике — правки применяются без перезапуска.

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

use crate::fsutil::home;

pub fn config_path() -> PathBuf {
    home().join(".config/diskgc/config.toml")
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct Config {
    pub scan: Scan,
    pub thresholds: Thresholds,
    pub daemon: Daemon,
    pub rust: Rust,
    pub node: Node,
    pub build_dirs: BuildDirs,
    pub scratch: Scratch,
    pub ai_tools: AiTools,
    pub docker: Docker,
    pub caches: Caches,
    pub compress: Compress,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct Scan {
    /// Где живут проекты (ищем node_modules, target, .dart_tool, ...)
    pub roots: Vec<String>,
    /// Где ещё искать каталоги сборки Rust (агенты кладут их во временные папки)
    pub extra_target_roots: Vec<String>,
    pub depth: usize,
    /// Подстроки пути, которые никогда не трогать
    pub exclude: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct Thresholds {
    /// Свободно больше — только плановая гигиена раз в N часов
    pub comfortable_gb: f64,
    /// Свободно меньше — чистка по обычным правилам сразу
    pub low_gb: f64,
    /// Свободно меньше — агрессивная чистка (сроки «свежести» урезаются в 4 раза)
    pub critical_gb: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct Daemon {
    pub tick_minutes: u64,
    pub hygiene_every_hours: u64,
    pub notify: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct Rust {
    /// Удалять из target/ файлы, к которым не обращались N дней
    pub sweep_days: u64,
    /// target/ больше N ГБ → убрать incremental и толстые тестовые бинарники,
    /// а если он при этом простаивает idle_hours — снести целиком
    pub target_max_gb: f64,
    pub idle_hours: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct Node {
    /// node_modules удаляется, если в проекте (кроме служебных каталогов) ничего не менялось N дней
    pub stale_days: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct BuildDirs {
    pub names: Vec<String>,
    pub stale_days: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct Scratch {
    /// /private/tmp/claude-501/<проект>/<сессия>: удалить, если сессия молчит N дней
    pub claude_stale_days: u64,
    /// Свои каталоги в /private/tmp: удалить, если не менялись N дней
    pub tmp_stale_days: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct AiTools {
    /// Сколько старых версий Codex хранить помимо текущей
    pub codex_keep_releases: usize,
    /// Удалять историю сессий Codex старше N дней (0 = не удалять, только сжимать)
    pub codex_sessions_delete_days: u64,
    /// Удалять историю сессий Claude Code старше N дней (0 = не удалять, только сжимать)
    pub claude_sessions_delete_days: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct Docker {
    pub enable: bool,
    /// Контейнеры testcontainers старше N часов — сироты от убитых тестов
    pub testcontainers_max_hours: u64,
    pub stopped_hours: u64,
    pub build_cache_days: u64,
    /// true = удалять и неиспользуемые образы (скачаются заново при надобности)
    pub image_prune_all: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct Caches {
    /// Клоны Chrome code_sign_clone старше N часов (остаются после обновлений)
    pub chrome_clone_hours: u64,
    pub go_cache_max_gb: f64,
    pub brew_prune_days: u64,
    pub xcode_derived_days: u64,
    pub vscode_shipit: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct Compress {
    /// Прозрачное APFS-сжатие холодных файлов (остаются читаемыми как обычные)
    pub enable: bool,
    pub min_age_days: u64,
    pub min_size_kb: u64,
    pub dirs: Vec<String>,
    pub extensions: Vec<String>,
}

fn s(v: &[&str]) -> Vec<String> {
    v.iter().map(|x| x.to_string()).collect()
}

impl Default for Config {
    fn default() -> Self {
        Self {
            scan: Scan::default(),
            thresholds: Thresholds::default(),
            daemon: Daemon::default(),
            rust: Rust::default(),
            node: Node::default(),
            build_dirs: BuildDirs::default(),
            scratch: Scratch::default(),
            ai_tools: AiTools::default(),
            docker: Docker::default(),
            caches: Caches::default(),
            compress: Compress::default(),
        }
    }
}
impl Default for Scan {
    fn default() -> Self {
        Self {
            roots: s(&["~/Monitezations", "~/UniOS", "~/Projects", "~/Documents", "~/Robo"]),
            extra_target_roots: s(&["/private/tmp"]),
            depth: 7,
            exclude: vec![],
        }
    }
}
impl Default for Thresholds {
    fn default() -> Self {
        Self { comfortable_gb: 150.0, low_gb: 100.0, critical_gb: 60.0 }
    }
}
impl Default for Daemon {
    fn default() -> Self {
        Self { tick_minutes: 10, hygiene_every_hours: 6, notify: true }
    }
}
impl Default for Rust {
    fn default() -> Self {
        Self { sweep_days: 7, target_max_gb: 20.0, idle_hours: 12 }
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
            names: s(&[
                ".dart_tool", "build", "_build", ".next", ".turbo", ".nuxt", ".svelte-kit",
                ".parcel-cache", "__pycache__", ".pytest_cache", ".mypy_cache", ".gradle",
            ]),
            stale_days: 30,
        }
    }
}
impl Default for Scratch {
    fn default() -> Self {
        Self { claude_stale_days: 3, tmp_stale_days: 7 }
    }
}
impl Default for AiTools {
    fn default() -> Self {
        Self { codex_keep_releases: 1, codex_sessions_delete_days: 0, claude_sessions_delete_days: 0 }
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
            brew_prune_days: 7,
            xcode_derived_days: 14,
            vscode_shipit: true,
        }
    }
}
impl Default for Compress {
    fn default() -> Self {
        Self {
            enable: true,
            min_age_days: 7,
            min_size_kb: 64,
            dirs: s(&["~/.codex/sessions", "~/.claude/projects", "~/.codex/logs", "~/Library/Logs"]),
            extensions: s(&["jsonl", "log", "json", "txt", "md"]),
        }
    }
}

/// Уровень агрессивности — зависит от свободного места.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Level {
    Normal,
    Low,
    Critical,
}

impl Level {
    pub fn name(self) -> &'static str {
        match self {
            Level::Normal => "normal",
            Level::Low => "low",
            Level::Critical => "critical",
        }
    }
    pub fn parse(s: &str) -> Option<Level> {
        match s {
            "normal" => Some(Level::Normal),
            "low" => Some(Level::Low),
            "critical" => Some(Level::Critical),
            _ => None,
        }
    }
}

/// Реальные сроки/лимиты после применения уровня.
#[derive(Debug, Clone)]
pub struct Effective {
    pub level: Level,
    pub rust_sweep_days: u64,
    pub rust_target_max_gb: f64,
    pub rust_idle_hours: u64,
    pub node_stale_days: u64,
    pub build_stale_days: u64,
    pub claude_scratch_days: u64,
    pub tmp_stale_days: u64,
    pub docker_stopped_hours: u64,
    pub testcontainers_hours: u64,
}

fn shrink(v: u64, div: u64) -> u64 {
    (v / div).max(1)
}

impl Config {
    pub fn load() -> Result<Config> {
        let p = config_path();
        if !p.exists() {
            let cfg = Config::default();
            cfg.save()?;
            return Ok(cfg);
        }
        let text = std::fs::read_to_string(&p).with_context(|| format!("read {}", p.display()))?;
        toml::from_str(&text).with_context(|| format!("parse {}", p.display()))
    }

    pub fn save(&self) -> Result<()> {
        let p = config_path();
        std::fs::create_dir_all(p.parent().unwrap())?;
        let text = toml::to_string_pretty(self)?;
        std::fs::write(&p, text)?;
        Ok(())
    }

    pub fn effective(&self, level: Level) -> Effective {
        let d = match level {
            Level::Normal => 1,
            Level::Low => 2,
            Level::Critical => 4,
        };
        Effective {
            level,
            rust_sweep_days: shrink(self.rust.sweep_days, d),
            rust_target_max_gb: if level == Level::Critical {
                self.rust.target_max_gb / 2.0
            } else {
                self.rust.target_max_gb
            },
            rust_idle_hours: shrink(self.rust.idle_hours, d),
            node_stale_days: shrink(self.node.stale_days, d),
            build_stale_days: shrink(self.build_dirs.stale_days, d),
            claude_scratch_days: shrink(self.scratch.claude_stale_days, d),
            tmp_stale_days: shrink(self.scratch.tmp_stale_days, d),
            docker_stopped_hours: shrink(self.docker.stopped_hours, d),
            testcontainers_hours: shrink(self.docker.testcontainers_max_hours, d),
        }
    }

    pub fn excluded(&self, path: &std::path::Path) -> bool {
        let s = path.to_string_lossy();
        self.scan.exclude.iter().any(|e| !e.is_empty() && s.contains(e.as_str()))
    }
}
