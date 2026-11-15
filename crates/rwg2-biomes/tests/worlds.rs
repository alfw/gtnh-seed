//! Held-out validation: every pack check re-run on the fresh GTNH 2.9.0 worlds the owner generated with rwg2dump
//! 2.9.0 (`testdata/pack/gtnh-2.9.0/worlds/<seed>/`, RWG2_CONTINENTS, never used for tuning).
//!
//! Prints a per-world scorecard plus an aggregate table. Only exact parts are asserted (seed maths, bit-exact
//! terrain, deterministic RNG replays, structure layouts); model estimates are reported and compared with the two
//! tuned worlds (1337, 92900795).
//!
//! `cargo test --release -p rwg2-biomes --features json --test worlds -- --nocapture`
//! Optional: `WORLDS=seed1,seed2` restricts the run.
#![cfg(feature = "json")]

use rwg2_biomes::chunkgen::ChunkGen;
use rwg2_biomes::config::RwgConfig;
use rwg2_biomes::dungeons::*;
use rwg2_biomes::forestry::{
    humidity_class, temperature_class, ClimateTable, Humidity, Temperature, TemperatureNoise,
};
use rwg2_biomes::gtores::*;
use rwg2_biomes::provider::{BiomeProvider, BiomeTable, TableHeights, WorldType};
use serde_json::Value;
use std::cell::RefCell;
use std::collections::{BTreeMap, BTreeSet};
use std::fmt::Write as _;
use std::path::PathBuf;

/// The world under test (per thread).
#[derive(Clone)]
struct Ctx {
    seed: i64,
    spawn: (i32, i32),
    dir: PathBuf,
}
thread_local! {
    static CTX: RefCell<Option<Ctx>> = const { RefCell::new(None) };
}
fn ctx() -> Ctx {
    CTX.with(|c| c.borrow().clone().expect("world context"))
}
fn seed() -> i64 {
    ctx().seed
}
fn spawn() -> (i32, i32) {
    ctx().spawn
}
fn wdir() -> PathBuf {
    ctx().dir
}
fn is_seed(v: &Value) -> bool {
    v.as_i64() == Some(seed()) || v.as_str().and_then(|s| s.parse::<i64>().ok()) == Some(seed())
}
fn root() -> PathBuf {
    std::env::var("RWG2_DATA_DIR")
        .map(|d| PathBuf::from(d).parent().unwrap().to_path_buf())
        .unwrap_or_else(|_| PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../.."))
}
fn json(name: &str) -> Option<Value> {
    serde_json::from_str(&std::fs::read_to_string(wdir().join(name)).ok()?).ok()
}
fn table() -> BiomeTable {
    BiomeTable::from_rwg2dump_json(
        &std::fs::read_to_string(root().join("data/gtnh-2.9.0/biomes.json")).unwrap(),
    )
    .unwrap()
}
fn climate() -> ClimateTable {
    ClimateTable::from_json(
        &std::fs::read_to_string(root().join("data/gtnh-2.9.0/climate.json")).unwrap(),
    )
    .unwrap()
}
fn pop_gates() -> rwg2_biomes::population::PreGates {
    let cj: Value = serde_json::from_str(
        &std::fs::read_to_string(root().join("data/gtnh-2.9.0/climate.json")).unwrap(),
    )
    .unwrap();
    rwg2_biomes::population::PreGates::new(cj["biomes"].as_array().unwrap().iter().map(|b| {
        (
            b["id"].as_u64().unwrap() as u16,
            b["name"].as_str().unwrap_or(""),
            b["dict"]
                .as_array()
                .map(|d| d.iter().filter_map(|x| x.as_str()).collect())
                .unwrap_or_default(),
        )
    }))
}
fn provider(wt: WorldType) -> BiomeProvider {
    BiomeProvider::new(seed(), wt, table(), RwgConfig::default())
}
fn i(v: &Value) -> i32 {
    v.as_i64()
        .unwrap_or_else(|| v.as_str().unwrap().parse().unwrap()) as i32
}
fn bits(v: &Value) -> f32 {
    f32::from_bits(u32::from_str_radix(v["bits"].as_str().unwrap(), 16).unwrap())
}
fn parse_aspects(s: &str) -> Vec<(String, u16)> {
    s.split_whitespace()
        .map(|t| {
            let (a, b) = t.split_once(':').unwrap();
            (a.to_string(), b.parse().unwrap())
        })
        .collect()
}

/// One scorecard section: `exact` rows are asserted, `est` rows are informational.
#[derive(Default)]
struct Card {
    name: &'static str,
    rows: Vec<String>,
    fails: Vec<String>,
    log: String,
    /// aggregated across worlds: (metric, hits, total)
    stats: Vec<(String, usize, usize)>,
    hists: Vec<(String, BTreeMap<i32, i32>)>,
}
impl Card {
    fn new(name: &'static str) -> Self {
        Card {
            name,
            ..Default::default()
        }
    }
    fn exact(&mut self, what: &str, ok: bool, detail: impl std::fmt::Display) {
        self.rows.push(format!(
            "  [{}] {what}: {detail}",
            if ok { "EXACT" } else { "FAIL " }
        ));
        if !ok {
            self.fails.push(format!("{}: {what}: {detail}", self.name));
        }
    }
    fn stat(&mut self, what: &str, hits: usize, total: usize) {
        self.stats.push((what.to_string(), hits, total));
    }
    fn hist(&mut self, what: &str, h: &BTreeMap<i32, i32>) {
        self.hists.push((what.to_string(), h.clone()));
    }
    fn est_if(&mut self, what: &str, ok: bool, detail: impl std::fmt::Display) {
        self.rows.push(format!(
            "  [est.{}] {what}: {detail}",
            if ok { " ok" } else { " MISS" }
        ));
    }
    fn est(&mut self, what: &str, detail: impl std::fmt::Display) {
        self.rows.push(format!("  [est.] {what}: {detail}"));
    }
    fn skip(&mut self, what: &str) {
        self.rows.push(format!("  [skip] {what}"));
    }
    fn note(&mut self, s: impl std::fmt::Display) {
        let _ = writeln!(self.log, "    {s}");
    }
}

// ============================================================================================= biome grids

fn sec_grids() -> Card {
    let mut c =
        Card::new("biome grids (getBiomeDataAt / getBiomeGenAt / meta / continent / getNoiseAt)");
    let Some(m) = json("manifest.json") else {
        c.skip("no manifest.json");
        return c;
    };
    let tbl = table();
    for (wt_name, wt) in [
        ("RWG2", WorldType::Rwg2),
        ("RWG2_CONTINENTS", WorldType::Rwg2Continents),
    ] {
        let p = provider(wt);
        let (mut n, mut bd, mut bg, mut bm, mut bc, mut nh, mut bh) =
            (0usize, 0, 0, 0, 0, 0usize, 0);
        let mut found = 0;
        for f in m["files"].as_array().unwrap() {
            let file = f["file"].as_str().unwrap();
            if !file.starts_with("grid_") || !is_seed(&f["seed"]) || f["worldType"] != wt_name {
                continue;
            }
            let Ok(bytes) = std::fs::read(wdir().join(file)) else {
                continue;
            };
            found += 1;
            let (x0, z0, w, h, step) = (
                i(&f["x0"]),
                i(&f["z0"]),
                i(&f["w"]),
                i(&f["h"]),
                i(&f["step"]),
            );
            let cnt = (w * h) as usize;
            let xz = |idx: usize| (x0 + (idx as i32 % w) * step, z0 + (idx as i32 / w) * step);
            if f["region"] == "heights" {
                for idx in 0..cnt {
                    let (x, z) = xz(idx);
                    let want = f32::from_le_bytes(bytes[idx * 4..idx * 4 + 4].try_into().unwrap());
                    let got = p.noise_at(x, z, &TableHeights).unwrap();
                    nh += 1;
                    if got.to_bits() != want.to_bits() {
                        if bh < 4 {
                            c.note(format_args!(
                                "{wt_name} height ({x},{z}) got {got} want {want}"
                            ));
                        }
                        bh += 1;
                    }
                }
                continue;
            }
            let cont = p.continents();
            for idx in 0..cnt {
                let (x, z) = xz(idx);
                let wd = u16::from_le_bytes([bytes[2 * idx], bytes[2 * idx + 1]]);
                let wg =
                    u16::from_le_bytes([bytes[2 * cnt + 2 * idx], bytes[2 * cnt + 2 * idx + 1]]);
                let wm = bytes[4 * cnt + idx];
                let s = p.biome_data(x, z, &TableHeights);
                let g = p.biome_gen_id(x, z, &TableHeights);
                n += 1;
                let bad = (s.data != wd, g != wg, s.meta != wm);
                if (bad.0 || bad.1 || bad.2) && bd + bg + bm < 6 {
                    let nm = |id: u16| {
                        tbl.biomes
                            .get(id as usize)
                            .map(|b| b.name.clone())
                            .unwrap_or_default()
                    };
                    c.note(format_args!(
                        "{wt_name} {} ({x},{z}): data {}({}) want {wd}({}) gen {g} want {wg} meta {} want {wm}",
                        f["region"],
                        s.data,
                        nm(s.data),
                        nm(wd),
                        s.meta
                    ));
                }
                bd += bad.0 as usize;
                bg += bad.1 as usize;
                bm += bad.2 as usize;
                if let Some(cn) = cont {
                    let o = 5 * cnt + idx * 4;
                    let want = f32::from_le_bytes(bytes[o..o + 4].try_into().unwrap());
                    bc += (cn.value(x, z).to_bits() != want.to_bits()) as usize;
                }
            }
        }
        if found == 0 {
            c.skip(&format!("{wt_name}: no grid_{}_* files", seed()));
            continue;
        }
        c.exact(
            &format!("{wt_name} biome grids (near/wide/far)"),
            bd + bg + bm == 0,
            format!("{n} samples: rwgId {bd}, genId {bg}, meta {bm} mismatches"),
        );
        if wt == WorldType::Rwg2Continents {
            c.exact(
                "CONTINENTS getContinentValue",
                bc == 0,
                format!("{n} samples, {bc} mismatches"),
            );
        }
        if nh > 0 {
            c.exact(
                &format!("{wt_name} getNoiseAt grid"),
                bh == 0,
                format!("{nh} samples, {bh} mismatches"),
            );
        }
    }
    c
}

// ============================================================================================= chunk generation

fn sec_chunkgen() -> Card {
    let mut c = Card::new("chunk generation (getNewNoise, provideChunk)");
    let Some(m) = json("manifest.json") else {
        c.skip("no manifest.json");
        return c;
    };
    let t = table();
    for f in m["files"].as_array().unwrap() {
        let name = f["file"].as_str().unwrap();
        if !name.starts_with("chunks_") || !is_seed(&f["seed"]) {
            continue;
        }
        let Ok(bytes) = std::fs::read(wdir().join(name)) else {
            c.skip(&format!("{name} missing"));
            continue;
        };
        let wt = if f["worldType"] == "RWG2" {
            WorldType::Rwg2
        } else {
            WorldType::Rwg2Continents
        };
        let (x0, z0, w, h) = (i(&f["x0"]), i(&f["z0"]), i(&f["w"]), i(&f["h"]));
        let n = (w * h * 256) as usize;
        assert_eq!(bytes.len(), n * 6);
        let p = BiomeProvider::new(seed(), wt, t.clone(), RwgConfig::default());
        let mut g = ChunkGen::new(&p);
        let (mut bad_h, mut bad_b) = (0usize, 0usize);
        for cz in 0..h {
            for cx in 0..w {
                let ch = g
                    .generate(&p, &TableHeights, (x0 >> 4) + cx, (z0 >> 4) + cz)
                    .unwrap();
                for idx in 0..256usize {
                    let k = ((cz * w + cx) as usize) * 256 + idx;
                    let bio = u16::from_le_bytes([bytes[k * 2], bytes[k * 2 + 1]]);
                    let o = n * 2 + k * 4;
                    let hv =
                        f32::from_le_bytes([bytes[o], bytes[o + 1], bytes[o + 2], bytes[o + 3]]);
                    bad_b += (p.table.biomes[ch.rwg[idx] as usize].gen_id != bio) as usize;
                    bad_h += (ch.height[idx].to_bits() != hv.to_bits()) as usize;
                }
            }
        }
        c.exact(
            name,
            bad_h + bad_b == 0,
            format!("{n} columns, height {bad_h}, biome {bad_b} mismatches"),
        );
    }

    // world chunks (provideChunk, pre-population)
    let (Some(wj), Ok(bin)) = (
        json("worldchunks.json"),
        std::fs::read(
            wdir().join(
                json("worldchunks.json")
                    .and_then(|w| w["file"].as_str().map(String::from))
                    .unwrap_or_default(),
            ),
        ),
    ) else {
        c.skip("worldchunks");
        return c;
    };
    let (x0, z0, w) = (i(&wj["chunkX0"]), i(&wj["chunkZ0"]), i(&wj["chunksW"]));
    let blocks: Vec<String> = wj["blocks"]
        .as_array()
        .unwrap()
        .iter()
        .map(|b| b.as_str().unwrap().to_string())
        .collect();
    let n = (w * w * 256) as usize;
    let p = provider(WorldType::Rwg2Continents);
    let sm = rwg2_biomes::spawn::SurfaceModel::new(&p.table);
    let mut g = ChunkGen::new(&p);
    let mut bad_b = 0;
    let mut pairs: BTreeMap<(u16, u16), usize> = BTreeMap::new();
    let mut bad_chunks: BTreeMap<(i32, i32), usize> = BTreeMap::new();
    let mut top_eq = 0;
    let mut gconf = [[0usize; 2]; 2];
    let mut sconf = [[0usize; 2]; 2];
    for cz in 0..w {
        for cx in 0..w {
            let ch = g.generate(&p, &TableHeights, x0 + cx, z0 + cz).unwrap();
            for idx in 0..256usize {
                let k = ((cz * w + cx) as usize) * 256 + idx;
                let real_b = u16::from_le_bytes([bin[k * 2], bin[k * 2 + 1]]);
                if real_b != ch.gen[idx] {
                    bad_b += 1;
                    *pairs.entry((ch.gen[idx], real_b)).or_insert(0usize) += 1;
                    let c0 = (x0 + cx, z0 + cz);
                    *bad_chunks.entry(c0).or_insert(0usize) += 1;
                }
                let top = match bin[n * 2 + k] {
                    255 => -1,
                    v => v as i32,
                };
                let hi = ch.height[idx] as i32;
                top_eq += (top.max(62) == hi.max(62)) as usize;
                let blk = u16::from_le_bytes([bin[n * 3 + k * 2], bin[n * 3 + k * 2 + 1]]) as usize;
                let bn = blocks.get(blk).map(|s| s.as_str()).unwrap_or("?");
                let (x, z) = (
                    (x0 + cx) * 16 + (idx & 15) as i32,
                    (z0 + cz) * 16 + (idx >> 4) as i32,
                );
                let gt = (bn == "minecraft:grass" && top >= 63) as usize;
                gconf[gt]
                    [(rwg2_biomes::spawn::likely_grass_top(&p, &sm, &ch, x, z) >= 0.5) as usize] +=
                    1;
                let st =
                    ((bn == "minecraft:stone" || bn == "minecraft:sand") && top >= 63) as usize;
                sconf[st]
                    [(rwg2_biomes::spawn::spawnable_top(&p, &sm, &ch, x, z) >= 0.5) as usize] += 1;
            }
        }
    }
    if bad_b > 0 {
        // does the plain RWG2 generator reproduce the world (world type mislabelled)?
        let p2 = provider(WorldType::Rwg2);
        let mut g2 = ChunkGen::new(&p2);
        let mut bad2 = 0;
        for cz in 0..w {
            for cx in 0..w {
                let ch = g2.generate(&p2, &TableHeights, x0 + cx, z0 + cz).unwrap();
                for idx in 0..256usize {
                    let k = ((cz * w + cx) as usize) * 256 + idx;
                    bad2 +=
                        (u16::from_le_bytes([bin[k * 2], bin[k * 2 + 1]]) != ch.gen[idx]) as usize;
                }
            }
        }
        c.note(format_args!(
            "same chunks with the plain RWG2 generator: {bad2} biome-array mismatches"
        ));
        let mut pv: Vec<_> = pairs.iter().collect();
        pv.sort_by_key(|x| std::cmp::Reverse(*x.1));
        c.note(format_args!(
            "biome array mismatches (model, real) -> n: {:?}",
            &pv[..pv.len().min(10)]
        ));
        c.note(format_args!(
            "bad chunks ({}): {:?}",
            bad_chunks.len(),
            bad_chunks.iter().take(30).collect::<Vec<_>>()
        ));
    }
    c.exact(
        "world chunk biome arrays (dithered)",
        bad_b == 0,
        format!("{n} columns, {bad_b} mismatches"),
    );
    c.est(
        "real top == max((int) testHeight, 62)",
        format!("{top_eq}/{n} ({:.1}%)", top_eq as f64 * 100.0 / n as f64),
    );
    c.est(
        "spawn surface model: grass top",
        format!(
            "[truth][pred] {gconf:?} -> {:.4}",
            (gconf[0][0] + gconf[1][1]) as f64 / n as f64
        ),
    );
    c.est(
        "spawn surface model: stone/sand top",
        format!(
            "[truth][pred>=0.5] {sconf:?} (real stone/sand tops {})",
            sconf[1][0] + sconf[1][1]
        ),
    );

    // caves (Hodgepodge fastRandom carver) vs pre-population tops
    use rwg2_biomes::gtorder::{Caves, Underground};
    for fast in [true, false] {
        let mut t = Terrain::new(&p, &TableHeights);
        let mut u = Underground::new(&mut t, Some(Caves::new(seed(), fast)), 0);
        let (mut same, mut carved, mut carved_ok) = (0, 0, 0);
        for cz in z0..z0 + w {
            for cx in x0..x0 + w {
                let ci = ((cz - z0) * w + cx - x0) as usize;
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
                            carved_ok += (real == model) as usize;
                        }
                        same += (real == model) as usize;
                    }
                }
            }
        }
        c.est(
            &format!("caves (fastRandom {fast}) top Y"),
            format!("{same}/{n} match; model opens {carved} tops, {carved_ok} exactly right"),
        );
    }
    c
}

