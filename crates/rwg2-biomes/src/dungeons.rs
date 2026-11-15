//! Roguelike Dungeons 1.6.6-GTNH and LootGames 2.2.14 placement.
//!
//! Exact parts (pure seed maths): LootGames dungeon chunks, Roguelike anchor chunks and the sequence of 50
//! candidate positions Roguelike tries. The terrain-dependent checks (Roguelike `validLocation`, LootGames
//! surface scan) run on the exact pre-population chunk heights (`chunkgen`). [`RogueModel`] (used by the viewer)
//! evaluates `validLocation` block by block: surface-pass top blocks (grass is not ground, stone/cobble/sand/dirt
//! are), Hodgepodge caves, sea water, and population as probabilities (a tall-grass-like `Material.vine` plant on
//! a grass top stops the descent one block higher; JUNGLE/DENSE canopies). Pre-population ground per column vs
//! the game: 36,778/36,864 (seed 1337) and 36,835/36,864 (seed 92900795).

use crate::chunkgen::{Chunk, ChunkGen};
use crate::forestry::ClimateTable;
use crate::java_random::JavaRandom;
use crate::jmath::FxMap;
use crate::provider::{BiomeProvider, Heights};

// ---------------------------------------------------------------- LootGames

/// `LootGamesWorldGen.canSpawnInChunk_v3`.
pub fn lootgames_can_spawn(world_seed: i64, rhomb: i32, cx: i32, cz: i32) -> bool {
    let xc = cx.wrapping_mul(2).wrapping_add(cz);
    let zc = cz.wrapping_mul(2).wrapping_add(cx);
    let d = rhomb * 2;
    let mut r = JavaRandom::new(
        world_seed
            .wrapping_add((xc / d) as i64)
            .wrapping_add(((zc / d) << 14) as i64),
    );
    let pos1 = 3 + r.next_int_bounded(d - 3);
    let pos2 = 3 + r.next_int_bounded(d - 3);
    let mx = xc.rem_euclid(d);
    let mz = zc.rem_euclid(d);
    mx >= 3
        && mz >= 3
        && ((mx == pos1 && mz == pos2) || (mx == pos1 + 1 && (mz == pos2 || mz == pos2 + 1)))
}

/// All LootGames dungeon chunks in the chunk rectangle (inclusive). Enumerates rhombus cells, not chunks.
pub fn lootgames_chunks(
    world_seed: i64,
    rhomb: i32,
    cx0: i32,
    cz0: i32,
    cx1: i32,
    cz1: i32,
) -> Vec<(i32, i32)> {
    let d = rhomb * 2;
    // (xc, zc) = (2cx + cz, 2cz + cx): bounding box of the rectangle in rhombus coordinates
    let xs = [2 * cx0 + cz0, 2 * cx1 + cz0, 2 * cx0 + cz1, 2 * cx1 + cz1];
    let zs = [2 * cz0 + cx0, 2 * cz1 + cx0, 2 * cz0 + cx1, 2 * cz1 + cx1];
    let (bx0, bx1) = (
        xs.iter().min().unwrap().div_euclid(d),
        xs.iter().max().unwrap().div_euclid(d),
    );
    let (bz0, bz1) = (
        zs.iter().min().unwrap().div_euclid(d),
        zs.iter().max().unwrap().div_euclid(d),
    );
    let mut out = Vec::new();
    for bz in bz0..=bz1 {
        for bx in bx0..=bx1 {
            // Java truncating division: floor block B < 0 shares the seed index of B + 1 (mod 0 never matches)
            let ix = if bx >= 0 { bx } else { bx + 1 };
            let iz = if bz >= 0 { bz } else { bz + 1 };
            let mut r = JavaRandom::new(
                world_seed
                    .wrapping_add(ix as i64)
                    .wrapping_add(((iz) << 14) as i64),
            );
            let pos1 = 3 + r.next_int_bounded(d - 3);
            let pos2 = 3 + r.next_int_bounded(d - 3);
            for (mx, mz) in [(pos1, pos2), (pos1 + 1, pos2), (pos1 + 1, pos2 + 1)] {
                if mx >= d || mz >= d {
                    continue;
                }
                let (xc, zc) = (bx * d + mx, bz * d + mz);
                // cx = (2xc - zc) / 3, cz = (2zc - xc) / 3 when integral
                let (a, b) = (2 * xc - zc, 2 * zc - xc);
                if a.rem_euclid(3) != 0 || b.rem_euclid(3) != 0 {
                    continue;
                }
                let (cx, cz) = (a / 3, b / 3);
                if cx >= cx0 && cx <= cx1 && cz >= cz0 && cz <= cz1 {
                    out.push((cx, cz));
                }
            }
        }
    }
    out.sort_by_key(|&(x, z)| (z, x));
    out
}

