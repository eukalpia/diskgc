//! Cold text files (session logs, JSONL histories) are compressed by the filesystem itself where
//! that is available. Nothing changes for the programs that read them; they just take 5–8× less
//! space on disk. Files found incompressible once are marked and not retried.

use std::time::SystemTime;

use walkdir::WalkDir;

use super::{Ctx, Rule};
use crate::fs::days;
use crate::model::{ActionKind, Group, Plan};
use crate::paths;
use crate::platform;

pub struct ColdFiles;

impl Rule for ColdFiles {
    fn name(&self) -> &'static str {
        "cold file compression"
    }

    fn plan(&self, ctx: &Ctx, plan: &mut Plan) {
        let c = &ctx.cfg.compress;
        if !c.enable || !platform::compression_supported() {
            return;
        }
        let cutoff = SystemTime::now() - days(c.min_age_days);
        let min = c.min_size_kb * 1024;
        for d in &c.dirs {
            let root = paths::expand(d);
            if !root.is_dir() {
                continue;
            }
            for e in WalkDir::new(&root)
                .follow_links(false)
                .into_iter()
                .filter_map(Result::ok)
            {
                if !e.file_type().is_file() {
                    continue;
                }
                let ext = e
                    .path()
                    .extension()
                    .map(|x| x.to_string_lossy().to_lowercase())
                    .unwrap_or_default();
                if !c.extensions.contains(&ext) {
                    continue;
                }
                let Ok(m) = e.metadata() else { continue };
                if m.len() < min
                    || platform::is_compressed(&m)
                    || m.modified().map(|t| t > cutoff).unwrap_or(true)
                {
                    continue;
                }
                if ctx.excluded(e.path()) || platform::has_nocompress_marker(e.path()) {
                    continue;
                }
                plan.push(
                    Group::Compress,
                    ActionKind::Compress,
                    e.path().to_path_buf(),
                    platform::file_usage(&m),
                    format!("cold file, unchanged for {} days", c.min_age_days),
                );
            }
        }
    }
}
