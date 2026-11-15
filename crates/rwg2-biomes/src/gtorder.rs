//! Chunk population order (Minecraft 1.7.10 + Forge) and the GT ore-vein processing chunk it implies.
//!
//! GT decides an oreseed's vein in the *first* chunk whose population task reaches it (`WorldGenContainer.run`
//! scans oreseeds within ±`MAX_VEIN_SIZE` = 2 chunks). Tasks run FIFO in population order (`PENDING_TASKS`, at
//! least one per `generate` call), so the first processed chunk is the first populated chunk within ±2.
//!
//! Population (`Chunk.populateChunk`, run when a chunk is loaded): the new chunk `(x, z)` populates itself if
//! `(x+1, z)`, `(x, z+1)`, `(x+1, z+1)` are loaded, then `(x-1, z)`, `(x, z-1)`, `(x-1, z-1)` if their own +x/+z
//! neighbours are loaded.
//!
//! Consequences used by [`processing_chunk`]:
//! - a square loaded x-major (spawn area, `MinecraftServer.initialWorldChunkLoad`: x outer, z inner over
//!   spawn ±192 blocks in 16-block steps) or z-major (a force-load loop) populates its chunks `[a, b-1]²` in
//!   lexicographic order, so an oreseed inside is decided at the window's north-west corner clipped to the square;
//! - a player walking outward loads rows/columns at the edge of the view square (x outer, z inner), so a fresh
//!   oreseed is decided at the corner of its ±2 window facing the player (ties: west / north). Not validated:
//!   the ground truth (rwg2dump 2.5, seed 1337) only has the spawn square and a z-major force-load, i.e. NW corners.
//!
//! Ground truth (252 oreseeds, `tests/gtores.rs`): exact replay of the dump world's load order with ore placement
//! 220; window-NW corner on exact terrain 221; [`resolve_ordered`] (exact, spawn (0, 0)) 220;
//! [`resolve_ordered_approx`] 217; old box-centre model 213. The ~30 misses are mostly deep candidates the game
//! rejected although the test column is solid on the terrain model (caves are modelled and do not explain them;
//! not modelled: lakes, dirt/gravel blobs, dungeons, mineshafts, other mods' population).

use crate::dungeons::Terrain;
use crate::gtores::{
    execute_chunkified, is_oreseed, resolve_at, resolve_at_world, BlockQuery, GtConfig, Placement,
    VeinTable, Xstr,
};
use crate::java_random::JavaRandom;
use crate::jmath::FxMap;

/// GT's `MAX_VEIN_SIZE` (chunks).
pub const MAX_VEIN_SIZE: i32 = 2;

/// Simulated chunk loading / population of one world, recording the population (= GT processing) order.
#[derive(Default)]
pub struct LoadSim {
    loaded: FxMap<(i32, i32), bool>, // value: populated
    pub order: Vec<(i32, i32)>,
}

impl LoadSim {
    pub fn new() -> Self {
        Self::default()
    }

    fn exists(&self, x: i32, z: i32) -> bool {
        self.loaded.contains_key(&(x, z))
    }

    fn try_populate(&mut self, x: i32, z: i32) {
        if self.loaded.get(&(x, z)) == Some(&false)
            && self.exists(x + 1, z)
            && self.exists(x, z + 1)
            && self.exists(x + 1, z + 1)
        {
            self.loaded.insert((x, z), true);
            self.order.push((x, z));
        }
    }

    /// `ChunkProviderServer.loadChunk` (+ `Chunk.populateChunk`). No-op for loaded chunks.
    pub fn load(&mut self, x: i32, z: i32) {
        if self.exists(x, z) {
            return;
        }
        self.loaded.insert((x, z), false);
        self.try_populate(x, z);
        self.try_populate(x - 1, z);
        self.try_populate(x, z - 1);
        self.try_populate(x - 1, z - 1);
    }

    /// `MinecraftServer.initialWorldChunkLoad` around the spawn block `(sx, sz)`.
    pub fn spawn_area(&mut self, sx: i32, sz: i32) {
        for k in (-192..=192).step_by(16) {
            for l in (-192..=192).step_by(16) {
                self.load((sx + k) >> 4, (sz + l) >> 4);
            }
        }
    }

