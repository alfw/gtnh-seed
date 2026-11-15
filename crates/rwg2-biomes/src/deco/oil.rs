//! DreamMaster `OilGeneratorFix` (NewHorizonsCoreMod, GTNH `dreamcraft.cfg` oilgen) and the `PopulateChunkEvent.Post`
//! listeners before it, on the chunk's shared populate Random (rwg2dump 2.16 / 2.18 traces). Every step is a pure
//! function of the Random state entering it, the world seed and the chunk biome array.

use crate::java_random::JavaRandom;

/// `0.0001D * OilSphereChance` (60, `dreamcraft.cfg`), capped at 0.2 in [`dm`].
const CHANCE: f64 = 0.0001 * 60.0;
const MIN_R: i32 = 8;
const MAX_R: i32 = 15;
const LARGE: i32 = 14;
const MEDIUM: i32 = 11;
const FOUNTAIN_SMALL: i32 = 5;
const FOUNTAIN_LARGE: i32 = 16;
const BOOST: f64 = 3.5;

/// An oil sphere (`buildOilStructure`): centre, radius 8..=15; blocks at `dx² + 3·dy² + dz² ≤ r²`,
/// `|dy| ≤ r − 2`, where the target and its 4 horizontal + lower neighbours are solid (not air, not a non-oil
/// liquid) and the block above is not sand / gravel. Medium (r ≥ 11): a 1-wide fountain from y + 1 to the ground
/// + 5; large (r ≥ 14): to the ground + 16, plus 4 side columns from y to the ground + 8.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct OilDeposit {
    pub x: i32,
    pub y: i32,
    pub z: i32,
    pub r: i32,
}

impl OilDeposit {
    /// 0 small, 1 medium, 2 large
    pub fn size(&self) -> u8 {
        if self.r >= LARGE {
            2
        } else if self.r >= MEDIUM {
            1
        } else {
            0
        }
    }
    /// fountain height above the ground (`getTopBlock`)
    pub fn fountain(&self) -> i32 {
        [0, FOUNTAIN_SMALL, FOUNTAIN_LARGE][self.size() as usize]
    }
    /// y range of the sphere in column (x, z), if it covers it (placement checks ignored)
    pub fn column(&self, x: i32, z: i32) -> Option<(i32, i32)> {
        let h = (x - self.x).pow(2) + (z - self.z).pow(2);
        let r2 = self.r * self.r;
        if h > r2 {
            return None;
        }
        // 3·dy² ≤ r² − h
        let mut dy = self.r - 2;
        while dy > 0 && 3 * dy * dy > r2 - h {
            dy -= 1;
        }
        Some((self.y - dy, self.y + dy))
    }
    /// number of sphere cells (placement checks ignored), ~ litres / 1000
    pub fn cells(&self) -> i32 {
        let r2 = self.r * self.r;
        let mut n = 0;
        for dx in -self.r..=self.r {
            for dy in -self.r + 2..=self.r - 2 {
                for dz in -self.r..=self.r {
                    if dx * dx + 3 * dy * dy + dz * dz <= r2 {
                        n += 1;
                    }
                }
            }
        }
        n
    }
}

/// `oilPresent` / `shouldSpawnOil` reseed of the shared Random at block (x, z).
pub fn reseed(r: &mut JavaRandom, seed: i64, x: i32, z: i32) {
    *r = JavaRandom::new(seed);
    let i1 = (r.next_int() as i64) / 2 * 2 + 1;
    let j1 = (r.next_int() as i64) / 2 * 2 + 1;
    let s = (x as i64).wrapping_mul(i1).wrapping_add((z as i64).wrapping_mul(j1)) ^ seed;
    *r = JavaRandom::new(s);
}

/// Galacticraft `EventHandlerGC.populate`: x, z jitter, then (biome at +8 not hell / sky) reseed + 2 `nextDouble`
/// (GTNH's oil factor never hits). Returns the jitter.
pub fn gc(r: &mut JavaRandom, seed: i64, gen_at: &mut dyn FnMut(i32, i32) -> u16, cx: i32, cz: i32) -> (i32, i32) {
    let jx = r.next_int_bounded(16);
    let jz = r.next_int_bounded(16);
    let (x, z) = (cx * 16 + jx, cz * 16 + jz);
    let g = gen_at(x + 8, z + 8);
    if g != 8 && g != 9 {
        reseed(r, seed, x, z);
        r.next_double();
        r.next_double();
    }
    (jx, jz)
}

/// Forestry `HiveDecorator`: shuffle of the 13 hives + one `nextFloat` each; `hit` = a hive passed its chance in a
/// good biome and all 4 placement tries failed (8 more draws).
pub fn hives(r: &mut JavaRandom, hit: bool) {
    for i in (2..=13).rev() {
        r.next_int_bounded(i);
    }
    for _ in 0..13 {
        r.next_float();
    }
    if hit {
        for _ in 0..8 {
            r.next_int_bounded(16);
        }
    }
}

