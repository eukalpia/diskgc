# Configuration reference

File: `~/.config/diskgc/config.toml` (Linux, macOS) or `%APPDATA%\diskgc\config.toml`
(Windows). Override the location with `--config FILE` or `DISKGC_CONFIG_DIR`.
The daemon re-reads the file on every tick; an invalid file is reported and the
previous configuration stays in effect. Unknown keys are errors, so typos are
caught instead of silently ignored.

Paths accept `~` and environment variables (`$HOME`, `${HOME}`, `%APPDATA%`).

## `[scan]`

| key | default | meaning |
|-----|---------|---------|
| `roots` | `["~/Projects", "~/Developer", "~/dev", "~/src", "~/code", "~/work"]` (plus `~/Documents` outside macOS) | where projects live; only existing directories are used. On macOS, `~/Documents`, `~/Desktop` and `~/Downloads` are privacy-protected: add them only after granting the daemon Full Disk Access (System Settings → Privacy & Security), otherwise macOS shows a permission dialog on every run. |
| `extra_target_roots` | `["/tmp"]` (`%TEMP%` on Windows) | additional places searched for Cargo `target/` directories |
| `depth` | `7` | directory depth searched below each root |
| `exclude` | `[]` | glob patterns matched against full paths; a pattern without `*` also matches everything below it |

## `[thresholds]` (GB of free space)

| key | default |
|-----|---------|
| `comfortable_gb` | `150` — above this only scheduled hygiene runs |
| `low_gb` | `100` — below this the *low* level halves every staleness window |
| `critical_gb` | `60` — below this the *critical* level quarters windows and halves size caps |

Must satisfy `critical_gb <= low_gb <= comfortable_gb`.

## `[daemon]`

| key | default |
|-----|---------|
| `tick_minutes` | `10` |
| `hygiene_every_hours` | `6` |
| `notify` | `true` on Linux/Windows, `false` on macOS — desktop notification after a run that freed more than 512 MB. On macOS the first notification from a command-line tool triggers a permission dialog, and the grant is lost on every rebuild of an unsigned binary; enable it only for a signed build. |

## `[safety]`

| key | default |
|-----|---------|
| `delete_mode` | `"permanent"` or `"trash"` (move to the system trash; not available on Windows yet) |
| `allowed_prefixes` | `[]` = home directory and temp directories. Nothing outside these prefixes is ever removed. |

## `[rust]`

| key | default |
|-----|---------|
| `sweep_days` | `7` — artifacts untouched this long are deleted |
| `target_max_gb` | `20` — above this incremental caches and fat test binaries go; if still above and idle for `idle_hours`, the whole directory |
| `idle_hours` | `12` |

Above `2 × target_max_gb` everything older than a day is swept even while a
build is running, so a permanently building agent cannot grow a target without
bound.

## `[node]`

| key | default |
|-----|---------|
| `stale_days` | `30` — `node_modules` is removed when nothing in the project changed for this long |

## `[build_dirs]`

| key | default |
|-----|---------|
| `names` | `[".dart_tool", "build", "_build", ".next", ".turbo", ".nuxt", ".svelte-kit", ".parcel-cache", "__pycache__", ".pytest_cache", ".mypy_cache", ".gradle", "cmake-build-debug", "cmake-build-release"]` |
| `stale_days` | `30` |

A directory is a candidate only when a project marker (`Cargo.toml`,
`package.json`, `pubspec.yaml`, `mix.exs`, `pyproject.toml`, `go.mod`,
`CMakeLists.txt`, `build.gradle`, `Makefile`, ...) sits next to it.

## `[scratch]`

| key | default |
|-----|---------|
| `claude_stale_days` | `3` — Claude Code per-session scratch directories |
| `tmp_stale_days` | `7` — your own entries in the system temp directory |

## `[ai_tools]`

| key | default |
|-----|---------|
| `codex_keep_releases` | `1` — old Codex application versions kept besides the current one |
| `codex_sessions_delete_days` | `0` — `0` never deletes session logs (they are compressed instead) |
| `claude_sessions_delete_days` | `0` |

## `[docker]`

| key | default |
|-----|---------|
| `enable` | `true` |
| `testcontainers_max_hours` | `2` — containers labelled by testcontainers older than this are orphans |
| `stopped_hours` | `24` |
| `build_cache_days` | `7` |
| `image_prune_all` | `false` — `true` also removes images not used by any container |

## `[caches]`

| key | default |
|-----|---------|
| `chrome_clone_hours` | `6` (macOS) |
| `go_cache_max_gb` | `2` |
| `npm_cache_max_gb` | `1` |
| `uv_cache_max_gb` | `1` |
| `brew_prune_days` | `7` |
| `xcode_derived_days` | `14` |
| `vscode_shipit` | `true` — VS Code update leftovers (macOS) |

## `[compress]`

| key | default |
|-----|---------|
| `enable` | `true` — transparent filesystem compression (APFS, NTFS); no-op elsewhere |
| `min_age_days` | `7` |
| `min_size_kb` | `64` |
| `dirs` | `["~/.codex/sessions", "~/.claude/projects", "~/.codex/logs"]` |
| `extensions` | `["jsonl", "log", "json", "txt", "md"]` |

Files the filesystem judges incompressible are marked and not retried.

## `[mem]`

| key | default |
|-----|---------|
| `automation_browser_max_hours` | `0` — browsers launched by automation tools (Playwright, Puppeteer, headless shells) running longer than this are terminated by the daemon; `0` = only shown in `diskgc mem`. They keep a live parent process, so the orphan rule never catches them, yet each holds 1–2 GB of RAM. |

## Environment variables

| variable | effect |
|----------|--------|
| `DISKGC_CONFIG_DIR` | config directory |
| `DISKGC_DATA_DIR` | history location |
| `DISKGC_LOG_FILE` | log file |
| `DISKGC_RUNTIME_DIR` | IPC endpoint directory |
| `DISKGC_VOLUME` | the volume whose free space is watched |
