//! Port of the biome-selection half of `rwg2.world.ChunkManagerRealistic` (`getBiomeDataAt`, `getBiomeGenAt`,
//! `getContinentValue`, meta-biome, ocean/coast/littoral/island/small/border selection, volcano + lava-cave gates).
//!
//! Biome candidate lists are *data* ([`BiomeTable`]), exported per pack version by `rwg2dump`.
//!
//! The two landmark gates need terrain height (`rNoise`), which is not ported yet; they go through [`Heights`].
//! Everything they need *except* `rNoise` is computed exactly here (river checks, the `river < 0.5 → 59` shortcut).

use std::cell::RefCell;

use crate::cell::CellNoise;
use crate::climate::{Climate, ClimateField};
use crate::config::RwgConfig;
use crate::continental::{
    self, ContinentalNoise, CONTINENTAL_SEED_XOR, NONE, VOLCANO_ISLAND_RADIUS,
};
use crate::jmath::FxMap;
use crate::noise::NoiseGenerator;
use crate::perlin::PerlinNoise;
use crate::poisson::PoissonPointNoise;

pub type BiomeId = u16;

const SHALLOW_OCEAN_WIDTH: f32 = 300.0;
const BIOME_WARP_SCALE_MULTIPLIER: f32 = 0.4;
const BIOME_WARP_STRENGTH_MULTIPLIER: f32 = 0.175;
const CLIMATE_BORDER_DISTANCE_DIFFERENCE: f64 = 288.0;
const LITTORAL_WIDTH: f32 = 432.0;
const SMALL_BIOME_RADIUS: f64 = 75.0;
const VOLCANO_RIVER_SAMPLE_SPACING: usize = 16;
pub const SMALL_POINTS_SEED_XOR: i64 = 0x510E527FADE682D1;

/// Per-biome data the selector needs. Indexed by RWG id (`RealisticBiomeBase.biomeID`).
#[derive(Clone, Debug, Default)]
pub struct BiomeInfo {
    pub rwg_id: BiomeId,
    /// `baseBiome.biomeID` (vanilla/EndlessIDs id, what `getBiomeGenAt` returns).
    pub gen_id: u16,
    /// `riverBiome.biomeID`: `replaceBlocksForBiome` writes it into the chunk biome array on inland river columns.
    pub river_gen_id: u16,
    /// `baseBiome.temperature` (coast ice vs dunes).
    pub temperature: f32,
    /// `riverBiome.temperature`
    pub river_temperature: f32,
    /// `baseBiome` class (e.g. `biomesoplenty.common.biome.overworld.BiomeGenFrostForest`)
    pub base_class: String,
    pub name: String,
    pub color: u32,
    pub class: String,
}

/// Per-climate lists, index 0..4 = snow, cold, hot, wet.
pub type PerClimate = [Vec<BiomeId>; 4];

/// The provider's candidate lists, in Java order (selection indexes into them; duplicates are meaningful).
#[derive(Clone, Debug, Default)]
pub struct BiomeTable {
    pub biomes: Vec<BiomeInfo>,
    pub core: PerClimate,
    pub border: PerClimate,
    pub cold_border: PerClimate,
    pub hot_border: PerClimate,
    pub very_cold_border: PerClimate,
    pub very_hot_border: PerClimate,
    pub littoral: PerClimate,
    pub small: PerClimate,
    pub island: PerClimate,
    pub small_island: PerClimate,
    pub large_island: PerClimate,
    pub wet_enabled: bool,
    pub coast_ice: BiomeId,
    pub coast_dunes: BiomeId,
    pub ocean_shallow: [BiomeId; 4],
    pub ocean_deep: [BiomeId; 4],
    pub ocean_shallow_kelp: BiomeId,
    pub ocean_shallow_coral: BiomeId,
    /// `Support.volcanoIsland` if it is a `RealisticBiomeIslandVolcano`.
    pub volcano_island: Option<BiomeId>,
    /// `Support.lavaCaveMarkerBiome` (gen/vanilla id).
    pub lava_cave_marker_gen_id: Option<u16>,
    /// Per RWG id: how `rNoise` is computed (from a v2 dump). Empty if unknown (v1 dump).
    pub heights: Vec<crate::terrain::HeightModel>,
}

impl BiomeTable {
    pub fn info(&self, id: BiomeId) -> &BiomeInfo {
        &self.biomes[id as usize]
    }
}

