//! Rules: each one knows a single kind of garbage and turns it into explained actions.
//! Rules never delete anything; the [`crate::executor`] does.

use std::cell::OnceCell;
use std::path::{Path, PathBuf};
use std::time::Instant;

use globset::GlobSet;

use crate::config::{Config, Effective};
use crate::fs::TreeIndex;
use crate::model::{Level, Plan, RuleTiming};
use crate::{fs, paths};

mod ai_tools;
mod build_dirs;
mod caches;
mod compress;
mod node;
pub mod rust;
pub use rust::is_big_binary;
mod scratch;

/// What a rule sees while planning.
/// Knowledge from previous runs that shapes the next plan.
#[derive(Debug, Clone, Default)]
pub struct PlanHints {
    /// Command lines that freed nothing recently, with the target's size at that run; skipped
    /// for now unless the target has grown well past that size (see [`PlanHints::should_skip`]).
    pub skip_commands: std::collections::HashMap<String, u64>,
}

impl PlanHints {
    /// Skip `line` when it recently freed nothing and its target (now `bytes`) has not grown by
    /// more than a quarter (at least 1 GiB) since. A cache that keeps growing is retried at once:
    /// "freed nothing" then usually means builds refilled it while it was being cleared.
    pub fn should_skip(&self, line: &str, bytes: u64) -> Option<u64> {
        let &was = self.skip_commands.get(line)?;
        let slack = (was / 4).max(1 << 30);
        (bytes <= was.saturating_add(slack)).then_some(was)
    }
}

pub struct Ctx<'a> {
    pub cfg: &'a Config,
    pub eff: &'a Effective,
    pub hints: PlanHints,
    exclude: GlobSet,
    roots: Vec<PathBuf>,
    index: OnceCell<TreeIndex>,
}

impl<'a> Ctx<'a> {
    pub fn new(cfg: &'a Config, eff: &'a Effective, hints: PlanHints) -> Self {
        let exclude = cfg.exclude_set().unwrap_or_else(|_| GlobSet::empty());
        let roots = cfg
            .scan
            .roots
            .iter()
            .map(|r| paths::expand(r))
            .filter(|p| p.is_dir())
            .collect();
        Self {
            cfg,
            eff,
            hints,
            exclude,
            roots,
            index: OnceCell::new(),
        }
    }

    /// Cargo targets, `node_modules` and build directories, found in one walk on first use.
    pub fn index(&self) -> &TreeIndex {
        self.index.get_or_init(|| {
            let mut names = vec!["node_modules"];
            names.extend(self.cfg.build_dirs.names.iter().map(String::as_str));
            let mut idx = fs::index_tree(
                &self.target_roots(),
                self.cfg.scan.depth,
                &names,
                |name, dir| {
                    // A build directory belongs to its project only when a project marker sits
                    // next to it; a user folder that happens to be called `build` is searched on.
                    name == "node_modules" || dir.parent().map(has_project_marker).unwrap_or(false)
                },
            );
            // The extra target roots (temp folders) are searched for Cargo output alone.
            for dirs in idx.named.values_mut() {
                dirs.retain(|d| self.roots.iter().any(|r| d.starts_with(r)));
            }
            idx
        })
    }

    pub fn roots(&self) -> &[PathBuf] {
        &self.roots
    }

    pub fn target_roots(&self) -> Vec<PathBuf> {
        let mut v = self.roots.clone();
        v.extend(
            self.cfg
                .scan
                .extra_target_roots
                .iter()
                .map(|r| paths::expand(r))
                .filter(|p| p.is_dir()),
        );
        if let Some(scratch) = crate::platform::claude_scratch_root() {
            v.push(scratch);
        }
        v.sort();
        v.dedup();
        v
    }

    pub fn excluded(&self, p: &Path) -> bool {
        self.exclude.is_match(p)
    }

    /// The project a directory belongs to, for judging activity. Walks up to the topmost ancestor
    /// that has a project marker, stopping at repository boundaries (a git worktree is its own
    /// project) and at the scan roots.
    pub fn project_root<'p>(&self, start: &'p Path) -> &'p Path {
        let mut best = start;
        let mut cur = start;
        while let Some(parent) = cur.parent() {
            if self.roots.iter().any(|r| r == parent) || parent.parent().is_none() {
                break;
            }
            if cur.join(".git").exists() {
                break;
            }
            if PROJECT_ROOT_MARKERS.iter().any(|m| parent.join(m).exists()) {
                best = parent;
            }
            cur = parent;
        }
        best
    }
}

