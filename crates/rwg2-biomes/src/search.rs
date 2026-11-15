//! Seed search: does a seed satisfy every condition ("X within R of the centre", or negated: "no X within R")?
//!
//! - Conditions run cheapest-first; a positive one stops at its nearest hit (candidates walk outward by distance),
//!   a negated one has to scan the whole disc.
//! - Biome conditions sample a grid of `step` blocks inside the radius. Small biomes (75-block patches around
//!   `smallBiomePoints`) are also tested exactly at every patch centre in range, so they are never missed.
//! - Volcanoes / lava caves come from the continental landmark field (centre + gates), strongholds, LootGames and
//!   Roguelike areas are seed maths (exact). Thaumcraft structures and GT ore veins use the same models as the map
//!   (rolls / candidates exact, terrain checks estimated).
//! - Centre: the origin, the estimated world spawn (`Center::Spawn`, overworld only, needs a spawn estimator) or a
//!   fixed point (`Center::Point`).
//! - Other dimensions (`Query::dim` = -1 Nether, 7 Twilight Forest): biome (sampled on the game's 1:4 biome
//!   generation grid at `step`), TF major features (exact), Nether fortress starts (exact), aura nodes (wild /
//!   silverwood, same replay as the map), GT ore veins (attempt 0 of the dimension's live vein list: the vein the
//!   map shows; right ~80%).
//! - `Target::Adjacent`: a sample of biome A within the radius that has biome B within `dist` blocks (B checked on
//!   a grid of `clamp(dist / 4, 4, step)` blocks; small overworld patches only where a grid point lands on them).
//! - Relations (`Cond::near`): a condition can name helper conditions (`Cond::helper`), whose radius is measured
//!   from each of its own candidates instead of the centre: "a village within 2000 of the centre that has a
//!   stronghold within 300 of it" (or, negated helper, "no stronghold within 300 of it"). Candidates are tried
//!   nearest first; the first one every helper accepts is the hit. A helper of the same target type looks for
//!   another instance (the candidate itself does not count).

use crate::config::RwgConfig;
use crate::continental::NONE;
use crate::dungeons::Terrain;
use crate::forestry::ClimateTable;
use crate::gtores::{is_oreseed, VeinTable, GT_GTNH};
use crate::provider::{BiomeId, BiomeProvider, BiomeTable, Heights, NoHeights, WorldType};
use crate::thaumcraft::{TcGen, TcKind, TC_GTNH};

#[derive(Clone, Debug, PartialEq)]
pub enum Target {
    /// Any of these RWG ids (an in-game biome or a bee climate maps to several RWG ids).
    Biome(Vec<BiomeId>),
    Volcano,
    LavaCave,
    /// Stronghold end-portal frame (exact piece port; was the start-chunk centre).
    Stronghold,
    /// Village start (VillageNames, GTNH config; start chunk centre). Exact start + biome gate.
    Village,
    /// LootGames dungeon chunk centre (GTNH rhomb size 20).
    LootGames,
    /// Roguelike anchor (search centre chunk*16+4); the dungeon lands 40-100 blocks from it, or not at all.
    Roguelike,
    /// Thaumcraft structure passing the terrain model (Mound, Ring, Hilltop, Totem).
    Thaumcraft(TcKind),
    /// GT ore vein (index into the vein table), at its oreseed chunk centre.
    OreVein(usize),
    /// [`Target::OreVein`] without the placement test in the Nether / Twilight Forest (first candidate only:
    /// right ~69% / ~83%, ~1000× faster). Same as `OreVein` in the overworld.
    OreVeinFast(usize),
    /// Thaumcraft aura node (wild / ring / totem / silverwood) holding at least `min` of `aspect` (aspect id as in
    /// `tc_nodes::Aspect`; `u8::MAX` = any node). Node contents: exact replay, environment counts estimated.
    TcAspect {
        aspect: u8,
        min: u16,
    },
    /// GT underground fluid field (8x8 chunks) of this fluid (`usize::MAX` = any) whose average yield is at least
    /// `min_lpo` L per pump operation; hit = field centre. Exact (seed maths).
    Fluid {
        fluid: usize,
        min_lpo: i32,
    },
    /// Biome A (ids as in [`Target::Biome`]) with biome B within `dist` blocks of it; hit = the A sample.
    Adjacent {
        a: Vec<BiomeId>,
        b: Vec<BiomeId>,
        dist: i32,
    },
    /// Twilight Forest major feature with one of these `TFFeature` ids (centre block; exact).
    TfFeature(Vec<u8>),
    /// Nether fortress start (start crossing centre; exact).
    Fortress,
    /// Nether fortress with at least this many blaze spawners (`Throne` pieces; exact piece port); hit = start
    /// crossing centre. Only starts within the radius are laid out (~66 µs each).
    FortressSpawners(u8),
    /// Village (VillageNames start, provider gate) of this style; hit = start chunk centre.
    VillageStyle(crate::villages::Style),
    /// Thaumcraft aura node of this type; `modifier`: `None` = any, `Some(None)` = no modifier, `Some(Some(m))` =
    /// that modifier. Valid nodes only (as [`Target::TcAspect`]); any dimension.
    TcNodeType {
        node_type: crate::tc_nodes::NodeType,
        modifier: Option<Option<crate::tc_nodes::NodeModifier>>,
    },
    /// Stronghold whose end-portal frame lies under ocean (`true`) or under land (`false`). Rule: exact chunkgen
    /// terrain top at the frame column; ocean iff top ≤ 61 (sea water fills up to y 62), i.e. there is sea water
    /// above that column. Population (lakes, trees) is ignored.
    StrongholdSurface {
        ocean: bool,
    },
    /// Mineshaft start (Et Futurum `MapGenMesaMineshaft`, exact); hit = start chunk centre.
    Mineshaft,
    /// Railcraft / NHCore abyssal geode with at least `min_diamond` dark diamond ores; `kind` 0 any, 1 mountain
    /// (NHCore), 2 deep ocean (Railcraft). Exact replay (`population::PopModel::geode_at`); chunks are pre-filtered
    /// by the populate Random's 0.3 rolls and the provider biome at the chunk centre and corners. Needs
    /// `SearchData::pop_gates`. Hit = geode centre.
    Geode {
        min_diamond: u16,
        kind: u8,
    },
    /// Underground lake (vanilla `WorldGenLakes` in RWG2 populate; never at the surface): `liquid` 0 any, 1 lava,
    /// 2 water; liquid top Y ≥ `min_y`; at least `min_cells` liquid blocks; `open` = must touch a cave. Exact
    /// populate replay up to the lake rolls ([`crate::population::PopModel::lakes_at`]); chunks whose Random can't
    /// roll a lake are skipped before terrain; chunks with a structure piece or near a village never match. Needs
    /// `SearchData::pop_gates`. Hit = lake centre.
    Lake {
        liquid: u8,
        min_y: i32,
        min_cells: u32,
        open: bool,
    },
    /// A chest holding at least `min_count` items whose unlocalized name (`item.diamond@0`) contains one of `items`
    /// (empty = any chest). `source` 0 any, 1 vanilla monster room, 2 mineshaft chest cart. Exact populate replay
    /// ([`crate::population::PopModel::dungeons_at`]); room-only chunks need the whole prefix, carts only chunks a
    /// mineshaft piece reaches. Needs `SearchData::pop_gates`. Hit = the chest.
    Loot {
        source: u8,
        items: Vec<String>,
        min_count: u32,
    },
    /// HEE biome island (End) with this biome (0 Infested Forest, 1 Burning Mountains, 2 Enchanted Island) and
    /// content variation id ([`crate::end::ISLAND_VARIATIONS`]), `None` = any; hit = island centre
    /// (start·16 + 112). Exact position and data; the island itself can be missing (HEE save bug).
    EndIsland {
        biome: Option<u8>,
        variation: Option<u8>,
    },
    /// HEE dungeon tower (End); hit = tower centre (start·16 + 16). Exact.
    EndTower,
    /// Draconic Evolution comet (End, default config) of at least this size (2..9); hit = core. Exact.
    EndComet {
        min_size: u8,
    },
    /// Flat area in a biome (overworld): a connected patch of these RWG ids (4-neighbour on a [`FLAT_STEP`] grid,
    /// followed past the radius up to [`FLAT_PAD`]) with at least `flat.min_pct` % of its samples in one height band
    /// of ±`flat.tol` blocks and at least `flat.min_area` blocks² in that band. Heights = the zoomed-out terrain model
    /// (`gtorder::ApproxTerrain`, rms ~2–5 blocks vs exact chunks, no caves / trees). Hit = the in-band sample
    /// nearest the centre (within the radius); [`Hit::flat`] carries the band.
    FlatBiome {
        ids: Vec<BiomeId>,
        flat: Flat,
    },
    /// Surface block (overworld): at least `min` columns within the radius whose exact top block
    /// ([`crate::surface`]: surface pass, caves, lava landmark, and the population snow / ice pass when
    /// [`SearchData::climate`] is set) is one of `blocks` ([`crate::surface::blk`] codes); `water`: the floor
    /// under water counts too. Trees / plants are not modelled. Hit = the nearest matching column.
    SurfaceBlock {
        blocks: Vec<u8>,
        min: u32,
        water: bool,
    },
}

/// Grid of the flat-area patch scan (blocks).
pub const FLAT_STEP: i32 = 32;
/// How far past the condition radius a patch is followed (blocks; larger patches are measured truncated).
pub const FLAT_PAD: i32 = 2048;
const FLAT_MAX_CELLS: usize = 120_000;

/// Flatness rule of [`Target::FlatBiome`].
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Flat {
    /// Band half-width: heights within ±tol of the band centre (band = 2·tol + 1 Y levels).
    pub tol: i32,
    /// Minimum share of the patch in the band, percent.
    pub min_pct: u8,
    /// Minimum in-band area, blocks².
    pub min_area: i64,
    /// Only land counts as in band (top ≥ 63, above sea level).
    pub land: bool,
}

/// What a [`Target::FlatBiome`] hit measured.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct FlatInfo {
    /// Band, inclusive (ground Y = top + 1).
    pub y_lo: i32,
    pub y_hi: i32,
    /// Share of the patch in the band, percent (rounded down).
    pub pct: u8,
    /// Patch area and in-band area, blocks².
    pub patch_area: i64,
    pub flat_area: i64,
    /// The patch was larger than the scan box (radius + [`FLAT_PAD`]) or [`FLAT_MAX_CELLS`].
    pub truncated: bool,
}

