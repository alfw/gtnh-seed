use rwg2_biomes::config::RwgConfig;
use rwg2_biomes::poisson::PoissonPointNoise;
use rwg2_biomes::provider::{BiomeTable, NoHeights, WorldType};
use rwg2_biomes::search::{Center, Cond, Query, SearchData, Searcher, Target};

fn table() -> BiomeTable {
    // under WASI the repo root is mapped to "/" and tools/wasi-runner.sh sets RWG2_DATA_DIR
    let dir = std::env::var("RWG2_DATA_DIR")
        .map(std::path::PathBuf::from)
        .unwrap_or_else(|_| {
            std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../data")
        });
    let p = dir.join("gtnh-2.9.0/biomes.json");
    BiomeTable::from_rwg2dump_json(&std::fs::read_to_string(p).unwrap()).unwrap()
}

/// Enumeration must agree with nearest-point sampling: the nearest enumerated point == sample().
#[test]
fn poisson_enumeration_matches_sample() {
    let p = PoissonPointNoise::new(42 ^ 0x510E527FADE682D1, 1020.0, 4);
    let pts = p.points_in_rect(-6000.0, -6000.0, 6000.0, 6000.0);
    assert!(pts.len() > 50, "{}", pts.len());
    let mut s: u64 = 7;
    for _ in 0..3000 {
        s = s
            .wrapping_mul(6364136223846793005)
            .wrapping_add(1442695040888963407);
        let x = ((s >> 33) % 8000) as f64 - 4000.0;
        let z = ((s >> 13) % 8000) as f64 - 4000.0;
        let want = p.sample(x, z);
        let got = pts
            .iter()
            .min_by(|a, b| {
                let da = (a.0 - x).powi(2) + (a.1 - z).powi(2);
                let db = (b.0 - x).powi(2) + (b.1 - z).powi(2);
                da.partial_cmp(&db).unwrap()
            })
            .unwrap();
        assert_eq!(
            (got.0.to_bits(), got.1.to_bits()),
            (want.x.to_bits(), want.z.to_bits()),
            "at ({x},{z})"
        );
    }
}

#[test]
fn search_hits_are_consistent_and_fast() {
    let tbl = table();
    let tainted: Vec<u16> = tbl
        .biomes
        .iter()
        .filter(|b| b.gen_id == 193)
        .map(|b| b.rwg_id)
        .collect();
    let volcano = [tbl.volcano_island.unwrap()];
    let q = Query {
        world_type: WorldType::Rwg2Continents,
        land_at_spawn: true,
        conds: vec![
            Cond {
                target: Target::Biome(tainted.clone()),
                radius: 1500,
                negate: false,
                near: vec![],
                helper: false,
            },
            Cond {
                target: Target::LavaCave,
                radius: 4000,
                negate: false,
                near: vec![],
                helper: false,
            },
            Cond {
                target: Target::Volcano,
                radius: 4000,
                negate: false,
                near: vec![],
                helper: false,
            },
        ],
        step: 64,
        center: Center::Origin,
        dim: 0,
    };
    let s = Searcher::new(q, tbl.clone(), RwgConfig::default(), SearchData::default());
    let t0 = std::time::Instant::now();
    let n = 300i64;
    let mut matches = Vec::new();
    let mut land = 0;
    for seed in 0..n {
        let p = s.provider(seed);
        if p.continent_value(0, 0) >= 0.0 {
            land += 1;
        }
        if let Some(m) = s.check_with(&p, &NoHeights) {
            let h = m.hits.clone();
            for hit in &h {
                let c = &s.query.conds[hit.cond];
                let r = c.radius as i64;
                assert!((hit.x as i64).pow(2) + (hit.z as i64).pow(2) <= r * r);
                match &c.target {
                    Target::LavaCave => {}
                    Target::Volcano => {
                        assert!(
                            volcano.contains(&p.biome_data(hit.x, hit.z, &NoHeights).data),
                            "seed {seed}: volcano centre {hit:?}"
                        )
                    }
                    Target::Biome(ids) => {
                        let got = p.biome_data(hit.x, hit.z, &NoHeights).data;
                        assert!(ids.contains(&got), "seed {seed}: hit {hit:?} is {got}");
                    }
                    _ => unreachable!(),
                }
            }
            matches.push((seed, h));
        }
    }
    let el = t0.elapsed();
    eprintln!(
        "{n} seeds in {el:.2?} ({:.0} seeds/s single core), land at spawn {land}, matches {}: {:?}",
        n as f64 / el.as_secs_f64(),
        matches.len(),
        matches.iter().take(3).collect::<Vec<_>>()
    );
    assert!(!matches.is_empty(), "expected some matches in {n} seeds");
}

#[test]
fn profile_parts() {
    use std::time::Instant;
    let tbl = table();
    let s = Searcher::new(
        Query {
            world_type: WorldType::Rwg2Continents,
            land_at_spawn: true,
            conds: vec![],
            step: 32,
            center: Center::Origin,
            dim: 0,
        },
        tbl,
        RwgConfig::default(),
        SearchData::default(),
    );
    let t = Instant::now();
    let ps: Vec<_> = (0..50).map(|i| s.provider(i)).collect();
    eprintln!("construct: {:.2?}/seed", t.elapsed() / 50);
    let t = Instant::now();
    let mut n = 0;
    for p in &ps {
        for i in -20..20 {
            for j in -20..20 {
                let _ = p.biome_data(i * 100, j * 100, &NoHeights);
                n += 1;
            }
        }
    }
    eprintln!(
        "biome_data cold-ish sparse (100-block grid): {:.2?}/sample",
        t.elapsed() / n
    );
    let t = Instant::now();
    let mut n = 0;
    for p in &ps {
        let c = p.continents().unwrap();
        for i in -20..20 {
            for j in -20..20 {
                let _ = c.lava_cave_seed_key(i * 128, j * 128);
                n += 1;
            }
        }
    }
    eprintln!("landform (128 grid): {:.2?}/sample", t.elapsed() / n);
}