/// Files whose presence next to a directory prove that it belongs to a software project.
pub const PROJECT_MARKERS: &[&str] = &[
    "Cargo.toml",
    "package.json",
    "pnpm-workspace.yaml",
    "pubspec.yaml",
    "mix.exs",
    "pyproject.toml",
    "setup.py",
    "requirements.txt",
    "build.gradle",
    "build.gradle.kts",
    "settings.gradle",
    "settings.gradle.kts",
    "CMakeLists.txt",
    "go.mod",
    "Makefile",
    "Package.swift",
    "*.xcodeproj",
];

const PROJECT_ROOT_MARKERS: &[&str] = &[
    "package.json",
    "pnpm-workspace.yaml",
    "Cargo.toml",
    "pubspec.yaml",
    "mix.exs",
    "pyproject.toml",
    "go.mod",
];

/// Directories that do not count as "the project is alive" when they change.
pub const PROJECT_NOISE: &[&str] = &[
    "node_modules",
    ".git",
    "target",
    ".dart_tool",
    "build",
    "_build",
    ".next",
    ".turbo",
    ".venv",
    "__pycache__",
    ".gradle",
    ".pytest_cache",
];

pub fn has_project_marker(dir: &Path) -> bool {
    PROJECT_MARKERS.iter().any(|m| {
        if let Some(ext) = m.strip_prefix("*.") {
            std::fs::read_dir(dir)
                .map(|it| {
                    it.flatten()
                        .any(|e| e.path().extension().map(|x| x == ext).unwrap_or(false))
                })
                .unwrap_or(false)
        } else {
            dir.join(m).exists()
        }
    })
}

pub trait Rule {
    fn name(&self) -> &'static str;
    fn plan(&self, ctx: &Ctx, plan: &mut Plan);
}

pub fn all_rules() -> Vec<Box<dyn Rule>> {
    vec![
        Box::new(rust::RustTargets),
        Box::new(node::NodeModules),
        Box::new(build_dirs::BuildDirs),
        Box::new(scratch::Scratch),
        Box::new(ai_tools::AiTools),
        Box::new(caches::Caches),
        Box::new(compress::ColdFiles),
    ]
}

/// The project-tree walk the rules share, for callers outside a plan (the report).
pub fn tree_index(cfg: &Config) -> TreeIndex {
    let eff = cfg.effective(Level::Normal);
    let ctx = Ctx::new(cfg, &eff, PlanHints::default());
    ctx.index();
    ctx.index.into_inner().unwrap_or_default()
}

/// Run every rule and produce a plan. `on_rule` is called before each rule for progress display.
pub fn build_plan(
    cfg: &Config,
    level: Level,
    hints: PlanHints,
    mut on_rule: impl FnMut(&str),
) -> Plan {
    let eff = cfg.effective(level);
    let ctx = Ctx::new(cfg, &eff, hints);
    let mut plan = Plan {
        level: Some(level),
        created_at: crate::history::now(),
        ..Default::default()
    };
    let timed = |plan: &mut Plan, name: &str, t: Instant| {
        plan.timings.push(RuleTiming {
            rule: name.to_string(),
            secs: t.elapsed().as_secs_f64(),
        })
    };
    on_rule("project tree");
    let t = Instant::now();
    ctx.index();
    timed(&mut plan, "project tree", t);
    for rule in all_rules() {
        on_rule(rule.name());
        let t = Instant::now();
        rule.plan(&ctx, &mut plan);
        timed(&mut plan, rule.name(), t);
    }
    on_rule("docker");
    let t = Instant::now();
    plan.docker = crate::docker::plan(cfg, &eff);
    timed(&mut plan, "docker", t);
    plan.sort_by_size();
    plan
}

#[cfg(test)]
mod hint_tests {
    use super::PlanHints;

    const GIB: u64 = 1 << 30;

    #[test]
    fn fruitless_command_is_retried_once_its_target_grows() {
        let mut h = PlanHints::default();
        h.skip_commands.insert("go clean -cache".into(), 7 * GIB);
        // Same size, or a little bigger: still on cooldown.
        assert_eq!(h.should_skip("go clean -cache", 7 * GIB), Some(7 * GIB));
        assert!(h.should_skip("go clean -cache", 8 * GIB).is_some());
        // Refilled well past the size it had (the 2026-11-04 case: 7 GB -> 69 GB): run it.
        assert_eq!(h.should_skip("go clean -cache", 20 * GIB), None);
        // Small targets get at least 1 GiB of slack.
        h.skip_commands.insert("uv cache clean".into(), 100 << 20);
        assert!(h.should_skip("uv cache clean", GIB).is_some());
        assert_eq!(h.should_skip("uv cache clean", 2 * GIB), None);
        assert_eq!(h.should_skip("brew cleanup", 50 * GIB), None);
    }
}
