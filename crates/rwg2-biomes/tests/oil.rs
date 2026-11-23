//! DreamMaster oil spheres vs real oil blocks in the rwg2dump 2.10+ test columns (`gtores.json` `testColumns`:
//! blocks at (cx·16 + 7, y, cz·16 + 9), y 0..127): every real oil column must be explained by a deposit variant
//! ([`oil::variants`]: any Galacticraft jitter, either hive outcome) of an even chunk within ±2.

use rwg2_biomes::config::RwgConfig;
use rwg2_biomes::deco::{oil, oil_row, DecoWorld};
use rwg2_biomes::provider::{BiomeProvider, BiomeTable, TableHeights, WorldType};
use serde_json::Value;
use std::collections::BTreeMap;
use std::path::PathBuf;

fn root() -> PathBuf {
    std::env::var("RWG2_DATA_DIR")
        .map(|d| PathBuf::from(d).parent().unwrap().to_path_buf())
        .unwrap_or_else(|_| PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../.."))
}

#[test]
fn oil_variants_explain_test_columns() {
    let table = BiomeTable::from_rwg2dump_json(
        &std::fs::read_to_string(root().join(pack_rel("biomes.json"))).unwrap(),
    )
    .unwrap();
    let (mut cols, mut explained, mut fountain, mut worlds, mut strict) = (0, 0, 0, 0, 0);
    let mut nvar: BTreeMap<usize, u32> = BTreeMap::new();
    for set in ["tune", "heldout"] {
        let Ok(rd) = std::fs::read_dir(root().join("testdata/pack/gtnh-2.10.0").join(set)) else { continue };
        for wd in rd.flatten() {
            let Ok(txt) = std::fs::read_to_string(wd.path().join("gtores.json")) else { continue };
            let Ok(seed) = wd.file_name().to_string_lossy().parse::<i64>() else { continue };
            let scan: Value = serde_json::from_str(&txt).unwrap();
            let Some(tc) = scan["testColumns"].as_array() else { continue };
            let names: Vec<&str> = scan["censusBlocks"].as_array().unwrap().iter().map(|b| b.as_str().unwrap()).collect();
            let Some(oil_id) = names.iter().position(|n| *n == "BuildCraft|Energy:blockOil") else {
                worlds += 1;
                continue;
            };
            worlds += 1;
            let p = BiomeProvider::new(seed, WorldType::Rwg2Continents, table.clone(), RwgConfig::default());
            let h = TableHeights;
            let mut w = DecoWorld::new(&p, &h, seed);
            let mut var: BTreeMap<(i32, i32), Vec<oil::OilDeposit>> = BTreeMap::new();
            for e in tc {
                let (cx, cz) = (e["cx"].as_i64().unwrap() as i32, e["cz"].as_i64().unwrap() as i32);
                let ys: Vec<i32> = e["ids"]
                    .as_str()
                    .unwrap()
                    .split(' ')
                    .enumerate()
                    .filter(|(_, t)| *t != "s" && t.split(':').next().unwrap().parse::<usize>().ok() == Some(oil_id))
                    .map(|(y, _)| y as i32)
                    .collect();
                if ys.is_empty() {
                    continue;
                }
                cols += 1;
                let (y0, y1) = (*ys.first().unwrap(), *ys.last().unwrap());
                let (x, z) = (cx * 16 + 7, cz * 16 + 9);
                let mut hits = 0usize;
                let mut via_fountain = false;
                let mut strict_hit = false;
                for dcx in -2..=2 {
                    for dcz in -2..=2 {
                        let (ox, oz) = (cx + dcx, cz + dcz);
                        if ox % 2 != 0 || oz % 2 != 0 {
                            continue;
                        }
                        let v = var.entry((ox, oz)).or_insert_with(|| {
                            let mut gen = |x: i32, z: i32| w.gen_id(x, z);
                            oil::variants(seed, &mut gen, &oil_row, ox, oz).into_iter().map(|v| v.3).collect()
                        });
                        for d in v.iter() {
                            let c = d.column(x, z);
                            // strict: every cell inside the sphere's column; else overlap (oil flows: down into
                            // caves next to the sphere, out of a fountain top onto the surface)
                            let inside = c.is_some_and(|(a, b)| a <= y0 && y1 <= b);
                            if inside {
                                strict_hit = true;
                            }
                            let sphere = c.is_some_and(|(a, b)| ys.iter().any(|&y| a <= y && y <= b));
                            // fountain axis (all sizes ≥ medium) / large side columns: from the sphere centre up
                            let axis = d.size() >= 1 && x == d.x && z == d.z
                                || d.size() == 2 && (x - d.x).abs() + (z - d.z).abs() == 1;
                            let up = axis && y0 >= d.y - (d.r - 2);
                            if sphere || up {
                                hits += 1;
                                via_fountain |= up && !sphere;
                            }
                        }
                    }
                }
                *nvar.entry(hits.min(5)).or_default() += 1;
                strict += strict_hit as u32;
                if hits > 0 {
                    explained += 1;
                    fountain += via_fountain as u32;
                } else {
                    // nearest variant: horizontal distance to its centre in units of its radius
                    let mut best: Option<(f32, oil::OilDeposit)> = None;
                    for dcx in -2..=2 {
                        for dcz in -2..=2 {
                            for d in var.get(&(cx + dcx, cz + dcz)).into_iter().flatten() {
                                let q = (((x - d.x).pow(2) + (z - d.z).pow(2)) as f32).sqrt() / d.r as f32;
                                if best.is_none_or(|b| q < b.0) {
                                    best = Some((q, *d));
                                }
                            }
                        }
                    }
                    eprintln!(
                        "  unexplained oil column {seed} chunk {cx},{cz} y {ys:?}; nearest variant {best:?} column {:?}",
                        best.and_then(|b| b.1.column(x, z))
                    );
                }
            }
        }
    }
    eprintln!(
        "{worlds} worlds: real oil test columns {cols}, explained by a deposit variant {explained} (all cells inside {strict}, via fountain {fountain}); variants per column {nvar:?}"
    );
    if cols > 0 {
        assert!(explained * 100 >= cols * 95, "oil columns explained {explained}/{cols}");
    }
}

fn gates() -> rwg2_biomes::population::PreGates {
    let cj: Value =
        serde_json::from_str(&std::fs::read_to_string(root().join(pack_rel("climate.json"))).unwrap()).unwrap();
    rwg2_biomes::population::PreGates::new(cj["biomes"].as_array().unwrap().iter().map(|b| {
        (
            b["id"].as_u64().unwrap() as u16,
            b["name"].as_str().unwrap_or(""),
            b["dict"].as_array().map(|d| d.iter().filter_map(|x| x.as_str()).collect()).unwrap_or_default(),
        )
    }))
}

/// The viewer's spawn-area prediction: the fresh world's populates replayed in the real load order (spawn walk
/// loads + `spawn_area`, as wasm `spawn_order`), DreamMaster deposits from the modelled chain vs real oil in the
/// test columns of the spawn square. Per deposit: confidence, test columns its sphere covers, of which real oil.
#[test]
#[ignore]
fn spawn_area_oil_vs_test_columns() {
    use rwg2_biomes::gtorder::{spawn_populated, LoadSim};
    let table = BiomeTable::from_rwg2dump_json(
        &std::fs::read_to_string(root().join(pack_rel("biomes.json"))).unwrap(),
    )
    .unwrap();
    let g = gates();
    // [predicted, predicted with a covered test column, of which real oil]; real oil columns in the square, explained
    let (mut pred, mut covering, mut confirmed, mut real_cols, mut real_expl) = (0, 0, 0, 0, 0);
    for set in ["tune", "heldout"] {
        let Ok(rd) = std::fs::read_dir(root().join("testdata/pack/gtnh-2.10.0").join(set)) else { continue };
        for wd in rd.flatten() {
            let Ok(seed) = wd.file_name().to_string_lossy().parse::<i64>() else { continue };
            let (Ok(txt), Ok(sp)) = (
                std::fs::read_to_string(wd.path().join("gtores.json")),
                std::fs::read_to_string(wd.path().join("spawn.json")),
            ) else {
                continue;
            };
            let scan: Value = serde_json::from_str(&txt).unwrap();
            let sp: Value = serde_json::from_str(&sp).unwrap();
            let Some(tc) = scan["testColumns"].as_array() else { continue };
            let spawn = (sp["spawn"][0].as_i64().unwrap() as i32, sp["spawn"][2].as_i64().unwrap() as i32);
            let names: Vec<&str> =
                scan["censusBlocks"].as_array().unwrap().iter().map(|b| b.as_str().unwrap()).collect();
            let oil_id = names.iter().position(|n| *n == "BuildCraft|Energy:blockOil");
            let mut real: BTreeMap<(i32, i32), Vec<i32>> = BTreeMap::new();
            let mut toks: BTreeMap<(i32, i32), Vec<String>> = BTreeMap::new();
            for e in tc {
                toks.insert(
                    (e["cx"].as_i64().unwrap() as i32, e["cz"].as_i64().unwrap() as i32),
                    e["ids"].as_str().unwrap().split(' ').map(|t| {
                        if t == "s" { "s".to_string() } else {
                            let i: usize = t.split(':').next().unwrap().parse().unwrap_or(usize::MAX);
                            names.get(i).map_or("?".to_string(), |n| n.to_string())
                        }
                    }).collect(),
                );
                let (cx, cz) = (e["cx"].as_i64().unwrap() as i32, e["cz"].as_i64().unwrap() as i32);
                let ys: Vec<i32> = e["ids"]
                    .as_str()
                    .unwrap()
                    .split(' ')
                    .enumerate()
                    .filter(|(_, t)| *t != "s" && t.split(':').next().unwrap().parse::<usize>().ok() == oil_id)
                    .map(|(y, _)| y as i32)
                    .collect();
                real.insert((cx, cz), ys);
            }
            let p = BiomeProvider::new(seed, WorldType::Rwg2Continents, table.clone(), RwgConfig::default());
            let h = TableHeights;
            let walk = rwg2_biomes::spawn::walk_positions(seed);
            let step = walk.iter().position(|&q| q == spawn);
            let mut sim = LoadSim::new();
            for &(x, z) in walk.iter().take(step.map_or(1, |s| s + 1)) {
                sim.load(x >> 4, z >> 4);
            }
            sim.spawn_area(spawn.0, spawn.1);
            let (a, b, c, d) = spawn_populated(spawn);
            let t = std::time::Instant::now();
            let (_, steps) =
                rwg2_biomes::deco::view::replay_ordered(&p, &h, &g, &sim.order, &|_, _| None, &|_, _| None);
            eprintln!(
                "== {seed} spawn {spawn:?} (walk step {step:?}), {} populates, {:.1} s",
                steps.len(),
                t.elapsed().as_secs_f64()
            );
            let mut deps = Vec::new();
            for s in &steps {
                if let Some((dep, exact)) = s.oil {
                    deps.push((s.cx, s.cz, dep, exact));
                }
            }
            for &(cx, cz, dep, exact) in &deps {
                pred += 1;
                let mut cov = Vec::new();
                for ((tx, tz), ys) in &real {
                    if let Some((y0, y1)) = dep.column(tx * 16 + 7, tz * 16 + 9) {
                        cov.push(((*tx, *tz), (y0, y1), ys.iter().filter(|&&y| y0 <= y && y <= y1).count()));
                    }
                }
                let hit = cov.iter().any(|c| c.2 > 0);
                covering += !cov.is_empty() as u32;
                confirmed += hit as u32;
                eprintln!("  model deposit chunk {cx},{cz} {dep:?} conf {exact:.2}: covered test columns {cov:?}");
            }
            // real oil columns inside the populated square (deposits there come from square chunks)
            for ((tx, tz), ys) in &real {
                if ys.is_empty() || !(a + 2..=c - 2).contains(tx) || !(b + 2..=d - 2).contains(tz) {
                    continue;
                }
                real_cols += 1;
                let ok = deps.iter().any(|(_, _, dep, _)| {
                    dep.column(tx * 16 + 7, tz * 16 + 9).is_some_and(|(y0, y1)| ys.iter().any(|&y| y0 <= y && y <= y1))
                });
                real_expl += ok as u32;
                eprintln!("  real oil column {tx},{tz} y {}..{}: predicted {ok}", ys[0], ys[ys.len() - 1]);
            }
        }
    }
    eprintln!(
        "spawn areas: model deposits {pred}, covering a test column {covering}, real oil there {confirmed}; real oil columns inside {real_cols}, predicted {real_expl}"
    );
}

/// Map-layer prediction outside the spawn area: 8×8-chunk canonical-order group replays (`view::region_oil`) over
/// the whole ±24-chunk test-column range of every world, vs real oil in the test columns (ignored, minutes).
#[test]
#[ignore]
fn region_oil_vs_test_columns() {
    let table = BiomeTable::from_rwg2dump_json(
        &std::fs::read_to_string(root().join(pack_rel("biomes.json"))).unwrap(),
    )
    .unwrap();
    let g = gates();
    let (mut pred, mut covering, mut confirmed, mut real_cols, mut real_expl) = (0, 0, 0, 0, 0);
    // confidence buckets: (covering predictions, confirmed)
    let mut by_conf = [(0u32, 0u32); 4];
    for set in ["tune", "heldout"] {
        let Ok(rd) = std::fs::read_dir(root().join("testdata/pack/gtnh-2.10.0").join(set)) else { continue };
        for wd in rd.flatten() {
            let Ok(seed) = wd.file_name().to_string_lossy().parse::<i64>() else { continue };
            let Ok(txt) = std::fs::read_to_string(wd.path().join("gtores.json")) else { continue };
            let scan: Value = serde_json::from_str(&txt).unwrap();
            let Some(tc) = scan["testColumns"].as_array() else { continue };
            let names: Vec<&str> =
                scan["censusBlocks"].as_array().unwrap().iter().map(|b| b.as_str().unwrap()).collect();
            let oil_id = names.iter().position(|n| *n == "BuildCraft|Energy:blockOil");
            let r = scan["rangeChunks"].as_i64().unwrap() as i32;
            let mut real: BTreeMap<(i32, i32), Vec<i32>> = BTreeMap::new();
            let mut toks: BTreeMap<(i32, i32), Vec<String>> = BTreeMap::new();
            for e in tc {
                toks.insert(
                    (e["cx"].as_i64().unwrap() as i32, e["cz"].as_i64().unwrap() as i32),
                    e["ids"].as_str().unwrap().split(' ').map(|t| {
                        if t == "s" { "s".to_string() } else {
                            let i: usize = t.split(':').next().unwrap().parse().unwrap_or(usize::MAX);
                            names.get(i).map_or("?".to_string(), |n| n.to_string())
                        }
                    }).collect(),
                );
                let (cx, cz) = (e["cx"].as_i64().unwrap() as i32, e["cz"].as_i64().unwrap() as i32);
                let ys: Vec<i32> = e["ids"]
                    .as_str()
                    .unwrap()
                    .split(' ')
                    .enumerate()
                    .filter(|(_, t)| *t != "s" && t.split(':').next().unwrap().parse::<usize>().ok() == oil_id)
                    .map(|(y, _)| y as i32)
                    .collect();
                real.insert((cx, cz), ys);
            }
            let p = BiomeProvider::new(seed, WorldType::Rwg2Continents, table.clone(), RwgConfig::default());
            let h = TableHeights;
            let t = std::time::Instant::now();
            let mut deps = Vec::new();
            let mut gz = (-r).div_euclid(8) * 8;
            while gz <= r {
                let mut gx = (-r).div_euclid(8) * 8;
                while gx <= r {
                    deps.extend(rwg2_biomes::deco::view::region_oil(&p, &h, &g, (gx, gz, gx + 7, gz + 7)));
                    gx += 8;
                }
                gz += 8;
            }
            let (mut wp, mut wc) = (0, 0);
            let diag = std::env::var("OIL_DIAG").is_ok();
            let mut bw = DecoWorld::new(&p, &h, seed);
            for (pc, dep, conf) in &deps {
                pred += 1;
                let mut cov = 0;
                let mut hit = false;
                let mut cols = Vec::new();
                for ((tx, tz), ys) in &real {
                    if let Some((y0, y1)) = dep.column(tx * 16 + 7, tz * 16 + 9) {
                        cov += 1;
                        hit |= ys.iter().any(|&y| y0 <= y && y <= y1);
                        if diag {
                            let t = &toks[&(*tx, *tz)];
                            let (mut st, mut air, mut liq, mut oth) = (0, 0, 0, 0);
                            for y in y0.max(0)..=y1.min(t.len() as i32 - 1) {
                                match t[y as usize].as_str() {
                                    "s" => st += 1,
                                    n if n.contains("air") => air += 1,
                                    n if n.contains("water") || n.contains("lava") || n.contains("Oil") => liq += 1,
                                    _ => oth += 1,
                                }
                            }
                            cols.push(format!("{y0}-{y1}:s{st}/a{air}/l{liq}/o{oth}"));
                        }
                    }
                }
                if diag && cov > 0 {
                    let gen = bw.gen_id(pc.0 * 16 + 8, pc.1 * 16 + 8);
                    let bord: Vec<String> = rwg2_biomes::deco::border(&p, &h, pc.0, pc.1)
                        .iter()
                        .map(|(id, st)| format!("{}@{st:.2}", table.info(*id as _).base_class.rsplit('.').next().unwrap_or("")))
                        .collect();
                    eprintln!("DIAG {seed} {pc:?} hit {hit} conf {conf:.2} gen {gen} y {}..{} r {} border [{}] cols [{}]", dep.y - dep.r, dep.y + dep.r, dep.r, bord.join(" "), cols.join(" "));
                }
                if cov > 0 {
                    covering += 1;
                    confirmed += hit as u32;
                    wp += 1;
                    wc += hit as u32;
                    let b = ((conf * 4.0) as usize).min(3);
                    by_conf[b].0 += 1;
                    by_conf[b].1 += hit as u32;
                }
            }
            let (mut rc, mut re) = (0, 0);
            for ((tx, tz), ys) in &real {
                if ys.is_empty() || tx.abs() > r - 2 || tz.abs() > r - 2 {
                    continue;
                }
                rc += 1;
                re += deps.iter().any(|(_, dep, _)| {
                    dep.column(tx * 16 + 7, tz * 16 + 9).is_some_and(|(y0, y1)| ys.iter().any(|&y| y0 <= y && y <= y1))
                }) as u32;
            }
            real_cols += rc;
            real_expl += re;
            eprintln!(
                "== {seed}: {} deposits ({wp} covering a test column, {wc} real); real oil columns {re}/{rc} predicted; {:.0} s",
                deps.len(),
                t.elapsed().as_secs_f64()
            );
        }
    }
    eprintln!(
        "group replays: deposits {pred}, covering {covering}, confirmed {confirmed}; real oil columns predicted {real_expl}/{real_cols}; by confidence quarter (covering, real) {by_conf:?}"
    );
}

/// `data/<PACK_DATA>/<name>` relative to the repo (`PACK_DATA` default `gtnh-2.9.0`; `gtnh-2.9.1` = RWG2 2.0.9 pack).
#[allow(dead_code)]
fn pack_rel(name: &str) -> String {
    format!("data/{}/{name}", std::env::var("PACK_DATA").unwrap_or_else(|_| "gtnh-2.9.0".into()))
}
