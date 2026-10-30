//! RAM: кто ест память и какие процессы — сироты (родитель умер, процесс переехал под launchd).
//! По умолчанию только отчёт; --kill-orphans шлёт SIGTERM явным сиротам старше 12 ч.

use std::process::Command;

#[derive(Debug, Clone)]
struct Proc {
    pid: i32,
    ppid: i32,
    rss_mb: f64,
    etime_h: f64,
    comm: String,
    args: String,
}

fn etime_hours(s: &str) -> f64 {
    // [[dd-]hh:]mm:ss
    let (days, rest) = match s.split_once('-') {
        Some((d, r)) => (d.parse::<f64>().unwrap_or(0.0), r),
        None => (0.0, s),
    };
    let parts: Vec<f64> = rest.split(':').map(|x| x.parse().unwrap_or(0.0)).collect();
    let secs = match parts.len() {
        3 => parts[0] * 3600.0 + parts[1] * 60.0 + parts[2],
        2 => parts[0] * 60.0 + parts[1],
        _ => 0.0,
    };
    days * 24.0 + secs / 3600.0
}

fn processes() -> Vec<Proc> {
    let out = Command::new("ps").args(["-axo", "pid=,ppid=,rss=,etime=,args="]).output();
    let text = out.map(|o| String::from_utf8_lossy(&o.stdout).into_owned()).unwrap_or_default();
    text.lines()
        .filter_map(|l| {
            let mut it = l.split_whitespace();
            let pid = it.next()?.parse().ok()?;
            let ppid = it.next()?.parse().ok()?;
            let rss_kb: f64 = it.next()?.parse().ok()?;
            let etime = it.next()?;
            let args: Vec<&str> = it.collect();
            let args = args.join(" ");
            let comm = program_name(&args);
            Some(Proc { pid, ppid, rss_mb: rss_kb / 1024.0, etime_h: etime_hours(etime), comm, args })
        })
        .collect()
}

/// Имя программы из командной строки: для .app — имя бандла, иначе basename первого слова.
fn program_name(args: &str) -> String {
    if let Some(i) = args.find(".app/") {
        let head = &args[..i];
        if let Some(j) = head.rfind('/') {
            return head[j + 1..].to_string();
        }
    }
    let first = args.split_whitespace().next().unwrap_or("");
    let base = std::path::Path::new(first).file_name().map(|s| s.to_string_lossy().into_owned()).unwrap_or(first.to_string());
    // node/python/bun: показать, что именно запущено
    if ["node", "python3", "python", "bun", "deno"].contains(&base.as_str()) {
        if let Some(script) = args.split_whitespace().nth(1) {
            let sb = std::path::Path::new(script).file_name().map(|s| s.to_string_lossy().into_owned()).unwrap_or_default();
            if !sb.is_empty() {
                return format!("{base} {sb}");
            }
        }
    }
    base
}

fn short(s: &str, n: usize) -> String {
    let s = s.replace(&crate::fsutil::home().display().to_string(), "~");
    if s.chars().count() > n { format!("{}…", s.chars().take(n).collect::<String>()) } else { s }
}

/// Признаки процессов, которые обычно остаются от завершённых сессий агентов/IDE.
const ORPHAN_PATTERNS: &[&str] = &[
    "codex-code-mode-host",
    "mcp/server.mjs",
    "rust-analyzer-proc-macro-srv",
    "rust-analyzer",
    "chrome-headless-shell",
    "Chromium",
    "playwright",
    "esbuild",
    "tsserver",
    "dart:analysis",
    "flutter_tester",
];

