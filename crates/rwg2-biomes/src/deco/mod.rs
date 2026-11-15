//! RWG2 `populate` decoration (the border-biome loop: each biome's `rDecorate`) on an exact block world.
//!
//! Experimental (trees). The world is the exact pre-population column (`surface::surface_chunk`) plus the modelled
//! populate prefix (`population::PopModel`, up to the border loop) plus what decoration writes; the heightmap
//! (`World.getHeightValue`) follows every write like the game's (light opacity > 0).
//!
//! - [`gens`]: the generators (RWG2 `rwg2.deco.*`, vanilla `WorldGen*`).
//! - [`biomes`]: the `rDecorate` of every RWG2 native biome class.

pub mod biomes;
pub mod bop;
pub mod bop_table;
pub mod bop_trees;
pub mod gens;
pub mod oil;
mod post_table;
pub mod spawn_table;
pub mod view;

use crate::chunkgen::ChunkGen;
use crate::gtorder::Caves;
use crate::jmath::FxMap;
use crate::provider::{BiomeProvider, Heights};
use crate::surface::{blk, surface_chunk, SurfaceTable};

/// Decoration block: low byte = id (surface codes `blk::*` below 100, decoration ids from 100), high byte = meta.
pub type Db = u16;

pub mod db {
    use super::Db;
    pub const MOSSY: u8 = 100;
    pub const LOG: u8 = 101;
    pub const LEAVES: u8 = 102;
    pub const LOG2: u8 = 103;
    pub const LEAVES2: u8 = 104;
    pub const CACTUS: u8 = 105;
    pub const TALLGRASS: u8 = 106;
    pub const DOUBLE_PLANT: u8 = 107;
    pub const DEADBUSH: u8 = 108;
    pub const REEDS: u8 = 109;
    pub const PUMPKIN: u8 = 110;
    pub const YELLOW_FLOWER: u8 = 111;
    pub const RED_FLOWER: u8 = 112;
    /// other solid opaque block (populate prefix: abyssal stone, chests, spawners, emerald ore, ...)
    pub const SOLID: u8 = 113;
    pub const VINE: u8 = 114;
    pub const COCOA: u8 = 115;
    pub const FARMLAND: u8 = 116;
    pub const POTATOES: u8 = 117;
    pub const CARROTS: u8 = 118;
    pub const WHEAT: u8 = 119;
    pub const BROWN_MUSHROOM: u8 = 120;
    pub const RED_MUSHROOM: u8 = 121;
    pub const WATERLILY: u8 = 122;
    pub const BOP_FOLIAGE: u8 = 123;
    pub const BOP_FLOWERS: u8 = 124;
    pub const BOP_FLOWERS2: u8 = 125;
    pub const BOP_MUSHROOMS: u8 = 126;
    pub const BOP_PLANTS: u8 = 127;
    pub const BOTANIA_FLOWER: u8 = 128;
    pub const BOTANIA_DOUBLE1: u8 = 129;
    pub const BOTANIA_DOUBLE2: u8 = 130;
    pub const CLAY: u8 = 131;
    /// BOP `logs1..4` (type `(n - 1) · 4 + meta & 3`), `leaves1..4`, `colorizedLeaves1..2`
    pub const BOP_LOGS1: u8 = 132;
    pub const BOP_LOGS2: u8 = 133;
    pub const BOP_LOGS3: u8 = 134;
    pub const BOP_LOGS4: u8 = 135;
    pub const BOP_LEAVES1: u8 = 136;
    pub const BOP_LEAVES2: u8 = 137;
    pub const BOP_LEAVES3: u8 = 138;
    pub const BOP_LEAVES4: u8 = 139;
    pub const BOP_CLEAVES1: u8 = 140;
    pub const BOP_CLEAVES2: u8 = 141;
    pub const BOP_APPLE_LEAVES: u8 = 142;
    pub const BOP_PERSIMMON_LEAVES: u8 = 143;
    /// giant flower petals (0 red, 1 yellow)
    pub const BOP_PETALS: u8 = 144;
    pub const BOP_BAMBOO: u8 = 145;
    /// `BOPCBlocks.ivy` (vine-like)
    pub const BOP_IVY: u8 = 146;
    /// `BOPCBlocks.fruitBop` (apples, persimmons, peaches, pears)
    pub const BOP_FRUIT: u8 = 147;
    /// `BOPCBlocks.coral1` (kelp 8–11, coral 12–15) / `coral2` (seaweed 8)
    pub const BOP_CORAL1: u8 = 148;
    pub const BOP_CORAL2: u8 = 149;
    /// `BOPCBlocks.moss` (vine-like, on wood / stone sides)
    pub const BOP_MOSS: u8 = 150;
    pub const fn mk(id: u8, meta: u8) -> Db {
        id as Db | (meta as Db) << 8
    }
    pub const fn id(b: Db) -> u8 {
        b as u8
    }
    pub const fn meta(b: Db) -> u8 {
        (b >> 8) as u8
    }
    /// Registry name.
    pub fn name(b: Db) -> &'static str {
        match id(b) {
            MOSSY => "minecraft:mossy_cobblestone",
            LOG => "minecraft:log",
            LEAVES => "minecraft:leaves",
            LOG2 => "minecraft:log2",
            LEAVES2 => "minecraft:leaves2",
            CACTUS => "minecraft:cactus",
            TALLGRASS => "minecraft:tallgrass",
            DOUBLE_PLANT => "minecraft:double_plant",
            DEADBUSH => "minecraft:deadbush",
            REEDS => "minecraft:reeds",
            PUMPKIN => "minecraft:pumpkin",
            YELLOW_FLOWER => "minecraft:yellow_flower",
            RED_FLOWER => "minecraft:red_flower",
            VINE => "minecraft:vine",
            COCOA => "minecraft:cocoa",
            FARMLAND => "minecraft:farmland",
            POTATOES => "minecraft:potatoes",
            CARROTS => "minecraft:carrots",
            WHEAT => "minecraft:wheat",
            BROWN_MUSHROOM => "minecraft:brown_mushroom",
            RED_MUSHROOM => "minecraft:red_mushroom",
            WATERLILY => "minecraft:waterlily",
            BOP_FOLIAGE => "BiomesOPlenty:foliage",
            BOP_FLOWERS => "BiomesOPlenty:flowers",
            BOP_FLOWERS2 => "BiomesOPlenty:flowers2",
            BOP_MUSHROOMS => "BiomesOPlenty:mushrooms",
            BOP_PLANTS => "BiomesOPlenty:plants",
            BOTANIA_FLOWER => "Botania:flower",
            BOTANIA_DOUBLE1 => "Botania:doubleFlower1",
            BOTANIA_DOUBLE2 => "Botania:doubleFlower2",
            CLAY => "minecraft:clay",
            BOP_LOGS1 => "BiomesOPlenty:logs1",
            BOP_LOGS2 => "BiomesOPlenty:logs2",
            BOP_LOGS3 => "BiomesOPlenty:logs3",
            BOP_LOGS4 => "BiomesOPlenty:logs4",
            BOP_LEAVES1 => "BiomesOPlenty:leaves1",
            BOP_LEAVES2 => "BiomesOPlenty:leaves2",
            BOP_LEAVES3 => "BiomesOPlenty:leaves3",
            BOP_LEAVES4 => "BiomesOPlenty:leaves4",
            BOP_CLEAVES1 => "BiomesOPlenty:colorizedLeaves1",
            BOP_CLEAVES2 => "BiomesOPlenty:colorizedLeaves2",
            BOP_APPLE_LEAVES => "BiomesOPlenty:appleLeaves",
            BOP_PERSIMMON_LEAVES => "BiomesOPlenty:persimmonLeaves",
            BOP_PETALS => "BiomesOPlenty:petals",
            BOP_BAMBOO => "BiomesOPlenty:bamboo",
            BOP_IVY => "BiomesOPlenty:ivy",
            BOP_FRUIT => "BiomesOPlenty:fruitBop",
            BOP_CORAL1 => "BiomesOPlenty:coral1",
            BOP_CORAL2 => "BiomesOPlenty:coral2",
            BOP_MOSS => "BiomesOPlenty:moss",
            SOLID => "?solid",
            s => crate::surface::block_name(s),
        }
    }
}

