# Updating the seed map for a new GTNH pack release

The port reproduces specific mod code and pack configs. A new GTNH release (2.9.1, 2.10, …) can change any of them.
This is the procedure to find out what changed and bring `data/gtnh-<version>/` and the code in line. Most
releases should end at step 4 with "no model-relevant changes" plus a data copy.

Tools:
- `tools/rwg2dump/` — server-side dump mod (≥ **2.9.1** captures every config file pack-diff checks).
- `tools/pack-tool/` — `pack-tool diff` and `pack-tool import` (Rust, standalone crate; not part of the workspace).
- `tools/pack-tool/gt_enrich.py` — vein colours/names from GT5-Unofficial source (only for new/changed veins).

```sh
(cd tools/rwg2dump && ./build.sh)                                   # → dist/rwg2dump-<v>.jar (JDK only)
cargo build --release --offline --manifest-path tools/pack-tool/Cargo.toml
PT=tools/pack-tool/target/release/pack-tool
```

## 1. Look at the pack before running anything

In the new pack's `config/` (and the changelog, `releases/changelogs/` in GTNewHorizons/DreamAssemblerXXL), check:

| File / item | Why it matters |
|---|---|
| `config/rwg2.cfg` **absent** | 2.9.0 ships none → RWG2 code defaults. If the pack ships one, every overworld biome/terrain number may change. |
| `config/defaultworldgenerator.cfg` | default world type (`RWG2_CONTINENTS` in 2.9.0) — the map's primary type. |
| `config/VillageNames4/newvillages/village_gen.cfg` | spacing (40 ± 10), `Spawn Biome Names` gate list (`villages.rs`). |
| `config/Thaumcraft.cfg` | `node_rarity` 36, `special_node_rarity` 18, generate_* flags, biome ids (`thaumcraft.rs`). |
| `config/GregTech/WorldGeneration.cfg`, `config/GregTech/UndergroundFluids.cfg` (2.9.0: absent → defaults) | ore vein %/attempts, fluids table. |
| `config/roguelike_dungeons/roguelike.cfg` | `spawnFrequency` 16, upper/lower limit 100/60, dimension white list. |
| `config/lootgames/general.cfg` | `dimRhombs {0=20}`. |
| `config/TwilightForest.cfg` | dimension id 7, empty TF seed, biome ids. |
| `config/hodgepodge.cfg` | `fastRandom` changes cave and structure RNG seeding everywhere. |
| `config/etfuturum/world.cfg` | mesa mineshafts. |
| Mod versions of the modelled mods (list below) | any code change can move worldgen. |

## 2. Make dumps (owner's instance)

1. Install `rwg2dump-<v>.jar` in the new pack's `mods/`.
2. Create **5–7 fresh worlds** with random seeds, world type **RWG2_CONTINENTS** (the default). One plain **RWG2**
   world is optional (Alt-cycle the world type); the dump writes grids for both types for 9 fixed seeds anyway.
3. In each world, after spawning: run `rwg2dump` (chat/console). It force-loads ±24 chunks of the overworld plus
   the Nether and Twilight Forest areas — use throwaway worlds.
4. Zip each `rwg2dump/<timestamp>/` folder; put them in `testdata/pack/gtnh-<version>/worlds/<seed>/`
   (`structures.json` → `worldSeed` gives the seed).

## 3. Diff

```sh
for d in testdata/pack/gtnh-<version>/worlds/*; do $PT diff "$d" --data data/gtnh-<old>; done
```

The report lists every area as `ok`, `CHANGE` (model-relevant; with the modules to update) or `info`, plus notes
for config files captured on only one side. Exit code: 0 no model-relevant change, 1 changes, 2 error.
Expected noise: none — per-session values (`largeTreeSeed`, world seeds, object hashes, timings) are stripped.
All dumps of one pack should give the same report; a difference between worlds of the same pack is itself a bug
(load-order or session state leaking into a "fact").

What each area compares (left: what the port assumes, right: the dump):

| Area | Baseline | Dump source |
|---|---|---|
| Modelled mod versions / jars | `pack-facts.json` | `env.json` mods |
| RWG2 config, noise implementation | `pack-facts.json` | `env.json` |
| RWG2 biomes | `data/…/biomes.json` | `biomes.json` |
| Biome climate / dictionary | `data/…/climate.json` | `forestry.json` |
| GT ore config | `gtores.rs GT_GTNH` | `gregtech.json` |
| GT overworld veins | `data/…/gregtech.json` | `gregtech.json` (overworld veins, registry order) |
| GT Nether / TF veins | `nether.rs GT_NETHER_VEINS / GT_TF_VEINS` | `dims.json` gt |
| GT fluids | `data/…/gtfluids.json` (HashBiMap order) | `gtfluids.json` table |
| Spawn provider, structure generators, Nether/TF providers, TC dim blacklist | `pack-facts.json` | spawn/structures/dims.json |
| Villages spacing + biome list | `villages.rs SPACING, SEPARATION, GTNH_290_SPAWN_BIOME_NAMES` | `villages.json`, `village_gen.cfg` |
| Roguelike | `dungeons.rs ROGUE_GTNH` | `roguelike.cfg` |
| LootGames | `pack-facts.json` | `dungeons.json` |
| Thaumcraft | `thaumcraft.rs TC_GTNH`, `tc_nodes.rs GTNH_ADDONS / GTNH_COMPLEX_COUNT` | `Thaumcraft.cfg`, `tcnodes.json` |
| TF biome ids | `twilight.rs TfBiomeIds::GTNH_290` | `dims.json` tfConfig |
| BOP Nether biomes | `nether.rs BOP_NETHER_BIOMES_GTNH` | `dims.json` |
| Config files (model-relevant keys) | `pack-facts.json` | `dungeons.json` configFiles |

