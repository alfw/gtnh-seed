#![cfg(feature = "json")]
//! [`Target::SurfaceBlock`] = direct enumeration (every chunk in the radius, no biome pre-filter, no early stop):
//! same hit and the same match / no-match verdict.
use rwg2_biomes::chunkgen::ChunkGen;
use rwg2_biomes::config::RwgConfig;
use rwg2_biomes::forestry::ClimateTable;
use rwg2_biomes::gtorder::Caves;
use rwg2_biomes::provider::{BiomeProvider, BiomeTable, TableHeights, WorldType};
use rwg2_biomes::search::{Center, Cond, Query, SearchData, Searcher, Target};
use rwg2_biomes::surface::{apply_snow_cover, blk, surface_chunk, SurfaceTable};
use std::path::PathBuf;

fn root() -> PathBuf {
    std::env::var("RWG2_DATA_DIR")
        .map(|d| PathBuf::from(d).parent().unwrap().to_path_buf())
        .unwrap_or_else(|_| PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../.."))
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

/// Every column within r of the origin: (count of matches, nearest match).
fn brute(
    p: &BiomeProvider,
    clim: &ClimateTable,
    blocks: &[u8],
    r: i32,
    water: bool,
) -> (u32, Option<(i64, i32, i32)>) {
    let st = SurfaceTable::new(&p.table);
    let sm = rwg2_biomes::spawn::SurfaceModel::new(&p.table);
    let caves = Caves::new(p.seed(), true);
    let mut g = ChunkGen::new(p);
    let r2 = r as i64 * r as i64;
    let (mut n, mut best) = (0u32, None::<(i64, i32, i32)>);
    for cz in (-r >> 4)..=(r >> 4) {
        for cx in (-r >> 4)..=(r >> 4) {
            let c = g.generate(p, &TableHeights, cx, cz).unwrap();
            let cc =
                rwg2_biomes::spawn::carved_chunk(p, &TableHeights, &sm, &c, cx, cz, Some(&caves));
            let mut sc = surface_chunk(p, &TableHeights, &st, &c, cx, cz, &cc);
            apply_snow_cover(
                p,
                &TableHeights,
                &|g| clim.get(g).map(|b| b.temperature),
                &c,
                &mut sc,
                cx,
                cz,
            );
            for idx in 0..256usize {
                let (x, z) = (cx * 16 + (idx & 15) as i32, cz * 16 + (idx >> 4) as i32);
                let d = x as i64 * x as i64 + z as i64 * z as i64;
                let mut y = sc.top[idx] as i32;
                if d > r2 || y < 0 {
                    continue;
                }
                if water {
                    while y > 0 && sc.cols[idx][y as usize] == blk::WATER {
                        y -= 1;
                    }
                }
                if blocks.contains(&sc.cols[idx][y as usize]) {
                    n += 1;
                    if best.map_or(true, |b| (d, x, z) < b) {
                        best = Some((d, x, z));
                    }
                }
            }
        }
    }
    (n, best)
}

#[test]
fn surface_block_matches_enumeration() {
    let t = table();
    let clay: Vec<u8> = (0..16).map(blk::clay).chain([blk::HARDENED_CLAY]).collect();
    let cases: Vec<(&str, Vec<u8>, u32, i32, bool)> = vec![
        ("red sand", vec![blk::RED_SAND], 1, 320, false),
        ("clay x40", clay, 40, 320, false),
        ("sand x200", vec![blk::SAND], 200, 256, false),
        ("snow layer", vec![blk::SNOW_LAYER], 1, 256, false),
        ("gravel under water", vec![blk::GRAVEL], 1, 256, true),
        ("podzol", vec![blk::PODZOL], 1, 256, false),
    ];
    let (mut agree, mut hits) = (0, 0);
    let t0 = std::time::Instant::now();
    for (name, blocks, min, r, water) in cases {
        let q = Query {
            world_type: WorldType::Rwg2Continents,
            land_at_spawn: false,
            conds: vec![Cond::new(
                Target::SurfaceBlock {
                    blocks: blocks.clone(),
                    min,
                    water,
                },
                r,
                false,
            )],
            step: 64,
            center: Center::Origin,
            dim: 0,
        };
        let sr = Searcher::new(
            q,
            t.clone(),
            RwgConfig::default(),
            SearchData {
                climate: Some(climate()),
                ..Default::default()
            },
        );
        let mut row = String::new();
        for seed in 1..=6i64 {
            let p = sr.provider(seed);
            let got = sr
                .check_with(&p, &TableHeights)
                .map(|m| (m.hits[0].x, m.hits[0].z));
            let (n, best) = brute(&p, &climate(), &blocks, r, water);
            let want = if n >= min {
                best.map(|b| (b.1, b.2))
            } else {
                None
            };
            // ties: same distance, any of them
            let ok = match (got, want) {
                (Some(g), Some(w)) => {
                    let d = |q: (i32, i32)| q.0 as i64 * q.0 as i64 + q.1 as i64 * q.1 as i64;
                    d(g) == d(w)
                }
                (a, b) => a == b,
            };
            assert!(
                ok,
                "{name} seed {seed}: search {got:?}, enumeration {want:?} ({n} columns)"
            );
            agree += 1;
            hits += got.is_some() as i32;
            row += &format!(" {seed}:{n}");
        }
        eprintln!("{name}: columns per seed{row}");
    }
    eprintln!(
        "surface search: {agree} seed checks agree, {hits} matches ({:?})",
        t0.elapsed()
    );
}

#[test]
#[ignore]
fn surface_block_speed() {
    let t = table();
    for (name, blocks, min, r) in [
        ("red sand ≤ 500", vec![blk::RED_SAND], 1u32, 500),
        ("snow ≤ 500", vec![blk::SNOW_LAYER], 1, 500),
        ("mycelium ≤ 1000", vec![blk::MYCELIUM], 1, 1000),
        ("sand ≥ 500 cols ≤ 300", vec![blk::SAND], 500, 300),
    ] {
        let q = Query {
            world_type: WorldType::Rwg2Continents,
            land_at_spawn: false,
            conds: vec![Cond::new(
                Target::SurfaceBlock {
                    blocks,
                    min,
                    water: false,
                },
                r,
                false,
            )],
            step: 64,
            center: Center::Origin,
            dim: 0,
        };
        let sr = Searcher::new(
            q,
            t.clone(),
            RwgConfig::default(),
            SearchData {
                climate: Some(climate()),
                ..Default::default()
            },
        );
        let t0 = std::time::Instant::now();
        let n = 20;
        let hits = (1..=n).filter(|&s| sr.check(s).is_some()).count();
        eprintln!(
            "{name}: {hits}/{n} match, {:.2} seeds/s",
            n as f64 / t0.elapsed().as_secs_f64()
        );
    }
}

/// Expected counts of the viewer's surface dev presets (seeds 1..=60, origin, CONTINENTS).
#[test]
#[ignore]
fn surface_preset_counts() {
    let t = table();
    for (name, blocks, min, r) in [
        ("red sand ≤ 500", vec![blk::RED_SAND], 1u32, 500),
        ("snow ≥ 2000 ≤ 200", vec![blk::SNOW_LAYER], 2000, 200),
    ] {
        let q = Query {
            world_type: WorldType::Rwg2Continents,
            land_at_spawn: false,
            conds: vec![Cond::new(
                Target::SurfaceBlock {
                    blocks,
                    min,
                    water: false,
                },
                r,
                false,
            )],
            step: 64,
            center: Center::Origin,
            dim: 0,
        };
        let sr = Searcher::new(
            q,
            t.clone(),
            RwgConfig::default(),
            SearchData {
                climate: Some(climate()),
                ..Default::default()
            },
        );
        let hits: Vec<i64> = (1..=60).filter(|&s| sr.check(s).is_some()).collect();
        eprintln!("{name}: {} / 60 {hits:?}", hits.len());
    }
}
