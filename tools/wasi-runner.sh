#!/usr/bin/env sh
# cargo target runner for wasm32-wasip1. Prefers wasmtime (`cargo install wasmtime-cli --locked`); falls back to
# Node's WASI, which segfaults intermittently after memory.grow in some sandboxes (see CLAUDE.md).
ROOT="$(cd "$(dirname "$0")/.." && pwd)"
WASM="$1"; shift
if command -v wasmtime >/dev/null 2>&1; then
  exec wasmtime run --dir "$ROOT::/" \
    --env RWG2_GOLDEN_DIR=/testdata/golden \
    --env RWG2_DUMP_DIR=/testdata/pack/gtnh-2.9.0/dump \
    --env RWG2_DATA_DIR=/data \
    "$WASM" "$@"
fi
exec node --no-warnings "$(dirname "$0")/run-wasi.mjs" "$WASM" "$@"
