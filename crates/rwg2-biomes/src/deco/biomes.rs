//! The native RWG2 `rDecorate` methods (`rwg2.biomes.realistic.*`), draw for draw.
//!
//! `chunkX` / `chunkY` in the Java are block coordinates (`chunk * 16`); loops `i < k * strength` compare an int
//! against a float, `rand.nextInt((int) (k / strength))` truncates (and saturates to `i32::MAX` at strength 0).

use super::gens::{Branchy, JungleKind, G};
use super::*;
use crate::java_random::JavaRandom;
use crate::noise::NoiseGenerator as _;

const CACTUS_FLOWERS: &[i32] = &[9, 9, 9, 9, 3, 3, 3, 3, 3, 2, 2, 2, 11, 11, 11];
const ALL_FLOWERS: &[i32] = &[0, 1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11];
const COLD_FLOWERS: &[i32] = &[9, 0, 3];
const RAINFOREST_FLOWERS: &[i32] = &[1, 1, 1, 8];
const POPPY: &[i32] = &[0];

const COBBLE: Db = blk::COBBLE as Db;
const MOSSY: Db = db::mk(db::MOSSY, 0);
const SHRUB: G = G::Shrub(0, 0);
const OAK: G = G::Trees(4, 0, 0, false);
const BIRCH: G = G::Forest(false);

/// number of iterations of `for (int i = 0; i < k; i++)` with a float bound
fn times(k: f32) -> i32 {
    let mut n = 0;
    while (n as f32) < k {
        n += 1;
    }
    n
}

fn sav(t: i32) -> G {
    G::Savannah(t, true)
}

#[allow(clippy::too_many_arguments)]
fn branchy(
    log: u8,
    leaves: u8,
    base: i32,
    root: i32,
    length: f32,
    branch: i32,
    vs: f32,
    vr: f32,
) -> Branchy {
    Branchy {
        log: db::mk(db::LOG, log),
        leaves: db::mk(db::LEAVES, leaves),
        base,
        root,
        length,
        branch,
        ver_start: vs,
        ver_rand: vr,
    }
}

type P = (i32, i32, i32);

struct C<'a, 'b, 'w> {
    w: &'a mut DecoWorld<'w>,
    r: &'b mut JavaRandom,
    x: i32,
    z: i32,
    s: f32,
    river: f32,
}

impl C<'_, '_, '_> {
    fn n(&mut self, b: i32) -> i32 {
        self.r.next_int_bounded(b)
    }
    fn bool(&mut self) -> bool {
        self.r.next_boolean()
    }
    fn f(&mut self) -> f32 {
        self.r.next_float()
    }
    fn px(&mut self) -> i32 {
        self.x + self.n(16) + 8
    }
    fn pz(&mut self) -> i32 {
        self.z + self.n(16) + 8
    }
    /// `x, z, world.getHeightValue(x, z)`
    fn surf(&mut self) -> P {
        let a = self.px();
        let c = self.pz();
        (a, self.w.height(a, c), c)
    }
    /// `x, base + rand.nextInt(n), z`
    fn scat(&mut self, base: i32, n: i32) -> P {
        let a = self.px();
        let b = base + self.n(n);
        let c = self.pz();
        (a, b, c)
    }
    fn gen(&mut self, g: G, (x, y, z): P) {
        g.generate(self.w, self.r, x, y, z);
    }
    /// `rand.nextInt((int) (k / strength)) == 0`
    fn chance(&mut self, k: f32) -> bool {
        let b = (k / self.s) as i32;
        self.n(b) == 0
    }
    fn noise(&self, x: f32, z: f32) -> f32 {
        self.w.p.perlin().noise2(x, z)
    }
    fn xf(&self) -> f32 {
        self.x as f32
    }
    fn zf(&self) -> f32 {
        self.z as f32
    }

    // --- shared fragments ---

    fn blob(&mut self, b: Db, p: P) {
        self.gen(G::Blob(b, 0), p);
    }
    fn reed(&mut self) {
        let a = self.px();
        let c = self.pz();
        let y = 60 + self.n(8);
        self.gen(G::Reed, (a, y, c));
    }
    fn pumpkin(&mut self, ymax: i32) {
        let p = self.scat(0, ymax);
        self.gen(G::Pumpkin, p);
    }
    fn flowers(&mut self, list: &'static [i32], base: i32, n: i32) {
        let p = self.scat(base, n);
        self.gen(G::Flowers(list), p);
    }
    fn tallgrass(&mut self, base: i32, n: i32) {
        let p = self.scat(base, n);
        self.gen(G::Grass(db::TALLGRASS, 1), p);
    }
    /// `nextInt(oneIn) == 0 ? DecoGrass(double_plant, 2) : DecoGrass(tallgrass, 1)`
    fn grass_mix(&mut self, base: i32, n: i32, one_in: i32) {
        let p = self.scat(base, n);
        let g = if self.n(one_in) == 0 {
            G::Grass(db::DOUBLE_PLANT, 2)
        } else {
            G::Grass(db::TALLGRASS, 1)
        };
        self.gen(g, p);
    }
    fn cacti(&mut self, base: i32, n: i32, below: i32, sand_only: bool) {
        let p = self.scat(base, n);
        if p.1 < below {
            self.gen(G::Cacti(sand_only, 0), p);
        }
    }
    fn dead_bush(&mut self, base: i32, n: i32) {
        let p = self.scat(base, n);
        self.gen(G::DeadBush, p);
    }
    fn wheat(&mut self) {
        let p = self.scat(60, 60);
        let t = self.n(3);
        self.gen(G::WildWheat(t), p);
    }
    fn mushrooms(&mut self) {
        let p = self.scat(64, 64);
        let g = if self.bool() {
            G::FlowersV(db::BROWN_MUSHROOM, 0)
        } else {
            G::FlowersV(db::RED_MUSHROOM, 0)
        };
        self.gen(g, p);
    }
    /// `if (rand.nextInt((int) (15f / strength)) == 0)` surface lake, `lo < y (< hi)`
    fn lake(&mut self, hi: i32) {
        if self.chance(15.0) {
            let p = self.surf();
            if p.1 > 63 && p.1 < hi {
                self.gen(G::Lakes(true), p);
            }
        }
    }
    /// `DecoShrub(nextInt(4) + 1, log, leaves)` with the log / leaves draws `nextInt(lr)` (or fixed when `lr == 0`)
    fn deco_shrub(&mut self, p: P, size_rand: i32, size_add: i32, log_r: i32, leaves_r: i32) {
        let s = self.n(size_rand) + size_add;
        let l = if log_r > 0 { self.n(log_r) } else { 0 };
        let v = if leaves_r > 0 { self.n(leaves_r) } else { 0 };
        self.gen(G::DecoShrub(s, l, v, false), p);
    }

    /// The `river > 0.7` block of the desert family (Desert, Dune Valley, (Red) Desert Mountains, Mesa, Savanna
    /// Dunes, Stone Mountains (Cactus)).
    fn desert_wet(&mut self, grass_one_in: i32) {
        if self.river > 0.86 {
            for _ in 0..times(10.0 * self.s) {
                let p = self.surf();
                if p.1 < 100 || (p.1 < 120 && self.n(10) == 0) {
                    let g = if self.n(4) != 0 { SHRUB } else { sav(1) };
                    self.gen(g, p);
                }
            }
        }
        for _ in 0..times(12.0 * self.s) {
            self.cacti(0, 160, 120, false);
        }
        for _ in 0..times(2.0 * self.s) {
            self.reed();
        }
        if self.n(28) == 0 {
            self.pumpkin(128);
        }
        for _ in 0..3 {
            self.flowers(CACTUS_FLOWERS, 0, 128);
        }
        for _ in 0..15 {
            self.grass_mix(0, 128, grass_one_in);
        }
    }

    /// the forest family's log + shrubs
    fn forest_log_shrub(&mut self) {
        if self.chance(4.0) {
            let p = self.surf();
            if p.1 < 100 {
                let l = 3 + self.n(4);
                self.gen(G::Log(1, l, false), p);
            }
        }
        for _ in 0..times(3.0 * self.s) {
            let p = self.surf();
            if p.1 < 110 {
                self.deco_shrub(p, 4, 1, 0, 3);
            }
        }
    }

    /// the forest family's wheat, pumpkins, flowers, grass
    fn forest_tail(&mut self) {
        if self.chance(150.0) {
            self.wheat();
        }
        if self.chance(15.0) {
            self.pumpkin(100);
        }
        for _ in 0..times(8.0 * self.s) {
            self.flowers(ALL_FLOWERS, 0, 128);
        }
        for _ in 0..times(12.0 * self.s) {
            self.tallgrass(0, 128);
        }
    }

    /// `rand.nextInt(4) == 0 ? DecoSmallPine(4 + n7, 6 + n9, 0) : nextInt(6) != 0 ? WorldGenTrees : WorldGenForest`
    fn redwood_small(&mut self, p: P) {
        let g = if self.n(4) == 0 {
            let a = 4 + self.n(7);
            let b = 6 + self.n(9);
            G::SmallPine(a, b, 0)
        } else if self.n(6) != 0 {
            OAK
        } else {
            BIRCH
        };
        self.gen(g, p);
    }

    /// `nextInt(3) != 0 ? DecoSmallPine(1 + n3, 3 + n5, 0) : DecoSmallSpruce(n2)`
    fn tundra_pine(&mut self, p: P) {
        let g = if self.n(3) != 0 {
            let a = 1 + self.n(3);
            let b = 3 + self.n(5);
            G::SmallPine(a, b, 0)
        } else {
            G::SmallSpruce(self.n(2))
        };
        self.gen(g, p);
    }

    fn mossy_rocks(&mut self, k: f32, odds: i32) {
        for _ in 0..times(k * self.s) {
            let p = self.surf();
            if p.1 < 95 && (p.1 < 64 || self.n(odds) == 0) {
                self.blob(MOSSY, p);
            }
        }
    }
}

