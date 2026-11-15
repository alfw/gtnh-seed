//! The Nether (dim −1) in GTNH 2.9.0, and GT ore veins outside the overworld (Nether, Twilight Forest).
//!
//! **Generators (read from the pack's mods + configs):**
//! - Provider: **BOP's `WorldProviderBOPHell`** — `BOPDimensions.init()` (FML init, unconditional in BOP 2.1.0.2308)
//!   unregisters provider type −1 and registers it. Natura `Override Nether=false`, Et Futurum
//!   `netherDimensionProvider=false`, no Netherlicious ⇒ nothing replaces it afterwards (confirm with the dump).
//!   Chunk manager `WorldChunkManagerBOPHell` → `BiomeLayerHell.initializeAllBiomeGenerators(seed, type, 1)`
//!   (`dim == 1` ⇒ biome size 2): create(1) → fuzzy zoom(2000) → zoom(2001, 2002) → **biomes(200)** → zoom(1000, 1001)
//!   → zoom(1000, 1001) → voronoi(10). Everything before `BiomeLayerHellBiomes` is the constant 1 and its output is
//!   unused (the layer ignores its parent), BOP's zoom/voronoi copies give the same values as vanilla's (window padding
//!   +3 instead of +2, same mode-or-random order, voronoi without `& 255`) ⇒ one biome cell per 4×4 chunks.
//!   Biome pick per cell: `WeightedRandom.getItem(list, (int) nextLong(total / 10) * 10)` (Forge `nextLong`) over
//!   `[Hell 10] + BOPBiomeManager.netherBiomes` in registration order with `biomeweights.cfg` weights:
//!   Corrupted Sands 10, Phantasmagoric Inferno 10, Boneyard 10, Visceral Heap 10, Undergarden 10, Polar Chasm 3
//!   (total 63 ⇒ `nextLong(6)·10` ∈ {0..50}: **Polar Chasm can never be picked**, the other six are equally likely).
//!   Ids (NEI dump): Hell 8, Corrupted Sands 136, Phantasmagoric Inferno 137, Boneyard 128, Visceral Heap 117,
//!   Undergarden 138, Polar Chasm 94. Pure seed function. Chunk generator `ChunkProviderBOPHell`.
//! - Fortresses: `MapGenNetherBridge` via `TerrainGen.getModdedMapGen` (nothing swaps it in the pack).
//!   `canSpawnStructureAtCoords` is seed-only: per 16×16-chunk region `Random((rx ^ rz << 4) ^ seed)`, `nextInt()`,
//!   1-in-3 region has a fortress, start chunk = region·16 + 4 + `nextInt(8)` on each axis. The start piece is a
//!   `Crossing3` at block (chunk·16 + 2, 64, chunk·16 + 2), 19×10×19; the whole fortress is then shifted by
//!   `setRandomHeight(48, 70)` (needs the pieces' bounding box → not ported).
//! - GT ores: `GTWorldgenerator` runs in every dim with a `DimensionDef` (Nether = `ChunkProviderHell`, TF =
//!   `ChunkProviderTwilightForest`), same oreseed maths as dim 0 with `dimensionId & 0xff` in the seed (−1 → 255,
//!   TF 7), vein list = `WorldgenGTOreLayer.sList` filtered by `canGenerateIn(dimName)` (`"Nether"`,
//!   `"Twilight Forest"`) and stone category, heights from `dimVeinHeights` (TF overrides). The lists below were
//!   derived from `OreMixes` + `MaterialsIDMap` at GT 5.09.54.265 and are **identical to the live lists** of
//!   rwg2dump 2.8 (`dims.json` gt, order, weights, heights, size, density and all ore ids; `tests/dims_dump28.rs`). The placement test (stone at the vein) needs the dimension's terrain, which is not
//!   ported for either dimension: [`gt_candidates`] gives attempt 0 and the alternatives.

use crate::genlayer::{world_gen_seed, BoxLayer, GenLayer, LayerRng, Voronoi, Zoom};
use crate::gtores::{candidate, orevein_seed, GtConfig, Placement, Vein, VeinTable, Xstr};
use crate::java_random::JavaRandom;

/// GT / Forge dimension id of the Nether.
pub const NETHER_DIM: i32 = -1;
/// `BiomeGenBase.hell` id.
pub const HELL_BIOME: i32 = 8;

