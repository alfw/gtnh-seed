//! GT ore veins in the End (`gtend.rs`) vs the real ore blocks rwg2dump scanned there (`dims.json` gtScan "1":
//! chunks ±12 force-loaded z outer / x inner from (−12, −12)). Worlds: the 2.10 held-out worlds with an End scan.
//!
//! Per oreseed the real vein = the End vein with ≥ 2 of its materials as large ores in the oreseed chunk; "empty"
//! = no large GT ores there. Model: the dump's load order (`LoadSim`, only chunks that process ores) and the viewer
//! rule (`resolve_end`).
//!
//! `cargo test --release -p rwg2-biomes --features json --test gtend -- --nocapture`
#![cfg(feature = "json")]

use rwg2_biomes::gtend::*;
use rwg2_biomes::gtorder::{LoadSim, MAX_VEIN_SIZE};
use rwg2_biomes::gtores::*;
use rwg2_biomes::nether::vein_table;
use serde_json::Value;
use std::collections::{BTreeMap, HashMap};
use std::path::PathBuf;

fn root() -> PathBuf {
    std::env::var("RWG2_DATA_DIR")
        .map(|d| PathBuf::from(d).parent().unwrap().to_path_buf())
        .unwrap_or_else(|_| PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../.."))
}

fn int(v: &Value) -> i64 {
    v.as_i64()
        .unwrap_or_else(|| v.as_str().unwrap().parse().unwrap())
}

#[test]
fn end_veins_vs_scan() {
    let base = root().join("testdata/pack/gtnh-2.10.0/heldout");
    let Ok(rd) = std::fs::read_dir(&base) else {
        return;
    };
    let table = vein_table(&GT_END_VEINS);
    let (
        mut tot_real,
        mut tot_empty,
        mut tot_type,
        mut tot_y,
        mut tot_view,
        mut tot_empty_ok,
        mut tot_os,
    ) = (0, 0, 0, 0, 0, 0, 0);
    for e in rd.flatten() {
        let Ok(s) = std::fs::read_to_string(e.path().join("dims.json")) else {
            continue;
        };
        let d: Value = serde_json::from_str(&s).unwrap();
        let scan = &d["gtScan"]["1"];
        if scan.is_null() {
            continue;
        }
        let seed = int(&d["dims"]["1"]["worldSeed"]);
        let rc = int(&scan["rangeChunks"]) as i32;
        let mut obs: BTreeMap<(i32, i32), BTreeMap<i32, (i32, i32)>> = BTreeMap::new();
        for c in scan["chunks"].as_array().unwrap() {
            let ent = obs
                .entry((int(&c["cx"]) as i32, int(&c["cz"]) as i32))
                .or_default();
            for o in c["ores"].as_array().unwrap() {
                let o: Vec<i64> = o.as_array().unwrap().iter().map(int).collect();
                if o[1] == 0 {
                    ent.insert(o[0] as i32, (o[3] as i32, o[4] as i32));
                }
            }
        }
        let mut sim = LoadSim::new();
        sim.area_z_major(-rc * 16, -rc * 16, (rc + 2) * 16 - 1, (rc + 2) * 16 - 1);
        let mut first: HashMap<(i32, i32), (i32, i32)> = HashMap::new();
        for &(px, pz) in &sim.order {
            if !processes_ores(seed, px, pz) {
                continue;
            }
            for ox in px - MAX_VEIN_SIZE..=px + MAX_VEIN_SIZE {
                for oz in pz - MAX_VEIN_SIZE..=pz + MAX_VEIN_SIZE {
                    if is_oreseed(&GT_GTNH, ox, oz) {
                        first.entry((ox, oz)).or_insert((px, pz));
                    }
                }
            }
        }
        let mut w = EndBlocks::new(seed, 0);
        let (mut real_n, mut empty_n, mut type_ok, mut y_ok, mut view_ok, mut empty_ok, mut os) =
            (0, 0, 0, 0, 0, 0, 0);
        let (mut ceil_ok, mut near_ok) = (0, 0);
        let mut misses = Vec::new();
        for cz in -rc + 1..rc {
            for cx in -rc + 1..rc {
                if !is_oreseed(&GT_GTNH, cx, cz) {
                    continue;
                }
                os += 1;
                let seen = obs.get(&(cx, cz)).cloned().unwrap_or_default();
                let real = GT_END_VEIN_IDS.iter().position(|ids| {
                    let mut u: Vec<i32> = ids.to_vec();
                    u.sort();
                    u.dedup();
                    u.iter().filter(|m| seen.contains_key(m)).count() >= u.len().min(2)
                });
                let model = first.get(&(cx, cz)).and_then(|&(px, pz)| {
                    resolve_end_at(seed, &table, &GT_GTNH, cx, cz, px, pz, &mut w)
                });
                let view = resolve_end(seed, &table, &GT_GTNH, cx, cz, &mut w);
                // any processing chunk of the window gives this outcome (load-order ceiling)
                let mut outs = Vec::new();
                for pz in cz - MAX_VEIN_SIZE..=cz + MAX_VEIN_SIZE {
                    for px in cx - MAX_VEIN_SIZE..=cx + MAX_VEIN_SIZE {
                        if processes_ores(seed, px, pz) {
                            outs.push(resolve_end_at(
                                seed, &table, &GT_GTNH, cx, cz, px, pz, &mut w,
                            ));
                        }
                    }
                }
                // a model vein whose ores missed the oreseed chunk: look for its materials at its Y in the 3×3 chunks
                let near = |v: usize, y: i32| {
                    (-1..=1).any(|dz| {
                        (-1..=1).any(|dx| {
                            obs.get(&(cx + dx, cz + dz)).is_some_and(|m| {
                                GT_END_VEIN_IDS[v]
                                    .iter()
                                    .filter(|id| {
                                        m.get(id).is_some_and(|r| r.0 <= y + 8 && r.1 >= y - 1)
                                    })
                                    .count()
                                    >= 2
                            })
                        })
                    })
                };
                match real {
                    Some(v) => {
                        ceil_ok += outs.iter().any(|o| o.is_some_and(|p| p.vein == v)) as usize;
                        real_n += 1;
                        let ry = GT_END_VEIN_IDS[v]
                            .iter()
                            .filter_map(|m| seen.get(m))
                            .map(|r| r.0)
                            .min()
                            .unwrap();
                        let t = model.is_some_and(|p| p.vein == v);
                        type_ok += t as usize;
                        y_ok += model
                            .is_some_and(|p| p.vein == v && ry >= p.min_y - 1 && ry <= p.min_y + 8)
                            as usize;
                        view_ok += view.is_some_and(|p| p.vein == v) as usize;
                        if !t {
                            misses.push(format!(
                                "({cx},{cz}) real {} y{ry} model {:?}",
                                GT_END_VEINS[v].0,
                                model.map(|p| (GT_END_VEINS[p.vein].0, p.min_y, p.rejected))
                            ));
                        }
                    }
                    None => {
                        empty_n += 1;
                        empty_ok += model.is_none() as usize;
                        if let Some(p) = model {
                            if near(p.vein, p.min_y) {
                                near_ok += 1;
                            }
                        }
                        if model.is_some() {
                            misses.push(format!(
                                "({cx},{cz}) real empty {:?} model {:?}",
                                seen.keys().collect::<Vec<_>>(),
                                model.map(|p| (GT_END_VEINS[p.vein].0, p.min_y, p.rejected))
                            ));
                        }
                    }
                }
            }
        }
        eprintln!(
            "End {seed}: oreseeds {os}, real veins {real_n} (type right {type_ok}, + Y {y_ok}, viewer rule {view_ok}, some window chunk {ceil_ok}), empty {empty_n} (model empty {empty_ok}, model vein seen next door {near_ok})"
        );
        for m in &misses {
            eprintln!("  {m}");
        }
        assert_eq!(
            ceil_ok, real_n,
            "a real End vein no window chunk reproduces"
        );
        if int(&scan["freshChunks"]) as usize >= 600 {
            // fresh End: the dump's own load order is the real one
            assert_eq!(type_ok, real_n);
            assert_eq!(y_ok, real_n);
        }
        tot_real += real_n;
        tot_empty += empty_n;
        tot_type += type_ok;
        tot_y += y_ok;
        tot_view += view_ok;
        tot_empty_ok += empty_ok;
        tot_os += os;
    }
    eprintln!("total: oreseeds {tot_os}, real {tot_real} type {tot_type} Y {tot_y} viewer {tot_view}; empty {tot_empty} ok {tot_empty_ok}");
}

#[test]
fn end_ore_zones_match() {
    let seed = 4285358484800856671i64;
    let z = OreZones::new(seed, 40, 40, 160, 160);
    let mut n = 0;
    for cz in 40..=160 {
        for cx in 40..=160 {
            let a = processes_ores(seed, cx, cz);
            assert_eq!(a, z.processes(cx, cz), "({cx},{cz})");
            n += a as usize;
        }
    }
    eprintln!("chunks processing ores in 121²: {n}");
    assert!(n > 0);
}

/// Do HEE island interiors (ores, Homeland cellars: not end stone) change GT End veins on islands? Compares
/// `resolve_end` on `island_cols_compact` (terrain + caves, `EndBlocks::terrain_only`) with the default `end_homeland::island_compact_interior` for
/// every island oreseed within ±`R` chunks (default 160) of a seed.
#[test]
#[ignore]
fn end_island_interior_effect() {
    let seed: i64 = std::env::var("SEED").ok().and_then(|s| s.parse().ok()).unwrap_or(3380521564351534918);
    let r: i32 = std::env::var("R").ok().and_then(|s| s.parse().ok()).unwrap_or(160);
    let table = vein_table(&GT_END_VEINS);
    let mut seeds = Vec::new();
    for cz in -r..=r {
        for cx in -r..=r {
            if cx.rem_euclid(3) == 1 && cz.rem_euclid(3) == 1 && cx * cx + cz * cz > 400 && processing_chunk_end(seed, cx, cz).is_some() {
                seeds.push((cx, cz));
            }
        }
    }
    let mut w = EndBlocks::new(seed, 0);
    w.terrain_only = true;
    let a: Vec<_> = seeds.iter().map(|&(cx, cz)| resolve_end(seed, &table, &GT_GTNH, cx, cz, &mut w).map(|p| format!("{p:?}"))).collect();
    let t = std::time::Instant::now();
    let mut w = EndBlocks::new(seed, 0);
    let b: Vec<_> = seeds.iter().map(|&(cx, cz)| resolve_end(seed, &table, &GT_GTNH, cx, cz, &mut w).map(|p| format!("{p:?}"))).collect();
    eprintln!("interior model {:.1} s", t.elapsed().as_secs_f64());
    let diff: Vec<_> = seeds.iter().zip(a.iter().zip(&b)).filter(|(_, (x, y))| x != y).collect();
    for (s, (x, y)) in &diff {
        eprintln!("  {s:?}: terrain {x:?} / full {y:?}");
    }
    eprintln!("island oreseeds {}, veins {} / {}, differ {}", seeds.len(), a.iter().flatten().count(), b.iter().flatten().count(), diff.len());
}

/// GT End veins on fresh HEE islands (rwg2dump 2.14 `endGenerated.checks214.island`: the island's 14×14 start chunks
/// generated fresh, real GT ores per chunk) with the island interior model (HEE ores + Homeland cellars, default) vs
/// terrain + caves only (`EndBlocks::terrain_only`, the pre-v1.66 model). Per oreseed whose ±2 window lies inside the
/// scanned rect: real vein = ≥ 2 of a vein's materials as large ores in the oreseed chunk; "ceiling" = some window
/// chunk reproduces the real outcome (the island's load order is not recorded), "viewer" = `resolve_end`.
#[test]
fn end_island_veins_vs_214() {
    let base = root().join("testdata/pack/gtnh-2.10.0/heldout");
    let Ok(rd) = std::fs::read_dir(&base) else { return };
    let table = vein_table(&GT_END_VEINS);
    let mut tot = [[0usize; 4]; 2]; // per model: [oreseeds, ceiling ok, viewer ok, viewer exact Y]
    for e in rd.flatten() {
        let Ok(s) = std::fs::read_to_string(e.path().join("dims.json")) else { continue };
        let d: Value = serde_json::from_str(&s).unwrap();
        let isl = &d["endGenerated"]["checks214"]["island"];
        let Some(rect) = isl["chunkRect"].as_array() else { continue };
        let r: Vec<i32> = rect.iter().map(|v| int(v) as i32).collect();
        let seed = int(&d["dims"]["1"]["worldSeed"]);
        let mut obs: BTreeMap<(i32, i32), BTreeMap<i32, (i32, i32)>> = BTreeMap::new();
        for c in isl["gtChunks"].as_array().unwrap() {
            let ent = obs.entry((int(&c["cx"]) as i32, int(&c["cz"]) as i32)).or_default();
            for o in c["ores"].as_array().unwrap() {
                let o: Vec<i64> = o.as_array().unwrap().iter().map(int).collect();
                if o[1] == 0 {
                    ent.insert(o[0] as i32, (o[3] as i32, o[4] as i32));
                }
            }
        }
        for (mi, terrain_only) in [false, true].into_iter().enumerate() {
            let mut w = EndBlocks::new(seed, 0);
            w.terrain_only = terrain_only;
            let mut misses = Vec::new();
            for cz in r[1] + MAX_VEIN_SIZE..=r[3] - MAX_VEIN_SIZE {
                for cx in r[0] + MAX_VEIN_SIZE..=r[2] - MAX_VEIN_SIZE {
                    if !is_oreseed(&GT_GTNH, cx, cz) {
                        continue;
                    }
                    let seen = obs.get(&(cx, cz)).cloned().unwrap_or_default();
                    let real = GT_END_VEIN_IDS.iter().position(|ids| {
                        let mut u: Vec<i32> = ids.to_vec();
                        u.sort();
                        u.dedup();
                        u.iter().filter(|m| seen.contains_key(m)).count() >= u.len().min(2)
                    });
                    let ry = real.map(|v| GT_END_VEIN_IDS[v].iter().filter_map(|m| seen.get(m)).map(|r| r.0).min().unwrap());
                    let same = |p: Option<Placement>| match (real, p) {
                        (None, None) => true,
                        (Some(v), Some(p)) => p.vein == v,
                        _ => false,
                    };
                    let mut outs = Vec::new();
                    for pz in cz - MAX_VEIN_SIZE..=cz + MAX_VEIN_SIZE {
                        for px in cx - MAX_VEIN_SIZE..=cx + MAX_VEIN_SIZE {
                            if processes_ores(seed, px, pz) {
                                outs.push(resolve_end_at(seed, &table, &GT_GTNH, cx, cz, px, pz, &mut w));
                            }
                        }
                    }
                    if outs.is_empty() {
                        continue;
                    }
                    let view = resolve_end(seed, &table, &GT_GTNH, cx, cz, &mut w);
                    let t = &mut tot[mi];
                    t[0] += 1;
                    t[1] += outs.iter().any(|&o| same(o)) as usize;
                    t[2] += same(view) as usize;
                    t[3] += (same(view)
                        && match (view, ry) {
                            (Some(p), Some(y)) => y >= p.min_y - 1 && y <= p.min_y + 8,
                            _ => true,
                        }) as usize;
                    if !same(view) {
                        misses.push(format!(
                            "({cx},{cz}) real {:?} y{ry:?} viewer {:?}",
                            real.map(|v| GT_END_VEINS[v].0),
                            view.map(|p| (GT_END_VEINS[p.vein].0, p.min_y, p.rejected))
                        ));
                    }
                }
            }
            eprintln!(
                "{seed} island {:?} {}: misses {:?}",
                isl["startChunk"],
                if terrain_only { "terrain only" } else { "interior" },
                misses
            );
        }
    }
    eprintln!("interior    [oreseeds, ceiling, viewer, viewer+Y] {:?}", tot[0]);
    eprintln!("terrain only [oreseeds, ceiling, viewer, viewer+Y] {:?}", tot[1]);
    assert!(tot[0][2] >= tot[1][2], "island interior model worse than terrain only");
}
