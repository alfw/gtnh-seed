//! Speed-path equivalence: the search results of the slow targets (aura-node aspect, Thaumcraft structures,
//! Nether/TF biomes) are pinned to a golden snapshot taken before the fast paths were added, and timed.
//! `SPEED_WRITE=1 cargo test --release --features json --test speed -- --nocapture` rewrites the snapshot.
#![cfg(feature = "json")]
use rwg2_biomes::config::RwgConfig;
use rwg2_biomes::forestry::ClimateTable;
use rwg2_biomes::provider::{BiomeTable, WorldType};
use rwg2_biomes::search::{Center, Cond, Query, SearchData, Searcher, Target};
use rwg2_biomes::thaumcraft::TcKind;

fn root() -> std::path::PathBuf {
    std::env::var("RWG2_DATA_DIR")
        .map(|d| std::path::PathBuf::from(d).parent().unwrap().to_path_buf())
        .unwrap_or_else(|_| std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../.."))
}

fn searcher(dim: i32, step: i32, conds: Vec<Cond>) -> Searcher {
    let r = root();
    let table = BiomeTable::from_rwg2dump_json(
        &std::fs::read_to_string(r.join(pack_rel("biomes.json"))).unwrap(),
    )
    .unwrap();
    let climate = ClimateTable::from_json(
        &std::fs::read_to_string(r.join(pack_rel("climate.json"))).unwrap(),
    )
    .unwrap();
    let q = Query {
        world_type: WorldType::Rwg2Continents,
        land_at_spawn: false,
        conds,
        step,
        center: Center::Origin,
        dim,
    };
    Searcher::new(
        q,
        table,
        RwgConfig::default(),
        SearchData {
            climate: Some(climate),
            ..Default::default()
        },
    )
}

fn c(target: Target, radius: i32) -> Cond {
    Cond {
        target,
        radius,
        negate: false,
        near: vec![],
        helper: false,
    }
}

/// (name, dim, grid step, conditions, seeds)
type Case = (&'static str, i32, i32, Vec<Cond>, std::ops::Range<i64>);

fn cases() -> Vec<Case> {
    vec![
        (
            "ow node any r300",
            0,
            64,
            vec![c(
                Target::TcAspect {
                    aspect: u8::MAX,
                    min: 1,
                },
                300,
            )],
            0..12,
        ),
        (
            "ow node terra>=40 r500",
            0,
            64,
            vec![c(Target::TcAspect { aspect: 1, min: 40 }, 500)],
            0..8,
        ),
        (
            "ow ring r1500",
            0,
            64,
            vec![c(Target::Thaumcraft(TcKind::Ring), 1500)],
            0..6,
        ),
        (
            "nether node any r250",
            -1,
            64,
            vec![c(
                Target::TcAspect {
                    aspect: u8::MAX,
                    min: 1,
                },
                250,
            )],
            0..16,
        ),
        (
            "nether node ignis>=30 r400",
            -1,
            64,
            vec![c(Target::TcAspect { aspect: 2, min: 30 }, 400)],
            0..8,
        ),
        (
            "tf node any r250",
            7,
            64,
            vec![c(
                Target::TcAspect {
                    aspect: u8::MAX,
                    min: 1,
                },
                250,
            )],
            0..16,
        ),
        (
            "tf node terra>=50 r400",
            7,
            64,
            vec![c(Target::TcAspect { aspect: 1, min: 50 }, 400)],
            0..8,
        ),
        (
            "tf biome 172 r1500",
            7,
            64,
            vec![c(Target::Biome(vec![172]), 1500)],
            0..30,
        ),
        (
            "tf biome 176 r2000",
            7,
            64,
            vec![c(Target::Biome(vec![176]), 2000)],
            0..20,
        ),
        (
            "nether biome 138 r700 g32",
            -1,
            32,
            vec![c(Target::Biome(vec![138]), 700)],
            0..30,
        ),
        // worst cases: nothing found, the whole disc is sampled
        (
            "tf biome none r1500",
            7,
            64,
            vec![c(Target::Biome(vec![999]), 1500)],
            0..10,
        ),
        (
            "nether biome none r1500 g32",
            -1,
            32,
            vec![c(Target::Biome(vec![999]), 1500)],
            0..10,
        ),
        (
            "tf adj 172|173 d64 r800",
            7,
            64,
            vec![c(
                Target::Adjacent {
                    a: vec![172],
                    b: vec![173],
                    dist: 64,
                },
                800,
            )],
            0..10,
        ),
    ]
}

/// Fast paths (no-terrain pre-checks, lazy dimension biome cache) give the same search results as the full
/// replay, on the current model; timings of both.
#[test]
fn speed_paths_match_full_replay() {
    use rwg2_biomes::thaumcraft::SLOW_PATHS;
    use std::sync::atomic::Ordering;
    for (name, dim, step, conds, seeds) in cases() {
        let s = searcher(dim, step, conds);
        let n = seeds.clone().count() as f64;
        let run = |slow: bool| {
            SLOW_PATHS.store(slow, Ordering::Relaxed);
            let t0 = std::time::Instant::now();
            let r: Vec<_> = seeds.clone().map(|seed| s.check(seed)).collect();
            (r, n / t0.elapsed().as_secs_f64())
        };
        let (fast, vf) = run(false);
        let (slow, vs) = run(true);
        SLOW_PATHS.store(false, Ordering::Relaxed);
        let hits = fast.iter().filter(|m| m.is_some()).count();
        eprintln!("{name}: {hits}/{n} hits, fast {vf:.1} seeds/s, full replay {vs:.1} seeds/s");
        assert_eq!(fast, slow, "{name}");
    }
}

/// Profiling target (callgrind): one overworld aspect search.
#[test]
#[ignore]
fn profile_one() {
    let which = std::env::var("PROF").unwrap_or_default();
    for (name, dim, step, conds, _) in cases() {
        if name == which {
            let s = searcher(dim, step, conds);
            let _ = s.check(3);
        }
    }
}

/// The lazy 1:4 search cache relies on this: a cell's value does not depend on the rect it is requested in
/// (single cells and 16² tiles vs one big grid, both dimensions).
#[test]
fn quarter_subrects_match_big_grid() {
    use rwg2_biomes::nether::NetherBiomes;
    use rwg2_biomes::twilight::{tf_seed, TwilightForest};
    for seed in [5i64, -77, 92_900_795] {
        let tf = TwilightForest::new(tf_seed(seed, ""));
        let nb = NetherBiomes::new(seed);
        let (big, bign) = (
            tf.biomes_quarter(-70, -70, 140, 140),
            nb.biomes_quarter(-70, -70, 140, 140),
        );
        for t in [1, 16] {
            for j in (-64..64).step_by(t.max(7) as usize) {
                for i in (-64..64).step_by(t.max(5) as usize) {
                    let (v, vn) = (tf.biomes_quarter(i, j, t, t), nb.biomes_quarter(i, j, t, t));
                    for b in 0..t {
                        for a in 0..t {
                            let k = ((j + b + 70) * 140 + (i + a + 70)) as usize;
                            assert_eq!(v[(b * t + a) as usize], big[k]);
                            assert_eq!(vn[(b * t + a) as usize], bign[k]);
                        }
                    }
                }
            }
        }
    }
}

/// Tile-size probe for the lazy 1:4 grid: sub-rect values equal the big grid, cost per tile size.
#[test]
#[ignore]
fn quarter_tiles_probe() {
    use rwg2_biomes::nether::NetherBiomes;
    use rwg2_biomes::twilight::{tf_seed, TwilightForest};
    let tf = TwilightForest::new(tf_seed(5, ""));
    let nb = NetherBiomes::new(5);
    let big = tf.biomes_quarter(-300, -300, 600, 600);
    let bign = nb.biomes_quarter(-300, -300, 600, 600);
    for t in [1, 8, 16, 32, 64, 128] {
        let t0 = std::time::Instant::now();
        let mut n = 0;
        for j in (0..512).step_by(t.max(16) as usize) {
            for i in (0..512).step_by(t.max(16) as usize) {
                let v = tf.biomes_quarter(-256 + i, -256 + j, t, t);
                let vn = nb.biomes_quarter(-256 + i, -256 + j, t, t);
                for b in 0..t {
                    for a in 0..t {
                        let k = ((b + j + 44) * 600 + (a + i + 44)) as usize;
                        assert_eq!(v[(b * t + a) as usize], big[k]);
                        assert_eq!(vn[(b * t + a) as usize], bign[k]);
                    }
                }
                n += 1;
            }
        }
        eprintln!(
            "tile {t}: {n} calls, {:.1} µs/call",
            t0.elapsed().as_secs_f64() * 1e6 / n as f64
        );
    }
    let t0 = std::time::Instant::now();
    let _ = tf.biomes_quarter(-500, -500, 1000, 1000);
    eprintln!("full 1000²: {:?}", t0.elapsed());
}

/// Viewer node layer cost (one 2048-block cell = 128² chunks, as `tc_nodes_json`): time and chunks generated.
#[test]
#[ignore]
fn node_cell_profile() {
    use rwg2_biomes::dungeons::Terrain;
    use rwg2_biomes::provider::{BiomeProvider, TableHeights};
    use rwg2_biomes::thaumcraft::{TcGen, TC_GTNH};
    let r = root();
    let table = BiomeTable::from_rwg2dump_json(
        &std::fs::read_to_string(r.join(pack_rel("biomes.json"))).unwrap(),
    )
    .unwrap();
    let climate = ClimateTable::from_json(
        &std::fs::read_to_string(r.join(pack_rel("climate.json"))).unwrap(),
    )
    .unwrap();
    let p = BiomeProvider::new(1337, WorldType::Rwg2Continents, table, RwgConfig::default());
    let mut t = Terrain::new(&p, &TableHeights);
    let mut g = TcGen {
        t: &mut t,
        climate: &climate,
        cfg: TC_GTNH,
    };
    let t0 = std::time::Instant::now();
    let (mut nodes, mut pass) = (Vec::new(), 0);
    for cz in 0..128 {
        for cx in 0..128 {
            pass += g.node_roll_possible(1337, cx, cz, false) as usize;
            let _ = g.nodes(1337, cx, cz, &mut nodes);
        }
    }
    eprintln!(
        "cell: {:?}, {} nodes, {pass}/16384 chunks pass the roll check, {} chunks generated",
        t0.elapsed(),
        nodes.len(),
        g.t.generated
    );
    let mut t2 = Terrain::new(&p, &TableHeights);
    let mut g2 = TcGen {
        t: &mut t2,
        climate: &climate,
        cfg: TC_GTNH,
    };
    let t1 = std::time::Instant::now();
    let mut nodes2 = Vec::new();
    g2.nodes_in_rect(1337, 0, 0, 127, 127, &mut nodes2);
    eprintln!(
        "nodes_in_rect: {:?}, {} chunks generated",
        t1.elapsed(),
        g2.t.generated
    );
    assert_eq!(format!("{nodes:?}"), format!("{nodes2:?}"));
}

/// `nodes_in_rect` lists exactly what per-chunk `nodes` lists (several seeds / rects, incl. desert and jungle
/// areas so every vegetation branch is exercised).
#[test]
fn nodes_in_rect_matches_nodes() {
    use rwg2_biomes::dungeons::Terrain;
    use rwg2_biomes::provider::{BiomeProvider, TableHeights};
    use rwg2_biomes::thaumcraft::{TcGen, TC_GTNH};
    let r = root();
    let table = BiomeTable::from_rwg2dump_json(
        &std::fs::read_to_string(r.join(pack_rel("biomes.json"))).unwrap(),
    )
    .unwrap();
    let climate = ClimateTable::from_json(
        &std::fs::read_to_string(r.join(pack_rel("climate.json"))).unwrap(),
    )
    .unwrap();
    for (seed, cx0, cz0) in [
        (1337i64, -40, -40),
        (92_900_795, 10, -60),
        (5, -200, 150),
        (77, 300, -20),
    ] {
        let p = BiomeProvider::new(
            seed,
            WorldType::Rwg2Continents,
            table.clone(),
            RwgConfig::default(),
        );
        let (cx1, cz1) = (cx0 + 31, cz0 + 31);
        let mut t = Terrain::new(&p, &TableHeights);
        let mut g = TcGen {
            t: &mut t,
            climate: &climate,
            cfg: TC_GTNH,
        };
        let mut a = Vec::new();
        for cz in cz0..=cz1 {
            for cx in cx0..=cx1 {
                let _ = g.nodes(seed, cx, cz, &mut a);
            }
        }
        let mut t2 = Terrain::new(&p, &TableHeights);
        let mut g2 = TcGen {
            t: &mut t2,
            climate: &climate,
            cfg: TC_GTNH,
        };
        let mut b = Vec::new();
        g2.nodes_in_rect(seed, cx0, cz0, cx1, cz1, &mut b);
        assert_eq!(format!("{a:?}"), format!("{b:?}"), "seed {seed}");
    }
}

#[test]
#[ignore]
fn hot_sandy_prevalence() {
    use rwg2_biomes::provider::{BiomeProvider, TableHeights};
    let r = root();
    let table = BiomeTable::from_rwg2dump_json(
        &std::fs::read_to_string(r.join(pack_rel("biomes.json"))).unwrap(),
    )
    .unwrap();
    let climate = ClimateTable::from_json(
        &std::fs::read_to_string(r.join(pack_rel("climate.json"))).unwrap(),
    )
    .unwrap();
    let hs = |b: u16| {
        climate.get(b).is_some_and(|c| c.temperature > 1.0)
            && climate.dict(b).iter().any(|d| d == "SANDY")
    };
    let ids: Vec<u16> = (0..400u16).filter(|&b| hs(b)).collect();
    eprintln!("hot+sandy ids: {ids:?}");
    for seed in [1337i64, 92_900_795, 5, 77] {
        let p = BiomeProvider::new(
            seed,
            WorldType::Rwg2Continents,
            table.clone(),
            RwgConfig::default(),
        );
        let mut cells_with = 0;
        for cz in -4..4 {
            for cx in -4..4 {
                let mut any = false;
                'o: for z in (cz * 2048 - 96..cz * 2048 + 2048 + 96).step_by(32) {
                    for x in (cx * 2048 - 96..cx * 2048 + 2048 + 96).step_by(32) {
                        let d = p.biome_data(x, z, &TableHeights);
                        if hs(p.table.biomes[d.data as usize].gen_id) {
                            any = true;
                            break 'o;
                        }
                    }
                }
                cells_with += any as usize;
            }
        }
        eprintln!("seed {seed}: {cells_with}/64 cells contain a hot sandy biome");
    }
}

/// Twilight Forest / Nether node layer cell (as `tc_nodes_json_dim`).
#[test]
#[ignore]
fn node_cell_profile_dims() {
    use rwg2_biomes::dimterrain::{NetherTerrain, TfTerrain};
    use rwg2_biomes::tc_dims::{DimTc, DimWorld};
    use rwg2_biomes::thaumcraft::TC_GTNH;
    use rwg2_biomes::twilight::tf_seed;
    let r = root();
    let climate = ClimateTable::from_json(
        &std::fs::read_to_string(r.join(pack_rel("climate.json"))).unwrap(),
    )
    .unwrap();
    let tt = TfTerrain::new(tf_seed(1337, ""));
    let nt = NetherTerrain::new(1337);
    for (name, w) in [
        ("tf", DimWorld::Twilight(&tt)),
        ("nether", DimWorld::Nether(&nt)),
    ] {
        let mut g = DimTc::new(w, &climate, TC_GTNH);
        let t0 = std::time::Instant::now();
        let mut nodes = Vec::new();
        let mut rolled = 0;
        for cz in 0..128 {
            for cx in 0..128 {
                rolled += !matches!(
                    g.peek_wild(1337, cx, cz),
                    rwg2_biomes::tc_dims::WildPeek::Nothing
                ) as usize;
            }
        }
        let tp = t0.elapsed();
        for cz in 0..128 {
            for cx in 0..128 {
                g.nodes(1337, cx, cz, &mut nodes);
            }
        }
        eprintln!(
            "{name}: peek {tp:?} ({rolled} chunks roll), total {:?}, {} nodes",
            t0.elapsed(),
            nodes.len()
        );
    }
}

/// `data/<PACK_DATA>/<name>` relative to the repo (`PACK_DATA` default `gtnh-2.9.0`; `gtnh-2.9.1` = RWG2 2.0.9 pack).
#[allow(dead_code)]
fn pack_rel(name: &str) -> String {
    format!("data/{}/{name}", std::env::var("PACK_DATA").unwrap_or_else(|_| "gtnh-2.9.0".into()))
}
