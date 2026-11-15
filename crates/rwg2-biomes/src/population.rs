//! The start of `ChunkGeneratorRealistic.populate` (RWG2), up to and including the dirt and gravel blobs:
//! structures → underground water lake (1/10) → lava lake (1/18) → 8 vanilla dungeon attempts → clay in strong
//! rivers → 10 dirt blobs (`WorldGenMinable(dirt, 32)`, y < 64) → 5 gravel blobs. All on the populate Random
//! (`setSeed(seed)`, two odd longs, `setSeed(cx·a + cz·b ^ seed)`).
//!
//! These run before GregTech's ore veins, so a blob in GT's 9-block test column at (chunkX + 7, minY..+8,
//! chunkZ + 9) turns stone into dirt/gravel and can make GT reject a vein the terrain model accepts.
//!
//! The draw count is exact while nothing world- or loot-dependent consumes the Random: a structure piece that
//! intersects the chunk's population box (mineshaft corridors, stronghold rooms, village houses use it per
//! block) or a dungeon that actually gets placed (chest loot) makes the rest unknowable → [`Outcome::Uncertain`].
//! Lake/dungeon validity is decided on the pre-population world (terrain, mountain-chain rivers, caves).
//!
//! `PopulateChunkEvent.Pre` runs first, on the same Random (GTNH 2.9: Railcraft 9.17.36 `GeodePopulator` +
//! `QuarryPopulator`, NHCore 2.9.99 `NH_GeodePopulator` + `NH_QuarryPopulator`, BuildCraft 7.1.65 `OilPopulate`
//! with `oilWellGenerationRate=0`): one `nextDouble` per biome-gated populator, 2 ints + 2–3 doubles for oil; a
//! geode that actually generates draws per placed block → uncertain.

use crate::column::{CarvedChunk, Pre};
use crate::dungeons::Terrain;
use crate::gtorder::Caves;
use crate::java_random::JavaRandom;
use crate::jmath::FxMap;
use std::sync::OnceLock;

/// Block class the population code distinguishes.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum B {
    Air,
    Water,
    Lava,
    /// `Blocks.stone` (what `WorldGenMinable` replaces)
    Stone,
    /// other solid blocks (surface layers, bedrock, ...)
    Solid,
    Dirt,
    Gravel,
    /// Railcraft abyssal stone (geode shell)
    Abyssal,
    /// dungeon walls/floor (cobblestone, mossy cobblestone)
    Cobble,
    /// RWG2 emerald ore
    Emerald,
    /// dungeon floor, mossy cobblestone
    Mossy,
    /// dungeon chest
    Chest,
    /// dungeon mob spawner
    Spawner,
    /// non-solid, non-air structure blocks (cobweb, rail, torch): not solid for dungeon / lake checks
    Web,
}

impl B {
    fn solid(self) -> bool {
        matches!(
            self,
            B::Stone
                | B::Solid
                | B::Dirt
                | B::Gravel
                | B::Abyssal
                | B::Cobble
                | B::Emerald
                | B::Mossy
                | B::Chest
                | B::Spawner
        )
    }
    /// Code used by the viewer's x-ray (`xray_pop`): 0 = unchanged.
    pub fn code(self) -> u8 {
        match self {
            B::Stone | B::Solid => 0,
            B::Water => 1,
            B::Lava => 2,
            B::Air => 3,
            B::Dirt => 4,
            B::Gravel => 5,
            B::Abyssal => 6,
            B::Cobble => 7,
            B::Emerald => 8,
            B::Mossy => 9,
            B::Chest => 10,
            B::Spawner => 11,
            B::Web => 0,
        }
    }
    fn liquid(self) -> bool {
        matches!(self, B::Water | B::Lava)
    }
}

/// Result of [`PopModel::populate`] for one chunk.
#[derive(Clone, Debug)]
pub enum Outcome {
    /// Every draw up to the gravel blobs known: the block changes (later writes win).
    Certain(Vec<(i32, i32, i32, B)>),
    /// The rest is unknown; the changes made before that point are given.
    Uncertain(&'static str, Vec<(i32, i32, i32, B)>),
}

/// Surface-pass depth treated as non-stone (grass/dirt/sand above stone).
pub const SURFACE: i32 = 4;

fn sin_table() -> &'static [f32] {
    static T: OnceLock<Vec<f32>> = OnceLock::new();
    T.get_or_init(|| {
        (0..65536)
            .map(|i| (i as f64 * std::f64::consts::PI * 2.0 / 65536.0).sin() as f32)
            .collect()
    })
}
#[inline]
pub(crate) fn msin(f: f32) -> f32 {
    sin_table()[((f * 10430.378) as i32 & 65535) as usize]
}
#[inline]
pub(crate) fn mcos(f: f32) -> f32 {
    sin_table()[((f * 10430.378 + 16384.0) as i32 & 65535) as usize]
}
#[inline]
pub(crate) fn floor_d(d: f64) -> i32 {
    let i = d as i32;
    if d < i as f64 {
        i - 1
    } else {
        i
    }
}

/// Pre-population world plus pending changes.
struct World<'m, 'a> {
    m: &'m mut PopModel<'a>,
    over: FxMap<(i32, i32, i32), B>,
    log: Vec<(i32, i32, i32, B)>,
}

impl Drop for World<'_, '_> {
    /// `stateful_world`: this populate's writes stay visible to the later ones (ordered replays)
    fn drop(&mut self) {
        if self.m.stateful_world {
            if self.m.world_mem.len() > 4_000_000 {
                self.m.world_mem.clear();
            }
            let over = std::mem::take(&mut self.over);
            self.m.world_mem.extend(over);
        }
    }
}

impl World<'_, '_> {
    fn get(&mut self, x: i32, y: i32, z: i32) -> B {
        if let Some(&b) = self.over.get(&(x, y, z)) {
            return b;
        }
        if let Some(&b) = self.m.world_mem.get(&(x, y, z)) {
            return b;
        }
        self.m.base(x, y, z)
    }
    /// Writes a block the later populate steps see, without logging it (structure pieces: not shown by x-ray).
    fn set_quiet(&mut self, x: i32, y: i32, z: i32, b: B) {
        if (0..256).contains(&y) {
            self.over.insert((x, y, z), b);
        }
    }
    fn set(&mut self, x: i32, y: i32, z: i32, b: B) {
        if !(0..256).contains(&y) {
            return;
        }
        self.over.insert((x, y, z), b);
        self.log.push((x, y, z, b));
    }
}

/// BiomeDictionary gates of the `PopulateChunkEvent.Pre` consumers, per in-game biome id.
#[derive(Clone, Default)]
pub struct PreGates {
    /// Railcraft geode: WATER and the name has no "river"
    pub rc_geode: Vec<bool>,
    /// Railcraft quarry: FOREST and not SNOWY
    pub rc_quarry: Vec<bool>,
    /// NHCore geode: MOUNTAIN
    pub nh_geode: Vec<bool>,
    /// NHCore quarry: FOREST and SNOWY
    pub nh_quarry: Vec<bool>,
    /// BuildCraft increasedOilIDs (SANDY, "Taiga"): one more `nextDouble`
    pub oil: Vec<bool>,
}

impl PreGates {
    /// From `(id, name, BiomeDictionary types)` rows (`climate.json`).
    pub fn new<'s>(rows: impl IntoIterator<Item = (u16, &'s str, Vec<&'s str>)>) -> Self {
        let mut g = PreGates::default();
        for (id, name, dict) in rows {
            let i = id as usize;
            for v in [
                &mut g.rc_geode,
                &mut g.rc_quarry,
                &mut g.nh_geode,
                &mut g.nh_quarry,
                &mut g.oil,
            ] {
                if v.len() <= i {
                    v.resize(i + 1, false);
                }
            }
            let has = |t: &str| dict.contains(&t);
            // Forge 1.7.10 `Type.WATER` = subtags OCEAN and RIVER; `isBiomeOfType` needs all subtags
            g.rc_geode[i] = (has("WATER") || (has("OCEAN") && has("RIVER")))
                && !name.to_lowercase().contains("river");
            g.rc_quarry[i] = has("FOREST") && !has("SNOWY");
            g.nh_geode[i] = has("MOUNTAIN");
            g.nh_quarry[i] = has("FOREST") && has("SNOWY");
            // BuildCraft resolves the list in preInit, before the other mods fill the BiomeDictionary: only
            // vanilla biomes can match (observed: no third draw in RWG2/BOP biomes)
            g.oil[i] = i < 40 && (has("SANDY") || name.eq_ignore_ascii_case("taiga"));
        }
        g
    }
    fn get(v: &[bool], id: u16) -> bool {
        v.get(id as usize).copied().unwrap_or(false)
    }
    /// `v[id]` (false if out of range).
    pub fn get_pub(v: &[bool], id: u16) -> bool {
        Self::get(v, id)
    }
}

