//! Tool caches that regrow on demand: Chrome update clones, VS Code update leftovers, Go / npm /
//! uv / sccache caches past a size cap, Metal shader caches, Homebrew downloads, Xcode
//! DerivedData, simulator caches.

use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime};

use super::{Ctx, Rule};
use crate::bytes::{from_gib, human};
use crate::fs::{self, days, hours};
use crate::model::{ActionKind, Group, Plan};
use crate::paths::home;
use crate::platform::{self, which};
use crate::proc;

pub struct Caches;

fn command(
    ctx: &Ctx,
    plan: &mut Plan,
    argv: &[&str],
    measure: PathBuf,
    bytes: u64,
    reason: impl Into<String>,
) {
    let line = argv.join(" ");
    if let Some(was) = ctx.hints.should_skip(&line, bytes) {
        plan.note(format!(
            "skipped `{line}`: it freed nothing last time at {} (retried after 7 days, \
             or sooner if the target grows)",
            human(was)
        ));
        return;
    }
    plan.push(
        Group::Cache,
        ActionKind::Command {
            argv: argv.iter().map(|s| s.to_string()).collect(),
            measure: Some(measure.clone()),
        },
        measure,
        bytes,
        reason,
    );
}

impl Rule for Caches {
    fn name(&self) -> &'static str {
        "tool caches"
    }

    fn plan(&self, ctx: &Ctx, plan: &mut Plan) {
        let h = home();
        let c = &ctx.cfg.caches;

        if let Some(clones) = platform::chrome_clone_dir() {
            for e in std::fs::read_dir(&clones).into_iter().flatten().flatten() {
                let p = e.path();
                if p.is_dir()
                    && !ctx.excluded(&p)
                    && fs::idle_for(&p, hours(c.chrome_clone_hours), &[])
                {
                    let b = fs::disk_usage(&p);
                    plan.push(
                        Group::Cache,
                        ActionKind::Remove,
                        p,
                        b,
                        "Chrome code-signing clone left after an update",
                    );
                }
            }
        }

        if c.vscode_shipit {
            let ship = if cfg!(target_os = "macos") {
                Some(h.join("Library/Caches/com.microsoft.VSCode.ShipIt"))
            } else {
                None
            };
            if let Some(ship) = ship {
                for e in std::fs::read_dir(&ship).into_iter().flatten().flatten() {
                    let p = e.path();
                    if !ctx.excluded(&p) && fs::idle_for(&p, hours(24), &[]) {
                        let b = fs::disk_usage(&p);
                        if b > 0 {
                            plan.push(
                                Group::Cache,
                                ActionKind::Remove,
                                p,
                                b,
                                "VS Code update leftover",
                            );
                        }
                    }
                }
            }
        }

        if which("go") {
            if let Some(gocache) = go_cache_dir() {
                let b = fs::disk_usage(&gocache);
                if b > from_gib(c.go_cache_max_gb) {
                    command(
                        ctx,
                        plan,
                        &["go", "clean", "-cache"],
                        gocache,
                        b,
                        format!(
                            "Go build cache is {} (cap {:.0} GB)",
                            human(b),
                            c.go_cache_max_gb
                        ),
                    );
                }
            }
        }
        let npm = h.join(".npm/_cacache");
        if npm.is_dir() && which("npm") {
            let b = fs::disk_usage(&npm);
            if b > from_gib(c.npm_cache_max_gb) {
                command(
                    ctx,
                    plan,
                    &["npm", "cache", "clean", "--force"],
                    npm,
                    b,
                    format!(
                        "npm cache is {} (cap {:.0} GB)",
                        human(b),
                        c.npm_cache_max_gb
                    ),
                );
            }
        }
        let uv = h.join(".cache/uv");
        if uv.is_dir() && which("uv") {
            let b = fs::disk_usage(&uv);
            if b > from_gib(c.uv_cache_max_gb) {
                command(
                    ctx,
                    plan,
                    &["uv", "cache", "clean"],
                    uv,
                    b,
                    format!("uv cache is {} (cap {:.0} GB)", human(b), c.uv_cache_max_gb),
                );
            }
        }
        if let Some(sc) = platform::sccache_dir().filter(|d| d.is_dir()) {
            if !ctx.excluded(&sc) {
                plan_sccache(
                    &sc,
                    from_gib(c.sccache_max_gb),
                    ctx.eff.cache_sweep_days,
                    plan,
                );
            }
        }
        if let Some(root) = platform::user_cache_dir() {
            plan_shader_caches(ctx, &root, from_gib(c.shader_cache_max_gb), plan);
        }
        if which("pnpm") {
            let store = h.join(".local/share/pnpm");
            command(
                ctx,
                plan,
                &["pnpm", "store", "prune"],
                store,
                0,
                "unreferenced packages in the pnpm store",
            );
        }
        if which("brew") {
            let bc = h.join("Library/Caches/Homebrew");
            command(
                ctx,
                plan,
                &[
                    "brew",
                    "cleanup",
                    "-s",
                    &format!("--prune={}", c.brew_prune_days),
                ],
                bc,
                0,
                "old Homebrew downloads",
            );
        }

        if cfg!(target_os = "macos") {
            let derived = h.join("Library/Developer/Xcode/DerivedData");
            for e in std::fs::read_dir(&derived).into_iter().flatten().flatten() {
                let p = e.path();
                if p.is_dir()
                    && !ctx.excluded(&p)
                    && fs::idle_for(&p, days(c.xcode_derived_days), &[])
                {
                    let b = fs::disk_usage(&p);
                    plan.push(
                        Group::Cache,
                        ActionKind::Remove,
                        p,
                        b,
                        format!(
                            "Xcode DerivedData untouched for {} days",
                            c.xcode_derived_days
                        ),
                    );
                }
            }
            for sub in [
                "Library/Developer/CoreSimulator/Caches",
                "Library/Developer/CoreSimulator/Temp",
            ] {
                for e in std::fs::read_dir(h.join(sub))
                    .into_iter()
                    .flatten()
                    .flatten()
                {
                    let p = e.path();
                    if !ctx.excluded(&p) && fs::idle_for(&p, days(7), &[]) {
                        let b = fs::disk_usage(&p);
                        if b > 0 {
                            plan.push(
                                Group::Cache,
                                ActionKind::Remove,
                                p,
                                b,
                                "iOS simulator cache",
                            );
                        }
                    }
                }
            }
            if which("xcrun") {
                command(
                    ctx,
                    plan,
                    &["xcrun", "simctl", "delete", "unavailable"],
                    h.join("Library/Developer/CoreSimulator/Devices"),
                    0,
                    "simulators whose runtime is gone",
                );
            }
        }
    }
}