// ---------------------------------------------------------------- Roguelike

#[derive(Clone, Copy, Debug)]
pub struct RogueConfig {
    pub spawn_frequency: i32,
    pub upper_limit: i32,
    pub lower_limit: i32,
}

/// GTNH 2.9.0 `config/roguelike_dungeons/roguelike.cfg` (spawnFrequency 16; mod default is 10).
pub const ROGUE_GTNH: RogueConfig = RogueConfig {
    spawn_frequency: 16,
    upper_limit: 100,
    lower_limit: 60,
};

impl RogueConfig {
    fn min_max(&self) -> (i32, i32) {
        let f = self.spawn_frequency;
        ((8 * f / 10).max(2), (32 * f / 10).max(8))
    }

    /// Size of the square anchor region, in chunks.
    pub fn region_chunks(&self) -> i32 {
        self.min_max().1
    }
}

/// `World.setRandomSeed(a, b, c)`.
fn world_random(world_seed: i64, a: i32, b: i32, c: i32) -> JavaRandom {
    JavaRandom::new(
        (a as i64)
            .wrapping_mul(341873128712)
            .wrapping_add((b as i64).wrapping_mul(132897987541))
            .wrapping_add(world_seed)
            .wrapping_add(c as i64),
    )
}

/// The anchor chunk of the Roguelike region containing chunk (cx, cz) (`Dungeon.canSpawnInChunk` inverted).
pub fn rogue_region_anchor(world_seed: i64, cfg: &RogueConfig, rx: i32, rz: i32) -> (i32, i32) {
    let (min, max) = cfg.min_max();
    let mut r = world_random(world_seed, rx, rz, 10387312);
    let ax = rx * max + r.next_int_bounded(max - min);
    let az = rz * max + r.next_int_bounded(max - min);
    (ax, az)
}

/// `Dungeon.canSpawnInChunk`.
pub fn rogue_can_spawn(world_seed: i64, cfg: &RogueConfig, cx: i32, cz: i32) -> bool {
    let (_, max) = cfg.min_max();
    let tx = if cx < 0 { cx - (max - 1) } else { cx };
    let tz = if cz < 0 { cz - (max - 1) } else { cz };
    rogue_region_anchor(world_seed, cfg, tx / max, tz / max) == (cx, cz)
}

/// Anchor chunks in a chunk rectangle (inclusive).
pub fn rogue_anchors(
    world_seed: i64,
    cfg: &RogueConfig,
    cx0: i32,
    cz0: i32,
    cx1: i32,
    cz1: i32,
) -> Vec<(i32, i32)> {
    let max = cfg.region_chunks();
    let mut out = Vec::new();
    for rz in cz0.div_euclid(max)..=cz1.div_euclid(max) {
        for rx in cx0.div_euclid(max)..=cx1.div_euclid(max) {
            let (ax, az) = rogue_region_anchor(world_seed, cfg, rx, rz);
            if ax >= cx0 && ax <= cx1 && az >= cz0 && az <= cz1 {
                out.push((ax, az));
            }
        }
    }
    out
}

