use rwg2_biomes::chunkgen::ChunkGen;
use rwg2_biomes::config::RwgConfig;
use rwg2_biomes::provider::{BiomeProvider, BiomeTable, TableHeights, WorldType};

/// Zoomed-out terrain (smoothed `getNoiseAt`) vs exact chunk heights.
#[test]
fn smoothed_heights_close_to_chunks() {
    let t = BiomeTable::from_rwg2dump_json(include_str!("../../../data/gtnh-2.9.0/biomes.json"))
        .unwrap();
    let p = BiomeProvider::new(1337, WorldType::Rwg2Continents, t, RwgConfig::default());
    let mut g = ChunkGen::new(&p);
    for (x0, z0) in [(-2600, 1000), (0, 0), (1500, -2000)] {
        let (w, step) = (64u32, 16);
        let mut exact = vec![0f32; (w * w) as usize];
        for j in 0..w as i32 {
            for i in 0..w as i32 {
                let (x, z) = (x0 + i * step, z0 + j * step);
                let c = g.generate(&p, &TableHeights, x >> 4, z >> 4).unwrap();
                exact[(j as u32 * w + i as u32) as usize] =
                    c.height[((z & 15) * 16 + (x & 15)) as usize];
            }
        }
        for r in [0, rwg2_wasm::BLEND_RADIUS] {
            let v = rwg2_wasm::smoothed_heights(&p, x0, z0, w, w, step, r);
            let (mut se, mut sea, mut n) = (0f64, 0usize, 0usize);
            for (a, b) in v.iter().zip(&exact) {
                se += ((a - b) as f64).powi(2);
                n += 1;
                // wrong side of sea level
                sea += ((*a < 62.0) != (*b < 62.0)) as usize;
            }
            let (rms, flips) = ((se / n as f64).sqrt(), 100.0 * sea as f64 / n as f64);
            eprintln!("({x0},{z0}) r={r}: rms {rms:.2}, sea-level flips {flips:.2}%");
            if r > 0 {
                // zoomed-out terrain stays within a few blocks of the real chunk heights
                assert!(
                    rms < 6.0 && flips < 7.0,
                    "({x0},{z0}): rms {rms:.2}, flips {flips:.2}%"
                );
            }
        }
    }
}
