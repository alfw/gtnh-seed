//! Block terrain of the other dimensions GTNH players visit: the Biomes O' Plenty Nether
//! (`biomesoplenty.common.world.ChunkProviderBOPHell`, BOP 2.1.0) and the Twilight Forest
//! (`twilightforest.world.ChunkProviderTwilightForest`, TF 2.7.45 GTNH), on top of vanilla 1.7.10
//! `NoiseGeneratorOctaves` / `NoiseGeneratorImproved`.
//!
//! Both chunk providers seed their own `java.util.Random` (TF: `FastRandom`, same scrambled LCG) from the
//! world seed; Hodgepodge's `fastRandom` only swaps `MapGenBase.rand` (caves, ravines, structures), not these.
//!
//! Covered (bit-exact against golden vectors from the verbatim Java methods, `tests/dimterrain.rs`):
//! * Nether: density field → netherrack/lava/air, then `replaceBlocksForBiome`'s solidity changes (top block
//!   removed where the exclusivity depth ≤ 0, bedrock floor/roof with their random 0–4 block jitter).
//!   Biome surface blocks (soul sand, ash, …) are all solid and are not tracked. `MapGenCavesHell` is not
//!   ported (caves carve the netherrack).
//! * Twilight Forest: `generateTerrain2` (256-high density, sea level 63) + `squishTerrain` (y → 2y + 1,
//!   sea level 31), dark forest canopy, glaciers, and `deformTerrainForFeature` (hollow hills / hydra lair,
//!   hedge maze / naga courtyard / quest grove flattening, yeti lairs). Not covered: troll-lair clouds (use
//!   `World.rand`, not predictable), `TFGenCaves` / `TFGenRavine`, decoration (trees, which TF has plenty of).

use crate::java_random::JavaRandom;
use crate::twilight::{TfBiomeIds, TwilightForest, TF_FEATURES};

/// `NoiseGeneratorImproved`.
#[derive(Clone)]
pub struct Improved {
    perm: [u16; 512],
    xc: f64,
    yc: f64,
    zc: f64,
}

const GX: [f64; 16] = [
    1., -1., 1., -1., 1., -1., 1., -1., 0., 0., 0., 0., 1., 0., -1., 0.,
];
const GY: [f64; 16] = [
    1., 1., -1., -1., 0., 0., 0., 0., 1., -1., 1., -1., 1., -1., 1., -1.,
];
const GZ: [f64; 16] = [
    0., 0., 0., 0., 1., 1., -1., -1., 1., 1., -1., -1., 0., 1., 0., -1.,
];
const G2X: [f64; 16] = [
    1., -1., 1., -1., 1., -1., 1., -1., 0., 0., 0., 0., 1., 0., -1., 0.,
];
const G2Z: [f64; 16] = [
    0., 0., 0., 0., 1., 1., -1., -1., 1., 1., -1., -1., 0., 1., 0., -1.,
];

#[inline]
fn lerp(t: f64, a: f64, b: f64) -> f64 {
    a + t * (b - a)
}
#[inline]
fn grad(h: u16, x: f64, y: f64, z: f64) -> f64 {
    let i = (h & 15) as usize;
    GX[i] * x + GY[i] * y + GZ[i] * z
}
#[inline]
fn grad2(h: u16, x: f64, z: f64) -> f64 {
    let i = (h & 15) as usize;
    G2X[i] * x + G2Z[i] * z
}
#[inline]
fn floor_i(v: f64) -> i32 {
    let i = v as i32;
    if v < i as f64 {
        i - 1
    } else {
        i
    }
}
#[inline]
fn fade(t: f64) -> f64 {
    t * t * t * (t * (t * 6.0 - 15.0) + 10.0)
}

impl Improved {
    pub fn new(r: &mut JavaRandom) -> Self {
        let xc = r.next_double() * 256.0;
        let yc = r.next_double() * 256.0;
        let zc = r.next_double() * 256.0;
        let mut perm = [0u16; 512];
        for (i, p) in perm.iter_mut().enumerate().take(256) {
            *p = i as u16;
        }
        for i in 0..256 {
            let j = r.next_int_bounded(256 - i as i32) as usize + i;
            perm.swap(i, j);
            perm[i + 256] = perm[i];
        }
        Improved { perm, xc, yc, zc }
    }

    #[inline]
    fn p(&self, i: i32) -> i32 {
        self.perm[i as usize] as i32
    }

    #[allow(clippy::too_many_arguments)]
    pub fn populate(
        &self,
        out: &mut [f64],
        x: f64,
        y: f64,
        z: f64,
        w: i32,
        h: i32,
        d: i32,
        sx: f64,
        sy: f64,
        sz: f64,
        amp: f64,
    ) {
        let inv = 1.0 / amp;
        if h == 1 {
            let mut idx = 0;
            for ix in 0..w {
                let mut fx = x + ix as f64 * sx + self.xc;
                let xi = floor_i(fx);
                let xl = xi & 255;
                fx -= xi as f64;
                let u = fade(fx);
                for iz in 0..d {
                    let mut fz = z + iz as f64 * sz + self.zc;
                    let zi = floor_i(fz);
                    let zl = zi & 255;
                    fz -= zi as f64;
                    let v = fade(fz);
                    let a = self.p(xl);
                    let aa = self.p(a) + zl;
                    let b = self.p(xl + 1);
                    let ba = self.p(b) + zl;
                    let l0 = lerp(
                        u,
                        grad2(self.perm[aa as usize], fx, fz),
                        grad(self.perm[ba as usize], fx - 1.0, 0.0, fz),
                    );
                    let l1 = lerp(
                        u,
                        grad(self.perm[aa as usize + 1], fx, 0.0, fz - 1.0),
                        grad(self.perm[ba as usize + 1], fx - 1.0, 0.0, fz - 1.0),
                    );
                    out[idx] += lerp(v, l0, l1) * inv;
                    idx += 1;
                }
            }
            return;
        }
        let mut idx = 0;
        let mut last = -1;
        let (mut l1, mut l2, mut l3, mut l4) = (0.0, 0.0, 0.0, 0.0);
        for ix in 0..w {
            let mut fx = x + ix as f64 * sx + self.xc;
            let xi = floor_i(fx);
            let xl = xi & 255;
            fx -= xi as f64;
            let u = fade(fx);
            for iz in 0..d {
                let mut fz = z + iz as f64 * sz + self.zc;
                let zi = floor_i(fz);
                let zl = zi & 255;
                fz -= zi as f64;
                let w2 = fade(fz);
                for iy in 0..h {
                    let mut fy = y + iy as f64 * sy + self.yc;
                    let yi = floor_i(fy);
                    let yl = yi & 255;
                    fy -= yi as f64;
                    let v = fade(fy);
                    if iy == 0 || yl != last {
                        last = yl;
                        let a = self.p(xl) + yl;
                        let aa = self.p(a) + zl;
                        let ab = self.p(a + 1) + zl;
                        let b = self.p(xl + 1) + yl;
                        let ba = self.p(b) + zl;
                        let bb = self.p(b + 1) + zl;
                        let pp = |i: i32| self.perm[i as usize];
                        l1 = lerp(u, grad(pp(aa), fx, fy, fz), grad(pp(ba), fx - 1.0, fy, fz));
                        l2 = lerp(
                            u,
                            grad(pp(ab), fx, fy - 1.0, fz),
                            grad(pp(bb), fx - 1.0, fy - 1.0, fz),
                        );
                        l3 = lerp(
                            u,
                            grad(pp(aa + 1), fx, fy, fz - 1.0),
                            grad(pp(ba + 1), fx - 1.0, fy, fz - 1.0),
                        );
                        l4 = lerp(
                            u,
                            grad(pp(ab + 1), fx, fy - 1.0, fz - 1.0),
                            grad(pp(bb + 1), fx - 1.0, fy - 1.0, fz - 1.0),
                        );
                    }
                    let a = lerp(v, l1, l2);
                    let b = lerp(v, l3, l4);
                    out[idx] += lerp(w2, a, b) * inv;
                    idx += 1;
                }
            }
        }
    }
}

/// `NoiseGeneratorOctaves`.
#[derive(Clone)]
pub struct Octaves {
    gens: Vec<Improved>,
}

impl Octaves {
    pub fn new(r: &mut JavaRandom, n: usize) -> Self {
        Octaves {
            gens: (0..n).map(|_| Improved::new(r)).collect(),
        }
    }

    /// `generateNoiseOctaves(null, x, y, z, w, h, d, sx, sy, sz)` (index order x, z, y).
    #[allow(clippy::too_many_arguments)]
    pub fn noise(
        &self,
        x: i32,
        y: i32,
        z: i32,
        w: i32,
        h: i32,
        d: i32,
        sx: f64,
        sy: f64,
        sz: f64,
    ) -> Vec<f64> {
        let mut out = vec![0.0; (w * h * d) as usize];
        let mut amp = 1.0f64;
        for g in &self.gens {
            let mut fx = x as f64 * amp * sx;
            let fy = y as f64 * amp * sy;
            let mut fz = z as f64 * amp * sz;
            let mut lx = floor_l(fx);
            let mut lz = floor_l(fz);
            fx -= lx as f64;
            fz -= lz as f64;
            lx %= 16_777_216;
            lz %= 16_777_216;
            fx += lx as f64;
            fz += lz as f64;
            g.populate(
                &mut out,
                fx,
                fy,
                fz,
                w,
                h,
                d,
                sx * amp,
                sy * amp,
                sz * amp,
                amp,
            );
            amp /= 2.0;
        }
        out
    }

    /// The 2D overload `generateNoiseOctaves(null, x, z, w, d, sx, sz, unused)` = 3D at y 10, height 1, y scale 1.
    pub fn noise2(&self, x: i32, z: i32, w: i32, d: i32, sx: f64, sz: f64) -> Vec<f64> {
        self.noise(x, 10, z, w, 1, d, sx, 1.0, sz)
    }
}

#[inline]
fn floor_l(v: f64) -> i64 {
    let l = v as i64;
    if v < l as f64 {
        l - 1
    } else {
        l
    }
}

// ============================================================================ Nether (BOP ChunkProviderBOPHell)

