//! Decoration for the viewer: trees, plants, boulders and canopy snow of one chunk as palette codes.
//!
//! Chunk (cx, cz) receives blocks from the populates of (cx - 1 ..= cx, cz - 1 ..= cz); they are replayed in
//! z-major order on a fresh [`DecoWorld`] (the game's order depends on how the world was explored). Biomes whose
//! decorator is not ported are skipped (that chunk's later biomes then run on an unknown Random: plausible, not
//! exact).

use super::*;
use crate::population::{Outcome, PopModel, PreGates};

/// Palette entry kinds.
pub const K_CUBE: u8 = 1;
pub const K_LEAVES: u8 = 2;
/// cross-shaped plant
pub const K_PLANT: u8 = 3;
/// thin top layer (snow layer)
pub const K_LAYER: u8 = 4;

/// (rgb, kind) per palette code (index 0 = nothing).
pub const PALETTE: &[(u32, u8)] = &[
    (0, 0),
    // 1..=6 vanilla logs: oak, spruce, birch, jungle, acacia, dark oak
    (0x6b5432, K_CUBE),
    (0x3d2a17, K_CUBE),
    (0xd8d4cb, K_CUBE),
    (0x57432a, K_CUBE),
    (0x6b6359, K_CUBE),
    (0x3f2f1c, K_CUBE),
    // 7..=12 vanilla leaves: oak, spruce, birch, jungle, acacia, dark oak
    (0x48a033, K_LEAVES),
    (0x3d6a3d, K_LEAVES),
    (0x7aa055, K_LEAVES),
    (0x3c9a1a, K_LEAVES),
    (0x5c9a2c, K_LEAVES),
    (0x3a6a1f, K_LEAVES),
    // 13 cactus, 14 pumpkin, 15 cobblestone, 16 mossy cobblestone, 17 snow layer, 18 vine
    (0x2f7a2f, K_CUBE),
    (0xe08a1f, K_CUBE),
    (0x7a7a7a, K_CUBE),
    (0x627a62, K_CUBE),
    (0xf4fafa, K_LAYER),
    (0x3a6a1f, K_LAYER),
    // 19 tall grass / fern, 20 double plant, 21 dead bush, 22 reeds, 23 yellow flower, 24 red / other flower,
    // 25 mushroom, 26 BOP foliage, 27 BOP flower, 28 cocoa, 29 crops
    (0x5d9a3a, K_PLANT),
    (0x6aa840, K_PLANT),
    (0x7a5a2a, K_PLANT),
    (0x8ab05a, K_PLANT),
    (0xf2e030, K_PLANT),
    (0xd83a3a, K_PLANT),
    (0xb08a6a, K_PLANT),
    (0x6a9a3a, K_PLANT),
    (0xd070c0, K_PLANT),
    (0x8a5a2a, K_CUBE),
    (0xc8b040, K_PLANT),
    // 30..=45 BOP logs: sacred oak, cherry, dark, fir, ethereal, magic, mangrove, palm, redwood, willow, dead,
    // big flower stem, pine, hellbark, jacaranda, mahogany
    (0x6a5233, K_CUBE),
    (0x7a4a3a, K_CUBE),
    (0x3a2e3a, K_CUBE),
    (0x5a4a35, K_CUBE),
    (0x4a7a8a, K_CUBE),
    (0x5a5aa0, K_CUBE),
    (0x6a6050, K_CUBE),
    (0x8a6a40, K_CUBE),
    (0x7a3a2a, K_CUBE),
    (0x5a5a3a, K_CUBE),
    (0x6a6a60, K_CUBE),
    (0x4a8a3a, K_CUBE),
    (0x5a4530, K_CUBE),
    (0x6a4a3a, K_CUBE),
    (0x8a7a6a, K_CUBE),
    (0x6a3a2a, K_CUBE),
    // 46..=59 BOP leaves: yellow autumn, bamboo, magic, dark, dead, fir, ethereal, orange autumn, origin, pink
    // cherry, maple, white cherry, hellbark, jacaranda
    (0xd8c040, K_LEAVES),
    (0x6aa040, K_LEAVES),
    (0x5a8ad8, K_LEAVES),
    (0x2a4a2a, K_LEAVES),
    (0x8a7a5a, K_LEAVES),
    (0x3a6a4a, K_LEAVES),
    (0x6ad0d0, K_LEAVES),
    (0xd8802a, K_LEAVES),
    (0x6ad03a, K_LEAVES),
    (0xf0a0c0, K_LEAVES),
    (0xc83a2a, K_LEAVES),
    (0xf0e8f0, K_LEAVES),
    (0xa0a040, K_LEAVES),
    (0xa070d8, K_LEAVES),
    // 60..=67 BOP colorized leaves: sacred oak, mangrove, palm, redwood, willow, pine, mahogany, flowering
    (0x4a9a2a, K_LEAVES),
    (0x5a8a3a, K_LEAVES),
    (0x5aa83a, K_LEAVES),
    (0x4a7a3a, K_LEAVES),
    (0x6a9a3a, K_LEAVES),
    (0x3a6a3a, K_LEAVES),
    (0x4a8a2a, K_LEAVES),
    (0x5a9a3a, K_LEAVES),
    // 68 apple leaves, 69 persimmon leaves, 70 / 71 red / yellow petals, 72 bamboo
    (0x4a9a3a, K_LEAVES),
    (0x8aa03a, K_LEAVES),
    (0xd83030, K_LEAVES),
    (0xe8d030, K_LEAVES),
    (0x8ab040, K_CUBE),
    // 73 kelp / seaweed, 74 coral
    (0x3c7a34, K_PLANT),
    (0xe0607a, K_PLANT),
];