/// One Railcraft / NHCore abyssal geode (centre, kind) with its dark ores (exact on the modelled world).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Geode {
    pub x: i32,
    pub y: i32,
    pub z: i32,
    /// Railcraft ocean geode (`GeodePopulator`) vs NHCore mountain geode (`NH_GeodePopulator`)
    pub ocean: bool,
    pub diamond: u16,
    pub emerald: u16,
    pub lapis: u16,
}

/// Block changes of one chunk's populate prefix.
pub type Changes = std::rc::Rc<FxMap<(i32, i32, i32), B>>;

/// Population model for one world (caches pre-population chunks and structure boxes).
pub struct PopModel<'a> {
    pub t: Terrain<'a>,
    seed: i64,
    caves: Caves,
    a: i64,
    b: i64,
    carved: FxMap<(i32, i32), Box<CarvedChunk>>,
    surf_table: Option<crate::surface::SurfaceTable>,
    surf: FxMap<(i32, i32), std::rc::Rc<crate::surface::SurfaceChunk>>,
    /// x/z boxes `[x0, z0, x1, z1]` of structure pieces whose `addComponentParts` draws from the Random and is
    /// not modelled (strongholds)
    pieces: Vec<[i32; 4]>,
    /// mineshafts whose corridor draws are replayed ([`mineshaft_parts`])
    shafts: Vec<Shaft>,
    /// village start chunks (pieces not ported: a square around the start is uncertain)
    villages: Vec<(i32, i32)>,
    /// chunks around a village start that count as "village pieces here"
    pub village_radius: i32,
    /// `PopulateChunkEvent.Populate` LAKE / LAVA allowed (Forge event, can be denied by mods)
    pub lakes: bool,
    pub lava_lakes: bool,
    pub gates: PreGates,
    /// model the `PopulateChunkEvent.Pre` consumers
    pub pre: bool,
    /// diagnostics of the last [`PopModel::populate`] (see `F_*`)
    pub flags: u32,
    /// geode generated by the last [`PopModel::populate`] / [`PopModel::geode_at`]
    pub last_geode: Option<Geode>,
    /// vanilla dungeons placed by the last [`PopModel::populate`]
    pub last_dungeons: Vec<Dungeon>,
    /// [`PopModel::populate`] stops after the dungeon attempts (see [`PopModel::dungeons_at`])
    stop_after_dungeons: bool,
    /// [`PopModel::populate`] stops after the two lake rolls (see [`PopModel::lakes_at`])
    stop_after_lakes: bool,
    /// test hook: the k-th placed mineshaft chest's contents consume `chest_probe[k]` draws (no loot table needed);
    /// every hit is logged in `chest_hits`
    #[doc(hidden)]
    pub chest_probe: Option<Vec<u32>>,
    #[doc(hidden)]
    pub chest_hits: Vec<(i32, i32, i32, bool)>,
    /// Random state right after the structure pieces (before the LAKE event) of the last populate
    #[doc(hidden)]
    pub struct_state: i64,
    /// Random state after the `PopulateChunkEvent.Pre` listeners of the last populate
    #[doc(hidden)]
    pub pre_state: i64,
    /// Random state at named points of the last populate (LAVA, DUNGEON, afterDungeon, decPre)
    #[doc(hidden)]
    pub marks: Vec<(&'static str, i64)>,
    /// carry mineshaft piece removal (failed liquid test) and placed spawners across populates, like the game.
    /// Off by default: per-chunk state only, so viewer cells and searches give one answer in any order. On for
    /// ordered replays (`deco::view::replay_ordered`): dump 2.16 then matches 5 more loop entries.
    pub stateful_shafts: bool,
    /// keep every populate's block writes (lakes, dungeons, blobs, corridors) for the later populates, like the
    /// game's world; for ordered replays only (with `stateful_shafts`)
    pub stateful_world: bool,
    world_mem: FxMap<(i32, i32, i32), B>,
    /// order key of overlapping mineshaft starts ([`shaft_scan_order`]; None: a chunk two starts reach is uncertain)
    pub shaft_order: Option<fn((i32, i32)) -> i64>,
    /// minecart chests the last populate placed (position, contents; `loot::LootTable::mineshaft`)
    pub mine_chests: Vec<(i32, i32, i32, crate::loot::Chest)>,
    /// lakes placed by the last [`PopModel::populate`]
    pub last_lakes: Vec<Lake>,
    /// the last [`PopModel::populate`] reached RWG2's border-biome decoration loop with every draw known: the
    /// shared Random there and the number of prefix changes made before it (`Outcome` log prefix)
    pub last_deco_start: Option<(JavaRandom, usize)>,
    /// cached [`PopModel::populate`] results per chunk (`None`: uncertain)
    done: FxMap<(i32, i32), Option<Changes>>,
}

pub const F_LAKE: u32 = 1;
pub const F_LAKE_PLACED: u32 = 2;
pub const F_LAVA: u32 = 4;
pub const F_LAVA_PLACED: u32 = 8;
pub const F_CLAY: u32 = 16;
pub const F_RC_GEODE: u32 = 32;
pub const F_RC_QUARRY: u32 = 64;
pub const F_NH_GEODE: u32 = 128;
pub const F_NH_QUARRY: u32 = 256;
pub const F_OIL: u32 = 512;
/// a `RealisticBiomeSupport` biome (BOP / vanilla base) has border strength > 0.3: its own `BiomeDecorator` runs
/// later in populate (vanilla `generateOres`: 20 dirt blobs, 10 gravel blobs over y 0..255) on a Random state that
/// depends on tree/plant decoration → extra blobs not modelled
pub const F_SUPPORT: u32 = 1024;
/// same for `RealisticBiomeOcean` (`rDecorateAfterIce`, if the ocean decorates its base biome)
pub const F_OCEAN: u32 = 2048;
/// the first decorating support biome's own dirt/gravel blobs are modelled
pub const F_DECO_MODELLED: u32 = 4096;
/// a Railcraft/NHCore geode generated in this chunk
pub const F_GEODE: u32 = 16384;
/// some later dirt/gravel source (another support biome, a vanilla-base support biome, BOP ocean, zinc gravel hit,
/// oasis edits) is not modelled
pub const F_DECO_UNKNOWN: u32 = 8192;
/// a vanilla dungeon was placed (room, chests and loot draws modelled)
pub const F_DUNGEON: u32 = 32768;

/// A placed `WorldGenLakes` lake (RWG2: water from y < 50, lava from y < 52, so always underground).
#[derive(Clone, Debug, PartialEq)]
pub struct Lake {
    /// centre of the liquid cells (rounded)
    pub x: i32,
    pub z: i32,
    /// liquid y range
    pub y0: i32,
    pub y1: i32,
    pub lava: bool,
    /// liquid blocks
    pub cells: u32,
    /// the lake touches pre-population air outside itself (a cave or a ravine: reachable without digging)
    pub open: bool,
    /// populating chunk
    pub cx: i32,
    pub cz: i32,
}

/// A placed vanilla dungeon: spawner at (x, y, z), room half-extents (l, i1), chests with their loot.
#[derive(Clone, Debug)]
pub struct Dungeon {
    pub x: i32,
    pub y: i32,
    pub z: i32,
    pub l: i32,
    pub i1: i32,
    pub chests: Vec<(i32, i32, i32, crate::loot::Chest)>,
}

impl<'a> PopModel<'a> {
    pub fn new(
        p: &'a crate::provider::BiomeProvider,
        h: &'a dyn crate::provider::Heights,
        seed: i64,
    ) -> Self {
        let mut r = JavaRandom::new(seed);
        let a = r.next_long() / 2 * 2 + 1;
        let b = r.next_long() / 2 * 2 + 1;
        PopModel {
            t: Terrain::new(p, h),
            seed,
            caves: Caves::new(seed, true),
            a,
            b,
            carved: FxMap::default(),
            surf_table: None,
            surf: FxMap::default(),
            pieces: Vec::new(),
            shafts: Vec::new(),
            villages: Vec::new(),
            village_radius: 3,
            lakes: true,
            lava_lakes: true,
            gates: PreGates::default(),
            pre: true,
            flags: 0,
            done: FxMap::default(),
            last_geode: None,
            last_dungeons: Vec::new(),
            stop_after_dungeons: false,
            stop_after_lakes: false,
            chest_probe: None,
            shaft_order: Some(shaft_scan_order),
            stateful_shafts: false,
            stateful_world: false,
            world_mem: FxMap::default(),
            chest_hits: Vec::new(),
            struct_state: 0,
            pre_state: 0,
            marks: Vec::new(),
            mine_chests: Vec::new(),
            last_deco_start: None,
            last_lakes: Vec::new(),
        }
    }