/// Forge `GameRegistry.generateWorld` per-chunk Random (reset before every IWorldGenerator).
pub fn forge_chunk_random(world_seed: i64, cx: i32, cz: i32) -> JavaRandom {
    let mut r = JavaRandom::new(world_seed);
    let xs = r.next_long() >> 3; // `nextLong() >> 2 + 1L` parses as >> 3
    let zs = r.next_long() >> 3;
    JavaRandom::new(
        xs.wrapping_mul(cx as i64)
            .wrapping_add(zs.wrapping_mul(cz as i64))
            ^ world_seed,
    )
}

/// The 50 candidate positions `Dungeon.generateNear` tries, in order (`getNearbyCoord(rand, x, z, 40, 100)`).
pub fn rogue_attempts(world_seed: i64, cx: i32, cz: i32) -> Vec<(i32, i32)> {
    let mut r = forge_chunk_random(world_seed, cx, cz);
    let (x0, z0) = (cx * 16 + 4, cz * 16 + 4);
    (0..50)
        .map(|_| {
            let dist = 40 + r.next_int_bounded(60);
            let angle = r.next_double() * 2.0 * std::f64::consts::PI;
            let xo = (angle.cos() * dist as f64) as i32;
            let zo = (angle.sin() * dist as f64) as i32;
            (x0 + xo, z0 + zo)
        })
        .collect()
}

// ---------------------------------------------------------------- terrain model

/// Chunk cache over `ChunkGen` for column queries.
pub struct Terrain<'a> {
    pub p: &'a BiomeProvider,
    pub h: &'a dyn Heights,
    g: ChunkGen,
    cache: FxMap<(i32, i32), Box<Chunk>>,
    /// Chunks generated so far (cache misses), for profiling.
    pub generated: usize,
}

impl<'a> Terrain<'a> {
    pub fn new(p: &'a BiomeProvider, h: &'a dyn Heights) -> Self {
        Terrain {
            p,
            h,
            g: ChunkGen::new(p),
            cache: FxMap::default(),
            generated: 0,
        }
    }

    pub fn chunk(&mut self, cx: i32, cz: i32) -> Option<&Chunk> {
        if self.cache.len() > 4096 {
            self.cache.clear();
        }
        if !self.cache.contains_key(&(cx, cz)) {
            let c = self.g.generate(self.p, self.h, cx, cz)?;
            self.generated += 1;
            self.cache.insert((cx, cz), Box::new(c));
        }
        self.cache.get(&(cx, cz)).map(|b| &**b)
    }

    /// (height, chunk biome id) at a column.
    pub fn column(&mut self, x: i32, z: i32) -> Option<(f32, u16)> {
        let c = self.chunk(x >> 4, z >> 4)?;
        let idx = ((z & 15) * 16 + (x & 15)) as usize;
        Some((c.height[idx], c.gen[idx]))
    }

    /// Highest solid block (`(int) height`; stone up to it before surface/caves).
    pub fn top(&mut self, x: i32, z: i32) -> Option<i32> {
        self.column(x, z).map(|(h, _)| h as i32)
    }
}

/// Highest "ground" block (not grass/water/air/...): below a grass top, i.e. top - 1 (model assumption).
fn ground_y(top: i32) -> i32 {
    top - 1
}

