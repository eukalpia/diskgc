//! Cargo `target/` directories. Cargo never garbage-collects them: every feature set, every
//! profile and every toolchain version adds another full copy of every dependency, and test
//! binaries with full debug info weigh 100–200 MB each. Agents running `cargo test` in a loop can
//! produce 100 GB in two days.

use std::time::SystemTime;

use std::path::Path;

use super::{Ctx, Rule};
use crate::bytes::{from_gib, gib, human};
use crate::fs::{self, days, hours, walk_bytes};
use crate::model::{ActionKind, Group, Plan};

pub struct RustTargets;

const BIG_BINARY: u64 = 30 * 1024 * 1024;

/// Extension-less executables in `deps/` larger than 30 MB, older than a day: test binaries.
pub fn is_big_binary(path: &Path, m: &std::fs::Metadata) -> bool {
    let day_ago = SystemTime::now() - hours(24);
    m.is_file()
        && m.len() > BIG_BINARY
        && !fs::file_name(path).contains('.')
        && path
            .parent()
            .map(|p| fs::file_name(p) == "deps")
            .unwrap_or(false)
        && m.modified().map(|t| t < day_ago).unwrap_or(false)
}

impl Rule for RustTargets {
    fn name(&self) -> &'static str {
        "rust targets"
    }

    fn plan(&self, ctx: &Ctx, plan: &mut Plan) {
        let eff = ctx.eff;
        let max = from_gib(eff.rust_target_max_gb);
        for t in fs::find_cargo_targets(&ctx.target_roots(), ctx.cfg.scan.depth) {
            if ctx.excluded(&t) {
                continue;
            }
            let total = fs::disk_usage(&t);
            if total == 0 {
                continue;
            }
            // Hard cap: above twice the limit, everything older than a day goes even while a
            // build is running — otherwise an always-building agent has no upper bound.
            let over_hard_cap = total > max * 2;
            let sweep_days = if over_hard_cap {
                1
            } else {
                eff.rust_sweep_days
            };
            let cutoff = SystemTime::now() - days(sweep_days);
            let old = walk_bytes(&t, |_, m| {
                m.is_file() && m.modified().map(|x| x < cutoff).unwrap_or(false)
            });
            let mut remaining = total.saturating_sub(old);
            if old > 0 {
                let why = if over_hard_cap {
                    format!(
                        "target is {} (> {} hard cap): everything older than a day",
                        human(total),
                        human(max * 2)
                    )
                } else {
                    format!("build artifacts untouched for {sweep_days} days")
                };
                plan.push(
                    Group::Rust,
                    ActionKind::SweepOlderThan { days: sweep_days },
                    t.clone(),
                    old,
                    why,
                );
            }
            if remaining <= max {
                continue;
            }
            for profile in std::fs::read_dir(&t).into_iter().flatten().flatten() {
                let inc = profile.path().join("incremental");
                if inc.is_dir() {
                    let b = walk_bytes(&inc, |_, m| {
                        m.is_file() && m.modified().map(|x| x >= cutoff).unwrap_or(true)
                    });
                    remaining = remaining.saturating_sub(b);
                    plan.push(
                        Group::Rust,
                        ActionKind::Remove,
                        inc,
                        b,
                        format!(
                            "target > {:.0} GB: incremental compilation cache",
                            eff.rust_target_max_gb
                        ),
                    );
                }
            }
            let bins = walk_bytes(&t, |p, m| {
                is_big_binary(p, m) && m.modified().map(|x| x >= cutoff).unwrap_or(false)
            });
            if bins > 0 {
                remaining = remaining.saturating_sub(bins);
                plan.push(
                    Group::Rust,
                    ActionKind::SweepBigBinaries,
                    t.clone(),
                    bins,
                    "test binaries over 30 MB older than a day",
                );
            }
            if remaining > max {
                if fs::idle_for(&t, hours(eff.rust_idle_hours), &[]) {
                    plan.push(
                        Group::Rust,
                        ActionKind::Remove,
                        t.clone(),
                        remaining,
                        format!(
                            "still > {:.0} GB and idle for {} h: full clean",
                            eff.rust_target_max_gb, eff.rust_idle_hours
                        ),
                    );
                } else {
                    plan.note(format!(
                        "kept {} ({:.1} GB): a build touched it within the last {} h",
                        t.display(),
                        gib(remaining),
                        eff.rust_idle_hours
                    ));
                }
            }
        }
    }
}
