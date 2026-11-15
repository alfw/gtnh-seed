//! Port of `rwg2.util.ContinentalNoise`: warped priority-Voronoi continent distance field + islands + landmarks.
//!
//! Java keeps several ThreadLocal single-entry caches (last landform sample, last island/landmark centre). They are
//! pure memoization; reproduced here as single-entry `RefCell` caches for speed only.

use std::cell::Cell;

use crate::config::RwgConfig;
use crate::island::IslandPointNoise;
use crate::jmath::{cell_hash, pack, round_d, unit};
use crate::landmark::{candidate_key, ContinentLandmarkNoise};
use crate::noise::NoiseGenerator;
use crate::perlin::PerlinNoise;
use crate::poisson::PoissonPointNoise;

pub const VOLCANO_ISLAND_RADIUS: f64 = 160.0;
pub const VOLCANO_RADIUS: f64 = 130.0;
pub const LAVA_CAVE_INFLUENCE_RADIUS: f64 = 320.0;
const POISSON_ROUNDS: i32 = 8;
const WARP_SCALE: f64 = 3600.0;
const WARP_STRENGTH: f64 = 1800.0;
const LAND_COVERAGE_DENSITY: f64 = 1.82;
const OVERLAP_CORRECTION_BASE: f64 = 0.75;
const OVERLAP_CORRECTION_SLOPE: f64 = 0.46;
const CONTINENT_VOLCANO_SEED_SALT: i64 = 0x082EFA98EC4E6C89;
const CONTINENT_LAVA_CAVE_SEED_SALT: i64 = 0x6A09E667F3BCC909;
pub const CONTINENTAL_SEED_XOR: i64 = 0x6A09E667F3BCC909;

/// Sentinel used throughout RWG2 for "none" in packed-coordinate APIs.
pub const NONE: i64 = i64::MIN;

#[derive(Clone, Copy, Debug, Default)]
pub struct Landform {
    pub value: f32,
    pub island: bool,
    pub island_width: f64,
    pub island_seed_x: i32,
    pub island_seed_z: i32,
    pub island_local_x: f32,
    pub island_local_z: f32,
    pub volcano: bool,
    pub volcano_island: bool,
    pub volcano_key: i64,
    pub volcano_center_x: f64,
    pub volcano_center_z: f64,
    pub lava_cave: bool,
    pub lava_cave_key: i64,
    pub lava_cave_center_x: f64,
    pub lava_cave_center_z: f64,
    pub lava_cave_local_x: f32,
    pub lava_cave_local_z: f32,
}

pub struct ContinentalNoise {
    seed: i64,
    points: PoissonPointNoise,
    islands: IslandPointNoise,
    volcanoes: ContinentLandmarkNoise,
    lava_caves: ContinentLandmarkNoise,
    minimum_continent_width: f64,
    continent_width_range: f64,
    maximum_island_width: f64,
    minimum_island_width: f64,
    island_width_range: f64,
    voronoi_radius: f64,
    continent_dilation: f64,
    warp_x: PerlinNoise,
    warp_y: PerlinNoise,
    offset_x: f64,
    offset_y: f64,
    landmarks_enabled: bool,
    large_island_volcano_chance: f64,
    last: Cell<Option<(i32, i32, Landform)>>,
    island_center: Cell<(f64, f64, f64, f64)>,
    volcano_center: Cell<(f64, f64, f64, f64)>,
    lava_center: Cell<(f64, f64, f64, f64)>,
}