/// `Dungeon.validLocation` on the terrain model. `None` if terrain unavailable.
/// `strict` also rejects JUNGLE/DENSE biomes (populated vegetation; heuristic).
pub fn rogue_valid(
    t: &mut Terrain,
    climate: &ClimateTable,
    cfg: &RogueConfig,
    x: i32,
    z: i32,
    strict: bool,
) -> Option<bool> {
    let (_, biome) = t.column(x, z)?;
    if climate
        .get(biome)
        .map(|c| c.rogue_invalid || strict && c.rogue_vegetation)
        .unwrap_or(false)
    {
        return Some(false);
    }
    let top = t.top(x, z)?;
    if top >= cfg.upper_limit {
        return Some(false); // not air at upperLimit
    }
    if top < 62 {
        return Some(false); // descending hits water (sea level 63: water at y <= 62)
    }
    let g = ground_y(top);
    if g < cfg.lower_limit {
        return Some(false);
    }
    for dz in -4..=4 {
        for dx in -4..=4 {
            if ground_y(t.top(x + dx, z + dz)?) >= g + 4 {
                return Some(false);
            }
        }
    }
    let mut air = 0;
    for dz in -4..=4 {
        for dx in -4..=4 {
            if ground_y(t.top(x + dx, z + dz)?) < g - 3 {
                air += 1;
                if air > 8 {
                    return Some(false);
                }
            }
        }
    }
    Some(true)
}

#[derive(Clone, Copy, Debug)]
pub struct RogueDungeon {
    pub anchor: (i32, i32),
    /// Chosen position, or None if all 50 attempts fail.
    pub pos: Option<(i32, i32)>,
    /// Ground y at the position (dungeon entrance level).
    pub y: i32,
    pub attempt: i32,
    /// false: only found by ignoring the vegetation heuristic (less likely to be the real spot).
    pub confident: bool,
}

/// Predicted dungeon: first attempt passing the strict model (vegetation heuristic), else the first passing the
/// lenient one. Validated against the pack (force-loaded terrain): the attempt sequence is exact; the pick
/// matches the game in about half the anchors and is otherwise another spot on the same 40..100 ring.
pub fn rogue_dungeon(
    t: &mut Terrain,
    climate: &ClimateTable,
    cfg: &RogueConfig,
    seed: i64,
    cx: i32,
    cz: i32,
) -> RogueDungeon {
    let attempts = rogue_attempts(seed, cx, cz);
    for strict in [true, false] {
        for (i, &(x, z)) in attempts.iter().enumerate() {
            if rogue_valid(t, climate, cfg, x, z, strict) == Some(true) {
                let y = t.top(x, z).map(ground_y).unwrap_or(0);
                return RogueDungeon {
                    anchor: (cx, cz),
                    pos: Some((x, z)),
                    y,
                    attempt: i as i32,
                    confident: strict,
                };
            }
        }
    }
    RogueDungeon {
        anchor: (cx, cz),
        pos: None,
        y: 0,
        attempt: -1,
        confident: false,
    }
}

/// LootGames `setSurfaceLevel`: highest y where the whole 21x21 area is "valid surface"; on the model that is
/// min over columns of (top for submerged columns, top - 1 under a grass top). Room: y-10 ..= y-2.
pub fn lootgames_surface(t: &mut Terrain, x: i32, z: i32) -> Option<i32> {
    let mut m = i32::MAX;
    for dz in -10..=10 {
        for dx in -10..=10 {
            let top = t.top(x + dx, z + dz)?;
            let s = if top < 62 { top } else { top - 1 };
            m = m.min(s);
        }
    }
    Some(m.min(128))
}

// ---------------------------------------------------------------- Roguelike block-level model

/// What `WorldEditor.validGroundBlock` sees at one block on the model.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RogueBlock {
    Air,
    /// validGroundBlock = true (stone, dirt, sand, gravel, lava, tall grass / vines = `Material.vine`, ...)
    Ground,
    /// solid or plant but not ground (grass block, leaves, logs, flowers, snow layer, ...)
    Soft,
    Water,
}

/// Why the model refused a spot (first failing check).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RogueReason {
    Ok,
    Biome,
    Upper,
    Water,
    Lower,
    High,
    Holes,
    Vegetation,
}

#[derive(Clone, Debug)]
pub struct RogueCheck {
    /// Probability-like score that the spot passes (1 = model certain).
    pub p: f32,
    /// Ground y (cursor) on the model; -1 if not reached.
    pub y: i32,
    pub reason: RogueReason,
    /// Diagnostics: P(plant on the probe), P(pass | no plant), P(pass | plant).
    pub dbg: [f32; 3],
}

