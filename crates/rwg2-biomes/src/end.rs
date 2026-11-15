//! The End in GTNH: Hardcore Ender Expansion 1.12.29-GTNH (`chylex.hee.world.ChunkProviderHardcoreEnd`).
//!
//! - Base terrain = vanilla 1.7.10 `ChunkProviderEnd` (HEE only swaps stone → end stone): the main island.
//! - HEE biome islands (`MapGenIsland`) and dungeon towers (`MapGenTower`) are `MapGenScatteredFeature` subclasses
//!   placed during population; their placement and the island's biome / content variation / deviations are pure
//!   functions of the world seed, the chunk and HEE's dragon death counter (`DragonSavefile`), see [`islands`].
//!   That counter only rises when the Temple Caller ritual deletes DIM1 (an End reset; a plain dragon kill leaves it
//!   unchanged: rwg2dump 2.12.0 before/after a kill: 0 and 0, identical layouts). `WorldProviderHardcoreEnd.getSeed()`
//!   returns world seed + counter and Forge's `World.getSeed()` goes through the provider, so the terrain generator
//!   ([`EndTerrain::new`]) gets `seed + resets` while island/tower placement uses `WorldInfo.getSeed()` (raw seed)
//!   with `nextInt(1 + resets)` (`kills` parameters below = resets). That reroll is a no-op in practice: any bound
//!   costs one `next(31)`, so islands and towers never move (dump: next-kill list == current, 131/131).
//! - HEE's decorator (blobs, energy clusters, endium, meteoroids, end powder) runs on the provider's `endRNG`
//!   without reseeding it, so it depends on the chunk load order: not predictable.
//!
//! HEE is "All Rights Reserved" (studying the source allowed); this is a reimplementation of the algorithms.

use crate::dimterrain::Octaves;
use crate::java_random::JavaRandom;

/// Vanilla `ChunkProviderEnd` density terrain.
pub struct EndTerrain {
    o1: Octaves,
    o2: Octaves,
    o3: Octaves,
}

/// Pre-population solidity of one chunk: bit y of `cols[lz * 16 + lx]` (y 0..127).
pub struct EndChunk {
    pub cols: [u128; 256],
}

impl EndChunk {
    #[inline]
    pub fn solid(&self, lx: i32, y: i32, lz: i32) -> bool {
        (0..128).contains(&y) && self.cols[(lz * 16 + lx) as usize] >> y & 1 != 0
    }
    /// Highest solid y of a column, -1 if void.
    pub fn top(&self, lx: i32, lz: i32) -> i32 {
        let c = self.cols[(lz * 16 + lx) as usize];
        if c == 0 {
            -1
        } else {
            127 - c.leading_zeros() as i32
        }
    }
}

impl EndTerrain {
    pub fn new(seed: i64) -> Self {
        let mut r = JavaRandom::new(seed);
        let o1 = Octaves::new(&mut r, 16);
        let o2 = Octaves::new(&mut r, 16);
        let o3 = Octaves::new(&mut r, 8);
        // noiseGen4 (10) / noiseGen5 (16) are built (draws) but their output never reaches the density
        let _o4 = Octaves::new(&mut r, 10);
        let _o5 = Octaves::new(&mut r, 16);
        EndTerrain { o1, o2, o3 }
    }

    /// `initializeNoiseField(x·2, 0, z·2, 3, 33, 3)`.
    fn density(&self, x: i32, z: i32) -> Vec<f64> {
        let (sx, sy, sz) = (3, 33, 3);
        let d0 = 684.412 * 2.0;
        let d1 = 684.412;
        let n1 = self
            .o3
            .noise(x, 0, z, sx, sy, sz, d0 / 80.0, d1 / 160.0, d0 / 80.0);
        let n2 = self.o1.noise(x, 0, z, sx, sy, sz, d0, d1, d0);
        let n3 = self.o2.noise(x, 0, z, sx, sy, sz, d0, d1, d0);
        let mut out = vec![0.0; (sx * sy * sz) as usize];
        let mut k = 0;
        for i in 0..sx {
            for j in 0..sz {
                let f = (i + x) as f32;
                let f1 = (j + z) as f32;
                let f2 = (100.0f32 - ((f * f + f1 * f1) as f64).sqrt() as f32 * 8.0f32)
                    .clamp(-100.0, 80.0);
                for y in 0..sy {
                    let d7 = n2[k] / 512.0;
                    let d8 = n3[k] / 512.0;
                    let d9 = (n1[k] / 10.0 + 1.0) / 2.0;
                    let mut d5 = if d9 < 0.0 {
                        d7
                    } else if d9 > 1.0 {
                        d8
                    } else {
                        d7 + (d8 - d7) * d9
                    };
                    d5 -= 8.0;
                    d5 += f2 as f64;
                    if y > sy / 2 - 2 {
                        let t = (((y - (sy / 2 - 2)) as f32 / 64.0f32) as f64).clamp(0.0, 1.0);
                        d5 = d5 * (1.0 - t) + -3000.0 * t;
                    }
                    if y < 8 {
                        let t = ((8 - y) as f32 / (8.0f32 - 1.0f32)) as f64;
                        d5 = d5 * (1.0 - t) + -30.0 * t;
                    }
                    out[k] = d5;
                    k += 1;
                }
            }
        }
        out
    }