// --- biomes ----------------------------------------------------------------------------------------------------

fn coast_cold_slope(c: &mut C) {
    for _ in 0..times(2.0 * c.s) {
        let p = c.surf();
        if p.1 < 68 || c.n(4) == 0 {
            c.blob(COBBLE, p);
        }
    }
    if c.chance(4.0) {
        let p = c.surf();
        if p.1 < 70 {
            let l = 3 + c.n(4);
            c.gen(G::Log(1, l, false), p);
        }
    }
    for _ in 0..times(c.s) {
        c.flowers(COLD_FLOWERS, 64, 64);
    }
    for _ in 0..times(3.0 * c.s) {
        c.tallgrass(64, 64);
    }
}

/// `CoastOasis`, `CoastTest`
fn coast_oasis(c: &mut C) {
    if c.s > 0.7 {
        for _ in 0..times(12.0 * c.s) {
            let p = c.surf();
            if p.1 > 65 && p.1 < 90 {
                let g = if c.n(3) != 0 {
                    SHRUB
                } else if c.n(6) != 0 {
                    sav(1)
                } else if c.n(5) != 0 {
                    sav(2)
                } else {
                    sav(0)
                };
                c.gen(g, p);
            }
        }
    }
    for _ in 0..times(12.0 * c.s) {
        c.cacti(0, 160, 120, false);
    }
    for _ in 0..times(2.0 * c.s) {
        c.reed();
    }
    if c.n(28) == 0 {
        c.pumpkin(128);
    }
    for _ in 0..3 {
        c.flowers(CACTUS_FLOWERS, 0, 128);
    }
    for _ in 0..15 {
        c.grass_mix(0, 128, 6);
    }
}

fn desert(c: &mut C) {
    if c.chance(4.0) {
        let p = c.surf();
        if p.1 < 85 {
            c.blob(COBBLE, p);
        }
    }
    if c.river > 0.7 {
        c.desert_wet(6);
    }
    for _ in 0..times(5.0 * c.s) {
        c.cacti(64, 64, 120, false);
    }
    for _ in 0..times(c.s) {
        c.dead_bush(64, 64);
    }
}

fn dune_valley(c: &mut C) {
    if c.chance(2.0) {
        let p = c.surf();
        if p.1 < 85 {
            c.blob(COBBLE, p);
        }
    }
    for _ in 0..times(12.0 * c.s) {
        c.cacti(0, 160, i32::MAX, false);
    }
    for _ in 0..times(3.0 * c.s) {
        c.dead_bush(0, 160);
    }
    if c.river > 0.7 {
        c.desert_wet(6);
    }
}

/// `DesertMountains`, `RedDesertMountains`
fn desert_mountains(c: &mut C) {
    if c.chance(2.0) {
        let p = c.surf();
        if p.1 < 85 {
            c.blob(COBBLE, p);
        }
    }
    if c.river > 0.7 {
        c.desert_wet(6);
    }
    for _ in 0..12 {
        c.cacti(0, 160, 120, false);
    }
    for _ in 0..times(3.0 * c.s) {
        c.dead_bush(0, 160);
    }
}

