//! AI coding assistants keep more than they need: Codex stores every application version it ever
//! updated through and never removes them; both Codex and Claude Code keep full session logs.

use std::path::PathBuf;
use std::time::SystemTime;

use walkdir::WalkDir;

use super::{Ctx, Rule};
use crate::fs::{self, days, file_name, hours};
use crate::model::{ActionKind, Group, Plan};
use crate::paths::home;
use crate::platform;

pub struct AiTools;

impl Rule for AiTools {
    fn name(&self) -> &'static str {
        "AI tools"
    }

    fn plan(&self, ctx: &Ctx, plan: &mut Plan) {
        let h = home();
        // Old Codex releases (~250 MB each), keeping the current one plus N newest.
        let releases = h.join(".codex/packages/standalone/releases");
        let current = std::fs::read_link(h.join(".codex/packages/standalone/current"))
            .ok()
            .map(|p| file_name(&p))
            .unwrap_or_default();
        let mut rel: Vec<(SystemTime, PathBuf)> = std::fs::read_dir(&releases)
            .into_iter()
            .flatten()
            .flatten()
            .map(|e| e.path())
            .filter(|p| p.is_dir() && file_name(p) != current && !ctx.excluded(p))
            .map(|p| {
                (
                    std::fs::metadata(&p)
                        .and_then(|m| m.modified())
                        .unwrap_or(SystemTime::UNIX_EPOCH),
                    p,
                )
            })
            .collect();
        rel.sort_by_key(|r| std::cmp::Reverse(r.0));
        for (_, p) in rel.into_iter().skip(ctx.cfg.ai_tools.codex_keep_releases) {
            let b = fs::disk_usage(&p);
            plan.push(
                Group::Codex,
                ActionKind::Remove,
                p,
                b,
                "old Codex application version",
            );
        }
        // Abandoned runtime installs.
        for e in std::fs::read_dir(h.join(".cache/codex-runtimes"))
            .into_iter()
            .flatten()
            .flatten()
        {
            let p = e.path();
            if file_name(&p).starts_with("codex-runtime-install-")
                && !ctx.excluded(&p)
                && fs::idle_for(&p, hours(24), &[])
            {
                let b = fs::disk_usage(&p);
                plan.push(
                    Group::Codex,
                    ActionKind::Remove,
                    p,
                    b,
                    "abandoned Codex runtime installation",
                );
            }
        }
        let ai = &ctx.cfg.ai_tools;
        if ai.codex_sessions_delete_days > 0 {
            old_files(
                ctx,
                h.join(".codex/sessions"),
                ai.codex_sessions_delete_days,
                Group::Codex,
                "Codex session log past the retention limit",
                plan,
            );
        }
        if ai.claude_sessions_delete_days > 0 {
            old_files(
                ctx,
                h.join(".claude/projects"),
                ai.claude_sessions_delete_days,
                Group::Claude,
                "Claude Code session log past the retention limit",
                plan,
            );
        }
        let _ = platform::compression_supported(); // sessions are compressed by the ColdFiles rule
    }
}

fn old_files(ctx: &Ctx, root: PathBuf, d: u64, group: Group, reason: &str, plan: &mut Plan) {
    let cutoff = SystemTime::now() - days(d);
    for e in WalkDir::new(root)
        .into_iter()
        .filter_map(Result::ok)
        .filter(|e| e.file_type().is_file())
    {
        if ctx.excluded(e.path()) {
            continue;
        }
        if let Ok(m) = e.metadata() {
            if m.modified().map(|t| t < cutoff).unwrap_or(false) {
                plan.push(
                    group,
                    ActionKind::Remove,
                    e.path().to_path_buf(),
                    platform::file_usage(&m),
                    reason,
                );
            }
        }
    }
}
