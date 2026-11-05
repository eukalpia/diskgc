"""Two weeks of plausible diskgc history (snapshots, category sizes, runs) for the demo."""
import json, math, random, sys
from datetime import datetime, timedelta

random.seed(7)
GB = 10**9
TOTAL = int(sys.argv[1]) if len(sys.argv) > 2 else 500 * GB
REAL_AVAIL = int(sys.argv[2]) if len(sys.argv) > 2 else 170 * GB
now = datetime.now().replace(minute=0, second=0, microsecond=0)
fmt = lambda t: t.strftime("%Y-%m-%d %H:%M:%S")
used = 330 * GB
out = []
paths = [
    ("~/work/payments-api/target/debug/incremental", "rust", "target > 20 GB: incremental compilation cache"),
    ("~/.cache/go-build", "cache", "Go build cache is 14.2 GB (cap 2 GB)"),
    ("~/work/storefront-web/node_modules", "node", "project untouched for 30 days; restored by install"),
    ("/tmp/claude-0/-home-dev-api/4c1d", "scratch", "Claude Code session silent for 3 days"),
]
for h in range(14 * 24, 0, -1):
    t = now - timedelta(hours=h)
    busy = 9 <= t.hour <= 23
    used += int((random.uniform(0.2, 1.6) if busy else random.uniform(0, 0.3)) * GB)
    level = "normal" if TOTAL - used > 100 * GB else "low"
    out.append({"kind": "snapshot", "ts": fmt(t), "used": used, "available": TOTAL - used, "level": level})
    if h % 6 == 0 or TOTAL - used < 140 * GB:
        freed = int(min(used - 300 * GB, random.uniform(6, 28) * GB)) if used > 310 * GB else 0
        if freed <= 0:
            continue
        used -= freed
        top, left = [], freed
        for p, g, r in random.sample(paths, 3):
            b = int(left * random.uniform(0.35, 0.6)); left -= b
            top.append({"path": p.replace("~", "/home/dev"), "bytes": b, "reason": r, "group": g, "freed": b})
        out.append({"kind": "run", "ts": fmt(t + timedelta(minutes=2)), "level": level, "dry_run": False,
                    "freed": freed, "actions": random.randint(8, 60), "available_after": TOTAL - used,
                    "duration_secs": random.uniform(4, 30), "top": top})
    if h % 24 == 0:
        day = 14 * 24 - h
        out.append({"kind": "categories", "ts": fmt(t), "sizes": {
            "rust_targets": int((18 + 0.9 * day / 24) * GB), "node_modules": int(9.5 * GB),
            "library_caches": int((6 + math.sin(day) * 2) * GB), "docker": int(12 * GB),
            "claude_scratch": int(1.8 * GB), "toolchain_caches": int(7.4 * GB)}})
# Shift the series so it ends at the real free space.
shift = (TOTAL - REAL_AVAIL) - used
for r in out:
    if r["kind"] == "snapshot":
        r["used"] = max(0, r["used"] + shift); r["available"] = TOTAL - r["used"]
        r["level"] = "normal" if r["available"] > 100 * GB else "low" if r["available"] > 60 * GB else "critical"
    elif r["kind"] == "run":
        r["available_after"] = min(TOTAL, max(0, r["available_after"] - shift))
        r["level"] = "normal"
    print(json.dumps(r))
