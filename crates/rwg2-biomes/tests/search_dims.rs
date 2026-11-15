//! Search in the Nether / Twilight Forest, adjacency and custom centres: hits agree with direct enumeration of the
//! same models, and "not within" is the exact complement of "within".
#![cfg(feature = "json")]
use rwg2_biomes::config::RwgConfig;
use rwg2_biomes::forestry::ClimateTable;
use rwg2_biomes::gtores::{is_oreseed, GT_GTNH};
use rwg2_biomes::nether::{fortresses_in, vein_table, NetherBiomes, GT_NETHER_VEINS, GT_TF_VEINS};
use rwg2_biomes::provider::{BiomeTable, WorldType};
use rwg2_biomes::search::{Center, Cond, Query, SearchData, Searcher, Target};
use rwg2_biomes::twilight::{feature_id, TwilightForest};

fn data_dir() -> std::path::PathBuf {
    std::env::var("RWG2_DATA_DIR")
        .map(std::path::PathBuf::from)
        .unwrap_or_else(|_| std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../data"))
}

fn table() -> BiomeTable {
    BiomeTable::from_rwg2dump_json(
        &std::fs::read_to_string(data_dir().join("gtnh-2.9.0/biomes.json")).unwrap(),
    )
    .unwrap()
}

fn climate() -> ClimateTable {
    ClimateTable::from_json(
        &std::fs::read_to_string(data_dir().join("gtnh-2.9.0/climate.json")).unwrap(),
    )
    .unwrap()
}

fn searcher(dim: i32, center: Center, step: i32, conds: Vec<Cond>) -> Searcher {
    let q = Query {
        world_type: WorldType::Rwg2Continents,
        land_at_spawn: false,
        conds,
        step,
        center,
        dim,
    };
    let data = SearchData {
        climate: Some(climate()),
        ..Default::default()
    };
    Searcher::new(q, table(), RwgConfig::default(), data)
}

fn cond(target: Target, radius: i32, negate: bool) -> Cond {
    Cond {
        target,
        radius,
        negate,
        near: vec![],
        helper: false,
    }
}

fn d2(a: (i32, i32), c: (i32, i32)) -> i64 {
    let (x, z) = ((a.0 - c.0) as i64, (a.1 - c.1) as i64);
    x * x + z * z
}

/// within / not within agree with each other for every seed
fn complement(
    dim: i32,
    center: Center,
    step: i32,
    t: Target,
    r: i32,
    seeds: std::ops::Range<i64>,
) -> (usize, f64) {
    let pos = searcher(dim, center, step, vec![cond(t.clone(), r, false)]);
    let neg = searcher(dim, center, step, vec![cond(t, r, true)]);
    let t0 = std::time::Instant::now();
    let mut n = 0;
    for s in seeds.clone() {
        let a = pos.check(s).is_some();
        n += a as usize;
        assert_eq!(a, neg.check(s).is_none(), "seed {s}");
    }
    (n, seeds.count() as f64 * 2.0 / t0.elapsed().as_secs_f64())
}

#[test]
fn tf_features_match_enumeration() {
    let c = (300, -700);
    let r = 1200;
    let want = vec![feature_id::LICH_TOWER];
    let s = searcher(
        7,
        Center::Point(c.0, c.1),
        64,
        vec![cond(Target::TfFeature(want.clone()), r, false)],
    );
    let t0 = std::time::Instant::now();
    let mut found = 0;
    for seed in 0..60i64 {
        let tf = TwilightForest::new(seed);
        let best = tf
            .features_in(
                (c.0 - r) >> 4,
                (c.1 - r) >> 4,
                (c.0 + r) >> 4,
                (c.1 + r) >> 4,
            )
            .into_iter()
            .filter(|f| want.contains(&f.feature.id))
            .map(|f| (f.x, f.z))
            .filter(|&p| d2(p, c) <= (r as i64).pow(2))
            .min_by_key(|&p| d2(p, c));
        let got = s.check(seed).map(|m| (m.hits[0].x, m.hits[0].z));
        assert_eq!(got.map(|p| d2(p, c)), best.map(|p| d2(p, c)), "seed {seed}");
        found += got.is_some() as usize;
    }
    eprintln!(
        "TF Lich Tower ≤ {r} of {c:?}: {found}/60 seeds, {:.0} seeds/s",
        60.0 / t0.elapsed().as_secs_f64()
    );
    let (n, rate) = complement(
        7,
        Center::Origin,
        64,
        Target::TfFeature(vec![feature_id::HYDRA_LAIR]),
        2000,
        0..40,
    );
    eprintln!("TF Hydra Lair ≤ 2000: {n}/40, {rate:.0} checks/s");
}

#[test]
fn fortresses_match_enumeration() {
    let r = 400;
    let s = searcher(
        -1,
        Center::Origin,
        64,
        vec![cond(Target::Fortress, r, false)],
    );
    let t0 = std::time::Instant::now();
    let mut found = 0;
    for seed in 0..500i64 {
        let best = fortresses_in(seed, -40, -40, 40, 40)
            .iter()
            .map(|f| f.start_center())
            .filter(|&p| d2(p, (0, 0)) <= (r as i64).pow(2))
            .min_by_key(|&p| d2(p, (0, 0)));
        let got = s.check(seed).map(|m| (m.hits[0].x, m.hits[0].z));
        assert_eq!(got, best, "seed {seed}");
        found += got.is_some() as usize;
    }
    eprintln!(
        "fortress ≤ {r}: {found}/500, {:.0} seeds/s",
        500.0 / t0.elapsed().as_secs_f64()
    );
    complement(
        -1,
        Center::Point(-2000, 900),
        64,
        Target::Fortress,
        300,
        0..200,
    );
}

#[test]
fn dim_biomes_match_quarter_grid() {
    // Nether: Undergarden (138); TF: Dark Forest Center (172)
    for (dim, id, r) in [(-1, 138u16, 700), (7, 172u16, 1500)] {
        let step = 32;
        let s = searcher(
            dim,
            Center::Origin,
            step,
            vec![cond(Target::Biome(vec![id]), r, false)],
        );
        let t0 = std::time::Instant::now();
        let mut found = 0;
        for seed in 0..30i64 {
            let biome = |x: i32, z: i32| -> i32 {
                if dim == 7 {
                    TwilightForest::new(seed).biomes_quarter(x >> 2, z >> 2, 1, 1)[0]
                } else {
                    NetherBiomes::new(seed).biomes_quarter(x >> 2, z >> 2, 1, 1)[0]
                }
            };
            let got = s.check(seed).map(|m| (m.hits[0].x, m.hits[0].z));
            if let Some(p) = got {
                assert_eq!(
                    biome(p.0, p.1),
                    id as i32,
                    "seed {seed}: hit not in the biome"
                );
                found += 1;
            } else {
                // spot-check: no grid point in range has it (coarse 4× grid of the search grid)
                let n = r / (step * 4);
                for j in -n..=n {
                    for i in -n..=n {
                        let (x, z) = (i * step * 4, j * step * 4);
                        if d2((x, z), (0, 0)) <= (r as i64).pow(2) {
                            assert_ne!(biome(x, z), id as i32, "seed {seed}: missed ({x},{z})");
                        }
                    }
                }
            }
        }
        eprintln!(
            "dim {dim} biome {id} ≤ {r}: {found}/30, {:.1} seeds/s",
            30.0 / t0.elapsed().as_secs_f64()
        );
        let (_, rate) = complement(
            dim,
            Center::Point(1000, 1000),
            64,
            Target::Biome(vec![id]),
            r,
            0..20,
        );
        eprintln!("  complement {rate:.1} checks/s");
    }
}

#[test]
fn dim_ore_veins_match_candidates() {
    for (dim, rows) in [(-1, &GT_NETHER_VEINS[..]), (7, &GT_TF_VEINS[..])] {
        let table = vein_table(rows);
        let want = 1usize;
        let r = 300;
        let s = searcher(
            dim,
            Center::Origin,
            64,
            vec![cond(Target::OreVein(want), r, false)],
        );
        let t0 = std::time::Instant::now();
        let mut in_check = std::time::Duration::ZERO;
        let mut found = 0;
        for seed in 0..100i64 {
            let mut best: Option<(i32, i32)> = None;
            // TF seed = world seed in GTNH (empty `TwilightForestSeed`)
            let mut stone = rwg2_biomes::gtdims::DimStone::new(dim, seed).unwrap();
            for cz in (-r >> 4) - 1..=(r >> 4) + 1 {
                for cx in (-r >> 4) - 1..=(r >> 4) + 1 {
                    let p = (cx * 16 + 8, cz * 16 + 8);
                    if !is_oreseed(&GT_GTNH, cx, cz) || d2(p, (0, 0)) > (r as i64).pow(2) {
                        continue;
                    }
                    let c = rwg2_biomes::gtdims::resolve_dim(
                        seed, dim, &table, &GT_GTNH, cx, cz, &mut stone,
                    );
                    if c.is_some_and(|pl| pl.vein == want)
                        && best.is_none_or(|b| d2(p, (0, 0)) < d2(b, (0, 0)))
                    {
                        best = Some(p);
                    }
                }
            }
            let t1 = std::time::Instant::now();
            let got = s.check(seed).map(|m| (m.hits[0].x, m.hits[0].z));
            in_check += t1.elapsed();
            assert_eq!(
                got.map(|p| d2(p, (0, 0))),
                best.map(|p| d2(p, (0, 0))),
                "dim {dim} seed {seed}"
            );
            found += got.is_some() as usize;
        }
        eprintln!(
            "dim {dim} vein #{want} ≤ {r}: {found}/100, search {:.0} seeds/s (test incl. brute-force reference {:.0} seeds/s)",
            100.0 / in_check.as_secs_f64(),
            100.0 / t0.elapsed().as_secs_f64()
        );
    }
}

/// Fast variant = nearest oreseed whose first candidate is the wanted vein; two ore conditions on one seed (shared
/// per-seed oreseed cache) find the same hits as each alone.
#[test]
fn dim_ore_veins_fast_and_shared_cache() {
    use rwg2_biomes::nether::gt_candidates;
    for (dim, rows) in [(-1, &GT_NETHER_VEINS[..]), (7, &GT_TF_VEINS[..])] {
        let table = vein_table(rows);
        let r = 300;
        let fast = searcher(
            dim,
            Center::Origin,
            64,
            vec![cond(Target::OreVeinFast(1), r, false)],
        );
        let t0 = std::time::Instant::now();
        for seed in 0..2000i64 {
            let got = fast.check(seed).map(|m| (m.hits[0].x, m.hits[0].z));
            if seed < 200 {
                let mut best: Option<(i32, i32)> = None;
                for cz in (-r >> 4) - 1..=(r >> 4) + 1 {
                    for cx in (-r >> 4) - 1..=(r >> 4) + 1 {
                        let p = (cx * 16 + 8, cz * 16 + 8);
                        if is_oreseed(&GT_GTNH, cx, cz)
                            && d2(p, (0, 0)) <= (r as i64).pow(2)
                            && gt_candidates(seed, dim, &table, &GT_GTNH, cx, cz, 1)
                                .first()
                                .is_some_and(|pl| pl.vein == 1)
                            && best.is_none_or(|b| d2(p, (0, 0)) < d2(b, (0, 0)))
                        {
                            best = Some(p);
                        }
                    }
                }
                assert_eq!(
                    got.map(|p| d2(p, (0, 0))),
                    best.map(|p| d2(p, (0, 0))),
                    "fast dim {dim} seed {seed}"
                );
            }
        }
        eprintln!(
            "dim {dim} fast vein #1 ≤ {r}: {:.0} seeds/s",
            2000.0 / t0.elapsed().as_secs_f64()
        );
        let (a, b) = (1usize, 3usize);
        let one = |w| {
            searcher(
                dim,
                Center::Origin,
                64,
                vec![cond(Target::OreVein(w), r, false)],
            )
        };
        let (sa, sb) = (one(a), one(b));
        let both = searcher(
            dim,
            Center::Origin,
            64,
            vec![
                cond(Target::OreVein(a), r, false),
                cond(Target::OreVein(b), r, false),
            ],
        );
        let (mut n, mut t_sep, mut t_both) =
            (0, std::time::Duration::ZERO, std::time::Duration::ZERO);
        for seed in 0..60i64 {
            let t1 = std::time::Instant::now();
            let ha = sa.check(seed).map(|m| (m.hits[0].x, m.hits[0].z));
            let hb = sb.check(seed).map(|m| (m.hits[0].x, m.hits[0].z));
            t_sep += t1.elapsed();
            let t1 = std::time::Instant::now();
            let hab = both.check(seed);
            t_both += t1.elapsed();
            match (ha, hb) {
                (Some(pa), Some(pb)) => {
                    let m = hab.expect("both");
                    let at = |c: usize| m.hits.iter().find(|h| h.cond == c).map(|h| (h.x, h.z));
                    assert_eq!(at(0), Some(pa), "dim {dim} seed {seed} a");
                    assert_eq!(at(1), Some(pb), "dim {dim} seed {seed} b");
                    n += 1;
                }
                _ => assert!(hab.is_none(), "dim {dim} seed {seed}"),
            }
        }
        eprintln!(
            "dim {dim} veins #{a} + #{b}: {n}/60 seeds, separate {t_sep:?}, one search {t_both:?}"
        );
    }
}

#[test]
fn adjacency_hits_have_both_biomes() {
    // Nether: Hell (8) next to Undergarden (138) within 64 blocks
    let (a, b, dist, r) = (8u16, 138u16, 64, 800);
    let t = Target::Adjacent {
        a: vec![a],
        b: vec![b],
        dist,
    };
    let s = searcher(-1, Center::Origin, 32, vec![cond(t.clone(), r, false)]);
    let t0 = std::time::Instant::now();
    let mut found = 0;
    for seed in 0..30i64 {
        let nb = NetherBiomes::new(seed);
        let at = |x: i32, z: i32| nb.biomes_quarter(x >> 2, z >> 2, 1, 1)[0];
        if let Some(m) = s.check(seed) {
            let (x, z) = (m.hits[0].x, m.hits[0].z);
            assert_eq!(at(x, z), a as i32);
            // a B cell within dist (+ one adjacency grid step of slack)
            let st = (dist / 4).clamp(4, 32);
            let reach = dist + st;
            let mut ok = false;
            'o: for dz in (-reach..=reach).step_by(4) {
                for dx in (-reach..=reach).step_by(4) {
                    if dx * dx + dz * dz <= reach * reach && at(x + dx, z + dz) == b as i32 {
                        ok = true;
                        break 'o;
                    }
                }
            }
            assert!(ok, "seed {seed}: no {b} near ({x},{z})");
            found += 1;
        }
    }
    eprintln!(
        "Nether Hell next to Undergarden (≤{dist}) within {r}: {found}/30, {:.1} seeds/s",
        30.0 / t0.elapsed().as_secs_f64()
    );
    complement(-1, Center::Origin, 32, t, r, 0..15);

    // overworld: Tainted Land (193) bordering any other biome within 200 blocks; the hit is Tainted Land
    let tbl = table();
    let tainted: Vec<u16> = tbl
        .biomes
        .iter()
        .filter(|x| x.gen_id == 193)
        .map(|x| x.rwg_id)
        .collect();
    let other: Vec<u16> = tbl
        .biomes
        .iter()
        .filter(|x| x.gen_id != 193)
        .map(|x| x.rwg_id)
        .collect();
    let t = Target::Adjacent {
        a: tainted.clone(),
        b: other,
        dist: 200,
    };
    let s = searcher(0, Center::Origin, 64, vec![cond(t.clone(), 2500, false)]);
    let p0 = std::time::Instant::now();
    let mut n = 0;
    for seed in 0..10i64 {
        if let Some(m) = s.check(seed) {
            let p = s.provider(seed);
            let id = p
                .biome_data(
                    m.hits[0].x,
                    m.hits[0].z,
                    &rwg2_biomes::provider::TableHeights,
                )
                .data;
            assert!(tainted.contains(&id), "seed {seed}");
            n += 1;
        }
    }
    eprintln!(
        "overworld Tainted bordering another biome ≤200 within 2500: {n}/10, {:.1} seeds/s",
        10.0 / p0.elapsed().as_secs_f64()
    );
    assert!(n > 0);
    complement(0, Center::Origin, 64, t, 1500, 0..6);
}

