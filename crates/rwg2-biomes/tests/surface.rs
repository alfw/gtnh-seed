#![cfg(feature = "json")]
//! `surface.rs` (exact pre-population surface blocks) vs every dumped `worldchunks` world: highest non-air y and
//! its block name per column (rwg2dump records the name only, no metadata, no filler layers).
use rwg2_biomes::chunkgen::ChunkGen;
use rwg2_biomes::config::RwgConfig;
use rwg2_biomes::gtorder::Caves;
use rwg2_biomes::provider::{BiomeProvider, BiomeTable, TableHeights, WorldType};
use rwg2_biomes::surface::{block_name, surface_chunk, SurfaceTable};
use serde_json::Value;
use std::collections::{BTreeMap, HashSet};
use std::path::PathBuf;

fn root() -> PathBuf {
    std::env::var("RWG2_DATA_DIR")
        .map(|d| PathBuf::from(d).parent().unwrap().to_path_buf())
        .unwrap_or_else(|_| PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../.."))
}

/// Every directory with a `worldchunks.json`, one per seed.
fn dumps() -> Vec<(i64, PathBuf)> {
    fn walk(d: &std::path::Path, out: &mut Vec<PathBuf>) {
        let Ok(rd) = std::fs::read_dir(d) else { return };
        for e in rd.flatten() {
            let p = e.path();
            if p.is_dir() {
                walk(&p, out);
            } else if p.file_name().is_some_and(|n| n == "worldchunks.json") {
                out.push(p.parent().unwrap().to_path_buf());
            }
        }
    }
    let mut dirs = Vec::new();
    walk(&root().join("testdata/pack"), &mut dirs);
    dirs.sort();
    let mut seen = HashSet::new();
    let mut out = Vec::new();
    for d in dirs {
        let Ok(js) = std::fs::read_to_string(d.join("worldchunks.json")) else {
            continue;
        };
        let j: Value = serde_json::from_str(&js).unwrap();
        let seed: i64 = j["worldSeed"]
            .as_str()
            .and_then(|s| s.parse().ok())
            .unwrap_or(1337);
        if seen.insert(seed) {
            out.push((seed, d));
        }
    }
    if let Ok(only) = std::env::var("SURFACE_SEEDS") {
        let o: Vec<i64> = only
            .split(',')
            .filter_map(|s| s.trim().parse().ok())
            .collect();
        out.retain(|d| o.contains(&d.0));
    }
    out
}

#[test]
fn surface_tops_match_worldchunks() {
    let table = BiomeTable::from_rwg2dump_json(
        &std::fs::read_to_string(root().join("data/gtnh-2.9.0/biomes.json")).unwrap(),
    )
    .unwrap();
    let st = SurfaceTable::new(&table);
    let guessed: Vec<_> = table
        .biomes
        .iter()
        .filter(|b| st.guessed[b.rwg_id as usize])
        .map(|b| (&b.name, &b.class))
        .collect();
    eprintln!("unported ids: {guessed:?}");
    let (mut total, mut y_ok, mut name_ok, mut name_n) = (0usize, 0usize, 0usize, 0usize);
    // (painting biome, real, model) -> count, over columns with the right top y
    let mut miss: BTreeMap<(String, String, String), usize> = BTreeMap::new();
    let mut ndumps = 0;
    let mut opened: BTreeMap<(String, String), usize> = BTreeMap::new();
    for (seed, dir) in dumps() {
        let j: Value =
            serde_json::from_str(&std::fs::read_to_string(dir.join("worldchunks.json")).unwrap())
                .unwrap();
        let Ok(bin) = std::fs::read(dir.join(j["file"].as_str().unwrap())) else {
            continue;
        };
        ndumps += 1;
        let names: Vec<String> = j["blocks"]
            .as_array()
            .unwrap()
            .iter()
            .map(|v| v.as_str().unwrap().to_string())
            .collect();
        let (cx0, cz0) = (
            j["chunkX0"].as_i64().unwrap() as i32,
            j["chunkZ0"].as_i64().unwrap() as i32,
        );
        let (w, hh) = (
            j["chunksW"].as_i64().unwrap() as i32,
            j["chunksH"].as_i64().unwrap() as i32,
        );
        let n = (w * hh * 256) as usize;
        let ty = &bin[n * 2..n * 3];
        let tb = &bin[n * 3..n * 5];
        let p = BiomeProvider::new(
            seed,
            WorldType::Rwg2Continents,
            table.clone(),
            RwgConfig::default(),
        );
        let caves = Caves::new(seed, true);
        let m = rwg2_biomes::spawn::SurfaceModel::new(&p.table);
        let mut g = ChunkGen::new(&p);
        let (mut dy, mut dn, mut dt) = (0, 0, 0);
        for ci in 0..(w * hh) {
            let (cx, cz) = (cx0 + ci % w, cz0 + ci / w);
            let c = g.generate(&p, &TableHeights, cx, cz).unwrap();
            let cc =
                rwg2_biomes::spawn::carved_chunk(&p, &TableHeights, &m, &c, cx, cz, Some(&caves));
            let s = surface_chunk(&p, &TableHeights, &st, &c, cx, cz, &cc);
            for idx in 0..256usize {
                let o = ci as usize * 256 + idx;
                let ry = if ty[o] == 0xff { -1 } else { ty[o] as i32 };
                let rb = u16::from_le_bytes([tb[o * 2], tb[o * 2 + 1]]) as usize;
                let rname = names.get(rb).map(String::as_str).unwrap_or("?");
                total += 1;
                dt += 1;
                if s.top[idx] as i32 != ry {
                    continue;
                }
                y_ok += 1;
                dy += 1;
                name_n += 1;
                let mname = block_name(s.top_block(idx));
                if cc.cave_dug((idx & 15) as i32, ry + 1, (idx >> 4) as i32) {
                    *opened
                        .entry((rname.to_string(), mname.to_string()))
                        .or_default() += 1;
                }
                if mname == rname {
                    name_ok += 1;
                    dn += 1;
                } else {
                    let b = &p.table.biomes[c.rwg[idx] as usize];
                    let (lx, lz) = ((idx & 15) as i32, (idx >> 4) as i32);
                    let dug = cc.cave_dug(lx, ry + 1, lz);
                    let who = format!(
                        "{} ({}) dug_above={dug}",
                        b.name,
                        b.class.rsplit('.').next().unwrap_or("")
                    );
                    *miss
                        .entry((who, rname.to_string(), mname.to_string()))
                        .or_default() += 1;
                }
            }
        }
        eprintln!(
            "seed {seed}: top y {dy}/{dt} ({:.2}%), block {dn}/{dy} ({:.2}%)",
            100.0 * dy as f64 / dt as f64,
            100.0 * dn as f64 / dy.max(1) as f64
        );
    }
    if ndumps == 0 {
        eprintln!("no worldchunks dumps; skipped");
        return;
    }
    let mut v: Vec<_> = miss.into_iter().collect();
    v.sort_by_key(|e| std::cmp::Reverse(e.1));
    for ((who, real, model), k) in v.iter().take(40) {
        eprintln!("  {k:6}  {who}: real {real}, model {model}");
    }
    eprintln!("cave-opened tops (real, model): {opened:?}");
    let yr = y_ok as f64 / total as f64;
    let nr = name_ok as f64 / name_n as f64;
    eprintln!(
        "ALL {ndumps} worlds: top y {y_ok}/{total} ({:.3}%), block {name_ok}/{name_n} ({:.3}%)",
        yr * 100.0,
        nr * 100.0
    );
    assert!(yr > 0.9999 && nr > 0.99999, "surface blocks regressed");
}

/// Cost per 256-chunk tile: chunk generation alone vs + carve + caves + surface pass.
#[test]
#[ignore]
fn surface_speed() {
    let table = BiomeTable::from_rwg2dump_json(
        &std::fs::read_to_string(root().join("data/gtnh-2.9.0/biomes.json")).unwrap(),
    )
    .unwrap();
    let st = SurfaceTable::new(&table);
    let p = BiomeProvider::new(1337, WorldType::Rwg2Continents, table, RwgConfig::default());
    let m = rwg2_biomes::spawn::SurfaceModel::new(&p.table);
    let caves = Caves::new(1337, true);
    let mut g = ChunkGen::new(&p);
    for (label, full, cv) in [
        ("gen only", false, false),
        ("+ surface, no caves", true, false),
        ("+ surface + caves", true, true),
    ] {
        let t = std::time::Instant::now();
        let mut k = 0usize;
        for cz in 40..56 {
            for cx in 40..56 {
                let c = g.generate(&p, &TableHeights, cx, cz).unwrap();
                if full {
                    let cc = rwg2_biomes::spawn::carved_chunk(
                        &p,
                        &TableHeights,
                        &m,
                        &c,
                        cx,
                        cz,
                        cv.then_some(&caves),
                    );
                    k += surface_chunk(&p, &TableHeights, &st, &c, cx, cz, &cc).top[0] as usize;
                }
            }
        }
        eprintln!(
            "{label}: {:.1} ms / 256 chunks ({k})",
            t.elapsed().as_secs_f64() * 1e3
        );
    }
    let cs: Vec<_> = (0..256)
        .map(|i| (40 + i % 16, 40 + i / 16))
        .map(|(cx, cz)| (cx, cz, g.generate(&p, &TableHeights, cx, cz).unwrap()))
        .collect();
    let t = std::time::Instant::now();
    let ccs: Vec<_> = cs
        .iter()
        .map(|(cx, cz, c)| {
            rwg2_biomes::spawn::carved_chunk(&p, &TableHeights, &m, c, *cx, *cz, None)
        })
        .collect();
    eprintln!("carved_chunk: {:.1} ms", t.elapsed().as_secs_f64() * 1e3);
    let t = std::time::Instant::now();
    for ((cx, cz, c), cc) in cs.iter().zip(&ccs) {
        surface_chunk(&p, &TableHeights, &st, c, *cx, *cz, cc);
    }
    eprintln!("surface_chunk: {:.1} ms", t.elapsed().as_secs_f64() * 1e3);
}

/// Population snow / ice (`apply_snow_cover`) vs every dumped spawn walk (`spawn.json`: the block the BOP climb
/// from y 63 ends on in the populated world). Real snow = `now` is a snow layer; a plant / leaves / other ground =
/// no snow (the layer replaces tall grass and sits on tree tops); air at y 63 is skipped.
#[test]
fn snow_cover_vs_spawn_walks() {
    let table = BiomeTable::from_rwg2dump_json(
        &std::fs::read_to_string(root().join("data/gtnh-2.9.0/biomes.json")).unwrap(),
    )
    .unwrap();
    let clim = rwg2_biomes::forestry::ClimateTable::from_json(
        &std::fs::read_to_string(root().join("data/gtnh-2.9.0/climate.json")).unwrap(),
    )
    .unwrap();
    let st = SurfaceTable::new(&table);
    let mut files = Vec::new();
    fn walk(d: &std::path::Path, out: &mut Vec<PathBuf>) {
        let Ok(rd) = std::fs::read_dir(d) else { return };
        for e in rd.flatten() {
            let p = e.path();
            if p.is_dir() {
                walk(&p, out);
            } else if p.file_name().is_some_and(|n| n == "spawn.json") {
                out.push(p);
            }
        }
    }
    walk(&root().join("testdata/pack"), &mut files);
    files.sort();
    let mut seen = HashSet::new();
    // (real snow, model snow) counts
    let mut conf = [[0usize; 2]; 2];
    for f in files {
        let j: Value = serde_json::from_str(&std::fs::read_to_string(&f).unwrap()).unwrap();
        let seed: i64 = j["seed"].as_str().and_then(|s| s.parse().ok()).unwrap_or(0);
        if !seen.insert(seed) || j["worldType"].as_str() != Some("RWG2_CONTINENTS") {
            continue;
        }
        let Some(steps) = j["walk"].as_array() else {
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
        let mut wc = [[0usize; 2]; 2];
        for s in steps {
            if s["populated"].as_bool() != Some(true) {
                continue;
            }
            let now = s["now"].as_str().unwrap_or("");
            if now == "minecraft:air" || now.is_empty() {
                continue;
            }
            let (x, z) = (
                s["x"].as_i64().unwrap() as i32,
                s["z"].as_i64().unwrap() as i32,
            );
            let (cx, cz) = (x >> 4, z >> 4);
            let c = g.generate(&p, &TableHeights, cx, cz).unwrap();
            let cc =
                rwg2_biomes::spawn::carved_chunk(&p, &TableHeights, &m, &c, cx, cz, Some(&caves));
            let mut sc = surface_chunk(&p, &TableHeights, &st, &c, cx, cz, &cc);
            rwg2_biomes::surface::apply_snow_cover(
                &p,
                &TableHeights,
                &|g| clim.get(g).map(|b| b.temperature),
                &c,
                &mut sc,
                cx,
                cz,
            );
            let col = &sc.cols[((z & 15) * 16 + (x & 15)) as usize];
            if col[63] == rwg2_biomes::surface::blk::AIR {
                continue;
            }
            let mut y = 63;
            while y < 255 && col[y + 1] != rwg2_biomes::surface::blk::AIR {
                y += 1;
            }
            let model = col[y] == rwg2_biomes::surface::blk::SNOW_LAYER;
            let real = now == "minecraft:snow_layer";
            wc[real as usize][model as usize] += 1;
        }
        eprintln!("seed {seed}: real/model snow [[no/no, no/yes], [yes/no, yes/yes]] = {wc:?}");
        for a in 0..2 {
            for b in 0..2 {
                conf[a][b] += wc[a][b];
            }
        }
    }
    eprintln!("ALL: {conf:?}");
    let wrong = conf[0][1] + conf[1][0];
    let total: usize = conf.iter().flatten().sum();
    assert!(
        total == 0 || wrong * 50 <= total,
        "snow cover regressed: {conf:?}"
    );
}
