#!/bin/sh
# GPL-3.0 Corresponding Source for the published viewer: everything needed to rebuild the wasm and the site
# (Rust crates + tests, embedded pack data, viewer sources), packed into viewer/public/source/ so every build ships it.
# Deterministic (sorted names, fixed mtimes, no extra attributes) so unchanged sources give an identical file.
# Zip for self-hosted builds; the claude.ai artifact gets the browsable pages (tools/source-pages.py).
set -e
cd "$(dirname "$0")/.."
root=$(pwd)
out="$root/viewer/public/source/gtnh-seedmap-src.zip"
stage=$(mktemp -d)
trap 'rm -rf "$stage"' EXIT
tar --exclude='viewer/src/lib/rwg2.wasm' -cf - LICENSE README.md Cargo.toml Cargo.lock \
  crates/rwg2-biomes/Cargo.toml crates/rwg2-biomes/src crates/rwg2-biomes/tests \
  crates/rwg2-wasm/Cargo.toml crates/rwg2-wasm/src crates/rwg2-wasm/tests \
  data/gtnh-2.9.0 \
  viewer/index.html viewer/package.json viewer/package-lock.json viewer/vite.config.js viewer/src \
  viewer/public/about.html viewer/public/favicon.svg THIRD_PARTY_LICENSES.txt tools/third-party-licenses.sh tools/og-image.mjs \
  tools/source-bundle.sh tools/source-pages.py | tar -xf - -C "$stage"
find "$stage" -exec touch -d '2026-01-01 00:00:00 UTC' {} +
mkdir -p "$(dirname "$out")"
rm -f "$out" "$root"/viewer/public/source/*.tar.gz
(cd "$stage" && find . -type f | LC_ALL=C sort | TZ=UTC zip -X -9 -q "$out" -@)
ls -l "$out"
# browsable copy (the artifact host serves neither archives nor downloads): source/index.html + files/<path>.txt
python3 "$root/tools/source-pages.py" "$stage" "$root/viewer/public/source"
