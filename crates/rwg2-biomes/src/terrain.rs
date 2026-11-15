//! Port of RWG2's height path: `rwg2.terrain.Terrain*` + biome-specific `rNoise` overrides.
//!
//! No transcendentals here: only Perlin/Cell noise, `sqrt`, `abs`, min/max, so results are bit-exact.
//! Note: the `cell` passed by `ChunkManagerRealistic.getNoiseAt` is the manager's `cell` (`useDistance = true`).
//!
//! Precision: `x / 30f` (float division, widened when a double is needed) vs `x / 25D` (double division) matter.

#![allow(
    clippy::misrefactored_assign_op,
    clippy::manual_clamp,
    clippy::assign_op_pattern,
    clippy::too_many_arguments
)] // Java's exact forms

use crate::cell::CellNoise;
use crate::noise::NoiseGenerator;
use crate::perlin::PerlinNoise;

/// A `rwg2.terrain.Terrain*` instance (parameters from `rwg2dump` v2 `biomes.json`).
#[derive(Clone, Debug, PartialEq)]
pub enum Terrain {
    /// `TerrainBase`: constant 70.
    Base,
    FlatLakes,
    SmallSupport,
    Polar,
    Marsh,
    SwampRiver,
    DuneValley {
        valley: f32,
    },
    GrasslandFlats,
    Dunes,
    SmallIsland,
    GrasslandMountains,
    Mountain,
    MountainSpikes,
    Highland {
        start: f32,
        height: f32,
        base: f32,
        width: f32,
        flat_detail_strength: f32,
    },
    MountainRiver,
    SwampMountain {
        height: f32,
        width: f32,
    },
    Mesa,
    Hilly {
        width: f32,
        strength: f32,
        lake_depth: f32,
        lake_width: f32,
        terrain_height: f32,
    },
    GrasslandHills {
        h_height: f32,
        h_width: f32,
        v_height: f32,
        v_width: f32,
        l_height: f32,
        l_width: f32,
        b_height: f32,
    },
    Canyon {
        small_river: bool,
        height: Vec<f32>,
        strength: f32,
        base: f32,
    },
}

/// How a biome computes `rNoise(perlin, cell, x, y, ocean, border, river)`.
#[derive(Clone, Debug, PartialEq)]
pub enum HeightModel {
    /// Delegates to a terrain object.
    Terrain(Terrain),
    /// `RealisticBiomeBase` default / test biomes.
    Const(f32),
    CoastColdCliff,
    CoastColdSlope,
    CoastDunes,
    CoastIce,
    /// `RealisticBiomeCoastMangrove` == ColdCliff formula.
    CoastMangrove,
    /// `RealisticBiomeCoastOasis` / `CoastTest`.
    CoastOasis,
    IslandTropical,
    IslandTundra,
    IslandVolcano,
    Ocean {
        shallow: bool,
    },
    OceanTest,
}

#[inline(always)]
fn n(p: &PerlinNoise, x: f32, y: f32) -> f32 {
    p.noise2(x, y)
}

