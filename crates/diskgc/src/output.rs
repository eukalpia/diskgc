//! Human-readable rendering of core data. `--json` prints the same data as JSON.

use diskgc_core::bytes::{gib, human};
use diskgc_core::engine::Status;
use diskgc_core::history::{Record, Stats};
use diskgc_core::mem::MemReport;
use diskgc_core::model::{ActionKind, Group, Plan, RunSummary};
use diskgc_core::report::Report;

fn json<T: serde::Serialize>(v: &T) {
    println!("{}", serde_json::to_string_pretty(v).unwrap_or_default());
}

fn shorten(p: &std::path::Path) -> String {
    let s = p.display().to_string();
    let home = diskgc_core::paths::home().display().to_string();
    s.strip_prefix(&home).map(|r| format!("~{r}")).unwrap_or(s)
}

pub fn report(r: &Report, as_json: bool) {
    if as_json {
        return json(r);
    }
    println!(
        "Disk {}: {} total, {} used, {} free",
        r.volume.display(),
        human(r.total),
        human(r.used),
        human(r.available)
    );
    println!("\nCategories");
    let mut cats: Vec<(&String, &u64)> = r.categories.iter().collect();
    cats.sort_by(|a, b| b.1.cmp(a.1));
    for (k, v) in cats {
        println!("  {:<20}{:>10}", k, human(*v));
    }
    for (title, items) in [
        ("Rust target/", &r.rust_targets),
        ("node_modules", &r.node_modules),
        ("Agent scratch directories", &r.scratch),
    ] {
        if items.is_empty() {
            continue;
        }
        println!("\n{title}");
        for s in items.iter().take(10) {
            println!("  {:>10}  {}", human(s.bytes), shorten(&s.path));
        }
    }
    if let Some(df) = &r.docker_df {
        println!("\nDocker");
        for l in df.lines() {
            println!("  {l}");
        }
    }
}

pub fn plan(p: &Plan, as_json: bool) {
    if as_json {
        return json(p);
    }
    if p.actions.is_empty() && p.notes.is_empty() {
        println!("Nothing to do.");
    }
    let mut groups: Vec<Group> = p.actions.iter().map(|a| a.group).collect();
    groups.sort();
    groups.dedup();
    for g in groups {
        if g == Group::Compress {
            let n = p.compress_count();
            let b: u64 = p
                .actions
                .iter()
                .filter(|a| a.kind == ActionKind::Compress)
                .map(|a| a.bytes)
                .sum();
            println!(
                "\ncompress: {n} cold files ({} on disk now), transparently, in place",
                human(b)
            );
            continue;
        }
        let acts: Vec<_> = p.actions.iter().filter(|a| a.group == g).collect();
        let total: u64 = acts.iter().map(|a| a.bytes).sum();
        println!("\n{} ({} actions, ~{})", g.name(), acts.len(), human(total));
        for a in acts.iter().take(25) {
            println!(
                "  {:>10}  {:<18} {}",
                human(a.bytes),
                a.kind.verb(),
                shorten(&a.path)
            );
            println!("              {}", a.reason);
        }
        if acts.len() > 25 {
            println!("  ... and {} more", acts.len() - 25);
        }
    }
    if let Some(d) = &p.docker {
        println!("\ndocker");
        if !d.available {
            println!("  daemon not running, skipped");
        } else {
            println!("  {} orphaned testcontainers; stopped > {} h; build cache > {} d; images {}; reclaimable ~{}",
                d.orphan_testcontainers, d.stopped_hours, d.build_cache_days, if d.image_prune_all { "all unused" } else { "dangling" }, human(d.reclaimable_bytes));
        }
    }
    for n in &p.notes {
        println!("\nnote: {n}");
    }
    println!(
        "\nEstimated: {} in {} actions",
        human(p.estimated_bytes()),
        p.actions.len()
    );
}

pub fn run(p: &Plan, s: &RunSummary, as_json: bool) {
    if as_json {
        return json(&serde_json::json!({ "plan": p, "summary": s }));
    }
    if s.dry_run {
        plan(p, false);
        println!("\nDry run: nothing was changed.");
        return;
    }
    for r in &s.results {
        let Some(a) = p.actions.iter().find(|a| a.id == r.id) else {
            continue;
        };
        if !r.ok {
            println!(
                "  FAILED   {:<18} {}: {}",
                a.kind.verb(),
                shorten(&a.path),
                r.error.clone().unwrap_or_default()
            );
        } else if r.freed > 0 || a.kind != ActionKind::Compress {
            println!(
                "  {:>10}  {:<18} {}",
                human(r.freed),
                a.kind.verb(),
                shorten(&a.path)
            );
        }
    }
    for l in &s.docker_lines {
        println!("  {l}");
    }
    println!(
        "\nFreed {} ({} actions, {:.0}s). {} free now.",
        human(s.freed_bytes + s.docker_freed_bytes),
        s.action_count,
        s.duration_secs,
        human(s.free_after)
    );
}