    /// Every chunk overlapping the block rectangle, z outer / x inner (a typical force-load loop).
    pub fn area_z_major(&mut self, x0: i32, z0: i32, x1: i32, z1: i32) {
        for cz in z0 >> 4..=z1 >> 4 {
            for cx in x0 >> 4..=x1 >> 4 {
                self.load(cx, cz);
            }
        }
    }

    /// First processed chunk per oreseed, for every oreseed some processed chunk reached.
    pub fn first_processed(
        &self,
        is_oreseed: impl Fn(i32, i32) -> bool,
    ) -> FxMap<(i32, i32), (i32, i32)> {
        let mut m = FxMap::default();
        for &(px, pz) in &self.order {
            for ox in px - MAX_VEIN_SIZE..=px + MAX_VEIN_SIZE {
                for oz in pz - MAX_VEIN_SIZE..=pz + MAX_VEIN_SIZE {
                    if is_oreseed(ox, oz) {
                        m.entry((ox, oz)).or_insert((px, pz));
                    }
                }
            }
        }
        m
    }
}

/// The chunks the spawn area populates: `[lo, hi]²` (chunk coordinates) around the spawn block.
pub fn spawn_populated(spawn: (i32, i32)) -> (i32, i32, i32, i32) {
    (
        (spawn.0 - 192) >> 4,
        (spawn.1 - 192) >> 4,
        ((spawn.0 + 192) >> 4) - 1,
        ((spawn.1 + 192) >> 4) - 1,
    )
}

/// Order-free estimate of the chunk GT decides oreseed `(ox, oz)` in, for a world created with spawn block
/// `spawn` and explored outward from there:
/// - window (±2) touches the spawn area's populated square → its north-west-most chunk in that square
///   (lexicographic population order of the spawn load);
/// - otherwise the window corner facing spawn (a player approaching from spawn; ties west / north).
pub fn processing_chunk(ox: i32, oz: i32, spawn: (i32, i32)) -> (i32, i32) {
    let r = MAX_VEIN_SIZE;
    let (x0, z0, x1, z1) = spawn_populated(spawn);
    if ox + r >= x0 && ox - r <= x1 && oz + r >= z0 && oz - r <= z1 {
        return ((ox - r).max(x0), (oz - r).max(z0));
    }
    let (scx, scz) = (spawn.0 >> 4, spawn.1 >> 4);
    let px = if scx > ox + r { ox + r } else { ox - r };
    let pz = if scz > oz + r { oz + r } else { oz - r };
    (px, pz)
}

/// Stone on the pre-population terrain model: below the column top minus the surface layer, above bedrock.
/// (Caves, ravines, lakes and surface blocks other than a uniform `surface` layer are not modelled.)
pub fn terrain_stone(t: &mut Terrain, x: i32, y: i32, z: i32, surface: i32) -> bool {
    y > 0 && t.top(x, z).map_or(true, |top| y <= top - surface)
}

/// Surface layer thickness used by [`resolve_ordered`] (grass/dirt/sand above stone; RWG2's default `rReplace`
/// puts grass + up to 5 dirt; 3–5 all score within one oreseed on the ground truth).
pub const SURFACE_DEPTH: i32 = 4;

/// The vein of oreseed `(ox, oz)` with GT's exact placement test run from [`processing_chunk`] on the
/// exact pre-population terrain. Costs the chunks the test touches (≤ 4 per oreseed, ~1.2 ms each, cached in `t`).
#[allow(clippy::too_many_arguments)]
pub fn resolve_ordered(
    world_seed: i64,
    table: &VeinTable,
    cfg: &GtConfig,
    ox: i32,
    oz: i32,
    spawn: (i32, i32),
    t: &mut Terrain,
) -> Option<Placement> {
    let (px, pz) = processing_chunk(ox, oz, spawn);
    resolve_at(world_seed, table, cfg, ox, oz, px, pz, |x, y, z| {
        terrain_stone(t, x, y, z, SURFACE_DEPTH)
    })
}

/// [`resolve_ordered`] on [`ApproxTerrain`] (no chunk generation; ~0.03 ms per oreseed native).
#[allow(clippy::too_many_arguments)]
pub fn resolve_ordered_approx(
    world_seed: i64,
    table: &VeinTable,
    cfg: &GtConfig,
    ox: i32,
    oz: i32,
    spawn: (i32, i32),
    a: &mut ApproxTerrain,
) -> Option<Placement> {
    resolve_ordered_with(world_seed, table, cfg, ox, oz, spawn, |x, z| {
        Some(a.top(x, z))
    })
}

