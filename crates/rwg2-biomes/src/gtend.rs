//! GT ore veins in the End (GT5-Unofficial / galacticgreg 5.09.54.265, GTNH 2.10).
//!
//! Same `GTWorldgenerator.WorldGenContainer.generateVein` as the overworld (oreveinSeed with `dim & 0xff` = 1,
//! FNV attempts, weighted pick over the `"The End"` list, placement test from the first processed chunk within
//! ±2), with three End rules:
//! - **Which chunks process veins** (`DimensionDef.getEffectiveDefForChunk`): only chunks whose effective def is
//!   `TheEnd` start a GT task: `cx² + cz² ≤ 256` (main island zone), or within ±7 chunks (both axes) of an HEE
//!   island start + 7 (`HEEIslandScanner`, `MapGenIsland.canSpawnStructureAtCoords`), or chunk centre within 200
//!   blocks of a Draconic Evolution chaos island. Every other chunk is `EndAsteroids` (`generatesOre` false).
//! - **Vein Y** (`resolveVeinPlacement`, `disableOreVeinHeightChecks`): after the usual `minY + nextInt(maxY −
//!   minY − 5)`, west and east draws, `veinMinY = lo + nextInt(hi − lo + 1)` with lo / hi the lowest / highest
//!   solid block of the processing chunk's column (x + 7, z + 9); an empty column gives lo = 128
//!   (`getActualHeight` of a no-sky provider), hi = 0 → `nextInt(−127)` (XSTR: `abs(state % −127)`, no
//!   exception) → Y 128..254 in the void → the pick fails. Then north / south.
//! - **Stone** = end stone (`StoneType.findStoneType`). HEE island surfaces are `end_stone_terrain` (not stone).
//!
//! Vein list (`"The End"`, `OreMixes` registry order, from source; not in any dump yet): gold, cassiterite,
//! tetrahedrite, copper, nickel, molybdenum, beryllium.
//!
//! Terrain: main island = exact pre-population End terrain (seed + resets); HEE islands = the terrain pass of
//! [`crate::end::island_cols`] + HEE ores and Homeland cellars (`end_homeland::island_compact_interior`, v1.66;
//! island decoration not modelled). Population before GT (HEE spikes,
//! blobs, clouds, DE ores) is not modelled.

use crate::end::{island_at, island_covering, EndTerrain};
use crate::gtorder::MAX_VEIN_SIZE;
use crate::gtores::{
    execute_chunkified, fnv_int_pub, fnv_long_pub, fnv_offset, orevein_seed, BlockQuery, GtConfig,
    Placement, VeinTable, Xstr,
};
use crate::jmath::FxMap;
use crate::nether::RawVein;

/// GT dimension id of the End.
pub const END_DIM: i32 = 1;

/// `"The End"` stone-vein list in registry order (GT 5.09.54.265 `OreMixes`, `enableInDim(THE_END)`).
pub const GT_END_VEINS: [RawVein; 7] = [
    (
        "ore.mix.gold",
        160,
        30,
        60,
        32,
        2,
        870,
        ["Magnetite", "Magnetite", "VanadiumMagnetite", "Gold"],
    ),
    (
        "ore.mix.cassiterite",
        50,
        60,
        220,
        24,
        4,
        57,
        ["Tin", "Tin", "Cassiterite", "Tin"],
    ),
    (
        "ore.mix.tetrahedrite",
        70,
        80,
        120,
        24,
        3,
        840,
        ["Tetrahedrite", "Tetrahedrite", "Copper", "Stibnite"],
    ),
    (
        "ore.mix.copper",
        80,
        5,
        60,
        24,
        3,
        855,
        ["Chalcopyrite", "Iron", "Pyrite", "Copper"],
    ),
    (
        "ore.mix.nickel",
        40,
        10,
        40,
        16,
        2,
        906,
        ["Garnierite", "Nickel", "Cobaltite", "Pentlandite"],
    ),
    (
        "ore.mix.molybdenum",
        5,
        20,
        50,
        16,
        2,
        882,
        ["Wulfenite", "Molybdenite", "Molybdenum", "Powellite"],
    ),
    (
        "ore.mix.beryllium",
        30,
        5,
        30,
        16,
        2,
        8,
        ["Beryllium", "Beryllium", "Emerald", "Thorium"],
    ),
];

