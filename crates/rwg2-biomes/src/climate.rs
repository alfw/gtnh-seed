//! Climate (meta-biome) selection from `ChunkManagerRealistic`: `getClimateAt` / `getClimateValue` /
//! `warpClimateCoordinates` / `getClimateFromValue`.
//!
//! Precision notes (match Java exactly):
//! - `scale`/`strength` are `float` (`climateWidth * .4f`, `* .8f`).
//! - `x + noise2(x / scale, y / scale) * strength` is **float** arithmetic (int promoted to float), only then widened
//!   to double into `output[]`.
//! - `(warped + 4000D) / climateWidth` widens the float `climateWidth` to double.

use crate::cell::CellNoise;
use crate::noise::NoiseGenerator;
use crate::perlin::PerlinNoise;

pub const CLIMATE_WARP_SEED_XOR: i64 = 0xBB67AE8584CAA73Bu64 as i64;

const CLIMATE_WARP_SCALE_MULTIPLIER: f32 = 0.4;
const CLIMATE_WARP_STRENGTH_MULTIPLIER: f32 = 0.8;
const SNOW_CLIMATE_LIMIT: f32 = 0.2251875;
const COLD_CLIMATE_LIMIT: f32 = 0.545;
const HOT_CLIMATE_LIMIT: f32 = 0.78;
const THREE_CLIMATE_SNOW_LIMIT: f32 = 0.3039375;
const THREE_CLIMATE_COLD_LIMIT: f32 = 0.62375;

/// Meta-biome / climate zone. Values match RWG2's ints (ocean = 0 is assigned by the caller).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
#[repr(u8)]
pub enum Climate {
    Snow = 1,
    Cold = 2,
    Hot = 3,
    Wet = 4,
}

pub struct ClimateField {
    pub(crate) climate_warp: PerlinNoise,
    pub(crate) biomecell: CellNoise,
    pub(crate) climate_width: f32,
    pub(crate) wet_enabled: bool,
    pub(crate) biome_offset_x: i32,
    pub(crate) biome_offset_z: i32,
}

impl ClimateField {
    /// `climate_width = 1400f * biomeSizeMultiplier`.
    pub fn new(seed: i64, climate_width: f32, wet_enabled: bool) -> Self {
        Self {
            climate_warp: PerlinNoise::new(seed ^ CLIMATE_WARP_SEED_XOR),
            biomecell: CellNoise::new(seed, 0),
            climate_width,
            wet_enabled,
            biome_offset_x: 0,
            biome_offset_z: 0,
        }
    }

    #[inline]
    pub fn warp(&self, x: i32, y: i32) -> (f64, f64) {
        let scale = self.climate_width * CLIMATE_WARP_SCALE_MULTIPLIER;
        let strength = self.climate_width * CLIMATE_WARP_STRENGTH_MULTIPLIER;
        let (xf, yf) = (x as f32, y as f32);
        let wx = xf + self.climate_warp.noise2(xf / scale, yf / scale) * strength;
        let wy = yf
            + self
                .climate_warp
                .noise2((xf + 1731.0) / scale, (yf - 2459.0) / scale)
                * strength;
        (wx as f64, wy as f64)
    }

    #[inline]
    pub fn value(&self, x: i32, y: i32) -> f32 {
        let (wx, wy) = self.warp(
            x.wrapping_add(self.biome_offset_x),
            y.wrapping_add(self.biome_offset_z),
        );
        let cw = self.climate_width as f64;
        self.biomecell.noise((wx + 4000.0) / cw, wy / cw, 1.0) * 0.5 + 0.5
    }

    #[inline]
    pub fn from_value(&self, c: f32) -> Climate {
        if self.wet_enabled {
            if c < SNOW_CLIMATE_LIMIT {
                Climate::Snow
            } else if c < COLD_CLIMATE_LIMIT {
                Climate::Cold
            } else if c < HOT_CLIMATE_LIMIT {
                Climate::Hot
            } else {
                Climate::Wet
            }
        } else if c < THREE_CLIMATE_SNOW_LIMIT {
            Climate::Snow
        } else if c < THREE_CLIMATE_COLD_LIMIT {
            Climate::Cold
        } else {
            Climate::Hot
        }
    }

    #[inline]
    pub fn at(&self, x: i32, y: i32) -> Climate {
        self.from_value(self.value(x, y))
    }
}
