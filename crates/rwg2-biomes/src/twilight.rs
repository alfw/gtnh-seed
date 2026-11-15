//! Twilight Forest 2.7.45 (GTNH fork, dimension 7 in GTNH 2.9.0): biome map and major-feature placement.
//!
//! - Seed: `WorldProviderTwilightForest.getSeed()` = `TwilightForestSeed.hashCode()` if that config string is set,
//!   else the world seed. Forge routes `World.getSeed()` through the provider, so the layer stack, the feature RNG and
//!   GT's oreveins in the TF all use it. GTNH ships `TwilightForestSeed=` (empty) ⇒ TF seed = world seed.
//! - Biomes: `TFWorldChunkManager` with `GenLayerTF.makeTheWorld` (`OldMapGen=false` in GTNH): TF base biomes →
//!   key biomes → companions → zoom ×2 → stabilize → thorn border → zoom ×4 → stream (+ smooth) → river mix →
//!   voronoi. Pure seed function (no terrain, no world). `getBiomeGenAt(x, z)` = voronoi layer at the block;
//!   `getBiomesForGeneration` = river-mix layer at 1:4.
//! - Features (`TFFeature.generateFeatureFor1Point7` + `getNearestCenterXYZ`): one feature per 16×16-chunk region
//!   (`region = (chunk + 8) >> 4`); the feature chunk is a seed-independent ±3-chunk jitter of the region centre; the
//!   feature kind comes from the biome at the region centre block `(16·region·16 + 8)` and, for "plain" biomes, from
//!   `Random(seed + cx·25117 + cz·151121).nextInt(16)` and the fixed lich-tower / naga positions.
//!
//! Biome ids are configurable (`config/TwilightForest.cfg`); [`TfBiomeIds::GTNH_290`] are the pack's values (match the
//! NEI biome registry dump).

use crate::genlayer::{world_gen_seed, BoxLayer, GenLayer, LayerRng, Smooth, Voronoi, Zoom};
use crate::java_random::JavaRandom;

/// `TFBiomeBase` ids.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct TfBiomeIds {
    pub lake: i32,
    pub twilight_forest: i32,
    pub twilight_forest2: i32,
    pub highlands: i32,
    pub mushrooms: i32,
    pub swamp: i32,
    pub stream: i32,
    pub snow: i32,
    pub glacier: i32,
    pub clearing: i32,
    pub oak_savanna: i32,
    pub firefly_forest: i32,
    pub deep_mushrooms: i32,
    pub dark_forest: i32,
    pub enchanted_forest: i32,
    pub fire_swamp: i32,
    pub dark_forest_center: i32,
    pub highlands_center: i32,
    pub thornlands: i32,
}

impl TfBiomeIds {
    /// GTNH 2.9.0 `config/TwilightForest.cfg` (= NEI registry dump of the owner's instance).
    pub const GTNH_290: TfBiomeIds = TfBiomeIds {
        clearing: 170,
        dark_forest: 171,
        dark_forest_center: 172,
        deep_mushrooms: 173,
        fire_swamp: 175,
        glacier: 176,
        highlands: 177,
        highlands_center: 178,
        lake: 179,
        firefly_forest: 180,
        mushrooms: 181,
        oak_savanna: 182,
        stream: 184,
        swamp: 185,
        thornlands: 186,
        twilight_forest: 187,
        twilight_forest2: 188,
        enchanted_forest: 189,
        snow: 196,
    };

