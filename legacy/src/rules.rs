//! Правила: что считается мусором. Каждое правило добавляет в План действия с причиной.
//! План можно показать (dry-run) или выполнить. Удаляется только то, что
//! восстанавливается сборкой/установкой/скачиванием: исходники, .git, базы, история — никогда.

use std::path::{Path, PathBuf};
use std::time::SystemTime;

use walkdir::WalkDir;

use crate::config::{Config, Effective};
use crate::fsutil::{self, days, disk_usage, expand, file_name, gb, home, hours, idle_for};

#[derive(Debug, Clone)]
pub enum Kind {
    /// rm -rf
    Remove,
    /// удалить внутри файлы старше N дней, потом пустые каталоги
    SweepOlderThan(u64),
    /// удалить исполняемые файлы > 30 МБ без расширения в */deps/ старше суток
    SweepBigBinaries,
    /// прозрачное APFS-сжатие файла
    Compress,
    /// внешняя команда; measure — путь, по разнице размера которого считаем эффект
    Command { argv: Vec<String>, measure: Option<PathBuf> },
}

#[derive(Debug, Clone)]
pub struct Action {
    pub kind: Kind,
    pub path: PathBuf,
    /// ожидаемый эффект в байтах (для Compress — текущий размер файла)
    pub bytes: u64,
    pub reason: String,
    pub group: &'static str,
}

#[derive(Default)]
pub struct Plan {
    pub actions: Vec<Action>,
    pub notes: Vec<String>,
}

impl Plan {
    fn add(&mut self, group: &'static str, kind: Kind, path: PathBuf, bytes: u64, reason: impl Into<String>) {
        self.actions.push(Action { kind, path, bytes, reason: reason.into(), group });
    }
}

/// Служебные каталоги, которые не считаются «активностью» проекта.
const PROJECT_NOISE: &[&str] = &[
    "node_modules", ".git", "target", ".dart_tool", "build", "_build", ".next", ".turbo", ".venv", "__pycache__",
];

fn roots(cfg: &Config) -> Vec<PathBuf> {
    cfg.scan.roots.iter().map(|r| expand(r)).collect()
}

/// Корень проекта для оценки активности: в монорепо поднимаемся до верхнего каталога
/// с package.json / pnpm-workspace.yaml / Cargo.toml / pubspec.yaml (но не выше корней сканирования).
fn project_root<'a>(cfg: &Config, start: &'a Path) -> &'a Path {
    let scan_roots = roots(cfg);
    let mut best = start;
    let mut cur = start;
    while let Some(parent) = cur.parent() {
        if scan_roots.iter().any(|r| r == parent) || parent.parent().is_none() {
            break;
        }
        // граница репозитория: git worktree / отдельный checkout — самостоятельный проект
        if cur.join(".git").exists() {
            break;
        }
        if ["package.json", "pnpm-workspace.yaml", "Cargo.toml", "pubspec.yaml", "mix.exs", "pyproject.toml"]
            .iter()
            .any(|m| parent.join(m).exists())
        {
            best = parent;
        }
        cur = parent;
    }
    best
}

pub fn build_plan(cfg: &Config, eff: &Effective) -> Plan {
    let mut plan = Plan::default();
    rust_targets(cfg, eff, &mut plan);
    node_modules(cfg, eff, &mut plan);
    build_dirs(cfg, eff, &mut plan);
    scratch(cfg, eff, &mut plan);
    ai_tools(cfg, &mut plan);
    caches(cfg, &mut plan);
    if cfg.compress.enable {
        crate::compress::plan(cfg, &mut plan);
    }
    plan
}

