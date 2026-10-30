//! Tool caches that regrow on demand: Chrome update clones, VS Code update leftovers, Go / npm /
//! uv caches past a size cap, Homebrew downloads, Xcode DerivedData, simulator caches.

use std::path::PathBuf;

use super::{Ctx, Rule};
use crate::bytes::{from_gib, human};
use crate::fs::{self, days, hours};
use crate::model::{ActionKind, Group, Plan};
use crate::paths::home;
use crate::platform::{self, which};

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
    if ctx.hints.skip_commands.contains(&line) {
        plan.note(format!(
            "skipped `{line}`: it freed nothing last time (retried after 7 days)"
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
    let out = std::process::Command::new("go")
        .args(["env", "GOCACHE"])
        .output()
        .ok()?;
    let p = PathBuf::from(String::from_utf8_lossy(&out.stdout).trim());
    p.is_dir().then_some(p)
}