/// Material ids (primary, secondary, between, sporadic) of [`GT_END_VEINS`] (`MaterialsIDMap`).
pub const GT_END_VEIN_IDS: [[i32; 4]; 7] = [
    [870, 870, 923, 86],
    [57, 57, 824, 57],
    [840, 840, 35, 945],
    [855, 32, 834, 35],
    [906, 34, 827, 909],
    [882, 942, 48, 883],
    [8, 8, 501, 96],
];

/// HEE island scan half-width (`HEEIslandScanner.ISLAND_RADIUS`).
const ISLAND_RADIUS: i32 = 7;
/// galacticgreg `ChaosIslandExclusionRadius` default (blocks).
pub const CHAOS_RADIUS: i64 = 200;
/// Draconic Evolution `chaosIslandSeparation` default.
pub const CHAOS_SEPARATION: i32 = 10000;

/// Does chunk (`cx`, `cz`) start GT ore tasks (effective def `TheEnd`)?
pub fn processes_ores(seed: i64, cx: i32, cz: i32) -> bool {
    if cx * cx + cz * cz <= 256 {
        return true;
    }
    // island start S with S + 7 within ±7 of the chunk: S in [c − 14, c]
    for sz in cz - 2 * ISLAND_RADIUS..=cz {
        for sx in cx - 2 * ISLAND_RADIUS..=cx {
            if island_at(seed, 0, sx, sz) {
                return true;
            }
        }
    }
    chaos_near(cx, cz)
}

/// [`processes_ores`] for many chunks of one area: the HEE island starts are enumerated once.
pub struct OreZones {
    seed: i64,
    starts: crate::jmath::FxMap<(i32, i32), ()>,
}

impl OreZones {
    /// Covers queries for chunks in [`cx0`, `cx1`] × [`cz0`, `cz1`].
    pub fn new(seed: i64, cx0: i32, cz0: i32, cx1: i32, cz1: i32) -> Self {
        let mut starts = FxMap::default();
        for i in crate::end::islands(
            seed,
            0,
            cx0 - 2 * ISLAND_RADIUS,
            cz0 - 2 * ISLAND_RADIUS,
            cx1,
            cz1,
        ) {
            starts.insert((i.cx, i.cz), ());
        }
        Self { seed, starts }
    }

    pub fn processes(&self, cx: i32, cz: i32) -> bool {
        if cx * cx + cz * cz <= 256 {
            return true;
        }
        if self.starts.keys().any(|&(sx, sz)| {
            (sx..=sx + 2 * ISLAND_RADIUS).contains(&cx)
                && (sz..=sz + 2 * ISLAND_RADIUS).contains(&cz)
        }) {
            return true;
        }
        let _ = self.seed;
        chaos_near(cx, cz)
    }
}

fn chaos_near(cx: i32, cz: i32) -> bool {
    let near = |c: i32| {
        ((c * 16) as f64 / CHAOS_SEPARATION as f64).round() as i64 * CHAOS_SEPARATION as i64
    };
    let (ix, iz) = (near(cx), near(cz));
    if ix == 0 && iz == 0 {
        return false;
    }
    let (dx, dz) = (cx as i64 * 16 + 8 - ix, cz as i64 * 16 + 8 - iz);
    dx * dx + dz * dz <= CHAOS_RADIUS * CHAOS_RADIUS
}

/// Per chunk: 256 columns of (solid, end stone) masks, `[lo, hi]` halves.
type ChunkMasks = Box<[([u128; 2], [u128; 2]); 256]>;

