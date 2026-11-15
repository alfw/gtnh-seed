//! Second real world: every pack validation re-run against rwg2dump 2.7 output of a fresh GTNH 2.9.0 world,
//! seed 92900795, RWG2_CONTINENTS (`testdata/pack/gtnh-2.9.0/dump27b/`). Real spawn (-37, 64, -350).
//!
//! Prints a per-feature scorecard. Only things that must be exact are asserted (seed maths, bit-exact terrain,
//! deterministic RNG replays); model estimates (Roguelike spot, TC validity, ore placement order, ...) are reported.
//!
//! `cargo test --release -p rwg2-biomes --features json --test second_world -- --nocapture`
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
use std::collections::{BTreeMap, BTreeSet};
use std::fmt::Write as _;
use std::path::PathBuf;

const SEED: i64 = 92_900_795;
/// WorldInfo spawn X/Z of the dump world (dump27b/spawn.json).
const SPAWN: (i32, i32) = (-37, -350);

fn root() -> PathBuf {
    std::env::var("RWG2_DATA_DIR")
        .map(|d| PathBuf::from(d).parent().unwrap().to_path_buf())
        .unwrap_or_else(|_| PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../.."))
}
fn d27() -> PathBuf {
    root().join("testdata/pack/gtnh-2.9.0/dump27b")
}
fn json(name: &str) -> Option<Value> {
    serde_json::from_str(&std::fs::read_to_string(d27().join(name)).ok()?).ok()
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
fn provider(wt: WorldType) -> BiomeProvider {
    BiomeProvider::new(SEED, wt, table(), RwgConfig::default())
}
fn i(v: &Value) -> i32 {
    v.as_i64().unwrap() as i32
}
fn bits(v: &Value) -> f32 {
    f32::from_bits(u32::from_str_radix(v["bits"].as_str().unwrap(), 16).unwrap())
}

/// One scorecard section: `exact` rows are asserted, `est` rows are informational.
#[derive(Default)]
struct Card {
    name: &'static str,
    rows: Vec<String>,
    fails: Vec<String>,
    log: String,
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
            if !file.starts_with("grid_") || f["seed"] != SEED || f["worldType"] != wt_name {
                continue;
            }
            let Ok(bytes) = std::fs::read(d27().join(file)) else {
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
            c.skip(&format!(
                "{wt_name}: no grid_{SEED}_* files (copy them from the rwg2dump output)"
            ));
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
        if !name.starts_with("chunks_") || f["seed"] != SEED {
            continue;
        }
        let Ok(bytes) = std::fs::read(d27().join(name)) else {
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
        let p = BiomeProvider::new(SEED, wt, t.clone(), RwgConfig::default());
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
        std::fs::read(d27().join(format!("worldchunks_{SEED}.bin"))),
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
    let mut top_eq = 0;
    let mut gconf = [[0usize; 2]; 2];
    let mut sconf = [[0usize; 2]; 2];
    for cz in 0..w {
        for cx in 0..w {
            let ch = g.generate(&p, &TableHeights, x0 + cx, z0 + cz).unwrap();
            for idx in 0..256usize {
                let k = ((cz * w + cx) as usize) * 256 + idx;
                bad_b += (u16::from_le_bytes([bin[k * 2], bin[k * 2 + 1]]) != ch.gen[idx]) as usize;
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
        let mut u = Underground::new(&mut t, Some(Caves::new(SEED, fast)), 0);
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
    assert_eq!(d["worldSeed"], SEED.to_string());
    let want: Vec<(i32, i32)> = d["lootgames"]["dungeons"]
        .as_array()
        .unwrap()
        .iter()
        .map(|e| (i(&e["chunkX"]), i(&e["chunkZ"])))
        .collect();
    let mut got = lootgames_chunks(SEED, 20, -128, -128, 128, 128);
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
    let mut ga = rogue_anchors(SEED, &ROGUE_GTNH, -128, -128, 128, 128);
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
        let mine = rogue_attempts(SEED, cx, cz);
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
        let r = rogue_dungeon(&mut t, &cl, &ROGUE_GTNH, SEED, cx, cz);
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
            g.chunk(SEED, cx, cz, &mut f).unwrap();
        }
    }
    let pre_maze: Vec<(i32, i32)> = f
        .iter()
        .filter(|x| x.kind == TcKind::Ring && x.valid)
        .map(|x| (x.x, x.z))
        .collect();
    let dropped = resolve_ring_mazes(&mut f);
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
            matches!(q.kind, TcKind::Totem | TcKind::Hilltop | TcKind::Mound)
                && (q.x - x).abs() <= 3
                && (q.z - z).abs() <= 3
        });
        c.note(format_args!(
            "real node ({x},{y},{z}) {ty}: model feature {:?}",
            near.map(|q| (q.kind, q.x, q.z, q.y, q.valid))
        ));
        if ty == "DARK" {
            dark += 1;
            dark_rolled += near.is_some() as usize;
            dark_valid += near.is_some_and(|q| q.valid) as usize;
        }
    }
    c.exact(
        "DARK nodes = rolled totem/hilltop",
        dark_rolled == dark,
        format!("{dark_rolled}/{dark}"),
    );
    let model_totems = f
        .iter()
        .filter(|x| matches!(x.kind, TcKind::Totem | TcKind::Hilltop) && x.valid)
        .count();
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
        let v = ovs ^ SEED.wrapping_shl(16);
        if (v >> 56) & 0xff != 0 {
            other_dims += 1;
            continue;
        }
        let sx = |b: i64| ((b & 0x0fff_ffff) << 36 >> 36) as i32;
        let (ox, oz) = (sx(v >> 28), sx(v));
        assert_eq!(orevein_seed(SEED, 0, ox, oz), ovs);
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
        let ovs = orevein_seed(SEED, 0, ox, oz);
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
            let ovs = orevein_seed(SEED, 0, cx, cz);
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
    let exact = score(&mut |cx, cz| resolve_ordered(SEED, &table, &GT_GTNH, cx, cz, SPAWN, &mut t));
    let exact_origin =
        score(&mut |cx, cz| resolve_ordered(SEED, &table, &GT_GTNH, cx, cz, (0, 0), &mut t));
    let exact_nw = score(&mut |cx, cz| {
        resolve_at(SEED, &table, &GT_GTNH, cx, cz, cx - 2, cz - 2, |x, y, z| {
            terrain_stone(&mut t, x, y, z, SURFACE_DEPTH)
        })
    });
    let mut a = ApproxTerrain::new(&p, &TableHeights);
    let approx =
        score(&mut |cx, cz| resolve_ordered_approx(SEED, &table, &GT_GTNH, cx, cz, SPAWN, &mut a));
    let old = score(&mut |cx, cz| {
        resolve(SEED, &table, &GT_GTNH, cx, cz, |pl| {
            terrain_stone_ok(&mut t, pl)
        })
    });
    let first = score(&mut |cx, cz| candidate(&table, orevein_seed(SEED, 0, cx, cz), 0, cx, cz));
    let fmt = |s: (usize, usize)| format!("{}/{n_all} all, {}/{n_near} in ±{range}", s.0, s.1);
    c.est("first candidate (no placement test)", fmt(first));
    c.est("old centre model (resolve + terrain_stone_ok)", fmt(old));
    c.est("resolve_ordered exact, spawn (-37,-350)", fmt(exact));
    c.est(
        "resolve_ordered exact, spawn assumed (0,0)",
        fmt(exact_origin),
    );
    c.est("window-NW exact", fmt(exact_nw));
    c.est(
        "resolve_ordered_approx, spawn (-37,-350)  [wasm/search]",
        fmt(approx),
    );

    // ---- dump-world load-order replay (spawn search walk, spawn area, LootGames, Roguelike, TC force-loads)
    let walk = rwg2_biomes::spawn::walk_positions(SEED);
    let spawn_step = walk.iter().position(|&q| q == SPAWN);
    for with_walk in [false, true] {
        let mut sim = LoadSim::new();
        if with_walk {
            for &(x, z) in walk.iter().take(spawn_step.map_or(1, |s| s + 1)) {
                sim.load(x >> 4, z >> 4);
            }
        } else {
            sim.load(0, 0);
        }
        sim.spawn_area(SPAWN.0, SPAWN.1);
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
        for caves in [false, true] {
            let mut t = Terrain::new(&p, &TableHeights);
            let mut u =
                Underground::new(&mut t, caves.then(|| Caves::new(SEED, true)), SURFACE_DEPTH);
            let mut w = OreWorld::new(SEED, &table, &GT_GTNH, &mut u);
            for &(px, pz) in &sim.order {
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
                            let ovs = orevein_seed(SEED, 0, k.0, k.1);
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
                &format!("dump-order replay (walk loads {with_walk}, caves {caves})"),
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
        let got = fluid_at(SEED, 0, cx, cz, &t)
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
    assert_eq!((i(&s[0]), i(&s[2])), SPAWN);
    c.exact(
        "provider = BOP WorldProviderSurfaceBOP",
        d["canSpawnDeclaredIn"] == "biomesoplenty.common.world.WorldProviderSurfaceBOP",
        d["canSpawnDeclaredIn"].clone(),
    );
    let pos = walk_positions(SEED);
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
    let real_step = pos.iter().position(|&q| q == SPAWN);
    c.exact(
        "real spawn is a walk position",
        real_step.is_some(),
        format!("step {real_step:?}"),
    );
    let p = provider(WorldType::Rwg2Continents);
    let t0 = std::time::Instant::now();
    let e = estimate_spawn(&p, &TableHeights, SEED);
    let dt = t0.elapsed();
    for cand in &e.candidates {
        c.note(format_args!(
            "candidate ({}, {}) step {} prob {:.3}",
            cand.x, cand.z, cand.attempt, cand.prob
        ));
    }
    c.exact(
        "estimate_spawn == real spawn (-37, -350)",
        (e.x, e.z) == SPAWN,
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
        assert_eq!(v["worldSeed"], SEED.to_string());
        let want: Vec<(i32, i32)> = v["strongholdGenerator"]["ring"]
            .as_array()
            .unwrap()
            .iter()
            .map(|q| (i(&q["chunkX"]), i(&q["chunkZ"])))
            .collect();
        let got = rwg2_biomes::structures::stronghold_ring(SEED).to_vec();
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
        let label = if seed == SEED {
            format!("landmark gates seed {seed} (new world)")
        } else {
            format!("landmark gates seed {seed}")
        };
        c.exact(
            &label,
            bad == 0,
            format!("{n} landmarks ({rej} rejected in-game), {bad} mismatches"),
        );
    }
    c
}

// ============================================================================================= runner

#[test]
fn second_world_scorecard() {
    if !d27().join("spawn.json").exists() {
        eprintln!("skip: no {}", d27().display());
        return;
    }
    let secs: Vec<fn() -> Card> = vec![
        sec_grids,
        sec_chunkgen,
        sec_forestry,
        sec_dungeons,
        sec_thaumcraft,
        sec_gtores,
        sec_gtfluids,
        sec_spawn,
        sec_structures,
    ];
    let t0 = std::time::Instant::now();
    // wasm32-wasip1 has no threads: run the sections in order there
    #[cfg(target_os = "wasi")]
    let cards: Vec<Card> = secs.iter().map(|&f| f()).collect();
    #[cfg(not(target_os = "wasi"))]
    let cards: Vec<Card> = std::thread::scope(|s| {
        let hs: Vec<_> = secs
            .iter()
            .map(|&f| {
                std::thread::Builder::new()
                    .stack_size(64 << 20)
                    .spawn_scoped(s, f)
                    .unwrap()
            })
            .collect();
        hs.into_iter()
            .enumerate()
            .map(|(k, h)| {
                h.join().unwrap_or_else(|e| {
                    let msg = e
                        .downcast_ref::<String>()
                        .cloned()
                        .or_else(|| e.downcast_ref::<&str>().map(|s| s.to_string()));
                    let mut c = Card::new("section");
                    c.exact(
                        &format!("section #{k} ran"),
                        false,
                        format!("panicked: {msg:?}"),
                    );
                    c
                })
            })
            .collect()
    });
    let mut out = format!(
        "\n===== second world: seed {SEED} RWG2_CONTINENTS, spawn {SPAWN:?} ({:.1?}) =====\n",
        t0.elapsed()
    );
    for c in &cards {
        let _ = writeln!(out, "\n## {}", c.name);
        for r in &c.rows {
            let _ = writeln!(out, "{r}");
        }
    }
    let _ = writeln!(out, "\n===== details =====");
    for c in &cards {
        if !c.log.is_empty() {
            let _ = writeln!(out, "\n## {}\n{}", c.name, c.log);
        }
    }
    println!("{out}");
    let fails: Vec<&String> = cards.iter().flat_map(|c| &c.fails).collect();
    assert!(
        fails.is_empty(),
        "exact checks failed:\n{}",
        fails
            .iter()
            .map(|s| s.as_str())
            .collect::<Vec<_>>()
            .join("\n")
    );
}
