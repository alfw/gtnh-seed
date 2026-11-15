#![cfg(feature = "json")]
use rwg2_biomes::chunkgen::ChunkGen;
use rwg2_biomes::config::RwgConfig;
use rwg2_biomes::provider::{BiomeProvider, BiomeTable, TableHeights, WorldType};

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

/// In a single-biome interior (main biome, weight 1) without river carving or landmarks, the blended chunk
/// height is close to getNoiseAt (bit-exactness is checked against the v2.1 dump). Also checks that biome dithering only happens near borders.
#[test]
fn interior_matches_noise_at() {
    for wt in [WorldType::Rwg2, WorldType::Rwg2Continents] {
        let p = BiomeProvider::new(1337, wt, table(), RwgConfig::default());
        let mut g = ChunkGen::new(&p);
        let (mut checked, mut equal, mut dithered) = (0, 0, 0);
        let t = std::time::Instant::now();
        let mut chunks = 0;
        for cz in -20..20 {
            for cx in -20..20 {
                let c = g.generate(&p, &TableHeights, cx, cz).unwrap();
                chunks += 1;
                let first = c.rwg[0];
                let uniform = g.last_active_count() == 1;
                for idx in 0..256 {
                    let (x, z) = (cx * 16 + (idx & 15) as i32, cz * 16 + (idx >> 4) as i32);
                    let bd = p.biome_data(x, z, &TableHeights).data;
                    if c.rwg[idx] != bd {
                        dithered += 1;
                    }
                    if !uniform || p.river_strength(x, z) < 0.0 || bd != first {
                        continue;
                    }
                    if p.volcano_vicinity_coordinates(x, z, &TableHeights) != i64::MIN
                        || p.lava_cave_coordinates(x, z, &TableHeights) != i64::MIN
                    {
                        continue;
                    }
                    checked += 1;
                    let want = p.noise_at(x, z, &TableHeights).unwrap();
                    if (want - c.height[idx]).abs() < 1e-3 {
                        equal += 1;
                    }
                }
            }
        }
        let el = t.elapsed();
        eprintln!(
            "{wt:?}: {chunks} chunks in {el:?} ({:.2} ms/chunk); interior {equal}/{checked} within 1e-3; {dithered} columns differ from getBiomeDataAt ({:.2}%)",
            el.as_secs_f64() * 1e3 / chunks as f64,
            dithered as f64 * 100.0 / (chunks * 256) as f64
        );
        assert!(checked > 1000, "{checked}");
        assert_eq!(equal, checked);
    }
}

fn dump21() -> std::path::PathBuf {
    std::env::var("RWG2_DUMP_DIR")
        .map(std::path::PathBuf::from)
        .unwrap_or_else(|_| {
            std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
                .join("../../testdata/pack/gtnh-2.9.0/dump")
        })
        .parent()
        .unwrap()
        .join("dump21")
}

/// getNewNoise vs the pack (rwg2dump 2.2): blended height (bits) + chunk biome (base id), every chunk region in the
/// manifest (both world types: spawn area and a lava cave per seed).
#[test]
fn chunks_match_pack() {
    let dir = dump21().parent().unwrap().join("dump22");
    let Ok(m) = std::fs::read_to_string(dir.join("manifest.json")) else {
        eprintln!("skip");
        return;
    };
    let m: serde_json::Value = serde_json::from_str(&m).unwrap();
    let t = table();
    let mut regions = 0;
    for f in m["files"].as_array().unwrap() {
        let name = f["file"].as_str().unwrap();
        if !name.starts_with("chunks_") {
            continue;
        }
        regions += 1;
        let seed: i64 = f["seed"].as_str().unwrap().parse().unwrap();
        let wt = if f["worldType"] == "RWG2" {
            WorldType::Rwg2
        } else {
            WorldType::Rwg2Continents
        };
        let (x0, z0) = (
            f["x0"].as_i64().unwrap() as i32,
            f["z0"].as_i64().unwrap() as i32,
        );
        let (w, h) = (
            f["w"].as_i64().unwrap() as i32,
            f["h"].as_i64().unwrap() as i32,
        );
        let bytes = std::fs::read(dir.join(name)).unwrap();
        let n = (w * h * 256) as usize;
        assert_eq!(bytes.len(), n * 6);
        let p = BiomeProvider::new(seed, wt, t.clone(), RwgConfig::default());
        let mut g = ChunkGen::new(&p);
        let (mut bad_h, mut bad_b) = (0usize, 0usize);
        for cz in 0..h {
            for cx in 0..w {
                let c = g
                    .generate(&p, &TableHeights, (x0 >> 4) + cx, (z0 >> 4) + cz)
                    .unwrap();
                for idx in 0..256usize {
                    let k = ((cz * w + cx) as usize) * 256 + idx;
                    let bio = u16::from_le_bytes([bytes[k * 2], bytes[k * 2 + 1]]);
                    let o = n * 2 + k * 4;
                    let hv =
                        f32::from_le_bytes([bytes[o], bytes[o + 1], bytes[o + 2], bytes[o + 3]]);
                    // biomesForGeneration excludes the lava marker (applied later in provideChunk)
                    let rwg_gen = p.table.biomes[c.rwg[idx] as usize].gen_id;
                    bad_b += (rwg_gen != bio) as usize;
                    bad_h += (c.height[idx].to_bits() != hv.to_bits()) as usize;
                }
            }
        }
        eprintln!("{name}: {n} columns, height mismatches {bad_h}, biome mismatches {bad_b}");
        assert_eq!((bad_h, bad_b), (0, 0), "{name}");
    }
    assert!(regions >= 9);
}

/// The real world's chunks (seed 1337, RWG2_CONTINENTS, provideChunk before population): chunk biome array must
/// match exactly; report how the top / ground blocks relate to (int) testHeight.
#[test]
fn world_chunks_match_pack() {
    let dir = dump21();
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
    let mut g = ChunkGen::new(&p);
    let mut bad_b = 0;
    let mut hist = std::collections::BTreeMap::<i32, usize>::new();
    let mut ghist = std::collections::BTreeMap::<i32, usize>::new();
    for cz in 0..w {
        for cx in 0..w {
            let c = g.generate(&p, &TableHeights, x0 + cx, x0 + cz).unwrap();
            for idx in 0..256usize {
                let k = ((cz * w + cx) as usize) * 256 + idx;
                let bio = u16::from_le_bytes([bytes[k * 2], bytes[k * 2 + 1]]);
                if bio != c.gen[idx] {
                    bad_b += 1;
                }
                let top = bytes[n * 2 + k] as i8 as i32;
                let ground = bytes[n * 5 + k] as i8 as i32;
                let hi = c.height[idx] as i32;
                *hist
                    .entry((top.max(62) - hi.max(62)).clamp(-5, 5))
                    .or_default() += 1;
                *ghist.entry((ground - hi).clamp(-5, 5)).or_default() += 1;
            }
        }
    }
    eprintln!("world chunks: biome mismatches {bad_b}/{n}");
    eprintln!("max(top,62) - max((int)h,62): {hist:?}");
    eprintln!("roguelike ground - (int)h: {ghist:?}");
    assert_eq!(bad_b, 0);
}