impl ContinentalNoise {
    /// `seed` is the already-xored continental seed (`worldSeed ^ 0x6A09E667F3BCC909`).
    pub fn new(seed: i64, cfg: &RwgConfig) -> Self {
        // Math.min/max on floats, then widened.
        let min_cw = cfg.minimum_continent_width.min(cfg.maximum_continent_width) as f64;
        let max_cw = cfg.minimum_continent_width.max(cfg.maximum_continent_width) as f64;
        let min_iw = cfg.minimum_island_width.min(cfg.maximum_island_width) as f64;
        let max_iw = cfg.minimum_island_width.max(cfg.maximum_island_width) as f64;
        let avg_cw = (min_cw + max_cw) * 0.5;
        let voronoi_radius = avg_cw + cfg.average_ocean_width as f64;
        let continent_dilation = continent_dilation(
            min_cw,
            max_cw,
            voronoi_radius,
            cfg.minimum_ocean_width as f64,
            cfg.maximum_ocean_fraction as f64,
        );
        let points = PoissonPointNoise::new(
            seed,
            max_cw * 2.0 + cfg.minimum_ocean_width as f64,
            POISSON_ROUNDS,
        );
        let islands = IslandPointNoise::new(
            seed ^ (0xA4093822299F31D0u64 as i64),
            &points,
            max_cw,
            cfg.minimum_island_width as f64,
            cfg.maximum_island_width as f64,
            cfg.minimum_ocean_width as f64,
            cfg.island_placement_chance as f64,
        );
        let lm = cfg.average_landmarks_per_type_and_continent as f64;
        let volcanoes = ContinentLandmarkNoise::new(
            seed,
            seed ^ CONTINENT_VOLCANO_SEED_SALT,
            &points,
            min_cw,
            max_cw,
            voronoi_radius,
            lm,
            VOLCANO_ISLAND_RADIUS,
        );
        let lava_caves = ContinentLandmarkNoise::new(
            seed,
            seed ^ CONTINENT_LAVA_CAVE_SEED_SALT,
            &points,
            min_cw,
            max_cw,
            voronoi_radius,
            lm,
            LAVA_CAVE_INFLUENCE_RADIUS,
        );
        let origin = points.sample(0.0, 0.0);
        let nan4 = (f64::NAN, f64::NAN, 0.0, 0.0);
        Self {
            seed,
            points,
            islands,
            volcanoes,
            lava_caves,
            minimum_continent_width: min_cw,
            continent_width_range: max_cw - min_cw,
            maximum_island_width: max_iw,
            minimum_island_width: min_iw,
            island_width_range: max_iw - min_iw,
            voronoi_radius,
            continent_dilation,
            warp_x: PerlinNoise::new(seed ^ 0x243F6A8885A308D3),
            warp_y: PerlinNoise::new(seed ^ 0x13198A2E03707344),
            offset_x: origin.x,
            offset_y: origin.z,
            landmarks_enabled: cfg.average_landmarks_per_type_and_continent > 0.0,
            large_island_volcano_chance: cfg.large_island_volcano_chance as f64,
            last: Cell::new(None),
            island_center: Cell::new(nan4),
            volcano_center: Cell::new(nan4),
            lava_center: Cell::new(nan4),
        }
    }

    /// Distance from shore in blocks: positive on land, negative at sea.
    pub fn value(&self, x: i32, y: i32) -> f32 {
        self.landform(x, y).value
    }

    pub fn is_island(&self, x: i32, y: i32) -> bool {
        self.landform(x, y).island
    }

    /// 0/1 small/large island, -1 if not on an island.
    pub fn island_size_tier(&self, x: i32, y: i32) -> i32 {
        let s = self.landform(x, y);
        if !s.island {
            return -1;
        }
        self.size_tier(s.island_width)
    }

    pub fn island_seed_coordinates(&self, x: i32, y: i32) -> i64 {
        let s = self.landform(x, y);
        if !s.island {
            return NONE;
        }
        pack(s.island_seed_x, s.island_seed_z)
    }

    pub fn island_center_coordinates(&self, x: i32, y: i32) -> i64 {
        let s = self.landform(x, y);
        if !s.island {
            return NONE;
        }
        let (_, _, cx, cz) = self.landmark_center(
            s.island_seed_x as f64,
            s.island_seed_z as f64,
            &self.island_center,
        );
        (round_d(cx) << 32) | (round_d(cz) & 0xffff_ffff)
    }

    pub fn volcano_coordinates(&self, x: i32, y: i32) -> i64 {
        self.volcano_coordinates_r(x, y, VOLCANO_RADIUS)
    }

    pub fn volcano_vicinity_coordinates(&self, x: i32, y: i32) -> i64 {
        self.volcano_coordinates_r(x, y, VOLCANO_ISLAND_RADIUS)
    }

    fn volcano_coordinates_r(&self, x: i32, y: i32, radius: f64) -> i64 {
        let s = self.landform(x, y);
        let d2 = s.island_local_x * s.island_local_x + s.island_local_z * s.island_local_z; // float math
        if !s.volcano || d2 as f64 > radius * radius {
            return NONE;
        }
        pack(
            s.island_local_x.to_bits() as i32,
            s.island_local_z.to_bits() as i32,
        )
    }

    pub fn volcano_seed_key(&self, x: i32, y: i32) -> i64 {
        self.landform(x, y).volcano_key
    }