impl Terrain {
    pub fn generate(
        &self,
        p: &PerlinNoise,
        c: &CellNoise,
        x: i32,
        y: i32,
        border: f32,
        river: f32,
    ) -> f32 {
        let (xf, yf) = (x as f32, y as f32);
        let (xd, yd) = (x as f64, y as f64);
        match self {
            Terrain::Base => 70.0,
            Terrain::FlatLakes => {
                let mut h = n(p, xf / 300.0, yf / 300.0) * 40.0 * river;
                h = if h > 3.0 { 3.0 } else { h };
                h += n(p, xf / 50.0, yf / 50.0) * (12.0 - h) * 0.4;
                h += n(p, xf / 15.0, yf / 15.0) * (12.0 - h) * 0.15;
                62.0 + h
            }
            Terrain::SmallSupport => {
                let mut h = n(p, xf / 100.0, yf / 100.0) * 8.0;
                h += n(p, xf / 30.0, yf / 30.0) * 4.0;
                h += n(p, xf / 15.0, yf / 15.0) * 2.0;
                h += n(p, xf / 7.0, yf / 7.0);
                70.0 + (20.0 * river) + h
            }
            Terrain::Polar => {
                let mut st = (n(p, xf / 160.0, yf / 160.0) + 0.38) * 35.0 * river;
                st = if st < 0.2 { 0.2 } else { st };
                let mut h = n(p, xf / 60.0, yf / 60.0) * st * 2.0;
                h = if h > 0.0 { -h } else { h };
                h += st;
                h *= h / 50.0;
                h += st;
                70.0 + h
            }
            Terrain::Marsh => {
                let mut h = n(p, xf / 130.0, yf / 130.0) * 30.0;
                h += n(p, xf / 12.0, yf / 12.0) * 2.0;
                h += n(p, xf / 18.0, yf / 18.0) * 4.0;
                h = if h < 4.0 { 0.0 } else { h - 4.0 };
                if h == 0.0 {
                    h += n(p, xf / 20.0, yf / 20.0) + n(p, xf / 5.0, yf / 5.0);
                }
                62.0 + h
            }
            Terrain::SwampRiver => {
                let mut h = n(p, xf / 180.0, yf / 180.0) * 40.0 * river;
                h *= h / 35.0;
                if h < 1.0 {
                    h = 1.0;
                }
                if h < 4.0 {
                    h += (n(p, xf / 50.0, yf / 50.0) + n(p, xf / 15.0, yf / 15.0)) * (4.0 - h);
                }
                60.0 + h
            }
            Terrain::DuneValley { valley } => {
                let mut h = (n(p, xf / valley, yf / valley) + 0.25) * 65.0 * river;
                h = if h < 1.0 { 1.0 } else { h };
                let r = c.noise(xd / 50.0, yd / 50.0, 1.0) * h * 2.0;
                h += r;
                h += n(p, xf / 40.0, yf / 40.0) * 8.0;
                h += n(p, xf / 14.0, yf / 14.0) * 2.0;
                70.0 + h
            }
            Terrain::GrasslandFlats => {
                let mut h = n(p, xf / 100.0, yf / 100.0) * 7.0;
                h += n(p, xf / 20.0, yf / 20.0) * 2.0;
                let mut m = n(p, xf / 180.0, yf / 180.0) * 70.0 * river;
                m *= m / 40.0;
                let mut sm = n(p, xf / 30.0, yf / 30.0) * 8.0;
                sm *= if m / 20.0 > 3.75 { 3.75 } else { m / 20.0 };
                m += sm;
                let mut l = n(p, xf / 260.0, yf / 260.0) * 38.0;
                l *= l / 25.0;
                l = if l < -8.0 { -8.0 } else { l };
                68.0 + h + m - l
            }
            Terrain::Dunes => {
                let mut st = (n(p, xf / 160.0, yf / 160.0) + 0.38) * 35.0;
                st = if st < 0.2 { 0.2 } else { st };
                let mut h = n(p, xf / 60.0, yf / 60.0) * st * 2.0;
                h = if h > 0.0 { -h } else { h };
                h += st;
                h *= h / 50.0;
                h += st;
                if h < 10.0 {
                    let mut d = (h - 10.0) / 2.0;
                    d = if d > 4.0 { 4.0 } else { d };
                    h += c.noise(xd / 25.0, yd / 25.0, 1.0) * d;
                    h += n(p, xf / 30.0, yf / 30.0) * d;
                    h += n(p, xf / 14.0, yf / 14.0) * d * 0.5;
                }
                70.0 + (h * river)
            }
            Terrain::SmallIsland => {
                const HILL_START: f32 = 6.0;
                const HILL_HEIGHT: f32 = 120.0;
                const BASE_HEIGHT: f32 = 58.0;
                const HILL_WIDTH: f32 = 200.0;
                let mut h = n(p, xf / HILL_WIDTH, yf / HILL_WIDTH) * HILL_HEIGHT * river;
                h = if h < HILL_START {
                    HILL_START + ((h - HILL_START) / 4.5)
                } else {
                    h
                };
                if h > 0.0 {
                    let st = if h * 1.5 > 15.0 { 15.0 } else { h * 1.5 };
                    h += c.noise(xd / 70.0, yd / 70.0, 1.0) * st;
                }
                h += n(p, xf / 20.0, yf / 20.0) * 5.0;
                h += n(p, xf / 12.0, yf / 12.0) * 3.0;
                h += n(p, xf / 5.0, yf / 5.0) * 1.5;
                jmax(BASE_HEIGHT, BASE_HEIGHT + h)
            }
            Terrain::GrasslandMountains => {
                let mut h = n(p, xf / 100.0, yf / 100.0) * 7.0;
                h += n(p, xf / 20.0, yf / 20.0) * 2.0;
                let mut m = n(p, xf / 230.0, yf / 230.0) * 120.0 * river;
                m *= m / 35.0;
                m = if m > 70.0 { 70.0 + (m - 70.0) / 2.5 } else { m };
                let cc = c.noise((xf / 30.0) as f64, (yf / 30.0) as f64, 1.0) * (m * 0.30);
                let mut sm = n(p, xf / 30.0, yf / 30.0) * 8.0 + n(p, xf / 8.0, yf / 8.0);
                sm *= if m / 20.0 > 2.5 { 2.5 } else { m / 20.0 };
                m += sm;
                m += cc;
                let mut l = n(p, xf / 260.0, yf / 260.0) * 38.0;
                l *= l / 25.0;
                l = if l < -8.0 { -8.0 } else { l };
                68.0 + h + m - l
            }
            Terrain::Mountain => {
                let mut h = n(p, xf / 300.0, yf / 300.0) * 135.0 * river;
                h *= h / 32.0;
                h = if h > 150.0 { 150.0 } else { h };
                if h > 10.0 {
                    let d = if (h - 10.0) / 2.0 > 8.0 {
                        8.0
                    } else {
                        (h - 10.0) / 2.0
                    };
                    h += n(p, xf / 35.0, yf / 35.0) * d;
                    h += n(p, xf / 60.0, yf / 60.0) * d * 0.5;
                    if h > 35.0 {
                        let d2 = if (h - 35.0) / 1.5 > 30.0 {
                            30.0
                        } else {
                            (h - 35.0) / 1.5
                        };
                        h += c.noise(xd / 25.0, yd / 25.0, 1.0) * d2;
                    }
                }
                h += n(p, xf / 28.0, yf / 28.0) * 4.0;
                h += n(p, xf / 18.0, yf / 18.0) * 2.0;
                h += n(p, xf / 8.0, yf / 8.0) * 2.0;
                h + 67.0
            }
            Terrain::MountainSpikes => {
                let b = 12.0 + (n(p, xf / 300.0, yf / 300.0) * 6.0);
                let mut h = c.noise(xd / 200.0, yd / 200.0, 1.0) * b * river;
                h *= h * 1.5;
                h = if h > 155.0 { 155.0 } else { h };
                if h > 2.0 {
                    let d = if (h - 2.0) / 2.0 > 8.0 {
                        8.0
                    } else {
                        (h - 2.0) / 2.0
                    };
                    h += n(p, xf / 30.0, yf / 30.0) * d;
                    h += n(p, xf / 50.0, yf / 50.0) * d * 0.5;
                    if h > 35.0 {
                        let d2 = if (h - 35.0) / 1.5 > 30.0 {
                            30.0
                        } else {
                            (h - 35.0) / 1.5
                        };
                        h += c.noise(xd / 25.0, yd / 25.0, 1.0) * d2;
                    }
                }
                h += n(p, xf / 18.0, yf / 18.0) * 3.0;
                h += n(p, xf / 8.0, yf / 8.0) * 2.0;
                45.0 + h + (b * 2.0)
            }
            Terrain::Highland {
                start,
                height,
                base,
                width,
                flat_detail_strength,
            } => {
                let (start, width) = (*start, *width);
                let mut h = n(p, xf / width, yf / width) * height * river;
                h = if h < start {
                    start + ((h - start) / 4.5)
                } else {
                    h
                };
                if h > 0.0 {
                    let st = if h * 1.5 > 15.0 { 15.0 } else { h * 1.5 };
                    h += c.noise(xd / 70.0, yd / 70.0, 1.0) * st;
                }
                let hill_detail = jmax(0.0, jmin(1.0, (h - start) / 15.0));
                let detail = flat_detail_strength + (1.0 - flat_detail_strength) * hill_detail;
                h += n(p, xf / 20.0, yf / 20.0) * 5.0 * detail;
                h += n(p, xf / 12.0, yf / 12.0) * 3.0 * detail;
                h += n(p, xf / 5.0, yf / 5.0) * 1.5 * detail;
                base + h
            }
            Terrain::MountainRiver => {
                let mut h = n(p, xf / 300.0, yf / 300.0) * 135.0 * river;
                h *= h / 32.0;
                h = if h > 150.0 { 150.0 } else { h };
                if h < 10.0 {
                    h += n(p, xf / 14.0, yf / 14.0) * (10.0 - h) * 0.2;
                }
                h = mountain_detail(p, c, xf, yf, xd, yd, h);
                h + 67.0
            }
            Terrain::SwampMountain { height, width } => {
                let width = *width;
                let mut h = n(p, xf / width, yf / width) * height * river;
                h *= h / 32.0;
                h = if h > 150.0 { 150.0 } else { h };
                if h < 14.0 {
                    h += n(p, xf / 25.0, yf / 25.0) * (14.0 - h) * 0.8;
                }
                if h < 6.0 {
                    h = 6.0 - ((6.0 - h) * 0.07)
                        + n(p, xf / 20.0, yf / 20.0)
                        + n(p, xf / 5.0, yf / 5.0);
                }
                h = mountain_detail(p, c, xf, yf, xd, yd, h);
                h + 56.0
            }
            Terrain::Mesa => {
                let mut b = n(p, xf / 130.0, yf / 130.0) * 50.0 * river;
                b *= b / 40.0;
                let hn = n(p, xf / 12.0, yf / 12.0);
                let mut sb = 0.0f32;
                if b > 2.0 {
                    sb = (b - 2.0) / 2.0;
                    sb = if sb < 0.0 {
                        0.0
                    } else if sb > 5.5 {
                        5.5
                    } else {
                        sb
                    };
                    sb *= hn;
                }
                b += sb;
                b = if b < 0.1 { 0.1 } else { b };
                let mut c1 = 0.0f32;
                if b > 1.0 {
                    c1 = if b > 5.5 { 4.5 } else { b - 1.0 };
                    c1 *= 3.0;
                }
                let mut c2 = 0.0f32;
                if b > 5.5 && border > 0.95 + hn * 0.09 {
                    c2 = if b > 6.0 { 0.5 } else { b - 5.5 };
                    c2 *= 35.0;
                }
                let mut bn = 0.0f32;
                if b < 7.0 {
                    let bnh = 5.0 - b;
                    bn += n(p, xf / 70.0, yf / 70.0) * (bnh * 0.4);
                    bn += n(p, xf / 20.0, yf / 20.0) * (bnh * 0.3);
                }
                let mut w = n(p, xf / 80.0, yf / 80.0) * 25.0;
                w *= w / 25.0;
                b += c1 + c2 + bn - w;
                74.0 + b
            }
            Terrain::Hilly {
                width,
                strength,
                lake_depth,
                lake_width,
                terrain_height,
            } => {
                let (width, lake_width) = (*width, *lake_width);
                let mut h = n(p, xf / 20.0, yf / 20.0) * 2.0;
                h += n(p, xf / 7.0, yf / 7.0) * 0.8;
                let mut m = n(p, xf / width, yf / width) * strength * river;
                m *= m / 35.0;
                m = if m > 70.0 { 70.0 + (m - 70.0) / 2.5 } else { m };
                let mut st = m * 0.7;
                st = if st > 20.0 { 20.0 } else { st };
                let cc = c.noise((xf / 30.0) as f64, (yf / 30.0) as f64, 1.0) * (5.0 + st);
                let mut sm = n(p, xf / 30.0, yf / 30.0) * 8.0 + n(p, xf / 8.0, yf / 8.0);
                sm *= if (m + 10.0) / 20.0 > 2.5 {
                    2.5
                } else {
                    (m + 10.0) / 20.0
                };
                m += sm;
                m += cc;
                let mut l = n(p, xf / lake_width, yf / lake_width) * lake_depth;
                l *= l / 25.0;
                l = if l < -8.0 { -8.0 } else { l };
                terrain_height + h + m - l
            }
            Terrain::GrasslandHills {
                h_height,
                h_width,
                v_height,
                v_width,
                l_height,
                l_width,
                b_height,
            } => {
                let (h_width, v_width, l_width) = (*h_width, *v_width, *l_width);
                let mut h = n(p, xf / v_width, yf / v_width) * v_height * river;
                h += n(p, xf / 20.0, yf / 20.0) * 2.0;
                let mut m = n(p, xf / h_width, yf / h_width) * h_height * river;
                m *= m / 40.0;
                let mut sm = n(p, xf / 30.0, yf / 30.0) * 8.0;
                sm *= if m / 20.0 > 3.75 { 3.75 } else { m / 20.0 };
                m += sm;
                let mut cm = c.noise(xd / 25.0, yd / 25.0, 1.0) * 12.0;
                cm *= if m / 20.0 > 3.75 { 3.75 } else { m / 20.0 };
                m += cm;
                let mut l = n(p, xf / l_width, yf / l_width) * l_height;
                l *= l / 25.0;
                l = if l < 8.0 { 8.0 } else { l };
                h += n(p, xf / 12.0, yf / 12.0) * 3.0;
                h += n(p, xf / 5.0, yf / 5.0) * 1.5;
                b_height + h + m - l
            }
            Terrain::Canyon {
                small_river,
                height,
                strength,
                base,
            } => {
                let mut river = river * 1.3;
                river = if river > 1.0 { 1.0 } else { river };
                let mut r = n(p, xf / 100.0, yf / 100.0) * 50.0;
                r = if r < -7.4 {
                    -7.4
                } else if r > 7.4 {
                    7.4
                } else {
                    r
                };
                let mut b = (17.0 + r) * river;
                let hn = n(p, xf / 12.0, yf / 12.0) * 0.5;
                let mut sb = 0.0f32;
                if b > 0.0 {
                    sb = b;
                    sb = if sb < 0.0 {
                        0.0
                    } else if sb > 7.0 {
                        7.0
                    } else {
                        sb
                    };
                    sb *= hn;
                }
                b += sb;
                let mut c_total = 0.0f32;
                let mut i = 0;
                while i + 1 < height.len() {
                    let mut c_temp = 0.0f32;
                    if b > height[i] && border > 0.6 + (height[i] * 0.015) + hn * 0.2 {
                        c_temp = if b > height[i] + height[i + 1] {
                            height[i + 1]
                        } else {
                            b - height[i]
                        };
                        c_temp *= strength;
                    }
                    c_total += c_temp;
                    i += 2;
                }
                let mut bn = 0.0f32;
                if *small_river {
                    if b < 5.0 {
                        bn = 5.0 - b;
                        for _ in 0..3 {
                            bn *= bn / 4.5;
                        }
                    }
                } else if b < 5.0 {
                    bn = (n(p, xf / 7.0, yf / 7.0) * 1.3 + n(p, xf / 15.0, yf / 15.0) * 2.0)
                        * (5.0 - b)
                        * 0.2;
                }
                b += c_total - bn;
                base + b
            }
        }
    }
}