pub(crate) const AIR: Db = blk::AIR as Db;

/// `Block.getMaterial()` classes the generators test.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Mat {
    Air,
    Grass,
    Ground,
    Sand,
    Rock,
    Wood,
    Leaves,
    Plants,
    Vine,
    Water,
    Lava,
    /// snow layer
    Snow,
    /// snow block
    CraftedSnow,
    Ice,
    PackedIce,
    Clay,
    Cactus,
    Gourd,
}

pub fn mat(b: Db) -> Mat {
    use db::*;
    match id(b) {
        blk::AIR => Mat::Air,
        blk::GRASS | blk::MYCELIUM | blk::BOP_LONG_GRASS | blk::BOP_ORIGIN_GRASS => Mat::Grass,
        blk::DIRT
        | blk::COARSE_DIRT
        | blk::PODZOL
        | blk::BOP_HARD_DIRT
        | blk::BOP_DRIED_DIRT
        | FARMLAND => Mat::Ground,
        blk::SAND
        | blk::RED_SAND
        | blk::GRAVEL
        | blk::BOP_HARD_SAND
        | blk::BOP_MUD
        | blk::BOP_ASH => Mat::Sand,
        blk::SNOW => Mat::CraftedSnow,
        blk::SNOW_LAYER => Mat::Snow,
        blk::ICE => Mat::Ice,
        blk::PACKED_ICE => Mat::PackedIce,
        blk::WATER => Mat::Water,
        blk::LAVA => Mat::Lava,
        LOG | LOG2 | BOP_LOGS1..=BOP_LOGS4 => Mat::Wood,
        LEAVES | LEAVES2 | BOP_LEAVES1..=BOP_PETALS => Mat::Leaves,
        TALLGRASS | DEADBUSH | VINE | BOP_IVY | BOP_MOSS => Mat::Vine,
        DOUBLE_PLANT
        | REEDS
        | YELLOW_FLOWER
        | RED_FLOWER
        | COCOA
        | POTATOES
        | CARROTS
        | WHEAT
        | BROWN_MUSHROOM
        | RED_MUSHROOM
        | WATERLILY
        | BOP_FOLIAGE..=BOTANIA_DOUBLE2
        | BOP_BAMBOO
        | BOP_FRUIT
        | BOP_CORAL1
        | BOP_CORAL2 => Mat::Plants,
        CLAY => Mat::Clay,
        CACTUS => Mat::Cactus,
        PUMPKIN => Mat::Gourd,
        _ => Mat::Rock,
    }
}