// ============================================================================================= forestry

fn sec_forestry() -> Card {
    let mut c = Card::new("Forestry climate");
    let Some(f) = json("forestry.json") else {
        c.skip("no forestry.json");
        return c;
    };
    let noise = TemperatureNoise::new();
    let ts = &f["temperatureSamples"];
    let base = f["biomes"]
        .as_array()
        .unwrap()
        .iter()
        .find(|b| b["id"] == ts["biomeId"])
        .unwrap();
    let t0 = bits(&base["temperature"]);
    let (mut bad, mut n) = (0, 0);
    for s in ts["samples"].as_array().unwrap() {
        let a = s.as_array().unwrap();
        let want = f32::from_bits(u32::from_str_radix(a[3].as_str().unwrap(), 16).unwrap());
        n += 1;
        bad += (noise
            .float_temperature(t0, i(&a[0]), i(&a[1]), i(&a[2]))
            .to_bits()
            != want.to_bits()) as usize;
    }
    c.exact(
        "getFloatTemperature samples",
        bad == 0,
        format!("{n} samples, {bad} mismatches"),
    );
    let (mut bad_c, mut nb) = (0, 0);
    let ct = climate();
    let mut bad_tbl = 0;
    for b in f["biomes"].as_array().unwrap() {
        let hell = b["hellish"].as_bool().unwrap();
        let t = if hell {
            Temperature::Hellish
        } else {
            temperature_class(bits(&b["temperature"]))
        };
        let h = match humidity_class(bits(&b["rainfall"])) {
            Humidity::Arid => "ARID",
            Humidity::Normal => "NORMAL",
            Humidity::Damp => "DAMP",
        };
        nb += 1;
        bad_c += (b["forestryTemperature"] != format!("{t:?}").to_uppercase().as_str()
            || b["forestryHumidity"] != h) as usize;
        // embedded climate.json still matches this instance
        let id = b["id"].as_u64().unwrap() as u16;
        if let Some(e) = ct.get(id) {
            bad_tbl += (e.temperature.to_bits() != bits(&b["temperature"]).to_bits()) as usize;
        }
    }
    c.exact(
        "Forestry classes per biome",
        bad_c == 0,
        format!("{nb} biomes, {bad_c} mismatches"),
    );
    c.exact(
        "data/climate.json temperatures vs this instance",
        bad_tbl == 0,
        format!("{bad_tbl} differ"),
    );
    c
}

// ============================================================================================= dungeons

fn sec_dungeons() -> Card {
    let mut c = Card::new("dungeons (LootGames, Roguelike)");
    let Some(d) = json("dungeons.json") else {
        c.skip("no dungeons.json");
        return c;
    };
    assert!(is_seed(&d["worldSeed"]));
    let want: Vec<(i32, i32)> = d["lootgames"]["dungeons"]
        .as_array()
        .unwrap()
        .iter()
        .map(|e| (i(&e["chunkX"]), i(&e["chunkZ"])))
        .collect();
    let mut got = lootgames_chunks(seed(), 20, -128, -128, 128, 128);
    let mut w2 = want.clone();
    w2.sort_by_key(|&(x, z)| (z, x));
    got.sort_by_key(|&(x, z)| (z, x));
    let missing = w2.iter().filter(|x| !got.contains(x)).count();
    let extra = got.iter().filter(|x| !w2.contains(x)).count();
    c.exact(
        "LootGames chunks (±128)",
        got == w2,
        format!(
            "{} real, {} ours, missing {missing}, extra {extra}",
            w2.len(),
            got.len()
        ),
    );

    let mut wa: Vec<(i32, i32)> = d["roguelike"]["anchors"]
        .as_array()
        .unwrap()
        .iter()
        .map(|e| (i(&e["chunkX"]), i(&e["chunkZ"])))
        .collect();
    let mut ga = rogue_anchors(seed(), &ROGUE_GTNH, -128, -128, 128, 128);
    wa.sort();
    ga.sort();
    c.exact(
        "Roguelike anchors (±128)",
        ga == wa,
        format!("{} real, {} ours", wa.len(), ga.len()),
    );

    let p = provider(WorldType::Rwg2Continents);
    let cl = climate();
    let mut t = Terrain::new(&p, &TableHeights);
    let (mut pos_ok, mut pos_n, mut v_ok, mut v_n, mut fp, mut fn_) = (0, 0, 0, 0, 0, 0);
    let (mut chosen_ok, mut chosen_n, mut ring_ok, mut ring_n, mut no_spot) = (0, 0, 0, 0, 0);
    let mut gy_hist = BTreeMap::<i32, i32>::new();
    for a in d["roguelike"]["anchors"].as_array().unwrap() {
        let Some(att) = a["attempts"].as_array() else {
            continue;
        };
        let (cx, cz) = (i(&a["chunkX"]), i(&a["chunkZ"]));
        let mine = rogue_attempts(seed(), cx, cz);
        let mut want_pos = None;
        let mut want_y = -1;
        for (k, e) in att.iter().enumerate() {
            let e = e.as_array().unwrap();
            let (x, z, ok) = (i(&e[0]), i(&e[1]), e[2].as_bool().unwrap());
            pos_n += 1;
            pos_ok += (mine[k] == (x, z)) as i32;
            let v = rogue_valid(&mut t, &cl, &ROGUE_GTNH, x, z, true).unwrap();
            v_n += 1;
            if v == ok {
                v_ok += 1;
            } else if v {
                fp += 1;
            } else {
                fn_ += 1;
            }
            if ok {
                want_pos = Some((x, z));
                want_y = e.get(3).map(i).unwrap_or(-1);
            }
        }
        let r = rogue_dungeon(&mut t, &cl, &ROGUE_GTNH, seed(), cx, cz);
        chosen_n += 1;
        if let Some(wp) = want_pos {
            ring_n += 1;
            ring_ok += mine.iter().take(50).any(|&q| q == wp) as i32;
        } else {
            no_spot += 1;
        }
        if r.pos == want_pos {
            chosen_ok += 1;
            if want_y >= 0 {
                *gy_hist.entry(r.y - want_y).or_default() += 1;
            }
        } else {
            c.note(format_args!(
                "roguelike anchor ({cx},{cz}): model {:?} (attempt {}, confident {}) real {want_pos:?} after {} attempts",
                r.pos,
                r.attempt,
                r.confident,
                att.len()
            ));
        }
    }
    c.exact(
        "Roguelike attempt positions",
        pos_ok == pos_n,
        format!("{pos_ok}/{pos_n}"),
    );
    c.exact(
        "Roguelike real spot is one of our attempts",
        ring_ok == ring_n,
        format!("{ring_ok}/{ring_n} ({no_spot} anchors: all 50 attempts failed in-game)"),
    );
    c.est(
        "Roguelike validLocation verdicts",
        format!("{v_ok}/{v_n} (model valid/game not {fp}, reverse {fn_})"),
    );
    c.stat(
        "Roguelike verdicts (old model)",
        v_ok as usize,
        v_n as usize,
    );
    c.stat(
        "Roguelike chosen spot (old model)",
        chosen_ok as usize,
        chosen_n as usize,
    );
    c.est(
        "Roguelike chosen spot",
        format!("{chosen_ok}/{chosen_n} (1337: 8/15); model y - real ground y on hits {gy_hist:?}"),
    );

    let (mut s_ok, mut s_n, mut s1, mut s_hist) = (0, 0, 0, BTreeMap::<i32, i32>::new());
    for e in d["lootgames"]["dungeons"].as_array().unwrap() {
        let Some(sy) = e["surfaceY"].as_i64() else {
            continue;
        };
        if e["surfaceFound"] != true {
            continue;
        }
        let my = lootgames_surface(&mut t, i(&e["x"]), i(&e["z"])).unwrap();
        s_n += 1;
        let dd = my - sy as i32;
        *s_hist.entry(dd.clamp(-5, 5)).or_default() += 1;
        s_ok += (dd == 0) as i32;
        s1 += (dd.abs() <= 1) as i32;
    }
    c.stat("LootGames surface y exact", s_ok as usize, s_n as usize);
    c.stat("LootGames surface y ±1", s1 as usize, s_n as usize);
    c.est(
        "LootGames surface y",
        format!("exact {s_ok}/{s_n}, ±1 {s1}/{s_n} (1337: 10/15, 13/15); diff {s_hist:?}"),
    );
    c
}

// ============================================================================================= thaumcraft

