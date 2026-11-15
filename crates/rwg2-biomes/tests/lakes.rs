#![cfg(feature = "json")]
//! `PopModel::lakes_in_rect` (RNG pre-check before terrain) = `lakes_at` on every chunk; lakes never reach the
//! surface in RWG2 (water from y < 50, lava from y < 52).
use rwg2_biomes::config::RwgConfig;
use rwg2_biomes::population::PopModel;
use rwg2_biomes::provider::{BiomeProvider, BiomeTable, TableHeights, WorldType};
use std::path::PathBuf;

fn root() -> PathBuf {
    std::env::var("RWG2_DATA_DIR")
        .map(|d| PathBuf::from(d).parent().unwrap().to_path_buf())
        .unwrap_or_else(|_| PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../.."))
}

#[test]
fn lakes_rect_matches_every_chunk() {
    let t = BiomeTable::from_rwg2dump_json(
        &std::fs::read_to_string(root().join("data/gtnh-2.9.0/biomes.json")).unwrap(),
    )
    .unwrap();
    let r = if cfg!(target_arch = "wasm32") { 6 } else { 14 };
    for seed in [1337i64, 92900795, -6999513761975938210] {
        let p = BiomeProvider::new(
            seed,
            WorldType::Rwg2Continents,
            t.clone(),
            RwgConfig::default(),
        );
        let mut a = PopModel::new(&p, &TableHeights, seed);
        let fast = a.lakes_in_rect(-r, -r, r - 1, r - 1);
        let mut b = PopModel::new(&p, &TableHeights, seed);
        b.load_structures(-r, -r, r - 1, r - 1);
        let mut all = Vec::new();
        for cz in -r..r {
            for cx in -r..r {
                all.extend(b.lakes_at(cx, cz));
            }
        }
        let lava = all.iter().filter(|l| l.lava).count();
        let open = all.iter().filter(|l| l.open).count();
        eprintln!(
            "seed {seed}: {} lakes ({lava} lava, {open} open to a cave), y max {:?}",
            all.len(),
            all.iter().map(|l| l.y1).max()
        );
        assert_eq!(fast, all, "seed {seed}");
        for l in &all {
            let top = a.t.top(l.x, l.z).unwrap_or(255);
            assert!(l.y1 < top, "lake at the surface: {l:?} top {top}");
        }
    }
}

#[test]
#[ignore]
fn lakes_rect_speed() {
    let t = BiomeTable::from_rwg2dump_json(
        &std::fs::read_to_string(root().join("data/gtnh-2.9.0/biomes.json")).unwrap(),
    )
    .unwrap();
    let p = BiomeProvider::new(1337, WorldType::Rwg2Continents, t, RwgConfig::default());
    let mut a = PopModel::new(&p, &TableHeights, 1337);
    let t0 = std::time::Instant::now();
    let v = a.lakes_in_rect(0, 0, 63, 63);
    eprintln!(
        "1024² cell: {} lakes in {:.2} s",
        v.len(),
        t0.elapsed().as_secs_f64()
    );
}