/// The Random a decoration generator sees: the plain shared Random (RWG2 natives, vanilla decorators) or BOP's
/// `RandomForcedPositiveOwned` (`nextInt(n > 0)` on the shared Random, everything else on its own copy).
pub trait DRng {
    fn next_int_bounded(&mut self, n: i32) -> i32;
    fn next_float(&mut self) -> f32;
    fn next_double(&mut self) -> f64;
    fn next_boolean(&mut self) -> bool;
    fn next_long(&mut self) -> i64;
}

impl DRng for crate::java_random::JavaRandom {
    fn next_int_bounded(&mut self, n: i32) -> i32 {
        crate::java_random::JavaRandom::next_int_bounded(self, n)
    }
    fn next_float(&mut self) -> f32 {
        crate::java_random::JavaRandom::next_float(self)
    }
    fn next_double(&mut self) -> f64 {
        crate::java_random::JavaRandom::next_double(self)
    }
    fn next_boolean(&mut self) -> bool {
        crate::java_random::JavaRandom::next_boolean(self)
    }
    fn next_long(&mut self) -> i64 {
        crate::java_random::JavaRandom::next_long(self)
    }
}

pub(crate) fn is_air(b: Db) -> bool {
    db::id(b) == blk::AIR
}
pub(crate) fn is_leaves(b: Db) -> bool {
    matches!(
        db::id(b),
        db::LEAVES | db::LEAVES2 | db::BOP_LEAVES1..=db::BOP_PETALS
    )
}
pub(crate) fn is(b: Db, id: u8) -> bool {
    db::id(b) == id
}
/// `== Blocks.sand` (sand and red sand are one block, meta 0 / 1)
pub(crate) fn is_sand(b: Db) -> bool {
    matches!(db::id(b), blk::SAND | blk::RED_SAND)
}
/// `== Blocks.dirt` (dirt, coarse dirt, podzol: meta 0 / 1 / 2)
pub(crate) fn is_dirt(b: Db) -> bool {
    matches!(db::id(b), blk::DIRT | blk::COARSE_DIRT | blk::PODZOL)
}
/// `== Blocks.grass || == Blocks.dirt`
pub(crate) fn is_earth(b: Db) -> bool {
    is(b, blk::GRASS) || is_dirt(b)
}
/// `== Blocks.water` (still water; the generator only writes still water)
pub(crate) fn is_water(b: Db) -> bool {
    db::id(b) == blk::WATER
}
/// `Material.isSolid()`
pub(crate) fn mat_solid(b: Db) -> bool {
    !matches!(
        mat(b),
        Mat::Air | Mat::Water | Mat::Lava | Mat::Plants | Mat::Vine | Mat::Snow
    )
}
pub(crate) fn mat_liquid(b: Db) -> bool {
    matches!(mat(b), Mat::Water | Mat::Lava)
}
/// `Material.isReplaceable()`
pub(crate) fn mat_replaceable(b: Db) -> bool {
    matches!(
        mat(b),
        Mat::Air | Mat::Water | Mat::Lava | Mat::Vine | Mat::Snow
    )
}
/// `Block.isOpaqueCube()` / `func_149730_j` (full opaque cube).
pub(crate) fn opaque(b: Db) -> bool {
    mat_solid(b) && !matches!(mat(b), Mat::Leaves | Mat::Cactus | Mat::Ice) && !is(b, db::FARMLAND)
}
/// Light opacity > 0 (what the heightmap tracks).
pub(crate) fn blocks_light(b: Db) -> bool {
    !matches!(
        mat(b),
        Mat::Air | Mat::Plants | Mat::Vine | Mat::Snow | Mat::Cactus
    )
}
/// `canSustainPlant(..., sapling)` (plains type): grass, dirt, farmland and BOP grasses.
pub(crate) fn sustains_sapling(b: Db) -> bool {
    is_earth(b)
        || matches!(
            db::id(b),
            db::FARMLAND | blk::BOP_LONG_GRASS | blk::BOP_ORIGIN_GRASS
        )
}

#[derive(Clone)]
struct DChunk {
    cols: Box<[[Db; 256]; 256]>,
    /// `heightMap`: lowest y with only light-transparent blocks at and above
    height: [u16; 256],
    /// chunk biome array (gen id), `getBiomeGenForCoords`
    gen: [u16; 256],
}

/// Undecorated chunks shared by every [`DecoWorld`] of the thread (seed, continental, cx, cz).
const BASE_CAP: usize = 192;
type BaseCache = FxMap<(i64, bool, i32, i32), std::rc::Rc<DChunk>>;

thread_local! {
    static BASE: std::cell::RefCell<BaseCache> = std::cell::RefCell::new(FxMap::default());
}