/// End blocks as GT sees them before it runs: solid / end stone per block, chunk-cached.
/// Islands whose columns `EndBlocks` keeps (~1.4 MB each).
const ISLANDS_KEPT: usize = 28;

/// Island column provider: (start cx, cz) → compact columns.
pub type IslandSource = Box<dyn Fn(i32, i32) -> std::rc::Rc<crate::end::IslandCompact>>;

pub struct EndBlocks {
    seed: i64,
    main: EndTerrain,
    /// per chunk: 256 columns `[lo, hi]` solid and stone masks
    cache: FxMap<(i32, i32), ChunkMasks>,
    /// compact island columns, least recently used evicted past `ISLANDS_KEPT` (a 2048 viewer cell spans ~20)
    islands: FxMap<(i32, i32), (u64, std::rc::Rc<crate::end::IslandCompact>)>,
    tick: u64,
    /// where island columns come from (a shared per-worker cache in the viewer), default: generate
    source: Option<IslandSource>,
    towers: FxMap<(i32, i32), crate::endtower::Tower>,
    resets: i32,
    /// diagnostics: islands as terrain + caves only (no HEE ores / Homeland cellars), the pre-v1.66 model
    pub terrain_only: bool,
}

impl EndBlocks {
    /// `seed` = world seed, `resets` = HEE End resets (main island terrain uses seed + resets).
    pub fn new(seed: i64, resets: i32) -> Self {
        Self {
            seed,
            main: EndTerrain::new(seed.wrapping_add(resets as i64)),
            cache: FxMap::default(),
            islands: FxMap::default(),
            tick: 0,
            source: None,
            towers: FxMap::default(),
            resets,
            terrain_only: false,
        }
    }

    /// [`EndBlocks::new`] taking island columns from `source` (e.g. a cache shared with the map renderer).
    pub fn with_source(seed: i64, resets: i32, source: IslandSource) -> Self {
        let mut w = Self::new(seed, resets);
        w.source = Some(source);
        w
    }

    fn chunk(&mut self, cx: i32, cz: i32) -> &[([u128; 2], [u128; 2]); 256] {
        if !self.cache.contains_key(&(cx, cz)) {
            if self.cache.len() > 4096 {
                self.cache.clear();
            }
            let mut cols = Box::new([([0u128; 2], [0u128; 2]); 256]);
            if cx.abs() <= crate::end::MAIN_ISLAND_CHUNKS
                && cz.abs() <= crate::end::MAIN_ISLAND_CHUNKS
            {
                let c = self.main.chunk(cx, cz);
                for (k, col) in cols.iter_mut().enumerate() {
                    col.0[0] = c.cols[k];
                    col.1[0] = c.cols[k];
                }
            }
            if let Some(isl) = island_covering(self.seed, cx, cz) {
                self.tick += 1;
                let tick = self.tick;
                let ic = match self.islands.get_mut(&(isl.cx, isl.cz)) {
                    Some(e) => {
                        e.0 = tick;
                        e.1.clone()
                    }
                    None => {
                        if self.islands.len() >= ISLANDS_KEPT {
                            let old = *self.islands.iter().min_by_key(|(_, v)| v.0).unwrap().0;
                            self.islands.remove(&old);
                        }
                        let ic = match &self.source {
                            Some(f) => f(isl.cx, isl.cz),
                            None if self.terrain_only => std::rc::Rc::new(
                                crate::end::island_cols_compact(self.seed, isl.cx, isl.cz),
                            ),
                            None => std::rc::Rc::new(crate::end_homeland::island_compact_interior(
                                self.seed, isl.cx, isl.cz,
                            )),
                        };
                        self.islands.insert((isl.cx, isl.cz), (tick, ic.clone()));
                        ic
                    }
                };
                let (x0, z0) = (isl.cx * 16 + 8, isl.cz * 16 + 8);
                for lz in 0..16 {
                    for lx in 0..16 {
                        let (i, j) = (cx * 16 + lx - x0, cz * 16 + lz - z0);
                        if !(0..208).contains(&i) || !(0..208).contains(&j) {
                            continue;
                        }
                        let (s, st) = ic.world((j * 208 + i) as usize);
                        let col = &mut cols[(lz * 16 + lx) as usize];
                        col.0[0] |= s[0];
                        col.0[1] |= s[1];
                        col.1[0] |= st[0];
                        col.1[1] |= st[1];
                    }
                }
            }
            // HEE towers (start chunk ± their 48-block footprint): end-stone island + obsidian shell
            for (tx, tz) in crate::end::towers(self.seed, 0, cx - 2, cz - 2, cx + 1, cz + 1) {
                if self.towers.len() > 16 {
                    self.towers.clear();
                }
                let (seed, resets) = (self.seed, self.resets);
                let t = self
                    .towers
                    .entry((tx, tz))
                    .or_insert_with(|| crate::endtower::tower(seed, resets, tx, tz));
                for (x, y, z, b) in t.blocks() {
                    if x >> 4 != cx || z >> 4 != cz || !b.solid() {
                        continue;
                    }
                    let col = &mut cols[(((z & 15) * 16) + (x & 15)) as usize];
                    let (k, bit) = ((y >> 7) as usize, 1u128 << (y & 127));
                    col.0[k] |= bit;
                    if b == crate::endtower::TB::EndStone {
                        col.1[k] |= bit;
                    }
                }
            }
            self.cache.insert((cx, cz), cols);
        }
        &self.cache[&(cx, cz)]
    }

