# GTNH Seed Map

Seed viewer and seed search for **GT New Horizons 2.9.1 and 2.9.0** (Minecraft 1.7.10, Realistic-World-Gen-2, world types
`RWG2_CONTINENTS` (the pack default) and `RWG2`). Everything runs in the browser: the world generation is a Rust port
of the pack's generators compiled to WebAssembly, checked block for block against dumps from a real GTNH instance.

**Status: public beta** (`1.68.0-beta.1`). Predictions are checked against real worlds (see the About page for what is exact and what is estimated); the viewer itself is new, so expect rough edges and please report them.

Repository: <https://github.com/alfw/gtnh-seed> · bug reports: <https://github.com/alfw/gtnh-seed/issues> · hosted build (GitHub Pages): <https://alfw.github.io/gtnh-seed/>.

## What it shows

The viewer has a **Simple** mode (the default) and an **Advanced** mode (Layers → Mode).

Simple:

- **Biomes** in all four dimensions (Overworld, Nether, Twilight Forest, End). Overworld biomes are exact, including
  RWG2's border dithering at 1 block/px.
- **Terrain** height map and a **Blocks** map (top block, exact before population) with trees and plants; a walkable
  **3D view** with caves, water and decoration.
- **Forestry bee climate.**
- **World spawn** (exact on checked worlds), **strongholds** (portal room, exact), **villages** (start, exact),
  **volcanoes and lava caves**.
- **Nether fortresses**, **Twilight Forest structures**, **HEE End islands and towers** (exact placement).
- **Seed search** for biomes, flat biome areas, climates, surface blocks, strongholds, villages, landmarks and the other
  dimensions' structures, with relations ("a village within 400 of a stronghold").

Advanced adds:

- **GregTech ore veins** in every dimension (candidate veins, Y and extent exact; which candidate the game places is
  right for ~90% of veins, depending on chunk load order) and **underground fluids** (exact).
- **Thaumcraft** obelisks, hilltop stones, barrow mounds, totems and aura nodes.
- **LootGames and Roguelike dungeons**, vanilla **monster rooms** and **mineshaft chest carts** with their loot, Nether
  **fortress chests** with their loot (exact on a checked world).
- **Abyssal geodes**, **underground lakes**, **DreamMaster oil deposits** (~50–80% of predictions real).
- **Draconic Evolution** comets and chaos islands, draconium.
- The matching search conditions (ore veins, fluids, loot, dungeons, Thaumcraft, geodes, lakes).

Per-feature accuracy and how each was checked are documented in `CLAUDE.md`, `BACKGROUND.md` and `HISTORY.md`.

## Build

```sh
rustup target add wasm32-unknown-unknown
cd viewer && npm ci && npm run build   # wasm, source bundle, then the static site in viewer/dist
npx vite preview                       # or serve viewer/dist with any static file server
```

Build-time settings (environment variables for `npm run build`):

| Variable | Effect |
| --- | --- |
| `VITE_DEFAULT_MODE=simple\|advanced` | Mode of a first visit (release default simple, dev builds advanced). |
| `VITE_DEV_TOOLS=1` | Shows the developer preset searches and "check all" in a release build. |
| `VITE_ISSUES_URL=https://…` | Adds a "Report a bug" link to the footer. |
| `VITE_SITE_URL=https://…/` | Public address; adds the absolute link-preview tags (`og:url`, `og:image` → `og.png`, canonical). |
| `VITE_SOURCE_URL=…` | Points the footer's source link somewhere other than the bundled `source/index.html`. |

The footer shows `v<version> · <build date> · <build id>`; the version is `viewer/package.json`'s, the build id is the
start of the source bundle's SHA-256 (the bundle is deterministic, so the id names the exact source).
`viewer/public/about.html` is the About page (supported packs, exact vs estimated, privacy, licences).

Tests (data-dependent tests skip when their dumps are missing):

```sh
cargo test --release --features json                 # native
cargo test --release --target wasm32-wasip1          # wasm (needs wasmtime, see tools/wasi-runner.sh)
node tools/check-wasm.mjs viewer/src/lib/rwg2.wasm   # browser module vs the pack dump
node tools/check-wasm.mjs viewer/src/lib/rwg2.wasm testdata/pack/gtnh-2.9.1/heldout/<seed> 1   # 2.9.1 (pack 1)
```

Browser smoke tests (Playwright, against a running preview):

```sh
cd viewer && npx vite preview --port 4789 --strictPort &
SMOKE_BROWSER=chromium|webkit|firefox tools/smoke/run.sh   # SMOKE_URL to test another address;
                                                          # SMOKE_EXPECT_DEV=1 for a VITE_DEV_TOOLS=1 build
```

Accuracy audits of the zoomed-out approximations: `tools/audit/*.mjs` and `tools/block-guess-audit.mjs`
(each takes the built wasm as its argument).

## Layout

| Path | What it is |
| --- | --- |
| `crates/rwg2-biomes` | The world generation port: biome provider, terrain, surface, caves, population, decoration, structures, dimensions, GregTech, Thaumcraft, dungeons, loot, seed search. Zero dependencies (serde_json behind the `json` feature). |
| `crates/rwg2-wasm` | Raw C-ABI wasm exports used by the viewer. |
| `viewer` | Vite + Svelte 5 viewer: canvas map, worker pool, three.js 3D view. |
| `data/gtnh-2.9.0`, `data/gtnh-2.9.1` | Pack data extracted from a real instance of each pack: biome lists, climate, ore veins, fluids, loot tables. `crates/rwg2-biomes/src/pack.rs` lists the packs and what differs. |
| `tools/rwg2dump` | Server-side Forge mod that dumps ground truth from a real instance. |
| `tools/source-bundle.sh`, `tools/source-pages.py` | Build the source bundle shipped with every build. |
| `tools/third-party-licenses.sh` | Regenerates `THIRD_PARTY_LICENSES.txt` (licences of the bundled libraries). |
| `tools/smoke`, `tools/audit` | Browser smoke tests; accuracy audits. |
| `tools/og-image.mjs` | Renders `viewer/public/og.png` (link preview) from a running preview. |
| `testdata` | Golden vectors the tests compare against. The real-world pack dumps (`testdata/pack/`, ~3 GB) are not in the repo; tests that need them skip. |

## AI assistance

This project was built with the help of an AI assistant (Anthropic's Claude) under the maintainer's direction. That
includes the Rust port, the viewer, the tests, the tooling and the documentation. The accuracy claims don't rest on
the AI: they come from the tests, which compare the output with dumps from real GTNH worlds (`crates/*/tests/`, `testdata/`,
`tools/rwg2dump`). Where something is not verified that way, the viewer and the About page say it is estimated.
Bug reports with in-game evidence are the best way to catch what the tests miss.

## License and source

The world generation port (`crates/rwg2-biomes`, `crates/rwg2-wasm`) is derived from Realistic-World-Gen-2 and licensed
**GPL-3.0-or-later** (`LICENSE`). Every viewer build ships its complete corresponding source (made by
`tools/source-bundle.sh` during `npm run build`): `source/gtnh-seedmap-src.zip` and a browsable copy,
`source/index.html` (every file as `.txt`), linked from the page footer.

Bundled third-party code (three.js, Svelte, serde_json and its dependencies) and its licences: `THIRD_PARTY_LICENSES.txt`.

Not affiliated with GT New Horizons, Mojang or the mod authors. Not an official Minecraft product.