    /// `(id, in-game name)` for legends.
    pub fn names(&self) -> [(i32, &'static str); 19] {
        [
            (self.clearing, "Twilight Clearing"),
            (self.dark_forest, "Dark Forest"),
            (self.dark_forest_center, "Dark Forest Center"),
            (self.deep_mushrooms, "Deep Mushroom Forest"),
            (self.fire_swamp, "Fire Swamp"),
            (self.glacier, "Twilight Glacier"),
            (self.highlands, "Twilight Highlands"),
            (self.highlands_center, "Highlands Center"),
            (self.lake, "Twilight Lake"),
            (self.firefly_forest, "Firefly Forest"),
            (self.mushrooms, "Mushroom Forest"),
            (self.oak_savanna, "Oak Savanna"),
            (self.stream, "Twilight Stream"),
            (self.swamp, "Twilight Swamp"),
            (self.thornlands, "Thornlands"),
            (self.twilight_forest, "Twilight Forest"),
            (self.twilight_forest2, "Dense Twilight Forest"),
            (self.enchanted_forest, "Enchanted Forest"),
            (self.snow, "Snowy Forest"),
        ]
    }

    /// Map colour from `TFBiomeBase.setColor`.
    pub fn color(&self, id: i32) -> u32 {
        let b = self;
        match id {
            _ if id == b.lake => 0x0000ff,
            _ if id == b.twilight_forest => 0x005500,
            _ if id == b.twilight_forest2 => 0x005522,
            _ if id == b.highlands => 0x556644,
            _ if id == b.mushrooms => 0x226622,
            _ if id == b.swamp => 0x334422,
            _ if id == b.stream => 0x3253b7,
            _ if id == b.snow => 0xccffff,
            _ if id == b.glacier => 0x7777bb,
            _ if id == b.clearing => 0x349b34,
            _ if id == b.oak_savanna => 0x446622,
            _ if id == b.firefly_forest => 0x006633,
            _ if id == b.deep_mushrooms => 0x663322,
            _ if id == b.dark_forest => 0x003311,
            _ if id == b.enchanted_forest => 0x115566,
            _ if id == b.fire_swamp => 0x42231a,
            _ if id == b.dark_forest_center => 0x002200,
            _ if id == b.highlands_center => 0x334422,
            _ if id == b.thornlands => 0x223322,
            _ => 0,
        }
    }
}

// ---------------------------------------------------------------------------------------------------------------
// TF layers (verbatim logic of twilightforest.world.layer.*)

/// `GenLayerTFBiomes1Point7`.
struct TfBiomes17 {
    wgs: i64,
    common: [i32; 5],
    rare: [i32; 4],
}

impl GenLayer for TfBiomes17 {
    fn get_ints(&self, x: i32, z: i32, w: i32, h: i32) -> Vec<i32> {
        let mut out = vec![0; (w * h) as usize];
        let mut rng = LayerRng::new(self.wgs);
        for dz in 0..h {
            for dx in 0..w {
                rng.init_chunk((dx + x) as i64, (dz + z) as i64);
                out[(dx + dz * w) as usize] = if rng.next_int(15) == 0 {
                    self.rare[rng.next_int(4) as usize]
                } else {
                    self.common[rng.next_int(5) as usize]
                };
            }
        }
        out
    }
}

/// `GenLayerTFKeyBiomes`.
struct TfKeyBiomes {
    parent: BoxLayer,
    wgs: i64,
    ids: TfBiomeIds,
}

impl TfKeyBiomes {
    fn key_biome_for(&self, rng: &mut LayerRng, map_x: i32, map_z: i32, index: i32) -> i32 {
        let rx = (map_x + 4) >> 3;
        let rz = (map_z + 4) >> 3;
        rng.init_chunk(rx as i64, rz as i64);
        let offset = rng.next_int(4);
        match (index + offset) % 4 {
            1 => self.ids.fire_swamp,
            2 => self.ids.dark_forest_center,
            3 => self.ids.highlands_center,
            _ => self.ids.glacier,
        }
    }
}

impl GenLayer for TfKeyBiomes {
    fn get_ints(&self, x: i32, z: i32, w: i32, h: i32) -> Vec<i32> {
        let src = self.parent.get_ints(x, z, w, h);
        let mut out = vec![0; (w * h) as usize];
        let mut rng = LayerRng::new(self.wgs);
        for dz in 0..h {
            for dx in 0..w {
                let (ax, az) = (dx + x, dz + z);
                rng.init_chunk((ax | 3) as i64, (az | 3) as i64);
                let ox = rng.next_int(3) + 1;
                let oz = rng.next_int(3) + 1;
                let i = (dx + dz * w) as usize;
                out[i] = if (ax & 3) == ox && (az & 3) == oz {
                    let index = match ((ax & 4) == 0, (az & 4) == 0) {
                        (true, true) => 0,
                        (true, false) => 1,
                        (false, true) => 2,
                        (false, false) => 3,
                    };
                    self.key_biome_for(&mut rng, ax, az, index)
                } else {
                    src[i]
                };
            }
        }
        out
    }
}

/// 3×3-neighbourhood layers without randomness: companion biomes, stabilize, thorn border, stream.
#[derive(Clone, Copy)]
enum Neigh {
    Companion,
    Stabilize,
    ThornBorder,
    Stream,
}

struct TfNeigh {
    parent: BoxLayer,
    kind: Neigh,
    ids: TfBiomeIds,
}

impl TfNeigh {
    /// `GenLayerTFStream.shouldStream(biome1, biome2)`.
    fn should_stream_pair(&self, b1: i32, b2: i32) -> bool {
        let t = &self.ids;
        let pair = |a: i32, b: i32| (b1 == a && b2 == b) || (b1 == b && b2 == a);
        if b1 == b2 || b1 == b2.wrapping_neg() {
            return false;
        }
        if pair(t.glacier, t.snow)
            || pair(t.deep_mushrooms, t.mushrooms)
            || pair(t.swamp, t.fire_swamp)
            || pair(t.highlands, t.highlands_center)
            || pair(t.dark_forest, t.dark_forest_center)
        {
            return false;
        }
        if b1 == t.lake || b2 == t.lake {
            return false;
        }
        // (sic) the clearing/savanna tests are "either side", not a pair
        if b1 == t.clearing || b2 == t.oak_savanna {
            return false;
        }
        if b1 == t.oak_savanna || b2 == t.clearing {
            return false;
        }
        if b1 == t.thornlands || b2 == t.thornlands {
            return false;
        }
        true
    }
}

impl GenLayer for TfNeigh {
    fn get_ints(&self, x: i32, z: i32, w: i32, h: i32) -> Vec<i32> {
        let nw = w + 2;
        let src = self.parent.get_ints(x - 1, z - 1, nw, h + 2);
        let at = |i: i32, j: i32| src[(i + j * nw) as usize];
        let t = &self.ids;
        let mut out = vec![0; (w * h) as usize];
        let (off_x, off_z) = (x & 3, z & 3);
        for dz in 0..h {
            for dx in 0..w {
                // Java names: "right" is the -x neighbour, "left" +x, "up" -z, "down" +z
                let right = at(dx, dz + 1);
                let left = at(dx + 2, dz + 1);
                let up = at(dx + 1, dz);
                let down = at(dx + 1, dz + 2);
                let center = at(dx + 1, dz + 1);
                let v = match self.kind {
                    Neigh::Companion => {
                        let is_key = |b: i32| {
                            center != b && (right == b || left == b || up == b || down == b)
                        };
                        if is_key(t.fire_swamp) {
                            t.swamp
                        } else if is_key(t.glacier) {
                            t.snow
                        } else if is_key(t.dark_forest_center) {
                            t.dark_forest
                        } else if is_key(t.highlands_center) {
                            t.highlands
                        } else {
                            center
                        }
                    }
                    Neigh::Stabilize => {
                        let cx = ((dx + off_x + 1) & !3) - off_x;
                        let cz = ((dz + off_z + 1) & !3) - off_z;
                        if dx <= cx + 1 && dx >= cx - 1 && dz <= cz + 1 && dz >= cz - 1 {
                            at(cx + 1, cz + 1)
                        } else {
                            center
                        }
                    }
                    Neigh::ThornBorder => {
                        let hc = t.highlands_center;
                        let on_border = |a: i32, b: i32, c: i32, d: i32| {
                            center != hc && (a == hc || b == hc || c == hc || d == hc)
                        };
                        let ur = at(dx, dz);
                        let ul = at(dx + 2, dz);
                        let dr = at(dx, dz + 2);
                        let dl = at(dx + 2, dz + 2);
                        if on_border(right, left, up, down) || on_border(ur, ul, dr, dl) {
                            t.thornlands
                        } else {
                            center
                        }
                    }
                    Neigh::Stream => {
                        // GenLayerTFStream names: left = -x, right = +x, down = -z, up = +z
                        let (l, r, d, u) = (right, left, up, down);
                        let s = self.should_stream_pair(center, l)
                            || self.should_stream_pair(center, r)
                            || self.should_stream_pair(center, d)
                            || self.should_stream_pair(center, u);
                        if s {
                            t.stream
                        } else {
                            -1
                        }
                    }
                };
                out[(dx + dz * w) as usize] = v;
            }
        }
        out
    }
}

/// `GenLayerTFRiverMix`.
struct TfRiverMix {
    biomes: std::rc::Rc<dyn GenLayer>,
    river: BoxLayer,
    stream: i32,
    max_biome_ids: i32,
}

impl GenLayer for TfRiverMix {
    fn get_ints(&self, x: i32, z: i32, w: i32, h: i32) -> Vec<i32> {
        let b = self.biomes.get_ints(x, z, w, h);
        let r = self.river.get_ints(x, z, w, h);
        b.iter()
            .zip(r.iter())
            .map(|(&b, &r)| {
                if r == self.stream {
                    r & (self.max_biome_ids - 1)
                } else {
                    b
                }
            })
            .collect()
    }
}

/// Shares one layer between the biome and river branches (Java passes the same object to both).
struct Shared(std::rc::Rc<dyn GenLayer>);

impl GenLayer for Shared {
    fn get_ints(&self, x: i32, z: i32, w: i32, h: i32) -> Vec<i32> {
        self.0.get_ints(x, z, w, h)
    }
}

// ---------------------------------------------------------------------------------------------------------------
// Features

/// `TFFeature` constants (id = `featureID`, size = feature radius in chunks used by `getNearestFeature`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct TfFeature {
    pub id: u8,
    pub size: u8,
    pub name: &'static str,
    /// `isStructureEnabled`: generated by `MapGenTFMajorFeature` (false = terrain/decoration feature).
    pub structure: bool,
}