impl Target {
    /// Does a predicted node satisfy this node condition (`TcAspect` / `TcNodeType`)?
    fn node_ok(&self, i: &crate::tc_nodes::NodeInfo) -> bool {
        match self {
            Target::TcAspect { aspect, min } => {
                *aspect == u8::MAX || i.amount(crate::tc_nodes::Aspect(*aspect)) >= (*min).max(1)
            }
            Target::TcNodeType {
                node_type,
                modifier,
            } => i.node_type == *node_type && modifier.map_or(true, |m| i.modifier == m),
            _ => false,
        }
    }
}

#[derive(Clone, Debug)]
pub struct Cond {
    pub target: Target,
    pub radius: i32,
    /// "No such target within radius".
    pub negate: bool,
    /// Helper conditions (indices into `Query::conds`) every candidate of this condition has to satisfy, measured
    /// from the candidate ("… with a stronghold within 300 of it").
    pub near: Vec<usize>,
    /// Measured from a candidate of another condition (listed in its `near`), not from the centre; not checked on
    /// its own.
    pub helper: bool,
}

impl Cond {
    pub fn new(target: Target, radius: i32, negate: bool) -> Self {
        Self {
            target,
            radius,
            negate,
            near: Vec::new(),
            helper: false,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Center {
    Origin,
    Spawn,
    Point(i32, i32),
}

#[derive(Clone, Debug)]
pub struct Query {
    pub world_type: WorldType,
    /// CONTINENTS: continent value at the centre >= 0 (land). Plain RWG2 has no oceans: always true.
    pub land_at_spawn: bool,
    pub conds: Vec<Cond>,
    /// Grid spacing for biome sampling (blocks).
    pub step: i32,
    pub center: Center,
    /// 0 overworld, -1 Nether, 7 Twilight Forest, 1 End (End* targets + ore veins). In other dimensions [`Target::Biome`] / [`Target::Adjacent`]
    /// ids are in-game biome ids (not RWG ids) and `Center::Spawn` means the origin.
    pub dim: i32,
}

/// Hit for a positive condition (index into `Query::conds`), nearest to the centre.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Hit {
    pub cond: usize,
    pub x: i32,
    pub z: i32,
    /// [`Target::FlatBiome`] hits only.
    pub flat: Option<FlatInfo>,
}

/// Result of a matching seed.
#[derive(Clone, Debug, PartialEq)]
pub struct Match {
    pub center: (i32, i32),
    pub hits: Vec<Hit>,
}

/// Spawn estimator hook: (provider, heights, seed) -> spawn x, z.
pub type SpawnFn = fn(&BiomeProvider, &dyn Heights, i64) -> (i32, i32);

/// Optional data some targets need.
#[derive(Clone, Default)]
pub struct SearchData {
    pub climate: Option<ClimateTable>,
    pub veins: Option<VeinTable>,
    pub fluids: Option<crate::gtfluids::FluidTable>,
    pub spawn: Option<SpawnFn>,
    /// BiomeDictionary gates of the populate-event consumers (for [`Target::Geode`]).
    pub pop_gates: Option<crate::population::PreGates>,
}

/// Per biome condition: membership bitmap over RWG ids + grid offsets (nearest first) + uses small points.
type BiomeData = (Vec<bool>, Vec<(i32, i32)>, bool);

/// Per adjacency condition: A / B membership, A grid offsets (nearest first), B check offsets and their step.
struct AdjData {
    a: Vec<bool>,
    b: Vec<bool>,
    offsets: Vec<(i32, i32)>,
    near: Vec<(i32, i32)>,
    step: i32,
    dist: i32,
}

fn bitmap(ids: &[BiomeId], len: usize) -> Vec<bool> {
    let n = ids
        .iter()
        .map(|&i| i as usize + 1)
        .max()
        .unwrap_or(0)
        .max(len)
        .max(1);
    let mut m = vec![false; n];
    for &id in ids {
        m[id as usize] = true;
    }
    m
}

fn member(m: &[bool], id: i32) -> bool {
    id >= 0 && m.get(id as usize).copied().unwrap_or(false)
}

/// Biome ids of another dimension over a block rectangle, from the game's 1:4 generation layer.
/// Per-seed state of an other-dimension check (built lazily).
#[derive(Default)]
struct DimCtx {
    tf: Option<crate::twilight::TwilightForest>,
    nb: Option<crate::nether::NetherBiomes>,
    /// Lazy 1:4 biome cache: `tile` × `tile` cells per entry (≤ 1: single cells), filled on first use. The
    /// layer stacks give the same value for a cell whatever rect it is requested in.
    tile: i32,
    tiles: crate::jmath::FxMap<(i32, i32), Vec<i32>>,
    cells: crate::jmath::FxMap<(i32, i32), i32>,
    nt: Option<crate::dimterrain::NetherTerrain>,
    tt: Option<crate::dimterrain::TfTerrain>,
    /// GT stone on the exact dimension terrain (ore-vein placement test).
    gs: Option<crate::gtdims::DimStone>,
    /// Resolved oreseeds of this seed (vein index or none), shared by every ore condition.
    veins: crate::jmath::FxMap<(i32, i32), Option<usize>>,
    /// End blocks for the GT placement test in the End.
    eb: Option<crate::gtend::EndBlocks>,
}

impl DimCtx {
    fn quarter(&mut self, dim: i32, seed: i64, x4: i32, z4: i32, w: i32, h: i32) -> Vec<i32> {
        if dim == 7 {
            self.tf
                .get_or_insert_with(|| {
                    crate::twilight::TwilightForest::new(crate::twilight::tf_seed(seed, ""))
                })
                .biomes_quarter(x4, z4, w, h)
        } else {
            self.nb
                .get_or_insert_with(|| crate::nether::NetherBiomes::new(seed))
                .biomes_quarter(x4, z4, w, h)
        }
    }

    /// Biome at block (x, z) (1:4 cell), computed lazily (single cells or `tile`² tiles) and cached.
    fn biome(&mut self, dim: i32, seed: i64, x: i32, z: i32) -> i32 {
        let (i, j) = (x >> 2, z >> 2);
        let t = self.tile;
        if t <= 1 {
            if let Some(&v) = self.cells.get(&(i, j)) {
                return v;
            }
            let v = self.quarter(dim, seed, i, j, 1, 1)[0];
            self.cells.insert((i, j), v);
            return v;
        }
        let key = (i.div_euclid(t), j.div_euclid(t));
        if !self.tiles.contains_key(&key) {
            let v = self.quarter(dim, seed, key.0 * t, key.1 * t, t, t);
            self.tiles.insert(key, v);
        }
        self.tiles[&key][(j.rem_euclid(t) * t + i.rem_euclid(t)) as usize]
    }
}

/// Precomputed per-query data, reusable across seeds.
pub struct Searcher {
    pub query: Query,
    table: BiomeTable,
    cfg: RwgConfig,
    data: SearchData,
    biome: Vec<Option<BiomeData>>,
    adj: Vec<Option<AdjData>>,
    order: Vec<usize>,
    /// Vein list of `query.dim` when it is not the overworld.
    dim_veins: Option<VeinTable>,
    /// Tile size (1:4 cells) of the lazy other-dimension biome cache: single cells when the samples are sparse
    /// (grid step ≥ 48 blocks, the cheaper way to sample them), 16² tiles for dense sampling (adjacency).
    dim_tile: i32,
    /// Band of the last accepted [`Target::FlatBiome`] candidate per condition (read by the hit that follows).
    flat_last: std::cell::RefCell<Vec<Option<FlatInfo>>>,
    /// Surface tables when a [`Target::SurfaceBlock`] condition exists, and per such condition the RWG ids whose
    /// painters can write one of its blocks (`None`: no pre-filter).
    surface: Option<(crate::surface::SurfaceTable, crate::spawn::SurfaceModel)>,
    surface_ids: Vec<Option<Vec<bool>>>,
}

fn d2(x: i32, z: i32, c: (i32, i32)) -> i64 {
    let (dx, dz) = (x as i64 - c.0 as i64, z as i64 - c.1 as i64);
    dx * dx + dz * dz
}

/// Nearest point within `r2` of `c` that `accept` takes (candidates nearest first).
fn nearest_acc(
    c: (i32, i32),
    r2: i64,
    accept: &mut dyn FnMut(i32, i32) -> bool,
    v: Vec<(i32, i32)>,
) -> Option<(i32, i32)> {
    let mut v: Vec<(i32, i32)> = v.into_iter().filter(|&(x, z)| d2(x, z, c) <= r2).collect();
    v.sort_by_key(|&(x, z)| d2(x, z, c));
    v.into_iter().find(|&(x, z)| accept(x, z))
}

/// [`Target::FlatBiome`] scan around `c` (biome and height sources passed in, so tests can swap them): patches of
/// `inside` cells on the absolute [`FLAT_STEP`] grid, seeded from cells within `r` of `c` nearest first; the first
/// patch passing `flat` whose nearest in-band cell within `r` is accepted wins. The band is the ±tol window holding
/// the most cells (lowest such window on ties).
pub fn flat_patch_search(
    c: (i32, i32),
    r: i32,
    flat: &Flat,
    inside: &mut dyn FnMut(i32, i32) -> bool,
    top: &mut dyn FnMut(i32, i32) -> i32,
    accept: &mut dyn FnMut(i32, i32) -> bool,
) -> Option<((i32, i32), FlatInfo)> {
    let s = FLAT_STEP;
    let cell_area = (s as i64) * (s as i64);
    let r2 = r as i64 * r as i64;
    let lim = r.saturating_add(FLAT_PAD);
    let (i0, j0) = ((c.0 - r).div_euclid(s), (c.1 - r).div_euclid(s));
    let (i1, j1) = ((c.0 + r).div_euclid(s), (c.1 + r).div_euclid(s));
    let (bi0, bj0) = ((c.0 - lim).div_euclid(s), (c.1 - lim).div_euclid(s));
    let (bi1, bj1) = ((c.0 + lim).div_euclid(s), (c.1 + lim).div_euclid(s));
    let mut memo: crate::jmath::FxMap<(i32, i32), bool> = Default::default();
    let mut is = |i: i32, j: i32, memo: &mut crate::jmath::FxMap<(i32, i32), bool>| {
        *memo.entry((i, j)).or_insert_with(|| inside(i * s, j * s))
    };
    // grid cells within r, nearest first; membership is looked up lazily (a near passing patch ends the scan early)
    let mut order: Vec<(i32, i32)> = Vec::new();
    for j in j0..=j1 {
        for i in i0..=i1 {
            if d2(i * s, j * s, c) <= r2 {
                order.push((i, j));
            }
        }
    }
    order.sort_by_key(|&(i, j)| (d2(i * s, j * s, c), j, i));
    let mut seen: crate::jmath::FxMap<(i32, i32), ()> = Default::default();
    let mut cells: Vec<(i32, i32)> = Vec::new();
    let mut stack: Vec<(i32, i32)> = Vec::new();
    // in-band cells of passing patches, tried nearest first: a patch's first seed is its nearest cell, so every
    // candidate closer than the next unseen seed is final
    let mut pool: Vec<(i64, i32, i32, FlatInfo)> = Vec::new();
    for &sd in &order {
        if seen.contains_key(&sd) || !is(sd.0, sd.1, &mut memo) {
            continue;
        }
        if let Some(v) = flat_drain(&mut pool, d2(sd.0 * s, sd.1 * s, c) - 1, accept) {
            return Some(v);
        }
        cells.clear();
        stack.clear();
        stack.push(sd);
        seen.insert(sd, ());
        let mut truncated = false;
        while let Some((i, j)) = stack.pop() {
            cells.push((i, j));
            for (ni, nj) in [(i + 1, j), (i - 1, j), (i, j + 1), (i, j - 1)] {
                if seen.contains_key(&(ni, nj)) {
                    continue;
                }
                if ni < bi0
                    || ni > bi1
                    || nj < bj0
                    || nj > bj1
                    || cells.len() + stack.len() >= FLAT_MAX_CELLS
                {
                    if is(ni, nj, &mut memo) {
                        truncated = true;
                    }
                    continue;
                }
                if is(ni, nj, &mut memo) {
                    seen.insert((ni, nj), ());
                    stack.push((ni, nj));
                }
            }
        }
        let n = cells.len() as i64;
        if n * cell_area < flat.min_area.max(1) {
            continue;
        }
        let tops: Vec<i32> = cells.iter().map(|&(i, j)| top(i * s, j * s)).collect();
        let Some((info, band)) = flat_band(&tops, cell_area, flat, truncated) else {
            continue;
        };
        for (&(i, j), t) in cells.iter().zip(&tops) {
            let (x, z) = (i * s, j * s);
            let d = d2(x, z, c);
            if d <= r2 && band.contains(t) {
                pool.push((d, x, z, info));
            }
        }
        pool.sort_by_key(|e| (e.0, e.2, e.1));
    }
    flat_drain(&mut pool, i64::MAX, accept)
}

/// The patch of a [`Target::FlatBiome`] hit, as the search measured it: flood from the hit's grid cell with the same
/// scan box (centre `c`, radius `r` + [`FLAT_PAD`], [`FLAT_MAX_CELLS`]); `None` if the hit is not on an inside cell.
/// Returns the cells (grid indices, ×[`FLAT_STEP`] = world) and whether the patch was cut.
pub fn flat_patch_cells(
    c: (i32, i32),
    r: i32,
    hit: (i32, i32),
    inside: &mut dyn FnMut(i32, i32) -> bool,
) -> Option<(Vec<(i32, i32)>, bool)> {
    let s = FLAT_STEP;
    let lim = r.saturating_add(FLAT_PAD);
    let (bi0, bj0) = ((c.0 - lim).div_euclid(s), (c.1 - lim).div_euclid(s));
    let (bi1, bj1) = ((c.0 + lim).div_euclid(s), (c.1 + lim).div_euclid(s));
    let start = (hit.0.div_euclid(s), hit.1.div_euclid(s));
    let mut memo: crate::jmath::FxMap<(i32, i32), bool> = Default::default();
    let mut is = |i: i32, j: i32| *memo.entry((i, j)).or_insert_with(|| inside(i * s, j * s));
    if !is(start.0, start.1) {
        return None;
    }
    let mut seen: crate::jmath::FxMap<(i32, i32), ()> = Default::default();
    seen.insert(start, ());
    let (mut cells, mut stack, mut truncated) = (Vec::new(), vec![start], false);
    while let Some((i, j)) = stack.pop() {
        cells.push((i, j));
        for (ni, nj) in [(i + 1, j), (i - 1, j), (i, j + 1), (i, j - 1)] {
            if seen.contains_key(&(ni, nj)) {
                continue;
            }
            if ni < bi0
                || ni > bi1
                || nj < bj0
                || nj > bj1
                || cells.len() + stack.len() >= FLAT_MAX_CELLS
            {
                if is(ni, nj) {
                    truncated = true;
                }
                continue;
            }
            if is(ni, nj) {
                seen.insert((ni, nj), ());
                stack.push((ni, nj));
            }
        }
    }
    Some((cells, truncated))
}

/// Band of a patch from its cells' tops (see [`flat_band`]): info + band in top values, or `None` if `flat` fails.
pub fn flat_patch_band(
    tops: &[i32],
    flat: &Flat,
    truncated: bool,
) -> Option<(FlatInfo, (i32, i32))> {
    let cell_area = (FLAT_STEP as i64) * (FLAT_STEP as i64);
    flat_band(tops, cell_area, flat, truncated).map(|(i, b)| (i, (*b.start(), *b.end())))
}

/// Best ±tol band of some tops regardless of the pass rule: (band low top, cells in it).
pub fn flat_best_band(tops: &[i32], tol: i32, land: bool) -> (i32, usize) {
    let mut hs = tops.to_vec();
    hs.sort_unstable();
    let w = 2 * tol.max(0);
    let first = if land {
        hs.partition_point(|&t| t < 63)
    } else {
        0
    };
    let (mut best, mut best_lo, mut lo) = (0usize, i32::MIN, first);
    for hi in first..hs.len() {
        while hs[hi] - hs[lo] > w {
            lo += 1;
        }
        if hi + 1 - lo > best {
            best = hi + 1 - lo;
            best_lo = hs[lo];
        }
    }
    (best_lo, best)
}

/// Tries the pooled candidates up to distance² `upto` (nearest first) and drops them.
fn flat_drain(
    pool: &mut Vec<(i64, i32, i32, FlatInfo)>,
    upto: i64,
    accept: &mut dyn FnMut(i32, i32) -> bool,
) -> Option<((i32, i32), FlatInfo)> {
    let k = pool.partition_point(|e| e.0 <= upto);
    let hit = pool[..k]
        .iter()
        .find(|e| accept(e.1, e.2))
        .map(|e| ((e.1, e.2), e.3));
    pool.drain(..k);
    hit
}

/// Band of a patch (its cells' tops): the ±tol window over the tops holding the most cells (lowest on ties);
/// `None` if the patch fails `flat`. Returns the info and the band in top values.
fn flat_band(
    tops: &[i32],
    cell_area: i64,
    flat: &Flat,
    truncated: bool,
) -> Option<(FlatInfo, std::ops::RangeInclusive<i32>)> {
    let n = tops.len() as i64;
    let mut hs = tops.to_vec();
    hs.sort_unstable();
    let w = 2 * flat.tol.max(0);
    let first_land = if flat.land {
        hs.partition_point(|&t| t < 63)
    } else {
        0
    };
    let (mut best, mut best_lo, mut lo) = (0usize, i32::MIN, first_land);
    for hi in first_land..hs.len() {
        while hs[hi] - hs[lo] > w {
            lo += 1;
        }
        if hi + 1 - lo > best {
            best = hi + 1 - lo;
            best_lo = hs[lo];
        }
    }
    if best == 0 {
        return None;
    }
    let flat_area = best as i64 * cell_area;
    let pct = (best as i64 * 100 / n) as u8;
    if pct < flat.min_pct || flat_area < flat.min_area {
        return None;
    }
    let info = FlatInfo {
        y_lo: best_lo + 1,
        y_hi: best_lo + w + 1,
        pct,
        patch_area: n * cell_area,
        flat_area,
        truncated,
    };
    Some((info, best_lo..=best_lo + w))
}

/// Accepts every candidate.
fn any_ok(_: i32, _: i32) -> bool {
    true
}

fn disc_offsets(radius: i32, step: i32) -> Vec<(i32, i32)> {
    let step = step.max(1);
    let n = radius / step;
    let r2 = radius as i64 * radius as i64;
    let mut v = Vec::new();
    for j in -n..=n {
        for i in -n..=n {
            let (x, z) = (i * step, j * step);
            if (x as i64 * x as i64 + z as i64 * z as i64) <= r2 {
                v.push((x, z));
            }
        }
    }
    v.sort_by_key(|&(x, z)| x as i64 * x as i64 + z as i64 * z as i64);
    v
}

/// Chunks whose centre is within `radius` of `c`, nearest first.
fn disc_chunks(c: (i32, i32), radius: i32, off: i32) -> Vec<(i32, i32)> {
    let r = radius + 16;
    let (cx0, cz0, cx1, cz1) = (
        (c.0 - r) >> 4,
        (c.1 - r) >> 4,
        (c.0 + r) >> 4,
        (c.1 + r) >> 4,
    );
    let r2 = radius as i64 * radius as i64;
    let mut v = Vec::new();
    for cz in cz0..=cz1 {
        for cx in cx0..=cx1 {
            if d2(cx * 16 + off, cz * 16 + off, c) <= r2 {
                v.push((cx, cz));
            }
        }
    }
    v.sort_by_key(|&(cx, cz)| d2(cx * 16 + off, cz * 16 + off, c));
    v
}

/// The populate Random's two odd multipliers of a world seed.
fn geode_ab(seed: i64) -> (i64, i64) {
    let mut r = crate::java_random::JavaRandom::new(seed);
    let a = r.next_long() / 2 * 2 + 1;
    let b = r.next_long() / 2 * 2 + 1;
    (a, b)
}

/// Relative cost, for ordering (cheapest first).
fn cost(t: &Target, radius: i32, step: i32) -> i64 {
    let area = (radius as i64 + 64).pow(2);
    match t {
        Target::Stronghold => 0,
        Target::Village => 2,
        Target::LootGames | Target::Roguelike => 1 + area / 1_000_000,
        Target::Volcano | Target::LavaCave => 10 + area / 16_384,
        Target::OreVein(_) => 20 + area / 2_304 * 3,
        Target::OreVeinFast(_) => 1 + area / 2_304,
        Target::Fluid { .. } => 5 + area / 16_384,
        Target::Biome(_) => 10 + area / (step.max(1) as i64).pow(2) * 2,
        Target::Thaumcraft(_) => 1_000 + area / 256 * 20,
        Target::TcAspect { .. } => 2_000 + area / 256 * 40,
        Target::Fortress => 1,
        Target::FortressSpawners(_) => 2 + area / 65_536 * 40,
        Target::VillageStyle(_) => 3 + area / 640_000 * 2,
        Target::TcNodeType { .. } => 2_000 + area / 256 * 40,
        Target::StrongholdSurface { .. } => 5,
        Target::Mineshaft => 1 + area / 1_000_000,
        Target::Geode { .. } => 500 + area / 256 * 10,
        Target::Lake { .. } => 800 + area / 256 * 10,
        Target::Loot { source: 2, .. } => 900 + area / 256 * 4,
        Target::Loot { .. } => 1_500 + area / 256 * 40,
        Target::EndTower => 1 + area / 10_000_000,
        Target::EndIsland { .. } => 2 + area / 5_000_000,
        Target::EndComet { .. } => 5 + area / 65_536,
        Target::TfFeature(_) => 3 + area / 65_536 * 5,
        Target::FlatBiome { .. } => 3_000 + area / (FLAT_STEP as i64).pow(2) * 30,
        Target::SurfaceBlock { .. } => 5_000 + area / 256 * 60,
        Target::Adjacent { dist, .. } => {
            50 + area / (step.max(1) as i64).pow(2) * (4 + (*dist as i64 / 16).min(60))
        }
    }
}

impl Searcher {
    pub fn new(query: Query, table: BiomeTable, cfg: RwgConfig, data: SearchData) -> Self {
        let small_ids: Vec<BiomeId> = table.small.iter().flatten().copied().collect();
        let ow = query.dim == 0;
        let len = if ow { table.biomes.len() } else { 0 };
        let biome = query
            .conds
            .iter()
            .map(|c| match &c.target {
                Target::Biome(ids) => {
                    let small = ow && ids.iter().any(|id| small_ids.contains(id));
                    Some((bitmap(ids, len), disc_offsets(c.radius, query.step), small))
                }
                Target::FlatBiome { ids, .. } if ow => Some((bitmap(ids, len), Vec::new(), false)),
                _ => None,
            })
            .collect();
        let adj = query
            .conds
            .iter()
            .map(|c| match &c.target {
                Target::Adjacent { a, b, dist } => {
                    let st = (dist / 4).clamp(4, query.step.max(4));
                    Some(AdjData {
                        a: bitmap(a, len),
                        b: bitmap(b, len),
                        offsets: disc_offsets(c.radius, query.step),
                        // B grid around the A point's grid cell: one extra step of reach
                        near: disc_offsets(*dist + st, st),
                        step: st,
                        dist: *dist,
                    })
                }
                _ => None,
            })
            .collect();
        let mut order: Vec<usize> = (0..query.conds.len())
            .filter(|&i| !query.conds[i].helper)
            .collect();
        order.sort_by_key(|&i| cost(&query.conds[i].target, query.conds[i].radius, query.step));
        let dim_veins = match query.dim {
            -1 => Some(crate::nether::vein_table(&crate::nether::GT_NETHER_VEINS)),
            7 => Some(crate::nether::vein_table(&crate::nether::GT_TF_VEINS)),
            1 => Some(crate::nether::vein_table(&crate::gtend::GT_END_VEINS)),
            _ => None,
        };
        let dense = query
            .conds
            .iter()
            .any(|c| matches!(c.target, Target::Adjacent { .. }));
        let dim_tile = if dense || query.step < 48 { 16 } else { 1 };
        let flat_last = std::cell::RefCell::new(vec![None; query.conds.len()]);
        let surface = (ow
            && query
                .conds
                .iter()
                .any(|c| matches!(c.target, Target::SurfaceBlock { .. })))
        .then(|| {
            (
                crate::surface::SurfaceTable::new(&table),
                crate::spawn::SurfaceModel::new(&table),
            )
        });
        let surface_ids = query
            .conds
            .iter()
            .map(|c| match (&c.target, &surface) {
                (Target::SurfaceBlock { blocks, .. }, Some((st, _))) => {
                    let cold = |id: BiomeId| {
                        let Some(b) = table.biomes.get(id as usize) else {
                            return true;
                        };
                        let t = |g: u16| {
                            data.climate
                                .as_ref()
                                .and_then(|c| c.get(g))
                                .map(|c| c.temperature)
                        };
                        b.temperature < 0.15 || t(b.river_gen_id).is_some_and(|v| v < 0.15)
                    };
                    st.can_paint(blocks, &cold)
                }
                _ => None,
            })
            .collect();
        Self {
            query,
            table,
            cfg,
            data,
            biome,
            adj,
            order,
            dim_veins,
            dim_tile,
            flat_last,
            surface,
            surface_ids,
        }
    }

    fn take_flat(&self, ci: usize) -> Option<FlatInfo> {
        self.flat_last
            .borrow_mut()
            .get_mut(ci)
            .and_then(Option::take)
    }

    pub fn provider(&self, seed: i64) -> BiomeProvider {
        BiomeProvider::new(
            seed,
            self.query.world_type,
            self.table.clone(),
            self.cfg.clone(),
        )
    }

    pub fn check(&self, seed: i64) -> Option<Match> {
        if self.table.heights.is_empty() {
            self.check_seed(seed, &NoHeights)
        } else {
            self.check_seed(seed, &crate::provider::TableHeights)
        }
    }

    /// [`Searcher::check_with`] for the overworld; other dimensions skip the (costly) RWG provider.
    pub fn check_seed(&self, seed: i64, h: &dyn Heights) -> Option<Match> {
        if self.query.dim == 0 {
            self.check_with(&self.provider(seed), h)
        } else {
            self.check_dim(seed)
        }
    }

    fn fixed_center(&self) -> (i32, i32) {
        match self.query.center {
            Center::Point(x, z) => (x, z),
            _ => (0, 0),
        }
    }

    /// Other dimensions (`query.dim` != 0).
    pub fn check_dim(&self, seed: i64) -> Option<Match> {
        let q = &self.query;
        let center = self.fixed_center();
        let mut ctx = DimCtx {
            tile: self.dim_tile,
            ..Default::default()
        };
        // helper conditions get their own lazy state (the outer search holds `ctx` while they run)
        let mut hctx = DimCtx {
            tile: self.dim_tile,
            ..Default::default()
        };
        let mut hits = Vec::with_capacity(q.conds.len());
        for &ci in &self.order {
            let c = &q.conds[ci];
            let mut helper_hits: Vec<Hit> = Vec::new();
            let found = if c.near.is_empty() {
                self.find_dim(seed, &mut ctx, ci, center, &mut any_ok)
            } else {
                let mut accept = |x: i32, z: i32| {
                    helper_hits.clear();
                    for &j in &c.near {
                        // same target type: another one ("two fortresses within 600"), not the candidate itself
                        let same = q.conds[j].target == c.target;
                        let f = self.find_dim(seed, &mut hctx, j, (x, z), &mut |hx, hz| {
                            !same || (hx, hz) != (x, z)
                        });
                        if f.is_some() == q.conds[j].negate {
                            return false;
                        }
                        if let Some((hx, hz)) = f {
                            helper_hits.push(Hit {
                                cond: j,
                                x: hx,
                                z: hz,
                                flat: self.take_flat(j),
                            });
                        }
                    }
                    true
                };
                self.find_dim(seed, &mut ctx, ci, center, &mut accept)
            };
            match (found, c.negate) {
                (Some(_), true) | (None, false) => return None,
                (Some((x, z)), false) => {
                    hits.push(Hit {
                        cond: ci,
                        x,
                        z,
                        flat: self.take_flat(ci),
                    });
                    hits.extend(helper_hits);
                }
                (None, true) => {}
            }
        }
        hits.sort_by_key(|h| h.cond);
        Some(Match { center, hits })
    }

    fn find_dim(
        &self,
        seed: i64,
        ctx: &mut DimCtx,
        ci: usize,
        c: (i32, i32),
        accept: &mut dyn FnMut(i32, i32) -> bool,
    ) -> Option<(i32, i32)> {
        let dim = self.query.dim;
        let cond = &self.query.conds[ci];
        let r = cond.radius;
        let r2 = r as i64 * r as i64;
        let chunk_rect = |pad: i32| {
            (
                (c.0 - r - pad) >> 4,
                (c.1 - r - pad) >> 4,
                (c.0 + r + pad) >> 4,
                (c.1 + r + pad) >> 4,
            )
        };
        if dim == 1 {
            return self.find_end(seed, ctx, ci, c, accept);
        }
        match &cond.target {
            Target::Biome(_) => {
                let (m, offsets, _) = self.biome[ci].as_ref()?;
                offsets
                    .iter()
                    .map(|&(dx, dz)| (c.0.wrapping_add(dx), c.1.wrapping_add(dz)))
                    .filter(|&(x, z)| member(m, ctx.biome(dim, seed, x, z)))
                    .find(|&(x, z)| accept(x, z))
            }
            Target::Adjacent { .. } => {
                let a = self.adj[ci].as_ref()?;
                let mut memo = crate::jmath::FxMap::default();
                for &(dx, dz) in &a.offsets {
                    let (x, z) = (c.0.wrapping_add(dx), c.1.wrapping_add(dz));
                    if !member(&a.a, ctx.biome(dim, seed, x, z)) {
                        continue;
                    }
                    if self.adj_near(a, x, z, &mut memo, &mut |x, z| ctx.biome(dim, seed, x, z))
                        && accept(x, z)
                    {
                        return Some((x, z));
                    }
                }
                None
            }
            Target::Fortress if dim == -1 => {
                let (a, b, e, f) = chunk_rect(32);
                nearest_acc(
                    c,
                    r2,
                    accept,
                    crate::nether::fortresses_in(seed, a, b, e, f)
                        .iter()
                        .map(|f| f.start_center())
                        .collect(),
                )
            }
            Target::FortressSpawners(n) if dim == -1 => {
                let (a, b, e, f) = chunk_rect(32);
                let mut v: Vec<_> = crate::nether::fortresses_in(seed, a, b, e, f)
                    .into_iter()
                    .map(|f| (f.start_center(), f))
                    .filter(|&((x, z), _)| d2(x, z, c) <= r2)
                    .collect();
                v.sort_by_key(|&((x, z), _)| d2(x, z, c));
                v.into_iter()
                    .filter(|(_, f)| {
                        crate::fortress::layout(seed, f.chunk_x, f.chunk_z)
                            .is_some_and(|l| l.spawners().len() >= *n as usize)
                    })
                    .map(|(p, _)| p)
                    .find(|&(x, z)| accept(x, z))
            }
            Target::TfFeature(ids) if dim == 7 => {
                let (a, b, e, f) = chunk_rect(16);
                let tf = ctx.tf.get_or_insert_with(|| {
                    crate::twilight::TwilightForest::new(crate::twilight::tf_seed(seed, ""))
                });
                nearest_acc(
                    c,
                    r2,
                    accept,
                    tf.features_in(a, b, e, f)
                        .into_iter()
                        .filter(|f| f.feature.id != 0 && ids.contains(&f.feature.id))
                        .map(|f| (f.x, f.z))
                        .collect(),
                )
            }
            Target::OreVeinFast(want) => {
                let table = self.dim_veins.as_ref()?;
                let ws = if dim == 7 {
                    crate::twilight::tf_seed(seed, "")
                } else {
                    seed
                };
                disc_chunks(c, r, 8)
                    .into_iter()
                    .filter(|&(cx, cz)| is_oreseed(&GT_GTNH, cx, cz))
                    .filter(|&(cx, cz)| {
                        crate::nether::gt_candidates(ws, dim, table, &GT_GTNH, cx, cz, 1)
                            .first()
                            .is_some_and(|pl| pl.vein == *want)
                    })
                    .map(|(cx, cz)| (cx * 16 + 8, cz * 16 + 8))
                    .find(|&(x, z)| accept(x, z))
            }
            Target::OreVein(want) => {
                // GT's placement test on the exact dimension terrain (`gtdims`), arrival at the origin, nearest
                // oreseed first with early exit; the candidate list (seed maths only) skips oreseeds that can
                // never get the wanted vein; resolved oreseeds are kept for the seed's other ore conditions
                let table = self.dim_veins.as_ref()?;
                let ws = if dim == 7 {
                    crate::twilight::tf_seed(seed, "")
                } else {
                    seed
                };
                let max = GT_GTNH.max_placement_attempts;
                let DimCtx { gs, veins, .. } = ctx;
                disc_chunks(c, r, 8)
                    .into_iter()
                    .filter(|&(cx, cz)| is_oreseed(&GT_GTNH, cx, cz))
                    .filter(|&(cx, cz)| {
                        crate::nether::gt_candidates(ws, dim, table, &GT_GTNH, cx, cz, max)
                            .iter()
                            .any(|pl| pl.vein == *want)
                    })
                    .filter(|&(cx, cz)| {
                        let v = *veins.entry((cx, cz)).or_insert_with(|| {
                            let stone = gs.get_or_insert_with(|| {
                                crate::gtdims::DimStone::new(dim, ws).expect("dim")
                            });
                            crate::gtdims::resolve_dim(ws, dim, table, &GT_GTNH, cx, cz, stone)
                                .map(|pl| pl.vein)
                        });
                        v == Some(*want)
                    })
                    .map(|(cx, cz)| (cx * 16 + 8, cz * 16 + 8))
                    .find(|&(x, z)| accept(x, z))
            }
            t @ (Target::TcAspect { .. } | Target::TcNodeType { .. }) => {
                use crate::tc_dims::{DimTc, DimWorld};
                let climate = self.data.climate.as_ref()?;
                let w = if dim == 7 {
                    DimWorld::Twilight(ctx.tt.get_or_insert_with(|| {
                        crate::dimterrain::TfTerrain::new(crate::twilight::tf_seed(seed, ""))
                    }))
                } else {
                    DimWorld::Nether(
                        ctx.nt
                            .get_or_insert_with(|| crate::dimterrain::NetherTerrain::new(seed)),
                    )
                };
                let mut g = DimTc::new(w, climate, TC_GTNH);
                let mut out = Vec::new();
                for (cx, cz) in disc_chunks(c, r + 16, 8) {
                    // vegetation and node draws replay without terrain here: skip chunks whose node is out of
                    // range or fails the condition for every census outcome
                    let peek = if crate::thaumcraft::slow_paths() {
                        crate::tc_dims::WildPeek::Unknown
                    } else {
                        g.peek_wild(seed, cx, cz)
                    };
                    match peek {
                        crate::tc_dims::WildPeek::Nothing => continue,
                        crate::tc_dims::WildPeek::Node { x, z, rng, biome } => {
                            if d2(x, z, c) > r2 {
                                continue;
                            }
                            let infos = g.possible_infos(&rng, biome);
                            if infos.iter().all(|i| t.node_ok(i)) {
                                // a hit whatever the census says (the node is valid)
                                if accept(x, z) {
                                    return Some((x, z));
                                }
                                continue;
                            }
                            if !infos.iter().any(|i| t.node_ok(i)) {
                                continue;
                            }
                        }
                        crate::tc_dims::WildPeek::Unknown => {}
                    }
                    out.clear();
                    g.nodes(seed, cx, cz, &mut out);
                    if let Some(n) = out.iter().find(|n| {
                        n.valid
                            && d2(n.x, n.z, c) <= r2
                            && n.info.as_ref().is_some_and(|i| t.node_ok(i))
                            && accept(n.x, n.z)
                    }) {
                        return Some((n.x, n.z));
                    }
                }
                None
            }
            // overworld-only targets never match in another dimension
            _ => None,
        }
    }

    /// End targets (`query.dim` == 1).
    fn find_end(
        &self,
        seed: i64,
        ctx: &mut DimCtx,
        ci: usize,
        c: (i32, i32),
        accept: &mut dyn FnMut(i32, i32) -> bool,
    ) -> Option<(i32, i32)> {
        use crate::end;
        let cond = &self.query.conds[ci];
        let r = cond.radius;
        let r2 = r as i64 * r as i64;
        let mut nearest = |v: Vec<(i32, i32)>| nearest_acc(c, r2, &mut *accept, v);
        // chunk rect of starts whose point (start·16 + off) can lie within r of c
        let starts = |off: i32| {
            (
                (c.0 - r - off) >> 4,
                (c.1 - r - off) >> 4,
                (c.0 + r - off) >> 4,
                (c.1 + r - off) >> 4,
            )
        };
        match &cond.target {
            Target::EndIsland { biome, variation } => {
                let (x0, z0, x1, z1) = starts(112);
                nearest(
                    end::islands(seed, 0, x0, z0, x1, z1)
                        .into_iter()
                        .filter(|i| {
                            biome.map_or(true, |b| i.biome == b)
                                && variation.map_or(true, |v| i.variation == v)
                        })
                        .map(|i| (i.cx * 16 + 112, i.cz * 16 + 112))
                        .collect(),
                )
            }
            Target::EndTower => {
                // the axis lies 7..24 blocks past the start chunk corner (rotation)
                let (x0, z0, x1, z1) = (
                    (c.0 - r - 24) >> 4,
                    (c.1 - r - 24) >> 4,
                    (c.0 + r - 7) >> 4,
                    (c.1 + r - 7) >> 4,
                );
                nearest(
                    end::towers(seed, 0, x0, z0, x1, z1)
                        .into_iter()
                        .map(|(x, z)| crate::endtower::tower_centre(seed, x, z))
                        .collect(),
                )
            }
            Target::EndComet { min_size } => {
                let (x0, z0, x1, z1) = (
                    (c.0 - r - 16) >> 4,
                    (c.1 - r - 16) >> 4,
                    (c.0 + r) >> 4,
                    (c.1 + r) >> 4,
                );
                nearest(
                    end::comets(seed, 10_000, x0, z0, x1, z1)
                        .into_iter()
                        .filter(|k| k.size >= *min_size as i32)
                        .map(|k| (k.x, k.z))
                        .collect(),
                )
            }
            Target::OreVeinFast(want) | Target::OreVein(want) => {
                // only oreseeds some chunk of whose window processes GT ores; fast = first candidate's type
                let table = self.dim_veins.as_ref()?;
                let fast = matches!(cond.target, Target::OreVeinFast(_));
                let zones = crate::gtend::OreZones::new(
                    seed,
                    (c.0 - r) >> 4,
                    (c.1 - r) >> 4,
                    (c.0 + r) >> 4,
                    (c.1 + r) >> 4,
                );
                let max = GT_GTNH.max_placement_attempts;
                let DimCtx { eb, veins, .. } = ctx;
                disc_chunks(c, r, 8)
                    .into_iter()
                    .filter(|&(cx, cz)| is_oreseed(&GT_GTNH, cx, cz))
                    .filter(|&(cx, cz)| {
                        crate::nether::gt_candidates(
                            seed,
                            1,
                            table,
                            &GT_GTNH,
                            cx,
                            cz,
                            if fast { 1 } else { max },
                        )
                        .iter()
                        .any(|pl| pl.vein == *want)
                    })
                    .filter(|&(cx, cz)| {
                        let Some((px, pz)) =
                            crate::gtend::processing_chunk_end_with(cx, cz, |x, z| {
                                zones.processes(x, z)
                            })
                        else {
                            return false;
                        };
                        if fast {
                            return true;
                        }
                        let v = *veins.entry((cx, cz)).or_insert_with(|| {
                            let w = eb.get_or_insert_with(|| crate::gtend::EndBlocks::new(seed, 0));
                            crate::gtend::resolve_end_at(seed, table, &GT_GTNH, cx, cz, px, pz, w)
                                .map(|pl| pl.vein)
                        });
                        v == Some(*want)
                    })
                    .map(|(cx, cz)| (cx * 16 + 8, cz * 16 + 8))
                    .find(|&(x, z)| accept(x, z))
            }
            _ => None,
        }
    }

    /// Is a B biome within `dist` of (x, z)? B is checked on an absolute grid of `a.step` blocks (memoised).
    fn adj_near(
        &self,
        a: &AdjData,
        x: i32,
        z: i32,
        memo: &mut crate::jmath::FxMap<(i32, i32), bool>,
        biome: &mut dyn FnMut(i32, i32) -> i32,
    ) -> bool {
        let st = a.step;
        let (gx, gz) = (x.div_euclid(st) * st, z.div_euclid(st) * st);
        let lim2 = a.dist as i64 * a.dist as i64;
        for &(dx, dz) in &a.near {
            let (px, pz) = (gx.wrapping_add(dx), gz.wrapping_add(dz));
            if d2(px, pz, (x, z)) > lim2 {
                continue;
            }
            let v = *memo
                .entry((px, pz))
                .or_insert_with(|| member(&a.b, biome(px, pz)));
            if v {
                return true;
            }
        }
        false
    }

    pub fn check_with(&self, p: &BiomeProvider, h: &dyn Heights) -> Option<Match> {
        let q = &self.query;
        let center = match (q.center, self.data.spawn) {
            (Center::Spawn, Some(f)) => f(p, h, p.seed()),
            (Center::Point(x, z), _) => (x, z),
            _ => (0, 0),
        };
        if q.land_at_spawn && p.is_continental() && p.continent_value(center.0, center.1) < 0.0 {
            return None;
        }
        let mut terrain: Option<Terrain> = None;
        let mut approx: Option<crate::gtorder::ApproxTerrain> = None;
        // helper conditions run inside the outer condition's search: own terrain caches
        let mut hterrain: Option<Terrain> = None;
        let mut happrox: Option<crate::gtorder::ApproxTerrain> = None;
        let mut hits = Vec::with_capacity(q.conds.len());
        for &ci in &self.order {
            let c = &q.conds[ci];
            let mut helper_hits: Vec<Hit> = Vec::new();
            let found = if c.near.is_empty() {
                self.find(p, h, &mut terrain, &mut approx, ci, center, &mut any_ok)
            } else {
                let mut accept = |x: i32, z: i32| {
                    helper_hits.clear();
                    for &j in &c.near {
                        // same target type: another one ("two volcanoes within 800"), not the candidate itself
                        let same = q.conds[j].target == c.target;
                        let f = self.find(
                            p,
                            h,
                            &mut hterrain,
                            &mut happrox,
                            j,
                            (x, z),
                            &mut |hx, hz| !same || (hx, hz) != (x, z),
                        );
                        if f.is_some() == q.conds[j].negate {
                            return false;
                        }
                        if let Some((hx, hz)) = f {
                            helper_hits.push(Hit {
                                cond: j,
                                x: hx,
                                z: hz,
                                flat: self.take_flat(j),
                            });
                        }
                    }
                    true
                };
                self.find(p, h, &mut terrain, &mut approx, ci, center, &mut accept)
            };
            match (found, c.negate) {
                (Some(_), true) | (None, false) => return None,
                (Some((x, z)), false) => {
                    hits.push(Hit {
                        cond: ci,
                        x,
                        z,
                        flat: self.take_flat(ci),
                    });
                    hits.extend(helper_hits);
                }
                (None, true) => {}
            }
        }
        hits.sort_by_key(|h| h.cond);
        Some(Match { center, hits })
    }

    /// [`Target::SurfaceBlock`]: chunks nearest first (min distance of the chunk square), pruned by the ±64-block
    /// 16-lattice of provider biomes that can paint one of the blocks; stops once `min` columns matched and no
    /// remaining chunk can hold a closer accepted column.
    #[allow(clippy::too_many_arguments)]
    fn find_surface(
        &self,
        p: &BiomeProvider,
        h: &dyn Heights,
        t: &mut Terrain,
        st: &crate::surface::SurfaceTable,
        sm: &crate::spawn::SurfaceModel,
        ci: usize,
        c: (i32, i32),
        blocks: &[u8],
        min: u32,
        water: bool,
        accept: &mut dyn FnMut(i32, i32) -> bool,
    ) -> Option<(i32, i32)> {
        use crate::surface::blk;
        let r = self.query.conds[ci].radius;
        let r2 = r as i64 * r as i64;
        let paint = self.surface_ids[ci].as_ref();
        // chunk square min distance² to c
        let cd2 = |cx: i32, cz: i32| {
            let dx = (cx * 16 - c.0).max(c.0 - (cx * 16 + 15)).max(0) as i64;
            let dz = (cz * 16 - c.1).max(c.1 - (cz * 16 + 15)).max(0) as i64;
            dx * dx + dz * dz
        };
        let (cx0, cz0, cx1, cz1) = (
            (c.0 - r) >> 4,
            (c.1 - r) >> 4,
            (c.0 + r) >> 4,
            (c.1 + r) >> 4,
        );
        let mut chunks: Vec<(i64, i32, i32)> = Vec::new();
        for cz in cz0..=cz1 {
            for cx in cx0..=cx1 {
                let d = cd2(cx, cz);
                if d <= r2 {
                    chunks.push((d, cx, cz));
                }
            }
        }
        chunks.sort_unstable();
        let mut lattice: crate::jmath::FxMap<(i32, i32), bool> = Default::default();
        let caves = crate::gtorder::Caves::new(p.seed(), true);
        let temp = |g: u16| {
            self.data
                .climate
                .as_ref()
                .and_then(|cl| cl.get(g))
                .map(|b| b.temperature)
        };
        let mut scores: crate::jmath::FxMap<(i32, i32), f32> = Default::default();
        let (mut count, mut best): (u32, Option<(i64, i32, i32)>) = (0, None);
        for (d, cx, cz) in chunks {
            if count >= min && best.is_some_and(|b| d > b.0) {
                break;
            }
            if let Some(ok) = paint {
                let mut any = false;
                'lat: for lz in cz - 4..=cz + 4 {
                    for lx in cx - 4..=cx + 4 {
                        let v = *lattice.entry((lx, lz)).or_insert_with(|| {
                            ok.get(p.biome_data(lx * 16 + 8, lz * 16 + 8, h).data as usize)
                                .copied()
                                .unwrap_or(true)
                        });
                        if v {
                            any = true;
                            break 'lat;
                        }
                    }
                }
                if !any {
                    continue;
                }
            }
            let Some(ch) = t.chunk(cx, cz) else { continue };
            let ch = ch.clone();
            let cc = crate::spawn::carved_chunk(p, h, sm, &ch, cx, cz, Some(&caves));
            let mut sc = crate::surface::surface_chunk(p, h, st, &ch, cx, cz, &cc);
            if self.data.climate.is_some() {
                crate::surface::apply_snow_cover_with(
                    p,
                    &temp,
                    &mut |x, z| {
                        *scores
                            .entry((x, z))
                            .or_insert_with(|| crate::surface::snow_score(p, h, x, z))
                    },
                    &ch,
                    &mut sc,
                    cx,
                    cz,
                );
            }
            for idx in 0..256usize {
                let (x, z) = (cx * 16 + (idx & 15) as i32, cz * 16 + (idx >> 4) as i32);
                let dd = d2(x, z, c);
                if dd > r2 {
                    continue;
                }
                let mut y = sc.top[idx] as i32;
                if y < 0 {
                    continue;
                }
                let col = &sc.cols[idx];
                if water {
                    while y > 0 && col[y as usize] == blk::WATER {
                        y -= 1;
                    }
                }
                if !blocks.contains(&col[y as usize]) {
                    continue;
                }
                count += 1;
                if best.map_or(true, |b| dd < b.0) && accept(x, z) {
                    best = Some((dd, x, z));
                }
            }
        }
        if count >= min {
            best.map(|b| (b.1, b.2))
        } else {
            None
        }
    }

    /// Nearest target of condition `ci` within its radius of `c` that `accept` takes.
    #[allow(clippy::too_many_arguments)]
    fn find<'a>(
        &self,
        p: &'a BiomeProvider,
        h: &'a dyn Heights,
        terrain: &mut Option<Terrain<'a>>,
        approx: &mut Option<crate::gtorder::ApproxTerrain<'a>>,
        ci: usize,
        c: (i32, i32),
        accept: &mut dyn FnMut(i32, i32) -> bool,
    ) -> Option<(i32, i32)> {
        let cond = &self.query.conds[ci];
        let r = cond.radius;
        let r2 = r as i64 * r as i64;
        let seed = p.seed();
        match &cond.target {
            Target::Stronghold => nearest_acc(
                c,
                r2,
                &mut *accept,
                crate::structures::stronghold_ring(seed)
                    .iter()
                    .map(|&k| {
                        crate::stronghold_pieces::portal_room(seed, k)
                            .map_or(crate::structures::chunk_center(k), |r| {
                                (r.eye_frame_centre.0, r.eye_frame_centre.2)
                            })
                    })
                    .collect(),
            ),
            Target::Village => {
                crate::villages::nearest_village_within_where(p, h, seed, c.0, c.1, r, &mut *accept)
                    .map(|(v, _)| (v.x, v.z))
            }
            Target::LootGames => {
                let k = (r + 31) >> 4;
                let (bx, bz) = (c.0 >> 4, c.1 >> 4);
                nearest_acc(
                    c,
                    r2,
                    &mut *accept,
                    crate::dungeons::lootgames_chunks(seed, 20, bx - k, bz - k, bx + k, bz + k)
                        .into_iter()
                        .map(|(cx, cz)| (cx * 16 + 8, cz * 16 + 8))
                        .collect(),
                )
            }
            Target::Roguelike => {
                let k = (r + 31) >> 4;
                let (bx, bz) = (c.0 >> 4, c.1 >> 4);
                nearest_acc(
                    c,
                    r2,
                    &mut *accept,
                    crate::dungeons::rogue_anchors(
                        seed,
                        &crate::dungeons::ROGUE_GTNH,
                        bx - k,
                        bz - k,
                        bx + k,
                        bz + k,
                    )
                    .into_iter()
                    .map(|(cx, cz)| (cx * 16 + 4, cz * 16 + 4))
                    .collect(),
                )
            }
            Target::Volcano => self.find_volcano(p, h, r, c, accept),
            Target::LavaCave => self.find_lava(p, h, r, c, accept),
            Target::Biome(_) => self.find_biome(p, h, ci, c, accept),
            Target::FlatBiome { flat, .. } => {
                let m = &self.biome[ci].as_ref()?.0;
                let a = approx.get_or_insert_with(|| crate::gtorder::ApproxTerrain::new(p, h));
                let (hit, info) = flat_patch_search(
                    c,
                    r,
                    flat,
                    &mut |x, z| member(m, p.biome_data(x, z, h).data as i32),
                    &mut |x, z| a.top(x, z),
                    accept,
                )?;
                self.flat_last.borrow_mut()[ci] = Some(info);
                Some(hit)
            }
            Target::SurfaceBlock { blocks, min, water } => {
                let (st, sm) = self.surface.as_ref()?;
                let t = terrain.get_or_insert_with(|| Terrain::new(p, h));
                self.find_surface(p, h, t, st, sm, ci, c, blocks, *min, *water, accept)
            }
            Target::Adjacent { .. } => {
                let a = self.adj[ci].as_ref()?;
                let mut memo = crate::jmath::FxMap::default();
                for &(dx, dz) in &a.offsets {
                    let (x, z) = (c.0.wrapping_add(dx), c.1.wrapping_add(dz));
                    if !member(&a.a, p.biome_data(x, z, h).data as i32) {
                        continue;
                    }
                    if self.adj_near(a, x, z, &mut memo, &mut |x, z| {
                        p.biome_data(x, z, h).data as i32
                    }) && accept(x, z)
                    {
                        return Some((x, z));
                    }
                }
                None
            }
            Target::Fortress | Target::TfFeature(_) | Target::FortressSpawners(_) => None,
            Target::VillageStyle(style) => {
                let (x0, z0) = ((c.0 - r - 8) >> 4, (c.1 - r - 8) >> 4);
                let (x1, z1) = ((c.0 + r - 8) >> 4, (c.1 + r - 8) >> 4);
                nearest_acc(
                    c,
                    r2,
                    &mut *accept,
                    crate::villages::villages_in_rect(p, h, seed, x0, z0, x1 + 1, z1 + 1)
                        .into_iter()
                        .filter(|v| v.viable_exact && v.style == *style)
                        .map(|v| (v.x, v.z))
                        .collect(),
                )
            }
            Target::StrongholdSurface { ocean } => {
                let mut v: Vec<(i32, i32)> = crate::structures::stronghold_ring(seed)
                    .iter()
                    .filter_map(|&k| crate::stronghold_pieces::portal_room(seed, k))
                    .map(|r| (r.eye_frame_centre.0, r.eye_frame_centre.2))
                    .filter(|&(x, z)| d2(x, z, c) <= r2)
                    .collect();
                v.sort_by_key(|&(x, z)| d2(x, z, c));
                let t = terrain.get_or_insert_with(|| Terrain::new(p, h));
                v.into_iter().find(|&(x, z)| {
                    t.top(x, z).is_some_and(|top| (top <= 61) == *ocean) && accept(x, z)
                })
            }
            Target::Geode { min_diamond, kind } => {
                let gates = self.data.pop_gates.as_ref()?;
                let mut pm: Option<crate::population::PopModel> = None;
                let ab = geode_ab(seed);
                let mut lattice: crate::jmath::FxMap<(i32, i32), u16> = Default::default();
                for (cx, cz) in disc_chunks(c, r, 8) {
                    // the three positions the NHCore / Railcraft 0.3 roll can take (0, 1 or 2 earlier draws)
                    let mut rr = crate::java_random::JavaRandom::new(
                        (cx as i64)
                            .wrapping_mul(ab.0)
                            .wrapping_add((cz as i64).wrapping_mul(ab.1))
                            ^ seed,
                    );
                    let d = [rr.next_double(), rr.next_double(), rr.next_double()];
                    if d.iter().all(|&v| v > 0.3) {
                        continue;
                    }
                    // the chunk biome at (8, 8) is a dithered pick over the 8-block samples within ±64 blocks of the
                    // chunk: any gated biome on a 16-block lattice there (memoised across chunks) keeps the chunk
                    let mut any = false;
                    let (bx, bz) = ((cx * 16 + 8) >> 4, (cz * 16 + 8) >> 4);
                    'lat: for lz in bz - 4..=bz + 4 {
                        for lx in bx - 4..=bx + 4 {
                            let g = *lattice
                                .entry((lx, lz))
                                .or_insert_with(|| p.biome_gen_id(lx * 16, lz * 16, h));
                            let ocean = crate::population::PreGates::get_pub(&gates.rc_geode, g);
                            let mountain = crate::population::PreGates::get_pub(&gates.nh_geode, g);
                            if (ocean && *kind != 1) || (mountain && *kind != 2) {
                                any = true;
                                break 'lat;
                            }
                        }
                    }
                    if !any {
                        continue;
                    }
                    let m = pm.get_or_insert_with(|| {
                        let mut m = crate::population::PopModel::new(p, h, seed);
                        m.gates = gates.clone();
                        m
                    });
                    if let Some(g) = m.geode_at(cx, cz) {
                        let kind_ok = match kind {
                            1 => !g.ocean,
                            2 => g.ocean,
                            _ => true,
                        };
                        if kind_ok
                            && g.diamond >= *min_diamond
                            && d2(g.x, g.z, c) <= r2
                            && accept(g.x, g.z)
                        {
                            return Some((g.x, g.z));
                        }
                    }
                }
                None
            }
            Target::Lake {
                liquid,
                min_y,
                min_cells,
                open,
            } => {
                let gates = self.data.pop_gates.as_ref()?;
                let chunks = disc_chunks(c, r + 32, 8);
                let mut pm = crate::population::PopModel::new(p, h, seed);
                pm.gates = gates.clone();
                let (x0, z0, x1, z1) = (
                    (c.0 - r - 48) >> 4,
                    (c.1 - r - 48) >> 4,
                    (c.0 + r + 16) >> 4,
                    (c.1 + r + 16) >> 4,
                );
                pm.load_structures(x0, z0, x1, z1);
                let mut lattice: crate::jmath::FxMap<(i32, i32), u16> = Default::default();
                let mut ids: Vec<u16> = Vec::new();
                // lakes found so far, (distance², x, z); a lake lies within 23 blocks (x and z) of its chunk's +8 point
                let mut pending: Vec<(i64, i32, i32)> = Vec::new();
                let bound = |cx: i32, cz: i32| {
                    let d = (d2(cx * 16 + 8, cz * 16 + 8, c) as f64).sqrt() - 33.0;
                    if d <= 0.0 {
                        0
                    } else {
                        (d * d) as i64
                    }
                };
                for (i, &(cx, cz)) in chunks.iter().enumerate() {
                    ids.clear();
                    let (bx, bz) = ((cx * 16 + 8) >> 4, (cz * 16 + 8) >> 4);
                    for lz in bz - 4..=bz + 5 {
                        for lx in bx - 4..=bx + 5 {
                            let g = *lattice
                                .entry((lx, lz))
                                .or_insert_with(|| p.biome_gen_id(lx * 16, lz * 16, h));
                            if !ids.contains(&g) {
                                ids.push(g);
                            }
                        }
                    }
                    if pm.touches_shaft(cx, cz) || pm.lake_roll_possible(cx, cz, &ids, &ids) {
                        for l in pm.lakes_at(cx, cz) {
                            let liq_ok = match liquid {
                                1 => l.lava,
                                2 => !l.lava,
                                _ => true,
                            };
                            let dd = d2(l.x, l.z, c);
                            if liq_ok
                                && l.y1 >= *min_y
                                && l.cells >= *min_cells
                                && (!*open || l.open)
                                && dd <= r2
                            {
                                pending.push((dd, l.x, l.z));
                            }
                        }
                    }
                    // every lake of the remaining chunks is at least this far
                    let next = chunks
                        .get(i + 1)
                        .map_or(i64::MAX, |&(nx, nz)| bound(nx, nz));
                    pending.sort();
                    while let Some(&(dd, x, z)) = pending.first() {
                        if dd > next {
                            break;
                        }
                        pending.remove(0);
                        if accept(x, z) {
                            return Some((x, z));
                        }
                    }
                }
                None
            }
            Target::Loot {
                source,
                items,
                min_count,
            } => {
                let gates = self.data.pop_gates.as_ref()?;
                let chunks = disc_chunks(c, r + 32, 8);
                let mut pm = crate::population::PopModel::new(p, h, seed);
                pm.gates = gates.clone();
                let (x0, z0, x1, z1) = (
                    (c.0 - r - 48) >> 4,
                    (c.1 - r - 48) >> 4,
                    (c.0 + r + 16) >> 4,
                    (c.1 + r + 16) >> 4,
                );
                pm.load_structures(x0, z0, x1, z1);
                let dt = crate::loot::LootTable::dungeon();
                let mt = crate::loot::LootTable::mineshaft();
                let want = |t: &crate::loot::LootTable, ch: &crate::loot::Chest| -> bool {
                    if items.is_empty() {
                        return true;
                    }
                    let n: u32 = ch
                        .slots
                        .iter()
                        .filter(|s| {
                            t.items
                                .get(s.1 as usize)
                                .is_some_and(|it| items.iter().any(|w| it.name.contains(w.as_str())))
                        })
                        .map(|s| s.2 as u32)
                        .sum();
                    n >= (*min_count).max(1)
                };
                let mut pending: Vec<(i64, i32, i32)> = Vec::new();
                // a chest lies inside its chunk's +8 box: at most 33 blocks from the chunk's +8 point
                let bound = |cx: i32, cz: i32| {
                    let d = (d2(cx * 16 + 8, cz * 16 + 8, c) as f64).sqrt() - 33.0;
                    if d <= 0.0 {
                        0
                    } else {
                        (d * d) as i64
                    }
                };
                for (i, &(cx, cz)) in chunks.iter().enumerate() {
                    let carts = *source != 1 && mt.is_some() && pm.touches_shaft(cx, cz);
                    if *source != 2 || carts {
                        let rooms = pm.dungeons_at(cx, cz);
                        if *source != 2 {
                            for d in &rooms {
                                for (x, _, z, ch) in &d.chests {
                                    let dd = d2(*x, *z, c);
                                    if dd <= r2 && want(dt, ch) {
                                        pending.push((dd, *x, *z));
                                    }
                                }
                            }
                        }
                        if carts {
                            let t = mt.unwrap();
                            for (x, _, z, ch) in std::mem::take(&mut pm.mine_chests) {
                                let dd = d2(x, z, c);
                                if dd <= r2 && want(t, &ch) {
                                    pending.push((dd, x, z));
                                }
                            }
                        }
                    }
                    let next = chunks
                        .get(i + 1)
                        .map_or(i64::MAX, |&(nx, nz)| bound(nx, nz));
                    pending.sort();
                    while let Some(&(dd, x, z)) = pending.first() {
                        if dd > next {
                            break;
                        }
                        pending.remove(0);
                        if accept(x, z) {
                            return Some((x, z));
                        }
                    }
                }
                None
            }
            // End-only targets never match in the overworld
            Target::EndIsland { .. } | Target::EndTower | Target::EndComet { .. } => None,
            Target::Mineshaft => {
                let s = crate::mineshaft::Seeds::new(seed);
                let (x0, z0) = ((c.0 - r - 8) >> 4, (c.1 - r - 8) >> 4);
                let (x1, z1) = ((c.0 + r - 8) >> 4, (c.1 + r - 8) >> 4);
                nearest_acc(
                    c,
                    r2,
                    &mut *accept,
                    crate::mineshaft::starts_in(&s, x0, z0, x1 + 1, z1 + 1)
                        .into_iter()
                        .map(|(cx, cz)| (cx * 16 + 8, cz * 16 + 8))
                        .collect(),
                )
            }
            Target::OreVein(want) | Target::OreVeinFast(want) => {
                let table = self.data.veins.as_ref()?;
                for (cx, cz) in disc_chunks(c, r, 8) {
                    if !is_oreseed(&GT_GTNH, cx, cz) {
                        continue;
                    }
                    // cheap pre-check: is the wanted vein among the candidates at all?
                    let ovs = crate::gtores::orevein_seed(seed, 0, cx, cz);
                    if !(0..GT_GTNH.max_placement_attempts).any(|i| {
                        crate::gtores::candidate(table, ovs, i, cx, cz)
                            .is_some_and(|k| k.vein == *want)
                    }) {
                        continue;
                    }
                    let a = approx.get_or_insert_with(|| crate::gtorder::ApproxTerrain::new(p, h));
                    let pl =
                        crate::gtorder::resolve_ordered_approx(seed, table, &GT_GTNH, cx, cz, c, a);
                    if pl.is_some_and(|pl| pl.vein == *want) && accept(cx * 16 + 8, cz * 16 + 8) {
                        return Some((cx * 16 + 8, cz * 16 + 8));
                    }
                }
                None
            }
            Target::Fluid { fluid, min_lpo } => {
                let t = self.data.fluids.as_ref()?;
                for (rx, rz) in crate::gtfluids::fields_within(c.0, c.1, r) {
                    let Some(f) = crate::gtfluids::field_at(seed, 0, rx, rz, t) else {
                        continue;
                    };
                    let lpo = if f.vein_average <= crate::gtfluids::DIVIDER {
                        0
                    } else {
                        f.vein_average / crate::gtfluids::DIVIDER
                    };
                    if (*fluid == usize::MAX || f.fluid_index == *fluid) && lpo >= *min_lpo {
                        // nearest point of the field to the centre (fields are 128x128 blocks)
                        let (x0, z0) = (rx * 128, rz * 128);
                        let pt = (c.0.clamp(x0, x0 + 127), c.1.clamp(z0, z0 + 127));
                        if accept(pt.0, pt.1) {
                            return Some(pt);
                        }
                    }
                }
                None
            }
            tgt @ (Target::TcAspect { .. } | Target::TcNodeType { .. }) => {
                let climate = self.data.climate.as_ref()?;
                let t = terrain.get_or_insert_with(|| Terrain::new(p, h));
                let mut g = TcGen {
                    t,
                    climate,
                    cfg: TC_GTNH,
                };
                let mut out = Vec::new();
                for (cx, cz) in disc_chunks(c, r + 16, 8) {
                    // no wild / obelisk / totem roll possible: no hit here, skip the terrain
                    if !g.node_roll_possible(seed, cx, cz, true) {
                        continue;
                    }
                    out.clear();
                    let _ = g.nodes(seed, cx, cz, &mut out);
                    let hit = out.iter().find(|n| {
                        n.valid
                            && d2(n.x, n.z, c) <= r2
                            && n.info.as_ref().is_some_and(|i| tgt.node_ok(i))
                            && accept(n.x, n.z)
                    });
                    if let Some(n) = hit {
                        return Some((n.x, n.z));
                    }
                }
                None
            }
            Target::Thaumcraft(kind) => {
                let climate = self.data.climate.as_ref()?;
                let t = terrain.get_or_insert_with(|| Terrain::new(p, h));
                let mut g = TcGen {
                    t,
                    climate,
                    cfg: TC_GTNH,
                };
                let mut out = Vec::new();
                for (cx, cz) in disc_chunks(c, r + 16, 8) {
                    if !g.feature_roll_possible(seed, cx, cz, *kind) {
                        continue;
                    }
                    out.clear();
                    g.chunk(seed, cx, cz, &mut out)?;
                    if let Some(f) = out
                        .iter()
                        // obelisks: only "likely" ones (p ≥ 0.5 before maze conflicts; see `ring_prob`)
                        .find(|f| {
                            f.kind == *kind
                                && f.valid
                                && (f.kind != TcKind::Ring || f.p_valid >= 0.5)
                                && d2(f.x, f.z, c) <= r2
                                && accept(f.x, f.z)
                        })
                    {
                        return Some((f.x, f.z));
                    }
                }
                None
            }
        }
    }

    fn find_biome(
        &self,
        p: &BiomeProvider,
        h: &dyn Heights,
        ci: usize,
        c: (i32, i32),
        accept: &mut dyn FnMut(i32, i32) -> bool,
    ) -> Option<(i32, i32)> {
        let (member, offsets, small) = self.biome[ci].as_ref()?;
        let is = |id: BiomeId| member.get(id as usize).copied().unwrap_or(false);
        let r = self.query.conds[ci].radius as f64;
        let mut best: Option<(i32, i32)> = None;
        if *small {
            // exact: every small-patch centre within the radius (biome coordinates = world + biome offset)
            let (bx, bz) = p.biome_offset();
            let (ox, oz) = (c.0.wrapping_add(bx) as f64, c.1.wrapping_add(bz) as f64);
            let mut pts = p
                .small_points()
                .points_in_rect(ox - r, oz - r, ox + r, oz + r);
            pts.retain(|&(x, z)| (x - ox).powi(2) + (z - oz).powi(2) <= r * r);
            pts.sort_by(|a, b| {
                let da = (a.0 - ox).powi(2) + (a.1 - oz).powi(2);
                let db = (b.0 - ox).powi(2) + (b.1 - oz).powi(2);
                da.partial_cmp(&db).unwrap_or(std::cmp::Ordering::Equal)
            });
            for (x, z) in pts {
                let (wx, wz) = ((x as i32).wrapping_sub(bx), (z as i32).wrapping_sub(bz));
                if is(p.biome_data(wx, wz, h).data) && accept(wx, wz) {
                    best = Some((wx, wz));
                    break;
                }
            }
        }
        for &(dx, dz) in offsets {
            let (x, z) = (c.0.wrapping_add(dx), c.1.wrapping_add(dz));
            if best.is_some_and(|b| d2(b.0, b.1, c) <= d2(x, z, c)) {
                break;
            }
            if is(p.biome_data(x, z, h).data) && accept(x, z) {
                best = Some((x, z));
                break;
            }
        }
        best
    }

    fn find_volcano(
        &self,
        p: &BiomeProvider,
        h: &dyn Heights,
        radius: i32,
        c0: (i32, i32),
        accept: &mut dyn FnMut(i32, i32) -> bool,
    ) -> Option<(i32, i32)> {
        let c = p.continents()?;
        let (lx, lz) = p.landmass_offset();
        let r2 = radius as i64 * radius as i64;
        let mut seen = Vec::new();
        let mut best: Option<(i32, i32)> = None;
        // influence radius 160; 128-block spacing always lands inside one
        for (dx, dz) in disc_offsets(radius + 160, 128) {
            let (x, z) = (c0.0.wrapping_add(dx), c0.1.wrapping_add(dz));
            let key = c.volcano_seed_key(x.wrapping_add(lx), z.wrapping_add(lz));
            if key == NONE || seen.contains(&key) {
                continue;
            }
            seen.push(key);
            let center = c.volcano_center_coordinates(x.wrapping_add(lx), z.wrapping_add(lz));
            let (cx, cz) = (
                ((center >> 32) as i32).wrapping_sub(lx),
                (center as i32).wrapping_sub(lz),
            );
            if d2(cx, cz, c0) > r2 || best.is_some_and(|b| d2(b.0, b.1, c0) <= d2(cx, cz, c0)) {
                continue;
            }
            // the volcano island biome is only placed if the gates pass (rivers + height)
            if p.volcano_coordinates(cx, cz, h) != NONE && accept(cx, cz) {
                best = Some((cx, cz));
            }
        }
        best
    }

    fn find_lava(
        &self,
        p: &BiomeProvider,
        h: &dyn Heights,
        radius: i32,
        c0: (i32, i32),
        accept: &mut dyn FnMut(i32, i32) -> bool,
    ) -> Option<(i32, i32)> {
        let c = p.continents()?;
        let (lx, lz) = p.landmass_offset();
        let r2 = radius as i64 * radius as i64;
        let mut seen = Vec::new();
        let mut best: Option<(i32, i32)> = None;
        // influence radius 320
        for (dx, dz) in disc_offsets(radius + 320, 128) {
            let (x, z) = (c0.0.wrapping_add(dx), c0.1.wrapping_add(dz));
            if c.lava_cave_seed_key(x.wrapping_add(lx), z.wrapping_add(lz)) == NONE {
                continue;
            }
            let center = c.lava_cave_center_coordinates(x.wrapping_add(lx), z.wrapping_add(lz));
            if center == NONE || seen.contains(&center) {
                continue;
            }
            seen.push(center);
            let (cx, cz) = (
                ((center >> 32) as i32).wrapping_sub(lx),
                (center as i32).wrapping_sub(lz),
            );
            if d2(cx, cz, c0) > r2 || best.is_some_and(|b| d2(b.0, b.1, c0) <= d2(cx, cz, c0)) {
                continue;
            }
            // gate (height) applied via the provider's own path
            if p.lava_cave_coordinates(x, z, h) != NONE && accept(cx, cz) {
                best = Some((cx, cz));
            }
        }
        best
    }
}