    /// Loads mineshaft / stronghold pieces and village starts that can reach chunks `cx0..=cx1, cz0..=cz1`.
    pub fn load_structures(&mut self, cx0: i32, cz0: i32, cx1: i32, cz1: i32) {
        use crate::mineshaft as ms;
        let s = ms::Seeds::new(self.seed);
        let m = 8;
        for (sx, sz) in ms::starts_in(&s, cx0 - m, cz0 - m, cx1 + m, cz1 + m) {
            if self.shafts.iter().any(|sh| sh.start == (sx, sz)) {
                continue;
            }
            if let Some(sh) = ms::mineshaft(&s, sx, sz, false) {
                let n = sh.pieces.len();
                self.shafts.push(Shaft {
                    start: (sx, sz),
                    pieces: sh.pieces,
                    removed: vec![false; n],
                    spawner: vec![false; n],
                });
            }
        }
        for c in crate::structures::stronghold_ring(self.seed) {
            for pc in crate::stronghold_pieces::pieces(self.seed, c) {
                self.pieces
                    .push([pc.bbox[0], pc.bbox[2], pc.bbox[3], pc.bbox[5]]);
            }
        }
        let (p, h) = (self.t.p, self.t.h);
        for v in
            crate::villages::villages_in_rect(p, h, self.seed, cx0 - m, cz0 - m, cx1 + m, cz1 + m)
        {
            if v.viable_exact {
                self.villages.push((v.cx, v.cz));
            }
        }
    }

    fn chunk(&mut self, cx: i32, cz: i32) -> Option<&CarvedChunk> {
        if !self.carved.contains_key(&(cx, cz)) {
            if self.carved.len() > 2048 {
                self.carved.clear();
            }
            let (p, h) = (self.t.p, self.t.h);
            let c = self.t.chunk(cx, cz)?.clone();
            let cc = CarvedChunk::new(p, h, &c, cx, cz, Some(&self.caves));
            self.carved.insert((cx, cz), Box::new(cc));
        }
        self.carved.get(&(cx, cz)).map(|b| &**b)
    }

    /// Mineshaft starts whose pieces reach chunk `(cx, cz)`'s population box (loaded structures only).
    pub fn overlapping_starts(&self, cx: i32, cz: i32) -> Vec<(i32, i32)> {
        let sbb = [cx * 16 + 8, cz * 16 + 8, cx * 16 + 23, cz * 16 + 23];
        self.shafts
            .iter()
            .filter(|sh| {
                sh.pieces.iter().any(|p| {
                    p.bb[0] <= sbb[2] && p.bb[3] >= sbb[0] && p.bb[2] <= sbb[3] && p.bb[5] >= sbb[1]
                })
            })
            .map(|sh| sh.start)
            .collect()
    }

    /// Exact painted pre-population blocks of chunk `(cx, cz)` (`surface::surface_chunk`, caves dug).
    pub fn surface_chunk(
        &mut self,
        cx: i32,
        cz: i32,
    ) -> Option<std::rc::Rc<crate::surface::SurfaceChunk>> {
        if let Some(s) = self.surf.get(&(cx, cz)) {
            return Some(s.clone());
        }
        let (p, h) = (self.t.p, self.t.h);
        if self.surf_table.is_none() {
            self.surf_table = Some(crate::surface::SurfaceTable::new(&p.table));
        }
        let c = self.t.chunk(cx, cz)?.clone();
        self.chunk(cx, cz)?;
        let cc = self.carved.get(&(cx, cz))?;
        let sc = std::rc::Rc::new(crate::surface::surface_chunk(
            p,
            h,
            self.surf_table.as_ref()?,
            &c,
            cx,
            cz,
            cc,
        ));
        if self.surf.len() > 64 {
            self.surf.clear();
        }
        self.surf.insert((cx, cz), sc.clone());
        Some(sc)
    }

    /// Pre-population block.
    pub fn base(&mut self, x: i32, y: i32, z: i32) -> B {
        if y < 0 {
            return B::Solid;
        }
        if y >= 256 {
            return B::Air;
        }
        let top = self.t.top(x, z);
        let Some(c) = self.chunk(x >> 4, z >> 4) else {
            return B::Stone;
        };
        let (lx, lz) = (x & 15, z & 15);
        match c.pre(lx, y, lz) {
            Pre::Water => B::Water,
            Pre::Air => {
                if c.air(lx, y, lz) {
                    B::Air
                } else {
                    B::Solid
                }
            }
            Pre::Stone => {
                if c.air(lx, y, lz) {
                    // vanilla digBlock: lava below y 10
                    if y < 10 {
                        B::Lava
                    } else {
                        B::Air
                    }
                } else if y < 5 || top.is_some_and(|t| y > t - SURFACE) {
                    B::Solid
                } else {
                    B::Stone
                }
            }
        }
    }

    /// The populate Random of chunk `(cx, cz)`.
    pub fn rand(&self, cx: i32, cz: i32) -> JavaRandom {
        JavaRandom::new(
            (cx as i64)
                .wrapping_mul(self.a)
                .wrapping_add((cz as i64).wrapping_mul(self.b))
                ^ self.seed,
        )
    }

    fn gen_at(&mut self, x: i32, z: i32) -> u16 {
        self.t.column(x, z).map(|c| c.1).unwrap_or(0)
    }

    /// Populates chunk `(cx, cz)` up to the gravel blobs.
    pub fn populate(&mut self, cx: i32, cz: i32) -> Outcome {
        let (x, z) = (cx * 16, cz * 16);
        // MapGenStructure.generateStructuresInChunk box: (x + 8, z + 8) .. + 15
        let (bx0, bz0, bx1, bz1) = (x + 8, z + 8, x + 23, z + 23);
        if self
            .pieces
            .iter()
            .any(|b| b[0] <= bx1 && b[2] >= bx0 && b[1] <= bz1 && b[3] >= bz0)
        {
            return Outcome::Uncertain("structure piece", Vec::new());
        }
        let vr = self.village_radius;
        if self
            .villages
            .iter()
            .any(|&(vx, vz)| (vx - cx).abs() <= vr && (vz - cz).abs() <= vr)
        {
            return Outcome::Uncertain("village", Vec::new());
        }
        // mineshafts: one start's corridors are replayed; two starts here = HashMap order unknown
        let sbb = [x + 8, z + 8, x + 23, z + 23];
        let hit = |p: &crate::mineshaft::Piece| {
            p.bb[0] <= sbb[2] && p.bb[3] >= sbb[0] && p.bb[2] <= sbb[3] && p.bb[5] >= sbb[1]
        };
        let shafts: Vec<usize> = (0..self.shafts.len())
            .filter(|&i| self.shafts[i].pieces.iter().any(hit))
            .collect();
        // fresh per-populate logs (also when giving up below)
        self.mine_chests.clear();
        self.chest_hits.clear();
        self.marks.clear();
        let mut shafts = shafts;
        if shafts.len() > 1 {
            // MapGenStructure.structureMap iteration order of the starts
            match &self.shaft_order {
                Some(f) => shafts.sort_by_key(|&i| f(self.shafts[i].start)),
                None => return Outcome::Uncertain("structure piece", Vec::new()),
            }
        }
        self.flags = 0;
        self.last_geode = None;
        self.last_dungeons.clear();
        self.last_lakes.clear();
        self.last_deco_start = None;
        let mut r = self.rand(cx, cz);
        let p = self.t.p;
        let h = self.t.h;
        let continental = p.is_continental();
        let pre = self.pre;
        let mut w = World {
            m: self,
            over: FxMap::default(),
            log: Vec::new(),
        };
        if pre {
            pre_event(&mut w, &mut r, cx, cz);
        }
        w.m.pre_state = r.state();
        for &i in &shafts {
            if !mineshaft_parts(&mut w, &mut r, i, sbb) {
                return Outcome::Uncertain("mineshaft chest", Vec::new());
            }
        }
        w.m.struct_state = r.state();
        if w.m.lakes && r.next_int_bounded(10) == 0 {
            let lx = x + r.next_int_bounded(16) + 8;
            let ly = r.next_int_bounded(50);
            let lz = z + r.next_int_bounded(16) + 8;
            let lava =
                continental && p.lava_cave_coordinates(lx, lz, h) != crate::continental::NONE;
            w.m.flags |= F_LAKE;
            let s = w.log.len();
            if lake(
                &mut w,
                &mut r,
                lx,
                ly,
                lz,
                if lava { B::Lava } else { B::Water },
            ) {
                w.m.flags |= F_LAKE_PLACED;
                lake_record(&mut w, s, cx, cz);
            }
        }
        w.m.marks.push(("LAVA", r.state()));
        if w.m.lava_lakes && r.next_int_bounded(18) == 0 {
            let lx = x + r.next_int_bounded(16) + 8;
            let b = r.next_int_bounded(45) + 8;
            let ly = r.next_int_bounded(b);
            let lz = z + r.next_int_bounded(16) + 8;
            if ly < 64 || r.next_int_bounded(10) == 0 {
                w.m.flags |= F_LAVA;
                let s = w.log.len();
                if lake(&mut w, &mut r, lx, ly, lz, B::Lava) {
                    w.m.flags |= F_LAVA_PLACED;
                    lake_record(&mut w, s, cx, cz);
                }
            }
        }
        if w.m.stop_after_lakes {
            return Outcome::Certain(std::mem::take(&mut w.log));
        }
        w.m.marks.push(("DUNGEON", r.state()));
        for _ in 0..8 {
            let dx = x + r.next_int_bounded(16) + 8;
            let dy = r.next_int_bounded(128);
            let dz = z + r.next_int_bounded(16) + 8;
            if let Some((l, i1)) = dungeon_valid(&mut w, &mut r, dx, dy, dz) {
                let d = dungeon_build(&mut w, &mut r, dx, dy, dz, l, i1);
                w.m.flags |= F_DUNGEON;
                w.m.last_dungeons.push(d);
            }
        }
        if w.m.stop_after_dungeons {
            return Outcome::Certain(std::mem::take(&mut w.log));
        }
        w.m.marks.push(("afterDungeon", r.state()));
        // OreGenEvent.Pre; clay where the river is strong
        let river = -p.river_strength(x + 16, z + 16);
        if river > 0.85 {
            w.m.flags |= F_CLAY;
            for _ in 0..3 {
                let _cx = x + r.next_int_bounded(16);
                let _cy = 53 + r.next_int_bounded(15);
                let _cz = z + r.next_int_bounded(16);
                // DecoClay: fixed draws (1 float, 2 ints, 21 doubles); only touches grass/dirt/sand/gravel
                r.next_float();
                r.next_int_bounded(3);
                r.next_int_bounded(3);
                for _ in 0..=20 {
                    r.next_double();
                }
            }
        }
        for _ in 0..10 {
            let bx = x + r.next_int_bounded(16);
            let by = r.next_int_bounded(64);
            let bz = z + r.next_int_bounded(16);
            minable(&mut w, &mut r, bx, by, bz, 32, B::Dirt);
        }
        for _ in 0..5 {
            let bx = x + r.next_int_bounded(16);
            let by = r.next_int_bounded(64);
            let bz = z + r.next_int_bounded(16);
            minable(&mut w, &mut r, bx, by, bz, 32, B::Gravel);
        }
        w.m.marks.push(("decPre", r.state()));
        decorate_phase(&mut w, &mut r, cx, cz, river);
        let log = std::mem::take(&mut w.log);
        drop(w);
        self.flags |= self.decorator_flags(cx, cz);
        Outcome::Certain(log)
    }

