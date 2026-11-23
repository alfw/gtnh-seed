//! Port of `ChunkGeneratorRealistic.getNewNoise` (stride 1) plus the chunk biome-array assembly of
//! `provideChunk` (`baseBiomesList` + `markLavaCaveOpeningBiome`).
//!
//! This is what the world actually stores: the blended terrain height (`testHeight`, the column is stone up to
//! `(int) height` before carving/caves) and the per-column biome, which near borders is a dithered pick over the
//! blended biome weights rather than `getBiomeGenAt`.

use crate::continental;
use crate::continental::NONE;
use crate::noise::NoiseGenerator as _;
use crate::provider::{
    lava_cave_is_marker, lava_cave_surface_height, BiomeId, BiomeProvider, Heights,
};
use crate::terrain::{volcano_inside_lava_fill, volcano_r_noise_at};

const SAMPLE_SIZE: i32 = 8;
const SAMPLE_ARRAY: usize = (SAMPLE_SIZE * 2 + 5) as usize; // 21
const PARABOLIC_SIZE: i32 = SAMPLE_SIZE;
const PARABOLIC_ARRAY: usize = (PARABOLIC_SIZE * 2 + 1) as usize; // 17

/// One generated chunk.
#[derive(Clone)]
pub struct Chunk {
    /// `testHeight`, index `lz * 16 + lx` (note: Java stores `[lx * 16 + lz]`).
    pub height: [f32; 256],
    /// `biomesForGeneration` (RWG id), index `lz * 16 + lx`.
    pub rwg: [BiomeId; 256],
    /// Chunk biome array (base biome id, lava-cave marker applied), index `lz * 16 + lx`.
    pub gen: [u16; 256],
    /// `carveMountainChainRivers` per column (index `lz * 16 + lx`): blocks `floor..=ceil` become water (≤ 62)
    /// or air, plus the chamber opening above. Runs before the surface pass.
    pub river_cave: [RiverCave; 256],
    /// `volcanoSurfaceDepth` (index `lz * 16 + lx`): ash depth where the volcano paints (`lavaBasin ? 6 :
    /// min(127, addedBlocks)`), else 0.
    pub volcano_depth: [u8; 256],
    /// `getLavaCaveCoordinates` per column (index `lz * 16 + lx`; [`NONE`] in a volcano's vicinity or when not
    /// continental), as the generator read it.
    pub lava_cave: [i64; 256],
}

/// One column of `ChunkGeneratorRealistic.carveMountainChainRivers` (RWG2 mountain-chain river tunnels and
/// junction chambers). `floor > ceil`: nothing carved.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct RiverCave {
    pub floor: i32,
    pub ceil: i32,
    /// `chamberStrength` (opening above the cave where `>= 0.70 + 0.15 * heightFraction`).
    pub chamber: f32,
    /// `(int) testHeight` (min 255).
    pub surface: i32,
}

impl RiverCave {
    pub const NONE: RiverCave = RiverCave {
        floor: 1,
        ceil: 0,
        chamber: 0.0,
        surface: 0,
    };

    #[inline]
    pub fn is_none(&self) -> bool {
        self.floor > self.ceil
    }

    /// Block `y` was set by the carve (water if `y <= 62`, else air).
    pub fn carved(&self, y: i32) -> bool {
        if self.floor > self.ceil {
            return false;
        }
        if y >= self.floor.max(1) && y <= self.ceil.min(255) {
            return true;
        }
        if self.chamber > 0.70 && self.surface > self.ceil {
            let bottom = (self.ceil + 1).max(63);
            if y >= bottom && y <= self.surface {
                let frac = (y - bottom) as f32 / (self.surface - bottom).max(1) as f32;
                return self.chamber >= 0.70 + frac * 0.15;
            }
        }
        false
    }
}

#[inline]
fn smoothstep(v: f32) -> f32 {
    let v = 0f32.max(1f32.min(v));
    v * v * (3.0 - 2.0 * v)
}

#[inline]
fn smootherstep(v: f32) -> f32 {
    let v = 0f32.max(1f32.min(v));
    v * v * v * (v * (v * 6.0 - 15.0) + 10.0)
}

/// Java `Math.round(float)` (= floor(x + 1/2) evaluated exactly).
#[inline]
fn jround(x: f32) -> i32 {
    (x as f64 + 0.5).floor() as i32
}

const MOUNTAIN_CHAIN_INFLUENCE_RADIUS: f32 = 48.0;