#[test]
fn dim_aura_nodes_consistent() {
    use rwg2_biomes::dimterrain::NetherTerrain;
    use rwg2_biomes::tc_dims::{DimTc, DimWorld};
    use rwg2_biomes::thaumcraft::TC_GTNH;
    let cl = climate();
    let r = 250;
    let s = searcher(
        -1,
        Center::Origin,
        64,
        vec![cond(
            Target::TcAspect {
                aspect: u8::MAX,
                min: 1,
            },
            r,
            false,
        )],
    );
    let t0 = std::time::Instant::now();
    let mut found = 0;
    for seed in 0..8i64 {
        let nt = NetherTerrain::new(seed);
        let mut g = DimTc::new(DimWorld::Nether(&nt), &cl, TC_GTNH);
        let mut nodes = Vec::new();
        for cz in -17..=16 {
            for cx in -17..=16 {
                g.nodes(seed, cx, cz, &mut nodes);
            }
        }
        let any = nodes
            .iter()
            .any(|n| n.valid && n.info.is_some() && d2((n.x, n.z), (0, 0)) <= (r as i64).pow(2));
        let got = s.check(seed);
        assert_eq!(got.is_some(), any, "seed {seed}");
        found += any as usize;
    }
    eprintln!(
        "Nether aura node ≤ {r}: {found}/8, {:.1} seeds/s",
        8.0 / t0.elapsed().as_secs_f64()
    );
}