/// The vein oreseed `(ox, oz)` gets when each corner chunk of its ±2 window is the one GT decides it in (where a
/// player arriving from that side triggers it), on [`ApproxTerrain`]: distinct outcomes, most frequent first, with
/// counts (sum 4). One outcome = the same whichever side the area is first loaded from: right 92.9% on 7 held-out
/// worlds vs 62% for order-dependent oreseeds, whose real vein is among the corners + the spawn-facing pick 93% of
/// the time. (All 25 window chunks: 93.3% / 66% / 95%, 5× the cost.)
#[allow(clippy::too_many_arguments)]
pub fn window_veins_approx(
    world_seed: i64,
    table: &VeinTable,
    cfg: &GtConfig,
    ox: i32,
    oz: i32,
    a: &mut ApproxTerrain,
) -> Vec<(Option<Placement>, u8)> {
    let mut out: Vec<(Option<Placement>, u8)> = Vec::new();
    for wx in [ox - MAX_VEIN_SIZE, ox + MAX_VEIN_SIZE] {
        for wz in [oz - MAX_VEIN_SIZE, oz + MAX_VEIN_SIZE] {
            let v = resolve_at(world_seed, table, cfg, ox, oz, wx, wz, |x, y, z| {
                y > 0 && y <= a.top(x, z) - SURFACE_DEPTH
            });
            let key = v.as_ref().map(|p| p.rejected);
            match out
                .iter_mut()
                .find(|(q, _)| q.as_ref().map(|p| p.rejected) == key)
            {
                Some(e) => e.1 += 1,
                None => out.push((v, 1)),
            }
        }
    }
    out.sort_by_key(|e| std::cmp::Reverse(e.1));
    out
}

/// [`resolve_ordered`] on any column-top function (`None` = unknown → stone).
#[allow(clippy::too_many_arguments)]
pub fn resolve_ordered_with(
    world_seed: i64,
    table: &VeinTable,
    cfg: &GtConfig,
    ox: i32,
    oz: i32,
    spawn: (i32, i32),
    mut top: impl FnMut(i32, i32) -> Option<i32>,
) -> Option<Placement> {
    let (px, pz) = processing_chunk(ox, oz, spawn);
    resolve_at(world_seed, table, cfg, ox, oz, px, pz, |x, y, z| {
        y > 0 && top(x, z).map_or(true, |h| y <= h - SURFACE_DEPTH)
    })
}

// ------------------------------------------------------------------------------------------------ caves

/// Vanilla 1.7.10 `MapGenCaves` (RWG2 calls it after the surface pass), as patched by Hodgepodge's
/// `fastRandom` (default on in GTNH): `MapGenBase.rand` and the tunnel `Random` are `StdLCG`, whose `setSeed`
/// stores the seed **without** the `^ 0x5DEECE66D` scramble, and one `StdLCG` per generator is shared by all
/// tunnel calls (a branch re-seeds it, so the second branch's seed is drawn after the first branch ran).
pub struct Caves {
    seed: i64,
    fast_random: bool,
    l: i64,
    l1: i64,
    sin: Vec<f32>,
}

/// Carved blocks of one chunk (`[lz][lx]` columns of 256 bits).
pub struct CaveMask(Vec<u64>);

impl CaveMask {
    #[inline]
    pub fn carved(&self, lx: i32, y: i32, lz: i32) -> bool {
        if !(0..256).contains(&y) {
            return false;
        }
        let bit = ((lz * 16 + lx) * 256 + y) as usize;
        self.0[bit >> 6] >> (bit & 63) & 1 != 0
    }

    fn set(&mut self, lx: i32, y: i32, lz: i32) {
        let bit = ((lz * 16 + lx) * 256 + y) as usize;
        self.0[bit >> 6] |= 1 << (bit & 63);
    }
}

struct CaveCtx<'a> {
    heights: &'a [i32; 256],
    /// extra water blocks (local x, y, local z) besides the sea above the column top
    water: &'a dyn Fn(i32, i32, i32) -> bool,
    mask: CaveMask,
    /// per dig step and column, in generation order: `[lx, lz, y_top, y_bottom]` (the blocks the step reaches,
    /// whatever they are), when recorded
    runs: Option<Vec<[u8; 4]>>,
    cx: i32,
    cz: i32,
    /// the generator's shared tunnel RNG (Hodgepodge) / a fresh `Random` per tunnel (vanilla)
    tunnel: JavaRandom,
}