/// Reusable buffers (the Java generator keeps them as fields).
pub struct ChunkGen {
    n: usize,
    parabolic: [f32; PARABOLIC_ARRAY * PARABOLIC_ARRAY],
    biome_data: [BiomeId; SAMPLE_ARRAY * SAMPLE_ARRAY],
    flags: Vec<bool>,
    active: Vec<BiomeId>,
    huge: Vec<f32>,  // 81 * n
    small: Vec<f32>, // 625 * n
    mountain: Vec<bool>,
    volcano_class: Vec<bool>,
    /// Per RWG id: `SurfaceDuneValley(valley, mix)` as the biome's first surface (its sand top sets the chunk biome
    /// array to `RWGBiomes.baseHotDesert`), plus that gen id.
    dune: Vec<Option<(f32, bool)>>,
    hot_desert: Option<u16>,
    /// 8-block biome samples by grid point (after the volcano replacement): `biome_data` is pure, and neighbouring
    /// chunks share most of their 21×21 window, so scattered chunk generation (structure checks) reuses them.
    /// Tagged with the provider/heights it was filled for.
    samples: crate::jmath::FxMap<(i32, i32), BiomeId>,
    samples_tag: (u64, [usize; 2]),
}

/// Sample cache bound (entries; ~24 B each with the hash table: ~3 MB).
const SAMPLE_CACHE_MAX: usize = 1 << 17;

/// [`ChunkGen::sample_window_grid`]: biome samples of a chunk rect.
pub struct SampleGrid {
    cx0: i32,
    cz0: i32,
    w: i32,
    gen: Vec<u16>,
    /// Ids an override can put in any chunk biome array (volcano surface, lava-cave marker).
    pub extra: Vec<u16>,
}

impl SampleGrid {
    /// Every in-game id chunk `(cx, cz)`'s biome array can hold: its 21×21 blend window plus [`Self::extra`].
    pub fn chunk_ids(&self, cx: i32, cz: i32, mut f: impl FnMut(u16)) {
        let (oi, oj) = ((cx - self.cx0) * 2, (cz - self.cz0) * 2);
        for j in 0..SAMPLE_ARRAY as i32 {
            let row = ((oj + j) * self.w + oi) as usize;
            for &g in &self.gen[row..row + SAMPLE_ARRAY] {
                f(g);
            }
        }
        for &g in &self.extra {
            f(g);
        }
    }
}

impl ChunkGen {
    /// The 8-block biome samples every chunk of `[cx0, cx1] × [cz0, cz1]` blends from, as in-game ids (volcano
    /// islands replaced by the biome they sit on, like [`ChunkGen::generate`]). One `biome_data` per 8×8 blocks
    /// of the rect plus its 80-block margin: far cheaper than generating the chunks.
    pub fn sample_window_grid(
        p: &BiomeProvider,
        h: &dyn Heights,
        cx0: i32,
        cz0: i32,
        cx1: i32,
        cz1: i32,
    ) -> SampleGrid {
        let volcano: Vec<bool> = p
            .table
            .biomes
            .iter()
            .map(|b| b.class.ends_with(".RealisticBiomeIslandVolcano"))
            .collect();
        let mut extra: Vec<u16> = p
            .table
            .biomes
            .iter()
            .enumerate()
            .filter(|(i, _)| volcano[*i])
            .map(|(_, b)| b.gen_id)
            .collect();
        if let Some(m) = p.table.lava_cave_marker_gen_id {
            extra.push(m);
        }
        // chunk x0 = cx * 16 samples x0 + i * 8 - 8 for i in -SAMPLE_SIZE..SAMPLE_SIZE + 5
        let lo = -SAMPLE_SIZE * 8 - 8;
        let (x0, z0) = (
            cx0.wrapping_mul(16).wrapping_add(lo),
            cz0.wrapping_mul(16).wrapping_add(lo),
        );
        let w = (cx1 - cx0) * 2 + SAMPLE_ARRAY as i32;
        let hh = (cz1 - cz0) * 2 + SAMPLE_ARRAY as i32;
        let mut gen = Vec::with_capacity((w * hh) as usize);
        for j in 0..hh {
            for i in 0..w {
                let (x, z) = (x0.wrapping_add(i * 8), z0.wrapping_add(j * 8));
                let mut b = p.biome_data(x, z, h).data;
                if volcano[b as usize] {
                    if let Some(u) = p.volcano_underlying_biome(x, z) {
                        b = u;
                    }
                }
                gen.push(p.table.biomes[b as usize].gen_id);
            }
        }
        SampleGrid {
            cx0,
            cz0,
            w,
            gen,
            extra,
        }
    }

