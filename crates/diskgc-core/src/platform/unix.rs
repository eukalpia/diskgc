//! Shared by macOS and Linux.

use std::ffi::CString;
use std::os::unix::ffi::OsStrExt;
use std::os::unix::fs::MetadataExt;
use std::path::{Path, PathBuf};

use anyhow::{ensure, Result};

pub fn disk_space(path: &Path) -> Result<(u64, u64, u64)> {
    let c = CString::new(path.as_os_str().as_bytes())?;
    let mut st: libc::statvfs = unsafe { std::mem::zeroed() };
    let r = unsafe { libc::statvfs(c.as_ptr(), &mut st) };
    ensure!(
        r == 0,
        "statvfs({}) failed: {}",
        path.display(),
        std::io::Error::last_os_error()
    );
    let frsize = st.f_frsize as u64;
    let total = st.f_blocks as u64 * frsize;
    let free = st.f_bfree as u64 * frsize;
    let avail = st.f_bavail as u64 * frsize;
    Ok((total, total.saturating_sub(free), avail))
}

pub fn file_usage(meta: &std::fs::Metadata) -> u64 {
    meta.blocks() * 512
}

pub fn hardlink_key(meta: &std::fs::Metadata) -> Option<(u64, u64)> {
    (meta.nlink() > 1).then(|| (meta.dev(), meta.ino()))
}

pub fn owned_by_current_user(meta: &std::fs::Metadata) -> bool {
    meta.uid() == unsafe { libc::getuid() }
}

pub fn current_uid() -> u32 {
    unsafe { libc::getuid() }
}

fn cpath(p: &Path) -> Option<CString> {
    CString::new(p.as_os_str().as_bytes()).ok()
}

const MARKER: &[u8] = b"user.diskgc.nocompress\0";

pub fn has_xattr_marker(p: &Path) -> bool {
    let Some(c) = cpath(p) else { return false };
    #[cfg(target_os = "macos")]
    let r = unsafe {
        libc::getxattr(
            c.as_ptr(),
            MARKER.as_ptr() as *const libc::c_char,
            std::ptr::null_mut(),
            0,
            0,
            0,
        )
    };
    #[cfg(target_os = "linux")]
    let r = unsafe {
        libc::getxattr(
            c.as_ptr(),
            MARKER.as_ptr() as *const libc::c_char,
            std::ptr::null_mut(),
            0,
        )
    };
    r >= 0
}

pub fn set_xattr_marker(p: &Path) {
    let Some(c) = cpath(p) else { return };
    let v = b"1";
    #[cfg(target_os = "macos")]
    unsafe {
        libc::setxattr(
            c.as_ptr(),
            MARKER.as_ptr() as *const libc::c_char,
            v.as_ptr() as *const libc::c_void,
            v.len(),
            0,
            0,
        );
    }
    #[cfg(target_os = "linux")]
    unsafe {
        libc::setxattr(
            c.as_ptr(),
            MARKER.as_ptr() as *const libc::c_char,
            v.as_ptr() as *const libc::c_void,
            v.len(),
            0,
        );
    }
}

pub fn claude_scratch_root(tmp: &Path) -> Option<PathBuf> {
    let p = tmp.join(format!("claude-{}", current_uid()));
    p.is_dir().then_some(p)
}

/// Run a command, returning stdout on success.
pub fn run(cmd: &str, args: &[&str]) -> Result<String> {
    let out = std::process::Command::new(cmd).args(args).output()?;
    ensure!(
        out.status.success(),
        "{} {}: {}",
        cmd,
        args.join(" "),
        String::from_utf8_lossy(&out.stderr).trim()
    );
    Ok(String::from_utf8_lossy(&out.stdout).into_owned())
}