/// Per chunk: surface-pass top probabilities and P(top is validGround).
type ChunkTops = ([crate::spawn::TopProbs; 256], [f32; 256]);

/// Block-level `Dungeon.validLocation` model: exact pre-population terrain, the surface pass' top block
/// (`spawn::chunk_tops`: grass is not ground, sand/stone are), Hodgepodge caves (air at the check levels; lava
/// below y 11 counts as ground), sea water; vegetation from the biome dictionary.
pub struct RogueModel<'a> {
    pub t: Terrain<'a>,
    sm: crate::spawn::SurfaceModel,
    caves: crate::gtorder::Caves,
    masks: FxMap<(i32, i32), crate::gtorder::CaveMask>,
    tops: FxMap<(i32, i32), Box<ChunkTops>>,
    pub use_caves: bool,
    pub params: RogueParams,
    /// Replayed decoration (`deco::view::region_blocks`, 4 × 4-chunk groups) instead of the vegetation
    /// probabilities: `Some(gates)` turns it on. Plausible, not exact (canonical populate order).
    pub deco: Option<crate::deco::view::DecoOverlay>,
    /// With `deco`: false = check the replayed blocks themselves; true = keep the probabilistic model and take
    /// its plant / canopy-vine rates from the replay's per-chunk statistics (positions are not exact).
    pub deco_stats: bool,
    /// K: P(canopy vine stops the check) = min(1, K · share of columns with a vine above top + 1).
    pub deco_vine_k: f32,
    stats: FxMap<(i32, i32), (f32, f32)>,
}

/// Decoration group size (chunks) for [`RogueModel::deco`].
pub const ROGUE_DECO_N: i32 = 4;

impl<'a> RogueModel<'a> {
    pub fn new(p: &'a BiomeProvider, h: &'a dyn Heights, seed: i64) -> Self {
        RogueModel {
            t: Terrain::new(p, h),
            sm: crate::spawn::SurfaceModel::new(&p.table),
            caves: crate::gtorder::Caves::new(seed, true),
            masks: FxMap::default(),
            tops: FxMap::default(),
            use_caves: true,
            params: ROGUE_PARAMS,
            deco: None,
            deco_stats: false,
            deco_vine_k: 3.0,
            stats: FxMap::default(),
        }
    }

    pub fn biome(&mut self, x: i32, z: i32) -> u16 {
        self.t.column(x, z).map(|c| c.1).unwrap_or(0)
    }

    fn tops_of(&mut self, x: i32, z: i32) -> (crate::spawn::TopProbs, f32) {
        let (cx, cz) = (x >> 4, z >> 4);
        if !self.tops.contains_key(&(cx, cz)) {
            if self.tops.len() > 4096 {
                self.tops.clear();
            }
            let p = self.t.p;
            let tp = match self.t.chunk(cx, cz) {
                Some(c) => crate::spawn::chunk_tops_ground(p, &self.sm, c, cx, cz),
                None => ([crate::spawn::TopProbs::default(); 256], [0.0; 256]),
            };
            self.tops.insert((cx, cz), Box::new(tp));
        }
        let i = ((z & 15) * 16 + (x & 15)) as usize;
        let t = &self.tops[&(cx, cz)];
        (t.0[i], t.1[i])
    }

    fn carved(&mut self, x: i32, y: i32, z: i32) -> bool {
        if !self.use_caves {
            return false;
        }
        let (cx, cz) = (x >> 4, z >> 4);
        if !self.masks.contains_key(&(cx, cz)) {
            if self.masks.len() > 4096 {
                self.masks.clear();
            }
            let Some(c) = self.t.chunk(cx, cz) else {
                return false;
            };
            let mut h = [0i32; 256];
            for (o, v) in h.iter_mut().zip(c.height.iter()) {
                *o = *v as i32;
            }
            let m = self.caves.carve(cx, cz, &h);
            self.masks.insert((cx, cz), m);
        }
        self.masks[&(cx, cz)].carved(x & 15, y, z & 15)
    }