/// Terrain-height oracle (`biome.rNoise`) for the landmark gates and `getNoiseAt`. Return `None` if unknown
/// (then a gate is *assumed open*, which is what it evaluates to in the overwhelmingly common case).
pub trait Heights {
    /// `biome.rNoise(perlin, cell, x, z, ocean, border = 1f, river)`.
    fn r_noise(
        &self,
        _p: &BiomeProvider,
        _biome: BiomeId,
        _x: i32,
        _z: i32,
        _ocean: f32,
        _river: f32,
    ) -> Option<f32> {
        None
    }
}

/// Exact heights from the biome table's height models (needs a v2 table).
pub struct TableHeights;
impl Heights for TableHeights {
    fn r_noise(
        &self,
        p: &BiomeProvider,
        biome: BiomeId,
        x: i32,
        z: i32,
        ocean: f32,
        river: f32,
    ) -> Option<f32> {
        let m = p.table.heights.get(biome as usize)?;
        Some(m.r_noise(&p.perlin, &p.cell, x, z, ocean, 1.0, river))
    }
}

/// No terrain: gates assumed open where height would decide.
pub struct NoHeights;
impl Heights for NoHeights {}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum WorldType {
    Rwg2,
    Rwg2Continents,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct BiomeSample {
    /// `getBiomeDataAt` → RWG id.
    pub data: BiomeId,
    /// meta-biome: 0 ocean, 1..4 snow/cold/hot/wet.
    pub meta: u8,
}

pub struct BiomeProvider {
    seed: i64,
    pub table: BiomeTable,
    pub cfg: RwgConfig,
    continental: bool,
    pub(crate) perlin: PerlinNoise,
    pub(crate) cell: CellNoise,
    climate: ClimateField,
    small_points: PoissonPointNoise,
    pub(crate) continents: Option<ContinentalNoise>,
    biome_width: f32,
    volcano_eligibility: RefCell<FxMap<i64, bool>>,
    lava_eligibility: RefCell<FxMap<i64, bool>>,
    /// Count of gate decisions that fell back to "assume open" because a height was unknown.
    pub assumed_gates: std::cell::Cell<u64>,
    /// Unique per constructed provider (keys caches that live outside it, e.g. [`crate::chunkgen::ChunkGen`]).
    uid: u64,
}

static PROVIDER_UID: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(1);

impl BiomeProvider {
    pub fn new(seed: i64, world_type: WorldType, table: BiomeTable, cfg: RwgConfig) -> Self {
        let continental = world_type == WorldType::Rwg2Continents;
        let climate_width = 1400.0f32 * cfg.biome_size_multiplier;
        let mut climate = ClimateField::new(seed, climate_width, table.wet_enabled);
        climate.biome_offset_x = cfg.biome_offset_x;
        climate.biome_offset_z = cfg.biome_offset_z;
        let mut cell = CellNoise::new(seed, 0);
        cell.set_use_distance(true);
        Self {
            seed,
            continental,
            perlin: PerlinNoise::new(seed),
            cell,
            climate,
            small_points: PoissonPointNoise::new(seed ^ SMALL_POINTS_SEED_XOR, 1020.0, 4),
            continents: continental
                .then(|| ContinentalNoise::new(seed ^ CONTINENTAL_SEED_XOR, &cfg)),
            biome_width: 500.0f32 * cfg.biome_size_multiplier,
            table,
            cfg,
            volcano_eligibility: RefCell::default(),
            lava_eligibility: RefCell::default(),
            assumed_gates: std::cell::Cell::new(0),
            uid: PROVIDER_UID.fetch_add(1, std::sync::atomic::Ordering::Relaxed),
        }
    }

    pub fn seed(&self) -> i64 {
        self.seed
    }
    /// Unique id of this provider instance.
    pub fn uid(&self) -> u64 {
        self.uid
    }
    pub fn perlin(&self) -> &PerlinNoise {
        &self.perlin
    }
    pub fn cell(&self) -> &CellNoise {
        &self.cell
    }
    pub fn continents(&self) -> Option<&ContinentalNoise> {
        self.continents.as_ref()
    }
    /// Small-biome patch centres (`smallBiomePoints`, biome coordinates).
    pub fn small_points(&self) -> &PoissonPointNoise {
        &self.small_points
    }
    pub fn is_continental(&self) -> bool {
        self.continental
    }
    pub fn landmass_offset(&self) -> (i32, i32) {
        (self.cfg.landmass_offset_x, self.cfg.landmass_offset_z)
    }
    pub fn biome_offset(&self) -> (i32, i32) {
        (self.cfg.biome_offset_x, self.cfg.biome_offset_z)
    }