// ---------------------------------------------------------------- Rust
fn rust_targets(cfg: &Config, eff: &Effective, plan: &mut Plan) {
    let mut r = roots(cfg);
    r.extend(cfg.scan.extra_target_roots.iter().map(|x| expand(x)));
    let max = (eff.rust_target_max_gb * fsutil::GB) as u64;
    for t in fsutil::find_cargo_targets(&r, cfg.scan.depth) {
        if cfg.excluded(&t) {
            continue;
        }
        let total = disk_usage(&t);
        if total == 0 {
            continue;
        }
        // Жёсткий потолок: target больше 2×лимита — режем всё старше суток, даже если сборка идёт.
        // Иначе непрерывно собирающий агент раздувает кэш без предела (105 ГБ за два дня).
        let over_hard_cap = total > max * 2;
        let sweep_days = if over_hard_cap { 1 } else { eff.rust_sweep_days };
        let cutoff = SystemTime::now() - days(sweep_days);
        let old = sum_files(&t, |_, m| m.modified().map(|x| x < cutoff).unwrap_or(false));
        let mut remaining = total.saturating_sub(old);
        if old > 0 {
            let why = if over_hard_cap {
                format!("target {:.0} ГБ > потолка {:.0} ГБ: всё старше суток", gb(total), gb(max * 2))
            } else {
                format!("артефакты, к которым не обращались > {} дн.", sweep_days)
            };
            plan.add("rust", Kind::SweepOlderThan(sweep_days), t.clone(), old, why);
        }
        if remaining <= max {
            continue;
        }
        // раздутый target: сначала дешёвое — incremental и толстые тестовые бинарники (считаем только то, что не ушло в sweep)
        for profile in std::fs::read_dir(&t).into_iter().flatten().flatten() {
            let inc = profile.path().join("incremental");
            if inc.is_dir() {
                let b = sum_files(&inc, |_, m| m.modified().map(|x| x >= cutoff).unwrap_or(true));
                remaining = remaining.saturating_sub(b);
                plan.add("rust", Kind::Remove, inc, b, format!("target > {:.0} ГБ: кэш инкрементальной компиляции", eff.rust_target_max_gb));
            }
        }
        let bins = sum_files(&t, |e, m| is_big_binary(e, m) && m.modified().map(|x| x >= cutoff).unwrap_or(false));
        if bins > 0 {
            remaining = remaining.saturating_sub(bins);
            plan.add("rust", Kind::SweepBigBinaries, t.clone(), bins, "тестовые бинарники > 30 МБ старше суток");
        }
        if remaining > max {
            if idle_for(&t, hours(eff.rust_idle_hours), &[]) {
                plan.add(
                    "rust",
                    Kind::Remove,
                    t.clone(),
                    remaining,
                    format!("всё ещё > {:.0} ГБ и простаивает > {} ч — полная очистка", eff.rust_target_max_gb, eff.rust_idle_hours),
                );
            } else {
                plan.notes.push(format!("оставлен (идёт сборка): {} — ещё {:.1} ГБ", t.display(), gb(remaining)));
            }
        }
    }
}

fn sum_files(root: &Path, pred: impl Fn(&walkdir::DirEntry, &std::fs::Metadata) -> bool) -> u64 {
    fsutil::walk_bytes(root, |e, m| e.file_type().is_file() && pred(e, m))
}

fn is_big_binary(e: &walkdir::DirEntry, m: &std::fs::Metadata) -> bool {
    let day_ago = SystemTime::now() - hours(24);
    e.file_type().is_file()
        && m.len() > 30 * 1024 * 1024
        && !e.file_name().to_string_lossy().contains('.')
        && e.path().parent().map(|p| file_name(p) == "deps").unwrap_or(false)
        && m.modified().map(|t| t < day_ago).unwrap_or(false)
}

// ---------------------------------------------------------------- Node
fn node_modules(cfg: &Config, eff: &Effective, plan: &mut Plan) {
    for nm in fsutil::find_dirs_named(&roots(cfg), cfg.scan.depth, "node_modules") {
        let proj = match nm.parent() {
            Some(p) => p,
            None => continue,
        };
        if cfg.excluded(&nm) || !proj.join("package.json").exists() {
            continue;
        }
        let root = project_root(cfg, proj);
        if idle_for(root, days(eff.node_stale_days), PROJECT_NOISE) {
            let b = disk_usage(&nm);
            plan.add("node", Kind::Remove, nm, b, format!("проект не трогали > {} дн. (восстановится install)", eff.node_stale_days));
        }
    }
}

// ---------------------------------------------------------------- прочие сборочные каталоги
const PROJECT_MARKERS: &[&str] = &[
    "pubspec.yaml", "mix.exs", "package.json", "pyproject.toml", "setup.py", "build.gradle", "build.gradle.kts",
    "settings.gradle", "settings.gradle.kts", "CMakeLists.txt", "Cargo.toml",
];

fn build_dirs(cfg: &Config, eff: &Effective, plan: &mut Plan) {
    let r = roots(cfg);
    for name in &cfg.build_dirs.names {
        for d in fsutil::find_dirs_named(&r, cfg.scan.depth, name) {
            let proj = match d.parent() {
                Some(p) => p,
                None => continue,
            };
            if cfg.excluded(&d) || !PROJECT_MARKERS.iter().any(|m| proj.join(m).exists()) {
                continue;
            }
            if idle_for(project_root(cfg, proj), days(eff.build_stale_days), PROJECT_NOISE) {
                let b = disk_usage(&d);
                if b > 0 {
                    plan.add("build", Kind::Remove, d, b, format!("{}: проект не трогали > {} дн.", name, eff.build_stale_days));
                }
            }
        }
    }
}

