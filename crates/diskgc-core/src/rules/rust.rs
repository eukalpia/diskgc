//! Cargo `target/` directories. Cargo never garbage-collects them: every feature set, every
//! profile and every toolchain version adds another full copy of every dependency, and test
//! binaries with full debug info weigh 100–200 MB each. Agents running `cargo test` in a loop can
//! produce 100 GB in two days — or a dozen worktrees of 5–20 GB each, none of them big enough to
//! trip a per-directory cap.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime};

use super::{Ctx, Rule};
use crate::bytes::{from_gib, gib, human};
use crate::fs::{self, days, hours};
use crate::model::{ActionKind, Group, Plan};
use crate::{platform, proc};

pub struct RustTargets;

const BIG_BINARY: u64 = 30 * 1024 * 1024;
const GIT_TIMEOUT: Duration = Duration::from_secs(10);

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

/// Everything the rule needs from one target, gathered in a single walk. The sweep window depends
/// on the total size (above the hard cap it shrinks to a day), which is only known at the end, so
/// files are counted against both windows.
#[derive(Debug, Default)]
struct Scan {
    total: u64,
    /// Bytes of files older than the sweep window, and older than a day.
    old: u64,
    old_day: u64,
    /// Bytes of files newer than the sweep window / than a day, per `<profile>/incremental`.
    incremental: BTreeMap<PathBuf, u64>,
    incremental_day: BTreeMap<PathBuf, u64>,
    /// Fat test binaries newer than the sweep window.
    big_bins: u64,
    /// The newest file: when something last built here.
    newest: Option<SystemTime>,
}

impl Scan {
    fn of(target: &Path, sweep_cutoff: SystemTime, day_cutoff: SystemTime) -> Scan {
        let mut s = Scan::default();
        fs::visit(target, |p, m| {
            let usage = platform::file_usage(m);
            if m.is_dir() {
                s.total += usage;
                return;
            }
            if m.file_type().is_symlink() {
                return;
            }
            s.total += usage;
            let mtime = m.modified().ok();
            s.newest = s.newest.max(mtime);
            let older = |cutoff| mtime.map(|t| t < cutoff).unwrap_or(false);
            if older(sweep_cutoff) {
                s.old += usage;
            } else if is_big_binary(p, m) {
                s.big_bins += usage;
            }
            if older(day_cutoff) {
                s.old_day += usage;
            }
            if let Some(inc) = incremental_dir(target, p) {
                if !older(sweep_cutoff) {
                    *s.incremental.entry(inc.clone()).or_default() += usage;
                }
                if !older(day_cutoff) {
                    *s.incremental_day.entry(inc).or_default() += usage;
                }
            }
        });
        s
    }

    fn idle_since(&self, cutoff: SystemTime) -> bool {
        self.newest.map(|t| t < cutoff).unwrap_or(true)
    }
}

/// `<target>/<profile>/incremental` if `file` lies inside one.
fn incremental_dir(target: &Path, file: &Path) -> Option<PathBuf> {
    let mut parts = file.strip_prefix(target).ok()?.components();
    let profile = parts.next()?;
    (parts.next()?.as_os_str() == "incremental" && parts.next().is_some())
        .then(|| target.join(profile).join("incremental"))
}

/// A linked git worktree whose branch is fully merged into the repository's main branch.
struct MergedWorktree {
    root: PathBuf,
    branch: String,
    base: String,
}