    #[inline]
    fn lx(&self, x: i32) -> i32 {
        x.wrapping_add(self.cfg.landmass_offset_x)
    }
    #[inline]
    fn lz(&self, z: i32) -> i32 {
        z.wrapping_add(self.cfg.landmass_offset_z)
    }
    #[inline]
    fn bx(&self, x: i32) -> i32 {
        x.wrapping_add(self.cfg.biome_offset_x)
    }
    #[inline]
    fn bz(&self, z: i32) -> i32 {
        z.wrapping_add(self.cfg.biome_offset_z)
    }

    #[inline]
    pub fn climate_at(&self, x: i32, z: i32) -> u8 {
        self.climate.at(x, z) as u8
    }

    /// `getContinentValue`.
    pub fn continent_value(&self, x: i32, z: i32) -> f32 {
        match &self.continents {
            Some(c) => c.value(self.lx(x), self.lz(z)),
            None => (self.legacy_ocean_value(x, z) - 1.0) * 100.0,
        }
    }

    /// `getBiomeDataAt` + meta-biome.
    pub fn biome_data(&self, x: i32, z: i32, h: &dyn Heights) -> BiomeSample {
        let Some(cont) = &self.continents else {
            let meta = self.climate_at(x, z);
            return BiomeSample {
                data: self.land_biome(x, z, meta),
                meta,
            };
        };
        let continent = cont.value(self.lx(x), self.lz(z));
        if continent < 0.0 {
            return BiomeSample {
                data: self.ocean_biome(continent, self.climate_at(x, z), x, z),
                meta: 0,
            };
        }
        if let Some(volcano) = self.table.volcano_island {
            if self.volcano_coordinates(x, z, h) != NONE {
                return BiomeSample {
                    data: volcano,
                    meta: self.climate_at(x, z),
                };
            }
        }
        let climate = self.climate_at(x, z);
        let mut meta = climate;
        let mut output: Option<BiomeId> = None;
        let mut island_selected = false;
        let mut littoral_selected = false;
        let tier = cont.island_size_tier(self.lx(x), self.lz(z));
        if tier >= 0 {
            let sc = cont.island_seed_coordinates(self.lx(x), self.lz(z));
            let (sx, sy) = ((sc >> 32) as i32, sc as i32);
            let island_climate = self.climate_at(sx, sy);
            output = self.select_island_biome(tier, island_climate, sx, sy);
            if output.is_some() {
                meta = island_climate;
                island_selected = true;
            }
        }
        let mut out = match output {
            Some(o) => o,
            None => {
                let mut o = self.land_biome(x, z, climate);
                let lit = &self.table.littoral[climate as usize - 1];
                if continent < LITTORAL_WIDTH && !lit.is_empty() {
                    o = self.select_biome(lit, self.bx(x), self.bz(z));
                    littoral_selected = true;
                }
                o
            }
        };
        if !island_selected && !littoral_selected && continent < 24.0 {
            out = if self.table.info(out).temperature < 0.15 {
                self.table.coast_ice
            } else {
                self.table.coast_dunes
            };
        }
        BiomeSample { data: out, meta }
    }

    /// `getBiomeGenAt` → base (vanilla) biome id, including the lava-cave marker override.
    pub fn biome_gen_id(&self, x: i32, z: i32, h: &dyn Heights) -> u16 {
        if let (Some(_), Some(marker)) = (&self.continents, self.table.lava_cave_marker_gen_id) {
            if self.volcano_vicinity_coordinates(x, z, h) == NONE {
                let cave = self.lava_cave_coordinates(x, z, h);
                if cave != NONE
                    && lava_cave_is_marker(
                        &self.perlin,
                        continental::unpack_x(cave),
                        continental::unpack_y(cave),
                    )
                {
                    return marker;
                }
            }
        }
        self.table.info(self.biome_data(x, z, h).data).gen_id
    }

