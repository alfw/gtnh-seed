//! Vanilla 1.7.10 structures as placed under RWG2.
//!
//! Strongholds (`MapGenStronghold.canSpawnStructureAtCoords`): 3 strongholds, distance 32 chunks, spread 3:
//! ```text
//! Random r = new Random(worldSeed); double angle = r.nextDouble() * 2π; int l = 1;
//! for i in 0..3:
//!   double dist = (1.25 * l + r.nextDouble()) * 32 * l;
//!   cx = (int) Math.round(cos(angle) * dist); cz = (int) Math.round(sin(angle) * dist);
//!   pos = chunkManager.findBiomePosition(cx*16+8, cz*16+8, 112, biomes, r);   // RWG2: returns null, no draws
//!   angle += 2π * l / 3;
//! ```
//! RWG2's `findBiomePosition` returns null without touching `r`, so the ring is pure seed maths.
//! `Math.cos/sin` (HotSpot intrinsics) vs Rust's libm can differ by an ulp; that only matters if `cos*dist` lands
//! within ~1e-13 of a .5 boundary before rounding to a chunk, i.e. effectively never.

use crate::java_random::JavaRandom;
use crate::jmath::round_d;

/// Stronghold chunk coordinates (start chunk; the structure centre is around block `chunk*16 + 8`).
pub fn stronghold_ring(world_seed: i64) -> [(i32, i32); 3] {
    let mut r = JavaRandom::new(world_seed);
    let mut angle = r.next_double() * std::f64::consts::PI * 2.0;
    let l = 1i32;
    let spread = 3i32;
    let distance = 32.0f64;
    let mut out = [(0, 0); 3];
    for slot in out.iter_mut() {
        let d1 = (1.25 * l as f64 + r.next_double()) * distance * l as f64;
        let j = round_d(angle.cos() * d1) as i32;
        let k = round_d(angle.sin() * d1) as i32;
        *slot = (j, k);
        angle += std::f64::consts::PI * 2.0 * l as f64 / spread as f64;
    }
    out
}

/// Block position of a stronghold start chunk's centre.
pub fn chunk_center(c: (i32, i32)) -> (i32, i32) {
    (c.0 * 16 + 8, c.1 * 16 + 8)
}