    pub fn col(&mut self, x: i32, z: i32) -> ([u128; 2], [u128; 2]) {
        self.chunk(x >> 4, z >> 4)[((z & 15) * 16 + (x & 15)) as usize]
    }

    /// `StoneType.findStoneType(world, x, y, z) != null` (end stone).
    pub fn stone(&mut self, x: i32, y: i32, z: i32) -> bool {
        if !(0..256).contains(&y) {
            return false;
        }
        let s = self.col(x, z).1;
        s[(y >> 7) as usize] >> (y & 127) & 1 != 0
    }

    /// Lowest and highest solid Y of column (x, z), `None` if empty.
    pub fn solid_range(&mut self, x: i32, z: i32) -> Option<(i32, i32)> {
        let s = self.col(x, z).0;
        let lo = if s[0] != 0 {
            s[0].trailing_zeros() as i32
        } else if s[1] != 0 {
            128 + s[1].trailing_zeros() as i32
        } else {
            return None;
        };
        let hi = if s[1] != 0 {
            255 - s[1].leading_zeros() as i32
        } else {
            127 - s[0].leading_zeros() as i32
        };
        Some((lo, hi))
    }
}

/// End variant of [`crate::gtores::candidate_rng`]: attempt `i` of oreseed (`ox`, `oz`) processed from chunk
/// (`pcx`, `pcz`), with the column-based vein Y.
#[allow(clippy::too_many_arguments)]
pub fn candidate_end(
    table: &VeinTable,
    ovs: i64,
    i: i32,
    ox: i32,
    oz: i32,
    pcx: i32,
    pcz: i32,
    w: &mut EndBlocks,
) -> Option<(Placement, Xstr)> {
    let total: i32 = table.overworld.iter().map(|v| v.weight).sum();
    if total <= 0 {
        return None;
    }
    let seed = fnv_int_pub(fnv_long_pub(fnv_offset(), ovs), i);
    let mut rng = Xstr::new(seed);
    let mut rem = rng.next_int(total);
    let mut pick = 0;
    for (k, v) in table.overworld.iter().enumerate() {
        rem -= v.weight;
        if rem < 0 {
            pick = k;
            break;
        }
    }
    let v = &table.overworld[pick];
    rng.set_seed(fnv_int_pub(seed, v.primary_id));
    let (sx, sz) = (ox * 16, oz * 16);
    let _ = rng.next_int(v.max_y - v.min_y - 5);
    let west = sx - rng.next_int(v.size);
    let east = sx + 16 + rng.next_int(v.size);
    let (lo, hi) = w
        .solid_range(pcx * 16 + 7, pcz * 16 + 9)
        .unwrap_or((128, 0));
    let min_y = lo + rng.next_int(hi - lo + 1);
    let north = sz - rng.next_int(v.size);
    let south = sz + 16 + rng.next_int(v.size);
    Some((
        Placement {
            vein: pick,
            min_y,
            west,
            east,
            north,
            south,
            rejected: 0,
        },
        rng,
    ))
}