/// Nether block classes (solidity is all that is tracked).
pub mod nb {
    pub const AIR: u8 = 0;
    pub const ROCK: u8 = 1;
    pub const LAVA: u8 = 2;
    pub const BEDROCK: u8 = 3;
    /// Solid surface blocks that are not netherrack (caves leave them): `SURFACE` is the generic class, the
    /// rest name the block `replaceBlocksForBiome` writes.
    pub const SURFACE: u8 = 4;
    pub const SOUL_SAND: u8 = 5;
    pub const GRAVEL: u8 = 6;
    /// BOP Phantasmagoric Inferno top / filler
    pub const ASH: u8 = 7;
    /// BOP Visceral Heap
    pub const FLESH: u8 = 8;
    /// BOP Polar Chasm (never generates)
    pub const HARD_ICE: u8 = 9;
    /// BOP Undergarden top
    pub const OVERGROWN: u8 = 10;

    #[inline]
    pub fn solid(b: u8) -> bool {
        matches!(b, ROCK | BEDROCK | SURFACE..=OVERGROWN)
    }

    /// Non-netherrack surface block (soul sand, gravel, BOP surfaces).
    #[inline]
    pub fn surface(b: u8) -> bool {
        (SURFACE..=OVERGROWN).contains(&b)
    }

    /// Registry name of a Nether block code.
    pub fn name(b: u8) -> &'static str {
        match b {
            AIR => "minecraft:air",
            ROCK => "minecraft:netherrack",
            LAVA => "minecraft:lava",
            BEDROCK => "minecraft:bedrock",
            SOUL_SAND | SURFACE => "minecraft:soul_sand",
            GRAVEL => "minecraft:gravel",
            ASH => "BiomesOPlenty:ash",
            FLESH => "BiomesOPlenty:flesh",
            HARD_ICE => "BiomesOPlenty:hardIce",
            OVERGROWN => "BiomesOPlenty:overgrownNetherrack",
            _ => "?",
        }
    }
}

pub struct NetherTerrain {
    n1: Octaves,
    n2: Octaves,
    n3: Octaves,
    ssg: Octaves,
    nex: Octaves,
    n6: Octaves,
    n7: Octaves,
    col: [f64; 17],
    biomes: crate::nether::NetherBiomes,
    caves: CaveSeeds,
}

/// One Nether chunk: `blocks[(x * 16 + z) * 128 + y]`.
pub struct NetherChunk {
    pub blocks: Box<[u8; 32768]>,
}

impl NetherChunk {
    #[inline]
    pub fn at(&self, x: i32, y: i32, z: i32) -> u8 {
        if !(0..128).contains(&y) {
            return nb::AIR;
        }
        self.blocks[(((x & 15) * 16 + (z & 15)) * 128 + y) as usize]
    }

    /// Floor of the highest open space below y 110 (the walkable surface a player sees first when digging down
    /// from the roof), or the lava ocean level. Returns (y of the floor block, is lava).
    pub fn floor(&self, x: i32, z: i32) -> (i32, bool) {
        let mut y = 110;
        while y > 0 && self.at(x, y, z) != nb::AIR {
            y -= 1;
        }
        while y > 0 && self.at(x, y, z) == nb::AIR {
            y -= 1;
        }
        (y, self.at(x, y, z) == nb::LAVA)
    }
}

impl NetherTerrain {
    pub fn new(seed: i64) -> Self {
        let mut r = JavaRandom::new(seed);
        let n1 = Octaves::new(&mut r, 16);
        let n2 = Octaves::new(&mut r, 16);
        let n3 = Octaves::new(&mut r, 8);
        let ssg = Octaves::new(&mut r, 4);
        let nex = Octaves::new(&mut r, 4);
        let n6 = Octaves::new(&mut r, 10);
        let n7 = Octaves::new(&mut r, 16);
        let mut col = [0.0; 17];
        for (i, c) in col.iter_mut().enumerate() {
            let mut v = (i as f64 * std::f64::consts::PI * 6.0 / 17.0).cos() * 2.0;
            let mut d2 = i as f64;
            if i > 17 / 2 {
                d2 = (17 - 1 - i) as f64;
            }
            if d2 < 4.0 {
                d2 = 4.0 - d2;
                v -= d2 * d2 * d2 * 10.0;
            }
            *c = v;
        }
        NetherTerrain {
            n1,
            n2,
            n3,
            ssg,
            nex,
            n6,
            n7,
            col,
            biomes: crate::nether::NetherBiomes::new(seed),
            // Hodgepodge: MapGenBase.rand is an unscrambled StdLCG
            caves: CaveSeeds::new(seed, true),
        }
    }

    /// BOP nether biome at a block (voronoi layer).
    pub fn biome_at(&self, x: i32, z: i32) -> i32 {
        self.biomes.biome_at(x, z)
    }

    /// `initializeNoiseField(x·4, 0, z·4, 5, 17, 5)`.
    fn noise_field(&self, x: i32, z: i32) -> Vec<f64> {
        self.noise_field_wd(x, z, 5, 5)
    }

    fn noise_field_wd(&self, x: i32, z: i32, w: i32, d: i32) -> Vec<f64> {
        let h = 17;
        let d0 = 684.412;
        let d1 = 2053.236;
        // noiseData4/5 (n6, n7) only feed d3/d5, which the density never reads
        let _ = (&self.n6, &self.n7);
        let nd1 = self
            .n3
            .noise(x, 0, z, w, h, d, d0 / 80.0, d1 / 60.0, d0 / 80.0);
        let nd2 = self.n1.noise(x, 0, z, w, h, d, d0, d1, d0);
        let nd3 = self.n2.noise(x, 0, z, w, h, d, d0, d1, d0);
        let mut out = vec![0.0; (w * h * d) as usize];
        let mut k1 = 0;
        for _ in 0..w {
            for _ in 0..d {
                for k2 in 0..h {
                    let d7 = self.col[k2 as usize];
                    let d8 = nd2[k1] / 512.0;
                    let d9 = nd3[k1] / 512.0;
                    let d10 = (nd1[k1] / 10.0 + 1.0) / 2.0;
                    let mut d6 = if d10 < 0.0 {
                        d8
                    } else if d10 > 1.0 {
                        d9
                    } else {
                        d8 + (d9 - d8) * d10
                    };
                    d6 -= d7;
                    if k2 > h - 4 {
                        let d11 = ((k2 - (h - 4)) as f32 / 3.0f32) as f64;
                        d6 = d6 * (1.0 - d11) + -10.0 * d11;
                    }
                    // `k2 < d4` with d4 = 0: never
                    out[k1] = d6;
                    k1 += 1;
                }
            }
        }
        out
    }

    /// Zoomed-out floor (see [`NetherChunk::floor`]) from the generator density: the lattice columns around
    /// (x, z), bilinear in x / z and linear in y like the generator; no surface pass, no caves. Columns with a zero
    /// weight are not computed.
    pub fn approx_floor(&self, x: i32, z: i32) -> (i32, bool) {
        let (rx, rz) = (x.rem_euclid(4), z.rem_euclid(4));
        let (w, d) = (1 + (rx != 0) as i32, 1 + (rz != 0) as i32);
        let f = self.noise_field_wd(x.div_euclid(4), z.div_euclid(4), w, d);
        floor_walk(&lattice_col(&f, d as usize, 0, 0, rx, rz))
    }

    /// [`Self::approx_floor`] at `x0 + i·stride`, `z0 + j·stride` (i < nx, j < nz; result index j·nx + i), with the
    /// lattice density computed once for the whole grid when that is cheaper than per sample.
    pub fn approx_floor_grid(&self, x0: i32, z0: i32, nx: i32, nz: i32, stride: i32) -> Vec<(i32, bool)> {
        let (la, lb) = (x0.div_euclid(4), (x0 + (nx - 1) * stride).div_euclid(4) + 1);
        let (ma, mb) = (z0.div_euclid(4), (z0 + (nz - 1) * stride).div_euclid(4) + 1);
        let (w, d) = (lb - la + 1, mb - ma + 1);
        let per_sample = (1 + (x0.rem_euclid(4) != 0 || stride % 4 != 0) as i32)
            * (1 + (z0.rem_euclid(4) != 0 || stride % 4 != 0) as i32);
        if (w * d) as i64 > (per_sample * nx * nz) as i64 {
            let mut out = Vec::with_capacity((nx * nz) as usize);
            for j in 0..nz {
                for i in 0..nx {
                    out.push(self.approx_floor(x0 + i * stride, z0 + j * stride));
                }
            }
            return out;
        }
        let f = self.noise_field_wd(la, ma, w, d);
        let mut out = Vec::with_capacity((nx * nz) as usize);
        for j in 0..nz {
            let z = z0 + j * stride;
            for i in 0..nx {
                let x = x0 + i * stride;
                let (ix, iz) = ((x.div_euclid(4) - la) as usize, (z.div_euclid(4) - ma) as usize);
                out.push(floor_walk(&lattice_col(&f, d as usize, ix, iz, x.rem_euclid(4), z.rem_euclid(4))));
            }
        }
        out
    }