    /// Vanilla dungeons chunk `(cx, cz)` places (populate up to the 8 dungeon attempts; empty for chunks a structure
    /// piece or village makes uncertain). Structures must be loaded for the chunk.
    pub fn dungeons_at(&mut self, cx: i32, cz: i32) -> Vec<Dungeon> {
        self.stop_after_dungeons = true;
        self.populate(cx, cz);
        self.stop_after_dungeons = false;
        std::mem::take(&mut self.last_dungeons)
    }

    /// Lakes chunk `(cx, cz)` places (populate up to the two lake rolls; empty for chunks a structure piece or village
    /// makes uncertain). Structures must be loaded for the chunk.
    pub fn lakes_at(&mut self, cx: i32, cz: i32) -> Vec<Lake> {
        self.stop_after_lakes = true;
        self.populate(cx, cz);
        self.stop_after_lakes = false;
        std::mem::take(&mut self.last_lakes)
    }

    /// Whether chunk `(cx, cz)` can roll a lake at all, from the draws before the rolls: `PopulateChunkEvent.Pre`
    /// gate combinations over the chunk's possible biomes (`ids`: chunk centre, `oil`: the oil sample point). A
    /// geode roll that passes needs terrain → `true`.
    pub fn lake_roll_possible(&self, cx: i32, cz: i32, ids: &[u16], oil: &[u16]) -> bool {
        let g = &self.gates;
        let mut combos: Vec<[bool; 4]> = Vec::new();
        for &b in ids {
            let c = [
                PreGates::get(&g.rc_geode, b),
                PreGates::get(&g.rc_quarry, b),
                PreGates::get(&g.nh_geode, b),
                PreGates::get(&g.nh_quarry, b),
            ];
            if !combos.contains(&c) {
                combos.push(c);
            }
        }
        let oils: Vec<bool> = {
            let mut v: Vec<bool> = oil.iter().map(|&b| PreGates::get(&g.oil, b)).collect();
            v.sort();
            v.dedup();
            v
        };
        for c in &combos {
            for &o in &oils {
                let mut r = self.rand(cx, cz);
                if self.pre {
                    if c[0] && r.next_double() <= 0.3 {
                        return true;
                    }
                    if c[1] {
                        r.next_double();
                    }
                    if c[2] && r.next_double() <= 0.3 {
                        return true;
                    }
                    if c[3] {
                        r.next_double();
                    }
                    r.next_int_bounded(16);
                    r.next_int_bounded(16);
                    r.next_double();
                    r.next_double();
                    if o {
                        r.next_double();
                    }
                }
                if self.lakes && r.next_int_bounded(10) == 0 {
                    return true;
                }
                if self.lava_lakes {
                    // the water roll failed: its draw is the only one consumed
                    if r.next_int_bounded(18) == 0 {
                        return true;
                    }
                }
            }
        }
        false
    }

    /// All lakes populated by chunks `[cx0, cx1] × [cz0, cz1]` (loads structures; skips chunks whose RNG can't
    /// roll a lake before generating terrain).
    pub fn lakes_in_rect(&mut self, cx0: i32, cz0: i32, cx1: i32, cz1: i32) -> Vec<Lake> {
        self.load_structures(cx0, cz0, cx1, cz1);
        let grid = crate::chunkgen::ChunkGen::sample_window_grid(
            self.t.p,
            self.t.h,
            cx0,
            cz0,
            cx1 + 1,
            cz1 + 1,
        );
        let mut out = Vec::new();
        let mut ids = Vec::new();
        let mut oil = Vec::new();
        for cz in cz0..=cz1 {
            for cx in cx0..=cx1 {
                ids.clear();
                oil.clear();
                grid.chunk_ids(cx, cz, |b| {
                    if !ids.contains(&b) {
                        ids.push(b)
                    }
                });
                for (ox, oz) in [(cx, cz), (cx + 1, cz), (cx, cz + 1), (cx + 1, cz + 1)] {
                    grid.chunk_ids(ox, oz, |b| {
                        if !oil.contains(&b) {
                            oil.push(b)
                        }
                    });
                }
                // mineshaft corridors draw before the lake rolls: those chunks skip the pre-check
                if self.touches_shaft(cx, cz) || self.lake_roll_possible(cx, cz, &ids, &oil) {
                    out.extend(self.lakes_at(cx, cz));
                }
            }
        }
        out
    }

    /// A loaded mineshaft piece intersects the populate box of chunk `(cx, cz)`.
    pub(crate) fn touches_shaft(&self, cx: i32, cz: i32) -> bool {
        let (x, z) = (cx * 16 + 8, cz * 16 + 8);
        self.shafts.iter().any(|sh| {
            sh.pieces
                .iter()
                .any(|p| p.bb[0] <= x + 15 && p.bb[3] >= x && p.bb[2] <= z + 15 && p.bb[5] >= z)
        })
    }

    /// The geode of chunk `(cx, cz)` (runs only the `PopulateChunkEvent.Pre` consumers).
    pub fn geode_at(&mut self, cx: i32, cz: i32) -> Option<Geode> {
        self.flags = 0;
        self.last_geode = None;
        let mut r = self.rand(cx, cz);
        let mut w = World {
            m: self,
            over: FxMap::default(),
            log: Vec::new(),
        };
        pre_event(&mut w, &mut r, cx, cz);
        drop(w);
        self.last_geode
    }

    /// All geodes in a chunk rectangle. Chunks whose possible biomes (dithered chunk biome array) can't pass a
    /// geode gate, or whose `nextDouble` rolls can't reach 0.3 for any gate combination, skip chunk generation.
    pub fn geodes_in_rect(&mut self, cx0: i32, cz0: i32, cx1: i32, cz1: i32) -> Vec<Geode> {
        let grid =
            crate::chunkgen::ChunkGen::sample_window_grid(self.t.p, self.t.h, cx0, cz0, cx1, cz1);
        let mut out = Vec::new();
        for cz in cz0..=cz1 {
            for cx in cx0..=cx1 {
                // gate combinations (rc_geode, rc_quarry, nh_geode) over the chunk's possible biomes
                let mut combos: Vec<(bool, bool, bool)> = Vec::new();
                grid.chunk_ids(cx, cz, |b| {
                    let g = &self.gates;
                    let c = (
                        PreGates::get(&g.rc_geode, b),
                        PreGates::get(&g.rc_quarry, b),
                        PreGates::get(&g.nh_geode, b),
                    );
                    if (c.0 || c.2) && !combos.contains(&c) {
                        combos.push(c);
                    }
                });
                let possible = combos.iter().any(|&(rg, rq, ng)| {
                    let mut r = self.rand(cx, cz);
                    if rg && r.next_double() <= 0.3 {
                        return true; // floor test needs terrain
                    }
                    if rq {
                        r.next_double();
                    }
                    ng && r.next_double() <= 0.3
                });
                if possible {
                    if let Some(g) = self.geode_at(cx, cz) {
                        out.push(g);
                    }
                }
            }
        }
        out
    }