/// Exact block world for decoration.
pub struct DecoWorld<'a> {
    pub p: &'a BiomeProvider,
    pub h: &'a dyn Heights,
    pub seed: i64,
    st: SurfaceTable,
    m: crate::spawn::SurfaceModel,
    caves: Caves,
    gen: ChunkGen,
    chunks: FxMap<(i32, i32), DChunk>,
    /// every write, in order (x, y, z, block)
    pub log: Vec<(i32, i32, i32, Db)>,
    /// generator calls: (name, x, y, z, log length at the call), when `tracing`
    pub trace: Vec<(&'static str, i32, i32, i32, usize)>,
    pub tracing: bool,
    /// validation: recorded shared Random state (scrambled) entering a chunk's deferred ocean decorator; the
    /// listeners between the springs and ICE reseed the Random in a way the model does not know
    pub ocean_entry: FxMap<(i32, i32), i64>,
    /// Diagnostics: start the deferred ocean decorators from `ocean_entry` instead of the modelled `post_tail`.
    pub use_recorded_ocean_entry: bool,
    /// (recorded ocean entries the modelled `post_tail` state matched, recorded entries seen)
    pub ocean_entry_check: (u32, u32),
    /// DreamMaster oil deposits of the replayed populates: (populating chunk, deposit, confidence = decoration
    /// `oil::chain_confidence` of the border × ANIMALS group-size share; a group placed on land drops it)
    pub oil: Vec<((i32, i32), oil::OilDeposit, f32)>,
    /// Random state entering ANIMALS of the last replayed populate (scrambled)
    pub post_entry: i64,
    /// BOP decorator phase marks (chunk x, z, Decorate event name, shared Random state), when `tracing`
    pub phases: Vec<(i32, i32, &'static str, i64)>,
    /// `HighRainforest` large-tree points (seed ^ salt, spacing 12, 2 rounds)
    pub(crate) rainforest_points: Option<crate::poisson::PoissonPointNoise>,
    /// `BiomeGenBase.temperature` by gen id (base and river biomes)
    gen_temp: FxMap<u16, f32>,
    /// base biome class suffix by gen id (`BiomeGenFrostForest`, ...; river ids absent)
    pub(crate) gen_class: FxMap<u16, &'a str>,
}

impl<'a> DecoWorld<'a> {
    pub fn new(p: &'a BiomeProvider, h: &'a dyn Heights, seed: i64) -> Self {
        DecoWorld {
            p,
            h,
            seed,
            st: SurfaceTable::new(&p.table),
            m: crate::spawn::SurfaceModel::new(&p.table),
            caves: Caves::new(seed, p.is_continental()),
            gen: ChunkGen::new(p),
            chunks: FxMap::default(),
            log: Vec::new(),
            trace: Vec::new(),
            tracing: false,
            ocean_entry: FxMap::default(),
            use_recorded_ocean_entry: false,
            ocean_entry_check: (0, 0),
            oil: Vec::new(),
            post_entry: 0,
            phases: Vec::new(),
            rainforest_points: None,
            gen_temp: {
                let mut m = FxMap::default();
                for b in &p.table.biomes {
                    m.entry(b.gen_id).or_insert(b.temperature);
                    m.entry(b.river_gen_id).or_insert(b.river_temperature);
                }
                m
            },
            gen_class: {
                let mut m = FxMap::default();
                for b in &p.table.biomes {
                    m.entry(b.gen_id).or_insert(b.base_class.as_str());
                }
                if let Some(g) = p.table.lava_cave_marker_gen_id {
                    // Support.lavaCaveMarkerBiome: BOP's Phantasmagoric Inferno (a BOPBiome, nether decorator)
                    m.insert(
                        g,
                        "biomesoplenty.common.biome.nether.BiomeGenPhantasmagoricInferno",
                    );
                }
                m
            },
        }
    }

    fn chunk(&mut self, cx: i32, cz: i32) -> &mut DChunk {
        if !self.chunks.contains_key(&(cx, cz)) {
            let key = (self.seed, self.p.is_continental(), cx, cz);
            let cached = BASE.with(|b| b.borrow().get(&key).cloned());
            let base = match cached {
                Some(c) => c,
                None => {
                    let (p, h) = (self.p, self.h);
                    let c = self.gen.generate(p, h, cx, cz).expect("chunk").clone();
                    let cc =
                        crate::spawn::carved_chunk(p, h, &self.m, &c, cx, cz, Some(&self.caves));
                    let sc = surface_chunk(p, h, &self.st, &c, cx, cz, &cc);
                    let mut cols = Box::new([[AIR; 256]; 256]);
                    let mut height = [0u16; 256];
                    for idx in 0..256 {
                        for y in 0..256 {
                            cols[idx][y] = sc.cols[idx][y] as Db;
                        }
                        height[idx] = (0..256)
                            .rev()
                            .find(|&y| blocks_light(cols[idx][y]))
                            .map_or(0, |y| y as u16 + 1);
                    }
                    let d = std::rc::Rc::new(DChunk {
                        cols,
                        height,
                        gen: c.gen,
                    });
                    BASE.with(|b| {
                        let mut b = b.borrow_mut();
                        if b.len() >= BASE_CAP {
                            b.clear();
                        }
                        b.insert(key, d.clone());
                    });
                    d
                }
            };
            self.chunks.insert((cx, cz), (*base).clone());
        }
        self.chunks.get_mut(&(cx, cz)).unwrap()
    }

    pub fn get(&mut self, x: i32, y: i32, z: i32) -> Db {
        if !(0..256).contains(&y) {
            return AIR;
        }
        let c = self.chunk(x >> 4, z >> 4);
        c.cols[((z & 15) * 16 + (x & 15)) as usize][y as usize]
    }

    pub fn set(&mut self, x: i32, y: i32, z: i32, b: Db) {
        if !(0..256).contains(&y) {
            return;
        }
        let c = self.chunk(x >> 4, z >> 4);
        let idx = ((z & 15) * 16 + (x & 15)) as usize;
        c.cols[idx][y as usize] = b;
        let hv = c.height[idx] as i32;
        if blocks_light(b) {
            if y >= hv {
                c.height[idx] = y as u16 + 1;
            }
        } else if y == hv - 1 {
            let col = &c.cols[idx];
            c.height[idx] = (0..y as usize)
                .rev()
                .find(|&k| blocks_light(col[k]))
                .map_or(0, |k| k as u16 + 1);
        }
        self.log.push((x, y, z, b));
    }

    /// `World.getHeightValue`
    pub fn height(&mut self, x: i32, z: i32) -> i32 {
        let c = self.chunk(x >> 4, z >> 4);
        c.height[((z & 15) * 16 + (x & 15)) as usize] as i32
    }

    /// `getBiomeGenForCoords` (chunk biome array)
    pub fn gen_id(&mut self, x: i32, z: i32) -> u16 {
        let c = self.chunk(x >> 4, z >> 4);
        c.gen[((z & 15) * 16 + (x & 15)) as usize]
    }

    /// base temperature of the chunk biome at (x, z)
    pub fn temperature(&mut self, x: i32, z: i32) -> f32 {
        let g = self.gen_id(x, z);
        self.gen_temp.get(&g).copied().unwrap_or(0.8)
    }

    /// `World.getTopSolidOrLiquidBlock`: one above the highest movement-blocking non-leaves block.
    pub(crate) fn top_solid(&mut self, x: i32, z: i32) -> i32 {
        let top = (self.height(x, z) + 16).min(256);
        (1..top)
            .rev()
            .find(|&y| {
                let b = self.get(x, y, z);
                mat_solid(b) && !is_leaves(b)
            })
            .map_or(-1, |y| y + 1)
    }

    pub(crate) fn mark(&mut self, name: &'static str, x: i32, y: i32, z: i32) {
        if self.tracing {
            self.trace.push((name, x, y, z, self.log.len()));
        }
    }

    pub(crate) fn air(&mut self, x: i32, y: i32, z: i32) -> bool {
        is_air(self.get(x, y, z))
    }

    /// Sky light reaches the block (approximation of `getFullBlockLightValue >= 8 || canBlockSeeTheSky`): no opaque
    /// block above it.
    pub(crate) fn lit(&mut self, x: i32, y: i32, z: i32) -> bool {
        let h = self.height(x, z);
        if y >= h {
            return true;
        }
        // nothing at or above the heightmap blocks light, so nothing there is opaque
        !(y + 1..h).any(|k| opaque(self.get(x, k, z)))
    }

    /// `BlockBush.canBlockStay` for grass-type plants (grass, dirt, farmland below).
    pub(crate) fn bush_stay(&mut self, x: i32, y: i32, z: i32) -> bool {
        let b = self.get(x, y - 1, z);
        (is_earth(b) || is(b, db::FARMLAND)) && self.lit(x, y, z)
    }

    /// `BlockDoublePlant.canPlaceBlockAt`: replaceable here, plantable below, air above.
    pub(crate) fn double_place(&mut self, x: i32, y: i32, z: i32) -> bool {
        y < 255
            && mat_replaceable(self.get(x, y, z))
            && self.bush_stay(x, y, z)
            && self.air(x, y + 1, z)
    }

    /// `BlockCactus.canBlockStay`
    pub(crate) fn cactus_stay(&mut self, x: i32, y: i32, z: i32) -> bool {
        for (dx, dz) in [(-1, 0), (1, 0), (0, -1), (0, 1)] {
            if mat_solid(self.get(x + dx, y, z + dz)) {
                return false;
            }
        }
        let b = self.get(x, y - 1, z);
        is(b, db::CACTUS) || is_sand(b)
    }

    /// `BlockReed.canBlockStay` (= `canPlaceBlockAt`)
    pub(crate) fn reed_stay(&mut self, x: i32, y: i32, z: i32) -> bool {
        let b = self.get(x, y - 1, z);
        if is(b, db::REEDS) {
            return true;
        }
        if !(is_earth(b) || is_sand(b)) {
            return false;
        }
        [(-1, 0), (1, 0), (0, -1), (0, 1)]
            .iter()
            .any(|&(dx, dz)| mat(self.get(x + dx, y - 1, z + dz)) == Mat::Water)
    }

    /// `BlockDeadBush.canPlaceBlockOn`: sand, hardened / stained clay, dirt.
    pub(crate) fn deadbush_stay(&mut self, x: i32, y: i32, z: i32) -> bool {
        let b = self.get(x, y - 1, z);
        let id = db::id(b);
        (is_sand(b)
            || is_dirt(b)
            || id == blk::HARDENED_CLAY
            || (blk::CLAY0..blk::CLAY0 + 16).contains(&id))
            && self.lit(x, y, z)
    }

    /// `BlockMushroom.canBlockStay`: mycelium / podzol below, or dark enough and an opaque block below. Light is not
    /// modelled: dark = an opaque block somewhere above.
    pub(crate) fn mushroom_stay(&mut self, x: i32, y: i32, z: i32) -> bool {
        if !(0..256).contains(&y) {
            return false;
        }
        let b = self.get(x, y - 1, z);
        if is(b, blk::MYCELIUM) || is(b, blk::PODZOL) {
            return true;
        }
        !self.lit(x, y, z) && opaque(b)
    }

    /// `BlockVine.canPlaceBlockOnSide`-style test used by `WorldGenVines` (sides 2..5): the neighbour on that side
    /// is a full opaque solid block.
    pub(crate) fn vine_side(&mut self, x: i32, y: i32, z: i32, side: i32) -> bool {
        let (dx, dz) = match side {
            2 => (0, 1),
            3 => (0, -1),
            4 => (1, 0),
            _ => (-1, 0),
        };
        let b = self.get(x + dx, y, z + dz);
        opaque(b) && mat_solid(b)
    }
}

/// `World.getPrecipitationHeight`: one above the highest block that blocks movement or is liquid.
fn precipitation_height(w: &mut DecoWorld, x: i32, z: i32) -> i32 {
    let top = w.height(x, z).max(1) + 16;
    (1..top.min(256))
        .rev()
        .find(|&y| {
            let b = w.get(x, y, z);
            mat_solid(b) || mat_liquid(b)
        })
        .map_or(-1, |y| y + 1)
}

/// The ice / snow pass after decoration (`ChunkGeneratorRealistic.populate`, no shared-Random draws). `border` is
/// the chunk's full border list (oceans included).
/// `instanceof RealisticBiomeOcean` (plain, test and BOP oceans; decorated after ICE, not in the border loop)
pub fn is_ocean_class(class: &str) -> bool {
    matches!(
        class.rsplit('.').next().unwrap_or(""),
        "RealisticBiomeOcean" | "RealisticBiomeBOPOcean"
    )
}

/// RWG2 `populate` after the border loop: the 50 water + 20 lava spring draws (every chunk: dump 2.16, 300 draws
/// between the last decorator and ANIMALS in all 2,304 land chunks; spring blocks not placed), [`post_tail`]
/// (ANIMALS + the Post listeners that reseed the shared Random), then for the deferred BOP oceans (strength > 0.3,
/// `rDecorateAfterIce`) the base biome's decorator (Kelp Forest, Coral Reef) and RWG2's kelp / coral sanitize pass.
pub fn ocean_after_ice(
    w: &mut DecoWorld,
    r: &mut crate::java_random::JavaRandom,
    cx: i32,
    cz: i32,
    border: &[(u16, f32)],
) {
    let bop: Vec<(u16, f32)> = border
        .iter()
        .copied()
        .filter(|&(id, s)| {
            s > 0.3
                && w.p
                    .table
                    .info(id as _)
                    .class
                    .contains("RealisticBiomeBOPOcean")
        })
        .collect();
    for _ in 0..50 {
        r.next_int_bounded(16);
        let t = r.next_int_bounded(120);
        r.next_int_bounded(t + 8);
        r.next_int_bounded(16);
    }
    for _ in 0..20 {
        r.next_int_bounded(16);
        let t = r.next_int_bounded(112);
        let u = r.next_int_bounded(t + 8);
        r.next_int_bounded(u + 8);
        r.next_int_bounded(16);
    }
    post_tail(w, r, cx, cz);
    if bop.is_empty() {
        return;
    }
    if let Some(&e) = w.ocean_entry.get(&(cx, cz)) {
        w.ocean_entry_check.1 += 1;
        if r.state() == e {
            w.ocean_entry_check.0 += 1;
        }
    }
    if w.use_recorded_ocean_entry {
        if let Some(&e) = w.ocean_entry.get(&(cx, cz)) {
            *r = crate::java_random::JavaRandom::new(e ^ 0x5DEECE66D);
        }
    }
    for (id, _) in bop {
        let info = w.p.table.info(id as _);
        let (class, gen) = (info.base_class.clone(), info.gen_id);
        let suffix = class.rsplit('.').next().unwrap_or("").to_string();
        bop::decorate(w, r, &suffix, cx, cz);
        ocean_sanitize(w, cx * 16, cz * 16, gen);
    }
}

fn post_row(id: u16) -> Option<&'static (u16, f32, bool, bool, bool)> {
    post_table::BIOMES
        .binary_search_by_key(&id, |r| r.0)
        .ok()
        .map(|i| &post_table::BIOMES[i])
}