/// Probability that no hive passed its chance (so the hive listener drew exactly 25), from the Random entering
/// it: the hive order is session state, so each of its 13 floats below a hive chance counts with the hit share
/// measured on dump 2.16 (2,672 chunks, 3 runs): f < 0.02: 0.26, < 0.03: 0.15, < 0.041: 0.03 (no hit above 0.041).
pub fn hive_no_hit(r: &JavaRandom) -> f32 {
    let mut r = r.clone();
    for i in (2..=13).rev() {
        r.next_int_bounded(i);
    }
    let mut p = 1.0f32;
    for _ in 0..13 {
        let f = r.next_float();
        p *= 1.0
            - if f < 0.02 {
                0.26
            } else if f < 0.03 {
                0.15
            } else if f < 0.041 {
                0.03
            } else {
                0.0
            };
    }
    p
}

/// Share of chunks whose decoration exit (Random entering ANIMALS) the replay gets exactly, by chunk biome (gen
/// id at +8, +8), measured on dump 2.16 with the real load order (3 runs, 192 chunks per biome); others 0.7.
/// BOP biomes that pick flower / grass generators with `Math.random()` are low (Garden, Fungi, Eucalyptus).
pub fn deco_exactness(gen: u16) -> f32 {
    match gen {
        51 => 0.96,  // Chaparral
        61 => 0.29,  // Eucalyptus Forest
        64 => 0.97,  // Frost Forest
        65 => 0.16,  // Fungi Forest
        66 => 0.0,   // Garden
        95 => 0.99,  // Prairie
        115 => 0.76, // Tundra
        129 => 0.78, // Sunflower Plains
        140 => 0.92, // Ice Plains Spikes
        322 | 234 => 0.79, // Jungle (RWG)
        324 | 236 => 0.87, // Temperate Forest
        5 | 19 | 30 | 31 | 133 => 0.8, // Taiga family
        _ => 0.7,
    }
}

/// Share of predicted deposits that are real, by the decorators that ran before the post chain (v1.66, 42 deposits
/// covering a test column in 7 worlds, `oil.rs::region_oil_vs_test_columns` `OIL_DIAG=1`): only RWG2 native biomes
/// in the populating chunk's border (non-native strength < 0.05) 10 of 12 real; any BOP decorator 15 of 30. The
/// earlier product (`deco_exactness` × hive no-hit) did not separate hits from misses (~0.7 for both).
pub fn chain_confidence(border: &[(u16, f32)], table: &crate::provider::BiomeTable) -> f32 {
    let bop: f32 = border
        .iter()
        .filter(|(id, _)| !table.info(*id as _).base_class.rsplit('.').next().unwrap_or("").starts_with("BaseBiome"))
        .map(|b| b.1)
        .sum();
    if bop < 0.05 {
        0.8
    } else {
        0.5
    }
}

/// DreamMaster `OilGeneratorFix.populate` on the Random entering it (chunk x, z both even, Java `%`; others: no
/// draws, None). `row(gen)` = (rootHeight, blacklisted, boosted) of a gen biome id.
pub fn dm(
    r: &mut JavaRandom,
    seed: i64,
    gen_at: &mut dyn FnMut(i32, i32) -> u16,
    row: &dyn Fn(u16) -> (f32, bool, bool),
    cx: i32,
    cz: i32,
) -> Option<OilDeposit> {
    if cx % 2 != 0 || cz % 2 != 0 {
        return None;
    }
    let x = cx * 16 + r.next_int_bounded(16);
    let z = cz * 16 + r.next_int_bounded(16);
    let (root, black, boost) = row(gen_at(x + 8, z + 8));
    if black {
        return None;
    }
    reseed(r, seed, x, z);
    let mut m = CHANCE.min(0.2);
    if root >= 0.45 {
        m /= 2.0;
    }
    if root < -0.5 {
        m *= 1.8;
    }
    if boost {
        m *= BOOST;
    }
    let f1 = r.next_double() <= m;
    let f2 = r.next_double() <= m;
    if !(f1 || f2) {
        return None;
    }
    let y = 17 + r.next_int_bounded(10) + r.next_int_bounded(5);
    let dx = x + r.next_int_bounded(16);
    let dz = z + r.next_int_bounded(16);
    let rad = r.next_int_bounded(MAX_R + 1 - MIN_R) + MIN_R;
    Some(OilDeposit { x: dx, y, z: dz, r: rad })
}

/// Every deposit chunk (cx, cz) can get, over the 256 Galacticraft jitters (the Random entering the Post listeners
/// is world-dependent, the GC reseed makes the rest a function of its jitter) and both hive outcomes:
/// `(gc jx, gc jz, hive hit, deposit)`.
pub fn variants(
    seed: i64,
    gen_at: &mut dyn FnMut(i32, i32) -> u16,
    row: &dyn Fn(u16) -> (f32, bool, bool),
    cx: i32,
    cz: i32,
) -> Vec<(u8, u8, bool, OilDeposit)> {
    let mut out = Vec::new();
    if cx % 2 != 0 || cz % 2 != 0 {
        return out;
    }
    for jx in 0..16 {
        for jz in 0..16 {
            let (x, z) = (cx * 16 + jx, cz * 16 + jz);
            let g = gen_at(x + 8, z + 8);
            if g == 8 || g == 9 {
                continue;
            }
            let mut base = JavaRandom::new(0);
            reseed(&mut base, seed, x, z);
            base.next_double();
            base.next_double();
            for hit in [false, true] {
                let mut r = base.clone();
                hives(&mut r, hit);
                if let Some(d) = dm(&mut r, seed, gen_at, row, cx, cz) {
                    out.push((jx as u8, jz as u8, hit, d));
                }
            }
        }
    }
    out
}