    /// Changes chunk `(cx, cz)`'s populate prefix made (cached; `None` if uncertain).
    pub fn changes(&mut self, cx: i32, cz: i32) -> Option<Changes> {
        if let Some(v) = self.done.get(&(cx, cz)) {
            return v.clone();
        }
        if self.done.len() > 8192 {
            self.done.clear();
        }
        // uncertain chunks still contribute what is known (e.g. the part before a dungeon)
        let v = match self.populate(cx, cz) {
            Outcome::Certain(log) | Outcome::Uncertain(_, log) if !log.is_empty() => Some(
                std::rc::Rc::new(log.into_iter().map(|(x, y, z, b)| ((x, y, z), b)).collect()),
            ),
            _ => None,
        };
        self.done.insert((cx, cz), v.clone());
        v
    }

    /// The block at `(x, y, z)` after the populate prefixes of the chunks `include` accepts, if one changed it
    /// (populate of chunk c writes into x ∈ [16c, 16c + 31]).
    pub fn changed(
        &mut self,
        x: i32,
        y: i32,
        z: i32,
        include: impl Fn(i32, i32) -> bool,
    ) -> Option<B> {
        let mut out = None;
        for cz in (z >> 4) - 1..=z >> 4 {
            for cx in (x >> 4) - 1..=x >> 4 {
                if !include(cx, cz) {
                    continue;
                }
                if let Some(m) = self.changes(cx, cz) {
                    if let Some(&b) = m.get(&(x, y, z)) {
                        out = Some(b);
                    }
                }
            }
        }
        out
    }

    /// Population changes inside chunk `(cx, cz)` after the populate prefixes of the four chunks that write into
    /// it, as [`B::code`] per block, index `(y · 16 + lz) · 16 + lx`, y < `h`.
    pub fn chunk_codes(&mut self, cx: i32, cz: i32, h: i32) -> Vec<u8> {
        let mut out = vec![0u8; (16 * 16 * h) as usize];
        for (pcx, pcz) in [(cx - 1, cz - 1), (cx, cz - 1), (cx - 1, cz), (cx, cz)] {
            let Some(m) = self.changes(pcx, pcz) else {
                continue;
            };
            for (&(x, y, z), &b) in m.iter() {
                if x >> 4 == cx && z >> 4 == cz && (0..h).contains(&y) {
                    out[((y * 16 + (z & 15)) * 16 + (x & 15)) as usize] = b.code();
                }
            }
        }
        out
    }

    /// [`F_SUPPORT`] / [`F_OCEAN`] from RWG2's 9×9 `borderNoise` samples.
    pub fn decorator_flags(&self, cx: i32, cz: i32) -> u32 {
        let (p, h) = (self.t.p, self.t.h);
        let mut cnt: FxMap<u16, i32> = FxMap::default();
        for bx in -4..=4 {
            for bz in -4..=4 {
                let id = p
                    .biome_data(cx * 16 + 24 + bx * 16, cz * 16 + 24 + bz * 16, h)
                    .data;
                *cnt.entry(id).or_default() += 1;
            }
        }
        let mut f = 0;
        for (&id, &n) in &cnt {
            if n < 25 {
                continue;
            }
            let class = &p.table.info(id as _).class;
            if class.contains("Support") {
                f |= F_SUPPORT;
            } else if class.contains("RealisticBiomeOcean") || class.contains("BOPOcean") {
                f |= F_OCEAN;
            }
        }
        f
    }
}

/// Table size assumed for the mineshaft `structureMap` (see [`shaft_scan_order`]).
pub const SHAFT_MAP_CAPACITY: u64 = 256;

/// Order in which overlapping mineshaft starts build their pieces in one populate. In GTNH every
/// `MapGenStructure.structureMap` is a fastutil `Long2ObjectOpenHashMap` (rwg2dump 2.17), whose iterator walks the
/// table from the last slot down: slot = `HashCommon.mix(chunkXZ2Int(x, z)) & (capacity − 1)`. The capacity grows
/// with the number of starts the world has loaded (all within 8 chunks of any generated chunk), so the order is
/// world-history dependent; [`SHAFT_MAP_CAPACITY`] = 256 (97–192 loaded starts) reproduces all dump 2.16 pairs
/// (capacity 16, 128 and 256 do, 32 / 64 / ≥ 512 don't). Same slot: ascending x, z (linear-probing order unknown).
pub fn shaft_scan_order(s: (i32, i32)) -> i64 {
    let key = (s.0 as u32 as u64) | ((s.1 as u32 as u64) << 32);
    let mut h = key.wrapping_mul(0x9E37_79B9_7F4A_7C15);
    h ^= h >> 32;
    h ^= h >> 16;
    let slot = (h & (SHAFT_MAP_CAPACITY - 1)) as i64;
    // descending slot first, then (x, z)
    -(slot << 48) + (((s.0 as i64) + (1 << 23)) << 24) + ((s.1 as i64) + (1 << 23))
}

/// One mineshaft start's pieces (generation order) and their per-start state across populates.
struct Shaft {
    start: (i32, i32),
    pieces: Vec<crate::mineshaft::Piece>,
    /// `addComponentParts` returned false (liquid at the box faces): dropped from the start for good
    removed: Vec<bool>,
    /// Corridor `spawnerPlaced`
    spawner: Vec<bool>,
}

