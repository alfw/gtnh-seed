//! First-load feature cell timings (ignored): `cargo test --release --test cell_speed -- --ignored --nocapture`.
use rwg2_biomes::config::RwgConfig;
use rwg2_biomes::provider::{BiomeProvider, BiomeTable, WorldType};

#[test]
#[ignore]
fn cell_timings() {
    let t = BiomeTable::from_rwg2dump_json(include_str!("../../../data/gtnh-2.9.0/biomes.json"))
        .unwrap();
    for seed in [1337i64, 92900795] {
        let p = BiomeProvider::new(
            seed,
            WorldType::Rwg2Continents,
            t.clone(),
            RwgConfig::default(),
        );
        for (name, flags) in [
            ("tc", 8u32),
            ("structures est", 1 | 2 | 64 | 128 | 4),
            ("ores", 16),
            ("fluids", 32),
        ] {
            for (cx, cz) in [(0, 0), (-1, 0), (1, 1)] {
                let mut out = Vec::new();
                let t0 = std::time::Instant::now();
                rwg2_wasm::features_into(
                    &p,
                    cx * 2048,
                    cz * 2048,
                    cx * 2048 + 2047,
                    cz * 2048 + 2047,
                    flags,
                    &mut out,
                );
                println!(
                    "seed {seed} {name:15} cell {cx},{cz}: {:7.1} ms  {} recs",
                    t0.elapsed().as_secs_f64() * 1e3,
                    out.len() / 6
                );
            }
        }
    }
}
