//! Temporary directories: per-session scratch space of Claude Code and the user's own entries in
//! the system temp directory.

use super::{Ctx, Rule};
use crate::fs::{self, days, file_name};
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
    }
}
