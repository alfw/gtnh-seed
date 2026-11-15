//! Approximate BOP tree generators for the BOP biomes without a hand-written port (`bop_table`).
//!
//! The selector (`func_150567_a`) is exact as a draw tree; the shapes are not: each BOP tree class is mapped to one
//! of a few parametrised shapes with the class's blocks and heights. Plausible, not the game's blocks.

use super::*;

/// Approximate tree shape (blocks are `Db`).
#[derive(Clone, Copy, Debug)]
pub enum TG {
    /// `BiomeGenBase.worldGeneratorTrees`
    Vanilla,
    /// `BiomeGenBase.worldGeneratorBigTree`
    VanillaBig,
    /// vanilla-style blob canopy: (log, leaves, height, extra height, hanging vine or AIR)
    Round(Db, Db, i32, i32, Db),
    /// round canopy mixing two leaves
    Mixed(Db, Db, Db),
    /// tall trunk, several leaf blobs: (log, leaves, height)
    Big(Db, Db, i32),
    /// conifer: (log, leaves, height, extra height, max radius)
    Cone(Db, Db, i32, i32, i32),
    /// log + leaf mound: (log, leaves, 0 small / 1 normal)
    Bush(Db, Db, i32),
    /// bare trunk with leaf clumps on top: (log, leaves, height, extra height)
    /// `WorldGenBulbTree(wood, leaves, ..., minHeight, randomHeight, vines)` (exact)
    Bulb(Db, Db, i32, i32, bool),
    /// `WorldGenBOPTaiga2(wood, leaves, ..., minHeight, randomHeight, altNo, metaFruit)` (exact)
    Taiga2(Db, Db, i32, i32, i32, i8),
    /// `WorldGenBOPTaiga3(wood, leaves, ..., minHeight, randomHeight, altNo, metaFruit)` (exact)
    Taiga3(Db, Db, i32, i32, i32, i8),
    /// `WorldGenBOPSwampTree(wood, leaves, ..., minHeight, randomHeight[, vineBlock, vineMeta])` (exact;
    /// vine AIR = vanilla vine with the side meta)
    Swamp(Db, Db, i32, i32, Db),
    /// `WorldGenPalmTree1()` (35, 10, 1.3) (exact)
    Palm1,
    /// `WorldGenTropicsShrub` (exact)
    TropicsShrub,
    /// `WorldGenRainforestTree1(wood, leaves, metaWood, metaLeaves, notify, minHeight, randomHeight)` (exact)
    Rainforest1(Db, Db, i32, i32),
    /// leafless trunk with branches
    Dead(Db),
    /// curved trunk, star canopy
    Palm(Db, Db),
    /// giant flower (0 red, 1 yellow): exact `WorldGenGiantFlower`
    Flower(u8),
    /// exact `WorldGenOriginalTree(wood, leaves, metaWood, metaLeaves, …, min, rand, vines)` (+ fruit draws)
    Orig(Db, Db, i32, i32, bool, bool),
    /// exact `WorldGenBOPShrub(wood, leaves, …, minHeight, maxHeight, soil…)` (soil = `SOIL_*` bits)
    BopShrub(Db, Db, i32, i32, u8),
    /// exact `WorldGenMiniShrub(wood, leaves, …, soil…)`
    MiniShrub(Db, Db, u8),
    /// exact `WorldGenChaparral3` (log + leaf on a grass / sand top, no draws)
    Chaparral3,
    /// an exact generator of [`super::gens::G`] (vanilla `WorldGenShrub`, `WorldGenTrees`, `WorldGenMegaJungle`)
    G(super::gens::G),
}

/// Soil list bits of the BOP shrub generators.
pub const SOIL_GRASS: u8 = 1;
pub const SOIL_DIRT: u8 = 2;
pub const SOIL_SAND: u8 = 4;
pub const SOIL_HARD_SAND: u8 = 8;
pub const SOIL_HARD_DIRT: u8 = 16;
pub const SOIL_LONG_GRASS: u8 = 32;
pub const SOIL_MYCELIUM: u8 = 64;

fn soil_ok(b: Db, soil: u8) -> bool {
    (soil & SOIL_GRASS != 0 && is(b, blk::GRASS))
        || (soil & SOIL_DIRT != 0 && is_dirt(b))
        || (soil & SOIL_SAND != 0 && is_sand(b))
        || (soil & SOIL_HARD_SAND != 0 && is(b, blk::BOP_HARD_SAND))
        || (soil & SOIL_HARD_DIRT != 0 && is(b, blk::BOP_HARD_DIRT))
        || (soil & SOIL_LONG_GRASS != 0 && is(b, blk::BOP_LONG_GRASS))
        || (soil & SOIL_MYCELIUM != 0 && is(b, blk::MYCELIUM))
}

