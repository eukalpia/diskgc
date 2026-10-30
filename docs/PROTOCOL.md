# diskgc IPC protocol (v1)

The daemon exposes a small request/response API with server-pushed events. The
Cinder UI, `diskgc ui` and any script can use it.

## Transport

| Platform | Endpoint | Authentication |
|----------|----------|----------------|
| macOS, Linux | Unix domain socket at `diskgc ipc_endpoint` (`$XDG_RUNTIME_DIR/diskgc/diskgc.sock`, else `~/.local/share/diskgc/diskgc.sock`) | socket file is mode `0600` |
| Windows | TCP `127.0.0.1:<port>`; `%LOCALAPPDATA%\diskgc\diskgc.ipc` holds `port\ntoken\n` | client sends the token as its first line |

Messages are UTF-8 JSON objects, one per line (`\n`). The client sends
requests; the server answers each with exactly one response carrying the same
`id`. After `subscribe`, the server also pushes events (objects with an `event`
field and no `id`) on the same connection, interleaved with responses.

## Requests

```json
{"id": 1, "method": "status"}
{"id": 2, "method": "plan", "params": {"level": "low"}}
{"id": 3, "method": "execute", "params": {"action_ids": [1, 4, 9], "dry_run": false}}
```

| method | params | returns |
|--------|--------|---------|
| `hello` | — | `{protocol, daemon_version, pid}` |
| `ping` | — | `"pong"` |
| `status` | — | `Status` (disk, thresholds, autostart, forecast, last run, paths) |
| `report` | `{refresh?: bool}` | `Report` (a report younger than 15 min is reused unless `refresh` is true) (categories, top offenders, docker df). Emits `progress` events with `stage: scanning`. |
| `plan` | `{level?: "normal"\|"low"\|"critical"}` | `Plan`. The plan is cached on this connection for `execute`. Also emits `plan_ready`. |
| `execute` | `{action_ids?: [u32], dry_run: bool}` | `RunSummary`. Executes the cached plan (all actions when `action_ids` is omitted). Emits `progress` events. |
| `clean` | `{level?, dry_run: bool}` | `{plan, summary}` — plan and execute in one step. |
| `stats` | — | `Stats` |
| `history` | `{limit: usize}` | `[Record]` (runs only) |
| `memory` | — | `MemReport` |
| `kill_orphans` | — | `[pid]` |
| `get_config` | — | `{toml}` |
| `set_config` | `{toml}` | `"saved"` — validated before saving; on error the old config stays. |
| `subscribe` | — | `"subscribed"` — this connection now receives events. |
| `shutdown` | — | `"stopping"` |

## Responses

```json
{"id": 1, "result": "ok", "data": { ... }}
{"id": 3, "result": "error", "data": {"message": "no plan on this connection; call `plan` first"}}
```

## Events

```json
{"event": "progress", "data": {"stage": "scanning", "rule": "rust targets"}}
{"event": "progress", "data": {"stage": "executing", "index": 3, "total": 41, "action": { ... }}}
{"event": "progress", "data": {"stage": "executed", "result": {"id": 7, "freed": 1048576, "ok": true}}}
{"event": "snapshot", "data": {"used": 1, "available": 2, "level": "normal"}}
{"event": "log", "data": {"level": "info", "message": "run finished: 4.2 GB freed"}}
{"event": "run_finished", "data": { RunSummary }}
{"event": "plan_ready", "data": { Plan }}
```

## Shapes

`Action`

```json
{"id": 7, "group": "rust", "kind": "sweep_older_than", "days": 7,
 "path": "/Users/me/proj/target", "bytes": 3221225472,
 "reason": "build artifacts untouched for 7 days"}
```

`kind` is one of `remove`, `sweep_older_than` (with `days`), `sweep_big_binaries`,
`compress`, `command` (with `argv`, `measure`). `group` is one of `rust`,
`node`, `build`, `scratch`, `tmp`, `codex`, `claude`, `cache`, `compress`, `docker`.

`Plan`

```json
{"level": "normal", "created_at": "2026-09-05 15:00:00",
 "actions": [Action], "notes": ["kept /x/target (12.0 GB): a build touched it within the last 12 h"],
 "docker": {"available": true, "orphan_testcontainers": 3, "stopped_hours": 24,
            "build_cache_days": 7, "image_prune_all": false, "reclaimable_bytes": 0}}
```

`RunSummary`

```json
{"level": "normal", "dry_run": false, "started_at": "...", "duration_secs": 61.2,
 "action_count": 41, "freed_bytes": 1, "docker_freed_bytes": 2,
 "results": [{"id": 7, "freed": 1048576, "ok": true}],
 "docker_lines": ["docker: 40.7 GB -> 22.1 GB"], "free_after": 3}
```

All byte quantities are integers in bytes. Timestamps are local time,
`YYYY-MM-DD HH:MM:SS`. Fields may be added in later protocol versions; clients
should ignore unknown fields. Breaking changes bump `protocol` in `hello`.
