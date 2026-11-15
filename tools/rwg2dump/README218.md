# rwg2dump 2.18 — `/rwg2dump probe`

A quick probe (a few minutes, server thread). It writes `rwg2dump/<ts>-probe/probe218.json` + `chestloot.json` and
zips the folder.

## Owner run

1. Create a **new singleplayer world** (GTNH 2.10 instance): type `RWG2_CONTINENTS`, seed **`1631302684446519836`**
   (the cart targets are predicted for this seed), render distance 8 or less.
2. On first spawn, don't move. Run `/rwg2dump probe` from chat.
3. Wait for `rwg2dump 2.18.0 probe -> …` (2–8 min freeze).
4. Send `<instance>/.minecraft/rwg2dump/<ts>-probe.zip`, then delete the world.

## Sections of `probe218.json`

| key | content | answers |
|---|---|---|
| `listeners` | every terrain-gen listener on `EVENT_BUS` / `TERRAIN_GEN_BUS` / `ORE_GEN_BUS` (owner class + ASM handler), the `IWorldGenerator`s with GameRegistry weights | who listens to Populate / Decorate events |
| `oceans` | nearest Kelp Forest and Coral Reef areas (≥ 2560 blocks out, 3×3 chunks + 1 ring), the 2.16 event log plus LOWEST twins (`populateLate`, `popPostLate`, `decPreLate`) and the firing stack (16 frames) of every CUSTOM / ICE / ANIMALS populate event and every `decPre` / `decPost` | which code reseeds the shared Random before RWG2's deferred ocean decorator (kelp / coral) |
| `carts` | 40 predicted mineshaft chest carts (`carts218.txt`, rwg2-wasm v1.57, 768–1230 blocks out); the populating chunk ±2 is force-loaded, then every minecart with an inventory in the inner 3×3 is listed: class, position, slots `[slot, stack, registry id, display name]` | chest-cart positions and contents vs the model |