/// `StructureStart.generateStructure` of mineshaft `i` for the chunk box `sbb` (`[x0, z0, x1, z1]`, y 1..512): the
/// shared-Random draws of every intersecting piece's `addComponentParts` (only corridors draw). False when a chest
/// roll hits (loot draws not modelled).
fn mineshaft_parts(w: &mut World, r: &mut JavaRandom, i: usize, sbb: [i32; 4]) -> bool {
    use crate::mineshaft::Kind;
    let inside = |x: i32, y: i32, z: i32| {
        x >= sbb[0] && x <= sbb[2] && z >= sbb[1] && z <= sbb[3] && (1..=512).contains(&y)
    };
    let n = w.m.shafts[i].pieces.len();
    // per-chunk state only (order-independent): a piece dropped / a spawner placed by an earlier populate is not
    // remembered, so the viewer's cells and the search give one answer whatever order they run in
    if !w.m.stateful_shafts {
        w.m.shafts[i].removed.iter_mut().for_each(|r| *r = false);
        w.m.shafts[i].spawner.iter_mut().for_each(|r| *r = false);
    }
    for k in 0..n {
        let p = w.m.shafts[i].pieces[k].clone();
        if w.m.shafts[i].removed[k] {
            continue;
        }
        let b = p.bb;
        if !(b[0] <= sbb[2] && b[3] >= sbb[0] && b[2] <= sbb[3] && b[5] >= sbb[1]) {
            continue;
        }
        // isLiquidInStructureBoundingBox (every piece type)
        let (x0, y0, z0) = (
            (b[0] - 1).max(sbb[0]),
            (b[1] - 1).max(1),
            (b[2] - 1).max(sbb[1]),
        );
        let (x1, y1, z1) = (
            (b[3] + 1).min(sbb[2]),
            (b[4] + 1).min(512),
            (b[5] + 1).min(sbb[3]),
        );
        let mut liquid = false;
        'l: for xx in x0..=x1 {
            for zz in z0..=z1 {
                for yy in [y0, y1] {
                    if matches!(w.get(xx, yy, zz), B::Water | B::Lava) {
                        liquid = true;
                        break 'l;
                    }
                }
            }
        }
        if !liquid {
            'l2: for yy in y0..=y1 {
                for xx in x0..=x1 {
                    for zz in [z0, z1] {
                        if matches!(w.get(xx, yy, zz), B::Water | B::Lava) {
                            liquid = true;
                            break 'l2;
                        }
                    }
                }
                for zz in z0..=z1 {
                    for xx in [x0, x1] {
                        if matches!(w.get(xx, yy, zz), B::Water | B::Lava) {
                            liquid = true;
                            break 'l2;
                        }
                    }
                }
            }
        }
        if liquid {
            w.m.shafts[i].removed[k] = true;
            continue;
        }
        if p.kind != Kind::Corridor {
            // rooms / crossings / stairs draw nothing; their air (approximate boxes) still matters for later
            // dungeon and lake checks
            {
                let a = crate::mineshaft::piece_air(&p);
                for x in a[0].max(sbb[0])..=a[3].min(sbb[2]) {
                    for z in a[2].max(sbb[1])..=a[5].min(sbb[3]) {
                        for y in a[1].max(1)..=a[4].min(255) {
                            w.set_quiet(x, y, z, B::Air);
                        }
                    }
                }
            }
            continue;
        }
        let dir = p.dir;
        let gx = |lx: i32, lz: i32| match dir {
            1 => b[3] - lz,
            3 => b[0] + lz,
            _ => b[0] + lx,
        };
        let gz = |lx: i32, lz: i32| match dir {
            0 => b[2] + lz,
            2 => b[5] - lz,
            _ => b[2] + lx,
        };
        let sections = if dir == 0 || dir == 2 {
            (b[5] - b[2] + 1) / 5
        } else {
            (b[3] - b[0] + 1) / 5
        };
        let len = sections * 5 - 1;
        let y0 = b[1];
        // world writes of this corridor (inside the chunk box only), seen by the later pieces and populate steps
        let put = |w: &mut World, lx: i32, ly: i32, lz: i32, v: B| {
            let (wx, wy, wz) = (gx(lx, lz), y0 + ly, gz(lx, lz));
            if inside(wx, wy, wz) {
                w.set_quiet(wx, wy, wz, v);
            }
        };
        for ly in 0..=1 {
            for lx in 0..=2 {
                for lz in 0..=len {
                    put(w, lx, ly, lz, B::Air);
                }
            }
        }
        // 0.8 air fill of the top layer, y outer / x / z inner
        for lx in 0..=2 {
            for lz in 0..=len {
                if r.next_float() <= 0.8 {
                    put(w, lx, 2, lz, B::Air);
                }
            }
        }
        // cobwebs (0.6) over y 0..1 (all edge positions)
        if p.spiders {
            for ly in 0..=1 {
                for lx in 0..=2 {
                    for lz in 0..=len {
                        if r.next_float() <= 0.6 {
                            put(w, lx, ly, lz, B::Web);
                        }
                    }
                }
            }
        }
        for s in 0..sections {
            let z = 2 + s * 5;
            for ly in 0..=1 {
                put(w, 0, ly, z, B::Solid);
                put(w, 2, ly, z, B::Solid);
            }
            if r.next_int_bounded(4) == 0 {
                put(w, 0, 2, z, B::Solid);
                put(w, 2, 2, z, B::Solid);
            } else {
                for lx in 0..=2 {
                    put(w, lx, 2, z, B::Solid);
                }
            }
            // 8 cobweb and 2 torch rolls on the top layer
            for (lx, lz, ch) in [
                (0, z - 1, 0.1f32),
                (2, z - 1, 0.1),
                (0, z + 1, 0.1),
                (2, z + 1, 0.1),
                (0, z - 2, 0.05),
                (2, z - 2, 0.05),
                (0, z + 2, 0.05),
                (2, z + 2, 0.05),
                (1, z - 1, 0.05),
                (1, z + 1, 0.05),
            ] {
                if r.next_float() < ch {
                    put(w, lx, 2, lz, B::Web);
                }
            }
            // two minecart-chest rolls: (2, 0, z − 1) and (0, 0, z + 1)
            for (lx, lz) in [(2, z - 1), (0, z + 1)] {
                if r.next_int_bounded(100) != 0 {
                    continue;
                }
                // arguments first: ChestGenHooks.getItems + getCount (Forge-patched corridor)
                let (wx, wy, wz) = (gx(lx, lz), y0, gz(lx, lz));
                let placed = inside(wx, wy, wz) && w.get(wx, wy, wz) == B::Air;
                let table = crate::loot::LootTable::mineshaft();
                let (book, cnt) = match table {
                    Some(t) => t.prepare(r),
                    // getItems draws only for the enchanted book; getCount is one draw (dump 2.16: 23/23 hits
                    // outside the chunk box cost exactly book + 1)
                    None => (crate::loot::book_roll(r), {
                        r.next_int_bounded(5);
                        0
                    }),
                };
                if placed {
                    // rail meta, then the cart's contents (27 slots)
                    let k = w.m.chest_hits.iter().filter(|h| h.3).count();
                    if w.m.chest_probe.as_ref().and_then(|p| p.get(k)) != Some(&u32::MAX) {
                        r.next_boolean();
                    }
                    w.set_quiet(wx, wy, wz, B::Web);
                    if let Some(probe) = &w.m.chest_probe {
                        let n = probe.get(k).copied().unwrap_or(0) % u32::MAX;
                        for _ in 0..n {
                            r.next_int();
                        }
                    } else {
                        let Some(t) = table else {
                            return false;
                        };
                        let c = t.fill(r, cnt, book);
                        w.m.mine_chests.push((wx, wy, wz, c));
                    }
                }
                w.m.chest_hits.push((wx, wy, wz, placed));
            }
            if p.spiders && !w.m.shafts[i].spawner[k] {
                let lz = z - 1 + r.next_int_bounded(3);
                if inside(gx(1, lz), y0, gz(1, lz)) {
                    w.m.shafts[i].spawner[k] = true;
                    put(w, 1, 0, lz, B::Spawner);
                }
            }
        }
        // planks under the floor where it is air
        for lx in 0..=2 {
            for lz in 0..=len {
                let (wx, wy, wz) = (gx(lx, lz), y0 - 1, gz(lx, lz));
                if inside(wx, wy, wz) && w.get(wx, wy, wz) == B::Air {
                    w.set_quiet(wx, wy, wz, B::Solid);
                }
            }
        }
        // rails: a 0.7 roll where the floor under the middle lane is a non-air opaque block inside the chunk box
        if p.rails {
            for lz in 0..=len {
                let (wx, wy, wz) = (gx(1, lz), y0 - 1, gz(1, lz));
                if inside(wx, wy, wz) && w.get(wx, wy, wz).solid() && r.next_float() < 0.7 {
                    put(w, 1, 0, lz, B::Web);
                }
            }
        }
    }
    true
}

/// TiC `SurfaceOreGen.findGround` from y 128 (64 + TiC sea level 64) down to 34 on the pre-population surface
/// (painted blocks, caves, this populate's writes): the first `Blocks.dirt` (any meta) / grass decides.
fn zinc_ground(w: &mut World, x: i32, z: i32) -> bool {
    use crate::surface::blk;
    let Some(sc) = w.m.surface_chunk(x >> 4, z >> 4) else {
        return false;
    };
    let idx = ((z & 15) * 16 + (x & 15)) as usize;
    let col = sc.cols[idx];
    let block = |w: &mut World, y: i32| -> u8 {
        match w.over.get(&(x, y, z)) {
            Some(B::Dirt) => blk::DIRT,
            Some(B::Air) => blk::AIR,
            Some(B::Water) => blk::WATER,
            Some(B::Lava) => blk::LAVA,
            Some(B::Gravel) => blk::GRAVEL,
            Some(_) => blk::STONE,
            None => col[y as usize],
        }
    };
    let opaque = |b: u8| !matches!(b, blk::AIR | blk::WATER | blk::LAVA | blk::SNOW_LAYER);
    for y in (34..=128).rev() {
        let b = block(w, y);
        if matches!(b, blk::DIRT | blk::COARSE_DIRT | blk::PODZOL | blk::GRASS) {
            return !opaque(block(w, y + 1));
        }
    }
    false
}

/// `PopulateChunkEvent.Pre` consumers in dispatch order (Railcraft geode + quarry, NHCore geode + quarry,
/// BuildCraft oil).
fn pre_event(w: &mut World, r: &mut JavaRandom, cx: i32, cz: i32) {
    let (x, z) = (cx * 16 + 8, cz * 16 + 8);
    let g = w.m.gen_at(x, z);
    let gt = &w.m.gates;
    let (rc_geode, rc_quarry, nh_geode, nh_quarry) = (
        PreGates::get(&gt.rc_geode, g),
        PreGates::get(&gt.rc_quarry, g),
        PreGates::get(&gt.nh_geode, g),
        PreGates::get(&gt.nh_quarry, g),
    );
    for (on, f) in [
        (rc_geode, F_RC_GEODE),
        (rc_quarry, F_RC_QUARRY),
        (nh_geode, F_NH_GEODE),
        (nh_quarry, F_NH_QUARRY),
    ] {
        if on {
            w.m.flags |= f;
        }
    }
    // RWG2's `RailcraftWorldgenFilter` DENYs the RAILCRAFT_GEODE event where `isOceanBiomeAt(x + 8, z + 8)`
    // (provider biome is a `RealisticBiomeOcean`, incl. BOP kelp / coral): no `canGen` draw there
    let rc_geode = rc_geode && {
        let (p, h) = (w.m.t.p, w.m.t.h);
        !crate::deco::is_ocean_class(&p.table.info(p.biome_data(x, z, h).data as _).class)
    };
    if rc_geode && r.next_double() <= 0.3 {
        // scanOceanFloor at the chunk centre: water depth and floor y
        let top = w.m.t.top(x, z).unwrap_or(64);
        let depth = (62 - top).max(0);
        if depth >= 16 && top >= 24 {
            let y = 12 + r.next_int_bounded(top - 12);
            geode(w, r, x, y, z, true);
        }
    }
    if rc_quarry {
        r.next_double();
    }
    if nh_geode && r.next_double() <= 0.3 {
        let y = 16 + r.next_int_bounded(16);
        geode(w, r, x, y, z, false);
    }
    if nh_quarry {
        r.next_double();
    }
    // BuildCraft OilPopulate (oilWellGenerationRate = 0: every chance test fails)
    let ox = cx * 16 + 8 + r.next_int_bounded(16);
    let oz = cz * 16 + 8 + r.next_int_bounded(16);
    let og = w.m.gen_at(ox, oz);
    r.next_double();
    r.next_double();
    if PreGates::get(&w.m.gates.oil, og) {
        w.m.flags |= F_OIL;
        r.next_double();
    }
}

