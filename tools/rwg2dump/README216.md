# rwg2dump 2.16 — one-run collection

Everything from 2.15.2 (cumulative: the 2.5.1 cache reader, the 2.9.1 config captures and the 2.11.1 loot
name / id / maxStack ran already in 2.15.1) plus the sections below. Each one is fail-safe: on an exception it
writes an `error` string and the rest goes on. Each is also time-boxed.

## Owner run

1. Create a **new singleplayer world** in GTNH (the 2.10 instance used for 2.15.1 is fine):
   - world type `RWG2_CONTINENTS` (the default);
   - any seed. `1631302684446519836` is preferred, because the 2.15.1 dump and `tree_probe::bop_find` already
     know it;
   - render distance 8 or less.
2. On the first spawn, **don't move or explore**. Run `/rwg2dump` at once from chat.
3. The server freezes for about 10–25 min (log lines `[rwg2dump] v2.16: ...`). Wait for the final line,
   `DONE -> rwg2dump/<ts>` and `ZIP -> ...zip`.
4. Send `<instance>/.minecraft/rwg2dump/<ts>.zip` (the game directory, next to `saves/`). Expect about 25–45 MB (2.15.x was about 20 MB). Then delete the world;
   it has force-loaded areas up to about 20k blocks out.

## New sections

| file | for | runtime / size |
|---|---|---|
| `deco216.json` + `deco216_<area>.bin.gz` | Decoration validation. Up to 14 areas of 7×7 chunks, each the nearest spot dominated (≥ 65/81 samples) by one target: native Temperate Forest / Taiga / Jungle; BOP Garden, Eucalyptus, Chaparral, Fungi Forest (generated table); Frost Forest, Prairie, Tundra (hand ports); Kelp Forest, Coral Reef; Ice Plains Spikes, Sunflower Plains. Searched from 2560 blocks out (fresh ground). Per area: the event log from force-loading the area + 1 ring (`PopulateChunkEvent` Pre/Populate/Post and `DecorateBiomeEvent` Pre/Decorate/Post in order, nesting depth, Random state); post-population blocks y 50..200 (id + meta); `getHeightValue` per column; the GT test column (x+7, z+9, y 0..127); tile entities; block columns y±6 at node tiles. | 3–10 min, ~5–15 MB gz |
| `postpop216.json` | A1 Roguelike: anchors within ±25 chunks of each area (≤ 3 each, ≤ 24). Each attempt is replayed like the game, `validLocation` runs once after force-loading, and it records ground y, 13×13 blocks at ground+4 / ground−3, and 13×13 heightValue. A4 LootGames: ≤ 2 per area, real `setSurfaceLevel` y + 21×21 heightValue and the block under it. A5 spawn walk (64 steps) on the populated spawn area: heightValue, `getTopBlock`, `canCoordinateBeSpawn`. | 1–5 min, < 3 MB |
| `nether216.json` | B9: Nether node tiles in [−12, 12]² (populated by the dims section) + columns y±6 (the Y+1 air pocket). | seconds |
| `gregtech216.json` | B6: the GT vein cache right after the decoration areas (same reader as `gregtech.json`). With the GT test columns in `deco216`, this lets decoration and dirt-blob effects on the stone test be measured. | seconds |

Random states: `s:<hex>` is `java.util.Random.seed` (the scrambled 48-bit state, `JavaRandom::state`).
`o:a,b` is the first two `nextInt()` of a serialised copy, used when reflection into `java.util` is closed; the
harness recovers it with a 2^16 search. `|f:<field>=…` gives the Random-typed fields of a subclass: BOP's
`RandomForcedPositiveOwned` holds the shared Random there. HIGHEST-priority listeners see the state before any
other listener draws; `popPreLate` (LOWEST) sees it after the Pre consumers (Railcraft / NHCore geodes, quarries,
BuildCraft oil).

The Thaumcraft check points (A2) are not computed in Java: ring, hilltop and mound rolls come from the Rust model,
and the dump provides heightValue + blocks + the tile census (eldritch altar / obelisk tiles, node tiles) for every
deco-area chunk.

## Harness

- `cargo test --release --features json --test deco_dump -- --nocapture` (after `gunzip -k deco216_*.bin.gz`) →
  recall / precision per area and biome for three replays (real order; real order + recorded loop-entry Random;
  the viewer's z-major order), loop-entry mismatches (LCG distance), native draws before the BOP decorator (real −
  model) and the real draws per BOP Decorate phase. `DECO_AREA=<name>`, `DECO_CHUNKS=1`.
- `cargo test --release --features json --test dump3 -- --nocapture` → parses and prints the other sections.