// ---------------------------------------------------------------- временные каталоги агентов
fn scratch(cfg: &Config, eff: &Effective, plan: &mut Plan) {
    let uid = unsafe { libc::getuid() };
    let claude_tmp = PathBuf::from(format!("/private/tmp/claude-{}", uid));
    for proj in std::fs::read_dir(&claude_tmp).into_iter().flatten().flatten() {
        for sess in std::fs::read_dir(proj.path()).into_iter().flatten().flatten() {
            let p = sess.path();
            if !p.is_dir() || cfg.excluded(&p) {
                continue;
            }
            if idle_for(&p, days(eff.claude_scratch_days), &[]) {
                let b = disk_usage(&p);
                if b > 0 {
                    plan.add("scratch", Kind::Remove, p, b, format!("сессия Claude Code молчит > {} дн.", eff.claude_scratch_days));
                }
            }
        }
    }
    for e in std::fs::read_dir("/private/tmp").into_iter().flatten().flatten() {
        let p = e.path();
        let n = file_name(&p);
        if n.starts_with('.') || n.starts_with("com.") || p == claude_tmp || cfg.excluded(&p) {
            continue;
        }
        let owned = std::fs::symlink_metadata(&p)
            .map(|m| std::os::unix::fs::MetadataExt::uid(&m) == uid)
            .unwrap_or(false);
        if !owned {
            continue;
        }
        if idle_for(&p, days(eff.tmp_stale_days), &[]) {
            let b = disk_usage(&p);
            if b > 0 {
                plan.add("tmp", Kind::Remove, p, b, format!("в /tmp без изменений > {} дн.", eff.tmp_stale_days));
            }
        }
    }
}

// ---------------------------------------------------------------- Codex / Claude
fn ai_tools(cfg: &Config, plan: &mut Plan) {
    let h = home();
    // старые версии Codex (каждая ~250 МБ, приложение их само не удаляет)
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
        .filter(|p| p.is_dir() && file_name(p) != current)
        .map(|p| (std::fs::metadata(&p).and_then(|m| m.modified()).unwrap_or(SystemTime::UNIX_EPOCH), p))
        .collect();
    rel.sort_by(|a, b| b.0.cmp(&a.0));
    for (_, p) in rel.into_iter().skip(cfg.ai_tools.codex_keep_releases) {
        let b = disk_usage(&p);
        plan.add("codex", Kind::Remove, p, b, "старая версия Codex");
    }
    // недоустановленные runtime Codex
    for e in std::fs::read_dir(h.join(".cache/codex-runtimes")).into_iter().flatten().flatten() {
        let p = e.path();
        if file_name(&p).starts_with("codex-runtime-install-") && idle_for(&p, hours(24), &[]) {
            let b = disk_usage(&p);
            plan.add("codex", Kind::Remove, p, b, "брошенная временная установка runtime Codex");
        }
    }
    if cfg.ai_tools.codex_sessions_delete_days > 0 {
        old_files(h.join(".codex/sessions"), cfg.ai_tools.codex_sessions_delete_days, "codex", "история сессий Codex старше лимита", plan);
    }
    if cfg.ai_tools.claude_sessions_delete_days > 0 {
        old_files(h.join(".claude/projects"), cfg.ai_tools.claude_sessions_delete_days, "claude", "история сессий Claude старше лимита", plan);
    }
}

fn old_files(root: PathBuf, d: u64, group: &'static str, reason: &str, plan: &mut Plan) {
    let cutoff = SystemTime::now() - days(d);
    for e in WalkDir::new(root).into_iter().filter_map(|e| e.ok()).filter(|e| e.file_type().is_file()) {
        if let Ok(m) = e.metadata() {
            if m.modified().map(|t| t < cutoff).unwrap_or(false) {
                plan.add(group, Kind::Remove, e.path().to_path_buf(), std::os::unix::fs::MetadataExt::blocks(&m) * 512, reason);
            }
        }
    }
}