/// Palette code of a decoration block (0 = not shown).
pub fn code(b: Db) -> u8 {
    let m = db::meta(b);
    match db::id(b) {
        db::LOG => [1, 2, 3, 4][(m & 3) as usize],
        db::LOG2 => [5, 6, 5, 5][(m & 3) as usize],
        db::LEAVES => [7, 8, 9, 10][(m & 3) as usize],
        db::LEAVES2 => [11, 12, 11, 11][(m & 3) as usize],
        db::CACTUS => 13,
        db::PUMPKIN => 14,
        blk::COBBLE => 15,
        db::MOSSY => 16,
        blk::SNOW_LAYER => 17,
        db::VINE | db::BOP_IVY | db::BOP_MOSS => 18,
        db::TALLGRASS => 19,
        db::DOUBLE_PLANT => 20,
        db::DEADBUSH => 21,
        db::REEDS => 22,
        db::YELLOW_FLOWER => 23,
        db::RED_FLOWER | db::BOTANIA_FLOWER | db::BOTANIA_DOUBLE1 | db::BOTANIA_DOUBLE2 => 24,
        db::BROWN_MUSHROOM | db::RED_MUSHROOM | db::BOP_MUSHROOMS => 25,
        db::BOP_FOLIAGE | db::BOP_PLANTS => 26,
        db::BOP_FLOWERS | db::BOP_FLOWERS2 => 27,
        db::COCOA | db::BOP_FRUIT => 28,
        db::POTATOES | db::CARROTS | db::WHEAT => 29,
        i @ db::BOP_LOGS1..=db::BOP_LOGS4 => 30 + (i - db::BOP_LOGS1) * 4 + (m & 3),
        i @ db::BOP_LEAVES1..=db::BOP_LEAVES4 => {
            let t = (i - db::BOP_LEAVES1) * 4 + (m & 3);
            if t < 14 {
                46 + t
            } else {
                7
            }
        }
        i @ db::BOP_CLEAVES1..=db::BOP_CLEAVES2 => 60 + (i - db::BOP_CLEAVES1) * 4 + (m & 3),
        db::BOP_APPLE_LEAVES => 68,
        db::BOP_PERSIMMON_LEAVES => 69,
        db::BOP_PETALS => 70 + (m & 1),
        db::BOP_BAMBOO => 72,
        db::BOP_CORAL1 if db::meta(b) < 12 => 73,
        db::BOP_CORAL2 => 73,
        db::BOP_CORAL1 => 74,
        _ => 0,
    }
}

