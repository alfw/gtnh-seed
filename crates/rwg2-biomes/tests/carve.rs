#![cfg(feature = "json")]
//! Pre-population carving (`column.rs`: mountain-chain river tunnels, caves, lava-cave landmark) vs the game:
//! highest non-air block of every dumped world-chunk column (`worldchunks_*.bin`) and the BOP spawn climb of
//! every dumped spawn-walk step (`spawn.json`).
use rwg2_biomes::chunkgen::ChunkGen;
use rwg2_biomes::column::CarvedChunk;
use rwg2_biomes::config::RwgConfig;
use rwg2_biomes::gtorder::Caves;
use rwg2_biomes::provider::{BiomeProvider, BiomeTable, TableHeights, WorldType};
use serde_json::Value;
use std::path::PathBuf;

fn root() -> PathBuf {
    std::env::var("RWG2_DATA_DIR")
        .map(|d| PathBuf::from(d).parent().unwrap().to_path_buf())
        .unwrap_or_else(|_| PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../.."))
}

fn dumps() -> Vec<(i64, PathBuf)> {
    let base = root().join("testdata/pack/gtnh-2.9.0");
    let mut v = vec![
        (1337i64, base.join("dump22")),
        (92_900_795, base.join("dump27b")),
    ];
    if let Ok(rd) = std::fs::read_dir(base.join("worlds")) {
        let mut w: Vec<(i64, PathBuf)> = rd
            .filter_map(|e| {
                let e = e.ok()?;
                Some((e.file_name().to_str()?.parse().ok()?, e.path()))
            })
            .collect();
        w.sort();
        v.extend(w);
    }
    if let Ok(only) = std::env::var("CARVE_SEEDS") {
        let o: Vec<i64> = only
            .split(',')
            .filter_map(|s| s.trim().parse().ok())
            .collect();
        v.retain(|d| o.contains(&d.0));
    }
    v
}

fn table() -> BiomeTable {
    BiomeTable::from_rwg2dump_json(
        &std::fs::read_to_string(root().join("data/gtnh-2.9.0/biomes.json")).unwrap(),
    )
    .unwrap()
}

fn tag(seed: i64) -> String {
    if seed < 0 {
        format!("m{}", seed.unsigned_abs())
    } else {
        seed.to_string()
    }
}

#[test]
fn carved_tops_match_worldchunks() {
    let table = table();
    let (mut total, mut bad_full, mut bad_base) = (0usize, 0usize, 0usize);
    for (seed, dir) in dumps() {
        let Ok(js) = std::fs::read_to_string(dir.join("worldchunks.json")) else {
            continue;
        };
        let j: Value = serde_json::from_str(&js).unwrap();
        let Ok(bin) = std::fs::read(
            dir.join(
                j["file"]
                    .as_str()
                    .unwrap_or(&format!("worldchunks_{}.bin", tag(seed))),
            ),
        ) else {
            continue;
        };
        let (cx0, cz0) = (
            j["chunkX0"].as_i64().unwrap() as i32,
            j["chunkZ0"].as_i64().unwrap() as i32,
        );
        let (w, hh) = (
            j["chunksW"].as_i64().unwrap() as i32,
            j["chunksH"].as_i64().unwrap() as i32,
        );
        let n = (w * hh * 256) as usize;
        let top = &bin[n * 2..n * 3];
        let p = BiomeProvider::new(
            seed,
            WorldType::Rwg2Continents,
            table.clone(),
            RwgConfig::default(),
        );
        let caves = Caves::new(seed, true);
        let m = rwg2_biomes::spawn::SurfaceModel::new(&p.table);
        let mut g = ChunkGen::new(&p);
        let (mut wb, mut wf, mut river_cols, mut lava_cols) = (0, 0, 0, 0);
        let mut examples = Vec::new();
        for ci in 0..(w * hh) {
            let (cx, cz) = (cx0 + ci % w, cz0 + ci / w);
            let c = g.generate(&p, &TableHeights, cx, cz).unwrap();
            let full =
                rwg2_biomes::spawn::carved_chunk(&p, &TableHeights, &m, &c, cx, cz, Some(&caves));
            let mut nr = c.clone();
            for r in nr.river_cave.iter_mut() {
                *r = rwg2_biomes::chunkgen::RiverCave::NONE;
            }
            river_cols += c.river_cave.iter().filter(|r| !r.is_none()).count();
            let base = CarvedChunk::new(&p, &TableHeights, &nr, cx, cz, Some(&caves));
            // `base`: no river carve and no hard layers (the previous model)
            for idx in 0..256 {
                let (lx, lz) = ((idx & 15) as i32, (idx >> 4) as i32);
                let byte = top[ci as usize * 256 + idx];
                let real = if byte == 0xff { -1 } else { byte as i32 };
                let f = full.top(lx, lz);
                let b = base.top(lx, lz);
                total += 1;
                if f != real {
                    wf += 1;
                    if examples.len() < 12 {
                        examples.push((
                            cx * 16 + lx,
                            cz * 16 + lz,
                            real,
                            f,
                            b,
                            c.height[idx],
                            c.river_cave[idx],
                        ));
                    }
                }
                wb += (b != real) as usize;
                lava_cols += full.lava_carved(lx, lz) as usize;
            }
        }
        eprintln!(
            "seed {seed}: columns {} | top mismatches full {wf} / baseline (no river carve, no hard layers, no snow) {wb} | river-carve columns {river_cols} | lava-carve columns {lava_cols}",
            w * hh * 256
        );
        for e in &examples {
            eprintln!(
                "   ({}, {}) real {} model {} (no river {}) h {:.2} river {:?}",
                e.0, e.1, e.2, e.3, e.4, e.5, e.6
            );
        }
        bad_full += wf;
        bad_base += wb;
    }
    eprintln!("ALL: {total} columns, top mismatches full {bad_full}, baseline {bad_base}");
    assert!(bad_full <= bad_base, "carving model made tops worse");
}

#[test]
fn bop_climb_matches_spawn_walk() {
    let table = table();
    let (mut n, mut ok_y, mut ok_air) = (0, 0, 0);
    for (seed, dir) in dumps() {
        let Ok(js) = std::fs::read_to_string(dir.join("spawn.json")) else {
            continue;
        };
        let d: Value = serde_json::from_str(&js).unwrap();
        let Some(walk) = d["walk"].as_array() else {
            continue;
        };
        let p = BiomeProvider::new(
            seed,
            WorldType::Rwg2Continents,
            table.clone(),
            RwgConfig::default(),
        );
        let caves = Caves::new(seed, true);
        let m = rwg2_biomes::spawn::SurfaceModel::new(&p.table);
        let mut g = ChunkGen::new(&p);
        let (mut wn, mut wy, mut wa) = (0, 0, 0);
        for s in walk {
            let (x, z) = (
                s["x"].as_i64().unwrap() as i32,
                s["z"].as_i64().unwrap() as i32,
            );
            let pre = s["pre"].as_str().unwrap_or("");
            let Some(py) = s["preY"].as_i64() else {
                continue;
            };
            let c = g.generate(&p, &TableHeights, x >> 4, z >> 4).unwrap();
            let cc = rwg2_biomes::spawn::carved_chunk(
                &p,
                &TableHeights,
                &m,
                &c,
                x >> 4,
                z >> 4,
                Some(&caves),
            );
            let (y, air) = cc.bop_climb(x & 15, z & 15);
            wn += 1;
            let yok = y == py as i32;
            let aok = air == (pre == "minecraft:air");
            wy += yok as usize;
            wa += aok as usize;
            if !yok || !aok {
                let idx = ((z & 15) * 16 + (x & 15)) as usize;
                eprintln!(
                    "  seed {seed} step {} ({x},{z}) real {pre} y {py} | model y {y} air {air} h {:.2} river {:?}",
                    s["i"], c.height[idx], c.river_cave[idx]
                );
            }
        }
        eprintln!("seed {seed}: walk steps {wn}, climb y ok {wy}, air/solid ok {wa}");
        n += wn;
        ok_y += wy;
        ok_air += wa;
    }
    eprintln!("ALL walk steps {n}: climb y {ok_y}, air/solid {ok_air}");
}

/// The BOP climb on the exact painted + cave-dug column (`surface::surface_chunk`) vs every dumped walk step's
/// pre-population block name and y.
#[test]
fn exact_climb_matches_spawn_walk() {
    use rwg2_biomes::surface::{block_name, surface_chunk, SurfaceTable};
    let table = table();
    let (mut n, mut ok) = (0, 0);
    for (seed, dir) in dumps() {
        let Ok(js) = std::fs::read_to_string(dir.join("spawn.json")) else {
            continue;
        };
        let d: Value = serde_json::from_str(&js).unwrap();
        let Some(walk) = d["walk"].as_array() else {
            continue;
        };
        let p = BiomeProvider::new(
            seed,
            WorldType::Rwg2Continents,
            table.clone(),
            RwgConfig::default(),
        );
        let caves = Caves::new(seed, true);
        let m = rwg2_biomes::spawn::SurfaceModel::new(&p.table);
        let st = SurfaceTable::new(&p.table);
        let mut g = ChunkGen::new(&p);
        let (mut wn, mut wok) = (0, 0);
        for s in walk {
            let (x, z) = (
                s["x"].as_i64().unwrap() as i32,
                s["z"].as_i64().unwrap() as i32,
            );
            let pre = s["pre"].as_str().unwrap_or("");
            let Some(py) = s["preY"].as_i64() else {
                continue;
            };
            let (cx, cz) = (x >> 4, z >> 4);
            let c = g.generate(&p, &TableHeights, cx, cz).unwrap();
            let cc =
                rwg2_biomes::spawn::carved_chunk(&p, &TableHeights, &m, &c, cx, cz, Some(&caves));
            let sc = surface_chunk(&p, &TableHeights, &st, &c, cx, cz, &cc);
            let (y, b) =
                rwg2_biomes::spawn::bop_climb_col(&sc.cols[((z & 15) * 16 + (x & 15)) as usize]);
            wn += 1;
            let good = y == py as i32 && block_name(b) == pre;
            wok += good as usize;
            if !good {
                eprintln!(
                    "  seed {seed} step {} ({x},{z}) real {pre} y {py} | model {} y {y}",
                    s["i"],
                    block_name(b)
                );
            }
        }
        eprintln!("seed {seed}: walk steps {wn}, exact climb (name + y) {wok}");
        n += wn;
        ok += wok;
    }
    eprintln!("ALL walk steps {n}: exact climb {ok}");
    assert!(ok * 1000 >= n * 999, "{ok}/{n}");
}

#[test]
#[ignore]
fn debug_columns() {
    use rwg2_biomes::spawn::{chunk_tops, chunk_tops_carved, SurfaceModel};
    let table = table();
    let cols: Vec<(i64, i32, i32)> = std::env::var("COLS")
        .unwrap_or_default()
        .split(';')
        .filter_map(|t| {
            let v: Vec<i64> = t.split(',').filter_map(|x| x.trim().parse().ok()).collect();
            (v.len() == 3).then(|| (v[0], v[1] as i32, v[2] as i32))
        })
        .collect();
    for (seed, x, z) in cols {
        let p = BiomeProvider::new(
            seed,
            WorldType::Rwg2Continents,
            table.clone(),
            RwgConfig::default(),
        );
        let m = SurfaceModel::new(&p.table);
        let caves = Caves::new(seed, true);
        let mut g = ChunkGen::new(&p);
        let c = g.generate(&p, &TableHeights, x >> 4, z >> 4).unwrap();
        let cc = rwg2_biomes::spawn::carved_chunk(
            &p,
            &TableHeights,
            &m,
            &c,
            x >> 4,
            z >> 4,
            Some(&caves),
        );
        let idx = ((z & 15) * 16 + (x & 15)) as usize;
        let old = chunk_tops(&p, &m, &c, x >> 4, z >> 4)[idx];
        let new = chunk_tops_carved(&p, &m, &c, x >> 4, z >> 4, &cc)[idx];
        let k = c.height[idx] as i32;
        let col: String = (55..=k + 2)
            .rev()
            .map(|y| if cc.air(x & 15, y, z & 15) { '.' } else { '#' })
            .collect();
        eprintln!(
            "seed {seed} ({x},{z}) biome {} h {:.2} climb {:?} runs {:?} old {old:?} new {new:?}\n   col {}..55: {col}",
            p.table.info(c.rwg[idx]).name,
            c.height[idx],
            cc.bop_climb(x & 15, z & 15),
            cc.runs(x & 15, z & 15),
            k + 2
        );
    }
}

/// Per RWG biome: when the cave model cuts into the top 4 layers of a column (no hard layers), does the game
/// show the cut (real top < (int) height)? Used to find surfaces whose painted top/filler caves cannot dig.
#[test]
#[ignore]
fn fit_carvable_top() {
    use std::collections::BTreeMap;
    let table = table();
    let mut stats: BTreeMap<String, (usize, usize)> = BTreeMap::new();
    for (seed, dir) in dumps() {
        let Ok(js) = std::fs::read_to_string(dir.join("worldchunks.json")) else {
            continue;
        };
        let j: Value = serde_json::from_str(&js).unwrap();
        let Ok(bin) = std::fs::read(dir.join(j["file"].as_str().unwrap())) else {
            continue;
        };
        let (cx0, cz0) = (
            j["chunkX0"].as_i64().unwrap() as i32,
            j["chunkZ0"].as_i64().unwrap() as i32,
        );
        let (w, hh) = (
            j["chunksW"].as_i64().unwrap() as i32,
            j["chunksH"].as_i64().unwrap() as i32,
        );
        let n = (w * hh * 256) as usize;
        let top = &bin[n * 2..n * 3];
        let p = BiomeProvider::new(
            seed,
            WorldType::Rwg2Continents,
            table.clone(),
            RwgConfig::default(),
        );
        let caves = Caves::new(seed, true);
        let mut g = ChunkGen::new(&p);
        let m = rwg2_biomes::spawn::SurfaceModel::new(&p.table);
        for ci in 0..(w * hh) {
            let (cx, cz) = (cx0 + ci % w, cz0 + ci / w);
            let c = g.generate(&p, &TableHeights, cx, cz).unwrap();
            let tops = rwg2_biomes::spawn::chunk_tops(&p, &m, &c, cx, cz);
            let soft = CarvedChunk::new(&p, &TableHeights, &c, cx, cz, Some(&caves));
            for idx in 0..256 {
                let (lx, lz) = ((idx & 15) as i32, (idx >> 4) as i32);
                let k = c.height[idx] as i32;
                if k < 64 || !c.river_cave[idx].is_none() || soft.lava_carved(lx, lz) {
                    continue;
                }
                let mt = soft.top(lx, lz);
                if mt >= k || mt < k - 4 {
                    continue; // model cave doesn't reach the top layers (or digs deeper than the band)
                }
                let byte = top[ci as usize * 256 + idx];
                let real = if byte == 0xff { -1 } else { byte as i32 };
                let t = tops[idx];
                let cls = if t.grass >= 0.5 {
                    "grass"
                } else if t.stone >= 0.5 {
                    "stone"
                } else if t.sand >= 0.5 {
                    "sand"
                } else {
                    "other"
                };
                let pg = p.biome_gen_id(cx * 16 + lx, cz * 16 + lz, &TableHeights);
                let pname = p
                    .table
                    .biomes
                    .iter()
                    .find(|b| b.gen_id == pg)
                    .map(|b| b.name.clone())
                    .unwrap_or_default();
                let e = stats
                    .entry(format!(
                        "{} [{cls}] prov {pname}",
                        p.table.info(c.rwg[idx]).name
                    ))
                    .or_default();
                if real < k {
                    e.0 += 1;
                } else {
                    e.1 += 1;
                }
            }
        }
    }
    for (name, (cut, blocked)) in &stats {
        eprintln!("{name:40} cut {cut:4} blocked {blocked:4}");
    }
}
