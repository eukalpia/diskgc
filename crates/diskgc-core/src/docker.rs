//! Docker (Docker Desktop, OrbStack, Colima, Podman with the docker CLI): orphaned
//! testcontainers, stopped containers, dangling volumes, build cache, dangling images.

use std::time::Duration;

use crate::bytes::parse_human;
use crate::config::{Config, Effective};
use crate::model::DockerPlan;
use crate::proc;

/// Queries answer in well under a second from a healthy daemon. Without a deadline, a VM that is
/// starting, stuck or asleep holds the whole run hostage.
const QUERY_TIMEOUT: Duration = Duration::from_secs(30);
/// A prune walks the VM's disk and can legitimately take minutes.
const PRUNE_TIMEOUT: Duration = Duration::from_secs(15 * 60);

fn run_for(args: &[&str], timeout: Duration) -> Option<String> {
    let mut argv = vec!["docker"];
    argv.extend_from_slice(args);
    let out = proc::stdout(&argv, timeout);
    if out.is_none() {
        tracing::debug!("docker {} failed or timed out", args.join(" "));
    }
    out
}

fn run(args: &[&str]) -> Option<String> {
    run_for(args, QUERY_TIMEOUT)
}

pub fn available() -> bool {
    run_for(
        &["info", "--format", "{{.ServerVersion}}"],
        Duration::from_secs(15),
    )
    .is_some()
}

/// Sum of the `Size` column of `docker system df`, in bytes (approximate).
pub fn total_bytes() -> u64 {
    run(&["system", "df", "--format", "{{.Size}}"])
        .unwrap_or_default()
        .lines()
        .map(parse_human)
        .sum()
}

pub fn reclaimable_bytes() -> u64 {
    run(&["system", "df", "--format", "{{.Reclaimable}}"])
        .unwrap_or_default()
        .lines()
        .map(|l| parse_human(l.split_whitespace().next().unwrap_or("")))
        .sum()
}

pub fn system_df() -> String {
    run(&["system", "df"]).unwrap_or_default()
}

/// Containers labelled by testcontainers that are older than `max_hours`: their test process is
/// long gone (killed, timed out, crashed) and nothing will ever clean them up.
pub fn orphan_testcontainers(max_hours: u64) -> Vec<String> {
    let ids: Vec<String> = run(&[
        "ps",
        "-aq",
        "--filter",
        "label=org.testcontainers.managed-by",
    ])
    .unwrap_or_default()
    .lines()
    .map(str::trim)
    .filter(|l| !l.is_empty())
    .map(String::from)
    .collect();
    if ids.is_empty() {
        return vec![];
    }
    let mut args = vec!["inspect", "-f", "{{.Id}} {{.Created}}"];
    args.extend(ids.iter().map(String::as_str));
    let now = chrono::Utc::now();
    run(&args)
        .unwrap_or_default()
        .lines()
        .filter_map(|l| {
            let mut it = l.split_whitespace();
            let id = it.next()?;
            let created = chrono::DateTime::parse_from_rfc3339(it.next()?).ok()?;
            let age_h = (now - created.with_timezone(&chrono::Utc)).num_hours();
            (age_h >= max_hours as i64).then(|| id.chars().take(12).collect())
        })
        .collect()
}

pub fn plan(cfg: &Config, eff: &Effective) -> Option<DockerPlan> {
    if !cfg.docker.enable {
        return None;
    }
    if !available() {
        return Some(DockerPlan {
            available: false,
            orphan_testcontainers: 0,
            stopped_hours: eff.docker_stopped_hours,
            build_cache_days: eff.docker_build_cache_hours / 24,
            image_prune_all: cfg.docker.image_prune_all,
            reclaimable_bytes: 0,
        });
    }
    Some(DockerPlan {
        available: true,
        orphan_testcontainers: orphan_testcontainers(eff.testcontainers_hours).len(),
        stopped_hours: eff.docker_stopped_hours,
        build_cache_days: eff.docker_build_cache_hours / 24,
        image_prune_all: cfg.docker.image_prune_all,
        reclaimable_bytes: reclaimable_bytes(),
    })
}

pub struct DockerOutcome {
    pub freed: u64,
    pub lines: Vec<String>,
}

pub fn clean(cfg: &Config, eff: &Effective) -> DockerOutcome {
    let mut lines = Vec::new();
    if !cfg.docker.enable {
        return DockerOutcome { freed: 0, lines };
    }
    if !available() {
        lines.push("docker: daemon not running, skipped".into());
        return DockerOutcome { freed: 0, lines };
    }
    let before = total_bytes();
    let orphans = orphan_testcontainers(eff.testcontainers_hours);
    if !orphans.is_empty() {
        let mut args = vec!["rm", "-fv"];
        args.extend(orphans.iter().map(String::as_str));
        run(&args);
        lines.push(format!(
            "docker: removed {} orphaned testcontainers (older than {} h)",
            orphans.len(),
            eff.testcontainers_hours
        ));
    }
    let prune = |args: &[&str], what: &str, lines: &mut Vec<String>| {
        if run_for(args, PRUNE_TIMEOUT).is_none() {
            lines.push(format!("docker: {what} failed or timed out"));
        }
    };
    prune(
        &[
            "container",
            "prune",
            "-f",
            "--filter",
            &format!("until={}h", eff.docker_stopped_hours),
        ],
        "container prune",
        &mut lines,
    );
    prune(&["volume", "prune", "-f"], "volume prune", &mut lines);
    // `-a`: without it only dangling records go, and BuildKit's cache is almost never dangling —
    // 7 GB of it used to survive every run.
    let until = format!("until={}h", eff.docker_build_cache_hours);
    let mut builder = vec!["builder", "prune", "-af"];
    if eff.docker_build_cache_hours > 0 {
        builder.extend(["--filter", until.as_str()]);
    }
    prune(&builder, "builder prune", &mut lines);
    if cfg.docker.image_prune_all {
        prune(&["image", "prune", "-af"], "image prune", &mut lines);
    } else {
        prune(&["image", "prune", "-f"], "image prune", &mut lines);
    }
    let after = total_bytes();
    lines.push(format!(
        "docker: {} -> {}",
        crate::bytes::human(before),
        crate::bytes::human(after)
    ));
    DockerOutcome {
        freed: before.saturating_sub(after),
        lines,
    }
}