/// The vein of End oreseed (`ox`, `oz`) when GT first reaches it from chunk (`pcx`, `pcz`).
#[allow(clippy::too_many_arguments)]
pub fn resolve_end_at(
    world_seed: i64,
    table: &VeinTable,
    cfg: &GtConfig,
    ox: i32,
    oz: i32,
    pcx: i32,
    pcz: i32,
    w: &mut EndBlocks,
) -> Option<Placement> {
    let ovs = orevein_seed(world_seed, END_DIM, ox, oz);
    if Xstr::new(ovs).next_int(100) >= cfg.percentage {
        return None;
    }
    let mut failed = 0;
    let mut i = 0;
    while i < cfg.attempts && failed < cfg.max_placement_attempts {
        let (mut p, rng) = candidate_end(table, ovs, i, ox, oz, pcx, pcz, w)?;
        let v = &table.overworld[p.vein];
        let r = execute_chunkified(
            v,
            &p,
            rng,
            ox,
            oz,
            pcx,
            pcz,
            true,
            |x, y, z, q| match q {
                BlockQuery::Stone | BlockQuery::Ore(_) => w.stone(x, y, z),
            },
            |_, _, _, _| {},
        );
        if r.accepted() {
            p.rejected = failed;
            return Some(p);
        }
        failed += 1;
        i += 1;
    }
    None
}

/// The End arrival chunk (obsidian platform at block (100, 0)).
pub const END_ARRIVAL: (i32, i32) = (100, 0);

/// Processing chunk for an End oreseed: [`crate::gtdims::processing_chunk_dim`] around the arrival platform,
/// moved to the window chunk nearest the arrival that processes ores if that one does not. `None` = no window
/// chunk processes ores (the oreseed never gets a vein).
pub fn processing_chunk_end(seed: i64, ox: i32, oz: i32) -> Option<(i32, i32)> {
    processing_chunk_end_with(ox, oz, |x, z| processes_ores(seed, x, z))
}

/// [`processing_chunk_end`] with a custom "chunk processes ores" test (e.g. [`OreZones::processes`]).
pub fn processing_chunk_end_with(
    ox: i32,
    oz: i32,
    processes_ores: impl Fn(i32, i32) -> bool,
) -> Option<(i32, i32)> {
    let p = crate::gtdims::processing_chunk_dim(ox, oz, END_ARRIVAL, crate::gtdims::ARRIVAL_RADIUS);
    if processes_ores(p.0, p.1) {
        return Some(p);
    }
    let (ax, az) = (END_ARRIVAL.0 >> 4, END_ARRIVAL.1 >> 4);
    let r = MAX_VEIN_SIZE;
    (oz - r..=oz + r)
        .flat_map(|z| (ox - r..=ox + r).map(move |x| (x, z)))
        .filter(|&(x, z)| processes_ores(x, z))
        .min_by_key(|&(x, z)| (((x - ax) as i64).pow(2) + ((z - az) as i64).pow(2), x, z))
}

/// The viewer's End vein for oreseed (`ox`, `oz`).
pub fn resolve_end(
    world_seed: i64,
    table: &VeinTable,
    cfg: &GtConfig,
    ox: i32,
    oz: i32,
    w: &mut EndBlocks,
) -> Option<Placement> {
    let (px, pz) = processing_chunk_end(world_seed, ox, oz)?;
    resolve_end_at(world_seed, table, cfg, ox, oz, px, pz, w)
}