const RANGE: i32 = 8;
const PI: f32 = std::f32::consts::PI;

impl Caves {
    pub fn new(world_seed: i64, fast_random: bool) -> Self {
        let sin = (0..65536)
            .map(|i| (i as f64 * std::f64::consts::PI * 2.0 / 65536.0).sin() as f32)
            .collect();
        let mut c = Caves {
            seed: world_seed,
            fast_random,
            l: 0,
            l1: 0,
            sin,
        };
        let mut r = c.random(world_seed);
        c.l = r.next_long();
        c.l1 = r.next_long();
        c
    }

    fn random(&self, seed: i64) -> JavaRandom {
        JavaRandom::new(if self.fast_random {
            seed ^ 0x5_DEEC_E66D
        } else {
            seed
        })
    }

    #[inline]
    fn msin(&self, f: f32) -> f32 {
        self.sin[((f * 10430.378) as i32 & 65535) as usize]
    }

    #[inline]
    fn mcos(&self, f: f32) -> f32 {
        self.sin[((f * 10430.378 + 16384.0) as i32 & 65535) as usize]
    }

    /// Blocks carved in chunk `(cx, cz)` whose pre-cave column tops (stone up to `heights[lz * 16 + lx]`,
    /// water up to 62) are given.
    pub fn carve(&self, cx: i32, cz: i32, heights: &[i32; 256]) -> CaveMask {
        self.carve_water(cx, cz, heights, &|_, _, _| false)
    }

    /// [`Caves::carve`] with extra water (`water(lx, y, lz)`), e.g. RWG2 mountain-chain river tunnels, which the
    /// vanilla dig refuses to cut into.
    pub fn carve_water(
        &self,
        cx: i32,
        cz: i32,
        heights: &[i32; 256],
        water: &dyn Fn(i32, i32, i32) -> bool,
    ) -> CaveMask {
        self.carve_water_runs(cx, cz, heights, water, false).0
    }

    /// [`Caves::carve_water`], optionally also the dig steps in generation order (`[lx, lz, y_top, y_bottom]` per
    /// step and column: `digBlock` replays need them, since `foundTop` resets per step and column).
    pub fn carve_water_runs(
        &self,
        cx: i32,
        cz: i32,
        heights: &[i32; 256],
        water: &dyn Fn(i32, i32, i32) -> bool,
        record: bool,
    ) -> (CaveMask, Vec<[u8; 4]>) {
        let mut ctx = CaveCtx {
            heights,
            water,
            mask: CaveMask(vec![0; 1024]),
            runs: record.then(Vec::new),
            cx,
            cz,
            tunnel: JavaRandom::new(0),
        };
        for j1 in cx - RANGE..=cx + RANGE {
            for k1 in cz - RANGE..=cz + RANGE {
                let mut rand = self.random(
                    (j1 as i64).wrapping_mul(self.l)
                        ^ (k1 as i64).wrapping_mul(self.l1)
                        ^ self.seed,
                );
                self.recursive_generate(&mut rand, j1, k1, &mut ctx);
            }
        }
        (ctx.mask, ctx.runs.unwrap_or_default())
    }

    fn recursive_generate(&self, rand: &mut JavaRandom, j1: i32, k1: i32, ctx: &mut CaveCtx) {
        let a = rand.next_int_bounded(15) + 1;
        let b = rand.next_int_bounded(a) + 1;
        let mut n = rand.next_int_bounded(b);
        if rand.next_int_bounded(7) != 0 {
            n = 0;
        }
        for _ in 0..n {
            let x = (j1 * 16 + rand.next_int_bounded(16)) as f64;
            let yb = rand.next_int_bounded(120) + 8;
            let y = rand.next_int_bounded(yb) as f64;
            let z = (k1 * 16 + rand.next_int_bounded(16)) as f64;
            let mut k = 1;
            if rand.next_int_bounded(4) == 0 {
                let s = rand.next_long();
                let w = 1.0 + rand.next_float() * 6.0;
                self.tunnel(ctx, s, x, y, z, w, 0.0, 0.0, -1, -1, 0.5);
                k += rand.next_int_bounded(4);
            }
            for _ in 0..k {
                let yaw = rand.next_float() * PI * 2.0;
                let pitch = (rand.next_float() - 0.5) * 2.0 / 8.0;
                let mut w = rand.next_float() * 2.0 + rand.next_float();
                if rand.next_int_bounded(10) == 0 {
                    w *= rand.next_float() * rand.next_float() * 3.0 + 1.0;
                }
                let s = rand.next_long();
                self.tunnel(ctx, s, x, y, z, w, yaw, pitch, 0, 0, 1.0);
            }
        }
    }

