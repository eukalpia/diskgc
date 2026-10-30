# diskgc architecture

diskgc keeps a developer machine's disk from filling up with things nobody
asked to keep: build artifacts, package caches, orphaned test containers,
leftovers of AI coding agents, stale temporary directories. It runs as a small
background daemon, decides how aggressively to act from the amount of free
space and its growth trend, and explains every action it takes.

## Components

```
┌───────────────────────────────┐        ┌──────────────────────────────┐
│ diskgc (Rust binary)          │  IPC   │ diskgc-ui (Dart + Cinder)    │
│  CLI  · daemon · IPC server   │◀──────▶│  dashboard, plan review,     │
│                               │ NDJSON │  history, memory, settings   │
│  ┌─────────────────────────┐  │        └──────────────────────────────┘
│  │ diskgc-core (library)   │  │
│  │ config · scan · rules   │  │
│  │ executor · forecast     │  │
│  │ history · mem · docker  │  │
│  │ platform (mac/lin/win)  │  │
│  └─────────────────────────┘  │
└───────────────────────────────┘
```

* **diskgc-core** is a library with no I/O policy of its own: it can be driven
  by the CLI, by the daemon, by the IPC server, or by tests. Everything that
  touches the OS differently per platform lives under `platform/`.
* **diskgc** is the binary. It hosts the CLI, the daemon loop and the IPC
  server. Every command has a `--json` form; the TUI never parses human text.
* **diskgc-ui** is a separate native executable built with Cinder. It talks to
  the running daemon over the IPC protocol (`docs/PROTOCOL.md`). If no daemon
  is running it can start one.

## Core concepts

**Rule** — a unit of knowledge about one kind of garbage (Rust `target/`,
`node_modules`, orphaned testcontainers, ...). A rule produces **Actions**; it
never deletes anything itself.

**Action** — one concrete step: remove a path, sweep files older than N days
inside a path, compress a file, run a command. Each action carries its
estimated size and a human-readable reason. Actions are the unit of user
review: the TUI lets you untick any of them before execution.

**Plan** — the list of actions for one run plus notes about things that were
deliberately left alone (for example a `target/` that is being written to right
now).

**Level** — `normal`, `low`, `critical`. The daemon picks the level from free
space and from the forecast. Lower levels shorten every "staleness" window
(30 days → 15 → 7) and lower size caps. The same rules run at every level;
only their parameters change, so behaviour stays predictable.

**Executor** — applies a plan, reports progress, and measures what was actually
freed (not what the plan estimated). Deletion is permanent by default; the
`delete_mode = "trash"` option moves to the system trash instead.

**Forecast** — a least-squares fit over recent free-space snapshots. If the
disk would cross a threshold within 24 hours at the current rate, the daemon
escalates ahead of time.

**History** — an append-only JSONL log of snapshots, category sizes and runs.
It feeds `diskgc stats`, the forecast and the "repeat offenders" table, which
points at root causes instead of symptoms.

## Safety model

* Only paths that can be regenerated are ever touched: build outputs, package
  caches, temporary directories, container leftovers, old application
  versions. Sources, `.git`, databases and chat histories are never candidates
  (histories may be *compressed*, never deleted, unless explicitly enabled).
* A build directory is deleted only if a marker of its project is present next
  to it (`Cargo.toml`, `package.json`, `pubspec.yaml`, ...), so a user folder
  that merely happens to be called `build` is safe.
* Staleness is judged on the whole project, not on the directory being
  removed, and a git worktree counts as its own project.
* `exclude` globs in the config are honoured by every rule.
* On macOS the scanner never enters privacy-protected locations (Music, Pictures, Mail, Safari,
  Messages, Apple's own caches and containers, …): nothing there is build output, and touching
  them from a background process triggers a system permission dialog.
* Every run is recorded with the reason for every action; `diskgc clean -n`
  prints the plan without executing it.

## Platform layer

| Concern              | macOS                     | Linux                      | Windows                       |
|----------------------|---------------------------|----------------------------|-------------------------------|
| Disk usage of a file | `st_blocks * 512`         | `st_blocks * 512`          | logical size                  |
| Hardlink dedupe      | dev+ino                   | dev+ino                    | not deduplicated              |
| Transparent compress | APFS via `ditto`          | not available              | NTFS via `compact`            |
| Autostart            | launchd agent (KeepAlive) | systemd user service       | Task Scheduler (ONLOGON)      |
| IPC transport        | Unix socket               | Unix socket                | TCP loopback + token file     |
| Processes / memory   | `sysinfo`                 | `sysinfo`                  | `sysinfo`                     |
| Chrome update clones | `$TMPDIR/../X/...`        | —                          | —                             |

## Repository layout

```
crates/diskgc-core/   library
crates/diskgc/        binary (CLI, daemon, IPC server)
ui/                   Dart + Cinder client
docs/                 ARCHITECTURE.md, PROTOCOL.md
legacy/               the original single-file prototype, kept for reference
```