    /// `func_147420_a`: blocks where the interpolated density > 0.
    pub fn chunk(&self, cx: i32, cz: i32) -> EndChunk {
        EndChunk {
            cols: interpolate(&self.density(cx * 2, cz * 2)),
        }
    }
}

/// Vanilla End trilinear fill of a 3×33×3 density field (`func_147420_a`; HEE's island `TerrainGenerator` is the
/// same loop): bit y of `cols[lz * 16 + lx]` set where density > 0.
fn interpolate(d: &[f64]) -> [u128; 256] {
    let (l, b1) = (3usize, 33usize);
    let at = |i: usize, j: usize, k: usize| d[(i * l + j) * b1 + k];
    let mut cols = [0u128; 256];
    for i1 in 0..2 {
        for j1 in 0..2 {
            for k1 in 0..32 {
                let mut d1 = at(i1, j1, k1);
                let mut d2 = at(i1, j1 + 1, k1);
                let mut d3 = at(i1 + 1, j1, k1);
                let mut d4 = at(i1 + 1, j1 + 1, k1);
                let d5 = (at(i1, j1, k1 + 1) - d1) * 0.25;
                let d6 = (at(i1, j1 + 1, k1 + 1) - d2) * 0.25;
                let d7 = (at(i1 + 1, j1, k1 + 1) - d3) * 0.25;
                let d8 = (at(i1 + 1, j1 + 1, k1 + 1) - d4) * 0.25;
                for l1 in 0..4 {
                    let mut d10 = d1;
                    let mut d11 = d2;
                    let d12 = (d3 - d1) * 0.125;
                    let d13 = (d4 - d2) * 0.125;
                    for i2 in 0..8 {
                        let x = i2 + i1 * 8;
                        let y = k1 * 4 + l1;
                        let mut d15 = d10;
                        let d16 = (d11 - d10) * 0.125;
                        for k2 in 0..8 {
                            let z = k2 + j1 * 8;
                            if d15 > 0.0 {
                                cols[z * 16 + x] |= 1u128 << y;
                            }
                            d15 += d16;
                        }
                        d10 += d12;
                        d11 += d13;
                    }
                    d1 += d5;
                    d2 += d6;
                    d3 += d7;
                    d4 += d8;
                }
            }
        }
    }
    cols
}

// ============================================================================ HEE islands + towers

/// HEE island biomes (`IslandBiomeBase.biomeList` order = `biomeID`).
/// The main island lies within ±`MAIN_ISLAND_CHUNKS` chunks of the origin (checked on 8 seeds; the wasm uses 24).
pub const MAIN_ISLAND_CHUNKS: i32 = 24;

pub const ISLAND_BIOMES: [&str; 3] = ["Infested Forest", "Burning Mountains", "Enchanted Island"];
/// Content variations per biome: (id, name, weight) in `contentVariations` order (Infested Forest's RUINS (4, 3)
/// is declared but never added).
pub const ISLAND_VARIATIONS: [&[(u8, &str, i32)]; 3] = [
    &[(0, "Deep", 8), (3, "Ravaged", 6)],
    &[(1, "Scorching", 8), (5, "Mine", 6)],
    &[(2, "Homeland", 6), (6, "Laboratory", 4)],
];
/// Random deviations per biome: (identifier, compatible variation ids) in `randomDeviations` order.
pub const ISLAND_DEVIATIONS: [&[(&str, &[u8])]; 3] = [
    &[("TallTrees", &[0, 3]), ("ThornyBushes", &[0])],
    &[
        ("ExcessiveCinder", &[1]),
        ("SingleLava", &[1]),
        ("LimitedOres", &[5]),
        ("DeepResPits", &[5]),
    ],
    &[
        ("TallPiles", &[2]),
        ("GooSwamp", &[2]),
        ("ShadowOrchids", &[2, 6]),
    ],
];

/// A HEE biome island: structure start chunk (box = chunk·16 .. +207, centre +104).
#[derive(Clone, Debug, PartialEq)]
pub struct Island {
    pub cx: i32,
    pub cz: i32,
    pub biome: u8,
    /// content variation id (see [`ISLAND_VARIATIONS`])
    pub variation: u8,
    /// selected deviations (indices into the biome's [`ISLAND_DEVIATIONS`]), in selection order
    pub deviations: Vec<u8>,
    /// `islandBottomY` = 6 + nextInt(25)
    pub bottom_y: i32,
}

#[inline]
fn lin2incr(d: f64, s: f64, e: f64) -> f64 {
    if d < s {
        0.0
    } else if d > e {
        1.0
    } else {
        (d - s) / (e - s)
    }
}
#[inline]
fn cub2decr(d: f64, s: f64, e: f64) -> f64 {
    if d < s || d > e {
        0.0
    } else {
        1.0 - ((d - s) / (e - s)).powf(3.0)
    }
}
#[inline]
fn chance(c: f64, r: &mut JavaRandom) -> bool {
    if c == 0.0 {
        false
    } else if c == 1.0 {
        true
    } else {
        r.next_double() < c
    }
}