/// Replays the populates of chunks (cx0 - 1 ..= cx1, cz0 - 1 ..= cz1) z-major on one fresh [`DecoWorld`] and
/// returns the visible decoration cells `(x, y, z, code)` inside chunks (cx0 ..= cx1, cz0 ..= cz1) (last write per
/// position; boulders only near the surface; snow layers only on decoration).
fn region_cells(
    p: &BiomeProvider,
    h: &dyn Heights,
    gates: &PreGates,
    (cx0, cz0, cx1, cz1): (i32, i32, i32, i32),
) -> Vec<(i32, i32, i32, u8)> {
    let mut w = region_world(p, h, gates, (cx0, cz0, cx1, cz1));
    region_cells_of(&mut w, (cx0, cz0, cx1, cz1))
}

/// The decorated world after replaying the populates of chunks (cx0 - 1 ..= cx1, cz0 - 1 ..= cz1) z-major.
pub fn region_world<'a>(
    p: &'a BiomeProvider,
    h: &'a dyn Heights,
    gates: &PreGates,
    (cx0, cz0, cx1, cz1): (i32, i32, i32, i32),
) -> DecoWorld<'a> {
    let seed = p.seed();
    let mut m = PopModel::new(p, h, seed);
    m.gates = gates.clone();
    m.load_structures(cx0 - 2, cz0 - 2, cx1 + 1, cz1 + 1);
    let mut w = DecoWorld::new(p, h, seed);
    for pcz in cz0 - 1..=cz1 {
        for pcx in cx0 - 1..=cx1 {
            let river = -p.river_strength(pcx * 16 + 16, pcz * 16 + 16);
            let out = m.populate(pcx, pcz);
            let log = match out {
                Outcome::Certain(l) | Outcome::Uncertain(_, l) => l,
            };
            let (mut r, n) = match m.last_deco_start.clone() {
                Some(s) => s,
                // prefix not modelled: decorate anyway on the chunk's populate Random (plausible, not exact)
                None => (m.rand(pcx, pcz), log.len()),
            };
            apply_prefix(&mut w, &log[..n.min(log.len())]);
            let border = border(p, h, pcx, pcz);
            for &(id, s) in &border {
                if is_ocean_class(&p.table.info(id as _).class) {
                    continue;
                }
                decorate_biome(&mut w, &mut r, id, pcx, pcz, s, river);
            }
            snow_pass(&mut w, pcx, pcz, &border);
            ocean_after_ice(&mut w, &mut r, pcx, pcz, &border);
        }
    }
    w
}

/// Every block the replay changed inside chunks (cx0 ..= cx1, cz0 ..= cz1) (last write per position; prefix
/// changes included: lakes, dungeons, dirt / gravel blobs), unfiltered.
pub fn region_blocks(
    p: &BiomeProvider,
    h: &dyn Heights,
    gates: &PreGates,
    (cx0, cz0, cx1, cz1): (i32, i32, i32, i32),
) -> FxMap<(i32, i32, i32), Db> {
    let w = region_world(p, h, gates, (cx0, cz0, cx1, cz1));
    let inside =
        |x: i32, z: i32| (cx0..=cx1).contains(&(x >> 4)) && (cz0..=cz1).contains(&(z >> 4));
    let mut last: FxMap<(i32, i32, i32), Db> = FxMap::default();
    for &(x, y, z, b) in &w.log {
        if inside(x, z) {
            last.insert((x, y, z), b);
        }
    }
    last
}

fn region_cells_of(
    w: &mut DecoWorld,
    (cx0, cz0, cx1, cz1): (i32, i32, i32, i32),
) -> Vec<(i32, i32, i32, u8)> {
    let inside =
        |x: i32, z: i32| (cx0..=cx1).contains(&(x >> 4)) && (cz0..=cz1).contains(&(z >> 4));
    let mut last: FxMap<(i32, i32, i32), Db> = FxMap::default();
    for &(x, y, z, b) in &w.log {
        if inside(x, z) {
            last.insert((x, y, z), b);
        }
    }
    let mut cells = Vec::new();
    for (&(x, y, z), &b) in &last {
        let mut c = code(b);
        // boulders only above ground (dungeon / geode cobble stays hidden)
        if matches!(c, 15 | 16) && y < w.height(x, z) - 6 {
            c = 0;
        }
        // snow layers: only where they sit on decoration (ground snow is the terrain's)
        if c == 17 && code(w.get(x, y - 1, z)) == 0 {
            c = 0;
        }
        if c != 0 {
            cells.push((x, y, z, c));
        }
    }
    cells
}

