//! Работа с файловой системой: реальный размер на диске, «простой» каталога,
//! свободное место, флаг APFS-сжатия, поиск каталогов по имени.

use anyhow::{ensure, Result};
use std::os::macos::fs::MetadataExt as MacMetadataExt;
use std::os::unix::ffi::OsStrExt;
use std::os::unix::fs::MetadataExt;
use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime};
use walkdir::WalkDir;

pub const GB: f64 = 1024.0 * 1024.0 * 1024.0;
const UF_COMPRESSED: u32 = 0x20;

pub fn gb(bytes: u64) -> f64 {
    bytes as f64 / GB
}

pub fn home() -> PathBuf {
    dirs::home_dir().expect("HOME is not set")
}

pub fn expand(p: &str) -> PathBuf {
    match p.strip_prefix('~') {
        Some(rest) => home().join(rest.trim_start_matches('/')),
        None => PathBuf::from(p),
    }
}

/// Сколько байт занято на диске (blocks*512, не длина файла — учитывает сжатие и дыры).
/// Жёсткие ссылки (cargo так дублирует бинарники) считаются один раз, как в du.
pub fn disk_usage(p: &Path) -> u64 {
    walk_bytes(p, |_, _| true)
}

/// Сумма занятого места по файлам внутри root, для которых pred == true; hardlinks — один раз.
pub fn walk_bytes(root: &Path, pred: impl Fn(&walkdir::DirEntry, &std::fs::Metadata) -> bool) -> u64 {
    match std::fs::symlink_metadata(root) {
        Ok(m) if !m.is_dir() => return m.blocks() * 512,
        Err(_) => return 0,
        _ => {}
    }
    let mut seen: std::collections::HashSet<(u64, u64)> = std::collections::HashSet::new();
    let mut total = 0u64;
    for e in WalkDir::new(root).follow_links(false).into_iter().filter_map(|e| e.ok()) {
        let m = match e.metadata() {
            Ok(m) => m,
            Err(_) => continue,
        };
        if m.nlink() > 1 && !seen.insert((m.dev(), m.ino())) {
            continue;
        }
        if e.file_type().is_dir() || pred(&e, &m) {
            total += m.blocks() * 512;
        }
    }
    total
}

pub fn is_compressed(m: &std::fs::Metadata) -> bool {
    m.st_flags() & UF_COMPRESSED != 0
}

/// Есть ли внутри файл, изменённый позже cutoff (каталоги из prune не смотрим).
pub fn has_file_newer_than(p: &Path, cutoff: SystemTime, prune: &[&str]) -> bool {
    if let Ok(m) = std::fs::symlink_metadata(p) {
        if !m.is_dir() {
            return m.modified().map(|t| t > cutoff).unwrap_or(true);
        }
    }
    let it = WalkDir::new(p).follow_links(false).into_iter().filter_entry(|e| {
        !(e.file_type().is_dir()
            && e.depth() > 0
            && prune.iter().any(|n| e.file_name().to_string_lossy() == *n))
    });
    for e in it.filter_map(|e| e.ok()) {
        if e.file_type().is_file() {
            match e.metadata().ok().and_then(|m| m.modified().ok()) {
                Some(t) if t > cutoff => return true,
                None => return true, // не смогли прочитать — считаем живым
                _ => {}
            }
        }
    }
    false
}

/// Каталог «простаивает» дольше d: ничего внутри не менялось.
pub fn idle_for(p: &Path, d: Duration, prune: &[&str]) -> bool {
    let cutoff = SystemTime::now() - d;
    !has_file_newer_than(p, cutoff, prune)
}

pub fn days(n: u64) -> Duration {
    Duration::from_secs(n * 86_400)
}
pub fn hours(n: u64) -> Duration {
    Duration::from_secs(n * 3_600)
}

/// (всего, занято, доступно) в байтах для тома, на котором лежит path.
pub fn disk_space(path: &Path) -> Result<(u64, u64, u64)> {
    let c = std::ffi::CString::new(path.as_os_str().as_bytes())?;
    let mut st: libc::statfs = unsafe { std::mem::zeroed() };
    let r = unsafe { libc::statfs(c.as_ptr(), &mut st) };
    ensure!(r == 0, "statfs({}) failed", path.display());
    let bs = st.f_bsize as u64;
    Ok((st.f_blocks * bs, (st.f_blocks - st.f_bfree) * bs, st.f_bavail * bs))
}

pub const DATA_VOLUME: &str = "/System/Volumes/Data";

pub fn free_bytes() -> u64 {
    disk_space(Path::new(DATA_VOLUME)).map(|t| t.2).unwrap_or(0)
}

const ALWAYS_SKIP: &[&str] = &["node_modules", ".git", "Library"];

/// Каталоги с именем name в пределах глубины; внутрь найденных не заходим.
pub fn find_dirs_named(roots: &[PathBuf], depth: usize, name: &str) -> Vec<PathBuf> {
    let mut out = Vec::new();
    for root in roots {
        if !root.is_dir() {
            continue;
        }
        let mut it = WalkDir::new(root)
            .follow_links(false)
            .max_depth(depth)
            .into_iter()
            .filter_entry(|e| {
                let n = e.file_name().to_string_lossy();
                !(e.depth() > 0 && e.file_type().is_dir() && ALWAYS_SKIP.contains(&n.as_ref()) && n != name)
            });
        while let Some(Ok(e)) = it.next() {
            if e.depth() > 0 && e.file_type().is_dir() && e.file_name().to_string_lossy() == name {
                out.push(e.path().to_path_buf());
                it.skip_current_dir();
            }
        }
    }
    out.sort();
    out.dedup();
    out
}

/// Каталоги сборки cargo: любой каталог с файлом CACHEDIR.TAG, в котором упомянут cargo.
pub fn find_cargo_targets(roots: &[PathBuf], depth: usize) -> Vec<PathBuf> {
    let mut out = Vec::new();
    for root in roots {
        if !root.is_dir() {
            continue;
        }
        let mut it = WalkDir::new(root)
            .follow_links(false)
            .max_depth(depth)
            .sort_by_file_name()
            .into_iter()
            .filter_entry(|e| {
                let n = e.file_name().to_string_lossy();
                !(e.depth() > 0 && e.file_type().is_dir() && ALWAYS_SKIP.contains(&n.as_ref()))
            });
        while let Some(Ok(e)) = it.next() {
            if !e.file_type().is_file() {
                continue;
            }
            // cargo кладёт в корень target/ оба файла; CACHEDIR.TAG иногда отсутствует
            let is_cargo = match e.file_name().to_str() {
                Some(".rustc_info.json") => true,
                Some("CACHEDIR.TAG") => std::fs::read_to_string(e.path())
                    .map(|t| t.to_lowercase().contains("cargo"))
                    .unwrap_or(false),
                _ => false,
            };
            if is_cargo {
                if let Some(parent) = e.path().parent() {
                    out.push(parent.to_path_buf());
                }
                it.skip_current_dir();
            }
        }
    }
    out.sort();
    out.dedup();
    out
}

pub fn file_name(p: &Path) -> String {
    p.file_name().map(|s| s.to_string_lossy().into_owned()).unwrap_or_default()
}

pub fn now_string() -> String {
    chrono::Local::now().format("%Y-%m-%d %H:%M:%S").to_string()
}