pub const TF_FEATURES: [TfFeature; 21] = [
    TfFeature {
        id: 0,
        size: 0,
        name: "No Feature",
        structure: false,
    },
    TfFeature {
        id: 1,
        size: 1,
        name: "Small Hollow Hill",
        structure: true,
    },
    TfFeature {
        id: 2,
        size: 2,
        name: "Medium Hollow Hill",
        structure: true,
    },
    TfFeature {
        id: 3,
        size: 3,
        name: "Large Hollow Hill",
        structure: true,
    },
    TfFeature {
        id: 4,
        size: 2,
        name: "Hedge Maze",
        structure: true,
    },
    TfFeature {
        id: 5,
        size: 3,
        name: "Naga Courtyard",
        structure: true,
    },
    TfFeature {
        id: 6,
        size: 1,
        name: "Lich Tower",
        structure: true,
    },
    TfFeature {
        id: 7,
        size: 2,
        name: "Ice Tower",
        structure: true,
    },
    TfFeature {
        id: 8,
        size: 1,
        name: "Quest Island",
        structure: false,
    },
    TfFeature {
        id: 9,
        size: 1,
        name: "Quest Grove",
        structure: true,
    },
    TfFeature {
        id: 10,
        size: 1,
        name: "Druid Grove",
        structure: false,
    },
    TfFeature {
        id: 11,
        size: 3,
        name: "Floating Ruins",
        structure: false,
    },
    TfFeature {
        id: 12,
        size: 2,
        name: "Hydra Lair",
        structure: true,
    },
    TfFeature {
        id: 13,
        size: 3,
        name: "Labyrinth",
        structure: true,
    },
    TfFeature {
        id: 14,
        size: 1,
        name: "Dark Tower",
        structure: true,
    },
    TfFeature {
        id: 15,
        size: 3,
        name: "Knight Stronghold",
        structure: true,
    },
    TfFeature {
        id: 16,
        size: 3,
        name: "World Tree",
        structure: false,
    },
    TfFeature {
        id: 17,
        size: 2,
        name: "Yeti Lairs",
        structure: true,
    },
    TfFeature {
        id: 18,
        size: 3,
        name: "Troll Lairs",
        structure: true,
    },
    TfFeature {
        id: 19,
        size: 3,
        name: "Final Castle",
        structure: true,
    },
    TfFeature {
        id: 20,
        size: 2,
        name: "Mushroom Tower",
        structure: true,
    },
];

