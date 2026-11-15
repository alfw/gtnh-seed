#![cfg(feature = "json")]
use rwg2_biomes::chunkgen::ChunkGen;
use rwg2_biomes::config::RwgConfig;
use rwg2_biomes::provider::{BiomeProvider, BiomeTable, TableHeights, WorldType};
use rwg2_biomes::spawn::{
    estimate_spawn, likely_grass_top, spawnable_top, top_probs, walk_positions, SurfaceModel,
};

fn table() -> BiomeTable {
    let dir = std::env::var("RWG2_DATA_DIR")
        .map(std::path::PathBuf::from)
        .unwrap_or_else(|_| {
            std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../data")
        });
    BiomeTable::from_rwg2dump_json(
        &std::fs::read_to_string(dir.join("gtnh-2.9.0/biomes.json")).unwrap(),
    )
    .unwrap()
}

/// Reference from `java.util.Random` (`x += nextInt(64) - nextInt(64); z += ...`), first 8 moves.
#[test]
fn walk_matches_java_random() {
    let want: [(i64, [(i32, i32); 8]); 3] = [
        (
            1337,
            [
                (31, -11),
                (26, -5),
                (-4, -3),
                (4, -46),
                (50, -62),
                (3, -89),
                (1, -100),
                (41, -137),
            ],
        ),
        (
            0,
            [
                (-7, -23),
                (14, 5),
                (2, 10),
                (-13, 11),
                (42, 60),
                (39, 45),
                (3, 46),
                (37, 71),
            ],
        ),
        (
            -4172144997902289642,
            [
                (-11, -33),
                (16, -45),
                (44, -65),
                (54, -86),
                (76, -121),
                (54, -118),
                (74, -87),
                (99, -107),
            ],
        ),
    ];
    for (seed, w) in want {
        let p = walk_positions(seed);
        assert_eq!(p.len(), 1001);
        assert_eq!(p[0], (0, 0));
        assert_eq!(&p[1..9], &w[..], "seed {seed}");
    }
}

/// Predicate vs the real pre-population world (seed 1337 CONTINENTS, chunks -6..5, rwg2dump 2.2 `worldchunks`).
#[test]
fn predicate_vs_world_chunks() {
    let dir = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../testdata/pack/gtnh-2.9.0/dump22");
    let Ok(bytes) = std::fs::read(dir.join("worldchunks_1337.bin")) else {
        eprintln!("skip");
        return;
    };
    let (w, x0) = (12i32, -6i32);
    let n = (w * w * 256) as usize;
    let p = BiomeProvider::new(
        1337,
        WorldType::Rwg2Continents,
        table(),
        RwgConfig::default(),
    );
    let m = SurfaceModel::new(&p.table);
    let mut g = ChunkGen::new(&p);
    // [truth][pred]
    let mut conf = [[0usize; 2]; 2];
    let mut sconf = [[0usize; 2]; 2]; // sand/stone top: [truth][pred >= 0.5]
    let mut ssum = (0f64, 0usize); // expected vs real count of sand/stone tops
    let mut by_biome = std::collections::BTreeMap::<u16, [[usize; 2]; 2]>::new();
    for cz in 0..w {
        for cx in 0..w {
            let c = g.generate(&p, &TableHeights, x0 + cx, x0 + cz).unwrap();
            for idx in 0..256usize {
                let k = ((cz * w + cx) as usize) * 256 + idx;
                let top = match bytes[n * 2 + k] {
                    255 => -1,
                    v => v as i32,
                };
                let blk = u16::from_le_bytes([bytes[n * 3 + k * 2], bytes[n * 3 + k * 2 + 1]]);
                let truth = (blk == 0 && top >= 63) as usize; // blocks[0] = minecraft:grass
                let (x, z) = (
                    (x0 + cx) * 16 + (idx & 15) as i32,
                    (x0 + cz) * 16 + (idx >> 4) as i32,
                );
                let pred = (likely_grass_top(&p, &m, &c, x, z) >= 0.5) as usize;
                conf[truth][pred] += 1;
                // blocks: 0 grass, 1 cobblestone, 2 stone, 3 water, 4 dirt, 5 clay (no sand in the area)
                let st = ((blk == 2) && top >= 63) as usize;
                let sp = spawnable_top(&p, &m, &c, x, z);
                sconf[st][(sp >= 0.5) as usize] += 1;
                ssum.0 += sp as f64;
                ssum.1 += st;
                by_biome.entry(c.rwg[idx]).or_default()[truth][pred] += 1;
            }
        }
    }
    // walk positions inside the dumped area: real pre-population top vs model
    for (i, &(x, z)) in walk_positions(1337).iter().take(12).enumerate() {
        let (cx, cz) = ((x >> 4) - x0, (z >> 4) - x0);
        if cx < 0 || cz < 0 || cx >= w || cz >= w {
            continue;
        }
        let idx = ((z & 15) * 16 + (x & 15)) as usize;
        let k = ((cz * w + cx) as usize) * 256 + idx;
        let top = match bytes[n * 2 + k] {
            255 => -1,
            v => v as i32,
        };
        let blk = u16::from_le_bytes([bytes[n * 3 + k * 2], bytes[n * 3 + k * 2 + 1]]);
        let c = g.generate(&p, &TableHeights, x >> 4, z >> 4).unwrap();
        let pr = spawnable_top(&p, &m, &c, x, z);
        println!(
            "  walk {i:2} ({x:4}, {z:4}): real top y {top} block #{blk}, model h {:.2} p(sand/stone) {pr:.2}",
            c.height[idx]
        );
    }
    println!(
        "sand/stone top vs world: [truth][pred>=0.5] {sconf:?}, expected {:.0} real {}",
        ssum.0, ssum.1
    );
    let total = n as f64;
    let ok = (conf[0][0] + conf[1][1]) as f64;
    println!(
        "predicate vs world: truth/pred grass {:?} -> accuracy {:.4}",
        conf,
        ok / total
    );
    for (b, c) in by_biome {
        println!("  {b:3} {:<22} {:?}", p.table.info(b).name, c);
    }
    assert!(ok / total > 0.99);
}