    pub fn new(p: &BiomeProvider) -> Self {
        let n = p.table.biomes.len().max(1);
        let mut parabolic = [0f32; PARABOLIC_ARRAY * PARABOLIC_ARRAY];
        let mut total = 0f32;
        for j in -PARABOLIC_SIZE..=PARABOLIC_SIZE {
            for k in -PARABOLIC_SIZE..=PARABOLIC_SIZE {
                let f = 0.445f32 / (((j * j + k * k) as f32 + 0.3f32) as f64).sqrt() as f32;
                parabolic[(j + PARABOLIC_SIZE) as usize
                    + (k + PARABOLIC_SIZE) as usize * PARABOLIC_ARRAY] = f;
                total += f;
            }
        }
        for v in parabolic.iter_mut() {
            *v /= total;
        }
        let mountain = p
            .table
            .biomes
            .iter()
            .map(|b| b.class.ends_with(".RealisticBiomeMountainChain"))
            .collect();
        let volcano_class = p
            .table
            .biomes
            .iter()
            .map(|b| b.class.ends_with(".RealisticBiomeIslandVolcano"))
            .collect();
        let sm = crate::spawn::SurfaceModel::new(&p.table);
        let dune = sm
            .by_id
            .iter()
            .map(|si| match si.surf {
                crate::spawn::Surf::DuneValley { valley, mix, .. } if !si.guessed => {
                    Some((valley, mix))
                }
                _ => None,
            })
            .collect();
        let hot_desert = p
            .table
            .biomes
            .iter()
            .find(|b| b.class.ends_with(".RealisticBiomeDesert"))
            .map(|b| b.gen_id);
        ChunkGen {
            n,
            parabolic,
            biome_data: [0; SAMPLE_ARRAY * SAMPLE_ARRAY],
            flags: vec![false; n],
            active: Vec::with_capacity(64),
            huge: vec![0.0; 81 * n],
            small: vec![0.0; 625 * n],
            mountain,
            volcano_class,
            dune,
            hot_desert,
            samples: Default::default(),
            samples_tag: (0, [0, 0]),
        }
    }

    #[inline]
    fn mix4_huge(&mut self, a: usize, b: usize, c: usize, d: usize, out: usize) {
        let n = self.n;
        for &id in &self.active {
            let id = id as usize;
            let v = (self.huge[a * n + id]
                + self.huge[b * n + id]
                + self.huge[c * n + id]
                + self.huge[d * n + id])
                * 0.25f32;
            self.huge[out * n + id] = v;
        }
    }

    #[inline]
    fn mix4_small(&mut self, a: usize, b: usize, c: usize, d: usize, out: usize) {
        let n = self.n;
        for &id in &self.active {
            let id = id as usize;
            let v = (self.small[a * n + id]
                + self.small[b * n + id]
                + self.small[c * n + id]
                + self.small[d * n + id])
                * 0.25f32;
            self.small[out * n + id] = v;
        }
    }

    /// Number of distinct biomes in the last chunk's 21x21 sample window.
    pub fn last_active_count(&self) -> usize {
        self.active.len()
    }

