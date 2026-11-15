# rwg2dump 2.19.1 — spawn lists (fix)

2.19.0's spawn-list dump died on the first entry (wrong SRG name for `maxGroupCount`, `field_76299_d`).
Fortress chests from 2.19.0 were fine and are verified (11/11 slot-exact); nothing else needed from them.

2.19.1 dumps every biome's creature / monster / water / ambient lists `[class, weight, min, max]`.
Spawn lists are static registry data, so any world works.

1. Replace `rwg2dump-2.19.0.jar` with `rwg2dump-2.19.1.jar` in `mods/`.
2. Open any world (the 3380521564351534918 one is fine; Nether already generated is fine).
3. `/rwg2dump probe` — the server freezes for ~30 s.
4. Send `<ts>-probe.zip` (≈ 0.1–0.2 MB).