/// Railcraft `WorldGenGeode` (abyssal stone shell r ≤ 8, air core r ≤ 4); one `nextDouble` per ore-shell block
/// (16 < r² ≤ 25) that ends up abyssal stone. `B::Solid` stands for abyssal stone.
fn geode(w: &mut World, r: &mut JavaRandom, x: i32, y: i32, z: i32, ocean: bool) {
    w.m.flags |= F_GEODE;
    let mut g = Geode {
        x,
        y,
        z,
        ocean,
        diamond: 0,
        emerald: 0,
        lapis: 0,
    };
    for i in -8..8 {
        for j in -8..8 {
            for k in -8..8 {
                let d = i * i + j * j + k * k;
                let (bx, by, bz) = (x + i, y + j, z + k);
                let shell_ore = d > 16 && d <= 25;
                if d > 64 && !shell_ore {
                    continue;
                }
                let b = w.get(bx, by, bz);
                // surface layers of an ocean floor are sand/gravel (replaceable); elsewhere `Solid` is not
                let surface = ocean && b == B::Solid && by > 4;
                let repl = matches!(b, B::Stone | B::Dirt | B::Gravel | B::Water) || surface;
                let geode_stone = b == B::Abyssal;
                if d <= 16 {
                    if repl {
                        w.set(bx, by, bz, B::Air);
                    }
                } else if repl {
                    w.set(bx, by, bz, B::Abyssal);
                }
                if shell_ore && (repl || geode_stone) {
                    let c = r.next_double();
                    if c <= 0.002 {
                        g.diamond += 1;
                    } else if c <= 0.004 {
                        g.emerald += 1;
                    } else if c <= 0.01 {
                        g.lapis += 1;
                    }
                }
            }
        }
    }
    w.m.last_geode = Some(g);
}

/// After the gravel blobs: RWG2's ore tail, mushrooms, the SAND decorate event, then the border-biome loop up to
/// the first decorating support biome, whose `BiomeDecorator.generateOres` (20 dirt + 10 gravel blobs, y 0..255)
/// is modelled when nothing world-dependent drew before it.
fn decorate_phase(w: &mut World, r: &mut JavaRandom, cx: i32, cz: i32, river: f32) {
    let (x, z) = (cx * 16, cz * 16);
    // coal..lapis: GregTech DENYs them (no draws). Emeralds: 4 × 3 draws, ore where stone.
    for _ in 0..4 {
        let ex = x + r.next_int_bounded(16);
        let ey = r.next_int_bounded(28) + 4;
        let ez = z + r.next_int_bounded(16);
        if w.get(ex, ey, ez) == B::Stone {
            w.set(ex, ey, ez, B::Emerald);
        }
    }
    // mushrooms: WorldGenFlowers, 64 × 6 fixed draws
    if r.next_int_bounded(5) == 0 {
        r.next_int_bounded(16);
        r.next_int_bounded(64);
        r.next_int_bounded(16);
        r.next_boolean();
        for _ in 0..64 {
            r.next_int_bounded(8);
            r.next_int_bounded(8);
            r.next_int_bounded(4);
            r.next_int_bounded(4);
            r.next_int_bounded(8);
            r.next_int_bounded(8);
        }
    }
    let (p, h) = (w.m.t.p, w.m.t.h);
    // Decorate SAND: TiC surface ores all off in GTNH (no draws); NHCore zinc gravel 1/150 (×2 in extreme hills)
    let g = p.biome_gen_id(x, z, h);
    let iters = if g == 3 || g == 20 { 2 } else { 1 };
    for _ in 0..iters {
        if r.next_int_bounded(150) == 0 {
            // NHCore `ZincGravelWorldgen` → TiC `SurfaceOreGen(zinc gravel, 12, alterSize)`: x, z in this chunk,
            // `findGround` scans y 128 → 34 for the first dirt / grass (found only with a non-opaque block above);
            // a hit draws 1 float, nextInt(7), 2 × nextInt(3), (n + 1) doubles. Decorations a neighbour already
            // placed over the column (logs, leaves) are not modelled.
            w.m.flags |= F_DECO_UNKNOWN;
            let zx = x + r.next_int_bounded(16);
            let zz = z + r.next_int_bounded(16);
            if zinc_ground(w, zx, zz) {
                r.next_float();
                let n = 4 + r.next_int_bounded(7);
                r.next_int_bounded(3);
                r.next_int_bounded(3);
                for _ in 0..=n {
                    r.next_double();
                }
            }
        }
    }
    // border-biome loop (ascending RWG id; oceans deferred to after ICE)
    w.m.last_deco_start = Some((r.clone(), w.log.len()));
    let mut cnt: std::collections::BTreeMap<u16, i32> = std::collections::BTreeMap::new();
    for bx in -4..=4 {
        for bz in -4..=4 {
            let id = p.biome_data(x + 24 + bx * 16, z + 24 + bz * 16, h).data;
            *cnt.entry(id).or_default() += 1;
        }
    }
    let strength = |n: i32| {
        let mut s = 0f32;
        for _ in 0..n {
            s += 0.012_345_69;
        }
        s.min(1.0)
    };
    let mut live = true; // the shared Random is still known
    for (&id, &n) in &cnt {
        let info = p.table.info(id as _);
        let s = strength(n);
        if info.class.contains("BOPOcean") {
            if s > 0.3 {
                w.m.flags |= F_DECO_UNKNOWN;
            }
            continue;
        }
        if info.class.contains("RealisticBiomeOcean") {
            continue;
        }
        if !info.class.contains("Support") {
            live = false;
            continue;
        }
        if s <= 0.3 {
            if river > 0.7 {
                live = false; // EditRiverOasis draws
            }
            continue;
        }
        let vanilla = info.gen_id == 129 || info.gen_id == 140;
        if !live || vanilla {
            w.m.flags |= F_DECO_UNKNOWN;
            live = false;
            continue;
        }
        // BOP decorator: generateOres on RandomForcedPositiveOwned
        let own = JavaRandom::new(r.state());
        let mut b = BopRandom { parent: r, own };
        for (count, block) in [(20, B::Dirt), (10, B::Gravel)] {
            for _ in 0..count {
                let bx = x + b.int(16);
                let by = b.int(256);
                let bz = z + b.int(16);
                minable(w, &mut b, bx, by, bz, 32, block);
            }
        }
        w.m.flags |= F_DECO_MODELLED;
        live = false;
    }
}

/// The draws `WorldGenMinable` makes.
trait Rng {
    fn float(&mut self) -> f32;
    fn int(&mut self, n: i32) -> i32;
    fn double(&mut self) -> f64;
}
impl Rng for JavaRandom {
    fn float(&mut self) -> f32 {
        self.next_float()
    }
    fn int(&mut self, n: i32) -> i32 {
        self.next_int_bounded(n)
    }
    fn double(&mut self) -> f64 {
        self.next_double()
    }
}
/// BiomesOPlenty `RandomForcedPositiveOwned`: `nextInt` goes to the shared Random, everything else to its own
/// `Random` seeded (scrambled again) from the shared one's state when the decorator started.
struct BopRandom<'r> {
    parent: &'r mut JavaRandom,
    own: JavaRandom,
}
impl Rng for BopRandom<'_> {
    fn float(&mut self) -> f32 {
        self.own.next_float()
    }
    fn int(&mut self, n: i32) -> i32 {
        if n > 0 {
            self.parent.next_int_bounded(n)
        } else {
            0
        }
    }
    fn double(&mut self) -> f64 {
        self.own.next_double()
    }
}