    /// What a map shows at (x, z): the RWG biome, or `None` where `getBiomeGenAt` returns the lava-cave marker.
    /// One `biome_data` evaluation (vs two for `biome_data` + `biome_gen_id`).
    pub fn display(&self, x: i32, z: i32, h: &dyn Heights) -> Option<BiomeId> {
        if self.continents.is_some()
            && self.table.lava_cave_marker_gen_id.is_some()
            && self.volcano_vicinity_coordinates(x, z, h) == NONE
        {
            let cave = self.lava_cave_coordinates(x, z, h);
            if cave != NONE
                && lava_cave_is_marker(
                    &self.perlin,
                    continental::unpack_x(cave),
                    continental::unpack_y(cave),
                )
            {
                return None;
            }
        }
        Some(self.biome_data(x, z, h).data)
    }

    // ------------------------------------------------------------------ landmarks

    pub fn volcano_coordinates(&self, x: i32, z: i32, h: &dyn Heights) -> i64 {
        let Some(c) = &self.continents else {
            return NONE;
        };
        let v = c.volcano_coordinates(self.lx(x), self.lz(z));
        if v != NONE && self.can_generate_volcano_at(x, z, h) {
            v
        } else {
            NONE
        }
    }

    pub fn volcano_vicinity_coordinates(&self, x: i32, z: i32, h: &dyn Heights) -> i64 {
        let Some(c) = &self.continents else {
            return NONE;
        };
        let v = c.volcano_vicinity_coordinates(self.lx(x), self.lz(z));
        if v != NONE && self.can_generate_volcano_at(x, z, h) {
            v
        } else {
            NONE
        }
    }

    pub fn lava_cave_coordinates(&self, x: i32, z: i32, h: &dyn Heights) -> i64 {
        let Some(c) = &self.continents else {
            return NONE;
        };
        let v = c.lava_cave_coordinates(self.lx(x), self.lz(z));
        if v != NONE && self.can_generate_lava_cave_at(x, z, h) {
            v
        } else {
            NONE
        }
    }

    fn can_generate_volcano_at(&self, x: i32, z: i32, h: &dyn Heights) -> bool {
        if self.table.volcano_island.is_none() {
            return false;
        }
        let c = self.continents.as_ref().unwrap();
        let (lx, lz) = (self.lx(x), self.lz(z));
        let key = c.volcano_seed_key(lx, lz);
        if key == NONE {
            return false;
        }
        if let Some(&e) = self.volcano_eligibility.borrow().get(&key) {
            return e;
        }
        let center = c.volcano_center_coordinates(lx, lz);
        let cx = ((center >> 32) as i32).wrapping_sub(self.cfg.landmass_offset_x);
        let cz = (center as i32).wrapping_sub(self.cfg.landmass_offset_z);
        let eligible = !self.has_river_near_volcano(cx, cz)
            && match self.volcano_base_height(x, z, h) {
                Some(base) => volcano_can_generate_at_height(base),
                None => {
                    self.assumed_gates.set(self.assumed_gates.get() + 1);
                    true
                }
            };
        self.volcano_eligibility.borrow_mut().insert(key, eligible);
        eligible
    }

    fn can_generate_lava_cave_at(&self, x: i32, z: i32, h: &dyn Heights) -> bool {
        let c = self.continents.as_ref().unwrap();
        let (lx, lz) = (self.lx(x), self.lz(z));
        let key = c.lava_cave_seed_key(lx, lz);
        if key == NONE {
            return false;
        }
        if let Some(&e) = self.lava_eligibility.borrow().get(&key) {
            return e;
        }
        let center = c.lava_cave_center_coordinates(lx, lz);
        let cx = ((center >> 32) as i32).wrapping_sub(self.cfg.landmass_offset_x);
        let cz = (center as i32).wrapping_sub(self.cfg.landmass_offset_z);
        let eligible = match self.noise_at(cx, cz, h) {
            Some(n) => n > 63.0,
            None => {
                self.assumed_gates.set(self.assumed_gates.get() + 1);
                true
            }
        };
        self.lava_eligibility.borrow_mut().insert(key, eligible);
        eligible
    }