/// Creature spawn list of a gen biome id (rwg2dump 2.19.1): (`getSpawningChance()`, entries (weight, min, max)).
pub fn creatures(id: u16) -> Option<(f32, &'static [spawn_table::Entry])> {
    spawn_table::CREATURES
        .binary_search_by_key(&id, |r| r.0)
        .ok()
        .map(|i| (spawn_table::CREATURES[i].1, spawn_table::CREATURES[i].2))
}

/// Vanilla `SpawnerAnimals.performWorldGenSpawning(world, biome, x0 + 8, z0 + 8, 16, 16, rand)` on the shared
/// Random. The entry is picked with `world.rand` (unknown), so the group size `min + nextInt(1 + max − min)` uses
/// the group-size range carrying the most list weight; returns (that weight share multiplied over the groups,
/// whether every placement test was decided on water, i.e. independent of the modelled world being exact).
/// Placement per creature: up to 4 tries at `getTopSolidOrLiquidBlock` (spawnable → yaw `nextFloat`, stop), then a
/// ±4 walk (`nextInt(5) − nextInt(5)` on x then z, redrawn from the group origin while outside the 16² area).
pub fn world_gen_spawning(w: &mut DecoWorld, r: &mut crate::java_random::JavaRandom, cx: i32, cz: i32) -> (f32, bool) {
    world_gen_spawning_with(w, r, cx, cz, &mut |_, (mn, mx), r| mn as i32 + r.next_int_bounded(1 + mx as i32 - mn as i32))
}