/// Shared tail of TerrainMountainRiver / TerrainSwampMountain.
#[inline]
fn mountain_detail(
    p: &PerlinNoise,
    c: &CellNoise,
    xf: f32,
    yf: f32,
    xd: f64,
    yd: f64,
    mut h: f32,
) -> f32 {
    if h > 10.0 {
        let d = if (h - 10.0) / 2.0 > 8.0 {
            8.0
        } else {
            (h - 10.0) / 2.0
        };
        h += n(p, xf / 35.0, yf / 35.0) * d;
        h += n(p, xf / 60.0, yf / 60.0) * d * 0.5;
        if h > 35.0 {
            let d2 = if (h - 35.0) / 1.5 > 30.0 {
                30.0
            } else {
                (h - 35.0) / 1.5
            };
            h += c.noise(xd / 25.0, yd / 25.0, 1.0) * d2;
        }
    }
    if h > 2.0 {
        let d = if (h - 2.0) / 2.0 > 4.0 {
            4.0
        } else {
            (h - 2.0) / 2.0
        };
        h += n(p, xf / 28.0, yf / 28.0) * d;
        h += n(p, xf / 18.0, yf / 18.0) * (d / 2.0);
        h += n(p, xf / 8.0, yf / 8.0) * (d / 2.0);
    }
    h
}

