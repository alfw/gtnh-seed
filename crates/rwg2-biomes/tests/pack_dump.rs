//! Diffs against `rwg2dump` output from a real GTNH 2.9.0 instance (`testdata/pack/gtnh-2.9.0/dump/`).
//! Skips (with a note) if the dump isn't present.

use std::path::PathBuf;

use rwg2_biomes::climate::ClimateField;

struct Grid {
    seed: i64,
    world_type: String,
    region: String,
    x0: i32,
    z0: i32,
    w: usize,
    h: usize,
    step: i32,
    file: String,
    bytes: Vec<u8>,
}

impl Grid {
    fn n(&self) -> usize {
        self.w * self.h
    }
    fn meta(&self, idx: usize) -> u8 {
        self.bytes[4 * self.n() + idx]
    }
    fn xz(&self, idx: usize) -> (i32, i32) {
        let (i, j) = ((idx % self.w) as i32, (idx / self.w) as i32);
        (self.x0 + i * self.step, self.z0 + j * self.step)
    }
}

fn dump_dir() -> PathBuf {
    std::env::var("RWG2_DUMP_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(|_| {
            PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../testdata/pack/gtnh-2.9.0/dump")
        })
}

/// Tiny manifest reader (avoids a serde dependency): pulls the per-file objects out of manifest.json.
fn grids(filter: impl Fn(&str, &str) -> bool) -> Option<Vec<Grid>> {
    let dir = dump_dir();
    let manifest = std::fs::read_to_string(dir.join("manifest.json")).ok()?;
    let files = &manifest[manifest.find("\"files\":[")? + 9..];
    let mut out = Vec::new();
    for obj in files.split("},{") {
        let get = |k: &str| -> String {
            let pat = format!("\"{k}\":");
            let s = &obj[obj.find(&pat).unwrap() + pat.len()..];
            let s = s.trim_start_matches('"');
            s[..s.find(['"', ',', '}']).unwrap()].to_string()
        };
        let (wt, region) = (get("worldType"), get("region"));
        let _ = &dir;
        if !filter(&wt, &region) {
            continue;
        }
        let file = get("file");
        out.push(Grid {
            seed: get("seed").parse().unwrap(),
            world_type: wt,
            region,
            x0: get("x0").parse().unwrap(),
            z0: get("z0").parse().unwrap(),
            w: get("w").parse().unwrap(),
            h: get("h").parse().unwrap(),
            step: get("step").parse().unwrap(),
            file,
            bytes: Vec::new(), // loaded lazily: one grid in memory at a time
        });
    }
    Some(out)
}

#[test]
fn rwg2_climate_matches_pack() {
    let Some(gs) = grids(|wt, r| wt == "RWG2" && r != "heights" && !r.starts_with("chunks")) else {
        eprintln!("SKIP: no dump at {}", dump_dir().display());
        return;
    };
    assert!(!gs.is_empty());
    let (mut total, mut bad) = (0usize, 0usize);
    for g in &gs {
        let mut g = Grid {
            bytes: std::fs::read(dump_dir().join(&g.file)).unwrap(),
            file: g.file.clone(),
            world_type: g.world_type.clone(),
            region: g.region.clone(),
            ..*g
        };
        let g = &mut g;
        let field = ClimateField::new(g.seed, 1400.0, true);
        let mut gbad = 0;
        for idx in 0..g.n() {
            let (x, z) = g.xz(idx);
            let got = field.at(x, z) as u8;
            if got != g.meta(idx) {
                if gbad < 5 {
                    eprintln!(
                        "  seed {} {} ({x},{z}): got {got} want {}",
                        g.seed,
                        g.region,
                        g.meta(idx)
                    );
                }
                gbad += 1;
            }
        }
        eprintln!(
            "seed {:>20} {:5} {:>7} samples, {gbad} mismatches",
            g.seed,
            g.region,
            g.n()
        );
        total += g.n();
        bad += gbad;
    }
    assert_eq!(bad, 0, "{bad}/{total} climate mismatches vs real GTNH");
    eprintln!(
        "RWG2 climate: {total} samples from real GTNH 2.9.0, 0 mismatches ({} grids, {})",
        gs.len(),
        gs[0].world_type
    );
}