    #[allow(clippy::too_many_arguments)]
    fn tunnel(
        &self,
        ctx: &mut CaveCtx,
        seed: i64,
        mut x: f64,
        mut y: f64,
        mut z: f64,
        width: f32,
        mut yaw: f32,
        mut pitch: f32,
        mut step: i32,
        mut max_steps: i32,
        y_scale: f64,
    ) {
        let (ccx, ccz) = (ctx.cx, ctx.cz);
        let d4 = (ccx * 16 + 8) as f64;
        let d5 = (ccz * 16 + 8) as f64;
        let mut f3 = 0f32;
        let mut f4 = 0f32;
        ctx.tunnel = self.random(seed);
        if max_steps <= 0 {
            let j1 = RANGE * 16 - 16;
            max_steps = j1 - ctx.tunnel.next_int_bounded(j1 / 4);
        }
        let mut room = false;
        if step == -1 {
            step = max_steps / 2;
            room = true;
        }
        let k1 = ctx.tunnel.next_int_bounded(max_steps / 2) + max_steps / 4;
        let steep = ctx.tunnel.next_int_bounded(6) == 0;
        while step < max_steps {
            let d6 = 1.5 + (self.msin(step as f32 * PI / max_steps as f32) * width * 1.0) as f64;
            let d7 = d6 * y_scale;
            let f5 = self.mcos(pitch);
            let f6 = self.msin(pitch);
            x += (self.mcos(yaw) * f5) as f64;
            y += f6 as f64;
            z += (self.msin(yaw) * f5) as f64;
            if steep {
                pitch *= 0.92;
            } else {
                pitch *= 0.7;
            }
            pitch += f4 * 0.1;
            yaw += f3 * 0.1;
            f4 *= 0.9;
            f3 *= 0.75;
            let r = &mut ctx.tunnel;
            f4 += (r.next_float() - r.next_float()) * r.next_float() * 2.0;
            f3 += (r.next_float() - r.next_float()) * r.next_float() * 4.0;
            if !room && step == k1 && width > 1.0 && max_steps > 0 {
                let s = ctx.tunnel.next_long();
                let w = ctx.tunnel.next_float() * 0.5 + 0.5;
                let parent = ctx.tunnel.clone();
                self.tunnel(
                    ctx,
                    s,
                    x,
                    y,
                    z,
                    w,
                    yaw - PI / 2.0,
                    pitch / 3.0,
                    step,
                    max_steps,
                    1.0,
                );
                if !self.fast_random {
                    ctx.tunnel = parent; // vanilla: each tunnel owns its Random
                }
                let s = ctx.tunnel.next_long();
                let w = ctx.tunnel.next_float() * 0.5 + 0.5;
                self.tunnel(
                    ctx,
                    s,
                    x,
                    y,
                    z,
                    w,
                    yaw + PI / 2.0,
                    pitch / 3.0,
                    step,
                    max_steps,
                    1.0,
                );
                return;
            }
            if room || ctx.tunnel.next_int_bounded(4) != 0 {
                let d8 = x - d4;
                let d9 = z - d5;
                let d10 = (max_steps - step) as f64;
                let d11 = (width + 2.0 + 16.0) as f64;
                if d8 * d8 + d9 * d9 - d10 * d10 > d11 * d11 {
                    return;
                }
                if x >= d4 - 16.0 - d6 * 2.0
                    && z >= d5 - 16.0 - d6 * 2.0
                    && x <= d4 + 16.0 + d6 * 2.0
                    && z <= d5 + 16.0 + d6 * 2.0
                {
                    self.dig(ctx, x, y, z, d6, d7);
                    if room {
                        break;
                    }
                }
            }
            step += 1;
        }
    }

