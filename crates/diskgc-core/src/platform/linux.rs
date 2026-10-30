//! Linux: systemd user service for autostart; no transparent compression (btrfs/zfs do their own).

use std::path::{Path, PathBuf};

use anyhow::Result;

use super::unix;
use super::{AutostartStatus, CompressOutcome};
use crate::paths;

pub use unix::{disk_space, file_usage, hardlink_key, owned_by_current_user};

const UNIT: &str = "diskgc.service";

pub fn is_compressed(_meta: &std::fs::Metadata) -> bool {
    false
}

pub fn compression_supported() -> bool {
    false
}

pub fn compress_file(_path: &Path) -> Result<CompressOutcome> {
    Ok(CompressOutcome::Unsupported)
}

pub fn has_nocompress_marker(p: &Path) -> bool {
    unix::has_xattr_marker(p)
}

pub fn set_nocompress_marker(p: &Path) {
    unix::set_xattr_marker(p)
}

pub fn system_tmp_dir() -> PathBuf {
    PathBuf::from("/tmp")
}

pub fn claude_scratch_root() -> Option<PathBuf> {
    unix::claude_scratch_root(&system_tmp_dir())
}

pub fn chrome_clone_dir() -> Option<PathBuf> {
    None
}

pub fn notify(title: &str, body: &str) {
    let _ = std::process::Command::new("notify-send")
        .arg(title)
        .arg(body)
        .output();
}

fn unit_path() -> PathBuf {
    paths::home().join(".config/systemd/user").join(UNIT)
}

pub fn autostart_install(exe: &Path) -> Result<()> {
    let unit = format!(
        "[Unit]\nDescription=diskgc background disk cleaner\nAfter=default.target\n\n[Service]\nExecStart={exe} daemon\nRestart=always\nRestartSec=30\nNice=10\nIOSchedulingClass=idle\n\n[Install]\nWantedBy=default.target\n",
        exe = exe.display()
    );
    let p = unit_path();
    std::fs::create_dir_all(p.parent().unwrap())?;
    std::fs::write(&p, unit)?;
    unix::run("systemctl", &["--user", "daemon-reload"])?;
    unix::run("systemctl", &["--user", "enable", "--now", UNIT])?;
    unix::run("systemctl", &["--user", "restart", UNIT])?;
    Ok(())
}

pub fn autostart_uninstall() -> Result<()> {
    let _ = unix::run("systemctl", &["--user", "disable", "--now", UNIT]);
    let _ = std::fs::remove_file(unit_path());
    let _ = unix::run("systemctl", &["--user", "daemon-reload"]);
    Ok(())
}

pub fn autostart_status() -> AutostartStatus {
    let installed = unit_path().exists();
    let running = unix::run("systemctl", &["--user", "is-active", UNIT])
        .map(|s| s.trim() == "active")
        .unwrap_or(false);
    let pid = unix::run(
        "systemctl",
        &["--user", "show", "-p", "MainPID", "--value", UNIT],
    )
    .ok()
    .and_then(|s| s.trim().parse::<u32>().ok())
    .filter(|p| *p > 0);
    AutostartStatus {
        installed,
        running,
        pid,
        detail: format!(
            "systemd --user: {}",
            if running { "active" } else { "inactive" }
        ),
    }
}
