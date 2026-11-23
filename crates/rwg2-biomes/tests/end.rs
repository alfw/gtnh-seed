#![cfg(feature = "json")]
//! The End (HEE over vanilla ChunkProviderEnd) vs rwg2dump 2.11.0 (`dimterrain_1.bin`, chunks [-12, 12]^2).
use rwg2_biomes::end::EndTerrain;

fn root() -> std::path::PathBuf {
    std::env::var("RWG2_DATA_DIR")
        .map(|d| std::path::PathBuf::from(d).parent().unwrap().to_path_buf())
        .unwrap_or_else(|_| std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../.."))
}

const SEED: i64 = 4_285_358_484_800_856_671;
const N: usize = 256;
const COLS: usize = 625 * N;

#[test]
fn end_terrain_vs_dump211() {
    for (seed, dir) in [
        (SEED, "4285358484800856671"),
        (-1_053_966_461_076_653_616, "-1053966461076653616"),
    ] {
        let f = root().join(format!(
            "testdata/pack/gtnh-2.10.0/heldout/{dir}/dimterrain_1.bin"
        ));
        let Ok(bin) = std::fs::read(&f) else {
            eprintln!("skip: {f:?}");
            continue;
        };
        let t = EndTerrain::new(seed);
        let (mut blocks, mut extra_solid, mut extra_air, mut top_ok, mut solid_cols) =
            (0u64, 0u64, 0u64, 0u64, 0u64);
        for k in 0..625usize {
            let (cx, cz) = (-12 + (k % 25) as i32, -12 + (k / 25) as i32);
            let c = t.chunk(cx, cz);
            for i in 0..N {
                let g = k * N + i;
                let (lx, lz) = ((i % 16) as i32, (i / 16) as i32);
                let pre_top = if bin[g] == 255 { -1 } else { bin[g] as i32 };
                top_ok += (c.top(lx, lz) == pre_top) as u64;
                solid_cols += (pre_top >= 0) as u64;
                for y in 0..128 {
                    let r = bin[COLS * 3 + g * 32 + (y >> 3)] >> (y & 7) & 1 != 0;
                    let m = c.solid(lx, y as i32, lz);
                    blocks += 1;
                    extra_solid += (m && !r) as u64;
                    extra_air += (!m && r) as u64;
                }
            }
        }
        eprintln!("end {seed}: {blocks} blocks, model solid/real air {extra_solid}, model air/real solid {extra_air}; tops {top_ok}/{} ({solid_cols} non-void)", COLS);
        assert_eq!((extra_solid, extra_air), (0, 0));
    }
}

#[test]
fn end_islands_towers_enumeration() {
    use rwg2_biomes::end::*;
    let (r, k) = (220, 0);
    let isl = islands(SEED, k, -r, -r, r, r);
    let tw = towers(SEED, k, -r, -r, r, r);
    let mut bi = Vec::new();
    let mut bt = Vec::new();
    for cz in -r..=r {
        for cx in -r..=r {
            if island_at(SEED, k, cx, cz) {
                bi.push((cx, cz));
            }
            if tower_at(SEED, k, cx, cz) {
                bt.push((cx, cz));
            }
        }
    }
    let mut ei: Vec<_> = isl.iter().map(|i| (i.cx, i.cz)).collect();
    ei.sort_by_key(|p| (p.1, p.0));
    let mut et = tw.clone();
    et.sort_by_key(|p| (p.1, p.0));
    assert_eq!(ei, bi);
    assert_eq!(et, bt);
    eprintln!("±{r} chunks: {} islands, {} towers", isl.len(), tw.len());
    for i in isl.iter().take(8) {
        let d: Vec<&str> = i
            .deviations
            .iter()
            .map(|&d| ISLAND_DEVIATIONS[i.biome as usize][d as usize].0)
            .collect();
        let v = ISLAND_VARIATIONS[i.biome as usize]
            .iter()
            .find(|v| v.0 == i.variation)
            .unwrap()
            .1;
        eprintln!(
            "  island chunk {} {} centre {} {}: {} / {} {:?} bottomY {}",
            i.cx,
            i.cz,
            i.cx * 16 + 104,
            i.cz * 16 + 104,
            ISLAND_BIOMES[i.biome as usize],
            v,
            d,
            i.bottom_y
        );
    }
    let near: Vec<_> = tw
        .iter()
        .map(|&(x, z)| {
            (
                (((x * 16 + 16) as f64).hypot((z * 16 + 16) as f64)) as i32,
                x,
                z,
            )
        })
        .filter(|t| t.0 < 1200)
        .collect();
    eprintln!("  towers within 1200: {near:?}");
}

/// HEE islands / towers vs HEE's own classes (rwg2dump ≥ 2.12.0 `dims.json` → `endStructures`). Skips without.
#[test]
fn end_structures_vs_hee() {
    use rwg2_biomes::end::*;
    let base = root().join(dumps_under());
    let mut found = 0;
    for set in ["gtnh-2.10.0/heldout", "gtnh-2.10.0/tune", "gtnh-2.11.0"] {
        let Ok(rd) = std::fs::read_dir(base.join(set)) else {
            continue;
        };
        for e in rd.flatten() {
            let Ok(s) = std::fs::read_to_string(e.path().join("dims.json")) else {
                continue;
            };
            let v: serde_json::Value = serde_json::from_str(&s).unwrap();
            let es = &v["endStructures"];
            if es.is_null() || es.get("error").is_some() {
                continue;
            }
            found += 1;
            let seed: i64 = es["worldSeed"].as_str().unwrap().parse().unwrap();
            let kills = es["dragonKills"].as_i64().unwrap().max(0) as i32;
            let r = es["rangeChunks"].as_i64().unwrap() as i32;
            let pairs = |k: &str| -> Vec<(i32, i32)> {
                es[k]
                    .as_array()
                    .unwrap()
                    .iter()
                    .map(|a| (a[0].as_i64().unwrap() as i32, a[1].as_i64().unwrap() as i32))
                    .collect()
            };
            let order = |mut v: Vec<(i32, i32)>| {
                v.sort_by_key(|p| (p.1, p.0));
                v
            };
            let mine = islands(seed, kills, -r, -r, r, r);
            let real = es["islands"].as_array().unwrap();
            assert_eq!(
                order(mine.iter().map(|i| (i.cx, i.cz)).collect()),
                order(pairs("islands")),
                "island positions"
            );
            assert_eq!(
                order(pairs("islandStarts")),
                order(pairs("islands")),
                "MapGenIsland == IslandSpawnChecker"
            );
            let mut data_ok = 0;
            for a in real {
                let (cx, cz) = (a[0].as_i64().unwrap() as i32, a[1].as_i64().unwrap() as i32);
                let i = island_data(seed, cx, cz);
                let devs: Vec<&str> = i
                    .deviations
                    .iter()
                    .map(|&d| ISLAND_DEVIATIONS[i.biome as usize][d as usize].0)
                    .collect();
                let devs = if devs.is_empty() {
                    "{}".to_string()
                } else {
                    format!("{{{}}}", devs.join(","))
                };
                let ok = i.biome as i64 == a[2].as_i64().unwrap()
                    && i.bottom_y as i64 == a[3].as_i64().unwrap()
                    && i.biome as i64 == a[4].as_i64().unwrap()
                    && i.variation as i64 == a[5].as_i64().unwrap()
                    && devs == a[6].as_str().unwrap();
                if !ok {
                    eprintln!("island {cx} {cz}: model {i:?} {devs} real {a:?}");
                }
                data_ok += ok as usize;
            }
            let next = islands(seed, kills + 1, -r, -r, r, r);
            assert_eq!(
                order(next.iter().map(|i| (i.cx, i.cz)).collect()),
                order(pairs("islandsNextKill")),
                "islands after one more kill"
            );
            let tw = towers(seed, kills, -r, -r, r, r);
            assert_eq!(order(tw), order(pairs("towerStarts")), "towers");
            eprintln!(
                "end structures {seed} (kills {kills}): islands {} (data {data_ok}), towers {}",
                real.len(),
                pairs("towerStarts").len()
            );
            assert_eq!(data_ok, real.len());
        }
    }
    eprintln!("end_structures_vs_hee: {found} dumps");
}

/// The main island never reaches past ±24 chunks (wasm `END_R`): density ≤ −100 + noise/512 out there.
#[test]
fn end_main_island_extent() {
    use rwg2_biomes::end::EndTerrain;
    for seed in [SEED, 0, 1, -1, 1337, 92_900_795, i64::MIN, i64::MAX] {
        let t = EndTerrain::new(seed);
        let mut maxr = 0;
        for cz in -30i32..=30 {
            for cx in -30i32..=30 {
                if cx.abs() <= 16 && cz.abs() <= 16 {
                    continue;
                }
                let c = t.chunk(cx, cz);
                if c.cols.iter().any(|&b| b != 0) {
                    maxr = maxr.max(cx.abs().max(cz.abs()));
                }
            }
        }
        assert!(maxr <= 24, "seed {seed}: solid at chunk radius {maxr}");
    }
}

/// HEE's `nextInt(1 + resets)` reroll draws one `next(31)` for any bound (rejection < 2^-28 per call for small
/// bounds), so island/tower layouts are the same for every reset count in practice.
#[test]
fn end_resets_do_not_move_structures() {
    use rwg2_biomes::end::*;
    let a = islands(SEED, 0, -150, -150, 150, 150);
    let ta = towers(SEED, 0, -150, -150, 150, 150);
    for k in [1, 2, 3, 7, 20] {
        assert_eq!(
            a,
            islands(SEED, k, -150, -150, 150, 150),
            "islands, resets {k}"
        );
        assert_eq!(
            ta,
            towers(SEED, k, -150, -150, 150, 150),
            "towers, resets {k}"
        );
    }
}

#[test]
fn end_island_shapes_and_de() {
    use rwg2_biomes::end::*;
    let seed = -1_053_966_461_076_653_616i64;
    let isl = islands(seed, 0, -150, -150, 150, 150);
    let t0 = std::time::Instant::now();
    for i in isl.iter().take(6) {
        let s = island_shape(seed, i.cx, i.cz);
        let solid: Vec<i16> = s.top.iter().copied().filter(|&t| t >= 0).collect();
        let (mn, mx) = (
            solid.iter().min().copied().unwrap_or(-1),
            solid.iter().max().copied().unwrap_or(-1),
        );
        // extent of solid columns relative to the centre
        let mut r = 0;
        for j in 0..208 {
            for k in 0..208 {
                if s.top[j * 208 + k] >= 0 {
                    r = r.max(((k as i32 - 96).abs()).max((j as i32 - 96).abs()));
                }
            }
        }
        eprintln!(
            "island {} {} {}: {} columns, top Y {mn}..{mx}, half-extent {r}",
            i.cx,
            i.cz,
            ISLAND_BIOMES[i.biome as usize],
            solid.len()
        );
        assert!(!solid.is_empty() && r < 104);
    }
    eprintln!("6 shapes in {:?}", t0.elapsed());
    let c = comets(seed, 10000, -500, -500, 500, 500);
    eprintln!(
        "comets in ±500 chunks: {} (expected ~{})",
        c.len(),
        1001 * 1001 / 10000
    );
    for x in c.iter().take(3) {
        eprintln!("  {x:?}");
    }
    assert_eq!(chaos_islands(10000, -15000, -15000, 15000, 15000).len(), 8);
}

/// Generated End samples (rwg2dump ≥ 2.13.0 `dims.json` → `endGenerated`): a real HEE island's post-population
/// tops vs the terrain-pass shape, and real blocks at the two nearest predicted DE comets. Skips without.
#[test]
fn end_generated_vs_model() {
    use rwg2_biomes::end::*;
    let base = root().join(dumps_under());
    for set in ["gtnh-2.10.0/heldout", "gtnh-2.10.0/tune", "gtnh-2.11.0"] {
        let Ok(rd) = std::fs::read_dir(base.join(set)) else {
            continue;
        };
        for e in rd.flatten() {
            let Ok(s) = std::fs::read_to_string(e.path().join("dims.json")) else {
                continue;
            };
            let v: serde_json::Value = serde_json::from_str(&s).unwrap();
            let g = &v["endGenerated"];
            if g.is_null() || g.get("error").is_some() {
                continue;
            }
            let raw: i64 = g["worldInfoSeed"].as_str().unwrap().parse().unwrap();
            let prov: i64 = g["worldGetSeed"].as_str().unwrap().parse().unwrap();
            eprintln!(
                "endGenerated {raw}: World.getSeed {prov} (resets {})",
                prov.wrapping_sub(raw)
            );
            let blocks: Vec<&str> = g["blocks"]
                .as_array()
                .unwrap()
                .iter()
                .map(|b| b.as_str().unwrap())
                .collect();
            if let Some(isl) = g.get("island") {
                let sc = &isl["startChunk"];
                let (cx, cz) = (
                    sc[0].as_i64().unwrap() as i32,
                    sc[1].as_i64().unwrap() as i32,
                );
                let sh = island_shape(raw, cx, cz);
                assert_eq!(
                    (sh.x0, sh.z0),
                    (
                        isl["x0"].as_i64().unwrap() as i32,
                        isl["z0"].as_i64().unwrap() as i32
                    )
                );
                let tops = isl["tops"].as_array().unwrap();
                let tb = isl["topBlocks"].as_array().unwrap();
                let (
                    mut both,
                    mut real_only,
                    mut model_only,
                    mut exact,
                    mut pm1,
                    mut higher,
                    mut lower,
                ) = (0, 0, 0, 0, 0, 0, 0);
                let mut top_names = std::collections::BTreeMap::<&str, i32>::new();
                for k in 0..208 * 208 {
                    let r = tops[k].as_i64().unwrap() as i32;
                    let m = sh.top[k] as i32;
                    match (r >= 0, m >= 0) {
                        (true, true) => {
                            both += 1;
                            exact += (r == m) as i32;
                            pm1 += ((r - m).abs() <= 1) as i32;
                            higher += (r > m + 1) as i32;
                            lower += (r < m - 1) as i32;
                            *top_names
                                .entry(blocks[tb[k].as_i64().unwrap() as usize])
                                .or_default() += 1;
                        }
                        (true, false) => real_only += 1,
                        (false, true) => model_only += 1,
                        _ => {}
                    }
                }
                eprintln!(
                    "  island {cx} {cz} ({}): solid both {both}, real only {real_only}, model only {model_only}; top exact {exact}, ±1 {pm1}, real higher {higher} (decoration), lower {lower} (caves)",
                    ISLAND_BIOMES[sh.island.biome as usize]
                );
                eprintln!("  real top blocks: {top_names:?}");
            }
            // 2.13.2: an island generated fresh in the dump session (`diag.probeFresh`): full-depth census every 4th
            // column vs terrain + caves. Real solids = census minus fluids, plants, clouds, minus the background of
            // an island-free area of the same size (`probeNearest`: HEE blobs, ores)
            if let Some(pf) = g["diag"].get("probeFresh") {
                let sc = &pf["startChunk"];
                let (cx, cz) = (
                    sc[0].as_i64().unwrap() as i32,
                    sc[1].as_i64().unwrap() as i32,
                );
                let (sh, n) = island_solids(raw, cx, cz);
                let model: u32 = (0..208)
                    .step_by(4)
                    .flat_map(|j| (0..208).step_by(4).map(move |k| (j, k)))
                    .map(|(j, k)| n[j * 208 + k] as u32)
                    .sum();
                let solids = |c: &serde_json::Value| -> i64 {
                    c.as_object()
                        .unwrap()
                        .iter()
                        .filter(|(k, _)| {
                            ![
                                "Cloud",
                                "chorus",
                                "flower",
                                "decoration",
                                "lava",
                                "ender_goo",
                                "water",
                            ]
                            .iter()
                            .any(|w| k.contains(w))
                        })
                        .map(|(_, v)| v.as_i64().unwrap())
                        .sum()
                };
                // the background area must be island-free: a probe that landed on a generated island (surface blocks)
                // is no background (2.14 world 4903…: an Enchanted Island laboratory) → census not comparable
                let near = g["diag"].get("probeNearest");
                if near.is_some_and(|p| {
                    p["censusBefore"]
                        .get("HardcoreEnderExpansion:end_stone_terrain")
                        .is_some()
                }) {
                    eprintln!("  fresh island census: background probe sits on an island, skipped (see fresh_island_blocks_vs_214)");
                    continue;
                }
                let background = near.map(|p| solids(&p["censusBefore"])).unwrap_or(0);
                let solid = solids(&pf["censusBefore"]) - background;
                let census = pf["censusBefore"].as_object().unwrap();
                let surface = census
                    .get("HardcoreEnderExpansion:end_stone_terrain")
                    .and_then(|v| v.as_i64())
                    .unwrap_or(0);
                eprintln!(
                    "  fresh island {cx} {cz} ({}, bottomY {}): real solids {solid} (background {background} removed, surface blocks {surface}) vs model terrain + caves {model}",
                    ISLAND_BIOMES[sh.island.biome as usize], sh.island.bottom_y
                );
                assert!(surface > 0, "fresh island has no HEE surface blocks");
                let ratio = solid as f64 / model as f64;
                assert!(
                    (0.97..1.03).contains(&ratio),
                    "fresh island solids ratio {ratio}"
                );
            }
            let mine = comets(
                prov,
                g["deConfig"]["cometRarity"].as_i64().unwrap_or(10000) as i32,
                -400,
                -400,
                400,
                400,
            );
            for c in g["comets"].as_array().unwrap() {
                let (x, y, z) = (
                    c["x"].as_i64().unwrap() as i32,
                    c["y"].as_i64().unwrap() as i32,
                    c["z"].as_i64().unwrap() as i32,
                );
                let found = mine.iter().any(|m| (m.x, m.y, m.z) == (x, y, z));
                eprintln!(
                    "  comet {x} {y} {z} size {}: in model {found}; real blocks in core cube {}",
                    c["size"], c["censusInCube"]
                );
            }
        }
    }
}

/// rwg2dump 2.14 `checks214.island`: block scan (every column, 208², bottom-up runs) of an island generated fresh in
/// the dump session vs the model's terrain + caves, block by block. Terrain = end stone (vanilla / HEE surface) and the
/// ores that replace it; decoration (plants, clouds, fluids, HEE structures) is reported, not compared.
#[test]
fn fresh_island_blocks_vs_214() {
    let base = root().join("testdata/pack/gtnh-2.10.0/heldout");
    let Ok(rd) = std::fs::read_dir(&base) else {
        return;
    };
    let mut checked = 0;
    for e in rd.flatten() {
        let Ok(s) = std::fs::read_to_string(e.path().join("dims.json")) else {
            continue;
        };
        let d: serde_json::Value = serde_json::from_str(&s).unwrap();
        let g = &d["endGenerated"];
        let isl = &g["checks214"]["island"];
        if isl["scan"].is_null() {
            continue;
        }
        let seed: i64 = g["worldInfoSeed"].as_str().unwrap().parse().unwrap();
        let names: Vec<&str> = g["blocks"]
            .as_array()
            .unwrap()
            .iter()
            .map(|v| v.as_str().unwrap())
            .collect();
        let (cx, cz) = (
            isl["startChunk"][0].as_i64().unwrap() as i32,
            isl["startChunk"][1].as_i64().unwrap() as i32,
        );
        let sc = &isl["scan"];
        let (x0, z0) = (
            sc["x0"].as_i64().unwrap() as i32,
            sc["z0"].as_i64().unwrap() as i32,
        );
        let (sh, _) = rwg2_biomes::end::island_cols(seed, cx, cz);
        // terrain + caves + ores (+ Homeland cellars), `end_homeland`
        let cols = rwg2_biomes::end_homeland::island_blocks(seed, cx, cz).cols();
        assert_eq!((sh.x0, sh.z0), (x0, z0), "scan origin = island area origin");
        // block name after the mod id ("HardcoreEnderExpansion" / "Forestry" contain "ore")
        let terrain = |n: &str| {
            let b = n.rsplit(':').next().unwrap_or(n);
            b.contains("end_stone") || b.contains("ore") || b.contains("Ore")
        };
        let (mut both, mut model_only, mut real_only) = (0u64, 0u64, 0u64);
        let mut other: std::collections::BTreeMap<&str, u64> = Default::default();
        let mut real_only_names: std::collections::BTreeMap<&str, u64> = Default::default();
        let mut diag: std::collections::BTreeMap<&str, u64> = Default::default();
        for (j, row) in sc["rows"].as_array().unwrap().iter().enumerate() {
            for (i, col) in row.as_array().unwrap().iter().enumerate() {
                let mut real = [0u128; 2];
                for run in col.as_str().unwrap().split(' ').filter(|r| !r.is_empty()) {
                    let p: Vec<i32> = run.split(':').map(|v| v.parse().unwrap()).collect();
                    let n = names[p[2] as usize];
                    if !terrain(n) {
                        *other.entry(n).or_default() += p[1] as u64;
                        continue;
                    }
                    for y in p[0]..p[0] + p[1] {
                        if (0..256).contains(&y) {
                            real[(y >> 7) as usize] |= 1u128 << (y & 127);
                        }
                    }
                    let _ = n;
                }
                let m = cols[j * 208 + i].0;
                // diagnostics: where the disagreements sit relative to the model column
                {
                    let bit =
                        |w: &[u128; 2], y: i32| w[(y >> 7) as usize] & (1u128 << (y & 127)) != 0;
                    let (mut lo, mut hi) = (-1, -1);
                    for y in 0..256 {
                        if bit(&m, y) {
                            if lo < 0 {
                                lo = y;
                            }
                            hi = y;
                        }
                    }
                    let mut occ = [0u128; 2];
                    for run in col.as_str().unwrap().split(' ').filter(|r| !r.is_empty()) {
                        let p: Vec<i32> = run.split(':').map(|v| v.parse().unwrap()).collect();
                        for y in p[0].max(0)..(p[0] + p[1]).min(256) {
                            occ[(y >> 7) as usize] |= 1u128 << (y & 127);
                        }
                    }
                    for y in 0..256 {
                        let (r, mm) = (bit(&real, y), bit(&m, y));
                        let k = if r && !mm {
                            if lo < 0 {
                                "real only: no model column"
                            } else if y > hi {
                                "real only: above model top"
                            } else if y < lo {
                                "real only: below model bottom"
                            } else {
                                "real only: inside (model cave)"
                            }
                        } else if mm && !r {
                            if bit(&occ, y) {
                                "model only: other block there"
                            } else if y == hi {
                                "model only: air at model top"
                            } else {
                                "model only: air below top"
                            }
                        } else {
                            continue;
                        };
                        *diag.entry(k).or_default() += 1;
                    }
                }
                for h in 0..2 {
                    both += (real[h] & m[h]).count_ones() as u64;
                    model_only += (m[h] & !real[h]).count_ones() as u64;
                    real_only += (real[h] & !m[h]).count_ones() as u64;
                }
                if real != m {
                    for run in col.as_str().unwrap().split(' ').filter(|r| !r.is_empty()) {
                        let p: Vec<i32> = run.split(':').map(|v| v.parse().unwrap()).collect();
                        let n = names[p[2] as usize];
                        if terrain(n)
                            && (p[0]..p[0] + p[1]).any(|y| {
                                (0..256).contains(&y)
                                    && m[(y >> 7) as usize] & (1u128 << (y & 127)) == 0
                            })
                        {
                            *real_only_names.entry(n).or_default() += 1;
                        }
                    }
                }
            }
        }
        // inside the island footprint (columns the model fills): model solid with any real block = agree (HEE paints
        // surface decoration such as cinder / goo over the terrain); end stone in columns the model leaves empty =
        // HEE decorator blobs in the void (unseeded `endRNG`, not predictable), reported only
        let other_there = diag
            .get("model only: other block there")
            .copied()
            .unwrap_or(0);
        let void_blobs = diag.get("real only: no model column").copied().unwrap_or(0);
        let ok = both + other_there;
        let total = ok + (model_only - other_there) + (real_only - void_blobs);
        eprintln!(
            "fresh island {cx},{cz} ({} variation {} deviations {:?}, bottomY {}): terrain both {both}, model only {model_only} ({other_there} under decoration), real only {real_only} ({void_blobs} void blobs); footprint agreement {:.2}%",
            rwg2_biomes::end::ISLAND_BIOMES[sh.island.biome as usize],
            sh.island.variation,
            sh.island.deviations,
            sh.island.bottom_y,
            100.0 * ok as f64 / total as f64
        );
        eprintln!("  real-only runs by block: {real_only_names:?}");
        eprintln!("  non-terrain blocks: {other:?}");
        eprintln!("  where: {diag:?}");
        // Homeland islands need the hidden cellars (`end_homeland`): world 3380… island (92, −27) 92.3% → 98.6%
        assert!(
            ok as f64 >= 0.98 * total as f64,
            "island footprint agreement {ok}/{total}"
        );
        checked += 1;
    }
    eprintln!("{checked} fresh island scans");
}

/// The fast GT-side island columns (terrain + caves + ores on compact masks) equal the full block model's.
#[test]
fn island_compact_interior_matches_blocks() {
    for (seed, cx, cz) in [(3380521564351534918i64, -27, 94), (3380521564351534918, -108, 4)] {
        let t = std::time::Instant::now();
        let a = rwg2_biomes::end_homeland::island_compact_interior(seed, cx, cz);
        let ta = t.elapsed();
        let t = std::time::Instant::now();
        let _ = rwg2_biomes::end::island_cols_compact(seed, cx, cz);
        eprintln!("island {cx},{cz}: interior {ta:?}, terrain only {:?}", t.elapsed());
        let b = rwg2_biomes::end_homeland::island_blocks(seed, cx, cz).compact();
        assert_eq!(a.base, b.base);
        assert!(a.cols == b.cols, "island {cx},{cz}");
    }
}

/// Dump walk root: `DUMPS_UNDER` (e.g. `testdata/pack/gtnh-2.9.1` with `PACK_DATA=gtnh-2.9.1`), else `testdata/pack`
/// minus the RWG2 2.0.9 pack's folder (its worlds need the 2.0.9 data).
#[allow(dead_code)]
fn dumps_under() -> String {
    std::env::var("DUMPS_UNDER").unwrap_or_else(|_| "testdata/pack".into())
}
