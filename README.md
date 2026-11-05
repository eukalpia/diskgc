<div align="center">

<picture>
  <source media="(prefers-color-scheme: dark)" srcset="docs/assets/banner-dark.svg">
  <img alt="diskgc — a background disk cleaner for developer machines" src="docs/assets/banner-light.svg" width="100%">
</picture>

**A background disk cleaner for developer machines that knows what it is deleting.**

[![CI](https://github.com/eukalpia/diskgc/actions/workflows/ci.yml/badge.svg)](https://github.com/eukalpia/diskgc/actions/workflows/ci.yml)
[![Release](https://img.shields.io/github/v/release/eukalpia/diskgc?display_name=tag&sort=semver)](https://github.com/eukalpia/diskgc/releases)
[![License: Apache-2.0](https://img.shields.io/badge/license-Apache--2.0-blue.svg)](LICENSE)
![Platforms](https://img.shields.io/badge/platforms-macOS%20%7C%20Linux%20%7C%20Windows-555)
![Rust 1.80+](https://img.shields.io/badge/rust-1.80%2B-orange.svg)

**English** · [Русский](docs/i18n/README.ru.md) · [O‘zbekcha](docs/i18n/README.uz.md) · [简体中文](docs/i18n/README.zh-CN.md) · [Español](docs/i18n/README.es.md) · [हिन्दी](docs/i18n/README.hi.md) · [العربية](docs/i18n/README.ar.md) · [Français](docs/i18n/README.fr.md) · [Português](docs/i18n/README.pt-BR.md) · [日本語](docs/i18n/README.ja.md)

<img alt="The diskgc terminal dashboard: overview, a plan with a reason for every action, report and history" src="docs/assets/dashboard.gif" width="100%">

</div>

## Why

Developer disks fill up with things nobody asked to keep: Cargo `target/`
directories that grow by 100 GB in two days, `node_modules` of branches you
abandoned in July, Go build caches refilled by every test run, containers a
killed test left behind, temp directories of AI coding agents. macOS calls all
of it "System Data" and offers no help; `du` tells you *where*, not *what is
safe to delete*.

diskgc runs as a small daemon, watches free space and its trend, and removes
only what a tool can regenerate — with a written reason for every action, a
plan you can read before anything happens, and a history of what it freed.

## Highlights

- **Explains itself.** Every action comes with a reason: *"target is 105 GB
  (> 40 GB cap): everything older than a day"*, *"project untouched for 30 days;
  restored by install"*.
- **Knows projects, not just folders.** `target/` is found by its content,
  build directories only next to their project marker, and activity is judged
  on the whole project (a git worktree counts as its own).
- **Acts before the disk is full.** A forecast over the last six hours
  escalates cleaning ahead of time; thresholds tighten from *normal* to *low*
  to *critical*.
- **Never touches what matters.** Sources, `.git`, databases and chat histories
  are never candidates; excluded globs are never touched; logs are compressed
  in place, not deleted.
- **Finds the cause.** `diskgc stats` shows growth per day and per category and
  the paths that keep coming back — the places to fix rather than clean.
- **Terminal dashboard and JSON.** A six-screen TUI, and `--json` on every
  command for scripts.

## Quick start

```sh
cargo install --locked --git https://github.com/eukalpia/diskgc diskgc
diskgc config --edit      # set [scan] roots to where your projects live
diskgc plan               # what a clean would remove, and why — nothing is touched
diskgc install            # start the daemon at login (launchd / systemd --user / Task Scheduler)
diskgc status
```

Prebuilt binaries for macOS (Apple Silicon, Intel), Linux (x86_64) and Windows
(x86_64) are attached to each [release](https://github.com/eukalpia/diskgc/releases):
unpack `diskgc` (and optionally `diskgc-ui`) somewhere on your `PATH`.

<div align="center">
<img alt="diskgc plan in a terminal: grouped actions with sizes and reasons" src="docs/assets/plan.gif" width="100%">
</div>

## What it cleans

| Rule | What | When |
|------|------|------|
| Rust targets | `target/` directories (found by content, wherever they are) | artifacts untouched for 7 days; above 20 GB also incremental caches and fat test binaries; above 40 GB everything older than a day; a 40 GB budget for all targets together |
| node_modules | `node_modules` next to a `package.json` | nothing in the project changed for 30 days |
| Build directories | `.dart_tool`, `build`, `_build`, `.next`, `.turbo`, `__pycache__`, `.gradle`, … next to a project marker | the same 30-day rule |
| Temporary directories | Claude Code per-session scratch, your own entries in the system temp directory, stale work directories in the per-user `$TMPDIR` (macOS) | silent for 3 days / 7 days / 24 h |
| AI tools | old Codex application versions, abandoned runtime installs | always (keeps current + 1) |
| Docker | orphaned testcontainers, stopped containers, dangling volumes and images, build cache | 2 h / 24 h / 7 d |
| Tool caches | Go, npm, uv, pnpm and sccache caches over a cap, Chrome and VS Code update leftovers, Homebrew downloads, Xcode DerivedData, simulator and Metal shader caches | per-rule caps |
| Cold files | session logs and JSONL histories of Codex and Claude Code | **compressed** by the filesystem (APFS, NTFS), never deleted |

## How it decides

The daemon wakes every 10 minutes and looks at free space:

| Free space | Behaviour |
|------------|-----------|
| above `comfortable_gb` (150) | scheduled hygiene every 6 hours |
| below `comfortable_gb` | clean now with the normal windows |
| below `low_gb` (100) | *low* level: every staleness window halved |
| below `critical_gb` (60) | *critical* level: windows quartered, size caps halved |

A least-squares fit over the last six hours of snapshots predicts free space in
24 hours; if that crosses a threshold, the daemon escalates ahead of time. Every
run is recorded, so `diskgc stats` shows growth per day, per category, and the
repeat offenders.

## Safety

diskgc deletes files — that is its job — so it is built to be predictable:

- It only proposes what a tool can regenerate (build output, dependency
  directories, caches, temp files) and gives the reason for each action;
  `plan` and `clean -n` show exactly that list without touching anything.
- Sources, `.git`, databases and chat histories are never candidates; anything
  matching `[scan] exclude` is never touched; the executor refuses any path
  outside `[safety] allowed_prefixes` (by default your home directory and the
  system temp and cache folders).
- `[safety] delete_mode = "trash"` moves things to the trash instead of deleting
  them permanently (macOS and Linux; when the trash is on another volume it
  falls back to deletion).
- Every run is logged (`diskgc history`) with what was planned and what was
  actually freed.

It still runs unattended with your permissions, so read the first `plan` on your
machine before installing the daemon.

## Terminal dashboard

`diskgc ui` opens a dashboard built with [Cinder](https://github.com/eukalpia/cinder)
that talks to the daemon over its [IPC protocol](docs/PROTOCOL.md):

| Screen | |
|--------|---|
| **Overview** | free space, level, forecast, where the space goes |
| **Plan** | every proposed action with its reason — untick what should stay, execute the rest, or dry-run |
| **Report** | the biggest offenders per category |
| **History** | runs, growth per day, repeat offenders |
| **Memory** | RAM by program, orphaned helper processes |
| **Log** | live daemon activity |

```
1-6 / tab   switch screens        r   refresh the current screen
p           build a plan          space / enter   toggle the highlighted action
x           execute selected      n   dry run             a   toggle all
k           terminate orphaned helper processes (Memory)   q   quit
```

The dashboard is a separate native executable (`diskgc-ui`); `diskgc ui` finds
it next to `diskgc` or on `PATH` and starts the daemon if none is running.

## Commands

```
diskgc report            what occupies the disk: categories and the biggest offenders
diskgc plan              what a clean would do, with reasons — nothing is touched
diskgc clean [-n]        clean now (-n = plan only)
diskgc status            daemon state, free space, forecast, last run
diskgc stats             growth per day, category trends, repeat offenders
diskgc history           recent runs and what they removed
diskgc mem               RAM by process, orphaned helper processes (--kill-orphans)
diskgc install | uninstall
diskgc config [--edit|--show]
diskgc ui                terminal dashboard
```

Every command accepts `--json`.

## Configuration

`~/.config/diskgc/config.toml` (Linux, macOS) or `%APPDATA%\diskgc\config.toml`
(Windows), created with defaults on first run and re-read by the daemon on every
tick. Every key is described in [docs/CONFIG.md](docs/CONFIG.md). The essentials:

```toml
[scan]
roots = ["~/work", "~/src"]          # where your projects are
exclude = ["~/work/keep-forever/**"] # globs no rule may touch

[thresholds]
comfortable_gb = 150
low_gb = 100
critical_gb = 60

[safety]
delete_mode = "permanent"            # or "trash"
```

## Platforms

| Platform | Status |
|----------|--------|
| macOS (APFS) | used daily; the main target |
| Linux | tested in CI and in containers; `systemd --user` autostart |
| Windows | builds and passes tests in CI; autostart via Task Scheduler is the least exercised part — reports welcome |

Transparent compression is available on APFS and NTFS. See
[docs/ARCHITECTURE.md](docs/ARCHITECTURE.md) for the platform matrix.

## Building from source

```sh
cargo build --release
cargo test
cd ui && dart pub get && dart compile exe bin/diskgc_ui.dart -o build/diskgc-ui
```

The recordings in this README are reproducible from a synthetic home directory:
see [docs/demo](docs/demo).

## Contributing

Issues and pull requests are welcome — see [CONTRIBUTING.md](CONTRIBUTING.md).
A new rule needs a test against a synthetic directory tree showing both what it
removes and what it must leave alone.

## License

[Apache-2.0](LICENSE).
