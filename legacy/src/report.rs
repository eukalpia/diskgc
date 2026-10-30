//! Отчёт: что и сколько занимает, по категориям и с топом виновников.

use std::collections::BTreeMap;
use std::path::PathBuf;

use crate::config::Config;
use crate::fsutil::{self, disk_usage, expand, gb, home, DATA_VOLUME};

pub struct Categories {
    pub sizes: BTreeMap<String, f64>,
    pub rust_targets: Vec<(u64, PathBuf)>,
    pub node_modules: Vec<(u64, PathBuf)>,
    pub scratch: Vec<(u64, PathBuf)>,
}

fn sized(paths: Vec<PathBuf>) -> Vec<(u64, PathBuf)> {
    let mut v: Vec<(u64, PathBuf)> = paths.into_iter().map(|p| (disk_usage(&p), p)).collect();
    v.sort_by(|a, b| b.0.cmp(&a.0));
    v
}

pub fn collect(cfg: &Config) -> Categories {
    let h = home();
    let roots: Vec<PathBuf> = cfg.scan.roots.iter().map(|r| expand(r)).collect();
    let mut troots = roots.clone();
    troots.extend(cfg.scan.extra_target_roots.iter().map(|x| expand(x)));

    let rust_targets = sized(fsutil::find_cargo_targets(&troots, cfg.scan.depth));
    let node_modules = sized(fsutil::find_dirs_named(&roots, cfg.scan.depth, "node_modules"));
    let uid = unsafe { libc::getuid() };
    let claude_tmp = PathBuf::from(format!("/private/tmp/claude-{}", uid));
    let scratch = sized(
        std::fs::read_dir(&claude_tmp)
            .into_iter()
            .flatten()
            .flatten()
            .flat_map(|p| std::fs::read_dir(p.path()).into_iter().flatten().flatten().map(|s| s.path()))
            .collect(),
    );

    let mut sizes = BTreeMap::new();
    let sum = |v: &Vec<(u64, PathBuf)>| gb(v.iter().map(|x| x.0).sum());
    sizes.insert("rust_targets".into(), sum(&rust_targets));
    sizes.insert("node_modules".into(), sum(&node_modules));
    sizes.insert("claude_scratch".into(), sum(&scratch));
    let tmp_other: u64 = std::fs::read_dir("/private/tmp")
        .into_iter()
        .flatten()
        .flatten()
        .map(|e| e.path())
        .filter(|p| *p != claude_tmp)
        .map(|p| disk_usage(&p))
        .sum();
    sizes.insert("tmp_other".into(), gb(tmp_other));
    sizes.insert("codex".into(), gb(disk_usage(&h.join(".codex")) + disk_usage(&h.join(".cache/codex-runtimes"))));
    sizes.insert(
        "claude".into(),
        gb(disk_usage(&h.join(".claude")) + disk_usage(&h.join("Library/Application Support/Claude"))),
    );
    sizes.insert("docker_orbstack".into(), gb(disk_usage(&h.join("Library/Group Containers/HUAQ24HBR6.dev.orbstack/data"))));
    sizes.insert("library_caches".into(), gb(disk_usage(&h.join("Library/Caches"))));
    sizes.insert("xcode_simulators".into(), gb(disk_usage(&h.join("Library/Developer"))));
    if let Some(x) = crate::rules::darwin_user_dir("X") {
        sizes.insert("chrome_clones".into(), gb(disk_usage(&x.join("com.google.Chrome.code_sign_clone"))));
    }
    sizes.insert("toolchain_caches".into(), gb([".cargo", ".rustup", ".npm", ".pub-cache", "go", ".cache", ".bun"].iter().map(|d| disk_usage(&h.join(d))).sum()));

    Categories { sizes, rust_targets, node_modules, scratch }
}

fn top(title: &str, v: &[(u64, PathBuf)], n: usize) {
    if v.is_empty() {
        return;
    }
    println!("\n== {} ==", title);
    for (b, p) in v.iter().take(n) {
        println!("  {:>7.1} ГБ  {}", gb(*b), p.display());
    }
}

pub fn print(cfg: &Config) {
    if let Ok((total, used, avail)) = fsutil::disk_space(std::path::Path::new(DATA_VOLUME)) {
        println!("== Диск: всего {:.0} ГБ, занято {:.0} ГБ, свободно {:.0} ГБ ==", gb(total), gb(used), gb(avail));
    }
    let c = collect(cfg);
    println!("\n== Категории (ГБ) ==");
    let mut keys: Vec<&String> = c.sizes.keys().collect();
    keys.sort_by(|a, b| c.sizes[*b].partial_cmp(&c.sizes[*a]).unwrap());
    for k in keys {
        println!("  {:<18}{:>8.1}", k, c.sizes[k]);
    }
    top("Rust target/", &c.rust_targets, 10);
    top("node_modules", &c.node_modules, 10);
    top("Временные каталоги сессий Claude Code", &c.scratch, 6);
    if crate::docker::available() {
        println!("\n== Docker ==");
        let out = std::process::Command::new("docker").args(["system", "df"]).output();
        if let Ok(o) = out {
            for l in String::from_utf8_lossy(&o.stdout).lines() {
                println!("  {}", l);
            }
        }
    }
}
