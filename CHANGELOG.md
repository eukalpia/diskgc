# Changelog

## 0.2.0 — unreleased

First version structured for other people's machines.

- Workspace split into `diskgc-core` (library) and `diskgc` (CLI, daemon, IPC server).
- Platform layer for macOS, Linux and Windows: disk usage, transparent compression
  (APFS via `ditto`, NTFS via `compact`), autostart (launchd, `systemd --user`,
  Task Scheduler), process and memory information via `sysinfo`.
- Rules produce explained actions; the executor measures what was actually freed,
  refuses paths outside the allowed prefixes, and supports `delete_mode = "trash"`.
- Cargo `target/` directories are found by content (`.rustc_info.json`, `CACHEDIR.TAG`),
  including ad-hoc `CARGO_TARGET_DIR`s in temp folders; hard cap at twice the limit.
- Project activity is judged at the repository/workspace root; git worktrees are
  separate projects.
- Growth forecast (least squares over 6 h) with predictive escalation.
- JSON output for every command; NDJSON IPC protocol for the dashboard.
- Configuration validated on load (`deny_unknown_fields`, threshold ordering).
- Test suite: unit tests per module, rule tests against synthetic project trees,
  IPC round-trip test.
- `diskgc-ui`: terminal dashboard in Dart + Cinder (Overview, Plan with per-action
  selection and dry run, Report, History, Memory, Log), started with `diskgc ui`.
- `diskgc mem` lists automation browsers (Playwright, headless shells); `--kill-browsers` and the
  `[mem] automation_browser_max_hours` daemon rule terminate stale ones.
- External cleanup commands time out after 90 s, and a command that freed nothing is put on a
  7-day cooldown (`uv cache clean` used to block a run for 5 minutes on another process's lock).
- The scanner skips macOS privacy-protected locations (Apple caches such as `com.apple.Music`,
  Mail, Safari, Messages, Music, Pictures, …), so the daemon never triggers a TCC dialog.
- Desktop notifications are off by default on macOS (they trigger a permission dialog).
- macOS defaults no longer scan `~/Documents` (privacy-protected; each rebuild re-triggered the dialog).
- Engine behind a read/write lock with a separate run lock: `status`, `stats`
  and `memory` answer instantly while a clean is in progress.

## 0.1.0 — 2026-09-05

Single-file prototype (kept in `legacy/`): report, clean, daemon, stats, mem,
launchd install. Freed 191 GB on the machine it was written on.
