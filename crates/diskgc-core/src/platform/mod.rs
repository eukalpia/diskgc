//! Everything that differs between operating systems. The rest of the crate calls these
//! functions and never uses `cfg!` itself.

use std::path::{Path, PathBuf};
use std::time::SystemTime;

use anyhow::Result;
use serde::{Deserialize, Serialize};

#[cfg(target_os = "linux")]
mod linux;
#[cfg(target_os = "macos")]
mod macos;
#[cfg(unix)]
mod unix;
#[cfg(windows)]
mod windows;

#[cfg(target_os = "linux")]
use linux as os;
#[cfg(target_os = "macos")]
use macos as os;
#[cfg(windows)]
use windows as os;

/// `(total, used, available)` in bytes for the volume containing `path`.
pub fn disk_space(path: &Path) -> Result<(u64, u64, u64)> {
    os::disk_space(path)
}

/// Bytes a file really occupies on disk (allocated blocks, so compressed and sparse files count
/// small). Falls back to the logical length where the OS does not expose allocation.
pub fn file_usage(meta: &std::fs::Metadata) -> u64 {
    os::file_usage(meta)
}

/// Identity of a hard-linked file so it is counted once, like `du` does. `None` when the file has
/// a single link or the platform does not expose link counts.
pub fn hardlink_key(meta: &std::fs::Metadata) -> Option<(u64, u64)> {
    os::hardlink_key(meta)
}

pub fn is_compressed(meta: &std::fs::Metadata) -> bool {
    os::is_compressed(meta)
}