pub fn report(kill_orphans: bool) {
    let mut ps = processes();
    // память системы
    let memsize: f64 = Command::new("sysctl")
        .args(["-n", "hw.memsize"])
        .output()
        .ok()
        .and_then(|o| String::from_utf8_lossy(&o.stdout).trim().parse().ok())
        .unwrap_or(0.0);
    let vm = Command::new("vm_stat").output().map(|o| String::from_utf8_lossy(&o.stdout).into_owned()).unwrap_or_default();
    let page: f64 = vm.lines().next().and_then(|l| l.split("page size of ").nth(1)).and_then(|s| s.split_whitespace().next()).and_then(|s| s.parse().ok()).unwrap_or(16384.0);
    let stat = |key: &str| -> f64 {
        vm.lines().find(|l| l.starts_with(key)).and_then(|l| l.split(':').nth(1)).map(|v| v.trim().trim_end_matches('.').parse::<f64>().unwrap_or(0.0)).unwrap_or(0.0) * page / 1e9
    };
    let free = stat("Pages free");
    let wired = stat("Pages wired down");
    let compressed = stat("Pages occupied by compressor");
    let active = stat("Pages active");
    let swap = Command::new("sysctl").args(["-n", "vm.swapusage"]).output().map(|o| String::from_utf8_lossy(&o.stdout).trim().to_string()).unwrap_or_default();
    println!("== RAM: всего {:.0} ГБ; активно {:.1}, wired {:.1}, в компрессоре {:.1}, свободно {:.1} ГБ ==", memsize / 1e9, active, wired, compressed, free);
    println!("   swap: {}", swap);
    if compressed > memsize / 1e9 * 0.15 || swap.contains("used = ") && !swap.contains("used = 0.00M") {
        println!("   ! память под давлением: система сжимает страницы / использует swap — см. топ ниже");
    }

    ps.sort_by(|a, b| b.rss_mb.partial_cmp(&a.rss_mb).unwrap());
    println!("\n== Топ процессов по памяти (RSS) ==");
    for p in ps.iter().take(15) {
        println!("  {:>7.0} МБ  {:>6}  {:>6.1} ч  {}", p.rss_mb, p.pid, p.etime_h, short(&p.args, 90));
    }
    let total: f64 = ps.iter().map(|p| p.rss_mb).sum();
    println!("  всего RSS процессов: {:.1} ГБ (общие страницы считаются несколько раз)", total / 1024.0);

    // группы: суммарно по имени процесса
    let mut groups: std::collections::BTreeMap<String, (f64, usize)> = Default::default();
    for p in &ps {
        let e = groups.entry(p.comm.clone()).or_insert((0.0, 0));
        e.0 += p.rss_mb;
        e.1 += 1;
    }
    let mut g: Vec<(&String, &(f64, usize))> = groups.iter().collect();
    g.sort_by(|a, b| b.1 .0.partial_cmp(&a.1 .0).unwrap());
    println!("\n== По программам ==");
    for (name, (mb, n)) in g.iter().take(10) {
        println!("  {:>7.0} МБ  {:>3} проц.  {}", mb, n, name);
    }

    // сироты: родитель = launchd (1), живут > 12 ч, похожи на хвосты сессий
    let orphans: Vec<&Proc> = ps
        .iter()
        .filter(|p| p.ppid == 1 && p.etime_h > 12.0 && ORPHAN_PATTERNS.iter().any(|pat| p.args.contains(pat) || p.comm.contains(pat)))
        .collect();
    println!("\n== Сироты (родитель умер, живут > 12 ч): {} ==", orphans.len());
    let mut orphan_mb = 0.0;
    for p in &orphans {
        orphan_mb += p.rss_mb;
        println!("  {:>7.0} МБ  {:>6}  {:>6.1} ч  {}", p.rss_mb, p.pid, p.etime_h, short(&p.args, 90));
    }
    if !orphans.is_empty() {
        println!("  итого {:.0} МБ. {}", orphan_mb, if kill_orphans { "Завершаю (SIGTERM)…" } else { "Завершить: diskgc mem --kill-orphans" });
        if kill_orphans {
            for p in &orphans {
                unsafe {
                    libc::kill(p.pid, libc::SIGTERM);
                }
            }
        }
    }
    if crate::docker::available() {
        let n = crate::docker::orphan_testcontainers(1).len();
        if n > 0 {
            println!("\n  ! {} контейнеров testcontainers старше часа — их уберёт diskgc clean (и они едят RAM в OrbStack)", n);
        }
    }
}