pub mod feature_id {
    pub const NOTHING: u8 = 0;
    pub const HILL1: u8 = 1;
    pub const HILL2: u8 = 2;
    pub const HILL3: u8 = 3;
    pub const HEDGE_MAZE: u8 = 4;
    pub const NAGA_COURTYARD: u8 = 5;
    pub const LICH_TOWER: u8 = 6;
    pub const ICE_TOWER: u8 = 7;
    pub const QUEST_ISLAND: u8 = 8;
    pub const QUEST_GROVE: u8 = 9;
    pub const HYDRA_LAIR: u8 = 12;
    pub const LABYRINTH: u8 = 13;
    pub const DARK_TOWER: u8 = 14;
    pub const KNIGHT_STRONGHOLD: u8 = 15;
    pub const YETI_CAVE: u8 = 17;
    pub const TROLL_CAVE: u8 = 18;
    pub const FINAL_CASTLE: u8 = 19;
    pub const MUSHROOM_TOWER: u8 = 20;
}

/// One region's major feature.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct FeatureAt {
    pub region_x: i32,
    pub region_z: i32,
    /// The feature chunk (`isInFeatureChunk` is true only here).
    pub chunk_x: i32,
    pub chunk_z: i32,
    /// `getNearestCenterXYZ` block (chunk·16 + 8, or + 9 in negative regions); y = TF sea level 31.
    pub x: i32,
    pub z: i32,
    pub feature: TfFeature,
    /// Biome at the region centre block that decided the feature.
    pub biome: i32,
}