    fn dig(&self, ctx: &mut CaveCtx, x: f64, y: f64, z: f64, d6: f64, d7: f64) {
        let (ccx, ccz) = (ctx.cx, ctx.cz);
        let i4 = (floor(x - d6) - ccx * 16 - 1).max(0);
        let l1 = (floor(x + d6) - ccx * 16 + 1).min(16);
        let j4 = (floor(y - d7) - 1).max(1);
        let i2 = (floor(y + d7) + 1).min(248);
        let k4 = (floor(z - d6) - ccz * 16 - 1).max(0);
        let j2 = (floor(z + d6) - ccz * 16 + 1).min(16);
        // water check: edge columns fully, interior columns top and bottom only
        let water = |lx: i32, yy: i32, lz: i32| {
            let h = ctx.heights[(lz * 16 + lx) as usize];
            (yy > h && yy < 63) || (ctx.water)(lx, yy, lz)
        };
        for k2 in i4..l1 {
            for l2 in k4..j2 {
                let mut i3 = i2 + 1;
                while i3 >= j4 - 1 {
                    if (0..256).contains(&i3) {
                        if water(k2, i3, l2) {
                            return;
                        }
                        if i3 != j4 - 1 && k2 != i4 && k2 != l1 - 1 && l2 != k4 && l2 != j2 - 1 {
                            i3 = j4;
                        }
                    }
                    i3 -= 1;
                }
            }
        }
        for k2 in i4..l1 {
            let d13 = ((k2 + ccx * 16) as f64 + 0.5 - x) / d6;
            for j3 in k4..j2 {
                let d14 = ((j3 + ccz * 16) as f64 + 0.5 - z) / d6;
                if d13 * d13 + d14 * d14 >= 1.0 {
                    continue;
                }
                let h = ctx.heights[(j3 * 16 + k2) as usize];
                let (mut top, mut bot) = (-1, -1);
                for l3 in (j4..i2).rev() {
                    let d12 = (l3 as f64 + 0.5 - y) / d7;
                    // vanilla off-by-one: the block written is one above the tested one
                    let by = l3 + 1;
                    if d12 > -0.7 && d13 * d13 + d12 * d12 + d14 * d14 < 1.0 {
                        if top < 0 {
                            top = by;
                        }
                        bot = by;
                        if by <= h {
                            ctx.mask.set(k2, by, j3);
                        }
                    }
                }
                if top >= 0 {
                    if let Some(r) = ctx.runs.as_mut() {
                        r.push([k2 as u8, j3 as u8, top as u8, bot as u8]);
                    }
                }
            }
        }
    }
}

#[inline]
fn floor(d: f64) -> i32 {
    let i = d as i32;
    if d < i as f64 {
        i - 1
    } else {
        i
    }
}

/// Stone oracle on the pre-population terrain with caves carved: `stone(x, y, z)` = stone up to the column top
/// minus `surface` blocks, above y 0, not carved.
pub struct Underground<'t, 'p> {
    pub t: &'t mut Terrain<'p>,
    pub caves: Option<Caves>,
    pub surface: i32,
    masks: FxMap<(i32, i32), CaveMask>,
    /// RWG2 populate prefix overlay (lakes, dungeons, geodes, dirt/gravel blobs): changed blocks are not stone
    pub pop: Option<Box<crate::population::PopModel<'p>>>,
    /// chunks populated so far (`None`: all), for [`Underground::pop`]
    pub populated: Option<std::collections::HashSet<(i32, i32)>>,
}

impl<'t, 'p> Underground<'t, 'p> {
    pub fn new(t: &'t mut Terrain<'p>, caves: Option<Caves>, surface: i32) -> Self {
        Underground {
            t,
            caves,
            surface,
            masks: FxMap::default(),
            pop: None,
            populated: None,
        }
    }

    /// Carved mask of a chunk (`None` without caves or terrain).
    pub fn mask(&mut self, cx: i32, cz: i32) -> Option<&CaveMask> {
        let caves = self.caves.as_ref()?;
        if !self.masks.contains_key(&(cx, cz)) {
            if self.masks.len() > 4096 {
                self.masks.clear();
            }
            let c = self.t.chunk(cx, cz)?;
            let mut h = [0i32; 256];
            for (o, v) in h.iter_mut().zip(c.height.iter()) {
                *o = *v as i32;
            }
            let m = caves.carve(cx, cz, &h);
            self.masks.insert((cx, cz), m);
        }
        self.masks.get(&(cx, cz))
    }

