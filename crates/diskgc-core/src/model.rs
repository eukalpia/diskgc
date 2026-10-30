//! Data types shared by rules, executor, history, CLI and the IPC protocol.
//! Everything here serialises to JSON; the Dart client mirrors these shapes.

use std::path::PathBuf;

use serde::{Deserialize, Serialize};

/// How aggressive a run is. Chosen from free space and the growth forecast.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Level {
    /// Plenty of space: only age-based hygiene.
    Normal,
    /// Below the `low` threshold: staleness windows halved.
    Low,
    /// Below the `critical` threshold: windows quartered, size caps halved.
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
}

impl std::str::FromStr for Level {
    type Err = String;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.to_ascii_lowercase().as_str() {
            "normal" => Ok(Level::Normal),
            "low" => Ok(Level::Low),
            "critical" => Ok(Level::Critical),
            other => Err(format!(
                "unknown level `{other}`; expected normal, low or critical"
            )),
        }
    }
}

/// Which rule produced an action. Used for grouping in the UI and in statistics.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Group {
    Rust,
    Node,
    Build,
    Scratch,
    Tmp,
    Codex,
    Claude,
    Cache,
    Compress,
    Docker,
}

impl Group {
    pub fn name(self) -> &'static str {
        match self {
            Group::Rust => "rust",
            Group::Node => "node",
            Group::Build => "build",
            Group::Scratch => "scratch",
            Group::Tmp => "tmp",
            Group::Codex => "codex",
            Group::Claude => "claude",
            Group::Cache => "cache",
            Group::Compress => "compress",
            Group::Docker => "docker",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum ActionKind {
    /// Remove the path recursively.
    Remove,
    /// Delete files under the path whose mtime is older than `days`, then empty directories.
    SweepOlderThan { days: u64 },
    /// Delete large extension-less executables in `*/deps/` that are older than a day.
    SweepBigBinaries,
    /// Transparently compress the file with the filesystem's own compression.
    Compress,
    /// Run an external command; `measure` is the directory whose size change is the effect.
    Command {
        argv: Vec<String>,
        measure: Option<PathBuf>,
    },
}

impl ActionKind {
    pub fn verb(&self) -> String {
        match self {
            ActionKind::Remove => "remove".into(),
            ActionKind::SweepOlderThan { days } => format!("sweep >{days}d"),
            ActionKind::SweepBigBinaries => "sweep test binaries".into(),
            ActionKind::Compress => "compress".into(),
            ActionKind::Command { argv, .. } => argv.join(" "),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Action {
    /// Stable within one plan; used to select actions for execution.
    pub id: u32,
    pub group: Group,
    #[serde(flatten)]
    pub kind: ActionKind,
    pub path: PathBuf,
    /// Estimated effect in bytes. For `Compress` this is the file's current size on disk.
    pub bytes: u64,
    pub reason: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DockerPlan {
    pub available: bool,
    pub orphan_testcontainers: usize,
    pub stopped_hours: u64,
    pub build_cache_days: u64,
    pub image_prune_all: bool,
    /// From `docker system df`, best effort.
    pub reclaimable_bytes: u64,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct Plan {
    pub level: Option<Level>,
    pub created_at: String,
    pub actions: Vec<Action>,
    /// Things deliberately left alone, with the reason.
    pub notes: Vec<String>,
    pub docker: Option<DockerPlan>,
}

impl Plan {
    pub fn push(
        &mut self,
        group: Group,
        kind: ActionKind,
        path: PathBuf,
        bytes: u64,
        reason: impl Into<String>,
    ) {
        let id = self.actions.len() as u32 + 1;
        self.actions.push(Action {
            id,
            group,
            kind,
            path,
            bytes,
            reason: reason.into(),
        });
    }

    pub fn note(&mut self, text: impl Into<String>) {
        self.notes.push(text.into());
    }

    /// Estimated bytes to free, excluding compression (whose gain is unknown until done).
    pub fn estimated_bytes(&self) -> u64 {
        self.actions
            .iter()
            .filter(|a| a.kind != ActionKind::Compress)
            .map(|a| a.bytes)
            .sum()
    }

    pub fn compress_count(&self) -> usize {
        self.actions
            .iter()
            .filter(|a| a.kind == ActionKind::Compress)
            .count()
    }

    /// Largest estimated effect first; stable ids are preserved.
    pub fn sort_by_size(&mut self) {
        self.actions.sort_by_key(|a| std::cmp::Reverse(a.bytes));
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ActionResult {
    pub id: u32,
    pub freed: u64,
    pub ok: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct RunSummary {
    pub level: Option<Level>,
    pub dry_run: bool,
    pub started_at: String,
    pub duration_secs: f64,
    pub action_count: usize,
    pub freed_bytes: u64,
    pub docker_freed_bytes: u64,
    pub results: Vec<ActionResult>,
    pub docker_lines: Vec<String>,
    pub free_after: u64,
}

/// Progress events emitted while planning and executing.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "stage", rename_all = "snake_case")]
pub enum Progress {
    Scanning {
        rule: String,
    },
    Planned {
        actions: usize,
        estimated_bytes: u64,
    },
    Executing {
        index: usize,
        total: usize,
        action: Action,
    },
    Executed {
        result: ActionResult,
    },
    Docker {
        line: String,
    },
    Finished {
        summary: RunSummary,
    },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DiskStatus {
    pub volume: PathBuf,
    pub total: u64,
    pub used: u64,
    pub available: u64,
    pub level: Level,
}