    /// `getNewNoise(cmr, cx*16, cz*16, biomes)` + chunk biome array. `None` if the table has no height models.
    #[allow(clippy::manual_clamp)] // Java ternaries, kept verbatim (NaN semantics)
    pub fn generate(
        &mut self,
        p: &BiomeProvider,
        h: &dyn Heights,
        cx: i32,
        cz: i32,
    ) -> Option<Chunk> {
        if p.table.heights.is_empty() {
            return None;
        }
        let x = cx.wrapping_mul(16);
        let y = cz.wrapping_mul(16);
        let n = self.n;

        // ---- biome samples (8-block grid), volcano island replaced by what it sits on
        for f in self.flags.iter_mut() {
            *f = false;
        }
        // heights identity: data pointer + vtable (zero-sized impls share a data address; a duplicated vtable only
        // costs a spurious clear)
        // SAFETY: a `*const dyn` is two pointer-sized words; only compared, never turned back into a pointer.
        let hid: [usize; 2] = unsafe {
            std::mem::transmute::<*const dyn Heights, [usize; 2]>(h as *const dyn Heights)
        };
        let tag = (p.uid(), hid);
        if self.samples_tag != tag || self.samples.len() > SAMPLE_CACHE_MAX {
            self.samples.clear();
            self.samples_tag = tag;
        }
        for i in -SAMPLE_SIZE..SAMPLE_SIZE + 5 {
            for j in -SAMPLE_SIZE..SAMPLE_SIZE + 5 {
                let sx = x.wrapping_add(i * 8 - 8);
                let sz = y.wrapping_add(j * 8 - 8);
                let volcano_class = &self.volcano_class;
                let b = *self.samples.entry((sx >> 3, sz >> 3)).or_insert_with(|| {
                    let mut b = p.biome_data(sx, sz, h).data;
                    if volcano_class[b as usize] {
                        if let Some(u) = p.volcano_underlying_biome(sx, sz) {
                            b = u;
                        }
                    }
                    b
                });
                self.biome_data
                    [(i + SAMPLE_SIZE) as usize * SAMPLE_ARRAY + (j + SAMPLE_SIZE) as usize] = b;
                self.flags[b as usize] = true;
            }
        }
        self.active.clear();
        for (id, &f) in self.flags.iter().enumerate() {
            if f {
                self.active.push(id as BiomeId);
            }
        }

        // ---- parabolic weights at the 5x5 coarse points (hugeRender even cells)
        for i in -1..4i32 {
            for j in -1..4i32 {
                let cell = ((i * 2 + 2) * 9 + (j * 2 + 2)) as usize;
                for &id in &self.active {
                    self.huge[cell * n + id as usize] = 0.0;
                }
                for k in -PARABOLIC_SIZE..=PARABOLIC_SIZE {
                    for l in -PARABOLIC_SIZE..=PARABOLIC_SIZE {
                        let b = self.biome_data[(i + k + SAMPLE_SIZE + 1) as usize * SAMPLE_ARRAY
                            + (j + l + SAMPLE_SIZE + 1) as usize]
                            as usize;
                        let w = self.parabolic[(k + PARABOLIC_SIZE) as usize
                            + (l + PARABOLIC_SIZE) as usize * PARABOLIC_ARRAY];
                        self.huge[cell * n + b] += w;
                    }
                }
            }
        }

        // ---- main biome check
        let mut main: Option<BiomeId> = None;
        for &id in &self.active {
            if self.huge[(4 * 9 + 4) * n + id as usize] > 0.95f32 {
                main = Some(id);
            }
        }

        // ---- HUGE 1
        for i in 0..4usize {
            for j in 0..4usize {
                self.mix4_huge(
                    (i * 2) * 9 + (j * 2),
                    (i * 2 + 2) * 9 + (j * 2),
                    (i * 2) * 9 + (j * 2 + 2),
                    (i * 2 + 2) * 9 + (j * 2 + 2),
                    (i * 2 + 1) * 9 + (j * 2 + 1),
                );
            }
        }
        // ---- HUGE 2 (huge -> small coarse grid; mixed-parity cells are only ever computed here)
        for i in 0..7usize {
            for j in 0..7usize {
                let out = (i * 4) * 25 + (j * 4);
                if !(i % 2 == 0 && j % 2 == 0) && !(i % 2 != 0 && j % 2 != 0) {
                    let (a, b, c, d) = (
                        i * 9 + (j + 1),
                        (i + 1) * 9 + j,
                        (i + 1) * 9 + (j + 2),
                        (i + 2) * 9 + (j + 1),
                    );
                    for &id in &self.active {
                        let id = id as usize;
                        let v = (self.huge[a * n + id]
                            + self.huge[b * n + id]
                            + self.huge[c * n + id]
                            + self.huge[d * n + id])
                            * 0.25f32;
                        self.small[out * n + id] = v;
                    }
                } else {
                    let src = (i + 1) * 9 + (j + 1);
                    for &id in &self.active {
                        let id = id as usize;
                        self.small[out * n + id] = self.huge[src * n + id];
                    }
                }
            }
        }
        // ---- SMALL 1
        for i in 0..6usize {
            for j in 0..6usize {
                self.mix4_small(
                    (i * 4) * 25 + (j * 4),
                    (i * 4 + 4) * 25 + (j * 4),
                    (i * 4) * 25 + (j * 4 + 4),
                    (i * 4 + 4) * 25 + (j * 4 + 4),
                    (i * 4 + 2) * 25 + (j * 4 + 2),
                );
            }
        }
        // ---- SMALL 2
        for i in 0..11usize {
            for j in 0..11usize {
                if !(i % 2 == 0 && j % 2 == 0) && !(i % 2 != 0 && j % 2 != 0) {
                    self.mix4_small(
                        (i * 2) * 25 + (j * 2 + 2),
                        (i * 2 + 2) * 25 + (j * 2),
                        (i * 2 + 2) * 25 + (j * 2 + 4),
                        (i * 2 + 4) * 25 + (j * 2 + 2),
                        (i * 2 + 2) * 25 + (j * 2 + 2),
                    );
                }
            }
        }
        // ---- SMALL 3
        for i in 0..9usize {
            for j in 0..9usize {
                self.mix4_small(
                    (i * 2 + 2) * 25 + (j * 2 + 2),
                    (i * 2 + 4) * 25 + (j * 2 + 2),
                    (i * 2 + 2) * 25 + (j * 2 + 4),
                    (i * 2 + 4) * 25 + (j * 2 + 4),
                    (i * 2 + 3) * 25 + (j * 2 + 3),
                );
            }
        }
        // ---- SMALL 4
        for i in 0..16usize {
            for j in 0..16usize {
                if !(i % 2 == 0 && j % 2 == 0) && !(i % 2 != 0 && j % 2 != 0) {
                    self.mix4_small(
                        (i + 3) * 25 + (j + 4),
                        (i + 4) * 25 + (j + 3),
                        (i + 4) * 25 + (j + 5),
                        (i + 5) * 25 + (j + 4),
                        (i + 4) * 25 + (j + 4),
                    );
                }
            }
        }

        // ---- columns
        let mut out = Chunk {
            height: [0.0; 256],
            rwg: [0; 256],
            gen: [0; 256],
            river_cave: [RiverCave::NONE; 256],
            volcano_depth: [0; 256],
            lava_cave: [NONE; 256],
        };
        // nearbyMountainChainInfluence: mountain-chain samples of the 21x21 window (local block offsets)
        let mut mountain_samples: Vec<(f32, f32)> = Vec::new();
        for sx in 0..SAMPLE_ARRAY as i32 {
            for sz in 0..SAMPLE_ARRAY as i32 {
                if self.mountain
                    [self.biome_data[(sx as usize) * SAMPLE_ARRAY + sz as usize] as usize]
                {
                    mountain_samples.push((
                        ((sx - SAMPLE_SIZE) * 8 - 8) as f32,
                        ((sz - SAMPLE_SIZE) * 8 - 8) as f32,
                    ));
                }
            }
        }
        let mut host_col = [0f32; 256];
        let mut bed_col = [0f32; 256];
        let rand_biome = main.is_none();
        if let Some(b) = main {
            out.rwg = [b; 256];
        }
        let continental = p.is_continental();
        let volcano = p.table.volcano_island;
        // per column (z * 16 + x): riverStrength and continentValues, read by replaceBlocksForBiome's river rule
        let mut river_col = [0f32; 256];
        let mut inland_col = [true; 256];
        for i in 0..16i32 {
            for j in 0..16i32 {
                let wx = x.wrapping_add(i);
                let wz = y.wrapping_add(j);
                let col = (j * 16 + i) as usize;
                let mut b_count = 0f32;
                let mut b_rand = 0f32;
                if rand_biome {
                    b_rand = 0.5f32 + p.perlin.noise2(wx as f32 / 15.0, wz as f32 / 15.0);
                    b_rand = if b_rand < 0.0 {
                        0.0
                    } else if b_rand > 0.99999f32 {
                        0.99999f32
                    } else {
                        b_rand
                    };
                }
                let ocean = if continental {
                    let c = p.continent_value(wx, wz);
                    inland_col[col] = c >= 24.0;
                    let v = 1.0f32 + c / 100.0f32;
                    if v < 0.0 {
                        0.0
                    } else if v > 2.0 {
                        2.0
                    } else {
                        v
                    }
                } else {
                    p.terrain_ocean_value(wx, wz)
                };
                let l = ((i + 4) * 25 + (j + 4)) as usize;
                let mut th = 0f32;
                let mut mountain = 0f32;
                let river = p.river_strength(wx, wz);
                river_col[col] = river;
                for &id in &self.active {
                    let w = self.small[l * n + id as usize];
                    if w > 0.0 {
                        if rand_biome && b_count <= 1.0 {
                            b_count += w;
                            if b_count > b_rand {
                                out.rwg[col] = id;
                                b_count = 2.0;
                            }
                        }
                        let bh = p.table.heights[id as usize].r_noise(
                            &p.perlin,
                            &p.cell,
                            wx,
                            wz,
                            ocean,
                            w,
                            river + 1.0,
                        );
                        th += bh * w;
                        if self.mountain[id as usize] {
                            mountain += w;
                        }
                    }
                }
                let mut vicinity = NONE;
                if let Some(vid) = volcano {
                    vicinity = p.volcano_vicinity_coordinates(wx, wz, h);
                    if vicinity != NONE {
                        let underlying = th;
                        let (lx, lz) = (
                            continental::unpack_x(vicinity),
                            continental::unpack_y(vicinity),
                        );
                        let base = p.volcano_base_height(wx, wz, h)?;
                        let vh = volcano_r_noise_at(&p.perlin, lx, lz, base, underlying);
                        th = vh;
                        let added = (vh as i32).wrapping_sub(underlying as i32);
                        let basin = volcano_inside_lava_fill(&p.perlin, lx, lz);
                        let ash_blend = 0f32.max(1f32.min((added as f32 - 1.0) / 4.0));
                        let ash_noise =
                            0f32.max(1f32.min(
                                0.5 + p.perlin.noise2(wx as f32 / 12.0, wz as f32 / 12.0) * 0.5,
                            ));
                        let surface = added >= 5 || added > 0 && ash_blend > ash_noise;
                        if (surface || basin) && p.volcano_coordinates(wx, wz, h) != NONE {
                            out.rwg[col] = vid;
                            out.volcano_depth[col] = if basin { 6 } else { added.min(127) as u8 };
                        }
                    }
                }
                if continental && vicinity == NONE {
                    let cave = p.lava_cave_coordinates(wx, wz, h);
                    out.lava_cave[col] = cave;
                    if cave != NONE {
                        th = lava_cave_surface_height(
                            &p.perlin,
                            continental::unpack_x(cave),
                            continental::unpack_y(cave),
                            th,
                        );
                    }
                }
                let mut near = 0f32;
                let r2 = MOUNTAIN_CHAIN_INFLUENCE_RADIUS * MOUNTAIN_CHAIN_INFLUENCE_RADIUS;
                for &(sx, sz) in &mountain_samples {
                    let (dx, dz) = (sx - i as f32, sz - j as f32);
                    let d2 = dx * dx + dz * dz;
                    if d2 >= r2 {
                        continue;
                    }
                    near =
                        near.max(1.0 - (d2 as f64).sqrt() as f32 / MOUNTAIN_CHAIN_INFLUENCE_RADIUS);
                }
                host_col[col] = mountain.max(near);
                let uncarved = th;
                let carved = p.calculate_river(wx, wz, river, uncarved);
                bed_col[col] = carved;
                let mut fade = 0f32.max(1f32.min((mountain - 0.35) / 0.65));
                fade = fade * fade * (3.0 - 2.0 * fade);
                out.height[col] = carved + (uncarved - carved) * fade;
            }
        }

        // ---- carveMountainChainRivers
        for col in 0..256usize {
            let mw = host_col[col];
            if mw <= 0.10 {
                continue;
            }
            let wx = x.wrapping_add((col & 15) as i32);
            let wz = y.wrapping_add((col >> 4) as i32);
            let tunnel = p.river_tunnel_strength(wx, wz);
            let junction = p.river_junction_strength(wx, wz);
            if tunnel <= 0.0 && junction <= 0.0 {
                continue;
            }
            let surface = 255.min(out.height[col] as i32);
            let host = smoothstep((mw - 0.10) / 0.40);
            let ts = tunnel * host;
            let curve = (0f32.max(ts * (2.0 - ts)) as f64).sqrt() as f32;
            let normal_floor = 62.min((bed_col[col] as f64).floor() as i32 + 1);
            let tunnel_floor = normal_floor.min(62 - jround(curve * 4.0));
            let tunnel_ceil = 62 + jround(curve * 11.0);
            let (mut floor, mut ceil) = (tunnel_floor, tunnel_ceil);
            let overhead = smoothstep((surface as f32 - 76.0) / 24.0);
            let chamber = junction * host * overhead;
            if chamber > 0.0 {
                let cc = (chamber as f64).sqrt() as f32;
                floor = floor.min(63 - jround(cc * 23.0));
                ceil = ceil.max(63 + jround(cc * 42.0));
                ceil = ceil.min(surface - 10);
                ceil = ceil.max(tunnel_ceil);
            }
            out.river_cave[col] = RiverCave {
                floor,
                ceil,
                chamber,
                surface,
            };
        }

        // ---- chunk biome array (baseBiomesList) + markLavaCaveOpeningBiome
        for idx in 0..256usize {
            out.gen[idx] = p.table.biomes[out.rwg[idx] as usize].gen_id;
        }
        // replaceBlocksForBiome: inland river columns take the biome's riverBiome
        // (`inland && river > 0.05 && river + perlin(x/10, z/10)·0.15 > 0.8`, river = −riverStrength)
        for idx in 0..256usize {
            let river = -river_col[idx];
            // RealisticBiomeMountainChain.rReplace then writes base = backingBiome.baseBiome again (undoes it)
            if inland_col[idx] && river > 0.05 && !self.mountain[out.rwg[idx] as usize] {
                let bx = x.wrapping_add((idx & 15) as i32);
                let bz = y.wrapping_add((idx >> 4) as i32);
                if river + p.perlin.noise2(bx as f32 / 10.0, bz as f32 / 10.0) * 0.15 > 0.8 {
                    out.gen[idx] = p.table.biomes[out.rwg[idx] as usize].river_gen_id;
                }
            }
            // then rReplace: SurfaceDuneValley paints a sand top (top stone k = (int) height) and writes
            // base = RWGBiomes.baseHotDesert
            if let (Some((valley, mix)), Some(desert)) = (
                self.dune.get(out.rwg[idx] as usize).copied().flatten(),
                self.hot_desert,
            ) {
                let k = out.height[idx] as i32;
                if k >= 0 {
                    let bx = x.wrapping_add((idx & 15) as i32) as f32;
                    let bz = y.wrapping_add((idx >> 4) as i32) as f32;
                    let mut hh = (p.perlin.noise2(bx / valley, bz / valley) + 0.25) * 65.0;
                    if hh < 1.0 {
                        hh = 1.0;
                    }
                    let m = p.perlin.noise2(bx / 12.0, bz / 12.0);
                    if k as f32 > 90.0 + p.perlin.noise2(bx / 24.0, bz / 24.0) * 10.0 - hh
                        || (m < -0.28 && mix)
                    {
                        out.gen[idx] = desert;
                    }
                }
            }
        }
        if continental {
            if let Some(marker) = p.table.lava_cave_marker_gen_id {
                for idx in 0..256usize {
                    // read in the column loop (NONE in a volcano's vicinity)
                    let cave = out.lava_cave[idx];
                    if cave != NONE
                        && lava_cave_is_marker(
                            &p.perlin,
                            continental::unpack_x(cave),
                            continental::unpack_y(cave),
                        )
                    {
                        out.gen[idx] = marker;
                    }
                }
            }
        }
        Some(out)
    }
}