/// `MapGenScatteredFeatureCustom.canSpawnStructureAtCoords` up to the per-type test: the Random after the
/// position draws and the float distance (`MathUtil.square` has no long overload → float), or `None`.
fn scattered(
    seed: i64,
    kills: i32,
    cx: i32,
    cz: i32,
    min_sp: i32,
    max_sp: i32,
    size: i32,
) -> Option<(JavaRandom, f64)> {
    let (mut x, mut z) = (cx, cz);
    if x < 0 {
        x -= max_sp - 1;
    }
    if z < 0 {
        z -= max_sp - 1;
    }
    let (x2, z2) = (x / max_sp, z / max_sp);
    let mut r = JavaRandom::new(
        (x2 as i64)
            .wrapping_mul(341_873_128_712)
            .wrapping_add((z2 as i64).wrapping_mul(132_897_987_541))
            .wrapping_add(seed)
            .wrapping_add(358_041),
    );
    r.next_int_bounded(1 + kills);
    let x2 = x2 * max_sp + r.next_int_bounded(max_sp - min_sp);
    let z2 = z2 * max_sp + r.next_int_bounded(max_sp - min_sp);
    if x2 != cx || z2 != cz {
        return None;
    }
    let fx = (x2 as i64 * 16 + (size >> 1) as i64) as f32;
    let fz = (z2 as i64 * 16 + (size >> 1) as i64) as f32;
    Some((r, ((fx * fx + fz * fz) as f64).sqrt()))
}

/// The island cell (28×28 chunks) containing chunk `(cx, cz)`'s candidate chunk.
fn island_candidate(seed: i64, kills: i32, gx: i32, gz: i32) -> (i32, i32) {
    let mut r = JavaRandom::new(
        (gx as i64)
            .wrapping_mul(341_873_128_712)
            .wrapping_add((gz as i64).wrapping_mul(132_897_987_541))
            .wrapping_add(seed)
            .wrapping_add(358_041),
    );
    r.next_int_bounded(1 + kills);
    (
        gx * 28 + r.next_int_bounded(15),
        gz * 28 + r.next_int_bounded(15),
    )
}

/// `MapGenIsland` start at chunk `(cx, cz)` (= `IslandSpawnChecker.getIslandBiomeAt != -1`).
pub fn island_at(seed: i64, kills: i32, cx: i32, cz: i32) -> bool {
    let Some((mut r, dist)) = scattered(seed, kills, cx, cz, 13, 28, 208) else {
        return false;
    };
    dist >= 1600.0
        && r.next_int_bounded(7) <= 4
        && chance(0.65 + 0.35 * lin2incr(dist, 1600.0, 4000.0), &mut r)
}

/// `ComponentIsland.setupStructure`'s `consistentRand` part: bottom Y, biome, `generateData`.
pub fn island_data(seed: i64, cx: i32, cz: i32) -> Island {
    island_rng(seed, cx, cz).0
}

/// [`island_data`] and `consistentRand` after it (next: the island's `TerrainGenerator` octaves).
fn island_rng(seed: i64, cx: i32, cz: i32) -> (Island, JavaRandom) {
    let (x, z) = (cx * 16, cz * 16);
    let mut r =
        JavaRandom::new(((x / 9) as i64 * 238_504 + (z / 9) as i64 * 10_058_432_215) ^ seed);
    let bottom_y = 6 + r.next_int_bounded(25);
    let biome = r.next_int_bounded(3) as usize;
    let vars = ISLAND_VARIATIONS[biome];
    let total: i32 = vars.iter().map(|v| v.2).sum();
    let mut i = r.next_int_bounded(total);
    let mut variation = vars[0].0;
    for v in vars {
        i -= v.2;
        if i < 0 {
            variation = v.0;
            break;
        }
    }
    let mut available: Vec<u8> = ISLAND_DEVIATIONS[biome]
        .iter()
        .enumerate()
        .filter(|(_, d)| d.1.contains(&variation))
        .map(|(k, _)| k as u8)
        .collect();
    let mut deviations = Vec::new();
    if !available.is_empty() {
        let n = available.len() as f64;
        let g = r.next_gaussian();
        let amt = (((g + 0.4) * n * (0.5 + 0.5 * r.next_double())).floor() as i32)
            .clamp(0, available.len() as i32);
        for _ in 0..amt {
            let k = r.next_int_bounded(available.len() as i32) as usize;
            deviations.push(available.remove(k));
        }
    }
    (
        Island {
            cx,
            cz,
            biome: biome as u8,
            variation,
            deviations,
            bottom_y,
        },
        r,
    )
}

