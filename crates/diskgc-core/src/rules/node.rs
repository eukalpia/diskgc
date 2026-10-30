//! `node_modules` of projects nobody has touched for a while. Restored by one `install`.

use super::{Ctx, Rule, PROJECT_NOISE};
use crate::fs::{self, days};
use crate::model::{ActionKind, Group, Plan};

pub struct NodeModules;

impl Rule for NodeModules {
    fn name(&self) -> &'static str {
        "node_modules"
    }

    fn plan(&self, ctx: &Ctx, plan: &mut Plan) {
        let stale = ctx.eff.node_stale_days;
        for nm in ctx.index().named("node_modules") {
            let Some(proj) = nm.parent() else { continue };
            if ctx.excluded(nm) || !proj.join("package.json").exists() {
                continue;
            }
            let root = ctx.project_root(proj);
            if fs::idle_for(root, days(stale), PROJECT_NOISE) {
                let b = fs::disk_usage(nm);
                if b > 0 {
                    plan.push(
                        Group::Node,
                        ActionKind::Remove,
                        nm.clone(),
                        b,
                        format!("project untouched for {stale} days; restored by install"),
                    );
                }
            }
        }
    }
}