fn sec_thaumcraft() -> Card {
    use rwg2_biomes::thaumcraft::*;
    let mut c = Card::new("Thaumcraft structures");
    let Some(truth) = json("thaumcraft.json") else {
        c.skip("no thaumcraft.json");
        return c;
    };
    let r = i(&truth["rangeChunks"]);
    let p = provider(WorldType::Rwg2Continents);
    let cl = climate();
    let mut t = Terrain::new(&p, &TableHeights);
    let sm = rwg2_biomes::spawn::SurfaceModel::new(&p.table);
    let mut g = TcGen {
        t: &mut t,
        climate: &cl,
        cfg: TC_GTNH,
    };
    let mut f = Vec::new();
    for cz in -r..=r {
        for cx in -r..=r {
            g.chunk(seed(), cx, cz, &mut f).unwrap();
        }
    }
    let pre_maze: Vec<(i32, i32)> = f
        .iter()
        .filter(|x| x.kind == TcKind::Ring && x.valid)
        .map(|x| (x.x, x.z))
        .collect();
    let dropped = resolve_ring_mazes_spawn(&mut f, spawn());
    c.note(format_args!(
        "model-valid rings before maze resolution: {pre_maze:?}"
    ));
    let tiles = truth["tiles"].as_array().unwrap();
    let cls = |x: &Value, s: &str| x["class"].as_str().unwrap().ends_with(s);
    let altars: Vec<(i32, i32, i32)> = tiles
        .iter()
        .filter(|x| cls(x, "TileEldritchAltar"))
        .map(|x| (i(&x["x"]), i(&x["y"]), i(&x["z"])))
        .collect();
    let nodes: Vec<(i32, i32, i32, String)> = tiles
        .iter()
        .filter(|x| cls(x, "TileNode"))
        .map(|x| {
            (
                i(&x["x"]),
                i(&x["y"]),
                i(&x["z"]),
                x["type"].as_str().unwrap_or("?").to_string(),
            )
        })
        .collect();
    let mut summary = String::new();
    for k in [
        TcKind::Ring,
        TcKind::Hilltop,
        TcKind::Mound,
        TcKind::Totem,
        TcKind::WildNode,
    ] {
        let all = f.iter().filter(|x| x.kind == k).count();
        let valid = f.iter().filter(|x| x.kind == k && x.valid).count();
        let _ = write!(summary, "{k:?} {valid}/{all}  ");
        for x in f.iter().filter(|x| x.kind == k && k != TcKind::WildNode) {
            c.note(format_args!(
                "{:?} ({}, {}) y {} valid {} exact {} size {:?}",
                x.kind, x.x, x.z, x.y, x.valid, x.exact, x.size
            ));
        }
    }
    c.est(
        "model valid/rolled",
        format!("{summary}(rings dropped by maze conflicts: {dropped})"),
    );
    // rings: every real altar must be a rolled ring at that exact position
    let mut rolled_ok = 0;
    let mut valid_ok = 0;
    for &(ax, ay, az) in &altars {
        if let Some(ring) = f
            .iter()
            .find(|x| x.kind == TcKind::Ring && x.x == ax && x.z == az)
        {
            rolled_ok += 1;
            valid_ok += ring.valid as usize;
            c.note(format_args!(
                "real ring altar ({ax},{ay},{az}): model j {} valid {} (before mazes {}) size {:?}",
                ring.y,
                ring.valid,
                pre_maze.contains(&(ax, az)),
                ring.size
            ));
            // LocationIsValidSpawn points on the model: top must be in [j-1, j+1]; FOREST/JUNGLE/DENSE kill rings
            for (dx, dz) in [(-3, -3), (0, 0), (3, 0), (3, 3), (0, 3)] {
                let (x, z) = (ax + dx, az + dz);
                let top = g.t.top(x, z).unwrap();
                let b = g.t.column(x, z).unwrap().1;
                let ch = g.t.chunk(x >> 4, z >> 4).unwrap().clone();
                let tp = rwg2_biomes::spawn::top_probs(&p, &sm, &ch, x, z);
                let rwg = ch.rwg[((z & 15) * 16 + (x & 15)) as usize];
                c.note(format_args!(
                    "    point ({x},{z}): model top {top} (j {}), biome {b} {:?}, rwg {} | surface model grass {:.2} stone {:.2}",
                    ring.y,
                    cl.dict(b),
                    p.table.info(rwg).name,
                    tp.grass,
                    tp.stone
                ));
            }
        } else {
            c.note(format_args!("real ring altar ({ax},{ay},{az}): NOT rolled"));
        }
    }
    // what the vegetation heuristic costs: rings passing the terrain-only part of LocationIsValidSpawn
    let mut terrain_only = Vec::new();
    for q in f.iter().filter(|x| x.kind == TcKind::Ring) {
        let mut ok = true;
        for (dx, dz) in [(-3, -3), (0, 0), (3, 0), (3, 3), (0, 3)] {
            let top = g.t.top(q.x + dx, q.z + dz).unwrap();
            let b = g.t.column(q.x + dx, q.z + dz).unwrap().1;
            ok &= top >= 62
                && top >= q.y - 1
                && top <= q.y + 1
                && !cl.dict(b).iter().any(|d| d == "MESA");
        }
        if ok {
            let b = g.t.column(q.x, q.z).unwrap().1;
            terrain_only.push((q.x, q.z, b));
        }
    }
    c.est(
        "rings passing terrain-only checks (no vegetation heuristic, no mazes)",
        format!("{} {:?}", terrain_only.len(), terrain_only),
    );
    c.exact(
        "real eldritch rings rolled at the altar",
        rolled_ok == altars.len(),
        format!("{rolled_ok}/{}", altars.len()),
    );
    let model_rings = f
        .iter()
        .filter(|x| x.kind == TcKind::Ring && x.valid)
        .count();
    let likely: Vec<_> = f
        .iter()
        .filter(|x| x.kind == TcKind::Ring && x.valid && x.p_valid >= 0.5)
        .collect();
    let likely_ok = likely
        .iter()
        .filter(|x| altars.iter().any(|a| (a.0, a.2) == (x.x, x.z)))
        .count();
    c.stat(
        "TC rings: real shown by model (p ≥ 5%)",
        valid_ok,
        altars.len(),
    );
    c.stat(
        "TC rings: model 'likely' (p ≥ 50%) right",
        likely_ok,
        likely.len(),
    );
    c.stat(
        "TC rings: model shown (p ≥ 5%)",
        model_rings,
        model_rings.max(1),
    );
    c.est(
        "rings: real accepted by model / model-valid rings",
        format!(
            "{valid_ok}/{} real; model claims {model_rings}",
            altars.len()
        ),
    );
    // DARK nodes come from totems / hilltop stones (structure nodes); PURE nodes from silverwood trees
    let mut dark = 0;
    let mut dark_rolled = 0;
    let mut dark_valid = 0;
    for (x, y, z, ty) in &nodes {
        let near = f.iter().find(|q| {
            // hilltop-stone nodes sit up to ~6 blocks from the roll point (seen: (+2,+3), (+6,-6))
            matches!(q.kind, TcKind::Totem | TcKind::Hilltop | TcKind::Mound)
                && (q.x - x).abs() <= 8
                && (q.z - z).abs() <= 8
        });
        c.note(format_args!(
            "real node ({x},{y},{z}) {ty}: model feature {:?}",
            near.map(|q| (q.kind, q.x, q.z, q.y, q.valid))
        ));
        if near.is_none() {
            let ch = (x >> 4, z >> 4);
            let mut ff = Vec::new();
            for (dx, dz) in [(0, 0), (-1, 0), (0, -1), (-1, -1)] {
                let mut v = Vec::new();
                let _ = g.chunk(seed(), ch.0 + dx, ch.1 + dz, &mut v);
                ff.extend(v.iter().map(|q| (q.kind, q.x, q.z, q.valid, q.exact)));
            }
            c.note(format_args!(
                "    no feature near real node: features rolled in chunk {ch:?} and NW neighbours {ff:?}"
            ));
        }
        if ty == "DARK" {
            dark += 1;
            dark_rolled += near.is_some() as usize;
            dark_valid += near.is_some_and(|q| q.valid) as usize;
        }
    }
    // rolls after vegetation (tree/flower draws are modelled, not exact) -> estimate on held-out worlds
    c.stat("TC DARK nodes rolled as totem/hilltop", dark_rolled, dark);
    c.est_if(
        "DARK nodes = rolled totem/hilltop",
        dark_rolled == dark,
        format!("{dark_rolled}/{dark}"),
    );
    let model_totems = f
        .iter()
        .filter(|x| matches!(x.kind, TcKind::Totem | TcKind::Hilltop) && x.valid)
        .count();
    c.stat("TC totems/hilltops: real accepted", dark_valid, dark);
    c.stat(
        "TC totems/hilltops: model claims",
        model_totems,
        model_totems.max(1),
    );
    c.est(
        "totems/hilltops: real accepted / model-valid",
        format!("{dark_valid}/{dark}; model claims {model_totems}"),
    );
    let wild = f.iter().filter(|x| x.kind == TcKind::WildNode).count();
    c.est(
        "wild aura nodes",
        format!(
            "{wild} rolled, {} non-structure nodes found ({} PURE)",
            nodes.iter().filter(|n| n.3 != "DARK").count(),
            nodes.iter().filter(|n| n.3 == "PURE").count()
        ),
    );
    // mounds: a mound crypt has a spawner below the surface near its centre
    let spawners: Vec<(i32, i32, i32)> = tiles
        .iter()
        .filter(|x| cls(x, "TileEntityMobSpawner"))
        .map(|x| (i(&x["x"]), i(&x["y"]), i(&x["z"])))
        .collect();
    let mut mound_hits = 0;
    let mut mound_valid = 0;
    for m in f.iter().filter(|x| x.kind == TcKind::Mound) {
        let hit = spawners.iter().any(|s| {
            (s.0 - m.x).abs() <= 9 && (s.2 - m.z).abs() <= 9 && s.1 < m.y && s.1 > m.y - 14
        });
        mound_hits += hit as usize;
        mound_valid += m.valid as usize;
        c.note(format_args!(
            "mound ({}, {}) y {} valid {} spawner nearby {hit}",
            m.x, m.z, m.y, m.valid
        ));
    }
    c.est(
        "mounds: model-valid / spawner found near roll",
        format!("{mound_valid} / {mound_hits}"),
    );
    c
}

// ============================================================================================= GT ores

struct CacheVein {
    layer: String,
    min_y: Option<i32>,
    ext: Option<(i32, i32, i32, i32)>,
}

