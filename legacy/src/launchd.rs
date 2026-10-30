//! Автозапуск демона через launchd (KeepAlive — перезапустится сам, стартует при логине).

use anyhow::{Context, Result};
use std::path::PathBuf;
use std::process::Command;

use crate::fsutil::home;

pub const LABEL: &str = "com.eukalpia.diskgc";

fn plist_path() -> PathBuf {
    home().join(format!("Library/LaunchAgents/{LABEL}.plist"))
}

fn uid() -> u32 {
    unsafe { libc::getuid() }
}

pub fn install() -> Result<()> {
    let exe = std::env::current_exe()?.canonicalize()?;
    let h = home();
    let path_env = format!(
        "{h}/.local/bin:{h}/.cargo/bin:{h}/go/bin:/opt/homebrew/bin:/usr/local/bin:/usr/bin:/bin:/usr/sbin:/sbin",
        h = h.display()
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
  <key>StandardOutPath</key><string>{home}/Library/Logs/diskgc.launchd.log</string>
  <key>StandardErrorPath</key><string>{home}/Library/Logs/diskgc.launchd.log</string>
</dict></plist>
"#,
        exe = exe.display(),
        home = h.display(),
    );
    let p = plist_path();
    std::fs::create_dir_all(p.parent().unwrap())?;
    std::fs::write(&p, plist).with_context(|| format!("write {}", p.display()))?;
    let _ = Command::new("launchctl").args(["bootout", &format!("gui/{}/{}", uid(), LABEL)]).output();
    let out = Command::new("launchctl")
        .args(["bootstrap", &format!("gui/{}", uid()), p.to_str().unwrap()])
        .output()?;
    anyhow::ensure!(out.status.success(), "launchctl bootstrap: {}", String::from_utf8_lossy(&out.stderr));
    println!("Демон установлен и запущен ({LABEL}). Бинарник: {}", exe.display());
    println!("Лог: {}   Статус: diskgc status", crate::log_path().display());
    Ok(())
}

pub fn uninstall() -> Result<()> {
    let _ = Command::new("launchctl").args(["bootout", &format!("gui/{}/{}", uid(), LABEL)]).output();
    let _ = std::fs::remove_file(plist_path());
    println!("Демон остановлен и убран из автозапуска.");
    Ok(())
}

pub fn status() {
    let out = Command::new("launchctl").args(["print", &format!("gui/{}/{}", uid(), LABEL)]).output();
    match out {
        Ok(o) if o.status.success() => {
            let s = String::from_utf8_lossy(&o.stdout);
            let state = s.lines().find(|l| l.contains("state =")).map(|l| l.trim().to_string()).unwrap_or_default();
            let pid = s.lines().find(|l| l.trim().starts_with("pid =")).map(|l| l.trim().to_string()).unwrap_or_default();
            println!("Демон: установлен, {} {}", state, pid);
        }
        _ => println!("Демон: не установлен (diskgc install)"),
    }
}
