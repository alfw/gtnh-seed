//! Draconic Evolution draconium ore in the End (`DraconicWorldGenerator.generateEnd`, 1.5.35-GTNH). Forge's
//! per-chunk Random (reseeded for every generator from `world.getSeed()`): comet position draws (x, y, z), the comet
//! roll when the chunk origin is > 200 blocks out, the chaos-island pass, then `oreWeightEnd` tries of
//! `WorldGenMinable(draconium, 4 + nextInt(1), end_stone)` at (x + 0..15, 1 + nextInt(69), z + 0..15), each followed by
//! a size-3 stone blob and a netherrack blob (draws only: neither block exists in the End). Chunks with a comet or
//! near a chaos island make world-dependent draws first; they are reported as inexact.

use crate::gtend::EndBlocks;
use crate::java_random::JavaRandom;
use crate::population::{floor_d, mcos, msin};

/// Config the End pass reads.
#[derive(Clone, Copy, Debug)]
pub struct DraconiumCfg {
    /// "End Draconium Ore Weight" (default 10)
    pub weight: i32,
    /// "Ender Comet Rarity" (default 10000)
    pub comet_rarity: i32,
    /// "Chaos Island Separation" (default 10000)
    pub chaos_separation: i32,
}

pub const DEFAULT_CFG: DraconiumCfg = DraconiumCfg {
    weight: 10,
    comet_rarity: 10000,
    chaos_separation: 10000,
};

/// One populate's draconium.
#[derive(Clone, Debug, Default)]
pub struct ChunkOre {
    /// predicted ore blocks (world x, y, z), may lie in the +x/+z neighbour chunks
    pub blocks: Vec<(i32, i32, i32)>,
    /// false: a comet or chaos island drew first, the blobs are not predictable
    pub exact: bool,
}

/// `WorldGenMinable.generate` cells for one blob (draws included), `place` gets every cell inside the ellipsoid.
pub fn minable_cells(
    r: &mut JavaRandom,
    x: i32,
    y: i32,
    z: i32,
    n: i32,
    mut place: impl FnMut(i32, i32, i32),
) {
    let f = r.next_float() * std::f32::consts::PI;
    let nf = n as f32;
    let d0 = ((x + 8) as f32 + msin(f) * nf / 8.0) as f64;
    let d1 = ((x + 8) as f32 - msin(f) * nf / 8.0) as f64;
    let d2 = ((z + 8) as f32 + mcos(f) * nf / 8.0) as f64;
    let d3 = ((z + 8) as f32 - mcos(f) * nf / 8.0) as f64;
    let d4 = (y + r.next_int_bounded(3) - 2) as f64;
    let d5 = (y + r.next_int_bounded(3) - 2) as f64;
    let nd = n as f64;
    for l in 0..=n {
        let ld = l as f64;
        let d6 = d0 + (d1 - d0) * ld / nd;
        let d7 = d4 + (d5 - d4) * ld / nd;
        let d8 = d2 + (d3 - d2) * ld / nd;
        let d9 = r.next_double() * nd / 16.0;
        let s = (msin(l as f32 * std::f32::consts::PI / nf) + 1.0) as f64;
        let d10 = s * d9 + 1.0;
        let d11 = s * d9 + 1.0;
        let (i1, j1, k1) = (
            floor_d(d6 - d10 / 2.0),
            floor_d(d7 - d11 / 2.0),
            floor_d(d8 - d10 / 2.0),
        );
        let (l1, i2, j2) = (
            floor_d(d6 + d10 / 2.0),
            floor_d(d7 + d11 / 2.0),
            floor_d(d8 + d10 / 2.0),
        );
        for k2 in i1..=l1 {
            let d12 = (k2 as f64 + 0.5 - d6) / (d10 / 2.0);
            if d12 * d12 >= 1.0 {
                continue;
            }
            for l2 in j1..=i2 {
                let d13 = (l2 as f64 + 0.5 - d7) / (d11 / 2.0);
                if d12 * d12 + d13 * d13 >= 1.0 {
                    continue;
                }
                for i3 in k1..=j2 {
                    let d14 = (i3 as f64 + 0.5 - d8) / (d10 / 2.0);
                    if d12 * d12 + d13 * d13 + d14 * d14 < 1.0 {
                        place(k2, l2, i3);
                    }
                }
            }
        }
    }
}

/// Is chunk (cx, cz) within `ChaosWorldGenHandler`'s 180-block copy range of a chaos island centre?
fn near_chaos(sep: i32, cx: i32, cz: i32) -> bool {
    if sep <= 0 {
        return false;
    }
    let (x, z) = (cx * 16, cz * 16);
    let nearest = |v: i32| ((v as f64 / sep as f64).round() as i64 * sep as i64) as i32;
    let (sx, sz) = (nearest(x), nearest(z));
    !(sx == 0 && sz == 0) && (x - sx).abs() <= 180 && (z - sz).abs() <= 180
}

/// Draconium the End populate of chunk (cx, cz) places. `terrain_seed` = `world.getSeed()` (seed + HEE resets).
pub fn chunk_ore(
    terrain_seed: i64,
    cfg: &DraconiumCfg,
    cx: i32,
    cz: i32,
    w: &mut EndBlocks,
) -> ChunkOre {
    let mut r = crate::dungeons::forge_chunk_random(terrain_seed, cx, cz);
    let (x, z) = (cx.wrapping_mul(16), cz.wrapping_mul(16));
    r.next_int_bounded(16);
    r.next_int_bounded(170);
    r.next_int_bounded(16);
    let d = (x.wrapping_mul(x).wrapping_add(z.wrapping_mul(z)) as f64).sqrt();
    let mut out = ChunkOre {
        blocks: Vec::new(),
        exact: true,
    };
    if d > 200.0 && r.next_int_bounded(cfg.comet_rarity.max(1)) == 0 {
        out.exact = false;
        return out;
    }
    if near_chaos(cfg.chaos_separation, cx, cz) {
        out.exact = false;
        return out;
    }
    for _ in 0..cfg.weight {
        let px = x + r.next_int_bounded(16);
        let py = 1 + r.next_int_bounded(69);
        let pz = z + r.next_int_bounded(16);
        let n = 4 + r.next_int_bounded(1);
        minable_cells(&mut r, px, py, pz, n, |a, b, c| {
            if w.stone(a, b, c) && !out.blocks.contains(&(a, b, c)) {
                out.blocks.push((a, b, c));
            }
        });
        minable_cells(&mut r, px, py, pz, 3, |_, _, _| {});
        let n = 4 + r.next_int_bounded(1);
        minable_cells(&mut r, px, py, pz, n, |_, _, _| {});
    }
    out
}

/// Draconium blocks in a chunk rectangle (populates of every chunk whose blobs can reach it).
pub fn ore_in(
    terrain_seed: i64,
    cfg: &DraconiumCfg,
    cx0: i32,
    cz0: i32,
    cx1: i32,
    cz1: i32,
    w: &mut EndBlocks,
) -> (Vec<(i32, i32, i32)>, usize) {
    let mut v = Vec::new();
    let mut inexact = 0;
    for cz in cz0 - 1..=cz1 {
        for cx in cx0 - 1..=cx1 {
            let o = chunk_ore(terrain_seed, cfg, cx, cz, w);
            if !o.exact {
                inexact += 1;
            }
            v.extend(o.blocks.into_iter().filter(|&(x, _, z)| {
                (cx0..=cx1).contains(&(x >> 4)) && (cz0..=cz1).contains(&(z >> 4))
            }));
        }
    }
    (v, inexact)
}
