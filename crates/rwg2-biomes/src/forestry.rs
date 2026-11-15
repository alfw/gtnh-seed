//! Forestry (4.11.40, GTNH fork) bee climate and the vanilla height-dependent biome temperature it reads.
//!
//! - `EnumTemperature.getFromBiome(biome, x, y, z)`: HELLISH if BiomeDictionary NETHER, otherwise thresholds on
//!   `biome.getFloatTemperature(x, y, z)`.
//! - `EnumHumidity.getFromValue(biome.rainfall)`.
//! - `BiomeGenBase.getFloatTemperature(x, y, z)` (1.7.10): above y = 64 it drops by
//!   `(temperatureNoise(x/8, z/8) * 4 + y - 64) * 0.05 / 30`, where `temperatureNoise` is the static
//!   `NoiseGeneratorPerlin(new Random(1234L), 1)` (one simplex octave), i.e. identical in every world.

use crate::java_random::JavaRandom;

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
#[repr(u8)]
pub enum Temperature {
    Icy = 1,
    Cold = 2,
    Normal = 3,
    Warm = 4,
    Hot = 5,
    Hellish = 6,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
#[repr(u8)]
pub enum Humidity {
    Arid = 0,
    Normal = 1,
    Damp = 2,
}

/// `EnumTemperature.getFromValue`.
pub fn temperature_class(t: f32) -> Temperature {
    if t > 1.00f32 {
        Temperature::Hot
    } else if t > 0.85f32 {
        Temperature::Warm
    } else if t > 0.35f32 {
        Temperature::Normal
    } else if t > 0.0f32 {
        Temperature::Cold
    } else {
        Temperature::Icy
    }
}

/// `EnumHumidity.getFromValue`.
pub fn humidity_class(r: f32) -> Humidity {
    if r > 0.85f32 {
        Humidity::Damp
    } else if r >= 0.3f32 {
        Humidity::Normal
    } else {
        Humidity::Arid
    }
}

const GRAD: [[i32; 3]; 12] = [
    [1, 1, 0],
    [-1, 1, 0],
    [1, -1, 0],
    [-1, -1, 0],
    [1, 0, 1],
    [-1, 0, 1],
    [1, 0, -1],
    [-1, 0, -1],
    [0, 1, 1],
    [0, -1, 1],
    [0, 1, -1],
    [0, -1, -1],
];

/// `NoiseGeneratorSimplex` (vanilla 1.7.10), 2D path only.
pub struct Simplex {
    perm: [i32; 512],
}

impl Simplex {
    pub fn new(r: &mut JavaRandom) -> Self {
        let _ox = r.next_double() * 256.0;
        let _oy = r.next_double() * 256.0;
        let _oz = r.next_double() * 256.0;
        let mut perm = [0i32; 512];
        for (i, p) in perm.iter_mut().enumerate().take(256) {
            *p = i as i32;
        }
        for i in 0..256usize {
            let j = r.next_int_bounded(256 - i as i32) as usize + i;
            perm.swap(i, j);
            perm[i + 256] = perm[i];
        }
        Simplex { perm }
    }

    #[inline]
    fn fastfloor(x: f64) -> i32 {
        if x > 0.0 {
            x as i32
        } else {
            (x as i32).wrapping_sub(1)
        }
    }

    #[inline]
    fn dot(g: &[i32; 3], x: f64, y: f64) -> f64 {
        g[0] as f64 * x + g[1] as f64 * y
    }

    /// `func_151605_a(x, y)`.
    pub fn noise(&self, x: f64, y: f64) -> f64 {
        let sqrt3 = 3.0f64.sqrt();
        let f2 = 0.5 * (sqrt3 - 1.0);
        let s = (x + y) * f2;
        let i = Self::fastfloor(x + s);
        let j = Self::fastfloor(y + s);
        let g2 = (3.0 - sqrt3) / 6.0;
        let t = (i.wrapping_add(j)) as f64 * g2;
        let x0 = x - (i as f64 - t);
        let y0 = y - (j as f64 - t);
        let (i1, j1) = if x0 > y0 { (1, 0) } else { (0, 1) };
        let x1 = x0 - i1 as f64 + g2;
        let y1 = y0 - j1 as f64 + g2;
        let x2 = x0 - 1.0 + 2.0 * g2;
        let y2 = y0 - 1.0 + 2.0 * g2;
        let ii = (i & 255) as usize;
        let jj = (j & 255) as usize;
        let p = &self.perm;
        let gi0 = (p[ii + p[jj] as usize] % 12) as usize;
        let gi1 = (p[ii + i1 + p[jj + j1] as usize] % 12) as usize;
        let gi2 = (p[ii + 1 + p[jj + 1] as usize] % 12) as usize;
        let mut t0 = 0.5 - x0 * x0 - y0 * y0;
        let n0 = if t0 < 0.0 {
            0.0
        } else {
            t0 *= t0;
            t0 * t0 * Self::dot(&GRAD[gi0], x0, y0)
        };
        let mut t1 = 0.5 - x1 * x1 - y1 * y1;
        let n1 = if t1 < 0.0 {
            0.0
        } else {
            t1 *= t1;
            t1 * t1 * Self::dot(&GRAD[gi1], x1, y1)
        };
        let mut t2 = 0.5 - x2 * x2 - y2 * y2;
        let n2 = if t2 < 0.0 {
            0.0
        } else {
            t2 *= t2;
            t2 * t2 * Self::dot(&GRAD[gi2], x2, y2)
        };
        70.0 * (n0 + n1 + n2)
    }
}

/// `BiomeGenBase.temperatureNoise` + `getFloatTemperature`.
pub struct TemperatureNoise {
    s: Simplex,
}

impl Default for TemperatureNoise {
    fn default() -> Self {
        Self::new()
    }
}

impl TemperatureNoise {
    pub fn new() -> Self {
        let mut r = JavaRandom::new(1234);
        TemperatureNoise {
            s: Simplex::new(&mut r),
        }
    }

