//! Relation conditions (`Cond::near` / `Cond::helper`): "A within R of the centre with B within D of it" agrees with
//! direct enumeration (every A candidate nearest first, each checked for B), for a positive and a negated helper,
//! in the overworld and the Nether; a relation search never matches more than its plain parent.
#![cfg(feature = "json")]
use rwg2_biomes::config::RwgConfig;
use rwg2_biomes::provider::{BiomeProvider, BiomeTable, TableHeights, WorldType};
use rwg2_biomes::search::{Center, Cond, Query, SearchData, Searcher, Target};

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

fn searcher(dim: i32, conds: Vec<Cond>) -> Searcher {
    let q = Query {
        world_type: WorldType::Rwg2Continents,
        land_at_spawn: false,
        conds,
        step: 64,
        center: Center::Origin,
        dim,
    };
    Searcher::new(q, table(), RwgConfig::default(), SearchData::default())
}

/// [A (radius r, near helper 1), B helper (radius d, negate)]
fn relation(a: Target, r: i32, b: Target, d: i32, negate: bool) -> Vec<Cond> {
    let mut ca = Cond::new(a, r, false);
    ca.near = vec![1];
    let mut cb = Cond::new(b, d, negate);
    cb.helper = true;
    vec![ca, cb]
}

fn d2(a: (i32, i32), c: (i32, i32)) -> i64 {
    let (x, z) = ((a.0 - c.0) as i64, (a.1 - c.1) as i64);
    x * x + z * z
}

fn portals(seed: i64) -> Vec<(i32, i32)> {
    rwg2_biomes::structures::stronghold_ring(seed)
        .iter()
        .map(|&k| {
            rwg2_biomes::stronghold_pieces::portal_room(seed, k)
                .map_or(rwg2_biomes::structures::chunk_center(k), |r| {
                    (r.eye_frame_centre.0, r.eye_frame_centre.2)
                })
        })
        .collect()
}

#[test]
fn village_near_stronghold_matches_enumeration() {
    let (r, d) = (2500, 500);
    for negate in [false, true] {
        let s = searcher(
            0,
            relation(Target::Village, r, Target::Stronghold, d, negate),
        );
        let plain = searcher(0, vec![Cond::new(Target::Village, r, false)]);
        let (mut hits, mut checked) = (0, 0);
        for seed in 0..60i64 {
            let p = BiomeProvider::new(
                seed,
                WorldType::Rwg2Continents,
                table(),
                RwgConfig::default(),
            );
            let h = &TableHeights;
            let k = (r + 8) / 16 + 1;
            let mut vs: Vec<(i32, i32)> =
                rwg2_biomes::villages::villages_in_rect(&p, h, seed, -k, -k, k + 1, k + 1)
                    .into_iter()
                    .filter(|v| v.viable_exact)
                    .map(|v| (v.x, v.z))
                    .filter(|&v| d2(v, (0, 0)) <= (r as i64).pow(2))
                    .collect();
            vs.sort_by_key(|&v| d2(v, (0, 0)));
            vs.dedup();
            let sp = portals(seed);
            let want = vs
                .into_iter()
                .find(|&v| sp.iter().any(|&q| d2(q, v) <= (d as i64).pow(2)) != negate);
            let got = s.check(seed);
            assert_eq!(
                got.as_ref().map(|m| (m.hits[0].x, m.hits[0].z)),
                want,
                "seed {seed} negate {negate}"
            );
            if let Some(m) = &got {
                hits += 1;
                assert!(plain.check(seed).is_some());
                // positive helper: its hit is reported and lies within d of the village
                if !negate {
                    let b = m.hits.iter().find(|h| h.cond == 1).expect("helper hit");
                    assert!(d2((b.x, b.z), (m.hits[0].x, m.hits[0].z)) <= (d as i64).pow(2));
                } else {
                    assert!(m.hits.iter().all(|h| h.cond == 0));
                }
            }
            checked += 1;
        }
        println!(
            "village ≤{r} with{} stronghold ≤{d}: {hits}/{checked}",
            if negate { "out" } else { "" }
        );
        assert!(hits > 0);
    }
}

#[test]
fn lootgames_near_mineshaft_matches_enumeration() {
    let (r, d) = (1200, 160);
    let s = searcher(
        0,
        relation(Target::LootGames, r, Target::Mineshaft, d, false),
    );
    let mut hits = 0;
    for seed in 0..80i64 {
        let k = (r + 31) >> 4;
        let mut lg: Vec<(i32, i32)> =
            rwg2_biomes::dungeons::lootgames_chunks(seed, 20, -k, -k, k, k)
                .into_iter()
                .map(|(cx, cz)| (cx * 16 + 8, cz * 16 + 8))
                .filter(|&v| d2(v, (0, 0)) <= (r as i64).pow(2))
                .collect();
        lg.sort_by_key(|&v| d2(v, (0, 0)));
        let ms = rwg2_biomes::mineshaft::Seeds::new(seed);
        let starts: Vec<(i32, i32)> =
            rwg2_biomes::mineshaft::starts_in(&ms, -k - 12, -k - 12, k + 13, k + 13)
                .into_iter()
                .map(|(cx, cz)| (cx * 16 + 8, cz * 16 + 8))
                .collect();
        let want = lg
            .into_iter()
            .find(|&v| starts.iter().any(|&q| d2(q, v) <= (d as i64).pow(2)));
        let got = s.check(seed).map(|m| (m.hits[0].x, m.hits[0].z));
        // equal distance ties may pick a different chunk: compare distances
        assert_eq!(
            got.map(|g| d2(g, (0, 0))),
            want.map(|w| d2(w, (0, 0))),
            "seed {seed}"
        );
        hits += got.is_some() as usize;
    }
    println!("lootgames with mineshaft ≤160: {hits}/80");
    assert!(hits > 0);
}

#[test]
fn nether_fortress_near_fortress() {
    // two fortress starts within 600 of each other (Nether region maths, exact)
    let (r, d) = (3000, 250);
    let mut ca = Cond::new(Target::Fortress, r, false);
    ca.near = vec![1];
    let mut cb = Cond::new(Target::Fortress, d, false);
    cb.helper = true;
    let s = searcher(-1, vec![ca, cb]);
    let mut hits = 0;
    for seed in 0..80i64 {
        let k = (r + 64) >> 4;
        let fs: Vec<(i32, i32)> = rwg2_biomes::nether::fortresses_in(seed, -k, -k, k, k)
            .iter()
            .map(|f| f.start_center())
            .collect();
        let mut a: Vec<(i32, i32)> = fs
            .iter()
            .copied()
            .filter(|&v| d2(v, (0, 0)) <= (r as i64).pow(2))
            .collect();
        a.sort_by_key(|&v| d2(v, (0, 0)));
        let kk = (r + d + 64) >> 4;
        let all: Vec<(i32, i32)> = rwg2_biomes::nether::fortresses_in(seed, -kk, -kk, kk, kk)
            .iter()
            .map(|f| f.start_center())
            .collect();
        // same target type: another fortress, not the candidate itself
        let want = a
            .into_iter()
            .find(|&v| all.iter().any(|&q| q != v && d2(q, v) <= (d as i64).pow(2)));
        assert_eq!(
            s.check(seed).map(|m| (m.hits[0].x, m.hits[0].z)),
            want,
            "seed {seed}"
        );
        hits += want.is_some() as usize;
    }
    println!("fortress with another fortress ≤250: {hits}/80");
    assert!(hits > 0);
}
