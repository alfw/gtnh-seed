//! Nether + Twilight Forest vs rwg2dump 2.8 (`testdata/pack/gtnh-2.9.0/dump28/dims.json` + grids), world seed
//! 92900795. Biome grids for 9 seeds (blocks + 1:4 layer), TF feature regions + major-feature starts, nether fortress
//! starts + boxes, GT ore veins in dims −1 / 7.
//!
//! `cargo test --release -p rwg2-biomes --features json --test dims_dump28 -- --nocapture`
#![cfg(feature = "json")]

use rwg2_biomes::nether::*;
use rwg2_biomes::twilight::*;
use serde_json::Value;
use std::collections::{BTreeMap, BTreeSet};
use std::path::PathBuf;

const SEED: i64 = 92_900_795;

fn root() -> PathBuf {
    std::env::var("RWG2_DATA_DIR")
        .map(|d| PathBuf::from(d).parent().unwrap().to_path_buf())
        .unwrap_or_else(|_| PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../.."))
}
fn d28() -> PathBuf {
    root().join("testdata/pack/gtnh-2.9.0/dump28")
}
fn dims() -> Option<Value> {
    serde_json::from_str(&std::fs::read_to_string(d28().join("dims.json")).ok()?).ok()
}
fn u16s(file: &str) -> Option<Vec<i32>> {
    let b = std::fs::read(d28().join(file)).ok()?;
    Some(
        b.chunks_exact(2)
            .map(|c| u16::from_le_bytes([c[0], c[1]]) as i32)
            .collect(),
    )
}
fn int(v: &Value) -> i64 {
    v.as_i64()
        .unwrap_or_else(|| v.as_str().unwrap().parse().unwrap())
}

fn grids(key: &str, f: impl Fn(i64, bool, i32, i32, i32, i32) -> Vec<i32>) -> (usize, usize) {
    let Some(d) = dims() else {
        eprintln!("dump28 missing, skipped");
        return (0, 0);
    };
    let (mut n, mut bad) = (0, 0);
    for e in d[key].as_array().unwrap() {
        let seed = int(&e["seed"]);
        let q = e["layer"].as_str().unwrap().contains("1:4");
        let (x, z, w, h) = (
            int(&e["x"]) as i32,
            int(&e["z"]) as i32,
            int(&e["w"]) as i32,
            int(&e["h"]) as i32,
        );
        let Some(real) = u16s(e["file"].as_str().unwrap()) else {
            continue;
        };
        let ours = f(seed, q, x, z, w, h);
        let m = real.iter().zip(&ours).filter(|(a, b)| a != b).count();
        if m > 0 {
            let i = real.iter().zip(&ours).position(|(a, b)| a != b).unwrap();
            eprintln!(
                "{key} {} : {m} mismatches, first at ({}, {}) real {} ours {}",
                e["file"],
                x + i as i32 % w,
                z + i as i32 / w,
                real[i],
                ours[i]
            );
        }
        n += real.len();
        bad += m;
    }
    eprintln!("{key}: {n} samples, {bad} mismatches");
    (n, bad)
}

#[test]
fn tf_biome_grids() {
    let (_, bad) = grids("tfBiomes", |s, q, x, z, w, h| {
        let tf = TwilightForest::new(s);
        if q {
            tf.biomes_quarter(x, z, w, h)
        } else {
            tf.biomes(x, z, w, h)
        }
    });
    assert_eq!(bad, 0);
}

#[test]
fn nether_biome_grids() {
    let (_, bad) = grids("netherBiomes", |s, q, x, z, w, h| {
        let nb = NetherBiomes::new(s);
        if q {
            nb.biomes_quarter(x, z, w, h)
        } else {
            nb.biomes(x, z, w, h)
        }
    });
    assert_eq!(bad, 0);
    let Some(d) = dims() else { return };
    let list: Vec<(i64, i64)> = d["nether"]["bopNetherBiomes"]
        .as_array()
        .unwrap()
        .iter()
        .map(|e| (int(&e[0]), int(&e[1])))
        .collect();
    let ours: Vec<(i64, i64)> = BOP_NETHER_BIOMES_GTNH[1..]
        .iter()
        .map(|b| (b.0 as i64, b.1 as i64))
        .collect();
    assert_eq!(list, ours);
    eprintln!("nether biome histogram {}", d["nether"]["biomeHistogram"]);
}

#[test]
fn tf_features() {
    let Some(d) = dims() else { return };
    let tf = TwilightForest::new(SEED);
    let f = &d["tfFeatures"];
    let (mut n, mut bad_c, mut bad_id) = (0, 0, 0);
    for r in f["regions"].as_array().unwrap() {
        let v: Vec<i64> = (0..6).map(|i| int(&r[i])).collect();
        let at = tf.feature_for_region(v[0] as i32, v[1] as i32);
        let (cx, cz) = region_center_block(v[0] as i32, v[1] as i32);
        n += 1;
        if (cx as i64, cz as i64) != (v[2], v[3]) {
            bad_c += 1;
            if bad_c < 4 {
                eprintln!(
                    "region {:?}: centre ours ({cx},{cz}) real ({},{})",
                    (v[0], v[1]),
                    v[2],
                    v[3]
                );
            }
        }
        // the map draws "No Feature" (id 0) as nothing; compare the 1.7 feature id
        if at.feature.id as i64 != v[4] {
            bad_id += 1;
            if bad_id < 6 {
                eprintln!(
                    "region {:?}: ours {} ({}) real gen17 {} direct {} biome {}",
                    (v[0], v[1]),
                    at.feature.id,
                    at.feature.name,
                    v[4],
                    v[5],
                    int(&r[7])
                );
            }
        }
    }
    eprintln!("tf regions {n}: centre mismatches {bad_c}, feature-id mismatches {bad_id}");
    // structure starts: every real start chunk must be one of our feature chunks with a structure feature
    let (mut s_n, mut s_bad) = (0, 0);
    let mut classes = BTreeMap::new();
    for s in f["structures"].as_array().unwrap() {
        let (cx, cz) = (int(&s["chunkX"]) as i32, int(&s["chunkZ"]) as i32);
        *classes
            .entry(s["pieces"][0][0].as_str().unwrap_or("?").to_string())
            .or_insert(0) += 1;
        s_n += 1;
        match tf.feature_at_chunk(cx, cz) {
            Some(_) => {}
            None => {
                s_bad += 1;
                if s_bad < 4 {
                    eprintln!("structure start chunk ({cx},{cz}) not a feature chunk");
                }
            }
        }
    }
    eprintln!("tf structure starts {s_n}, not a feature chunk {s_bad}; first pieces {classes:?}");
    assert_eq!(bad_c, 0);
    assert_eq!(bad_id, 0);
    assert_eq!(s_bad, 0);
}

#[test]
fn nether_fortresses() {
    let Some(d) = dims() else { return };
    let ne = &d["nether"];
    let r = int(&ne["checkRadiusChunks"]) as i32;
    let real: BTreeSet<(i32, i32)> = ne["starts"]
        .as_array()
        .unwrap()
        .iter()
        .map(|s| (int(&s[0]) as i32, int(&s[1]) as i32))
        .collect();
    let ours: BTreeSet<(i32, i32)> = fortresses_in(SEED, -r, -r, r, r)
        .into_iter()
        .map(|f| (f.chunk_x, f.chunk_z))
        .filter(|&(x, z)| x.abs() <= r && z.abs() <= r)
        .collect();
    let miss: Vec<_> = real.difference(&ours).take(5).collect();
    let extra: Vec<_> = ours.difference(&real).take(5).collect();
    eprintln!(
        "fortress starts real {} ours {} missing {:?} extra {:?}",
        real.len(),
        ours.len(),
        miss,
        extra
    );
    // start piece y
    let mut ys = BTreeMap::new();
    for s in ne["structures"].as_array().unwrap() {
        let p = &s["pieces"][0];
        *ys.entry(int(&p[2])).or_insert(0) += 1;
    }
    eprintln!("start piece minY histogram {ys:?}");
    assert_eq!(real, ours);
}

/// The source-derived vein lists equal the live lists (order, weights, heights, size, density, ore ids).
#[test]
fn gt_vein_lists_are_live() {
    let Some(d) = dims() else { return };
    let ids = gt_material_ids(&d);
    for (key, rows) in [
        ("Nether", &GT_NETHER_VEINS[..]),
        ("Twilight Forest", &GT_TF_VEINS[..]),
    ] {
        let live = d["gt"][key].as_array().unwrap();
        assert_eq!(live.len(), rows.len(), "{key}");
        for (v, r) in live.iter().zip(rows) {
            let got = (
                v["name"].as_str().unwrap(),
                int(&v["weight"]),
                int(&v["minY"]),
                int(&v["maxY"]),
                int(&v["size"]),
                int(&v["density"]),
                int(&v["primary"]["id"]),
            );
            assert_eq!(
                got,
                (r.0, r.1 as i64, r.2 as i64, r.3 as i64, r.4 as i64, r.5 as i64, r.6 as i64),
                "{key}"
            );
            let names: Vec<&str> = ["primary", "secondary", "between", "sporadic"]
                .iter()
                .map(|k| v[k]["name"].as_str().unwrap())
                .collect();
            assert_eq!(names, r.7.to_vec(), "{key} {}", r.0);
        }
        assert_eq!(ids[key].len(), rows.len());
    }
}

/// `dim name -> per-vein ore material ids` from the live lists.
fn gt_material_ids(d: &Value) -> BTreeMap<String, Vec<BTreeSet<i32>>> {
    let mut out = BTreeMap::new();
    for key in ["Nether", "Twilight Forest"] {
        let v = d["gt"][key]
            .as_array()
            .unwrap()
            .iter()
            .map(|v| {
                ["primary", "secondary", "between", "sporadic"]
                    .iter()
                    .map(|k| int(&v[k]["id"]) as i32)
                    .collect()
            })
            .collect();
        out.insert(key.to_string(), v);
    }
    out
}

/// Real GT ore blocks in ±12 chunks of dims −1 / 7 vs the oreseed candidates: is the real vein one of the first
/// `max_placement_attempts` picks, and which attempt (no terrain model in these dims, so attempt 0 is what the map shows).
#[test]
fn gt_ore_scan_dims() {
    use rwg2_biomes::gtores::*;
    let Some(d) = dims() else { return };
    let ids = gt_material_ids(&d);
    for (dim, key, rows) in [
        (-1, "Nether", &GT_NETHER_VEINS[..]),
        (7, "Twilight Forest", &GT_TF_VEINS[..]),
    ] {
        let table = vein_table(rows);
        let scan = &d["gtScan"][dim.to_string()];
        let rc = int(&scan["rangeChunks"]) as i32;
        let mut obs: BTreeMap<(i32, i32), BTreeMap<i32, (i32, i32)>> = BTreeMap::new();
        for c in scan["chunks"].as_array().unwrap() {
            let e = obs
                .entry((int(&c["cx"]) as i32, int(&c["cz"]) as i32))
                .or_default();
            for o in c["ores"].as_array().unwrap() {
                let o: Vec<i64> = o.as_array().unwrap().iter().map(int).collect();
                if o[1] == 0 {
                    e.insert(o[0] as i32, (o[3] as i32, o[4] as i32));
                }
            }
        }
        let (mut n, mut empty, mut found, mut y_ok) = (0, 0, 0, 0);
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
                    SEED,
                    dim,
                    &table,
                    &GT_GTNH,
                    cx,
                    cz,
                    GT_GTNH.max_placement_attempts + 1,
                );
                for (i, c) in cands.iter().enumerate() {
                    let want = &ids[key][c.vein];
                    let hit = want
                        .iter()
                        .filter(|m| {
                            seen.get(m)
                                .is_some_and(|v| v.0 <= c.min_y + 8 && v.1 >= c.min_y - 1)
                        })
                        .count();
                    if hit >= want.len().min(2) {
                        found += 1;
                        *hist.entry(i).or_insert(0) += 1;
                        let lo = want
                            .iter()
                            .filter_map(|m| seen.get(m))
                            .map(|v| v.0)
                            .min()
                            .unwrap();
                        let hi = want
                            .iter()
                            .filter_map(|m| seen.get(m))
                            .map(|v| v.1)
                            .max()
                            .unwrap();
                        if lo >= c.min_y - 1 && hi <= c.min_y + 8 {
                            y_ok += 1;
                        }
                        break;
                    }
                }
            }
        }
        eprintln!("{key}: oreseeds {n}, no ore blocks {empty}, real vein = a candidate {found}, Y inside {y_ok}, attempt index {hist:?}");
        assert!(
            found + empty >= n - 2,
            "{key}: seed maths {found}+{empty}/{n}"
        );
    }
}