/// All islands whose start chunk lies in the chunk rectangle (one candidate per 28×28-chunk cell).
pub fn islands(seed: i64, kills: i32, cx0: i32, cz0: i32, cx1: i32, cz1: i32) -> Vec<Island> {
    let cell = |c: i32| c.div_euclid(28);
    let mut out = Vec::new();
    for gz in cell(cz0)..=cell(cz1) {
        for gx in cell(cx0)..=cell(cx1) {
            let (x, z) = island_candidate(seed, kills, gx, gz);
            if (cx0..=cx1).contains(&x) && (cz0..=cz1).contains(&z) && island_at(seed, kills, x, z)
            {
                out.push(island_data(seed, x, z));
            }
        }
    }
    out
}

/// `MapGenTower` start at chunk `(cx, cz)`: spacing 9..15, ≥ 350 from the centre, 29% (fading past 900), not
/// within 125 blocks (centre to centre) of an island centre.
pub fn tower_at(seed: i64, kills: i32, cx: i32, cz: i32) -> bool {
    let Some((mut r, dist)) = scattered(seed, kills, cx, cz, 9, 15, 32) else {
        return false;
    };
    if dist < 350.0 || r.next_int_bounded(100) > 28 {
        return false;
    }
    if dist > 900.0 && !chance(0.25 + 0.75 * cub2decr(dist, 900.0, 3800.0), &mut r) {
        return false;
    }
    let (tx, tz) = (cx * 16 + 16, cz * 16 + 16);
    let check = (208.0f32 * 0.6f32).ceil() as i32; // 125
    for xx in cx - 12..=cx + 12 {
        for zz in cz - 12..=cz + 12 {
            if island_at(seed, kills, xx, zz) {
                let (dx, dz) = ((tx - (xx * 16 + 104)) as f64, (tz - (zz * 16 + 104)) as f64);
                if (dx * dx + dz * dz).sqrt() < check as f64 {
                    return false;
                }
            }
        }
    }
    true
}

/// The `MapGenTower` placement Random after `tower_at`'s draws (spacing, offsets, 29% roll, distance chance), for
/// the draws that follow it on the same Random; `None` if the chunk is no tower start (island rule not applied).
pub fn tower_scatter_random(seed: i64, kills: i32, cx: i32, cz: i32) -> Option<JavaRandom> {
    let (mut r, dist) = scattered(seed, kills, cx, cz, 9, 15, 32)?;
    if dist < 350.0 || r.next_int_bounded(100) > 28 {
        return None;
    }
    if dist > 900.0 && !chance(0.25 + 0.75 * cub2decr(dist, 900.0, 3800.0), &mut r) {
        return None;
    }
    Some(r)
}

/// Tower start chunks in a chunk rectangle (one candidate per 15×15-chunk cell).
pub fn towers(seed: i64, kills: i32, cx0: i32, cz0: i32, cx1: i32, cz1: i32) -> Vec<(i32, i32)> {
    let cell = |c: i32| c.div_euclid(15);
    let mut out = Vec::new();
    for gz in cell(cz0)..=cell(cz1) {
        for gx in cell(cx0)..=cell(cx1) {
            let mut r = JavaRandom::new(
                (gx as i64)
                    .wrapping_mul(341_873_128_712)
                    .wrapping_add((gz as i64).wrapping_mul(132_897_987_541))
                    .wrapping_add(seed)
                    .wrapping_add(358_041),
            );
            r.next_int_bounded(1 + kills);
            let (x, z) = (
                gx * 15 + r.next_int_bounded(6),
                gz * 15 + r.next_int_bounded(6),
            );
            if (cx0..=cx1).contains(&x) && (cz0..=cz1).contains(&z) && tower_at(seed, kills, x, z) {
                out.push((x, z));
            }
        }
    }
    out
}

/// Island terrain (HEE `ComponentIsland.setupStructure` terrain pass): top Y per column of the 208×208 area starting
/// at block (`x0`, `z0`) = start chunk·16 + 8; -1 = void. Caves, ores and biome decoration (trees, piles, goo …)
/// that run afterwards are not included.
#[derive(Clone, Debug)]
pub struct IslandShape {
    pub island: Island,
    pub x0: i32,
    pub z0: i32,
    pub top: Vec<i16>,
    /// lowest solid Y per column after the caves (-1 = void)
    pub bottom: Vec<i16>,
}

impl IslandShape {
    /// Lowest solid Y at world (x, z), -1 outside or void.
    pub fn bottom_at(&self, x: i32, z: i32) -> i32 {
        let (i, j) = (x - self.x0, z - self.z0);
        if !(0..208).contains(&i) || !(0..208).contains(&j) {
            return -1;
        }
        self.bottom[(j * 208 + i) as usize] as i32
    }

    /// Top Y at world (x, z), -1 outside or void.
    pub fn top_at(&self, x: i32, z: i32) -> i32 {
        let (i, j) = (x - self.x0, z - self.z0);
        if !(0..208).contains(&i) || !(0..208).contains(&j) {
            return -1;
        }
        self.top[(j * 208 + i) as usize] as i32
    }
}

