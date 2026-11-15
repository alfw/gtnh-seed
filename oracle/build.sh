#!/usr/bin/env bash
# Builds the golden-vector oracle against verbatim RWG2 sources and writes testdata/golden/.
# Usage: oracle/build.sh <path-to-Realistic-World-Gen-2 checkout>
set -euo pipefail
HERE="$(cd "$(dirname "$0")" && pwd)"
RWG2="${1:?path to Realistic-World-Gen-2 checkout}"
PIN="$(cat "$HERE/RWG2_COMMIT")"
HEAD="$(git -C "$RWG2" rev-parse HEAD)"
[[ "$HEAD" == "$PIN" ]] || echo "WARN: RWG2 at $HEAD, oracle pinned to $PIN" >&2

UTIL="$RWG2/src/main/java/rwg2/util"
# Only standalone classes are compiled verbatim; anything touching Forge/MC gets a shim in oracle/shims/.
SRCS=(
  "$UTIL/NoiseGenerator.java"
  "$UTIL/PerlinNoise.java"
  "$UTIL/CellNoise.java"
  "$UTIL/PoissonPointNoise.java"
  "$UTIL/IslandPointNoise.java"
  "$UTIL/ContinentLandmarkNoise.java"
  "$UTIL/ContinentalNoise.java"
)
OUT="$(mktemp -d)"
javac -d "$OUT" --release 8 "${SRCS[@]}" $(find "$HERE/src" "$HERE/shims" -name '*.java') 2>&1 | grep -v "bootstrap class path\|^1 warning\|^warning: \[options\]" || true
mkdir -p "$HERE/../testdata/golden"
java -cp "$OUT" oracle.Golden "$HERE/../testdata/golden"
rm -rf "$OUT"
ls -la "$HERE/../testdata/golden"