    pub fn is_island_volcano(&self, x: i32, y: i32) -> bool {
        let s = self.landform(x, y);
        s.volcano && s.volcano_island
    }

    pub fn volcano_center_coordinates(&self, x: i32, y: i32) -> i64 {
        let s = self.landform(x, y);
        if !s.volcano {
            return NONE;
        }
        (round_d(s.volcano_center_x) << 32) | (round_d(s.volcano_center_z) & 0xffff_ffff)
    }

    pub fn lava_cave_coordinates(&self, x: i32, y: i32) -> i64 {
        let s = self.landform(x, y);
        if !s.lava_cave {
            return NONE;
        }
        pack(
            s.lava_cave_local_x.to_bits() as i32,
            s.lava_cave_local_z.to_bits() as i32,
        )
    }

    pub fn lava_cave_seed_key(&self, x: i32, y: i32) -> i64 {
        self.landform(x, y).lava_cave_key
    }

    pub fn lava_cave_center_coordinates(&self, x: i32, y: i32) -> i64 {
        let s = self.landform(x, y);
        if !s.lava_cave {
            return NONE;
        }
        (round_d(s.lava_cave_center_x) << 32) | (round_d(s.lava_cave_center_z) & 0xffff_ffff)
    }

    /// Full landform sample (memoized for the last coordinate, like Java's ThreadLocal).
    pub fn landform(&self, x: i32, y: i32) -> Landform {
        if let Some((lx, ly, s)) = self.last.get() {
            if lx == x && ly == y {
                return s;
            }
        }
        let s = self.compute_landform(x, y);
        self.last.set(Some((x, y, s)));
        s
    }

    #[inline]
    fn warp(&self, x: f64, y: f64) -> (f64, f64) {
        let nx = x / WARP_SCALE;
        let ny = y / WARP_SCALE;
        let (fx, fy, fx2, fy2) = (nx as f32, ny as f32, (nx * 2.0) as f32, (ny * 2.0) as f32);
        let wx = x
            + self.warp_x.noise2(fx, fy) as f64 * WARP_STRENGTH
            + self.warp_x.noise2(fx2, fy2) as f64 * WARP_STRENGTH;
        let wy = y
            + self.warp_y.noise2(fx, fy) as f64 * WARP_STRENGTH
            + self.warp_y.noise2(fx2, fy2) as f64 * WARP_STRENGTH;
        (wx, wy)
    }

    fn compute_landform(&self, x: i32, y: i32) -> Landform {
        let (wx, wy) = self.warp(x as f64, y as f64);
        let (px, py) = (wx + self.offset_x, wy + self.offset_y);
        let ps = self.points.sample(px, py);
        let continent = self.continent_field(ps.distance, ps.cell_x, ps.cell_z);

        let mut r = Landform {
            volcano_key: NONE,
            lava_cave_key: NONE,
            ..Default::default()
        };
        if continent >= 0.0 && self.landmarks_enabled {
            let v = self.volcanoes.sample(&self.points, px, py);
            if v.distance.is_finite() {
                let (_, _, cx, cz) = self.landmark_center(v.x, v.z, &self.volcano_center);
                let (lx, lz) = (x as f64 - cx, y as f64 - cz);
                if lx * lx + lz * lz <= VOLCANO_ISLAND_RADIUS * VOLCANO_ISLAND_RADIUS {
                    r.volcano = true;
                    r.volcano_key = candidate_key(
                        self.seed ^ CONTINENT_VOLCANO_SEED_SALT,
                        v.cell_x,
                        v.cell_z,
                        v.round,
                    );
                    r.volcano_center_x = cx;
                    r.volcano_center_z = cz;
                    r.island_local_x = lx as f32;
                    r.island_local_z = lz as f32;
                }
            }
            let l = self.lava_caves.sample(&self.points, px, py);
            if l.distance.is_finite() {
                let (_, _, cx, cz) = self.landmark_center(l.x, l.z, &self.lava_center);
                let (lx, lz) = (x as f64 - cx, y as f64 - cz);
                if lx * lx + lz * lz <= LAVA_CAVE_INFLUENCE_RADIUS * LAVA_CAVE_INFLUENCE_RADIUS {
                    r.lava_cave = true;
                    r.lava_cave_key = candidate_key(
                        self.seed ^ CONTINENT_LAVA_CAVE_SEED_SALT,
                        l.cell_x,
                        l.cell_z,
                        l.round,
                    );
                    r.lava_cave_center_x = cx;
                    r.lava_cave_center_z = cz;
                    r.lava_cave_local_x = lx as f32;
                    r.lava_cave_local_z = lz as f32;
                }
            }
        }
        if continent >= self.maximum_island_width {
            r.value = continent as f32;
            return r;
        }

        let is = self.islands.sample(&self.points, px, py);
        let island = is.value;
        // Java Math.max(double,double): NaN-propagating; no NaNs occur here.
        r.value = continent.max(island) as f32;
        r.island = island >= 0.0 && island > continent;
        if r.island {
            r.island_width = is.width;
            r.island_seed_x = round_d(is.x) as i32; // (int) Math.round: long → int truncation
            r.island_seed_z = round_d(is.z) as i32;
            let (_, _, cx, cz) = self.landmark_center(
                r.island_seed_x as f64,
                r.island_seed_z as f64,
                &self.island_center,
            );
            r.island_local_x = (x as f64 - cx) as f32;
            r.island_local_z = (y as f64 - cz) as f32;
            r.volcano = self.size_tier(r.island_width) == 1
                && self.random01(r.island_seed_x, r.island_seed_z, 17)
                    < self.large_island_volcano_chance;
            if r.volcano {
                r.volcano_island = true;
                r.volcano_key = pack(r.island_seed_x, r.island_seed_z);
                r.volcano_center_x = cx;
                r.volcano_center_z = cz;
            }
        }
        r
    }