/// `[Hell] + BOPBiomeManager.netherBiomes` as `(id, weight, name)` in list order (GTNH 2.9.0).
pub const BOP_NETHER_BIOMES_GTNH: [(i32, i32, &str); 7] = [
    (8, 10, "Hell"),
    (136, 10, "Corrupted Sands"),
    (137, 10, "Phantasmagoric Inferno"),
    (128, 10, "Boneyard"),
    (117, 10, "Visceral Heap"),
    (138, 10, "Undergarden"),
    (94, 3, "Polar Chasm"),
];

/// `BiomeLayerHellBiomes`.
struct HellBiomes {
    wgs: i64,
    entries: Vec<(i32, i32)>,
    total: i32,
}

impl GenLayer for HellBiomes {
    fn get_ints(&self, x: i32, z: i32, w: i32, h: i32) -> Vec<i32> {
        // (the parent chain is the constant 1 and its values are not used)
        let mut out = vec![0; (w * h) as usize];
        let mut rng = LayerRng::new(self.wgs);
        for dz in 0..h {
            for dx in 0..w {
                rng.init_chunk((dx + x) as i64, (dz + z) as i64);
                let mut weight = (rng.next_long((self.total / 10) as i64) as i32) * 10;
                let mut id = self.entries[0].0;
                for &(b, wt) in &self.entries {
                    weight -= wt;
                    if weight < 0 {
                        id = b;
                        break;
                    }
                }
                out[(dx + dz * w) as usize] = id;
            }
        }
        out
    }
}

/// The Nether biome provider (`WorldChunkManagerBOPHell`) for one world seed.
pub struct NetherBiomes {
    quarter: BoxLayer,
    blocks: BoxLayer,
}

impl NetherBiomes {
    pub fn new(seed: i64) -> Self {
        let e: Vec<(i32, i32)> = BOP_NETHER_BIOMES_GTNH.iter().map(|b| (b.0, b.1)).collect();
        Self::with_biomes(seed, &e)
    }

    /// `entries` = `[(id, weight)]` with Hell first, as `BiomeLayerHellBiomes` builds it.
    pub fn with_biomes(seed: i64, entries: &[(i32, i32)]) -> Self {
        let total = entries.iter().map(|e| e.1).sum();
        let mut l: BoxLayer = Box::new(HellBiomes {
            wgs: world_gen_seed(200, seed),
            entries: entries.to_vec(),
            total,
        });
        for s in [1000, 1001, 1000, 1001] {
            l = Box::new(Zoom::new(s, l, seed));
        }
        let quarter: std::rc::Rc<dyn GenLayer> = std::rc::Rc::from(l);
        let mut v = Voronoi::new(10, Box::new(SharedLayer(quarter.clone())), seed);
        v.mask_255 = false;
        NetherBiomes {
            quarter: Box::new(SharedLayer(quarter)),
            blocks: Box::new(v),
        }
    }

    /// `getBiomeGenAt` over a block rectangle (row-major `w × h`).
    pub fn biomes(&self, x: i32, z: i32, w: i32, h: i32) -> Vec<i32> {
        self.blocks.get_ints(x, z, w, h)
    }

    pub fn biome_at(&self, x: i32, z: i32) -> i32 {
        self.blocks.get_ints(x, z, 1, 1)[0]
    }

    /// `getBiomesForGeneration`: 1:4 cells.
    pub fn biomes_quarter(&self, x4: i32, z4: i32, w: i32, h: i32) -> Vec<i32> {
        self.quarter.get_ints(x4, z4, w, h)
    }
}

struct SharedLayer(std::rc::Rc<dyn GenLayer>);

impl GenLayer for SharedLayer {
    fn get_ints(&self, x: i32, z: i32, w: i32, h: i32) -> Vec<i32> {
        self.0.get_ints(x, z, w, h)
    }
}

/// A fortress start.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Fortress {
    pub chunk_x: i32,
    pub chunk_z: i32,
}

impl Fortress {
    /// Centre of the start crossing (x/z; its y is 64 before `setRandomHeight` moves the whole fortress, final
    /// y unknown without the pieces).
    pub fn start_center(&self) -> (i32, i32) {
        ((self.chunk_x << 4) + 2 + 9, (self.chunk_z << 4) + 2 + 9)
    }
}

/// The fortress of a 16×16-chunk region (`chunk >> 4`), if any.
pub fn fortress_in_region(seed: i64, region_x: i32, region_z: i32) -> Option<Fortress> {
    fortress_in_region_rand(seed, region_x, region_z).map(|(f, _)| f)
}