/// `Oasis`, `RedOasis`
fn oasis(c: &mut C, red: bool) {
    if c.chance(2.0) {
        let p = c.surf();
        if p.1 < 85 {
            c.blob(COBBLE, p);
        }
    }
    if red {
        for _ in 0..times(3.0 * c.s) {
            let p = c.surf();
            if p.1 < 100 || (p.1 < 120 && c.n(10) == 0) {
                let g = if c.n(4) != 0 {
                    SHRUB
                } else {
                    G::Savannah(1, false)
                };
                c.gen(g, p);
            }
        }
    } else {
        for _ in 0..times(4.0 * c.s) {
            let p = c.surf();
            if p.1 < 71 {
                let g = if c.n(4) != 0 {
                    SHRUB
                } else if c.n(6) == 0 {
                    G::Palm
                } else {
                    G::Savannah(1, false)
                };
                c.gen(g, p);
            }
        }
    }
    for _ in 0..times(12.0 * c.s) {
        c.cacti(0, 160, 120, false);
    }
    for _ in 0..times(2.0 * c.s) {
        c.reed();
    }
    if c.n(28) == 0 {
        c.pumpkin(128);
    }
    for _ in 0..3 {
        c.flowers(CACTUS_FLOWERS, 0, 128);
    }
    for _ in 0..15 {
        c.grass_mix(0, 128, 6);
    }
    for _ in 0..12 {
        c.cacti(0, 160, 120, false);
    }
    for _ in 0..times(3.0 * c.s) {
        c.dead_bush(0, 160);
    }
}

fn dark_redwood(c: &mut C, plains: bool) {
    let l = if plains {
        c.noise(c.xf() / 80.0, c.zf() / 80.0) * 60.0 - 15.0
    } else {
        c.noise(c.xf() / 80.0, c.zf() / 80.0) * 60.0 + 10.0
    };
    for _ in 0..times(l * c.s) {
        if c.n(6) == 0 {
            let p = c.surf();
            if p.1 < 110 {
                let (a, b) = if plains {
                    let a = 11 + c.n(11);
                    (a, 15 + c.n(15))
                } else {
                    let d = (p.1 - 70) as f32 / 5.0;
                    let a = (14.0 - d) as i32 + c.n(6);
                    (a, (15.0 - d) as i32 + c.n(10))
                };
                c.gen(G::LargePine(a, b, 1, 1), p);
            }
        }
    }
    if l > 5.0 {
        for _ in 0..times(3.0 * c.s) {
            let p = c.surf();
            if p.1 < 120 {
                c.redwood_small(p);
            }
        }
    }
    c.forest_log_shrub();
    c.forest_tail();
}

/// `Redwood`, `RedwoodSnow`
fn redwood(c: &mut C) {
    let l = c.noise(c.xf() / 80.0, c.zf() / 80.0) * 40.0 + 10.0;
    for _ in 0..times(l * c.s) {
        if c.n(10) == 0 {
            let p = c.surf();
            if p.1 < 110 {
                let d = (p.1 - 70) as f32 / 5.0;
                let a = (14.0 - d) as i32 + c.n(8);
                let b = (15.0 - d) as i32 + c.n(12);
                c.gen(G::LargePine(a, b, 0, 0), p);
            }
        }
    }
    if l > 5.0 {
        for _ in 0..times(6.0 * c.s) {
            let p = c.surf();
            if p.1 < 120 {
                c.redwood_small(p);
            }
        }
    }
    c.forest_log_shrub();
    c.forest_tail();
}

/// `WoodHills`, `WoodMountains`
fn wood_hills(c: &mut C) {
    let l = (c.noise(c.xf() / 80.0, c.zf() / 80.0) * 20.0).min(10.0);
    for _ in 0..times(c.s * l) {
        let p = c.surf();
        if p.1 < 120 {
            let g = if c.n(10) == 0 {
                G::BigTree(-1, 0)
            } else if c.n(6) != 0 {
                OAK
            } else {
                BIRCH
            };
            c.gen(g, p);
        }
    }
    if l > -1.0 {
        c.forest_log_shrub();
    }
    c.forest_tail();
}

fn coast_dunes(c: &mut C) {
    for _ in 0..times(11.0 * c.s) {
        let p = c.scat(64, 64);
        if c.n(5) == 0 {
            c.gen(G::WaterGrass(db::DOUBLE_PLANT, 2, 69), p);
        }
        let g = if c.n(3) == 0 {
            G::WaterGrass(db::LEAVES, 4, 67)
        } else {
            G::WaterGrass(db::TALLGRASS, 1, 65)
        };
        c.gen(g, p);
    }
}

fn coast_mangrove(c: &mut C) {
    for _ in 0..times(2.0 * c.s) {
        let p = c.surf();
        if p.1 < 68 {
            c.blob(COBBLE, p);
        }
    }
    for _ in 0..times(5.0 * c.s) {
        let p = c.surf();
        let base = 1 + c.n(4);
        let root = 4 + c.n(2);
        c.gen(
            G::Jungle(
                JungleKind::Mangrove,
                branchy(0, 0, base, root, 5.0, 2, 0.32, 0.14),
            ),
            p,
        );
    }
    for _ in 0..times(9.0 * c.s) {
        let p = c.scat(64, 64);
        c.gen(G::WaterGrass(db::DOUBLE_PLANT, 2, 10), p);
    }
}

fn high_rainforest(c: &mut C) {
    if c.s <= 0.0 {
        return;
    }
    for _ in 0..times(2.0 * c.s) {
        let p = c.surf();
        c.blob(MOSSY, p);
    }
    if c.w.rainforest_points.is_none() {
        c.w.rainforest_points = Some(crate::poisson::PoissonPointNoise::new(
            c.w.seed ^ 0x3C6E_F372_FE94_F82B,
            12.0,
            2,
        ));
    }
    let (cell, rounds) = {
        let pts = c.w.rainforest_points.as_ref().unwrap();
        (pts.cell_size(), pts.rounds())
    };
    let floor = |v: f64| -> i32 {
        let i = v as i32;
        if v < i as f64 {
            i - 1
        } else {
            i
        }
    };
    let (min_x, min_z) = (c.x + 8, c.z + 8);
    let (max_x, max_z) = (min_x + 16, min_z + 16);
    let (cx0, cx1) = (floor(min_x as f64 / cell), floor((max_x - 1) as f64 / cell));
    let (cz0, cz1) = (floor(min_z as f64 / cell), floor((max_z - 1) as f64 / cell));
    let lim = c.s.min(1.0);
    for cz in cz0..=cz1 {
        for cx in cx0..=cx1 {
            for round in 0..rounds {
                let (acc, (px, pz)) = {
                    let pts = c.w.rainforest_points.as_ref().unwrap();
                    (
                        pts.is_accepted_at(cx, cz, round),
                        pts.candidate_point(cx, cz, round),
                    )
                };
                if !acc {
                    continue;
                }
                let (tx, tz) = (floor(px), floor(pz));
                if tx < min_x || tx >= max_x || tz < min_z || tz >= max_z || c.f() >= lim {
                    continue;
                }
                let y = c.w.height(tx, tz);
                let base = 15 + c.n(15);
                let root = 4 + c.n(2);
                c.gen(
                    G::Jungle(
                        JungleKind::Fat,
                        branchy(3, 3, base, root, 14.0, 3, 0.2, 0.25),
                    ),
                    (tx, y, tz),
                );
            }
        }
    }
    for _ in 0..13 {
        jungle_grass(c);
    }
    for _ in 0..2 {
        if c.f() >= lim {
            continue;
        }
        c.flowers(RAINFOREST_FLOWERS, 0, 128);
    }
}