    pub fn stone(&mut self, x: i32, y: i32, z: i32) -> bool {
        if y <= 0 {
            return false;
        }
        match self.t.top(x, z) {
            Some(top) if y > top - self.surface => return false,
            None => return true,
            _ => {}
        }
        if self
            .mask(x >> 4, z >> 4)
            .is_some_and(|m| m.carved(x & 15, y, z & 15))
        {
            return false;
        }
        match &mut self.pop {
            Some(pm) => {
                let done = &self.populated;
                pm.changed(x, y, z, |cx, cz| {
                    done.as_ref().map_or(true, |d| d.contains(&(cx, cz)))
                })
                .map_or(true, |b| b == crate::population::B::Stone)
            }
            None => true,
        }
    }
}

// ------------------------------------------------------------------------------------------------ ore world

/// Replays GT's vein decisions *and* ore placement in a given processing order, so the placement test sees the
/// ores earlier veins put into the world (an ore block of another material is not stone: it fails the no-overlap
/// stone count and the dry run, while the same natural ore counts as placeable).
pub struct OreWorld<'a, 'u, 't, 'p> {
    pub world_seed: i64,
    pub table: &'a VeinTable,
    pub cfg: &'a GtConfig,
    pub u: &'u mut Underground<'t, 'p>,
    /// placed ore material per block (interned material name)
    ores: FxMap<(i32, i32, i32), u16>,
    mat_ids: Vec<[u16; 4]>,
    /// decided oreseeds: vein + its RNG (`generationSeed`), `None` = empty
    pub decided: FxMap<(i32, i32), Option<(Placement, Xstr)>>,
    /// processing chunk that decided each oreseed
    pub decided_at: FxMap<(i32, i32), (i32, i32)>,
}

impl<'a, 'u, 't, 'p> OreWorld<'a, 'u, 't, 'p> {
    pub fn new(
        world_seed: i64,
        table: &'a VeinTable,
        cfg: &'a GtConfig,
        u: &'u mut Underground<'t, 'p>,
    ) -> Self {
        let mut names: Vec<&str> = Vec::new();
        let mut id = |n: &'a str| -> u16 {
            if let Some(i) = names.iter().position(|&m| m == n) {
                i as u16
            } else {
                names.push(n);
                (names.len() - 1) as u16
            }
        };
        let mat_ids = table
            .overworld
            .iter()
            .map(|v| {
                [
                    id(&v.ores[0]),
                    id(&v.ores[1]),
                    id(&v.ores[2]),
                    id(&v.ores[3]),
                ]
            })
            .collect();
        OreWorld {
            world_seed,
            table,
            cfg,
            u,
            ores: FxMap::default(),
            mat_ids,
            decided: FxMap::default(),
            decided_at: FxMap::default(),
        }
    }

    pub fn ore_count(&self) -> usize {
        self.ores.len()
    }

    /// `WorldGenContainer.run`'s oregen part for processing chunk `(px, pz)`.
    pub fn process(&mut self, px: i32, pz: i32) {
        for ox in px - MAX_VEIN_SIZE..=px + MAX_VEIN_SIZE {
            for oz in pz - MAX_VEIN_SIZE..=pz + MAX_VEIN_SIZE {
                if !is_oreseed(self.cfg, ox, oz) {
                    continue;
                }
                if !self.decided.contains_key(&(ox, oz)) {
                    let (ores, mats, u) = (&self.ores, &self.mat_ids, &mut *self.u);
                    let table = self.table;
                    let d = resolve_at_world(
                        self.world_seed,
                        table,
                        self.cfg,
                        ox,
                        oz,
                        px,
                        pz,
                        |v, x, y, z, q| match ores.get(&(x, y, z)) {
                            Some(&m) => match q {
                                BlockQuery::Stone => false,
                                BlockQuery::Ore(k) => {
                                    let vi = table
                                        .overworld
                                        .iter()
                                        .position(|w| std::ptr::eq(w, v))
                                        .unwrap();
                                    mats[vi][k] == m
                                }
                            },
                            None => u.stone(x, y, z),
                        },
                    );
                    self.decided.insert((ox, oz), d);
                    self.decided_at.insert((ox, oz), (px, pz));
                }
                if let Some(Some((p, rng))) = self.decided.get(&(ox, oz)).cloned() {
                    let v = &self.table.overworld[p.vein];
                    let mats = self.mat_ids[p.vein];
                    let (ores, u) = (&mut self.ores, &mut *self.u);
                    let mut placed: Vec<(i32, i32, i32, u16)> = Vec::new();
                    execute_chunkified(
                        v,
                        &p,
                        rng,
                        ox,
                        oz,
                        px,
                        pz,
                        false,
                        // a run never visits a block twice, so this run's own ores need not be visible yet
                        |x, y, z, _| !ores.contains_key(&(x, y, z)) && u.stone(x, y, z),
                        |x, y, z, k| placed.push((x, y, z, mats[k])),
                    );
                    for (x, y, z, m) in placed {
                        ores.insert((x, y, z), m);
                    }
                }
            }
        }
    }
}