    fn has_river_near_volcano(&self, cx: i32, cz: i32) -> bool {
        let radius = VOLCANO_ISLAND_RADIUS.ceil() as i32;
        let r2 = radius * radius;
        for ox in (-radius..=radius).step_by(VOLCANO_RIVER_SAMPLE_SPACING) {
            for oz in (-radius..=radius).step_by(VOLCANO_RIVER_SAMPLE_SPACING) {
                if ox * ox + oz * oz <= r2 && self.raw_river_strength(cx + ox, cz + oz) < 0.0 {
                    return true;
                }
            }
        }
        false
    }

    // ------------------------------------------------------------------ rivers / height entry points

    #[inline]
    fn river_warp(&self, x: i32, y: i32) -> (f32, f32) {
        let px = x as f32 + self.perlin.noise1(y as f32 / 240.0) * 220.0;
        let py = y as f32 + self.perlin.noise1(x as f32 / 240.0) * 220.0;
        (px, py)
    }

    /// `ChunkManagerRealistic.getRiverTunnelStrength`: `-cell.border(warped / 1250, 9 / 1250, 1)`.
    pub fn river_tunnel_strength(&self, x: i32, y: i32) -> f32 {
        let (px, py) = self.river_warp(x, y);
        -self
            .cell
            .border(px as f64 / 1250.0, py as f64 / 1250.0, 9.0 / 1250.0, 1.0)
    }

    /// `ChunkManagerRealistic.getRiverJunctionStrength`: `-cell.junction(warped / 1250, 60 / 1250, 1)`.
    pub fn river_junction_strength(&self, x: i32, y: i32) -> f32 {
        let (px, py) = self.river_warp(x, y);
        -self
            .cell
            .junction(px as f64 / 1250.0, py as f64 / 1250.0, 60.0 / 1250.0, 1.0)
    }

    /// `getRawRiverStrength(int, int)`.
    pub fn raw_river_strength(&self, x: i32, y: i32) -> f32 {
        let (px, py) = self.river_warp(x, y);
        self.cell
            .border(px as f64 / 1250.0, py as f64 / 1250.0, 50.0 / 300.0, 1.0)
    }

    /// `getRiverStrength(int, int)` (with the volcano blend in continental mode).
    pub fn river_strength(&self, x: i32, y: i32) -> f32 {
        let (px, py) = self.river_warp(x, y);
        let strength = self
            .cell
            .border(px as f64 / 1250.0, py as f64 / 1250.0, 50.0 / 300.0, 1.0);
        let Some(c) = &self.continents else {
            return strength;
        };
        let coords = c.volcano_vicinity_coordinates(self.lx(x), self.lz(y));
        if coords == NONE {
            return strength;
        }
        let (lx, lz) = (continental::unpack_x(coords), continental::unpack_y(coords));
        let distance = ((lx * lx + lz * lz) as f64).sqrt() as f32;
        if distance as f64 <= continental::VOLCANO_RADIUS {
            return strength.max(0.0);
        }
        let blend = ((distance as f64 - continental::VOLCANO_RADIUS)
            / (VOLCANO_ISLAND_RADIUS - continental::VOLCANO_RADIUS)) as f32;
        if strength < 0.0 {
            strength * blend
        } else {
            strength
        }
    }

    /// `getTerrainOceanValue(int, int)`.
    pub fn terrain_ocean_value(&self, x: i32, y: i32) -> f32 {
        if self.continental {
            let v = 1.0 + self.continent_value(x, y) / 100.0;
            v.clamp(0.0, 2.0)
        } else {
            self.legacy_ocean_value(x, y)
        }
    }

    /// `getVolcanoUnderlyingBiome`: the biome a volcano sits on (used by chunk generation's biome samples).
    pub fn volcano_underlying_biome(&self, x: i32, z: i32) -> Option<BiomeId> {
        let c = self.continents.as_ref()?;
        let (lx, lz) = (self.lx(x), self.lz(z));
        let center = c.volcano_center_coordinates(lx, lz);
        if center == NONE {
            return None;
        }
        let cx = ((center >> 32) as i32).wrapping_sub(self.cfg.landmass_offset_x);
        let cz = (center as i32).wrapping_sub(self.cfg.landmass_offset_z);
        let climate = self.climate_at(cx, cz);
        let b = if c.is_island_volcano(lx, lz) {
            self.select_island_biome(1, climate, cx, cz)
        } else {
            Some(self.land_biome(x, z, self.climate_at(x, z)))
        };
        Some(b.unwrap_or_else(|| self.land_biome(cx, cz, climate)))
    }