/// `nextInt(6) == 0 ? DecoGrass(double_plant, bool ? 2 : 3) : DecoGrass(tallgrass, bool ? 1 : 2)`
fn jungle_grass(c: &mut C) {
    let p = c.scat(0, 128);
    let g = if c.n(6) == 0 {
        G::Grass(db::DOUBLE_PLANT, if c.bool() { 2 } else { 3 })
    } else {
        G::Grass(db::TALLGRASS, if c.bool() { 1 } else { 2 })
    };
    c.gen(g, p);
}

fn jungle_canyon(c: &mut C) {
    for _ in 0..times(c.s) {
        let p = c.surf();
        if p.1 < 70 {
            c.blob(MOSSY, p);
        }
    }
    for _ in 0..times(5.0 * c.s) {
        let p = c.surf();
        let base = 1 + c.n(4);
        c.gen(
            G::Jungle(
                JungleKind::Small,
                branchy(3, 3, base, 0, 5.0, 2, 0.32, 0.14),
            ),
            p,
        );
    }
}

fn jungle_hills(c: &mut C) {
    for _ in 0..2 {
        let p = c.surf();
        if p.1 < 100 || c.n(3) == 0 {
            let g = if c.n(5) == 0 {
                SHRUB
            } else if c.n(3) == 0 {
                G::MegaJungle(5, 10, 3, 3)
            } else {
                G::Trees(3 + c.n(5), 3, 3, true)
            };
            c.gen(g, p);
        }
    }
    for _ in 0..15 {
        c.grass_mix(0, 128, 6);
    }
}

fn snow_lakes(c: &mut C) {
    c.lake(105);
    c.mossy_rocks(6.0, 15);
    if c.chance(25.0) {
        c.pumpkin(128);
    }
    for _ in 0..times(2.0 * c.s) {
        c.flowers(COLD_FLOWERS, 0, 128);
    }
    for _ in 0..times(3.0 * c.s) {
        c.tallgrass(0, 128);
    }
    if c.chance(4.0) {
        let p = c.surf();
        if p.1 < 120 {
            c.tundra_pine(p);
        }
    }
    if c.chance(2.0) {
        let p = c.surf();
        if p.1 < 110 {
            c.deco_shrub(p, 4, 1, 2, 2);
        }
    }
}

fn taiga_hills(c: &mut C) {
    c.mossy_rocks(6.0, 15);
    let l = c.noise(c.xf() / 100.0, c.zf() / 100.0) * 12.0 + 4.0;
    for _ in 0..times(l * c.s) {
        let p = c.surf();
        if p.1 < 90 {
            let g = if c.n(10) != 0 {
                G::PineTree(4, if c.n(4) == 0 { 1 } else { 0 })
            } else if c.n(3) != 0 {
                let a = 3 + c.n(6);
                G::SmallPine(a, 6 + c.n(8), 0)
            } else {
                G::SmallSpruce(c.n(2) + 1)
            };
            c.gen(g, p);
        } else if p.1 < 120 {
            let g = if c.n(4) != 0 {
                let a = 1 + c.n(3);
                let b = 3 + c.n(5);
                G::SmallPine(a, b, c.n(2) as u8)
            } else {
                G::SmallSpruce(c.n(2))
            };
            c.gen(g, p);
        }
    }
    if c.chance(4.0) {
        let p = c.surf();
        if p.1 < 100 {
            let l = 3 + c.n(4);
            c.gen(G::Log(1, l, false), p);
        }
    }
    for _ in 0..times(4.0 * c.s) {
        let p = c.surf();
        if p.1 < 110 {
            c.deco_shrub(p, 4, 1, 2, 2);
        }
    }
    if c.chance(150.0) {
        c.wheat();
    }
    if c.chance(20.0) {
        c.pumpkin(100);
    }
    for _ in 0..times(4.0 * c.s) {
        c.flowers(COLD_FLOWERS, 0, 128);
    }
    for _ in 0..times(12.0 * c.s) {
        c.tallgrass(0, 128);
    }
}

fn tundra_hills(c: &mut C) {
    c.lake(105);
    for _ in 0..times(c.s) {
        let p = c.surf();
        if p.1 < 95 {
            c.blob(COBBLE, p);
        }
    }
    if c.river > 0.75 {
        for _ in 0..times(6.0 * c.s) {
            let p = c.surf();
            c.tundra_pine(p);
        }
        for _ in 0..times(4.0 * c.s) {
            let p = c.surf();
            if p.1 < 110 {
                c.deco_shrub(p, 4, 1, 2, 2);
            }
        }
        if c.chance(120.0) {
            c.wheat();
        }
    } else {
        if c.chance(4.0) {
            let p = c.surf();
            if p.1 < 120 {
                c.tundra_pine(p);
            }
        }
        if c.chance(2.0) {
            let p = c.surf();
            if p.1 < 110 {
                c.deco_shrub(p, 4, 1, 2, 2);
            }
        }
    }
    if c.chance(20.0) {
        c.pumpkin(100);
    }
    for _ in 0..times(4.0 * c.s) {
        c.flowers(COLD_FLOWERS, 0, 128);
    }
    for _ in 0..times(8.0 * c.s) {
        c.tallgrass(0, 128);
    }
}

fn canyon(c: &mut C) {
    {
        let p = c.surf();
        if p.1 < 70 {
            c.blob(MOSSY, p);
        }
    }
    let n = if c.river > 0.8 { 10 } else { 5 };
    for _ in 0..n {
        let p = c.surf();
        c.gen(SHRUB, p);
    }
    if c.river > 0.8 {
        return;
    }
    if c.n(3) == 0 {
        c.reed();
    }
    if c.n(28) == 0 {
        c.pumpkin(128);
    }
    for _ in 0..5 {
        c.dead_bush(0, 160);
    }
    for _ in 0..25 {
        c.cacti(0, 160, i32::MAX, true);
    }
}