    /// Top block of a column is not a validGround block (grass, BOP grass-like) on the model.
    fn grass_top(&mut self, x: i32, z: i32) -> bool {
        self.tops_of(x, z).1 < 0.5
    }

    /// Decoration write at (x, y, z) from the replayed populates, if any.
    pub fn deco_at(&mut self, x: i32, y: i32, z: i32) -> Option<u16> {
        let (p, h) = (self.t.p, self.t.h);
        self.deco.as_mut()?.at(p, h, x, y, z)
    }

    /// Block class with replayed decoration on top of the pre-population model.
    fn block_deco(&mut self, x: i32, y: i32, z: i32) -> RogueBlock {
        use crate::deco::{mat, Mat};
        if let Some(b) = self.deco_at(x, y, z) {
            return match mat(b) {
                Mat::Air => RogueBlock::Air,
                Mat::Water => RogueBlock::Water,
                // validGroundBlock: wood, water, cactus, snow, grass, gourd, leaves, plants are not ground;
                // Material.vine (tall grass, dead bush, vines), ice, dirt, stone, sand, clay... are
                Mat::Wood
                | Mat::Cactus
                | Mat::Snow
                | Mat::Grass
                | Mat::Gourd
                | Mat::Leaves
                | Mat::Plants => RogueBlock::Soft,
                _ => RogueBlock::Ground,
            };
        }
        self.block_pre(x, y, z)
    }

    /// Per chunk from the replayed decoration: (P(a `Material.vine` plant on a grass top), share of columns
    /// with a vine block above top + 1).
    fn chunk_stats(&mut self, cx: i32, cz: i32) -> (f32, f32) {
        if let Some(&v) = self.stats.get(&(cx, cz)) {
            return v;
        }
        use crate::deco::{mat, Mat};
        let (mut grass, mut tall, mut vine) = (0u32, 0u32, 0u32);
        for lz in 0..16 {
            for lx in 0..16 {
                let (x, z) = (cx * 16 + lx, cz * 16 + lz);
                let Some(top) = self.t.top(x, z) else {
                    continue;
                };
                if top >= 62 && self.grass_top(x, z) {
                    grass += 1;
                    if self
                        .deco_at(x, top + 1, z)
                        .is_some_and(|b| mat(b) == Mat::Vine)
                    {
                        tall += 1;
                    }
                }
                if (top + 2..top + 40)
                    .any(|y| self.deco_at(x, y, z).is_some_and(|b| mat(b) == Mat::Vine))
                {
                    vine += 1;
                }
            }
        }
        let v = (
            if grass > 0 {
                tall as f32 / grass as f32
            } else {
                0.0
            },
            vine as f32 / 256.0,
        );
        self.stats.insert((cx, cz), v);
        v
    }

    /// Block class (with decoration when [`RogueModel::deco`] is on and not in stats mode).
    pub fn block(&mut self, x: i32, y: i32, z: i32) -> RogueBlock {
        if self.deco.is_some() && !self.deco_stats {
            self.block_deco(x, y, z)
        } else {
            self.block_pre(x, y, z)
        }
    }

    /// Pre-population block class.
    pub fn block_pre(&mut self, x: i32, y: i32, z: i32) -> RogueBlock {
        let Some(top) = self.t.top(x, z) else {
            return RogueBlock::Air;
        };
        if y > top {
            return if y <= 62 {
                RogueBlock::Water
            } else {
                RogueBlock::Air
            };
        }
        if y <= 0 {
            return RogueBlock::Ground;
        }
        if self.carved(x, y, z) {
            return if y < 11 {
                RogueBlock::Ground
            } else {
                RogueBlock::Air
            };
        }
        if y == top && top >= 62 && self.grass_top(x, z) {
            return RogueBlock::Soft;
        }
        RogueBlock::Ground
    }