/// Java `Math.max(float, float)` (NaN-propagating; -0 < +0).
#[inline(always)]
fn jmax(a: f32, b: f32) -> f32 {
    if a.is_nan() || b.is_nan() {
        f32::NAN
    } else if a == 0.0 && b == 0.0 {
        if a.is_sign_negative() {
            b
        } else {
            a
        }
    } else if a >= b {
        a
    } else {
        b
    }
}

/// Java `Math.min(float, float)`.
#[inline(always)]
fn jmin(a: f32, b: f32) -> f32 {
    if a.is_nan() || b.is_nan() {
        f32::NAN
    } else if a == 0.0 && b == 0.0 {
        if a.is_sign_negative() {
            a
        } else {
            b
        }
    } else if a <= b {
        a
    } else {
        b
    }
}

#[inline(always)]
fn smoothstep(v: f32) -> f32 {
    let v = jmax(0.0, jmin(1.0, v));
    v * v * (3.0 - 2.0 * v)
}

// RealisticBiomeIslandVolcano constants
const CRATER_RADIUS: f32 = 26.0;
const LAVA_FILL_RADIUS: f32 = 37.5;
const FULL_UNDERLYING_INFLUENCE_RADIUS: f32 = 70.0;
const RIM_RADIUS: f32 = 44.0;
const OUTER_SLOPE_WIDTH: f32 = 86.0;
const CRATER_FLOOR_OFFSET: f32 = 4.0;
const RIM_HEIGHT_OFFSET: f32 = 44.0;
const TERRAIN_HEIGHT_RELIEF_FACTOR: f32 = 0.2;
const MIN_RIM_HEIGHT_OFFSET: f32 = 40.0;
const MAX_RIM_HEIGHT_OFFSET: f32 = 60.0;