/// Decoration of chunk (cx, cz): `(y0, height, codes)`, codes indexed `((y - y0) · 16 + lz) · 16 + lx`.
pub fn chunk_voxels(
    p: &BiomeProvider,
    h: &dyn Heights,
    gates: &PreGates,
    cx: i32,
    cz: i32,
) -> (i32, i32, Vec<u8>) {
    pack_voxels(region_cells(p, h, gates, (cx, cz, cx, cz)))
}

/// Cells of one chunk → `(y0, height, codes)` as [`chunk_voxels`].
fn pack_voxels(cells: Vec<(i32, i32, i32, u8)>) -> (i32, i32, Vec<u8>) {
    if cells.is_empty() {
        return (0, 0, Vec::new());
    }
    let y0 = cells.iter().map(|c| c.1).min().unwrap();
    let y1 = cells.iter().map(|c| c.1).max().unwrap() + 1;
    let hh = y1 - y0;
    let mut v = vec![0u8; (256 * hh) as usize];
    for (x, y, z, c) in cells {
        v[(((y - y0) * 16 + (z & 15)) * 16 + (x & 15)) as usize] = c;
    }
    (y0, hh, v)
}

/// Top view of the decoration of `n × n` chunks from (cx0, cz0): per column (index `z · 16n + x`, local) the
/// highest visible decoration code and its y (code 0 = none). One populate replay for the whole group.
pub fn region_tops(
    p: &BiomeProvider,
    h: &dyn Heights,
    gates: &PreGates,
    cx0: i32,
    cz0: i32,
    n: i32,
) -> Vec<(u8, u8)> {
    let side = 16 * n;
    let mut out = vec![(0u8, 0u8); (side * side) as usize];
    for (x, y, z, c) in region_cells(p, h, gates, (cx0, cz0, cx0 + n - 1, cz0 + n - 1)) {
        let i = ((z - cz0 * 16) * side + (x - cx0 * 16)) as usize;
        if out[i].0 == 0 || y > out[i].1 as i32 {
            out[i] = (c, y.clamp(0, 255) as u8);
        }
    }
    out
}

/// One populate of an ordered replay ([`replay_ordered`]).
#[derive(Clone, Debug)]
pub struct ReplayStep {
    pub cx: i32,
    pub cz: i32,
    /// the populate prefix was modelled up to the border loop (else the chunk's populate Random was used)
    pub prefix_modelled: bool,
    /// Random state (scrambled, `JavaRandom::state`) entering the border loop as replayed (after any override)
    pub entry: i64,
    /// the model's own entry state (before an override)
    pub model_entry: i64,
    /// per border biome: (rwg id, strength, state before its `rDecorate`, ported)
    pub biomes: Vec<(u16, f32, i64, bool)>,
    /// Random state entering ANIMALS (scrambled), as modelled
    pub post_entry: i64,
    /// DreamMaster oil deposit of this populate (deposit, confidence)
    pub oil: Option<(crate::deco::oil::OilDeposit, f32)>,
}

/// Replays the populates in `order` (chunk coordinates, as the game ran them) on one fresh [`DecoWorld`].
/// `entry(cx, cz)` may override the Random state entering the border loop (a recorded `Random.seed`, scrambled).
pub fn replay_ordered<'a>(
    p: &'a BiomeProvider,
    h: &'a dyn Heights,
    gates: &PreGates,
    order: &[(i32, i32)],
    entry: &dyn Fn(i32, i32) -> Option<i64>,
    ocean_entry: &dyn Fn(i32, i32) -> Option<i64>,
) -> (DecoWorld<'a>, Vec<ReplayStep>) {
    replay_ordered_with(p, h, gates, order, entry, ocean_entry, true)
}

