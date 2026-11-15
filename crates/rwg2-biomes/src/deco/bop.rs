//! BiomesOPlenty 2.1.0.2308 overworld decoration (`BOPBiome.decorate` → `BOPBiomeDecorator.decorateChunk`), as
//! RWG2's `RealisticBiomeSupport` runs it above strength 0.3, plus the Forge / GTNH event listeners that draw from
//! the decorator's Random.
//!
//! The decorator wraps the shared Random in `RandomForcedPositiveOwned`: `nextInt(n > 0)` goes to the shared
//! Random, `nextInt(n <= 0)` returns 0 without a draw, everything else (`nextFloat`, `nextDouble`,
//! `nextBoolean`) to its own `Random` seeded from the shared one's state at `decorateChunk` (Hodgepodge reuses the
//! instance and re-seeds it the same way). Forge's patched `BiomeDecorator` adds `nextInt(i <= 1) = 0` (no draw)
//! for the height-bounded y draws.
//!
//! Order (Forge 1.7.10 `genDecorations`, then `BOPBiomeDecorator`): Pre, ores (dirt 20, gravel 10; GT denies the
//! rest), SAND event (NHCore zinc gravel `nextInt(150)`), sand / clay / gravel-as-sand at water, trees, big
//! mushrooms, FLOWERS event (Botania: 2 patches + 40 underground mushrooms) + flowers, GRASS, dead bush, lily pad,
//! mushrooms (always the two extra 1/4, 1/8 rolls), reeds (+10 always), PUMPKIN (denied by BOP's handler when the
//! biome at chunk + 16 has `generatePumpkins = false`), cacti, LAKE (springs; denied when the biome at chunk + 16
//! is any BOP biome), then the BOP features in `Class.getFields()` order (OverworldBiomeFeatures declaration
//! order, then BiomeFeaturesBase, then `bopFlowersPerChunk` (FLOWERS event again) and `bopGrassPerChunk`).
//! `getRandomWeightedWorldGenerator` picks the flower / grass generator with `Math.random()`: every candidate
//! draws the same, so only the block type is unknown (the heaviest candidate is used).

use super::bop_trees::TS;
use super::gens::G;
use super::*;
use crate::java_random::JavaRandom;

/// `RandomForcedPositiveOwned` over the shared Random.
pub struct Bop<'r> {
    pub p: &'r mut JavaRandom,
    pub own: JavaRandom,
    /// a vanilla `BiomeDecorator` on the plain Random: floats / doubles come from the shared Random too
    pub vanilla: bool,
}

impl Bop<'_> {
    /// `nextInt(n)`: the shared Random for n > 0, else 0
    fn ni(&mut self, n: i32) -> i32 {
        if n > 0 {
            self.p.next_int_bounded(n)
        } else {
            0
        }
    }
    /// `nextFloat` (own Random under BOP's wrapper)
    fn of(&mut self) -> f32 {
        if self.vanilla {
            self.p.next_float()
        } else {
            self.own.next_float()
        }
    }
    /// `nextDouble` (own Random under BOP's wrapper)
    fn od(&mut self) -> f64 {
        if self.vanilla {
            self.p.next_double()
        } else {
            self.own.next_double()
        }
    }
    /// Forge `BiomeDecorator.nextInt`: 0 without a draw for n <= 1
    fn fi(&mut self, n: i32) -> i32 {
        if n <= 1 {
            0
        } else {
            self.p.next_int_bounded(n)
        }
    }
}

impl DRng for Bop<'_> {
    fn next_int_bounded(&mut self, n: i32) -> i32 {
        self.ni(n)
    }
    fn next_float(&mut self) -> f32 {
        self.of()
    }
    fn next_double(&mut self) -> f64 {
        self.od()
    }
    fn next_boolean(&mut self) -> bool {
        if self.vanilla {
            self.p.next_boolean()
        } else {
            self.own.next_boolean()
        }
    }
    fn next_long(&mut self) -> i64 {
        if self.vanilla {
            self.p.next_long()
        } else {
            self.own.next_long()
        }
    }
}

/// A flora block the BOP generators place (block, meta).
#[derive(Clone, Copy, Debug)]
pub enum Flora {
    /// `BlockBOPFoliage` / `BlockBOPFlower` / `BlockBOPFlower2` default rule (`BOPBlockWorldDecor.canReplace`:
    /// a non-air block below that sustains plains plants)
    Decor(u8, u8),
    /// `BlockBOPMushroom` (grass, dirt, mycelium below)
    Mushroom(u8),
    /// `BlockBOPPlant` 15 (root: grass / dirt block above)
    Root,
    /// `BlockBOPPlant` 14 (reed: water below, no water two below, lit)
    Reed,
    /// miner's delight (`BlockBOPFlower2` 6: stone below)
    MinersDelight,
    /// vanilla tall grass (`BlockBush.canPlaceBlockAt`: replaceable here, grass / dirt / farmland below)
    TallGrass(u8),
    /// `BlockBOPFoliage` 0 (algae: on water)
    Algae,
}

impl Flora {
    fn block(self) -> Db {
        match self {
            Flora::Decor(b, m) => db::mk(b, m),
            Flora::Mushroom(m) => db::mk(db::BOP_MUSHROOMS, m),
            Flora::Root => db::mk(db::BOP_PLANTS, 15),
            Flora::Reed => db::mk(db::BOP_PLANTS, 14),
            Flora::MinersDelight => db::mk(db::BOP_FLOWERS2, 6),
            Flora::TallGrass(m) => db::mk(db::TALLGRASS, m),
            Flora::Algae => db::mk(db::BOP_FOLIAGE, 0),
        }
    }

    /// `canReplace(world, x, y, z, 0, stack)` (the generators test `isAirBlock` first)
    fn can_place(self, w: &mut DecoWorld, x: i32, y: i32, z: i32) -> bool {
        let below = w.get(x, y - 1, z);
        match self {
            Flora::Decor(..) => !is_air(below) && sustains_sapling(below),
            Flora::Mushroom(_) => is_earth(below) || is(below, blk::MYCELIUM),
            Flora::Root => is_earth(w.get(x, y + 1, z)),
            Flora::Reed => w.lit(x, y, z) && is_water(below) && !is_water(w.get(x, y - 2, z)),
            Flora::MinersDelight => !is_air(below) && is(below, blk::STONE),
            Flora::TallGrass(_) => sustains_sapling(below),
            Flora::Algae => is_water(below),
        }
    }
}

/// A weighted flower / grass generator (`weightedFlowerGen` / `weightedGrassGen` entry).
#[derive(Clone, Copy, Debug)]
pub enum FloraGen {
    /// `WorldGenBOPFlora(block, meta, groupCount)`
    Flora(Flora, i32),
    /// `WorldGenBOPDoubleFlora` (bottom, top, groupCount)
    Double(Db, Db, i32),
    /// `WorldGenBOPTallGrass` / `WorldGenTallGrass` (walk down, 128 attempts)
    Grass(Flora),
    /// empty weighted list: the manager places nothing
    Nothing,
}

/// `getRandomWorldGenForGrass` override: `random.nextInt(n) == 0 ? a : b` chain on the shared Random.
#[derive(Debug)]
pub enum FS {
    Leaf(FloraGen),
    Pick(i32, &'static FS, &'static FS),
}

impl FS {
    fn pick(&'static self, b: &mut Bop) -> FloraGen {
        let mut t = self;
        loop {
            match t {
                FS::Leaf(g) => return *g,
                FS::Pick(n, a, c) => t = if b.ni(*n) == 0 { a } else { c },
            }
        }
    }
}

fn run_flora(w: &mut DecoWorld, b: &mut Bop, g: FloraGen, x: i32, y: i32, z: i32) {
    match g {
        FloraGen::Flora(f, n) => flora(w, b, f, n, x, y, z),
        FloraGen::Double(lo, hi, n) => double_flora(w, b, lo, hi, n, x, y, z),
        FloraGen::Grass(f) => tall_grass(w, b, f, x, y, z),
        FloraGen::Nothing => {}
    }
}

/// `WorldGenBOPFlora.generate` (group attempts, ±7 / ±3)
fn flora(w: &mut DecoWorld, b: &mut Bop, f: Flora, group: i32, x: i32, y: i32, z: i32) {
    w.mark("bopflora", x, y, z);
    for _ in 0..group {
        let a = x + b.ni(8) - b.ni(8);
        let c = y + b.ni(4) - b.ni(4);
        let d = z + b.ni(8) - b.ni(8);
        if w.air(a, c, d) && c < 255 && f.can_place(w, a, c, d) {
            w.set(a, c, d, f.block());
        }
    }
}

/// `WorldGenBOPTallGrass` / vanilla `WorldGenTallGrass` / `WorldGenBOPUndergroundDecoration`: walk down through
/// air and leaves, then 128 attempts.
fn tall_grass(w: &mut DecoWorld, b: &mut Bop, f: Flora, x: i32, mut y: i32, z: i32) {
    w.mark("boptallgrass", x, y, z);
    loop {
        let bl = w.get(x, y, z);
        if !(is_air(bl) || is_leaves(bl)) {
            break;
        }
        y -= 1;
        if y <= 0 {
            break;
        }
    }
    for _ in 0..128 {
        let a = x + b.ni(8) - b.ni(8);
        let c = y + b.ni(4) - b.ni(4);
        let d = z + b.ni(8) - b.ni(8);
        if w.air(a, c, d) && f.can_place(w, a, c, d) {
            w.set(a, c, d, f.block());
        }
    }
}

/// `WorldGenBOPDoubleFlora` (non-vanilla: bottom + top block)
#[allow(clippy::too_many_arguments)]
fn double_flora(
    w: &mut DecoWorld,
    b: &mut Bop,
    bot: Db,
    top: Db,
    group: i32,
    x: i32,
    y: i32,
    z: i32,
) {
    w.mark("bopdouble", x, y, z);
    for _ in 0..group {
        let a = x + b.ni(8) - b.ni(8);
        let c = y + b.ni(4) - b.ni(4);
        let d = z + b.ni(8) - b.ni(8);
        if w.air(a, c, d) {
            let below = w.get(a, c - 1, d);
            if !is_air(below) && sustains_sapling(below) {
                w.set(a, c, d, bot);
                w.set(a, c + 1, d, top);
            }
        }
    }
}

