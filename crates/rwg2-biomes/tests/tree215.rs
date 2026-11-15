#![cfg(feature = "json")]
#![allow(dead_code, unused_imports)]
//! rwg2dump 2.15 `blocks215.json` block areas (full columns, id + metadata, before and after population):
//! overworld pre-population blocks incl. metadata vs `surface_chunk` (stained clay, red sand, coarse dirt, podzol,
//! filler layers), the population snow / ice pass vs the populated chunks, and Nether / TF pre-population block
//! names vs `dimterrain`. Skips when no 2.15 dump is present. The `.bin.gz` files must be gunzipped next to
//! themselves first (`gunzip -k blocks215_*.bin.gz`; the crate has no inflate).
use rwg2_biomes::chunkgen::ChunkGen;
use rwg2_biomes::config::RwgConfig;
use rwg2_biomes::dimterrain::{nb, tb, NetherTerrain, TfTerrain};
use rwg2_biomes::gtorder::Caves;
use rwg2_biomes::provider::{BiomeProvider, BiomeTable, TableHeights, WorldType};
use rwg2_biomes::surface::{
    apply_snow_cover, blk, block_label, block_meta, block_name, surface_chunk, SurfaceTable,
};
use serde_json::Value;
use std::collections::BTreeMap;
use std::path::PathBuf;