/// `func_150567_a` as a draw tree.
#[derive(Debug)]
pub enum TS {
    Leaf(TG),
    /// `random.nextInt(n) == 0 ? a : b`
    Pick(i32, &'static TS, &'static TS),
    /// a draw in the constructor arguments
    Draw(i32, &'static TS),
}

impl TS {
    pub fn pick(&'static self, r: &mut impl DRng) -> TG {
        let mut t = self;
        loop {
            match t {
                TS::Leaf(g) => return *g,
                TS::Pick(n, a, b) => t = if r.next_int_bounded(*n) == 0 { a } else { b },
                TS::Draw(n, a) => {
                    r.next_int_bounded(*n);
                    t = a;
                }
            }
        }
    }
}

fn leaf(w: &mut DecoWorld, x: i32, y: i32, z: i32, b: Db) {
    let o = w.get(x, y, z);
    if !opaque(o) && !matches!(mat(o), Mat::Wood | Mat::Water | Mat::Lava | Mat::Leaves) {
        w.set(x, y, z, b);
    }
}

fn log(w: &mut DecoWorld, x: i32, y: i32, z: i32, b: Db) {
    let o = w.get(x, y, z);
    if !opaque(o) || matches!(mat(o), Mat::Leaves) {
        w.set(x, y, z, b);
    }
}

/// Ground under (x, y, z) for a sapling (sand too for palms / bushes); the clearance above.
fn rooted(w: &mut DecoWorld, x: i32, y: i32, z: i32, sand: bool, h: i32) -> bool {
    if y < 2 || y + h + 2 > 255 {
        return false;
    }
    let b = w.get(x, y - 1, z);
    if !(sustains_sapling(b) || sand && is_sand(b)) {
        return false;
    }
    (y..y + h).all(|k| {
        let o = w.get(x, k, z);
        !opaque(o) && !matches!(mat(o), Mat::Wood | Mat::Water | Mat::Lava)
    })
}

#[allow(clippy::too_many_arguments)]
fn blob(
    w: &mut DecoWorld,
    r: &mut impl DRng,
    x: i32,
    y: i32,
    z: i32,
    rad: i32,
    b: Db,
    b2: Option<Db>,
) {
    let r2 = rad * rad + rad;
    for dy in -rad..=rad {
        for dx in -rad..=rad {
            for dz in -rad..=rad {
                let d = dx * dx + dy * dy * 2 + dz * dz;
                if d <= r2 && (d < r2 - rad || r.next_int_bounded(3) != 0) {
                    let blk = match b2 {
                        Some(o) if r.next_int_bounded(3) == 0 => o,
                        _ => b,
                    };
                    leaf(w, x + dx, y + dy, z + dz, blk);
                }
            }
        }
    }
}

fn vines(w: &mut DecoWorld, r: &mut impl DRng, x: i32, y: i32, z: i32, rad: i32, v: Db) {
    for _ in 0..6 {
        let (dx, dz) = match r.next_int_bounded(4) {
            0 => (rad + 1, r.next_int_bounded(2 * rad + 1) - rad),
            1 => (-rad - 1, r.next_int_bounded(2 * rad + 1) - rad),
            2 => (r.next_int_bounded(2 * rad + 1) - rad, rad + 1),
            _ => (r.next_int_bounded(2 * rad + 1) - rad, -rad - 1),
        };
        let len = 1 + r.next_int_bounded(4);
        for k in 0..len {
            if !is_air(w.get(x + dx, y - k, z + dz)) {
                break;
            }
            w.set(x + dx, y - k, z + dz, v);
        }
    }
}

impl TG {
    pub fn generate(self, w: &mut DecoWorld, r: &mut impl DRng, x: i32, y: i32, z: i32) {
        match self {
            TG::G(g) => g.generate(w, r, x, y, z),
            TG::Chaparral3 => {
                let mut y = y;
                while is_air(w.get(x, y, z)) && y > 2 {
                    y -= 1;
                }
                let b = w.get(x, y, z);
                if !(is(b, blk::GRASS) || is_sand(b)) {
                    return;
                }
                for a in -2..=2 {
                    for c in -2..=2 {
                        if is_air(w.get(x + a, y - 1, z + c)) && is_air(w.get(x + a, y - 2, z + c))
                        {
                            return;
                        }
                    }
                }
                w.set(x, y, z, db::mk(db::LOG, 0));
                w.set(x, y + 1, z, db::mk(db::LEAVES, 0));
            }
            TG::Orig(lg, lv, min, rand, vines, fruit) => {
                original_tree(w, r, lg, lv, min, rand, vines, fruit, x, y, z)
            }
            TG::BopShrub(lg, lv, miny, maxy, soil) => {
                let mut y = y;
                loop {
                    let b = w.get(x, y, z);
                    if !(is_leaves(b) || is_air(b)) || y <= 0 {
                        break;
                    }
                    y -= 1;
                }
                if !soil_ok(w.get(x, y, z), soil) {
                    return;
                }
                y += 1;
                if !(y > miny && y < maxy) {
                    return;
                }
                w.set(x, y, z, lg);
                for l in y..=y + 2 {
                    let j1 = 2 - (l - y);
                    for k1 in x - j1..=x + j1 {
                        for i2 in z - j1..=z + j1 {
                            if ((k1 - x).abs() != j1
                                || (i2 - z).abs() != j1
                                || r.next_int_bounded(2) != 0)
                                && !opaque(w.get(k1, l, i2))
                            {
                                w.set(k1, l, i2, lv);
                            }
                        }
                    }
                }
            }
            TG::MiniShrub(lg, lv, soil) => {
                let mut y = y;
                let mut b;
                loop {
                    b = w.get(x, y, z);
                    if !(is_air(b) || is(b, blk::BEDROCK)) {
                        break;
                    }
                    y -= 1;
                    if y <= 0 {
                        break;
                    }
                }
                if !soil_ok(b, soil) {
                    return;
                }
                let ok = [
                    (0, 1, 0),
                    (0, 2, 0),
                    (1, 2, 0),
                    (-1, 2, 0),
                    (0, 2, 1),
                    (0, 2, -1),
                    (0, 3, 0),
                ]
                .iter()
                .all(|&(dx, dy, dz)| super::gens::tree_passable(w.get(x + dx, y + dy, z + dz)));
                if !ok {
                    return;
                }
                if is(b, blk::GRASS) {
                    w.set(x, y, z, blk::DIRT as Db);
                }
                w.set(x, y + 1, z, lg);
                w.set(x, y + 2, z, lg);
                for (dx, dz) in [(1, 0), (-1, 0), (0, 1), (0, -1)] {
                    w.set(x + dx, y + 2, z + dz, lv);
                }
                w.set(x, y + 3, z, lv);
            }
            TG::Vanilla => super::gens::G::Trees(4, 0, 0, false).generate(w, r, x, y, z),
            TG::VanillaBig => super::gens::vanilla_big_tree(w, r, x, y, z),
            TG::Round(lg, lv, h, v, vine) => {
                let h = h + r.next_int_bounded(v + 1);
                if !rooted(w, x, y, z, false, h) {
                    return;
                }
                // `WorldGenTrees` canopy: the top 4 layers, radius 2 / 2 / 1 / 1, random corners
                for k in y + h - 3..=y + h {
                    let m = 1 - (k - (y + h)) / 2;
                    for dx in -m..=m {
                        for dz in -m..=m {
                            if dx.abs() != m
                                || dz.abs() != m
                                || r.next_int_bounded(2) != 0 && k < y + h
                            {
                                leaf(w, x + dx, k, z + dz, lv);
                            }
                        }
                    }
                }
                for k in y..y + h {
                    log(w, x, k, z, lg);
                }
                if vine != AIR {
                    vines(w, r, x, y + h - 1, z, 2, vine);
                }
            }
            TG::Mixed(lg, lv, lv2) => {
                let h = 4 + r.next_int_bounded(3);
                if !rooted(w, x, y, z, false, h) {
                    return;
                }
                blob(w, r, x, y + h - 1, z, 2, lv, Some(lv2));
                for k in y..y + h {
                    log(w, x, k, z, lg);
                }
            }
            TG::Big(lg, lv, h) => {
                let h = h + r.next_int_bounded(h / 2 + 1);
                if !rooted(w, x, y, z, false, h) {
                    return;
                }
                blob(w, r, x, y + h - 1, z, 3, lv, None);
                for _ in 0..2 + h / 6 {
                    let k = y + h / 2 + r.next_int_bounded(h / 2);
                    let (dx, dz) = (r.next_int_bounded(5) - 2, r.next_int_bounded(5) - 2);
                    blob(w, r, x + dx, k, z + dz, 2, lv, None);
                    log(w, x + dx / 2, k - 1, z + dz / 2, lg);
                }
                for k in y..y + h {
                    log(w, x, k, z, lg);
                }
            }
            TG::Cone(lg, lv, h, v, wd) => {
                let h = h + r.next_int_bounded(v + 1);
                if !rooted(w, x, y, z, false, h) {
                    return;
                }
                let bottom = y + 1 + h / 3;
                let mut rad = 0i32;
                for k in (bottom..=y + h).rev() {
                    for dx in -rad..=rad {
                        for dz in -rad..=rad {
                            if dx.abs() != rad || dz.abs() != rad || rad == 0 {
                                leaf(w, x + dx, k, z + dz, lv);
                            }
                        }
                    }
                    rad = if rad >= wd.max(1) || rad >= 1 && (y + h - k) % 2 == 1 {
                        (rad - 1).max(0)
                    } else {
                        rad + 1
                    };
                    if k == y + h {
                        rad = 1;
                    }
                }
                leaf(w, x, y + h + 1, z, lv);
                for k in y..y + h {
                    log(w, x, k, z, lg);
                }
            }
            TG::Bush(lg, lv, s) => {
                if !rooted(w, x, y, z, true, 1) {
                    return;
                }
                let rad = 1 + s;
                for k in y..=y + rad {
                    let m = rad - (k - y);
                    for dx in -m..=m {
                        for dz in -m..=m {
                            if dx.abs() != m || dz.abs() != m || r.next_int_bounded(2) != 0 {
                                leaf(w, x + dx, k, z + dz, lv);
                            }
                        }
                    }
                }
                log(w, x, y, z, lg);
            }
            TG::Taiga2(lg, lv, min, rand, alt, fruit) => {
                super::bop::bop_taiga2(w, r, lg, lv, min, rand, alt, fruit, x, y, z)
            }
            TG::Taiga3(lg, lv, min, rand, alt, fruit) => {
                super::bop::bop_taiga3(w, r, lg, lv, min, rand, alt, fruit, x, y, z)
            }
            TG::Swamp(lg, lv, min, rand, vine) => {
                let v = if vine == AIR { None } else { Some(vine) };
                super::bop::bop_swamp_tree(w, r, lg, lv, min, rand, v, x, y, z)
            }
            TG::Palm1 => palm_tree1(w, r, x, y, z),
            TG::TropicsShrub => tropics_shrub(w, r, x, y, z),
            TG::Rainforest1(lg, lv, min, rand) => {
                rainforest_tree1(w, r, lg, lv, min, rand, x, y, z)
            }
            TG::Bulb(lg, lv, min, rand, vines) => {
                bulb_tree(w, r, lg, lv, min, rand, vines, x, y, z)
            }
            TG::Dead(lg) => {
                let h = 5 + r.next_int_bounded(4);
                if !rooted(w, x, y, z, true, h) {
                    return;
                }
                for k in y..y + h {
                    log(w, x, k, z, lg);
                }
                for _ in 0..2 + r.next_int_bounded(2) {
                    let (dx, dz) =
                        [(1, 0), (-1, 0), (0, 1), (0, -1)][r.next_int_bounded(4) as usize];
                    let k = y + h / 2 + r.next_int_bounded(h / 2);
                    for s in 1..=2 {
                        log(w, x + dx * s, k + s - 1, z + dz * s, lg);
                    }
                }
            }
            TG::Palm(lg, lv) => {
                let h = 6 + r.next_int_bounded(4);
                if !rooted(w, x, y, z, true, h) {
                    return;
                }
                let (dx, dz) = [(1, 0), (-1, 0), (0, 1), (0, -1)][r.next_int_bounded(4) as usize];
                let (mut tx, mut tz) = (x, z);
                for k in y..y + h {
                    if k - y == h / 2 || k - y == h * 3 / 4 {
                        tx += dx;
                        tz += dz;
                    }
                    log(w, tx, k, tz, lg);
                }
                let top = y + h;
                leaf(w, tx, top, tz, lv);
                for (ax, az) in [
                    (1, 0),
                    (-1, 0),
                    (0, 1),
                    (0, -1),
                    (1, 1),
                    (-1, -1),
                    (1, -1),
                    (-1, 1),
                ] {
                    let n = if ax != 0 && az != 0 { 2 } else { 3 };
                    for s in 1..=n {
                        leaf(w, tx + ax * s, top - (s == n) as i32, tz + az * s, lv);
                    }
                }
            }
            TG::Flower(t) => giant_flower(w, t, x, y, z),
        }
    }
}

/// BOP `WorldGenPalmTree1.generate` with (strengthRand 35, bMax 10, offset 1.3): no space check beyond the 5×5
/// overhang test; `buildBlock` places only into air / leaves.
fn palm_tree1(w: &mut DecoWorld, r: &mut impl DRng, x: i32, mut y: i32, z: i32) {
    w.mark("palm1", x, y, z);
    while w.air(x, y, z) && y > 2 {
        y -= 1;
    }
    if !is(w.get(x, y, z), blk::GRASS) {
        return;
    }
    for dx in -2..=2 {
        for dz in -2..=2 {
            if w.air(x + dx, y - 1, z + dz)
                && w.air(x + dx, y - 2, z + dz)
                && !w.air(x + dx, y, z + dz)
            {
                return;
            }
        }
    }
    let build = |w: &mut DecoWorld, x: i32, y: i32, z: i32, b: Db| {
        let t = w.get(x, y, z);
        if is_air(t) || is_leaves(t) {
            w.set(x, y, z, b);
        }
    };
    let strength = r.next_int_bounded(35) as f64 / 100.0;
    let (mut xoff, mut yoff) = (0.0f64, 0.0f64);
    match r.next_int_bounded(4) {
        0 => xoff = strength,
        1 => xoff = -strength,
        2 => yoff = strength,
        _ => yoff = -strength,
    }
    let mut h = 1;
    build(w, x, y, z, blk::DIRT as Db);
    let (lg, lv) = (db::mk(db::BOP_LOGS2, 3), db::mk(db::BOP_CLEAVES1, 2));
    for b in 0..10 {
        let (tx, tz) = (x + xoff.floor() as i32, z + yoff.floor() as i32);
        build(w, tx, y + h, tz, lg);
        if b == 9 {
            let ty = y + h;
            for (dx, dy, dz) in [
                (2, -1, 0),
                (-2, -1, 0),
                (0, -1, 2),
                (0, -1, -2),
                (1, 0, 0),
                (-1, 0, 0),
                (0, 0, 1),
                (0, 0, -1),
                (2, 0, 2),
                (-2, 0, -2),
                (2, 0, -2),
                (-2, 0, 2),
                (1, 1, -1),
                (-1, 1, 1),
                (1, 1, 1),
                (-1, 1, -1),
                (0, 1, 0),
                (2, 2, 0),
                (-2, 2, 0),
                (0, 2, 2),
                (0, 2, -2),
            ] {
                build(w, tx + dx, ty + dy, tz + dz, lv);
            }
        } else {
            h += 1;
            xoff *= 1.3;
            yoff *= 1.3;
        }
    }
}

/// BOP `WorldGenTropicsShrub.generate` (blocks set unconditionally)
fn tropics_shrub(w: &mut DecoWorld, r: &mut impl DRng, x: i32, mut y: i32, z: i32) {
    w.mark("tropicsshrub", x, y, z);
    while w.air(x, y, z) && y > 2 {
        y -= 1;
    }
    let g = w.get(x, y, z);
    if !(is(g, blk::GRASS) || is(g, blk::SAND)) {
        return;
    }
    for dx in -2..=2 {
        for dz in -2..=2 {
            if w.air(x + dx, y - 1, z + dz) && w.air(x + dx, y - 2, z + dz) {
                return;
            }
        }
    }
    let (lg, lv) = (db::mk(db::LOG, 3), db::mk(db::LEAVES, 3));
    w.set(x, y, z, blk::DIRT as Db);
    w.set(x, y + 1, z, lg);
    w.set(x, y + 2, z, lg);
    for (dx, dy, dz) in [(1, 2, 0), (-1, 2, 0), (0, 2, 1), (0, 2, -1), (0, 3, 0)] {
        w.set(x + dx, y + dy, z + dz, lv);
    }
    let a = r.next_int_bounded(12);
    let age = r.next_int_bounded(4);
    if age < 3 && a < 4 {
        // (side, meta low bits): -x 3, +x 1, -z 0, +z 2; age 0 → 8, 1 → 4, 2 → 0
        let (dx, dz, m) = [(-1, 0, 3u8), (1, 0, 1), (0, -1, 0), (0, 1, 2)][a as usize];
        let hi = [8u8, 4, 0][age as usize];
        w.set(x + dx, y + 1, z + dz, db::mk(db::COCOA, hi | m));
    }
}

/// BOP `WorldGenRainforestTree1.generate` (exact; the trunk loop re-sets the whole crown each step and clears
/// two trunk blocks with air)
#[allow(clippy::too_many_arguments)]
fn rainforest_tree1(
    w: &mut DecoWorld,
    r: &mut impl DRng,
    lg: Db,
    lv: Db,
    min: i32,
    rand: i32,
    x: i32,
    y: i32,
    z: i32,
) {
    w.mark("rainforest1", x, y, z);
    let l = r.next_int_bounded(rand) + min;
    if !(y >= 1 && y + l < 256) || !super::gens::tree_space(w, x, y, z, l) {
        return;
    }
    let below = w.get(x, y - 1, z);
    if !(sustains_sapling(below) && y < 256 - l - 1) {
        return;
    }
    if is(below, blk::GRASS) || is(below, db::FARMLAND) {
        w.set(x, y - 1, z, blk::DIRT as Db);
    }
    for k1 in y - 3 + l..=y + l {
        let i3 = k1 - (y + l);
        let l1 = 1 - i3;
        for xx in x - l1..=x + l1 {
            for zz in z - l1..=z + l1 {
                if (xx - x).abs() != l1
                    || (zz - z).abs() != l1
                    || (r.next_int_bounded(2) != 0 && i3 != 0)
                {
                    let b = w.get(xx, k1, zz);
                    if is_air(b) || is_leaves(b) {
                        w.set(xx, k1, zz, lv);
                    }
                }
            }
        }
    }
    let m = |d: u8| db::mk(db::id(lg), db::meta(lg) + d);
    for k1 in 0..l {
        let b = w.get(x, y + k1, z);
        if is_air(b) || is_leaves(b) {
            w.set(x, y + k1, z, lg);
            for (dx, dy, dz, d) in [
                (-3, -3, 0, 4u8),
                (3, -3, 0, 4),
                (0, -3, -3, 8),
                (0, -3, 3, 8),
                (-2, -4, 0, 0),
                (2, -4, 0, 0),
                (0, -4, -2, 0),
                (0, -4, 2, 0),
                (-2, -5, 0, 0),
                (2, -5, 0, 0),
                (0, -5, -2, 0),
                (0, -5, 2, 0),
                (-1, -6, 0, 4),
                (1, -6, 0, 4),
                (0, -6, -1, 8),
                (0, -6, 1, 8),
            ] {
                w.set(x + dx, y + l + dy, z + dz, m(d));
            }
            for dy in [-3, -2, -1, 0] {
                w.set(x, y + l + dy, z, lv);
            }
            w.set(x, y + l - 4, z, blk::AIR as Db);
            w.set(x, y + l - 5, z, blk::AIR as Db);
            for (dx, dz, d) in [(-1, 0, 4u8), (1, 0, 4), (0, -1, 8), (0, 1, 8)] {
                w.set(x + dx, y + l - 3, z + dz, m(d));
            }
            w.set(x, y + l - 2, z, lg);
        }
    }
}

/// BOP `WorldGenBulbTree.generate` (exact): no grass → dirt under the trunk; the eight fixed leaf rings are set on
/// every trunk step (unconditionally); `growVines` keeps placing vines downward while the block below is *not* air.
#[allow(clippy::too_many_arguments)]
fn bulb_tree(
    w: &mut DecoWorld,
    r: &mut impl DRng,
    lg: Db,
    lv: Db,
    min: i32,
    rand: i32,
    vines: bool,
    x: i32,
    y: i32,
    z: i32,
) {
    w.mark("bulb", x, y, z);
    let l = r.next_int_bounded(rand) + min;
    if !(y >= 1 && y + l < 256) || !super::gens::tree_space(w, x, y, z, l) {
        return;
    }
    if !(sustains_sapling(w.get(x, y - 1, z)) && y < 256 - l - 1) {
        return;
    }
    for yy in y - 3 + l..=y + l {
        let i3 = yy - (y + l);
        let l1 = 1 - i3 / 3;
        for xx in x - l1..=x + l1 {
            for zz in z - l1..=z + l1 {
                if (xx - x).abs() != l1
                    || (zz - z).abs() != l1
                    || (r.next_int_bounded(2) != 0 && i3 != 0)
                {
                    let b = w.get(xx, yy, zz);
                    if is_air(b) || is_leaves(b) {
                        w.set(xx, yy, zz, lv);
                    }
                }
            }
        }
    }
    const RINGS: [(i32, i32, i32); 28] = [
        (-1, -4, 0),
        (1, -4, 0),
        (0, -4, -1),
        (0, -4, 1),
        (-1, -5, 1),
        (-1, -5, -1),
        (1, -5, 1),
        (1, -5, -1),
        (-1, -5, 0),
        (1, -5, 0),
        (0, -5, -1),
        (0, -5, 1),
        (-1, -6, 0),
        (1, -6, 0),
        (0, -6, -1),
        (0, 0, 1),
        (-1, -7, 1),
        (-1, -7, -1),
        (1, -7, 1),
        (1, -7, -1),
        (-1, -7, 0),
        (1, -7, 0),
        (0, -7, -1),
        (0, -7, 1),
        (-1, -8, 0),
        (1, -8, 0),
        (0, -8, -1),
        (0, -8, 1),
    ];
    let vine = |m: u8| db::mk(db::VINE, m);
    for k in 0..l {
        let b = w.get(x, y + k, z);
        if is_air(b) || is_leaves(b) {
            w.set(x, y + k, z, lg);
            // the l − 6 ring's (0, +1) is `y + l` in the source (typo kept: (0, 0, 1))
            for (dx, dy, dz) in RINGS {
                w.set(x + dx, y + l + dy, z + dz, lv);
            }
            if vines && k > 0 {
                for (dx, dz, m) in [(-1, 0, 8u8), (1, 0, 2), (0, -1, 1), (0, 1, 4)] {
                    if r.next_int_bounded(3) > 0 && w.air(x + dx, y + k, z + dz) {
                        w.set(x + dx, y + k, z + dz, vine(m));
                    }
                }
            }
        }
    }
    if !vines {
        return;
    }
    for yy in y - 3 + l..=y + l {
        let i3 = yy - (y + l);
        let l1 = 2 - i3 / 2;
        for xx in x - l1..=x + l1 {
            for zz in z - l1..=z + l1 {
                if is_leaves(w.get(xx, yy, zz)) {
                    for (dx, dz, m) in [(-1, 0, 8u8), (1, 0, 2), (0, -1, 1), (0, 1, 4)] {
                        if r.next_int_bounded(4) == 0 && w.air(xx + dx, yy, zz + dz) {
                            // growVines
                            let (vx, vz) = (xx + dx, zz + dz);
                            w.set(vx, yy, vz, vine(m));
                            let (mut vy, mut n) = (yy, 4);
                            loop {
                                vy -= 1;
                                if w.air(vx, vy, vz) || n <= 0 {
                                    break;
                                }
                                w.set(vx, vy, vz, vine(m));
                                n -= 1;
                            }
                        }
                    }
                }
            }
        }
    }
    if r.next_int_bounded(5) == 0 && l > 5 {
        // Direction.offsetX / offsetZ [rotateOpposite[i3]]
        const OFF: [(i32, i32); 4] = [(0, -1), (1, 0), (0, 1), (-1, 0)];
        for k in 0..2 {
            for i3 in 0..4u8 {
                if r.next_int_bounded(4 - k) == 0 {
                    let age = r.next_int_bounded(3) as u8;
                    let (dx, dz) = OFF[i3 as usize];
                    w.set(
                        x + dx,
                        y + l - 5 + k,
                        z + dz,
                        db::mk(db::COCOA, (age << 2) | i3),
                    );
                }
            }
        }
    }
}

/// exact `WorldGenOriginalTree` (= vanilla `WorldGenTrees` with BOP blocks, `nextInt(rand) + min` height, a
/// `growVines` that stops at the first air block, optional fruit draws)
#[allow(clippy::too_many_arguments)]
fn original_tree(
    w: &mut DecoWorld,
    r: &mut impl DRng,
    lg: Db,
    lv: Db,
    min: i32,
    rand: i32,
    vines: bool,
    fruit: bool,
    x: i32,
    y: i32,
    z: i32,
) {
    let h = r.next_int_bounded(rand) + min;
    if !(y >= 1 && y + h < 256) || !super::gens::tree_space(w, x, y, z, h) {
        return;
    }
    let below = w.get(x, y - 1, z);
    if !(sustains_sapling(below) && y < 256 - h - 1) {
        return;
    }
    if is(below, blk::GRASS) {
        w.set(x, y - 1, z, blk::DIRT as Db);
    }
    for yy in y - 3 + h..=y + h {
        let rel = yy - (y + h);
        let rad = 1 - rel / 2;
        for xx in x - rad..=x + rad {
            for zz in z - rad..=z + rad {
                if (xx - x).abs() != rad
                    || (zz - z).abs() != rad
                    || (r.next_int_bounded(2) != 0 && rel != 0)
                {
                    let b = w.get(xx, yy, zz);
                    if is_air(b) || is_leaves(b) {
                        w.set(xx, yy, zz, lv);
                    }
                }
            }
        }
    }
    let vine = |m: u8| db::mk(db::VINE, m);
    for k in 0..h {
        let b = w.get(x, y + k, z);
        if is_air(b) || is_leaves(b) {
            w.set(x, y + k, z, lg);
            if vines && k > 0 {
                for (dx, dz, m) in [(-1, 0, 8u8), (1, 0, 2), (0, -1, 1), (0, 1, 4)] {
                    if r.next_int_bounded(3) > 0 && w.air(x + dx, y + k, z + dz) {
                        w.set(x + dx, y + k, z + dz, vine(m));
                    }
                }
            }
        }
    }
    if fruit {
        let n = r.next_int_bounded(4);
        for _ in 0..n {
            r.next_int_bounded(4);
            r.next_int_bounded(4);
        }
    }
    if vines {
        for yy in y - 3 + h..=y + h {
            let rad = 2 - (yy - (y + h)) / 2;
            for xx in x - rad..=x + rad {
                for zz in z - rad..=z + rad {
                    if is_leaves(w.get(xx, yy, zz)) {
                        for (dx, dz, m) in [(-1, 0, 8u8), (1, 0, 2), (0, -1, 1), (0, 1, 4)] {
                            if r.next_int_bounded(4) == 0 && is_air(w.get(xx + dx, yy, zz + dz)) {
                                // BOP's growVines returns at the first air block below: only the first vine
                                w.set(xx + dx, yy, zz + dz, vine(m));
                                if !is_air(w.get(xx + dx, yy - 1, zz + dz)) {
                                    let mut yv = yy;
                                    let mut n = 4;
                                    loop {
                                        yv -= 1;
                                        if is_air(w.get(xx + dx, yv, zz + dz)) || n <= 0 {
                                            break;
                                        }
                                        w.set(xx + dx, yv, zz + dz, vine(m));
                                        n -= 1;
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }
        if r.next_int_bounded(5) == 0 && h > 5 {
            for k in 0..2 {
                for _ in 0..4 {
                    if r.next_int_bounded(4 - k) == 0 {
                        r.next_int_bounded(3);
                    }
                }
            }
        }
    }
}

/// exact `WorldGenGiantFlower(meta)` (no draws)
fn giant_flower(w: &mut DecoWorld, t: u8, x: i32, mut y: i32, z: i32) {
    while is_air(w.get(x, y, z)) && y > 2 {
        y -= 1;
    }
    if !is(w.get(x, y, z), blk::BOP_LONG_GRASS) {
        return;
    }
    for a in -2..=2 {
        for b in -2..=2 {
            if is_air(w.get(x + a, y - 1, z + b)) && is_air(w.get(x + a, y - 2, z + b)) {
                return;
            }
        }
    }
    w.set(x, y, z, blk::DIRT as Db);
    let stem = db::mk(db::BOP_LOGS3, 3);
    for k in 1..=5 {
        w.set(x, y + k, z, stem);
    }
    let p = db::mk(db::BOP_PETALS, t);
    const RED: &[(i32, i32, i32)] = &[
        (-1, 5, 0),
        (1, 5, 0),
        (0, 5, -1),
        (0, 5, 1),
        (0, 6, 0),
        (-1, 6, 0),
        (1, 6, 0),
        (0, 6, -1),
        (0, 6, 1),
        (1, 6, 1),
        (1, 6, -1),
        (-1, 6, 1),
        (-1, 6, -1),
        (2, 6, 0),
        (-2, 6, 0),
        (0, 6, 2),
        (0, 6, -2),
        (1, 7, 1),
        (1, 7, -1),
        (-1, 7, 1),
        (-1, 7, -1),
        (2, 7, 0),
        (-2, 7, 0),
        (0, 7, 2),
        (0, 7, -2),
        (2, 7, 2),
        (2, 7, -2),
        (-2, 7, 2),
        (-2, 7, -2),
        (2, 8, 0),
        (-2, 8, 0),
        (0, 8, 2),
        (0, 8, -2),
        (3, 9, 0),
        (-3, 9, 0),
        (0, 9, 3),
        (0, 9, -3),
    ];
    const YELLOW: &[(i32, i32, i32)] = &[
        (-1, 5, 0),
        (1, 5, 0),
        (0, 5, -1),
        (0, 5, 1),
        (2, 5, 2),
        (2, 5, -2),
        (-2, 5, 2),
        (-2, 5, -2),
        (0, 6, 0),
        (-1, 6, 0),
        (1, 6, 0),
        (0, 6, -1),
        (0, 6, 1),
        (1, 6, 1),
        (1, 6, -1),
        (-1, 6, 1),
        (-1, 6, -1),
        (2, 6, 0),
        (-2, 6, 0),
        (0, 6, 2),
        (0, 6, -2),
        (0, 7, 0),
        (3, 7, 0),
        (-3, 7, 0),
        (0, 7, 3),
        (0, 7, -3),
    ];
    for &(dx, dy, dz) in if t == 0 { RED } else { YELLOW } {
        w.set(x + dx, y + dy, z + dz, p);
    }
}
