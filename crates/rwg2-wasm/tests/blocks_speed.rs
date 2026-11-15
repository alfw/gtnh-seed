//! Blocks map mode (render mode 3) tile timings (ignored):
//! `cargo test --release --test blocks_speed -- --ignored --nocapture`.
use rwg2_biomes::chunkgen::ChunkGen;
use rwg2_biomes::config::RwgConfig;
use rwg2_biomes::provider::{BiomeProvider, BiomeTable, TableHeights, WorldType};

#[test]
#[ignore]
fn blocks_tile_timings() {
    let t = BiomeTable::from_rwg2dump_json(include_str!("../../../data/gtnh-2.9.0/biomes.json"))
        .unwrap();
    for seed in [1337i64, 92900795] {
        let p = BiomeProvider::new(
            seed,
            WorldType::Rwg2Continents,
            t.clone(),
            RwgConfig::default(),
        );
        for (tx, tz) in [(0, 0), (1, 0), (-3, 2)] {
            let mut g = ChunkGen::new(&p);
            let mut out = Vec::new();
            let t0 = std::time::Instant::now();
            rwg2_wasm::render_mode_into(&p, &mut g, tx * 256, tz * 256, 256, 256, 1, 3, &mut out);
            let full = t0.elapsed().as_secs_f64() * 1e3;
            // parts: chunk generation, carve + caves, surface, snow
            let caves = rwg2_biomes::gtorder::Caves::new(seed, true);
            let m = rwg2_biomes::spawn::SurfaceModel::new(&p.table);
            let st = rwg2_biomes::surface::SurfaceTable::new(&p.table);
            let mut g = ChunkGen::new(&p);
            let (mut tg, mut tc, mut ts, mut tn) = (0.0, 0.0, 0.0, 0.0);
            for cz in tz * 16..tz * 16 + 16 {
                for cx in tx * 16..tx * 16 + 16 {
                    let a = std::time::Instant::now();
                    let c = g.generate(&p, &TableHeights, cx, cz).unwrap();
                    let b = std::time::Instant::now();
                    let cc = rwg2_biomes::spawn::carved_chunk(
                        &p,
                        &TableHeights,
                        &m,
                        &c,
                        cx,
                        cz,
                        Some(&caves),
                    );
                    let d = std::time::Instant::now();
                    let mut sc = rwg2_biomes::surface::surface_chunk(
                        &p,
                        &TableHeights,
                        &st,
                        &c,
                        cx,
                        cz,
                        &cc,
                    );
                    let e = std::time::Instant::now();
                    rwg2_biomes::surface::apply_snow_cover(
                        &p,
                        &TableHeights,
                        &|_| Some(0.5),
                        &c,
                        &mut sc,
                        cx,
                        cz,
                    );
                    let f = std::time::Instant::now();
                    tg += (b - a).as_secs_f64();
                    tc += (d - b).as_secs_f64();
                    ts += (e - d).as_secs_f64();
                    tn += (f - e).as_secs_f64();
                }
            }
            println!(
                "seed {seed} tile {tx},{tz}: render {full:6.0} ms | parts: gen {:5.0} carve {:5.0} surface {:5.0} snow(uncached) {:5.0} ms",
                tg * 1e3, tc * 1e3, ts * 1e3, tn * 1e3
            );
        }
    }
}

#[test]
#[ignore]
fn carve_parts() {
    let t = BiomeTable::from_rwg2dump_json(include_str!("../../../data/gtnh-2.9.0/biomes.json"))
        .unwrap();
    let seed = 1337i64;
    let p = BiomeProvider::new(seed, WorldType::Rwg2Continents, t, RwgConfig::default());
    let caves = rwg2_biomes::gtorder::Caves::new(seed, true);
    let m = rwg2_biomes::spawn::SurfaceModel::new(&p.table);
    let mut g = ChunkGen::new(&p);
    let chunks: Vec<_> = (0..16)
        .flat_map(|cz| (16..32).map(move |cx| (cx, cz)))
        .collect();
    let cs: Vec<_> = chunks
        .iter()
        .map(|&(cx, cz)| g.generate(&p, &TableHeights, cx, cz).unwrap())
        .collect();
    let tm = |name: &str, f: &mut dyn FnMut(usize)| {
        let a = std::time::Instant::now();
        for i in 0..cs.len() {
            f(i);
        }
        println!(
            "{name:14} {:6.1} ms / tile",
            a.elapsed().as_secs_f64() * 1e3
        );
    };
    tm("hard_layers", &mut |i| {
        std::hint::black_box(rwg2_biomes::spawn::hard_layers(
            &p,
            &TableHeights,
            &m,
            &cs[i],
            chunks[i].0,
            chunks[i].1,
        ));
    });
    tm("chunk_tops", &mut |i| {
        std::hint::black_box(rwg2_biomes::spawn::chunk_tops(
            &p,
            &m,
            &cs[i],
            chunks[i].0,
            chunks[i].1,
        ));
    });
    tm("snow_layers", &mut |i| {
        std::hint::black_box(rwg2_biomes::spawn::snow_layers(
            &p,
            &m,
            &cs[i],
            chunks[i].0,
            chunks[i].1,
        ));
    });
    tm("CarvedChunk", &mut |i| {
        std::hint::black_box(rwg2_biomes::column::CarvedChunk::new(
            &p,
            &TableHeights,
            &cs[i],
            chunks[i].0,
            chunks[i].1,
            Some(&caves),
        ));
    });
    tm("no caves", &mut |i| {
        std::hint::black_box(rwg2_biomes::column::CarvedChunk::new(
            &p,
            &TableHeights,
            &cs[i],
            chunks[i].0,
            chunks[i].1,
            None,
        ));
    });
    tm("biome_gen_id", &mut |i| {
        for k in 0..256 {
            std::hint::black_box(p.biome_gen_id(
                chunks[i].0 * 16 + (k & 15),
                chunks[i].1 * 16 + (k >> 4),
                &TableHeights,
            ));
        }
    });
}

#[test]
#[ignore]
fn one_tile() {
    let t = BiomeTable::from_rwg2dump_json(include_str!("../../../data/gtnh-2.9.0/biomes.json"))
        .unwrap();
    let p = BiomeProvider::new(1337, WorldType::Rwg2Continents, t, RwgConfig::default());
    let mut g = ChunkGen::new(&p);
    let mut out = Vec::new();
    rwg2_wasm::render_mode_into(&p, &mut g, 256, 0, 256, 256, 1, 3, &mut out);
}

#[test]
#[ignore]
fn gen_tile() {
    let t = BiomeTable::from_rwg2dump_json(include_str!("../../../data/gtnh-2.9.0/biomes.json"))
        .unwrap();
    let p = BiomeProvider::new(1337, WorldType::Rwg2Continents, t, RwgConfig::default());
    let mut g = ChunkGen::new(&p);
    let t0 = std::time::Instant::now();
    let mut n = 0;
    for cz in 0..16 {
        for cx in 16..32 {
            n += g
                .generate(&p, &TableHeights, cx, cz)
                .map_or(0, |c| c.height[0] as i32);
        }
    }
    println!("gen tile {:.1} ms ({n})", t0.elapsed().as_secs_f64() * 1e3);
}