/// The worktree `target` belongs to, if it is a linked one (its `.git` is a file naming a
/// `worktrees/` entry; the main checkout has a directory) and its `HEAD` is already contained in
/// `main` or `master`.
fn merged_worktree(target: &Path) -> Option<MergedWorktree> {
    let root = target
        .ancestors()
        .skip(1)
        .find(|a| a.join(".git").exists())?;
    // `gitdir: <repo>/.git/worktrees/<name>`; a submodule's file points into `.git/modules`.
    let link = std::fs::read_to_string(root.join(".git")).ok()?;
    if !link.replace('\\', "/").contains("/worktrees/") {
        return None;
    }
    let dir = root.to_string_lossy();
    let git = |args: &[&str]| {
        let mut argv = vec!["git", "-C", &dir];
        argv.extend_from_slice(args);
        proc::output(&argv, GIT_TIMEOUT)
            .ok()
            .filter(|o| o.status.success())
            .map(|o| String::from_utf8_lossy(&o.stdout).trim().to_string())
    };
    let base = ["main", "master"].into_iter().find(|b| {
        git(&[
            "rev-parse",
            "--verify",
            "--quiet",
            &format!("refs/heads/{b}"),
        ])
        .is_some()
    })?;
    let branch = git(&["symbolic-ref", "--short", "-q", "HEAD"]).unwrap_or_else(|| "HEAD".into());
    if branch == base {
        return None;
    }
    git(&["merge-base", "--is-ancestor", "HEAD", base])?;
    Some(MergedWorktree {
        root: root.to_path_buf(),
        branch,
        base: base.to_string(),
    })
}

/// One target and what the rule decided for it.
struct Target {
    path: PathBuf,
    scan: Scan,
    /// Partial cleanups that leave the directory usable: kind, path, bytes, reason.
    partial: Vec<(ActionKind, PathBuf, u64, String)>,
    /// Bytes left after the partial cleanups.
    remaining: u64,
    /// Set when the whole directory goes, with the reason.
    remove: Option<String>,
    note: Option<String>,
}

impl Rule for RustTargets {
    fn name(&self) -> &'static str {
        "rust targets"
    }

    fn plan(&self, ctx: &Ctx, plan: &mut Plan) {
        let eff = ctx.eff;
        let max = from_gib(eff.rust_target_max_gb);
        let now = SystemTime::now();
        let sweep_cutoff = now - days(eff.rust_sweep_days);
        let day_cutoff = now - days(1);
        let idle_cutoff = now - hours(eff.rust_idle_hours);
        let merged_cutoff = now - hours(eff.rust_merged_idle_hours);
        let git = platform::which("git");

        let mut targets = Vec::new();
        for t in &ctx.index().cargo_targets {
            if ctx.excluded(t) {
                continue;
            }
            let scan = Scan::of(t, sweep_cutoff, day_cutoff);
            if scan.total == 0 {
                continue;
            }
            let mut target = Target {
                path: t.clone(),
                remaining: scan.total,
                scan,
                partial: Vec::new(),
                remove: None,
                note: None,
            };
            plan_one(
                &mut target,
                max,
                eff.rust_sweep_days,
                eff.rust_idle_hours,
                idle_cutoff,
            );
            if target.remove.is_none() && git && target.scan.idle_since(merged_cutoff) {
                if let Some(wt) = merged_worktree(t) {
                    target.remove = Some(format!(
                        "worktree branch `{}` is merged into `{}` and nothing was built for {} h",
                        wt.branch, wt.base, eff.rust_merged_idle_hours
                    ));
                    plan.note(format!(
                        "{} (branch `{}`) is merged into `{}`: `git worktree remove` would free \
                         its sources and dependencies too",
                        wt.root.display(),
                        wt.branch,
                        wt.base
                    ));
                }
            }
            targets.push(target);
        }

        apply_budget(
            &mut targets,
            from_gib(eff.rust_total_max_gb),
            idle_cutoff,
            eff.rust_idle_hours,
            plan,
        );

        for t in targets {
            if let Some(why) = t.remove {
                plan.push(Group::Rust, ActionKind::Remove, t.path, t.scan.total, why);
                continue;
            }
            for (kind, path, bytes, why) in t.partial {
                plan.push(Group::Rust, kind, path, bytes, why);
            }
            if let Some(n) = t.note {
                plan.note(n);
            }
        }
    }
}

