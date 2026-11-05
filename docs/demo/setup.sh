#!/usr/bin/env bash
# Builds a synthetic developer home for the demo: stale build output, dependency
# directories, agent scratch space and old app versions, plus two weeks of history.
# Every byte is allocated for real (fallocate), so sizes on screen are real too.
set -euo pipefail
H="$HOME"

blob() { # path size [days old]
  mkdir -p "$(dirname "$1")"
  fallocate -l "$2" "$1"
  if [ -n "${3:-}" ]; then touch -d "$3 days ago" "$1"; fi
}
age() { # days dir...
  local d=$1; shift
  find "$@" -exec touch -h -d "$d days ago" {} +
}

# A Rust service nobody built for two weeks.
P=$H/work/payments-api
mkdir -p "$P/src"; echo '[package]' > "$P/Cargo.toml"; echo 'fn main() {}' > "$P/src/main.rs"
mkdir -p "$P/target"; echo '{}' > "$P/target/.rustc_info.json"
for i in $(seq 1 6); do blob "$P/target/debug/deps/libpayments_core-$i.rlib" 140M; done
for i in $(seq 1 4); do blob "$P/target/debug/incremental/payments_api-$i/s-dep-graph.bin" 120M; done
blob "$P/target/release/payments-api" 90M
age 16 "$P"

# A web app on a branch abandoned in summer.
W=$H/work/storefront-web
mkdir -p "$W"; echo '{"name":"storefront-web"}' > "$W/package.json"
for p in next react-dom @swc/core typescript esbuild @img/sharp; do blob "$W/node_modules/$p/dist/index.js" 85M; done
blob "$W/.next/cache/webpack/client-production/0.pack" 260M
blob "$W/.turbo/cache/a1b2c3.tar.zst" 120M
age 45 "$W"

# A Flutter app and its build output.
F=$H/src/field-app
mkdir -p "$F/lib"; echo 'name: field_app' > "$F/pubspec.yaml"
blob "$F/build/app/outputs/apk/debug/app-debug.apk" 180M
blob "$F/.dart_tool/flutter_build/kernel_snapshot.dill" 95M
age 38 "$F"

# Active projects: built today, so they stay.
A=$H/work/billing-service
mkdir -p "$A/src"; echo '[package]' > "$A/Cargo.toml"; mkdir -p "$A/target"; echo '{}' > "$A/target/.rustc_info.json"
blob "$A/target/debug/deps/libbilling-1.rlib" 150M
D=$H/src/docs-site
mkdir -p "$D"; echo '{"name":"docs-site"}' > "$D/package.json"
blob "$D/node_modules/astro/dist/index.js" 120M

# Scratch space of AI coding agents, silent for days.
S=/tmp/claude-$(id -u)
for s in 3f2a9c1e 8b7d4e20; do blob "$S/-home-dev-api/$s/scratchpad/bundle.tar" 210M; done
age 5 "$S"
blob /tmp/vite-ssr-bundle-1f9e/server.mjs 140M 9
blob /tmp/playwright-artifacts-x7Kq/trace.zip 110M 8

# Old Codex app versions next to the current one.
R=$H/.codex/packages/standalone/releases
for v in 0.63.0 0.64.1 0.65.0; do blob "$R/$v/codex" 230M; done
age 20 "$R/0.63.0"; age 12 "$R/0.64.1"
ln -sfn "$R/0.65.0" "$H/.codex/packages/standalone/current"

mkdir -p "$H/.local/share/diskgc" "$H/.config/diskgc"
cat > "$H/.config/diskgc/config.toml" <<TOML
[scan]
roots = ["~/work", "~/src"]
extra_target_roots = []
TOML
# History ends at the container's real disk numbers, so the forecast lines up.
read -r total avail < <(df -B1 --output=size,avail "$H" | tail -1)
python3 /usr/local/share/diskgc-demo/history.py "$total" "$avail" > "$H/.local/share/diskgc/history.jsonl"