    /// `getVolcanoBaseHeight`: terrain beneath the volcano centre (biome there, `rNoise` + river carving).
    pub fn volcano_base_height(&self, x: i32, z: i32, h: &dyn Heights) -> Option<f32> {
        let c = self.continents.as_ref()?;
        let (lx, lz) = (self.lx(x), self.lz(z));
        if c.volcano_seed_key(lx, lz) == NONE {
            return Some(63.0);
        }
        let center = c.volcano_center_coordinates(lx, lz);
        let cx = ((center >> 32) as i32).wrapping_sub(self.cfg.landmass_offset_x);
        let cz = (center as i32).wrapping_sub(self.cfg.landmass_offset_z);
        let climate = self.climate_at(cx, cz);
        let biome = if c.is_island_volcano(lx, lz) {
            self.select_island_biome(1, climate, cx, cz)
        } else {
            Some(self.land_biome(cx, cz, climate))
        }
        .unwrap_or_else(|| self.land_biome(cx, cz, climate));
        let continent = self.continent_value(cx, cz);
        let river = self.raw_river_strength(cx, cz);
        let ocean = {
            let v = 1.0 + continent / 100.0;
            v.clamp(0.0, 2.0)
        };
        let height = h.r_noise(self, biome, cx, cz, ocean, river + 1.0)?;
        Some(self.calculate_river(cx, cz, river, height))
    }

    /// `calculateRiver(x, y, st, biomeHeight)`.
    pub fn calculate_river(&self, x: i32, y: i32, st: f32, biome_height: f32) -> f32 {
        if st < 0.0 && biome_height > 59.0 {
            let (px, py) = self.river_warp(x, y);
            let carving =
                self.cell
                    .border(px as f64 / 1250.0, py as f64 / 1250.0, 50.0 / 1300.0, 1.0);
            let (xf, yf) = (x as f32, y as f32);
            (biome_height * (carving + 1.0))
                + ((59.0
                    + self.perlin.noise2(xf / 12.0, yf / 12.0) * 2.0
                    + self.perlin.noise2(xf / 8.0, yf / 8.0) * 1.5)
                    * (-carving))
        } else {
            biome_height
        }
    }

    /// `getNoiseAt`, or `None` when it would need an unported `rNoise`.
    pub fn noise_at(&self, x: i32, y: i32, h: &dyn Heights) -> Option<f32> {
        let river = self.river_strength(x, y) + 1.0;
        if river < 0.5 {
            return Some(59.0);
        }
        let ocean = self.terrain_ocean_value(x, y);
        let biome = self.biome_data(x, y, h).data;
        h.r_noise(self, biome, x, y, ocean, river)
    }

    fn legacy_ocean_value(&self, x: i32, y: i32) -> f32 {
        let p = &self.perlin;
        let (xf, yf) = (x as f32, y as f32);
        let s1 = p.noise2(xf / 1200.0, yf / 1200.0);
        let mut highest: f32 = if s1 == 0.0 { 1.0 } else { 0.0 };
        let diff = |a: f32, b: f32| a < 0.0 && b > 0.0 || a > 0.0 && b < 0.0;
        let s2 = p.noise2((xf - 100.0) / 1200.0, yf / 1200.0);
        if diff(s1, s2) {
            highest = highest.max(1.0 - (s1 / (s1 - s2).abs()).abs());
        } else {
            let s2 = p.noise2((xf + 100.0) / 1200.0, yf / 1200.0);
            if diff(s1, s2) {
                highest = highest.max(1.0 - (s1 / (s1 - s2).abs()).abs());
            }
        }
        let s2 = p.noise2(xf / 1200.0, (yf + 100.0) / 1200.0);
        if diff(s1, s2) {
            highest = highest.max(1.0 - (s1 / (s1 - s2).abs()).abs());
        } else {
            let s2 = p.noise2(xf / 1200.0, (yf - 100.0) / 1200.0);
            if diff(s1, s2) {
                highest = highest.max(1.0 - (s1 / (s1 - s2).abs()).abs());
            }
        }
        if s1 > 0.0 {
            2.0 - highest
        } else {
            highest
        }
    }

    // ------------------------------------------------------------------ selection