    pub fn chunk(&self, cx: i32, cz: i32) -> NetherChunk {
        let mut blocks = Box::new([0u8; 32768]);
        let nf = self.noise_field(cx * 4, cz * 4);
        let (l, b2) = (5usize, 17usize);
        for i1 in 0..4usize {
            for j1 in 0..4usize {
                for k1 in 0..16usize {
                    let d0 = 0.125;
                    let mut d1 = nf[(i1 * l + j1) * b2 + k1];
                    let mut d2 = nf[(i1 * l + j1 + 1) * b2 + k1];
                    let mut d3 = nf[((i1 + 1) * l + j1) * b2 + k1];
                    let mut d4 = nf[((i1 + 1) * l + j1 + 1) * b2 + k1];
                    let d5 = (nf[(i1 * l + j1) * b2 + k1 + 1] - d1) * d0;
                    let d6 = (nf[(i1 * l + j1 + 1) * b2 + k1 + 1] - d2) * d0;
                    let d7 = (nf[((i1 + 1) * l + j1) * b2 + k1 + 1] - d3) * d0;
                    let d8 = (nf[((i1 + 1) * l + j1 + 1) * b2 + k1 + 1] - d4) * d0;
                    for l1 in 0..8usize {
                        let d9 = 0.25;
                        let mut d10 = d1;
                        let mut d11 = d2;
                        let d12 = (d3 - d1) * d9;
                        let d13 = (d4 - d2) * d9;
                        for i2 in 0..4usize {
                            let mut j2 = (i2 + i1 * 4) << 11 | (j1 * 4) << 7 | (k1 * 8 + l1);
                            let d14 = 0.25;
                            let mut d15 = d10;
                            let d16 = (d11 - d10) * d14;
                            for _ in 0..4 {
                                let mut b = nb::AIR;
                                if k1 * 8 + l1 < 32 {
                                    b = nb::LAVA;
                                }
                                if d15 > 0.0 {
                                    b = nb::ROCK;
                                }
                                blocks[j2] = b;
                                j2 += 128;
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
        self.replace(cx, cz, &mut blocks);
        self.carve_caves(cx, cz, &mut blocks);
        NetherChunk { blocks }
    }

    /// `replaceBlocksForBiome` (block classes: netherrack vs other solid surface blocks vs air/lava/bedrock).
    fn replace(&self, cx: i32, cz: i32, blocks: &mut [u8; 32768]) {
        let mut r = JavaRandom::new(
            (cx as i64)
                .wrapping_mul(341_873_128_712)
                .wrapping_add((cz as i64).wrapping_mul(132_897_987_541)),
        );
        let d0 = 0.03125;
        let slow = self.ssg.noise(cx * 16, cz * 16, 0, 16, 16, 1, d0, d0, 1.0);
        let grav = self
            .ssg
            .noise(cx * 16, 109, cz * 16, 16, 1, 16, d0, 1.0, d0);
        let excl = self
            .nex
            .noise(cx * 16, cz * 16, 0, 16, 16, 1, d0 * 2.0, d0 * 2.0, d0 * 2.0);
        let biomes = self.biomes.biomes(cx * 16, cz * 16, 16, 16);
        let b0 = 32;
        for k in 0..16usize {
            for l in 0..16usize {
                // l = x, k = z
                let biome = biomes[l + k * 16];
                let (top0, fill0) = match biome {
                    136 => (nb::SOUL_SAND, nb::SOUL_SAND), // Corrupted Sands
                    137 => (nb::ASH, nb::ASH),             // Phantasmagoric Inferno
                    117 => (nb::FLESH, nb::FLESH),         // Visceral Heap
                    94 => (nb::HARD_ICE, nb::HARD_ICE),    // Polar Chasm
                    138 => (nb::OVERGROWN, nb::ROCK), // Undergarden: overgrown netherrack / netherrack
                    _ => (nb::ROCK, nb::ROCK),        // Hell, Boneyard: netherrack
                };
                let flag = slow[k + l * 16] + r.next_double() * 0.2 > 0.0;
                let flag1 = grav[k + l * 16] + r.next_double() * 0.2 > 0.0;
                let i1 = (excl[k + l * 16] / 3.0 + 3.0 + r.next_double() * 0.25) as i32;
                let mut j1 = -1;
                let (mut b1, mut b2) = (top0, fill0);
                for k1 in (0..128i32).rev() {
                    let l1 = (l * 16 + k) * 128 + k1 as usize;
                    if k1 < 127 - r.next_int_bounded(5) && k1 > r.next_int_bounded(5) {
                        let b3 = blocks[l1];
                        if b3 == nb::AIR {
                            j1 = -1;
                        } else if b3 == nb::ROCK {
                            if j1 == -1 {
                                if i1 <= 0 {
                                    b1 = nb::AIR;
                                    b2 = nb::ROCK;
                                } else if (b0 - 4..=b0 + 1).contains(&k1) {
                                    b1 = nb::ROCK;
                                    b2 = nb::ROCK;
                                    if flag1 {
                                        b1 = nb::GRAVEL;
                                    }
                                    if flag {
                                        b1 = nb::SOUL_SAND;
                                        b2 = nb::SOUL_SAND;
                                    }
                                }
                                if k1 < b0 && b1 == nb::AIR {
                                    b1 = nb::LAVA;
                                }
                                j1 = i1;
                                blocks[l1] = if k1 >= b0 - 1 { b1 } else { b2 };
                            } else if j1 > 0 {
                                j1 -= 1;
                                blocks[l1] = b2;
                            }
                        }
                    } else {
                        blocks[l1] = nb::BEDROCK;
                    }
                }
            }
        }
    }

    /// `MapGenCavesHell` (range 8; Hodgepodge StdLCG for the per-chunk rand, vanilla `new Random(seed)` per tunnel).
    fn carve_caves(&self, cx: i32, cz: i32, blocks: &mut [u8; 32768]) {
        for sx in cx - 8..=cx + 8 {
            for sz in cz - 8..=cz + 8 {
                let mut r = self.caves.rand(sx, sz);
                let a = r.next_int_bounded(10) + 1;
                let b2 = r.next_int_bounded(a) + 1;
                let mut n = r.next_int_bounded(b2);
                if r.next_int_bounded(5) != 0 {
                    n = 0;
                }
                for _ in 0..n {
                    let x = (sx * 16 + r.next_int_bounded(16)) as f64;
                    let y = r.next_int_bounded(128) as f64;
                    let z = (sz * 16 + r.next_int_bounded(16)) as f64;
                    let mut k = 1;
                    if r.next_int_bounded(4) == 0 {
                        let s = r.next_long();
                        let w = 1.0 + r.next_float() * 6.0;
                        hell_tunnel(blocks, cx, cz, s, x, y, z, w, 0.0, 0.0, -1, -1, 0.5);
                        k += r.next_int_bounded(4);
                    }
                    for _ in 0..k {
                        let yaw = r.next_float() * PI_F * 2.0;
                        let pitch = (r.next_float() - 0.5) * 2.0 / 8.0;
                        let w = r.next_float() * 2.0 + r.next_float();
                        let s = r.next_long();
                        hell_tunnel(blocks, cx, cz, s, x, y, z, w * 2.0, yaw, pitch, 0, 0, 0.5);
                    }
                }
            }
        }
    }
}

const PI_F: f32 = std::f32::consts::PI;

/// `MathHelper` sine table.
fn sin_table() -> &'static [f32] {
    static T: std::sync::OnceLock<Vec<f32>> = std::sync::OnceLock::new();
    T.get_or_init(|| {
        (0..65536)
            .map(|i| (i as f64 * std::f64::consts::PI * 2.0 / 65536.0).sin() as f32)
            .collect()
    })
}
#[inline]
fn msin(f: f32) -> f32 {
    sin_table()[((f * 10430.378) as i32 & 65535) as usize]
}
#[inline]
fn mcos(f: f32) -> f32 {
    sin_table()[((f * 10430.378 + 16384.0) as i32 & 65535) as usize]
}

/// `MapGenBase.func_151539_a` seeding: `rand.setSeed(world)`, two longs, then per source chunk
/// `setSeed(x·l ^ z·l1 ^ world)`. `raw`: Hodgepodge's unscrambled StdLCG.
#[derive(Clone, Copy)]
struct CaveSeeds {
    seed: i64,
    raw: bool,
    l: i64,
    l1: i64,
}

impl CaveSeeds {
    fn new(seed: i64, raw: bool) -> Self {
        let mut c = CaveSeeds {
            seed,
            raw,
            l: 0,
            l1: 0,
        };
        let mut r = c.mk(seed);
        c.l = r.next_long();
        c.l1 = r.next_long();
        c
    }
    fn mk(&self, s: i64) -> JavaRandom {
        JavaRandom::new(if self.raw { s ^ 0x5_DEEC_E66D } else { s })
    }
    fn rand(&self, x: i32, z: i32) -> JavaRandom {
        self.mk((x as i64).wrapping_mul(self.l) ^ (z as i64).wrapping_mul(self.l1) ^ self.seed)
    }
}

#[inline]
fn floor_d(d: f64) -> i32 {
    floor_i(d)
}

#[allow(clippy::too_many_arguments)]
fn hell_tunnel(
    blocks: &mut [u8; 32768],
    cx: i32,
    cz: i32,
    seed: i64,
    mut x: f64,
    mut y: f64,
    mut z: f64,
    width: f32,
    mut yaw: f32,
    mut pitch: f32,
    mut step: i32,
    mut max: i32,
    yscale: f64,
) {
    let d4 = (cx * 16 + 8) as f64;
    let d5 = (cz * 16 + 8) as f64;
    let (mut f3, mut f4) = (0f32, 0f32);
    let mut r = JavaRandom::new(seed);
    if max <= 0 {
        let j = 8 * 16 - 16;
        max = j - r.next_int_bounded(j / 4);
    }
    let mut room = false;
    if step == -1 {
        step = max / 2;
        room = true;
    }
    let branch = r.next_int_bounded(max / 2) + max / 4;
    let steep = r.next_int_bounded(6) == 0;
    while step < max {
        let d6 = 1.5 + (msin(step as f32 * PI_F / max as f32) * width * 1.0) as f64;
        let d7 = d6 * yscale;
        let f5 = mcos(pitch);
        let f6 = msin(pitch);
        x += (mcos(yaw) * f5) as f64;
        y += f6 as f64;
        z += (msin(yaw) * f5) as f64;
        pitch *= if steep { 0.92 } else { 0.7 };
        pitch += f4 * 0.1;
        yaw += f3 * 0.1;
        f4 *= 0.9;
        f3 *= 0.75;
        f4 += (r.next_float() - r.next_float()) * r.next_float() * 2.0;
        f3 += (r.next_float() - r.next_float()) * r.next_float() * 4.0;
        if !room && step == branch && width > 1.0 {
            let s = r.next_long();
            let w = r.next_float() * 0.5 + 0.5;
            hell_tunnel(
                blocks,
                cx,
                cz,
                s,
                x,
                y,
                z,
                w,
                yaw - PI_F / 2.0,
                pitch / 3.0,
                step,
                max,
                1.0,
            );
            let s = r.next_long();
            let w = r.next_float() * 0.5 + 0.5;
            hell_tunnel(
                blocks,
                cx,
                cz,
                s,
                x,
                y,
                z,
                w,
                yaw + PI_F / 2.0,
                pitch / 3.0,
                step,
                max,
                1.0,
            );
            return;
        }
        if room || r.next_int_bounded(4) != 0 {
            let d8 = x - d4;
            let d9 = z - d5;
            let d10 = (max - step) as f64;
            let d11 = (width + 2.0 + 16.0) as f64;
            if d8 * d8 + d9 * d9 - d10 * d10 > d11 * d11 {
                return;
            }
            if x >= d4 - 16.0 - d6 * 2.0
                && z >= d5 - 16.0 - d6 * 2.0
                && x <= d4 + 16.0 + d6 * 2.0
                && z <= d5 + 16.0 + d6 * 2.0
            {
                let x1 = (floor_d(x - d6) - cx * 16 - 1).max(0);
                let x2 = (floor_d(x + d6) - cx * 16 + 1).min(16);
                let y1 = (floor_d(y - d7) - 1).max(1);
                let y2 = (floor_d(y + d7) + 1).min(120);
                let z1 = (floor_d(z - d6) - cz * 16 - 1).max(0);
                let z2 = (floor_d(z + d6) - cz * 16 + 1).min(16);
                let mut lava = false;
                let mut bx = x1;
                while !lava && bx < x2 {
                    let mut bz = z1;
                    while !lava && bz < z2 {
                        let mut by = y2 + 1;
                        while !lava && by >= y1 - 1 {
                            if (0..128).contains(&by) {
                                if blocks[((bx * 16 + bz) * 128 + by) as usize] == nb::LAVA {
                                    lava = true;
                                }
                                if by != y1 - 1
                                    && bx != x1
                                    && bx != x2 - 1
                                    && bz != z1
                                    && bz != z2 - 1
                                {
                                    by = y1;
                                }
                            }
                            by -= 1;
                        }
                        bz += 1;
                    }
                    bx += 1;
                }
                if !lava {
                    for bx in x1..x2 {
                        let ex = ((bx + cx * 16) as f64 + 0.5 - x) / d6;
                        for bz in z1..z2 {
                            let ez = ((bz + cz * 16) as f64 + 0.5 - z) / d6;
                            let mut i = (bx * 16 + bz) * 128 + y2;
                            let mut by = y2 - 1;
                            while by >= y1 {
                                let ey = (by as f64 + 0.5 - y) / d7;
                                if ey > -0.7
                                    && ex * ex + ey * ey + ez * ez < 1.0
                                    && blocks[i as usize] == nb::ROCK
                                {
                                    blocks[i as usize] = nb::AIR;
                                }
                                i -= 1;
                                by -= 1;
                            }
                        }
                    }
                    if room {
                        break;
                    }
                }
            }
        }
        step += 1;
    }
}

/// `NoiseGeneratorSimplex` (array form) for `NoiseGeneratorPerlin.func_151599_a`.
#[derive(Clone)]
struct Simplex {
    perm: [u16; 512],
    xo: f64,
    yo: f64,
}

const GRAD3: [[f64; 2]; 12] = [
    [1., 1.],
    [-1., 1.],
    [1., -1.],
    [-1., -1.],
    [1., 0.],
    [-1., 0.],
    [1., 0.],
    [-1., 0.],
    [0., 1.],
    [0., -1.],
    [0., 1.],
    [0., -1.],
];

impl Simplex {
    fn new(r: &mut JavaRandom) -> Self {
        let xo = r.next_double() * 256.0;
        let yo = r.next_double() * 256.0;
        let _zo = r.next_double() * 256.0;
        let mut perm = [0u16; 512];
        for (i, p) in perm.iter_mut().enumerate().take(256) {
            *p = i as u16;
        }
        for i in 0..256 {
            let j = r.next_int_bounded(256 - i as i32) as usize + i;
            perm.swap(i, j);
            perm[i + 256] = perm[i];
        }
        Simplex { perm, xo, yo }
    }

    #[allow(clippy::too_many_arguments)]
    fn add(&self, out: &mut [f64], x: f64, z: f64, w: i32, d: i32, sx: f64, sz: f64, amp: f64) {
        let sqrt3 = 3.0f64.sqrt();
        let f2 = 0.5 * (sqrt3 - 1.0);
        let g2 = (3.0 - sqrt3) / 6.0;
        let fl = |v: f64| if v > 0.0 { v as i32 } else { v as i32 - 1 };
        let p = |i: i32| self.perm[i as usize] as i32;
        let mut idx = 0;
        for iz in 0..d {
            let zz = (z + iz as f64) * sz + self.yo;
            for ix in 0..w {
                let xx = (x + ix as f64) * sx + self.xo;
                let s = (xx + zz) * f2;
                let i = fl(xx + s);
                let j = fl(zz + s);
                let t = (i + j) as f64 * g2;
                let x0 = xx - (i as f64 - t);
                let y0 = zz - (j as f64 - t);
                let (i1, j1) = if x0 > y0 { (1, 0) } else { (0, 1) };
                let x1 = x0 - i1 as f64 + g2;
                let y1 = y0 - j1 as f64 + g2;
                let x2 = x0 - 1.0 + 2.0 * g2;
                let y2 = y0 - 1.0 + 2.0 * g2;
                let ii = i & 255;
                let jj = j & 255;
                let gi0 = (p(ii + p(jj)) % 12) as usize;
                let gi1 = (p(ii + i1 + p(jj + j1)) % 12) as usize;
                let gi2 = (p(ii + 1 + p(jj + 1)) % 12) as usize;
                let corner = |t: f64, g: usize, a: f64, b: f64| {
                    if t < 0.0 {
                        0.0
                    } else {
                        let t = t * t;
                        t * t * (GRAD3[g][0] * a + GRAD3[g][1] * b)
                    }
                };
                let n0 = corner(0.5 - x0 * x0 - y0 * y0, gi0, x0, y0);
                let n1 = corner(0.5 - x1 * x1 - y1 * y1, gi1, x1, y1);
                let n2 = corner(0.5 - x2 * x2 - y2 * y2, gi2, x2, y2);
                out[idx] += 70.0 * (n0 + n1 + n2) * amp;
                idx += 1;
            }
        }
    }
}

/// `NoiseGeneratorPerlin` (octaves of simplex).
#[derive(Clone)]
struct PerlinOct {
    gens: Vec<Simplex>,
}

impl PerlinOct {
    fn new(r: &mut JavaRandom, n: usize) -> Self {
        PerlinOct {
            gens: (0..n).map(|_| Simplex::new(r)).collect(),
        }
    }
    /// `func_151599_a(null, x, z, w, d, sx, sz, p)` (= func_151600_a with lacunarity 0.5); index z·w + x.
    #[allow(clippy::too_many_arguments)]
    fn noise(&self, x: f64, z: f64, w: i32, d: i32, sx: f64, sz: f64, p12: f64) -> Vec<f64> {
        let mut out = vec![0.0; (w * d) as usize];
        let (mut a21, mut a18) = (1.0f64, 1.0f64);
        for g in &self.gens {
            g.add(
                &mut out,
                x,
                z,
                w,
                d,
                sx * a18 * a21,
                sz * a18 * a21,
                0.55 / a21,
            );
            a18 *= p12;
            a21 *= 0.5;
        }
        out
    }
}

// ============================================================================ Twilight Forest

/// TF block classes.
pub mod tb {
    /// `null` in the generator's block array (never set to air: some deform passes compare against air).
    pub const VOID: u8 = 0;
    pub const STONE: u8 = 1;
    pub const WATER: u8 = 2;
    pub const AIR: u8 = 3;
    pub const LEAVES: u8 = 4;
    pub const GRAVEL: u8 = 5;
    pub const ICE: u8 = 6;
    pub const PACKED_ICE: u8 = 7;
    /// dirt / grass (carved by caves)
    pub const GROUND: u8 = 8;
    /// deadrock (Thornlands / Final Plateau; not carved)
    pub const DEADROCK: u8 = 9;
    pub const BEDROCK: u8 = 10;
    /// grass (the surface pass's top; `GROUND` = dirt filler / cave lining)
    pub const GRASS: u8 = 11;

    /// Registry name of a TF block code.
    pub fn name(b: u8) -> &'static str {
        match b {
            VOID | AIR => "minecraft:air",
            STONE => "minecraft:stone",
            WATER => "minecraft:water",
            LEAVES => "TwilightForest:tile.DarkLeaves",
            GRAVEL => "minecraft:gravel",
            ICE => "minecraft:ice",
            PACKED_ICE => "minecraft:packed_ice",
            GROUND => "minecraft:dirt",
            DEADROCK => "TwilightForest:tile.TFDeadrock",
            BEDROCK => "minecraft:bedrock",
            GRASS => "minecraft:grass",
            _ => "?",
        }
    }

    /// Non-air, non-water (what the dump's "pre solid" mask counts).
    #[inline]
    pub fn solid(b: u8) -> bool {
        !matches!(b, VOID | AIR | WATER)
    }
}

/// `(rootHeight, heightVariation)` of the TF biomes (pack registry, f32), by `TfBiomeIds` field.
fn tf_heights(ids: &TfBiomeIds, id: i32) -> (f32, f32) {
    let t = ids;
    match id {
        x if x == t.clearing
            || x == t.dark_forest
            || x == t.dark_forest_center
            || x == t.deep_mushrooms
            || x == t.firefly_forest =>
        {
            (0.125, 0.05)
        }
        x if x == t.fire_swamp
            || x == t.glacier
            || x == t.mushrooms
            || x == t.twilight_forest
            || x == t.enchanted_forest =>
        {
            (0.1, 0.2)
        }
        x if x == t.highlands => (3.5, 0.05),
        x if x == t.highlands_center => (10.5, 0.025),
        x if x == t.lake => (-1.8, 0.1),
        x if x == t.oak_savanna || x == t.twilight_forest2 || x == t.snow => (0.2, 0.2),
        x if x == t.stream => (-0.5, 0.0),
        x if x == t.swamp => (-0.125, 0.125),
        x if x == t.thornlands => (6.0, 0.1),
        _ => (0.1, 0.2), // BiomeGenBase defaults
    }
}

pub struct TfTerrain {
    pub seed: i64,
    pub layers: TwilightForest,
    noise4: Octaves,
    noise6: Octaves,
    gj: Octaves,
    gk: Octaves,
    gl: Octaves,
    perlin: PerlinOct,
    parabolic: [f32; 25],
    caves: CaveSeeds,
    highlands: std::cell::RefCell<crate::jmath::FxMap<(i32, i32), bool>>,
}

/// One TF chunk after deformation: `blocks[(x * 16 + z) * 256 + y]`.
pub struct TfChunk {
    pub blocks: Box<[u8; 65536]>,
}

impl TfChunk {
    #[inline]
    pub fn at(&self, x: i32, y: i32, z: i32) -> u8 {
        if !(0..256).contains(&y) {
            return tb::AIR;
        }
        self.blocks[(((x & 15) * 16 + (z & 15)) * 256 + y) as usize]
    }
    /// Highest non-air block (water and leaves included; `heightValue − 1`).
    pub fn top(&self, x: i32, z: i32) -> i32 {
        let mut y = 255;
        while y > 0 && matches!(self.at(x, y, z), tb::AIR | tb::VOID) {
            y -= 1;
        }
        y
    }
    /// Highest ground block (no water, no canopy).
    pub fn ground(&self, x: i32, z: i32) -> i32 {
        let mut y = 255;
        while y > 0 && (!tb::solid(self.at(x, y, z)) || self.at(x, y, z) == tb::LEAVES) {
            y -= 1;
        }
        y
    }
}

const CH: usize = 256;

impl TfTerrain {
    pub fn new(seed: i64) -> Self {
        Self::with_layers(seed, TwilightForest::new(seed))
    }

    pub fn with_layers(seed: i64, layers: TwilightForest) -> Self {
        let mut r = JavaRandom::new(seed);
        let noise4 = Octaves::new(&mut r, 4);
        let _noise5 = Octaves::new(&mut r, 10);
        let noise6 = Octaves::new(&mut r, 16);
        let _mob = Octaves::new(&mut r, 8);
        let gj = Octaves::new(&mut r, 16);
        let gk = Octaves::new(&mut r, 16);
        let gl = Octaves::new(&mut r, 8);
        let perlin = PerlinOct::new(&mut r, 4);
        let mut parabolic = [0f32; 25];
        for j in -2i32..=2 {
            for k in -2i32..=2 {
                let f = 10.0f32 / (((j * j + k * k) as f32 + 0.2f32) as f64).sqrt() as f32;
                parabolic[(j + 2 + (k + 2) * 5) as usize] = f;
            }
        }
        TfTerrain {
            seed,
            layers,
            noise4,
            noise6,
            gj,
            gk,
            gl,
            perlin,
            parabolic,
            caves: CaveSeeds::new(seed, false), // TF sets MapGenBase.rand = FastRandom (scrambled)
            highlands: Default::default(),
        }
    }

    fn land_per_biome(&self, x: i32, z: i32, biomes: &[i32]) -> Vec<f64> {
        self.land_wd(x, z, 5, 5, biomes)
    }

    /// `makeLandPerBiome2` over a `w × d` lattice block; `biomes` = 1:4 cells `(w + 5) × (d + 5)` from (x − 2, z − 2)
    /// (stride `w + 5`, like the generator's 10 × 10).
    fn land_wd(&self, x: i32, z: i32, w: i32, d: i32, biomes: &[i32]) -> Vec<f64> {
        let ids = &self.layers.ids;
        let stride = w + 5;
        let n6 = self.noise6.noise2(x, z, w, d, 200.0, 200.0);
        let nd = self.gl.noise(
            x,
            0,
            z,
            w,
            33,
            d,
            8.555150000000001,
            4.277575000000001,
            8.555150000000001,
        );
        let ne = self.gj.noise(x, 0, z, w, 33, d, 684.412, 684.412, 684.412);
        let nf = self.gk.noise(x, 0, z, w, 33, d, 684.412, 684.412, 684.412);
        let mut out = vec![0.0; (w * d * 33) as usize];
        let (mut ti, mut ni) = (0usize, 0usize);
        for ax in 0..w {
            for az in 0..d {
                let (mut tv, mut th, mut tf) = (0f32, 0f32, 0f32);
                let centre = tf_heights(ids, biomes[(ax + 2 + (az + 2) * stride) as usize]).0;
                for ox in -2i32..=2 {
                    for oz in -2i32..=2 {
                        let (rh, hv) = tf_heights(
                            ids,
                            biomes[(ax + ox + 2 + (az + oz + 2) * stride) as usize],
                        );
                        let mut hf =
                            self.parabolic[(ox + 2 + (oz + 2) * 5) as usize] / (rh + 2.0f32);
                        if rh > centre {
                            hf /= 2.0f32;
                        }
                        tv += hv * hf;
                        th += rh * hf;
                        tf += hf;
                    }
                }
                tv /= tf;
                th /= tf;
                tv = tv * 0.9f32 + 0.1f32;
                th = (th * 4.0f32 - 1.0f32) / 8.0f32;
                let mut tn = n6[ni] / 8000.0;
                if tn < 0.0 {
                    tn = -tn * 0.3;
                }
                tn = tn * 3.0 - 2.0;
                if tn < 0.0 {
                    tn /= 2.0;
                    if tn < -1.0 {
                        tn = -1.0;
                    }
                    tn /= 1.4;
                    tn /= 2.0;
                } else {
                    if tn > 1.0 {
                        tn = 1.0;
                    }
                    tn /= 8.0;
                }
                ni += 1;
                let mut hc = th as f64;
                let vc = tv as f64;
                hc += tn * 0.2;
                hc = hc * 8.5 / 8.0;
                let d5 = 8.5 + hc * 4.0;
                for ay in 0..33i32 {
                    let mut d6 = (ay as f64 - d5) * 12.0 * 128.0 / 256.0 / vc;
                    if d6 < 0.0 {
                        d6 *= 4.0;
                    }
                    let d7 = ne[ti] / 512.0;
                    let d8 = nf[ti] / 512.0;
                    let d9 = (nd[ti] / 10.0 + 1.0) / 2.0;
                    let dc = if d9 < 0.0 {
                        d7
                    } else if d9 > 1.0 {
                        d8
                    } else {
                        d7 + (d8 - d7) * d9
                    };
                    let mut tc = dc - d6;
                    if ay > 29 {
                        let d11 = ((ay - 29) as f32 / 3.0f32) as f64;
                        tc = tc * (1.0 - d11) + -10.0 * d11;
                    }
                    out[ti] = tc;
                    ti += 1;
                }
            }
        }
        out
    }

    /// Zoomed-out ground height from the density column of the 4-block lattice point under (x, z) (biome blend,
    /// y interpolation and squish as in the generator; glaciers raised; no canopy, hills, surface or caves).
    pub fn approx_ground(&self, x: i32, z: i32) -> i32 {
        let (gx, gz) = (x.div_euclid(4), z.div_euclid(4));
        let q = self.layers.biomes_quarter(gx - 2, gz - 2, 6, 6);
        let col = self.land_wd(gx, gz, 1, 1, &q);
        let mut top = -1;
        for yy in (0..256i32).rev() {
            if yy % 2 == 0 {
                continue; // squish keeps odd y
            }
            let k = (yy / 8) as usize;
            let l = (yy % 8) as f64;
            if col[k] + (col[k + 1] - col[k]) * 0.125 * l > 0.0 {
                top = (yy - 1) / 2;
                break;
            }
        }
        if q[2 + 2 * 6] == self.layers.ids.glacier {
            top = (top + 33).min(127);
        }
        top
    }

    /// [`TfTerrain::chunk`] without the surface pass, caves and ravines (zoomed-in overview: same ground except
    /// where a cave opens the surface or the surface pass removes the top block).
    pub fn chunk_basic(&self, cx: i32, cz: i32) -> TfChunk {
        self.chunk_opts(cx, cz, false)
    }

    /// Generator array after `generateTerrain2` + `squishTerrain` (+ canopy, glaciers, feature deformation,
    /// surface, caves, ravines) = `provideChunk` before population.
    pub fn chunk(&self, cx: i32, cz: i32) -> TfChunk {
        self.chunk_opts(cx, cz, true)
    }

    fn chunk_opts(&self, cx: i32, cz: i32, full_pass: bool) -> TfChunk {
        let quarter = self.layers.biomes_quarter(cx * 4 - 2, cz * 4 - 2, 10, 10);
        let tc = self.land_per_biome(cx * 4, cz * 4, &quarter);
        // full-height density (only odd y survive the squish)
        let mut raw = vec![tb::VOID; 16 * 16 * CH];
        for k in 0..4usize {
            let l = k * 5;
            let i1 = (k + 1) * 5;
            for j1 in 0..4usize {
                let k1 = (l + j1) * 33;
                let l1 = (l + j1 + 1) * 33;
                let i2 = (i1 + j1) * 33;
                let j2 = (i1 + j1 + 1) * 33;
                for k2 in 0..32usize {
                    let d0 = 0.125;
                    let mut d1 = tc[k1 + k2];
                    let mut d2 = tc[l1 + k2];
                    let mut d3 = tc[i2 + k2];
                    let mut d4 = tc[j2 + k2];
                    let d5 = (tc[k1 + k2 + 1] - d1) * d0;
                    let d6 = (tc[l1 + k2 + 1] - d2) * d0;
                    let d7 = (tc[i2 + k2 + 1] - d3) * d0;
                    let d8 = (tc[j2 + k2 + 1] - d4) * d0;
                    for l2 in 0..8usize {
                        let d9 = 0.25;
                        let mut d10 = d1;
                        let mut d11 = d2;
                        let d12 = (d3 - d1) * d9;
                        let d13 = (d4 - d2) * d9;
                        for i3 in 0..4usize {
                            let mut j3 =
                                ((i3 + k * 4) << 12 | (j1 * 4) << 8 | (k2 * 8 + l2)) as isize - 256;
                            let d14 = 0.25;
                            let d16 = (d11 - d10) * d14;
                            let mut d15 = d10 - d16;
                            for _ in 0..4 {
                                j3 += 256;
                                d15 += d16;
                                raw[j3 as usize] = if d15 > 0.0 {
                                    tb::STONE
                                } else if k2 * 8 + l2 < 63 {
                                    tb::WATER
                                } else {
                                    tb::VOID
                                };
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
        // squish: index x*4096 | z*256 | y  ==  (x*16+z)*256 + y
        let mut b = Box::new([tb::AIR; 65536]);
        for col in 0..256usize {
            for y in 0..128usize {
                b[col * CH + y] = raw[col * CH + y * 2 + 1];
            }
        }
        self.canopy(cx, cz, &quarter, &mut b);
        let full = self.layers.biomes(cx * 16, cz * 16, 16, 16);
        self.glaciers(&full, &mut b);
        self.deform(cx, cz, &mut b);
        if full_pass {
            self.surface(cx, cz, &full, &mut b);
            self.carve_caves(cx, cz, &mut b);
            self.carve_ravines(cx, cz, &mut b);
        }
        TfChunk { blocks: b }
    }

    /// `TFFeature.getNearestFeature` + `getNearestCenter` (chunk-relative block centre).
    pub fn nearest_feature(&self, cx: i32, cz: i32) -> Option<(u8, i32, i32)> {
        for rad in 1..=3i32 {
            for x in -rad..=rad {
                for z in -rad..=rad {
                    if let Some(f) = self.layers.feature_at_chunk(x + cx, z + cz) {
                        if f.feature.size as i32 == rad {
                            return Some((f.feature.id, x * 16 + 8, z * 16 + 8));
                        }
                    }
                }
            }
        }
        None
    }

    /// `replaceBlocksForBiome` → `genTwilightBiomeTerrain` (sea level 32; no air reset of the filler depth).
    fn surface(&self, cx: i32, cz: i32, full: &[i32], b: &mut [u8; 65536]) {
        let ids = self.layers.ids;
        let mut r = JavaRandom::new(
            (cx as i64)
                .wrapping_mul(0x4f_9939_f508)
                .wrapping_add((cz as i64).wrapping_mul(0x1e_f156_5bd5)),
        );
        let d0 = 0.03125;
        let sn = self.perlin.noise(
            (cx * 16) as f64,
            (cz * 16) as f64,
            16,
            16,
            d0 * 2.0,
            d0 * 2.0,
            1.0,
        );
        let sea = 32;
        for z in 0..16usize {
            for x in 0..16usize {
                let biome = full[x + z * 16];
                let deadrock = biome == ids.thornlands || biome == ids.highlands_center;
                let cold = biome == ids.glacier || biome == ids.snow; // temperature 0.0 / 0.125 < 0.15
                let (btop, bfill) = if deadrock {
                    (tb::DEADROCK, tb::DEADROCK)
                } else {
                    (tb::GRASS, tb::GROUND)
                };
                let noise = sn[x + z * 16];
                let max_fill = (noise / 3.0 + 3.0 + r.next_double() * 0.25) as i32;
                // genTerrainBlocks(x = chunkX*16 + z, z = chunkZ*16 + x): index (maskZ*16 + maskX) = (x*16 + z)
                let col = (x * 16 + z) * CH;
                let (mut top, mut fill) = (btop, bfill);
                let mut depth = -1;
                for y in (0..256i32).rev() {
                    let i = col + y as usize;
                    if y <= r.next_int_bounded(5) {
                        b[i] = tb::BEDROCK;
                        continue;
                    }
                    let cur = b[i];
                    if cur != tb::STONE {
                        continue;
                    }
                    if deadrock {
                        b[i] = tb::DEADROCK;
                    }
                    if depth == -1 {
                        if max_fill <= 0 {
                            top = tb::VOID;
                            fill = tb::STONE;
                        } else if y >= sea - 5 && y <= sea {
                            top = btop;
                            fill = bfill;
                        }
                        if y < sea - 1 && matches!(top, tb::VOID | tb::AIR) {
                            top = if cold { tb::ICE } else { tb::WATER };
                        }
                        depth = max_fill;
                        if y >= sea - 2 {
                            b[i] = top;
                        } else if y < sea - 8 - max_fill {
                            top = tb::VOID;
                            fill = tb::STONE;
                            b[i] = tb::GRAVEL;
                        } else {
                            b[i] = fill;
                        }
                    } else if depth > 0 {
                        depth -= 1;
                        b[i] = fill;
                    }
                }
            }
        }
    }

    fn is_highlands(&self, sx: i32, sz: i32) -> bool {
        if let Some(&v) = self.highlands.borrow().get(&(sx, sz)) {
            return v;
        }
        let v = self.layers.biome_at(sx * 16, sz * 16) == self.layers.ids.highlands;
        self.highlands.borrow_mut().insert((sx, sz), v);
        v
    }

    /// `TFGenCaves` (range 8, 1 in 15 source chunks, up to 40 systems).
    fn carve_caves(&self, cx: i32, cz: i32, b: &mut [u8; 65536]) {
        for sx in cx - 8..=cx + 8 {
            for sz in cz - 8..=cz + 8 {
                let mut r = self.caves.rand(sx, sz);
                let a = r.next_int_bounded(40) + 1;
                let b2 = r.next_int_bounded(a) + 1;
                let mut n = r.next_int_bounded(b2);
                if r.next_int_bounded(15) != 0 {
                    n = 0;
                }
                if n == 0 {
                    continue;
                }
                let hl = self.is_highlands(sx, sz);
                for _ in 0..n {
                    let x = (sx * 16 + r.next_int_bounded(16)) as f64;
                    let yb = r.next_int_bounded(120) + 8;
                    let y = r.next_int_bounded(yb) as f64;
                    let z = (sz * 16 + r.next_int_bounded(16)) as f64;
                    let mut k = 1;
                    if r.next_int_bounded(4) == 0 {
                        let s = r.next_long();
                        let w = 1.0 + r.next_float() * 6.0;
                        tf_tunnel(b, cx, cz, s, x, y, z, w, 0.0, 0.0, -1, -1, 0.5, hl);
                        k += r.next_int_bounded(4);
                    }
                    for _ in 0..k {
                        let yaw = r.next_float() * PI_F * 2.0;
                        let pitch = (r.next_float() - 0.5) * 2.0 / 8.0;
                        let mut w = r.next_float() * 2.0 + r.next_float();
                        if r.next_int_bounded(10) == 0 {
                            w *= r.next_float() * r.next_float() * 3.0 + 1.0;
                        }
                        let s = r.next_long();
                        tf_tunnel(b, cx, cz, s, x, y, z, w, yaw, pitch, 0, 0, 1.0, hl);
                    }
                }
            }
        }
    }

    /// `TFGenRavine` (1 in 127 source chunks, only where the generated chunk's nearest feature allows decorations).
    fn carve_ravines(&self, cx: i32, cz: i32, b: &mut [u8; 65536]) {
        use crate::twilight::feature_id::*;
        let mut deco: Option<bool> = None;
        for sx in cx - 8..=cx + 8 {
            for sz in cz - 8..=cz + 8 {
                let mut r = self.caves.rand(sx, sz);
                if r.next_int_bounded(127) != 0 {
                    continue;
                }
                let ok = *deco.get_or_insert_with(|| {
                    matches!(
                        self.nearest_feature(cx, cz).map_or(NOTHING, |f| f.0),
                        NOTHING
                            | HILL1
                            | HILL2
                            | HILL3
                            | LABYRINTH
                            | KNIGHT_STRONGHOLD
                            | YETI_CAVE
                            | TROLL_CAVE
                    )
                });
                if !ok {
                    continue;
                }
                let x = (sx * 16 + r.next_int_bounded(16)) as f64;
                let yb = r.next_int_bounded(40) + 8;
                let y = (r.next_int_bounded(yb) + 20) as f64;
                let z = (sz * 16 + r.next_int_bounded(16)) as f64;
                let f = r.next_float() * TF_PI * 2.0;
                let f1 = ((r.next_float() - 0.5) * 2.0) / 8.0;
                let f2 = (r.next_float() * 2.0 + r.next_float()) * 2.0;
                let s = r.next_long();
                tf_ravine(b, cx, cz, s, x, y, z, f2, f, f1, 0, 0, 3.0);
            }
        }
    }

    fn canopy(&self, cx: i32, cz: i32, q: &[i32], b: &mut [u8; 65536]) {
        let ids = &self.layers.ids;
        let mut thicks = [0i32; 25];
        for z in 0..5i32 {
            for x in 0..5i32 {
                for bx in -1..=1 {
                    for bz in -1..=1 {
                        let id = q[(x + bx + 2 + (z + bz + 2) * 10) as usize];
                        if id == ids.dark_forest || id == ids.dark_forest_center {
                            thicks[(x + z * 5) as usize] += 1;
                        }
                    }
                }
            }
        }
        if thicks.iter().all(|&t| t == 0) {
            return;
        }
        let near = self.nearest_feature(cx, cz);
        let dark_tower = near.is_some_and(|n| n.0 == crate::twilight::feature_id::DARK_TOWER);
        let (hx, hz) = if dark_tower {
            near.map(|n| (n.1, n.2)).unwrap()
        } else {
            (0, 0)
        };
        let d = 0.03125;
        let sn = self
            .noise4
            .noise(cx * 16, cz * 16, 0, 16, 16, 1, d * 2.0, d * 2.0, d * 2.0);
        for z in 0..16i32 {
            for x in 0..16i32 {
                let qx = (x / 4) as usize;
                let qz = (z / 4) as usize;
                let xw = (x % 4) as f32 * 0.25f32 + 0.125f32;
                let zw = (z % 4) as f32 * 0.25f32 + 0.125f32;
                let mut th = 0f32;
                th += thicks[qx + qz * 5] as f32 * (1f32 - xw) * (1f32 - zw);
                th += thicks[qx + 1 + qz * 5] as f32 * xw * (1f32 - zw);
                th += thicks[qx + (qz + 1) * 5] as f32 * (1f32 - xw) * zw;
                th += thicks[qx + 1 + (qz + 1) * 5] as f32 * xw * zw;
                th -= 4f32;
                if dark_tower {
                    let dx = x - hx;
                    let dz = z - hz;
                    let dist = ((dx * dx + dz * dz) as f64).sqrt() as i32;
                    if dist < 24 {
                        th -= (24 - dist) as f32;
                    }
                }
                if th > 1f32 {
                    let col = ((x * 16 + z) as usize) * CH;
                    let mut top = -1;
                    for y in (0..128usize).rev() {
                        let c = b[col + y];
                        if c == tb::WATER {
                            break;
                        }
                        if c == tb::STONE {
                            top = y as i32;
                            break;
                        }
                    }
                    if top != -1 {
                        let noise =
                            3.min((sn[(z & 15 | (x & 15) << 4) as usize] / 1.25f32 as f64) as i32);
                        let mut bottom = top + 12 - (th * 0.5f32) as i32;
                        let tp = bottom + (th * 1.5f32) as i32;
                        bottom -= noise;
                        for y in bottom..tp {
                            if (0..CH as i32).contains(&y) {
                                b[col + y as usize] = tb::LEAVES;
                            }
                        }
                    }
                }
            }
        }
    }

    fn glaciers(&self, full: &[i32], b: &mut [u8; 65536]) {
        let g = self.layers.ids.glacier;
        for z in 0..16usize {
            for x in 0..16usize {
                if full[x | z << 4] != g {
                    continue;
                }
                let col = (x * 16 + z) * CH;
                let mut top: i32 = -1;
                for y in (0..128usize).rev() {
                    if b[col + y] == tb::STONE {
                        top = y as i32;
                        b[col + y] = tb::GRAVEL;
                        break;
                    }
                }
                let gtop = top + 32 + 1;
                let mut y = top + 1;
                while y <= gtop && y < 128 {
                    b[col + y as usize] = tb::ICE;
                    y += 1;
                }
            }
        }
    }

    fn deform(&self, cx: i32, cz: i32, b: &mut [u8; 65536]) {
        use crate::twilight::feature_id::*;
        let Some((fid, hx, hz)) = self.nearest_feature(cx, cz) else {
            return;
        };
        let f = TF_FEATURES[fid as usize];
        let altered = matches!(
            fid,
            HILL1
                | HILL2
                | HILL3
                | HEDGE_MAZE
                | NAGA_COURTYARD
                | QUEST_GROVE
                | HYDRA_LAIR
                | YETI_CAVE
                | TROLL_CAVE
        );
        if !altered {
            return;
        }
        // troll clouds (deformTerrainForTrollCloud2) use World.rand: not modelled
        for x in 0..16i32 {
            for z in 0..16i32 {
                let dx = x - hx;
                let dz = z - hz;
                let col = ((x * 16 + z) as usize) * CH;
                match fid {
                    HILL1 | HILL2 | HILL3 | HYDRA_LAIR => {
                        let hdiam = (f.size as i32 * 2 + 1) * 16;
                        let dist = ((dx * dx + dz * dz) as f64).sqrt() as i32;
                        let hh = (((dist as f32 / hdiam as f32) as f64 * std::f64::consts::PI)
                            .cos()
                            * (hdiam as f32 / 3f32) as f64) as i32;
                        raise_hills(b, col, f.size as i32, fid == HYDRA_LAIR, hdiam, dx, dz, hh);
                    }
                    HEDGE_MAZE | NAGA_COURTYARD | QUEST_GROVE => {
                        flatten(b, col, f.size as i32, dx, dz)
                    }
                    YETI_CAVE => yeti(b, col, f.size as i32, dx, dz),
                    _ => {}
                }
            }
        }
    }
}

const SEA: i32 = 31;

/// TF's ravine literal `3.141593F` (one f32 ulp above `f32::consts::PI`).
#[allow(clippy::approx_constant)]
const TF_PI: f32 = 3.141593;

#[allow(clippy::too_many_arguments)]
fn raise_hills(
    b: &mut [u8; 65536],
    col: usize,
    size: i32,
    hydra: bool,
    hdiam: i32,
    dx: i32,
    dz: i32,
    hh: i32,
) {
    let mut new_ground = -1;
    let mut found = false;
    for y in 0..CH as i32 {
        let i = col + y as usize;
        if b[i] != tb::STONE && !found {
            new_ground = y + hh;
            found = true;
        }
        if found && y <= new_ground {
            b[i] = tb::STONE;
        }
    }
    let mut hollow = hh - 4 - size;
    if hydra {
        let mx = dx + 16;
        let mz = dz + 16;
        let mdist = ((mx * mx + mz * mz) as f64).sqrt() as i32;
        let mh = ((mdist as f64 / (hdiam as f64 / 1.5) * std::f64::consts::PI).cos()
            * (hdiam as f64 / 1.5)) as i32;
        hollow = (mh - 4).max(hollow);
    }
    if hollow < 0 {
        hollow = 0;
    }
    for y in 0..CH as i32 {
        let i = col + y as usize;
        if hh > 0 && y < SEA && b[i] != tb::STONE {
            b[i] = tb::STONE;
        }
        let floor = if hydra { SEA } else { SEA - 3 - hollow / 8 };
        if y > floor && y < floor + hollow {
            b[i] = tb::AIR;
        }
    }
}

/// Java `int += float` (`i = (int)(i + f)`).
#[inline]
fn iadd_f(i: i32, f: f32) -> i32 {
    (i as f32 + f) as i32
}

fn squish_factor(size: i32, dx: i32, dz: i32) -> f32 {
    let bnd = (size * 2 + 1) * 8 - 8;
    let mut sq = 0f32;
    if dx <= -bnd {
        sq = (-dx - bnd) as f32 / 8.0f32;
    }
    if dx >= bnd {
        sq = (dx - bnd) as f32 / 8.0f32;
    }
    if dz <= -bnd {
        sq = sq.max((-dz - bnd) as f32 / 8.0f32);
    }
    if dz >= bnd {
        sq = sq.max((dz - bnd) as f32 / 8.0f32);
    }
    sq
}

fn first_non_stone(b: &[u8; 65536], col: usize) -> Option<i32> {
    (0..=127).find(|&y| b[col + y as usize] != tb::STONE)
}

fn flatten(b: &mut [u8; 65536], col: usize, size: i32, dx: i32, dz: i32) {
    let mut maze = SEA + 1;
    let sq = squish_factor(size, dx, dz);
    if sq > 0.0 {
        if let Some(old) = first_non_stone(b, col) {
            maze = iadd_f(maze, (old - maze) as f32 * sq);
        }
    }
    for y in 0..=127i32 {
        let i = col + y as usize;
        if y < maze && (b[i] == tb::AIR || b[i] == tb::WATER) {
            b[i] = tb::STONE;
        }
        if y >= maze && b[i] != tb::WATER {
            b[i] = tb::AIR;
        }
    }
}

fn yeti(b: &mut [u8; 65536], col: usize, size: i32, dx: i32, dz: i32) {
    let mut top_h = SEA + 24;
    let sq = squish_factor(size, dx, dz);
    let cave = (size * 2) * 8 - 8;
    let offset = dx.abs().min(dz.abs());
    let mut ceil = (SEA + 40) - offset * 4;
    if dx >= -cave && dz >= -cave && dx <= cave && dz <= cave {
        ceil = SEA + 16;
    }
    ceil -= offset / 6;
    ceil = ceil.min(SEA + 16);
    let mut floor = SEA - 1 + offset / 6;
    if sq > 0.0 {
        if let Some(old) = first_non_stone(b, col) {
            top_h = iadd_f(top_h, (old - top_h) as f32 * sq);
            floor = iadd_f(floor, (old - floor) as f32 * sq);
        }
    }
    for y in 0..=127i32 {
        let i = col + y as usize;
        if y < top_h && matches!(b[i], tb::VOID | tb::AIR | tb::WATER) {
            b[i] = tb::STONE;
        }
        if y > floor && y < ceil {
            b[i] = tb::AIR;
        }
        if y == floor && y < ceil && y < SEA + 3 {
            b[i] = tb::PACKED_ICE;
        }
    }
}

#[inline]
fn tf_carvable(b: u8) -> bool {
    matches!(b, tb::STONE | tb::GROUND | tb::GRASS)
}

#[allow(clippy::too_many_arguments)]
fn tf_tunnel(
    b: &mut [u8; 65536],
    cx: i32,
    cz: i32,
    seed: i64,
    mut x: f64,
    mut y: f64,
    mut z: f64,
    mut size: f32,
    mut yaw: f32,
    mut pitch: f32,
    mut step: i32,
    mut end: i32,
    yscale: f64,
    hl: bool,
) {
    let ocx = (cx * 16 + 8) as f64;
    let ocz = (cz * 16 + 8) as f64;
    let (mut v23, mut v24) = (0f32, 0f32);
    let mut r = JavaRandom::new(seed);
    if hl && size < 6.0 {
        size *= 2.5;
    }
    if end <= 0 {
        let j = 8 * 16 - 16;
        end = j - r.next_int_bounded(j / 4);
    }
    let mut stop = false;
    if step == -1 {
        step = end / 2;
        stop = true;
    }
    let branch = r.next_int_bounded(end / 2) + end / 4;
    let steep = r.next_int_bounded(6) == 0;
    while step < end {
        let sv = 1.5 + (msin(step as f32 * PI_F / end as f32) * size * 1.0) as f64;
        let ss = sv * yscale;
        let ca = mcos(pitch);
        let sa = msin(pitch);
        x += (mcos(yaw) * ca) as f64;
        y += sa as f64;
        z += (msin(yaw) * ca) as f64;
        pitch *= if steep { 0.92 } else { 0.7 };
        pitch += v24 * 0.1;
        yaw += v23 * 0.1;
        v24 *= 0.9;
        v23 *= 0.75;
        v24 += (r.next_float() - r.next_float()) * r.next_float() * 2.0;
        v23 += (r.next_float() - r.next_float()) * r.next_float() * 4.0;
        if !stop && step == branch && size > 1.0 && end > 0 {
            let sa_ = r.next_long();
            let wa = r.next_float() * 0.5 + 0.5;
            let saved = r.clone();
            tf_tunnel(
                b,
                cx,
                cz,
                sa_,
                x,
                y,
                z,
                wa,
                yaw - PI_F / 2.0,
                pitch / 3.0,
                step,
                end,
                1.0,
                hl,
            );
            r = saved;
            let sb = r.next_long();
            let wb = r.next_float() * 0.5 + 0.5;
            tf_tunnel(
                b,
                cx,
                cz,
                sb,
                x,
                y,
                z,
                wb,
                yaw + PI_F / 2.0,
                pitch / 3.0,
                step,
                end,
                1.0,
                hl,
            );
            return;
        }
        if stop || r.next_int_bounded(4) != 0 {
            let dx = x - ocx;
            let dz = z - ocz;
            let left = (end - step) as f64;
            let reach = (size + 2.0 + 16.0) as f64;
            if dx * dx + dz * dz - left * left > reach * reach {
                return;
            }
            if x >= ocx - 16.0 - sv * 2.0
                && z >= ocz - 16.0 - sv * 2.0
                && x <= ocx + 16.0 + sv * 2.0
                && z <= ocz + 16.0 + sv * 2.0
            {
                let min_x = (floor_d(x - sv) - cx * 16 - 1).max(0);
                let max_x = (floor_d(x + sv) - cx * 16 + 1).min(16);
                let max_y = (floor_d(y - ss) - 1).max(1);
                let min_y = (floor_d(y + ss) + 1).min(120);
                let min_z = (floor_d(z - sv) - cz * 16 - 1).max(0);
                let max_z = (floor_d(z + sv) - cz * 16 + 1).min(16);
                let mut water = false;
                let mut gx = min_x;
                while !water && gx < max_x {
                    let mut gz = min_z;
                    while !water && gz < max_z {
                        let mut gy = min_y + 1;
                        while !water && gy >= max_y - 1 {
                            if (0..128).contains(&gy) {
                                // TF bug: 128-high index into the 256-high array
                                let wi = ((gx * 16 + gz) * 128 + gy) as usize;
                                if b[wi] == tb::WATER {
                                    water = true;
                                }
                                if gy != max_y - 1
                                    && gx != min_x
                                    && gx != max_x - 1
                                    && gz != min_z
                                    && gz != max_z - 1
                                {
                                    gy = max_y;
                                }
                            }
                            gy -= 1;
                        }
                        gz += 1;
                    }
                    gx += 1;
                }
                if !water {
                    for gx in min_x..max_x {
                        let ex = ((gx + cx * 16) as f64 + 0.5 - x) / sv;
                        for gz in min_z..max_z {
                            let ez = ((gz + cz * 16) as f64 + 0.5 - z) / sv;
                            if ex * ex + ez * ez >= 1.0 {
                                continue;
                            }
                            let mut i = ((gx * 16 + gz) * 256 + min_y) as usize;
                            let mut cy = min_y - 1;
                            // `hitGrass`: a dug grass top makes the lining grass and the dirt below it the biome top
                            let mut hit_grass = false;
                            while cy >= max_y {
                                let ey = (cy as f64 + 0.5 - y) / ss;
                                let dd = ex * ex + ey * ey + ez * ez;
                                if ey > -0.7 && dd < 20.0 {
                                    if b[i] == tb::GRASS {
                                        hit_grass = true;
                                    }
                                    if tf_carvable(b[i]) {
                                        if dd < 0.85 {
                                            b[i] = if cy < 10 { tb::WATER } else { tb::AIR };
                                        } else {
                                            b[i] = if hit_grass {
                                                tb::GRASS
                                            } else if hl {
                                                tb::STONE
                                            } else {
                                                tb::GROUND
                                            };
                                            hit_grass = false;
                                        }
                                        if hit_grass && i > 0 && b[i - 1] == tb::GROUND {
                                            b[i - 1] = tb::GRASS;
                                        }
                                    }
                                }
                                i -= 1;
                                cy -= 1;
                            }
                        }
                    }
                    if stop {
                        break;
                    }
                }
            }
        }
        step += 1;
    }
}

#[allow(clippy::too_many_arguments)]
fn tf_ravine(
    b: &mut [u8; 65536],
    i: i32,
    j: i32,
    seed: i64,
    mut d: f64,
    mut d1: f64,
    mut d2: f64,
    f: f32,
    mut f1: f32,
    mut f2: f32,
    mut k: i32,
    mut i1: i32,
    d3: f64,
) {
    let mut r = JavaRandom::new(seed);
    let d4 = (i * 16 + 8) as f64;
    let d5 = (j * 16 + 8) as f64;
    let (mut f3, mut f4) = (0f32, 0f32);
    if i1 <= 0 {
        let j1 = 8 * 16 - 16;
        i1 = j1 - r.next_int_bounded(j1 / 4);
    }
    let mut flag = false;
    if k == -1 {
        k = i1 / 2;
        flag = true;
    }
    let mut f5 = 1.0f32;
    let mut mul = [0f32; 256];
    for (k1, m) in mul.iter_mut().enumerate() {
        if k1 == 0 || r.next_int_bounded(3) == 0 {
            f5 = 1.0 + r.next_float() * r.next_float() * 1.0;
        }
        *m = f5 * f5;
    }
    while k < i1 {
        let mut d6 = 1.5 + (msin((k as f32 * TF_PI) / i1 as f32) * f * 1.0) as f64;
        let mut d7 = d6 * d3;
        d6 *= r.next_float() as f64 * 0.25 + 0.75;
        d7 *= r.next_float() as f64 * 0.25 + 0.75;
        let f6 = mcos(f2);
        let f7 = msin(f2);
        d += (mcos(f1) * f6) as f64;
        d1 += f7 as f64;
        d2 += (msin(f1) * f6) as f64;
        f2 *= 0.7;
        f2 += f4 * 0.05;
        f1 += f3 * 0.05;
        f4 *= 0.8;
        f3 *= 0.5;
        f4 += (r.next_float() - r.next_float()) * r.next_float() * 2.0;
        f3 += (r.next_float() - r.next_float()) * r.next_float() * 4.0;
        if !flag && r.next_int_bounded(4) == 0 {
            k += 1;
            continue;
        }
        let d8a = d - d4;
        let d9a = d2 - d5;
        let d10a = (i1 - k) as f64;
        let d11 = (f + 2.0 + 16.0) as f64;
        if (d8a * d8a + d9a * d9a) - d10a * d10a > d11 * d11 {
            return;
        }
        if d < d4 - 16.0 - d6 * 2.0
            || d2 < d5 - 16.0 - d6 * 2.0
            || d > d4 + 16.0 + d6 * 2.0
            || d2 > d5 + 16.0 + d6 * 2.0
        {
            k += 1;
            continue;
        }
        let x0 = (floor_d(d - d6) - i * 16 - 1).max(0);
        let x1 = (floor_d(d + d6) - i * 16 + 1).min(16);
        let y0 = (floor_d(d1 - d7) - 1).max(1);
        let y1 = (floor_d(d1 + d7) + 1).min(256 - 8);
        let z0 = (floor_d(d2 - d6) - j * 16 - 1).max(0);
        let z1 = (floor_d(d2 + d6) - j * 16 + 1).min(16);
        let mut water = false;
        let mut k2 = x0;
        while !water && k2 < x1 {
            let mut i3 = z0;
            while !water && i3 < z1 {
                let mut j3 = y1 + 1;
                while !water && j3 >= y0 - 1 {
                    if (0..256).contains(&j3) {
                        if b[((k2 * 16 + i3) * 256 + j3) as usize] == tb::WATER {
                            water = true;
                        }
                        if j3 != y0 - 1 && k2 != x0 && k2 != x1 - 1 && i3 != z0 && i3 != z1 - 1 {
                            j3 = y0;
                        }
                    }
                    j3 -= 1;
                }
                i3 += 1;
            }
            k2 += 1;
        }
        if water {
            k += 1;
            continue;
        }
        for l2 in x0..x1 {
            let d12 = (((l2 + i * 16) as f64) + 0.5 - d) / d6;
            for l3 in z0..z1 {
                let d13 = (((l3 + j * 16) as f64) + 0.5 - d2) / d6;
                if d12 * d12 + d13 * d13 >= 1.0 {
                    continue;
                }
                let mut i4 = ((l2 * 16 + l3) * 256 + y1) as usize;
                let mut j4 = y1 - 1;
                // vanilla `MapGenRavine`: a dug grass top turns the dirt below it into the biome top
                let mut found_top = false;
                while j4 >= y0 {
                    let d14 = ((j4 as f64) + 0.5 - d1) / d7;
                    if (d12 * d12 + d13 * d13) * mul[j4 as usize] as f64 + (d14 * d14) / 6.0 < 1.0 {
                        if b[i4] == tb::GRASS {
                            found_top = true;
                        }
                        if tf_carvable(b[i4]) {
                            b[i4] = tb::AIR;
                            if found_top && b[i4 - 1] == tb::GROUND {
                                b[i4 - 1] = tb::GRASS;
                            }
                        }
                    }
                    i4 -= 1;
                    j4 -= 1;
                }
            }
        }
        if flag {
            break;
        }
        k += 1;
    }
}

/// Density column at (ix + rx/4, iz + rz/4) of a lattice field laid out `(ix·d + iz)·17 + k` (bilinear; a column
/// with zero weight is not read).
fn lattice_col(f: &[f64], d: usize, ix: usize, iz: usize, rx: i32, rz: i32) -> [f64; 17] {
    let (fx, fz) = (rx as f64 * 0.25, rz as f64 * 0.25);
    let at = |a: usize, b: usize, k: usize| f[(a * d + b) * 17 + k];
    let mut col = [0.0; 17];
    for (k, c) in col.iter_mut().enumerate() {
        let a = at(ix, iz, k);
        let z0 = if rx != 0 { a + (at(ix + 1, iz, k) - a) * fx } else { a };
        *c = if rz != 0 {
            let b = at(ix, iz + 1, k);
            let z1 = if rx != 0 { b + (at(ix + 1, iz + 1, k) - b) * fx } else { b };
            z0 + (z1 - z0) * fz
        } else {
            z0
        };
    }
    col
}

/// The [`NetherChunk::floor`] walk on one density column (linear in y between the 8-block lattice rows).
fn floor_walk(col: &[f64; 17]) -> (i32, bool) {
    let dens = |y: i32| {
        let k = (y / 8) as usize;
        let l = (y % 8) as f64;
        col[k] + (col[k + 1] - col[k]) * 0.125 * l
    };
    let solid = |y: i32| y <= 4 || y >= 123 || dens(y) > 0.0;
    let mut y = 110;
    while y > 0 && solid(y) {
        y -= 1;
    }
    while y > 0 && !solid(y) && y >= 32 {
        y -= 1;
    }
    if y < 32 && !solid(y) {
        return (31, true);
    }
    (y, false)
}