#[test]
fn rwg2_continents_value_matches_pack() {
    use rwg2_biomes::config::RwgConfig;
    use rwg2_biomes::continental::{ContinentalNoise, CONTINENTAL_SEED_XOR};
    let Some(gs) = grids(|wt, r| wt == "RWG2_CONTINENTS" && r != "heights" && !r.starts_with("chunks"))
    else {
        eprintln!("SKIP: no dump at {}", dump_dir().display());
        return;
    };
    let cfg = RwgConfig::default();
    let (mut total, mut bad) = (0usize, 0usize);
    let t0 = std::time::Instant::now();
    for g in &gs {
        let bytes = std::fs::read(dump_dir().join(&g.file)).unwrap();
        let n = g.n();
        let cont = ContinentalNoise::new(g.seed ^ CONTINENTAL_SEED_XOR, &cfg);
        let mut gbad = 0;
        for idx in 0..n {
            let (x, z) = g.xz(idx);
            let o = 5 * n + idx * 4;
            let want = f32::from_le_bytes(bytes[o..o + 4].try_into().unwrap());
            let got = cont.value(x, z);
            if got.to_bits() != want.to_bits() {
                if gbad < 5 {
                    eprintln!(
                        "  seed {} {} ({x},{z}): got {got} want {want}",
                        g.seed, g.region
                    );
                }
                gbad += 1;
            }
        }
        total += n;
        bad += gbad;
    }
    eprintln!(
        "CONTINENTS continent value: {total} samples, {bad} mismatches, {:.1?}",
        t0.elapsed()
    );
    assert_eq!(bad, 0);
}

fn table() -> rwg2_biomes::provider::BiomeTable {
    let text = std::fs::read_to_string(dump_dir().join("biomes.json")).unwrap();
    rwg2_biomes::provider::BiomeTable::from_rwg2dump_json(&text).unwrap()
}

fn check_biomes(world_type: &str) {
    use rwg2_biomes::config::RwgConfig;
    use rwg2_biomes::provider::{BiomeProvider, Heights, NoHeights, TableHeights, WorldType};
    let Some(gs) = grids(|wt, r| wt == world_type && r != "heights" && !r.starts_with("chunks")) else {
        eprintln!("SKIP: no dump at {}", dump_dir().display());
        return;
    };
    let wt = if world_type == "RWG2" {
        WorldType::Rwg2
    } else {
        WorldType::Rwg2Continents
    };
    let tbl = table();
    // exact gates when the table carries terrain models (v2 dump); else assume-open
    let hts: &dyn Heights = if tbl.heights.is_empty() {
        &NoHeights
    } else {
        &TableHeights
    };
    let t0 = std::time::Instant::now();
    let (mut total, mut bad_data, mut bad_gen, mut bad_meta) = (0usize, 0usize, 0usize, 0usize);
    let mut cur_seed = None;
    let mut p: Option<BiomeProvider> = None;
    let (mut extra_marker, mut missing_marker) = (0usize, 0usize);
    let mut caves = std::collections::BTreeSet::new();
    for g in &gs {
        if cur_seed != Some(g.seed) {
            p = Some(BiomeProvider::new(
                g.seed,
                wt,
                tbl.clone(),
                RwgConfig::default(),
            ));
            cur_seed = Some(g.seed);
        }
        let p = p.as_ref().unwrap();
        let bytes = std::fs::read(dump_dir().join(&g.file)).unwrap();
        let n = g.n();
        let mut shown = 0;
        for idx in 0..n {
            let (x, z) = g.xz(idx);
            let want_data = u16::from_le_bytes([bytes[2 * idx], bytes[2 * idx + 1]]);
            let want_gen = u16::from_le_bytes([bytes[2 * n + 2 * idx], bytes[2 * n + 2 * idx + 1]]);
            let want_meta = bytes[4 * n + idx];
            let s = p.biome_data(x, z, hts);
            let gen = p.biome_gen_id(x, z, hts);
            let (bd, bg, bm) = (s.data != want_data, gen != want_gen, s.meta != want_meta);
            if bg {
                let marker = tbl.lava_cave_marker_gen_id;
                if Some(gen) == marker && Some(want_gen) != marker {
                    extra_marker += 1;
                    if let Some(c) = p.continents() {
                        caves.insert((g.seed, c.lava_cave_center_coordinates(x, z)));
                    }
                } else if Some(want_gen) == marker {
                    missing_marker += 1;
                }
            }
            if bd || bg || bm {
                if shown < 6 {
                    let i = |id: u16| {
                        tbl.biomes
                            .get(id as usize)
                            .map(|b| b.name.clone())
                            .unwrap_or_default()
                    };
                    eprintln!(
                        "  seed {} {} ({x},{z}): data {}({}) want {}({}) | gen {gen} want {want_gen} | meta {} want {want_meta}",
                        g.seed, g.region, s.data, i(s.data), want_data, i(want_data), s.meta
                    );
                    shown += 1;
                }
                bad_data += bd as usize;
                bad_gen += bg as usize;
                bad_meta += bm as usize;
            }
        }
        total += n;
    }
    let assumed = p.as_ref().map(|p| p.assumed_gates.get()).unwrap_or(0);
    eprintln!(
        "{world_type} biomes: {total} samples | data mismatches {bad_data} | gen mismatches {bad_gen} | meta mismatches {bad_meta} | {:.1?} (last seed assumed {assumed} height gates)",
        t0.elapsed()
    );
    if bad_gen > 0 {
        eprintln!(
            "  gen mismatches: {extra_marker} extra lava-marker samples (caves the game rejected by height: {:?}), {missing_marker} missing",
            caves
        );
    }
    // With terrain models the gates are exact: nothing tolerated. Without (v1 dump): only extra lava markers.
    let unexplained_gen = if tbl.heights.is_empty() {
        bad_gen - extra_marker
    } else {
        bad_gen
    };
    assert_eq!(
        (bad_data, unexplained_gen, bad_meta),
        (0, 0, 0),
        "{world_type} biome mismatches vs real GTNH"
    );
}

