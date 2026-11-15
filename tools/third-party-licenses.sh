#!/bin/sh
# THIRD_PARTY_LICENSES.txt: licence texts of the third-party code that ships in the viewer build
# (JS: three.js, Svelte runtime + clsx; Rust: the wasm's crates from Cargo.lock). Rerun after dependency updates.
set -e
cd "$(dirname "$0")/.."
out=THIRD_PARTY_LICENSES.txt
reg=$(ls -d ~/.cargo/registry/src/*/ | head -1)
{
  echo "Third-party software in the GTNH Seed Map viewer build"
  echo "(the project itself: GPL-3.0-or-later, see LICENSE)"
  for m in three svelte clsx; do
    v=$(node -p "require('./viewer/node_modules/$m/package.json').version")
    l=$(node -p "require('./viewer/node_modules/$m/package.json').license")
    f=$(ls viewer/node_modules/$m/ | grep -i -E '^licen[cs]e' | head -1)
    printf '\n==== %s %s (%s, npm) ====\n\n' "$m" "$v" "$l"; cat "viewer/node_modules/$m/$f"
  done
  # crates in the wasm build (dual-licensed crates: the MIT text is reproduced, the option this distribution uses)
  for c in itoa memchr serde serde_core serde_json zmij; do
    v=$(awk -v n="$c" '$0=="name = \""n"\""{getline; gsub(/version = |"/,""); print; exit}' Cargo.lock)
    d="$reg$c-$v"
    # not unpacked when the crate is locked but not compiled: same licence in the nearest unpacked version
    [ -d "$d" ] || d=$(ls -d "$reg$c"-[0-9]* | sort -V | tail -1)
    l=$(grep -m1 '^license' "$d/Cargo.toml" | sed 's/license = //; s/"//g')
    printf '\n==== %s %s (%s, crates.io; MIT text below) ====\n\n' "$c" "$v" "$l"; cat "$d/LICENSE-MIT"
  done
} > "$out"
echo "$out: $(grep -c '^====' "$out") components"