fn dump26() -> Option<serde_json::Value> {
    let f = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../testdata/pack/gtnh-2.9.0/dump26/spawn.json");
    serde_json::from_str(&std::fs::read_to_string(f).ok()?).ok()
}

/// Real world (rwg2dump 2.6, seed 1337 CONTINENTS): spawn (40, 64, -104) = walk step 26.
#[test]
fn seed_1337() {
    for wt in [WorldType::Rwg2Continents, WorldType::Rwg2] {
        let p = BiomeProvider::new(1337, wt, table(), RwgConfig::default());
        let t = std::time::Instant::now();
        let a = estimate_spawn(&p, &TableHeights, 1337);
        let dt = t.elapsed();
        let b = estimate_spawn(&p, &TableHeights, 1337);
        assert_eq!((a.x, a.z, a.attempts), (b.x, b.z, b.attempts));
        println!(
            "seed 1337 {wt:?}: spawn ({}, {}) attempt {} prob {:.3} confident {} evaluated {} in {:?}",
            a.x, a.z, a.attempts, a.prob, a.confident, a.evaluated, dt
        );
        for c in &a.candidates {
            println!(
                "   candidate ({}, {}) attempt {} prob {:.3}",
                c.x, c.z, c.attempt, c.prob
            );
        }
        if wt == WorldType::Rwg2Continents {
            if let Some(d) = dump26() {
                let s = &d["spawn"];
                assert_eq!(
                    (a.x, a.z),
                    (s[0].as_i64().unwrap() as i32, s[2].as_i64().unwrap() as i32)
                );
                assert_eq!(a.attempts, 26);
            }
        }
    }
}