/// [`fortress_in_region`] plus the structure rand as `getStructureStart` receives it (pieces: [`crate::fortress`]).
pub fn fortress_in_region_rand(
    seed: i64,
    region_x: i32,
    region_z: i32,
) -> Option<(Fortress, JavaRandom)> {
    // MapGenBase.rand is Hodgepodge's StdLCG in GTNH (`fastRandom`): setSeed stores the seed unscrambled.
    let mut r = JavaRandom::new(((region_x ^ (region_z << 4)) as i64 ^ seed) ^ 0x5DEECE66D);
    r.next_int();
    if r.next_int_bounded(3) != 0 {
        return None;
    }
    let chunk_x = (region_x << 4) + 4 + r.next_int_bounded(8);
    let chunk_z = (region_z << 4) + 4 + r.next_int_bounded(8);
    Some((Fortress { chunk_x, chunk_z }, r))
}

/// `MapGenNetherBridge.canSpawnStructureAtCoords(chunkX, chunkZ)`.
pub fn is_fortress_chunk(seed: i64, chunk_x: i32, chunk_z: i32) -> bool {
    fortress_in_region(seed, chunk_x >> 4, chunk_z >> 4) == Some(Fortress { chunk_x, chunk_z })
}

/// Every fortress start with its chunk in `[cx0, cx1] × [cz0, cz1]` (inclusive), region-major order.
pub fn fortresses_in(seed: i64, cx0: i32, cz0: i32, cx1: i32, cz1: i32) -> Vec<Fortress> {
    let mut out = Vec::new();
    for rz in cz0 >> 4..=cz1 >> 4 {
        for rx in cx0 >> 4..=cx1 >> 4 {
            if let Some(f) = fortress_in_region(seed, rx, rz) {
                if f.chunk_x >= cx0 && f.chunk_x <= cx1 && f.chunk_z >= cz0 && f.chunk_z <= cz1 {
                    out.push(f);
                }
            }
        }
    }
    out
}

// ---------------------------------------------------------------------------------------------------------------
// GT ore veins in other dimensions