fn root() -> PathBuf {
    std::env::var("RWG2_DATA_DIR")
        .map(|d| PathBuf::from(d).parent().unwrap().to_path_buf())
        .unwrap_or_else(|_| PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../.."))
}

fn dumps() -> Vec<PathBuf> {
    fn walk(d: &std::path::Path, out: &mut Vec<PathBuf>) {
        let Ok(rd) = std::fs::read_dir(d) else { return };
        for e in rd.flatten() {
            let p = e.path();
            if p.is_dir() {
                walk(&p, out);
            } else if p.file_name().is_some_and(|n| n == "blocks215.json") {
                out.push(p.parent().unwrap().to_path_buf());
            }
        }
    }
    let mut v = Vec::new();
    walk(&root().join("testdata/pack"), &mut v);
    v.sort();
    v
}

/// One dumped area: `pre`/`post` (block index, meta) per chunk, column, y.
struct Area {
    name: String,
    dim: i64,
    cx0: i32,
    cz0: i32,
    w: i32,
    bin: Vec<u8>,
}

impl Area {
    fn n(&self) -> usize {
        (self.w * self.w) as usize * 65536
    }
    /// (block index, meta) at chunk ci, column idx (lz*16+lx), y; `post` selects the populated pass.
    fn get(&self, post: bool, ci: usize, idx: usize, y: usize) -> (usize, u8) {
        let n = self.n();
        let base = if post { n * 3 } else { 0 };
        let o = ci * 65536 + idx * 256 + y;
        let id = u16::from_le_bytes([self.bin[base + o * 2], self.bin[base + o * 2 + 1]]) as usize;
        (id, self.bin[base + n * 2 + o])
    }
}

fn load() -> Vec<(i64, String, Vec<String>, Vec<Area>)> {
    let mut out = Vec::new();
    for d in dumps() {
        let j: Value =
            serde_json::from_str(&std::fs::read_to_string(d.join("blocks215.json")).unwrap())
                .unwrap();
        let seed: i64 = j["worldSeed"]
            .as_str()
            .and_then(|s| s.parse().ok())
            .unwrap_or(0);
        let wt = j["worldType"]
            .as_str()
            .unwrap_or("RWG2_CONTINENTS")
            .to_string();
        let names: Vec<String> = j["blocks"]
            .as_array()
            .map(|a| {
                a.iter()
                    .map(|v| v.as_str().unwrap_or("?").to_string())
                    .collect()
            })
            .unwrap_or_default();
        let mut areas = Vec::new();
        for a in j["areas"].as_array().into_iter().flatten() {
            let Some(f) = a["file"].as_str() else {
                eprintln!("{}: area {} failed: {}", d.display(), a["name"], a["error"]);
                continue;
            };
            let raw = d.join(f.trim_end_matches(".gz"));
            let Ok(bin) = std::fs::read(&raw) else {
                eprintln!("{}: missing {} (gunzip -k {f})", d.display(), raw.display());
                continue;
            };
            areas.push(Area {
                name: a["name"].as_str().unwrap_or("?").into(),
                dim: a["dim"].as_i64().unwrap_or(0),
                cx0: a["chunkX0"].as_i64().unwrap() as i32,
                cz0: a["chunkZ0"].as_i64().unwrap() as i32,
                w: a["chunks"].as_i64().unwrap() as i32,
                bin,
            });
        }
        out.push((seed, wt, names, areas));
    }
    out
}

fn top_of(a: &Area, names: &[String], post: bool, ci: usize, idx: usize) -> i32 {
    (0..256)
        .rev()
        .find(|&y| {
            names.get(a.get(post, ci, idx, y).0).map(String::as_str) != Some("minecraft:air")
        })
        .map_or(-1, |y| y as i32)
}

#[test]
#[ignore]
fn tree_census_215() {
    let data = load();
    let table = BiomeTable::from_rwg2dump_json(
        &std::fs::read_to_string(root().join("data/gtnh-2.9.0/biomes.json")).unwrap(),
    )
    .unwrap();
    for (seed, _wt, names, areas) in &data {
        let p = BiomeProvider::new(
            *seed,
            WorldType::Rwg2Continents,
            table.clone(),
            RwgConfig::default(),
        );
        for a in areas.iter().filter(|a| a.dim == 0) {
            eprintln!("== area {} ({}, {}) {}²", a.name, a.cx0, a.cz0, a.w);
            let mut blocks: BTreeMap<String, usize> = BTreeMap::new();
            let mut decos: BTreeMap<String, usize> = BTreeMap::new();
            for ci in 0..(a.w * a.w) as usize {
                let (cx, cz) = (a.cx0 + ci as i32 % a.w, a.cz0 + ci as i32 / a.w);
                let mut cnt: BTreeMap<u16, i32> = BTreeMap::new();
                for bx in -4..=4 {
                    for bz in -4..=4 {
                        *cnt.entry(
                            p.biome_data(
                                cx * 16 + 24 + bx * 16,
                                cz * 16 + 24 + bz * 16,
                                &TableHeights,
                            )
                            .data,
                        )
                        .or_default() += 1;
                    }
                }
                let list: Vec<String> = cnt
                    .iter()
                    .map(|(&id, &c)| {
                        let i = p.table.info(id as _);
                        format!(
                            "{}#{}[{}]{}",
                            i.name,
                            id,
                            c,
                            i.class.rsplit('.').next().unwrap_or("")
                        )
                    })
                    .collect();
                *decos.entry(list.join(" + ")).or_default() += 1;
                for idx in 0..256 {
                    for y in 60..200 {
                        let (pid, pm) = a.get(false, ci, idx, y);
                        let (id, m) = a.get(true, ci, idx, y);
                        if id != pid || m != pm {
                            let n = names.get(id).map(String::as_str).unwrap_or("?");
                            *blocks.entry(format!("{n}:{m}")).or_default() += 1;
                        }
                    }
                }
            }
            let mut v: Vec<_> = blocks.into_iter().collect();
            v.sort_by_key(|e| std::cmp::Reverse(e.1));
            eprintln!(
                "  changed blocks y60..199 (post != pre): {:?}",
                &v[..v.len().min(60)]
            );
            let mut d: Vec<_> = decos.into_iter().collect();
            d.sort_by_key(|e| std::cmp::Reverse(e.1));
            for (k, c) in d.iter().take(8) {
                eprintln!("  {c:3} chunks: {k}");
            }
        }
    }
}

/// Key assumption check: the populate Random at the border-loop start. Stone Mountains Cactus (#23, first in id
/// order in the red area) starts with a cobblestone `DecoBlob` at (x + nextInt(16) + 8, z + nextInt(16) + 8).
#[test]
#[ignore]
fn deco_start_cobble_215() {
    use rwg2_biomes::population::PopModel;
    let data = load();
    let table = BiomeTable::from_rwg2dump_json(
        &std::fs::read_to_string(root().join("data/gtnh-2.9.0/biomes.json")).unwrap(),
    )
    .unwrap();
    for (seed, _wt, names, areas) in &data {
        let p = BiomeProvider::new(
            *seed,
            WorldType::Rwg2Continents,
            table.clone(),
            RwgConfig::default(),
        );
        let h = TableHeights;
        let cobble = names.iter().position(|n| n == "minecraft:cobblestone");
        for a in areas.iter().filter(|a| a.dim == 0 && a.name == "red") {
            let mut m = PopModel::new(&p, &h, *seed);
            m.load_structures(a.cx0 - 2, a.cz0 - 2, a.cx0 + a.w + 2, a.cz0 + a.w + 2);
            // real cobblestone added by population at world (x, z)?
            let real = |x: i32, z: i32| -> bool {
                let (cx, cz) = (x.div_euclid(16) - a.cx0, z.div_euclid(16) - a.cz0);
                if cx < 0 || cz < 0 || cx >= a.w || cz >= a.w {
                    return false;
                }
                let ci = (cz * a.w + cx) as usize;
                let idx = (z.rem_euclid(16) * 16 + x.rem_euclid(16)) as usize;
                (40..200).any(|y| {
                    Some(a.get(true, ci, idx, y).0) == cobble
                        && Some(a.get(false, ci, idx, y).0) != cobble
                })
            };
            let near = |x: i32, z: i32| (-1..=1).any(|dx| (-1..=1).any(|dz| real(x + dx, z + dz)));
            let (mut n, mut hit, mut ctrl, mut live, mut agree) = (0, 0, 0, 0, 0);
            for ci in 0..(a.w * a.w) {
                let (cx, cz) = (a.cx0 + ci % a.w, a.cz0 + ci / a.w);
                if cx == a.cx0 + a.w - 1 || cz == a.cz0 + a.w - 1 {
                    continue; // decorates into the next chunk, outside the area
                }
                let _ = m.populate(cx, cz);
                n += 1;
                let Some((mut r, _)) = m.last_deco_start.clone() else {
                    continue;
                };
                live += 1;
                let first = {
                    let mut ids = std::collections::BTreeSet::new();
                    for bx in -4..=4 {
                        for bz in -4..=4 {
                            ids.insert(
                                p.biome_data(cx * 16 + 24 + bx * 16, cz * 16 + 24 + bz * 16, &h)
                                    .data,
                            );
                        }
                    }
                    *ids.iter().next().unwrap()
                };
                if !p
                    .table
                    .info(first as _)
                    .class
                    .contains("StoneMountainsCactus")
                {
                    continue;
                }
                let x = cx * 16 + r.next_int_bounded(16) + 8;
                let z = cz * 16 + r.next_int_bounded(16) + 8;
                let ok = near(x, z);
                // getHeightValue on the pre-population column (+ earlier decorations ignored)
                let hv = {
                    let (acx, acz) = (x.div_euclid(16) - a.cx0, z.div_euclid(16) - a.cz0);
                    let ci = (acz * a.w + acx) as usize;
                    let idx = (z.rem_euclid(16) * 16 + x.rem_euclid(16)) as usize;
                    top_of(a, names, false, ci, idx) + 1
                };
                let expect = hv < 80;
                if expect == ok {
                    agree += 1
                }
                hit += ok as i32;
                // control: the same chunk, a position from the wrong draw
                let x2 = cx * 16 + r.next_int_bounded(16) + 8;
                let z2 = cz * 16 + r.next_int_bounded(16) + 8;
                ctrl += near(x2, z2) as i32;
                eprintln!("  chunk ({cx},{cz}) blob at ({x},{z}) h {hv} expect {expect} real cobble near: {ok}");
            }
            eprintln!("red: chunks {n}, deco start known {live}, predicted blob has real cobble {hit} (control {ctrl}), agrees with h<80 rule {agree}");
        }
    }
}

/// Trees step 1 proof of concept: replay the populate prefix + the ported `rDecorate`s (Stone Mountains Cactus,
/// Canyon) on the exact block world, compare decoration blocks with the real populated world.
#[test]
#[ignore]
fn deco_poc_215() {
    use rwg2_biomes::deco::{self, db, DecoWorld};
    use rwg2_biomes::population::{Outcome, PopModel};
    use std::collections::{BTreeSet, HashMap};
    let data = load();
    let table = BiomeTable::from_rwg2dump_json(
        &std::fs::read_to_string(root().join("data/gtnh-2.9.0/biomes.json")).unwrap(),
    )
    .unwrap();
    const NAMES: [&str; 22] = [
        "BiomesOPlenty:foliage",
        "BiomesOPlenty:flowers",
        "BiomesOPlenty:flowers2",
        "BiomesOPlenty:mushrooms",
        "BiomesOPlenty:plants",
        "Botania:flower",
        "Botania:doubleFlower1",
        "Botania:doubleFlower2",
        "minecraft:log",
        "minecraft:leaves",
        "minecraft:log2",
        "minecraft:leaves2",
        "minecraft:cactus",
        "minecraft:tallgrass",
        "minecraft:double_plant",
        "minecraft:deadbush",
        "minecraft:reeds",
        "minecraft:pumpkin",
        "minecraft:yellow_flower",
        "minecraft:red_flower",
        "minecraft:cobblestone",
        "minecraft:mossy_cobblestone",
    ];
    for (seed, _wt, names, areas) in &data {
        let p = BiomeProvider::new(
            *seed,
            WorldType::Rwg2Continents,
            table.clone(),
            RwgConfig::default(),
        );
        let h = TableHeights;
        for a in areas.iter().filter(|a| a.dim == 0) {
            let mut m = PopModel::new(&p, &h, *seed);
            m.load_structures(a.cx0 - 2, a.cz0 - 2, a.cx0 + a.w + 2, a.cz0 + a.w + 2);
            let mut w = DecoWorld::new(&p, &h, *seed);
            let mut modelled: BTreeSet<(i32, i32)> = BTreeSet::new();
            let mut why: std::collections::BTreeMap<String, usize> = Default::default();
            let mut deco_writes: HashMap<(i32, i32, i32), u16> = HashMap::new();
            for ci in 0..(a.w * a.w) {
                let (cx, cz) = (a.cx0 + ci % a.w, a.cz0 + ci / a.w);
                let river = -p.river_strength(cx * 16 + 16, cz * 16 + 16);
                let out = m.populate(cx, cz);
                let Some((r0, n)) = m.last_deco_start.clone() else {
                    *why.entry("prefix not live".into()).or_default() += 1;
                    continue;
                };
                let log = match out {
                    Outcome::Certain(l) | Outcome::Uncertain(_, l) => l,
                };
                deco::apply_prefix(&mut w, &log[..n]);
                let mut r = r0;
                let s0 = w.log.len();
                let dbg = std::env::var("DBG_CHUNK").ok() == Some(format!("{cx},{cz}"));
                w.tracing = dbg;
                w.trace.clear();
                let mut ok = true;
                for (id, s) in deco::border(&p, &h, cx, cz) {
                    let info = p.table.info(id as _);
                    let cl = &info.class;
                    if cl.contains("RealisticBiomeOcean") {
                        continue;
                    }
                    let bopish = info.class.ends_with("RealisticBiomeSupport")
                        || info.class.ends_with("RealisticBiomeMountainChain");
                    if (dbg && std::env::var("SCAN").is_ok())
                        || (std::env::var("SYNC").is_ok() && bopish && s > 0.3)
                    {
                        // offset of the shared Random (LCG steps) that best explains the real ore blobs
                        let mut best = (0usize, 0i64, 0usize);
                        let span: i64 = std::env::var("SPAN")
                            .ok()
                            .and_then(|v| v.parse().ok())
                            .unwrap_or(4000);
                        let mut t0 = r.clone();
                        t0.rewind(span as u32);
                        for d in -span..span {
                            t0.next(1);
                            let t = t0.clone();
                            let d = d + 1;
                            let pts = deco::bop::ore_probe(&t, cx, cz, 6);
                            let (mut hit, mut tot) = (0, 0);
                            for &(x, y, z, dirt) in &pts {
                                if !(40..200).contains(&y) {
                                    continue;
                                }
                                let (acx, acz) =
                                    (x.div_euclid(16) - a.cx0, z.div_euclid(16) - a.cz0);
                                if acx < 0 || acz < 0 || acx >= a.w || acz >= a.w {
                                    continue;
                                }
                                let ci = (acz * a.w + acx) as usize;
                                let idx = (z.rem_euclid(16) * 16 + x.rem_euclid(16)) as usize;
                                let (pid, _) = a.get(false, ci, idx, y as usize);
                                if names.get(pid).map(String::as_str) != Some("minecraft:stone") {
                                    continue;
                                }
                                tot += 1;
                                let (id, _) = a.get(true, ci, idx, y as usize);
                                let want = if dirt {
                                    "minecraft:dirt"
                                } else {
                                    "minecraft:gravel"
                                };
                                if names.get(id).map(String::as_str) == Some(want) {
                                    hit += 1;
                                }
                            }
                            if hit > best.0 {
                                best = (hit, d, tot);
                            }
                        }
                        if dbg {
                            eprintln!(
                                "    SCAN before {} (id {id}, s {s:.2}): best offset {} hits {}/{}",
                                info.name, best.1, best.0, best.2
                            );
                        }
                        if std::env::var("SYNC").is_ok() && best.2 > 40 && best.0 * 10 > best.2 * 7
                        {
                            let d = best.1;
                            if d < 0 {
                                r.rewind((-d) as u32)
                            } else {
                                for _ in 0..d {
                                    r.next(1);
                                }
                            }
                            *why.entry(format!("synced {}", if d == 0 { "0" } else { "moved" }))
                                .or_default() += 1;
                        } else if std::env::var("SYNC").is_ok() {
                            *why.entry("sync failed".into()).or_default() += 1;
                        }
                    }
                    if !deco::decorate_biome(&mut w, &mut r, id, cx, cz, s, river) {
                        *why.entry(format!(
                            "not ported: {} {} {}",
                            info.name,
                            cl.rsplit('.').next().unwrap(),
                            info.base_class.rsplit('.').next().unwrap()
                        ))
                        .or_default() += 1;
                        ok = false;
                        break;
                    }
                }
                if ok {
                    deco::snow_pass(&mut w, cx, cz, &deco::border(&p, &h, cx, cz));
                }
                if dbg {
                    let mut realv: std::collections::BTreeMap<String, Vec<(i32, i32, i32)>> =
                        Default::default();
                    for x in cx * 16 + 1..cx * 16 + 31 {
                        for z in cz * 16 + 1..cz * 16 + 31 {
                            let (acx, acz) = (x.div_euclid(16) - a.cx0, z.div_euclid(16) - a.cz0);
                            if acx < 0 || acz < 0 || acx >= a.w || acz >= a.w {
                                continue;
                            }
                            let ci = (acz * a.w + acx) as usize;
                            let idx = (z.rem_euclid(16) * 16 + x.rem_euclid(16)) as usize;
                            for y in 50..200usize {
                                let (pid, pm) = a.get(false, ci, idx, y);
                                let (id, mt) = a.get(true, ci, idx, y);
                                let rn = names.get(id).map(String::as_str).unwrap_or("?");
                                let mt = if rn.starts_with("minecraft:leaves") {
                                    mt & 7
                                } else {
                                    mt
                                };
                                if (id != pid || mt != pm) && NAMES.contains(&rn) {
                                    realv
                                        .entry(format!("{rn}:{mt}"))
                                        .or_default()
                                        .push((x, y as i32, z));
                                }
                            }
                        }
                    }
                    for (k, v) in &realv {
                        eprintln!(
                            "    real {k}: {} e.g. {:?}",
                            v.len(),
                            &v[..v.len().min(
                                if std::env::var("DBG_ALL").is_ok() && k.contains("log") {
                                    999
                                } else {
                                    6
                                }
                            )]
                        );
                    }
                    let tr = w.trace.clone();
                    for (i, &(nm, x, y, z, st)) in tr.iter().enumerate() {
                        let en = tr.get(i + 1).map_or(w.log.len(), |t| t.4);
                        let (mut good, mut bad) = (0, 0);
                        for &(bx, by, bz, b) in &w.log[st..en] {
                            let (acx, acz) = (bx.div_euclid(16) - a.cx0, bz.div_euclid(16) - a.cz0);
                            if acx < 0
                                || acz < 0
                                || acx >= a.w
                                || acz >= a.w
                                || !(0..256).contains(&by)
                            {
                                continue;
                            }
                            let ci = (acz * a.w + acx) as usize;
                            let idx = (bz.rem_euclid(16) * 16 + bx.rem_euclid(16)) as usize;
                            let (id, mt) = a.get(true, ci, idx, by as usize);
                            let rn = names.get(id).map(String::as_str).unwrap_or("?");
                            let mt = if rn.starts_with("minecraft:leaves") {
                                mt & 7
                            } else {
                                mt
                            };
                            if rn == db::name(b) && mt == db::meta(b) {
                                good += 1
                            } else {
                                bad += 1;
                                if bad <= 4 && std::env::var("DBG_MISS").is_ok() {
                                    let (pid, pm) = a.get(false, ci, idx, by as usize);
                                    let below = a.get(true, ci, idx, by as usize - 1);
                                    eprintln!("        miss ({bx},{by},{bz}) model {}:{} real {rn}:{mt} pre {}:{pm} below {}:{}", db::name(b), db::meta(b), names.get(pid).map(String::as_str).unwrap_or("?"), names.get(below.0).map(String::as_str).unwrap_or("?"), below.1);
                                }
                            }
                        }
                        eprintln!("    call {i:3} {nm:9} at ({x},{y},{z}): writes {} match {good} miss {bad}", en - st);
                    }
                }
                if ok {
                    modelled.insert((cx, cz));
                    for &(x, y, z, b) in &w.log[s0..] {
                        deco_writes.insert((x, y, z), b);
                    }
                }
            }
            if let Ok(t) = std::env::var("TOPS") {
                for pr in t.split(';') {
                    let v: Vec<i32> = pr.split(',').map(|q| q.parse().unwrap()).collect();
                    let (x, z) = (v[0], v[1]);
                    let (acx, acz) = (x.div_euclid(16) - a.cx0, z.div_euclid(16) - a.cz0);
                    if acx < 0 || acz < 0 || acx >= a.w || acz >= a.w {
                        continue;
                    }
                    let ci = (acz * a.w + acx) as usize;
                    let idx = (z.rem_euclid(16) * 16 + x.rem_euclid(16)) as usize;
                    let col = |post: bool| {
                        (60..200)
                            .rev()
                            .filter_map(|y| {
                                let (id, m) = a.get(post, ci, idx, y);
                                let n = names.get(id).map(String::as_str).unwrap_or("?");
                                (n != "minecraft:air").then(|| format!("{y}:{n}:{m}"))
                            })
                            .take(4)
                            .collect::<Vec<_>>()
                    };
                    eprintln!(
                        "  TOPS ({x},{z}) pre {:?} post {:?} model h {}",
                        col(false),
                        col(true),
                        w.height(x, z)
                    );
                }
            }
            eprintln!(
                "== area {}: modelled chunks {} / {}; {:?}",
                a.name,
                modelled.len(),
                a.w * a.w,
                why
            );
            if modelled.is_empty() {
                continue;
            }
            // positions whose decorating chunk ((x-8)>>4, (z-8)>>4) is modelled
            let mut tp: std::collections::BTreeMap<String, [usize; 3]> = Default::default(); // [both, real only, model only]
            for ci in 0..(a.w * a.w) as usize {
                let (cx, cz) = (a.cx0 + ci as i32 % a.w, a.cz0 + ci as i32 / a.w);
                for idx in 0..256usize {
                    let (x, z) = (cx * 16 + (idx % 16) as i32, cz * 16 + (idx / 16) as i32);
                    if !modelled.contains(&((x - 8) >> 4, (z - 8) >> 4)) {
                        continue;
                    }
                    for y in 50..200usize {
                        let (pid, pm) = a.get(false, ci, idx, y);
                        let (id, mt) = a.get(true, ci, idx, y);
                        let rn = names.get(id).map(String::as_str).unwrap_or("?");
                        let mt = if rn.starts_with("minecraft:leaves") {
                            mt & 7
                        } else {
                            mt
                        };
                        let real = (id != pid || mt != pm) && NAMES.contains(&rn);
                        let real_key = format!("{rn}:{mt}");
                        let model = deco_writes
                            .get(&(x, y as i32, z))
                            .map(|&b| format!("{}:{}", db::name(b), db::meta(b)));
                        let model =
                            model.filter(|k| NAMES.iter().any(|n| k.starts_with(&format!("{n}:"))));
                        match (real, model) {
                            (true, Some(k)) if k == real_key => tp.entry(k).or_default()[0] += 1,
                            (true, Some(k)) => {
                                tp.entry(real_key).or_default()[1] += 1;
                                tp.entry(k).or_default()[2] += 1;
                            }
                            (true, None) => tp.entry(real_key).or_default()[1] += 1,
                            (false, Some(k)) => tp.entry(k).or_default()[2] += 1,
                            (false, None) => {}
                        }
                    }
                }
            }
            if std::env::var("PER_CHUNK").is_ok() {
                for &(dcx, dcz) in &modelled {
                    let mut c = [0usize; 3];
                    for x in dcx * 16 + 8..dcx * 16 + 24 {
                        for z in dcz * 16 + 8..dcz * 16 + 24 {
                            let (acx, acz) = (x.div_euclid(16) - a.cx0, z.div_euclid(16) - a.cz0);
                            if acx < 0 || acz < 0 || acx >= a.w || acz >= a.w {
                                continue;
                            }
                            let ci = (acz * a.w + acx) as usize;
                            let idx = (z.rem_euclid(16) * 16 + x.rem_euclid(16)) as usize;
                            for y in 50..200usize {
                                let (pid, pm) = a.get(false, ci, idx, y);
                                let (id, mt) = a.get(true, ci, idx, y);
                                let rn = names.get(id).map(String::as_str).unwrap_or("?");
                                let mt = if rn.starts_with("minecraft:leaves") {
                                    mt & 7
                                } else {
                                    mt
                                };
                                let real = (id != pid || mt != pm) && NAMES.contains(&rn);
                                let rk = format!("{rn}:{mt}");
                                let mk = deco_writes
                                    .get(&(x, y as i32, z))
                                    .map(|&b| format!("{}:{}", db::name(b), db::meta(b)))
                                    .filter(|k| {
                                        NAMES.iter().any(|n| k.starts_with(&format!("{n}:")))
                                    });
                                match (real, mk) {
                                    (true, Some(k)) if k == rk => c[0] += 1,
                                    (true, Some(_)) => {
                                        c[1] += 1;
                                        c[2] += 1
                                    }
                                    (true, None) => c[1] += 1,
                                    (false, Some(_)) => c[2] += 1,
                                    _ => {}
                                }
                            }
                        }
                    }
                    let b = deco::border(&p, &h, dcx, dcz)
                        .iter()
                        .map(|(id, s)| format!("{}:{:.2}", id, s))
                        .collect::<Vec<_>>()
                        .join(" ");
                    eprintln!("  chunk ({dcx},{dcz}) river {:.2} both {:4} real-only {:4} model-only {:4}  [{b}]", -p.river_strength(dcx*16+16, dcz*16+16), c[0], c[1], c[2]);
                }
            }
            let (mut b, mut ro, mut mo) = (0, 0, 0);
            for (k, v) in &tp {
                eprintln!(
                    "  {k:32} both {:5}  real only {:5}  model only {:5}",
                    v[0], v[1], v[2]
                );
                b += v[0];
                ro += v[1];
                mo += v[2];
            }
            eprintln!(
                "  TOTAL both {b} real only {ro} model only {mo}: recall {:.3} precision {:.3}",
                b as f64 / (b + ro).max(1) as f64,
                b as f64 / (b + mo).max(1) as f64
            );
        }
    }
}