/// Pre-population top of the 65 dumped walk steps (fresh `provideChunk`, BOP climb) vs the model.
#[test]
fn walk_pre_tops_vs_dump26() {
    let Some(d) = dump26() else {
        eprintln!("skip");
        return;
    };
    let p = BiomeProvider::new(
        1337,
        WorldType::Rwg2Continents,
        table(),
        RwgConfig::default(),
    );
    let m = SurfaceModel::new(&p.table);
    let mut g = ChunkGen::new(&p);
    let pos = walk_positions(1337);
    let (mut ok, mut n, mut ll) = (0, 0, 0f64);
    for s in d["walk"].as_array().unwrap() {
        let i = s["i"].as_u64().unwrap() as usize;
        let (x, z) = (
            s["x"].as_i64().unwrap() as i32,
            s["z"].as_i64().unwrap() as i32,
        );
        assert_eq!(pos[i], (x, z), "walk step {i}");
        let pre = s["pre"].as_str().unwrap();
        let c = g.generate(&p, &TableHeights, x >> 4, z >> 4).unwrap();
        let t = top_probs(&p, &m, &c, x, z);
        let truth = pre == "minecraft:stone" || pre == "minecraft:sand";
        let ps = t.spawnable();
        // classify: grass / stone+sand / other by the largest share
        let other = (1.0 - t.grass - ps).max(0.0);
        let cls = if t.grass >= ps && t.grass >= other {
            "grass"
        } else if ps >= other {
            "stone/sand"
        } else {
            "other"
        };
        let real = match pre {
            "minecraft:grass" => "grass",
            "minecraft:stone" | "minecraft:sand" => "stone/sand",
            _ => "other",
        };
        ok += (cls == real) as usize;
        n += 1;
        let pt = if truth { ps } else { 1.0 - ps };
        ll += (pt.max(1e-4) as f64).ln();
        if cls != real || truth || ps > 0.05 {
            println!(
                "  step {i:2} ({x:4}, {z:4}) pre {pre} y {} | model h {:.2} biome {} grass {:.2} stone {:.2} sand {:.2}",
                s["preY"], c.height[((z & 15) * 16 + (x & 15)) as usize], p.table.info(c.rwg[((z & 15) * 16 + (x & 15)) as usize]).name,
                t.grass, t.stone, t.sand
            );
        }
    }
    println!(
        "walk pre tops: class match {ok}/{n}, mean log-lik of sand/stone {:.3}",
        ll / n as f64
    );
    assert!(ok * 10 >= n * 9);
}

/// Cost per seed (fresh provider + model + terrain each, as in a seed search) and the spread of outcomes.
#[test]
fn many_seeds_cost() {
    let tab = table();
    for wt in [WorldType::Rwg2Continents, WorldType::Rwg2] {
        let n = 200;
        let (mut total, mut worst) = (std::time::Duration::ZERO, std::time::Duration::ZERO);
        let (mut origin, mut confident, mut evals, mut far) = (0, 0, 0usize, 0);
        for s in 0..n {
            let seed = (s as i64).wrapping_mul(0x9E3779B97F4A7C15u64 as i64);
            let t = std::time::Instant::now();
            let p = BiomeProvider::new(seed, wt, tab.clone(), RwgConfig::default());
            let e = estimate_spawn(&p, &TableHeights, seed);
            let dt = t.elapsed();
            total += dt;
            worst = worst.max(dt);
            origin += (e.attempts == 0) as usize;
            confident += e.confident as usize;
            evals += e.evaluated;
            far += (e.attempts >= 1000) as usize;
        }
        println!(
            "{wt:?}: {n} seeds, mean {:?}/seed, worst {worst:?}; spawn at origin {origin}, confident {confident}, cap {far}, mean steps {:.1}",
            total / n as u32,
            evals as f64 / n as f64
        );
    }
}

/// Search-preset drift check (ignored): seeds 1..=160, "Village ≤ 600 of spawn" with the exact (v1.49) and the
/// legacy fitted spawn model; prints the seeds whose spawn or verdict differ.
#[test]
#[ignore]
fn village_preset_spawn_drift() {
    use rwg2_biomes::config::RwgConfig;
    use rwg2_biomes::provider::{BiomeProvider, BiomeTable, TableHeights, WorldType};
    let dir = concat!(env!("CARGO_MANIFEST_DIR"), "/../../data/gtnh-2.9.0");
    let table = BiomeTable::from_rwg2dump_json(&std::fs::read_to_string(format!("{dir}/biomes.json")).unwrap()).unwrap();
    let (mut a, mut b) = (0, 0);
    for seed in 1..=160i64 {
        let p = BiomeProvider::new(seed, WorldType::Rwg2Continents, table.clone(), RwgConfig::default());
        let h = TableHeights;
        let m = rwg2_biomes::spawn::SurfaceModel::new(&p.table);
        let st = rwg2_biomes::surface::SurfaceTable::new(&p.table);
        let mut res = Vec::new();
        for exact in [true, false] {
            let mut t = rwg2_biomes::dungeons::Terrain::new(&p, &h);
            let s = rwg2_biomes::spawn::estimate_spawn_in_model(&mut t, &m, &st, seed, exact);
            let land = p.continent_value(s.x, s.z) >= 0.0;
            let v = land && rwg2_biomes::villages::nearest_village_within(&p, &h, seed, s.x, s.z, 600).is_some();
            res.push(((s.x, s.z), v));
        }
        a += res[0].1 as u32;
        b += res[1].1 as u32;
        if res[0] != res[1] {
            eprintln!("seed {seed}: exact {:?} legacy {:?}", res[0], res[1]);
        }
    }
    eprintln!("village ≤ 600 of spawn: exact model {a}/160, legacy model {b}/160");
}