/// (mass height, surface height, fill) multipliers of `IslandBiomeBase` subclasses for this island's variation.
fn island_mults(i: &Island) -> (f32, f32, f32) {
    match i.biome {
        // Infested Forest: RAVAGED = 3, RUINS = 4 (never chosen)
        0 => (
            if i.variation == 3 { 1.15 } else { 0.95 },
            if i.variation == 4 { 0.9 } else { 0.7 },
            if i.variation == 3 { 1.08 } else { 1.04 },
        ),
        // Burning Mountains: SCORCHING = 1
        1 => (
            if i.variation == 1 { 0.9 } else { 1.0 },
            if i.variation == 1 { 6.5 } else { 7.5 },
            1.0,
        ),
        // Enchanted Island
        _ => (0.8, 1.0, 0.92),
    }
}

/// [`IslandShape`] of the island whose structure starts at chunk (`cx`, `cz`) (call only for real starts). ~0.15 s.
pub fn island_shape(seed: i64, cx: i32, cz: i32) -> IslandShape {
    island_shape_full(seed, cx, cz, None)
}

/// [`island_shape`] plus the terrain-pass solid block count per column (index j·208 + i), for validation.
pub fn island_solids(seed: i64, cx: i32, cz: i32) -> (IslandShape, Vec<u16>) {
    let (s, cols) = island_cols(seed, cx, cz);
    let n = cols
        .iter()
        .map(|c| (c.0[0].count_ones() + c.0[1].count_ones()) as u16)
        .collect();
    (s, n)
}

/// [`island_shape`] plus per column (index j·208 + i) the solid and the end-stone masks after terrain + caves:
/// bit y of `[lo, hi]` (`c[y >> 7] >> (y & 127)`), world Y. End stone = solid minus the pre-cave surface
/// (`end_stone_terrain`); HEE ores, energy clusters and decoration are not modelled.
pub fn island_cols(seed: i64, cx: i32, cz: i32) -> (IslandShape, Vec<IslandCol>) {
    let mut n = vec![([0u128; 2], [0u128; 2]); 208 * 208];
    let s = island_shape_full(seed, cx, cz, Some(&mut n));
    (s, n)
}

/// (solid, end stone) world-Y masks of one island column, see [`island_cols`].
pub type IslandCol = ([u128; 2], [u128; 2]);

/// Compact island columns: structure-space (solid, end stone) masks per column (index j·208 + i); world Y =
/// structure bit + `base`. 32 bytes per column instead of 64 (`EndBlocks` keeps many of these).
pub struct IslandCompact {
    pub base: i32,
    pub cols: Vec<(u128, u128)>,
}

impl IslandCompact {
    /// World-Y `[lo, hi]` masks of column k (same as [`island_cols`]).
    #[inline]
    pub fn world(&self, k: usize) -> IslandCol {
        let (s, st) = self.cols[k];
        (shift_up(s, self.base), shift_up(st, self.base))
    }
}

/// `c << base` as a 256-bit [lo, hi] pair, bits past 255 dropped (0 ≤ base < 128).
#[inline]
fn shift_up(c: u128, base: i32) -> [u128; 2] {
    if c == 0 {
        return [0, 0];
    }
    let b = base as u32;
    if b == 0 {
        [c, 0]
    } else {
        [c << b, c >> (128 - b)]
    }
}

/// [`IslandShape`] (top / bottom per column) from compact columns, without regenerating the island.
pub fn shape_from_compact(seed: i64, cx: i32, cz: i32, c: &IslandCompact) -> IslandShape {
    let mut top = vec![-1i16; 208 * 208];
    let mut bottom = vec![-1i16; 208 * 208];
    for (k, &(s, _)) in c.cols.iter().enumerate() {
        if s != 0 {
            top[k] = (c.base + 127 - s.leading_zeros() as i32) as i16;
            bottom[k] = (c.base + s.trailing_zeros() as i32) as i16;
        }
    }
    IslandShape {
        x0: cx * 16 + 8,
        z0: cz * 16 + 8,
        island: island_data(seed, cx, cz),
        top,
        bottom,
    }
}

/// [`island_cols`] in the compact form.
pub fn island_cols_compact(seed: i64, cx: i32, cz: i32) -> IslandCompact {
    let mut n = vec![(0u128, 0u128); 208 * 208];
    let s = island_shape_full2(seed, cx, cz, None, Some(&mut n));
    IslandCompact {
        base: 20 + s.island.bottom_y,
        cols: n,
    }
}