Modelled mods (env.json id → what it drives): `mcp` vanilla 1.7.10, `Forge` per-chunk Random, `rwg2` biomes/terrain,
`BiomesOPlenty` biomes/Nether/spawn provider, `gregtech` ores/fluids, `gtnhlib` vein hashing, `hodgepodge` fastRandom,
`Thaumcraft` + `gadomancy` + `salisarcana` + `tc4tweak` aura nodes/structures, `VillageNames` villages, `etfuturum`
mineshafts, `TwilightForest` TF, `Roguelike`, `lootgames`, `Forestry` bee climate.

## 4. Import the data

```sh
$PT import testdata/pack/gtnh-<version>/worlds/<one seed> data/gtnh-<version> --prev data/gtnh-<old> --pack <version>
```

Writes `biomes.json` (the dump's file), `climate.json` (from `forestry.json`), `gregtech.json` (overworld veins,
live mix names + local ore names, colours carried over by vein), `gtfluids.json` (previous file if the live table is
unchanged, else the live HashBiMap order) and `pack-facts.json` (everything else `diff` compares; not read by the
viewer). Exit 1 = warnings, e.g. new veins without colours:

```sh
git clone --depth 1 --branch <gt-version> https://github.com/GTNewHorizons/GT5-Unofficial /tmp/gt5u
python3 tools/pack-tool/gt_enrich.py data/gtnh-<version>/gregtech.json --gt-src /tmp/gt5u --only-missing
```

Verified on 2.9.0: import of world `977343572691104904` reproduces `data/gtnh-2.9.0` (`gtfluids.json`
byte-identical; `biomes.json`, `climate.json`, `gregtech.json` equal as JSON — only `largeTreeSeed` and the `source`
strings differ), and `gt_enrich.py` reproduces the 2.9.0 colours/names for all 21 veins.

Then point the code at the new data — every hard-coded pack path:

```sh
grep -rn "gtnh-2.9.0\|GTNH 2.9.0" crates tools viewer/src --include=*.rs --include=*.mjs --include=*.sh --include=*.svelte
```

(`crates/rwg2-wasm/src/lib.rs` `include_str!` + `PACK` label, `tools/check-wasm.mjs`, `tools/wasi-runner.sh`,
`tools/run-wasi.mjs`, the tests' `testdata/pack/gtnh-2.9.0/...` paths, the viewer footer text.)

## 5. Fix what changed

| Change | Do |
|---|---|
| `biomes.json` lists/ids/terrain params | Data only; re-run `tests/pack_dump.rs`, `chunkgen`, `worlds.rs`. New biome classes or terrain types → port them (`terrain.rs`, `chunkgen.rs` surface rules). |
| rwg2 jar version | Read the RWG2 diff (`oracle/RWG2_COMMIT` pins 2.0.6's source). Re-run all exact tests on the new worlds. |
| GT veins / config / fluids | Data (`gregtech.json`, `gtfluids.json`) or the `nether.rs` consts for Nether/TF; ore model numbers from `worlds.rs`. |
| Thaumcraft config / aspects | `TC_GTNH`, `GTNH_ADDONS`, `GTNH_COMPLEX_COUNT` (wrong count shifts every complex aspect pick). |
| VillageNames spacing / list | `villages.rs` consts; `tests/villages.rs`. |
| Roguelike / LootGames | `ROGUE_GTNH`; LootGames dimRhombs in `dungeons.rs`. |
| TF ids / BOP Nether list | `TfBiomeIds::GTNH_290`, `BOP_NETHER_BIOMES_GTNH`. |
| hodgepodge `fastRandom` toggled | Cave/structure RNG seeding (`JavaRandom::new(seed ^ 0x5DEECE66D)` vs vanilla) everywhere. |
| Any modelled mod version | Read that mod's diff for worldgen code; re-run its tests. |

Bump constants that carry the version in their name (`GTNH_290`, `gtnh_2_9_0()`) when their values change; keep the
old ones if the viewer should still support the previous pack.

## 6. Validate and publish

```sh
cargo test --release --features json                                  # native
cargo test --release --features json --target wasm32-wasip1           # wasm (tools/wasi-runner.sh)
cargo clippy --release --features json --all-targets
WORLDS_REPORT=/tmp/worlds.txt cargo test --release --features json --test worlds -- --nocapture   # held-out worlds
(cd viewer && npm run build) && node tools/check-wasm.mjs             # 0 mismatches expected
```

Every exact part in `worlds.rs` must be 0 failures on the new worlds; compare the estimate rates with the previous
pack's (`CLAUDE.md`, "Held-out validation") and update the viewer's accuracy labels if they moved. Update the pack
label (`PACK` in `crates/rwg2-wasm/src/lib.rs`), then publish the artifact and note the release in `HISTORY.md` /
`CLAUDE.md`.
