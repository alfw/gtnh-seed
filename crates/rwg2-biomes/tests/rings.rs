#![cfg(feature = "json")]
//! Thaumcraft eldritch rings (obelisks) vs every dumped world: per rolled ring, the model's inputs and verdict next
//! to whether the game built it (a `TileEldritchAltar` at the roll position). `RINGS_DIAG=1` prints every ring.
use rwg2_biomes::config::RwgConfig;
use rwg2_biomes::dungeons::Terrain;
use rwg2_biomes::forestry::ClimateTable;
use rwg2_biomes::provider::{BiomeProvider, BiomeTable, TableHeights, WorldType};
use rwg2_biomes::thaumcraft::*;
use serde_json::Value;
use std::path::PathBuf;

fn root() -> PathBuf {
    std::env::var("RWG2_DATA_DIR")
        .map(|d| PathBuf::from(d).parent().unwrap().to_path_buf())
        .unwrap_or_else(|_| PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../.."))
}

fn gates() -> rwg2_biomes::population::PreGates {
    let cj: Value = serde_json::from_str(
        &std::fs::read_to_string(root().join(pack_rel("climate.json"))).unwrap(),
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

/// `RINGS_DECO_N`: decoration replay group size in chunks (default 1).
fn deco_n() -> i32 {
    std::env::var("RINGS_DECO_N")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(1)
}

fn i(v: &Value) -> i32 {
    v.as_i64().unwrap() as i32
}

/// (seed, thaumcraft.json, real spawn)
fn worlds() -> Vec<(i64, PathBuf, (i32, i32))> {
    let base = root().join("testdata/pack/gtnh-2.9.0");
    let mut v = vec![
        (1337, base.join("dump23/thaumcraft.json"), (40, -104)),
        (
            92_900_795,
            base.join("dump27b/thaumcraft.json"),
            (-37, -350),
        ),
    ];
    if let Ok(rd) = std::fs::read_dir(base.join("worlds")) {
        let mut w: Vec<_> = rd
            .filter_map(|e| {
                let e = e.ok()?;
                let s: i64 = e.file_name().to_str()?.parse().ok()?;
                let sp: Value = serde_json::from_str(
                    &std::fs::read_to_string(e.path().join("spawn.json")).ok()?,
                )
                .ok()?;
                Some((
                    s,
                    e.path().join("thaumcraft.json"),
                    (i(&sp["spawn"][0]), i(&sp["spawn"][2])),
                ))
            })
            .collect();
        w.sort();
        v.extend(w);
    }
    v.retain(|w| w.1.exists());
    v
}

fn csv(line: &str) {
    if let Ok(path) = std::env::var("RINGS_CSV") {
        use std::io::Write;
        let mut f = std::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(path)
            .unwrap();
        writeln!(f, "{line}").unwrap();
    }
}

#[derive(Debug, Clone)]
pub struct RingRow {
    seed: i64,
    terrain: bool,
    maze_real: bool,
    x: i32,
    z: i32,
    real: bool,
    valid: bool,
    p: f32,
    note: String,
    /// `RINGS_DECO`: the 5-point check on the replayed decoration (terrain-passing rings only)
    deco: Option<bool>,
}

fn run(seed: i64, path: &PathBuf, spawn: (i32, i32), verbose: bool) -> Vec<RingRow> {
    let table = BiomeTable::from_rwg2dump_json(
        &std::fs::read_to_string(root().join(pack_rel("biomes.json"))).unwrap(),
    )
    .unwrap();
    let climate = ClimateTable::from_json(
        &std::fs::read_to_string(root().join(pack_rel("climate.json"))).unwrap(),
    )
    .unwrap();
    let truth: Value = serde_json::from_str(&std::fs::read_to_string(path).unwrap()).unwrap();
    let r = i(&truth["rangeChunks"]);
    let altars: Vec<(i32, i32)> = truth["tiles"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|x| x["class"].as_str().unwrap().ends_with("TileEldritchAltar"))
        .map(|x| (i(&x["x"]), i(&x["z"])))
        .collect();
    let p = BiomeProvider::new(seed, WorldType::Rwg2Continents, table, RwgConfig::default());
    let mut t = Terrain::new(&p, &TableHeights);
    let mut g = TcGen {
        t: &mut t,
        climate: &climate,
        cfg: TC_GTNH,
    };
    let mut f = Vec::new();
    // maze conflicts reach ~42 chunks: replay a margin, judge only the dumped area
    let m = 20;
    for cz in -r - m..=r + m {
        for cx in -r - m..=r + m {
            g.chunk(seed, cx, cz, &mut f).unwrap();
        }
    }
    // RINGS_DECO_LR=pass,fail: Bayes update of the terrain-passing rings' p by the decorated 5-point check, before
    // the maze resolution
    if let Ok(v) = std::env::var("RINGS_DECO_LR") {
        let lr: Vec<f32> = v.split(',').filter_map(|x| x.parse().ok()).collect();
        let mut ov = rwg2_biomes::deco::view::DecoOverlay::new(gates(), deco_n());
        let mut tt = Terrain::new(&p, &TableHeights);
        if lr.len() == 2 {
            for q in f
                .iter_mut()
                .filter(|q| q.kind == TcKind::Ring && q.p_valid > 0.0)
            {
                let l = if ring_deco_ok(&mut tt, &mut ov, q.x, q.z) {
                    lr[0]
                } else {
                    lr[1]
                };
                q.p_valid = q.p_valid * l / (q.p_valid * l + 1.0 - q.p_valid);
                q.valid = q.p_valid >= RING_MIN_P;
            }
        } else {
            apply_ring_deco(
                &mut f,
                &mut tt,
                &mut ov,
                std::env::var("RINGS_DECO_MINP").map_or(0.0, |v| v.parse().unwrap()),
            );
        }
        eprintln!("{seed}: {} decoration replays", ov.replays);
    }
    let pre: Vec<TcFeature> = f
        .iter()
        .filter(|q| q.kind == TcKind::Ring)
        .copied()
        .collect();
    resolve_ring_mazes_spawn(&mut f, spawn);
    let altar_y: Vec<(i32, i32, i32)> = truth["tiles"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|x| x["class"].as_str().unwrap().ends_with("TileEldritchAltar"))
        .map(|x| (i(&x["x"]), i(&x["y"]), i(&x["z"])))
        .collect();
    let real_rings: Vec<TcFeature> = pre
        .iter()
        .filter(|q| altars.contains(&(q.x, q.z)))
        .copied()
        .collect();
    let sm = rwg2_biomes::spawn::SurfaceModel::new(&p.table);
    let caves = rwg2_biomes::gtorder::Caves::new(seed, true);
    let mut dm = std::env::var("RINGS_DECO").is_ok().then(|| {
        (
            Terrain::new(&p, &TableHeights),
            rwg2_biomes::deco::view::DecoOverlay::new(gates(), deco_n()),
        )
    });
    let mut out = Vec::new();
    for q in f.iter().filter(|q| q.kind == TcKind::Ring) {
        if q.chunk.0.abs() > r || q.chunk.1.abs() > r {
            continue;
        }
        let real = altars.contains(&(q.x, q.z));
        let b = g.t.column(q.x, q.z).unwrap().1;
        let terrain = pre.iter().find(|o| (o.x, o.z) == (q.x, q.z)).unwrap().valid;
        let maze_real = real_rings
            .iter()
            .any(|o| (o.x, o.z) != (q.x, q.z) && (maze_conflict(o, q) || maze_conflict(q, o)));
        let ay = altar_y
            .iter()
            .find(|a| (a.0, a.2) == (q.x, q.z))
            .map(|a| a.1);
        let mut pts = String::new();
        let mut terr_only = true;
        let mut topv_min = 1.0f32;
        let mut unknown = 0;
        let mut names = Vec::new();
        for (dx, dz) in [(-3, -3), (0, 0), (3, 0), (3, 3), (0, 3)] {
            let (x, z) = (q.x + dx, q.z + dz);
            let (cx, cz) = (x >> 4, z >> 4);
            let ch = g.t.chunk(cx, cz).unwrap().clone();
            let cc =
                rwg2_biomes::spawn::carved_chunk(&p, &TableHeights, &sm, &ch, cx, cz, Some(&caves));
            let tops = rwg2_biomes::spawn::chunk_tops_carved(&p, &sm, &ch, cx, cz, &cc);
            let (lx, lz) = (x & 15, z & 15);
            let tp = tops[(lz * 16 + lx) as usize];
            let ctop = cc.top(lx, lz);
            let rwg = ch.rwg[(lz * 16 + lx) as usize];
            pts += &format!(
                " [{}:{} g{:.1}s{:.1}n{:.1} {}]",
                ch.height[(lz * 16 + lx) as usize] as i32,
                ctop,
                tp.grass,
                tp.stone,
                tp.sand,
                p.table.info(rwg).name
            );
            let top = ch.height[(lz * 16 + lx) as usize] as i32;
            let bb = ch.gen[(lz * 16 + lx) as usize];
            terr_only &= top >= 62
                && top >= q.y - 1
                && top <= q.y + 1
                && !climate.dict(bb).iter().any(|d| d == "MESA");
            if ctop >= 63 {
                topv_min = topv_min.min(tp.grass + tp.stone + tp.sand);
            } else {
                unknown += 1;
            }
            names.push(p.table.info(rwg).name.clone());
        }
        let note = format!(
            "terrain {terrain} mazeReal {maze_real} altarY {ay:?} biome {b} {:?} j {} size {:?} exact {} pts{pts}",
            climate.dict(b),
            q.y,
            q.size,
            q.exact
        );
        if verbose {
            eprintln!(
                "{seed} ring ({},{}) real {real} model {} p {:.2} | {note}",
                q.x, q.z, q.valid, q.p_valid
            );
        }
        names.sort();
        names.dedup();
        csv(&format!(
            "{seed},{},{},{real},{terrain},{terr_only},{maze_real},{topv_min:.2},{unknown},{b},{},{},{}",
            q.x,
            q.z,
            climate.dict(b).join("|"),
            names.join("|"),
            q.y
        ));
        let deco = match (&mut dm, terrain) {
            (Some((tt, ov)), true) => Some(ring_deco_ok(tt, ov, q.x, q.z)),
            _ => None,
        };
        out.push(RingRow {
            deco,
            seed,
            terrain,
            maze_real,
            x: q.x,
            z: q.z,
            real,
            valid: q.valid,
            p: q.p_valid,
            note,
        });
    }
    for a in &altars {
        if !out.iter().any(|o| (o.x, o.z) == *a) {
            eprintln!("{seed} real altar {a:?} not rolled in the dumped area");
        }
    }
    out
}

#[test]
fn rings_all_worlds() {
    let ws = worlds();
    if ws.is_empty() {
        eprintln!("skip");
        return;
    }
    let verbose = std::env::var("RINGS_DIAG").is_ok();
    let rows: Vec<RingRow> = {
        #[cfg(target_os = "wasi")]
        {
            ws.iter()
                .flat_map(|(s, p, sp)| run(*s, p, *sp, verbose))
                .collect()
        }
        #[cfg(not(target_os = "wasi"))]
        {
            std::thread::scope(|sc| {
                let hs: Vec<_> = ws
                    .iter()
                    .map(|(s, p, sp)| {
                        std::thread::Builder::new()
                            .stack_size(64 << 20)
                            .spawn_scoped(sc, move || run(*s, p, *sp, verbose))
                            .unwrap()
                    })
                    .collect();
                hs.into_iter().flat_map(|h| h.join().unwrap()).collect()
            })
        }
    };
    let real = rows.iter().filter(|r| r.real).count();
    let kept = rows.iter().filter(|r| r.real && r.valid).count();
    let claims = rows.iter().filter(|r| r.valid).count();
    let right = rows.iter().filter(|r| r.real && r.valid).count();
    let agree = rows.iter().filter(|r| r.real == r.valid).count();
    eprintln!(
        "rings rolled {} | real {real} | model keeps real {kept}/{real} | claims {claims}, right {right} | agreement {agree}/{}",
        rows.len(),
        rows.len()
    );
    let brier = rows
        .iter()
        .map(|r| (r.p - r.real as u8 as f32).powi(2))
        .sum::<f32>()
        / rows.len() as f32;
    let base = real as f32 / rows.len() as f32;
    let brier0 = rows
        .iter()
        .map(|r| (base - r.real as u8 as f32).powi(2))
        .sum::<f32>()
        / rows.len() as f32;
    let likely = rows.iter().filter(|r| r.p >= 0.5).count();
    let likely_ok = rows.iter().filter(|r| r.p >= 0.5 && r.real).count();
    let shown = rows.iter().filter(|r| r.p >= RING_MIN_P).count();
    let shown_ok = rows.iter().filter(|r| r.p >= RING_MIN_P && r.real).count();
    eprintln!(
        "p_valid: Brier {brier:.4} (constant-rate baseline {brier0:.4}) | likely (p≥0.5): {likely_ok}/{likely} real | shown (p≥{RING_MIN_P}): {shown_ok}/{shown} real, covers {shown_ok}/{real} real rings"
    );
    if rows.iter().any(|r| r.deco.is_some()) {
        let c = |real: bool, ok: bool| {
            rows.iter()
                .filter(|r| r.real == real && r.deco == Some(ok))
                .count()
        };
        eprintln!(
            "deco 5-point check on terrain-passing rings: real pass {} fail {} | non-real pass {} fail {}",
            c(true, true),
            c(true, false),
            c(false, true),
            c(false, false)
        );
        // leave-one-out likelihood ratio of the decorated check (negatives: non-real rings no real maze explains)
        let lr = |me: &RingRow, pass: bool| {
            let n = |real: bool, ok: bool| {
                rows.iter()
                    .filter(|r| {
                        !std::ptr::eq(*r, me)
                            && !r.maze_real
                            && r.real == real
                            && r.deco == Some(ok)
                    })
                    .count() as f32
            };
            let (rp, rf, np, nf) = (
                n(true, true),
                n(true, false),
                n(false, true),
                n(false, false),
            );
            if pass {
                ((rp + 0.5) / (rp + rf + 1.0)) / ((np + 0.5) / (np + nf + 1.0))
            } else {
                ((rf + 0.5) / (rp + rf + 1.0)) / ((nf + 0.5) / (np + nf + 1.0))
            }
        };
        let p2: Vec<f32> = rows
            .iter()
            .map(|r| match r.deco {
                Some(ok) => {
                    let l = lr(r, ok);
                    r.p * l / (r.p * l + 1.0 - r.p)
                }
                None => r.p,
            })
            .collect();
        let brier2 = rows
            .iter()
            .zip(&p2)
            .map(|(r, p)| (p - r.real as u8 as f32).powi(2))
            .sum::<f32>()
            / rows.len() as f32;
        let lk = rows.iter().zip(&p2).filter(|(_, p)| **p >= 0.5).count();
        let lk_ok = rows
            .iter()
            .zip(&p2)
            .filter(|(r, p)| **p >= 0.5 && r.real)
            .count();
        let sh = rows
            .iter()
            .zip(&p2)
            .filter(|(_, p)| **p >= RING_MIN_P)
            .count();
        let sh_ok = rows
            .iter()
            .zip(&p2)
            .filter(|(r, p)| **p >= RING_MIN_P && r.real)
            .count();
        eprintln!(
            "with deco (LOO Bayes): Brier {brier2:.4} | likely {lk_ok}/{lk} real | shown {sh_ok}/{sh} real, covers {sh_ok}/{real}"
        );
        for (lo, hi) in [(0.0, 0.1), (0.1, 0.3), (0.3, 0.5), (0.5, 1.01)] {
            let b: Vec<_> = rows
                .iter()
                .zip(&p2)
                .filter(|(_, p)| **p >= lo && **p < hi)
                .collect();
            eprintln!(
                "  deco p in [{lo:.1},{hi:.1}): {}/{} real",
                b.iter().filter(|(r, _)| r.real).count(),
                b.len()
            );
        }
        for r in rows
            .iter()
            .filter(|r| r.deco.is_some() && std::env::var("RINGS_DECO_ROWS").is_ok())
        {
            eprintln!(
                "  deco {} real {} p {:.3} ({},{}) {}",
                r.deco.unwrap(),
                r.real,
                r.p,
                r.x,
                r.z,
                &r.note[..r.note.len().min(90)]
            );
        }
    }
    // calibration of p_valid
    for (lo, hi) in [(0.0, 0.05), (0.05, 0.3), (0.3, 0.5), (0.5, 1.01)] {
        let b: Vec<_> = rows.iter().filter(|r| r.p >= lo && r.p < hi).collect();
        let hit = b.iter().filter(|r| r.real).count();
        eprintln!("  p in [{lo:.1},{hi:.1}): {hit}/{} real", b.len());
    }
    for s in ws.iter().map(|w| w.0) {
        let w: Vec<_> = rows.iter().filter(|r| r.seed == s).collect();
        eprintln!(
            "  {s}: rolled {} real {} kept {} claims {}",
            w.len(),
            w.iter().filter(|r| r.real).count(),
            w.iter().filter(|r| r.real && r.valid).count(),
            w.iter().filter(|r| r.valid).count()
        );
    }
    let tp = rows.iter().filter(|r| r.terrain && r.real).count();
    let fp_clean = rows
        .iter()
        .filter(|r| r.terrain && !r.real && !r.maze_real)
        .count();
    let fp_maze = rows
        .iter()
        .filter(|r| r.terrain && !r.real && r.maze_real)
        .count();
    let tn = rows.iter().filter(|r| !r.terrain && !r.real).count();
    eprintln!("terrain model: real pass {tp}/{real}; non-real pass: {fp_clean} not explained by a real ring's maze, {fp_maze} explained; terrain rejects {tn}");
    for r in rows.iter().filter(|r| r.terrain && !r.real && !r.maze_real) {
        eprintln!("  FP {} ({},{}): {}", r.seed, r.x, r.z, r.note);
    }
    assert!(
        brier < brier0,
        "ring probabilities must beat the constant rate"
    );
    assert!(
        shown_ok >= 6,
        "regression: real rings shown {shown_ok}/{real}"
    );
    for r in rows.iter().filter(|r| r.real) {
        eprintln!(
            "  real ring {} ({},{}): model {} p {:.2} | {}",
            r.seed, r.x, r.z, r.valid, r.p, r.note
        );
    }
}

/// `data/<PACK_DATA>/<name>` relative to the repo (`PACK_DATA` default `gtnh-2.9.0`; `gtnh-2.9.1` = RWG2 2.0.9 pack).
#[allow(dead_code)]
fn pack_rel(name: &str) -> String {
    format!("data/{}/{name}", std::env::var("PACK_DATA").unwrap_or_else(|_| "gtnh-2.9.0".into()))
}