/// Region of a chunk (`(chunk + 8) >> 4`).
#[inline]
pub fn region_of_chunk(c: i32) -> i32 {
    (c + 8) >> 4
}

/// `TFFeature.getNearestCenterXYZ` for a region (seed-independent).
pub fn region_center_block(region_x: i32, region_z: i32) -> (i32, i32) {
    let mut seed =
        (region_x as i64).wrapping_mul(3129871) ^ (region_z as i64).wrapping_mul(116129781);
    seed = seed
        .wrapping_mul(seed)
        .wrapping_mul(42317861)
        .wrapping_add(seed.wrapping_mul(7));
    let num0 = ((seed >> 12) & 3) as i32;
    let num1 = ((seed >> 15) & 3) as i32;
    let num2 = ((seed >> 18) & 3) as i32;
    let num3 = ((seed >> 21) & 3) as i32;
    let center_x = 8 + num0 - num1;
    let center_z = 8 + num2 - num3;
    let ccz = if region_z >= 0 {
        (region_z * 16 + center_z - 8) * 16 + 8
    } else {
        (region_z * 16 + (16 - center_z) - 8) * 16 + 9
    };
    let ccx = if region_x >= 0 {
        (region_x * 16 + center_x - 8) * 16 + 8
    } else {
        (region_x * 16 + (16 - center_x) - 8) * 16 + 9
    };
    (ccx, ccz)
}

/// `TFWorldChunkManager.isInFeatureChunk` (block coords → chunk).
pub fn is_feature_chunk(chunk_x: i32, chunk_z: i32) -> bool {
    let (x, z) = region_center_block(region_of_chunk(chunk_x), region_of_chunk(chunk_z));
    chunk_x == x >> 4 && chunk_z == z >> 4
}

// ---------------------------------------------------------------------------------------------------------------

/// The TF biome provider for one TF seed.
pub struct TwilightForest {
    pub seed: i64,
    pub ids: TfBiomeIds,
    /// River-mix layer (`unzoomedBiomes`, 1:4).
    quarter: BoxLayer,
    /// Voronoi layer (`zoomedBiomes`, 1:1).
    blocks: BoxLayer,
    /// `feature_for_region` memo (a pure function of the region; terrain deformation asks for the same
    /// regions dozens of times per chunk).
    features: std::cell::RefCell<crate::jmath::FxMap<(i32, i32), FeatureAt>>,
}

impl TwilightForest {
    /// `seed` = TF seed ([`tf_seed`]). GTNH: EndlessIDs is loaded ⇒ `maxBiomeIDs = 65536`.
    pub fn new(seed: i64) -> Self {
        Self::with_ids(seed, TfBiomeIds::GTNH_290, 65536)
    }

