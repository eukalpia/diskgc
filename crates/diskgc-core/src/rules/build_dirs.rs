//! Build outputs of other ecosystems (`.dart_tool`, `build`, `_build`, `.next`, `__pycache__`, ...).
//! A directory is only a candidate when a project marker sits next to it, so a folder that just
//! happens to be called `build` is never touched.

use super::{has_project_marker, Ctx, Rule, PROJECT_NOISE};
use crate::fs::{self, days};
use crate::model::{ActionKind, Group, Plan};

pub struct BuildDirs;

impl Rule for BuildDirs {
    fn name(&self) -> &'static str {
        "build directories"
    }

    fn plan(&self, ctx: &Ctx, plan: &mut Plan) {
        let stale = ctx.eff.build_stale_days;
        for name in &ctx.cfg.build_dirs.names {
            for d in fs::find_dirs_named(ctx.roots(), ctx.cfg.scan.depth, name) {
                let Some(proj) = d.parent() else { continue };
                if ctx.excluded(&d) || !has_project_marker(proj) {
                    continue;
                }
                if fs::idle_for(ctx.project_root(proj), days(stale), PROJECT_NOISE) {
                    let b = fs::disk_usage(&d);
                    if b > 0 {
                        plan.push(
                            Group::Build,
                            ActionKind::Remove,
                            d,
                            b,
                            format!("{name}: project untouched for {stale} days"),
                        );
                    }
                }
            }
        }
    }
}
