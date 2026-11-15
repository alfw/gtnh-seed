#![cfg(feature = "json")]
//! Roguelike `validLocation` model diagnostics vs the dumped verdicts (seed 1337: dump22, seed 92900795: dump27b
//! and dump28 = same world, different load state).
use rwg2_biomes::config::RwgConfig;
use rwg2_biomes::dungeons::*;
use rwg2_biomes::forestry::ClimateTable;
use rwg2_biomes::provider::{BiomeProvider, BiomeTable, TableHeights, WorldType};

fn root() -> std::path::PathBuf {
    std::env::var("RWG2_DATA_DIR")
        .map(|d| std::path::PathBuf::from(d).parent().unwrap().to_path_buf())
        .unwrap_or_else(|_| std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../.."))
}

struct Att {
    x: i32,
    z: i32,
    ok: bool,
    y: i32,
}

struct Anchor {
    cx: i32,
    cz: i32,
    attempts: Vec<Att>,
}

fn load(dir: &str) -> Option<(i64, Vec<Anchor>)> {
    let s = std::fs::read_to_string(
        root().join(format!("testdata/pack/gtnh-2.9.0/{dir}/dungeons.json")),
    )
    .ok()?;
    let d: serde_json::Value = serde_json::from_str(&s).ok()?;
    let seed: i64 = d["worldSeed"].as_str()?.parse().ok()?;
    let mut out = Vec::new();
    for a in d["roguelike"]["anchors"].as_array()? {
        let Some(att) = a["attempts"].as_array() else {
            continue;
        };
        out.push(Anchor {
            cx: a["chunkX"].as_i64().unwrap() as i32,
            cz: a["chunkZ"].as_i64().unwrap() as i32,
            attempts: att
                .iter()
                .map(|e| {
                    let e = e.as_array().unwrap();
                    Att {
                        x: e[0].as_i64().unwrap() as i32,
                        z: e[1].as_i64().unwrap() as i32,
                        ok: e[2].as_bool().unwrap(),
                        y: e[3].as_i64().unwrap_or(-1) as i32,
                    }
                })
                .collect(),
        });
    }
    Some((seed, out))
}

fn gates() -> rwg2_biomes::population::PreGates {
    let cj: serde_json::Value = serde_json::from_str(
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

fn names() -> Vec<String> {
    let c: serde_json::Value = serde_json::from_str(
        &std::fs::read_to_string(root().join("data/gtnh-2.9.0/climate.json")).unwrap(),
    )
    .unwrap();
    let mut v = vec![String::new(); 70000];
    for b in c["biomes"].as_array().unwrap() {
        v[b["id"].as_u64().unwrap() as usize] = b["name"].as_str().unwrap().to_string();
    }
    v
}

#[derive(Default, Debug, Clone, Copy)]
struct Score {
    v_ok: usize,
    v_n: usize,
    fp: usize,
    fn_: usize,
    chosen_ok: usize,
    chosen_n: usize,
    y_ok: usize,
    y_n: usize,
}

fn run(dir: &str, verbose: bool) -> Option<(Score, Score)> {
    let (seed, anchors) = load(dir)?;
    let table = BiomeTable::from_rwg2dump_json(
        &std::fs::read_to_string(root().join("data/gtnh-2.9.0/biomes.json")).unwrap(),
    )
    .unwrap();
    let climate = ClimateTable::from_json(
        &std::fs::read_to_string(root().join("data/gtnh-2.9.0/climate.json")).unwrap(),
    )
    .unwrap();
    let nm = names();
    let p = BiomeProvider::new(seed, WorldType::Rwg2Continents, table, RwgConfig::default());
    let mut old = Score::default();
    let mut new = Score::default();
    let mut m = RogueModel::new(&p, &TableHeights, seed);
    let env = |k: &str, d: f32| {
        std::env::var(k)
            .ok()
            .and_then(|v| v.parse().ok())
            .unwrap_or(d)
    };
    m.params = RogueParams {
        dens_open: env("R_OPEN", m.params.dens_open),
        dens_mid: env("R_MID", m.params.dens_mid),
        dens_barren: env("R_BARREN", m.params.dens_barren),
        vine_ok: env("R_VINE", m.params.vine_ok),
    };
    m.use_caves = env("R_CAVES", 1.0) > 0.5;
    if env("R_DECO", 0.0) > 0.5 {
        m.deco = Some(rwg2_biomes::deco::view::DecoOverlay::new(
            gates(),
            ROGUE_DECO_N,
        ));
        m.deco_stats = env("R_DECO", 0.0) > 1.5;
        m.deco_vine_k = env("R_VK", m.deco_vine_k);
    }
    for a in &anchors {
        let want = a.attempts.iter().find(|e| e.ok).map(|e| (e.x, e.z));
        // old model
        let mut t = Terrain::new(&p, &TableHeights);
        for e in &a.attempts {
            let v = rogue_valid(&mut t, &climate, &ROGUE_GTNH, e.x, e.z, true).unwrap();
            old.v_n += 1;
            if v == e.ok {
                old.v_ok += 1
            } else if v {
                old.fp += 1
            } else {
                old.fn_ += 1
            }
        }
        let anchor_chunk = (0, 0); // unused by rogue_valid
        let _ = anchor_chunk;
        // new model
        for e in &a.attempts {
            let r = m.check(&climate, &ROGUE_GTNH, e.x, e.z);
            let v = r.p >= 0.5;
            new.v_n += 1;
            if v == e.ok {
                new.v_ok += 1
            } else if v {
                new.fp += 1
            } else {
                new.fn_ += 1
            }
            if e.ok {
                new.y_n += 1;
                new.y_ok += (r.y == e.y) as usize;
            }
            if verbose && (v != e.ok || (e.ok && r.y != e.y)) {
                let b = m.biome(e.x, e.z);
                eprintln!(
                    "  {dir} ({},{}) real {} y {} | model p {:.2} y {} reason {:?} | biome {} {:?} {:?}",
                    e.x,
                    e.z,
                    e.ok,
                    e.y,
                    r.p,
                    r.y,
                    r.reason,
                    nm[b as usize],
                    climate.dict(b),
                    r.dbg
                );
            }
        }
        // chosen spot over the game's full 50 attempts
        old.chosen_n += 1;
        new.chosen_n += 1;
        let mut t = Terrain::new(&p, &TableHeights);
        let od = rogue_dungeon(&mut t, &climate, &ROGUE_GTNH, seed, a.cx, a.cz);
        old.chosen_ok += (od.pos == want) as usize;
        let t0 = std::time::Instant::now();
        let nd = m.dungeon(&climate, &ROGUE_GTNH, seed, a.cx, a.cz);
        if verbose {
            eprintln!(
                "  {dir} anchor ({},{}) model pick in {:?}",
                a.cx,
                a.cz,
                t0.elapsed()
            );
        }
        new.chosen_ok += (nd.pos == want) as usize;
        if verbose && nd.pos != want {
            eprintln!(
                "  {dir} anchor pick: model {:?} (attempt {}, conf {:.2}) real {want:?}",
                nd.pos, nd.attempt, nd.confidence
            );
        }
    }
    Some((old, new))
}

#[test]
fn rogue_model_report() {
    let verbose = std::env::var("ROGUE_VERBOSE").is_ok();
    for dir in ["dump22", "dump27b", "dump28"] {
        let Some((o, n)) = run(dir, verbose) else {
            eprintln!("skip {dir}");
            continue;
        };
        eprintln!("{dir}: old verdict {}/{} (fp {} fn {}) | new verdict {}/{} (fp {} fn {}) y {}/{} | chosen old {}/{} new {}/{}",
            o.v_ok, o.v_n, o.fp, o.fn_, n.v_ok, n.v_n, n.fp, n.fn_, n.y_ok, n.y_n, o.chosen_ok, o.chosen_n, n.chosen_ok, n.chosen_n);
        assert!(
            std::env::var("R_DECO").is_ok() || n.v_ok >= o.v_ok && n.chosen_ok >= o.chosen_ok,
            "{dir}: new Roguelike model regressed"
        );
    }
}

/// Pre-population ground (highest `validGroundBlock`) per column vs the world's own `provideChunk`
/// (`worldchunks_*.bin`, chunks [-6, 6)²).
#[test]
fn rogue_prepop_ground() {
    for (dir, seed) in [("dump22", 1337i64), ("dump27b", 92_900_795)] {
        let Ok(bin) = std::fs::read(root().join(format!(
            "testdata/pack/gtnh-2.9.0/{dir}/worldchunks_{seed}.bin"
        ))) else {
            eprintln!("skip {dir}");
            continue;
        };
        let meta: serde_json::Value = serde_json::from_str(
            &std::fs::read_to_string(
                root().join(format!("testdata/pack/gtnh-2.9.0/{dir}/worldchunks.json")),
            )
            .unwrap(),
        )
        .unwrap();
        let blocks: Vec<String> = meta["blocks"]
            .as_array()
            .unwrap()
            .iter()
            .map(|v| v.as_str().unwrap().to_string())
            .collect();
        let n = 144 * 256;
        let top = &bin[n * 2..n * 3];
        let tblk = &bin[n * 3..n * 5];
        let gy = &bin[n * 5..n * 6];
        let table = BiomeTable::from_rwg2dump_json(
            &std::fs::read_to_string(root().join("data/gtnh-2.9.0/biomes.json")).unwrap(),
        )
        .unwrap();
        let p = BiomeProvider::new(seed, WorldType::Rwg2Continents, table, RwgConfig::default());
        let mut m = RogueModel::new(&p, &TableHeights, seed);
        let (mut ok, mut tot) = (0, 0);
        let mut hist = std::collections::BTreeMap::<(String, i32), usize>::new();
        for ci in 0..144usize {
            let (cz, cx) = (ci as i32 / 12 - 6, ci as i32 % 12 - 6);
            for idx in 0..256usize {
                let k = ci * 256 + idx;
                let (x, z) = (cx * 16 + (idx % 16) as i32, cz * 16 + (idx / 16) as i32);
                let real = if gy[k] == 0xff { -1 } else { gy[k] as i32 };
                let _ = top[k];
                let mut y = 255;
                while y > 0 && m.block(x, y, z) != RogueBlock::Ground {
                    y -= 1;
                }
                tot += 1;
                if y == real {
                    ok += 1;
                } else {
                    let tb = u16::from_le_bytes([tblk[2 * k], tblk[2 * k + 1]]);
                    let name = blocks.get(tb as usize).cloned().unwrap_or(format!("#{tb}"));
                    *hist.entry((name, (y - real).clamp(-4, 4))).or_default() += 1;
                }
            }
        }
        eprintln!("{dir}: pre-population ground y {ok}/{tot}; misses by (real top block, model - real): {hist:?}");
    }
}
