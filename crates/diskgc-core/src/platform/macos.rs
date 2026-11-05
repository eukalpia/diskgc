//! macOS: APFS transparent compression via `ditto`, launchd autostart, Chrome update clones.

use std::os::macos::fs::MetadataExt as MacMetadataExt;
use std::path::{Path, PathBuf};

use anyhow::{bail, Context, Result};

use super::unix;
use super::{AutostartStatus, CompressOutcome};
use crate::paths;

pub use unix::{disk_space, file_usage, hardlink_key, owned_by_current_user};

const UF_COMPRESSED: u32 = 0x20;
const LABEL: &str = "com.eukalpia.diskgc";

pub fn is_compressed(meta: &std::fs::Metadata) -> bool {
    meta.st_flags() & UF_COMPRESSED != 0
}

pub fn compression_supported() -> bool {
    true
}

/// `ditto --hfsCompression` writes a compressed copy; we swap it in atomically. ditto itself
/// decides whether the data compresses well enough; if it declines, the copy is uncompressed and
/// we report `NotWorthIt`.
pub fn compress_file(path: &Path) -> Result<CompressOutcome> {
    let meta = std::fs::metadata(path).with_context(|| format!("stat {}", path.display()))?;
    let before = file_usage(&meta);
    let mtime = meta.modified()?;
    let name = path
        .file_name()
        .map(|s| s.to_string_lossy().into_owned())
        .unwrap_or_default();
    let tmp = path.with_file_name(format!(".{name}.diskgc-tmp"));
    let cleanup = |tmp: &Path| {
        let _ = std::fs::remove_file(tmp);
    };
    let out = std::process::Command::new("ditto")
        .arg("--hfsCompression")
        .arg(path)
        .arg(&tmp)
        .output()
        .context("running ditto")?;
    if !out.status.success() {
        cleanup(&tmp);
        bail!(
            "ditto failed: {}",
            String::from_utf8_lossy(&out.stderr).trim()
        );
    }
    let tmeta = std::fs::metadata(&tmp)?;
    if tmeta.len() != meta.len() {
        cleanup(&tmp);
        bail!("size mismatch after ditto");
    }
    if std::fs::metadata(path)?.modified()? != mtime {
        cleanup(&tmp);
        bail!("file changed while compressing");
    }
    // Never open the compressed copy for writing: APFS would decompress it on the spot.
    super::set_mtime(&tmp, mtime)?;
    let compressed = is_compressed(&tmeta);
    std::fs::rename(&tmp, path)?;
    if !compressed {
        return Ok(CompressOutcome::NotWorthIt);
    }
    let after = file_usage(&std::fs::metadata(path)?);
    Ok(CompressOutcome::Compressed { before, after })
}

pub fn has_nocompress_marker(p: &Path) -> bool {
    unix::has_xattr_marker(p)
}

pub fn set_nocompress_marker(p: &Path) {
    unix::set_xattr_marker(p)
}

pub fn system_tmp_dir() -> PathBuf {
    PathBuf::from("/private/tmp")
}

pub fn claude_scratch_root() -> Option<PathBuf> {
    unix::claude_scratch_root(&system_tmp_dir())
}

/// `$TMPDIR` is `/var/folders/xx/yyyy/T`; its siblings are the other per-user system folders.
fn darwin_user_dir(name: &str) -> Option<PathBuf> {
    let t = std::env::var_os("TMPDIR")?;
    let t = PathBuf::from(t);
    let t = t.to_string_lossy().trim_end_matches('/').to_string();
    Some(Path::new(&t).parent()?.join(name))
}

/// The sibling `X` holds Chrome's code-signing clones.
pub fn chrome_clone_dir() -> Option<PathBuf> {
    let x = darwin_user_dir("X")?.join("com.google.Chrome.code_sign_clone");
    x.is_dir().then_some(x)
}

/// `$TMPDIR` itself (`DARWIN_USER_TEMP_DIR`).
pub fn user_tmp_dir() -> Option<PathBuf> {
    darwin_user_dir("T").filter(|t| t.is_dir())
}