/// Group-size chooser for [`world_gen_spawning_with`].
pub type GroupSize<'a> = dyn FnMut(u32, (u8, u8), &mut crate::java_random::JavaRandom) -> i32 + 'a;

/// [`world_gen_spawning`] with the group size chosen by `size(group index, dominant (min, max), shared Random)`
/// (which must draw the size draw); diagnostics.
pub fn world_gen_spawning_with(
    w: &mut DecoWorld,
    r: &mut crate::java_random::JavaRandom,
    cx: i32,
    cz: i32,
    size: &mut GroupSize,
) -> (f32, bool) {
    let (x0, z0) = (cx * 16 + 8, cz * 16 + 8);
    let Some((chance, list)) = creatures(w.gen_id(cx * 16 + 16, cz * 16 + 16)) else {
        // unknown biome: assume a non-empty list, and nothing beyond the first roll
        return if r.next_float() < 0.1 { (0.0, false) } else { (1.0, true) };
    };
    if list.is_empty() {
        return (1.0, true);
    }
    let total: u32 = list.iter().map(|e| e.0 as u32).sum();
    let mut by: Vec<((u8, u8), u32)> = Vec::new();
    for &(wt, mn, mx) in list {
        match by.iter_mut().find(|b| b.0 == (mn, mx)) {
            Some(b) => b.1 += wt as u32,
            None => by.push(((mn, mx), wt as u32)),
        }
    }
    let &((mn, mx), wt) = by.iter().max_by_key(|b| b.1).unwrap();
    let share = wt as f32 / total as f32;
    let (mut conf, mut water) = (1.0f32, true);
    let mut g = 0;
    while r.next_float() < chance {
        conf *= share;
        let n = size(g, (mn, mx), r);
        g += 1;
        let (ox, oz) = (x0 + r.next_int_bounded(16), z0 + r.next_int_bounded(16));
        let (mut x, mut z) = (ox, oz);
        for _ in 0..n {
            let mut spawned = false;
            for _ in 0..4 {
                if spawned {
                    break;
                }
                let (ok, wet) = creature_spawnable(w, x, z);
                water &= wet;
                if ok {
                    r.next_float();
                    spawned = true;
                }
                x += r.next_int_bounded(5) - r.next_int_bounded(5);
                z += r.next_int_bounded(5) - r.next_int_bounded(5);
                while x < x0 || x >= x0 + 16 || z < z0 || z >= z0 + 16 {
                    x = ox + r.next_int_bounded(5) - r.next_int_bounded(5);
                    z = oz + r.next_int_bounded(5) - r.next_int_bounded(5);
                }
            }
        }
    }
    (conf, water)
}