#[test]
fn profile_construct() {
    use rwg2_biomes::continental::ContinentalNoise;
    use rwg2_biomes::perlin::PerlinNoise;
    use std::time::Instant;
    let t = Instant::now();
    for i in 0..50 {
        std::hint::black_box(PerlinNoise::new(i));
    }
    eprintln!("perlin new: {:.2?}", t.elapsed() / 50);
    let t = Instant::now();
    for i in 0..50 {
        std::hint::black_box(PoissonPointNoise::new(i, 5450.0, 8).sample(0.0, 0.0));
    }
    eprintln!("poisson(8 rounds) first sample: {:.2?}", t.elapsed() / 50);
    let t = Instant::now();
    for i in 0..50 {
        std::hint::black_box(ContinentalNoise::new(i, &RwgConfig::default()));
    }
    eprintln!("continental new: {:.2?}", t.elapsed() / 50);
    let t = Instant::now();
    for _ in 0..50 {
        std::hint::black_box(table());
    }
    eprintln!("table parse (not per seed): {:.2?}", t.elapsed() / 50);
}

/// "not within R" is the exact complement of "within R" (same seeds, same models).
#[test]
fn negation_is_complement() {
    let tbl = table();
    let tainted: Vec<u16> = tbl
        .biomes
        .iter()
        .filter(|b| b.gen_id == 193)
        .map(|b| b.rwg_id)
        .collect();
    let mk = |t: Target, r: i32, negate: bool| {
        Searcher::new(
            Query {
                world_type: WorldType::Rwg2Continents,
                land_at_spawn: false,
                conds: vec![Cond {
                    target: t,
                    radius: r,
                    negate,
                    near: vec![],
                    helper: false,
                }],
                step: 64,
                center: Center::Origin,
                dim: 0,
            },
            tbl.clone(),
            RwgConfig::default(),
            SearchData::default(),
        )
    };
    for (t, r) in [
        (Target::Roguelike, 500),
        (Target::LootGames, 300),
        (Target::Biome(tainted.clone()), 700),
        (Target::Stronghold, 1200),
    ] {
        let (a, b) = (mk(t.clone(), r, false), mk(t.clone(), r, true));
        let mut pos = 0;
        for seed in 0..60 {
            let (x, y) = (
                a.check_with(&a.provider(seed), &NoHeights).is_some(),
                b.check_with(&b.provider(seed), &NoHeights).is_some(),
            );
            assert_ne!(x, y, "{t:?} seed {seed}");
            pos += x as i32;
        }
        eprintln!("{t:?} <= {r}: {pos}/60 seeds");
    }
}

#[test]
fn profile_mixed_query() {
    let tbl = table();
    let tainted: Vec<u16> = tbl
        .biomes
        .iter()
        .filter(|b| b.gen_id == 193)
        .map(|b| b.rwg_id)
        .collect();
    let veins = rwg2_biomes::gtores::VeinTable::from_json(
        &std::fs::read_to_string(
            std::env::var("RWG2_DATA_DIR")
                .map(std::path::PathBuf::from)
                .unwrap_or_else(|_| {
                    std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../data")
                })
                .join("gtnh-2.9.0/gregtech.json"),
        )
        .unwrap(),
    )
    .unwrap();
    let diamond = veins
        .overworld
        .iter()
        .position(|v| v.name == "ore.mix.diamond")
        .unwrap();
    for conds in [
        vec![Cond {
            target: Target::Roguelike,
            radius: 1500,
            negate: true,
            near: vec![],
            helper: false,
        }],
        vec![Cond {
            target: Target::Biome(tainted.clone()),
            radius: 500,
            negate: false,
            near: vec![],
            helper: false,
        }],
        vec![Cond {
            target: Target::OreVein(diamond),
            radius: 600,
            negate: false,
            near: vec![],
            helper: false,
        }],
        vec![Cond {
            target: Target::OreVein(diamond),
            radius: 600,
            negate: true,
            near: vec![],
            helper: false,
        }],
    ] {
        let name = format!(
            "{:?}",
            conds
                .iter()
                .map(|c| (&c.target, c.negate))
                .collect::<Vec<_>>()
        );
        let s = Searcher::new(
            Query {
                world_type: WorldType::Rwg2Continents,
                land_at_spawn: true,
                conds,
                step: 64,
                center: Center::Origin,
                dim: 0,
            },
            tbl.clone(),
            RwgConfig::default(),
            SearchData {
                veins: Some(veins.clone()),
                ..Default::default()
            },
        );
        let t = std::time::Instant::now();
        let mut n = 0;
        for seed in 0..40 {
            n += s.check(seed).is_some() as i32;
        }
        eprintln!("{name}: {:.1?}/seed, {n}/40 match", t.elapsed() / 40);
    }
}
