#![cfg(feature = "json")]
//! Decoration diagnostics for the vegetation-sensitive models (column dumps).
use rwg2_biomes::config::RwgConfig;
use rwg2_biomes::dungeons::*;
use rwg2_biomes::provider::{BiomeProvider, BiomeTable, TableHeights, WorldType};

fn root() -> std::path::PathBuf {
    std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..")
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

/// `VEG_COL=seed,x,z`: decoration + model classes of a column y 55..100.
#[test]
#[ignore]
fn veg_column() {
    let v: Vec<i64> = std::env::var("VEG_COL")
        .unwrap()
        .split(',')
        .map(|s| s.parse().unwrap())
        .collect();
    let table = BiomeTable::from_rwg2dump_json(
        &std::fs::read_to_string(root().join("data/gtnh-2.9.0/biomes.json")).unwrap(),
    )
    .unwrap();
    let p = BiomeProvider::new(v[0], WorldType::Rwg2Continents, table, RwgConfig::default());
    let mut m = RogueModel::new(&p, &TableHeights, v[0]);
    m.deco = Some(rwg2_biomes::deco::view::DecoOverlay::new(
        gates(),
        ROGUE_DECO_N,
    ));
    let (x, z) = (v[1] as i32, v[2] as i32);
    for dz in -1..=1 {
        for dx in -1..=1 {
            let (xx, zz) = (x + dx, z + dz);
            let mut s = String::new();
            for y in (55..100).rev() {
                if let Some(b) = m.deco_at(xx, y, zz) {
                    s += &format!(" {y}:{}", rwg2_biomes::deco::db::name(b));
                }
            }
            eprintln!(
                "({xx},{zz}) top {:?} pre-ground {:?} |{s}",
                m.t.top(xx, zz),
                (55..100)
                    .rev()
                    .find(|&y| m.block_pre(xx, y, zz) == RogueBlock::Ground)
            );
        }
    }
}

/// Cold cost of `RogueModel::dungeon` per anchor: probabilistic vs decoration statistics.
#[test]
#[ignore]
fn veg_rogue_cost() {
    use rwg2_biomes::forestry::ClimateTable;
    let table = BiomeTable::from_rwg2dump_json(
        &std::fs::read_to_string(root().join("data/gtnh-2.9.0/biomes.json")).unwrap(),
    )
    .unwrap();
    let climate = ClimateTable::from_json(
        &std::fs::read_to_string(root().join("data/gtnh-2.9.0/climate.json")).unwrap(),
    )
    .unwrap();
    let seed = 92_900_795i64;
    let p = BiomeProvider::new(seed, WorldType::Rwg2Continents, table, RwgConfig::default());
    let anchors = rogue_anchors(seed, &ROGUE_GTNH, -64, -64, 64, 64);
    for stats in [false, true] {
        let t0 = std::time::Instant::now();
        let mut worst = 0.0f64;
        let mut same = 0;
        for &(cx, cz) in &anchors {
            let t = std::time::Instant::now();
            let mut m = RogueModel::new(&p, &TableHeights, seed);
            if stats {
                m.deco = Some(rwg2_biomes::deco::view::DecoOverlay::new(
                    gates(),
                    ROGUE_DECO_N,
                ));
                m.deco_stats = true;
            }
            let d = m.dungeon(&climate, &ROGUE_GTNH, seed, cx, cz);
            same += d.pos.is_some() as usize;
            worst = worst.max(t.elapsed().as_secs_f64());
        }
        eprintln!(
            "stats {stats}: {} anchors, {:.0} ms mean, worst {:.0} ms, {same} with a spot",
            anchors.len(),
            t0.elapsed().as_secs_f64() * 1000.0 / anchors.len() as f64,
            worst * 1000.0
        );
    }
}