/// `WorldGenWaterReeds`
fn water_reeds(w: &mut DecoWorld, b: &mut Bop, x: i32, y: i32, z: i32) {
    w.mark("bopreeds", x, y, z);
    for _ in 0..128 {
        let a = x + b.ni(8) - b.ni(8);
        let c = y + b.ni(2) - b.ni(2);
        let d = z + b.ni(8) - b.ni(8);
        if w.air(a, c, d)
            && Flora::Reed.can_place(w, a, c, d)
            && (-3..=4).any(|i| !is_water(w.get(a - i, c - 1, d - i)))
        {
            w.set(a, c, d, Flora::Reed.block());
        }
    }
}

/// vanilla `WorldGenSand` / `WorldGenClay`, BOP `WorldGenWaterside`: at water, a disc replacing `from`.
#[allow(clippy::too_many_arguments)]
fn waterside(
    w: &mut DecoWorld,
    b: &mut Bop,
    block: Db,
    radius: i32,
    dy: i32,
    from: fn(Db) -> bool,
    x: i32,
    y: i32,
    z: i32,
) {
    if !is_water(w.get(x, y, z)) {
        return;
    }
    w.mark("waterside", x, y, z);
    let l = b.ni(radius - 2) + 2;
    for i in x - l..=x + l {
        for j in z - l..=z + l {
            let (dx, dz) = (i - x, j - z);
            if dx * dx + dz * dz <= l * l {
                for k in y - dy..=y + dy {
                    if from(w.get(i, k, j)) {
                        w.set(i, k, j, block);
                    }
                }
            }
        }
    }
}

/// `WorldGenBOPBlob` (cobble rock piles; grass / dirt / stone floor)
fn bop_blob(
    w: &mut DecoWorld,
    b: &mut Bop,
    block: Db,
    size: i32,
    mut x: i32,
    mut y: i32,
    mut z: i32,
) {
    w.mark("bopblob", x, y, z);
    while y > 3 {
        if !w.air(x, y - 1, z) {
            let bl = w.get(x, y - 1, z);
            if is_earth(bl) || is(bl, blk::STONE) {
                break;
            }
        }
        y -= 1;
    }
    if y <= 3 {
        return;
    }
    let k2 = size;
    let mut l = 0;
    while k2 >= 0 && l < 3 {
        let i1 = k2 + b.ni(2);
        let j1 = k2 + b.ni(2);
        let k1 = k2 + b.ni(2);
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
        x += -(k2 + 1) + b.ni(2 + k2 * 2);
        z += -(k2 + 1) + b.ni(2 + k2 * 2);
        y -= b.ni(2);
        l += 1;
    }
}

/// `WorldGenBOPTaiga2(wood, leaves, metaWood, metaLeaves, false, minH, randH, altNo, metaFruit)` (fruit < 0: none)
#[allow(clippy::too_many_arguments)]
pub(crate) fn bop_taiga2(
    w: &mut DecoWorld,
    b: &mut impl DRng,
    wood: Db,
    leaves: Db,
    min_h: i32,
    rand_h: i32,
    alt: i32,
    fruit: i8,
    x: i32,
    y: i32,
    z: i32,
) {
    w.mark("boptaiga2", x, y, z);
    let l = b.next_int_bounded(rand_h) + min_h;
    let (i1, k1) = match alt {
        1 => (2 + b.next_int_bounded(4), 2 + b.next_int_bounded(4)),
        2 => (4 + b.next_int_bounded(4), 2),
        3 => (1 - b.next_int_bounded(4), 2 + b.next_int_bounded(2)),
        4 => (8 + b.next_int_bounded(4), 2 + b.next_int_bounded(2)),
        5 => (2, 2 + b.next_int_bounded(2)),
        6 => (2 + b.next_int_bounded(4), 2 + b.next_int_bounded(2)),
        7 => (2, 3),
        8 => (2 + b.next_int_bounded(3), 2 + b.next_int_bounded(2)),
        _ => (1 + b.next_int_bounded(2), 2 + b.next_int_bounded(2)),
    };
    let j1 = l - i1;
    if !(y >= 1 && y + l < 256) {
        return;
    }
    for l1 in y..=y + 1 + l {
        let r = if l1 - y < i1 { 0 } else { k1 };
        for i2 in x - r..=x + r {
            for j2 in z - r..=z + r {
                if !(0..256).contains(&l1) {
                    return;
                }
                let bl = w.get(i2, l1, j2);
                if !is_air(bl) && !is_leaves(bl) {
                    return;
                }
            }
        }
    }
    let below = w.get(x, y - 1, z);
    if !(sustains_sapling(below) && y < 256 - l - 1) {
        return;
    }
    if is(below, blk::GRASS) || is(below, db::FARMLAND) {
        w.set(x, y - 1, z, blk::DIRT as Db);
    }
    let mut l3 = b.next_int_bounded(2);
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
    let i4 = b.next_int_bounded(3);
    for k2 in 0..l - i4 {
        let bl = w.get(x, y + k2, z);
        if is_air(bl) || is_leaves(bl) {
            w.set(x, y + k2, z, wood);
        }
    }
    if fruit >= 0 {
        let fr = b.next_int_bounded(10);
        for _ in 0..fr {
            let f1 = b.next_int_bounded(4);
            let f2 = l - 5 - b.next_int_bounded(l / 2 + l / 4);
            let f3 = b.next_int_bounded(4);
            let (fx, fy, fz) = (x - 2 + f1, y + f2, z - 2 + f3);
            if w.air(fx, fy, fz) && is_leaves(w.get(fx, fy + 1, fz)) {
                w.set(fx, fy, fz, db::mk(db::BOP_FRUIT, fruit as u8));
            }
        }
    }
}

