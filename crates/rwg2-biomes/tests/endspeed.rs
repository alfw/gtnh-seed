use rwg2_biomes::gtend::*;
use rwg2_biomes::gtores::{is_oreseed, GT_GTNH};
use std::time::Instant;

/// Timing of one viewer End ore cell (2048² blocks) over an HEE island (run with --ignored --nocapture).
#[test]
#[ignore]
fn end_ore_cell_speed() {
    let seed: i64 = -1053966461076653616;
    let table = rwg2_biomes::nether::vein_table(&GT_END_VEINS);
    let (cx0, cz0) = (64, 0);
    let (cx1, cz1) = (cx0 + 127, cz0 + 127);
    let t0 = Instant::now();
    let zones = OreZones::new(seed, cx0 - 2, cz0 - 2, cx1 + 2, cz1 + 2);
    let tz = t0.elapsed();
    let mut w = EndBlocks::new(seed, 0);
    let (mut n, mut v) = (0, 0);
    let t1 = Instant::now();
    for cz in cz0..=cz1 {
        for cx in cx0..=cx1 {
            if !is_oreseed(&GT_GTNH, cx, cz) {
                continue;
            }
            let Some((px, pz)) = processing_chunk_end_with(cx, cz, |x, z| zones.processes(x, z))
            else {
                continue;
            };
            n += 1;
            if resolve_end_at(seed, &table, &GT_GTNH, cx, cz, px, pz, &mut w).is_some() {
                v += 1;
            }
            for (qx, qz) in [
                (cx - 2, cz - 2),
                (cx + 2, cz - 2),
                (cx - 2, cz + 2),
                (cx + 2, cz + 2),
            ] {
                if (qx, qz) != (px, pz) && zones.processes(qx, qz) {
                    resolve_end_at(seed, &table, &GT_GTNH, cx, cz, qx, qz, &mut w);
                }
            }
        }
    }
    eprintln!(
        "zones {:?}, {} processed oreseeds ({} veins) {:?}",
        tz,
        n,
        v,
        t1.elapsed()
    );
    let t2 = Instant::now();
    let cfg = rwg2_biomes::draconium::DEFAULT_CFG;
    let mut k = 0;
    for cz in cz0..cz0 + 32 {
        for cx in cx0..cx0 + 32 {
            k += rwg2_biomes::draconium::chunk_ore(seed, &cfg, cx, cz, &mut w)
                .blocks
                .len();
        }
    }
    eprintln!("draconium 32x32 chunks: {k} blocks {:?}", t2.elapsed());
}

#[test]
#[ignore]
fn end_blocks_parts() {
    let seed: i64 = -1053966461076653616;
    let (cx0, cz0) = (64, 0);
    let t = Instant::now();
    let mut n = 0;
    for cz in cz0..cz0 + 128 {
        for cx in cx0..cx0 + 128 {
            n += rwg2_biomes::end::island_covering(seed, cx, cz).is_some() as i32;
        }
    }
    eprintln!("island_covering x16384: {:?} ({n})", t.elapsed());
    let t = Instant::now();
    let mut n = 0;
    for cz in cz0..cz0 + 128 {
        for cx in cx0..cx0 + 128 {
            n += rwg2_biomes::end::towers(seed, 0, cx - 2, cz - 2, cx + 1, cz + 1).len();
        }
    }
    eprintln!("towers x16384: {:?} ({n})", t.elapsed());
    let t = Instant::now();
    let mut w = EndBlocks::new(seed, 0);
    for cz in cz0..cz0 + 128 {
        for cx in cx0..cx0 + 128 {
            w.solid_range(cx * 16, cz * 16);
        }
    }
    eprintln!("EndBlocks all chunks: {:?}", t.elapsed());
}

#[test]
#[ignore]
fn island_shape_speed() {
    let seed: i64 = -1053966461076653616;
    let isl = rwg2_biomes::end::islands(seed, 0, 64, 0, 191, 127);
    let t = Instant::now();
    for i in isl.iter().take(8) {
        std::hint::black_box(rwg2_biomes::end::island_cols_compact(seed, i.cx, i.cz));
    }
    eprintln!("island_cols_compact: {:?} per island", t.elapsed() / 8);
}

/// The viewer derives island land from the compact columns: must equal `island_shape`.
#[test]
fn shape_from_compact_matches() {
    let seed: i64 = -1053966461076653616;
    for i in rwg2_biomes::end::islands(seed, 0, 64, 0, 191, 127)
        .iter()
        .take(3)
    {
        let a = rwg2_biomes::end::island_shape(seed, i.cx, i.cz);
        let b = rwg2_biomes::end::shape_from_compact(
            seed,
            i.cx,
            i.cz,
            &rwg2_biomes::end::island_cols_compact(seed, i.cx, i.cz),
        );
        assert_eq!(a.top, b.top);
        assert_eq!(a.bottom, b.bottom);
        assert_eq!(
            (a.x0, a.z0, a.island.biome, a.island.bottom_y),
            (b.x0, b.z0, b.island.biome, b.island.bottom_y)
        );
        let (_, cols) = rwg2_biomes::end::island_cols(seed, i.cx, i.cz);
        let c = rwg2_biomes::end::island_cols_compact(seed, i.cx, i.cz);
        for k in (0..208 * 208).step_by(7) {
            assert_eq!(cols[k], c.world(k), "col {k}");
        }
    }
}