    /// P(a `Material.vine` plant — tall grass, BOP foliage — stands on this column's top after population).
    fn plant_prob(&mut self, climate: &ClimateTable, x: i32, z: i32) -> f32 {
        if !self.grass_top(x, z) {
            return 0.0;
        }
        if self.deco.is_some() && self.deco_stats {
            return self.chunk_stats(x >> 4, z >> 4).0;
        }
        let b = self.biome(x, z);
        self.params.density(climate.dict(b))
    }

    /// One scenario for the probe column: cursor at `c` (`None`: refused before the flatness checks).
    /// Returns P(pass | scenario) from the neighbours' plant probabilities.
    fn flat_checks(
        &mut self,
        climate: &ClimateTable,
        x: i32,
        z: i32,
        c: i32,
    ) -> (f32, RogueReason) {
        let mut p_ok = 1.0f32;
        for dz in -4..=4 {
            for dx in -4..=4 {
                let (xx, zz) = (x + dx, z + dz);
                if self.block(xx, c + 4, zz) == RogueBlock::Ground {
                    return (0.0, RogueReason::High);
                }
                if (dx, dz) != (0, 0) && self.t.top(xx, zz) == Some(c + 3) {
                    p_ok *= 1.0 - self.plant_prob(climate, xx, zz);
                }
            }
        }
        let mut air = 0;
        for dz in -4..=4 {
            for dx in -4..=4 {
                if self.block(x + dx, c - 3, z + dz) != RogueBlock::Ground {
                    air += 1;
                }
            }
        }
        if air > 8 {
            return (0.0, RogueReason::Holes);
        }
        (
            p_ok,
            if p_ok < 0.5 {
                RogueReason::High
            } else {
                RogueReason::Ok
            },
        )
    }

    pub fn check(
        &mut self,
        climate: &ClimateTable,
        cfg: &RogueConfig,
        x: i32,
        z: i32,
    ) -> RogueCheck {
        let mut r = RogueCheck {
            p: 0.0,
            y: -1,
            reason: RogueReason::Ok,
            dbg: [0.0; 3],
        };
        let b = self.biome(x, z);
        let c = climate.get(b).unwrap_or_default();
        if c.rogue_invalid {
            r.reason = RogueReason::Biome;
            return r;
        }
        if self.block(x, cfg.upper_limit, z) != RogueBlock::Air {
            r.reason = RogueReason::Upper;
            return r;
        }
        let mut y = cfg.upper_limit;
        while self.block(x, y, z) != RogueBlock::Ground {
            y -= 1;
            if y < cfg.lower_limit {
                r.reason = RogueReason::Lower;
                return r;
            }
            if self.block(x, y, z) == RogueBlock::Water {
                r.reason = RogueReason::Water;
                return r;
            }
        }
        if self.deco.is_some() && !self.deco_stats {
            // decorated world: deterministic checks on the replayed blocks
            r.y = y;
            for dz in -4..=4 {
                for dx in -4..=4 {
                    if self.block(x + dx, y + 4, z + dz) == RogueBlock::Ground {
                        r.reason = RogueReason::High;
                        return r;
                    }
                }
            }
            let mut air = 0;
            for dz in -4..=4 {
                for dx in -4..=4 {
                    if self.block(x + dx, y - 3, z + dz) != RogueBlock::Ground {
                        air += 1;
                    }
                }
            }
            if air > 8 {
                r.reason = RogueReason::Holes;
                return r;
            }
            r.p = 1.0;
            return r;
        }
        // scenario A: no plant on the probe column (cursor = pre-population ground y); B: a tall-grass-like plant
        // on its top (cursor = top + 1, only above a grass top that is above water)
        let top = self.t.top(x, z).unwrap_or(y);
        let q = if top >= 62 {
            self.plant_prob(climate, x, z)
        } else {
            0.0
        };
        let (pa, ra) = self.flat_checks(climate, x, z, y);
        let (pb, rb) = if q > 0.0 && top + 1 >= cfg.lower_limit {
            self.flat_checks(climate, x, z, top + 1)
        } else {
            (0.0, RogueReason::Ok)
        };
        r.dbg = [q, pa, pb];
        let wa = (1.0 - q) * pa;
        let wb = q * pb;
        let mut p = wa + wb;
        r.y = if wb > wa { top + 1 } else { y };
        r.reason = if p >= 0.5 {
            RogueReason::Ok
        } else if wb > wa {
            rb
        } else {
            ra
        };
        let veg = if self.deco.is_some() && self.deco_stats {
            1.0 - (self.deco_vine_k * self.chunk_stats(x >> 4, z >> 4).1).min(1.0)
        } else if c.rogue_vegetation {
            self.params.vine_ok
        } else {
            1.0
        };
        if veg < 1.0 {
            p *= veg;
            if p < 0.5 && r.reason == RogueReason::Ok {
                r.reason = RogueReason::Vegetation;
            }
        }
        r.p = p;
        r
    }
}