/// HEE `ComponentIsland.setupStructure` up to and including the caves: (island, `consistentRand` after the caves,
/// structure-space solid columns (bit = structure y, index j·208 + i), pre-cave surface (`end_stone_terrain`) bits).
pub(crate) fn island_terrain(seed: i64, cx: i32, cz: i32) -> (Island, JavaRandom, Vec<u128>, Vec<u128>) {
    let (island, mut r) = island_rng(seed, cx, cz);
    let o1 = Octaves::new(&mut r, 16);
    let o2 = Octaves::new(&mut r, 16);
    let o3 = Octaves::new(&mut r, 8);
    let _o4 = Octaves::new(&mut r, 10); // built by TerrainGenerator, unused; its draws precede the caves
    let (mass, surf, fill) = island_mults(&island);
    let y_mp = 0.66f32 * mass;
    // structure-space columns (bit = structure y; world y = 20 + bottomY + y)
    let mut sm = vec![0u128; 208 * 208];
    // block yy → structure y bit, ⌊yy·(0.66f·mass)⌋ in f32 like HEE
    let ymap: [u128; 128] = std::array::from_fn(|b| 1u128 << ((b as f32 * y_mp) as i32));
    let (sx, sy, sz) = (3, 33, 3);
    let (scale_xz, scale_y) = (1368.824 * 2.0, 684.412 * 2.0);
    for chunk_x in 0..13 {
        for chunk_z in 0..13 {
            let (x, z) = ((chunk_x - 6) * 2, (chunk_z - 6) * 2);
            let n1 = o3.noise(
                x,
                0,
                z,
                sx,
                sy,
                sz,
                scale_xz / 80.0,
                scale_y / 160.0,
                scale_xz / 80.0,
            );
            let n2 = o1.noise(x, 0, z, sx, sy, sz, scale_xz, scale_y, scale_xz);
            let n3 = o2.noise(x, 0, z, sx, sy, sz, scale_xz, scale_y, scale_xz);
            let mut d = vec![0.0; (sx * sy * sz) as usize];
            let mut k = 0;
            for xx in 0..sx {
                for zz in 0..sz {
                    let dx = (xx + x) as f32 / 0.58f32;
                    let dz = (zz + z) as f32 / 0.58f32;
                    let f2 = (100f32 * fill - ((dx * dx + dz * dz) as f64).sqrt() as f32 * 8f32)
                        .clamp(-100.0, 80.0);
                    for yy in 0..sy {
                        let lo = n2[k] / 512.0;
                        let hi = n3[k] / 512.0;
                        let t = (n1[k] / 10.0 + 1.0) / 2.0;
                        let mut v = if t < 0.0 {
                            lo
                        } else if t > 1.0 {
                            hi
                        } else {
                            lo + (hi - lo) * t
                        };
                        v -= 8.0;
                        v += f2 as f64;
                        if yy > sy / 2 - 2 {
                            let t = (((yy - (sy / 2 - 2)) as f32 / (64f32 * surf)) as f64)
                                .clamp(0.0, 1.0);
                            v = v * (1.0 - t) - 3000.0 * t;
                        }
                        if yy < 8 {
                            let t = ((8 - yy) as f32 / (8f32 - 1f32)) as f64;
                            v = v * (1.0 - t) - 30.0 * t;
                        }
                        d[k] = v;
                        k += 1;
                    }
                }
            }
            let cols = interpolate(&d);
            for lz in 0..16 {
                for lx in 0..16 {
                    let c = cols[lz * 16 + lx];
                    if c == 0 {
                        continue;
                    }
                    let mut ys = 0u128;
                    let mut m = c;
                    while m != 0 {
                        let b = m.trailing_zeros() as usize;
                        m &= m - 1;
                        ys |= ymap[b];
                    }
                    sm[(chunk_z as usize * 16 + lz) * 208 + chunk_x as usize * 16 + lx] = ys;
                }
            }
        }
    }
    // top block: end stone with air above, structure y ≥ 5 (before the caves)
    let surface: Vec<u128> = sm
        .iter()
        .map(|&c| c & !(c >> 1) & !((1u128 << 5) - 1))
        .collect();
    island_caves(&mut r, &island, &mut sm);
    (island, r, sm, surface)
}

fn island_shape_full(
    seed: i64,
    cx: i32,
    cz: i32,
    solids: Option<&mut Vec<IslandCol>>,
) -> IslandShape {
    island_shape_full2(seed, cx, cz, solids, None)
}

fn island_shape_full2(
    seed: i64,
    cx: i32,
    cz: i32,
    mut solids: Option<&mut Vec<IslandCol>>,
    mut compact: Option<&mut Vec<(u128, u128)>>,
) -> IslandShape {
    let (island, _r, sm, surface) = island_terrain(seed, cx, cz);
    let base = 20 + island.bottom_y;
    let mut top = vec![-1i16; 208 * 208];
    let mut bottom = vec![-1i16; 208 * 208];
    for (k, &c) in sm.iter().enumerate() {
        if c == 0 {
            continue;
        }
        top[k] = (base + 127 - c.leading_zeros() as i32) as i16;
        bottom[k] = (base + c.trailing_zeros() as i32) as i16;
        if let Some(n) = solids.as_deref_mut() {
            n[k] = (shift_cols(c, base), shift_cols(c & !surface[k], base));
        }
        if let Some(n) = compact.as_deref_mut() {
            n[k] = (c, c & !surface[k]);
        }
    }
    IslandShape {
        x0: cx * 16 + 8,
        z0: cz * 16 + 8,
        island,
        top,
        bottom,
    }
}

/// Structure-space column mask → world-Y `[lo, hi]` halves (world y = structure y + `base`).
fn shift_cols(c: u128, base: i32) -> [u128; 2] {
    let mut out = [0u128; 2];
    let mut m = c;
    while m != 0 {
        let b = m.trailing_zeros() as i32;
        m &= m - 1;
        let wy = base + b;
        if (0..256).contains(&wy) {
            out[(wy >> 7) as usize] |= 1u128 << (wy & 127);
        }
    }
    out
}

