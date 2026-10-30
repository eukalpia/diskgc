//! Memory report: who uses RAM, and which processes are orphans of finished sessions
//! (their parent is gone, they were re-parented to PID 1, and they look like tooling helpers).

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};
use sysinfo::{ProcessesToUpdate, System};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProcInfo {
    pub pid: u32,
    pub parent: Option<u32>,
    pub rss: u64,
    pub run_hours: f64,
    pub program: String,
    pub command: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MemReport {
    pub total: u64,
    pub used: u64,
    pub available: u64,
    pub swap_total: u64,
    pub swap_used: u64,
    pub pressure: bool,
    pub top: Vec<ProcInfo>,
    pub by_program: Vec<(String, u64, usize)>,
    pub orphans: Vec<ProcInfo>,
    /// Main processes of browsers launched by automation tooling (see [`is_automation_browser`]).
    pub automation_browsers: Vec<ProcInfo>,
    pub orphan_containers: usize,
}

/// Markers of a browser started by Playwright, Puppeteer, Selenium or a headless shell rather
/// than by a person. Only the main browser process is matched; its helpers die with it.
const AUTOMATION_MARKERS: &[&str] = &[
    "playwright",
    "puppeteer",
    "--remote-debugging-pipe",
    "--remote-debugging-port",
    "--headless",
    "chrome-headless-shell",
    "headless_shell",
    "--enable-automation",
];

const BROWSER_NAMES: &[&str] = &[
    "Google Chrome",
    "Chromium",
    "chrome",
    "headless_shell",
    "chrome-headless-shell",
    "msedge",
    "firefox",
];

pub fn is_automation_browser(program: &str, command: &str) -> bool {
    let is_browser = BROWSER_NAMES
        .iter()
        .any(|b| program.contains(b) || command.contains(b));
    let is_main = !command.contains("--type=");
    is_browser && is_main && AUTOMATION_MARKERS.iter().any(|m| command.contains(m))
}

/// Helpers that outlive the sessions that spawned them.
const ORPHAN_PATTERNS: &[&str] = &[
    "codex-code-mode-host",
    "mcp/server.mjs",
    "rust-analyzer-proc-macro-srv",
    "rust-analyzer",
    "chrome-headless-shell",
    "headless_shell",
    "playwright",
    "esbuild",
    "tsserver",
    "dart:analysis",
    "flutter_tester",
    "gopls",
];

const ORPHAN_MIN_HOURS: f64 = 12.0;

fn program_name(cmd: &str, name: &str) -> String {
    if let Some(i) = cmd.find(".app/") {
        if let Some(j) = cmd[..i].rfind('/') {
            return cmd[j + 1..i].to_string();
        }
    }
    let first = cmd.split_whitespace().next().unwrap_or(name);
    let base = std::path::Path::new(first)
        .file_name()
        .map(|s| s.to_string_lossy().into_owned())
        .unwrap_or_else(|| first.to_string());
    if ["node", "python3", "python", "bun", "deno", "dart"].contains(&base.as_str()) {
        if let Some(script) = cmd.split_whitespace().nth(1) {
            let sb = std::path::Path::new(script)
                .file_name()
                .map(|s| s.to_string_lossy().into_owned())
                .unwrap_or_default();
            if !sb.is_empty() && !sb.starts_with('-') {
                return format!("{base} {sb}");
            }
        }
    }
    if base.is_empty() {
        name.to_string()
    } else {
        base
    }
}

pub fn report(include_docker: bool) -> MemReport {
    let mut sys = System::new();
    sys.refresh_memory();
    sys.refresh_processes(ProcessesToUpdate::All, true);
    let mut procs: Vec<ProcInfo> = sys
        .processes()
        .iter()
        .map(|(pid, p)| {
            let cmd: Vec<String> = p
                .cmd()
                .iter()
                .map(|s| s.to_string_lossy().into_owned())
                .collect();
            let command = if cmd.is_empty() {
                p.name().to_string_lossy().into_owned()
            } else {
                cmd.join(" ")
            };
            ProcInfo {
                pid: pid.as_u32(),
                parent: p.parent().map(|x| x.as_u32()),
                rss: p.memory(),
                run_hours: p.run_time() as f64 / 3600.0,
                program: program_name(&command, &p.name().to_string_lossy()),
                command,
            }
        })
        .collect();
    procs.sort_by_key(|p| std::cmp::Reverse(p.rss));
    let mut groups: BTreeMap<String, (u64, usize)> = BTreeMap::new();
    for p in &procs {
        let e = groups.entry(p.program.clone()).or_insert((0, 0));
        e.0 += p.rss;
        e.1 += 1;
    }
    let mut by_program: Vec<(String, u64, usize)> =
        groups.into_iter().map(|(k, (b, n))| (k, b, n)).collect();
    by_program.sort_by_key(|g| std::cmp::Reverse(g.1));
    by_program.truncate(12);
    let orphans: Vec<ProcInfo> = procs
        .iter()
        .filter(|p| {
            p.parent == Some(1)
                && p.run_hours > ORPHAN_MIN_HOURS
                && ORPHAN_PATTERNS.iter().any(|pat| p.command.contains(pat))
        })
        .cloned()
        .collect();
    let automation_browsers: Vec<ProcInfo> = procs
        .iter()
        .filter(|p| is_automation_browser(&p.program, &p.command))
        .cloned()
        .collect();
    let total = sys.total_memory();
    let used = sys.used_memory();
    let swap_used = sys.used_swap();
    let pressure = swap_used > total / 8 || used > total * 9 / 10;
    let orphan_containers = if include_docker && crate::docker::available() {
        crate::docker::orphan_testcontainers(1).len()
    } else {
        0
    };
    MemReport {
        total,
        used,
        available: sys.available_memory(),
        swap_total: sys.total_swap(),
        swap_used,
        pressure,
        top: procs.into_iter().take(15).collect(),
        by_program,
        orphans,
        automation_browsers,
        orphan_containers,
    }
}

fn terminate(pids: impl IntoIterator<Item = u32>) -> Vec<u32> {
    let mut sys = System::new();
    sys.refresh_processes(ProcessesToUpdate::All, true);
    let mut killed = Vec::new();
    for pid in pids {
        if let Some(p) = sys.process(sysinfo::Pid::from_u32(pid)) {
            if p.kill_with(sysinfo::Signal::Term).unwrap_or(false) {
                killed.push(pid);
            }
        }
    }
    killed
}

/// Terminate orphaned helper processes. Returns the pids signalled.
pub fn kill_orphans() -> Vec<u32> {
    let rep = report(false);
    terminate(rep.orphans.into_iter().map(|p| p.pid))
}

/// Terminate automation browsers that have been running longer than `max_hours`.
/// A tool that still needs a browser simply launches a fresh one.
pub fn kill_automation_browsers(max_hours: f64) -> Vec<u32> {
    let rep = report(false);
    terminate(
        rep.automation_browsers
            .into_iter()
            .filter(|p| p.run_hours >= max_hours)
            .map(|p| p.pid),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn automation_browsers_are_told_from_the_users_browser() {
        assert!(is_automation_browser(
            "Google Chrome",
            "/Applications/Google Chrome.app/Contents/MacOS/Google Chrome --disable-field-trial-config --user-data-dir=/var/folders/x/playwright_chromiumdev_profile-abc"
        ));
        assert!(is_automation_browser(
            "chrome-headless-shell",
            "/x/chrome-headless-shell --headless --remote-debugging-pipe"
        ));
        assert!(!is_automation_browser(
            "Google Chrome",
            "/Applications/Google Chrome.app/Contents/MacOS/Google Chrome --no-startup-window"
        ));
        assert!(!is_automation_browser("Google Chrome", "/Applications/Google Chrome.app/.../Google Chrome Helper (Renderer) --type=renderer --enable-automation"));
    }

    #[test]
    fn program_names() {
        assert_eq!(
            program_name(
                "/Applications/Visual Studio Code.app/Contents/MacOS/Electron --type=x",
                "Electron"
            ),
            "Visual Studio Code"
        );
        assert_eq!(
            program_name("/usr/local/bin/node /x/mcp/server.mjs", "node"),
            "node server.mjs"
        );
        assert_eq!(program_name("/opt/bin/codex --flag", "codex"), "codex");
        assert_eq!(program_name("", "kernel_task"), "kernel_task");
    }
}