    pub fn with_ids(seed: i64, ids: TfBiomeIds, max_biome_ids: i32) -> Self {
        let w = seed;
        let neigh =
            |parent: BoxLayer, kind: Neigh| -> BoxLayer { Box::new(TfNeigh { parent, kind, ids }) };
        let mut b: BoxLayer = Box::new(TfBiomes17 {
            wgs: world_gen_seed(1, w),
            common: [
                ids.twilight_forest,
                ids.twilight_forest2,
                ids.mushrooms,
                ids.oak_savanna,
                ids.firefly_forest,
            ],
            rare: [
                ids.lake,
                ids.deep_mushrooms,
                ids.enchanted_forest,
                ids.clearing,
            ],
        });
        b = Box::new(TfKeyBiomes {
            parent: b,
            wgs: world_gen_seed(1000, w),
            ids,
        });
        b = neigh(b, Neigh::Companion); // GenLayerTFCompanionBiomes(1000): no draws
        b = Box::new(Zoom::new(1000, b, w));
        b = Box::new(Zoom::new(1001, b, w));
        b = neigh(b, Neigh::Stabilize); // (700): no draws
        b = neigh(b, Neigh::ThornBorder); // (500): no draws
        for s in 1002..=1005 {
            b = Box::new(Zoom::new(s, b, w));
        }
        let biomes: std::rc::Rc<dyn GenLayer> = std::rc::Rc::from(b);
        let mut river: BoxLayer = neigh(Box::new(Shared(biomes.clone())), Neigh::Stream); // GenLayerTFStream(1)
        river = Box::new(Smooth::new(7000, river, w));
        let mix = std::rc::Rc::new(TfRiverMix {
            biomes,
            river,
            stream: ids.stream,
            max_biome_ids,
        });
        let quarter: BoxLayer = Box::new(Shared(mix.clone()));
        let blocks: BoxLayer = Box::new(Voronoi::new(10, Box::new(Shared(mix)), w));
        TwilightForest {
            seed,
            ids,
            quarter,
            blocks,
            features: Default::default(),
        }
    }

    /// `getBiomeGenAt` over a block rectangle (row-major `w × h`). Negative ids never occur here
    /// (the null-biome fallback to Twilight Forest is applied anyway).
    pub fn biomes(&self, x: i32, z: i32, w: i32, h: i32) -> Vec<i32> {
        let tf = self.ids.twilight_forest;
        let mut v = self.blocks.get_ints(x, z, w, h);
        for b in v.iter_mut() {
            if *b < 0 {
                *b = tf;
            }
        }
        v
    }

    pub fn biome_at(&self, x: i32, z: i32) -> i32 {
        self.biomes(x, z, 1, 1)[0]
    }

    /// `getBiomesForGeneration`: 1:4 cells (cell (i, j) covers blocks 4i..4i+3; for overviews).
    pub fn biomes_quarter(&self, x4: i32, z4: i32, w: i32, h: i32) -> Vec<i32> {
        let tf = self.ids.twilight_forest;
        let mut v = self.quarter.get_ints(x4, z4, w, h);
        for b in v.iter_mut() {
            if *b < 0 {
                *b = tf;
            }
        }
        v
    }