/// HEE `CaveGenerator` (centre (104, 28, 104), radii (96, 24, 96)) on the island's `consistentRand` after the
/// terrain octaves: carves air into the structure columns. Energy clusters only consume their draws.
fn island_caves(r: &mut JavaRandom, island: &Island, sm: &mut [u128]) {
    let (amount_mul, branching) = match (island.biome, island.variation) {
        (0, 3) => (0.0f32, 0.026f32), // Infested Ravaged
        (0, _) => (1.15, 0.026),
        (1, 5) => (2.0, 0.05), // Burning Mine
        (1, _) => (1.6, 0.035),
        _ => (0.45, 0.005),
    };
    let mut g = Caves { r, air: Vec::new() };
    let (cx, cy, cz, rx, ry) = (104i32, 28i32, 104i32, 96i32, 24i32);
    let amount = (amount_mul * (8f32 + g.r.next_float() * 5.5f32)) as i32;
    for _ in 0..amount {
        let a = (g.r.next_double() * 2.0 * std::f64::consts::PI).sin();
        let x = cx + (a * rx as f64 * (g.r.next_double() * 0.1 + 0.9)) as i32;
        let b = (g.r.next_double() * 2.0 * std::f64::consts::PI).cos();
        let z = cz + (b * ry as f64 * (g.r.next_double() * 0.1 + 0.9)) as i32;
        let y = cy + g.r.next_int_bounded(ry * 2) - ry;
        let rad = g.r.next_float() * 0.45f32 + 2.65f32;
        let vx = if g.r.next_boolean() { cx - x } else { x - cx } as f64;
        let vy = (cy - y) as f64 * g.r.next_double() * 4.0;
        let vz = if g.r.next_boolean() { cz - z } else { z - cz } as f64;
        let v = normalize(vx, vy, vz);
        g.node(x as f32, y as f32, z as f32, rad, branching, v, 0);
    }
    for (x, y, z) in g.air {
        sm[(z * 208 + x) as usize] &= !(1u128 << y);
    }
}

/// MC 1.7.10 `Vec3.normalize` (length through `MathHelper.sqrt_double` = float sqrt).
fn normalize(x: f64, y: f64, z: f64) -> (f64, f64, f64) {
    let d = (x * x + y * y + z * z).sqrt() as f32 as f64;
    if d < 1.0e-4 {
        (0.0, 0.0, 0.0)
    } else {
        (x / d, y / d, z / d)
    }
}

struct Caves<'a> {
    r: &'a mut JavaRandom,
    air: Vec<(i32, i32, i32)>,
}

impl Caves<'_> {
    const C: (i32, i32, i32) = (104, 28, 104);
    const R: (i32, i32, i32) = (96, 24, 96);

    #[allow(clippy::too_many_arguments)]
    fn node(
        &mut self,
        mut x: f32,
        mut y: f32,
        mut z: f32,
        rad: f32,
        branching: f32,
        mut v: (f64, f64, f64),
        it: u32,
    ) {
        if it == 3 {
            return;
        }
        let mut cycle = 0;
        loop {
            cycle += 1;
            if cycle >= 150 {
                break;
            }
            v.0 += 0.65f32 as f64 * (self.r.next_double() - 0.5);
            v.1 += 0.4f32 as f64 * (self.r.next_double() - 0.5);
            v.2 += 0.65f32 as f64 * (self.r.next_double() - 0.5);
            x = (x as f64 + v.0) as f32;
            y = (y as f64 + v.1) as f32;
            z = (z as f64 + v.2) as f32;
            if !self.blob(
                (x as f64).floor() as i32,
                (y as f64).floor() as i32,
                (z as f64).floor() as i32,
                rad,
            ) {
                break;
            }
            if self.r.next_float() < branching {
                let nr = rad - self.r.next_float() * 0.15f32;
                let nv = (
                    v.0 + 0.8f32 as f64 * (self.r.next_double() - 0.5),
                    v.1 + 0.4f32 as f64 * (self.r.next_double() - 0.5),
                    v.2 + 0.8f32 as f64 * (self.r.next_double() - 0.5),
                );
                self.node(x, y, z, nr, branching * 0.75f32, nv, it + 1);
            }
        }
    }

    fn blob(&mut self, x: i32, y: i32, z: i32, rad: f32) -> bool {
        let (cx, cy, cz) = Self::C;
        let (rx, ry, rz) = Self::R;
        let mut rad = rad + self.r.next_float() * 0.2f32;
        if self.r.next_int_bounded(30) == 0 {
            rad *= 1.5f32;
        }
        let mut placed = false;
        let ir = (rad as f64).ceil() as i32;
        for xx in x - ir..=x + ir {
            for yy in y - ir..=y + ir {
                for zz in z - ir..=z + ir {
                    if xx <= cx - rx
                        || xx >= cx + rx
                        || yy <= cy - ry
                        || yy >= cy + ry
                        || zz <= cz - rz
                        || zz >= cz + rz
                    {
                        continue;
                    }
                    let d = dist(xx - x, yy - y, zz - z);
                    if d < (rad + self.r.next_float() * 0.15f32) as f64 {
                        placed = true;
                        self.air.push((xx, yy, zz));
                    }
                }
            }
        }
        if self.r.next_int_bounded(120) == 0 {
            let xx = x + self.r.next_int_bounded(ir) - self.r.next_int_bounded(ir);
            let yy = y + self.r.next_int_bounded(ir) - self.r.next_int_bounded(ir);
            let zz = z + self.r.next_int_bounded(ir) - self.r.next_int_bounded(ir);
            if dist(xx - x, yy - y, zz - z) >= (rad * 0.5f32) as f64 {
                let _ = self.r.next_int_bounded(8);
            }
        }
        placed
    }
}