    fn ocean_biome(&self, continent: f32, climate: u8, x: i32, y: i32) -> BiomeId {
        let ci = climate as usize - 1;
        if continent < -SHALLOW_OCEAN_WIDTH {
            return self.table.ocean_deep[ci];
        }
        let (bx, bz) = (self.bx(x) as f32, self.bz(y) as f32);
        let patch = self.perlin.noise2(bx / 180.0, bz / 180.0) * 0.7
            + self.perlin.noise2(bx / 55.0, bz / 55.0) * 0.3;
        if (climate == 1 || climate == 2) && continent < -90.0 && patch > 0.0 {
            return self.table.ocean_shallow_kelp;
        }
        if (climate == 3 || climate == 4) && continent < -20.0 && continent > -150.0 && patch > 0.07
        {
            return self.table.ocean_shallow_coral;
        }
        self.table.ocean_shallow[ci]
    }

    pub(crate) fn select_island_biome(
        &self,
        tier: i32,
        climate: u8,
        sx: i32,
        sy: i32,
    ) -> Option<BiomeId> {
        let ci = climate as usize - 1;
        let sized = if tier == 0 {
            &self.table.small_island[ci]
        } else {
            &self.table.large_island[ci]
        };
        let general = &self.table.island[ci];
        if general.is_empty() && sized.is_empty() {
            None
        } else {
            Some(self.select_combined(general, sized, self.bx(sx), self.bz(sy)))
        }
    }

    /// `getLandBiomeAt(x, z, climate)`.
    pub(crate) fn land_biome(&self, x: i32, z: i32, climate: u8) -> BiomeId {
        let (p1, p2) = (self.bx(x), self.bz(z));
        let ci = climate as usize - 1;
        let small = &self.table.small[ci];
        if !small.is_empty() {
            let pt = self.small_points.sample(p1 as f64, p2 as f64);
            if pt.distance < SMALL_BIOME_RADIUS {
                return self.select_biome(small, pt.x as i32, pt.z as i32);
            }
        }
        let (wx, wy) = self.climate.warp(p1, p2);
        let cw = self.climate.climate_width as f64;
        let cp = self
            .climate
            .biomecell
            .sample_two_2d((wx + 4000.0) / cw, wy / cw, 1.0);
        let neighbor = self.climate.from_value((cp[3] * 0.5 + 0.5) as f32) as u8;
        let climate_border = neighbor != climate
            && (cp[2] - cp[0]) * (self.climate.climate_width as f64)
                < CLIMATE_BORDER_DISTANCE_DIFFERENCE;
        if climate_border {
            let cold = neighbor < climate;
            if (neighbor as i32 - climate as i32).abs() > 1 {
                let ext = if cold {
                    &self.table.very_cold_border[ci]
                } else {
                    &self.table.very_hot_border[ci]
                };
                if !ext.is_empty() {
                    return self.select_biome(ext, p1, p2);
                }
            }
            let directional = if cold {
                &self.table.cold_border[ci]
            } else {
                &self.table.hot_border[ci]
            };
            let shared = &self.table.border[ci];
            if !shared.is_empty() || !directional.is_empty() {
                return self.select_combined(shared, directional, p1, p2);
            }
        }
        let core = match climate {
            1 => &self.table.core[0],
            2 => &self.table.core[1],
            4 => &self.table.core[3],
            _ => &self.table.core[2],
        };
        self.select_biome(core, p1, p2)
    }

    #[inline]
    #[allow(clippy::manual_clamp)] // kept in Java's exact comparison form
    fn select_biome(&self, list: &[BiomeId], x: i32, y: i32) -> BiomeId {
        let mut v = self.biome_selector(x, y);
        v = if v < 0.0 {
            0.0
        } else if v >= 0.9999999 {
            0.9999999
        } else {
            v
        };
        list[(v * list.len() as f32) as i32 as usize]
    }

    #[inline]
    fn select_combined(
        &self,
        shared: &[BiomeId],
        directional: &[BiomeId],
        x: i32,
        y: i32,
    ) -> BiomeId {
        let len = shared.len() + directional.len();
        let v = self.biome_selector(x, y);
        let idx = (0f32.max(0.9999999f32.min(v)) * len as f32) as i32 as usize;
        if idx < shared.len() {
            shared[idx]
        } else {
            directional[idx - shared.len()]
        }
    }

