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

fn write_aged(p: &Path, bytes: usize, age: Duration) {
    fs::create_dir_all(p.parent().unwrap()).unwrap();
    fs::write(p, vec![b'x'; bytes]).unwrap();
    let t = SystemTime::now() - age;
    filetime::set_file_mtime(p, filetime::FileTime::from_system_time(t)).unwrap();
}

/// A Cargo target whose every file was last written `age` ago.
fn cargo_target(dir: &Path, bytes: usize, age: Duration) {
    write_aged(&dir.join(".rustc_info.json"), 10, age);
    write_aged(&dir.join("debug/deps/libx.rlib"), bytes, age);
}

const HOUR: Duration = Duration::from_secs(3600);

#[test]
fn targets_over_the_total_budget_go_least_recently_built_first() {
    let d = tempfile::tempdir().unwrap();
    let root = d.path();
    cargo_target(&root.join("a/target"), 500_000, HOUR * 72);
    cargo_target(&root.join("b/target"), 500_000, HOUR * 48);
    cargo_target(&root.join("c/target"), 500_000, Duration::ZERO);
    let mut cfg = config_for(root);
    cfg.rust.total_max_gb = 1_300_000.0 / (1u64 << 30) as f64;
    let plan = plan_for(&cfg, Level::Normal);
    let removed: Vec<_> = plan
        .actions
        .iter()
        .filter(|a| a.group == Group::Rust && a.kind == ActionKind::Remove)
        .map(|a| a.path.clone())
        .collect();
    assert_eq!(removed, vec![root.join("a/target")], "{:#?}", plan.actions);

    // Under the budget nothing goes, however idle.
    cfg.rust.total_max_gb = 1.0;
    let plan = plan_for(&cfg, Level::Normal);
    assert!(plan.actions.iter().all(|a| a.group != Group::Rust));
}

#[test]
fn budget_never_takes_a_target_that_is_being_built() {
    let d = tempfile::tempdir().unwrap();
    let root = d.path();
    cargo_target(&root.join("busy/target"), 500_000, Duration::ZERO);
    let mut cfg = config_for(root);
    cfg.rust.total_max_gb = 100_000.0 / (1u64 << 30) as f64;
    let plan = plan_for(&cfg, Level::Normal);
    assert!(plan.actions.iter().all(|a| a.group != Group::Rust));
    assert!(
        plan.notes.iter().any(|n| n.contains("budget")),
        "{:?}",
        plan.notes
    );
}

fn git(dir: &Path, args: &[&str]) {
    let st = std::process::Command::new("git")
        .args(["-c", "user.name=t", "-c", "user.email=t@t", "-C"])
        .arg(dir)
        .args(args)
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .status()
        .unwrap();
    assert!(st.success(), "git {args:?}");
}

#[test]
fn target_of_a_merged_worktree_goes_once_idle() {
    if std::process::Command::new("git")
        .arg("--version")
        .output()
        .is_err()
    {
        return;
    }
    let d = tempfile::tempdir().unwrap();
    let root = d.path();
    let repo = root.join("repo");
    fs::create_dir_all(&repo).unwrap();
    git(&repo, &["init", "-q", "-b", "main"]);
    fs::write(repo.join("Cargo.toml"), "[package]\n").unwrap();
    git(&repo, &["add", "."]);
    git(&repo, &["commit", "-q", "-m", "init"]);
    git(
        &repo,
        &["worktree", "add", "-q", "-b", "done", ".worktrees/done"],
    );
    git(
        &repo,
        &["worktree", "add", "-q", "-b", "wip", ".worktrees/wip"],
    );
    let wip = repo.join(".worktrees/wip");
    fs::write(wip.join("new.rs"), "fn main() {}\n").unwrap();
    git(&wip, &["add", "."]);
    git(&wip, &["commit", "-q", "-m", "work in progress"]);

    cargo_target(&repo.join(".worktrees/done/target"), 50_000, HOUR * 24);
    cargo_target(&wip.join("target"), 50_000, HOUR * 24);
    cargo_target(&repo.join("target"), 50_000, HOUR * 24); // the main checkout: never "merged"
                                                           // A submodule has a `.git` file too, and its HEAD usually sits on the main branch.
    fs::create_dir_all(repo.join("vendor/lib")).unwrap();
    fs::write(
        repo.join("vendor/lib/.git"),
        "gitdir: ../../.git/modules/lib\n",
    )
    .unwrap();
    cargo_target(&repo.join("vendor/lib/target"), 50_000, HOUR * 24);

    let plan = plan_for(&config_for(root), Level::Normal);
    let removed: Vec<_> = plan
        .actions
        .iter()
        .filter(|a| a.group == Group::Rust)
        .map(|a| (a.path.clone(), a.reason.clone()))
        .collect();
    assert_eq!(removed.len(), 1, "{removed:#?}");
    assert!(removed[0].0.ends_with(".worktrees/done/target"));
    assert!(removed[0].1.contains("merged into `main`"));
    assert!(plan.notes.iter().any(|n| n.contains("git worktree remove")));

    // Built an hour ago: still in use, kept.
    cargo_target(&repo.join(".worktrees/done/target"), 50_000, HOUR);
    let plan = plan_for(&config_for(root), Level::Normal);
    assert!(plan.actions.iter().all(|a| a.group != Group::Rust));
}

#[test]
fn discovery_skips_cargo_output_but_searches_user_folders() {
    let d = tempfile::tempdir().unwrap();
    let root = d.path();
    cargo_target(&root.join("app/target"), 10, Duration::ZERO);
    write_aged(&root.join("app/Cargo.toml"), 10, Duration::ZERO);
    write_aged(
        &root.join("app/target/debug/build/x/out/gen.rs"),
        10,
        Duration::ZERO,
    );
    // A user folder called `build`, with a project inside it.
    write_aged(
        &root.join("photos/build/site/package.json"),
        10,
        Duration::ZERO,
    );
    write_aged(
        &root.join("photos/build/site/node_modules/x/i.js"),
        10,
        Duration::ZERO,
    );
    let idx = rules::tree_index(&config_for(root));
    assert_eq!(idx.cargo_targets, vec![root.join("app/target")]);
    assert_eq!(idx.named("build"), &[root.join("photos/build")]);
    assert_eq!(
        idx.named("node_modules"),
        &[root.join("photos/build/site/node_modules")]
    );
}

#[test]
fn plan_records_how_long_each_rule_took() {
    let d = tempfile::tempdir().unwrap();
    let plan = plan_for(&config_for(d.path()), Level::Normal);
    let rules: Vec<_> = plan.timings.iter().map(|t| t.rule.as_str()).collect();
    assert_eq!(rules.first(), Some(&"project tree"));
    assert!(rules.contains(&"rust targets"));
    assert!(rules.contains(&"docker"));
}