fn canyon_forest(c: &mut C) {
    for _ in 0..times(c.s) {
        let p = c.surf();
        if p.1 < 70 {
            c.blob(MOSSY, p);
        }
    }
    if c.river > 0.7 {
        for _ in 0..10 {
            let p = c.surf();
            c.gen(SHRUB, p);
        }
        if c.n(3) == 0 {
            c.reed();
        }
        for _ in 0..times(7.0 * c.s) {
            let p = c.surf();
            if p.1 < 80 {
                let g = if c.n(3) == 0 {
                    SHRUB
                } else {
                    G::Savannah(2, false)
                };
                c.gen(g, p);
            } else if p.1 < 111 {
                let g = if c.n(3) == 0 {
                    SHRUB
                } else {
                    G::Savannah(1, false)
                };
                c.gen(g, p);
            }
        }
    } else {
        for _ in 0..times(6.0 * c.s) {
            let p = c.surf();
            if p.1 > 127 {
                c.gen(SHRUB, p);
            } else if p.1 < 127 {
                let g = if c.n(3) != 0 {
                    SHRUB
                } else {
                    G::Savannah(1, false)
                };
                c.gen(g, p);
            }
        }
        for _ in 0..5 {
            c.dead_bush(64, 100);
        }
        for _ in 0..25 {
            c.cacti(64, 100, i32::MAX, true);
        }
    }
    for _ in 0..3 {
        c.flowers(CACTUS_FLOWERS, 0, 128);
    }
    for _ in 0..times(8.0 * c.s) {
        c.grass_mix(60, 70, 3);
    }
}

/// `HotForest`, `HotRedwood`
fn hot_forest(c: &mut C) {
    c.lake(i32::MAX);
    for _ in 0..times(3.0 * c.s) {
        let p = c.surf();
        if p.1 < 95 && (p.1 < 64 || c.n(7) == 0) {
            c.blob(COBBLE, p);
        }
    }
    let l = c.noise((c.xf() + 16.0) / 60.0, (c.zf() + 16.0) / 60.0) * 6.0 + 0.2;
    for _ in 0..times(l * 4.0 * c.s) {
        let p = c.surf();
        let g = if c.n(6) == 0 {
            OAK
        } else if c.n(12) == 0 {
            let a = 4 + c.n(5);
            G::Birch(a, 6 + c.n(5))
        } else if c.n(6) == 0 {
            let a = 3 + c.n(2);
            G::SmallPine(a, 3 + c.n(3), 0)
        } else {
            let a = 6 + c.n(5);
            G::SmallPine(a, 3 + c.n(6), 0)
        };
        c.gen(g, p);
    }
    if l < 0.5 {
        for _ in 0..times((l + 2.0) * 4.0 * c.s) {
            let p = c.scat(64, 64);
            c.gen(G::WaterGrass(db::TALLGRASS, 1, 65), p);
        }
    }
    if l > 0.0 && c.n(6) == 0 {
        let p = c.surf();
        let n = 3 + c.n(4);
        c.gen(G::Log(0, n, false), p);
    }
    for _ in 0..times(2.0 * c.s) {
        let p = c.surf();
        if c.n(10) == 0 {
            c.deco_shrub(p, 5, 4, 2, 0);
        } else {
            c.deco_shrub(p, 4, 1, 2, 0);
        }
    }
    if c.chance(3.0) {
        c.mushrooms();
    }
    if c.chance(20.0) {
        c.pumpkin(128);
    }
    for _ in 0..times(4.0 * c.s) {
        c.flowers(CACTUS_FLOWERS, 0, 128);
    }
    for _ in 0..times(10.0 * c.s) {
        c.tallgrass(0, 128);
    }
}

fn savanna(c: &mut C) {
    {
        let p = c.surf();
        c.blob(COBBLE, p);
    }
    if c.river > 0.8 {
        for _ in 0..times(15.0 * c.s) {
            let p = c.surf();
            let g = if c.n(3) != 0 {
                SHRUB
            } else if c.n(7) == 0 {
                sav(1)
            } else {
                sav(2)
            };
            c.gen(g, p);
        }
        for _ in 0..times(2.0 * c.s) {
            c.reed();
        }
    } else if c.noise(c.xf() / 180.0, c.zf() / 180.0) > 0.20 {
        for _ in 0..times(7.0 * c.s) {
            let p = c.surf();
            let g = if c.n(9) == 0 {
                SHRUB
            } else if c.n(7) == 0 {
                sav(1)
            } else {
                sav(2)
            };
            c.gen(g, p);
        }
    } else {
        let a = 3 - (c.noise(c.xf() / 100.0, c.zf() / 100.0) * 7.0) as i32;
        if a < 1 || c.n(a) == 0 {
            let p = c.surf();
            let g = if c.bool() {
                SHRUB
            } else if c.n(5) == 0 {
                sav(0)
            } else {
                sav(1)
            };
            c.gen(g, p);
        }
    }
    if c.chance(3.0) {
        c.reed();
    }
    if c.n(28) == 0 {
        c.pumpkin(128);
    }
    for _ in 0..3 {
        c.flowers(CACTUS_FLOWERS, 0, 128);
    }
    for _ in 0..12 {
        c.cacti(0, 160, i32::MAX, false);
    }
    for _ in 0..15 {
        c.grass_mix(0, 128, 6);
    }
}

fn savanna_forest(c: &mut C) {
    {
        let p = c.surf();
        c.blob(COBBLE, p);
    }
    let n = c.noise((c.xf() + 1000.0) / 150.0, c.zf() / 150.0);
    if c.river > 0.8 {
        for _ in 0..times(15.0 * c.s) {
            let p = c.surf();
            if p.1 < 100 || (p.1 < 120 && c.n(10) == 0) {
                let g = if c.n(3) != 0 {
                    SHRUB
                } else if c.n(10) == 0 {
                    sav(0)
                } else {
                    sav(2)
                };
                c.gen(g, p);
            }
        }
        for _ in 0..times(12.0 * c.s) {
            c.cacti(0, 160, 120, false);
        }
        for _ in 0..times(2.0 * c.s) {
            c.reed();
        }
    } else if n > -0.25 && n < 0.25 {
        for _ in 0..times(7.0 * c.s) {
            let p = c.surf();
            if p.1 < 100 || (p.1 < 120 && c.n(10) == 0) {
                let g = if c.n(9) == 0 {
                    SHRUB
                } else if c.n(7) == 0 {
                    sav(1)
                } else {
                    sav(2)
                };
                c.gen(g, p);
            }
        }
        for _ in 0..times(12.0 * c.s) {
            c.cacti(0, 160, 120, false);
        }
    } else {
        let a = 3 - (c.noise(c.xf() / 100.0, c.zf() / 100.0) * 7.0) as i32;
        if a < 1 || c.n(a) == 0 {
            let p = c.surf();
            if p.1 < 100 || (p.1 < 120 && c.n(10) == 0) {
                let g = if c.bool() {
                    SHRUB
                } else if c.n(5) == 0 {
                    sav(0)
                } else {
                    sav(1)
                };
                c.gen(g, p);
            }
        }
        if n > 0.25 {
            for _ in 0..times(13.0 * c.s) {
                let p = c.surf();
                if p.1 < 120 {
                    c.gen(G::Cacti(false, 0), p);
                }
            }
        }
    }
    if c.chance(3.0) {
        c.reed();
    }
    if c.n(28) == 0 {
        c.pumpkin(128);
    }
    for _ in 0..3 {
        c.flowers(CACTUS_FLOWERS, 0, 128);
    }
    for _ in 0..15 {
        c.grass_mix(0, 128, 6);
    }
}