/// Predicted Roguelike spot with the probability-aware model.
#[derive(Clone, Copy, Debug)]
pub struct RogueSpot {
    pub pos: Option<(i32, i32)>,
    pub y: i32,
    pub attempt: i32,
    /// P(this attempt is the one the game picks) on the model.
    pub confidence: f32,
}

impl RogueModel<'_> {
    /// The game takes the first of its 50 attempts that passes: pick the attempt with the highest
    /// P(passes and all earlier ones fail).
    pub fn dungeon(
        &mut self,
        climate: &ClimateTable,
        cfg: &RogueConfig,
        seed: i64,
        cx: i32,
        cz: i32,
    ) -> RogueSpot {
        let mut best = RogueSpot {
            pos: None,
            y: 0,
            attempt: -1,
            confidence: 0.0,
        };
        let mut none_before = 1.0f32;
        for (i, (x, z)) in rogue_attempts(seed, cx, cz).into_iter().enumerate() {
            let r = self.check(climate, cfg, x, z);
            let w = none_before * r.p;
            if w > best.confidence {
                best = RogueSpot {
                    pos: Some((x, z)),
                    y: r.y,
                    attempt: i as i32,
                    confidence: w,
                };
            }
            none_before *= 1.0 - r.p;
            if none_before < best.confidence {
                break;
            }
        }
        best
    }
}

/// Tunables of [`RogueModel`].
#[derive(Clone, Copy, Debug)]
pub struct RogueParams {
    /// P(tall grass / foliage on a grass top): open biomes (PLAINS/SAVANNA/LUSH/JUNGLE), others, barren
    /// (SANDY/WASTELAND/SNOWY/DEAD/MOUNTAIN).
    pub dens_open: f32,
    pub dens_mid: f32,
    pub dens_barren: f32,
    /// Factor for JUNGLE/DENSE biomes (jungle trees carry vines = `Material.vine` = ground, so the descent can
    /// stop in the canopy; dense undergrowth). Fitted: JUNGLE-only and JUNGLE+DENSE score the same overall.
    pub vine_ok: f32,
}

impl RogueParams {
    pub fn density(&self, d: &[String]) -> f32 {
        let has = |t: &str| d.iter().any(|x| x == t);
        if has("SANDY") || has("WASTELAND") || has("SNOWY") || has("DEAD") || has("MOUNTAIN") {
            self.dens_barren
        } else if has("PLAINS") || has("SAVANNA") || has("LUSH") || has("JUNGLE") {
            self.dens_open
        } else {
            self.dens_mid
        }
    }
}

pub const ROGUE_PARAMS: RogueParams = RogueParams {
    dens_open: 0.3,
    dens_mid: 0.2,
    dens_barren: 0.06,
    vine_ok: 0.4,
};