/// `canCreatureTypeSpawnAtLocation(creature, x, getTopSolidOrLiquidBlock(x, z), z)`: (spawnable, decided by liquid
/// at the spot). Top = first block from the top that blocks movement (not leaves / foliage) + 1.
fn creature_spawnable(w: &mut DecoWorld, x: i32, z: i32) -> (bool, bool) {
    let mut y = 255;
    while y > 0 {
        let b = w.get(x, y, z);
        if mat_solid(b) && !is_leaves(b) {
            break;
        }
        y -= 1;
    }
    let y = y + 1;
    let at = w.get(x, y, z);
    if mat_liquid(at) || is(at, db::BOP_CORAL1) || is(at, db::BOP_CORAL2) {
        return (false, true);
    }
    let below = w.get(x, y - 1, z);
    let ok = opaque(below) && !is(below, blk::BEDROCK) && !opaque(at) && !opaque(w.get(x, y + 1, z));
    (ok, false)
}

/// RWG2 `populate` from ANIMALS to ICE (rwg2dump 2.16 / 2.18 / 2.19 traces): ANIMALS ([`world_gen_spawning`],
/// chunk biome at +16, +16), then the `PopulateChunkEvent.Post` listeners ([`oil`]): Galacticraft (jitter, reseed,
/// 2 doubles), BuildCraft springs (no draws), Forestry hives (no hit assumed), DreamMaster `OilGeneratorFix` (even
/// chunks). A deposit is recorded in `w.oil` ([`oil::chain_confidence`] × the ANIMALS group-size share; dropped
/// when a group was placed on modelled land); returns it.
pub fn post_tail(
    w: &mut DecoWorld,
    r: &mut crate::java_random::JavaRandom,
    cx: i32,
    cz: i32,
) -> Option<oil::OilDeposit> {
    w.post_entry = r.state();
    let (aconf, water) = world_gen_spawning(w, r, cx, cz);
    let exact = water && aconf > 0.0;
    let seed = w.seed;
    oil::gc(r, seed, &mut |x, z| w.gen_id(x, z), cx, cz);
    oil::hives(r, false);
    let d = oil::dm(r, seed, &mut |x, z| w.gen_id(x, z), &oil_row, cx, cz);
    match d {
        Some(d) if exact => {
            let conf = oil::chain_confidence(&border(w.p, w.h, cx, cz), &w.p.table) * aconf;
            w.oil.push(((cx, cz), d, conf));
            Some(d)
        }
        _ => None,
    }
}

/// (rootHeight, DreamMaster blacklist, boost) of a gen biome id (`post_table`; unknown ids: 0.1, no, no).
pub fn oil_row(id: u16) -> (f32, bool, bool) {
    post_row(id).map_or((0.1, false, false), |b| (b.1, b.3, b.4))
}