#[test]
fn custom_centre_overworld() {
    // the custom centre is used as-is (and reported back)
    let tbl = table();
    let tainted: Vec<u16> = tbl
        .biomes
        .iter()
        .filter(|x| x.gen_id == 193)
        .map(|x| x.rwg_id)
        .collect();
    let c = (12_000, -8_000);
    let s = searcher(
        0,
        Center::Point(c.0, c.1),
        64,
        vec![cond(Target::Biome(tainted), 1500, false)],
    );
    let mut n = 0;
    for seed in 0..10i64 {
        if let Some(m) = s.check(seed) {
            assert_eq!(m.center, c);
            assert!(d2((m.hits[0].x, m.hits[0].z), c) <= 1500i64 * 1500);
            n += 1;
        }
    }
    assert!(n > 0);
}

/// End targets vs direct enumeration (`end::islands` / `towers` / `comets` over a wide rect), complement, speed.
#[test]
fn end_targets_match_enumeration() {
    use rwg2_biomes::end;
    let c = (-400, 900);
    let r = 4000;
    let within = |x: i32, z: i32| d2((x, z), c) <= r as i64 * r as i64;
    for seed in 0..12i64 {
        let isl: Vec<_> = end::islands(seed, 0, -400, -400, 400, 400)
            .into_iter()
            .filter(|i| i.biome == 2 && within(i.cx * 16 + 112, i.cz * 16 + 112))
            .map(|i| (i.cx * 16 + 112, i.cz * 16 + 112))
            .collect();
        let s = searcher(
            1,
            Center::Point(c.0, c.1),
            64,
            vec![cond(
                Target::EndIsland {
                    biome: Some(2),
                    variation: None,
                },
                r,
                false,
            )],
        );
        let hit = s.check(seed).map(|m| (m.hits[0].x, m.hits[0].z));
        assert_eq!(
            hit,
            isl.iter().copied().min_by_key(|&p| d2(p, c)),
            "island seed {seed}"
        );
        let tw: Vec<_> = end::towers(seed, 0, -400, -400, 400, 400)
            .into_iter()
            .map(|(x, z)| rwg2_biomes::endtower::tower_centre(seed, x, z))
            .filter(|&(x, z)| within(x, z))
            .collect();
        let s = searcher(
            1,
            Center::Point(c.0, c.1),
            64,
            vec![cond(Target::EndTower, r, false)],
        );
        assert_eq!(
            s.check(seed).map(|m| (m.hits[0].x, m.hits[0].z)),
            tw.iter().copied().min_by_key(|&p| d2(p, c)),
            "tower seed {seed}"
        );
        let co: Vec<_> = end::comets(seed, 10_000, -400, -400, 400, 400)
            .into_iter()
            .filter(|k| k.size >= 5 && within(k.x, k.z))
            .map(|k| (k.x, k.z))
            .collect();
        let s = searcher(
            1,
            Center::Point(c.0, c.1),
            64,
            vec![cond(Target::EndComet { min_size: 5 }, r, false)],
        );
        assert_eq!(
            s.check(seed).map(|m| (m.hits[0].x, m.hits[0].z)),
            co.iter().copied().min_by_key(|&p| d2(p, c)),
            "comet seed {seed}"
        );
    }
    let (n, rate) = complement(
        1,
        Center::Origin,
        64,
        Target::EndIsland {
            biome: Some(1),
            variation: Some(5),
        },
        3000,
        0..400,
    );
    eprintln!("End Burning Mountains (Mine) island ≤ 3000: {n}/400, {rate:.0} checks/s");
    let (n, rate) = complement(
        1,
        Center::Origin,
        64,
        Target::EndComet { min_size: 2 },
        2500,
        0..60,
    );
    eprintln!("End comet ≤ 2500: {n}/60, {rate:.0} checks/s");
    let (n, rate) = complement(1, Center::Origin, 64, Target::OreVeinFast(6), 3000, 0..60);
    eprintln!("End beryllium vein (fast) ≤ 3000: {n}/60, {rate:.0} checks/s");
    let (n, rate) = complement(1, Center::Origin, 64, Target::OreVein(6), 300, 0..30);
    eprintln!("End beryllium vein ≤ 300 (main island zone): {n}/30, {rate:.1} checks/s");
}