/// The per-directory rules: sweep old artifacts; above `max`, drop incremental caches and fat
/// test binaries; if still above and idle, the whole directory.
fn plan_one(t: &mut Target, max: u64, sweep_days: u64, idle_hours: u64, idle_cutoff: SystemTime) {
    let s = &t.scan;
    // Hard cap: above twice the limit, everything older than a day goes even while a build is
    // running — otherwise an always-building agent has no upper bound.
    let over_hard_cap = s.total > max * 2;
    let (days, old, incremental, bins) = if over_hard_cap {
        (1, s.old_day, &s.incremental_day, 0)
    } else {
        (sweep_days, s.old, &s.incremental, s.big_bins)
    };
    t.remaining = s.total.saturating_sub(old);
    if old > 0 {
        let why = if over_hard_cap {
            format!(
                "target is {} (> {} hard cap): everything older than a day",
                human(s.total),
                human(max * 2)
            )
        } else {
            format!("build artifacts untouched for {days} days")
        };
        t.partial.push((
            ActionKind::SweepOlderThan { days },
            t.path.clone(),
            old,
            why,
        ));
    }
    if t.remaining <= max {
        return;
    }
    for (inc, &b) in incremental {
        if b == 0 {
            continue;
        }
        t.remaining = t.remaining.saturating_sub(b);
        t.partial.push((
            ActionKind::Remove,
            inc.clone(),
            b,
            format!("target > {}: incremental compilation cache", human(max)),
        ));
    }
    if bins > 0 {
        t.remaining = t.remaining.saturating_sub(bins);
        t.partial.push((
            ActionKind::SweepBigBinaries,
            t.path.clone(),
            bins,
            "test binaries over 30 MB older than a day".into(),
        ));
    }
    if t.remaining <= max {
        return;
    }
    if s.idle_since(idle_cutoff) {
        t.remove = Some(format!(
            "still > {} and idle for {idle_hours} h: full clean",
            human(max)
        ));
    } else {
        t.note = Some(format!(
            "kept {} ({:.1} GB): a build touched it within the last {idle_hours} h",
            t.path.display(),
            gib(t.remaining),
        ));
    }
}

/// Keep all targets together under `budget`: whole directories go, least recently built first,
/// as long as nothing was built in them for `idle_hours`.
fn apply_budget(
    targets: &mut [Target],
    budget: u64,
    idle_cutoff: SystemTime,
    idle_hours: u64,
    plan: &mut Plan,
) {
    let total: u64 = targets
        .iter()
        .filter(|t| t.remove.is_none())
        .map(|t| t.remaining)
        .sum();
    if total <= budget {
        return;
    }
    let mut order: Vec<usize> = (0..targets.len())
        .filter(|&i| targets[i].remove.is_none() && targets[i].remaining > 0)
        .collect();
    order.sort_by_key(|&i| targets[i].scan.newest);
    let mut kept = total;
    for i in order {
        if kept <= budget {
            break;
        }
        let t = &mut targets[i];
        if !t.scan.idle_since(idle_cutoff) {
            continue;
        }
        kept -= t.remaining;
        t.remove = Some(format!(
            "all targets together are {} (budget {}): least recently built, idle for {idle_hours} h",
            human(total),
            human(budget)
        ));
        t.note = None;
    }
    if kept > budget {
        plan.note(format!(
            "Rust targets still total {} after cleanup (budget {}): the rest were built within \
             the last {idle_hours} h",
            human(kept),
            human(budget)
        ));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn incremental_dir_is_recognised_two_levels_down() {
        let t = Path::new("/p/target");
        assert_eq!(
            incremental_dir(
                t,
                Path::new("/p/target/debug/incremental/crate-1/s-1/dep-graph.bin")
            ),
            Some(PathBuf::from("/p/target/debug/incremental"))
        );
        assert_eq!(
            incremental_dir(t, Path::new("/p/target/debug/deps/libx.rlib")),
            None
        );
        assert_eq!(
            incremental_dir(t, Path::new("/p/target/debug/incremental")),
            None
        );
    }
}