/// [`replay_ordered`]; `use_ocean_entry = false` only checks the modelled ocean state against `ocean_entry`
/// (`DecoWorld::ocean_entry_check`) instead of starting the deferred ocean decorators from it.
pub fn replay_ordered_with<'a>(
    p: &'a BiomeProvider,
    h: &'a dyn Heights,
    gates: &PreGates,
    order: &[(i32, i32)],
    entry: &dyn Fn(i32, i32) -> Option<i64>,
    ocean_entry: &dyn Fn(i32, i32) -> Option<i64>,
    use_ocean_entry: bool,
) -> (DecoWorld<'a>, Vec<ReplayStep>) {
    let seed = p.seed();
    let mut m = PopModel::new(p, h, seed);
    m.gates = gates.clone();
    m.stateful_shafts = true;
    m.stateful_world = true;
    if let (Some(x0), Some(x1), Some(z0), Some(z1)) = (
        order.iter().map(|c| c.0).min(),
        order.iter().map(|c| c.0).max(),
        order.iter().map(|c| c.1).min(),
        order.iter().map(|c| c.1).max(),
    ) {
        m.load_structures(x0 - 1, z0 - 1, x1 + 1, z1 + 1);
    }
    let mut w = DecoWorld::new(p, h, seed);
    w.use_recorded_ocean_entry = use_ocean_entry;
    // BOP decorator phase marks (`DecoWorld::phases`) for the validation harness
    w.tracing = true;
    let mut steps = Vec::with_capacity(order.len());
    for &(pcx, pcz) in order {
        let river = -p.river_strength(pcx * 16 + 16, pcz * 16 + 16);
        if let Some(s) = ocean_entry(pcx, pcz) {
            w.ocean_entry.insert((pcx, pcz), s);
        }
        let out = m.populate(pcx, pcz);
        let log = match out {
            Outcome::Certain(l) | Outcome::Uncertain(_, l) => l,
        };
        let modelled = m.last_deco_start.is_some();
        let (mut r, n) = match m.last_deco_start.clone() {
            Some(s) => s,
            None => (m.rand(pcx, pcz), log.len()),
        };
        apply_prefix(&mut w, &log[..n.min(log.len())]);
        let model_entry = r.state();
        if let Some(s) = entry(pcx, pcz) {
            r = crate::java_random::JavaRandom::new(s ^ 0x5DEECE66D);
        }
        let mut step = ReplayStep {
            cx: pcx,
            cz: pcz,
            prefix_modelled: modelled,
            entry: r.state(),
            model_entry,
            biomes: Vec::new(),
            post_entry: 0,
            oil: None,
        };
        let border = border(p, h, pcx, pcz);
        for &(id, s) in &border {
            if is_ocean_class(&p.table.info(id as _).class) {
                continue;
            }
            let before = r.state();
            let ok = decorate_biome(&mut w, &mut r, id, pcx, pcz, s, river);
            step.biomes.push((id, s, before, ok));
        }
        snow_pass(&mut w, pcx, pcz, &border);
        let n_oil = w.oil.len();
        ocean_after_ice(&mut w, &mut r, pcx, pcz, &border);
        step.post_entry = w.post_entry;
        step.oil = w.oil.get(n_oil).map(|o| (o.1, o.2));
        steps.push(step);
    }
    (w, steps)
}

/// Top view (as [`region_tops`]) of chunks `(cx0, cz0) ..= (cx1, cz1)` after replaying the populates in `order`
/// (as the game ran them, e.g. `gtorder::LoadSim` for a fresh world's spawn area) on one stateful world: per column,
/// index `(z - cz0·16) · 16(cx1 - cx0 + 1) + (x - cx0·16)`, the highest visible decoration code and its y.
/// Only columns whose writers (populates of the chunk and its −x / −z neighbours) are all in `order` are complete.
pub fn ordered_region_tops(
    p: &BiomeProvider,
    h: &dyn Heights,
    gates: &PreGates,
    order: &[(i32, i32)],
    (cx0, cz0, cx1, cz1): (i32, i32, i32, i32),
) -> Vec<(u8, u8)> {
    let (mut w, _) = replay_ordered(p, h, gates, order, &|_, _| None, &|_, _| None);
    let (sx, sz) = (16 * (cx1 - cx0 + 1), 16 * (cz1 - cz0 + 1));
    let mut out = vec![(0u8, 0u8); (sx * sz).max(0) as usize];
    for (x, y, z, c) in region_cells_of(&mut w, (cx0, cz0, cx1, cz1)) {
        let i = ((z - cz0 * 16) * sx + (x - cx0 * 16)) as usize;
        if out[i].0 == 0 || y > out[i].1 as i32 {
            out[i] = (c, y.clamp(0, 255) as u8);
        }
    }
    out
}

