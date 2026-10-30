//! Docker (Docker Desktop, OrbStack, Colima, Podman with the docker CLI): orphaned
//! testcontainers, stopped containers, dangling volumes, build cache, dangling images.

use std::process::Command;

use crate::bytes::parse_human;
use crate::config::{Config, Effective};
use crate::model::DockerPlan;

fn run(args: &[&str]) -> Option<String> {
    let out = Command::new("docker").args(args).output().ok()?;
    out.status
        .success()
        .then(|| String::from_utf8_lossy(&out.stdout).into_owned())
}

pub fn available() -> bool {
    run(&["info", "--format", "{{.ServerVersion}}"]).is_some()
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
            build_cache_days: cfg.docker.build_cache_days,
            image_prune_all: cfg.docker.image_prune_all,
            reclaimable_bytes: 0,
        });
    }
    Some(DockerPlan {
        available: true,
        orphan_testcontainers: orphan_testcontainers(eff.testcontainers_hours).len(),
        stopped_hours: eff.docker_stopped_hours,
        build_cache_days: cfg.docker.build_cache_days,
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
    run(&[
        "container",
        "prune",
        "-f",
        "--filter",
        &format!("until={}h", eff.docker_stopped_hours),
    ]);
    run(&["volume", "prune", "-f"]);
    run(&[
        "builder",
        "prune",
        "-f",
        "--filter",
        &format!("until={}h", cfg.docker.build_cache_days * 24),
    ]);
    if cfg.docker.image_prune_all {
        run(&["image", "prune", "-af"]);
    } else {
        run(&["image", "prune", "-f"]);
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