fn go_cache_dir() -> Option<PathBuf> {
    let out = proc::stdout(&["go", "env", "GOCACHE"], Duration::from_secs(15))?;
    let p = PathBuf::from(out.trim());
    p.is_dir().then_some(p)
}

/// sccache evicts by itself only at 10 GB. Above `cap`, entries older than `sweep_days` go —
/// all of it, in effect, when nothing compiled through it for that long.
fn plan_sccache(dir: &Path, cap: u64, sweep_days: u64, plan: &mut Plan) {
    let cutoff = SystemTime::now() - days(sweep_days);
    let (mut total, mut old) = (0u64, 0u64);
    fs::visit(dir, |_, m| {
        let usage = platform::file_usage(m);
        total += usage;
        if m.is_file() && m.modified().map(|t| t < cutoff).unwrap_or(false) {
            old += usage;
        }
    });
    if total > cap && old > 0 {
        plan.push(
            Group::Cache,
            ActionKind::SweepOlderThan { days: sweep_days },
            dir.to_path_buf(),
            old,
            format!(
                "sccache cache is {} (cap {}): entries unused for {sweep_days} days",
                human(total),
                human(cap)
            ),
        );
    }
}

/// Where Metal keeps compiled shaders inside an application's folder of the system cache.
const SHADER_DIRS: &[&str] = &[
    "com.apple.metal",
    "com.apple.metalfe",
    "com.apple.gpuarchiver",
];