/// `RealisticBiomeBOPOcean.sanitizeColumn` over x, z in chunk origin −3 ..= +30, y 1..62.
fn ocean_sanitize(w: &mut DecoWorld, x0: i32, z0: i32, gen: u16) {
    let floor = |b: Db| {
        matches!(
            db::id(b),
            blk::DIRT
                | blk::COARSE_DIRT
                | blk::PODZOL
                | blk::SAND
                | blk::RED_SAND
                | blk::STONE
                | blk::GRAVEL
                | blk::GRASS
                | blk::BOP_MUD
        ) || db::id(b) == db::CLAY
    };
    let water = |b: Db| db::id(b) == blk::WATER;
    let kelp = |b: Db| db::id(b) == db::BOP_CORAL1 && (8..=11).contains(&db::meta(b));
    for x in x0 - 3..=x0 + 30 {
        for z in z0 - 3..=z0 + 30 {
            let mut y = 1;
            while y < 63 {
                let b = w.get(x, y, z);
                if kelp(b) {
                    let mut top = y;
                    while top + 1 < 63 && kelp(w.get(x, top + 1, z)) {
                        top += 1;
                    }
                    let below = w.get(x, y - 1, z);
                    let valid = w.gen_id(x, z) == gen
                        && (kelp(below) || floor(below))
                        && water(w.get(x, top + 1, z));
                    if !valid {
                        for k in y..=top {
                            w.set(x, k, z, blk::WATER as Db);
                        }
                    } else if y == top {
                        w.set(x, y, z, db::mk(db::BOP_CORAL1, 11));
                    } else {
                        w.set(x, y, z, db::mk(db::BOP_CORAL1, 8));
                        for k in y + 1..top {
                            w.set(x, k, z, db::mk(db::BOP_CORAL1, 9));
                        }
                        w.set(x, top, z, db::mk(db::BOP_CORAL1, 10));
                    }
                    y = top;
                } else if matches!(db::id(b), db::BOP_CORAL1 | db::BOP_CORAL2) {
                    let below = w.get(x, y - 1, z);
                    if !(w.gen_id(x, z) == gen && water(w.get(x, y + 1, z)) && floor(below)) {
                        w.set(x, y, z, blk::WATER as Db);
                    }
                }
                y += 1;
            }
        }
    }
}

pub fn snow_pass(w: &mut DecoWorld, cx: i32, cz: i32, border: &[(u16, f32)]) {
    use crate::noise::NoiseGenerator as _;
    let mut snow = 0f32;
    for &(id, s) in border {
        if w.p.table.info(id as _).temperature < 0.15 {
            snow -= 0.6 * s;
        } else {
            snow += 0.6 * s;
        }
    }
    if snow >= 0.59 {
        return;
    }
    let (x0, z0) = (cx * 16 + 8, cz * 16 + 8);
    for i in 0..16 {
        for j in 0..16 {
            let (x, z) = (x0 + i, z0 + j);
            if w.temperature(x, z) >= 0.15 {
                continue;
            }
            let s = if snow < -0.59 {
                -1.0
            } else {
                w.p.perlin().noise2(x as f32 / 3.0, z as f32 / 3.0) + snow
            };
            if s >= 0.0 {
                continue;
            }
            let y = precipitation_height(w, x, z);
            let b1 = w.get(x, y, z);
            let b2 = w.get(x, y - 1, z);
            if is_water(b2) {
                w.set(x, y - 1, z, blk::ICE as Db);
            }
            // BlockSnow.canPlaceBlockAt (the block below, read after the freeze)
            let below = w.get(x, y - 1, z);
            let can = match mat(below) {
                Mat::Ice | Mat::PackedIce => false,
                Mat::Leaves => true,
                Mat::Snow => db::meta(below) & 7 == 7,
                _ => opaque(below) && mat_solid(below),
            };
            if can
                && !matches!(mat(b2), Mat::Ice | Mat::Water)
                && y > 62
                && !is(b1, blk::SNOW_LAYER)
                && !is(b2, blk::PACKED_ICE)
            {
                w.set(x, y, z, blk::SNOW_LAYER as Db);
            }
        }
    }
}

/// Applies the populate prefix changes (`population::B`) before decoration.
pub fn apply_prefix(w: &mut DecoWorld, log: &[(i32, i32, i32, crate::population::B)]) {
    use crate::population::B;
    for &(x, y, z, b) in log {
        let d = match b {
            B::Air => AIR,
            B::Water => blk::WATER as Db,
            B::Lava => blk::LAVA as Db,
            B::Stone => blk::STONE as Db,
            B::Dirt => blk::DIRT as Db,
            B::Gravel => blk::GRAVEL as Db,
            B::Cobble => blk::COBBLE as Db,
            B::Mossy => db::mk(db::MOSSY, 0),
            B::Solid | B::Abyssal | B::Emerald | B::Chest | B::Spawner => db::mk(db::SOLID, 0),
            // never logged (structure pieces write quietly)
            B::Web => continue,
        };
        w.set(x, y, z, d);
    }
}

/// Border strengths of chunk `(cx, cz)` in RWG2 order (ascending rwg id; 9×9 `getBiomeDataAt`, 0.01234569 each).
pub fn border(p: &BiomeProvider, h: &dyn Heights, cx: i32, cz: i32) -> Vec<(u16, f32)> {
    let mut cnt: std::collections::BTreeMap<u16, i32> = std::collections::BTreeMap::new();
    for bx in -4..=4 {
        for bz in -4..=4 {
            *cnt.entry(
                p.biome_data(cx * 16 + 24 + bx * 16, cz * 16 + 24 + bz * 16, h)
                    .data,
            )
            .or_default() += 1;
        }
    }
    cnt.into_iter()
        .map(|(id, n)| {
            let mut s = 0f32;
            for _ in 0..n {
                s += 0.012_345_69;
            }
            (id, s.min(1.0))
        })
        .collect()
}

pub use biomes::{decorate_biome, r_decorate};