/// `StoneMountains` (`cactus = false`), `StoneMountainsCactus`
fn stone_mountains(c: &mut C, cactus: bool) {
    {
        let p = c.surf();
        if p.1 < 80 {
            c.blob(COBBLE, p);
        }
    }
    if c.river > 0.7 {
        c.desert_wet(3);
        return;
    }
    if cactus {
        let a = 6 - (c.noise(c.xf() / 100.0, c.zf() / 100.0) * 10.0) as i32;
        if a < 1 || c.n(a) == 0 {
            let p = c.surf();
            if p.1 < 100 || (p.1 < 120 && c.n(10) == 0) {
                let g = if c.n(14) != 0 {
                    SHRUB
                } else {
                    G::Savannah(1, true)
                };
                c.gen(g, p);
            }
        }
        for _ in 0..70 {
            c.cacti(64, 64, 120, false);
        }
    } else {
        let a = 3 - (c.noise(c.xf() / 100.0, c.zf() / 100.0) * 10.0) as i32;
        if a < 1 || c.n(a) == 0 {
            let p = c.surf();
            if p.1 < 100 || (p.1 < 120 && c.n(10) == 0) {
                let g = if c.n(14) != 0 { SHRUB } else { sav(1) };
                c.gen(g, p);
            }
        }
        for _ in 0..10 {
            c.cacti(0, 160, 120, false);
        }
    }
    if c.chance(3.0) {
        c.reed();
    }
    if c.n(28) == 0 {
        c.pumpkin(128);
    }
    for _ in 0..3 {
        c.flowers(CACTUS_FLOWERS, 0, 128);
    }
    for _ in 0..15 {
        c.grass_mix(0, 128, 3);
    }
}

fn polar(c: &mut C) {
    if c.river > 0.86 {
        for _ in 0..times(5.0 * c.s) {
            let p = c.surf();
            if p.1 < 64 {
                c.blob(blk::PACKED_ICE as Db, p);
            }
        }
        if c.chance(2.0) {
            let p = c.surf();
            let s = c.n(2);
            c.gen(G::SmallSpruce(s), p);
        }
    }
}

fn redwood_jungle(c: &mut C) {
    for _ in 0..times(2.0 * c.s) {
        let p = c.surf();
        c.blob(MOSSY, p);
    }
    let l = c.noise(c.xf() / 80.0, c.zf() / 80.0) * 34.0 + 10.0;
    if c.chance(50.0) {
        let a = c.x + c.n(6) + 13;
        let b = c.z + c.n(6) + 13;
        let h = c.w.height(a, b);
        if h < 110 {
            let base = 20 + c.n(8);
            let root = 7 + c.n(2);
            c.gen(
                G::Jungle(
                    JungleKind::Fat,
                    branchy(3, 3, base, root, 23.0, 7, 0.22, 0.2),
                ),
                (a, h, b),
            );
        }
    }
    for _ in 0..times(l * c.s) {
        if c.n(3) == 0 {
            let a = c.x + c.n(12) + 10;
            let b = c.z + c.n(12) + 10;
            let h = c.w.height(a, b);
            let g = if c.n(24) == 0 {
                let base = 14 + c.n(8);
                let root = 5 + c.n(2);
                G::Jungle(
                    JungleKind::Fat,
                    branchy(3, 3, base, root, 16.0, 5, 0.32, 0.1),
                )
            } else {
                let base = 3 + c.n(7);
                let root = 3 + c.n(2);
                G::Jungle(
                    JungleKind::Tall,
                    branchy(3, 3, base, root, 9.0, 3, 0.32, 0.1),
                )
            };
            c.gen(g, (a, h, b));
        }
    }
    for _ in 0..times(3.0 * c.s) {
        let p = c.surf();
        let g = if c.n(10) == 0 {
            let base = 1 + c.n(4);
            G::Jungle(
                JungleKind::Small,
                branchy(3, 3, base, 0, 5.0, 2, 0.32, 0.14),
            )
        } else {
            SHRUB
        };
        c.gen(g, p);
    }
    for _ in 0..times(2.0 * c.s) {
        c.reed();
    }
    for _ in 0..13 {
        jungle_grass(c);
    }
}

/// `SnowHills` (Snow Forest, `off = +0.5`), `SnowRivers` (`off = -0.5`)
fn snow_hills(c: &mut C, off: f32) {
    c.lake(105);
    c.mossy_rocks(6.0, 15);
    if c.chance(25.0) {
        c.pumpkin(128);
    }
    for _ in 0..times(2.0 * c.s) {
        c.flowers(COLD_FLOWERS, 0, 128);
    }
    for _ in 0..times(3.0 * c.s) {
        c.tallgrass(0, 128);
    }
    let l = c.noise(c.xf() / 100.0, c.zf() / 100.0) * 5.0 + off;
    for _ in 0..times(l * 2.0 * c.s) {
        let p = c.surf();
        if p.1 < 75 {
            let g = if c.n(8) != 0 {
                G::SmallSpruce(1 + c.n(2))
            } else {
                let a = 1 + c.n(3);
                G::SmallPine(a, 2 + c.n(4), 1)
            };
            c.gen(g, p);
        } else if p.1 < 110 {
            let g = if c.n(4) != 0 {
                G::SmallSpruce(c.n(2))
            } else {
                let a = 2 + c.n(2);
                G::SmallPine(a, 4 + c.n(5), 1)
            };
            c.gen(g, p);
        }
    }
    if l > -0.4 {
        for _ in 0..times(2.0 * c.s) {
            let p = c.surf();
            if p.1 < 110 {
                if c.n(10) == 0 {
                    c.deco_shrub(p, 5, 4, 2, 2);
                } else {
                    c.deco_shrub(p, 4, 1, 2, 2);
                }
            }
        }
    }
}