pub fn compression_supported() -> bool {
    os::compression_supported()
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CompressOutcome {
    /// The file is now transparently compressed; bytes on disk before and after.
    Compressed { before: u64, after: u64 },
    /// The filesystem tool judged the file not worth compressing (already dense data).
    NotWorthIt,
    /// This platform/filesystem has no transparent compression.
    Unsupported,
}

/// Transparently compress `path` in place. The file keeps its name, contents and mtime; only the
/// on-disk representation changes.
pub fn compress_file(path: &Path) -> Result<CompressOutcome> {
    os::compress_file(path)
}

/// Marker for files that were tried and found incompressible, so they are not retried.
pub fn has_nocompress_marker(path: &Path) -> bool {
    os::has_nocompress_marker(path)
}

pub fn set_nocompress_marker(path: &Path) {
    os::set_nocompress_marker(path)
}

/// Set mtime without opening the file for writing (which would undo transparent compression).
pub fn set_mtime(path: &Path, t: SystemTime) -> Result<()> {
    filetime::set_file_mtime(path, filetime::FileTime::from_system_time(t))?;
    Ok(())
}

pub fn owned_by_current_user(meta: &std::fs::Metadata) -> bool {
    os::owned_by_current_user(meta)
}

/// The shared system temp directory (not the per-user `$TMPDIR` on macOS).
pub fn system_tmp_dir() -> PathBuf {
    os::system_tmp_dir()
}

/// The per-user temp directory the system manages besides the shared one (`$TMPDIR`, i.e.
/// `/var/folders/…/T`, on macOS), where build tools leave work directories. `None` elsewhere.
pub fn user_tmp_dir() -> Option<PathBuf> {
    os::user_tmp_dir()
}

/// Where Claude Code keeps per-session scratch directories for the current user.
pub fn claude_scratch_root() -> Option<PathBuf> {
    os::claude_scratch_root()
}

/// Directory in which Chrome leaves code-signing clones after updates (macOS only).
pub fn chrome_clone_dir() -> Option<PathBuf> {
    os::chrome_clone_dir()
}

/// The per-user cache directory the system itself manages (`/var/folders/…/C` on macOS), where
/// Metal keeps compiled shaders per application. `None` elsewhere.
pub fn user_cache_dir() -> Option<PathBuf> {
    os::user_cache_dir()
}

/// sccache's local disk cache: `$SCCACHE_DIR`, else the platform default it uses itself.
pub fn sccache_dir() -> Option<PathBuf> {
    if let Some(d) = std::env::var_os("SCCACHE_DIR") {
        return Some(PathBuf::from(d));
    }
    let base = dirs::cache_dir()?;
    Some(if cfg!(target_os = "macos") {
        base.join("Mozilla.sccache")
    } else if cfg!(windows) {
        base.join("Mozilla").join("sccache").join("cache")
    } else {
        base.join("sccache")
    })
}

pub fn notify(title: &str, body: &str) {
    os::notify(title, body)
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct AutostartStatus {
    pub installed: bool,
    pub running: bool,
    pub pid: Option<u32>,
    pub detail: String,
}

/// Register `exe daemon` to start at login and keep running.
pub fn autostart_install(exe: &Path) -> Result<()> {
    os::autostart_install(exe)
}

pub fn autostart_uninstall() -> Result<()> {
    os::autostart_uninstall()
}

pub fn autostart_status() -> AutostartStatus {
    os::autostart_status()
}

/// Locations the operating system guards with a privacy prompt (macOS TCC). The scanner never
/// descends into them: nothing there is build output, and touching them from a background
/// daemon makes the system ask the user for permission — on every rebuild of an unsigned binary.
pub fn protected_paths() -> &'static [PathBuf] {
    static LIST: std::sync::OnceLock<Vec<PathBuf>> = std::sync::OnceLock::new();
    LIST.get_or_init(|| {
        if !cfg!(target_os = "macos") {
            return Vec::new();
        }
        let h = crate::paths::home();
        [
            "Music",
            "Pictures",
            "Movies",
            "Desktop",
            "Downloads",
            "Documents",
            "Library/Mail",
            "Library/Safari",
            "Library/Messages",
            "Library/Cookies",
            "Library/HomeKit",
            "Library/Suggestions",
            "Library/Calendars",
            "Library/Reminders",
            "Library/Containers",
            "Library/Group Containers",
            "Library/Application Support/AddressBook",
            "Library/Application Support/CallHistoryDB",
            "Library/Application Support/CallHistoryTransactions",
            "Library/Application Support/com.apple.TCC",
            "Library/Application Support/com.apple.sharedfilelist",
            "Library/Application Support/Knowledge",
            "Library/Metadata/CoreSpotlight",
            "Library/PersonalizationPortrait",
            "Library/IdentityServices",
            "Library/Autosave Information",
            "Library/Sharing",
            "Library/Daemon Containers",
            "Library/Trial",
            "Library/Biome",
            "Library/Accounts",
            "Library/Photos",
            "Library/Weather",
            "Library/DuetExpertCenter",
            "Library/CoreFollowUp",
            "Library/Passes",
            "Library/StatusKit",
        ]
        .iter()
        .map(|p| h.join(p))
        .collect()
    })
}

/// Should the scanner stay out of `path`? Protected locations plus Apple's own caches and
/// application-support folders, which are system-managed and TCC-guarded (Music, Safari, Mail…).
pub fn is_protected(path: &Path) -> bool {
    if protected_paths().iter().any(|p| path.starts_with(p)) {
        return true;
    }
    if !cfg!(target_os = "macos") {
        return false;
    }
    // Apple's own caches / support folders anywhere up the chain: ~/Library/Caches/com.apple.Music/…
    const APPLE_PARENTS: &[&str] = &[
        "/Library/Caches",
        "/Library/Application Support",
        "/Library/HTTPStorages",
        "/Library/WebKit",
        "/Library/Saved Application State",
    ];
    path.ancestors().any(
        |a| match (a.parent(), a.file_name().and_then(|n| n.to_str())) {
            (Some(parent), Some(name)) if name.starts_with("com.apple.") => {
                let parent = parent.to_string_lossy();
                APPLE_PARENTS.iter().any(|p| parent.ends_with(p))
            }
            _ => false,
        },
    )
}

/// True if `name` resolves to an executable on `PATH`.
pub fn which(name: &str) -> bool {
    let path = std::env::var_os("PATH").unwrap_or_default();
    let exts: Vec<String> = if cfg!(windows) {
        std::env::var("PATHEXT")
            .unwrap_or_else(|_| ".EXE;.CMD;.BAT".into())
            .split(';')
            .map(|s| s.to_lowercase())
            .collect()
    } else {
        vec![String::new()]
    };
    std::env::split_paths(&path).any(|dir| {
        exts.iter()
            .any(|ext| dir.join(format!("{name}{ext}")).is_file())
    })
}