#[test]
fn rwg2_biomes_match_pack() {
    check_biomes("RWG2");
}

#[test]
fn rwg2_continents_biomes_match_pack() {
    check_biomes("RWG2_CONTINENTS");
}

/// v2 dump: `getNoiseAt` height grids, bit-exact.
#[test]
fn heights_match_pack() {
    use rwg2_biomes::config::RwgConfig;
    use rwg2_biomes::provider::{BiomeProvider, TableHeights, WorldType};
    if !dump_dir().join("biomes.json").exists() {
        eprintln!("SKIP: no dump at {}", dump_dir().display());
        return;
    }
    let tbl = table();
    if tbl.heights.is_empty() {
        eprintln!("SKIP: dump has no terrain models (needs rwg2dump v2)");
        return;
    }
    let Some(gs) = grids(|_, r| r == "heights") else {
        return;
    };
    let (mut total, mut bad) = (0usize, 0usize);
    let mut worst = 0f32;
    let mut by_biome: std::collections::BTreeMap<u16, (usize, usize)> = Default::default();
    for g in &gs {
        let wt = if g.world_type == "RWG2" {
            WorldType::Rwg2
        } else {
            WorldType::Rwg2Continents
        };
        let p = BiomeProvider::new(g.seed, wt, tbl.clone(), RwgConfig::default());
        let bytes = std::fs::read(dump_dir().join(&g.file)).unwrap();
        let mut shown = 0;
        for idx in 0..g.n() {
            let (x, z) = g.xz(idx);
            let want = f32::from_le_bytes(bytes[idx * 4..idx * 4 + 4].try_into().unwrap());
            let got = p.noise_at(x, z, &TableHeights).unwrap();
            let biome = p.biome_data(x, z, &TableHeights).data;
            let e = by_biome.entry(biome).or_default();
            e.0 += 1;
            if got.to_bits() != want.to_bits() {
                e.1 += 1;
                bad += 1;
                worst = worst.max((got - want).abs());
                if shown < 4 {
                    eprintln!(
                        "  seed {} {} ({x},{z}) biome {} {}: got {got} want {want}",
                        g.seed,
                        g.world_type,
                        biome,
                        tbl.biomes[biome as usize].class.rsplit('.').next().unwrap()
                    );
                    shown += 1;
                }
            }
        }
        total += g.n();
    }
    let failing: Vec<_> = by_biome
        .iter()
        .filter(|(_, v)| v.1 > 0)
        .map(|(k, v)| (*k, v.1, v.0))
        .collect();
    eprintln!("heights: {total} samples, {bad} mismatches (max |d| {worst}); failing biomes (id, bad, n): {failing:?}");
    assert_eq!(bad, 0);
}