/// Compiled Metal shaders the system keeps per application under `root`. They grow without bound
/// for 3D tools (Blender reached 4 GB) and are rebuilt on the next launch. Apple's own folders are
/// left alone, and so is an application that compiled shaders within the last day.
fn plan_shader_caches(ctx: &Ctx, root: &Path, cap: u64, plan: &mut Plan) {
    for app in std::fs::read_dir(root).into_iter().flatten().flatten() {
        let app = app.path();
        let name = fs::file_name(&app);
        if name.starts_with("com.apple.") || !app.is_dir() {
            continue;
        }
        let dirs: Vec<(PathBuf, u64)> = SHADER_DIRS
            .iter()
            .map(|d| app.join(d))
            .filter(|d| d.is_dir() && !ctx.excluded(d))
            .map(|d| {
                let b = fs::disk_usage(&d);
                (d, b)
            })
            .collect();
        let total: u64 = dirs.iter().map(|(_, b)| b).sum();
        if total <= cap {
            continue;
        }
        if !dirs.iter().all(|(d, _)| fs::idle_for(d, hours(24), &[])) {
            plan.note(format!(
                "kept the Metal shader cache of {name} ({}): it compiled shaders within the last day",
                human(total)
            ));
            continue;
        }
        for (d, b) in dirs {
            plan.push(
                Group::Cache,
                ActionKind::Remove,
                d,
                b,
                format!(
                    "Metal shader cache of {name} is {} (cap {}); rebuilt on the next launch",
                    human(total),
                    human(cap)
                ),
            );
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::Config;
    use crate::model::Level;
    use crate::rules::PlanHints;

    fn write(p: &Path, bytes: usize, age: Duration) {
        std::fs::create_dir_all(p.parent().unwrap()).unwrap();
        std::fs::write(p, vec![b'x'; bytes]).unwrap();
        let t = SystemTime::now() - age;
        filetime::set_file_mtime(p, filetime::FileTime::from_system_time(t)).unwrap();
    }

    #[test]
    fn oversized_shader_caches_of_idle_apps_go() {
        let d = tempfile::tempdir().unwrap();
        let root = d.path();
        write(
            &root.join("org.blender/com.apple.metal/a.data"),
            300_000,
            days(3),
        );
        write(
            &root.join("org.blender/com.apple.gpuarchiver/b"),
            300_000,
            days(3),
        );
        write(
            &root.join("com.busy.app/com.apple.metal/a.data"),
            600_000,
            hours(1),
        );
        write(
            &root.join("com.apple.Safari/com.apple.metal/a.data"),
            600_000,
            days(30),
        );
        write(
            &root.join("small.app/com.apple.metal/a.data"),
            1_000,
            days(30),
        );
        let cfg = Config::default();
        let eff = cfg.effective(Level::Normal);
        let ctx = Ctx::new(&cfg, &eff, PlanHints::default());
        let mut plan = Plan::default();
        plan_shader_caches(&ctx, root, 500_000, &mut plan);
        let mut removed: Vec<_> = plan.actions.iter().map(|a| a.path.clone()).collect();
        removed.sort();
        assert_eq!(
            removed,
            vec![
                root.join("org.blender/com.apple.gpuarchiver"),
                root.join("org.blender/com.apple.metal"),
            ]
        );
        assert!(plan.notes.iter().any(|n| n.contains("com.busy.app")));
    }

    #[test]
    fn sccache_over_cap_loses_old_entries_only() {
        let d = tempfile::tempdir().unwrap();
        write(&d.path().join("a/old"), 200_000, days(30));
        write(&d.path().join("b/new"), 200_000, days(1));
        let mut plan = Plan::default();
        plan_sccache(d.path(), 1_000_000, 7, &mut plan);
        assert!(plan.actions.is_empty(), "under the cap nothing happens");
        plan_sccache(d.path(), 100_000, 7, &mut plan);
        let a = &plan.actions[0];
        assert!(matches!(a.kind, ActionKind::SweepOlderThan { days: 7 }));
        assert!(a.bytes >= 200_000 && a.bytes < 400_000, "{}", a.bytes);
    }
}