    /// `TFFeature.generateFeatureFor1Point7(chunkX, chunkZ, world)` (the chunk is rounded to its region centre).
    pub fn feature_for_chunk(&self, chunk_x: i32, chunk_z: i32) -> (TfFeature, i32) {
        use feature_id::*;
        // Math.round(c / 16F) * 16 == ((c + 8) >> 4) << 4 for every int chunk coordinate in range
        let cx = region_of_chunk(chunk_x) << 4;
        let cz = region_of_chunk(chunk_z) << 4;
        let biome = self.biome_at((cx << 4) + 8, (cz << 4) + 8);
        let mut rng = JavaRandom::new(
            self.seed
                .wrapping_add((cx as i64).wrapping_mul(25117))
                .wrapping_add((cz as i64).wrapping_mul(151121)),
        );
        let randnum = rng.next_int_bounded(16);
        let t = &self.ids;
        let f = |id: u8| (TF_FEATURES[id as usize], biome);
        if biome == t.glacier {
            return f(ICE_TOWER);
        }
        if biome == t.snow {
            return f(YETI_CAVE);
        }
        if biome == t.lake {
            return f(QUEST_ISLAND);
        }
        if biome == t.enchanted_forest {
            return f(QUEST_GROVE);
        }
        if biome == t.fire_swamp {
            return f(HYDRA_LAIR);
        }
        if biome == t.swamp {
            return f(LABYRINTH);
        }
        if biome == t.dark_forest {
            return f(KNIGHT_STRONGHOLD);
        }
        if biome == t.dark_forest_center {
            return f(DARK_TOWER);
        }
        if biome == t.highlands_center {
            return f(FINAL_CASTLE);
        }
        if biome == t.highlands {
            return f(TROLL_CAVE);
        }
        if biome == t.deep_mushrooms {
            return f(MUSHROOM_TOWER);
        }
        let rox = (((cx + 64) >> 4) % 8).abs();
        let roz = (((cz + 64) >> 4) % 8).abs();
        if (rox == 4 && roz == 5) || (rox == 4 && roz == 3) {
            return f(LICH_TOWER);
        }
        if (rox == 5 && roz == 4) || (rox == 3 && roz == 4) {
            return f(NAGA_COURTYARD);
        }
        f(match randnum {
            6..=8 => HILL2,
            9 => HILL3,
            10 | 11 => HEDGE_MAZE,
            12 | 13 => NAGA_COURTYARD,
            14 | 15 => LICH_TOWER,
            _ => HILL1,
        })
    }

    /// The feature of a 16×16-chunk region.
    pub fn feature_for_region(&self, region_x: i32, region_z: i32) -> FeatureAt {
        if let Some(f) = self.features.borrow().get(&(region_x, region_z)) {
            return *f;
        }
        let f = self.feature_for_region_uncached(region_x, region_z);
        let mut m = self.features.borrow_mut();
        if m.len() > 1 << 16 {
            m.clear();
        }
        m.insert((region_x, region_z), f);
        f
    }

    fn feature_for_region_uncached(&self, region_x: i32, region_z: i32) -> FeatureAt {
        let (x, z) = region_center_block(region_x, region_z);
        let (feature, biome) = self.feature_for_chunk(x >> 4, z >> 4);
        FeatureAt {
            region_x,
            region_z,
            chunk_x: x >> 4,
            chunk_z: z >> 4,
            x,
            z,
            feature,
            biome,
        }
    }

    /// `TFFeature.getFeatureDirectlyAt(chunk)`: the feature if this is the feature chunk.
    pub fn feature_at_chunk(&self, chunk_x: i32, chunk_z: i32) -> Option<FeatureAt> {
        let f = self.feature_for_region(region_of_chunk(chunk_x), region_of_chunk(chunk_z));
        (f.chunk_x == chunk_x && f.chunk_z == chunk_z).then_some(f)
    }

    /// Every feature whose chunk lies in `[cx0, cx1] × [cz0, cz1]` (inclusive chunk bounds).
    pub fn features_in(&self, cx0: i32, cz0: i32, cx1: i32, cz1: i32) -> Vec<FeatureAt> {
        let mut out = Vec::new();
        for rz in region_of_chunk(cz0) - 1..=region_of_chunk(cz1) + 1 {
            for rx in region_of_chunk(cx0) - 1..=region_of_chunk(cx1) + 1 {
                let (x, z) = region_center_block(rx, rz);
                let (c, d) = (x >> 4, z >> 4);
                if c >= cx0 && c <= cx1 && d >= cz0 && d <= cz1 {
                    out.push(self.feature_for_region(rx, rz));
                }
            }
        }
        out
    }
}

/// TF seed from the world seed and the `TwilightForestSeed` config string (empty in GTNH).
pub fn tf_seed(world_seed: i64, config_seed: &str) -> i64 {
    if config_seed.is_empty() {
        world_seed
    } else {
        crate::gtfluids::java_string_hash(config_seed) as i64
    }
}

/// GT dimension id of the Twilight Forest in GTNH (`dimensionID=7`), for `orevein_seed`.
pub const TF_DIM: i32 = 7;