/// `WorldGenBOPTaiga3(wood, leaves, metaWood, metaLeaves, notify, minHeight, randomHeight, altNo, metaFruit)`:
/// plus-shaped (5-block) leaves and trunk, all five soil blocks must sustain a sapling.
#[allow(clippy::too_many_arguments)]
pub(crate) fn bop_taiga3(
    w: &mut DecoWorld,
    b: &mut impl DRng,
    wood: Db,
    leaves: Db,
    min_h: i32,
    rand_h: i32,
    alt: i32,
    fruit: i8,
    x: i32,
    y: i32,
    z: i32,
) {
    w.mark("boptaiga3", x, y, z);
    let l = b.next_int_bounded(rand_h) + min_h;
    let (i1, k1) = match alt {
        1 => (b.next_int_bounded(4) + 8, 4),
        2 => (b.next_int_bounded(5) + 5, 3),
        _ => (b.next_int_bounded(5) + 10, 4),
    };
    let j1 = l - i1;
    if !(y >= 1 && y + l < 256) {
        return;
    }
    for l1 in y..=y + 1 + l {
        let r = if l1 - y < i1 { 0 } else { k1 };
        for i2 in x - r..=x + r {
            for j2 in z - r..=z + r {
                if !(0..256).contains(&l1) {
                    return;
                }
                let bl = w.get(i2, l1, j2);
                if !is_air(bl) && !is_leaves(bl) {
                    return;
                }
            }
        }
    }
    let soil = [(x, z), (x + 1, z), (x - 1, z), (x, z + 1), (x, z - 1)];
    if !soil
        .iter()
        .all(|&(sx, sz)| sustains_sapling(w.get(sx, y - 1, sz)))
        || y >= 256 - l - 1
    {
        return;
    }
    for &(sx, sz) in &soil {
        let bl = w.get(sx, y - 1, sz);
        if is(bl, blk::GRASS) || is(bl, db::FARMLAND) {
            w.set(sx, y - 1, sz, blk::DIRT as Db);
        }
    }
    let plus = |w: &mut DecoWorld, px: i32, py: i32, pz: i32, v: Db| {
        for (dx, dz) in [(0, 0), (1, 0), (-1, 0), (0, 1), (0, -1)] {
            w.set(px + dx, py, pz + dz, v);
        }
    };
    let mut l3 = b.next_int_bounded(2);
    let mut i2 = 1;
    let mut b0 = 0;
    for i4 in 0..=j1 {
        let k2 = y + l - i4;
        for l2 in x - l3..=x + l3 {
            let i3 = l2 - x;
            for j3 in z - l3..=z + l3 {
                let k3 = j3 - z;
                if (i3.abs() != l3 || k3.abs() != l3 || l3 <= 0) && !opaque(w.get(l2, k2, j3)) {
                    plus(w, l2, k2, j3, leaves);
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
    let i4 = b.next_int_bounded(3);
    for k2 in 0..l - i4 {
        let bl = w.get(x, y + k2, z);
        if is_air(bl) || is_leaves(bl) {
            plus(w, x, y + k2, z, wood);
        }
    }
    if fruit >= 0 {
        let fr = b.next_int_bounded(10);
        for _ in 0..fr {
            let f1 = b.next_int_bounded(4);
            let f2 = l - 5 - b.next_int_bounded(l / 2 + l / 4);
            let f3 = b.next_int_bounded(4);
            let (fx, fy, fz) = (x - 2 + f1, y + f2, z - 2 + f3);
            if w.air(fx, fy, fz) && is_leaves(w.get(fx, fy + 1, fz)) {
                w.set(fx, fy, fz, db::mk(db::BOP_FRUIT, fruit as u8));
            }
        }
    }
}

/// `WorldGenBOPSwampTree(wood, leaves, metaWood, metaLeaves, minHeight, randomHeight[, vineBlock, vineMeta])`:
/// sinks through water, `WorldGenSwamp`-style canopy with random corners, vines (side meta, or `vine` with its own
/// meta when the tree names one).
#[allow(clippy::too_many_arguments)]
pub(crate) fn bop_swamp_tree(
    w: &mut DecoWorld,
    b: &mut impl DRng,
    wood: Db,
    leaves: Db,
    min_h: i32,
    rand_h: i32,
    vine: Option<Db>,
    x: i32,
    mut y: i32,
    z: i32,
) {
    w.mark("bopswamp", x, y, z);
    let l = b.next_int_bounded(rand_h) + min_h;
    while y > 0 && matches!(mat(w.get(x, y - 1, z)), Mat::Water) {
        y -= 1;
    }
    if !(y >= 1 && y + l < 256) {
        return;
    }
    for i1 in y..=y + 1 + l {
        let r = if i1 >= y + 1 + l - 2 {
            3
        } else if i1 == y {
            0
        } else {
            1
        };
        for j1 in x - r..=x + r {
            for k1 in z - r..=z + r {
                if !(0..256).contains(&i1) {
                    return;
                }
                let bl = w.get(j1, i1, k1);
                if !(is_air(bl) || is_leaves(bl)) && (!is_water(bl) || i1 > y) {
                    return;
                }
            }
        }
    }
    let below = w.get(x, y - 1, z);
    if !sustains_sapling(below) || y >= 256 - l - 1 {
        return;
    }
    if is(below, blk::GRASS) || is(below, db::FARMLAND) {
        w.set(x, y - 1, z, blk::DIRT as Db);
    }
    for k2 in y - 3 + l..=y + l {
        let j1 = k2 - (y + l);
        let k1 = 2 - j1 / 2;
        for l2 in x - k1..=x + k1 {
            let l1 = l2 - x;
            for i2 in z - k1..=z + k1 {
                let j2 = i2 - z;
                if (l1.abs() != k1 || j2.abs() != k1 || b.next_int_bounded(2) != 0 && j1 != 0)
                    && !opaque(w.get(l2, k2, i2))
                {
                    w.set(l2, k2, i2, leaves);
                }
            }
        }
    }
    for k2 in 0..l {
        let bl = w.get(x, y + k2, z);
        if is_air(bl) || is_leaves(bl) || is_water(bl) {
            w.set(x, y + k2, z, wood);
        }
    }
    let hang = |w: &mut DecoWorld, vx: i32, mut vy: i32, vz: i32, side: u8| {
        let v = vine.unwrap_or(db::mk(db::VINE, side));
        w.set(vx, vy, vz, v);
        let mut n = 4;
        loop {
            vy -= 1;
            if !w.air(vx, vy, vz) || n <= 0 {
                return;
            }
            w.set(vx, vy, vz, v);
            n -= 1;
        }
    };
    for k2 in y - 3 + l..=y + l {
        let j1 = k2 - (y + l);
        let k1 = 2 - j1 / 2;
        for l2 in x - k1..=x + k1 {
            for l1 in z - k1..=z + k1 {
                if is_leaves(w.get(l2, k2, l1)) {
                    for (dx, dz, side) in [(-1, 0, 8u8), (1, 0, 2), (0, -1, 1), (0, 1, 4)] {
                        if b.next_int_bounded(4) == 0 && w.air(l2 + dx, k2, l1 + dz) {
                            hang(w, l2 + dx, k2, l1 + dz, side);
                        }
                    }
                }
            }
        }
    }
}

/// `WorldGenMinable(block, 32)` on stone (draw order: float, 2 ints, 33 doubles).
fn minable(w: &mut DecoWorld, b: &mut Bop, block: Db, x: i32, y: i32, z: i32) {
    w.mark("minable", x, y, z);
    use crate::population::{mcos, msin};
    let n = 32;
    let f = b.of() * std::f32::consts::PI;
    let nf = n as f32;
    let d0 = ((x + 8) as f32 + msin(f) * nf / 8.0) as f64;
    let d1 = ((x + 8) as f32 - msin(f) * nf / 8.0) as f64;
    let d2 = ((z + 8) as f32 + mcos(f) * nf / 8.0) as f64;
    let d3 = ((z + 8) as f32 - mcos(f) * nf / 8.0) as f64;
    let d4 = (y + b.ni(3) - 2) as f64;
    let d5 = (y + b.ni(3) - 2) as f64;
    let nd = n as f64;
    let fl = |v: f64| v.floor() as i32;
    for l in 0..=n {
        let ld = l as f64;
        let d6 = d0 + (d1 - d0) * ld / nd;
        let d7 = d4 + (d5 - d4) * ld / nd;
        let d8 = d2 + (d3 - d2) * ld / nd;
        let d9 = b.od() * nd / 16.0;
        let s = (msin(l as f32 * std::f32::consts::PI / nf) + 1.0) as f64;
        let d10 = s * d9 + 1.0;
        let d11 = s * d9 + 1.0;
        for k2 in fl(d6 - d10 / 2.0)..=fl(d6 + d10 / 2.0) {
            let d12 = (k2 as f64 + 0.5 - d6) / (d10 / 2.0);
            if d12 * d12 >= 1.0 {
                continue;
            }
            for l2 in fl(d7 - d11 / 2.0)..=fl(d7 + d11 / 2.0) {
                let d13 = (l2 as f64 + 0.5 - d7) / (d11 / 2.0);
                if d12 * d12 + d13 * d13 >= 1.0 {
                    continue;
                }
                for i3 in fl(d8 - d10 / 2.0)..=fl(d8 + d10 / 2.0) {
                    let d14 = (i3 as f64 + 0.5 - d8) / (d10 / 2.0);
                    if d12 * d12 + d13 * d13 + d14 * d14 < 1.0 && is(w.get(k2, l2, i3), blk::STONE)
                    {
                        w.set(k2, l2, i3, block);
                    }
                }
            }
        }
    }
}

/// Per-biome decorator settings (only what the ported biomes use).
/// A generated (`bop_table`) BOP biome: decorator counts as declared, approximate tree shapes.
pub struct Gen {
    pub class: &'static str,
    pub trees: i32,
    pub grass: i32,
    pub flowers: i32,
    pub mushrooms: i32,
    pub reeds: i32,
    pub sand: i32,
    pub sand2: i32,
    pub clay: i32,
    pub dead_bush: i32,
    pub cacti: i32,
    pub lily: i32,
    pub big_mush: i32,
    pub pumpkins: bool,
    pub gem: bool,
    pub feats: &'static [(&'static str, i32)],
    pub bop_flowers: i32,
    pub bop_grass: i32,
    pub flower: FloraGen,
    pub grass_gen: FloraGen,
    /// `getRandomWorldGenForGrass` override (vanilla grass section), else plain tallgrass without a draw
    pub grass_pick: Option<&'static FS>,
    pub tree: &'static TS,
}

/// Which decorator a [`Cfg`] runs.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Vanilla {
    /// BOP (`RandomForcedPositiveOwned`, BOP features, gem ore)
    Bop,
    /// vanilla `BiomeGenPlains` (Sunflower Plains: plant noise, double plants, sunflowers)
    SunflowerPlains,
    /// vanilla `BiomeGenSnow` with spikes (Ice Plains Spikes)
    IceSpikes,
}

struct Cfg {
    vanilla: Vanilla,
    /// from `bop_table` (approximate): unported features are skipped instead of failing
    generic: bool,
    sand2: i32,
    clay: i32,
    dead_bush: i32,
    cacti: i32,
    lily: i32,
    big_mush: i32,
    trees: i32,
    tree: Tree,
    grass: i32,
    flowers: i32,
    mushrooms: i32,
    reeds: i32,
    sand: i32,
    gem: bool,
    pumpkins: bool,
    /// non-default `OverworldBiomeFeatures` / `BiomeFeaturesBase` counts
    feats: &'static [(&'static str, i32)],
    bop_flowers: i32,
    bop_grass: i32,
    flower: FloraGen,
    grass_gen: FloraGen,
    grass_pick: Option<&'static FS>,
}