fn volcano_rim_offset(base: f32) -> f32 {
    jmax(
        MIN_RIM_HEIGHT_OFFSET,
        jmin(
            MAX_RIM_HEIGHT_OFFSET,
            RIM_HEIGHT_OFFSET + (base - 63.0) * TERRAIN_HEIGHT_RELIEF_FACTOR,
        ),
    )
}

/// `RealisticBiomeIslandVolcano.rNoiseAt(perlin, localX, localZ, baseHeight, underlyingHeight)`.
pub fn volcano_r_noise_at(p: &PerlinNoise, lx: f32, lz: f32, base: f32, underlying: f32) -> f32 {
    let mut distance = ((lx as f64) * (lx as f64) + (lz as f64) * (lz as f64)).sqrt() as f32;
    distance += n(p, lx / 18.0, lz / 18.0) * 3.0;
    let foot = smoothstep((distance - RIM_RADIUS) / OUTER_SLOPE_WIDTH);
    let local_base = base + (underlying - base) * foot;
    let structure = 1.0 - foot;
    let rim = volcano_rim_offset(base);
    let height = if distance < CRATER_RADIUS {
        local_base + CRATER_FLOOR_OFFSET + n(p, lx / 9.0, lz / 9.0) * 1.5
    } else if distance < RIM_RADIUS {
        let r = (distance - CRATER_RADIUS) / (RIM_RADIUS - CRATER_RADIUS);
        local_base + CRATER_FLOOR_OFFSET + r * (rim - CRATER_FLOOR_OFFSET)
    } else {
        let slope = jmax(0.0, 1.0 - (distance - RIM_RADIUS) / OUTER_SLOPE_WIDTH);
        local_base + slope * rim
    };
    let volcano = height + n(p, lx / 24.0, lz / 24.0) * 2.0 * structure;
    let infl = smoothstep(
        (distance - LAVA_FILL_RADIUS) / (FULL_UNDERLYING_INFLUENCE_RADIUS - LAVA_FILL_RADIUS),
    );
    volcano + jmax(0.0, underlying - volcano) * infl
}