    fn continent_field(&self, distance: f64, cx: i32, cy: i32) -> f64 {
        let width =
            self.minimum_continent_width + self.random01(cx, cy, 0) * self.continent_width_range;
        self.voronoi_radius.min(width) + self.continent_dilation - distance
    }

    fn size_tier(&self, width: f64) -> i32 {
        let f = if self.island_width_range == 0.0 {
            0.0
        } else {
            (width - self.minimum_island_width) / self.island_width_range
        };
        if f < 0.5 {
            0
        } else {
            1
        }
    }

    /// `getIslandCenter` / `getLandmarkCenter`: invert the coastline warp by 12 damped fixed-point iterations.
    fn landmark_center(
        &self,
        seed_x: f64,
        seed_z: f64,
        cache: &Cell<(f64, f64, f64, f64)>,
    ) -> (f64, f64, f64, f64) {
        let c = cache.get();
        if c.0 == seed_x && c.1 == seed_z {
            return c;
        }
        let mut cx = seed_x - self.offset_x;
        let mut cz = seed_z - self.offset_y;
        for _ in 0..12 {
            let (wx, wz) = self.warp(cx, cz);
            let wx = wx + self.offset_x;
            let wz = wz + self.offset_y;
            cx -= (wx - seed_x) * 0.5;
            cz -= (wz - seed_z) * 0.5;
        }
        let r = (seed_x, seed_z, cx, cz);
        cache.set(r);
        r
    }

    fn random01(&self, cx: i32, cy: i32, salt: i32) -> f64 {
        unit(cell_hash(self.seed, cx, cy, salt))
    }
}

fn continent_dilation(
    min_w: f64,
    max_w: f64,
    radius_limit: f64,
    min_ocean: f64,
    ocean_limit: f64,
) -> f64 {
    if ocean_limit >= 1.0 {
        return 0.0;
    }
    let lower = radius_limit.min(min_w);
    let upper = radius_limit.min(max_w);
    let mean = (lower + upper) * 0.5;
    let mean_sq = (lower * lower + lower * upper + upper * upper) / 3.0;
    let spacing = max_w * 2.0 + min_ocean;
    let requested = 1.0 - ocean_limit;
    let correction = OVERLAP_CORRECTION_BASE + OVERLAP_CORRECTION_SLOPE * requested;
    let desired = 1f64.min(requested * correction);
    let desired_mean_sq = desired * spacing * spacing / LAND_COVERAGE_DENSITY;
    if desired_mean_sq <= mean_sq {
        return 0.0;
    }
    (mean * mean + desired_mean_sq - mean_sq).sqrt() - mean
}

/// `ContinentalNoise.unpackVolcanoX`
pub fn unpack_x(c: i64) -> f32 {
    f32::from_bits(((c as u64) >> 32) as u32)
}

/// `ContinentalNoise.unpackVolcanoY`
pub fn unpack_y(c: i64) -> f32 {
    f32::from_bits(c as u32)
}