/// Per-chunk voxels (as [`chunk_voxels`]) of chunks `(cx0, cz0) ..= (cx1, cz1)`, z-major, after replaying the
/// populates in `order` on one stateful world (as [`ordered_region_tops`]; same completeness caveat).
pub fn ordered_region_voxels(
    p: &BiomeProvider,
    h: &dyn Heights,
    gates: &PreGates,
    order: &[(i32, i32)],
    (cx0, cz0, cx1, cz1): (i32, i32, i32, i32),
) -> Vec<(i32, i32, Vec<u8>)> {
    let (mut w, _) = replay_ordered(p, h, gates, order, &|_, _| None, &|_, _| None);
    let (nx, nz) = ((cx1 - cx0 + 1).max(0), (cz1 - cz0 + 1).max(0));
    let mut per: Vec<Vec<(i32, i32, i32, u8)>> = vec![Vec::new(); (nx * nz) as usize];
    for c in region_cells_of(&mut w, (cx0, cz0, cx1, cz1)) {
        per[(((c.2 >> 4) - cz0) * nx + ((c.0 >> 4) - cx0)) as usize].push(c);
    }
    per.into_iter().map(pack_voxels).collect()
}

/// DreamMaster oil deposits whose populating chunk lies in `(cx0, cz0) ..= (cx1, cz1)`, from one canonical-order
/// replay of the rect (+ the −x / −z ring, as [`region_world`]): `(populating chunk, deposit, confidence)`.
pub fn region_oil(
    p: &BiomeProvider,
    h: &dyn Heights,
    gates: &PreGates,
    (cx0, cz0, cx1, cz1): (i32, i32, i32, i32),
) -> Vec<((i32, i32), crate::deco::oil::OilDeposit, f32)> {
    let w = region_world(p, h, gates, (cx0, cz0, cx1, cz1));
    w.oil
        .into_iter()
        .filter(|((cx, cz), _, _)| (cx0..=cx1).contains(cx) && (cz0..=cz1).contains(cz))
        .collect()
}

/// Replayed blocks by position.
pub type BlockMap = FxMap<(i32, i32, i32), Db>;

/// Lazily replayed decoration in `n × n`-chunk groups ([`region_blocks`] per group), for the estimate models
/// (Roguelike, Thaumcraft rings): `at` = the replayed write at a block, if any. Canonical populate order, so
/// plausible rather than exact.
pub struct DecoOverlay {
    pub gates: PreGates,
    pub n: i32,
    groups: FxMap<(i32, i32), std::rc::Rc<BlockMap>>,
    /// replays done (cost accounting)
    pub replays: usize,
}

impl DecoOverlay {
    pub fn new(gates: PreGates, n: i32) -> Self {
        DecoOverlay {
            gates,
            n: n.max(1),
            groups: FxMap::default(),
            replays: 0,
        }
    }

    pub fn at(&mut self, p: &BiomeProvider, h: &dyn Heights, x: i32, y: i32, z: i32) -> Option<Db> {
        let n = self.n;
        let g = ((x >> 4).div_euclid(n), (z >> 4).div_euclid(n));
        if !self.groups.contains_key(&g) {
            if self.groups.len() > 64 {
                self.groups.clear();
            }
            let rect = (g.0 * n, g.1 * n, g.0 * n + n - 1, g.1 * n + n - 1);
            self.groups
                .insert(g, std::rc::Rc::new(region_blocks(p, h, &self.gates, rect)));
            self.replays += 1;
        }
        self.groups[&g].get(&(x, y, z)).copied()
    }
}