// ---------------------------------------------------------------- кэши
fn caches(cfg: &Config, plan: &mut Plan) {
    let h = home();
    // Chrome оставляет клоны приложения после обновлений (по ~1.4 ГБ каждый)
    if let Some(x) = darwin_user_dir("X") {
        let clones = x.join("com.google.Chrome.code_sign_clone");
        for e in std::fs::read_dir(&clones).into_iter().flatten().flatten() {
            let p = e.path();
            if p.is_dir() && idle_for(&p, hours(cfg.caches.chrome_clone_hours), &[]) {
                let b = disk_usage(&p);
                plan.add("cache", Kind::Remove, p, b, "клон Chrome после обновления");
            }
        }
    }
    if cfg.caches.vscode_shipit {
        let ship = h.join("Library/Caches/com.microsoft.VSCode.ShipIt");
        for e in std::fs::read_dir(&ship).into_iter().flatten().flatten() {
            let p = e.path();
            if idle_for(&p, hours(24), &[]) {
                let b = disk_usage(&p);
                if b > 0 {
                    plan.add("cache", Kind::Remove, p, b, "остаток обновления VS Code");
                }
            }
        }
    }
    let gocache = h.join("Library/Caches/go-build");
    if gocache.is_dir() {
        let b = disk_usage(&gocache);
        if gb(b) > cfg.caches.go_cache_max_gb && which("go") {
            plan.add(
                "cache",
                Kind::Command { argv: vec!["go".into(), "clean".into(), "-cache".into()], measure: Some(gocache.clone()) },
                gocache,
                b,
                format!("кэш сборки Go > {:.0} ГБ", cfg.caches.go_cache_max_gb),
            );
        }
    }
    // кэши пакетных менеджеров: всё скачается заново при надобности
    let npm = h.join(".npm/_cacache");
    if npm.is_dir() && gb(disk_usage(&npm)) > 1.0 && which("npm") {
        let b = disk_usage(&npm);
        plan.add("cache", Kind::Command { argv: vec!["npm".into(), "cache".into(), "clean".into(), "--force".into()], measure: Some(npm.clone()) }, npm, b, "кэш npm > 1 ГБ");
    }
    let uv = h.join(".cache/uv");
    if uv.is_dir() && gb(disk_usage(&uv)) > 1.0 && which("uv") {
        let b = disk_usage(&uv);
        plan.add("cache", Kind::Command { argv: vec!["uv".into(), "cache".into(), "clean".into()], measure: Some(uv.clone()) }, uv, b, "кэш uv > 1 ГБ");
    }
    if which("pnpm") {
        plan.add("cache", Kind::Command { argv: vec!["pnpm".into(), "store".into(), "prune".into()], measure: None }, h.join(".local/share/pnpm"), 0, "неиспользуемые пакеты в store pnpm");
    }
    if which("brew") {
        let bc = h.join("Library/Caches/Homebrew");
        plan.add(
            "cache",
            Kind::Command {
                argv: vec!["brew".into(), "cleanup".into(), "-s".into(), format!("--prune={}", cfg.caches.brew_prune_days)],
                measure: Some(bc.clone()),
            },
            bc,
            0,
            "старые загрузки Homebrew",
        );
    }
    let derived = h.join("Library/Developer/Xcode/DerivedData");
    for e in std::fs::read_dir(&derived).into_iter().flatten().flatten() {
        let p = e.path();
        if p.is_dir() && idle_for(&p, days(cfg.caches.xcode_derived_days), &[]) {
            let b = disk_usage(&p);
            plan.add("cache", Kind::Remove, p, b, format!("Xcode DerivedData > {} дн.", cfg.caches.xcode_derived_days));
        }
    }
    for sub in ["Library/Developer/CoreSimulator/Caches", "Library/Developer/CoreSimulator/Temp"] {
        for e in std::fs::read_dir(h.join(sub)).into_iter().flatten().flatten() {
            let p = e.path();
            if idle_for(&p, days(7), &[]) {
                let b = disk_usage(&p);
                if b > 0 {
                    plan.add("cache", Kind::Remove, p, b, "кэш iOS-симулятора");
                }
            }
        }
    }
    if which("xcrun") {
        plan.add(
            "cache",
            Kind::Command { argv: vec!["xcrun".into(), "simctl".into(), "delete".into(), "unavailable".into()], measure: None },
            h.join("Library/Developer/CoreSimulator/Devices"),
            0,
            "симуляторы без runtime",
        );
    }
}

/// /private/var/folders/xx/yyyy/<sub>  (T = temp, C = cache, X = прочее)
pub fn darwin_user_dir(sub: &str) -> Option<PathBuf> {
    let t = std::env::var("TMPDIR").ok()?;
    let t = PathBuf::from(t.trim_end_matches('/'));
    Some(t.parent()?.join(sub))
}

