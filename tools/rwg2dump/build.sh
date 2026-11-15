#!/usr/bin/env bash
# Builds rwg2dump-<ver>.jar (server-side Forge 1.7.10 mod). Only needs a JDK: the FML annotation/event
# classes are compile-only stubs and are NOT packaged; everything Minecraft/RWG2 is resolved via reflection.
set -euo pipefail
HERE="$(cd "$(dirname "$0")" && pwd)"
VER="$(grep -o 'VERSION = "[^"]*"' "$HERE/src/rwg2dump/Rwg2Dump.java" | cut -d'"' -f2)"
TMP="$(mktemp -d)"; trap 'rm -rf "$TMP"' EXIT
mkdir -p "$TMP/stubs" "$TMP/out"
javac -Xlint:-options --release 8 -d "$TMP/stubs" $(find "$HERE/stubs" -name '*.java')
javac -Xlint:-options --release 8 -cp "$TMP/stubs" -d "$TMP/out" $(find "$HERE/src" -name '*.java')
cp "$HERE"/resources/* "$TMP/out/"
mkdir -p "$HERE/dist"
(cd "$TMP/out" && jar --create --file "$HERE/dist/rwg2dump-$VER.jar" .)
echo "$HERE/dist/rwg2dump-$VER.jar"