pub fn status(s: &Status, as_json: bool) {
    if as_json {
        return json(s);
    }
    let d = &s.disk;
    println!("diskgc {}", s.version);
    println!(
        "Disk {}: {} free of {} (level: {})",
        d.volume.display(),
        human(d.available),
        human(d.total),
        d.level.name()
    );
    println!(
        "Thresholds: comfortable {:.0} / low {:.0} / critical {:.0} GB",
        s.thresholds_gb.0, s.thresholds_gb.1, s.thresholds_gb.2
    );
    println!(
        "Daemon: {}{}",
        s.autostart.detail,
        s.autostart
            .pid
            .map(|p| format!(", pid {p}"))
            .unwrap_or_default()
    );
    match &s.forecast {
        Some(f) if f.rate_bytes_per_hour > 0.0 => {
            let eta = f
                .hours_to_critical
                .map(|h| format!(", critical in ~{h:.0} h at this rate"))
                .unwrap_or_default();
            println!(
                "Forecast: +{}/h over the last {:.0} h{eta}",
                human(f.rate_bytes_per_hour as u64),
                f.window_hours
            );
        }
        Some(f) => println!(
            "Forecast: stable ({}/h over the last {:.0} h)",
            human(f.rate_bytes_per_hour.abs() as u64),
            f.window_hours
        ),
        None => println!("Forecast: not enough history yet"),
    }
    if let Some(Record::Run {
        ts,
        level,
        freed,
        actions,
        ..
    }) = &s.last_run
    {
        println!(
            "Last run: {ts} [{}] freed {} in {actions} actions",
            level.map(|l| l.name()).unwrap_or("-"),
            human(*freed)
        );
    }
    println!("Config: {}", s.config_path.display());
    println!("Log:    {}", s.log_path.display());
}

pub fn stats(s: &Stats, as_json: bool) {
    if as_json {
        return json(s);
    }
    if s.days.is_empty() {
        println!("No history yet. Run `diskgc clean` or install the daemon.");
        return;
    }
    println!(
        "{:<12}{:>12}{:>12}{:>10}{:>12}",
        "day", "used (am)", "used (pm)", "delta", "freed"
    );
    for (day, d) in &s.days {
        println!(
            "{:<12}{:>10.1} G{:>10.1} G{:>+9.1} G{:>12}",
            day,
            gib(d.used_first),
            gib(d.used_last),
            gib(d.used_last) - gib(d.used_first),
            human(d.freed)
        );
    }
    if let (Some((t0, a)), Some((t1, b))) = (&s.categories_first, &s.categories_last) {
        println!(
            "\nCategories: {} -> {}",
            &t0[..10.min(t0.len())],
            &t1[..10.min(t1.len())]
        );
        let mut keys: Vec<&String> = b.keys().collect();
        keys.sort_by(|x, y| b[*y].cmp(&b[*x]));
        for k in keys {
            let (x, y) = (a.get(k).copied().unwrap_or(0), b[k]);
            println!(
                "  {:<20}{:>10} -> {:>10}  ({:+.1} G)",
                k,
                human(x),
                human(y),
                gib(y) - gib(x)
            );
        }
    }
    println!(
        "\nRuns: {}, freed in total: {}",
        s.runs,
        human(s.total_freed)
    );
    if !s.repeat_offenders.is_empty() {
        println!("\nRepeat offenders (cleaned 2+ times — fix the cause, not the symptom)");
        for r in &s.repeat_offenders {
            println!("  {:>2}x  {:>10}  {}", r.times, human(r.bytes), r.path);
        }
    }
}

pub fn history(recs: &[Record], as_json: bool) {
    if as_json {
        return json(&recs);
    }
    if recs.is_empty() {
        println!("No runs yet.");
    }
    for r in recs {
        if let Record::Run {
            ts,
            level,
            dry_run,
            freed,
            actions,
            available_after,
            top,
            ..
        } = r
        {
            println!(
                "{ts}  [{}{}]  freed {} in {actions} actions, {} free after",
                level.map(|l| l.name()).unwrap_or("-"),
                if *dry_run { ", dry-run" } else { "" },
                human(*freed),
                human(*available_after)
            );
            for a in top.iter().take(5) {
                println!("    {:>10}  {}  — {}", human(a.bytes), a.path, a.reason);
            }
        }
    }
}

pub fn memory(m: &MemReport, as_json: bool) {
    if as_json {
        return json(m);
    }
    println!(
        "RAM: {} total, {} used, {} available; swap {} of {} used{}",
        human(m.total),
        human(m.used),
        human(m.available),
        human(m.swap_used),
        human(m.swap_total),
        if m.pressure { "  (under pressure)" } else { "" }
    );
    println!("\nTop processes");
    for p in &m.top {
        let cmd: String = p.command.chars().take(80).collect();
        println!(
            "  {:>9}  {:>7}  {:>6.1} h  {}",
            human(p.rss),
            p.pid,
            p.run_hours,
            cmd
        );
    }
    println!("\nBy program");
    for (name, bytes, n) in &m.by_program {
        println!("  {:>9}  {:>3} proc  {}", human(*bytes), n, name);
    }
    println!(
        "\nAutomation browsers (Playwright, headless shells): {}",
        m.automation_browsers.len()
    );
    for p in &m.automation_browsers {
        let cmd: String = p.command.chars().take(80).collect();
        println!(
            "  {:>9}  {:>7}  {:>6.1} h  {}",
            human(p.rss),
            p.pid,
            p.run_hours,
            cmd
        );
    }
    if !m.automation_browsers.is_empty() {
        println!("  terminate those older than an hour with: diskgc mem --kill-browsers");
    }
    println!(
        "\nOrphaned helpers (parent gone, running > 12 h): {}",
        m.orphans.len()
    );
    for p in &m.orphans {
        let cmd: String = p.command.chars().take(80).collect();
        println!(
            "  {:>9}  {:>7}  {:>6.1} h  {}",
            human(p.rss),
            p.pid,
            p.run_hours,
            cmd
        );
    }
    if !m.orphans.is_empty() {
        println!("  terminate them with: diskgc mem --kill-orphans");
    }
    if m.orphan_containers > 0 {
        println!(
            "\n{} orphaned testcontainers are still running (removed by the next clean).",
            m.orphan_containers
        );
    }
}