/// Zoomed-out terrain, part 1 (per sample, to be blurred): the column's own biome height with the chunk generator's
/// inputs (continental ocean value, `border = 1`, river strength) and its mountain-chain weight (0/1). Mirrors
/// `getNewNoise` with a single biome; [`approx_finish`] applies what the generator does after the blend.
pub fn approx_parts(p: &BiomeProvider, h: &dyn Heights, x: i32, z: i32) -> Option<(f32, f32)> {
    let mut id = p.biome_data(x, z, h).data;
    if Some(id) == p.table.volcano_island {
        if let Some(u) = p.volcano_underlying_biome(x, z) {
            id = u;
        }
    }
    let ocean = if p.is_continental() {
        (1.0f32 + p.continent_value(x, z) / 100.0f32).clamp(0.0, 2.0)
    } else {
        p.terrain_ocean_value(x, z)
    };
    let river = p.river_strength(x, z);
    let m = p.table.heights.get(id as usize)?;
    let th = m.r_noise(&p.perlin, &p.cell, x, z, ocean, 1.0, river + 1.0);
    let mountain = p
        .table
        .biomes
        .get(id as usize)
        .is_some_and(|b| b.class.ends_with(".RealisticBiomeMountainChain"));
    Some((th, if mountain { 1.0 } else { 0.0 }))
}