fn taiga_plains(c: &mut C) {
    c.lake(i32::MAX);
    for _ in 0..times(3.0 * c.s) {
        let p = c.surf();
        if p.1 < 95 && (p.1 < 64 || c.n(7) == 0) {
            c.blob(MOSSY, p);
        }
    }
    let l = c.noise(c.xf() / 100.0, c.zf() / 100.0) * 6.0 + 0.8;
    for _ in 0..times(l * 4.0 * c.s) {
        let p = c.surf();
        let g = if c.n(4) == 0 {
            G::SmallSpruce(1 + c.n(2))
        } else if c.n(6) == 0 {
            let a = 1 + c.n(3);
            G::SmallPine(a, 4 + c.n(4), 1)
        } else {
            let a = 4 + c.n(6);
            G::SmallPine(a, 5 + c.n(10), 1)
        };
        c.gen(g, p);
    }
    if l > 0.0 && c.n(6) == 0 {
        let p = c.surf();
        let n = 3 + c.n(4);
        c.gen(G::Log(1, n, false), p);
    }
    for _ in 0..times(2.0 * c.s) {
        let p = c.surf();
        if c.n(10) == 0 {
            c.deco_shrub(p, 5, 4, 2, 2);
        } else {
            c.deco_shrub(p, 4, 1, 2, 2);
        }
    }
    if c.chance(3.0) {
        c.mushrooms();
    }
    if c.chance(20.0) {
        c.pumpkin(128);
    }
    if c.chance(150.0) {
        c.wheat();
    }
    for _ in 0..times(4.0 * c.s) {
        c.flowers(COLD_FLOWERS, 0, 128);
    }
    for _ in 0..times(10.0 * c.s) {
        c.tallgrass(0, 128);
    }
}

fn tundra_plains(c: &mut C) {
    c.lake(105);
    for _ in 0..times(3.0 * c.s) {
        let p = c.surf();
        if p.1 < 85 && (p.1 < 64 || c.n(5) == 0) {
            c.blob(COBBLE, p);
        }
    }
    if c.chance(2.0) {
        let p = c.surf();
        let g = if c.n(7) == 0 {
            G::SmallSpruce(c.n(2))
        } else {
            let a = 2 + c.n(3);
            let b = 4 + c.n(4);
            G::SmallPine(a, b, c.n(2) as u8)
        };
        c.gen(g, p);
    }
    if c.chance(1.0) {
        let p = c.surf();
        if p.1 < 90 {
            c.deco_shrub(p, 4, 1, 2, 2);
        }
    }
    if c.chance(25.0) {
        c.pumpkin(100);
    }
    for _ in 0..times(2.0 * c.s) {
        c.flowers(POPPY, 0, 128);
    }
    for _ in 0..times(4.0 * c.s) {
        c.tallgrass(0, 128);
    }
}

fn mesa(c: &mut C) {
    {
        let p = c.surf();
        if p.1 < 83 {
            c.blob(COBBLE, p);
        }
    }
    if c.river > 0.7 {
        c.desert_wet(6);
    } else {
        for _ in 0..3 {
            let p = c.surf();
            if p.1 < 90 {
                c.gen(SHRUB, p);
            }
        }
        for _ in 0..3 {
            c.dead_bush(60, 40);
        }
        for _ in 0..18 {
            let p = c.scat(60, 40);
            c.gen(G::CactusV, p);
        }
    }
}

fn dune_valley_forest(c: &mut C) {
    if c.chance(2.0) {
        let p = c.surf();
        if p.1 < 85 {
            c.blob(COBBLE, p);
        }
    }
    let t = (c.noise(c.xf() / 30.0, c.zf() / 30.0) * 8.0).min(4.0);
    if t > 0.0 && c.n(((10.0 - t) / c.s) as i32) == 0 {
        let p = c.surf();
        if p.1 < 82 {
            let base = 6 + c.n(4);
            let root = 3 + c.n(2);
            let len = 9.0 + c.f() * 4.0;
            c.gen(
                G::Jungle(
                    JungleKind::Tall,
                    branchy(2, 2, base, root, len, 3, 0.32, 0.1),
                ),
                p,
            );
        }
    }
    let (k, one_in) = if c.river > 0.8 {
        (8.0 * c.s, 2)
    } else {
        ((1.0 + t) * c.s, 6)
    };
    for _ in 0..times(k) {
        let p = c.surf();
        if p.1 < 82 {
            let g = if c.n(one_in) != 0 {
                SHRUB
            } else {
                G::Savannah(1, false)
            };
            c.gen(g, p);
        }
    }
    if c.chance(3.0) {
        c.reed();
    }
    if c.n(28) == 0 {
        c.pumpkin(128);
    }
    for _ in 0..3 {
        c.flowers(CACTUS_FLOWERS, 0, 128);
    }
    for _ in 0..times(26.0 * c.s) {
        c.cacti(66, 20, i32::MAX, false);
    }
    for _ in 0..times(8.0 * c.s) {
        c.grass_mix(60, 40, 3);
    }
}

fn mesa_plains(c: &mut C) {
    for _ in 0..times(c.s) {
        let p = c.surf();
        if p.1 < 83 {
            c.blob(COBBLE, p);
        }
    }
    if c.river > 0.7 {
        for _ in 0..times(6.0 * c.s) {
            let p = c.surf();
            if p.1 < 90 {
                let g = if c.n(3) != 0 {
                    SHRUB
                } else {
                    G::Savannah(1, false)
                };
                c.gen(g, p);
            }
        }
    } else if c.chance(2.0) {
        let p = c.surf();
        if p.1 < 90 {
            let g = if c.n(4) != 0 {
                SHRUB
            } else if c.n(12) == 0 {
                sav(0)
            } else {
                sav(1)
            };
            c.gen(g, p);
        }
    }
    for _ in 0..5 {
        c.dead_bush(64, 100);
    }
    for _ in 0..15 {
        c.cacti(64, 80, i32::MAX, false);
    }
    for _ in 0..3 {
        c.flowers(CACTUS_FLOWERS, 0, 128);
    }
    for _ in 0..times(8.0 * c.s) {
        c.tallgrass(60, 70);
    }
}

fn savanna_dunes(c: &mut C) {
    if c.chance(2.0) {
        let p = c.surf();
        if p.1 < 85 {
            c.blob(COBBLE, p);
        }
    }
    if c.river > 0.7 {
        c.desert_wet(6);
        return;
    }
    if c.chance(22.0) {
        c.pumpkin(128);
    }
    if c.chance(3.0) {
        c.reed();
    }
    for _ in 0..times(3.0 * c.s) {
        c.flowers(CACTUS_FLOWERS, 0, 128);
    }
    if c.chance(100.0) {
        c.wheat();
    }
    if c.chance(30.0) {
        let p = c.surf();
        if p.1 < 85 {
            c.gen(sav(1), p);
        }
    }
    for _ in 0..times(36.0 * c.s) {
        c.cacti(0, 160, 90, false);
    }
    for _ in 0..times(8.0 * c.s) {
        c.grass_mix(60, 40, 3);
    }
}