/// `WorldGenMinable.generate` (target stone).
fn minable(w: &mut World, r: &mut impl Rng, x: i32, y: i32, z: i32, n: i32, block: B) {
    let f = r.float() * std::f32::consts::PI;
    let nf = n as f32;
    let d0 = ((x + 8) as f32 + msin(f) * nf / 8.0) as f64;
    let d1 = ((x + 8) as f32 - msin(f) * nf / 8.0) as f64;
    let d2 = ((z + 8) as f32 + mcos(f) * nf / 8.0) as f64;
    let d3 = ((z + 8) as f32 - mcos(f) * nf / 8.0) as f64;
    let d4 = (y + r.int(3) - 2) as f64;
    let d5 = (y + r.int(3) - 2) as f64;
    let nd = n as f64;
    for l in 0..=n {
        let ld = l as f64;
        let d6 = d0 + (d1 - d0) * ld / nd;
        let d7 = d4 + (d5 - d4) * ld / nd;
        let d8 = d2 + (d3 - d2) * ld / nd;
        let d9 = r.double() * nd / 16.0;
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
                    if d12 * d12 + d13 * d13 + d14 * d14 < 1.0 && w.get(k2, l2, i3) == B::Stone {
                        w.set(k2, l2, i3, block);
                    }
                }
            }
        }
    }
}

/// Summarises the lake written to `w.log[s..]` into `w.m.last_lakes`.
fn lake_record(w: &mut World, s: usize, cx: i32, cz: i32) {
    let cells: Vec<(i32, i32, i32, B)> = w.log[s..].to_vec();
    let set: std::collections::HashSet<(i32, i32, i32)> =
        cells.iter().map(|&(x, y, z, _)| (x, y, z)).collect();
    let (mut n, mut sx, mut sz, mut y0, mut y1, mut lava) = (0u32, 0i64, 0i64, 255, 0, false);
    for &(x, y, z, b) in &cells {
        if b.liquid() {
            n += 1;
            sx += x as i64;
            sz += z as i64;
            y0 = y0.min(y);
            y1 = y1.max(y);
            lava |= b == B::Lava;
        }
    }
    if n == 0 {
        return;
    }
    let mut open = false;
    'o: for &(x, y, z, b) in &cells {
        if !(b.liquid() || b == B::Air) {
            continue;
        }
        for (dx, dy, dz) in [
            (1, 0, 0),
            (-1, 0, 0),
            (0, 1, 0),
            (0, -1, 0),
            (0, 0, 1),
            (0, 0, -1),
        ] {
            let q = (x + dx, y + dy, z + dz);
            if !set.contains(&q) && w.get(q.0, q.1, q.2) == B::Air {
                open = true;
                break 'o;
            }
        }
    }
    let (x, z) = (
        (sx as f64 / n as f64).round() as i32,
        (sz as f64 / n as f64).round() as i32,
    );
    w.m.last_lakes.push(Lake {
        x,
        z,
        y0,
        y1,
        lava,
        cells: n,
        open,
        cx,
        cz,
    });
}

/// `WorldGenLakes.generate`.
fn lake(w: &mut World, r: &mut JavaRandom, x: i32, mut y: i32, z: i32, liquid: B) -> bool {
    let (x, z) = (x - 8, z - 8);
    while y > 5 && w.get(x, y, z) == B::Air {
        y -= 1;
    }
    if y <= 4 {
        return false;
    }
    y -= 4;
    let mut m = [false; 2048];
    let n = r.next_int_bounded(4) + 4;
    for _ in 0..n {
        let d0 = r.next_double() * 6.0 + 3.0;
        let d1 = r.next_double() * 4.0 + 2.0;
        let d2 = r.next_double() * 6.0 + 3.0;
        let d3 = r.next_double() * (16.0 - d0 - 2.0) + 1.0 + d0 / 2.0;
        let d4 = r.next_double() * (8.0 - d1 - 4.0) + 2.0 + d1 / 2.0;
        let d5 = r.next_double() * (16.0 - d2 - 2.0) + 1.0 + d2 / 2.0;
        for i in 1..15 {
            for k in 1..15 {
                for j in 1..7 {
                    let a = (i as f64 - d3) / (d0 / 2.0);
                    let b = (j as f64 - d4) / (d1 / 2.0);
                    let c = (k as f64 - d5) / (d2 / 2.0);
                    if a * a + b * b + c * c < 1.0 {
                        m[((i * 16 + k) * 8 + j) as usize] = true;
                    }
                }
            }
        }
    }
    let at = |i: i32, k: i32, j: i32| m[((i * 16 + k) * 8 + j) as usize];
    let border = |i: i32, k: i32, j: i32| {
        !at(i, k, j)
            && (i < 15 && at(i + 1, k, j)
                || i > 0 && at(i - 1, k, j)
                || k < 15 && at(i, k + 1, j)
                || k > 0 && at(i, k - 1, j)
                || j < 7 && at(i, k, j + 1)
                || j > 0 && at(i, k, j - 1))
    };
    for i in 0..16 {
        for k in 0..16 {
            for j in 0..8 {
                if border(i, k, j) {
                    let b = w.get(x + i, y + j, z + k);
                    if j >= 4 && b.liquid() {
                        return false;
                    }
                    if j < 4 && !b.solid() && b != liquid {
                        return false;
                    }
                }
            }
        }
    }
    for i in 0..16 {
        for k in 0..16 {
            for j in 0..8 {
                if at(i, k, j) {
                    w.set(x + i, y + j, z + k, if j >= 4 { B::Air } else { liquid });
                }
            }
        }
    }
    if liquid == B::Lava {
        for i in 0..16 {
            for k in 0..16 {
                for j in 0..8 {
                    if border(i, k, j)
                        && (j < 4 || r.next_int_bounded(2) != 0)
                        && w.get(x + i, y + j, z + k).solid()
                    {
                        w.set(x + i, y + j, z + k, B::Stone);
                    }
                }
            }
        }
    }
    true
}

/// `WorldGenDungeons.generate` after a positive verdict: room (top-down; a block over a non-solid one becomes air,
/// floor blocks roll moss), two chests (3 tries each: an air floor spot with exactly one solid neighbour), loot,
/// spawner and its mob pick (`DungeonHooks.getRandomDungeonMob`: one weighted `nextInt`).
fn dungeon_build(
    w: &mut World,
    r: &mut JavaRandom,
    x: i32,
    y: i32,
    z: i32,
    l: i32,
    i1: i32,
) -> Dungeon {
    for xx in x - l - 1..=x + l + 1 {
        for yy in (y - 1..=y + 3).rev() {
            for zz in z - i1 - 1..=z + i1 + 1 {
                let inner = xx != x - l - 1
                    && xx != x + l + 1
                    && zz != z - i1 - 1
                    && zz != z + i1 + 1
                    && yy != y - 1;
                // interior, or a wall/floor block over a non-solid one → air
                if inner || (yy >= 0 && !w.get(xx, yy - 1, zz).solid()) {
                    w.set(xx, yy, zz, B::Air);
                } else if w.get(xx, yy, zz).solid() {
                    let moss = yy == y - 1 && r.next_int_bounded(4) != 0;
                    w.set(xx, yy, zz, if moss { B::Mossy } else { B::Cobble });
                }
            }
        }
    }
    let mut chests = Vec::new();
    for _ in 0..2 {
        for _ in 0..3 {
            let cx = x + r.next_int_bounded(l * 2 + 1) - l;
            let cz = z + r.next_int_bounded(i1 * 2 + 1) - i1;
            if w.get(cx, y, cz) == B::Air {
                let n = [(-1, 0), (1, 0), (0, -1), (0, 1)]
                    .iter()
                    .filter(|&&(a, b)| w.get(cx + a, y, cz + b).solid())
                    .count();
                if n == 1 {
                    w.set(cx, y, cz, B::Chest);
                    chests.push((cx, y, cz, crate::loot::LootTable::dungeon().roll(r)));
                    break;
                }
            }
        }
    }
    w.set(x, y, z, B::Spawner);
    // DungeonHooks mob list total weight (vanilla 100+100+200+100 + mods): not a power of two → one draw
    r.next_int_bounded(1000);
    Dungeon {
        x,
        y,
        z,
        l,
        i1,
        chests,
    }
}

/// `WorldGenDungeons.generate` up to the placement decision (`Some((half x, half z))` = a dungeon is placed).
fn dungeon_valid(w: &mut World, r: &mut JavaRandom, x: i32, y: i32, z: i32) -> Option<(i32, i32)> {
    let l = r.next_int_bounded(2) + 2;
    let i1 = r.next_int_bounded(2) + 2;
    let mut open = 0;
    for xx in x - l - 1..=x + l + 1 {
        for yy in y - 1..=y + 4 {
            for zz in z - i1 - 1..=z + i1 + 1 {
                let b = w.get(xx, yy, zz);
                if (yy == y - 1 || yy == y + 4) && !b.solid() {
                    return None;
                }
                if (xx == x - l - 1 || xx == x + l + 1 || zz == z - i1 - 1 || zz == z + i1 + 1)
                    && yy == y
                    && b == B::Air
                    && w.get(xx, yy + 1, zz) == B::Air
                {
                    open += 1;
                }
            }
        }
    }
    (1..=5).contains(&open).then_some((l, i1))
}
