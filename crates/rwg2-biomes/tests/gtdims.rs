//! GT ore-vein placement test in the Nether / Twilight Forest (`gtdims.rs`) vs the real ore blocks rwg2dump
//! scanned there (`dims.json` gtScan: chunks ±12 force-loaded z outer / x inner from (−12, −12), a fresh
//! dimension). Worlds: dump28 (seed 92900795) + the 7 held-out worlds in `worlds/`.
//!
//! Per oreseed the real vein is the first candidate whose ores are in the oreseed chunk at its Y; "empty" =
//! no GT vein ores there. Compared: attempt 0 (what the map showed before), the model with the dump's exact load
//! order (`LoadSim` replay = ceiling of the placement test itself), and the viewer rule (`resolve_dim`, arrival
//! at the origin).
//!
//! `cargo test --release -p rwg2-biomes --features json --test gtdims -- --nocapture`
#![cfg(feature = "json")]

use rwg2_biomes::gtdims::*;
use rwg2_biomes::gtorder::LoadSim;
use rwg2_biomes::gtores::*;
use rwg2_biomes::nether::*;
use serde_json::Value;
use std::collections::BTreeMap;
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

/// (label, dims.json) of every world with a dims.json.
fn worlds() -> Vec<(String, PathBuf)> {
    let base = root().join("testdata/pack/gtnh-2.9.0");
    let mut v = vec![("92900795".to_string(), base.join("dump28/dims.json"))];
    if let Ok(rd) = std::fs::read_dir(base.join("worlds")) {
        let mut ws: Vec<_> = rd.filter_map(|e| e.ok()).map(|e| e.path()).collect();
        ws.sort();
        for w in ws {
            v.push((
                w.file_name().unwrap().to_string_lossy().to_string(),
                w.join("dims.json"),
            ));
        }
    }
    if let Ok(only) = std::env::var("WORLDS") {
        let keep: Vec<&str> = only.split(',').collect();
        v.retain(|w| keep.contains(&w.0.as_str()));
    }
    v.into_iter().filter(|w| w.1.exists()).collect()
}

#[derive(Default, Clone, Copy, Debug)]
struct Score {
    oreseeds: usize,
    /// oreseeds with a real vein in the oreseed chunk
    real: usize,
    empty: usize,
    a0: usize,
    exact_order: usize,
    viewer: usize,
    /// empty oreseeds the model (exact order) also leaves empty
    empty_ok: usize,
    chunks: usize,
}

impl std::ops::AddAssign for Score {
    fn add_assign(&mut self, o: Score) {
        self.oreseeds += o.oreseeds;
        self.real += o.real;
        self.empty += o.empty;
        self.a0 += o.a0;
        self.exact_order += o.exact_order;
        self.viewer += o.viewer;
        self.empty_ok += o.empty_ok;
        self.chunks += o.chunks;
    }
}

fn pct(a: usize, b: usize) -> String {
    format!("{a}/{b} = {:.1}%", 100.0 * a as f64 / b.max(1) as f64)
}

