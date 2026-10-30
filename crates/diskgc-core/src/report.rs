//! What occupies the disk: category sizes and the biggest offenders per category.

use std::collections::BTreeMap;
use std::path::PathBuf;

use serde::{Deserialize, Serialize};

use crate::config::Config;
use crate::fs::{self, disk_usage};
use crate::paths::{self, home};
use crate::platform;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Sized {
    pub path: PathBuf,
    pub bytes: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Report {
    pub volume: PathBuf,
    pub total: u64,
    pub used: u64,
    pub available: u64,
    /// Category name → bytes.
    pub categories: BTreeMap<String, u64>,
    pub rust_targets: Vec<Sized>,
    pub node_modules: Vec<Sized>,
    pub scratch: Vec<Sized>,
    pub docker_df: Option<String>,
}

fn sized(paths: Vec<PathBuf>, limit: usize) -> Vec<Sized> {
    let mut v: Vec<Sized> = paths
        .into_iter()
        .map(|p| Sized {
            bytes: disk_usage(&p),
            path: p,
        })
        .collect();
    v.sort_by_key(|s| std::cmp::Reverse(s.bytes));
    v.truncate(limit);
    v
}

fn sum(v: &[Sized]) -> u64 {
    v.iter().map(|s| s.bytes).sum()
}

pub fn collect(cfg: &Config, mut on_step: impl FnMut(&str)) -> Report {
    let h = home();
    let volume = paths::watched_volume();
    let (total, used, available) = platform::disk_space(&volume).unwrap_or((0, 0, 0));
    on_step("project tree");
    let mut index = crate::rules::tree_index(cfg);
    on_step("rust targets");
    let rust_targets = sized(std::mem::take(&mut index.cargo_targets), 50);
    on_step("node_modules");
    let node_modules = sized(index.named.remove("node_modules").unwrap_or_default(), 50);
    on_step("scratch");
    let scratch_dirs: Vec<PathBuf> = platform::claude_scratch_root()
        .into_iter()
        .flat_map(|root| {
            std::fs::read_dir(root)
                .into_iter()
                .flatten()
                .flatten()
                .map(|p| p.path())
        })
        .flat_map(|proj| {
            std::fs::read_dir(proj)
                .into_iter()
                .flatten()
                .flatten()
                .map(|s| s.path())
        })
        .collect();
    let scratch = sized(scratch_dirs, 20);

    let mut categories = BTreeMap::new();
    categories.insert("rust_targets".into(), sum(&rust_targets));
    categories.insert("node_modules".into(), sum(&node_modules));
    categories.insert("claude_scratch".into(), sum(&scratch));
    on_step("temp");
    let tmp = platform::system_tmp_dir();
    let claude_root = platform::claude_scratch_root();
    let tmp_other: u64 = std::fs::read_dir(&tmp)
        .into_iter()
        .flatten()
        .flatten()
        .map(|e| e.path())
        .filter(|p| Some(p) != claude_root.as_ref())
        .filter(|p| {
            std::fs::symlink_metadata(p)
                .map(|m| platform::owned_by_current_user(&m))
                .unwrap_or(false)
        })
        .map(|p| disk_usage(&p))
        .sum();
    categories.insert("tmp_other".into(), tmp_other);
    on_step("AI tools");
    categories.insert(
        "codex".into(),
        disk_usage(&h.join(".codex")) + disk_usage(&h.join(".cache/codex-runtimes")),
    );
    let claude_app = if cfg!(target_os = "macos") {
        h.join("Library/Application Support/Claude")
    } else {
        PathBuf::new()
    };
    categories.insert(
        "claude".into(),
        disk_usage(&h.join(".claude")) + disk_usage(&claude_app),
    );
    on_step("caches");
    if cfg!(target_os = "macos") {
        categories.insert(
            "library_caches".into(),
            disk_usage(&h.join("Library/Caches")),
        );
        categories.insert(
            "xcode_simulators".into(),
            disk_usage(&h.join("Library/Developer")),
        );
    }
    if let Some(x) = platform::chrome_clone_dir() {
        categories.insert("chrome_clones".into(), disk_usage(&x));
    }
    if let Some(sc) = platform::sccache_dir() {
        categories.insert("sccache".into(), disk_usage(&sc));
    }
    if let Some(c) = platform::user_cache_dir() {
        // Apple's own folders here belong to system processes; count the applications'.
        let apps: u64 = std::fs::read_dir(&c)
            .into_iter()
            .flatten()
            .flatten()
            .filter(|e| !fs::file_name(&e.path()).starts_with("com.apple."))
            .map(|e| disk_usage(&e.path()))
            .sum();
        categories.insert("system_app_caches".into(), apps);
    }
    categories.insert(
        "toolchain_caches".into(),
        [
            ".cargo",
            ".rustup",
            ".npm",
            ".pub-cache",
            "go",
            ".cache",
            ".bun",
            ".gradle",
            ".m2",
            ".nuget",
        ]
        .iter()
        .map(|d| disk_usage(&h.join(d)))
        .sum(),
    );
    on_step("docker");
    let docker_available = crate::docker::available();
    if docker_available {
        // Reported by Docker itself: walking the VM's disk image inside ~/Library/Group Containers
        // would trigger the "access data from other apps" privacy dialog on macOS.
        categories.insert("docker".into(), crate::docker::total_bytes());
    }
    let docker_df = docker_available.then(crate::docker::system_df);

    Report {
        volume,
        total,
        used,
        available,
        categories,
        rust_targets,
        node_modules,
        scratch,
        docker_df,
    }
}
