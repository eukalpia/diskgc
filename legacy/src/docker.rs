//! Docker (OrbStack): сироты testcontainers, остановленные контейнеры, висячие тома,
//! кэш сборки, висячие образы.

use std::process::Command;

use crate::config::{Config, Effective};

pub fn available() -> bool {
    Command::new("docker").arg("info").output().map(|o| o.status.success()).unwrap_or(false)
}

fn run(args: &[&str]) -> String {
    Command::new("docker")
        .args(args)
        .output()
        .map(|o| String::from_utf8_lossy(&o.stdout).into_owned())
        .unwrap_or_default()
}

/// Сумма Size из `docker system df` (байты, приблизительно).
pub fn total_bytes() -> u64 {
    run(&["system", "df", "--format", "{{.Size}}"]).lines().map(parse_human).sum()
}

pub fn parse_human(s: &str) -> u64 {
    let s = s.trim();
    let idx = s.find(|c: char| c.is_ascii_alphabetic()).unwrap_or(s.len());
    let num: f64 = s[..idx].trim().parse().unwrap_or(0.0);
    let mult = match s[idx..].trim().to_ascii_lowercase().as_str() {
        "kb" => 1e3,
        "mb" => 1e6,
        "gb" => 1e9,
        "tb" => 1e12,
        "kib" => 1024.0,
        "mib" => 1024.0 * 1024.0,
        "gib" => 1024.0 * 1024.0 * 1024.0,
        _ => 1.0,
    };
    (num * mult) as u64
}

/// Контейнеры с меткой testcontainers старше max_hours — тесты убиты, контейнеры остались.
pub fn orphan_testcontainers(max_hours: u64) -> Vec<String> {
    let ids: Vec<String> = run(&["ps", "-aq", "--filter", "label=org.testcontainers.managed-by"])
        .lines()
        .map(|l| l.trim().to_string())
        .filter(|l| !l.is_empty())
        .collect();
    if ids.is_empty() {
        return vec![];
    }
    let mut args = vec!["inspect", "-f", "{{.Id}} {{.Created}}"];
    let refs: Vec<&str> = ids.iter().map(|s| s.as_str()).collect();
    args.extend(refs);
    let now = chrono::Utc::now();
    run(&args)
        .lines()
        .filter_map(|l| {
            let mut it = l.split_whitespace();
            let id = it.next()?;
            let created = chrono::DateTime::parse_from_rfc3339(it.next()?).ok()?;
            let age_h = (now - created.with_timezone(&chrono::Utc)).num_hours();
            (age_h >= max_hours as i64).then(|| id[..12].to_string())
        })
        .collect()
}

pub struct DockerOutcome {
    pub freed: u64,
    pub lines: Vec<String>,
}

pub fn clean(cfg: &Config, eff: &Effective, dry: bool) -> DockerOutcome {
    let mut lines = Vec::new();
    if !cfg.docker.enable {
        return DockerOutcome { freed: 0, lines };
    }
    if !available() {
        lines.push("docker: не запущен — пропуск".into());
        return DockerOutcome { freed: 0, lines };
    }
    let before = total_bytes();
    let orphans = orphan_testcontainers(eff.testcontainers_hours);
    let until_stopped = format!("until={}h", eff.docker_stopped_hours);
    let until_build = format!("until={}h", cfg.docker.build_cache_days * 24);
    if dry {
        if !orphans.is_empty() {
            lines.push(format!(
                "[план] docker rm -fv {} контейнеров testcontainers старше {} ч (сироты от убитых тестов)",
                orphans.len(),
                eff.testcontainers_hours
            ));
        }
        lines.push(format!(
            "[план] docker container prune ({}), volume prune, builder prune ({}), image prune{}",
            until_stopped,
            until_build,
            if cfg.docker.image_prune_all { " -a" } else { "" }
        ));
        let df = run(&["system", "df"]);
        for l in df.lines() {
            lines.push(format!("        {}", l));
        }
        return DockerOutcome { freed: 0, lines };
    }
    if !orphans.is_empty() {
        let mut args = vec!["rm", "-fv"];
        let refs: Vec<&str> = orphans.iter().map(|s| s.as_str()).collect();
        args.extend(refs);
        run(&args);
        lines.push(format!("docker rm -fv: {} сирот testcontainers", orphans.len()));
    }
    run(&["container", "prune", "-f", "--filter", &until_stopped]);
    run(&["volume", "prune", "-f"]);
    run(&["builder", "prune", "-f", "--filter", &until_build]);
    if cfg.docker.image_prune_all {
        run(&["image", "prune", "-af"]);
    } else {
        run(&["image", "prune", "-f"]);
    }
    let after = total_bytes();
    let freed = before.saturating_sub(after);
    lines.push(format!("docker: {:.2} → {:.2} ГБ", before as f64 / 1e9, after as f64 / 1e9));
    DockerOutcome { freed, lines }
}
