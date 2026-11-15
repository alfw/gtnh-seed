//! World spawn point (MC 1.7.10 `WorldServer.createSpawnPosition` on an RWG2 world, GTNH 2.9.0).
//!
//! RWG2: `getBiomesToSpawnIn()` is empty and `findBiomePosition` returns null without touching the Random, so
//! vanilla starts at (0, 0) and walks with `rand = new Random(worldSeed)`:
//! `while (!provider.canCoordinateBeSpawn(x, z)) { x += nextInt(64) - nextInt(64); z += nextInt(64) -
//! nextInt(64); if (++n == 1000) break; }`. Spawn Y is `getAverageGroundLevel()` (64).
//!
//! In GTNH the overworld provider is BOP's `WorldProviderSurfaceBOP` (rwg2dump 2.7), whose
//! `canCoordinateBeSpawn` climbs like `getTopBlock` (y = 63; while the block above is not air, y++) and accepts
//! a top of `Blocks.sand` or `Blocks.stone` (snow layers only in spawn-list biomes: RWG2 has none; its water
//! check is buggy and always passes). Grass, gravel, cobblestone, dirt, clay and BOP blocks do not count.
//!
//! The walk positions are pure seed maths (exact). Only the stopping index needs the world: the top block of
//! each visited column. Visited chunks are generated (`findingSpawnPoint` forces loads) but only populated once
//! a 2x2 block of them is loaded, so most checks see the pre-population surface, modelled here from the exact
//! `chunkgen` heights and the RWG2 surface classes (`paintTerrain`) per biome. Each column gets a probability of
//! a sand/stone top; the cliff stone/cobblestone picks are replayed on the chunk Random, so tops are exact up to
//! caves and unmodelled biomes. The estimate is the most likely stopping step.
//! ✅ seed 1337 CONTINENTS: step 26 = (40, -104), as in the real world (rwg2dump 2.6).
//!
//! [`estimate_spawn_in`] climbs the exact pre-population column (`surface::surface_chunk` on [`carved_chunk`]):
//! RWG2 mountain-chain river tunnels and junction chambers, the painted surface, the Hodgepodge cave dig replayed
//! step by step (`digBlock`), the lava-cave landmark and polar snow layers ([`chunk_spawnable_exact`]).
//! ✅ 9/9 dumped worlds; BOP climb block name + y of all 520 dumped walk steps exact (`tests/carve.rs`).
//! [`chunk_tops`] / [`chunk_tops_carved`] / [`hard_layers`] are the older probabilistic model, kept for the
//! surface-class users (Thaumcraft, Roguelike, rings).

use crate::chunkgen::Chunk;
use crate::dungeons::Terrain;
use crate::java_random::JavaRandom;
use crate::jmath::FxMap;
use crate::noise::NoiseGenerator as _;
use crate::provider::{BiomeProvider, BiomeTable, Heights};

/// Vanilla attempt cap.
pub const MAX_ATTEMPTS: usize = 1000;
/// Share of land columns whose top is carved by caves / landmarks, leaving a stone top lower down
/// (dump22 world chunks: 71 of ~31.4k modelled grass columns, nearly all stone).
const CAVE_STONE: f32 = 0.002;
/// A step is reported as `confident` if it holds at least this share of the stopping probability.
const CONFIDENT: f32 = 0.8;

