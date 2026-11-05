//! Temporary directories: per-session scratch space of Claude Code, the user's own entries in
//! the system temp directory, and stale work directories in the per-user `$TMPDIR` (macOS).

use super::{Ctx, Rule};
use crate::fs::{self, days, file_name, hours};
use crate::model::{ActionKind, Group, Plan};
use crate::platform;

pub struct Scratch;

impl Rule for Scratch {
    fn name(&self) -> &'static str {
        "temporary directories"
    }

    fn plan(&self, ctx: &Ctx, plan: &mut Plan) {
        let claude_root = platform::claude_scratch_root();
        if let Some(root) = &claude_root {
            for proj in std::fs::read_dir(root).into_iter().flatten().flatten() {
                for sess in std::fs::read_dir(proj.path())
                    .into_iter()
                    .flatten()
                    .flatten()
                {
                    let p = sess.path();
                    if !p.is_dir() || ctx.excluded(&p) {
                        continue;
                    }
                    let stale = ctx.eff.claude_scratch_days;
                    if fs::idle_for(&p, days(stale), &[]) {
                        let b = fs::disk_usage(&p);
                        if b > 0 {
                            plan.push(
                                Group::Scratch,
                                ActionKind::Remove,
                                p,
                                b,
                                format!("Claude Code session silent for {stale} days"),
                            );
                        }
                    }
                }
            }
        }
        let tmp = platform::system_tmp_dir();
        for e in std::fs::read_dir(&tmp).into_iter().flatten().flatten() {
            let p = e.path();
            let n = file_name(&p);
            if n.starts_with('.')
                || n.starts_with("com.")
                || Some(&p) == claude_root.as_ref()
                || ctx.excluded(&p)
            {
                continue;
            }
            let Ok(meta) = std::fs::symlink_metadata(&p) else {
                continue;
            };
            if !platform::owned_by_current_user(&meta) {
                continue;
            }
            let stale = ctx.eff.tmp_stale_days;
            if fs::idle_for(&p, days(stale), &[]) {
                let b = fs::disk_usage(&p);
                if b > 0 {
                    plan.push(
                        Group::Tmp,
                        ActionKind::Remove,
                        p,
                        b,
                        format!("in the temp directory, unchanged for {stale} days"),
                    );
                }
            }
        }
        if let Some(utmp) = platform::user_tmp_dir() {
            let stale = ctx.eff.user_tmp_stale_hours;
            for e in std::fs::read_dir(&utmp).into_iter().flatten().flatten() {
                let p = e.path();
                if !is_user_tmp_candidate(&file_name(&p)) || ctx.excluded(&p) {
                    continue;
                }
                let Ok(meta) = std::fs::symlink_metadata(&p) else {
                    continue;
                };
                if !platform::owned_by_current_user(&meta) {
                    continue;
                }
                if fs::idle_for(&p, hours(stale), &[]) {
                    let b = fs::disk_usage(&p);
                    if b > 0 {
                        plan.push(
                            Group::Tmp,
                            ActionKind::Remove,
                            p,
                            b,
                            format!("in $TMPDIR, unchanged for {stale} h"),
                        );
                    }
                }
            }
        }
    }
}

/// Entries of `$TMPDIR` that are ours to clear: not hidden, and not the per-daemon folders the
/// system and apps keep there (`com.apple.*`, `TemporaryItems`, `TemporaryDirectory.*`).
fn is_user_tmp_candidate(name: &str) -> bool {
    !(name.starts_with('.')
        || name.starts_with("com.apple.")
        || name.starts_with("TemporaryDirectory.")
        || name == "TemporaryItems"
        || !name.contains(|c: char| c.is_ascii_alphanumeric()))
}

#[cfg(test)]
mod tests {
    use super::is_user_tmp_candidate;

    #[test]
    fn user_tmp_skips_system_entries() {
        for ok in [
            "go-build2876470175",
            "remotion-webpack-bundle-abc",
            "playwright_chromiumdev_profile-x",
            "sWClxugmGJ9TkW8B_zIlx",
        ] {
            assert!(is_user_tmp_candidate(ok), "{ok}");
        }
        for no in [
            ".X11-unix",
            "com.apple.replayd",
            "TemporaryItems",
            "TemporaryDirectory.Ab12Cd",
        ] {
            assert!(!is_user_tmp_candidate(no), "{no}");
        }
    }
}