/// `RealisticBiomeMountainChain`: the backing biome's `rDecorate` at strength × gentle / 16 (`None` when
/// `getNoiseAt` is unavailable).
fn mountain_chain_strength(w: &DecoWorld, cx: i32, cz: i32, strength: f32) -> Option<f32> {
    let (x0, z0) = (cx * 16, cz * 16);
    let (mut n, mut gentle) = (0, 0);
    let at = |x: i32, z: i32| w.p.noise_at(x, z, w.h);
    for ox in (8..24).step_by(4) {
        for oz in (8..24).step_by(4) {
            let (x, z) = (x0 + ox, z0 + oz);
            let h = at(x, z)?;
            let d = (h - at(x - 4, z)?).abs().max((h - at(x + 4, z)?).abs());
            let e = (h - at(x, z - 4)?).abs().max((h - at(x, z + 4)?).abs());
            if d.max(e) <= 4.0 {
                gentle += 1;
            }
            n += 1;
        }
    }
    Some(strength * gentle as f32 / n as f32)
}

/// BOP support biomes whose `RealisticBiomeSupport` carries `EditRiverOasis`.
const RIVER_OASIS: &[&str] = &["BiomeGenBrushland", "BiomeGenHeathland", "BiomeGenOutback"];

/// `RealisticBiomeSupport.rDecorate`: the BOP biome's decorator above 0.3, then its edits.
fn support(
    w: &mut DecoWorld,
    r: &mut JavaRandom,
    base_class: &str,
    cx: i32,
    cz: i32,
    strength: f32,
    river: f32,
) -> bool {
    let suffix = base_class.rsplit('.').next().unwrap_or("");
    let decorated = base_class.starts_with("biomesoplenty.")
        || matches!(
            base_class,
            "net.minecraft.world.biome.BiomeGenPlains" | "net.minecraft.world.biome.BiomeGenSnow"
        );
    if strength > 0.3 && (!decorated || !super::bop::decorate(w, r, suffix, cx, cz)) {
        return false;
    }
    if RIVER_OASIS.contains(&suffix) {
        let mut c = C {
            w,
            r,
            x: cx * 16,
            z: cz * 16,
            s: strength,
            river,
        };
        if c.river > 0.7 {
            c.desert_wet(6);
        }
    }
    true
}

/// One border-loop entry: rwg biome `id` at `strength`. `false` = not ported (the Random is unknown from here on).
/// Oceans are deferred by the caller (`rDecorateAfterIce`).
pub fn decorate_biome(
    w: &mut DecoWorld,
    r: &mut JavaRandom,
    id: u16,
    cx: i32,
    cz: i32,
    strength: f32,
    river: f32,
) -> bool {
    let info = w.p.table.info(id as _);
    let name = info.class.rsplit('.').next().unwrap_or("");
    match name {
        "RealisticBiomeSupport" => {
            let bc = info.base_class.as_str();
            support(w, r, bc, cx, cz, strength, river)
        }
        "RealisticBiomeMountainChain" => {
            // the backing biome: the support biome with the same base biome
            let gen = info.gen_id;
            let Some(back) =
                w.p.table
                    .biomes
                    .iter()
                    .find(|b| b.gen_id == gen && b.class.ends_with("RealisticBiomeSupport"))
            else {
                return false;
            };
            let bc = back.base_class.as_str();
            match mountain_chain_strength(w, cx, cz, strength) {
                Some(s) => support(w, r, bc, cx, cz, s, river),
                None => false,
            }
        }
        _ => r_decorate(w, r, &info.class, cx, cz, strength, river),
    }
}

/// Runs one biome's `rDecorate` if it is ported; `false` = not ported (the Random is unknown from here on).
pub fn r_decorate(
    w: &mut DecoWorld,
    r: &mut JavaRandom,
    class: &str,
    cx: i32,
    cz: i32,
    strength: f32,
    river: f32,
) -> bool {
    let name = class.rsplit('.').next().unwrap_or(class);
    let Some(name) = name.strip_prefix("RealisticBiome") else {
        return false;
    };
    let mut c = C {
        w,
        r,
        x: cx * 16,
        z: cz * 16,
        s: strength,
        river,
    };
    let c = &mut c;
    match name {
        "CoastIce" | "CoastColdCliff" | "IslandTundra" | "IslandTropical" | "Ocean"
        | "OceanTest" | "Test" | "TestRiver" => {}
        "CoastColdSlope" => coast_cold_slope(c),
        "CoastOasis" | "CoastTest" => coast_oasis(c),
        "CoastDunes" => coast_dunes(c),
        "CoastMangrove" => coast_mangrove(c),
        "Desert" => desert(c),
        "DuneValley" => dune_valley(c),
        "DesertMountains" | "RedDesertMountains" => desert_mountains(c),
        "Oasis" => oasis(c, false),
        "RedOasis" => oasis(c, true),
        "DarkRedwood" => dark_redwood(c, false),
        "DarkRedwoodPlains" => dark_redwood(c, true),
        "Redwood" | "RedwoodSnow" => redwood(c),
        "WoodHills" | "WoodMountains" => wood_hills(c),
        "HighRainforest" => high_rainforest(c),
        "JungleCanyon" => jungle_canyon(c),
        "JungleHills" => jungle_hills(c),
        "RedwoodJungle" => redwood_jungle(c),
        "SnowLakes" => snow_lakes(c),
        "SnowHills" => snow_hills(c, 0.5),
        "SnowRivers" => snow_hills(c, -0.5),
        "Polar" => polar(c),
        "TaigaHills" => taiga_hills(c),
        "TaigaPlains" => taiga_plains(c),
        "TundraHills" => tundra_hills(c),
        "TundraPlains" => tundra_plains(c),
        "Canyon" => canyon(c),
        "CanyonForest" => canyon_forest(c),
        "HotForest" | "HotRedwood" => hot_forest(c),
        "Savanna" => savanna(c),
        "SavannaForest" => savanna_forest(c),
        "SavannaDunes" => savanna_dunes(c),
        "StoneMountains" => stone_mountains(c, false),
        "StoneMountainsCactus" => stone_mountains(c, true),
        "Mesa" => mesa(c),
        "MesaPlains" => mesa_plains(c),
        "DuneValleyForest" => dune_valley_forest(c),
        _ => return false,
    }
    true
}