/// `(name, weight, minY, maxY, size, density, primaryId, [primary, secondary, between, sporadic])`, with the
/// dimension's effective heights (`dimVeinHeights` override, `validateHeights` applied).
pub type RawVein = (
    &'static str,
    i32,
    i32,
    i32,
    i32,
    i32,
    i32,
    [&'static str; 4],
);

/// `"Nether"` stone-vein list (= live list, rwg2dump 2.8) in `sList` order (GT 5.09.54.265 source; see module docs).
pub const GT_NETHER_VEINS: [RawVein; 11] = [
    (
        "ore.mix.iron",
        120,
        10,
        40,
        24,
        3,
        930,
        ["BrownLimonite", "YellowLimonite", "BandedIron", "Malachite"],
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
        "ore.mix.netherquartz",
        80,
        40,
        80,
        24,
        4,
        522,
        ["NetherQuartz", "NetherQuartz", "NetherQuartz", "Quartzite"],
    ),
    (
        "ore.mix.sulfur",
        100,
        5,
        20,
        24,
        4,
        22,
        ["Sulfur", "Sulfur", "Pyrite", "Sphalerite"],
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
        "ore.mix.redstone",
        60,
        5,
        40,
        24,
        2,
        810,
        ["Redstone", "Redstone", "Ruby", "Cinnabar"],
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
        "ore.mix.manganese",
        20,
        20,
        30,
        16,
        2,
        831,
        ["Grossular", "Spessartine", "Pyrolusite", "Tantalite"],
    ),
    (
        "ore.mix.quartz",
        20,
        80,
        120,
        16,
        2,
        523,
        ["Quartzite", "Barite", "CertusQuartz", "CertusQuartz"],
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
    (
        "ore.mix.saltpeterelectrotine",
        40,
        5,
        45,
        16,
        3,
        836,
        ["Saltpeter", "Diatomite", "Electrotine", "Alunite"],
    ),
];

/// `"Twilight Forest"` stone-vein list (= live list, rwg2dump 2.8) in `sList` order, TF height overrides applied.
pub const GT_TF_VEINS: [RawVein; 18] = [
    (
        "ore.mix.coal",
        80,
        15,
        40,
        32,
        5,
        535,
        ["Coal", "Coal", "Coal", "Lignite"],
    ),
    (
        "ore.mix.magnetite",
        160,
        20,
        32,
        32,
        2,
        870,
        ["Magnetite", "Magnetite", "Iron", "VanadiumMagnetite"],
    ),
    (
        "ore.mix.gold",
        160,
        15,
        30,
        32,
        2,
        870,
        ["Magnetite", "Magnetite", "VanadiumMagnetite", "Gold"],
    ),
    (
        "ore.mix.iron",
        120,
        5,
        20,
        24,
        3,
        930,
        ["BrownLimonite", "YellowLimonite", "BandedIron", "Malachite"],
    ),
    (
        "ore.mix.cassiterite",
        50,
        20,
        32,
        24,
        4,
        57,
        ["Tin", "Tin", "Cassiterite", "Tin"],
    ),
    (
        "ore.mix.salts",
        50,
        25,
        45,
        24,
        2,
        944,
        ["RockSalt", "Salt", "Lepidolite", "Spodumene"],
    ),
    (
        "ore.mix.soapstone",
        40,
        10,
        25,
        16,
        2,
        877,
        ["Soapstone", "Talc", "Glauconite", "Pentlandite"],
    ),
    (
        "ore.mix.nickel",
        40,
        5,
        20,
        16,
        2,
        906,
        ["Garnierite", "Nickel", "Cobaltite", "Pentlandite"],
    ),
    (
        "ore.mix.molybdenum",
        5,
        10,
        25,
        16,
        2,
        882,
        ["Wulfenite", "Molybdenite", "Molybdenum", "Powellite"],
    ),
    (
        "ore.mix.sapphire",
        60,
        10,
        25,
        16,
        2,
        820,
        ["Almandine", "Pyrope", "Sapphire", "GreenSapphire"],
    ),
    (
        "ore.mix.diamond",
        40,
        5,
        20,
        16,
        1,
        865,
        ["Graphite", "Graphite", "Diamond", "Coal"],
    ),
    (
        "ore.mix.olivine",
        60,
        10,
        25,
        16,
        2,
        927,
        ["Bentonite", "Magnesite", "Olivine", "Glauconite"],
    ),
    (
        "ore.mix.apatite",
        60,
        20,
        30,
        16,
        2,
        530,
        ["Apatite", "Apatite", "TricalciumPhosphate", "Pyrochlore"],
    ),
    (
        "ore.mix.lapis",
        40,
        10,
        25,
        16,
        4,
        524,
        ["Lazurite", "Sodalite", "Lapis", "Calcite"],
    ),
    (
        "ore.mix.aquaignis",
        16,
        5,
        20,
        16,
        2,
        543,
        ["InfusedWater", "InfusedFire", "Amber", "Cinnabar"],
    ),
    (
        "ore.mix.terraaer",
        16,
        5,
        20,
        16,
        2,
        542,
        ["InfusedEarth", "InfusedAir", "Amber", "Cinnabar"],
    ),
    (
        "ore.mix.perditioordo",
        16,
        5,
        20,
        16,
        2,
        544,
        ["InfusedEntropy", "InfusedOrder", "Amber", "Cinnabar"],
    ),
    (
        "ore.mix.tfgalena",
        40,
        5,
        30,
        16,
        4,
        830,
        ["Galena", "Silver", "Lead", "Cryolite"],
    ),
];

/// A [`VeinTable`] (its `overworld` field is just "the dimension's list") from raw rows.
pub fn vein_table(rows: &[RawVein]) -> VeinTable {
    VeinTable {
        overworld: rows
            .iter()
            .map(|r| Vein {
                name: r.0.to_string(),
                weight: r.1,
                min_y: r.2,
                max_y: r.3,
                size: r.4,
                density: r.5,
                primary_id: r.6,
                ores: [
                    r.7[0].to_string(),
                    r.7[1].to_string(),
                    r.7[2].to_string(),
                    r.7[3].to_string(),
                ],
            })
            .collect(),
    }
}

/// The first `n` placement attempts of an oreseed in dimension `dim` (attempt `i` is GT's i-th pick; the game takes
/// the first one whose placement test passes, at most `max_placement_attempts` failures). Empty if the
/// `oreveinPercentage` roll fails (never with GTNH's 100) or the list is empty. `world_seed` must be the dimension's
/// `World.getSeed()` (TF: [`crate::twilight::tf_seed`]).
pub fn gt_candidates(
    world_seed: i64,
    dim: i32,
    table: &VeinTable,
    cfg: &GtConfig,
    ox: i32,
    oz: i32,
    n: i32,
) -> Vec<Placement> {
    let ovs = orevein_seed(world_seed, dim, ox, oz);
    if Xstr::new(ovs).next_int(100) >= cfg.percentage {
        return Vec::new();
    }
    (0..n.min(cfg.attempts))
        .filter_map(|i| candidate(table, ovs, i, ox, oz))
        .collect()
}