fn score_world(d: &Value, dim: i32, key: &str, rows: &[RawVein]) -> Option<Score> {
    let seed = int(&d["dims"][dim.to_string()]["worldSeed"]);
    let table = vein_table(rows);
    let ids: Vec<Vec<i32>> = d["gt"][key]
        .as_array()?
        .iter()
        .map(|v| {
            ["primary", "secondary", "between", "sporadic"]
                .iter()
                .map(|k| int(&v[k]["id"]) as i32)
                .collect()
        })
        .collect();
    let scan = &d["gtScan"][dim.to_string()];
    let rc = int(&scan["rangeChunks"]) as i32;
    let mut obs: BTreeMap<(i32, i32), BTreeMap<i32, (i32, i32)>> = BTreeMap::new();
    for c in scan["chunks"].as_array()? {
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
    // the dump's load order: chunks overlapping [-r*16, (r+2)*16-1]^2, z outer / x inner, fresh dimension
    let mut sim = LoadSim::new();
    sim.area_z_major(-rc * 16, -rc * 16, (rc + 2) * 16 - 1, (rc + 2) * 16 - 1);
    let first = sim.first_processed(|x, z| is_oreseed(&GT_GTNH, x, z));
    let mut s = DimStone::new(dim, seed)?;
    let mut sc = Score::default();
    for cz in -rc + 1..rc {
        for cx in -rc + 1..rc {
            if !is_oreseed(&GT_GTNH, cx, cz) {
                continue;
            }
            sc.oreseeds += 1;
            let seen = obs.get(&(cx, cz)).cloned().unwrap_or_default();
            let cands = gt_candidates(
                seed,
                dim,
                &table,
                &GT_GTNH,
                cx,
                cz,
                GT_GTNH.max_placement_attempts + 1,
            );
            let real = if seen.is_empty() {
                None
            } else {
                cands.iter().position(|c| {
                    let want = &ids[c.vein];
                    let hit = want
                        .iter()
                        .filter(|m| {
                            seen.get(m)
                                .is_some_and(|v| v.0 <= c.min_y + 8 && v.1 >= c.min_y - 1)
                        })
                        .count();
                    hit >= want.len().min(2)
                })
            };
            let (px, pz) = first[&(cx, cz)];
            let m_exact = resolve_dim_at(seed, dim, &table, &GT_GTNH, cx, cz, px, pz, &mut s);
            let m_view = resolve_dim(seed, dim, &table, &GT_GTNH, cx, cz, &mut s);
            let same =
                |m: &Option<Placement>, i: usize| m.is_some_and(|p| p.rejected as usize == i);
            match real {
                Some(i) => {
                    sc.real += 1;
                    sc.a0 += (i == 0) as usize;
                    sc.exact_order += same(&m_exact, i) as usize;
                    sc.viewer += same(&m_view, i) as usize;
                }
                None if seen.is_empty() => {
                    sc.empty += 1;
                    sc.empty_ok += m_exact.is_none() as usize;
                }
                None => {} // ores seen but no candidate matches (seed maths miss; asserted elsewhere)
            }
        }
    }
    sc.chunks = s.generated;
    Some(sc)
}

#[test]
fn gt_dim_placement_vs_dumps() {
    let ws = worlds();
    if ws.is_empty() {
        eprintln!("skip: no dims.json");
        return;
    }
    let run = |label: &str, path: &PathBuf| -> Vec<(i32, Score)> {
        let Ok(txt) = std::fs::read_to_string(path) else {
            return vec![];
        };
        let d: Value = serde_json::from_str(&txt).unwrap();
        let mut out = Vec::new();
        for (dim, key, rows) in [
            (-1, "Nether", &GT_NETHER_VEINS[..]),
            (7, "Twilight Forest", &GT_TF_VEINS[..]),
        ] {
            let t0 = std::time::Instant::now();
            if let Some(sc) = score_world(&d, dim, key, rows) {
                eprintln!(
                    "{label} {key}: oreseeds {} real {} empty {} | attempt0 {} | model exact-order {} | viewer rule {} | empty kept empty {} | {} chunks {:?}",
                    sc.oreseeds,
                    sc.real,
                    sc.empty,
                    pct(sc.a0, sc.real),
                    pct(sc.exact_order, sc.real),
                    pct(sc.viewer, sc.real),
                    pct(sc.empty_ok, sc.empty),
                    sc.chunks,
                    t0.elapsed()
                );
                out.push((dim, sc));
            }
        }
        out
    };
    #[cfg(not(target_os = "wasi"))]
    let all: Vec<Vec<(i32, Score)>> = std::thread::scope(|s| {
        let hs: Vec<_> = ws.iter().map(|(l, p)| s.spawn(move || run(l, p))).collect();
        hs.into_iter().map(|h| h.join().unwrap()).collect()
    });
    #[cfg(target_os = "wasi")]
    let all: Vec<Vec<(i32, Score)>> = ws.iter().take(1).map(|(l, p)| run(l, p)).collect();
    let mut tot: BTreeMap<i32, Score> = BTreeMap::new();
    for w in all {
        for (dim, sc) in w {
            *tot.entry(dim).or_default() += sc;
        }
    }
    for (dim, sc) in &tot {
        eprintln!(
            "TOTAL dim {dim}: real {} | attempt0 {} | model exact-order {} | viewer rule {} | empty kept empty {}",
            sc.real,
            pct(sc.a0, sc.real),
            pct(sc.exact_order, sc.real),
            pct(sc.viewer, sc.real),
            pct(sc.empty_ok, sc.empty)
        );
        assert!(
            sc.exact_order >= sc.a0,
            "dim {dim}: placement test worse than attempt 0"
        );
    }
}

#[test]
fn processing_chunk_dim_rules() {
    // inside the arrival square: window NW corner clipped to the square
    assert_eq!(processing_chunk_dim(1, 1, (0, 0), 10), (-1, -1));
    assert_eq!(processing_chunk_dim(-11, 4, (0, 0), 10), (-10, 2));
    // outside: corner facing the arrival chunk
    assert_eq!(processing_chunk_dim(40, 40, (0, 0), 10), (38, 38));
    assert_eq!(processing_chunk_dim(-40, 40, (0, 0), 10), (-38, 38));
}

/// Cost of the viewer path: every oreseed of one 2048-block cell (and of one 1024 quadrant).
#[test]
#[ignore]
fn gt_dim_cell_cost() {
    for (dim, rows) in [(-1, &GT_NETHER_VEINS[..]), (7, &GT_TF_VEINS[..])] {
        let table = vein_table(rows);
        for size in [64, 128] {
            let mut s = DimStone::new(dim, 92_900_795).unwrap();
            let t0 = std::time::Instant::now();
            let mut n = 0;
            for cz in 128..128 + size {
                for cx in 128..128 + size {
                    if is_oreseed(&GT_GTNH, cx, cz) {
                        n += resolve_dim(92_900_795, dim, &table, &GT_GTNH, cx, cz, &mut s)
                            .is_some() as usize;
                    }
                }
            }
            eprintln!(
                "dim {dim} {size}² chunks: {n} veins, {} chunks generated, {:?}",
                s.generated,
                t0.elapsed()
            );
        }
    }
}

/// Profile of the search path: chunk cost, and chunks / oreseeds per seed for "nearest vein #1 within 300".
#[test]
#[ignore]
fn gt_dim_search_profile() {
    use rwg2_biomes::dimterrain::{NetherTerrain, TfTerrain};
    let n = NetherTerrain::new(5);
    let t = TfTerrain::new(5);
    let t0 = std::time::Instant::now();
    for i in 0..50 {
        std::hint::black_box(n.chunk(i, 3));
    }
    eprintln!("nether chunk {:?}", t0.elapsed() / 50);
    let t0 = std::time::Instant::now();
    for i in 0..50 {
        std::hint::black_box(t.chunk(i, 3));
    }
    eprintln!("tf chunk {:?}", t0.elapsed() / 50);
    let t0 = std::time::Instant::now();
    for i in 0..50 {
        std::hint::black_box(t.chunk_basic(i, 3));
    }
    eprintln!("tf chunk_basic {:?}", t0.elapsed() / 50);
    let t0 = std::time::Instant::now();
    for i in 0..50 {
        std::hint::black_box(NetherTerrain::new(i));
    }
    eprintln!("nether new {:?}", t0.elapsed() / 50);
    let t0 = std::time::Instant::now();
    for i in 0..10 {
        std::hint::black_box(TfTerrain::new(i));
    }
    eprintln!("tf new {:?}", t0.elapsed() / 10);
    for (dim, rows) in [(-1, &GT_NETHER_VEINS[..]), (7, &GT_TF_VEINS[..])] {
        let table = vein_table(rows);
        let (mut chunks, mut tested, mut hits) = (0, 0, 0);
        let t0 = std::time::Instant::now();
        for seed in 0..20i64 {
            let mut s = DimStone::new(dim, seed).unwrap();
            let mut v: Vec<(i32, i32)> = (-20..=20)
                .flat_map(|z| (-20..=20).map(move |x| (x, z)))
                .filter(|&(x, z)| {
                    is_oreseed(&GT_GTNH, x, z)
                        && (x * 16 + 8).pow(2) + (z * 16 + 8).pow(2) <= 300 * 300
                })
                .collect();
            v.sort_by_key(|&(x, z)| (x * 16 + 8).pow(2) + (z * 16 + 8).pow(2));
            for (cx, cz) in v {
                if !gt_candidates(seed, dim, &table, &GT_GTNH, cx, cz, 8)
                    .iter()
                    .any(|p| p.vein == 1)
                {
                    continue;
                }
                tested += 1;
                if resolve_dim(seed, dim, &table, &GT_GTNH, cx, cz, &mut s)
                    .is_some_and(|p| p.vein == 1)
                {
                    hits += 1;
                    break;
                }
            }
            chunks += s.generated;
        }
        eprintln!(
            "dim {dim}: 20 seeds {:?}, tested oreseeds {tested}, hits {hits}, chunks {chunks}",
            t0.elapsed()
        );
        // the old search (attempt 0 only), same loop
        let t0 = std::time::Instant::now();
        let mut hits0 = 0;
        for seed in 0..2000i64 {
            let mut v: Vec<(i32, i32)> = (-20..=20)
                .flat_map(|z| (-20..=20).map(move |x| (x, z)))
                .filter(|&(x, z)| {
                    is_oreseed(&GT_GTNH, x, z)
                        && (x * 16 + 8).pow(2) + (z * 16 + 8).pow(2) <= 300 * 300
                })
                .collect();
            v.sort_by_key(|&(x, z)| (x * 16 + 8).pow(2) + (z * 16 + 8).pow(2));
            hits0 += v.into_iter().any(|(cx, cz)| {
                gt_candidates(seed, dim, &table, &GT_GTNH, cx, cz, 1)
                    .first()
                    .is_some_and(|p| p.vein == 1)
            }) as usize;
        }
        eprintln!(
            "dim {dim}: attempt-0 search {:.0} seeds/s ({hits0}/2000)",
            2000.0 / t0.elapsed().as_secs_f64()
        );
    }
}
