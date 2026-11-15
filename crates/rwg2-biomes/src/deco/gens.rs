//! Decoration generators: RWG2 `rwg2.deco.*` / `rwg2.deco.trees.*` and the vanilla 1.7.10 `WorldGen*` classes the
//! RWG2 decorators call. Every shared-Random draw is replayed in order; draws the Java code makes only after a
//! world test are made only when the same test passes here.

use super::*;
use crate::java_random::JavaRandom;
use crate::population::{mcos, msin};

/// A generator instance (the Java `WorldGenerator` with its constructor arguments).
#[derive(Clone, Copy, Debug)]
pub enum G {
    /// `DecoBlob(block, size)`
    Blob(Db, i32),
    /// vanilla `WorldGenShrub(logMeta, leavesMeta)`
    Shrub(u8, u8),
    /// `DecoShrub(size, log, leaves, sand)`
    DecoShrub(i32, i32, i32, bool),
    /// `DecoSavannah(type, sand)`
    Savannah(i32, bool),
    /// `DecoCacti(sandOnly, extraHeight)`
    Cacti(bool, i32),
    Reed,
    Pumpkin,
    DeadBush,
    /// `DecoFlowers(list)`
    Flowers(&'static [i32]),
    /// `DecoGrass(block, meta)` (block id `db::TALLGRASS` / `DOUBLE_PLANT` / `LEAVES`)
    Grass(u8, u8),
    /// `DecoWaterGrass(block, meta, minHeight)`
    WaterGrass(u8, u8, i32),
    /// `DecoSmallPine(start, size, meta)`
    SmallPine(i32, i32, u8),
    /// `DecoSmallSpruce(size)`
    SmallSpruce(i32),
    /// `DecoLargePine(start, size, logMeta, leavesMeta)`
    LargePine(i32, i32, u8, u8),
    /// `DecoPineTree(height, meta)`
    PineTree(i32, u8),
    /// `DecoBirch(start, size)`
    Birch(i32, i32),
    Palm,
    /// `DecoBigTree(size, type)`; size < 0: `new DecoBigTree()` (size from `Math.random()`, not reproducible)
    BigTree(i32, i32),
    /// `DecoJungleSmall / Tall / Fat / Mangrove`
    Jungle(JungleKind, Branchy),
    /// `DecoLog(meta, length, leaves)`
    Log(i32, i32, bool),
    /// `DecoWildWheat(type)`
    WildWheat(i32),
    /// vanilla `WorldGenTrees(false, minHeight, metaWood, metaLeaves, vines)`
    Trees(i32, u8, u8, bool),
    /// vanilla `WorldGenForest(false, tall)`
    Forest(bool),
    /// vanilla `WorldGenMegaJungle(false, baseHeight, randHeight, wood, leaves)`
    MegaJungle(i32, i32, u8, u8),
    /// BOP `WorldGenBOPJungle(false, baseHeight, randHeight, wood, leaves)`: huge-tree crown, branches walking up
    /// from y + 2 in `nextInt(4)` steps, ivy instead of vines
    BopJungle(i32, i32, u8, u8),
    /// vanilla `WorldGenLakes(water / lava)`
    Lakes(bool),
    /// vanilla `WorldGenFlowers(block, meta)`
    FlowersV(u8, u8),
    Vines,
    /// vanilla `WorldGenCactus`
    CactusV,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum JungleKind {
    Small,
    Tall,
    Fat,
    Mangrove,
}

/// `DecoJungle*` / `DecoMangrove` constructor arguments.
#[derive(Clone, Copy, Debug)]
pub struct Branchy {
    pub log: Db,
    pub leaves: Db,
    pub base: i32,
    pub root: i32,
    pub length: f32,
    pub branch: i32,
    pub ver_start: f32,
    pub ver_rand: f32,
}

impl G {
    pub fn generate(self, w: &mut DecoWorld, r: &mut impl DRng, x: i32, y: i32, z: i32) {
        match self {
            G::Blob(b, s) => blob(w, r, b, s, x, y, z),
            G::Shrub(l, v) => shrub(w, r, l, v, x, y, z),
            G::DecoShrub(s, l, v, sand) => deco_shrub(w, r, s, l, v, sand, x, y, z),
            G::Savannah(t, s) => savannah(w, r, t, s, x, y, z),
            G::Cacti(s, e) => cacti(w, r, s, e, x, y, z),
            G::Reed => reed(w, r, x, y, z),
            G::Pumpkin => pumpkin(w, r, x, y, z),
            G::DeadBush => dead_bush(w, r, x, y, z),
            G::Flowers(l) => flowers(w, r, l, x, y, z),
            G::Grass(b, m) => deco_grass(w, r, b, m, x, y, z),
            G::WaterGrass(b, m, mh) => water_grass(w, r, b, m, mh, x, y, z),
            G::SmallPine(s, t, m) => small_pine(w, r, s, t, m, x, y, z),
            G::SmallSpruce(s) => small_spruce(w, r, s, x, y, z),
            G::LargePine(s, t, l, v) => large_pine(w, r, s, t, l, v, x, y, z),
            G::PineTree(h, m) => pine_tree(w, r, h, m, x, y, z),
            G::Birch(s, t) => birch(w, r, s, t, x, y, z),
            G::Palm => palm(w, r, x, y, z),
            G::BigTree(s, t) => big_tree(w, r, s, t, x, y, z),
            G::Jungle(k, b) => jungle(w, r, k, b, x, y, z),
            G::Log(m, l, v) => deco_log(w, r, m, l, v, x, y, z),
            G::WildWheat(t) => wild_wheat(w, r, t, x, y, z),
            G::Trees(h, wo, le, v) => trees(w, r, h, wo, le, v, x, y, z),
            G::Forest(t) => forest(w, r, t, x, y, z),
            G::MegaJungle(b, h, wo, le) => mega_jungle(w, r, b, h, wo, le, x, y, z, false),
            G::BopJungle(b, h, wo, le) => mega_jungle(w, r, b, h, wo, le, x, y, z, true),
            G::Lakes(water) => lakes(w, r, water, x, y, z),
            G::FlowersV(b, m) => flowers_v(w, r, b, m, x, y, z),
            G::Vines => vines(w, r, x, y, z),
            G::CactusV => cactus_v(w, r, x, y, z),
        }
    }
}

fn log_of(block: u8, meta: u8) -> Db {
    db::mk(block, meta)
}

// --- small generators ------------------------------------------------------------------------------------------

/// `rwg2.deco.DecoBlob` (cobblestone / mossy boulders; `enableCobblestoneBoulders` on).
fn blob(
    w: &mut DecoWorld,
    r: &mut impl DRng,
    block: Db,
    size: i32,
    mut x: i32,
    mut y: i32,
    mut z: i32,
) {
    w.mark("blob", x, y, z);
    while y > 3 {
        if !w.air(x, y - 1, z) {
            let b = w.get(x, y - 1, z);
            if is_earth(b) || is_sand(b) || matches!(db::id(b), blk::STONE | blk::GRAVEL) {
                break;
            }
        }
        y -= 1;
    }
    if y <= 3 {
        return;
    }
    if matches!(db::id(block), blk::COBBLE | db::MOSSY) && y <= 63 {
        return;
    }
    let k2 = size;
    let mut l = 0;
    while k2 >= 0 && l < 3 {
        let i1 = k2 + r.next_int_bounded(2);
        let j1 = k2 + r.next_int_bounded(2);
        let k1 = k2 + r.next_int_bounded(2);
        let f = (i1 + j1 + k1) as f32 * 0.333 + 0.5;
        for l1 in x - i1..=x + i1 {
            for i2 in z - k1..=z + k1 {
                for j2 in y - j1..=y + j1 {
                    let (f1, f2, f3) = ((l1 - x) as f32, (i2 - z) as f32, (j2 - y) as f32);
                    if f1 * f1 + f2 * f2 + f3 * f3 <= f * f {
                        w.set(l1, j2, i2, block);
                    }
                }
            }
        }
        x += -(k2 + 1) + r.next_int_bounded(2 + k2 * 2);
        z += -(k2 + 1) + r.next_int_bounded(2 + k2 * 2);
        y -= r.next_int_bounded(2);
        l += 1;
    }
}

/// vanilla `WorldGenShrub(logMeta, leavesMeta)`
fn shrub(
    w: &mut DecoWorld,
    r: &mut impl DRng,
    log_meta: u8,
    leaves_meta: u8,
    x: i32,
    mut y: i32,
    z: i32,
) {
    w.mark("shrub", x, y, z);
    loop {
        let b = w.get(x, y, z);
        if matches!(mat(b), Mat::Air | Mat::Leaves) && y > 0 {
            y -= 1;
        } else {
            break;
        }
    }
    // Forge: `canSustainPlant(..., sapling)` (grass, dirt, farmland, BOP grasses)
    if !sustains_sapling(w.get(x, y, z)) {
        return;
    }
    y += 1;
    w.set(x, y, z, db::mk(db::LOG, log_meta));
    for k in y..=y + 2 {
        let m = 2 - (k - y);
        for xx in x - m..=x + m {
            let dx = xx - x;
            for zz in z - m..=z + m {
                let dz = zz - z;
                if (dx.abs() != m || dz.abs() != m || r.next_int_bounded(2) != 0)
                    && !opaque(w.get(xx, k, zz))
                {
                    w.set(xx, k, zz, db::mk(db::LEAVES, leaves_meta));
                }
            }
        }
    }
}

/// `rwg2.deco.trees.DecoShrub(size, log, leaves, sand)`
#[allow(clippy::too_many_arguments)]
fn deco_shrub(
    w: &mut DecoWorld,
    r: &mut impl DRng,
    size: i32,
    log: i32,
    leav: i32,
    sand: bool,
    x: i32,
    y: i32,
    z: i32,
) {
    w.mark("decoshrub", x, y, z);
    let logb = if log < 3 {
        log_of(db::LOG, log as u8)
    } else {
        log_of(db::LOG2, (log - 3) as u8)
    };
    let leafb = if leav < 4 {
        log_of(db::LEAVES, leav as u8)
    } else {
        log_of(db::LEAVES2, (leav - 4) as u8)
    };
    let width = size.min(6);
    let height = if size > 3 { 2 } else { 1 };
    for i in 0..size {
        let rx = r.next_int_bounded(width * 2) - width;
        let ry = r.next_int_bounded(height);
        let rz = r.next_int_bounded(width * 2) - width;
        let (bx, by, bz, s) = if i == 0 && size > 4 {
            (x + rx, y, z + rz, 3)
        } else if i == 1 && size > 2 {
            (x + rx, y, z + rz, 2)
        } else {
            (x + rx, y + ry, z + rz, 1)
        };
        let g = w.get(bx, by - 2, bz);
        let m = mat(g);
        if (m == Mat::Grass || m == Mat::Ground || (sand && m == Mat::Sand))
            && !is_water(w.get(bx, by - 1, bz))
        {
            for a in -s..=s {
                for j in -1i32..=1 {
                    for k in -s..=s {
                        if i32::abs(a) + i32::abs(j) + i32::abs(k) <= s {
                            let t = w.get(bx + a, by + j, bz + k);
                            if matches!(mat(t), Mat::Air | Mat::Vine | Mat::Plants)
                                || is(t, blk::SNOW_LAYER)
                            {
                                w.set(bx + a, by + j, bz + k, leafb);
                            }
                        }
                    }
                }
            }
            w.set(bx, by - 1, bz, logb);
        }
    }
}

/// vanilla `WorldGenReed`
fn reed(w: &mut DecoWorld, r: &mut impl DRng, x: i32, y: i32, z: i32) {
    w.mark("reed", x, y, z);
    for _ in 0..20 {
        let a = x + r.next_int_bounded(4) - r.next_int_bounded(4);
        let c = z + r.next_int_bounded(4) - r.next_int_bounded(4);
        if w.air(a, y, c)
            && [(-1, 0), (1, 0), (0, -1), (0, 1)]
                .iter()
                .any(|&(dx, dz)| mat(w.get(a + dx, y - 1, c + dz)) == Mat::Water)
        {
            let b = r.next_int_bounded(3) + 1;
            let n = 2 + r.next_int_bounded(b);
            for k in 0..n {
                if w.reed_stay(a, y + k, c) {
                    w.set(a, y + k, c, db::mk(db::REEDS, 0));
                }
            }
        }
    }
}

/// vanilla `WorldGenPumpkin`
fn pumpkin(w: &mut DecoWorld, r: &mut impl DRng, x: i32, y: i32, z: i32) {
    w.mark("pumpkin", x, y, z);
    for _ in 0..64 {
        let a = x + r.next_int_bounded(8) - r.next_int_bounded(8);
        let b = y + r.next_int_bounded(4) - r.next_int_bounded(4);
        let c = z + r.next_int_bounded(8) - r.next_int_bounded(8);
        if w.air(a, b, c) && is(w.get(a, b - 1, c), blk::GRASS) {
            let m = r.next_int_bounded(4) as u8;
            w.set(a, b, c, db::mk(db::PUMPKIN, m));
        }
    }
}

/// vanilla `WorldGenDeadBush`
fn dead_bush(w: &mut DecoWorld, r: &mut impl DRng, x: i32, mut y: i32, z: i32) {
    w.mark("deadbush", x, y, z);
    loop {
        let b = w.get(x, y, z);
        if matches!(mat(b), Mat::Air | Mat::Leaves) && y > 0 {
            y -= 1;
        } else {
            break;
        }
    }
    for _ in 0..4 {
        let a = x + r.next_int_bounded(8) - r.next_int_bounded(8);
        let b = y + r.next_int_bounded(4) - r.next_int_bounded(4);
        let c = z + r.next_int_bounded(8) - r.next_int_bounded(8);
        if w.air(a, b, c) && w.deadbush_stay(a, b, c) {
            w.set(a, b, c, db::mk(db::DEADBUSH, 0));
        }
    }
}

/// `rwg2.deco.DecoCacti(sandOnly, extraHeight)`
fn cacti(
    w: &mut DecoWorld,
    r: &mut impl DRng,
    sand_only: bool,
    extra: i32,
    x: i32,
    y: i32,
    z: i32,
) {
    w.mark("cacti", x, y, z);
    for _ in 0..10 {
        let a = x + r.next_int_bounded(8) - r.next_int_bounded(8);
        let b = y + r.next_int_bounded(4) - r.next_int_bounded(4);
        let c = z + r.next_int_bounded(8) - r.next_int_bounded(8);
        if w.air(a, b, c) {
            let g = w.get(a, b - 1, c);
            let earth = is_earth(g);
            if is_sand(g) || (!sand_only && earth) {
                let t = r.next_int_bounded(3) + 1;
                let l1 = 1 + r.next_int_bounded(t);
                if earth {
                    w.set(a, b - 1, c, blk::SAND as Db);
                }
                for i in 0..l1 + extra {
                    if w.cactus_stay(a, b + i, c) {
                        w.set(a, b + i, c, db::mk(db::CACTUS, 0));
                    }
                }
            }
        }
    }
}

/// vanilla `WorldGenCactus`
fn cactus_v(w: &mut DecoWorld, r: &mut impl DRng, x: i32, y: i32, z: i32) {
    w.mark("cactusv", x, y, z);
    for _ in 0..10 {
        let a = x + r.next_int_bounded(8) - r.next_int_bounded(8);
        let b = y + r.next_int_bounded(4) - r.next_int_bounded(4);
        let c = z + r.next_int_bounded(8) - r.next_int_bounded(8);
        if w.air(a, b, c) {
            let t = r.next_int_bounded(3) + 1;
            let n = 1 + r.next_int_bounded(t);
            for i in 0..n {
                if w.cactus_stay(a, b + i, c) {
                    w.set(a, b + i, c, db::mk(db::CACTUS, 0));
                }
            }
        }
    }
}

/// `BlockDoublePlant.func_149889_c`: lower half (meta) + upper half (8).
fn place_double(w: &mut DecoWorld, x: i32, y: i32, z: i32, meta: u8) {
    w.set(x, y, z, db::mk(db::DOUBLE_PLANT, meta));
    w.set(x, y + 1, z, db::mk(db::DOUBLE_PLANT, 8));
}

/// `rwg2.deco.DecoFlowers(int[] list)`
fn flowers(w: &mut DecoWorld, r: &mut impl DRng, list: &[i32], x: i32, y: i32, z: i32) {
    w.mark("flowers", x, y, z);
    let f = list[r.next_int_bounded(list.len() as i32) as usize];
    for _ in 0..64 {
        let a = x + r.next_int_bounded(8) - r.next_int_bounded(8);
        let b = y + r.next_int_bounded(4) - r.next_int_bounded(4);
        let c = z + r.next_int_bounded(8) - r.next_int_bounded(8);
        if f > 9 {
            if w.air(a, b, c) && w.double_place(a, b, c) {
                place_double(w, a, b, c, (f - 10) as u8);
            }
        } else if w.air(a, b, c) && w.bush_stay(a, b, c) {
            if f == 9 {
                w.set(a, b, c, db::mk(db::YELLOW_FLOWER, 0));
            } else {
                w.set(a, b, c, db::mk(db::RED_FLOWER, f as u8));
            }
        }
    }
}

/// vanilla `WorldGenFlowers(block, meta)` (flowers, mushrooms)
fn flowers_v(w: &mut DecoWorld, r: &mut impl DRng, block: u8, meta: u8, x: i32, y: i32, z: i32) {
    w.mark("flowersv", x, y, z);
    for _ in 0..64 {
        let a = x + r.next_int_bounded(8) - r.next_int_bounded(8);
        let b = y + r.next_int_bounded(4) - r.next_int_bounded(4);
        let c = z + r.next_int_bounded(8) - r.next_int_bounded(8);
        if !w.air(a, b, c) {
            continue;
        }
        let ok = match block {
            db::BROWN_MUSHROOM | db::RED_MUSHROOM => w.mushroom_stay(a, b, c),
            _ => w.bush_stay(a, b, c),
        };
        if ok {
            w.set(a, b, c, db::mk(block, meta));
        }
    }
}

/// `rwg2.deco.DecoGrass(block, meta)` for tallgrass / double_plant / leaves.
fn deco_grass(
    w: &mut DecoWorld,
    r: &mut impl DRng,
    block: u8,
    meta: u8,
    x: i32,
    mut y: i32,
    z: i32,
) {
    w.mark("grass", x, y, z);
    while y > 0 {
        let b = w.get(x, y, z);
        if !is_air(b) || is_leaves(b) {
            break;
        }
        y -= 1;
    }
    let n = if block == db::TALLGRASS { 128 } else { 64 };
    for _ in 0..n {
        let a = x + r.next_int_bounded(8) - r.next_int_bounded(8);
        let b = y + r.next_int_bounded(4) - r.next_int_bounded(4);
        let c = z + r.next_int_bounded(8) - r.next_int_bounded(8);
        match block {
            db::DOUBLE_PLANT => {
                if w.air(a, b, c) && b < 254 && w.double_place(a, b, c) {
                    place_double(w, a, b, c, meta);
                }
            }
            db::LEAVES => {
                if w.air(a, b, c) && is(w.get(a, b - 1, c), blk::GRASS) {
                    w.set(a, b, c, db::mk(db::LEAVES, meta));
                }
            }
            _ => {
                if w.air(a, b, c) && w.bush_stay(a, b, c) {
                    w.set(a, b, c, db::mk(block, meta));
                }
            }
        }
    }
}

/// `rwg2.deco.DecoWaterGrass(block, meta, minHeight)`
#[allow(clippy::too_many_arguments)]
fn water_grass(
    w: &mut DecoWorld,
    r: &mut impl DRng,
    block: u8,
    meta: u8,
    min_h: i32,
    x: i32,
    mut y: i32,
    z: i32,
) {
    w.mark("watergrass", x, y, z);
    while y > 0 {
        let b = w.get(x, y, z);
        if !is_air(b) || is_leaves(b) {
            break;
        }
        if y < min_h {
            return;
        }
        y -= 1;
    }
    let (n, dy) = match block {
        db::DOUBLE_PLANT => (32, 2),
        db::LEAVES => (64, 4),
        _ => (128, 4),
    };
    for _ in 0..n {
        let a = x + r.next_int_bounded(8) - r.next_int_bounded(8);
        let b = y + r.next_int_bounded(dy) - r.next_int_bounded(dy);
        let c = z + r.next_int_bounded(8) - r.next_int_bounded(8);
        let g = w.get(a, b - 1, c);
        if ((is_water(g) && is_sand(w.get(a, b - 2, c))) || is_sand(g)) && w.air(a, b, c) {
            w.set(a, b - 1, c, blk::GRASS as Db);
        }
        match block {
            db::DOUBLE_PLANT => {
                if w.air(a, b, c) && b < 254 && w.double_place(a, b, c) {
                    place_double(w, a, b, c, meta);
                }
            }
            db::LEAVES => {
                if w.air(a, b, c) && is(w.get(a, b - 1, c), blk::GRASS) {
                    w.set(a, b, c, db::mk(db::LEAVES, meta));
                }
            }
            _ => {
                if w.air(a, b, c) && w.bush_stay(a, b, c) {
                    w.set(a, b, c, db::mk(block, meta));
                }
            }
        }
    }
}

/// `rwg2.deco.DecoWildWheat(type)`
fn wild_wheat(w: &mut DecoWorld, r: &mut impl DRng, ty: i32, x: i32, mut y: i32, z: i32) {
    w.mark("wheat", x, y, z);
    let crop = match ty {
        0 => db::POTATOES,
        1 => db::CARROTS,
        _ => db::WHEAT,
    };
    while y > 0 {
        let b = w.get(x, y, z);
        if !is_air(b) || is_leaves(b) {
            break;
        }
        y -= 1;
    }
    if !is_earth(w.get(x, y, z)) {
        return;
    }
    for (dx, dz) in [(-1, 0), (1, 0), (0, -1), (0, 1)] {
        if !matches!(mat(w.get(x + dx, y, z + dz)), Mat::Ground | Mat::Grass) {
            return;
        }
    }
    for _ in 0..30 {
        let rx = r.next_int_bounded(5) - 2;
        let ry = r.next_int_bounded(2) - 1;
        let rz = r.next_int_bounded(5) - 2;
        let b = w.get(x + rx, y + ry, z + rz);
        if is_earth(b) && w.air(x + rx, y + ry + 1, z + rz) {
            let m1 = r.next_int_bounded(4) as u8 + 4;
            w.set(x + rx, y + ry, z + rz, db::mk(db::FARMLAND, m1));
            let m2 = r.next_int_bounded(4) as u8 + 4;
            w.set(x + rx, y + ry + 1, z + rz, db::mk(crop, m2));
        }
    }
    w.set(x, y, z, blk::WATER as Db);
}

/// `rwg2.deco.DecoLog(meta, length, leaves)`
#[allow(clippy::too_many_arguments)]
fn deco_log(
    w: &mut DecoWorld,
    r: &mut impl DRng,
    meta: i32,
    length: i32,
    leaves: bool,
    mut x: i32,
    y: i32,
    mut z: i32,
) {
    w.mark("log", x, y, z);
    let (logb, leafb, meta) = if meta > 2 {
        (db::LOG2, db::LEAVES2, meta - 2)
    } else {
        (db::LOG, db::LEAVES, meta)
    };
    let leaves_meta = if leaves { meta } else { -1 };
    let len = length.max(2);
    let g = mat(w.get(x, y - 1, z));
    if !matches!(g, Mat::Ground | Mat::Grass | Mat::Sand | Mat::Rock) {
        return;
    }
    let dir = r.next_int_bounded(2);
    let dir_meta = (4 + dir * 4 + meta) as u8;
    let (ddx, ddz) = if dir == 0 { (1, 0) } else { (0, 1) };
    let open = |b: Db| matches!(mat(b), Mat::Air | Mat::Vine | Mat::Plants);
    let air_check = |w: &mut DecoWorld, x: i32, z: i32| -> i32 {
        let a = |b: Db| matches!(mat(b), Mat::Air | Mat::Vine | Mat::Water | Mat::Plants);
        if a(w.get(x, y - 1, z)) {
            if a(w.get(x, y - 2, z)) {
                return 99;
            }
            return 1;
        }
        0
    };
    for _ in 0..len {
        if !open(w.get(x - ddx, y, z - ddz)) {
            break;
        }
        x -= ddx;
        z -= ddz;
        if air_check(w, x, z) > 0 {
            return;
        }
    }
    let mut air = 0;
    for _ in 0..len * 2 {
        if !open(w.get(x + ddx, y, z + ddz)) {
            break;
        }
        air += air_check(w, x, z);
        if air > 2 {
            return;
        }
        w.set(x, y, z, db::mk(logb, dir_meta));
        if leaves_meta > -1 {
            let lm = db::mk(leafb, leaves_meta as u8);
            let sides: [(i32, i32); 2] = if dir == 0 {
                [(0, -1), (0, 1)]
            } else {
                [(-1, 0), (1, 0)]
            };
            for (sx, sz) in sides {
                if open(w.get(x + sx, y, z + sz)) && r.next_int_bounded(3) == 0 {
                    w.set(x + sx, y, z + sz, lm);
                }
            }
            if open(w.get(x, y + 1, z)) && r.next_int_bounded(3) == 0 {
                w.set(x, y + 1, z, lm);
            }
        }
        x += ddx;
        z += ddz;
    }
}

// --- RWG2 trees --------------------------------------------------------------------------------------------------

fn cos_sin_deg_f(dir: f64) -> (f32, f32) {
    let a = dir * std::f64::consts::PI / 180.0;
    (a.cos() as f32, a.sin() as f32)
}

/// `rwg2.deco.trees.DecoSavannah(type, sand)`
fn savannah(w: &mut DecoWorld, r: &mut impl DRng, ty: i32, sand: bool, x: i32, y: i32, z: i32) {
    w.mark("savannah", x, y, z);
    let b = w.get(x, y - 1, z);
    if !(is_earth(b) || (sand && is_sand(b))) {
        return;
    }
    let log2 = db::mk(db::LOG2, 0);
    let (h, bh, branches_base, branches_rand, sh_rand) = match ty {
        0 => {
            let h = 10 + r.next_int_bounded(5);
            (h, h - 6, 7 - 1, 3, 6 - 2)
        }
        1 => {
            let h = 6 + r.next_int_bounded(3);
            (h, h - 3, 1, 2, 3 - 1)
        }
        _ => {
            let h = 12 + r.next_int_bounded(5);
            (h, h - 3, 1, 2, 3 - 1)
        }
    };
    for i in 0..h {
        w.set(x, y + i, z, log2);
    }
    savannah_leaves(w, ty, x, y + h, z);
    let mut a = branches_base + r.next_int_bounded(branches_rand);
    while a > -1 {
        let mut sh = bh + r.next_int_bounded(sh_rand);
        let dir = r.next_int_bounded(360);
        let (cx, sx) = cos_sin_deg_f(dir as f64);
        let (xd, yd) = (cx * 2.0, sx * 2.0);
        let mut c = 1f32;
        while sh < h {
            w.set(x + (xd * c) as i32, y + sh, z + (yd * c) as i32, log2);
            sh += 1;
            c += 0.5;
        }
        savannah_leaves(w, ty, x + (xd * c) as i32, y + sh, z + (yd * c) as i32);
        a -= 1;
    }
}

fn savannah_leaves(w: &mut DecoWorld, ty: i32, x: i32, y: i32, z: i32) {
    let leaves2 = db::mk(db::LEAVES2, 0);
    let (r1, l1, r0, l0) = if ty == 0 { (2, 4, 3, 5) } else { (1, 99, 2, 4) };
    for i in -r1..=r1 {
        for j in -r1..=r1 {
            if w.air(x + i, y + 1, z + j) && (i32::abs(i) + i32::abs(j)) < l1 {
                w.set(x + i, y + 1, z + j, leaves2);
            }
        }
    }
    for i in -r0..=r0 {
        for j in -r0..=r0 {
            if w.air(x + i, y, z + j) && (i32::abs(i) + i32::abs(j)) < l0 {
                w.set(x + i, y, z + j, leaves2);
            }
        }
    }
    w.set(x, y, z, db::mk(db::LOG2, 0));
}

/// `buildLeaves` of the pine-style trees: leaves only into air.
fn leaves_into_air(w: &mut DecoWorld, x: i32, y: i32, z: i32, leaves: Db) {
    if w.air(x, y, z) {
        w.set(x, y, z, leaves);
    }
}

/// `buildBranch` of `DecoSmallPine` / `DecoBirch` / `DecoLargePine`.
#[allow(clippy::too_many_arguments)]
fn pine_branch(
    w: &mut DecoWorld,
    x: i32,
    y: i32,
    z: i32,
    dx: i32,
    dz: i32,
    log_len: i32,
    leave_size: i32,
    log: Db,
    leaves: Db,
) {
    for i in -1..=1 {
        for j in -1..=1 {
            for k in 0..2 {
                if i32::abs(i) + i32::abs(j) + k < leave_size + 1 {
                    leaves_into_air(w, x + i + dx * log_len, y + k, z + j + dz * log_len, leaves);
                }
            }
        }
    }
    for m in 1..=log_len {
        w.set(x + dx * m, y, z + dz * m, log);
    }
}

/// shared branch direction draw: `dX, dZ` in −1..1, redrawn once if both 0, flipped against the previous one
fn pine_dir(r: &mut impl DRng, px: &mut i32, pz: &mut i32) -> (i32, i32) {
    let mut dx = -1 + r.next_int_bounded(3);
    let mut dz = -1 + r.next_int_bounded(3);
    if dx == 0 && dz == 0 {
        dx = -1 + r.next_int_bounded(3);
        dz = -1 + r.next_int_bounded(3);
    }
    if *px == dx && r.next_boolean() {
        dx = -dx;
    }
    if *pz == dz && r.next_boolean() {
        dz = -dz;
    }
    *px = dx;
    *pz = dz;
    (dx, dz)
}

/// `rwg2.deco.trees.DecoSmallPine(start, size, meta)`
#[allow(clippy::too_many_arguments)]
fn small_pine(
    w: &mut DecoWorld,
    r: &mut impl DRng,
    start: i32,
    size: i32,
    meta: u8,
    x: i32,
    mut y: i32,
    z: i32,
) {
    w.mark("smallpine", x, y, z);
    if !is_earth(w.get(x, y - 1, z)) {
        return;
    }
    let (log, leaves) = (db::mk(db::LOG, meta), db::mk(db::LEAVES, meta));
    for _ in 0..start {
        w.set(x, y, z, log);
        y += 1;
    }
    let (mut px, mut pz) = (0, 0);
    for i in 0..size {
        // rand.nextInt(1) == 0 (one draw, always true)
        let _ = r.next_int_bounded(1);
        if i < size - 2 {
            let (dx, dz) = pine_dir(r, &mut px, &mut pz);
            let ll = if i < size - 10 { 2 } else { 1 };
            let ls = if i < size - 6 { 2 } else { 1 };
            pine_branch(w, x, y, z, dx, dz, ll, ls, log, leaves);
        }
        w.set(x, y, z, log);
        if i < size - 2 {
            for (ox, oz) in [(0, 1), (0, -1), (1, 0), (-1, 0)] {
                if r.next_boolean() {
                    leaves_into_air(w, x + ox, y, z + oz, leaves);
                }
            }
        }
        y += 1;
    }
    for (ox, oy, oz) in [(0, -1, 1), (0, -1, -1), (1, -1, 0), (-1, -1, 0), (0, 0, 0)] {
        leaves_into_air(w, x + ox, y + oy, z + oz, leaves);
    }
}

/// `rwg2.deco.trees.DecoBirch(start, size)`
fn birch(w: &mut DecoWorld, r: &mut impl DRng, start: i32, size: i32, x: i32, mut y: i32, z: i32) {
    w.mark("birch", x, y, z);
    if !is_earth(w.get(x, y - 1, z)) {
        return;
    }
    let (log, leaves) = (db::mk(db::LOG, 2), db::mk(db::LEAVES, 2));
    for _ in 0..start {
        w.set(x, y, z, log);
        y += 1;
    }
    let (mut px, mut pz) = (0, 0);
    for i in 0..size {
        let _ = r.next_int_bounded(1);
        if i < size - 2 {
            let (dx, dz) = pine_dir(r, &mut px, &mut pz);
            pine_branch(
                w,
                x,
                y,
                z,
                dx,
                dz,
                1,
                if i < size - 2 { 2 } else { 1 },
                log,
                leaves,
            );
        }
        w.set(x, y, z, log);
        if i < size - 2 {
            for (ox, oz) in [(0, 1), (0, -1), (1, 0), (-1, 0)] {
                if r.next_boolean() {
                    leaves_into_air(w, x + ox, y, z + oz, leaves);
                }
            }
        }
        y += 1;
    }
    for (ox, oy, oz) in [(0, -1, 1), (0, -1, -1), (1, -1, 0), (-1, -1, 0), (0, 0, 0)] {
        leaves_into_air(w, x + ox, y + oy, z + oz, leaves);
    }
}

/// `rwg2.deco.trees.DecoLargePine(start, size, logMeta, leavesMeta)`
#[allow(clippy::too_many_arguments)]
fn large_pine<R: DRng>(
    w: &mut DecoWorld,
    r: &mut R,
    start: i32,
    size: i32,
    log_meta: u8,
    leaves_meta: u8,
    x: i32,
    mut y: i32,
    z: i32,
) {
    w.mark("largepine", x, y, z);
    if !is_earth(w.get(x, y - 1, z)) {
        return;
    }
    let (log, leaves) = (db::mk(db::LOG, log_meta), db::mk(db::LEAVES, leaves_meta));
    let trunk = |w: &mut DecoWorld, r: &mut R, tx: i32, tz: i32| {
        let h0 = (start as f32 / 4.0).ceil() as i32;
        let h = h0 + r.next_int_bounded(h0 * 2);
        for i in -1..h {
            w.set(tx, y + i, tz, db::mk(db::LOG, log_meta + 12));
        }
    };
    trunk(w, r, x + 1, z);
    trunk(w, r, x - 1, z);
    trunk(w, r, x, z + 1);
    trunk(w, r, x, z - 1);
    let branch = |w: &mut DecoWorld, y: i32, dx: i32, dz: i32, mut ll: i32, ls: i32| {
        if ll == 3 && dx.abs() + dz.abs() == 2 {
            ll -= 1;
        }
        pine_branch(w, x, y, z, dx, dz, ll, ls, log, leaves);
    };
    for i in 0..start {
        w.set(x, y, z, log);
        if i > 5 && r.next_int_bounded(7) == 0 {
            let mut dx = -1 + r.next_int_bounded(3);
            let mut dz = -1 + r.next_int_bounded(3);
            if dx == 0 && dz == 0 {
                dx = -1 + r.next_int_bounded(3);
                dz = -1 + r.next_int_bounded(3);
            }
            branch(w, y, dx, dz, 1, 1);
        }
        y += 1;
    }
    let (mut px, mut pz) = (0, 0);
    for i in 0..size {
        let bound = if i < size - 12 && i > 2 { 2 } else { 1 };
        if r.next_int_bounded(bound) == 0 && i < size - 2 {
            let (dx, dz) = pine_dir(r, &mut px, &mut pz);
            let ll = if i < size - 12 && i > 3 {
                3
            } else if i < size - 8 {
                2
            } else {
                1
            };
            branch(w, y, dx, dz, ll, if i < size - 5 { 2 } else { 1 });
        }
        w.set(x, y, z, log);
        if i < size - 2 {
            for (ox, oz) in [(0, 1), (0, -1), (1, 0), (-1, 0)] {
                if r.next_boolean() {
                    leaves_into_air(w, x + ox, y, z + oz, leaves);
                }
            }
        }
        y += 1;
    }
    for (ox, oy, oz) in [(0, -1, 1), (0, -1, -1), (1, -1, 0), (-1, -1, 0), (0, 0, 0)] {
        leaves_into_air(w, x + ox, y + oy, z + oz, leaves);
    }
}

/// `rwg2.deco.trees.DecoSmallSpruce(size)`
fn small_spruce(w: &mut DecoWorld, r: &mut impl DRng, size: i32, x: i32, mut y: i32, z: i32) {
    w.mark("smallspruce", x, y, z);
    if !is_earth(w.get(x, y - 1, z)) {
        return;
    }
    let lm = r.next_int_bounded(2) as u8;
    let (log, leaves) = (db::mk(db::LOG, 0), db::mk(db::LEAVES, lm));
    let (mut start, mut small, mut large) = (1, 2, 0);
    if size == 1 {
        small = 2;
        large = 2;
    } else if size == 2 {
        start = 1 + r.next_int_bounded(2);
        small = 3;
        large = 3;
    }
    for _ in 0..start {
        w.set(x, y, z, log);
        y += 1;
    }
    for _ in 0..large {
        for j in -2i32..=2 {
            for k in -2i32..=2 {
                if j.abs() + k.abs() != 4
                    && ((j > -2 && k > -2 && j < 2 && k < 2) || r.next_int_bounded(4) != 0)
                {
                    w.set(x + j, y, z + k, leaves);
                }
            }
        }
        w.set(x, y, z, log);
        y += 1;
    }
    for i in 0..small {
        for j in -1i32..=1 {
            for k in -1i32..=1 {
                if j.abs() + k.abs() < 2 || r.next_int_bounded(4) != 0 {
                    w.set(x + j, y, z + k, leaves);
                }
            }
        }
        if i == 0 {
            for (ox, oz) in [
                (1, 0),
                (-1, 0),
                (0, 1),
                (0, -1),
                (2, 0),
                (-2, 0),
                (0, 2),
                (0, -2),
            ] {
                w.set(x + ox, y, z + oz, leaves);
            }
        }
        w.set(x, y, z, log);
        y += 1;
    }
    w.set(x, y, z, log);
    for (ox, oy, oz) in [
        (1, 0, 0),
        (-1, 0, 0),
        (0, 0, 1),
        (0, 0, -1),
        (0, 1, 0),
        (0, 2, 0),
    ] {
        w.set(x + ox, y + oy, z + oz, leaves);
    }
}

/// `rwg2.deco.trees.DecoPineTree(height, meta)` (a `WorldGenAbstractTree`)
fn pine_tree(w: &mut DecoWorld, r: &mut impl DRng, height: i32, meta: u8, x: i32, y: i32, z: i32) {
    w.mark("pinetree", x, y, z);
    let l = r.next_int_bounded(height * 2) + height * 2;
    let i1 = height + r.next_int_bounded(height);
    let j1 = l - i1;
    let k1 = 2 + r.next_int_bounded(2);
    if !(y >= 1 && y + l < 256) {
        return;
    }
    let mut ok = true;
    let mut l1 = y;
    while l1 <= y + 1 + l && ok {
        let l3 = if l1 - y < i1 { 0 } else { k1 };
        let mut i2 = x - l3;
        while i2 <= x + l3 && ok {
            let mut j2 = z - l3;
            while j2 <= z + l3 && ok {
                if (0..256).contains(&l1) {
                    let b = w.get(i2, l1, j2);
                    if !is_air(b) && !is_leaves(b) && !is(b, blk::SNOW_LAYER) {
                        ok = false;
                    }
                } else {
                    ok = false;
                }
                j2 += 1;
            }
            i2 += 1;
        }
        l1 += 1;
    }
    if !ok {
        return;
    }
    let below = w.get(x, y - 1, z);
    if !(sustains_sapling(below) && y < 256 - l - 1) {
        return;
    }
    if is(below, blk::GRASS) || is(below, db::FARMLAND) {
        w.set(x, y - 1, z, blk::DIRT as Db);
    }
    let leaves = db::mk(db::LEAVES, meta);
    let mut l3 = r.next_int_bounded(2);
    let mut i2 = 1;
    let mut b0 = 0;
    for i4 in 0..=j1 {
        let k2 = y + l - i4;
        for l2 in x - l3..=x + l3 {
            let i3 = l2 - x;
            for j3 in z - l3..=z + l3 {
                let k3 = j3 - z;
                if (i3.abs() != l3 || k3.abs() != l3 || l3 <= 0) && !opaque(w.get(l2, k2, j3)) {
                    w.set(l2, k2, j3, leaves);
                }
            }
        }
        if l3 >= i2 {
            l3 = b0;
            b0 = 1;
            i2 += 1;
            if i2 > k1 {
                i2 = k1;
            }
        } else {
            l3 += 1;
        }
    }
    let i4 = r.next_int_bounded(3);
    for k2 in 0..l - i4 {
        let b = w.get(x, y + k2, z);
        if is_air(b) || is_leaves(b) || is(b, blk::SNOW_LAYER) {
            w.set(x, y + k2, z, db::mk(db::LOG, 0));
        }
    }
    if height > 4 {
        let pos = [0, 0, 1, 0, 0, 1, -1, 0, 0, -1];
        for t in 0..5 {
            let mut sh = r.next_int_bounded(4) + y - 2;
            while sh > y - 1 {
                if is(w.get(x + pos[t * 2], sh, z + pos[t * 2 + 1]), blk::GRASS) {
                    break;
                }
                w.set(x + pos[t * 2], sh, z + pos[t * 2 + 1], db::mk(db::LOG, 12));
                sh -= 1;
            }
        }
    }
}

const PALM_LEAVES: [i32; 135] = [
    1, 0, 0, 2, 0, 0, 3, -1, 0, 3, -2, 0, -1, 0, 0, -2, 0, 0, -3, -1, 0, -3, -2, 0, 0, 0, 1, 0, 0,
    2, 0, -1, 3, 0, -2, 3, 0, 0, -1, 0, 0, -2, 0, -1, -3, 0, -2, -3, 0, 1, 0, 1, 1, 1, -1, 1, -1,
    -1, 1, 1, 1, 1, -1, 1, 2, 0, -1, 2, 0, 0, 2, 1, 0, 2, -1, 2, 3, 0, 3, 3, 0, 4, 2, 0, -2, 3, 0,
    -3, 3, 0, -4, 2, 0, 0, 3, 2, 0, 3, 3, 0, 2, 4, 0, 3, -2, 0, 3, -3, 0, 2, -4, 2, 2, -2, -2, 2,
    2, -2, 2, -2, 2, 2, 2, 3, 2, -3, -3, 2, 3, -3, 2, -3, 3, 2, 3,
];
const PALM_COCOAS: [i32; 16] = [2, 0, -2, 1, 1, 1, -2, 0, 0, 0, -2, -1, 3, -1, -2, 0];

/// `rwg2.deco.trees.DecoPalm`
fn palm(w: &mut DecoWorld, r: &mut impl DRng, x: i32, y: i32, z: i32) {
    w.mark("palm", x, y, z);
    let b = w.get(x, y - 1, z);
    if !(is_earth(b) || is_sand(b)) {
        return;
    }
    let hor = match r.next_int_bounded(4) {
        0 => -180.0,
        1 => -90.0,
        2 => 0.0,
        _ => 90.0,
    };
    let mut ver = 0.3f32 + r.next_float() * 0.4;
    let length = 15f32;
    let (mut px, mut py, mut pz) = (x as f32, y as f32, z as f32);
    let mut c = 0f32;
    if ver < 0.0 {
        ver = -ver;
    }
    let mut vel_y = 1.0 - ver;
    if ver > 1.0 {
        ver = 1.0 - (ver - 1.0);
    }
    let (cs, sn) = cos_sin_deg_f(hor);
    let mut vel_x = cs * ver;
    let mut vel_z = sn * ver;
    while c < length {
        w.set(px as i32, py as i32, pz as i32, db::mk(db::LOG, 15));
        if c < length - 3.0 {
            let mut loss = vel_x.abs() + vel_z.abs();
            vel_x *= 0.9;
            px += vel_x;
            vel_z *= 0.9;
            pz += vel_z;
            loss -= vel_x.abs() + vel_z.abs();
            vel_y += loss;
            py += vel_y;
        } else {
            py += vel_y;
        }
        c += 1.0;
    }
    let (x, y, z) = (px as i32, py as i32 - 1, pz as i32);
    for j in (0..133).step_by(3) {
        w.set(
            x + PALM_LEAVES[j],
            y + PALM_LEAVES[j + 1],
            z + PALM_LEAVES[j + 2],
            db::mk(db::LEAVES, 3),
        );
    }
    for k in (0..16).step_by(4) {
        if r.next_int_bounded(20) == 0 {
            w.set(
                x + PALM_COCOAS[k + 1],
                y + PALM_COCOAS[k + 2],
                z + PALM_COCOAS[k + 3],
                db::mk(db::COCOA, (PALM_COCOAS[k] + 8) as u8),
            );
        }
    }
}

/// `rwg2.deco.trees.DecoJungleSmall / Tall / Fat`, `DecoMangrove`
fn jungle(
    w: &mut DecoWorld,
    r: &mut impl DRng,
    kind: JungleKind,
    p: Branchy,
    x: i32,
    y: i32,
    z: i32,
) {
    w.mark("jungle", x, y, z);
    let b = w.get(x, y - 1, z);
    let ground = match kind {
        JungleKind::Small | JungleKind::Tall => is_earth(b),
        JungleKind::Fat => is_earth(b) || is_sand(b),
        JungleKind::Mangrove => {
            is_earth(b)
                || is_sand(b)
                || (is_water(b) && is_sand(w.get(x, y - 2, z)) && is_air(w.get(x, y, z)))
        }
    };
    if !ground {
        return;
    }
    let branch = |w: &mut DecoWorld,
                  mut bx: f32,
                  mut by: f32,
                  mut bz: f32,
                  hor: f64,
                  mut ver: f32,
                  len: f32| {
        if ver < 0.0 {
            ver = -ver;
        }
        let mut c = 0f32;
        let vel_y = 1.0 - ver;
        if ver > 1.0 {
            ver = 1.0 - (ver - 1.0);
        }
        let (cs, sn) = cos_sin_deg_f(hor);
        let (vel_x, vel_z) = (cs * ver, sn * ver);
        while c < len {
            w.set(bx as i32, by as i32, bz as i32, p.log);
            bx += vel_x;
            by += vel_y;
            bz += vel_z;
            c += 1.0;
        }
    };
    if kind == JungleKind::Fat {
        let rr = r.next_float() * 360.0;
        if p.root > 0 {
            for k in 0..5 {
                let hor = ((120 * k) - 25 + r.next_int_bounded(50)) as f32 + rr;
                let ver = 1.6 + r.next_float() * 0.1;
                branch(
                    w,
                    x as f32 + 0.5,
                    (y + p.root) as f32,
                    z as f32 + 0.5,
                    hor as f64,
                    ver,
                    p.root as f32 * 1.8,
                );
            }
        }
        for i in y + p.root - 2..y + p.base + 2 {
            w.set(x, i, z, p.log);
            w.set(x + 1, i, z + 1, p.log);
        }
        for j in 0..p.branch {
            let hor = ((80 * j) - 40 + r.next_int_bounded(80)) as f32;
            let ver = p.ver_start + r.next_float() * p.ver_rand;
            branch(
                w,
                x as f32 + 0.5,
                (y + p.base) as f32,
                z as f32 + 0.5,
                hor as f64,
                ver,
                p.length,
            );
            let a = hor as f64 * std::f64::consts::PI / 180.0;
            let mut ex = (a.cos() * ver as f64 * p.length as f64) as i32;
            let mut ez = (a.sin() * ver as f64 * p.length as f64) as i32;
            let mut ey = ((1.0 - ver) * p.length) as i32;
            for _ in 0..2 {
                let lx = x + ex - 2 + r.next_int_bounded(5);
                let ly = y + p.base + ey + r.next_int_bounded(2);
                let lz = z + ez - 2 + r.next_int_bounded(5);
                jungle_leaves(w, r, kind, p, lx as f32, ly as f32, lz as f32, 4.0, 1.5);
            }
            ex = (ex as f32 * 0.8) as i32;
            ey = (ey as f32 * 0.8) as i32;
            ez = (ez as f32 * 0.8) as i32;
            jungle_leaves(
                w,
                r,
                kind,
                p,
                (x + ex) as f32,
                (y + p.base + ey) as f32,
                (z + ez) as f32,
                3.0,
                1.5,
            );
        }
        return;
    }
    let root_mult = match kind {
        JungleKind::Mangrove => 2.0,
        _ => 1.7,
    };
    if p.root > 0 {
        for k in 0..3 {
            let hor = (120 * k) - 40 + r.next_int_bounded(80);
            let ver = 1.6 + r.next_float() * 0.1;
            branch(
                w,
                x as f32,
                (y + p.root) as f32,
                z as f32,
                hor as f64,
                ver,
                p.root as f32 * root_mult,
            );
        }
    }
    for i in y + p.root..y + p.base {
        w.set(x, i, z, p.log);
    }
    let width = if kind == JungleKind::Mangrove {
        1.2
    } else {
        1.5
    };
    for j in 0..p.branch {
        let hor = ((120 * j) - 60 + r.next_int_bounded(120)) as f32;
        let ver = p.ver_start + r.next_float() * p.ver_rand;
        branch(
            w,
            x as f32,
            (y + p.base) as f32,
            z as f32,
            hor as f64,
            ver,
            p.length,
        );
        let a = hor as f64 * std::f64::consts::PI / 180.0;
        let ex = x + (a.cos() * ver as f64 * p.length as f64) as i32;
        let ez = z + (a.sin() * ver as f64 * p.length as f64) as i32;
        let ey = y + p.base + ((1.0 - ver) * p.length) as i32;
        jungle_leaves(w, r, kind, p, ex as f32, ey as f32, ez as f32, 4.0, width);
    }
}

#[allow(clippy::too_many_arguments)]
fn jungle_leaves(
    w: &mut DecoWorld,
    r: &mut impl DRng,
    kind: JungleKind,
    p: Branchy,
    x: f32,
    y: f32,
    z: f32,
    size: f32,
    width: f32,
) {
    let s = (size - 1.0) as i32;
    let wd = ((size - 1.0) * width) as i32;
    let (j0, core) = if kind == JungleKind::Fat {
        (-s + 1, 1.3)
    } else {
        (-s, 0.6)
    };
    let (bx, by, bz) = (x as i32, y as i32, z as i32);
    for i in -wd..=wd {
        for j in j0..=s {
            for k in -wd..=wd {
                let dist = (i as f32 / width).abs() + (j.abs() as f32) + (k as f32 / width).abs();
                if dist <= size - 0.5 || (dist <= size && r.next_boolean()) {
                    if dist < core {
                        w.set(bx + i, by + j, bz + k, p.log);
                    }
                    if w.air(bx + i, by + j, bz + k) {
                        w.set(bx + i, by + j, bz + k, p.leaves);
                    }
                }
            }
        }
    }
}

/// `rwg2.deco.trees.DecoBigTree(size, type)`: one `nextLong` from the shared Random seeds the tree's own Random.
#[allow(clippy::too_many_arguments)]
fn big_tree(w: &mut DecoWorld, r: &mut impl DRng, size: i32, ty: i32, x: i32, y: i32, z: i32) {
    w.mark("bigtree", x, y, z);
    let seed = r.next_long();
    let size = if size < 0 { 10 } else { size }; // `new DecoBigTree()`: 7 + floor(Math.random() * 7)
    let mut t = BigTree {
        rand: JavaRandom::new(seed),
        base: [x, y, z],
        height_limit: size,
        height: 0,
        leaf_nodes: Vec::new(),
        vanilla: false,
    };
    if ty != 10 && !t.valid(w) {
        return;
    }
    t.leaf_node_list(w);
    for i in 0..t.leaf_nodes.len() {
        let [nx, ny, nz, _] = t.leaf_nodes[i];
        for yy in ny..ny + BigTree::LEAF_DIST {
            let s = BigTree::leaf_size(yy - ny);
            t.layer(w, nx, yy, nz, s);
        }
    }
    let (b0, b1, b2) = (t.base[0], t.base[1], t.base[2]);
    t.line(w, [b0, b1, b2], [b0, b1 + t.height, b2], db::mk(db::LOG, 0));
    for i in 0..t.leaf_nodes.len() {
        let n = t.leaf_nodes[i];
        let base = [b0, n[3], b2];
        if (n[3] - b1) as f64 >= t.height_limit as f64 * 0.2 {
            t.line(w, base, [n[0], n[1], n[2]], db::mk(db::LOG, 0));
        }
    }
}

/// Vanilla `WorldGenBigTree` (`BiomeGenBase.worldGeneratorBigTree`, `setScale(1, 1, 1)` by the decorator): one
/// `nextLong` from `r` (inside a BOP decorator that is the wrapper's own Random: no shared draw) seeds the tree.
/// `heightLimit` is an instance field the game sets once per biome object (`5 + nextInt(12)` on its first tree, kept
/// and shrunk by blocked trees afterwards): session state, so every tree is modelled as that biome's first.
pub(crate) fn vanilla_big_tree(w: &mut DecoWorld, r: &mut impl DRng, x: i32, y: i32, z: i32) {
    w.mark("bigtree", x, y, z);
    let mut rand = JavaRandom::new(r.next_long());
    let hl = 5 + rand.next_int_bounded(12);
    let mut t = BigTree {
        rand,
        base: [x, y, z],
        height_limit: hl,
        height: 0,
        leaf_nodes: Vec::new(),
        vanilla: true,
    };
    if !t.valid(w) {
        return;
    }
    t.leaf_node_list(w);
    for i in 0..t.leaf_nodes.len() {
        let [nx, ny, nz, _] = t.leaf_nodes[i];
        for yy in ny..ny + BigTree::LEAF_DIST {
            let s = BigTree::leaf_size(yy - ny);
            t.layer(w, nx, yy, nz, s);
        }
    }
    let (b0, b1, b2) = (t.base[0], t.base[1], t.base[2]);
    t.line(w, [b0, b1, b2], [b0, b1 + t.height, b2], db::mk(db::LOG, 0));
    for i in 0..t.leaf_nodes.len() {
        let n = t.leaf_nodes[i];
        if (n[3] - b1) as f64 >= t.height_limit as f64 * 0.2 {
            t.line(w, [b0, n[3], b2], [n[0], n[1], n[2]], db::mk(db::LOG, 0));
        }
    }
}

const OTHER: [usize; 6] = [2, 0, 0, 1, 2, 1];

struct BigTree {
    rand: JavaRandom,
    base: [i32; 3],
    height_limit: i32,
    height: i32,
    leaf_nodes: Vec<[i32; 4]>,
    /// vanilla `WorldGenBigTree` rules (line test = `func_150523_a`, soil dirt / grass / farmland)
    vanilla: bool,
}

impl BigTree {
    // heightAttenuation 0.618, branchSlope 0.381, scaleWidth 1, leafDensity 1, leafDistanceLimit 4 (setScale(1,1,1)
    // sets it to 5 only for par1 > 0.5: setScale is called with 1.0 by the decorators, so 5)
    const LEAF_DIST: i32 = 5;

    fn leaf_size(i: i32) -> f32 {
        if (0..Self::LEAF_DIST).contains(&i) {
            if i != 0 && i != Self::LEAF_DIST - 1 {
                3.0
            } else {
                2.0
            }
        } else {
            -1.0
        }
    }

    fn layer_size(&self, i: i32) -> f32 {
        if (i as f64) < (self.height_limit as f32) as f64 * 0.3 {
            return -1.618;
        }
        let v2 = self.height_limit as f32 / 2.0;
        let v3 = self.height_limit as f32 / 2.0 - i as f32;
        let mut v4 = if v3 == 0.0 {
            v2
        } else if v3.abs() >= v2 {
            0.0
        } else {
            ((v2.abs() as f64).powf(2.0) - (v3.abs() as f64).powf(2.0)).sqrt() as f32
        };
        v4 *= 0.5;
        v4
    }

    fn passable(&self, b: Db) -> bool {
        if self.vanilla {
            tree_passable(b)
        } else {
            is_air(b) || is(b, db::LEAVES)
        }
    }

    fn check_line(&self, w: &mut DecoWorld, a: [i32; 3], b: [i32; 3]) -> i32 {
        let mut d = [0i32; 3];
        let mut m = 0usize;
        for i in 0..3 {
            d[i] = b[i] - a[i];
            if d[i].abs() > d[m].abs() {
                m = i;
            }
        }
        if d[m] == 0 {
            return -1;
        }
        let (o1, o2) = (OTHER[m], OTHER[m + 3]);
        let step = if d[m] > 0 { 1 } else { -1 };
        let r1 = d[o1] as f64 / d[m] as f64;
        let r2 = d[o2] as f64 / d[m] as f64;
        let mut p = [0i32; 3];
        let mut i = 0;
        let end = d[m] + step;
        while i != end {
            p[m] = a[m] + i;
            p[o1] = (a[o1] as f64 + i as f64 * r1).floor() as i32;
            p[o2] = (a[o2] as f64 + i as f64 * r2).floor() as i32;
            if !self.passable(w.get(p[0], p[1], p[2])) {
                break;
            }
            i += step;
        }
        if i == end {
            -1
        } else {
            i.abs()
        }
    }

    fn line(&self, w: &mut DecoWorld, a: [i32; 3], b: [i32; 3], block: Db) {
        let mut d = [0i32; 3];
        let mut m = 0usize;
        for i in 0..3 {
            d[i] = b[i] - a[i];
            if d[i].abs() > d[m].abs() {
                m = i;
            }
        }
        if d[m] == 0 {
            return;
        }
        let (o1, o2) = (OTHER[m], OTHER[m + 3]);
        let step = if d[m] > 0 { 1 } else { -1 };
        let r1 = d[o1] as f64 / d[m] as f64;
        let r2 = d[o2] as f64 / d[m] as f64;
        let mut p = [0i32; 3];
        let mut i = 0;
        let end = d[m] + step;
        while i != end {
            p[m] = ((a[m] + i) as f64 + 0.5).floor() as i32;
            p[o1] = (a[o1] as f64 + i as f64 * r1 + 0.5).floor() as i32;
            p[o2] = (a[o2] as f64 + i as f64 * r2 + 0.5).floor() as i32;
            w.set(p[0], p[1], p[2], block);
            i += step;
        }
    }

    fn layer(&self, w: &mut DecoWorld, x: i32, y: i32, z: i32, size: f32) {
        // genTreeLayer(..., axis 1 (y), leaves)
        let r = (size as f64 + 0.618) as i32;
        let (o1, o2) = (OTHER[1], OTHER[4]);
        let c = [x, y, z];
        let mut p = [0i32; 3];
        p[1] = c[1];
        for a in -r..=r {
            p[o1] = c[o1] + a;
            for b in -r..=r {
                let d = ((a.abs() as f64) + 0.5).powf(2.0) + ((b.abs() as f64) + 0.5).powf(2.0);
                if d > (size * size) as f64 {
                    continue;
                }
                p[o2] = c[o2] + b;
                let o = w.get(p[0], p[1], p[2]);
                // vanilla genTreeLayer: only air / leaves material; RWG's own: air / oak leaves
                let ok = if self.vanilla {
                    matches!(mat(o), Mat::Air | Mat::Leaves)
                } else {
                    is_air(o) || is(o, db::LEAVES)
                };
                if ok {
                    w.set(p[0], p[1], p[2], db::mk(db::LEAVES, 0));
                }
            }
        }
    }

    fn leaf_node_list(&mut self, w: &mut DecoWorld) {
        self.height = (self.height_limit as f64 * 0.618) as i32;
        if self.height >= self.height_limit {
            self.height = self.height_limit - 1;
        }
        let mut per = (1.382 + (1.0 * self.height_limit as f64 / 13.0).powf(2.0)) as i32;
        if per < 1 {
            per = 1;
        }
        let mut y = self.base[1] + self.height_limit - Self::LEAF_DIST;
        let top = self.base[1] + self.height;
        let mut rel = y - self.base[1];
        self.leaf_nodes.clear();
        self.leaf_nodes.push([self.base[0], y, self.base[2], top]);
        y -= 1;
        while rel >= 0 {
            let s = self.layer_size(rel);
            if s < 0.0 {
                y -= 1;
                rel -= 1;
                continue;
            }
            for _ in 0..per {
                let d = 1.0 * s as f64 * (self.rand.next_float() as f64 + 0.328);
                let ang = self.rand.next_float() as f64 * 2.0 * std::f64::consts::PI;
                let nx = (d * ang.sin() + self.base[0] as f64 + 0.5).floor() as i32;
                let nz = (d * ang.cos() + self.base[2] as f64 + 0.5).floor() as i32;
                let n = [nx, y, nz];
                let n2 = [nx, y + Self::LEAF_DIST, nz];
                if self.check_line(w, n, n2) == -1 {
                    let mut b = [self.base[0], self.base[1], self.base[2]];
                    let dist = ((self.base[0] - n[0]).abs() as f64).powf(2.0)
                        + ((self.base[2] - n[2]).abs() as f64).powf(2.0);
                    let slope = dist.sqrt() * 0.381;
                    if n[1] as f64 - slope > top as f64 {
                        b[1] = top;
                    } else {
                        b[1] = (n[1] as f64 - slope) as i32;
                    }
                    if self.check_line(w, b, n) == -1 {
                        self.leaf_nodes.push([nx, y, nz, b[1]]);
                    }
                }
            }
            y -= 1;
            rel -= 1;
        }
    }

    fn valid(&mut self, w: &mut DecoWorld) -> bool {
        let a = self.base;
        let b = [a[0], a[1] + self.height_limit - 1, a[2]];
        let soil = w.get(a[0], a[1] - 1, a[2]);
        if !(is_earth(soil) || self.vanilla && is(soil, db::FARMLAND)) {
            return false;
        }
        let v = self.check_line(w, a, b);
        if v == -1 {
            true
        } else if v < 6 {
            false
        } else {
            self.height_limit = v;
            true
        }
    }
}

// --- vanilla trees ---------------------------------------------------------------------------------------------

/// `WorldGenAbstractTree.func_150523_a`: a tree may grow through this block.
pub(crate) fn tree_passable(b: Db) -> bool {
    matches!(mat(b), Mat::Air | Mat::Leaves)
        || is_earth(b)
        || matches!(db::id(b), db::LOG | db::LOG2 | db::VINE)
}

/// space check of `WorldGenTrees` / `WorldGenForest`
pub(crate) fn tree_space(w: &mut DecoWorld, x: i32, y: i32, z: i32, h: i32) -> bool {
    for yy in y..=y + 1 + h {
        let mut r = 1;
        if yy == y {
            r = 0;
        }
        if yy >= y + 1 + h - 2 {
            r = 2;
        }
        for xx in x - r..=x + r {
            for zz in z - r..=z + r {
                if !(0..256).contains(&yy) || !tree_passable(w.get(xx, yy, zz)) {
                    return false;
                }
            }
        }
    }
    true
}

/// vanilla `WorldGenTrees(false, minTreeHeight, metaWood, metaLeaves, vinesGrow)`
#[allow(clippy::too_many_arguments)]
fn trees(
    w: &mut DecoWorld,
    r: &mut impl DRng,
    min_h: i32,
    wood: u8,
    leav: u8,
    vines: bool,
    x: i32,
    y: i32,
    z: i32,
) {
    w.mark("trees", x, y, z);
    let h = r.next_int_bounded(3) + min_h;
    if !(y >= 1 && y + h < 256) {
        return;
    }
    if !tree_space(w, x, y, z, h) {
        return;
    }
    let below = w.get(x, y - 1, z);
    if !((is_earth(below) || is(below, db::FARMLAND)) && y < 256 - h - 1) {
        return;
    }
    w.set(x, y - 1, z, blk::DIRT as Db);
    let leaves = db::mk(db::LEAVES, leav);
    for yy in y - 3 + h..=y + h {
        let rel = yy - (y + h);
        let rad = 1 - rel / 2;
        for xx in x - rad..=x + rad {
            let dx = xx - x;
            for zz in z - rad..=z + rad {
                let dz = zz - z;
                if dx.abs() != rad || dz.abs() != rad || (r.next_int_bounded(2) != 0 && rel != 0) {
                    let b = w.get(xx, yy, zz);
                    if matches!(mat(b), Mat::Air | Mat::Leaves) {
                        w.set(xx, yy, zz, leaves);
                    }
                }
            }
        }
    }
    for k in 0..h {
        let b = w.get(x, y + k, z);
        if matches!(mat(b), Mat::Air | Mat::Leaves) {
            w.set(x, y + k, z, db::mk(db::LOG, wood));
            if vines && k > 0 {
                for (dx, dz, m) in [(-1, 0, 8u8), (1, 0, 2), (0, -1, 1), (0, 1, 4)] {
                    if r.next_int_bounded(3) > 0 && w.air(x + dx, y + k, z + dz) {
                        w.set(x + dx, y + k, z + dz, db::mk(db::VINE, m));
                    }
                }
            }
        }
    }
    if vines {
        for yy in y - 3 + h..=y + h {
            let rel = yy - (y + h);
            let rad = 2 - rel / 2;
            for xx in x - rad..=x + rad {
                for zz in z - rad..=z + rad {
                    if mat(w.get(xx, yy, zz)) == Mat::Leaves {
                        for (dx, dz, m) in [(-1, 0, 8u8), (1, 0, 2), (0, -1, 1), (0, 1, 4)] {
                            if r.next_int_bounded(4) == 0 && is_air(w.get(xx + dx, yy, zz + dz)) {
                                grow_vines(w, xx + dx, yy, zz + dz, m);
                            }
                        }
                    }
                }
            }
        }
        if r.next_int_bounded(5) == 0 && h > 5 {
            // Direction.rotateOpposite = {2, 3, 0, 1}; offsetX = {0, -1, 0, 1}, offsetZ = {1, 0, -1, 0}
            const OPP: [usize; 4] = [2, 3, 0, 1];
            const OX: [i32; 4] = [0, -1, 0, 1];
            const OZ: [i32; 4] = [1, 0, -1, 0];
            for k in 0..2 {
                #[allow(clippy::needless_range_loop)]
                for d in 0..4usize {
                    if r.next_int_bounded(4 - k) == 0 {
                        let age = r.next_int_bounded(3);
                        let o = OPP[d];
                        w.set(
                            x + OX[o],
                            y + h - 5 + k,
                            z + OZ[o],
                            db::mk(db::COCOA, (age << 2 | d as i32) as u8),
                        );
                    }
                }
            }
        }
    }
}

fn grow_vines(w: &mut DecoWorld, x: i32, mut y: i32, z: i32, m: u8) {
    w.set(x, y, z, db::mk(db::VINE, m));
    let mut n = 4;
    loop {
        y -= 1;
        if !is_air(w.get(x, y, z)) || n <= 0 {
            return;
        }
        w.set(x, y, z, db::mk(db::VINE, m));
        n -= 1;
    }
}

/// vanilla `WorldGenForest(false, tall)` (birch)
fn forest(w: &mut DecoWorld, r: &mut impl DRng, tall: bool, x: i32, y: i32, z: i32) {
    w.mark("forest", x, y, z);
    let mut h = r.next_int_bounded(3) + 5;
    if tall {
        h += r.next_int_bounded(7);
    }
    if !(y >= 1 && y + h < 256) {
        return;
    }
    if !tree_space(w, x, y, z, h) {
        return;
    }
    let below = w.get(x, y - 1, z);
    if !((is_earth(below) || is(below, db::FARMLAND)) && y < 256 - h - 1) {
        return;
    }
    w.set(x, y - 1, z, blk::DIRT as Db);
    for yy in y - 3 + h..=y + h {
        let rel = yy - (y + h);
        let rad = 1 - rel / 2;
        for xx in x - rad..=x + rad {
            let dx = xx - x;
            for zz in z - rad..=z + rad {
                let dz = zz - z;
                if dx.abs() != rad || dz.abs() != rad || (r.next_int_bounded(2) != 0 && rel != 0) {
                    let b = w.get(xx, yy, zz);
                    if matches!(mat(b), Mat::Air | Mat::Leaves) {
                        w.set(xx, yy, zz, db::mk(db::LEAVES, 2));
                    }
                }
            }
        }
    }
    for k in 0..h {
        let b = w.get(x, y + k, z);
        if matches!(mat(b), Mat::Air | Mat::Leaves) {
            w.set(x, y + k, z, db::mk(db::LOG, 2));
        }
    }
}

/// vanilla `WorldGenMegaJungle(false, baseHeight, field_150538_d, wood, leaves)`
#[allow(clippy::too_many_arguments)]
fn mega_jungle(
    w: &mut DecoWorld,
    r: &mut impl DRng,
    base: i32,
    rnd: i32,
    wood: u8,
    leav: u8,
    x: i32,
    y: i32,
    z: i32,
    bop: bool,
) {
    w.mark("megajungle", x, y, z);
    let mut h = r.next_int_bounded(3) + base;
    if rnd > 1 {
        h += r.next_int_bounded(rnd);
    }
    // func_150536_b: space
    if !(y >= 1 && y + h < 256) {
        return;
    }
    for yy in y..=y + 1 + h {
        let mut rad = 2;
        if yy == y {
            rad = 1;
        }
        if yy >= y + 1 + h - 2 {
            rad = 2;
        }
        for xx in x - rad..=x + rad {
            for zz in z - rad..=z + rad {
                if !(0..256).contains(&yy) || !tree_passable(w.get(xx, yy, zz)) {
                    return;
                }
            }
        }
    }
    // func_150532_c: ground
    // Forge: `canSustainPlant(sapling)`; `onPlantGrow` turns only grass / farmland into dirt
    let below = w.get(x, y - 1, z);
    if !(sustains_sapling(below) && y >= 2) {
        return;
    }
    for (dx, dz) in [(0, 0), (1, 0), (0, 1), (1, 1)] {
        let b = w.get(x + dx, y - 1, z + dz);
        if matches!(db::id(b), blk::GRASS | db::FARMLAND) {
            w.set(x + dx, y - 1, z + dz, blk::DIRT as Db);
        }
    }
    let vine = if bop { db::BOP_IVY } else { db::VINE };
    let leaves = db::mk(db::LEAVES, leav);
    let logb = db::mk(db::LOG, wood);
    // crown: func_150543_c(x, z, y + h, 2)
    for yy in y + h - 2..=y + h {
        let rel = yy - (y + h);
        huge_leaves_sq(w, x, yy, z, 2 + 1 - rel, leaves);
    }
    let mut yy = if bop {
        y + 2
    } else {
        y + h - 2 - r.next_int_bounded(4)
    };
    while if bop { yy < y + h } else { yy > y + h / 2 } {
        let f = r.next_float() * std::f32::consts::PI * 2.0;
        let mut bx = x + (0.5 + mcos(f) * 4.0) as i32;
        let mut bz = z + (0.5 + msin(f) * 4.0) as i32;
        for k in 0..5 {
            bx = x + (1.5 + mcos(f) * k as f32) as i32;
            bz = z + (1.5 + msin(f) * k as f32) as i32;
            w.set(bx, yy - 3 + k / 2, bz, logb);
        }
        let n = 1 + r.next_int_bounded(2);
        for ly in yy - n..=yy {
            let rel = ly - yy;
            huge_leaves_disc(w, bx, ly, bz, 1 - rel, leaves);
        }
        if bop {
            yy += r.next_int_bounded(4);
        } else {
            yy -= 2 + r.next_int_bounded(4);
        }
    }
    let am = |b: Db| matches!(mat(b), Mat::Air | Mat::Leaves);
    for k in 0..h {
        if am(w.get(x, y + k, z)) {
            w.set(x, y + k, z, logb);
            if k > 0 {
                if r.next_int_bounded(3) > 0 && w.air(x - 1, y + k, z) {
                    w.set(x - 1, y + k, z, db::mk(vine, 8));
                }
                if r.next_int_bounded(3) > 0 && w.air(x, y + k, z - 1) {
                    w.set(x, y + k, z - 1, db::mk(vine, 1));
                }
            }
        }
        if k < h - 1 {
            if am(w.get(x + 1, y + k, z)) {
                w.set(x + 1, y + k, z, logb);
                if k > 0 {
                    if r.next_int_bounded(3) > 0 && w.air(x + 2, y + k, z) {
                        w.set(x + 2, y + k, z, db::mk(vine, 2));
                    }
                    if r.next_int_bounded(3) > 0 && w.air(x + 1, y + k, z - 1) {
                        w.set(x + 1, y + k, z - 1, db::mk(vine, 1));
                    }
                }
            }
            if am(w.get(x + 1, y + k, z + 1)) {
                w.set(x + 1, y + k, z + 1, logb);
                if k > 0 {
                    if r.next_int_bounded(3) > 0 && w.air(x + 2, y + k, z + 1) {
                        w.set(x + 2, y + k, z + 1, db::mk(vine, 2));
                    }
                    if r.next_int_bounded(3) > 0 && w.air(x + 1, y + k, z + 2) {
                        w.set(x + 1, y + k, z + 2, db::mk(vine, 4));
                    }
                }
            }
            if am(w.get(x, y + k, z + 1)) {
                w.set(x, y + k, z + 1, logb);
                if k > 0 {
                    if r.next_int_bounded(3) > 0 && w.air(x - 1, y + k, z + 1) {
                        w.set(x - 1, y + k, z + 1, db::mk(vine, 8));
                    }
                    if r.next_int_bounded(3) > 0 && w.air(x, y + k, z + 2) {
                        w.set(x, y + k, z + 2, db::mk(vine, 4));
                    }
                }
            }
        }
    }
}

/// `WorldGenHugeTrees.func_150535_a` (2×2-centred leaf square)
fn huge_leaves_sq(w: &mut DecoWorld, x: i32, y: i32, z: i32, rad: i32, leaves: Db) {
    let r2 = rad * rad;
    for xx in x - rad..=x + rad + 1 {
        let a = xx - x;
        for zz in z - rad..=z + rad + 1 {
            let b = zz - z;
            let (c, d) = (a - 1, b - 1);
            if a * a + b * b <= r2
                || c * c + d * d <= r2
                || a * a + d * d <= r2
                || c * c + b * b <= r2
            {
                let t = w.get(xx, y, zz);
                if matches!(mat(t), Mat::Air | Mat::Leaves) {
                    w.set(xx, y, zz, leaves);
                }
            }
        }
    }
}

/// `WorldGenHugeTrees.func_150534_b` (leaf disc)
fn huge_leaves_disc(w: &mut DecoWorld, x: i32, y: i32, z: i32, rad: i32, leaves: Db) {
    let r2 = rad * rad;
    for xx in x - rad..=x + rad {
        let a = xx - x;
        for zz in z - rad..=z + rad {
            let b = zz - z;
            if a * a + b * b <= r2 {
                let t = w.get(xx, y, zz);
                if matches!(mat(t), Mat::Air | Mat::Leaves) {
                    w.set(xx, y, zz, leaves);
                }
            }
        }
    }
}

/// vanilla `WorldGenVines`
fn vines(w: &mut DecoWorld, r: &mut impl DRng, x0: i32, mut y: i32, z0: i32) {
    w.mark("vines", x0, y, z0);
    let (mut x, mut z) = (x0, z0);
    while y < 128 {
        if w.air(x, y, z) {
            for side in 2..=5 {
                if w.vine_side(x, y, z, side) {
                    // 1 << Direction.facingToDirection[Facing.oppositeSide[side]]
                    let m = match side {
                        2 => 1,
                        3 => 4,
                        4 => 2,
                        _ => 8,
                    };
                    w.set(x, y, z, db::mk(db::VINE, m));
                    break;
                }
            }
        } else {
            x = x0 + r.next_int_bounded(4) - r.next_int_bounded(4);
            z = z0 + r.next_int_bounded(4) - r.next_int_bounded(4);
        }
        y += 1;
    }
}

/// vanilla `WorldGenLakes(water / lava)` on the decoration world (surface lakes).
fn lakes(w: &mut DecoWorld, r: &mut impl DRng, water: bool, x: i32, mut y: i32, z: i32) {
    w.mark("lake", x, y, z);
    let (x, z) = (x - 8, z - 8);
    while y > 5 && w.air(x, y, z) {
        y -= 1;
    }
    if y <= 4 {
        return;
    }
    y -= 4;
    let liquid: Db = if water {
        blk::WATER as Db
    } else {
        blk::LAVA as Db
    };
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
            for j in 1..15 {
                for k in 1..7 {
                    let a = (i as f64 - d3) / (d0 / 2.0);
                    let b = (k as f64 - d4) / (d1 / 2.0);
                    let c = (j as f64 - d5) / (d2 / 2.0);
                    if a * a + b * b + c * c < 1.0 {
                        m[(i * 16 + j) * 8 + k] = true;
                    }
                }
            }
        }
    }
    let at = |i: usize, j: usize, k: usize| m[(i * 16 + j) * 8 + k];
    let edge = |i: usize, j: usize, k: usize| {
        !at(i, j, k)
            && ((i < 15 && at(i + 1, j, k))
                || (i > 0 && at(i - 1, j, k))
                || (j < 15 && at(i, j + 1, k))
                || (j > 0 && at(i, j - 1, k))
                || (k < 7 && at(i, j, k + 1))
                || (k > 0 && at(i, j, k - 1)))
    };
    for i in 0..16usize {
        for j in 0..16usize {
            for k in 0..8usize {
                if edge(i, j, k) {
                    let b = w.get(x + i as i32, y + k as i32, z + j as i32);
                    if k >= 4 && mat_liquid(b) {
                        return;
                    }
                    if k < 4 && !mat_solid(b) && b != liquid {
                        return;
                    }
                }
            }
        }
    }
    for i in 0..16usize {
        for j in 0..16usize {
            for k in 0..8usize {
                if at(i, j, k) {
                    w.set(
                        x + i as i32,
                        y + k as i32,
                        z + j as i32,
                        if k >= 4 { AIR } else { liquid },
                    );
                }
            }
        }
    }
    for i in 0..16usize {
        for j in 0..16usize {
            for k in 4..8usize {
                let (bx, by, bz) = (x + i as i32, y + k as i32, z + j as i32);
                if at(i, j, k) && is(w.get(bx, by - 1, bz), blk::DIRT) && w.lit(bx, by, bz) {
                    w.set(bx, by - 1, bz, blk::GRASS as Db);
                }
            }
        }
    }
    if !water {
        for i in 0..16usize {
            for j in 0..16usize {
                for k in 0..8usize {
                    if edge(i, j, k) && (k < 4 || r.next_int_bounded(2) != 0) {
                        let (bx, by, bz) = (x + i as i32, y + k as i32, z + j as i32);
                        if mat_solid(w.get(bx, by, bz)) {
                            w.set(bx, by, bz, blk::STONE as Db);
                        }
                    }
                }
            }
        }
    } else {
        for i in 0..16i32 {
            for j in 0..16i32 {
                let (bx, by, bz) = (x + i, y + 4, z + j);
                if is_water(w.get(bx, by, bz)) && w.temperature(bx, bz) < 0.15 {
                    w.set(bx, by, bz, blk::ICE as Db);
                }
            }
        }
    }
}
