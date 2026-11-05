# diskgc

[![CI](https://github.com/eukalpia/diskgc/actions/workflows/ci.yml/badge.svg)](https://github.com/eukalpia/diskgc/actions/workflows/ci.yml)
[![License: Apache-2.0](https://img.shields.io/badge/license-Apache--2.0-blue.svg)](LICENSE)

**A background disk cleaner for developer machines that knows what it is deleting.**

Developer disks fill up with things nobody asked to keep: Cargo `target/`
directories that grow by 100 GB in two days, `node_modules` of branches you
abandoned in July, containers a killed test run left behind, application
versions an updater forgot to delete, scratch directories of AI coding agents.
macOS Storage Management calls all of it "System Data" and offers no help.

diskgc runs as a small daemon, watches free space and its trend, and removes
only what can be regenerated — with a written reason for every action.

```
$ diskgc plan

rust (3 actions, ~41.2 GB)
     32.3 GB  sweep >1d          ~/work/backend/target
              target is 105.0 GB (> 40.0 GB hard cap): everything older than a day
      8.9 GB  remove             ~/work/backend/target/debug/incremental
              target > 20 GB: incremental compilation cache

node (7 actions, ~15.1 GB)
      3.5 GB  remove             ~/work/app/.worktrees/admin-degree-transfer/node_modules
              project untouched for 30 days; restored by install

cache (13 actions, ~17.2 GB)
      1.4 GB  remove             .../X/com.google.Chrome.code_sign_clone/code_sign_clone.tTzACB
              Chrome code-signing clone left after an update

compress: 568 cold files (3.6 GB on disk now), transparently, in place

docker
  129 orphaned testcontainers; stopped > 24 h; build cache > 7 d; reclaimable ~24.1 GB

Estimated: 112.9 GB in 1011 actions
```

## What it cleans

| Rule | What | When |
|------|------|------|
| Rust targets | `target/` directories (found by content, wherever they are) | artifacts untouched for 7 days; above 20 GB also incremental caches and fat test binaries; above 40 GB everything older than a day |
| node_modules | `node_modules` next to a `package.json` | nothing in the project changed for 30 days |
| Build directories | `.dart_tool`, `build`, `_build`, `.next`, `.turbo`, `__pycache__`, `.gradle`, ... next to a project marker | same 30-day rule |
| Temporary directories | Claude Code per-session scratch, your own entries in the system temp dir, stale work directories in the per-user `$TMPDIR` (macOS) | silent for 3 days / 7 days / 24 h |
| AI tools | old Codex application versions, abandoned runtime installs | always (keeps current + 1) |
| Docker | orphaned testcontainers, stopped containers, dangling volumes and images, build cache | 2 h / 24 h / 7 d |
| Tool caches | Chrome update clones, VS Code update leftovers, Go / npm / uv caches over a cap, Homebrew downloads, Xcode DerivedData, simulator caches | per-rule caps |
| Cold files | session logs and JSONL histories of Codex and Claude Code | **compressed** by the filesystem (APFS, NTFS), never deleted |

Sources, `.git`, databases and chat histories are never candidates. A build
directory is only touched when its project's marker file sits next to it, so a
photo folder called `build` is safe. Activity is judged on the whole project
(a git worktree counts as its own project), not on the directory being removed.

## How it decides

The daemon wakes every 10 minutes and looks at free space:

| Free space | Behaviour |
|------------|-----------|
| above `comfortable_gb` (150) | scheduled hygiene every 6 hours |
| below `comfortable_gb` | clean now with normal windows |
| below `low_gb` (100) | *low* level: every staleness window halved |
| below `critical_gb` (60) | *critical* level: windows quartered, size caps halved |

A least-squares fit over the last six hours of snapshots predicts where free
space will be in 24 hours; if that crosses a threshold the daemon escalates
ahead of time. Every run is recorded, so `diskgc stats` shows growth per day,
per category, and the paths that keep coming back — the places to fix the cause
rather than the symptom.

## Install

Prebuilt binaries for macOS (Apple Silicon, Intel), Linux (x86_64) and Windows
(x86_64) are attached to every [release](https://github.com/eukalpia/diskgc/releases):
unpack `diskgc` (and optionally `diskgc-ui`) somewhere on your `PATH`.

Or build from source (Rust 1.80+):

```sh
cargo install --locked --git https://github.com/eukalpia/diskgc diskgc
```

Then look before you let it act:

```sh
diskgc config --edit      # set [scan] roots to where your projects live
diskgc plan               # what a clean would remove, and why — nothing is touched
diskgc install            # start the daemon at login (launchd / systemd --user / Task Scheduler)
diskgc status
```

## Safety

diskgc deletes files — that is its job — so it is built to be predictable:

- It only proposes things that a tool can regenerate (build output, dependency
  directories, caches, temp files) and gives the reason for each one; `plan` and
  `clean -n` show exactly that list without touching anything.
- Sources, `.git`, databases and chat histories are never candidates; anything
  matching `[scan] exclude` is never touched; the executor refuses any path
  outside `[safety] allowed_prefixes` (by default your home directory and the
  system temp and cache folders).
- `[safety] delete_mode = "trash"` moves things to the trash instead of deleting
  them permanently (when the trash is on another volume it falls back to deletion).
- Every run is logged (`diskgc history`), with what was planned and what was
  actually freed.

It still runs unattended with your permissions, so read the first `plan` on your
machine before installing the daemon. Bug reports with the `plan` output are welcome.

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
diskgc ui                terminal dashboard (diskgc-ui, built with Cinder)
```

Every command accepts `--json`.

## Dashboard

`diskgc ui` opens a terminal dashboard built with [Cinder](https://github.com/eukalpia/cinder),
talking to the daemon over the IPC protocol. Six screens: Overview (free space,
level, forecast, where the space goes), Plan (every proposed action with its
reason — untick what should stay, execute the rest, or dry-run), Report (biggest
offenders per category), History (runs, growth per day, repeat offenders),
Memory (RAM by program, orphaned helper processes), Log (live daemon activity).

```
1-6 / tab   switch screens        r   refresh the current screen
p           build a plan          space / enter   toggle the highlighted action
x           execute selected      n   dry run             a   toggle all
k           terminate orphaned helper processes (Memory)   q   quit
```

The dashboard is a separate native executable (`diskgc-ui`); `diskgc ui` finds
it next to `diskgc` or on `PATH` and starts the daemon if none is running.

## Configuration

`~/.config/diskgc/config.toml` (Linux/macOS) or `%APPDATA%\diskgc\config.toml`
(Windows), created with defaults on first run and re-read by the daemon on every
tick. See [docs/CONFIG.md](docs/CONFIG.md) for every key. The essentials:

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

Transparent compression is available on APFS and NTFS.
See [docs/ARCHITECTURE.md](docs/ARCHITECTURE.md) for the platform matrix and
[docs/PROTOCOL.md](docs/PROTOCOL.md) for the IPC API the dashboard uses.

## Building

```sh
cargo build --release              # crates/diskgc, crates/diskgc-core
cargo test
cd ui && dart pub get && dart compile exe bin/diskgc_ui.dart -o build/diskgc-ui
cp build/diskgc-ui ~/.local/bin/                            # next to diskgc
```

`dart test` and `dart analyze` in `ui/` cover the client and models; the Rust
side has unit tests per module, rule tests against synthetic project trees and
an IPC round-trip test (`cargo test`).

## Contributing

Issues and pull requests are welcome — see [CONTRIBUTING.md](CONTRIBUTING.md).
A new rule needs a test against a synthetic directory tree (`crates/diskgc-core/tests/rules.rs`)
showing both what it removes and what it must leave alone.

## License

[Apache-2.0](LICENSE).
