# rwg2dump 2.19 — `/rwg2dump probe`

Same command as 2.18; the probe now also writes the biome spawn lists and the Nether fortress chests. Output:
`rwg2dump/<ts>-probe/probe219.json` + `chestloot.json`, zipped.

## Owner run

1. Install `rwg2dump-2.19.0.jar` (remove 2.18.0).
2. New singleplayer world, type `RWG2_CONTINENTS`, seed **`3380521564351534918`** (13 fortress chests are predicted
   within ±24 chunks of the Nether origin for this seed). Don't enter the Nether first.
3. On first spawn run `/rwg2dump probe`, wait for `rwg2dump 2.19.0 probe -> …` (the Nether part force-loads
   49×49 Nether chunks: a few minutes).
4. Send `<instance>/.minecraft/rwg2dump/<ts>-probe.zip`, then delete the world.

## New sections of `probe219.json`

| key | content | answers |
|---|---|---|
| `spawnLists` | every registered biome: name, `getSpawningChance()`, creature list `[class, weight, min, max]` | ANIMALS draws (`performWorldGenSpawning`): the group size range per entry, i.e. whether Kelp Forest spawn rolls are predictable |
| `fortressChests` | Nether chunks ±24 force-loaded row-major; generator / fortress map-gen classes, structure-map class, every fortress start (chunk, box), every chest tile entity in chunks ±23 with slots `[slot, stack, registry id]` | fortress chest positions and contents vs `fortress::chunk_chest_loot` (`tests/fortress.rs::fortress_chests_vs_probe219`) |

The 2.18 sections (listeners, traced oceans, carts) are unchanged.
