//! End-to-end rule tests against a synthetic home directory.

use std::fs;
use std::path::Path;
use std::time::{Duration, SystemTime};

use diskgc_core::config::Config;
use diskgc_core::model::{ActionKind, Group, Level};
use diskgc_core::rules;

fn write(p: &Path, bytes: usize, age_days: u64) {
    fs::create_dir_all(p.parent().unwrap()).unwrap();
    fs::write(p, vec![b'x'; bytes]).unwrap();
    let t = SystemTime::now() - Duration::from_secs(age_days * 86_400);
    filetime::set_file_mtime(p, filetime::FileTime::from_system_time(t)).unwrap();
}

fn config_for(root: &Path) -> Config {
    let mut cfg = Config::default();
    cfg.scan.roots = vec![root.display().to_string()];
    cfg.scan.extra_target_roots = vec![];
    cfg.docker.enable = false;
    cfg.compress.enable = false;
    cfg.caches.vscode_shipit = false;
    cfg
}

fn plan_for(cfg: &Config, level: Level) -> diskgc_core::model::Plan {
    rules::build_plan(cfg, level, rules::PlanHints::default(), |_| {})
}

#[test]
fn stale_node_modules_are_removed_but_active_ones_kept() {
    let d = tempfile::tempdir().unwrap();
    let root = d.path();
    write(&root.join("old/package.json"), 10, 60);
    write(&root.join("old/src/index.js"), 10, 60);
    write(&root.join("old/node_modules/dep/index.js"), 50_000, 60);
    write(&root.join("fresh/package.json"), 10, 60);
    write(&root.join("fresh/src/index.js"), 10, 1);
    write(&root.join("fresh/node_modules/dep/index.js"), 50_000, 60);
    let plan = plan_for(&config_for(root), Level::Normal);
    let removed: Vec<_> = plan
        .actions
        .iter()
        .filter(|a| a.group == Group::Node)
        .map(|a| a.path.clone())
        .collect();
    assert_eq!(removed, vec![root.join("old/node_modules")]);
}

#[test]
fn worktree_is_judged_on_its_own_not_on_the_parent_repo() {
    let d = tempfile::tempdir().unwrap();
    let root = d.path();
    write(&root.join("repo/package.json"), 10, 60);
    write(&root.join("repo/src/app.js"), 10, 0); // parent repo is active
    fs::create_dir_all(root.join("repo/.git")).unwrap();
    write(&root.join("repo/.worktrees/feat/package.json"), 10, 60);
    write(&root.join("repo/.worktrees/feat/.git"), 10, 60); // worktree marker file
    write(&root.join("repo/.worktrees/feat/src/app.js"), 10, 60);
    write(
        &root.join("repo/.worktrees/feat/node_modules/x/i.js"),
        50_000,
        60,
    );
    let plan = plan_for(&config_for(root), Level::Normal);
    assert!(plan
        .actions
        .iter()
        .any(|a| a.path == root.join("repo/.worktrees/feat/node_modules")));
}

#[test]
fn monorepo_package_follows_the_root_activity() {
    let d = tempfile::tempdir().unwrap();
    let root = d.path();
    write(&root.join("mono/package.json"), 10, 60);
    write(&root.join("mono/pnpm-workspace.yaml"), 10, 60);
    write(&root.join("mono/apps/web/src/a.ts"), 10, 0); // active elsewhere in the monorepo
    write(&root.join("mono/apps/bot/package.json"), 10, 60);
    write(&root.join("mono/apps/bot/node_modules/x/i.js"), 50_000, 60);
    let plan = plan_for(&config_for(root), Level::Normal);
    assert!(
        plan.actions.iter().all(|a| a.group != Group::Node),
        "bot's node_modules must survive: the monorepo is active"
    );
}

#[test]
fn cargo_target_sweeps_old_artifacts_and_caps_size() {
    let d = tempfile::tempdir().unwrap();
    let root = d.path();
    write(&root.join("app/Cargo.toml"), 10, 1);
    write(&root.join("app/target/.rustc_info.json"), 10, 1);
    write(&root.join("app/target/debug/deps/libold.rlib"), 100_000, 20);
    write(&root.join("app/target/debug/deps/libnew.rlib"), 100_000, 1);
    let plan = plan_for(&config_for(root), Level::Normal);
    let sweep = plan
        .actions
        .iter()
        .find(|a| a.group == Group::Rust)
        .expect("a sweep action");
    assert!(matches!(sweep.kind, ActionKind::SweepOlderThan { days: 7 }));
    assert!(
        sweep.bytes >= 100_000 && sweep.bytes < 200_000,
        "only the old artifact is counted, got {}",
        sweep.bytes
    );

    // Critical level shortens the window to a day: still only the old file qualifies.
    let plan = plan_for(&config_for(root), Level::Critical);
    let sweep = plan
        .actions
        .iter()
        .find(|a| a.group == Group::Rust)
        .unwrap();
    assert!(matches!(sweep.kind, ActionKind::SweepOlderThan { days: 1 }));
}

#[test]
fn build_dir_without_project_marker_is_left_alone() {
    let d = tempfile::tempdir().unwrap();
    let root = d.path();
    write(&root.join("photos/build/img.jpg"), 50_000, 90); // a user folder called "build"
    write(&root.join("flutter_app/pubspec.yaml"), 10, 90);
    write(&root.join("flutter_app/build/app.apk"), 50_000, 90);
    let plan = plan_for(&config_for(root), Level::Normal);
    let build: Vec<_> = plan
        .actions
        .iter()
        .filter(|a| a.group == Group::Build)
        .map(|a| a.path.clone())
        .collect();
    assert_eq!(build, vec![root.join("flutter_app/build")]);
}

#[test]
fn excluded_paths_are_never_touched() {
    let d = tempfile::tempdir().unwrap();
    let root = d.path();
    write(&root.join("keep/package.json"), 10, 90);
    write(&root.join("keep/node_modules/x/i.js"), 50_000, 90);
    let mut cfg = config_for(root);
    cfg.scan.exclude = vec![root.join("keep").display().to_string()];
    let plan = plan_for(&cfg, Level::Critical);
    assert!(plan
        .actions
        .iter()
        .all(|a| !a.path.starts_with(root.join("keep"))));
}