/// `CaveGenerator.getDistance` (table for |d| ≤ 3 is the same sqrt).
fn dist(x: i32, y: i32, z: i32) -> f64 {
    ((x * x + y * y + z * z) as f64).sqrt()
}

/// The island (if any) whose terrain area covers chunk (`cx`, `cz`): starts lie 0..13 chunks west/north of it.
pub fn island_covering(seed: i64, cx: i32, cz: i32) -> Option<Island> {
    islands(seed, 0, cx - 13, cz - 13, cx, cz)
        .into_iter()
        .find(|i| {
            let (x0, z0) = (i.cx * 16 + 8, i.cz * 16 + 8);
            let (bx, bz) = (cx * 16, cz * 16);
            bx + 15 >= x0 && bx <= x0 + 207 && bz + 15 >= z0 && bz <= z0 + 207
        })
}

// ============================================================================ Draconic Evolution (1.5.35-GTNH)

/// A Draconic Evolution Ender comet (`WorldGenEnderComet`): core sphere of radius `size` at (x, y, z) (draconium ore
/// 10%, obsidian 30%, end stone), trail of thinning shells toward the tail.
#[derive(Clone, Debug, PartialEq)]
pub struct Comet {
    pub x: i32,
    pub y: i32,
    pub z: i32,
    pub size: i32,
    pub tail: (i32, i32, i32),
}

/// `DraconicWorldGenerator.generateEnd` comets in a chunk rectangle. `terrain_seed` = `world.getSeed()` (End: world
/// seed + HEE resets; Forge's per-chunk Random is reseeded for every generator). `rarity` = config "Ender Comet
/// Rarity" (default 10000). Chunk origin > 200 blocks from 0,0 (int maths, wraps like Java).
pub fn comets(
    terrain_seed: i64,
    rarity: i32,
    cx0: i32,
    cz0: i32,
    cx1: i32,
    cz1: i32,
) -> Vec<Comet> {
    let mut out = Vec::new();
    for cz in cz0..=cz1 {
        for cx in cx0..=cx1 {
            let (x, z) = (cx.wrapping_mul(16), cz.wrapping_mul(16));
            let d2 = x.wrapping_mul(x).wrapping_add(z.wrapping_mul(z));
            // Java `Math.sqrt(x * x + z * z) > 200` on wrapping ints (NaN for an overflowed negative sum → false)
            let d = (d2 as f64).sqrt();
            if d.is_nan() || d <= 200.0 {
                continue;
            }
            let mut r = crate::dungeons::forge_chunk_random(terrain_seed, cx, cz);
            let x1 = x + r.next_int_bounded(16);
            let y = 20 + r.next_int_bounded(170);
            let z1 = z + r.next_int_bounded(16);
            if r.next_int_bounded(rarity.max(1)) != 0 {
                continue;
            }
            let rot = r.next_int() as f64;
            let (xm, zm) = (rot.sin(), rot.cos());
            let dist = 150 + r.next_int_bounded(50);
            let tail = (
                x1 + (xm * dist as f64) as i32,
                y + 40 + r.next_int_bounded(40),
                z1 + (zm * dist as f64) as i32,
            );
            let size = 2 + r.next_int_bounded(8);
            out.push(Comet {
                x: x1,
                y,
                z: z1,
                size,
                tail,
            });
        }
    }
    out
}

/// Draconic Evolution chaos island centres in a block rectangle: every multiple of `separation` (config "Chaos
/// Island Separation", default 10000) on both axes except 0, 0. Seed-independent.
pub fn chaos_islands(separation: i32, x0: i32, z0: i32, x1: i32, z1: i32) -> Vec<(i32, i32)> {
    let s = separation.max(1);
    let mut out = Vec::new();
    for gz in z0.div_euclid(s) - 1..=z1.div_euclid(s) + 1 {
        let z = gz * s;
        if z < z0 || z > z1 {
            continue;
        }
        for gx in x0.div_euclid(s) - 1..=x1.div_euclid(s) + 1 {
            let x = gx * s;
            if x < x0 || x > x1 || (x == 0 && z == 0) {
                continue;
            }
            out.push((x, z));
        }
    }
    out
}
