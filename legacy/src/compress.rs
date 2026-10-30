//! Прозрачное APFS-сжатие холодных файлов через `ditto --hfsCompression`.
//! Файл остаётся обычным файлом для всех программ, но занимает на диске в разы меньше
//! (JSONL-истории сессий сжимаются в 5–8 раз). Повторно не сжимается — по флагу UF_COMPRESSED.

use anyhow::{bail, Context, Result};
use std::os::unix::fs::MetadataExt;
use std::path::Path;
use std::time::SystemTime;
use walkdir::WalkDir;

use crate::config::Config;
use crate::fsutil::{days, expand, is_compressed};
use crate::rules::{Kind, Plan};

pub fn plan(cfg: &Config, plan: &mut Plan) {
    let cutoff = SystemTime::now() - days(cfg.compress.min_age_days);
    let min = cfg.compress.min_size_kb * 1024;
    for d in &cfg.compress.dirs {
        let root = expand(d);
        if !root.is_dir() {
            continue;
        }
        for e in WalkDir::new(&root).follow_links(false).into_iter().filter_map(|e| e.ok()) {
            if !e.file_type().is_file() {
                continue;
            }
            let ext = e.path().extension().map(|x| x.to_string_lossy().to_lowercase()).unwrap_or_default();
            if !cfg.compress.extensions.iter().any(|x| *x == ext) {
                continue;
            }
            let m = match e.metadata() {
                Ok(m) => m,
                Err(_) => continue,
            };
            if m.len() < min || is_compressed(&m) || m.modified().map(|t| t > cutoff).unwrap_or(true) {
                continue;
            }
            if has_marker(e.path()) {
                continue; // уже пробовали — не сжимается (картинки/base64 внутри)
            }
            if cfg.excluded(e.path()) {
                continue;
            }
            plan.actions.push(crate::rules::Action {
                kind: Kind::Compress,
                path: e.path().to_path_buf(),
                bytes: m.blocks() * 512,
                reason: format!("холодный файл > {} дн.", cfg.compress.min_age_days),
                group: "compress",
            });
        }
    }
}

/// Возвращает, сколько байт на диске сэкономлено.
pub fn compress_file(p: &Path) -> Result<u64> {
    let m = std::fs::metadata(p).with_context(|| format!("stat {}", p.display()))?;
    let before = m.blocks() * 512;
    let mtime = m.modified()?;
    let tmp = p.with_file_name(format!(".{}.diskgc-tmp", crate::fsutil::file_name(p)));
    let st = std::process::Command::new("ditto")
        .arg("--hfsCompression")
        .arg(p)
        .arg(&tmp)
        .output()
        .context("ditto")?;
    if !st.status.success() {
        let _ = std::fs::remove_file(&tmp);
        bail!("ditto failed: {}", String::from_utf8_lossy(&st.stderr).trim());
    }
    let tm = std::fs::metadata(&tmp)?;
    if tm.len() != m.len() {
        let _ = std::fs::remove_file(&tmp);
        bail!("size mismatch after ditto");
    }
    // исходник изменился, пока мы сжимали — не трогаем
    if std::fs::metadata(p)?.modified()? != mtime {
        let _ = std::fs::remove_file(&tmp);
        bail!("file changed during compression");
    }
    // ВАЖНО: нельзя открывать сжатый файл на запись — APFS тут же его распакует.
    // Поэтому дата изменения ставится через utimensat по пути.
    set_mtime(&tmp, mtime)?;
    let compressed = is_compressed(&tm);
    std::fs::rename(&tmp, p)?;
    if !compressed {
        set_marker(p); // ditto решил, что файл не стоит сжимать — больше не трогаем
        return Ok(0);
    }
    let after = std::fs::metadata(p)?.blocks() * 512;
    Ok(before.saturating_sub(after))
}

const MARKER: &str = "com.diskgc.nocompress\0";

fn cpath(p: &Path) -> Option<std::ffi::CString> {
    use std::os::unix::ffi::OsStrExt;
    std::ffi::CString::new(p.as_os_str().as_bytes()).ok()
}

fn has_marker(p: &Path) -> bool {
    let c = match cpath(p) {
        Some(c) => c,
        None => return false,
    };
    let r = unsafe { libc::getxattr(c.as_ptr(), MARKER.as_ptr() as *const libc::c_char, std::ptr::null_mut(), 0, 0, 0) };
    r >= 0
}

fn set_marker(p: &Path) {
    if let Some(c) = cpath(p) {
        let v = b"1";
        unsafe {
            libc::setxattr(c.as_ptr(), MARKER.as_ptr() as *const libc::c_char, v.as_ptr() as *const libc::c_void, v.len(), 0, 0);
        }
    }
}

fn set_mtime(p: &Path, t: SystemTime) -> Result<()> {
    let d = t.duration_since(SystemTime::UNIX_EPOCH).unwrap_or_default();
    let ts = libc::timespec { tv_sec: d.as_secs() as libc::time_t, tv_nsec: d.subsec_nanos() as libc::c_long };
    let omit = libc::timespec { tv_sec: 0, tv_nsec: libc::UTIME_OMIT };
    let times = [omit, ts];
    let c = cpath(p).context("path")?;
    let r = unsafe { libc::utimensat(libc::AT_FDCWD, c.as_ptr(), times.as_ptr(), 0) };
    anyhow::ensure!(r == 0, "utimensat failed");
    Ok(())
}