/// Zoomed-out terrain, part 2: volcano / lava-cave overrides, river carving and the mountain fade on a blended
/// `(height, mountain weight)`.
pub fn approx_finish(
    p: &BiomeProvider,
    h: &dyn Heights,
    x: i32,
    z: i32,
    mut th: f32,
    mountain: f32,
) -> f32 {
    let mut vicinity = NONE;
    if p.table.volcano_island.is_some() {
        vicinity = p.volcano_vicinity_coordinates(x, z, h);
        if vicinity != NONE {
            if let Some(base) = p.volcano_base_height(x, z, h) {
                th = volcano_r_noise_at(
                    &p.perlin,
                    continental::unpack_x(vicinity),
                    continental::unpack_y(vicinity),
                    base,
                    th,
                );
            }
        }
    }
    if p.is_continental() && vicinity == NONE {
        let cave = p.lava_cave_coordinates(x, z, h);
        if cave != NONE {
            th = lava_cave_surface_height(
                &p.perlin,
                continental::unpack_x(cave),
                continental::unpack_y(cave),
                th,
            );
        }
    }
    let river = p.river_strength(x, z);
    let carved = p.calculate_river(x, z, river, th);
    let mut fade = 0f32.max(1f32.min((mountain - 0.35) / 0.65));
    fade = fade * fade * (3.0 - 2.0 * fade);
    carved + (th - carved) * fade
}