impl HeightModel {
    /// `rNoise(perlin, cell, x, y, ocean, border, river)`.
    pub fn r_noise(
        &self,
        p: &PerlinNoise,
        c: &CellNoise,
        x: i32,
        y: i32,
        ocean: f32,
        border: f32,
        river: f32,
    ) -> f32 {
        let (xf, yf) = (x as f32, y as f32);
        let coast_river = |r: f32| if r > 0.5 { 1.0 } else { r * 2.0 };
        let start = || {
            n(p, xf / 90.0, yf / 90.0) * 1.0
                + n(p, xf / 40.0, yf / 40.0) * 0.15
                + n(p, xf / 9.0, yf / 9.0) * 0.07
        };
        match self {
            HeightModel::Terrain(t) => t.generate(p, c, x, y, border, river),
            HeightModel::Const(v) => *v,
            HeightModel::CoastColdCliff | HeightModel::CoastMangrove => {
                let mut h = if ocean < 0.5 { ocean * 14.0 } else { 7.0 };
                if ocean < 1.9 {
                    h += n(p, xf / 12.0, yf / 12.0) * 1.5;
                    h += n(p, xf / 20.0, yf / 20.0) * 3.0;
                }
                55.0 + h
            }
            HeightModel::CoastColdSlope => {
                let river = coast_river(river);
                let start = start();
                let mut h = if ocean < 1.0 { ocean * 15.0 } else { 15.0 };
                let mut cc = 0.0f32;
                if ocean + start > 0.8 {
                    cc = if ocean + start > 1.5 {
                        0.7
                    } else {
                        (ocean + start) - 0.8
                    };
                    cc *= 35.0 + n(p, xf / 50.0, yf / 50.0) * 8.0;
                }
                if ocean < 1.6 {
                    h += n(p, xf / 12.0, yf / 12.0);
                    h += n(p, xf / 20.0, yf / 20.0) * 2.0;
                }
                55.0 + h + (cc * river)
            }
            HeightModel::CoastDunes => {
                let mut h = if ocean < 0.5 { ocean * 18.0 } else { 9.0 };
                if ocean < 1.9 {
                    h += n(p, xf / 12.0, yf / 12.0) * 1.0;
                    h += n(p, xf / 23.0, yf / 23.0) * 2.0;
                }
                if ocean > 1.0 {
                    h += n(p, xf / 15.0, yf / 15.0) * (ocean - 1.0) * 8.0;
                    h += n(p, xf / 25.0, yf / 25.0) * (ocean - 1.0) * 13.0;
                    h += (ocean - 1.0) * 9.0;
                }
                55.0 + h
            }
            HeightModel::CoastIce => {
                let river = coast_river(river);
                let start = start();
                let mut h = 0.0f32;
                let mut cc = 0.0f32;
                if ocean + start > 1.4 {
                    cc = if ocean + start > 1.5 {
                        0.1
                    } else {
                        (ocean + start) - 1.4
                    };
                    cc *= 250.0 + n(p, xf / 50.0, yf / 50.0) * 25.0;
                }
                if ocean < 1.3 {
                    h += n(p, xf / 12.0, yf / 12.0);
                    h += n(p, xf / 20.0, yf / 20.0) * 2.0;
                }
                55.0 + h + (cc * river)
            }
            HeightModel::CoastOasis => {
                let river = coast_river(river);
                let start = start();
                let mut h = if ocean < 1.0 { ocean * 10.0 } else { 10.0 };
                let mut cc = 0.0f32;
                if ocean + start > 1.0 {
                    cc = if ocean + start > 1.2 {
                        0.2
                    } else {
                        (ocean + start) - 1.0
                    };
                    cc *= 100.0 + n(p, xf / 50.0, yf / 50.0) * 55.0;
                }
                if ocean < 1.2 {
                    h += n(p, xf / 12.0, yf / 12.0);
                    h += n(p, xf / 20.0, yf / 20.0) * 2.0;
                }
                55.0 + h + (cc * river)
            }
            HeightModel::IslandTropical => {
                let mut st = 15.0
                    - ((c.noise(x as f64 / 500.0, y as f64 / 500.0, 1.0) * 42.0)
                        + (n(p, xf / 30.0, yf / 30.0) * 2.0));
                st = if st < 0.0 { 0.0 } else { st };
                let mut h = st;
                h = if h < 0.0 { 0.0 } else { h };
                h += (h * 0.4) * ((h * 0.4) * 2.0);
                if h > 10.0 {
                    let d2 = if (h - 10.0) / 1.5 > 30.0 {
                        30.0
                    } else {
                        (h - 10.0) / 1.5
                    };
                    h += c.noise(x as f64 / 25.0, y as f64 / 25.0, 1.0) * d2;
                }
                h += n(p, xf / 18.0, yf / 18.0) * 3.0;
                h += n(p, xf / 8.0, yf / 8.0) * 2.0;
                55.0 + h * border
            }
            HeightModel::IslandTundra => {
                let mut h = 0.0f32;
                if border > 0.9 {
                    h = (border - 0.9) * (150.0 + n(p, xf / 45.0, yf / 45.0) * 50.0);
                }
                66.0 + h
            }
            // rNoise(perlin, cell, x, y, ...) -> rNoiseAt(perlin, x, y, 61f): world coords used as local coords
            HeightModel::IslandVolcano => volcano_r_noise_at(p, xf, yf, 61.0, 61.0),
            HeightModel::Ocean { shallow } => {
                let height = if *shallow { 52.0 } else { 34.0 };
                height + n(p, xf / 220.0, yf / 220.0) * 4.0 + n(p, xf / 55.0, yf / 55.0) * 1.5
            }
            HeightModel::OceanTest => {
                45.0 + n(p, xf / 220.0, yf / 220.0) * 4.0 + n(p, xf / 55.0, yf / 55.0) * 1.5
            }
        }
    }
}

/// `RealisticBiomeIslandVolcano.isInsideLavaFill(perlin, localX, localZ)`.
pub fn volcano_inside_lava_fill(p: &PerlinNoise, lx: f32, lz: f32) -> bool {
    let mut distance = ((lx as f64) * (lx as f64) + (lz as f64) * (lz as f64)).sqrt() as f32;
    distance += n(p, lx / 18.0, lz / 18.0) * 3.0;
    distance < LAVA_FILL_RADIUS
}