// ------------------------------------------------------------------------------------------------ approximate terrain

/// Cheap column tops without chunk generation: `chunkgen::approx_parts` on a 16-block grid (cached), blurred with
/// the same 48-block triangle the viewer's zoomed-out terrain uses, bilinear between grid nodes, then
/// `approx_finish` at the column. rms error vs exact chunks ≈ 2–5 blocks.
pub struct ApproxTerrain<'p> {
    pub p: &'p crate::provider::BiomeProvider,
    pub h: &'p dyn crate::provider::Heights,
    parts: FxMap<(i32, i32), (f32, f32)>,
    blurred: FxMap<(i32, i32), (f32, f32)>,
}

const GRID: i32 = 16;
const BLUR: i32 = 3; // nodes (48 blocks)

impl<'p> ApproxTerrain<'p> {
    pub fn new(p: &'p crate::provider::BiomeProvider, h: &'p dyn crate::provider::Heights) -> Self {
        ApproxTerrain {
            p,
            h,
            parts: FxMap::default(),
            blurred: FxMap::default(),
        }
    }

    fn part(&mut self, i: i32, j: i32) -> (f32, f32) {
        if let Some(v) = self.parts.get(&(i, j)) {
            return *v;
        }
        if self.parts.len() > 1 << 20 {
            self.parts.clear();
        }
        let v = crate::chunkgen::approx_parts(self.p, self.h, i * GRID, j * GRID)
            .unwrap_or((64.0, 0.0));
        self.parts.insert((i, j), v);
        v
    }

    fn node(&mut self, i: i32, j: i32) -> (f32, f32) {
        if let Some(v) = self.blurred.get(&(i, j)) {
            return *v;
        }
        if self.blurred.len() > 1 << 20 {
            self.blurred.clear();
        }
        let (mut s0, mut s1, mut ks) = (0f32, 0f32, 0f32);
        for dj in -BLUR..=BLUR {
            for di in -BLUR..=BLUR {
                let k = ((BLUR + 1 - di.abs()) * (BLUR + 1 - dj.abs())) as f32;
                let v = self.part(i + di, j + dj);
                s0 += v.0 * k;
                s1 += v.1 * k;
                ks += k;
            }
        }
        let v = (s0 / ks, s1 / ks);
        self.blurred.insert((i, j), v);
        v
    }

    /// Approximate `(int) testHeight` at a column.
    pub fn top(&mut self, x: i32, z: i32) -> i32 {
        let (i, j) = (x.div_euclid(GRID), z.div_euclid(GRID));
        let (fx, fz) = (
            (x.rem_euclid(GRID)) as f32 / GRID as f32,
            (z.rem_euclid(GRID)) as f32 / GRID as f32,
        );
        let a = self.node(i, j);
        let b = self.node(i + 1, j);
        let c = self.node(i, j + 1);
        let d = self.node(i + 1, j + 1);
        let lerp = |p: f32, q: f32, t: f32| p + (q - p) * t;
        let th = lerp(lerp(a.0, b.0, fx), lerp(c.0, d.0, fx), fz);
        let m = lerp(lerp(a.1, b.1, fx), lerp(c.1, d.1, fx), fz);
        crate::chunkgen::approx_finish(self.p, self.h, x, z, th, m) as i32
    }
}
