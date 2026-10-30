//! Windows: NTFS compression via `compact`, Task Scheduler autostart, sizes from `sysinfo`.

use std::os::windows::fs::MetadataExt;
use std::path::{Path, PathBuf};

use anyhow::{bail, ensure, Context, Result};

use super::{AutostartStatus, CompressOutcome};

const FILE_ATTRIBUTE_COMPRESSED: u32 = 0x800;
const TASK: &str = "diskgc";

pub fn disk_space(path: &Path) -> Result<(u64, u64, u64)> {
    let disks = sysinfo::Disks::new_with_refreshed_list();
    let path = path.canonicalize().unwrap_or_else(|_| path.to_path_buf());
    let best = disks
        .iter()
        .filter(|d| path.starts_with(d.mount_point()))
        .max_by_key(|d| d.mount_point().as_os_str().len())
        .with_context(|| format!("no volume found for {}", path.display()))?;
    let total = best.total_space();
    let avail = best.available_space();
    Ok((total, total.saturating_sub(avail), avail))
}

pub fn file_usage(meta: &std::fs::Metadata) -> u64 {
    meta.len()
}

pub fn hardlink_key(_meta: &std::fs::Metadata) -> Option<(u64, u64)> {
    None
}

pub fn owned_by_current_user(_meta: &std::fs::Metadata) -> bool {
    true
}

pub fn is_compressed(meta: &std::fs::Metadata) -> bool {
    meta.file_attributes() & FILE_ATTRIBUTE_COMPRESSED != 0
}

pub fn compression_supported() -> bool {
    true
}

pub fn compress_file(path: &Path) -> Result<CompressOutcome> {
    let meta = std::fs::metadata(path)?;
    let before = meta.len();
    let out = std::process::Command::new("compact")
        .arg("/C")
        .arg("/Q")
        .arg(path)
        .output()
        .context("running compact")?;
    ensure!(
        out.status.success(),
        "compact failed: {}",
        String::from_utf8_lossy(&out.stdout).trim()
    );
    let after_meta = std::fs::metadata(path)?;
    if !is_compressed(&after_meta) {
        return Ok(CompressOutcome::NotWorthIt);
    }
    // NTFS does not expose the compressed size through std; report a conservative estimate.
    let stdout = String::from_utf8_lossy(&out.stdout);
    let ratio: Option<f64> = stdout
        .lines()
        .find(|l| l.contains("to 1"))
        .and_then(|l| {
            l.split_whitespace()
                .find_map(|w| w.trim_end_matches(',').parse::<f64>().ok())
        })
        .filter(|r| *r >= 1.0);
    let after = match ratio {
        Some(r) => (before as f64 / r) as u64,
        None => before,
    };
    if after >= before {
        bail!("compact reported no gain");
    }
    Ok(CompressOutcome::Compressed { before, after })
}

pub fn has_nocompress_marker(path: &Path) -> bool {
    // NTFS marks the file compressed even when the gain is small, so a retry never happens.
    std::fs::metadata(path)
        .map(|m| is_compressed(&m))
        .unwrap_or(false)
}

pub fn set_nocompress_marker(_path: &Path) {}

pub fn system_tmp_dir() -> PathBuf {
    std::env::temp_dir()
}

pub fn claude_scratch_root() -> Option<PathBuf> {
    let tmp = system_tmp_dir();
    std::fs::read_dir(&tmp)
        .ok()?
        .flatten()
        .map(|e| e.path())
        .find(|p| {
            p.is_dir()
                && p.file_name()
                    .map(|n| n.to_string_lossy().starts_with("claude-"))
                    .unwrap_or(false)
        })
}

pub fn chrome_clone_dir() -> Option<PathBuf> {
    None
}

pub fn notify(title: &str, body: &str) {
    let script = format!(
        "[reflection.assembly]::loadwithpartialname('System.Windows.Forms') | Out-Null; $n = New-Object System.Windows.Forms.NotifyIcon; $n.Icon = [System.Drawing.SystemIcons]::Information; $n.Visible = $true; $n.ShowBalloonTip(5000, '{}', '{}', 'Info')",
        title.replace('\'', ""),
        body.replace('\'', "")
    );
    let _ = std::process::Command::new("powershell")
        .args(["-NoProfile", "-Command", &script])
        .output();
}

fn run(cmd: &str, args: &[&str]) -> Result<String> {
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

pub fn autostart_install(exe: &Path) -> Result<()> {
    let tr = format!("\"{}\" daemon", exe.display());
    let _ = run("schtasks", &["/Delete", "/TN", TASK, "/F"]);
    run(
        "schtasks",
        &[
            "/Create", "/TN", TASK, "/SC", "ONLOGON", "/RL", "LIMITED", "/TR", &tr, "/F",
        ],
    )?;
    run("schtasks", &["/Run", "/TN", TASK])?;
    Ok(())
}

pub fn autostart_uninstall() -> Result<()> {
    let _ = run("schtasks", &["/End", "/TN", TASK]);
    let _ = run("schtasks", &["/Delete", "/TN", TASK, "/F"]);
    Ok(())
}

pub fn autostart_status() -> AutostartStatus {
    match run("schtasks", &["/Query", "/TN", TASK, "/FO", "LIST"]) {
        Ok(out) => {
            let running = out
                .lines()
                .any(|l| l.starts_with("Status:") && l.contains("Running"));
            AutostartStatus {
                installed: true,
                running,
                pid: None,
                detail: format!(
                    "Task Scheduler: {}",
                    if running { "running" } else { "ready" }
                ),
            }
        }
        Err(_) => AutostartStatus {
            installed: false,
            running: false,
            pid: None,
            detail: "Task Scheduler: not registered".into(),
        },
    }
}