/// Walk positions p0 = (0, 0), p1, ..., p1000 (vanilla order: x draws before z, minuend before subtrahend).
pub fn walk_positions(seed: i64) -> Vec<(i32, i32)> {
    let mut r = JavaRandom::new(seed);
    let (mut x, mut z) = (0i32, 0i32);
    let mut out = Vec::with_capacity(MAX_ATTEMPTS + 1);
    out.push((x, z));
    for _ in 0..MAX_ATTEMPTS {
        let a = r.next_int_bounded(64);
        let b = r.next_int_bounded(64);
        x += a - b;
        let a = r.next_int_bounded(64);
        let b = r.next_int_bounded(64);
        z += a - b;
        out.push((x, z));
    }
    out
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct SpawnCandidate {
    pub x: i32,
    pub z: i32,
    /// Index into the walk (0 = origin); 1000 = cap reached.
    pub attempt: i32,
    /// Modelled probability that the walk stops here.
    pub prob: f32,
}

#[derive(Clone, Debug)]
pub struct SpawnEstimate {
    pub x: i32,
    pub z: i32,
    pub attempts: i32,
    /// `prob >= 0.8`.
    pub confident: bool,
    /// Probability of this stopping step under the model.
    pub prob: f32,
    /// Steps with prob >= 2% in walk order (includes the estimate), max 8.
    pub candidates: Vec<SpawnCandidate>,
    /// Steps evaluated (chunk loads modelled).
    pub evaluated: usize,
}

// ---------------------------------------------------------------- surface model

/// Top block a surface writes where it paints "topBlock".
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Top {
    Grass,
    Sand,
    /// dirt, BOP grass / mud / hard sand / hard dirt, hardened clay, snow, ice, mycelium, ...
    Other,
}

/// Top-block probabilities of one column (the rest: anything else).
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct TopProbs {
    pub grass: f32,
    pub stone: f32,
    pub sand: f32,
}

impl TopProbs {
    const NONE: TopProbs = TopProbs {
        grass: 0.0,
        stone: 0.0,
        sand: 0.0,
    };
    const STONE: TopProbs = TopProbs {
        grass: 0.0,
        stone: 1.0,
        sand: 0.0,
    };

    fn top(t: Top) -> TopProbs {
        match t {
            Top::Grass => TopProbs {
                grass: 1.0,
                ..Self::NONE
            },
            Top::Sand => TopProbs {
                sand: 1.0,
                ..Self::NONE
            },
            Top::Other => Self::NONE,
        }
    }

    /// `WorldProviderSurfaceBOP.canCoordinateBeSpawn`: sand or stone.
    pub fn spawnable(&self) -> f32 {
        self.stone + self.sand
    }
}

/// Top-relevant part of one RWG2 surface (`rwg2.surface.*` / inline `rReplace`), at the top stone y = k.
#[derive(Clone, Copy, Debug)]
pub enum Surf {
    /// Fixed outcome (ocean, polar snow, coast ice, volcano ash, bare stone, ...).
    Fixed(TopProbs),
    /// `SurfaceGrassland` / `SurfaceGrasslandMix1`: cliff `c > 1.4` → stone/cobble; Mix1: mix block where
    /// `noise2(x/w, z/w) > h`; else top.
    Grassland {
        top: Top,
        mix: Option<(f32, f32, Top)>,
    },
    /// `SurfaceGrasslandMixBig`: cliff 1.4; mix where `noise2(x/w, z/w) + noise2(x/sw, z/sw) * ss > h`.
    MixBig {
        top: Top,
        mix_top: Top,
        w: f32,
        h: f32,
        sw: f32,
        ss: f32,
    },
    /// `SurfaceMountainStone` / `Snow` / `StoneMix1` / `IslandMountainStone` / `DesertMountain`:
    /// cliff 1 → stone/cobble (1 draw), cliff 2 → clay, snow, Mix1 stone mix, island sand beach below `beach`.
    Mountain {
        top: Top,
        min: f32,
        s_cliff: f32,
        s_height: f32,
        s_strength: f32,
        c_cliff: f32,
        /// `SurfaceMountainSnow`: (iCliff, iHeight, iStrength); also forces `Blocks.grass` as top.
        snow: Option<(f32, f32, f32)>,
        /// `SurfaceMountainStoneMix1`: stone where `noise2(x/12, z/12) > mixHeight`.
        mix: Option<f32>,
        /// Island beach height (sand below it); 63 for the others (irrelevant above 62).
        beach: i32,
    },
    Tundra,
    /// `SurfacePolar`: snow; river paint gives dirt or a `nextInt(2)` stone/cobblestone top.
    Polar,
    /// `SurfaceDuneValley(valley, mix, dirt)`.
    DuneValley {
        top: Top,
        valley: f32,
        mix: bool,
        dirt: bool,
    },
    /// `SurfaceDesertOasis`: cliff 1.3 (stone/cobble if `stone_cliff`, else clay); grass where
    /// `noise2(x/12, z/12) > -0.3 + (k - 61) / 15`, else sand.
    DesertOasis {
        stone_cliff: bool,
    },
    /// `SurfaceDesert` (cliff → stone/cobble) / `SurfaceRedDesert` (cliff → clay): sand top.
    Desert {
        cliff: f32,
        stone_cliff: bool,
    },
    /// `SurfaceGrassCanyon`: clay cliffs (1.3), grass top.
    GrassCanyon,
    /// `SurfaceCanyon(grassRaise)`: clay cliffs (1.3), grass band, random grass/dirt/top band, then top (4/5).
    Canyon {
        raise: i32,
        top: Top,
    },
    /// `SurfaceMesa`: clay cliffs (1.3), dirt below 69, random dirt/top up to 77, then top (4/5).
    Mesa {
        top: Top,
    },
    /// Inline coast `rReplace`: cliff → stone/cobble; `k >= grass_k` grass, else `low`.
    Coast {
        cliff: f32,
        grass_k: i32,
        low: Top,
    },
}

#[derive(Clone, Copy, Debug)]
pub struct SurfaceInfo {
    pub surf: Surf,
    /// Followed by `SurfaceRiverOasis` (grass near rivers, any top).
    pub river_oasis: bool,
    /// Probability that a sand/stone top is still the top after population (snow layers, cacti, dead bushes,
    /// boulders, GT ores replacing surface stone). Heuristic.
    pub kept_after_population: f32,
    /// Surface assignment is a guess (unknown biome).
    pub guessed: bool,
}

const fn mountain(top: Top, min: f32) -> Surf {
    Surf::Mountain {
        top,
        min,
        s_cliff: 1.5,
        s_height: 60.0,
        s_strength: 65.0,
        c_cliff: 1.5,
        snow: None,
        mix: None,
        beach: 63,
    }
}

const fn mountain_snow(
    min: f32,
    s_height: f32,
    s_strength: f32,
    i_cliff: f32,
    i_height: f32,
) -> Surf {
    Surf::Mountain {
        top: Top::Grass,
        min,
        s_cliff: 1.5,
        s_height,
        s_strength,
        c_cliff: 1.5,
        snow: Some((i_cliff, i_height, 50.0)),
        mix: None,
        beach: 63,
    }
}

const fn mix1(top: Top, mix: f32) -> Surf {
    Surf::Mountain {
        top,
        min: 0.0,
        s_cliff: 1.5,
        s_height: 60.0,
        s_strength: 65.0,
        c_cliff: 1.5,
        snow: None,
        mix: Some(mix),
        beach: 63,
    }
}

const fn grassland(top: Top) -> Surf {
    Surf::Grassland { top, mix: None }
}

const GRASS: Surf = grassland(Top::Grass);
const NEVER: Surf = Surf::Fixed(TopProbs::NONE);

/// Native RWG2 biomes, by class (RWG2 @ pinned commit, constructors in `biomes/realistic/**`).
fn native_surface(class: &str) -> Option<(Surf, bool)> {
    use Top::*;
    let snow_default = mountain_snow(0.2, 60.0, 65.0, 0.3, 100.0);
    Some(match class {
        "RealisticBiomeTest"
        | "RealisticBiomeSnowHills"
        | "RealisticBiomeSnowRivers"
        | "RealisticBiomeSnowLakes"
        | "RealisticBiomeRedwoodSnow"
        | "RealisticBiomeTaigaHills"
        | "RealisticBiomeWoodMountains" => (snow_default, false),
        "RealisticBiomeTestRiver" => (mix1(Grass, 0.08), false),
        "RealisticBiomeTundraHills"
        | "RealisticBiomeTundraPlains"
        | "RealisticBiomeTaigaPlains" => (Surf::Tundra, false),
        "RealisticBiomeRedwood" => (mountain(Grass, 0.2), false),
        "RealisticBiomeDarkRedwood" => (mix1(Other, 0.15), false), // dirt top, stone mix
        "RealisticBiomeDarkRedwoodPlains" => (mix1(Grass, 0.15), false),
        "RealisticBiomeWoodHills" => (mountain(Grass, 0.0), false),
        // SurfaceDuneValley(top, fill, valley, d = dirt, m = mix): (220f, false, true)
        "RealisticBiomeDuneValleyForest" => (
            Surf::DuneValley {
                top: Grass,
                valley: 220.0,
                mix: true,
                dirt: false,
            },
            false,
        ),
        "RealisticBiomeSavanna" => (
            Surf::Grassland {
                top: Grass,
                mix: Some((13.0, 0.27, Sand)),
            },
            false,
        ),
        "RealisticBiomeSavannaForest" => (mountain(Grass, 0.6), false),
        "RealisticBiomeSavannaDunes" => (
            Surf::DuneValley {
                top: Grass,
                valley: 300.0,
                mix: true,
                dirt: true,
            },
            true,
        ),
        "RealisticBiomeStoneMountains" => (mix1(Grass, 0.08), true),
        "RealisticBiomeStoneMountainsCactus" => (mix1(Grass, 0.20), true),
        "RealisticBiomeHotForest" | "RealisticBiomeHotRedwood" => (
            Surf::MixBig {
                top: Sand,
                mix_top: Grass,
                w: 60.0,
                h: -0.14,
                sw: 14.0,
                ss: 0.25,
            },
            false,
        ),
        "RealisticBiomeCanyonForest" => (
            Surf::Canyon {
                raise: 47,
                top: Sand,
            },
            false,
        ),
        "RealisticBiomeMesaPlains" => (
            Surf::Canyon {
                raise: 20,
                top: Sand,
            },
            false,
        ),
        "RealisticBiomeCanyon" => (
            Surf::Canyon {
                raise: 0,
                top: Sand,
            },
            false,
        ),
        "RealisticBiomeDesert" | "RealisticBiomeDesertMountains" => (mountain(Sand, 0.0), true),
        "RealisticBiomeDuneValley" => (
            Surf::Desert {
                cliff: 2.8,
                stone_cliff: true,
            },
            true,
        ),
        "RealisticBiomeRedDesertMountains" => (
            Surf::Desert {
                cliff: 1.4,
                stone_cliff: false,
            },
            true,
        ),
        "RealisticBiomeMesa" => (Surf::Mesa { top: Sand }, true),
        "RealisticBiomeOasis" => (Surf::DesertOasis { stone_cliff: true }, true),
        "RealisticBiomeRedOasis" => (Surf::DesertOasis { stone_cliff: false }, true),
        "RealisticBiomeHighRainforest" | "RealisticBiomeRedwoodJungle" => (GRASS, false),
        "RealisticBiomeJungleHills" => (mountain(Grass, 1.0), false),
        "RealisticBiomeJungleCanyon" => (Surf::GrassCanyon, false),
        "RealisticBiomeCoastTest" | "RealisticBiomeCoastOasis" => (
            Surf::Coast {
                cliff: 1.4,
                grass_k: 65,
                low: Sand,
            },
            false,
        ),
        "RealisticBiomeCoastColdSlope" => (
            Surf::Coast {
                cliff: 0.5,
                grass_k: 65,
                low: Other,
            },
            false,
        ),
        "RealisticBiomeCoastMangrove" => (
            Surf::Coast {
                cliff: 1.3,
                grass_k: 68,
                low: Sand,
            },
            false,
        ),
        "RealisticBiomeCoastDunes" => (
            Surf::Coast {
                cliff: 1.3,
                grass_k: 69,
                low: Sand,
            },
            false,
        ),
        "RealisticBiomeIslandTropical" | "RealisticBiomeIslandTundra" => (
            Surf::Mountain {
                top: Grass,
                min: 0.0,
                s_cliff: 1.5,
                s_height: 60.0,
                s_strength: 65.0,
                c_cliff: 1.5,
                snow: None,
                mix: None,
                beach: 67,
            },
            false,
        ),
        "RealisticBiomeCoastColdCliff" => (Surf::Fixed(TopProbs::STONE), false), // empty rReplace: bare stone
        "RealisticBiomeOceanTest" => (Surf::Fixed(TopProbs::top(Sand)), false),
        // shallow sand / deep gravel (variant not in the table); tops above 62 are rare
        "RealisticBiomeOcean" | "RealisticBiomeBOPOcean" => (
            Surf::Fixed(TopProbs {
                sand: 0.5,
                ..TopProbs::NONE
            }),
            false,
        ),
        "RealisticBiomePolar" => (Surf::Polar, false),
        // volcano: ash (the underlying biome's surface near the rim is not modelled)
        "RealisticBiomeIslandVolcano" | "RealisticBiomeCoastIce" => (NEVER, false),
        "RealisticBiomeFungiCanyonIsland" => (mountain(Grass, 0.95), false),
        // backing biome tops are all grass in GTNH 2.9.0 (Boreal, Tundra, Jade Cliffs, rainforests, Chaparral)
        "RealisticBiomeMountainChain" => (mix1(Grass, 0.20), false),
        _ => return None,
    })
}

/// `RealisticBiomeSupport` entries by base-biome name and occurrence index (SupportBOP / Support / SupportTC
/// registration order). Tops from BOP 2.1 (1.7.10): default `Blocks.grass` unless set; `newBopGrass`,
/// `longGrass`, mud, hard sand/dirt, hardened clay are `Other`; Mangrove is sand.
fn support_surface(name: &str, occurrence: usize) -> Option<(Surf, bool)> {
    use Top::*;
    let boreal = mountain_snow(0.45, 60.0, 65.0, 0.4, 130.0);
    Some(match (name, occurrence) {
        ("Bamboo Forest", _) => (mountain(Other, 0.95), false), // dirt top
        (
            "Bayou"
            | "Bog"
            | "Dead Forest"
            | "Deciduous Forest"
            | "Fen"
            | "Flower Field"
            | "Frost Forest"
            | "Grove"
            | "Land of Lakes Marsh"
            | "Lush Swamp"
            | "Maple Woods"
            | "Prairie"
            | "Sacred Springs"
            | "Seasonal Forest"
            | "Shield"
            | "Shrubland"
            | "Tropical Rainforest"
            | "Tropics"
            | "Oasis"
            | "Tundra"
            | "Woodland"
            | "Sunflower Plains"
            | "Magical Forest"
            | "Tainted Land",
            _,
        ) => (GRASS, false),
        (
            "Dead Swamp" | "Ominous Woods" | "Sludgepit" | "Wetland" | "Lush Desert" | "Quagmire"
            | "MushroomIsland" | "Ice Plains Spikes",
            _,
        ) => (grassland(Other), false),
        ("Mangrove", _) => (grassland(Sand), false),
        ("Boreal Forest", _) => (boreal, false),
        ("Brushland", 0) => (
            Surf::Grassland {
                top: Grass,
                mix: Some((13.0, 0.27, Sand)),
            },
            false,
        ),
        // SupportBOP: SurfaceDuneValley(top, fill, 300f, d = false, m = true)
        ("Brushland", _) | ("Heathland", _) => (
            Surf::DuneValley {
                top: Grass,
                valley: 300.0,
                mix: true,
                dirt: false,
            },
            true,
        ),
        ("Canyon", _) => (
            Surf::Canyon {
                raise: 0,
                top: Other,
            },
            false,
        ),
        ("Chaparral", _) => (
            Surf::Grassland {
                top: Grass,
                mix: Some((26.0, 0.35, Sand)),
            },
            false,
        ),
        ("Cherry Blossom Grove", _) => (mountain(Grass, 0.45), false),
        ("Coniferous Forest", _) => (mountain_snow(0.45, 60.0, 65.0, 0.3, 100.0), false),
        ("Snowy Coniferous Forest", _) => (mountain_snow(0.45, 50.0, 60.0, 0.4, 100.0), false),
        ("Eucalyptus Forest" | "Fungi Forest" | "Jade Cliffs", _) => (mountain(Grass, 0.95), false),
        ("Garden", 0) => (boreal, false),
        ("Garden", _) => (grassland(Other), false), // BOP longGrass top
        ("Highland", _) => (
            Surf::Mountain {
                top: Grass,
                min: 1.0,
                s_cliff: 1.5,
                s_height: 85.0,
                s_strength: 20.0,
                c_cliff: 4.0,
                snow: None,
                mix: None,
                beach: 63,
            },
            false,
        ),
        ("Lavender Fields" | "Meadow", _) => (mountain(Grass, 1.2), false),
        ("Mountain", _) => (mountain(Grass, 0.75), false),
        ("Outback", _) => (
            Surf::DuneValley {
                top: Other,
                valley: 300.0,
                mix: false,
                dirt: false,
            },
            true,
        ),
        ("Rainforest", _) => (mountain(Grass, 1.3), false),
        ("Redwood Forest", _) => (mountain(Grass, 0.4), false),
        ("Temperate Rainforest", _) => (mountain(Grass, 0.45), false),
        _ => return None,
    })
}

fn kept_after_population(temperature: f32) -> f32 {
    if temperature < 0.15 {
        0.05 // snow layers from the freeze pass
    } else {
        0.75
    }
}

/// Per-RWG-id surface table.
#[derive(Clone, Debug)]
pub struct SurfaceModel {
    pub by_id: Vec<SurfaceInfo>,
}

impl SurfaceModel {
    pub fn new(table: &BiomeTable) -> Self {
        let mut seen = std::collections::HashMap::<String, usize>::new();
        let by_id = table
            .biomes
            .iter()
            .map(|b| {
                let class = b.class.rsplit('.').next().unwrap_or("");
                let found = if class == "RealisticBiomeSupport" {
                    let occ = seen.entry(b.name.clone()).or_insert(0);
                    *occ += 1;
                    support_surface(&b.name, *occ - 1)
                } else {
                    native_surface(class)
                };
                let ((surf, river_oasis), guessed) = match found {
                    Some(v) => (v, false),
                    None => ((GRASS, false), true), // unknown biome: plain grassland
                };
                SurfaceInfo {
                    surf,
                    river_oasis,
                    kept_after_population: kept_after_population(b.temperature),
                    guessed,
                }
            })
            .collect();
        SurfaceModel { by_id }
    }
}

/// `CliffCalculator.calc`: max |dh| to the in-chunk 4-neighbours.
fn cliff(c: &Chunk, lx: usize, lz: usize) -> f32 {
    let h = |x: usize, z: usize| c.height[z * 16 + x];
    let v = h(lx, lz);
    let mut m = 0f32;
    if lz > 0 {
        m = m.max((v - h(lx, lz - 1)).abs());
    }
    if lx > 0 {
        m = m.max((v - h(lx - 1, lz)).abs());
    }
    if lz < 15 {
        m = m.max((v - h(lx, lz + 1)).abs());
    }
    if lx < 15 {
        m = m.max((v - h(lx + 1, lz)).abs());
    }
    m
}

/// What a column's surface pass decides, and which chunk-Random draws it consumes.
#[derive(Clone, Copy, Debug)]
enum Spec {
    /// No draws.
    Fixed(TopProbs),
    /// Cliff top `rand.nextInt(3) == 0 ? cobblestone : stone`; `draws` = 1 (depth 0) or 2 (depth 0 and 1).
    Cliff { draws: u8 },
    /// `SurfaceCanyon` non-cliff column with top stone y = k.
    Canyon { k: i32, raise: i32, top: Top },
    /// `SurfaceMesa` non-cliff column.
    Mesa { k: i32, top: Top },
    /// `SurfacePolar` river paint without grass: `nextInt(2) == 0 ? stone : cobblestone`.
    Polar,
    /// No draws; a non-grass, non-stone, non-sand top that is solid ground (dirt, clay, snow block, ice, ash).
    Ground,
}

/// Surface outcome of one column (any height: draws happen under water too).
fn column_spec(p: &BiomeProvider, info: &SurfaceInfo, k: i32, cl: f32, x: i32, z: i32) -> Spec {
    let per = p.perlin();
    let (fx, fz, kf) = (x as f32, z as f32, k as f32);
    let fixed = |t: Top| Spec::Fixed(TopProbs::top(t));
    // dirt / clay / snow block / ice tops: validGround for Roguelike, not spawnable
    const NONE: Spec = Spec::Ground;
    const CLIFF2: Spec = Spec::Cliff { draws: 2 };
    match info.surf {
        Surf::Fixed(t) if t == TopProbs::NONE => Spec::Ground,
        Surf::Fixed(t) => Spec::Fixed(t),
        Surf::Polar => {
            let river = -p.river_strength(x, z);
            if river > 0.05 && river + per.noise2(fx / 10.0, fz / 10.0) * 0.1 > 0.86 {
                if per.noise2(fx / 12.0, fz / 12.0) > 0.25 {
                    NONE // dirt
                } else {
                    Spec::Polar
                }
            } else {
                NONE // snow
            }
        }
        Surf::Grassland { top, mix } => {
            if cl > 1.4 {
                CLIFF2
            } else {
                match mix {
                    Some((w, h, mt)) if per.noise2(fx / w, fz / w) > h => fixed(mt),
                    _ => fixed(top),
                }
            }
        }
        Surf::MixBig {
            top,
            mix_top,
            w,
            h,
            sw,
            ss,
        } => {
            if cl > 1.4 {
                CLIFF2
            } else if per.noise2(fx / w, fz / w) + per.noise2(fx / sw, fz / sw) * ss > h {
                fixed(mix_top)
            } else {
                fixed(top)
            }
        }
        Surf::Mountain {
            top,
            min,
            s_cliff,
            s_height,
            s_strength,
            c_cliff,
            snow,
            mix,
            beach,
        } => {
            let pn = per.noise3(fx / 8.0, fz / 8.0, kf / 8.0) * 0.5;
            let mut cliff = 0;
            if cl > min && cl > s_cliff - ((kf - s_height) / s_strength) + pn {
                cliff = 1;
            }
            if cl > c_cliff {
                cliff = 2;
            }
            if let Some((ic, ih, is)) = snow {
                if kf > 110.0 + pn * 4.0 && cl < ic + ((kf - ih) / is) + pn {
                    cliff = 3;
                }
            }
            match cliff {
                1 => Spec::Cliff { draws: 1 },
                2 | 3 => NONE, // clay, snow
                _ if k < beach => fixed(if beach > 63 { Top::Sand } else { Top::Other }),
                _ if snow.is_some() => fixed(Top::Grass), // MountainSnow writes Blocks.grass above 62
                _ if mix.is_some_and(|mh| per.noise2(fx / 12.0, fz / 12.0) > mh) => {
                    Spec::Fixed(TopProbs::STONE)
                }
                _ => fixed(top),
            }
        }
        Surf::Tundra => {
            let pn = per.noise2(fx / 8.0, fz / 8.0) * 0.5;
            let mut cliff = 0;
            if cl > 0.45 && cl > 1.5 - ((kf - 60.0) / 65.0) + pn {
                cliff = 1;
            }
            if cl > 1.5 {
                cliff = 2;
            }
            if kf > 110.0 + pn * 4.0 && cl < 0.3 + ((kf - 100.0) / 50.0) + pn {
                cliff = 3;
            }
            match cliff {
                1 => Spec::Cliff { draws: 1 },
                2 | 3 => NONE,
                _ if per.noise2(fx / 50.0, fz / 50.0) + pn * 0.6 > 0.24 => NONE, // dirt
                _ => fixed(Top::Grass),
            }
        }
        Surf::DuneValley {
            top,
            valley,
            mix,
            dirt,
        } => {
            let h = ((per.noise2(fx / valley, fz / valley) + 0.25) * 65.0).max(1.0);
            let mm = per.noise2(fx / 12.0, fz / 12.0);
            if kf > 90.0 + per.noise2(fx / 24.0, fz / 24.0) * 10.0 - h || (mm < -0.28 && mix) {
                fixed(Top::Sand)
            } else if (dirt && mm < 0.22) || k < 62 {
                NONE
            } else {
                fixed(top)
            }
        }
        Surf::DesertOasis { stone_cliff } => {
            if cl > 1.3 {
                if stone_cliff {
                    CLIFF2
                } else {
                    NONE
                }
            } else if per.noise2(fx / 12.0, fz / 12.0) > -0.3 + ((kf - 61.0) / 15.0) {
                fixed(Top::Grass)
            } else {
                fixed(Top::Sand)
            }
        }
        Surf::Desert { cliff, stone_cliff } => {
            if cl > cliff {
                if stone_cliff {
                    CLIFF2
                } else {
                    NONE
                }
            } else {
                fixed(Top::Sand)
            }
        }
        Surf::GrassCanyon => {
            if cl > 1.3 {
                NONE
            } else {
                fixed(Top::Grass)
            }
        }
        Surf::Canyon { raise, top } => {
            if cl > 1.3 {
                NONE
            } else {
                Spec::Canyon { k, raise, top }
            }
        }
        Surf::Mesa { top } => {
            if cl > 1.3 {
                NONE
            } else {
                Spec::Mesa { k, top }
            }
        }
        Surf::Coast {
            cliff,
            grass_k,
            low,
        } => {
            if cl > cliff {
                CLIFF2
            } else if k >= grass_k {
                fixed(Top::Grass)
            } else {
                fixed(low)
            }
        }
    }
}

/// Replays the chunk Random through one column's surface draws; returns the top.
fn replay(spec: Spec, r: &mut JavaRandom) -> TopProbs {
    let top = |t: Top| TopProbs::top(t);
    match spec {
        Spec::Fixed(t) => t,
        Spec::Ground => TopProbs::NONE,
        Spec::Cliff { draws } => {
            let first = r.next_int_bounded(3);
            for _ in 1..draws {
                r.next_int_bounded(3);
            }
            if first == 0 {
                TopProbs::NONE
            } else {
                TopProbs::STONE
            }
        }
        Spec::Polar => {
            if r.next_int_bounded(2) == 0 {
                TopProbs::STONE
            } else {
                TopProbs::NONE
            }
        }
        Spec::Canyon { k, raise, top: t } => {
            let mut out = TopProbs::NONE;
            for d in 0..5 {
                let y = k - d;
                if y > 74 + raise {
                    let v = r.next_int_bounded(5);
                    if d == 0 && v != 0 {
                        out = top(t);
                    }
                } else if y < 62 {
                } else if y < 62 + raise {
                    if d == 0 {
                        out = top(Top::Grass);
                    }
                } else if y < 75 + raise && d == 0 {
                    let rr = ((y - (62 + raise)) as f32 / 2.0) as i32;
                    if r.next_int_bounded(rr + 1) == 0 {
                        out = top(Top::Grass);
                    } else if r.next_int_bounded((rr as f32 / 2.0) as i32 + 1) != 0 {
                        out = top(t);
                    }
                }
            }
            out
        }
        Spec::Mesa { k, top: t } => {
            let mut out = TopProbs::NONE;
            for d in 0..5 {
                let y = k - d;
                if y > 77 {
                    let v = r.next_int_bounded(5);
                    if d == 0 && v != 0 {
                        out = top(t);
                    }
                } else if (69..78).contains(&y) && d == 0 {
                    if y < 72 && r.next_int_bounded(y - 69 + 1) == 0 {
                        continue; // dirt
                    }
                    if r.next_int_bounded(5) != 0 {
                        out = top(t);
                    }
                }
            }
            out
        }
    }
}

/// Top-block probabilities of every column of chunk (cx, cz) right after `provideChunk` (pre-population), with
/// the BOP climb from y 63. Replays the chunk Random (`setSeed(cx * 0x4f9939f508 + cz * 0x1ef1565bd5)`) through
/// `replaceBlocksForBiome`: columns in (z, x) order, surface draws, then 4 bedrock draws. Index `lz * 16 + lx`.
pub fn chunk_tops(
    p: &BiomeProvider,
    m: &SurfaceModel,
    c: &Chunk,
    cx: i32,
    cz: i32,
) -> [TopProbs; 256] {
    chunk_tops_ground(p, m, c, cx, cz).0
}

/// [`chunk_tops`] plus, per column, P(the top is a Roguelike `validGroundBlock`: stone, cobblestone, sand,
/// dirt, clay, snow block, ice). Grass and BOP grass-like tops (`Top::Other`, counted 0.3) are not.
pub fn chunk_tops_ground(
    p: &BiomeProvider,
    m: &SurfaceModel,
    c: &Chunk,
    cx: i32,
    cz: i32,
) -> ([TopProbs; 256], [f32; 256]) {
    let mut r = JavaRandom::new(
        (cx as i64)
            .wrapping_mul(0x4f9939f508)
            .wrapping_add((cz as i64).wrapping_mul(0x1ef1565bd5)),
    );
    let mut out = [TopProbs::NONE; 256];
    let mut ground = [0f32; 256];
    for lz in 0..16usize {
        for lx in 0..16usize {
            let idx = lz * 16 + lx;
            let (x, z) = (cx * 16 + lx as i32, cz * 16 + lz as i32);
            let k = c.height[idx] as i32;
            let info = m
                .by_id
                .get(c.rwg[idx] as usize)
                .copied()
                .unwrap_or(SurfaceInfo {
                    surf: GRASS,
                    river_oasis: false,
                    kept_after_population: 0.75,
                    guessed: true,
                });
            let spec = column_spec(p, &info, k, cliff(c, lx, lz), x, z);
            let mut t = replay(spec, &mut r);
            let mut g = match spec {
                Spec::Ground | Spec::Cliff { .. } | Spec::Polar => 1.0,
                Spec::Canyon { .. } | Spec::Mesa { .. } => 1.0 - t.grass,
                Spec::Fixed(f) if f == TopProbs::NONE => 0.3,
                Spec::Fixed(f) => f.stone + f.sand,
            };
            for b in 2..=5 {
                r.next_int_bounded(b); // bedrock
            }
            if k < 63 {
                continue; // the climb stops on the air at y 63
            }
            if info.river_oasis {
                let river = -p.river_strength(x, z);
                if river > 0.05
                    && river + p.perlin().noise2(x as f32 / 10.0, z as f32 / 10.0) * 0.15 > 0.8
                {
                    t = TopProbs::top(Top::Grass);
                    g = 0.0;
                }
            }
            if info.guessed {
                t = scale(t, 0.7);
            }
            // caves / landmark carving: the climb then ends on stone lower down
            let mut o = scale(t, 1.0 - CAVE_STONE);
            o.stone += CAVE_STONE;
            out[idx] = o;
            ground[idx] = g * (1.0 - CAVE_STONE) + CAVE_STONE;
        }
    }
    (out, ground)
}

/// Painted depth range `[from, to)` of a column's surface that vanilla caves cannot cut. `MapGenCaves.digBlock`
/// only replaces stone and the top/filler block of `world.getBiomeGenForCoords` (the provider biome during
/// generation); RWG2 paints its own blocks. Rules (fitted per painted class on 9 dumped worlds, 2.2k columns,
/// see `tests/carve.rs::fit_carvable_top`): grass/dirt and stone layers dig; sand only under a sand-topped
/// (desert) provider biome; other tops only where the painting biome is the provider biome and paints its own
/// top (no mix block); polar snow (9 deep), canyon / mesa hardened clay and desert sandstone never dig.
pub fn hard_layers(
    p: &BiomeProvider,
    h: &dyn Heights,
    m: &SurfaceModel,
    c: &Chunk,
    cx: i32,
    cz: i32,
) -> [(i32, i32); 256] {
    let tops = chunk_tops(p, m, c, cx, cz);
    let mut out = [(0, 0); 256];
    for (idx, o) in out.iter_mut().enumerate() {
        let Some(info) = m.by_id.get(c.rwg[idx] as usize) else {
            continue;
        };
        *o = match info.surf {
            Surf::Polar => (0, 9),
            // clay: whole run on cliffs (> 1.3), below depth 4 otherwise; and every depth >= 12 above y 63
            Surf::Canyon { .. } | Surf::GrassCanyon | Surf::Mesa { .. } => {
                if cliff(c, idx & 15, idx >> 4) > 1.3 {
                    (0, crate::column::HARD_CLAY_ABOVE_63)
                } else {
                    (5, crate::column::HARD_CLAY_ABOVE_63)
                }
            }
            _ => {
                let t = tops[idx];
                let (x, z) = (cx * 16 + (idx & 15) as i32, cz * 16 + (idx >> 4) as i32);
                // the provider biome (a full `getBiomeGenAt`) only for the sand / mixed branches
                let prov = || p.biome_gen_id(x, z, h);
                let sand_desert = matches!(
                    info.surf,
                    Surf::Desert { .. } | Surf::DesertOasis { .. } | Surf::DuneValley { .. }
                );
                if t.grass >= 0.5 || t.stone >= 0.5 {
                    if sand_desert {
                        (4, 6)
                    } else {
                        (0, 0)
                    }
                } else if t.sand >= 0.5 {
                    let prov = prov();
                    let pn = p
                        .table
                        .biomes
                        .iter()
                        .find(|b| b.gen_id == prov)
                        .map(|b| b.name.as_str())
                        .unwrap_or("");
                    if pn.contains("Desert") && !pn.contains("Snow") {
                        (4, 6)
                    } else {
                        (0, 4)
                    }
                } else {
                    let mixed = matches!(
                        info.surf,
                        Surf::Grassland { mix: Some(_), .. } | Surf::MixBig { .. }
                    );
                    if !mixed && p.table.info(c.rwg[idx]).gen_id == prov() {
                        (0, 0)
                    } else {
                        (0, 4)
                    }
                }
            }
        };
    }
    out
}

/// Columns where `SurfacePolar` puts a snow layer on its top (no river paint).
pub fn snow_layers(
    p: &BiomeProvider,
    m: &SurfaceModel,
    c: &Chunk,
    cx: i32,
    cz: i32,
) -> [bool; 256] {
    let mut out = [false; 256];
    for (idx, o) in out.iter_mut().enumerate() {
        if matches!(
            m.by_id.get(c.rwg[idx] as usize).map(|i| i.surf),
            Some(Surf::Polar)
        ) {
            let (x, z) = (cx * 16 + (idx & 15) as i32, cz * 16 + (idx >> 4) as i32);
            let river = -p.river_strength(x, z);
            let river_paint = river > 0.05
                && river + p.perlin().noise2(x as f32 / 10.0, z as f32 / 10.0) * 0.1 > 0.86;
            *o = !river_paint;
        }
    }
    out
}

/// [`crate::column::CarvedChunk`] with the surface-dependent parts ([`hard_layers`], [`snow_layers`]).
pub fn carved_chunk(
    p: &BiomeProvider,
    h: &dyn Heights,
    m: &SurfaceModel,
    c: &Chunk,
    cx: i32,
    cz: i32,
    caves: Option<&crate::gtorder::Caves>,
) -> crate::column::CarvedChunk {
    let mut cc = crate::column::CarvedChunk::with_hard(
        p,
        h,
        c,
        cx,
        cz,
        caves,
        hard_layers(p, h, m, c, cx, cz),
    );
    cc.set_snow(&snow_layers(p, m, c, cx, cz));
    cc
}

/// [`chunk_tops`] on the carved pre-population column ([`crate::column::CarvedChunk`]: mountain-chain river
/// tunnels before the surface pass, Hodgepodge caves after it, lava-cave landmark last), with the exact BOP climb
/// (first air above y 63). The surface pass is replayed run by run: every stone block below an air gap is painted
/// as a new top and draws its own randoms. A climb that ends inside a run below its top sees filler (same class
/// as the top, grass → dirt) for depth 1..3 and stone below that.
pub fn chunk_tops_carved(
    p: &BiomeProvider,
    m: &SurfaceModel,
    c: &Chunk,
    cx: i32,
    cz: i32,
    cc: &crate::column::CarvedChunk,
) -> [TopProbs; 256] {
    let mut r = JavaRandom::new(
        (cx as i64)
            .wrapping_mul(0x4f9939f508)
            .wrapping_add((cz as i64).wrapping_mul(0x1ef1565bd5)),
    );
    let mut out = [TopProbs::NONE; 256];
    for lz in 0..16usize {
        for lx in 0..16usize {
            let idx = lz * 16 + lx;
            let (x, z) = (cx * 16 + lx as i32, cz * 16 + lz as i32);
            let info = m
                .by_id
                .get(c.rwg[idx] as usize)
                .copied()
                .unwrap_or(SurfaceInfo {
                    surf: GRASS,
                    river_oasis: false,
                    kept_after_population: 0.75,
                    guessed: true,
                });
            let cl = cliff(c, lx, lz);
            let runs = cc.runs(lx as i32, lz as i32);
            let mut run_tops: Vec<(i32, TopProbs)> = Vec::with_capacity(runs.len());
            for (n, &k) in runs.iter().enumerate() {
                let spec = column_spec(p, &info, k, cl, x, z);
                let mut t = replay(spec, &mut r);
                if n == 0 && info.river_oasis && k >= 63 {
                    let river = -p.river_strength(x, z);
                    if river > 0.05
                        && river + p.perlin().noise2(x as f32 / 10.0, z as f32 / 10.0) * 0.15 > 0.8
                    {
                        t = TopProbs::top(Top::Grass);
                    }
                }
                if info.guessed {
                    t = scale(t, 0.7);
                }
                run_tops.push((k, t));
            }
            for b in 2..=5 {
                r.next_int_bounded(b); // bedrock
            }
            let (y, air) = cc.bop_climb(lx as i32, lz as i32);
            if air {
                continue;
            }
            // the run holding y: the lowest run top >= y (runs are contiguous stone down from their top)
            // above every run: the polar snow layer (not spawnable)
            let Some(&(k, t)) = run_tops.iter().rev().find(|(k, _)| *k >= y) else {
                continue;
            };
            let d = k - y;
            out[idx] = if d == 0 {
                t
            } else if d <= 3 {
                TopProbs {
                    grass: 0.0,
                    stone: t.stone,
                    sand: t.sand,
                }
            } else {
                TopProbs::STONE
            };
        }
    }
    out
}

/// `WorldProviderSurfaceBOP.canCoordinateBeSpawn` climb on a painted column (`surface::surface_chunk`): from y 63
/// up while the block above is not air. Returns (y, block at y).
pub fn bop_climb_col(col: &[u8; 256]) -> (i32, u8) {
    use crate::surface::blk::AIR;
    let mut y = 63usize;
    while y < 255 && col[y + 1] != AIR {
        y += 1;
    }
    (y as i32, col[y])
}

/// Spawnable tops of a chunk on the exact pre-population column (painted surface, cave dig replay, landmark):
/// 1 where the BOP climb ends on sand (any meta) or stone, else 0.
pub fn chunk_spawnable_exact(sc: &crate::surface::SurfaceChunk) -> [f32; 256] {
    use crate::surface::blk::{RED_SAND, SAND, STONE};
    let mut out = [0f32; 256];
    for (idx, o) in out.iter_mut().enumerate() {
        let (_, b) = bop_climb_col(&sc.cols[idx]);
        *o = matches!(b, SAND | RED_SAND | STONE) as u8 as f32;
    }
    out
}

fn scale(t: TopProbs, f: f32) -> TopProbs {
    TopProbs {
        grass: t.grass * f,
        stone: t.stone * f,
        sand: t.sand * f,
    }
}

/// One column (computes the whole chunk; use [`chunk_tops`] for many).
pub fn top_probs(p: &BiomeProvider, m: &SurfaceModel, c: &Chunk, x: i32, z: i32) -> TopProbs {
    chunk_tops(p, m, c, x >> 4, z >> 4)[((z & 15) * 16 + (x & 15)) as usize]
}

/// P(`WorldProviderSurfaceBOP.canCoordinateBeSpawn`) before population: top is sand or stone.
pub fn spawnable_top(p: &BiomeProvider, m: &SurfaceModel, c: &Chunk, x: i32, z: i32) -> f32 {
    top_probs(p, m, c, x, z).spawnable()
}

/// P(top is `Blocks.grass`) before population (surface-model check; vanilla's predicate, not the pack's).
pub fn likely_grass_top(p: &BiomeProvider, m: &SurfaceModel, c: &Chunk, x: i32, z: i32) -> f32 {
    top_probs(p, m, c, x, z).grass
}

// ---------------------------------------------------------------- walk

/// Which chunks the walk has loaded and which of them got populated (vanilla `Chunk.populateChunk`: chunk D is
/// populated once D, D+x, D+z, D+xz are all loaded). Population cascades are not modelled.
#[derive(Default)]
struct LoadState {
    loaded: std::collections::HashSet<(i32, i32)>,
    populated: std::collections::HashSet<(i32, i32)>,
}

impl LoadState {
    fn load(&mut self, cx: i32, cz: i32) {
        if !self.loaded.insert((cx, cz)) {
            return;
        }
        for (dx, dz) in [(0, 0), (-1, 0), (0, -1), (-1, -1)] {
            let d = (cx + dx, cz + dz);
            if !self.populated.contains(&d)
                && [(0, 0), (1, 0), (0, 1), (1, 1)]
                    .iter()
                    .all(|&(a, b)| self.loaded.contains(&(d.0 + a, d.1 + b)))
            {
                self.populated.insert(d);
            }
        }
    }

    /// Decoration of chunk D lands in [16D + 8, 16D + 24): the column's quadrant decides which D matters.
    fn decorated(&self, x: i32, z: i32) -> bool {
        let dx = if x & 15 >= 8 { x >> 4 } else { (x >> 4) - 1 };
        let dz = if z & 15 >= 8 { z >> 4 } else { (z >> 4) - 1 };
        self.populated.contains(&(dx, dz))
    }
}

/// Estimate on an existing terrain cache.
pub fn estimate_spawn_in(
    t: &mut Terrain,
    m: &SurfaceModel,
    st: &crate::surface::SurfaceTable,
    seed: i64,
) -> SpawnEstimate {
    estimate_spawn_in_model(t, m, st, seed, true)
}

/// [`estimate_spawn_in`]; `exact = false` uses the pre-v1.49 fitted column model ([`chunk_tops_carved`]
/// `spawnable()`), kept for comparisons only.
pub fn estimate_spawn_in_model(
    t: &mut Terrain,
    m: &SurfaceModel,
    st: &crate::surface::SurfaceTable,
    seed: i64,
    exact: bool,
) -> SpawnEstimate {
    let pos = walk_positions(seed);
    let mut ls = LoadState::default();
    let mut alive = 1f32; // P(walk still running)
    let mut steps: Vec<SpawnCandidate> = Vec::new();
    let mut evaluated = 0;
    let mut tops: FxMap<(i32, i32), Option<Box<[f32; 256]>>> = FxMap::default();
    // Hodgepodge fastRandom caves (GTNH default)
    let caves = crate::gtorder::Caves::new(seed, true);
    for (i, &(x, z)) in pos.iter().enumerate() {
        if i == MAX_ATTEMPTS {
            steps.push(SpawnCandidate {
                x,
                z,
                attempt: i as i32,
                prob: alive,
            });
            break;
        }
        evaluated += 1;
        ls.load(x >> 4, z >> 4);
        let p = t.p;
        let (cx, cz) = (x >> 4, z >> 4);
        let h = t.h;
        let v = tops.entry((cx, cz)).or_insert_with(|| {
            t.chunk(cx, cz).map(|c| {
                let cc = carved_chunk(p, h, m, c, cx, cz, Some(&caves));
                if !exact {
                    let tp = chunk_tops_carved(p, m, c, cx, cz, &cc);
                    return Box::new(std::array::from_fn(|i| tp[i].spawnable()));
                }
                let sc = crate::surface::surface_chunk(p, h, st, c, cx, cz, &cc);
                let mut o = chunk_spawnable_exact(&sc);
                for (idx, v) in o.iter_mut().enumerate() {
                    if st.guessed.get(c.rwg[idx] as usize).copied().unwrap_or(true) {
                        *v *= 0.7;
                    }
                }
                Box::new(o)
            })
        });
        let mut g = v
            .as_ref()
            .map_or(0.0, |v| v[((z & 15) * 16 + (x & 15)) as usize]);
        if g > 0.0 && ls.decorated(x, z) {
            let id = t
                .chunk(x >> 4, z >> 4)
                .map(|c| c.rwg[((z & 15) * 16 + (x & 15)) as usize])
                .unwrap_or(0);
            g *= m
                .by_id
                .get(id as usize)
                .map(|s| s.kept_after_population)
                .unwrap_or(0.75);
        }
        if g > 0.0 {
            steps.push(SpawnCandidate {
                x,
                z,
                attempt: i as i32,
                prob: alive * g,
            });
            alive *= 1.0 - g;
        }
        if alive < 0.01 {
            // later steps can only hold < 1% (candidates need 2%)
            break;
        }
    }
    let best = steps
        .iter()
        .copied()
        .fold(None::<SpawnCandidate>, |b, s| {
            if b.map_or(true, |b| s.prob > b.prob) {
                Some(s)
            } else {
                b
            }
        })
        .unwrap_or(SpawnCandidate {
            x: 0,
            z: 0,
            attempt: 0,
            prob: 0.0,
        });
    let mut candidates: Vec<SpawnCandidate> =
        steps.into_iter().filter(|s| s.prob >= 0.02).collect();
    if candidates.len() > 8 {
        candidates.sort_by(|a, b| b.prob.total_cmp(&a.prob));
        candidates.truncate(8);
        candidates.sort_by_key(|s| s.attempt);
    }
    SpawnEstimate {
        x: best.x,
        z: best.z,
        attempts: best.attempt,
        confident: best.prob >= CONFIDENT,
        prob: best.prob,
        candidates,
        evaluated,
    }
}

/// World spawn estimate for `seed` (provider built for that seed and world type).
pub fn estimate_spawn(p: &BiomeProvider, h: &dyn Heights, seed: i64) -> SpawnEstimate {
    let m = SurfaceModel::new(&p.table);
    let st = crate::surface::SurfaceTable::new(&p.table);
    let mut t = Terrain::new(p, h);
    estimate_spawn_in(&mut t, &m, &st, seed)
}