pub fn which(bin: &str) -> bool {
    std::env::var("PATH")
        .unwrap_or_default()
        .split(':')
        .any(|d| Path::new(d).join(bin).is_file())
}

// ---------------------------------------------------------------- выполнение
pub struct Outcome {
    pub freed: u64,
    pub lines: Vec<String>,
}

pub fn execute(plan: &Plan, dry: bool) -> Outcome {
    let mut freed = 0u64;
    let mut lines = Vec::new();
    let tag = if dry { "[план] " } else { "" };
    for a in &plan.actions {
        let before = match &a.kind {
            Kind::Command { measure: Some(m), .. } => disk_usage(m),
            Kind::Compress => disk_usage(&a.path),
            _ => a.bytes,
        };
        let desc = match &a.kind {
            Kind::Remove => format!("[{}] rm -rf {}", a.group, a.path.display()),
            Kind::SweepOlderThan(d) => format!("[{}] sweep >{}d {}", a.group, d, a.path.display()),
            Kind::SweepBigBinaries => format!("[{}] sweep big test bins {}", a.group, a.path.display()),
            Kind::Compress => format!("[{}] compress {}", a.group, a.path.display()),
            Kind::Command { argv, .. } => format!("[{}] {}", a.group, argv.join(" ")),
        };
        if dry {
            if matches!(a.kind, Kind::Compress) {
                continue; // сжатие показываем сводкой ниже
            }
            lines.push(format!("{}{}  ({:.2} ГБ) — {}", tag, desc, gb(a.bytes), a.reason));
            freed += a.bytes;
            continue;
        }
        let got = match &a.kind {
            Kind::Remove => {
                let b = disk_usage(&a.path); // часть могла уйти в предыдущем sweep
                let ok = if a.path.is_dir() { std::fs::remove_dir_all(&a.path).is_ok() } else { std::fs::remove_file(&a.path).is_ok() };
                if ok { b } else { 0 }
            }
            Kind::SweepOlderThan(d) => {
                let cutoff = SystemTime::now() - days(*d);
                sweep(&a.path, |_, m| m.modified().map(|t| t < cutoff).unwrap_or(false))
            }
            Kind::SweepBigBinaries => sweep(&a.path, is_big_binary),
            Kind::Compress => match crate::compress::compress_file(&a.path) {
                Ok(saved) => saved,
                Err(e) => {
                    lines.push(format!("  ! compress {}: {}", a.path.display(), e));
                    0
                }
            },
            Kind::Command { argv, measure } => {
                let _ = std::process::Command::new(&argv[0]).args(&argv[1..]).output();
                match measure {
                    Some(m) => before.saturating_sub(disk_usage(m)),
                    None => 0,
                }
            }
        };
        freed += got;
        if !matches!(a.kind, Kind::Compress) || got > 0 {
            lines.push(format!("{}  ({:.2} ГБ) — {}", desc, gb(got), a.reason));
        }
    }
    if dry {
        let n = plan.actions.iter().filter(|a| matches!(a.kind, Kind::Compress)).count();
        let b: u64 = plan.actions.iter().filter(|a| matches!(a.kind, Kind::Compress)).map(|a| a.bytes).sum();
        if n > 0 {
            lines.push(format!("{}сжать {} холодных файлов ({:.2} ГБ сейчас; APFS, прозрачно)", tag, n, gb(b)));
        }
    }
    for n in &plan.notes {
        lines.push(format!("  ~ {}", n));
    }
    Outcome { freed, lines }
}

fn sweep(root: &Path, pred: impl Fn(&walkdir::DirEntry, &std::fs::Metadata) -> bool) -> u64 {
    let before = disk_usage(root);
    for e in WalkDir::new(root).follow_links(false).into_iter().filter_map(|e| e.ok()) {
        if !e.file_type().is_file() {
            continue;
        }
        if let Ok(m) = e.metadata() {
            if pred(&e, &m) {
                let _ = std::fs::remove_file(e.path());
            }
        }
    }
    let freed = before.saturating_sub(disk_usage(root));
    // пустые каталоги — снизу вверх
    let mut dirs: Vec<PathBuf> = WalkDir::new(root)
        .min_depth(1)
        .into_iter()
        .filter_map(|e| e.ok())
        .filter(|e| e.file_type().is_dir())
        .map(|e| e.path().to_path_buf())
        .collect();
    dirs.sort_by(|a, b| b.components().count().cmp(&a.components().count()));
    for d in dirs {
        let _ = std::fs::remove_dir(&d); // не пустой — молча не удалится
    }
    freed
}