#[derive(Clone, Copy)]
enum Tree {
    /// `BiomeGenBase.worldGeneratorTrees` (`WorldGenTrees(false)`)
    Oak,
    /// `WorldGenShrub(0, 0)`
    Shrub,
    /// `WorldGenBOPTaiga2(log, leaves, 0, 0, false, 6, 1, 7, -1)`
    PrairieTaiga,
    /// generated selector
    Sel(&'static TS),
}

fn cfg(class: &str) -> Option<Cfg> {
    let base = Cfg {
        vanilla: Vanilla::Bop,
        generic: false,
        sand2: 3,
        clay: 1,
        dead_bush: 0,
        cacti: 0,
        lily: 0,
        big_mush: 0,
        trees: 0,
        tree: Tree::Oak,
        grass: 0,
        flowers: 0,
        mushrooms: 0,
        reeds: 0,
        sand: 1,
        gem: false,
        pumpkins: true,
        feats: &[],
        bop_flowers: 0,
        bop_grass: 0,
        flower: FloraGen::Flora(Flora::Decor(db::BOP_FLOWERS, 0), 64),
        grass_gen: FloraGen::Grass(Flora::TallGrass(1)),
        grass_pick: None,
    };
    Some(match class {
        "BiomeGenFrostForest" => Cfg {
            trees: 3,
            grass: 1,
            flowers: -999,
            mushrooms: -999,
            gem: true,
            pumpkins: false,
            feats: &[
                ("shrubsPerChunk", 1),
                ("leafPilesPerChunk", 2),
                ("deadLeafPilesPerChunk", 4),
            ],
            bop_flowers: 3,
            bop_grass: 1,
            flower: FloraGen::Flora(Flora::Decor(db::BOP_FLOWERS2, 7), 64),
            grass_gen: FloraGen::Grass(Flora::TallGrass(1)),
            ..base
        },
        "BiomeGenPrairie" => Cfg {
            trees: 1,
            tree: Tree::PrairieTaiga,
            gem: true,
            feats: &[
                ("portobellosPerChunk", 2),
                ("berryBushesPerChunk", 2),
                ("shrubsPerChunk", 3),
                ("waterReedsPerChunk", 4),
                ("leafPilesPerChunk", 15),
                ("flaxPerChunk", 1),
            ],
            bop_flowers: 30,
            bop_grass: 999,
            flower: FloraGen::Flora(Flora::Decor(db::BOP_FLOWERS2, 4), 64),
            // tallgrass 1.0 vs foliage 4 × 0.5: foliage is the likelier block
            grass_gen: FloraGen::Grass(Flora::Decor(db::BOP_FOLIAGE, 1)),
            ..base
        },
        "BiomeGenTundra" => Cfg {
            trees: 3,
            tree: Tree::Shrub,
            grass: 8,
            flowers: -999,
            reeds: -999,
            sand: 8,
            gem: true,
            feats: &[
                ("gravelPerChunk", 9),
                ("shrubsPerChunk", 2),
                ("waterReedsPerChunk", 2),
                ("rockpilesPerChunk", 2),
                ("leafPilesPerChunk", 5),
                ("deadLeafPilesPerChunk", 5),
            ],
            bop_flowers: 1,
            bop_grass: 8,
            flower: FloraGen::Flora(Flora::Decor(db::BOP_FLOWERS, 8), 64),
            grass_gen: FloraGen::Grass(Flora::Decor(db::BOP_FOLIAGE, 1)),
            ..base
        },
        // vanilla support biomes (RWG2 `RealisticBiomeSupport` over a vanilla base): BiomeDecorator defaults
        "BiomeGenPlains" => Cfg {
            vanilla: Vanilla::SunflowerPlains,
            trees: -999,
            flowers: 4,
            grass: 10,
            ..base
        },
        "BiomeGenSnow" => Cfg {
            vanilla: Vanilla::IceSpikes,
            flowers: 2,
            grass: 1,
            ..base
        },
        _ => {
            let g = super::bop_table::TABLE.iter().find(|g| g.class == class)?;
            Cfg {
                vanilla: Vanilla::Bop,
                generic: true,
                sand2: g.sand2,
                clay: g.clay,
                dead_bush: g.dead_bush,
                cacti: g.cacti,
                lily: g.lily,
                big_mush: g.big_mush,
                trees: g.trees,
                tree: Tree::Sel(g.tree),
                grass: g.grass,
                flowers: g.flowers,
                mushrooms: g.mushrooms,
                reeds: g.reeds,
                sand: g.sand,
                gem: g.gem,
                pumpkins: g.pumpkins,
                feats: g.feats,
                bop_flowers: g.bop_flowers,
                bop_grass: g.bop_grass,
                flower: g.flower,
                grass_gen: g.grass_gen,
                grass_pick: g.grass_pick,
            }
        }
    })
}

/// Generic BOP feature handlers in `getFields` order (`WorldGenFieldAssociation`): (field, generator).
#[derive(Clone, Copy)]
enum F {
    Flo(Flora, i32),
    Water(u8, i32),
    Reeds,
    Rocks(u8),
    Dbl(Db, Db, i32),
    /// `WorldGenBOPLily(block, meta, group)`: random meta on each placement
    Lily(i32),
    /// `WorldGenBOPCoral(block, meta, group)`: random meta 12 + nextInt(4) when `meta` < 0
    Coral(u8, i8, i32),
    /// `WorldGenKelp(minHeight, randomHeight)` with the feature's x / z spread (16, or 8 for thick kelp)
    Kelp(i32, i32, i32),
    /// `WorldGenRiverCane` (draws only)
    RiverCane,
    /// `LakesForcedGenerator` + `WorldGenLakes` (draws only): water / other
    Lakes(bool),
    /// `WorldGenGrassSplatter`: walk down, 128 × (3 offsets + `nextInt(5)`), grass over sand / mud / hard dirt /
    /// stone with BOP foliage on top
    GrassSplatter,
    /// `WorldGenLog`: walk up to air, length 3 + `nextInt(3)`, direction `nextInt(2)`, fallen oak log on grass
    Log,
    /// `WorldGenXericSplatter`: like the grass splatter with `nextInt(8)` flowers / foliage
    XericSplatter,
    /// `WorldGenSplatter(sand, 1, hardSand)`: y = `nextInt(256)` (x, y, z order), hard sand → red sand
    RedSandSplatter,
    /// `WorldGenWaterside(grass, 7, sand, hardened_clay)`
    Oasis,
}

/// The `generate*` splotch features and melons (declared before the springs): (field, block, size, attempts,
/// y = nextInt(a) + b, `None` = melons).
#[allow(clippy::type_complexity)]
const PRE: &[(&str, Option<(u8, i32, i32, i32, i32)>)] = &[
    ("generateQuicksand", Some((blk::BOP_MUD, 24, 5, 64, 64))),
    ("generateCanyon", Some((0, 48, 15, 64, 64))),
    ("generateStoneInGrass", Some((blk::STONE, 32, 15, 64, 64))),
    ("generateStoneInGrass2", Some((blk::STONE, 48, 20, 64, 64))),
    ("generateGrass", Some((0, 48, 15, 128, 0))),
    ("generateSand", Some((0, 32, 15, 128, 0))),
    ("generateQuagmire", Some((0, 48, 15, 64, 64))),
    ("generateMelons", None),
    ("generateMycelium", Some((blk::MYCELIUM, 32, 10, 128, 0))),
    ("generateSponge", Some((0, 24, 5, 64, 0))),
];

/// `WorldGenSplotches.generate`: shared draws 2 × `nextInt(3)`, the shape from the own Random (floats / doubles).
/// Only replacements of grass are placed (stone, mycelium, mud); block 0 = draws only.
fn splotch(w: &mut DecoWorld, b: &mut Bop, block: u8, n: i32, x: i32, y: i32, z: i32) {
    use crate::population::{mcos, msin};
    let f = b.of() * std::f32::consts::PI;
    let nf = n as f32;
    let x0 = (x + 8) as f64 + (msin(f) * nf / 8.0) as f64;
    let x1 = (x + 8) as f64 - (msin(f) * nf / 8.0) as f64;
    let z0 = (z + 8) as f64 + (mcos(f) * nf / 8.0) as f64;
    let z1 = (z + 8) as f64 - (mcos(f) * nf / 8.0) as f64;
    let y0 = (y + b.ni(3) - 2) as f64;
    let y1 = (y + b.ni(3) - 2) as f64;
    for i in 0..=n {
        let t = i as f64 / n as f64;
        let cx = x0 + (x1 - x0) * t;
        let cy = y0 + (y1 - y0) * t;
        let cz = z0 + (z1 - z0) * t;
        let r = b.od() * n as f64 / 16.0;
        let s = (msin(i as f32 * std::f32::consts::PI / nf) + 1.0) as f64;
        let h = s * r + 1.0;
        if block == 0 {
            continue;
        }
        let lo = |c: f64| (c - h / 2.0).floor() as i32;
        let hi = |c: f64| (c + h / 2.0).floor() as i32;
        for xx in lo(cx)..=hi(cx) {
            let dx = (xx as f64 + 0.5 - cx) / (h / 2.0);
            if dx * dx >= 1.0 {
                continue;
            }
            for yy in lo(cy)..=hi(cy) {
                let dy = (yy as f64 + 0.5 - cy) / (h / 2.0);
                if dx * dx + dy * dy >= 1.0 {
                    continue;
                }
                for zz in lo(cz)..=hi(cz) {
                    let dz = (zz as f64 + 0.5 - cz) / (h / 2.0);
                    if dx * dx + dy * dy + dz * dz < 1.0 && is(w.get(xx, yy, zz), blk::GRASS) {
                        w.set(xx, yy, zz, block as Db);
                    }
                }
            }
        }
    }
}

const GENERIC: &[(&str, F)] = &[
    ("waterLakesPerChunk", F::Lakes(true)),
    ("lavaLakesPerChunk", F::Lakes(false)),
    ("poisonLakesPerChunk", F::Lakes(false)),
    ("bopLilyPerChunk", F::Lily(128)),
    ("gravelPerChunk", F::Water(blk::GRAVEL, 7)),
    ("mudPerChunk", F::Water(blk::BOP_MUD, 7)),
    ("riverCanePerChunk", F::RiverCane),
    (
        "shrubsPerChunk",
        F::Flo(Flora::Decor(db::BOP_FOLIAGE, 9), 64),
    ),
    (
        "bushesPerChunk",
        F::Flo(Flora::Decor(db::BOP_FOLIAGE, 4), 64),
    ),
    (
        "cloverPatchesPerChunk",
        F::Flo(Flora::Decor(db::BOP_FOLIAGE, 13), 128),
    ),
    (
        "leafPilesPerChunk",
        F::Flo(Flora::Decor(db::BOP_FOLIAGE, 14), 256),
    ),
    (
        "deadLeafPilesPerChunk",
        F::Flo(Flora::Decor(db::BOP_FOLIAGE, 15), 256),
    ),
    (
        "lavenderPerChunk",
        F::Flo(Flora::Decor(db::BOP_FLOWERS2, 3), 64),
    ),
    (
        "desertGrassPerChunk",
        F::Flo(Flora::Decor(db::BOP_PLANTS, 3), 64),
    ),
    (
        "desertSproutsPerChunk",
        F::Flo(Flora::Decor(db::BOP_PLANTS, 2), 64),
    ),
    (
        "bromeliadsPerChunk",
        F::Flo(Flora::Decor(db::BOP_FLOWERS, 12), 64),
    ),
    ("waterReedsPerChunk", F::Reeds),
    (
        "wildCarrotsPerChunk",
        F::Flo(Flora::Decor(db::BOP_PLANTS, 11), 64),
    ),
    (
        "poisonIvyPerChunk",
        F::Flo(Flora::Decor(db::BOP_FOLIAGE, 7), 64),
    ),
    (
        "berryBushesPerChunk",
        F::Flo(Flora::Decor(db::BOP_FOLIAGE, 8), 64),
    ),
    ("portobellosPerChunk", F::Flo(Flora::Mushroom(1), 64)),
    (
        "koruPerChunk",
        F::Flo(Flora::Decor(db::BOP_FOLIAGE, 12), 64),
    ),
    (
        "wildRicePerChunk",
        F::Flo(Flora::Decor(db::BOP_PLANTS, 6), 64),
    ),
    ("blueMilksPerChunk", F::Flo(Flora::Mushroom(2), 64)),
    (
        "cattailsPerChunk",
        F::Flo(Flora::Decor(db::BOP_PLANTS, 7), 64),
    ),
    (
        "highCattailsPerChunk",
        F::Dbl(db::mk(db::BOP_PLANTS, 10), db::mk(db::BOP_PLANTS, 9), 64),
    ),
    ("algaePerChunk", F::Flo(Flora::Algae, 256)),
    (
        "sproutsPerChunk",
        F::Flo(Flora::Decor(db::BOP_FOLIAGE, 5), 64),
    ),
    (
        "tinyCactiPerChunk",
        F::Flo(Flora::Decor(db::BOP_PLANTS, 12), 64),
    ),
    ("oasesPerChunk", F::Oasis),
    ("rootsPerChunk", F::Flo(Flora::Root, 64)),
    ("grassSplatterPerChunk", F::GrassSplatter),
    ("xericSplatterPerChunk", F::XericSplatter),
    ("rockpilesPerChunk", F::Rocks(blk::COBBLE)),
    ("logsPerChunk", F::Log),
    ("wastelandRockPilesPerChunk", F::Rocks(blk::BOP_DRIED_DIRT)),
    ("redSandSplatterPerChunk", F::RedSandSplatter),
    (
        "flaxPerChunk",
        F::Dbl(db::mk(db::BOP_FOLIAGE, 3), db::mk(db::BOP_FOLIAGE, 6), 24),
    ),
    ("seaweedPerChunk", F::Coral(db::BOP_CORAL2, 8, 256)),
    ("coralPerChunk", F::Coral(db::BOP_CORAL1, -1, 64)),
    ("kelpPerChunk", F::Kelp(4, 8, 16)),
    ("kelpThickPerChunk", F::Kelp(4, 8, 8)),
    ("shortKelpPerChunk", F::Kelp(2, 4, 16)),
    (
        "thornsPerChunk",
        F::Flo(Flora::Decor(db::BOP_PLANTS, 5), 64),
    ),
    ("toadstoolsPerChunk", F::Flo(Flora::Mushroom(0), 64)),
    ("glowshroomsPerChunk", F::Flo(Flora::Mushroom(3), 64)),
];

/// BOP biome classes with `generatePumpkins = false`.
const NO_PUMPKINS: &[&str] = &[
    "BiomeGenBambooForest",
    "BiomeGenBayou",
    "BiomeGenBog",
    "BiomeGenCanyon",
    "BiomeGenCanyonRavine",
    "BiomeGenChaparral",
    "BiomeGenCherryBlossomGrove",
    "BiomeGenFrostForest",
    "BiomeGenGrassland",
    "BiomeGenGrove",
    "BiomeGenHeathland",
    "BiomeGenLushDesert",
    "BiomeGenMarsh",
    "BiomeGenMeadow",
    "BiomeGenMeadowForest",
    "BiomeGenMoor",
    "BiomeGenOasis",
    "BiomeGenOriginValley",
    "BiomeGenOutback",
    "BiomeGenRainforest",
    "BiomeGenRedwoodForest",
    "BiomeGenScrubland",
    "BiomeGenShrubland",
    "BiomeGenTemperateRainforest",
    "BiomeGenTropicalRainforest",
    "BiomeGenTropics",
    "BiomeGenXericShrubland",
];

/// The `OverworldBiomeFeatures` feature handlers this port implements (the rest must be 0 / false).
const KNOWN: &[&str] = &[
    "gravelPerChunk",
    "shrubsPerChunk",
    "leafPilesPerChunk",
    "deadLeafPilesPerChunk",
    "waterReedsPerChunk",
    "berryBushesPerChunk",
    "portobellosPerChunk",
    "rockpilesPerChunk",
    "flaxPerChunk",
];

/// `BlockBOPCoral.canBlockStay` floor (kelp bottom, coral, seaweed): dirt, sand, sponge, stone, clay, gravel, grass,
/// mud.
fn coral_floor(b: Db) -> bool {
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
}

/// Floor the grass / xeric splatters turn into grass: hard sand, mud, hard dirt, sand (any), stone.
fn splatter_base(b: Db) -> bool {
    matches!(
        db::id(b),
        blk::BOP_HARD_SAND
            | blk::BOP_MUD
            | blk::BOP_HARD_DIRT
            | blk::SAND
            | blk::RED_SAND
            | blk::STONE
    )
}

/// `WorldGenMoss.generate` (y up to 79): on air, the first side (2..5: z+1, z−1, x+1, x−1) with a log or plain
/// stone neighbour rolls `nextInt(4)` (0 = moss facing it); on anything else the column moves by ±3 (4 draws).
fn moss(w: &mut DecoWorld, r: &mut JavaRandom, x0: i32, y0: i32, z0: i32) {
    w.mark("moss", x0, y0, z0);
    let (mut x, mut z) = (x0, z0);
    for y in y0..80 {
        if w.air(x, y, z) {
            // (dx, dz, meta = 1 << direction of the opposite face)
            for (dx, dz, meta) in [(0, 1, 1u8), (0, -1, 4), (1, 0, 8), (-1, 0, 2)] {
                let n = w.get(x + dx, y, z + dz);
                if mat(n) == Mat::Wood || is(n, blk::STONE) {
                    if r.next_int_bounded(4) == 0 {
                        w.set(x, y, z, db::mk(db::BOP_MOSS, meta));
                    }
                    break;
                }
            }
        } else {
            x = x0 + r.next_int_bounded(4) - r.next_int_bounded(4);
            z = z0 + r.next_int_bounded(4) - r.next_int_bounded(4);
        }
    }
}

/// Phase mark for the decoration validation (`DecoWorld::phases`, only when tracing).
fn mark(w: &mut DecoWorld, b: &Bop, cx: i32, cz: i32, name: &'static str) {
    if w.tracing {
        w.phases.push((cx, cz, name, b.p.state()));
    }
}

/// `BiomeGenBase.decorate` of the BOP biome `class` (class suffix) for chunk (cx, cz). `false` = not ported.
pub fn decorate(w: &mut DecoWorld, r: &mut JavaRandom, class: &str, cx: i32, cz: i32) -> bool {
    let Some(mut c) = cfg(class) else {
        return false;
    };
    let (x0, z0) = (cx * 16, cz * 16);
    // biome at chunk + 16 for BOP's DecorationModificationEventHandler (LAKE / PUMPKIN denial)
    let here = w.gen_id(x0 + 16, z0 + 16);
    let here_class = w.gen_class.get(&here).copied().unwrap_or("");
    let here_bop = here_class.starts_with("biomesoplenty.");
    let here_suffix = here_class.rsplit('.').next().unwrap_or("");
    let deny_pumpkin = here_bop && NO_PUMPKINS.contains(&here_suffix);
    // zinc gravel is placed by `SurfaceOreGen` (draws on a hit): unknown
    let zinc_iters = {
        let g = w.p.biome_gen_id(x0, z0, w.h);
        if g == 3 || g == 20 {
            2
        } else {
            1
        }
    };

    let own = JavaRandom::new(r.state());
    let vanilla = c.vanilla != Vanilla::Bop;
    let mut b = Bop { p: r, own, vanilla };
    let b = &mut b;
    // the vanilla biome's own decorate() before `super.decorate`
    match c.vanilla {
        Vanilla::SunflowerPlains => {
            let d0 = plant_noise((x0 + 8) as f64 / 200.0, (z0 + 8) as f64 / 200.0);
            let tall = |w: &mut DecoWorld, b: &mut Bop, meta: u8| {
                let x = x0 + b.ni(16) + 8;
                let z = z0 + b.ni(16) + 8;
                let h = w.height(x, z);
                let y = b.ni(h + 32);
                double_flora(
                    w,
                    b,
                    db::mk(db::DOUBLE_PLANT, meta),
                    db::mk(db::DOUBLE_PLANT, 8),
                    64,
                    x,
                    y,
                    z,
                );
            };
            if d0 < -0.8 {
                c.flowers = 15;
                c.grass = 5;
            } else {
                c.flowers = 4;
                c.grass = 10;
                for _ in 0..7 {
                    tall(w, b, 2);
                }
            }
            // field_150628_aC (sunflower plains): 10 sunflower patches
            for _ in 0..10 {
                tall(w, b, 0);
            }
        }
        Vanilla::IceSpikes => {
            for _ in 0..3 {
                let x = x0 + b.ni(16) + 8;
                let z = z0 + b.ni(16) + 8;
                let y = w.height(x, z);
                ice_spike(w, b, x, y, z);
            }
            for _ in 0..2 {
                let x = x0 + b.ni(16) + 8;
                let z = z0 + b.ni(16) + 8;
                let y = w.height(x, z);
                ice_path(w, b, x, y, z);
            }
        }
        Vanilla::Bop => {}
    }
    mark(w, b, cx, cz, "decPre");
    // ores: dirt 20, gravel 10 (y 0..255)
    for (n, block) in [(20, blk::DIRT as Db), (10, blk::GRAVEL as Db)] {
        for _ in 0..n {
            let x = x0 + b.ni(16);
            let y = b.ni(256);
            let z = z0 + b.ni(16);
            minable(w, b, block, x, y, z);
        }
    }
    mark(w, b, cx, cz, "SAND");
    for _ in 0..zinc_iters {
        if b.ni(150) == 0 {
            return false;
        }
    }
    let surf = |w: &mut DecoWorld, b: &mut Bop| {
        let x = x0 + b.ni(16) + 8;
        let z = z0 + b.ni(16) + 8;
        (x, w.top_solid(x, z), z)
    };
    // sandPerChunk2 = 3 (sand, r 7), clayPerChunk = 1 (clay, r 4), sandPerChunk (gravel, r 6)
    for _ in 0..c.sand2 {
        let (x, y, z) = surf(w, b);
        waterside(w, b, blk::SAND as Db, 7, 2, is_earth, x, y, z);
    }
    mark(w, b, cx, cz, "CLAY");
    for _ in 0..c.clay {
        let (x, y, z) = surf(w, b);
        waterside(
            w,
            b,
            db::mk(db::CLAY, 0),
            4,
            1,
            |d| is_dirt(d) || is(d, db::CLAY),
            x,
            y,
            z,
        );
    }
    mark(w, b, cx, cz, "SAND_PASS2");
    for _ in 0..c.sand {
        let (x, y, z) = surf(w, b);
        waterside(w, b, blk::GRAVEL as Db, 6, 2, is_earth, x, y, z);
    }
    mark(w, b, cx, cz, "TREE");
    // trees
    let mut n = c.trees;
    if b.ni(10) == 0 {
        n += 1;
    }
    for _ in 0..n {
        let x = x0 + b.ni(16) + 8;
        let z = z0 + b.ni(16) + 8;
        let y = w.height(x, z);
        match c.tree {
            Tree::Oak => G::Trees(4, 0, 0, false).generate(w, b, x, y, z),
            Tree::Sel(t) => t.pick(b.p).generate(w, b, x, y, z),
            Tree::Shrub => G::Shrub(0, 0).generate(w, b, x, y, z),
            Tree::PrairieTaiga => {
                bop_taiga2(
                    w,
                    b,
                    db::mk(db::LOG, 0),
                    db::mk(db::LEAVES, 0),
                    6,
                    1,
                    7,
                    -1,
                    x,
                    y,
                    z,
                );
            }
        }
    }
    mark(w, b, cx, cz, "BIG_SHROOM");
    // bigMushroomsPerChunk: vanilla `WorldGenBigMushroom` (x, z, type, height; placement not modelled)
    for _ in 0..c.big_mush {
        b.ni(16);
        b.ni(16);
        b.ni(2);
        b.ni(3);
    }
    mark(w, b, cx, cz, "FLOWERS");
    // FLOWERS event: Botania does not draw here (rwg2dump 2.16: the shared Random is unchanged across this event;
    // the 162 draws of `botania` would be 2 patch rolls + 40 underground mushrooms)
    if c.flowers > 0 && !c.generic && c.vanilla == Vanilla::Bop {
        return false;
    }
    for _ in 0..c.flowers {
        let x = x0 + b.ni(16) + 8;
        let z = z0 + b.ni(16) + 8;
        let h = w.height(x, z);
        let y = b.ni(h + 32);
        // biome.func_150572_a: the flower type (draws)
        let (block, meta) = if c.vanilla == Vanilla::SunflowerPlains {
            if plant_noise(x as f64 / 200.0, z as f64 / 200.0) < -0.8 {
                (db::RED_FLOWER, 4 + b.ni(4) as u8)
            } else if b.ni(3) > 0 {
                (db::RED_FLOWER, [0u8, 3, 8][b.ni(3) as usize])
            } else {
                (db::YELLOW_FLOWER, 0)
            }
        } else if b.ni(3) > 0 {
            (db::YELLOW_FLOWER, 0)
        } else {
            (db::RED_FLOWER, 0)
        };
        G::FlowersV(block, meta).generate(w, b, x, y, z);
    }
    mark(w, b, cx, cz, "GRASS");
    // GRASS: vanilla tallgrass
    for _ in 0..c.grass {
        let x = x0 + b.ni(16) + 8;
        let z = z0 + b.ni(16) + 8;
        let h = w.height(x, z);
        let y = b.fi(h * 2);
        match c.grass_pick {
            Some(f) => {
                let g = f.pick(b);
                run_flora(w, b, g, x, y, z);
            }
            None => tall_grass(w, b, Flora::TallGrass(1), x, y, z),
        }
    }
    // dead bush 0, lily pad 0, mushrooms
    mark(w, b, cx, cz, "DEAD_BUSH");
    for _ in 0..c.dead_bush {
        let x = x0 + b.ni(16) + 8;
        let z = z0 + b.ni(16) + 8;
        let h = w.height(x, z);
        let y = b.fi(h * 2);
        G::DeadBush.generate(w, b, x, y, z);
    }
    mark(w, b, cx, cz, "LILYPAD");
    // waterlilyPerChunk: vanilla `WorldGenWaterlily` (10 tries)
    for _ in 0..c.lily {
        let x = x0 + b.ni(16) + 8;
        let z = z0 + b.ni(16) + 8;
        let h = w.height(x, z);
        let mut y = b.fi(h * 2);
        while y > 0 && is_air(w.get(x, y - 1, z)) {
            y -= 1;
        }
        for _ in 0..10 {
            let a = x + b.ni(8) - b.ni(8);
            let c2 = y + b.ni(4) - b.ni(4);
            let d = z + b.ni(8) - b.ni(8);
            if is_air(w.get(a, c2, d)) && is_water(w.get(a, c2 - 1, d)) {
                w.set(a, c2, d, db::mk(db::WATERLILY, 0));
            }
        }
    }
    mark(w, b, cx, cz, "SHROOM");
    if c.mushrooms > 0 && !c.generic {
        return false;
    }
    let mush = |w: &mut DecoWorld, b: &mut Bop, block: u8| {
        let x = x0 + b.ni(16) + 8;
        let z = z0 + b.ni(16) + 8;
        let h = w.height(x, z);
        let y = b.fi(h * 2);
        G::FlowersV(block, 0).generate(w, b, x, y, z);
    };
    for _ in 0..c.mushrooms {
        // in the loop the brown mushroom sits at the height value (no y draw)
        if b.ni(4) == 0 {
            let x = x0 + b.ni(16) + 8;
            let z = z0 + b.ni(16) + 8;
            let y = w.height(x, z);
            G::FlowersV(db::BROWN_MUSHROOM, 0).generate(w, b, x, y, z);
        }
        if b.ni(8) == 0 {
            mush(w, b, db::RED_MUSHROOM);
        }
    }
    if b.ni(4) == 0 {
        mush(w, b, db::BROWN_MUSHROOM);
    }
    if b.ni(8) == 0 {
        mush(w, b, db::RED_MUSHROOM);
    }
    mark(w, b, cx, cz, "REED");
    // reeds: reedsPerChunk (0 / -999) + 10
    for _ in 0..c.reeds.max(0) + 10 {
        let x = x0 + b.ni(16) + 8;
        let z = z0 + b.ni(16) + 8;
        let h = w.height(x, z);
        let y = b.fi(h * 2);
        G::Reed.generate(w, b, x, y, z);
    }
    mark(w, b, cx, cz, "PUMPKIN");
    if !deny_pumpkin && b.ni(32) == 0 {
        let x = x0 + b.ni(16) + 8;
        let z = z0 + b.ni(16) + 8;
        let h = w.height(x, z);
        let y = b.fi(h * 2);
        G::Pumpkin.generate(w, b, x, y, z);
    }
    mark(w, b, cx, cz, "CACTUS");
    for _ in 0..c.cacti {
        let x = x0 + b.ni(16) + 8;
        let z = z0 + b.ni(16) + 8;
        let h = w.height(x, z);
        let y = b.fi(h * 2);
        G::CactusV.generate(w, b, x, y, z);
    }
    mark(w, b, cx, cz, "LAKE");
    // LAKE: springs (placement not modelled)
    if !here_bop {
        for _ in 0..50 {
            b.ni(16);
            let t = b.ni(248);
            b.ni(t + 8);
            b.ni(16);
        }
        for _ in 0..20 {
            b.ni(16);
            let t = b.ni(240);
            let u = b.ni(t + 8);
            b.ni(u + 8);
            b.ni(16);
        }
    }
    mark(w, b, cx, cz, "decPost");
    // vanilla biomes (Sunflower Plains, Ice Plains Spikes) stop here: rwg2dump 2.16 shows no BOP feature section
    // for them (BOP's inherited-biome overrides are not the registered biomes in GTNH)
    if c.vanilla != Vanilla::Bop {
        return true;
    }
    let _ = c.pumpkins;
    // BOP features
    for &(name, _) in c.feats {
        if !c.generic && !KNOWN.contains(&name) {
            return false;
        }
    }
    let count = |name: &str| {
        let d = match name {
            "minersDelightPerChunk" => 1,
            "rootsPerChunk" => 30,
            _ => 0,
        };
        c.feats.iter().find(|f| f.0 == name).map_or(d, |f| f.1)
    };
    // generate* splotches and melons (declared before the springs)
    if c.generic {
        for &(name, sp) in PRE {
            if count(name) == 0 {
                continue;
            }
            match sp {
                Some((block, n, k, a, off)) => {
                    for _ in 0..k {
                        let x = x0 + b.ni(16);
                        let y = b.ni(a) + off;
                        let z = z0 + b.ni(16);
                        splotch(w, b, block, n, x, y, z);
                    }
                }
                None => {
                    // MelonForcedGenerator + vanilla WorldGenMelon (64 attempts, melon on grass)
                    let x = x0 + b.ni(16) + 8;
                    let z = z0 + b.ni(16) + 8;
                    let h = w.height(x, z);
                    let y = b.ni(h * 2);
                    for _ in 0..64 {
                        let a = x + b.ni(8) - b.ni(8);
                        let c2 = y + b.ni(4) - b.ni(4);
                        let d = z + b.ni(8) - b.ni(8);
                        // melon blocks are not in the decoration palette: draws only
                        let _ = (a, c2, d);
                    }
                }
            }
        }
    }
    // waterSpringsPerChunk and lavaSpringsPerChunk each run the whole spring routine (50 + 20)
    for _ in 0..2 {
        for _ in 0..50 {
            b.ni(16);
            let t = b.ni(120);
            b.ni(t + 8);
            b.ni(16);
        }
        for _ in 0..20 {
            b.ni(16);
            let t = b.ni(112);
            let u = b.ni(t + 8);
            b.ni(u + 8);
            b.ni(16);
        }
    }
    let scat = |w: &mut DecoWorld, b: &mut Bop, ymode: i32| -> (i32, i32, i32) {
        let x = x0 + b.ni(16) + 8;
        let z = z0 + b.ni(16) + 8;
        let y = match ymode {
            0 => b.ni(256),
            1 => {
                let h = w.height(x, z);
                b.ni(h * 2)
            }
            2 => {
                let h = w.height(x, z);
                b.ni(h + 32)
            }
            _ => b.ni(60),
        };
        (x, y, z)
    };
    if c.generic {
        for &(name, f) in GENERIC {
            if name == "rootsPerChunk" {
                // minersDelightPerChunk (BiomeFeaturesBase) precedes it
                for _ in 0..count("minersDelightPerChunk") {
                    let (x, y, z) = scat(w, b, 3);
                    tall_grass(w, b, Flora::MinersDelight, x, y, z);
                }
            }
            for _ in 0..count(name) {
                match f {
                    F::Flo(fl, group) => {
                        let (x, y, z) = scat(w, b, 0);
                        flora(w, b, fl, group, x, y, z);
                    }
                    F::Water(block, rad) => {
                        let (x, y, z) = surf(w, b);
                        waterside(w, b, block as Db, rad, 2, is_earth, x, y, z);
                    }
                    F::Reeds => {
                        let (x, y, z) = scat(w, b, 1);
                        water_reeds(w, b, x, y, z);
                    }
                    F::Rocks(block) => {
                        let (x, y, z) = scat(w, b, 1);
                        bop_blob(w, b, block as Db, 0, x, y, z);
                    }
                    F::Dbl(lo, hi, group) => {
                        let (x, y, z) = scat(w, b, 1);
                        double_flora(w, b, lo, hi, group, x, y, z);
                    }
                    F::Lily(group) => {
                        let (x, y, z) = scat(w, b, 0);
                        for _ in 0..group {
                            let a = x + b.ni(8) - b.ni(8);
                            let c2 = y + b.ni(4) - b.ni(4);
                            let d = z + b.ni(8) - b.ni(8);
                            if w.air(a, c2, d) && c2 < 255 && is_water(w.get(a, c2 - 1, d)) {
                                let m = b.ni(3) as u8;
                                w.set(a, c2, d, db::mk(db::WATERLILY, m));
                            }
                        }
                    }
                    F::Coral(block, meta, group) => {
                        let (x, y, z) = scat(w, b, 1);
                        for _ in 0..group {
                            let a = x + b.ni(8) - b.ni(8);
                            let c2 = y + b.ni(4) - b.ni(4);
                            let d = z + b.ni(8) - b.ni(8);
                            let m = if meta < 0 {
                                12 + b.ni(4) as u8
                            } else {
                                meta as u8
                            };
                            if is_water(w.get(a, c2, d))
                                && is_water(w.get(a, c2 + 1, d))
                                && coral_floor(w.get(a, c2 - 1, d))
                            {
                                w.set(a, c2, d, db::mk(block, m));
                            }
                        }
                    }
                    F::Kelp(min, rand, spread) => {
                        // +8: the pack's BOP places kelp in the decoration square like every other feature
                        // (rwg2dump 2.16 Kelp Forest: real = model + (8, 8) exactly; the 2.1.x source has no +8)
                        let x = x0 + 8 + b.ni(spread);
                        let z = z0 + 8 + b.ni(spread);
                        let y = b.ni(64);
                        let x2 = x + b.ni(4) - b.ni(4);
                        let z2 = z + b.ni(4) - b.ni(4);
                        let height = min + b.ni(rand);
                        if is_water(w.get(x2, y, z2))
                            && is_water(w.get(x2, y + 1, z2))
                            && coral_floor(w.get(x2, y - 1, z2))
                        {
                            for i in 0..height {
                                let m = if i == 0 {
                                    8
                                } else if i == height - 1 {
                                    10
                                } else {
                                    9
                                };
                                if is_water(w.get(x2, y + i, z2))
                                    && is_water(w.get(x2, y + i + 1, z2))
                                {
                                    w.set(x2, y + i, z2, db::mk(db::BOP_CORAL1, m));
                                } else {
                                    if i > 0 {
                                        w.set(x2, y + i - 1, z2, db::mk(db::BOP_CORAL1, 10));
                                    }
                                    break;
                                }
                            }
                        }
                    }
                    F::Oasis => {
                        let (x, y, z) = surf(w, b);
                        let from = |bl: Db| is_sand(bl) || is(bl, blk::HARDENED_CLAY);
                        waterside(w, b, blk::GRASS as Db, 7, 2, from, x, y, z);
                    }
                    F::RedSandSplatter => {
                        let x = x0 + b.ni(16) + 8;
                        let mut y = b.ni(256);
                        let z = z0 + b.ni(16) + 8;
                        loop {
                            let bl = w.get(x, y, z);
                            if !(is_leaves(bl) || is_air(bl)) {
                                break;
                            }
                            y -= 1;
                            if y <= 0 {
                                break;
                            }
                        }
                        for _ in 0..128 {
                            let a = x + b.ni(8) - b.ni(8);
                            let c2 = y + b.ni(4) - b.ni(4);
                            let d = z + b.ni(8) - b.ni(8);
                            if w.air(a, c2, d) && is(w.get(a, c2 - 1, d), blk::BOP_HARD_SAND) {
                                w.set(a, c2 - 1, d, blk::RED_SAND as Db);
                            }
                        }
                    }
                    F::XericSplatter => {
                        let (x, mut y, z) = scat(w, b, 1);
                        loop {
                            let bl = w.get(x, y, z);
                            if !(is_leaves(bl) || is_air(bl)) {
                                break;
                            }
                            y -= 1;
                            if y <= 0 {
                                break;
                            }
                        }
                        for _ in 0..128 {
                            let a = x + b.ni(8) - b.ni(8);
                            let c2 = y + b.ni(4) - b.ni(4);
                            let d = z + b.ni(8) - b.ni(8);
                            let v = b.ni(8);
                            if w.air(a, c2, d) && splatter_base(w.get(a, c2 - 1, d)) {
                                w.set(a, c2 - 1, d, blk::GRASS as Db);
                                let f = match v {
                                    0 => db::mk(db::BOP_FLOWERS, 7),
                                    2 => db::mk(db::RED_FLOWER, 0),
                                    3 => db::mk(db::YELLOW_FLOWER, 0),
                                    5 => db::mk(db::RED_FLOWER, 3),
                                    7 => db::mk(db::BOP_FOLIAGE, 9),
                                    _ => db::mk(db::BOP_FOLIAGE, 4),
                                };
                                w.set(a, c2, d, f);
                            }
                        }
                    }
                    F::GrassSplatter => {
                        let (x, mut y, z) = scat(w, b, 1);
                        w.mark("grasssplatter", x, y, z);
                        loop {
                            let bl = w.get(x, y, z);
                            if !(is_leaves(bl) || is_air(bl)) {
                                break;
                            }
                            y -= 1;
                            if y <= 0 {
                                break;
                            }
                        }
                        for _ in 0..128 {
                            let a = x + b.ni(8) - b.ni(8);
                            let c2 = y + b.ni(4) - b.ni(4);
                            let d = z + b.ni(8) - b.ni(8);
                            let v = b.ni(5);
                            if w.air(a, c2, d) && splatter_base(w.get(a, c2 - 1, d)) {
                                w.set(a, c2 - 1, d, blk::GRASS as Db);
                                let m = [2, 10, 11, 2, 2][v as usize];
                                w.set(a, c2, d, db::mk(db::BOP_FOLIAGE, m));
                            }
                        }
                    }
                    F::Log => {
                        let (x, mut y, z) = scat(w, b, 1);
                        while !w.air(x, y, z) && y > 2 {
                            y += 1;
                        }
                        let len = 3 + b.ni(3);
                        let dir = b.ni(2);
                        let at = |i: i32| if dir == 0 { (x + i, z) } else { (x, z + i) };
                        if (0..len).all(|i| {
                            let (a, d) = at(i);
                            w.air(a, y, d) && is(w.get(a, y - 1, d), blk::GRASS)
                        }) {
                            for i in 0..len {
                                let (a, d) = at(i);
                                w.set(a, y, d, db::mk(db::LOG, if dir == 0 { 4 } else { 8 }));
                            }
                        }
                    }
                    F::RiverCane => {
                        let (x, y, z) = scat(w, b, 1);
                        for _ in 0..5 {
                            let a = x + b.ni(8) - b.ni(8);
                            let d = z + b.ni(8) - b.ni(8);
                            if w.air(a, y, d) {
                                let k = b.ni(3);
                                b.ni(k + 1);
                            }
                        }
                    }
                    F::Lakes(water) => {
                        let x = x0 + b.ni(16) + 8;
                        let y = if water {
                            let t = b.ni(240);
                            b.ni(t + 8)
                        } else {
                            let t = b.ni(112);
                            let u = b.ni(t + 8);
                            b.ni(u + 8)
                        };
                        let z = z0 + b.ni(16) + 8;
                        // WorldGenLakes: walk down through air at (x - 8, z - 8); nextInt(4) + 4 blobs (shared),
                        // 6 doubles each (own Random); lava borders (world-dependent nextInt(2)) not modelled
                        let (lx, lz) = (x - 8, z - 8);
                        let mut ly = y;
                        while ly > 5 && is_air(w.get(lx, ly, lz)) {
                            ly -= 1;
                        }
                        if ly > 4 {
                            let n = b.ni(4) + 4;
                            for _ in 0..n * 6 {
                                b.od();
                            }
                        }
                    }
                }
            }
        }
    } else {
        // declaration order of OverworldBiomeFeatures
        for _ in 0..count("gravelPerChunk") {
            let (x, y, z) = surf(w, b);
            waterside(w, b, blk::GRAVEL as Db, 7, 2, is_earth, x, y, z);
        }
        for (name, f, group) in [
            ("shrubsPerChunk", Flora::Decor(db::BOP_FOLIAGE, 9), 64),
            ("leafPilesPerChunk", Flora::Decor(db::BOP_FOLIAGE, 14), 256),
            (
                "deadLeafPilesPerChunk",
                Flora::Decor(db::BOP_FOLIAGE, 15),
                256,
            ),
        ] {
            for _ in 0..count(name) {
                let (x, y, z) = scat(w, b, 0);
                flora(w, b, f, group, x, y, z);
            }
        }
        for _ in 0..count("waterReedsPerChunk") {
            let (x, y, z) = scat(w, b, 1);
            water_reeds(w, b, x, y, z);
        }
        for (name, f) in [
            ("berryBushesPerChunk", Flora::Decor(db::BOP_FOLIAGE, 8)),
            ("portobellosPerChunk", Flora::Mushroom(1)),
        ] {
            for _ in 0..count(name) {
                let (x, y, z) = scat(w, b, 0);
                flora(w, b, f, 64, x, y, z);
            }
        }
        // minersDelightPerChunk = 1, rootsPerChunk = 30
        {
            let (x, y, z) = scat(w, b, 3);
            tall_grass(w, b, Flora::MinersDelight, x, y, z);
        }
        for _ in 0..30 {
            let (x, y, z) = scat(w, b, 0);
            flora(w, b, Flora::Root, 64, x, y, z);
        }
        for _ in 0..count("rockpilesPerChunk") {
            let (x, y, z) = scat(w, b, 1);
            bop_blob(w, b, blk::COBBLE as Db, 0, x, y, z);
        }
        for _ in 0..count("flaxPerChunk") {
            let (x, y, z) = scat(w, b, 1);
            double_flora(
                w,
                b,
                db::mk(db::BOP_FOLIAGE, 3),
                db::mk(db::BOP_FOLIAGE, 6),
                24,
                x,
                y,
                z,
            );
        }
    }
    // bopFlowersPerChunk: FLOWERS event first (no Botania draws, see above)
    mark(w, b, cx, cz, "FLOWERS2");
    for _ in 0..c.bop_flowers {
        let (x, y, z) = scat(w, b, 2);
        run_flora(w, b, c.flower, x, y, z);
    }
    mark(w, b, cx, cz, "GRASS2");
    for _ in 0..c.bop_grass {
        let (x, y, z) = scat(w, b, 0);
        run_flora(w, b, c.grass_gen, x, y, z);
    }
    // the biome's own decorate after `super.decorate`, on the plain shared Random: gem ore (underground), then
    // per class moss (WorldGenMoss × 20 from y 58), ivy (WorldGenIvy × 50 from y 128: draws x, z only) or the Lush
    // Desert's water pockets (same count as the gem ore)
    if c.gem {
        let r = &mut *b.p;
        let n = 12 + r.next_int_bounded(6);
        for _ in 0..n {
            r.next_int_bounded(16);
            r.next_int_bounded(28);
            r.next_int_bounded(16);
        }
        match class.rsplit('.').next().unwrap_or(class) {
            "BiomeGenBayou"
            | "BiomeGenEucalyptusForest"
            | "BiomeGenFen"
            | "BiomeGenShield"
            | "BiomeGenTemperateRainforest"
            | "BiomeGenWetland" => {
                for _ in 0..20 {
                    let x = x0 + r.next_int_bounded(16) + 8;
                    let z = z0 + r.next_int_bounded(16) + 8;
                    moss(w, r, x, 58, z);
                }
            }
            "BiomeGenFungiForest" => {
                for _ in 0..50 {
                    r.next_int_bounded(16);
                    r.next_int_bounded(16);
                }
            }
            "BiomeGenLushDesert" => {
                for _ in 0..n {
                    let x = x0 + r.next_int_bounded(16);
                    let y = r.next_int_bounded(53) + 75;
                    let z = z0 + r.next_int_bounded(16);
                    if is(w.get(x, y, z), blk::STONE) {
                        w.set(x, y, z, blk::WATER as Db);
                    }
                }
            }
            _ => {}
        }
    }
    mark(w, b, cx, cz, "END");
    true
}

/// Diagnostics: the dirt (true) / gravel (false) block positions the BOP ore phase would place in stone when the
/// decorator starts with shared Random `r` at chunk (cx, cz) (first `blobs` blobs).
pub fn ore_probe(r: &JavaRandom, cx: i32, cz: i32, blobs: usize) -> Vec<(i32, i32, i32, bool)> {
    use crate::population::{mcos, msin};
    let mut p = r.clone();
    let mut own = JavaRandom::new(p.state());
    let mut out = Vec::new();
    let (x0, z0) = (cx * 16, cz * 16);
    for k in 0..blobs.min(30) {
        let dirt = k < 20;
        let x = x0 + p.next_int_bounded(16);
        let y = p.next_int_bounded(256);
        let z = z0 + p.next_int_bounded(16);
        let f = own.next_float() * std::f32::consts::PI;
        let nf = 32f32;
        let d0 = ((x + 8) as f32 + msin(f) * nf / 8.0) as f64;
        let d1 = ((x + 8) as f32 - msin(f) * nf / 8.0) as f64;
        let d2 = ((z + 8) as f32 + mcos(f) * nf / 8.0) as f64;
        let d3 = ((z + 8) as f32 - mcos(f) * nf / 8.0) as f64;
        let d4 = (y + p.next_int_bounded(3) - 2) as f64;
        let d5 = (y + p.next_int_bounded(3) - 2) as f64;
        for l in 0..=32 {
            let ld = l as f64;
            let d6 = d0 + (d1 - d0) * ld / 32.0;
            let d7 = d4 + (d5 - d4) * ld / 32.0;
            let d8 = d2 + (d3 - d2) * ld / 32.0;
            let d9 = own.next_double() * 2.0;
            let s = (msin(l as f32 * std::f32::consts::PI / nf) + 1.0) as f64;
            let d10 = s * d9 + 1.0;
            let fl = |v: f64| v.floor() as i32;
            for a in fl(d6 - d10 / 2.0)..=fl(d6 + d10 / 2.0) {
                for b in fl(d7 - d10 / 2.0)..=fl(d7 + d10 / 2.0) {
                    for c in fl(d8 - d10 / 2.0)..=fl(d8 + d10 / 2.0) {
                        let (e, g, h) = (
                            (a as f64 + 0.5 - d6) / (d10 / 2.0),
                            (b as f64 + 0.5 - d7) / (d10 / 2.0),
                            (c as f64 + 0.5 - d8) / (d10 / 2.0),
                        );
                        if e * e + g * g + h * h < 1.0 {
                            out.push((a, b, c, dirt));
                        }
                    }
                }
            }
        }
    }
    out
}

/// `BiomeGenBase.plantNoise` = `NoiseGeneratorPerlin(new Random(2345L), 1).func_151601_a(x, z)`.
fn plant_noise(x: f64, z: f64) -> f64 {
    use std::sync::OnceLock;
    static N: OnceLock<crate::forestry::Simplex> = OnceLock::new();
    N.get_or_init(|| crate::forestry::Simplex::new(&mut JavaRandom::new(2345)))
        .noise(x, z)
}

/// vanilla `WorldGenIceSpike` (on a snow block)
fn ice_spike(w: &mut DecoWorld, b: &mut Bop, x: i32, mut y: i32, z: i32) {
    while is_air(w.get(x, y, z)) && y > 2 {
        y -= 1;
    }
    if !is(w.get(x, y, z), blk::SNOW) {
        return;
    }
    y += b.ni(4);
    let l = b.ni(4) + 7;
    let i1 = l / 4 + b.ni(2);
    if i1 > 1 && b.ni(60) == 0 {
        y += 10 + b.ni(30);
    }
    let pi = blk::PACKED_ICE as Db;
    let soft = |d: Db| is_air(d) || is_dirt(d) || is(d, blk::SNOW) || is(d, blk::ICE);
    for j1 in 0..l {
        let f = (1.0 - j1 as f32 / l as f32) * i1 as f32;
        let k1 = f.ceil() as i32;
        for l1 in -k1..=k1 {
            let f1 = l1.abs() as f32 - 0.25;
            for i2 in -k1..=k1 {
                let f2 = i2.abs() as f32 - 0.25;
                if (l1 == 0 && i2 == 0 || f1 * f1 + f2 * f2 <= f * f)
                    && (l1 != -k1 && l1 != k1 && i2 != -k1 && i2 != k1 || b.of() <= 0.75)
                {
                    if soft(w.get(x + l1, y + j1, z + i2)) {
                        w.set(x + l1, y + j1, z + i2, pi);
                    }
                    if j1 != 0 && k1 > 1 && soft(w.get(x + l1, y - j1, z + i2)) {
                        w.set(x + l1, y - j1, z + i2, pi);
                    }
                }
            }
        }
    }
    let j1 = (i1 - 1).clamp(0, 1);
    for j2 in -j1..=j1 {
        for l1 in -j1..=j1 {
            let mut i2 = y - 1;
            let mut k2 = 50;
            if j2.abs() == 1 && l1.abs() == 1 {
                k2 = b.ni(5);
            }
            while i2 > 50 {
                let d = w.get(x + j2, i2, z + l1);
                if !(soft(d) || is(d, blk::PACKED_ICE)) {
                    break;
                }
                w.set(x + j2, i2, z + l1, pi);
                i2 -= 1;
                k2 -= 1;
                if k2 <= 0 {
                    i2 -= b.ni(5) + 1;
                    k2 = b.ni(5);
                }
            }
        }
    }
}

/// vanilla `WorldGenIcePath(4)` (on a snow block)
fn ice_path(w: &mut DecoWorld, b: &mut Bop, x: i32, mut y: i32, z: i32) {
    while is_air(w.get(x, y, z)) && y > 2 {
        y -= 1;
    }
    if !is(w.get(x, y, z), blk::SNOW) {
        return;
    }
    let l = b.ni(2) + 2;
    for i1 in x - l..=x + l {
        for j1 in z - l..=z + l {
            let (k1, l1) = (i1 - x, j1 - z);
            if k1 * k1 + l1 * l1 <= l * l {
                for i2 in y - 1..=y + 1 {
                    let d = w.get(i1, i2, j1);
                    if is_dirt(d) || is(d, blk::SNOW) || is(d, blk::ICE) {
                        w.set(i1, i2, j1, blk::PACKED_ICE as Db);
                    }
                }
            }
        }
    }
}