/// One column of `LavaCaveLandmark.generate` (runs after the surface pass and caves, on continental worlds):
/// the chamber / tunnel carve and the surface funnel. Built by [`lava_cave_column`].
#[derive(Clone, Copy, Debug)]
pub struct LavaColumn {
    /// Carved `bottom..=top` (lava up to y 25, obsidian boulders in the main chamber, else air).
    pub bottom: i32,
    pub top: i32,
    main: bool,
    wx: f32,
    wz: f32,
    distance: f32,
    surface: i32,
}

impl LavaColumn {
    /// `(top, surface)` when the surface funnel runs in this column (`distance < 28`, `surface ≥ 63`, above `top`).
    pub fn funnel(&self) -> Option<(i32, i32)> {
        (self.distance < 28.0 && self.surface >= 63 && self.surface > self.top)
            .then_some((self.top, self.surface))
    }

    /// Block `y` is air after the landmark (`falling_above`: the block above `top` is sand/gravel, which turns
    /// `top` into stone).
    pub fn air(&self, y: i32, falling_above: bool) -> bool {
        if self.distance < 28.0
            && self.surface >= 63
            && self.surface > self.top
            && y > self.top
            && y <= self.surface
        {
            let height = 1.max(self.surface - self.top);
            let vertical = (y - self.top) as f32 / height as f32;
            let r = if vertical < 0.58 {
                28.0 + (8.0 - 28.0) * smootherstep(vertical / 0.58)
            } else {
                8.0 + (14.0 - 8.0) * smootherstep((vertical - 0.58) / 0.42)
            };
            if self.distance < r {
                return true;
            }
        }
        if y < self.bottom || y > self.top || self.top < self.bottom {
            return false;
        }
        if y == self.top && falling_above {
            return false;
        }
        if self.main && lava_boulder(self.wx, self.wz, y) {
            return false;
        }
        y > 25
    }
}

/// What `LavaCaveLandmark` leaves at one block of its column.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LavaBlock {
    /// Not touched (the block before the landmark stays).
    Keep,
    Air,
    Lava,
    Obsidian,
    Stone,
}

impl LavaColumn {
    /// Block `y` after the landmark ([`LavaColumn::air`] plus what the carve leaves solid).
    pub fn block(&self, y: i32, falling_above: bool) -> LavaBlock {
        if self.air(y, falling_above) {
            return LavaBlock::Air;
        }
        if y < self.bottom || y > self.top || self.top < self.bottom {
            return LavaBlock::Keep;
        }
        if y == self.top && falling_above {
            return LavaBlock::Stone;
        }
        if self.main && lava_boulder(self.wx, self.wz, y) {
            return LavaBlock::Obsidian;
        }
        LavaBlock::Lava
    }
}

fn lava_ellipsoid(x: f32, z: f32, y: f32, r: f32, h: f32) -> bool {
    (x * x + z * z) / (r * r) + y * y / (h * h) <= 1.0
}