/// The sibling `C` (`DARWIN_USER_CACHE_DIR`).
pub fn user_cache_dir() -> Option<PathBuf> {
    darwin_user_dir("C").filter(|c| c.is_dir())
}

pub fn notify(title: &str, body: &str) {
    let script = format!(
        "display notification \"{}\" with title \"{}\"",
        body.replace('"', "'"),
        title.replace('"', "'")
    );
    let _ = std::process::Command::new("osascript")
        .arg("-e")
        .arg(script)
        .output();
}

fn plist_path() -> PathBuf {
    paths::home().join(format!("Library/LaunchAgents/{LABEL}.plist"))
}

fn domain() -> String {
    format!("gui/{}", unix::current_uid())
}

pub fn autostart_install(exe: &Path) -> Result<()> {
    let home = paths::home();
    let log = paths::log_file();
    let launchd_log = log.with_file_name("diskgc.launchd.log");
    let path_env = format!(
        "{h}/.local/bin:{h}/.cargo/bin:{h}/go/bin:/opt/homebrew/bin:/usr/local/bin:/usr/bin:/bin:/usr/sbin:/sbin",
        h = home.display()
    );
    let plist = format!(
        r#"<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0"><dict>
  <key>Label</key><string>{LABEL}</string>
  <key>ProgramArguments</key><array><string>{exe}</string><string>daemon</string></array>
  <key>RunAtLoad</key><true/>
  <key>KeepAlive</key><true/>
  <key>ThrottleInterval</key><integer>30</integer>
  <key>ProcessType</key><string>Background</string>
  <key>Nice</key><integer>10</integer>
  <key>LowPriorityIO</key><true/>
  <key>EnvironmentVariables</key><dict>
    <key>PATH</key><string>{path_env}</string>
    <key>HOME</key><string>{home}</string>
  </dict>
  <key>StandardOutPath</key><string>{launchd_log}</string>
  <key>StandardErrorPath</key><string>{launchd_log}</string>
</dict></plist>
"#,
        exe = exe.display(),
        home = home.display(),
        launchd_log = launchd_log.display(),
    );
    let p = plist_path();
    std::fs::create_dir_all(p.parent().unwrap())?;
    if let Some(parent) = launchd_log.parent() {
        std::fs::create_dir_all(parent)?;
    }
    std::fs::write(&p, plist)?;
    let label = format!("{}/{}", domain(), LABEL);
    let _ = unix::run("launchctl", &["bootout", &label]);
    // launchd tears the old instance down asynchronously; bootstrapping too early fails with
    // "Input/output error". Wait until the label is gone, then retry the bootstrap a few times.
    for _ in 0..40 {
        if unix::run("launchctl", &["print", &label]).is_err() {
            break;
        }
        std::thread::sleep(std::time::Duration::from_millis(250));
    }
    let mut last: Result<String> = Ok(String::new());
    for attempt in 0..10 {
        last = unix::run("launchctl", &["bootstrap", &domain(), p.to_str().unwrap()]);
        if last.is_ok() {
            break;
        }
        std::thread::sleep(std::time::Duration::from_millis(300 * (attempt + 1)));
    }
    last.map(|_| ())
}

pub fn autostart_uninstall() -> Result<()> {
    let _ = unix::run(
        "launchctl",
        &["bootout", &format!("{}/{}", domain(), LABEL)],
    );
    let _ = std::fs::remove_file(plist_path());
    Ok(())
}

pub fn autostart_status() -> AutostartStatus {
    let installed = plist_path().exists();
    match unix::run("launchctl", &["print", &format!("{}/{}", domain(), LABEL)]) {
        Ok(out) => {
            let state = out
                .lines()
                .find(|l| l.contains("state ="))
                .map(|l| l.trim().to_string())
                .unwrap_or_default();
            let pid = out
                .lines()
                .find(|l| l.trim().starts_with("pid ="))
                .and_then(|l| l.split('=').nth(1))
                .and_then(|v| v.trim().parse().ok());
            AutostartStatus {
                installed,
                running: state.contains("running"),
                pid,
                detail: format!("launchd: {state}"),
            }
        }
        Err(_) => AutostartStatus {
            installed,
            running: false,
            pid: None,
            detail: "launchd: not loaded".into(),
        },
    }
}