    /// `NoiseGeneratorPerlin(r, 1).func_151601_a(x, z)` (one octave: d2 += noise(x*1, z*1)/1).
    pub fn sample(&self, x: f64, z: f64) -> f64 {
        0.0 + self.s.noise(x * 1.0, z * 1.0) / 1.0
    }

    /// `getFloatTemperature(x, y, z)` for a biome with base `temperature`.
    pub fn float_temperature(&self, temperature: f32, x: i32, y: i32, z: i32) -> f32 {
        if y > 64 {
            let f = self.sample(x as f64 * 1.0 / 8.0, z as f64 * 1.0 / 8.0) as f32 * 4.0f32;
            temperature - (f + y as f32 - 64.0f32) * 0.05f32 / 30.0f32
        } else {
            temperature
        }
    }
}

/// Per in-game biome id: what Forestry reads.
#[derive(Clone, Copy, Debug, Default)]
pub struct BiomeClimate {
    pub temperature: f32,
    pub rainfall: f32,
    pub hellish: bool,
    /// BiomeDictionary RIVER/BEACH/MUSHROOM/OCEAN (Roguelike refuses these).
    pub rogue_invalid: bool,
    /// BiomeDictionary JUNGLE/DENSE: populated trees/vines (`Material.vine` counts as ground) almost always fail
    /// Roguelike's flatness checks (heuristic, from the pack dump).
    pub rogue_vegetation: bool,
}

/// Biome id → climate (dense by id; EndlessIDs ids are u16).
#[derive(Clone, Default)]
pub struct ClimateTable {
    pub by_id: Vec<Option<BiomeClimate>>,
    /// BiomeDictionary types per biome id, in `getTypesForBiome` order (Thaumcraft indexes into it).
    pub dicts: Vec<Vec<String>>,
}

impl ClimateTable {
    pub fn dict(&self, id: u16) -> &[String] {
        self.dicts
            .get(id as usize)
            .map(|v| v.as_slice())
            .unwrap_or(&[])
    }

    pub fn get(&self, id: u16) -> Option<BiomeClimate> {
        self.by_id.get(id as usize).copied().flatten()
    }

    /// Forestry classes at a block. `None` if the biome is unknown.
    pub fn classify(
        &self,
        noise: &TemperatureNoise,
        id: u16,
        x: i32,
        y: i32,
        z: i32,
    ) -> Option<(Temperature, Humidity)> {
        let c = self.get(id)?;
        let t = if c.hellish {
            Temperature::Hellish
        } else {
            temperature_class(noise.float_temperature(c.temperature, x, y, z))
        };
        Some((t, humidity_class(c.rainfall)))
    }

    /// Parse `data/<pack>/climate.json`: `{"biomes":[{"id":..,"t":"<f32 bits hex>","r":"<hex>","hellish":bool}]}`.
    #[cfg(feature = "json")]
    pub fn from_json(s: &str) -> Result<Self, String> {
        let v: serde_json::Value = serde_json::from_str(s).map_err(|e| e.to_string())?;
        let arr = v["biomes"].as_array().ok_or("biomes missing")?;
        let mut by_id: Vec<Option<BiomeClimate>> = Vec::new();
        let mut dicts: Vec<Vec<String>> = Vec::new();
        for b in arr {
            let id = b["id"].as_u64().ok_or("id")? as usize;
            let bits = |k: &str| -> Result<f32, String> {
                let h = b[k].as_str().ok_or(format!("{k} missing"))?;
                Ok(f32::from_bits(
                    u32::from_str_radix(h, 16).map_err(|e| e.to_string())?,
                ))
            };
            if by_id.len() <= id {
                by_id.resize(id + 1, None);
                dicts.resize(id + 1, Vec::new());
            }
            dicts[id] = b["dict"]
                .as_array()
                .map(|a| {
                    a.iter()
                        .filter_map(|t| t.as_str().map(String::from))
                        .collect()
                })
                .unwrap_or_default();
            by_id[id] = Some(BiomeClimate {
                temperature: bits("t")?,
                rainfall: bits("r")?,
                hellish: b["hellish"].as_bool().unwrap_or(false),
                rogue_invalid: b["rogueInvalid"].as_bool().unwrap_or(false),
                rogue_vegetation: b["rogueVegetation"].as_bool().unwrap_or(false),
            });
        }
        Ok(ClimateTable { by_id, dicts })
    }
}
