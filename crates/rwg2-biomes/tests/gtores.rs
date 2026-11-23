#![cfg(feature = "json")]
use rwg2_biomes::config::RwgConfig;
use rwg2_biomes::dungeons::Terrain;
use rwg2_biomes::gtores::*;
use rwg2_biomes::provider::{BiomeProvider, BiomeTable, TableHeights, WorldType};
use std::collections::{BTreeMap, BTreeSet};

fn root() -> std::path::PathBuf {
    std::env::var("RWG2_DATA_DIR")
        .map(|d| std::path::PathBuf::from(d).parent().unwrap().to_path_buf())
        .unwrap_or_else(|_| std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../.."))
}

/// GT ore veins vs real ore blocks (rwg2dump 2.5: seed 1337 CONTINENTS, chunks [-24, 24]^2 scanned; per chunk the
/// normal-ore materials with count and Y range).
#[test]
fn gt_veins_match_pack() {
    let Ok(s) = std::fs::read_to_string(root().join("testdata/pack/gtnh-2.9.0/dump25/gtores.json"))
    else {
        eprintln!("skip");
        return;
    };
    let truth: serde_json::Value = serde_json::from_str(&s).unwrap();
    // chunk -> material -> (count, minY, maxY), normal ores only
    type Seen = BTreeMap<i32, (i64, i32, i32)>;
    let mut obs: BTreeMap<(i32, i32), Seen> = BTreeMap::new();
    for c in truth["chunks"].as_array().unwrap() {
        let k = (
            c["cx"].as_i64().unwrap() as i32,
            c["cz"].as_i64().unwrap() as i32,
        );
        let e = obs.entry(k).or_default();
        for o in c["ores"].as_array().unwrap() {
            let o: Vec<i64> = o
                .as_array()
                .unwrap()
                .iter()
                .map(|v| v.as_i64().unwrap())
                .collect();
            if o[1] == 0 {
                e.insert(o[0] as i32, (o[2], o[3] as i32, o[4] as i32));
            }
        }
    }
    let gj: serde_json::Value = serde_json::from_str(
        &std::fs::read_to_string(root().join(pack_rel("gregtech.json"))).unwrap(),
    )
    .unwrap();
    let ids: Vec<BTreeSet<i32>> = gj["veins"]
        .as_array()
        .unwrap()
        .iter()
        .map(|v| {
            v["oreIds"]
                .as_array()
                .unwrap()
                .iter()
                .map(|x| x.as_i64().unwrap() as i32)
                .collect()
        })
        .collect();
    let table = VeinTable::from_json(&gj.to_string()).unwrap();
    let btable = BiomeTable::from_rwg2dump_json(
        &std::fs::read_to_string(root().join(pack_rel("biomes.json"))).unwrap(),
    )
    .unwrap();
    let p = BiomeProvider::new(
        1337,
        WorldType::Rwg2Continents,
        btable,
        RwgConfig::default(),
    );
    let mut t = Terrain::new(&p, &TableHeights);

    let (mut n, mut first_ok, mut model_ok, mut empty, mut y_ok) = (0, 0, 0, 0, 0);
    let mut misses = Vec::new();
    let mut truth_pick: Vec<((i32, i32), Option<usize>)> = Vec::new();
    for cz in -23..=23 {
        for cx in -23..=23 {
            if !is_oreseed(&GT_GTNH, cx, cz) {
                continue;
            }
            n += 1;
            let seen = obs.get(&(cx, cz)).cloned().unwrap_or_default();
            if seen.is_empty() {
                empty += 1;
            }
            let ovs = orevein_seed(1337, 0, cx, cz);
            // which candidate attempt does the real block data agree with?
            let mut real = None;
            for i in 0..GT_GTNH.max_placement_attempts {
                let c = candidate(&table, ovs, i, cx, cz).unwrap();
                let want = &ids[c.vein];
                let hit = want
                    .iter()
                    .filter(|m| {
                        seen.get(m)
                            .is_some_and(|v| v.1 <= c.min_y + 8 && v.2 >= c.min_y - 1)
                    })
                    .count();
                if hit >= want.len().min(2) {
                    real = Some((i, c));
                    break;
                }
            }
            let model = resolve(1337, &table, &GT_GTNH, cx, cz, |pl| {
                terrain_stone_ok(&mut t, pl)
            });
            truth_pick.push(((cx, cz), real.map(|r| r.1.vein)));
            if let Some((i, c)) = real {
                if i == 0 {
                    first_ok += 1;
                }
                if model.map(|m| m.vein) == Some(c.vein) {
                    model_ok += 1;
                } else {
                    misses.push((cx, cz, i, c.vein, model.map(|m| (m.vein, m.rejected))));
                }
                let lo = ids[c.vein]
                    .iter()
                    .filter_map(|m| seen.get(m))
                    .map(|v| v.1)
                    .min()
                    .unwrap();
                let hi = ids[c.vein]
                    .iter()
                    .filter_map(|m| seen.get(m))
                    .map(|v| v.2)
                    .max()
                    .unwrap();
                if lo >= c.min_y - 1 && hi <= c.min_y + 8 {
                    y_ok += 1;
                } else {
                    eprintln!(
                        "Y ({cx},{cz}) {} predicted {}..{} seen {lo}..{hi}",
                        table.overworld[c.vein].name,
                        c.min_y - 1,
                        c.min_y + 7
                    );
                }
            } else {
                misses.push((cx, cz, -1, usize::MAX, model.map(|m| (m.vein, m.rejected))));
            }
        }
    }
    eprintln!(
        "oreseeds {n}: no ore blocks {empty}; real vein = a candidate {}, first candidate {first_ok}, model picks it {model_ok}, Y inside {y_ok}",
        n - misses.iter().filter(|m| m.2 < 0).count()
    );
    for m in misses.iter().take(10) {
        eprintln!("miss {:?}", m);
    }
    // the alternative shown in the tooltip: the first pick if the model rejected it, else the next pick
    let mut alt_ok = 0;
    for &((cx, cz), want) in &truth_pick {
        let Some(want) = want else { continue };
        let r = resolve(1337, &table, &GT_GTNH, cx, cz, |pl| {
            terrain_stone_ok(&mut t, pl)
        });
        let alt = alternative(&table, 1337, cx, cz, r.as_ref());
        if r.map(|m| m.vein) == Some(want) || alt.map(|a| a.vein) == Some(want) {
            alt_ok += 1;
        }
    }
    let real = n - misses.iter().filter(|m| m.2 < 0).count();
    eprintln!("model or alternative right: {alt_ok}");
    assert!(
        real >= 250,
        "seed maths: real vein among the candidates {real}/{n}"
    );
    assert!(y_ok >= real - 2, "vein Y {y_ok}/{real}");
    assert!(model_ok >= 205, "placement model {model_ok}/{real}");
    assert!(alt_ok >= 228, "model or alternative {alt_ok}/{real}");
}

struct Truth {
    table: VeinTable,
    /// oreseed -> real vein (first candidate whose ores sit in the oreseed chunk at its Y; None: no match)
    picks: Vec<((i32, i32), Option<usize>)>,
}

fn load_truth() -> Option<Truth> {
    let s =
        std::fs::read_to_string(root().join("testdata/pack/gtnh-2.9.0/dump25/gtores.json")).ok()?;
    let truth: serde_json::Value = serde_json::from_str(&s).unwrap();
    type Seen = BTreeMap<i32, (i64, i32, i32)>;
    let mut obs: BTreeMap<(i32, i32), Seen> = BTreeMap::new();
    for c in truth["chunks"].as_array().unwrap() {
        let k = (
            c["cx"].as_i64().unwrap() as i32,
            c["cz"].as_i64().unwrap() as i32,
        );
        let e = obs.entry(k).or_default();
        for o in c["ores"].as_array().unwrap() {
            let o: Vec<i64> = o
                .as_array()
                .unwrap()
                .iter()
                .map(|v| v.as_i64().unwrap())
                .collect();
            if o[1] == 0 {
                e.insert(o[0] as i32, (o[2], o[3] as i32, o[4] as i32));
            }
        }
    }
    let gj: serde_json::Value = serde_json::from_str(
        &std::fs::read_to_string(root().join(pack_rel("gregtech.json"))).unwrap(),
    )
    .unwrap();
    let ids: Vec<BTreeSet<i32>> = gj["veins"]
        .as_array()
        .unwrap()
        .iter()
        .map(|v| {
            v["oreIds"]
                .as_array()
                .unwrap()
                .iter()
                .map(|x| x.as_i64().unwrap() as i32)
                .collect()
        })
        .collect();
    let table = VeinTable::from_json(&gj.to_string()).unwrap();
    let mut picks = Vec::new();
    for cz in -23..=23 {
        for cx in -23..=23 {
            if !is_oreseed(&GT_GTNH, cx, cz) {
                continue;
            }
            let seen = obs.get(&(cx, cz)).cloned().unwrap_or_default();
            let ovs = orevein_seed(1337, 0, cx, cz);
            let mut real = None;
            for i in 0..GT_GTNH.max_placement_attempts {
                let c = candidate(&table, ovs, i, cx, cz).unwrap();
                let want = &ids[c.vein];
                let hit = want
                    .iter()
                    .filter(|m| {
                        seen.get(m)
                            .is_some_and(|v| v.1 <= c.min_y + 8 && v.2 >= c.min_y - 1)
                    })
                    .count();
                if hit >= want.len().min(2) {
                    real = Some(c.vein);
                    break;
                }
            }
            picks.push(((cx, cz), real));
        }
    }
    Some(Truth { table, picks })
}

fn provider() -> BiomeProvider {
    let btable = BiomeTable::from_rwg2dump_json(
        &std::fs::read_to_string(root().join(pack_rel("biomes.json"))).unwrap(),
    )
    .unwrap();
    BiomeProvider::new(
        1337,
        WorldType::Rwg2Continents,
        btable,
        RwgConfig::default(),
    )
}

/// The dump world's chunk loading: spawn search + spawn area around the real spawn (`SPAWN`, from rwg2dump 2.6
/// spawn.json: walk step 26), the rwg2dump 2.5 server-thread sections that force-load (LootGames ±26 blocks,
/// Roguelike ±21 blocks per attempt, attempts as recorded by the 2.2 dump of the same seed), then the Thaumcraft
/// force-load of chunks [-24, 25]^2 (z-major).
fn dump_order() -> rwg2_biomes::gtorder::LoadSim {
    use rwg2_biomes::gtorder::LoadSim;
    let mut sim = LoadSim::new();
    sim.load(0, 0);
    sim.spawn_area(SPAWN.0, SPAWN.1);
    let d: serde_json::Value = serde_json::from_str(
        &std::fs::read_to_string(root().join("testdata/pack/gtnh-2.9.0/dump22/dungeons.json"))
            .unwrap(),
    )
    .unwrap();
    for g in d["lootgames"]["dungeons"].as_array().unwrap() {
        if g.get("freshChunks").is_none() {
            continue;
        }
        let (x, z) = (
            g["x"].as_i64().unwrap() as i32,
            g["z"].as_i64().unwrap() as i32,
        );
        sim.area_z_major(x - 26, z - 26, x + 26, z + 26);
    }
    for a in d["roguelike"]["anchors"].as_array().unwrap() {
        let Some(att) = a.get("attempts").and_then(|v| v.as_array()) else {
            continue;
        };
        for at in att.iter().filter_map(|v| v.as_array()) {
            let (x, z) = (
                at[0].as_i64().unwrap() as i32,
                at[1].as_i64().unwrap() as i32,
            );
            sim.area_z_major(x - 21, z - 21, x + 21, z + 21);
        }
    }
    sim.area_z_major(-24 * 16, -24 * 16, 26 * 16 - 1, 26 * 16 - 1);
    sim
}

/// Order-aware upper bound: replay the dump world's population order with GT's exact placement test and ore
/// placement (earlier veins' ores are not stone), exact pre-population terrain, caves (Hodgepodge fastRandom).
#[test]
fn gt_veins_dump_order() {
    use rwg2_biomes::gtorder::*;
    let Some(tr) = load_truth() else {
        eprintln!("skip");
        return;
    };
    let p = provider();
    let real: Vec<_> = tr.picks.iter().filter(|x| x.1.is_some()).cloned().collect();
    let sim = dump_order();
    let first = sim.first_processed(|x, z| is_oreseed(&GT_GTNH, x, z));
    let nw = real
        .iter()
        .filter(|((cx, cz), _)| first[&(*cx, *cz)] == (cx - 2, cz - 2))
        .count();
    let mut scores = Vec::new();
    for caves in [false, true] {
        let mut t = Terrain::new(&p, &TableHeights);
        let mut u = Underground::new(&mut t, caves.then(|| Caves::new(1337, true)), SURFACE_DEPTH);
        let mut w = OreWorld::new(1337, &tr.table, &GT_GTNH, &mut u);
        for &(px, pz) in &sim.order {
            if px.abs() <= 27 && pz.abs() <= 27 {
                w.process(px, pz);
            }
        }
        let ok = real
            .iter()
            .filter(|((cx, cz), want)| {
                w.decided
                    .get(&(*cx, *cz))
                    .and_then(|d| d.as_ref().map(|d| d.0.vein))
                    == *want
            })
            .count();
        eprintln!("dump-order replay (caves {caves}): {ok}/{}; first processed chunk = window NW corner for {nw} (others: clipped by the spawn square / scanned area / dungeon force-loads)", real.len());
        scores.push(ok);
    }
    assert!(nw >= 175, "processing order: NW corner {nw}");
    assert!(
        scores[0] >= 219 && scores[1] >= 218,
        "dump-order replay {scores:?}"
    );
}

/// Order-free production rules on the ground truth, exact and approximate terrain.
#[test]
fn gt_veins_ordered_rules() {
    use rwg2_biomes::gtorder::*;
    let Some(tr) = load_truth() else {
        eprintln!("skip");
        return;
    };
    let p = provider();
    let table = &tr.table;
    let real: Vec<_> = tr.picks.iter().filter(|x| x.1.is_some()).cloned().collect();
    let n = real.len() as f64;
    let score = |f: &mut dyn FnMut(i32, i32) -> Option<Placement>| {
        real.iter()
            .filter(|((cx, cz), want)| f(*cx, *cz).map(|m| m.vein) == *want)
            .count()
    };

    let mut t = Terrain::new(&p, &TableHeights);
    let t0 = std::time::Instant::now();
    let exact = score(&mut |cx, cz| resolve_ordered(1337, table, &GT_GTNH, cx, cz, SPAWN, &mut t));
    let te = t0.elapsed().as_secs_f64() * 1e3 / n;
    let exact_nw = score(&mut |cx, cz| {
        resolve_at(1337, table, &GT_GTNH, cx, cz, cx - 2, cz - 2, |x, y, z| {
            terrain_stone(&mut t, x, y, z, SURFACE_DEPTH)
        })
    });
    let mut a = ApproxTerrain::new(&p, &TableHeights);
    let t0 = std::time::Instant::now();
    let approx =
        score(&mut |cx, cz| resolve_ordered_approx(1337, table, &GT_GTNH, cx, cz, SPAWN, &mut a));
    let ta = t0.elapsed().as_secs_f64() * 1e3 / n;
    let approx_nw = score(&mut |cx, cz| {
        resolve_at(1337, table, &GT_GTNH, cx, cz, cx - 2, cz - 2, |x, y, z| {
            y > 0 && y <= a.top(x, z) - SURFACE_DEPTH
        })
    });
    let noise = score(&mut |cx, cz| {
        resolve_ordered_with(1337, table, &GT_GTNH, cx, cz, (0, 0), |x, z| {
            p.noise_at(x, z, &TableHeights).map(|h| h as i32)
        })
    });
    let old_noise = score(&mut |cx, cz| {
        resolve(1337, table, &GT_GTNH, cx, cz, |pl| {
            let (x, z) = (
                pl.west + (pl.east - pl.west) / 2,
                pl.north + (pl.south - pl.north) / 2,
            );
            p.noise_at(x, z, &TableHeights)
                .map_or(true, |h| h as i32 >= pl.min_y + 4)
        })
    });
    eprintln!(
        "of {}: resolve_ordered exact {exact} ({te:.2} ms/oreseed incl. chunkgen), window-NW exact {exact_nw}; \
         approx {approx} ({ta:.3} ms/oreseed), window-NW approx {approx_nw}; noise_at {noise}; old model on noise_at {old_noise}",
        real.len()
    );
    assert!(exact >= 219, "resolve_ordered exact {exact}");
    assert!(exact_nw >= 220, "window NW exact {exact_nw}");
    assert!(approx >= 216, "resolve_ordered_approx {approx}");
    assert!(approx_nw >= 218, "window NW approx {approx_nw}");
}

/// The cave carver (vanilla MapGenCaves + Hodgepodge fastRandom) vs the dump's pre-population top Y
/// (worldchunks, chunks [-6, 6)^2): column tops opened by caves.
#[test]
fn caves_vs_worldchunks() {
    use rwg2_biomes::gtorder::*;
    let Ok(bin) =
        std::fs::read(root().join("testdata/pack/gtnh-2.9.0/dump22/worldchunks_1337.bin"))
    else {
        eprintln!("skip");
        return;
    };
    let p = provider();
    let n = 12 * 12 * 256;
    let mut res = Vec::new();
    for fast in [true, false] {
        let mut t = Terrain::new(&p, &TableHeights);
        let mut u = Underground::new(&mut t, Some(Caves::new(1337, fast)), 0);
        let (mut same, mut carved, mut carved_ok) = (0, 0, 0);
        for cz in -6..6 {
            for cx in -6..6 {
                let ci = ((cz + 6) * 12 + cx + 6) as usize;
                for lz in 0..16 {
                    for lx in 0..16 {
                        let real = bin[n * 2 + ci * 256 + (lz * 16 + lx) as usize] as i32;
                        let h = u.t.top(cx * 16 + lx, cz * 16 + lz).unwrap();
                        let mut y = h;
                        if h >= 63 {
                            let m = u.mask(cx, cz).unwrap();
                            while y > 0 && m.carved(lx, y, lz) {
                                y -= 1;
                            }
                        }
                        let model = y.max(62);
                        if y < h {
                            carved += 1;
                            if real == model {
                                carved_ok += 1;
                            }
                        }
                        if real == model {
                            same += 1;
                        }
                    }
                }
            }
        }
        eprintln!(
            "caves fastRandom {fast}: top Y match {same}/{n}; model opens {carved} tops, {carved_ok} exactly right"
        );
        res.push((same, carved, carved_ok));
    }
    assert!(
        res[0].0 >= n - 10 && res[0].2 >= 150,
        "caves (fastRandom) {:?}",
        res[0]
    );
    assert!(res[1].0 < res[0].0, "vanilla seeding should not fit");
}

/// Cost per 2048-block viewer cell (128 x 128 chunks). `cargo test --release ... -- --ignored gt_cell_cost`
#[test]
#[ignore]
fn gt_cell_cost() {
    use rwg2_biomes::gtorder::*;
    let Some(tr) = load_truth() else { return };
    let p = provider();
    let (c0x, c0z) = (128, -256); // some cell away from the cached origin
    let seeds: Vec<(i32, i32)> = (c0z..c0z + 128)
        .flat_map(|cz| (c0x..c0x + 128).map(move |cx| (cx, cz)))
        .filter(|&(x, z)| is_oreseed(&GT_GTNH, x, z))
        .collect();
    let mut a = ApproxTerrain::new(&p, &TableHeights);
    let t0 = std::time::Instant::now();
    let na = seeds
        .iter()
        .filter(|&&(x, z)| {
            resolve_ordered_approx(1337, &tr.table, &GT_GTNH, x, z, (0, 0), &mut a).is_some()
        })
        .count();
    let ta = t0.elapsed().as_secs_f64();
    let mut t = Terrain::new(&p, &TableHeights);
    let t0 = std::time::Instant::now();
    let ne = seeds
        .iter()
        .filter(|&&(x, z)| {
            resolve_ordered(1337, &tr.table, &GT_GTNH, x, z, (0, 0), &mut t).is_some()
        })
        .count();
    let te = t0.elapsed().as_secs_f64();
    eprintln!(
        "{} oreseeds/cell: approx {ta:.2} s ({na} veins), exact {te:.2} s ({ne} veins)",
        seeds.len()
    );
}

/// World spawn of the dump world (rwg2dump 2.6 spawn.json; WorldInfo spawn X/Z).
const SPAWN: (i32, i32) = (40, -104);

/// `data/<PACK_DATA>/<name>` relative to the repo (`PACK_DATA` default `gtnh-2.9.0`; `gtnh-2.9.1` = RWG2 2.0.9 pack).
#[allow(dead_code)]
fn pack_rel(name: &str) -> String {
    format!("data/{}/{name}", std::env::var("PACK_DATA").unwrap_or_else(|_| "gtnh-2.9.0".into()))
}