    /// `sampleBiomeSelector`: note `warpedX / biomeWidth` is float/float, then widened.
    #[inline]
    fn biome_selector(&self, x: i32, y: i32) -> f32 {
        let scale = self.biome_width * BIOME_WARP_SCALE_MULTIPLIER;
        let strength = self.biome_width * BIOME_WARP_STRENGTH_MULTIPLIER;
        let (xf, yf) = (x as f32, y as f32);
        let w = &self.climate.climate_warp;
        let wx = xf + w.noise2((xf - 8191.0) / scale, (yf + 3137.0) / scale) * strength;
        let wy = yf + w.noise2((xf + 5171.0) / scale, (yf - 6971.0) / scale) * strength;
        self.climate.biomecell.noise(
            (wx / self.biome_width) as f64,
            (wy / self.biome_width) as f64,
            1.0,
        ) * 0.5
            + 0.5
    }

    /// Core biome only (no border/island/small replacement) — `getCoreBiomeAt`.
    pub fn core_biome(&self, x: i32, z: i32) -> BiomeId {
        let c = self.climate.at(x, z);
        let list = match c {
            Climate::Snow => &self.table.core[0],
            Climate::Cold => &self.table.core[1],
            Climate::Wet => &self.table.core[3],
            Climate::Hot => &self.table.core[2],
        };
        self.select_biome(list, self.bx(x), self.bz(z))
    }
}

/// `RealisticBiomeIslandVolcano.canGenerateAtHeight`.
pub fn volcano_can_generate_at_height(base: f32) -> bool {
    let rim = 40f32.max(60f32.min(44.0 + (base - 63.0) * 0.2));
    base + rim + 2.0 <= 250.0
}

#[inline]
fn smoothstep(v: f32) -> f32 {
    let v = 0f32.max(1f32.min(v));
    v * v * (3.0 - 2.0 * v)
}

pub(crate) fn lava_cave_warp(perlin: &PerlinNoise, x: f32, z: f32) -> (f32, f32) {
    let anchor = smoothstep(((x * x + z * z) as f64).sqrt() as f32 / 48.0);
    let wx = x
        + (perlin.noise2((x + 1300.0) / 175.0, (z - 700.0) / 175.0) * 34.0
            + perlin.noise2((x - 170.0) / 58.0, (z + 290.0) / 58.0) * 14.0)
            * anchor;
    let wz = z
        + (perlin.noise2((x - 2100.0) / 175.0, (z + 1900.0) / 175.0) * 34.0
            + perlin.noise2((x + 370.0) / 58.0, (z - 510.0) / 58.0) * 14.0)
            * anchor;
    (wx, wz)
}

/// `LavaCaveLandmark.surfaceHeight(perlin, localX, localZ, underlyingHeight)`.
pub fn lava_cave_surface_height(perlin: &PerlinNoise, x: f32, z: f32, underlying: f32) -> f32 {
    const SURFACE_RADIUS: f32 = 65.0;
    const OPENING_RADIUS: f32 = 14.0;
    let (wx, wz) = lava_cave_warp(perlin, x, z);
    let distance = ((wx * wx + wz * wz) as f64).sqrt() as f32;
    if distance >= SURFACE_RADIUS || underlying < 63.0 {
        return underlying;
    }
    let cone = smoothstep(1.0 - distance / SURFACE_RADIUS) * 20.0;
    let vent = smoothstep(1.0 - distance / (OPENING_RADIUS + 4.0));
    let rim = (1.0 - (distance - (OPENING_RADIUS + 2.0)).abs() / 5.0).max(0.0) * 2.5;
    underlying + cone * (1.0 - vent) + rim
}

/// `LavaCaveLandmark.isMarkerBiome`: warped distance < 25.
pub fn lava_cave_is_marker(perlin: &PerlinNoise, x: f32, z: f32) -> bool {
    let anchor = smoothstep(((x * x + z * z) as f64).sqrt() as f32 / 48.0);
    let wx = x
        + (perlin.noise2((x + 1300.0) / 175.0, (z - 700.0) / 175.0) * 34.0
            + perlin.noise2((x - 170.0) / 58.0, (z + 290.0) / 58.0) * 14.0)
            * anchor;
    let wz = z
        + (perlin.noise2((x - 2100.0) / 175.0, (z + 1900.0) / 175.0) * 34.0
            + perlin.noise2((x + 370.0) / 58.0, (z - 510.0) / 58.0) * 14.0)
            * anchor;
    wx * wx + wz * wz < 25.0 * 25.0
}