fn sec_gtores() -> Card {
    use rwg2_biomes::gtorder::*;
    let mut c = Card::new("GregTech ore veins");
    let (Some(gt), Some(scan), Some(dg)) = (
        json("gregtech.json"),
        json("gtores.json"),
        json("dungeons.json"),
    ) else {
        c.skip("no gregtech.json / gtores.json / dungeons.json");
        return c;
    };
    let gj: Value = serde_json::from_str(
        &std::fs::read_to_string(root().join("data/gtnh-2.9.0/gregtech.json")).unwrap(),
    )
    .unwrap();
    let table = VeinTable::from_json(&gj.to_string()).unwrap();
    let ids: Vec<BTreeSet<i32>> = gj["veins"]
        .as_array()
        .unwrap()
        .iter()
        .map(|v| v["oreIds"].as_array().unwrap().iter().map(i).collect())
        .collect();

    // live vein list of this instance == embedded table (sList order, weights, Y, size, primary)
    let live: Vec<&Value> = gt["veins"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|v| v["overworld"] == true)
        .collect();
    let same = live.len() == table.overworld.len()
        && live.iter().zip(&table.overworld).all(|(l, v)| {
            l["name"] == v.name.as_str()
                && i(&l["weight"]) == v.weight
                && i(&l["minY"]) == v.min_y
                && i(&l["maxY"]) == v.max_y
                && i(&l["size"]) == v.size
                && i(&l["primary"]["id"]) == v.primary_id
        });
    c.exact(
        "live overworld vein list == data/gregtech.json",
        same,
        format!("{} live, {} embedded", live.len(), table.overworld.len()),
    );
    let cfg_ok = gt["oreveinPercentage"] == 100
        && gt["oreveinAttempts"] == 64
        && gt["oreveinMaxPlacementAttempts"] == 8
        && gt["pattern"] == "EQUAL_SPACING";
    c.exact(
        "GT config (100%, 64, 8, EQUAL_SPACING)",
        cfg_ok,
        format!("pattern source {}", gt["patternSource"]),
    );

    // ---- truth 1: GT's own validOreveins cache (rwg2dump 2.7 reads it correctly): real decisions, exact
    let mut cache: BTreeMap<(i32, i32), CacheVein> = BTreeMap::new();
    let mut other_dims = 0;
    for e in gt["cache"].as_array().unwrap() {
        let ovs: i64 = e["oreveinSeed"].as_str().unwrap().parse().unwrap();
        let v = ovs ^ seed().wrapping_shl(16);
        if (v >> 56) & 0xff != 0 {
            other_dims += 1;
            continue;
        }
        let sx = |b: i64| ((b & 0x0fff_ffff) << 36 >> 36) as i32;
        let (ox, oz) = (sx(v >> 28), sx(v));
        assert_eq!(orevein_seed(seed(), 0, ox, oz), ovs);
        let ext = e["veinWestX"].as_i64().map(|_| {
            (
                i(&e["veinWestX"]),
                i(&e["veinEastX"]),
                i(&e["veinNorthZ"]),
                i(&e["veinSouthZ"]),
            )
        });
        cache.insert(
            (ox, oz),
            CacheVein {
                layer: e["layer"].as_str().unwrap().to_string(),
                min_y: e["veinMinY"].as_i64().map(|y| y as i32),
                ext,
            },
        );
    }
    let not_seed = cache
        .keys()
        .filter(|&&(x, z)| !is_oreseed(&GT_GTNH, x, z))
        .count();
    c.exact(
        "cache keys are oreseed chunks (floorMod 3 == 1)",
        not_seed == 0,
        format!(
            "{} overworld entries ({other_dims} other dims), {not_seed} not oreseeds",
            cache.len()
        ),
    );
    // every cached vein == one of our candidates (name, minY, all four extents)
    let (mut matched, mut no_ores, mut unmatched) = (0, 0, 0);
    let mut idx_hist = BTreeMap::<i32, i32>::new();
    let mut truth: BTreeMap<(i32, i32), Option<usize>> = BTreeMap::new();
    let mut truth_idx: BTreeMap<(i32, i32), i32> = BTreeMap::new();
    for (&(ox, oz), e) in &cache {
        if e.layer == "NoOresInVein" {
            no_ores += 1;
            truth.insert((ox, oz), None);
            continue;
        }
        let ovs = orevein_seed(seed(), 0, ox, oz);
        let hit = (0..GT_GTNH.attempts).find_map(|k| {
            let p = candidate(&table, ovs, k, ox, oz)?;
            let ok = table.overworld[p.vein].name == e.layer
                && Some(p.min_y) == e.min_y
                && Some((p.west, p.east, p.north, p.south)) == e.ext;
            ok.then_some((k, p))
        });
        match hit {
            Some((k, p)) => {
                matched += 1;
                *idx_hist.entry(k).or_default() += 1;
                truth.insert((ox, oz), Some(p.vein));
                truth_idx.insert((ox, oz), k);
            }
            None => {
                unmatched += 1;
                c.note(format_args!(
                    "cache ({ox},{oz}) {} minY {:?} ext {:?}: no candidate matches",
                    e.layer, e.min_y, e.ext
                ));
            }
        }
    }
    c.exact(
        "cached veins == a candidate (vein, minY, extents)",
        unmatched == 0,
        format!("{matched} matched, {no_ores} NoOresInVein, {unmatched} unmatched; real attempt index {idx_hist:?}"),
    );
    c.hist("ore real attempt index (validOreveins)", &idx_hist);
    // NoOresInVein: 8 NO_OVERLAP_AIR failures (all picks above the terrain, e.g. under oceans); scored as None below
    for &(ox, oz) in cache
        .iter()
        .filter(|x| x.1.layer == "NoOresInVein")
        .map(|x| x.0)
    {
        c.note(format_args!(
            "NoOresInVein at oreseed ({ox},{oz}) (block {} {})",
            ox * 16,
            oz * 16
        ));
    }

    // ---- truth 2: ore blocks in the scanned chunks (the method used for seed 1337), checked against the cache
    type Seen = BTreeMap<i32, (i64, i32, i32)>;
    let mut obs: BTreeMap<(i32, i32), Seen> = BTreeMap::new();
    for ch in scan["chunks"].as_array().unwrap() {
        let e = obs.entry((i(&ch["cx"]), i(&ch["cz"]))).or_default();
        for o in ch["ores"].as_array().unwrap() {
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
    let range = i(&scan["rangeChunks"]) - 1;
    let (mut n_scan, mut blk_found, mut blk_agree, mut blk_vs_cache_n, mut y_in) = (0, 0, 0, 0, 0);
    for cz in -range..=range {
        for cx in -range..=range {
            if !is_oreseed(&GT_GTNH, cx, cz) {
                continue;
            }
            n_scan += 1;
            let seen = obs.get(&(cx, cz)).cloned().unwrap_or_default();
            let ovs = orevein_seed(seed(), 0, cx, cz);
            let mut real = None;
            for k in 0..GT_GTNH.max_placement_attempts {
                let p = candidate(&table, ovs, k, cx, cz).unwrap();
                let want = &ids[p.vein];
                let hit = want
                    .iter()
                    .filter(|m| {
                        seen.get(m)
                            .is_some_and(|v| v.1 <= p.min_y + 8 && v.2 >= p.min_y - 1)
                    })
                    .count();
                if hit >= want.len().min(2) {
                    real = Some(p);
                    break;
                }
            }
            if let Some(p) = real {
                blk_found += 1;
                let lo = ids[p.vein]
                    .iter()
                    .filter_map(|m| seen.get(m))
                    .map(|v| v.1)
                    .min()
                    .unwrap();
                let hi = ids[p.vein]
                    .iter()
                    .filter_map(|m| seen.get(m))
                    .map(|v| v.2)
                    .max()
                    .unwrap();
                y_in += (lo >= p.min_y - 1 && hi <= p.min_y + 8) as i32;
            }
            if let Some(tv) = truth.get(&(cx, cz)) {
                blk_vs_cache_n += 1;
                if real.map(|p| p.vein) == *tv {
                    blk_agree += 1;
                } else {
                    c.note(format_args!(
                        "ore blocks vs cache at ({cx},{cz}): blocks say {:?}, cache {:?}",
                        real.map(|p| table.overworld[p.vein].name.clone()),
                        tv.map(|v| table.overworld[v].name.clone())
                    ));
                }
            }
        }
    }
    let in_cache = (-range..=range)
        .flat_map(|z| (-range..=range).map(move |x| (x, z)))
        .filter(|&(x, z)| is_oreseed(&GT_GTNH, x, z) && cache.contains_key(&(x, z)))
        .count();
    c.est(
        "ore-block inference (1337 method) vs cache",
        format!("{n_scan} oreseeds in ±{range}: blocks give a candidate {blk_found}, Y inside {y_in}; agrees with cache {blk_agree}/{blk_vs_cache_n}; {in_cache} of them cached"),
    );

    // ---- order-aware models scored against the cache truth
    let p = provider(WorldType::Rwg2Continents);
    let all: Vec<((i32, i32), Option<usize>)> = truth.iter().map(|(k, v)| (*k, *v)).collect();
    let near = |k: &(i32, i32)| k.0.abs() <= range && k.1.abs() <= range;
    let score = |f: &mut dyn FnMut(i32, i32) -> Option<Placement>| -> (usize, usize) {
        let (mut a, mut b) = (0, 0);
        for (k, want) in &all {
            let ok = f(k.0, k.1).map(|m| m.vein) == *want;
            a += ok as usize;
            b += (ok && near(k)) as usize;
        }
        (a, b)
    };
    let n_all = all.len();
    let n_near = all.iter().filter(|x| near(&x.0)).count();
    let mut t = Terrain::new(&p, &TableHeights);
    let exact =
        score(&mut |cx, cz| resolve_ordered(seed(), &table, &GT_GTNH, cx, cz, spawn(), &mut t));
    let exact_origin =
        score(&mut |cx, cz| resolve_ordered(seed(), &table, &GT_GTNH, cx, cz, (0, 0), &mut t));
    let exact_nw = score(&mut |cx, cz| {
        resolve_at(
            seed(),
            &table,
            &GT_GTNH,
            cx,
            cz,
            cx - 2,
            cz - 2,
            |x, y, z| terrain_stone(&mut t, x, y, z, SURFACE_DEPTH),
        )
    });
    // RWG2 populate prefix (Pre-event consumers, lakes, dungeons, dirt/gravel blobs) on top of the exact model
    let mut pm = rwg2_biomes::population::PopModel::new(&p, &TableHeights, seed());
    {
        let cj: Value = serde_json::from_str(
            &std::fs::read_to_string(root().join("data/gtnh-2.9.0/climate.json")).unwrap(),
        )
        .unwrap();
        pm.gates = rwg2_biomes::population::PreGates::new(
            cj["biomes"].as_array().unwrap().iter().map(|b| {
                (
                    b["id"].as_u64().unwrap() as u16,
                    b["name"].as_str().unwrap_or(""),
                    b["dict"]
                        .as_array()
                        .map(|d| d.iter().filter_map(|x| x.as_str()).collect())
                        .unwrap_or_default(),
                )
            }),
        );
        let (mut x0, mut z0, mut x1, mut z1) = (i32::MAX, i32::MAX, i32::MIN, i32::MIN);
        for ((x, z), _) in &all {
            x0 = x0.min(*x);
            z0 = z0.min(*z);
            x1 = x1.max(*x);
            z1 = z1.max(*z);
        }
        pm.load_structures(x0 - 4, z0 - 4, x1 + 4, z1 + 4);
    }
    // processing-chunk model + the own chunk's populate prefix (measured: no gain without the real load order,
    // −0..4 per world; the load-order replay below gains +2..13)
    type Inc = fn((i32, i32), i32, i32) -> bool;
    let pop_modes: [(&str, Inc); 1] = [("own chunk", |pc, cx, cz| (cx, cz) == pc)];
    let mut pop_scores = Vec::new();
    for (name, inc) in pop_modes {
        let s = score(&mut |cx, cz| {
            let pc = processing_chunk(cx, cz, spawn());
            resolve_at(seed(), &table, &GT_GTNH, cx, cz, pc.0, pc.1, |x, y, z| {
                terrain_stone(&mut t, x, y, z, SURFACE_DEPTH)
                    && pm
                        .changed(x, y, z, |a, b| inc(pc, a, b))
                        .map_or(true, |b| b == rwg2_biomes::population::B::Stone)
            })
        });
        pop_scores.push((name, s));
    }
    let mut a = ApproxTerrain::new(&p, &TableHeights);
    let approx = score(&mut |cx, cz| {
        resolve_ordered_approx(seed(), &table, &GT_GTNH, cx, cz, spawn(), &mut a)
    });
    let old = score(&mut |cx, cz| {
        resolve(seed(), &table, &GT_GTNH, cx, cz, |pl| {
            terrain_stone_ok(&mut t, pl)
        })
    });
    let first = score(&mut |cx, cz| candidate(&table, orevein_seed(seed(), 0, cx, cz), 0, cx, cz));
    let fmt = |s: (usize, usize)| format!("{}/{n_all} all, {}/{n_near} in ±{range}", s.0, s.1);
    c.est("first candidate (no placement test)", fmt(first));
    // ---- diagnosis (ORE_DIAG=1): order-dependent vs order-independent misses of the exact model
    if std::env::var("ORE_DIAG").is_ok() {
        let mut ug = Underground::new(&mut t, Some(Caves::new(seed(), true)), SURFACE_DEPTH);
        let (mut ok, mut order, mut model, mut model_caves_fix) = (0, 0, 0, 0);
        let mut early = 0; // model accepted an earlier pick than the game
        let mut late = 0;
        let mut depth_hist = BTreeMap::<i32, i32>::new();
        let mut path_hist = BTreeMap::<String, i32>::new();
        let mut wrong_depth = BTreeMap::<i32, i32>::new();
        let mut diag_lines = 0;
        // rwg2dump 2.10: real post-generation test columns (cx*16+7, y, cz*16+9), y 0..127
        let cblocks: Vec<String> = scan["censusBlocks"]
            .as_array()
            .map(|a| a.iter().map(|b| b.as_str().unwrap().to_string()).collect())
            .unwrap_or_default();
        let mut tcol: BTreeMap<(i32, i32), Vec<String>> = BTreeMap::new();
        for e in scan["testColumns"].as_array().into_iter().flatten() {
            let col = e["ids"]
                .as_str()
                .unwrap()
                .split(' ')
                .map(|t| {
                    if t == "s" {
                        "stone".to_string()
                    } else {
                        let (a, m) = t
                            .split_once(':')
                            .map(|(a, m)| (a, Some(m)))
                            .unwrap_or((t, None));
                        let n = cblocks[a.parse::<usize>().unwrap()].clone();
                        match m {
                            Some(m) => format!("{n}:{m}"),
                            None => n,
                        }
                    }
                })
                .collect();
            tcol.insert((i(&e["cx"]), i(&e["cz"])), col);
        }
        let mut blockers = BTreeMap::<String, i32>::new(); // non-stone blocks in rejected 9-block windows
        let mut blk_stone = BTreeMap::<i32, i32>::new(); // real stone count in those windows
        let mut blk_seen = 0;
        let mut pop_state = BTreeMap::<&str, i32>::new();
        let mut deco_acc = BTreeMap::<&str, (i32, i32)>::new();
        for (k, want) in &all {
            let (ox, oz) = *k;
            let (px, pz) = processing_chunk(ox, oz, spawn());
            let at = |ug: &mut Underground, px: i32, pz: i32, caves: bool| {
                resolve_at(seed(), &table, &GT_GTNH, ox, oz, px, pz, |x, y, z| {
                    if caves {
                        ug.stone(x, y, z)
                    } else {
                        terrain_stone(ug.t, x, y, z, SURFACE_DEPTH)
                    }
                })
                .map(|m| m.vein)
            };
            let ours = at(&mut ug, px, pz, false);
            {
                // decorator state of the 4 populate chunks writing into the processing chunk's test column
                let mut st = "certain";
                for (a, b) in [(px - 1, pz - 1), (px, pz - 1), (px - 1, pz), (px, pz)] {
                    if !matches!(
                        pm.populate(a, b),
                        rwg2_biomes::population::Outcome::Certain(_)
                    ) {
                        st = "uncertain";
                    } else if pm.flags & rwg2_biomes::population::F_DECO_UNKNOWN != 0
                        && st == "certain"
                    {
                        st = "unknown-decorator";
                    }
                }
                let e = deco_acc.entry(st).or_insert((0, 0));
                e.0 += (ours == *want) as i32;
                e.1 += 1;
            }
            if ours == *want {
                ok += 1;
                continue;
            }
            let mut any = false;
            for wx in ox - 2..=ox + 2 {
                for wz in oz - 2..=oz + 2 {
                    if at(&mut ug, wx, wz, false) == *want {
                        any = true;
                    }
                }
            }
            if any {
                order += 1;
            } else {
                model += 1;
                if at(&mut ug, px, pz, true) == *want {
                    model_caves_fix += 1;
                }
                let ovs = orevein_seed(seed(), 0, ox, oz);
                let kr = truth_idx.get(k).copied();
                let ko = resolve_at(seed(), &table, &GT_GTNH, ox, oz, px, pz, |x, y, z| {
                    terrain_stone(ug.t, x, y, z, SURFACE_DEPTH)
                })
                .map(|m| m.rejected);
                match (kr, ko) {
                    (Some(a), Some(b)) if b < a => {
                        early += 1;
                        // our accepted pick (attempt index b counts failures; attempts == failures here)
                        if let Some((p, rng)) = candidate_rng(&table, ovs, b, ox, oz) {
                            let v = &table.overworld[p.vein];
                            let mut hits = 0;
                            let r = execute_chunkified(
                                v,
                                &p,
                                rng,
                                ox,
                                oz,
                                px,
                                pz,
                                true,
                                |x, y, z, q| {
                                    let st = terrain_stone(ug.t, x, y, z, SURFACE_DEPTH);
                                    if matches!(q, BlockQuery::Stone) && st {
                                        hits += 1;
                                    }
                                    st
                                },
                                |_, _, _, _| {},
                            );
                            let top = ug.t.top(px * 16 + 7, pz * 16 + 9).unwrap_or(0);
                            {
                                let mut st = "certain";
                                for (a, b) in
                                    [(px - 1, pz - 1), (px, pz - 1), (px - 1, pz), (px, pz)]
                                {
                                    if !matches!(
                                        pm.populate(a, b),
                                        rwg2_biomes::population::Outcome::Certain(_)
                                    ) {
                                        st = "uncertain";
                                    } else if pm.decorator_flags(a, b) != 0 && st == "certain" {
                                        st = "support-decorated";
                                    }
                                }
                                *pop_state.entry(st).or_default() += 1;
                            }
                            if let Some(col) = tcol.get(&(px, pz)) {
                                blk_seen += 1;
                                let mut st = 0;
                                for y in p.min_y..p.min_y + 9 {
                                    let b =
                                        col.get(y as usize).cloned().unwrap_or_else(|| "?".into());
                                    if b == "stone" {
                                        st += 1
                                    } else {
                                        let b = if b.starts_with("gregtech:gt.blockores") {
                                            "gregtech:gt.blockores*".to_string()
                                        } else {
                                            b
                                        };
                                        *blockers.entry(b).or_default() += 1;
                                    }
                                }
                                *blk_stone.entry(st).or_default() += 1;
                            }
                            let path = format!("{r:?}");
                            *path_hist.entry(path).or_default() += 1;
                            *wrong_depth.entry(((top - p.min_y) / 10) * 10).or_default() += 1;
                            if diag_lines < 12 {
                                diag_lines += 1;
                                c.note(format_args!("  model accepted ({ox},{oz}) pick {b} {} minY {} [{r:?}, stone hits {hits}] at chunk ({px},{pz}) top {top}; game took pick {a} {}", v.name, p.min_y, candidate(&table, ovs, a, ox, oz).map(|q| table.overworld[q.vein].name.clone()).unwrap_or_default()));
                            }
                        }
                    }
                    _ => late += 1,
                }
                if let Some(a) = kr {
                    // the real pick: depth of its test column below the terrain top at the processing chunk
                    if let Some(p) = candidate(&table, ovs, a, ox, oz) {
                        let top = ug.t.top(px * 16 + 7, pz * 16 + 9).unwrap_or(0);
                        *depth_hist.entry(((top - p.min_y) / 10) * 10).or_default() += 1;
                    }
                }
            }
        }
        let mut bl: Vec<_> = blockers.into_iter().collect();
        bl.sort_by_key(|x| -x.1);
        c.note(format_args!(
            "ORE_DIAG populate state of wrongly accepted picks' columns: {pop_state:?}; right/all by decorator state: {deco_acc:?}"
        ));
        c.note(format_args!("ORE_DIAG test columns: {blk_seen} wrongly accepted picks with a real column; real stone count {blk_stone:?}; blockers {bl:?}"));
        c.note(format_args!(
            "ORE_DIAG: ok {ok}, order-dependent {order}, model misses {model} (caves would fix {model_caves_fix}; model picked earlier than game {early}, later/none {late}); real pick depth below top at test column (10s): {depth_hist:?}; wrongly accepted picks: path {path_hist:?}, depth (10s) {wrong_depth:?}"
        ));
    }
    c.est("old centre model (resolve + terrain_stone_ok)", fmt(old));
    c.est("resolve_ordered exact, real spawn", fmt(exact));
    c.stat("ore exact model", exact.0, n_all);
    c.est(
        "resolve_ordered exact, spawn assumed (0,0)",
        fmt(exact_origin),
    );
    c.est("window-NW exact", fmt(exact_nw));
    for (name, s) in &pop_scores {
        c.est(&format!("exact + populate prefix ({name})"), fmt(*s));
    }
    c.est(
        "resolve_ordered_approx, real spawn  [wasm/search]",
        fmt(approx),
    );
    c.stat("ore approx model [wasm/search]", approx.0, n_all);
    c.stat("ore first candidate", first.0, n_all);

    // ---- load-order sensitivity: the vein from every chunk of the ±2 window (approx terrain = viewer path)
    {
        let mut a2 = ApproxTerrain::new(&p, &TableHeights);
        let (mut agree, mut agree_ok, mut dis, mut dis_ok, mut dis_in) = (0, 0, 0, 0, 0);
        let (mut c_agree, mut c_agree_ok, mut c_dis_ok, mut c_dis_in) = (0, 0, 0, 0);
        for (k, want) in &all {
            let (ox, oz) = *k;
            let mut set: Vec<Option<usize>> = Vec::new();
            let mut corners: Vec<Option<usize>> = Vec::new();
            for wx in ox - 2..=ox + 2 {
                for wz in oz - 2..=oz + 2 {
                    let v = resolve_at(seed(), &table, &GT_GTNH, ox, oz, wx, wz, |x, y, z| {
                        y > 0 && y <= a2.top(x, z) - SURFACE_DEPTH
                    })
                    .map(|m| m.vein);
                    if !set.contains(&v) {
                        set.push(v);
                    }
                    if (wx == ox - 2 || wx == ox + 2)
                        && (wz == oz - 2 || wz == oz + 2)
                        && !corners.contains(&v)
                    {
                        corners.push(v);
                    }
                }
            }
            let heur = resolve_ordered_approx(seed(), &table, &GT_GTNH, ox, oz, spawn(), &mut a2)
                .map(|m| m.vein);
            if set.len() == 1 {
                agree += 1;
                agree_ok += (heur == *want) as i32;
            } else {
                dis += 1;
                dis_ok += (heur == *want) as i32;
                dis_in += set.contains(want) as i32;
            }
            if corners.len() == 1 {
                c_agree += 1;
                c_agree_ok += (heur == *want) as i32;
            } else {
                c_dis_ok += (heur == *want) as i32;
                c_dis_in += (corners.contains(want) || heur == *want) as i32;
            }
        }
        c.est(
            "load-order sensitivity (approx terrain, ±2 window)",
            format!("all 25 agree {agree} (right {agree_ok}); differ {dis} (heuristic right {dis_ok}, real vein among them {dis_in}) · 4 corners agree {c_agree} (right {c_agree_ok}); differ {} (right {c_dis_ok}, real among corners+heuristic {c_dis_in})", all.len() - c_agree as usize),
        );
    }

    // ---- later biome decoration (BOP decorators: 20 dirt + 10 gravel pockets on an unknowable Random state) near
    // the deciding chunk: does it predict misses of the order-free model?
    {
        let pm = rwg2_biomes::population::PopModel::new(&p, &TableHeights, seed());
        let mut a3 = ApproxTerrain::new(&p, &TableHeights);
        let mut by = BTreeMap::<u32, (i32, i32)>::new();
        for (k, want) in &all {
            let (px, pz) = processing_chunk(k.0, k.1, spawn());
            let mut f = 0;
            for (a, b) in [(px - 1, pz - 1), (px, pz - 1), (px - 1, pz), (px, pz)] {
                f |= pm.decorator_flags(a, b);
            }
            let ok = resolve_ordered_approx(seed(), &table, &GT_GTNH, k.0, k.1, spawn(), &mut a3)
                .map(|m| m.vein)
                == *want;
            let e = by.entry(f).or_default();
            e.0 += 1;
            e.1 += ok as i32;
        }
        c.est(
            "order-free model by decorating biomes at the deciding chunk (flags: 1024 support, 2048 ocean)",
            format!("{by:?} (n, right)"),
        );
    }

    // ---- what a fresh world alone determines: spawn walk + spawn area (viewer candidate), oreseeds it decides
    {
        let walk = rwg2_biomes::spawn::walk_positions(seed());
        let step = walk.iter().position(|&q| q == spawn());
        let mut sim = LoadSim::new();
        for &(x, z) in walk.iter().take(step.map_or(1, |s| s + 1)) {
            sim.load(x >> 4, z >> 4);
        }
        sim.spawn_area(spawn().0, spawn().1);
        let firsts = sim.first_processed(|x, z| is_oreseed(&GT_GTNH, x, z));
        let inside: Vec<_> = all
            .iter()
            .filter(|(k, _)| firsts.contains_key(k))
            .cloned()
            .collect();
        let heur = inside
            .iter()
            .filter(|(k, want)| {
                resolve_ordered(seed(), &table, &GT_GTNH, k.0, k.1, spawn(), &mut t).map(|m| m.vein)
                    == *want
            })
            .count();
        let mut row = vec![format!("heuristic {heur}")];
        for (caves, popm) in [(false, false), (true, false), (true, true)] {
            let mut t2 = Terrain::new(&p, &TableHeights);
            let mut u = Underground::new(
                &mut t2,
                caves.then(|| Caves::new(seed(), true)),
                SURFACE_DEPTH,
            );
            if popm {
                let mut pm = rwg2_biomes::population::PopModel::new(&p, &TableHeights, seed());
                pm.gates = pop_gates();
                let (a, b, c2, d) = spawn_populated(spawn());
                pm.load_structures(a - 4, b - 4, c2 + 4, d + 4);
                u.pop = Some(Box::new(pm));
                u.populated = Some(Default::default());
            }
            let t_0 = std::time::Instant::now();
            let mut w = OreWorld::new(seed(), &table, &GT_GTNH, &mut u);
            for &(px, pz) in &sim.order {
                if let Some(d) = w.u.populated.as_mut() {
                    d.insert((px, pz));
                }
                w.process(px, pz);
            }
            let ok = inside
                .iter()
                .filter(|(k, want)| {
                    w.decided.get(k).map(|d| d.as_ref().map(|d| d.0.vein)) == Some(*want)
                })
                .count();
            row.push(format!(
                "replay caves {caves} pop {popm}: {ok} ({:.1} s)",
                t_0.elapsed().as_secs_f64()
            ));
        }
        c.est(
            &format!(
                "spawn walk + spawn area only: {} oreseeds decided",
                inside.len()
            ),
            row.join(" · "),
        );
    }

    // ---- dump-world load-order replay (spawn search walk, spawn area, LootGames, Roguelike, TC force-loads)
    let walk = rwg2_biomes::spawn::walk_positions(seed());
    let spawn_step = walk.iter().position(|&q| q == spawn());
    for with_walk in [false, true] {
        let mut sim = LoadSim::new();
        if with_walk {
            for &(x, z) in walk.iter().take(spawn_step.map_or(1, |s| s + 1)) {
                sim.load(x >> 4, z >> 4);
            }
        } else {
            sim.load(0, 0);
        }
        sim.spawn_area(spawn().0, spawn().1);
        for g in dg["lootgames"]["dungeons"].as_array().unwrap() {
            if g.get("freshChunks").is_none() {
                continue;
            }
            let (x, z) = (i(&g["x"]), i(&g["z"]));
            sim.area_z_major(x - 26, z - 26, x + 26, z + 26);
        }
        for an in dg["roguelike"]["anchors"].as_array().unwrap() {
            let Some(att) = an.get("attempts").and_then(|v| v.as_array()) else {
                continue;
            };
            for at in att.iter().filter_map(|v| v.as_array()) {
                let (x, z) = (i(&at[0]), i(&at[1]));
                sim.area_z_major(x - 21, z - 21, x + 21, z + 21);
            }
        }
        sim.area_z_major(-24 * 16, -24 * 16, 26 * 16 - 1, 26 * 16 - 1);
        // which oreseeds does the replay reach at all, vs what the cache holds
        let firsts = sim.first_processed(|x, z| is_oreseed(&GT_GTNH, x, z));
        let reach_missing = cache.keys().filter(|k| !firsts.contains_key(k)).count();
        if with_walk {
            let miss: Vec<_> = cache.keys().filter(|k| !firsts.contains_key(k)).collect();
            c.note(format_args!(
                "cached oreseeds the load replay never touches: {miss:?}"
            ));
        }
        let reach_extra = firsts.keys().filter(|k| !cache.contains_key(k)).count();
        for (caves, popm) in [(false, false), (true, false), (true, true)] {
            let mut t = Terrain::new(&p, &TableHeights);
            let mut u = Underground::new(
                &mut t,
                caves.then(|| Caves::new(seed(), true)),
                SURFACE_DEPTH,
            );
            if popm {
                let mut pm = rwg2_biomes::population::PopModel::new(&p, &TableHeights, seed());
                pm.gates = pop_gates();
                pm.load_structures(-40, -40, 40, 40);
                u.pop = Some(Box::new(pm));
                u.populated = Some(Default::default());
            }
            let mut w = OreWorld::new(seed(), &table, &GT_GTNH, &mut u);
            for &(px, pz) in &sim.order {
                if let Some(d) = w.u.populated.as_mut() {
                    d.insert((px, pz));
                }
                w.process(px, pz);
            }
            let (mut ok, mut ok_near, mut undecided) = (0, 0, 0);
            // miss classes: model rejected the real pick (model index > real) / model accepted a pick the game rejected
            let (mut cls_model_rejects, mut cls_game_rejects) = (0, 0);
            let mut misses = Vec::new();
            for (k, want) in &all {
                match w.decided.get(k) {
                    None => undecided += 1,
                    Some(d) => {
                        let got = d.as_ref().map(|d| d.0.vein);
                        let got_idx = d.as_ref().map_or(i32::MAX, |d| {
                            let ovs = orevein_seed(seed(), 0, k.0, k.1);
                            (0..GT_GTNH.attempts)
                                .find(|&j| {
                                    candidate(&table, ovs, j, k.0, k.1).is_some_and(|q| {
                                        q.vein == d.0.vein
                                            && q.min_y == d.0.min_y
                                            && q.west == d.0.west
                                    })
                                })
                                .unwrap_or(-1)
                        });
                        let real_idx = truth_idx.get(k).copied().unwrap_or(i32::MAX);
                        if got != *want {
                            if got_idx > real_idx {
                                cls_model_rejects += 1;
                            } else {
                                cls_game_rejects += 1;
                            }
                        }
                        if got == *want {
                            ok += 1;
                            ok_near += near(k) as usize;
                        } else {
                            misses.push((*k, got, *want, w.decided_at.get(k).copied()));
                        }
                    }
                }
            }
            c.est(
                &format!("dump-order replay (walk loads {with_walk}, caves {caves}, populate {popm})"),
                format!(
                    "{ok}/{n_all} all, {ok_near}/{n_near} in ±{range}; {undecided} cached oreseeds not reached; misses: model rejected the real pick {cls_model_rejects}, game rejected the model's pick {cls_game_rejects}"
                ),
            );
            if with_walk && caves {
                for m in misses.iter().take(40) {
                    let nm = |v: Option<usize>| {
                        v.map_or("-".to_string(), |v| {
                            table.overworld[v]
                                .name
                                .trim_start_matches("ore.mix.")
                                .to_string()
                        })
                    };
                    c.note(format_args!(
                        "replay miss oreseed {:?}: model {} real {} decided at {:?}",
                        m.0,
                        nm(m.1),
                        nm(m.2),
                        m.3
                    ));
                }
            }
        }
        c.est(
            &format!("replay reach (walk loads {with_walk})"),
            format!("{} oreseeds touched; cached but not touched {reach_missing}, touched but not cached {reach_extra}; {} chunks populated", firsts.len(), sim.order.len()),
        );
    }
    c.est("spawn step used for walk loads", format!("{spawn_step:?}"));
    c
}

// ============================================================================================= GT fluids

fn sec_gtfluids() -> Card {
    use rwg2_biomes::gtfluids::*;
    let mut c = Card::new("GT underground fluids");
    let Some(v) = json("gtfluids.json") else {
        c.skip("no gtfluids.json");
        return c;
    };
    let t = FluidTable::overworld_default();
    let live: Vec<(String, i32, i32, i32)> = v["table"]["fluids"]
        .as_array()
        .unwrap()
        .iter()
        .map(|f| {
            (
                f["registry"].as_str().unwrap().to_string(),
                i(&f["minAmount"]),
                i(&f["maxAmount"]),
                i(&f["chance"]),
            )
        })
        .collect();
    let ours: Vec<(String, i32, i32, i32)> = t
        .fluids
        .iter()
        .map(|f| (f.registry.clone(), f.min_amount, f.max_amount, f.chance))
        .collect();
    c.exact(
        "live fluid table (HashBiMap order, amounts, chances)",
        live == ours,
        format!("{} fluids", live.len()),
    );
    let (mut n, mut bad) = (0, 0);
    for ch in v["chunks"].as_array().unwrap() {
        let (cx, cz) = (i(&ch[0]), i(&ch[1]));
        let got = fluid_at(seed(), 0, cx, cz, &t)
            .map(|g| (t.fluids[g.fluid_index].registry.clone(), g.amount));
        let want = ch[2].as_str().map(|r| (r.to_string(), i(&ch[3])));
        n += 1;
        if got != want {
            if bad < 5 {
                c.note(format_args!(
                    "fluid chunk ({cx},{cz}) got {got:?} want {want:?}"
                ));
            }
            bad += 1;
        }
    }
    c.exact(
        "pristine chunk amounts",
        bad == 0,
        format!("{n} chunks, {bad} mismatches"),
    );
    c
}

// ============================================================================================= spawn

fn sec_spawn() -> Card {
    use rwg2_biomes::spawn::*;
    let mut c = Card::new("world spawn");
    let Some(d) = json("spawn.json") else {
        c.skip("no spawn.json");
        return c;
    };
    let s = &d["spawn"];
    assert_eq!((i(&s[0]), i(&s[2])), spawn());
    c.exact(
        "provider = BOP WorldProviderSurfaceBOP",
        d["canSpawnDeclaredIn"] == "biomesoplenty.common.world.WorldProviderSurfaceBOP",
        d["canSpawnDeclaredIn"].clone(),
    );
    let pos = walk_positions(seed());
    let walk = d["walk"].as_array().unwrap();
    let bad_pos = walk
        .iter()
        .filter(|w| pos[w["i"].as_u64().unwrap() as usize] != (i(&w["x"]), i(&w["z"])))
        .count();
    c.exact(
        "walk positions (java.util.Random)",
        bad_pos == 0,
        format!("{} steps, {bad_pos} differ", walk.len()),
    );
    let real_step = pos.iter().position(|&q| q == spawn());
    c.exact(
        "real spawn is a walk position",
        real_step.is_some(),
        format!("step {real_step:?}"),
    );
    let p = provider(WorldType::Rwg2Continents);
    let t0 = std::time::Instant::now();
    let e = estimate_spawn(&p, &TableHeights, seed());
    let dt = t0.elapsed();
    for cand in &e.candidates {
        c.note(format_args!(
            "candidate ({}, {}) step {} prob {:.3}",
            cand.x, cand.z, cand.attempt, cand.prob
        ));
    }
    c.stat(
        "spawn estimate == real",
        ((e.x, e.z) == spawn()) as usize,
        1,
    );
    c.est_if(
        "estimate_spawn == real spawn",
        (e.x, e.z) == spawn(),
        format!(
            "model ({}, {}) step {} prob {:.3} confident {} evaluated {} in {dt:?}",
            e.x, e.z, e.attempts, e.prob, e.confident, e.evaluated
        ),
    );
    // pre-population top class of every dumped walk step
    let m = SurfaceModel::new(&p.table);
    let mut g = ChunkGen::new(&p);
    let (mut ok, mut n) = (0, 0);
    let mut stops = Vec::new();
    for w in walk {
        let k = w["i"].as_u64().unwrap() as usize;
        let (x, z) = (i(&w["x"]), i(&w["z"]));
        let pre = w["pre"].as_str().unwrap();
        let ch = g.generate(&p, &TableHeights, x >> 4, z >> 4).unwrap();
        let tp = top_probs(&p, &m, &ch, x, z);
        let ps = tp.spawnable();
        let other = (1.0 - tp.grass - ps).max(0.0);
        let cls = if tp.grass >= ps && tp.grass >= other {
            "grass"
        } else if ps >= other {
            "stone/sand"
        } else {
            "other"
        };
        let real = match pre {
            "minecraft:grass" => "grass",
            "minecraft:stone" | "minecraft:sand" => "stone/sand",
            _ => "other",
        };
        ok += (cls == real) as usize;
        n += 1;
        if real == "stone/sand" {
            stops.push(k);
        }
        if cls != real || ps > 0.05 || real == "stone/sand" {
            let idx = ((z & 15) * 16 + (x & 15)) as usize;
            c.note(format_args!(
                "walk {k:2} ({x:4},{z:4}) pre {pre} y {} now {} canSpawnNow {} | model h {:.2} {} grass {:.2} stone {:.2} sand {:.2}",
                w["preY"],
                w["now"],
                w["canSpawnNow"],
                ch.height[idx],
                p.table.info(ch.rwg[idx]).name,
                tp.grass,
                tp.stone,
                tp.sand
            ));
        }
    }
    c.est(
        "walk pre-population top class",
        format!("{ok}/{n}; real stone/sand pre tops at steps {stops:?}"),
    );
    c
}

// ============================================================================================= structures + landmarks

fn sec_structures() -> Card {
    let mut c = Card::new("structures + landmarks");
    if let Some(v) = json("structures.json") {
        assert!(is_seed(&v["worldSeed"]));
        let want: Vec<(i32, i32)> = v["strongholdGenerator"]["ring"]
            .as_array()
            .unwrap()
            .iter()
            .map(|q| (i(&q["chunkX"]), i(&q["chunkZ"])))
            .collect();
        let got = rwg2_biomes::structures::stronghold_ring(seed()).to_vec();
        c.exact(
            "stronghold ring",
            got == want,
            format!("ours {got:?} real {want:?}"),
        );
        let starts: Vec<(i32, i32)> = v["strongholdGenerator"]["starts"]
            .as_array()
            .unwrap()
            .iter()
            .map(|q| (i(&q["chunkX"]), i(&q["chunkZ"])))
            .collect();
        let (mut s1, mut s2) = (starts.clone(), want.clone());
        s1.sort();
        s2.sort();
        c.exact(
            "stronghold starts at the ring chunks",
            s1 == s2,
            format!("{starts:?}"),
        );
        c.est(
            "generators",
            format!(
                "village {} ({} starts, {} sizeable), mineshaft {} ({} starts)",
                v["villageGenerator"]["class"],
                v["villageGenerator"]["starts"]
                    .as_array()
                    .map_or(0, |a| a.len()),
                v["villageGenerator"]["starts"]
                    .as_array()
                    .map_or(0, |a| a.iter().filter(|s| s["sizeable"] == true).count()),
                v["mineshaftGenerator"]["class"],
                v["mineshaftGenerator"]["starts"]
                    .as_array()
                    .map_or(0, |a| a.len())
            ),
        );
    } else {
        c.skip("no structures.json");
    }
    let Some(v) = json("landmarks.json") else {
        c.skip("no landmarks.json");
        return c;
    };
    let tbl = table();
    for s in v["seeds"].as_array().unwrap() {
        // the fixed extra seeds were checked by earlier dumps; only the world seed here
        if !is_seed(&s["seed"]) {
            continue;
        }
        let seed: i64 = s["seed"].as_str().unwrap().parse().unwrap();
        let p = BiomeProvider::new(
            seed,
            WorldType::Rwg2Continents,
            tbl.clone(),
            RwgConfig::default(),
        );
        let (mut n, mut bad, mut rej) = (0, 0, 0);
        for vo in s["volcanoes"].as_array().unwrap() {
            let (x, z) = (i(&vo["x"]), i(&vo["z"]));
            let base = p.volcano_base_height(x, z, &TableHeights).unwrap();
            let elig = p.volcano_coordinates(x, z, &TableHeights) != rwg2_biomes::continental::NONE;
            n += 1;
            rej += !vo["eligible"].as_bool().unwrap() as usize;
            if base.to_bits() != bits(&vo["baseHeight"]).to_bits()
                || elig != vo["eligible"].as_bool().unwrap()
            {
                bad += 1;
                c.note(format_args!(
                    "volcano seed {seed} ({x},{z}) base {base} want {} elig {elig} want {}",
                    bits(&vo["baseHeight"]),
                    vo["eligible"]
                ));
            }
        }
        for lo in s["lavaCaves"].as_array().unwrap() {
            let (x, z) = (i(&lo["x"]), i(&lo["z"]));
            let h = p.noise_at(x, z, &TableHeights).unwrap();
            n += 1;
            rej += !lo["eligible"].as_bool().unwrap() as usize;
            if h.to_bits() != bits(&lo["noiseAtCenter"]).to_bits()
                || (h > 63.0) != lo["eligible"].as_bool().unwrap()
            {
                bad += 1;
                c.note(format_args!(
                    "lava cave seed {seed} ({x},{z}) noise {h} want {} eligible {}",
                    bits(&lo["noiseAtCenter"]),
                    lo["eligible"]
                ));
            }
            // genIdAtCenter: lava marker iff eligible (and the provider agrees)
            if let Some(gid) = lo["genIdAtCenter"].as_i64() {
                let ours = p.biome_gen_id(x, z, &TableHeights);
                if ours as i64 != gid {
                    bad += 1;
                    c.note(format_args!(
                        "lava cave seed {seed} ({x},{z}) genId at centre {ours} want {gid}"
                    ));
                }
            }
        }
        let label = format!("landmark gates seed {seed}");
        c.exact(
            &label,
            bad == 0,
            format!("{n} landmarks ({rej} rejected in-game), {bad} mismatches"),
        );
    }
    c
}

// ============================================================================================= Roguelike (current model)

fn sec_rogue_model() -> Card {
    let mut c = Card::new("Roguelike (RogueModel, viewer/search model)");
    let Some(d) = json("dungeons.json") else {
        c.skip("no dungeons.json");
        return c;
    };
    let p = provider(WorldType::Rwg2Continents);
    let cl = climate();
    let mut m = RogueModel::new(&p, &TableHeights, seed());
    let (mut v_ok, mut v_n, mut fp, mut fn_, mut y_ok, mut y_n) = (0, 0, 0, 0, 0, 0);
    let (mut spot_ok, mut spot_n) = (0, 0);
    // confidence buckets [0,.25) [.25,.5) [.5,.75) [.75,1]: (hits, n)
    let mut buckets = [(0usize, 0usize); 4];
    for a in d["roguelike"]["anchors"].as_array().unwrap() {
        let Some(att) = a["attempts"].as_array() else {
            continue;
        };
        let (cx, cz) = (i(&a["chunkX"]), i(&a["chunkZ"]));
        let mut want = None;
        for e in att {
            let e = e.as_array().unwrap();
            let (x, z, ok) = (i(&e[0]), i(&e[1]), e[2].as_bool().unwrap());
            let ry = e.get(3).and_then(|v| v.as_i64()).unwrap_or(-1) as i32;
            let r = m.check(&cl, &ROGUE_GTNH, x, z);
            let v = r.p >= 0.5;
            v_n += 1;
            if v == ok {
                v_ok += 1;
            } else if v {
                fp += 1;
            } else {
                fn_ += 1;
            }
            if ok {
                want = Some((x, z));
                y_n += 1;
                y_ok += (r.y == ry) as usize;
            }
        }
        let s = m.dungeon(&cl, &ROGUE_GTNH, seed(), cx, cz);
        spot_n += 1;
        let hit = s.pos == want;
        spot_ok += hit as usize;
        let b = ((s.confidence * 4.0) as usize).min(3);
        buckets[b].1 += 1;
        buckets[b].0 += hit as usize;
        if !hit {
            c.note(format_args!(
                "anchor ({cx},{cz}): model {:?} (attempt {}, conf {:.2}) real {want:?}",
                s.pos, s.attempt, s.confidence
            ));
        }
    }
    c.est(
        "validLocation verdicts",
        format!("{v_ok}/{v_n} (model pass/game fail {fp}, reverse {fn_})"),
    );
    c.est("chosen spot", format!("{spot_ok}/{spot_n}"));
    c.est("ground Y at the real spot", format!("{y_ok}/{y_n}"));
    c.est(
        "confidence calibration [0-.25, .25-.5, .5-.75, .75-1] (hits/n)",
        format!("{buckets:?}"),
    );
    c.stat("Roguelike verdicts", v_ok, v_n);
    c.stat("Roguelike chosen spot", spot_ok, spot_n);
    c.stat("Roguelike ground Y (real spot)", y_ok, y_n);
    for (k, (h, n)) in buckets.iter().enumerate() {
        c.stat(
            &format!(
                "Roguelike spot hit | conf {:.2}-{:.2}",
                k as f32 / 4.0,
                (k + 1) as f32 / 4.0
            ),
            *h,
            *n,
        );
    }
    c
}

// ============================================================================================= villages

fn style_of_piece(class: &str) -> Option<rwg2_biomes::villages::Style> {
    use rwg2_biomes::villages::Style;
    let c = class.rsplit('.').next()?;
    Some(match c.split('$').next()? {
        "PlainsStructures" => Style::Plains,
        "DesertStructures" => Style::Desert,
        "TaigaStructures" => Style::Taiga,
        "SavannaStructures" => Style::Savanna,
        "SnowyStructures" => Style::Snowy,
        "JungleStructures" => Style::Jungle,
        "SwampStructures" => Style::Swamp,
        _ => return None,
    })
}

fn sec_villages() -> Card {
    use rwg2_biomes::villages::*;
    let mut c = Card::new("villages (VillageNames)");
    let p = provider(WorldType::Rwg2Continents);
    let h = TableHeights;
    let vs = Villages::gtnh_2_9_0();
    if let Some(d) = json("structures.json") {
        let vg = &d["villageGenerator"];
        let r = d["scanRadiusChunks"].as_i64().unwrap() as i32;
        let last = -r + ((2 * r) / 17) * 17;
        let (lo, hi) = (-r - 8, last + 8);
        let inside = |x: i32, z: i32| (lo..=hi).contains(&x) && (lo..=hi).contains(&z);
        let mut want: Vec<(i32, i32, Option<Style>, bool)> = vg["starts"]
            .as_array()
            .unwrap()
            .iter()
            .map(|e| {
                (
                    i(&e["chunkX"]),
                    i(&e["chunkZ"]),
                    style_of_piece(e["pieces"][0]["class"].as_str().unwrap_or("")),
                    e["sizeable"].as_bool().unwrap_or(true),
                )
            })
            .filter(|w| inside(w.0, w.1))
            .collect();
        want.sort_by_key(|w| (w.0, w.1));
        let all = vs.in_rect(&p, &h, seed(), lo, lo, hi, hi);
        let mut got: Vec<_> = all.iter().filter(|v| v.viable_exact).cloned().collect();
        got.sort_by_key(|v| (v.cx, v.cz));
        let gs: Vec<(i32, i32)> = got.iter().map(|v| (v.cx, v.cz)).collect();
        let ws: Vec<(i32, i32)> = want.iter().map(|w| (w.0, w.1)).collect();
        c.exact(
            "village starts (structures.json scan window)",
            gs == ws,
            format!("{} real, {} model", ws.len(), gs.len()),
        );
        if gs != ws {
            c.note(format_args!("model {gs:?}\n    real  {ws:?}"));
        }
        let style_ok = got
            .iter()
            .filter(|v| {
                want.iter()
                    .any(|w| (w.0, w.1) == (v.cx, v.cz) && w.2 == Some(v.style))
            })
            .count();
        c.exact(
            "village style",
            style_ok == want.len(),
            format!("{style_ok}/{}", want.len()),
        );
        let sizeable = want.iter().filter(|w| w.3).count();
        c.stat("village sizeable (structures.json)", sizeable, want.len());
    } else {
        c.skip("no structures.json");
    }
    // real village starts generated in play (structures.json): a later villages.json probe of a *loaded* chunk reads
    // the chunk biome array (World.getBiomeGenForCoords), not the provider -> probe artifact if the start exists
    let real_starts: BTreeSet<(i32, i32)> = json("structures.json")
        .and_then(|d| {
            d["villageGenerator"]["starts"].as_array().map(|a| {
                a.iter()
                    .map(|e| (i(&e["chunkX"]), i(&e["chunkZ"])))
                    .collect()
            })
        })
        .unwrap_or_default();
    if let Some(v) = json("villages.json") {
        if let Some(cands) = v["candidates"].as_array() {
            let (mut pos_ok, mut ok, mut real, mut small, mut artifact) = (0, 0, 0, 0, 0);
            let mut bad = Vec::new();
            for e in cands {
                let (cx, cz) = (i(&e["chunkX"]), i(&e["chunkZ"]));
                let can = e["canSpawn"].as_bool().unwrap();
                real += can as usize;
                small += (can && e["sizeable"] == false) as usize;
                pos_ok += is_candidate(seed(), cx, cz) as usize;
                let g = vs.in_rect(&p, &h, seed(), cx, cz, cx, cz);
                let pred = g
                    .iter()
                    .find(|q| (q.cx, q.cz) == (cx, cz))
                    .is_some_and(|q| q.viable_exact);
                if pred == can {
                    ok += 1;
                } else if pred && real_starts.contains(&(cx, cz)) {
                    artifact += 1;
                    let ch = ChunkGen::new(&p)
                        .generate(&p, &TableHeights, cx, cz)
                        .map(|c| c.gen[8 * 16 + 8]);
                    c.note(format_args!(
                        "villages.json says no village at ({cx},{cz}) but structures.json has its start (chunk loaded at probe time: chunk-array biome {ch:?} vs provider)"
                    ));
                } else if bad.len() < 10 {
                    bad.push((
                        cx,
                        cz,
                        can,
                        e["biomeName"].as_str().unwrap_or("").to_string(),
                    ));
                }
            }
            for b in &bad {
                c.note(format_args!("villages.json verdict mismatch {b:?}"));
            }
            c.exact(
                "villages.json candidate positions",
                pos_ok == cands.len(),
                format!("{pos_ok}/{}", cands.len()),
            );
            c.exact(
                "villages.json canSpawnStructureAtCoords verdicts (±1200 chunks)",
                ok + artifact == cands.len(),
                format!(
                    "{ok}/{} ({real} villages, {small} not sizeable; {artifact} loaded-chunk probe artifacts, real start exists)",
                    cands.len()
                ),
            );
            c.stat("village sizeable (villages.json)", real - small, real);
        } else {
            c.skip(&format!(
                "villages.json: {}",
                v.get("error")
                    .map(|e| e.to_string())
                    .unwrap_or_default()
                    .chars()
                    .take(80)
                    .collect::<String>()
            ));
        }
    }
    c
}

// ============================================================================================= mineshafts

fn sec_mineshafts() -> Card {
    use rwg2_biomes::mineshaft::*;
    let mut c = Card::new("mineshafts (Et Futurum MapGenMesaMineshaft)");
    let Some(d) = json("structures.json") else {
        c.skip("no structures.json");
        return c;
    };
    let r = d["scanRadiusChunks"].as_i64().unwrap() as i32;
    let last = -r + ((2 * r) / 17) * 17;
    let (lo, hi) = (-r - 8, last + 8);
    let inside = |e: &&Value| {
        let (x, z) = (i(&e["chunkX"]), i(&e["chunkZ"]));
        (lo..=hi).contains(&x) && (lo..=hi).contains(&z)
    };
    let starts: Vec<&Value> = d["mineshaftGenerator"]["starts"]
        .as_array()
        .unwrap()
        .iter()
        .filter(inside)
        .collect();
    let p = provider(WorldType::Rwg2Continents);
    let cl = climate();
    let seeds = Seeds::new(seed());
    let mut got = starts_in(&seeds, lo, lo, hi, hi);
    got.sort();
    let mut want: Vec<(i32, i32)> = starts
        .iter()
        .map(|e| (i(&e["chunkX"]), i(&e["chunkZ"])))
        .collect();
    want.sort();
    c.exact(
        "mineshaft start chunks",
        got == want,
        format!("{} real, {} model", want.len(), got.len()),
    );
    let (mut xz_ok, mut exact, mut pieces) = (0, 0, 0);
    for e in &starts {
        let (cx, cz) = (i(&e["chunkX"]), i(&e["chunkZ"]));
        let gid = p.biome_gen_id(cx * 16 + 8, cz * 16 + 8, &TableHeights);
        let mesa = cl.dict(gid).iter().any(|t| t == "MESA");
        let wp: Vec<(String, Vec<i32>)> = e["pieces"]
            .as_array()
            .unwrap()
            .iter()
            .map(|q| {
                (
                    q["class"]
                        .as_str()
                        .unwrap()
                        .rsplit('$')
                        .next()
                        .unwrap()
                        .to_string(),
                    q["box"].as_array().unwrap().iter().map(i).collect(),
                )
            })
            .collect();
        pieces += wp.len();
        let Some(m) = mineshaft(&seeds, cx, cz, mesa) else {
            c.note(format_args!("({cx},{cz}): model has no mineshaft"));
            continue;
        };
        let gp: Vec<(String, Vec<i32>)> = m
            .pieces
            .iter()
            .map(|q| (q.kind.java_name().to_string(), q.bb.to_vec()))
            .collect();
        let strip = |v: &[(String, Vec<i32>)]| {
            v.iter()
                .map(|(k, b)| (k.clone(), b[0], b[2], b[3], b[5]))
                .collect::<Vec<_>>()
        };
        if strip(&gp) == strip(&wp) {
            xz_ok += 1;
        } else {
            c.note(format_args!(
                "({cx},{cz}) layout differs: {} vs {} pieces",
                gp.len(),
                wp.len()
            ));
        }
        exact += (gp == wp) as usize;
    }
    c.exact(
        "mineshaft layouts (piece kinds + x/z boxes)",
        xz_ok == starts.len(),
        format!("{xz_ok}/{} ({pieces} pieces)", starts.len()),
    );
    c.exact(
        "mineshaft layouts incl. Y",
        exact == starts.len(),
        format!("{exact}/{}", starts.len()),
    );
    c
}

// ============================================================================================= aura nodes (overworld)

fn sec_tcnodes() -> Card {
    use rwg2_biomes::thaumcraft::*;
    let mut c = Card::new("Thaumcraft aura nodes (overworld)");
    let Some(truth) = json("tcnodes.json") else {
        c.skip("no tcnodes.json");
        return c;
    };
    let p = provider(WorldType::Rwg2Continents);
    let cl = climate();
    let mut t = Terrain::new(&p, &TableHeights);
    let mut g = TcGen {
        t: &mut t,
        climate: &cl,
        cfg: TC_GTNH,
    };
    let mut nodes = Vec::new();
    for cz in -24..=24 {
        for cx in -24..=24 {
            g.nodes(seed(), cx, cz, &mut nodes).unwrap();
        }
    }
    let real = truth["nodes"].as_array().unwrap();
    let (mut xz, mut y, mut ty, mut asp) = (0, 0, 0, 0);
    let mut by_kind: BTreeMap<String, (usize, usize)> = BTreeMap::new();
    for tl in real {
        let (x, ry, z) = (i(&tl["x"]), i(&tl["y"]), i(&tl["z"]));
        let obs = tl["aspects"].as_str().unwrap();
        let rty = tl["type"].as_str().unwrap();
        let rmod = tl["modifier"].as_str().unwrap();
        let Some(n) = nodes.iter().find(|n| (n.x, n.z) == (x, z)) else {
            c.note(format_args!(
                "real node ({x},{ry},{z}) {rty}/{rmod} {obs}: not predicted"
            ));
            continue;
        };
        xz += 1;
        y += (n.y == ry) as usize;
        let e = by_kind.entry(format!("{:?}", n.kind)).or_default();
        e.1 += 1;
        if let Some(info) = n.info {
            let pt = format!("{:?}", info.node_type).to_uppercase();
            let pm = info
                .modifier
                .map(|m| format!("{m:?}").to_uppercase())
                .unwrap_or("null".into());
            let tok = pt == rty && pm == rmod;
            ty += tok as usize;
            let aok =
                tok && parse_aspects(&info.aspects_string(&TC_ASPECTS_GTNH)) == parse_aspects(obs);
            asp += aok as usize;
            e.0 += aok as usize;
            if !aok {
                c.note(format_args!(
                    "{:?} ({x},{z}) y {ry}/{}: real {rty}/{rmod} {obs} | model {pt}/{pm} {}",
                    n.kind,
                    n.y,
                    info.aspects_string(&TC_ASPECTS_GTNH)
                ));
            }
        }
    }
    let realxz: BTreeSet<(i32, i32)> = real.iter().map(|t| (i(&t["x"]), i(&t["z"]))).collect();
    let extra: Vec<_> = nodes
        .iter()
        .filter(|n| !realxz.contains(&(n.x, n.z)))
        .collect();
    let extra_firm = extra
        .iter()
        .filter(|n| n.valid && !matches!(n.kind, TcKind::Ring | TcKind::Silverwood))
        .count();
    let sw_pred = nodes
        .iter()
        .filter(|n| n.kind == TcKind::Silverwood)
        .count();
    let sw_real = nodes
        .iter()
        .filter(|n| n.kind == TcKind::Silverwood && realxz.contains(&(n.x, n.z)))
        .count();
    c.est(
        "positions / type+modifier / aspects / Y",
        format!(
            "{xz}/{n} / {ty} / {asp} / {y}  (by kind contents ok/n {by_kind:?})",
            n = real.len()
        ),
    );
    c.est(
        "model extras",
        format!(
            "{} (firm non-ring non-silverwood {extra_firm}); silverwood predicted {sw_pred}, grew {sw_real}",
            extra.len()
        ),
    );
    c.stat("TC nodes overworld: position", xz, real.len());
    c.stat("TC nodes overworld: type+modifier (of placed)", ty, xz);
    c.stat("TC nodes overworld: aspects (of placed)", asp, xz);
    c.stat("TC nodes overworld: Y (of placed)", y, xz);
    c.stat(
        "TC nodes overworld: firm extras (per real)",
        extra_firm,
        real.len(),
    );
    c.stat("TC silverwood grew / predicted", sw_real, sw_pred);
    c
}

// ============================================================================================= Nether + Twilight Forest

fn u16s_at(file: &str) -> Option<Vec<i32>> {
    let b = std::fs::read(wdir().join(file)).ok()?;
    Some(
        b.chunks_exact(2)
            .map(|c| u16::from_le_bytes([c[0], c[1]]) as i32)
            .collect(),
    )
}

fn sec_dims() -> Card {
    use rwg2_biomes::nether::*;
    use rwg2_biomes::twilight::*;
    let mut c = Card::new("Nether + Twilight Forest");
    let Some(d) = json("dims.json") else {
        c.skip("no dims.json");
        return c;
    };
    // biome grids of the world seed (the fixed extra seeds were checked by dump28)
    for (key, is_tf) in [("tfBiomes", true), ("netherBiomes", false)] {
        let (mut n, mut bad) = (0usize, 0usize);
        for e in d[key].as_array().unwrap() {
            if !is_seed(&e["seed"]) {
                continue;
            }
            let q = e["layer"].as_str().unwrap().contains("1:4");
            let (x, z, w, h) = (i(&e["x"]), i(&e["z"]), i(&e["w"]), i(&e["h"]));
            let Some(real) = u16s_at(e["file"].as_str().unwrap()) else {
                continue;
            };
            let ours = if is_tf {
                let tf = TwilightForest::new(seed());
                if q {
                    tf.biomes_quarter(x, z, w, h)
                } else {
                    tf.biomes(x, z, w, h)
                }
            } else {
                let nb = NetherBiomes::new(seed());
                if q {
                    nb.biomes_quarter(x, z, w, h)
                } else {
                    nb.biomes(x, z, w, h)
                }
            };
            n += real.len();
            bad += real.iter().zip(&ours).filter(|(a, b)| a != b).count();
        }
        if n == 0 {
            c.skip(&format!("{key}: no grids for the world seed"));
        } else {
            c.exact(
                &format!("{key} grids (world seed)"),
                bad == 0,
                format!("{n} samples, {bad} mismatches"),
            );
        }
    }
    // TF features
    let tf = TwilightForest::new(seed());
    let f = &d["tfFeatures"];
    if let Some(regions) = f["regions"].as_array() {
        let (mut n, mut bad) = (0, 0);
        for r in regions {
            let v: Vec<i32> = (0..6).map(|k| i(&r[k])).collect();
            let at = tf.feature_for_region(v[0], v[1]);
            let (cx, cz) = region_center_block(v[0], v[1]);
            n += 1;
            bad += ((cx, cz) != (v[2], v[3]) || at.feature.id as i32 != v[4]) as usize;
        }
        c.exact(
            "TF feature regions (centre + feature id)",
            bad == 0,
            format!("{n} regions, {bad} mismatches"),
        );
        let (mut sn, mut sbad) = (0, 0);
        for s in f["structures"].as_array().unwrap() {
            sn += 1;
            sbad += tf
                .feature_at_chunk(i(&s["chunkX"]), i(&s["chunkZ"]))
                .is_none() as usize;
        }
        c.exact(
            "TF structure starts on feature chunks",
            sbad == 0,
            format!("{sn} starts, {sbad} off"),
        );
    }
    // fortresses
    let ne = &d["nether"];
    if let Some(st) = ne["starts"].as_array() {
        let r = i(&ne["checkRadiusChunks"]);
        let real: BTreeSet<(i32, i32)> = st.iter().map(|s| (i(&s[0]), i(&s[1]))).collect();
        let ours: BTreeSet<(i32, i32)> = fortresses_in(seed(), -r, -r, r, r)
            .into_iter()
            .map(|f| (f.chunk_x, f.chunk_z))
            .filter(|&(x, z)| x.abs() <= r && z.abs() <= r)
            .collect();
        c.exact(
            "fortress starts",
            real == ours,
            format!("{} real, {} model", real.len(), ours.len()),
        );
        let structs = ne["structures"].as_array().unwrap();
        let (mut ok, mut pieces) = (0, 0);
        for s in structs {
            let (cx, cz) = (i(&s["chunkX"]), i(&s["chunkZ"]));
            let Some(l) = rwg2_biomes::fortress::layout(seed(), cx, cz) else {
                continue;
            };
            let want: Vec<(String, [i32; 6])> = s["pieces"]
                .as_array()
                .unwrap()
                .iter()
                .map(|p| {
                    let a = p.as_array().unwrap();
                    (
                        a[0].as_str()
                            .unwrap()
                            .rsplit('$')
                            .next()
                            .unwrap()
                            .to_string(),
                        [i(&a[1]), i(&a[2]), i(&a[3]), i(&a[4]), i(&a[5]), i(&a[6])],
                    )
                })
                .collect();
            pieces += want.len();
            let same = l.pieces.len() == want.len()
                && l.pieces
                    .iter()
                    .zip(&want)
                    .all(|(p, w)| p.kind.java_name() == w.0 && p.bbox == w.1);
            ok += same as usize;
            if !same {
                c.note(format_args!(
                    "fortress ({cx},{cz}) layout differs ({} vs {} pieces)",
                    l.pieces.len(),
                    want.len()
                ));
            }
        }
        c.exact(
            "fortress layouts incl. Y",
            ok == structs.len(),
            format!("{ok}/{} ({pieces} pieces)", structs.len()),
        );
    }
    // GT veins in dims -1 / 7: real vein is a candidate; which attempt
    let gtid = |key: &str| -> Vec<BTreeSet<i32>> {
        d["gt"][key]
            .as_array()
            .map(|a| {
                a.iter()
                    .map(|v| {
                        ["primary", "secondary", "between", "sporadic"]
                            .iter()
                            .map(|k| i(&v[k]["id"]))
                            .collect()
                    })
                    .collect()
            })
            .unwrap_or_default()
    };
    for (dim, key, rows) in [
        (-1, "Nether", &GT_NETHER_VEINS[..]),
        (7, "Twilight Forest", &GT_TF_VEINS[..]),
    ] {
        let ids = gtid(key);
        let scan = &d["gtScan"][dim.to_string()];
        let (Some(chunks), false) = (scan["chunks"].as_array(), ids.is_empty()) else {
            c.skip(&format!("{key}: no ore scan / vein list"));
            continue;
        };
        let live_ok = ids.len() == rows.len()
            && d["gt"][key]
                .as_array()
                .unwrap()
                .iter()
                .zip(rows)
                .all(|(v, r)| v["name"].as_str() == Some(r.0) && i(&v["weight"]) == r.1);
        c.exact(
            &format!("{key} live vein list == model"),
            live_ok,
            format!("{} veins", ids.len()),
        );
        let table = vein_table(rows);
        let rc = i(&scan["rangeChunks"]);
        let mut obs: BTreeMap<(i32, i32), BTreeMap<i32, (i32, i32)>> = BTreeMap::new();
        for ch in chunks {
            let e = obs.entry((i(&ch["cx"]), i(&ch["cz"]))).or_default();
            for o in ch["ores"].as_array().unwrap() {
                let o: Vec<i32> = o.as_array().unwrap().iter().map(i).collect();
                if o[1] == 0 {
                    e.insert(o[0], (o[3], o[4]));
                }
            }
        }
        let (mut n, mut empty, mut found) = (0, 0, 0);
        let mut hist = BTreeMap::new();
        for cz in -rc + 1..rc {
            for cx in -rc + 1..rc {
                if !is_oreseed(&GT_GTNH, cx, cz) {
                    continue;
                }
                n += 1;
                let seen = obs.get(&(cx, cz)).cloned().unwrap_or_default();
                if seen.is_empty() {
                    empty += 1;
                    continue;
                }
                let cands = gt_candidates(
                    seed(),
                    dim,
                    &table,
                    &GT_GTNH,
                    cx,
                    cz,
                    GT_GTNH.max_placement_attempts + 1,
                );
                for (k, cd) in cands.iter().enumerate() {
                    let want = &ids[cd.vein];
                    let hit = want
                        .iter()
                        .filter(|m| {
                            seen.get(m)
                                .is_some_and(|v| v.0 <= cd.min_y + 8 && v.1 >= cd.min_y - 1)
                        })
                        .count();
                    if hit >= want.len().min(2) {
                        found += 1;
                        *hist.entry(k as i32).or_insert(0) += 1;
                        break;
                    }
                }
            }
        }
        c.exact(
            &format!("{key} ore veins: real vein is a candidate"),
            found + empty >= n - 2,
            format!("{found} found, {empty} without ore blocks, of {n} oreseeds; attempt index {hist:?}"),
        );
        c.stat(
            &format!("{key} ore: attempt 0 (map vein) right"),
            *hist.get(&0).unwrap_or(&0) as usize,
            found,
        );
        c.hist(&format!("{key} ore attempt index"), &hist);
    }
    sec_dim_terrain(&mut c, &d);
    c
}

/// Pre-population terrain (`dimterrain_*.bin`) + aura nodes of dims -1 / 7.
fn sec_dim_terrain(c: &mut Card, d: &Value) {
    use rwg2_biomes::dimterrain::*;
    use rwg2_biomes::tc_dims::{DimTc, DimWorld};
    use rwg2_biomes::thaumcraft::{TC_ASPECTS_GTNH, TC_GTNH};
    let nether = NetherTerrain::new(seed());
    let tf = TfTerrain::new(seed());
    let cl = climate();
    for (dim, key, file) in [
        (-1, "-1:terrain", "dimterrain_m1.bin"),
        (7, "7:terrain", "dimterrain_7.bin"),
    ] {
        let meta = &d["gtScan"][key];
        let Ok(bin) = std::fs::read(wdir().join(file)) else {
            c.skip(&format!("{file} missing"));
            continue;
        };
        let (lo, hi) = (i(&meta["chunkMin"]), i(&meta["chunkMax"]));
        let w = (hi - lo + 1) as usize;
        let cols = w * w * 256;
        assert_eq!(bin.len(), cols * (1 + 2 + 32 + 1 + 2 + 32));
        let pre_solid = |g: usize, y: usize| bin[cols * 3 + g * 32 + (y >> 3)] >> (y & 7) & 1 != 0;
        let pre_top = |g: usize| match bin[g] {
            255 => -1,
            v => v as i32,
        };
        let (mut blocks, mut bad, mut top_bad) = (0u64, 0u64, 0u64);
        for k in 0..w * w {
            let (cx, cz) = (lo + (k % w) as i32, lo + (k / w) as i32);
            if dim == -1 {
                let ch = nether.chunk(cx, cz);
                for idx in 0..256usize {
                    let g = k * 256 + idx;
                    let (lx, lz) = ((idx % 16) as i32, (idx / 16) as i32);
                    for y in 0..128 {
                        blocks += 1;
                        bad += (nb::solid(ch.at(lx, y as i32, lz)) != pre_solid(g, y)) as u64;
                    }
                }
            } else {
                let ch = tf.chunk(cx, cz);
                for idx in 0..256usize {
                    let g = k * 256 + idx;
                    let (lx, lz) = ((idx % 16) as i32, (idx / 16) as i32);
                    for y in 0..256 {
                        blocks += 1;
                        bad += (tb::solid(ch.at(lx, y as i32, lz)) != pre_solid(g, y)) as u64;
                    }
                    top_bad += (ch.top(lx, lz) != pre_top(g)) as u64;
                }
            }
        }
        let name = if dim == -1 {
            "Nether"
        } else {
            "Twilight Forest"
        };
        c.exact(
            &format!("{name} pre-population terrain (solid/air per block)"),
            bad == 0 && top_bad == 0,
            format!("{blocks} blocks, {bad} mismatches, top {top_bad}"),
        );
        // aura nodes
        let Some(real) = meta["tcNodes"].as_array() else {
            continue;
        };
        let wv = if dim == -1 {
            DimWorld::Nether(&nether)
        } else {
            DimWorld::Twilight(&tf)
        };
        let mut g = DimTc::new(wv, &cl, TC_GTNH);
        let mut nodes = Vec::new();
        for cz in lo..=hi {
            for cx in lo..=hi {
                g.nodes(seed(), cx, cz, &mut nodes);
            }
        }
        let (mut xz, mut y, mut asp) = (0, 0, 0);
        for n in real {
            let (x, ry, z) = (i(&n["x"]), i(&n["y"]), i(&n["z"]));
            let Some(m) = nodes.iter().find(|m| (m.x, m.z) == (x, z)) else {
                c.note(format_args!(
                    "{name} real node ({x},{ry},{z}) not predicted"
                ));
                continue;
            };
            xz += 1;
            y += (m.y == ry) as usize;
            if let Some(info) = m.info {
                let pt = format!("{:?}", info.node_type).to_uppercase();
                let pm = info
                    .modifier
                    .map(|m| format!("{m:?}").to_uppercase())
                    .unwrap_or("null".into());
                asp += (pt == n["type"].as_str().unwrap()
                    && pm == n["modifier"].as_str().unwrap()
                    && parse_aspects(&info.aspects_string(&TC_ASPECTS_GTNH))
                        == parse_aspects(n["aspects"].as_str().unwrap()))
                    as usize;
            }
        }
        let extra = nodes
            .iter()
            .filter(|m| m.valid && !real.iter().any(|n| (i(&n["x"]), i(&n["z"])) == (m.x, m.z)))
            .count();
        c.est(
            &format!("{name} aura nodes: positions / contents / Y / firm extras"),
            format!("{xz}/{} / {asp} / {y} / {extra}", real.len()),
        );
        c.stat(&format!("TC nodes {name}: position"), xz, real.len());
        c.stat(&format!("TC nodes {name}: contents (of placed)"), asp, xz);
        c.stat(&format!("TC nodes {name}: Y (of placed)"), y, xz);
        c.stat(
            &format!("TC nodes {name}: firm extras (per real)"),
            extra,
            real.len(),
        );
    }
}

// ============================================================================================= runner

fn worlds() -> Vec<(i64, PathBuf)> {
    let dir = std::env::var("WORLDS_DIR")
        .map(|d| root().join(d))
        .unwrap_or_else(|_| root().join("testdata/pack/gtnh-2.9.0/worlds"));
    let only: Option<Vec<i64>> = std::env::var("WORLDS")
        .ok()
        .map(|s| s.split(',').filter_map(|x| x.trim().parse().ok()).collect());
    let mut v: Vec<(i64, PathBuf)> = std::fs::read_dir(&dir)
        .map(|rd| {
            rd.filter_map(|e| {
                let e = e.ok()?;
                let s: i64 = e.file_name().to_str()?.parse().ok()?;
                e.path()
                    .join("spawn.json")
                    .exists()
                    .then_some((s, e.path()))
            })
            .collect()
        })
        .unwrap_or_default();
    if let Some(o) = only {
        v.retain(|w| o.contains(&w.0));
    }
    v.sort();
    v
}

fn run_world(seed: i64, dir: PathBuf) -> Vec<Card> {
    let sp: Value =
        serde_json::from_str(&std::fs::read_to_string(dir.join("spawn.json")).unwrap()).unwrap();
    let spawn = (i(&sp["spawn"][0]), i(&sp["spawn"][2]));
    CTX.with(|c| *c.borrow_mut() = Some(Ctx { seed, spawn, dir }));
    type Sec = (&'static str, fn() -> Card);
    let all: Vec<Sec> = vec![
        ("grids", sec_grids),
        ("chunkgen", sec_chunkgen),
        ("forestry", sec_forestry),
        ("structures", sec_structures),
        ("spawn", sec_spawn),
        ("villages", sec_villages),
        ("mineshafts", sec_mineshafts),
        ("dungeons", sec_dungeons),
        ("rogue", sec_rogue_model),
        ("gtfluids", sec_gtfluids),
        ("gtores", sec_gtores),
        ("thaumcraft", sec_thaumcraft),
        ("tcnodes", sec_tcnodes),
        ("dims", sec_dims),
    ];
    // WORLDS_SECS=spawn,chunkgen runs only those sections
    let pick: Option<Vec<String>> = std::env::var("WORLDS_SECS")
        .ok()
        .map(|s| s.split(',').map(|x| x.trim().to_string()).collect());
    let secs: Vec<fn() -> Card> = all
        .into_iter()
        .filter(|(n, _)| pick.as_ref().map_or(true, |p| p.iter().any(|q| q == n)))
        .map(|(_, f)| f)
        .collect();
    secs.iter()
        .map(|&f| {
            std::panic::catch_unwind(f).unwrap_or_else(|e| {
                let msg = e
                    .downcast_ref::<String>()
                    .cloned()
                    .or_else(|| e.downcast_ref::<&str>().map(|s| s.to_string()));
                let mut c = Card::new("section");
                c.exact("section ran", false, format!("panicked: {msg:?}"));
                c
            })
        })
        .collect()
}

#[test]
fn held_out_worlds() {
    let ws = worlds();
    if ws.is_empty() {
        eprintln!("skip: no worlds in testdata/pack/gtnh-2.9.0/worlds");
        return;
    }
    let t0 = std::time::Instant::now();
    #[cfg(target_os = "wasi")]
    let results: Vec<(i64, Vec<Card>, std::time::Duration)> = ws
        .into_iter()
        .map(|(s, d)| {
            let t = std::time::Instant::now();
            let c = run_world(s, d);
            (s, c, t.elapsed())
        })
        .collect();
    #[cfg(not(target_os = "wasi"))]
    let results: Vec<(i64, Vec<Card>, std::time::Duration)> = {
        let n = std::thread::available_parallelism()
            .map_or(2, |n| n.get())
            .min(ws.len());
        let queue = std::sync::Mutex::new(ws);
        let out = std::sync::Mutex::new(Vec::new());
        std::thread::scope(|s| {
            for _ in 0..n {
                std::thread::Builder::new()
                    .stack_size(256 << 20)
                    .spawn_scoped(s, || loop {
                        let Some((sd, d)) = queue.lock().unwrap().pop() else {
                            break;
                        };
                        let t = std::time::Instant::now();
                        let c = run_world(sd, d);
                        eprintln!("world {sd} done in {:.1?}", t.elapsed());
                        out.lock().unwrap().push((sd, c, t.elapsed()));
                    })
                    .unwrap();
            }
        });
        let mut v = out.into_inner().unwrap();
        v.sort_by_key(|r| r.0);
        v
    };
    let mut out = String::new();
    let mut agg: BTreeMap<String, (usize, usize, Vec<String>)> = BTreeMap::new();
    let mut hagg: BTreeMap<String, BTreeMap<i32, i32>> = BTreeMap::new();
    let mut fails = Vec::new();
    for (sd, cards, dt) in &results {
        let _ = writeln!(
            out,
            "\n===================== world {sd} ({dt:.1?}) ====================="
        );
        for c in cards {
            let _ = writeln!(out, "## {}", c.name);
            for r in &c.rows {
                let _ = writeln!(out, "{r}");
            }
            for (k, h, n) in &c.stats {
                let e = agg.entry(k.clone()).or_default();
                e.0 += h;
                e.1 += n;
                e.2.push(format!("{h}/{n}"));
            }
            for (k, h) in &c.hists {
                let e = hagg.entry(k.clone()).or_default();
                for (a, b) in h {
                    *e.entry(*a).or_default() += b;
                }
            }
            fails.extend(c.fails.iter().map(|f| format!("world {sd}: {f}")));
        }
    }
    let _ = writeln!(out, "\n===================== details =====================");
    for (sd, cards, _) in &results {
        for c in cards.iter().filter(|c| !c.log.is_empty()) {
            let _ = writeln!(out, "\n## world {sd} — {}\n{}", c.name, c.log);
        }
    }
    let _ = writeln!(
        out,
        "\n===================== aggregate over {} worlds ({:.1?}) =====================",
        results.len(),
        t0.elapsed()
    );
    for (k, (h, n, per)) in &agg {
        let pct = if *n > 0 {
            *h as f64 * 100.0 / *n as f64
        } else {
            f64::NAN
        };
        let _ = writeln!(
            out,
            "  {k:<52} {h:>5}/{n:<5} {pct:>6.1}%   per world {}",
            per.join(" ")
        );
    }
    for (k, h) in &hagg {
        let _ = writeln!(out, "  {k:<52} {h:?}");
    }
    let _ = writeln!(out, "  exact-check failures: {}", fails.len());
    println!("{out}");
    if let Ok(p) = std::env::var("WORLDS_REPORT") {
        let _ = std::fs::write(p, &out);
    }
    assert!(
        fails.is_empty(),
        "exact checks failed:\n{}",
        fails.join("\n")
    );
}