fn lava_boulder(x: f32, z: f32, y: i32) -> bool {
    let y = y as f32;
    lava_ellipsoid(x + 19.0, z - 7.0, y - 26.0, 7.0, 6.0)
        || lava_ellipsoid(x - 14.0, z + 18.0, y - 25.0, 6.0, 5.0)
        || lava_ellipsoid(x - 27.0, z - 11.0, y - 24.0, 5.0, 6.0)
        || lava_ellipsoid(x + 5.0, z + 25.0, y - 25.0, 4.5, 5.0)
}

/// `LavaCaveLandmark.carveColumn` for world column (x, z) with `surface` = highest non-air block after caves
/// (`findSurface`). `None` outside lava caves or where nothing is carved.
pub fn lava_cave_column(
    p: &BiomeProvider,
    h: &dyn Heights,
    x: i32,
    z: i32,
    surface: i32,
) -> Option<LavaColumn> {
    if !p.is_continental() || p.volcano_vicinity_coordinates(x, z, h) != NONE {
        return None;
    }
    lava_cave_column_at(p, p.lava_cave_coordinates(x, z, h), x, z, surface)
}

/// [`lava_cave_column`] with the column's `getLavaCaveCoordinates` already known ([`Chunk::lava_cave`]).
pub fn lava_cave_column_at(
    p: &BiomeProvider,
    packed: i64,
    x: i32,
    z: i32,
    surface: i32,
) -> Option<LavaColumn> {
    const MAIN_RADIUS: f32 = 88.0;
    const NETWORK_RADIUS: f32 = 320.0;
    const CHAMBER_Y: f32 = 29.0;
    const LAVA_LEVEL: f32 = 25.0;
    if packed == NONE {
        return None;
    }
    let perlin = p.perlin();
    let (wx, wz) = crate::provider::lava_cave_warp(
        perlin,
        continental::unpack_x(packed),
        continental::unpack_y(packed),
    );
    let distance = ((wx * wx + wz * wz) as f64).sqrt() as f32;
    let main = distance < MAIN_RADIUS;
    let (xf, zf) = (x as f32, z as f32);
    let scale = 108f32;
    let nwx = wx + perlin.noise2((xf + 1700.0) / 130.0, (zf - 900.0) / 130.0) * 24.0;
    let nwz = wz + perlin.noise2((xf - 2300.0) / 130.0, (zf + 1100.0) / 130.0) * 24.0;
    let s = p
        .cell()
        .sample_two_2d((nwx / scale) as f64, (nwz / scale) as f64, 1.0);
    let edge = ((s[2] - s[0]) as f32) * scale;
    let fade = smoothstep(1.0 - distance / NETWORK_RADIUS);
    let radius = 4.0 + fade * 19.0;
    let tunnel = distance < NETWORK_RADIUS && edge < radius;
    if !main && !tunnel {
        return None;
    }
    let center = CHAMBER_Y
        + perlin.noise2(xf / 105.0, zf / 105.0) * 2.2
        + perlin.noise2(xf / 41.0, zf / 41.0) * 1.1;
    let horizontal = edge / radius;
    let half = radius * (0f32.max(1.0 - horizontal * horizontal) as f64).sqrt() as f32;
    let t_floor = (center - half).min(LAVA_LEVEL - 2.0);
    let t_ceil = (center + half).max(LAVA_LEVEL + 4.0);
    let rough =
        perlin.noise2(xf / 13.0, zf / 13.0) * 2.2 + perlin.noise2(xf / 37.0, zf / 37.0) * 2.8;
    let (floor, ceiling) = if main {
        let radial = 1f32.min(distance / MAIN_RADIUS);
        let dome = 1.0 - smootherstep(radial);
        let mut floor = 11.0 + smootherstep(radial) * 21.0 + rough * 0.3;
        let stal_noise = perlin.noise2((xf + 947.0) / 8.0, (zf - 613.0) / 8.0)
            + perlin.noise2(xf / 21.0, zf / 21.0) * 0.35;
        let stal = (0f32.max(stal_noise - 0.48) as f64).powf(2.0) as f32 * 16.0;
        let mut ceiling = (45.0 + dome * 29.0 + rough).min(surface as f32 - 8.0) - stal;
        if tunnel {
            let profile = (0f32.max(1.0 - horizontal * horizontal) as f64).sqrt() as f32;
            let blend = smootherstep(profile);
            floor += (floor.min(t_floor) - floor) * blend;
            let roof = (t_ceil + rough * 0.25).min(surface as f32 - 8.0);
            ceiling += (ceiling.max(roof) - ceiling) * blend;
        }
        (floor, ceiling)
    } else {
        (t_floor, t_ceil + rough * 0.25)
    };
    let bottom = 5.max((floor as f64).ceil() as i32);
    let top = 250.min((ceiling as f64).floor() as i32).min(surface - 2);
    if top < bottom {
        return None;
    }
    Some(LavaColumn {
        bottom,
        top,
        main,
        wx,
        wz,
        distance,
        surface,
    })
}