/// v2 dump: landmark gates (volcano base height / eligibility, lava-cave centre height / eligibility).
#[test]
fn landmark_gates_match_pack() {
    use rwg2_biomes::config::RwgConfig;
    use rwg2_biomes::provider::{BiomeProvider, TableHeights, WorldType};
    if !dump_dir().join("biomes.json").exists() {
        eprintln!("SKIP: no dump at {}", dump_dir().display());
        return;
    }
    let tbl = table();
    let Ok(text) = std::fs::read_to_string(dump_dir().join("landmarks.json")) else {
        eprintln!("SKIP: no landmarks.json (needs rwg2dump v2)");
        return;
    };
    if tbl.heights.is_empty() {
        eprintln!("SKIP: no terrain models");
        return;
    }
    let v: serde_json::Value = serde_json::from_str(&text).unwrap();
    let bits = |o: &serde_json::Value| {
        f32::from_bits(u32::from_str_radix(o["bits"].as_str().unwrap(), 16).unwrap())
    };
    let (mut n, mut bad) = (0, 0);
    for s in v["seeds"].as_array().unwrap() {
        let seed: i64 = s["seed"].as_str().unwrap().parse().unwrap();
        let p = BiomeProvider::new(
            seed,
            WorldType::Rwg2Continents,
            tbl.clone(),
            RwgConfig::default(),
        );
        for vo in s["volcanoes"].as_array().unwrap() {
            let (x, z) = (
                vo["x"].as_i64().unwrap() as i32,
                vo["z"].as_i64().unwrap() as i32,
            );
            let base = p.volcano_base_height(x, z, &TableHeights).unwrap();
            let elig = p.volcano_coordinates(x, z, &TableHeights) != rwg2_biomes::continental::NONE;
            let ok = base.to_bits() == bits(&vo["baseHeight"]).to_bits()
                && elig == vo["eligible"].as_bool().unwrap();
            n += 1;
            if !ok {
                bad += 1;
                eprintln!(
                    "  volcano seed {seed} ({x},{z}): base {base} want {} elig {elig} want {}",
                    bits(&vo["baseHeight"]),
                    vo["eligible"]
                );
            }
        }
        for lo in s["lavaCaves"].as_array().unwrap() {
            let (x, z) = (
                lo["x"].as_i64().unwrap() as i32,
                lo["z"].as_i64().unwrap() as i32,
            );
            let h = p.noise_at(x, z, &TableHeights).unwrap();
            let want = bits(&lo["noiseAtCenter"]);
            let ok =
                h.to_bits() == want.to_bits() && (h > 63.0) == lo["eligible"].as_bool().unwrap();
            n += 1;
            if !ok {
                bad += 1;
                eprintln!(
                    "  lava seed {seed} ({x},{z}): noise {h} want {want} eligible(game) {}",
                    lo["eligible"]
                );
            }
        }
    }
    eprintln!("landmark gates: {n} landmarks, {bad} mismatches");
    assert_eq!(bad, 0);
}

/// v2 dump: stronghold ring for the loaded world.
#[test]
fn stronghold_ring_matches_pack() {
    let Ok(text) = std::fs::read_to_string(dump_dir().join("structures.json")) else {
        eprintln!("SKIP: no structures.json");
        return;
    };
    let v: serde_json::Value = serde_json::from_str(&text).unwrap();
    let seed: i64 = v["worldSeed"].as_str().unwrap().parse().unwrap();
    let want: Vec<(i32, i32)> = v["strongholdGenerator"]["ring"]
        .as_array()
        .unwrap()
        .iter()
        .map(|p| {
            (
                p["chunkX"].as_i64().unwrap() as i32,
                p["chunkZ"].as_i64().unwrap() as i32,
            )
        })
        .collect();
    let got = rwg2_biomes::structures::stronghold_ring(seed);
    eprintln!("stronghold ring seed {seed}: got {got:?} want {want:?}");
    assert_eq!(got.to_vec(), want);
}
