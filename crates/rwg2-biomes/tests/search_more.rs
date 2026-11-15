//! Search conditions added in v1.12: fortress with ≥ N blaze spawners, village style, aura node type, stronghold
//! under land / ocean, mineshaft. Hits agree with direct enumeration of the same models; "not within" is the exact
//! complement of "within".
#![cfg(feature = "json")]
use rwg2_biomes::config::RwgConfig;
use rwg2_biomes::dungeons::Terrain;
use rwg2_biomes::forestry::ClimateTable;
use rwg2_biomes::provider::{BiomeProvider, BiomeTable, TableHeights, WorldType};
use rwg2_biomes::search::{Center, Cond, Query, SearchData, Searcher, Target};
use rwg2_biomes::tc_nodes::{NodeModifier, NodeType};
use rwg2_biomes::villages::Style;

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

fn searcher(dim: i32, center: Center, conds: Vec<Cond>) -> Searcher {
    let q = Query {
        world_type: WorldType::Rwg2Continents,
        land_at_spawn: false,
        conds,
        step: 64,
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

fn nearest(v: impl IntoIterator<Item = (i32, i32)>, c: (i32, i32), r: i32) -> Option<i64> {
    v.into_iter()
        .map(|p| d2(p, c))
        .filter(|&d| d <= (r as i64).pow(2))
        .min()
}

/// within / not within agree for every seed; returns (matches, checks/s)
fn complement(
    dim: i32,
    center: Center,
    t: Target,
    r: i32,
    seeds: std::ops::Range<i64>,
) -> (usize, f64) {
    let pos = searcher(dim, center, vec![cond(t.clone(), r, false)]);
    let neg = searcher(dim, center, vec![cond(t, r, true)]);
    let t0 = std::time::Instant::now();
    let mut n = 0;
    for s in seeds.clone() {
        let a = pos.check(s).is_some();
        n += a as usize;
        assert_eq!(a, neg.check(s).is_none(), "seed {s}");
    }
    (n, seeds.count() as f64 * 2.0 / t0.elapsed().as_secs_f64())
}

fn hit_d2(s: &Searcher, seed: i64, c: (i32, i32)) -> Option<i64> {
    s.check(seed).map(|m| d2((m.hits[0].x, m.hits[0].z), c))
}

#[test]
fn fortress_spawners_match_enumeration() {
    let c = (100, -50);
    let r = 600;
    for n in [1u8, 2] {
        let s = searcher(
            -1,
            Center::Point(c.0, c.1),
            vec![cond(Target::FortressSpawners(n), r, false)],
        );
        let mut found = 0;
        for seed in 0..80i64 {
            let fs = rwg2_biomes::nether::fortresses_in(
                seed,
                (c.0 - r - 32) >> 4,
                (c.1 - r - 32) >> 4,
                (c.0 + r + 32) >> 4,
                (c.1 + r + 32) >> 4,
            );
            let best = nearest(
                fs.iter()
                    .filter(|f| {
                        rwg2_biomes::fortress::layout(seed, f.chunk_x, f.chunk_z)
                            .is_some_and(|l| l.spawners().len() >= n as usize)
                    })
                    .map(|f| f.start_center()),
                c,
                r,
            );
            assert_eq!(hit_d2(&s, seed, c), best, "seed {seed} n {n}");
            found += best.is_some() as usize;
        }
        let (m, rate) = complement(-1, Center::Origin, Target::FortressSpawners(n), 400, 0..400);
        eprintln!("fortress ≥{n} spawners ≤{r}: {found}/80 enumerated; ≤400 complement {m}/400, {rate:.0} checks/s");
    }
}

#[test]
fn village_style_matches_enumeration() {
    let c = (0, 0);
    let r = 2500;
    for style in [Style::Plains, Style::Taiga, Style::Desert] {
        let s = searcher(
            0,
            Center::Origin,
            vec![cond(Target::VillageStyle(style), r, false)],
        );
        let mut found = 0;
        for seed in 0..12i64 {
            let p = BiomeProvider::new(
                seed,
                WorldType::Rwg2Continents,
                table(),
                RwgConfig::default(),
            );
            let k = (r >> 4) + 4;
            let best = nearest(
                rwg2_biomes::villages::villages_in_rect(&p, &TableHeights, seed, -k, -k, k, k)
                    .into_iter()
                    .filter(|v| v.viable_exact && v.style == style)
                    .map(|v| (v.x, v.z)),
                c,
                r,
            );
            assert_eq!(hit_d2(&s, seed, c), best, "seed {seed} {style:?}");
            found += best.is_some() as usize;
        }
        let (m, rate) = complement(
            0,
            Center::Point(-700, 300),
            Target::VillageStyle(style),
            1500,
            0..12,
        );
        eprintln!(
            "village {} ≤{r}: {found}/12; complement {m}/12, {rate:.1} checks/s",
            style.name()
        );
    }
}

#[test]
fn stronghold_surface_matches_enumeration() {
    let c = (0, 0);
    let r = 2000;
    let mut both = (0, 0);
    for ocean in [false, true] {
        let s = searcher(
            0,
            Center::Origin,
            vec![cond(Target::StrongholdSurface { ocean }, r, false)],
        );
        for seed in 0..20i64 {
            let p = BiomeProvider::new(
                seed,
                WorldType::Rwg2Continents,
                table(),
                RwgConfig::default(),
            );
            let mut t = Terrain::new(&p, &TableHeights);
            let best = nearest(
                rwg2_biomes::structures::stronghold_ring(seed)
                    .iter()
                    .filter_map(|&k| rwg2_biomes::stronghold_pieces::portal_room(seed, k))
                    .map(|r| (r.eye_frame_centre.0, r.eye_frame_centre.2))
                    .filter(|&(x, z)| t.top(x, z).is_some_and(|top| (top <= 61) == ocean))
                    .collect::<Vec<_>>(),
                c,
                r,
            );
            assert_eq!(hit_d2(&s, seed, c), best, "seed {seed} ocean {ocean}");
            if best.is_some() {
                if ocean {
                    both.1 += 1
                } else {
                    both.0 += 1
                }
            }
        }
        let (m, rate) = complement(
            0,
            Center::Origin,
            Target::StrongholdSurface { ocean },
            1800,
            0..20,
        );
        eprintln!(
            "stronghold under {} ≤1800: complement {m}/20, {rate:.0} checks/s",
            if ocean { "ocean" } else { "land" }
        );
    }
    eprintln!(
        "stronghold ≤{r}: under land in {}/20 seeds, under ocean in {}/20",
        both.0, both.1
    );
}

#[test]
fn mineshaft_matches_enumeration() {
    let c = (-300, 900);
    let r = 700;
    let s = searcher(
        0,
        Center::Point(c.0, c.1),
        vec![cond(Target::Mineshaft, r, false)],
    );
    let mut found = 0;
    for seed in 0..200i64 {
        let sd = rwg2_biomes::mineshaft::Seeds::new(seed);
        let best = nearest(
            rwg2_biomes::mineshaft::starts_in(
                &sd,
                (c.0 - r) >> 4,
                (c.1 - r) >> 4,
                ((c.0 + r) >> 4) + 1,
                ((c.1 + r) >> 4) + 1,
            )
            .into_iter()
            .map(|(cx, cz)| (cx * 16 + 8, cz * 16 + 8)),
            c,
            r,
        );
        assert_eq!(hit_d2(&s, seed, c), best, "seed {seed}");
        found += best.is_some() as usize;
    }
    let (m, rate) = complement(0, Center::Origin, Target::Mineshaft, 300, 0..200);
    eprintln!("mineshaft ≤{r} of {c:?}: {found}/200; ≤300 of origin {m}/200, {rate:.0} checks/s");
}

#[test]
fn node_type_matches_enumeration() {
    use rwg2_biomes::tc_dims::{DimTc, DimWorld};
    use rwg2_biomes::thaumcraft::TC_GTNH;
    let cl = climate();
    let c = (0, 0);
    let r = 250;
    let t = Target::TcNodeType {
        node_type: NodeType::Normal,
        modifier: Some(None),
    };
    let s = searcher(-1, Center::Origin, vec![cond(t.clone(), r, false)]);
    let t0 = std::time::Instant::now();
    let mut found = 0;
    for seed in 0..6i64 {
        let nt = rwg2_biomes::dimterrain::NetherTerrain::new(seed);
        let mut g = DimTc::new(DimWorld::Nether(&nt), &cl, TC_GTNH);
        let mut all = Vec::new();
        for cz in ((c.1 - r - 32) >> 4)..=((c.1 + r + 32) >> 4) {
            for cx in ((c.0 - r - 32) >> 4)..=((c.0 + r + 32) >> 4) {
                g.nodes(seed, cx, cz, &mut all);
            }
        }
        let best = nearest(
            all.iter()
                .filter(|n| {
                    n.valid
                        && n.info.as_ref().is_some_and(|i| {
                            i.node_type == NodeType::Normal && i.modifier.is_none()
                        })
                })
                .map(|n| (n.x, n.z)),
            c,
            r,
        );
        // the search returns the first match chunk by chunk (nearest chunk first), so compare existence + radius
        let got = hit_d2(&s, seed, c);
        assert_eq!(got.is_some(), best.is_some(), "seed {seed}");
        if let Some(g) = got {
            assert!(g <= (r as i64).pow(2));
        }
        found += got.is_some() as usize;
    }
    eprintln!(
        "Nether plain Normal node ≤{r}: {found}/6, {:.1} seeds/s",
        6.0 / t0.elapsed().as_secs_f64()
    );
    // a rare type: complement only (overworld, small radius)
    let pure = Target::TcNodeType {
        node_type: NodeType::Pure,
        modifier: None,
    };
    let (m, rate) = complement(0, Center::Origin, pure, 150, 0..4);
    eprintln!("overworld Pure node ≤150: {m}/4, {rate:.1} checks/s");
    let bright = Target::TcNodeType {
        node_type: NodeType::Normal,
        modifier: Some(Some(NodeModifier::Bright)),
    };
    let (m, rate) = complement(7, Center::Origin, bright, 200, 0..4);
    eprintln!("TF Normal Bright node ≤200: {m}/4, {rate:.1} checks/s");
}

fn pop_gates() -> rwg2_biomes::population::PreGates {
    let v: serde_json::Value = serde_json::from_str(
        &std::fs::read_to_string(data_dir().join("gtnh-2.9.0/climate.json")).unwrap(),
    )
    .unwrap();
    rwg2_biomes::population::PreGates::new(v["biomes"].as_array().unwrap().iter().map(|b| {
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

/// Geode search: the hit is the nearest qualifying geode of the direct enumeration (allowing border chunks the
/// provider-biome pre-filter skips to be closer), and seeds/s for the common queries.
#[test]
fn geode_search() {
    let gates = pop_gates();
    let mk = |t: Target, r: i32| {
        let q = Query {
            world_type: WorldType::Rwg2Continents,
            land_at_spawn: false,
            conds: vec![cond(t, r, false)],
            step: 64,
            center: Center::Origin,
            dim: 0,
        };
        Searcher::new(
            q,
            table(),
            RwgConfig::default(),
            SearchData {
                pop_gates: Some(gates.clone()),
                ..Default::default()
            },
        )
    };
    let r = 400;
    let tab = table();
    let (mut agree, mut n) = (0, 0);
    for seed in [12345i64, 1337, 42, 92900795, -7] {
        for (min_d, kind) in [(0u16, 0u8), (1, 0)] {
            let s = mk(
                Target::Geode {
                    min_diamond: min_d,
                    kind,
                },
                r,
            );
            let hit = s
                .check(seed)
                .map(|m| d2((m.hits[0].x, m.hits[0].z), (0, 0)));
            let p = BiomeProvider::new(
                seed,
                WorldType::Rwg2Continents,
                tab.clone(),
                RwgConfig::default(),
            );
            let mut m = rwg2_biomes::population::PopModel::new(&p, &TableHeights, seed);
            m.gates = gates.clone();
            let k = (r + 16) >> 4;
            let all = m.geodes_in_rect(-k, -k, k, k);
            let want = nearest(
                all.iter()
                    .filter(|g| g.diamond >= min_d)
                    .map(|g| (g.x, g.z)),
                (0, 0),
                r,
            );
            n += 1;
            if hit == want {
                agree += 1;
            } else {
                eprintln!("seed {seed} min {min_d}: search {hit:?} vs enumeration {want:?}");
                // a miss may only come from a skipped border chunk: the search hit, if any, must still be a real geode
                assert!(hit.is_none() || want.is_some_and(|w| hit.unwrap() >= w));
            }
        }
    }
    let t0 = std::time::Instant::now();
    let s = mk(
        Target::Geode {
            min_diamond: 1,
            kind: 0,
        },
        1000,
    );
    let mut found = 0;
    for seed in 0..20 {
        found += s.check(seed).is_some() as usize;
    }
    eprintln!("geode search: {agree}/{n} hits equal the enumeration; diamond geode within 1000: {found}/20 seeds, {:.1} seeds/s", 20.0 / t0.elapsed().as_secs_f64());
    assert!(agree * 10 >= n * 8);
}

/// Lake search = the nearest qualifying lake of `lakes_in_rect` over the radius (exact: same hit for every seed).
#[test]
fn lake_search() {
    let gates = pop_gates();
    let mk = |t: Target, r: i32| {
        let q = Query {
            world_type: WorldType::Rwg2Continents,
            land_at_spawn: false,
            conds: vec![cond(t, r, false)],
            step: 64,
            center: Center::Origin,
            dim: 0,
        };
        Searcher::new(
            q,
            table(),
            RwgConfig::default(),
            SearchData {
                pop_gates: Some(gates.clone()),
                ..Default::default()
            },
        )
    };
    let r = if cfg!(target_arch = "wasm32") {
        200
    } else {
        400
    };
    let tab = table();
    let (mut agree, mut n) = (0, 0);
    let seeds: &[i64] = if cfg!(target_arch = "wasm32") {
        &[1337, 42]
    } else {
        &[12345, 1337, 42, 92900795, -7]
    };
    for &seed in seeds {
        let p = BiomeProvider::new(
            seed,
            WorldType::Rwg2Continents,
            tab.clone(),
            RwgConfig::default(),
        );
        let mut m = rwg2_biomes::population::PopModel::new(&p, &TableHeights, seed);
        m.gates = gates.clone();
        let k = (r + 48) >> 4;
        let all = m.lakes_in_rect(-k, -k, k, k);
        for (liquid, min_y, min_cells, open) in [
            (0u8, 0, 0u32, false),
            (1, 0, 0, false),
            (2, 30, 50, false),
            (0, 0, 0, true),
        ] {
            let s = mk(
                Target::Lake {
                    liquid,
                    min_y,
                    min_cells,
                    open,
                },
                r,
            );
            let hit = s
                .check(seed)
                .map(|m| d2((m.hits[0].x, m.hits[0].z), (0, 0)));
            let want = nearest(
                all.iter()
                    .filter(|l| {
                        (liquid == 0 || (liquid == 1) == l.lava)
                            && l.y1 >= min_y
                            && l.cells >= min_cells
                            && (!open || l.open)
                    })
                    .map(|l| (l.x, l.z)),
                (0, 0),
                r,
            );
            n += 1;
            if hit == want {
                agree += 1;
            } else {
                eprintln!(
                    "seed {seed} lake {liquid}/{min_y}/{min_cells}/{open}: search {hit:?} vs enumeration {want:?}"
                );
            }
        }
    }
    let t0 = std::time::Instant::now();
    let s = mk(
        Target::Lake {
            liquid: 1,
            min_y: 0,
            min_cells: 0,
            open: false,
        },
        500,
    );
    let mut found = 0;
    let ns = if cfg!(target_arch = "wasm32") { 4 } else { 30 };
    for seed in 0..ns {
        found += s.check(seed).is_some() as usize;
    }
    eprintln!(
        "lake search: {agree}/{n} = enumeration; lava lake within 500: {found}/{ns} seeds, {:.1} seeds/s",
        ns as f64 / t0.elapsed().as_secs_f64()
    );
    assert_eq!(agree, n);
}

/// Loot search = the nearest qualifying chest of a direct populate replay over every chunk in range.
#[test]
fn loot_search() {
    let gates = pop_gates();
    let mk = |t: Target, r: i32| {
        let q = Query {
            world_type: WorldType::Rwg2Continents,
            land_at_spawn: false,
            conds: vec![cond(t, r, false)],
            step: 64,
            center: Center::Origin,
            dim: 0,
        };
        Searcher::new(
            q,
            table(),
            RwgConfig::default(),
            SearchData {
                pop_gates: Some(gates.clone()),
                ..Default::default()
            },
        )
    };
    let r = if cfg!(target_arch = "wasm32") { 160 } else { 320 };
    let tab = table();
    let dt = rwg2_biomes::loot::LootTable::dungeon();
    let mt = rwg2_biomes::loot::LootTable::mineshaft().expect("mineshaftCorridor table");
    let seeds: &[i64] = if cfg!(target_arch = "wasm32") {
        &[1631302684446519836]
    } else {
        &[1631302684446519836, 1337, 42]
    };
    let (mut agree, mut n, mut found) = (0, 0, 0);
    for &seed in seeds {
        let p = BiomeProvider::new(
            seed,
            WorldType::Rwg2Continents,
            tab.clone(),
            RwgConfig::default(),
        );
        let mut m = rwg2_biomes::population::PopModel::new(&p, &TableHeights, seed);
        m.gates = gates.clone();
        let k = (r + 48) >> 4;
        m.load_structures(-k - 3, -k - 3, k, k);
        // every chest: (source, table, x, z, slots)
        type Chest = (u8, bool, i32, i32, Vec<(u8, u16, u8)>);
        let mut all: Vec<Chest> = Vec::new();
        for cz in -k - 1..=k {
            for cx in -k - 1..=k {
                for d in m.dungeons_at(cx, cz) {
                    for (x, _, z, ch) in d.chests {
                        all.push((1, false, x, z, ch.slots));
                    }
                }
                for (x, _, z, ch) in std::mem::take(&mut m.mine_chests) {
                    all.push((2, true, x, z, ch.slots));
                }
            }
        }
        for (source, items, min) in [
            (0u8, vec![], 1u32),
            (2, vec![], 1),
            (1, vec!["item.enchantedBook"], 1),
            (0, vec!["item.ingotIron@", "item.ingotGold@"], 3),
            (2, vec!["item.diamond@"], 1),
        ] {
            let s = mk(
                Target::Loot {
                    source,
                    items: items.iter().map(|s| s.to_string()).collect(),
                    min_count: min,
                },
                r,
            );
            let hit = s
                .check(seed)
                .map(|m| d2((m.hits[0].x, m.hits[0].z), (0, 0)));
            let want = nearest(
                all.iter()
                    .filter(|c| {
                        (source == 0 || source == c.0) && {
                            let t = if c.1 { mt } else { dt };
                            let cnt: u32 = c
                                .4
                                .iter()
                                .filter(|sl| {
                                    let nm = &t.items[sl.1 as usize].name;
                                    items.iter().any(|w| nm.contains(w))
                                })
                                .map(|sl| sl.2 as u32)
                                .sum();
                            items.is_empty() || cnt >= min
                        }
                    })
                    .map(|c| (c.2, c.3)),
                (0, 0),
                r,
            );
            n += 1;
            found += want.is_some() as u32;
            if hit == want {
                agree += 1;
            } else {
                eprintln!("seed {seed} loot {source}/{items:?}/{min}: search {hit:?} vs enumeration {want:?}");
            }
        }
    }
    eprintln!("loot search: {agree}/{n} agree, {found} with a hit");
    assert_eq!(agree, n);
    assert!(found > 0);
}
