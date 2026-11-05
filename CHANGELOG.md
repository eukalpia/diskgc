# Changelog

## 0.3.0 — unreleased

Found on a machine where agents kept a dozen git worktrees, each with its own `target/`.

- Rust targets have a budget for all of them together (`[rust] total_max_gb`, 40 GB): above it,
  whole targets go, least recently built first, once idle. Eleven targets of 3–19 GB had stayed
  under the per-directory cap while taking 62 GB between them.
- The target of a linked worktree whose branch is merged into `main`/`master` goes once idle
  (`[rust] merged_idle_hours`); the plan suggests `git worktree remove` for the rest.
- New caches: sccache over `[caches] sccache_max_gb` loses entries unused for a week; Metal shader
  caches per application on macOS (`shader_cache_max_gb`) — Blender's had reached 4 GB.
- Docker build cache is really pruned: `docker builder prune` now runs with `-a` (without it only
  dangling records went, and 7 GB survived every run); the window shrinks with the level.
- Planning walks the project roots once instead of once per rule and per build-directory name
  (fourteen walks with the default names), and no longer descends into Cargo targets. Each target
  is read once instead of four or five times; a sweep is one pass instead of four. A dry run on
  the machine above went from 7–9 s to 1.2 s.
- Every external command has a deadline, `docker` and `go` included; output after the command
  exits is awaited for 5 s at most, so a background helper holding the pipe cannot hang a run.
- The daemon logs how long planning took and which rules were slow, names actions over 30 s, and
  a watchdog reports any step still running after 5 minutes. A run that stalled for hours used to
  leave nothing in the log. `clean -n` prints the planning time.
- `report` gains `sccache` and `system_app_caches` categories.
- A command that "freed nothing" no longer stays on its 7-day cooldown while its target keeps
  growing: it runs again once the target is a quarter (at least 1 GiB) bigger than it was. A Go
  build cache cleared while agents were building read as "freed nothing", sat out the week, and
  grew from 7 GB to 69 GB in a day.
- A failed action is recorded as "freed: unknown", not "freed nothing", so it cannot put its
  command on cooldown; the daemon logs every failed action with its error.
- The per-user `$TMPDIR` on macOS (`/var/folders/…/T`) is cleaned: entries untouched for
  `[scratch] user_tmp_stale_hours` (24) go, system folders (`com.apple.*`, `TemporaryItems`,
  `TemporaryDirectory.*`) stay. Go work directories, SSR bundles and Playwright profiles had grown
  it from 6 GB to 16 GB in a day. `report` shows it as `user_tmp`.

## 0.2.0

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
