//! Exact pre-population blocks of an RWG2 overworld chunk's surface: `replaceBlocksForBiome` (every
//! `rwg2.surface.*` class and inline `rReplace` ported block for block, chunk Random replayed in column order),
//! then vanilla caves (geometry from [`CarvedChunk`], `digBlock`'s filler → top fix), the volcano lava fill and
//! magma vent, and `LavaCaveLandmark`. Population (trees, plants, ores, snow cover, lakes) is not included.
//!
//! Block codes are [`blk`] constants; stained hardened clay carries its metadata (`blk::clay(meta)`).

use crate::chunkgen::{Chunk, LavaBlock};
use crate::column::CarvedChunk;
use crate::continental;
use crate::java_random::JavaRandom;
use crate::noise::NoiseGenerator;
use crate::perlin::PerlinNoise;
use crate::provider::{BiomeId, BiomeProvider, BiomeTable, Heights};

/// Block codes.
pub mod blk {
    pub const AIR: u8 = 0;
    pub const STONE: u8 = 1;
    pub const GRASS: u8 = 2;
    pub const DIRT: u8 = 3;
    pub const COARSE_DIRT: u8 = 4;
    pub const PODZOL: u8 = 5;
    pub const COBBLE: u8 = 6;
    pub const SAND: u8 = 7;
    pub const RED_SAND: u8 = 8;
    pub const SANDSTONE: u8 = 9;
    pub const GRAVEL: u8 = 10;
    pub const HARDENED_CLAY: u8 = 11;
    pub const SNOW: u8 = 12;
    pub const SNOW_LAYER: u8 = 13;
    pub const ICE: u8 = 14;
    pub const PACKED_ICE: u8 = 15;
    pub const WATER: u8 = 16;
    pub const LAVA: u8 = 17;
    pub const OBSIDIAN: u8 = 18;
    pub const BEDROCK: u8 = 19;
    pub const MYCELIUM: u8 = 20;
    pub const BOP_LONG_GRASS: u8 = 21;
    pub const BOP_HARD_SAND: u8 = 22;
    pub const BOP_HARD_DIRT: u8 = 23;
    pub const BOP_MUD: u8 = 24;
    pub const BOP_DRIED_DIRT: u8 = 25;
    pub const BOP_CRAG_ROCK: u8 = 26;
    pub const BOP_ORIGIN_GRASS: u8 = 27;
    pub const BOP_HARD_ICE: u8 = 28;
    pub const BOP_ASH: u8 = 29;
    pub const BOP_ASH_STONE: u8 = 30;
    /// Number of plain codes (clay is `CLAY0..CLAY0 + 16`).
    pub const COUNT: u8 = 31;
    pub const CLAY0: u8 = 64;
    pub const fn clay(meta: u8) -> u8 {
        CLAY0 + (meta & 15)
    }
}

/// Registry name (metadata dropped: all stained clay is `minecraft:stained_hardened_clay`, red sand is sand).
pub fn block_name(b: u8) -> &'static str {
    use blk::*;
    match b {
        AIR => "minecraft:air",
        STONE => "minecraft:stone",
        GRASS => "minecraft:grass",
        DIRT | COARSE_DIRT | PODZOL => "minecraft:dirt",
        COBBLE => "minecraft:cobblestone",
        SAND | RED_SAND => "minecraft:sand",
        SANDSTONE => "minecraft:sandstone",
        GRAVEL => "minecraft:gravel",
        HARDENED_CLAY => "minecraft:hardened_clay",
        SNOW => "minecraft:snow",
        SNOW_LAYER => "minecraft:snow_layer",
        ICE => "minecraft:ice",
        PACKED_ICE => "minecraft:packed_ice",
        WATER => "minecraft:water",
        LAVA => "minecraft:lava",
        OBSIDIAN => "minecraft:obsidian",
        BEDROCK => "minecraft:bedrock",
        MYCELIUM => "minecraft:mycelium",
        BOP_LONG_GRASS => "BiomesOPlenty:longGrass",
        BOP_HARD_SAND => "BiomesOPlenty:hardSand",
        BOP_HARD_DIRT => "BiomesOPlenty:hardDirt",
        BOP_MUD => "BiomesOPlenty:mud",
        BOP_DRIED_DIRT => "BiomesOPlenty:driedDirt",
        BOP_CRAG_ROCK => "BiomesOPlenty:cragRock",
        BOP_ORIGIN_GRASS => "BiomesOPlenty:originGrass",
        BOP_HARD_ICE => "BiomesOPlenty:hardIce",
        BOP_ASH => "BiomesOPlenty:ash",
        BOP_ASH_STONE => "BiomesOPlenty:ashStone",
        _ if (blk::CLAY0..blk::CLAY0 + 16).contains(&b) => "minecraft:stained_hardened_clay",
        _ => "?",
    }
}

/// Block metadata as the game stores it (coarse dirt 1, podzol 2, red sand 1, stained clay colour); 0 otherwise.
pub fn block_meta(b: u8) -> u8 {
    use blk::*;
    match b {
        COARSE_DIRT | RED_SAND => 1,
        PODZOL => 2,
        _ if (CLAY0..CLAY0 + 16).contains(&b) => b - CLAY0,
        _ => 0,
    }
}

/// Display name with metadata ("Red sand", "Stained clay (orange)").
pub fn block_label(b: u8) -> String {
    use blk::*;
    const CLAY: [&str; 16] = [
        "white",
        "orange",
        "magenta",
        "light blue",
        "yellow",
        "lime",
        "pink",
        "gray",
        "light gray",
        "cyan",
        "purple",
        "blue",
        "brown",
        "green",
        "red",
        "black",
    ];
    match b {
        COARSE_DIRT => "Coarse dirt".into(),
        PODZOL => "Podzol".into(),
        RED_SAND => "Red sand".into(),
        _ if (CLAY0..CLAY0 + 16).contains(&b) => {
            format!("Stained clay ({})", CLAY[(b - CLAY0) as usize])
        }
        _ => {
            let n = block_name(b);
            let n = n.rsplit(':').next().unwrap_or(n);
            let mut s = String::new();
            for (i, ch) in n.chars().enumerate() {
                if ch.is_ascii_uppercase() {
                    s.push(' ');
                    s.push(ch.to_ascii_lowercase());
                } else if ch == '_' {
                    s.push(' ');
                } else if i == 0 {
                    s.push(ch.to_ascii_uppercase());
                } else {
                    s.push(ch);
                }
            }
            s
        }
    }
}

/// Map colour (0xRRGGBB, untinted texture averages; grass / water are the plains tint).
pub fn block_color(b: u8) -> u32 {
    use blk::*;
    const CLAY: [u32; 16] = [
        0xd1b2a1, 0xa25426, 0x95586c, 0x716c89, 0xba8523, 0x677534, 0xa14e4e, 0x392a23, 0x876b62,
        0x575b5b, 0x764656, 0x4a3c5b, 0x4d3324, 0x4c532a, 0x8f3d2e, 0x251610,
    ];
    match b {
        AIR => 0x000000,
        STONE => 0x7d7d7d,
        GRASS => 0x6a9f40,
        DIRT => 0x866043,
        COARSE_DIRT => 0x77553b,
        PODZOL => 0x5b3f18,
        COBBLE => 0x6e6e6e,
        SAND => 0xdbd3a0,
        RED_SAND => 0xa95821,
        SANDSTONE => 0xd2c48f,
        GRAVEL => 0x857f7e,
        HARDENED_CLAY => 0x965d43,
        SNOW | SNOW_LAYER => 0xf0fbfb,
        ICE => 0x91b6ff,
        PACKED_ICE => 0xa6c3f2,
        WATER => 0x3f76e4,
        LAVA => 0xe0641a,
        OBSIDIAN => 0x1b1726,
        BEDROCK => 0x555555,
        MYCELIUM => 0x6f6265,
        BOP_LONG_GRASS => 0x55902e,
        BOP_HARD_SAND => 0xb3a17a,
        BOP_HARD_DIRT => 0x7a5c48,
        BOP_MUD => 0x4c3a2b,
        BOP_DRIED_DIRT => 0xa58e6c,
        BOP_CRAG_ROCK => 0x6d6c75,
        BOP_ORIGIN_GRASS => 0x8ccc6c,
        BOP_HARD_ICE => 0x8fb0e0,
        BOP_ASH => 0x3e3b3b,
        BOP_ASH_STONE => 0x343132,
        _ if (CLAY0..CLAY0 + 16).contains(&b) => CLAY[(b - CLAY0) as usize],
        _ => 0xff00ff,
    }
}

/// Registry name → code (inverse of [`block_name`]; `None` for blocks the surface pass never writes).
pub fn block_from_name(n: &str) -> Option<u8> {
    (0..blk::COUNT)
        .chain(blk::CLAY0..blk::CLAY0 + 1)
        .find(|&b| block_name(b) == n)
}

/// `topBlock` / `fillerBlock` of a base biome at RWG2 init (BOP 2.1 / vanilla 1.7.10 constructors; values set
/// only in `genTerrainBlocks` don't count: RWG2 never calls it).
pub fn base_top_filler(name: &str) -> (u8, u8) {
    use blk::*;
    match name {
        "Alps" => (SNOW, SNOW),
        "Canyon" | "Canyon Ravine" => (BOP_HARD_DIRT, BOP_HARD_DIRT),
        "Crag" => (BOP_CRAG_ROCK, BOP_CRAG_ROCK),
        "Garden" => (BOP_LONG_GRASS, DIRT),
        "Lush Desert" => (HARDENED_CLAY, HARDENED_CLAY),
        "Mangrove" | "Xeric Shrubland" | "Dry River" => (SAND, SAND),
        "Origin Valley" => (BOP_ORIGIN_GRASS, DIRT),
        "Outback" => (BOP_HARD_SAND, BOP_HARD_SAND),
        "Quagmire" => (BOP_MUD, BOP_MUD),
        "Wasteland" => (BOP_DRIED_DIRT, BOP_DRIED_DIRT),
        "Glacier" => (BOP_HARD_ICE, BOP_HARD_ICE),
        "Volcano" => (BOP_ASH_STONE, BOP_ASH_STONE),
        "MushroomIsland" | "MushroomIslandShore" => (MYCELIUM, DIRT),
        "Ice Plains Spikes" => (SNOW, DIRT),
        "Desert" | "DesertHills" | "Beach" | "Cold Beach" => (SAND, SAND),
        "Stone Beach" => (STONE, STONE),
        "Mesa" | "Mesa Plateau F" | "Mesa Plateau" => (RED_SAND, blk::clay(1)),
        _ => (GRASS, DIRT),
    }
}

// ---------------------------------------------------------------- painters

/// `SurfaceMountainStone` family: Stone / Mix1 / DesertMountain (`snow: None, island: None`), MountainSnow
/// (`snow`), IslandMountainStone (`island = beach height`).
#[derive(Clone, Copy, Debug)]
pub struct Mountain {
    top: u8,
    fill: u8,
    /// `genBeach` → `beachBlock`.
    beach: Option<u8>,
    min: f32,
    s_cliff: f32,
    s_height: f32,
    s_strength: f32,
    c_cliff: f32,
    /// MountainSnow (iCliff, iHeight, iStrength): snow cliffs, grass top above 62, dirt / gravel filler.
    snow: Option<(f32, f32, f32)>,
    /// StoneMix1 (mix block, mixHeight).
    mix: Option<(u8, f32)>,
    /// IslandMountainStone beach height.
    island: Option<i32>,
}

/// One `SurfaceBase.paintTerrain` / inline `rReplace`.
#[derive(Clone, Copy, Debug)]
pub enum Painter {
    /// `RealisticBiomeBase.rReplace`.
    Default,
    /// `SurfaceGrassland` (`mix`: `SurfaceGrasslandMix1` (mix block, width, height)).
    Grassland {
        top: u8,
        fill: u8,
        c1: u8,
        c2: u8,
        mix: Option<(u8, f32, f32)>,
    },
    /// `SurfaceGrasslandMixBig`.
    MixBig {
        top: u8,
        fill: u8,
        mtop: u8,
        mfill: u8,
        c1: u8,
        c2: u8,
        w: f32,
        h: f32,
        sw: f32,
        ss: f32,
    },
    Mountain(Mountain),
    Tundra,
    Polar,
    /// `SurfaceDuneValley(top, fill, valley, dirt, mix)`.
    DuneValley {
        top: u8,
        fill: u8,
        valley: f32,
        dirt: bool,
        mix: bool,
    },
    /// `SurfaceDesert(top, filler, bottom, cliff1, cliff2)`.
    Desert {
        top: u8,
        fill: u8,
        bottom: u8,
        c1: u8,
        c2: u8,
    },
    RedDesert,
    /// `SurfaceDesertOasis(top, filler, cliff1, cliff2, sandMeta → sand block, cliffType)`.
    DesertOasis {
        top: u8,
        fill: u8,
        c1: u8,
        c2: u8,
        sand: u8,
        clay_cliff: bool,
    },
    /// `SurfaceGrassCanyon(top, fill, clay colour)`.
    GrassCanyon {
        top: u8,
        fill: u8,
        clay: u8,
    },
    /// `SurfaceCanyon(top, fill, meta, grassRaise)` (top / fill already carry the meta).
    Canyon {
        top: u8,
        fill: u8,
        raise: i32,
    },
    Mesa {
        top: u8,
        fill: u8,
    },
    RiverOasis,
    /// Coast `rReplace`s: CoastTest / CoastOasis (`sand_below = 65`), Mangrove (`grass_above = 67`), Dunes (68).
    CoastSand {
        cliff: f32,
    },
    CoastDunes {
        cliff: f32,
        grass_above: i32,
    },
    CoastColdSlope,
    CoastIce,
    /// Ocean (sand shallow / gravel deep; water resets depth); OceanTest: sand.
    Ocean {
        block: u8,
    },
    OceanTest,
    /// `RealisticBiomeFungiCanyonIsland.paintShallowPools` (after its MountainStone).
    FungiPools {
        top: u8,
        fill: u8,
    },
    /// `SurfaceVolcanoAsh` with `surfaceDepth`.
    VolcanoAsh {
        depth: i32,
    },
}

/// Per-column inputs of a paint call.
struct Ctx<'a> {
    perlin: &'a PerlinNoise,
    clay: &'a [u8; 100],
    /// World x, z (`i`, `j`).
    i: f32,
    j: f32,
    cliff: f32,
    river: f32,
    /// `testHeight` (SnowheightCalculator).
    height: f32,
}

fn snow_layer(col: &mut [u8; 256], k: usize, height: f32) {
    // SnowheightCalculator: h = (byte) (frac * 8) is at most 7, so always one layer at k + 1
    if k < 254 {
        let _ = height;
        col[k + 1] = blk::SNOW_LAYER;
    }
}

impl Painter {
    fn paint(&self, col: &mut [u8; 256], r: &mut JavaRandom, x: &Ctx) {
        use blk::*;
        let per = x.perlin;
        let c = x.cliff;
        let mut depth: i32 = -1;
        // rows above the highest non-air block only reset `depth` (and `ash`), which start reset: skip them
        let hi = col.iter().rposition(|&b| b != AIR).map_or(0, |y| y + 1);
        match *self {
            Painter::Default => {
                for k in (0..hi).rev() {
                    let b = col[k];
                    if b == AIR {
                        depth = -1;
                    } else if b == STONE {
                        depth += 1;
                        if depth == 0 {
                            col[k] = if k < 62 { DIRT } else { GRASS };
                        } else if depth < 6 {
                            col[k] = DIRT;
                        }
                    }
                }
            }
            Painter::Grassland {
                top,
                fill,
                c1,
                c2,
                mix,
            } => {
                let cliff = c > 1.4;
                for k in (0..hi).rev() {
                    let b = col[k];
                    if b == AIR {
                        depth = -1;
                    } else if b == STONE {
                        depth += 1;
                        if cliff {
                            if depth < 2 {
                                col[k] = if r.next_int_bounded(3) == 0 { c2 } else { c1 };
                            } else if depth < 10 {
                                col[k] = c1;
                            }
                        } else if depth == 0 && k > 61 {
                            col[k] = match mix {
                                Some((m, w, h)) if per.noise2(x.i / w, x.j / w) > h => m,
                                _ => top,
                            };
                        } else if depth < 4 {
                            col[k] = fill;
                        }
                    }
                }
            }
            Painter::MixBig {
                top,
                fill,
                mtop,
                mfill,
                c1,
                c2,
                w,
                h,
                sw,
                ss,
            } => {
                let cliff = c > 1.4;
                let mut mix = false;
                for k in (0..hi).rev() {
                    let b = col[k];
                    if b == AIR {
                        depth = -1;
                    } else if b == STONE {
                        depth += 1;
                        if cliff {
                            if depth < 2 {
                                col[k] = if r.next_int_bounded(3) == 0 { c2 } else { c1 };
                            } else if depth < 10 {
                                col[k] = c1;
                            }
                        } else if depth == 0 && k > 61 {
                            if per.noise2(x.i / w, x.j / w) + per.noise2(x.i / sw, x.j / sw) * ss
                                > h
                            {
                                col[k] = mtop;
                                mix = true;
                            } else {
                                col[k] = top;
                            }
                        } else if depth < 4 {
                            col[k] = if mix { mfill } else { fill };
                        }
                    }
                }
            }
            Painter::Mountain(m) => {
                let mut cliff = 0;
                let mut gravel = false;
                let mut mixed = false;
                let beach_k = m.island.unwrap_or(63);
                for k in (0..hi).rev() {
                    let b = col[k];
                    if b == AIR {
                        depth = -1;
                    } else if b == STONE {
                        depth += 1;
                        let kf = k as f32;
                        if depth == 0 {
                            if (k as i32) < beach_k && (m.island.is_some() || m.beach.is_some()) {
                                gravel = true;
                            }
                            let p = per.noise3(x.i / 8.0, x.j / 8.0, kf / 8.0) * 0.5;
                            if c > m.min && c > m.s_cliff - ((kf - m.s_height) / m.s_strength) + p {
                                cliff = 1;
                            }
                            if c > m.c_cliff {
                                cliff = 2;
                            }
                            if let Some((ic, ih, is)) = m.snow {
                                if kf > 110.0 + (p * 4.0) && c < ic + ((kf - ih) / is) + p {
                                    cliff = 3;
                                }
                            }
                            col[k] = if cliff == 1 {
                                if r.next_int_bounded(3) == 0 {
                                    COBBLE
                                } else {
                                    STONE
                                }
                            } else if cliff == 2 {
                                blk::clay(9)
                            } else if cliff == 3 {
                                SNOW
                            } else if m.island.is_some() {
                                if (k as i32) < beach_k {
                                    gravel = true;
                                    m.beach.unwrap_or(SAND)
                                } else {
                                    m.top
                                }
                            } else if k < 63 {
                                if let Some(bb) = m.beach {
                                    gravel = true;
                                    bb
                                } else if k < 62 {
                                    m.fill
                                } else {
                                    m.top
                                }
                            } else if m.snow.is_some() {
                                GRASS
                            } else if let Some((mb, mh)) = m.mix {
                                if per.noise2(x.i / 12.0, x.j / 12.0) > mh {
                                    mixed = true;
                                    mb
                                } else {
                                    m.top
                                }
                            } else {
                                m.top
                            };
                        } else if depth < 6 {
                            col[k] = if cliff == 1 {
                                STONE
                            } else if cliff == 2 {
                                blk::clay(9)
                            } else if cliff == 3 {
                                SNOW
                            } else if gravel {
                                if m.snow.is_some() {
                                    GRAVEL
                                } else {
                                    m.beach.unwrap_or(SAND)
                                }
                            } else if mixed {
                                m.mix.map_or(m.fill, |(mb, _)| mb)
                            } else if m.snow.is_some() {
                                DIRT
                            } else {
                                m.fill
                            };
                        }
                    }
                }
            }
            Painter::Tundra => {
                let p = per.noise2(x.i / 8.0, x.j / 8.0) * 0.5;
                let mut cliff = 0;
                let mut gravel = false;
                for k in (0..hi).rev() {
                    let b = col[k];
                    if b == AIR {
                        depth = -1;
                    } else if b == STONE {
                        depth += 1;
                        let kf = k as f32;
                        if depth == 0 {
                            if k < 63 {
                                gravel = true;
                            }
                            if c > 0.45 && c > 1.5 - ((kf - 60.0) / 65.0) + p {
                                cliff = 1;
                            }
                            if c > 1.5 {
                                cliff = 2;
                            }
                            if kf > 110.0 + (p * 4.0) && c < 0.3 + ((kf - 100.0) / 50.0) + p {
                                cliff = 3;
                            }
                            col[k] = if cliff == 1 {
                                if r.next_int_bounded(3) == 0 {
                                    COBBLE
                                } else {
                                    STONE
                                }
                            } else if cliff == 2 {
                                blk::clay(9)
                            } else if cliff == 3 {
                                SNOW
                            } else if per.noise2(x.i / 50.0, x.j / 50.0) + p * 0.6 > 0.24 {
                                PODZOL
                            } else if k < 63 {
                                gravel = true;
                                GRAVEL
                            } else {
                                GRASS
                            };
                        } else if depth < 6 {
                            col[k] = match cliff {
                                1 => STONE,
                                2 => blk::clay(9),
                                3 => SNOW,
                                _ if gravel => GRAVEL,
                                _ => DIRT,
                            };
                        }
                    }
                }
            }
            Painter::Polar => {
                let mut water = false;
                let (mut river_paint, mut grass) = (false, false);
                if x.river > 0.05 && x.river + (per.noise2(x.i / 10.0, x.j / 10.0) * 0.1) > 0.86 {
                    river_paint = true;
                    grass = per.noise2(x.i / 12.0, x.j / 12.0) > 0.25;
                }
                for k in (0..hi).rev() {
                    let b = col[k];
                    if b == AIR {
                        depth = -1;
                    } else if b == STONE {
                        depth += 1;
                        if river_paint {
                            if grass && depth < 4 {
                                col[k] = DIRT;
                            } else if depth == 0 {
                                col[k] = if r.next_int_bounded(2) == 0 {
                                    STONE
                                } else {
                                    COBBLE
                                };
                            }
                        } else if depth < 9 {
                            col[k] = SNOW;
                            if depth == 0 && k > 61 && k < 254 {
                                snow_layer(col, k, x.height);
                            }
                        }
                    } else if !water && b == WATER {
                        col[k] = ICE;
                        water = true;
                    }
                }
            }
            Painter::DuneValley {
                top,
                fill,
                valley,
                dirt,
                mix,
            } => {
                let h = ((per.noise2(x.i / valley, x.j / valley) + 0.25) * 65.0).max(1.0);
                let m = per.noise2(x.i / 12.0, x.j / 12.0);
                let mut sand = false;
                for k in (0..hi).rev() {
                    let b = col[k];
                    if b == AIR {
                        depth = -1;
                    } else if b == STONE {
                        depth += 1;
                        if depth == 0 {
                            if k as f32 > 90.0 + per.noise2(x.i / 24.0, x.j / 24.0) * 10.0 - h
                                || (m < -0.28 && mix)
                            {
                                col[k] = SAND;
                                sand = true;
                            } else if dirt && m < 0.22 || k < 62 {
                                col[k] = COARSE_DIRT;
                            } else {
                                col[k] = top;
                            }
                        } else if depth < 6 {
                            col[k] = if sand {
                                if depth < 4 {
                                    SAND
                                } else {
                                    SANDSTONE
                                }
                            } else {
                                fill
                            };
                        }
                    }
                }
            }
            Painter::Desert {
                top,
                fill,
                bottom,
                c1,
                c2,
            } => {
                let cliff = c > 2.8;
                for k in (0..hi).rev() {
                    let b = col[k];
                    if b == AIR {
                        depth = -1;
                    } else if b == STONE {
                        depth += 1;
                        if cliff {
                            if depth < 2 {
                                col[k] = if r.next_int_bounded(3) == 0 { c2 } else { c1 };
                            } else if depth < 10 {
                                col[k] = c1;
                            }
                        } else if depth < 6 {
                            col[k] = if depth == 0 && k > 61 {
                                top
                            } else if depth < 4 {
                                fill
                            } else {
                                bottom
                            };
                        }
                    }
                }
            }
            Painter::RedDesert => {
                let cliff = c > 1.4;
                for k in (0..hi).rev() {
                    let b = col[k];
                    if b == AIR {
                        depth = -1;
                    } else if b == STONE {
                        depth += 1;
                        if depth < 6 {
                            col[k] = if cliff {
                                blk::clay(14)
                            } else if depth == 0 && k > 61 || depth < 4 {
                                RED_SAND
                            } else {
                                SANDSTONE
                            };
                        }
                    }
                }
            }
            Painter::DesertOasis {
                top,
                fill,
                c1,
                c2,
                sand,
                clay_cliff,
            } => {
                let cliff = c > 1.3;
                let mut dirt = false;
                for k in (0..hi).rev() {
                    let b = col[k];
                    if b == AIR {
                        depth = -1;
                    } else if b == STONE {
                        depth += 1;
                        if cliff {
                            if clay_cliff {
                                if depth < 6 {
                                    col[k] = blk::clay(14);
                                }
                            } else if depth < 2 {
                                col[k] = if r.next_int_bounded(3) == 0 { c2 } else { c1 };
                            } else if depth < 10 {
                                col[k] = c1;
                            }
                        } else if depth < 6 {
                            if depth == 0 && k > 61 {
                                if per.noise2(x.i / 12.0, x.j / 12.0)
                                    > -0.3 + ((k as f32 - 61.0) / 15.0)
                                {
                                    dirt = true;
                                    col[k] = top;
                                } else {
                                    col[k] = sand;
                                }
                            } else if depth < 4 {
                                col[k] = if dirt { fill } else { sand };
                            } else if !dirt {
                                col[k] = SANDSTONE;
                            }
                        }
                    }
                }
            }
            Painter::GrassCanyon { top, fill, clay } => {
                let cliff = c > 1.3;
                for k in (0..hi).rev() {
                    let b = col[k];
                    if b == AIR {
                        depth = -1;
                    } else if b == STONE {
                        depth += 1;
                        if depth < 12 {
                            col[k] = if cliff || depth > 4 {
                                clay
                            } else if depth == 0 {
                                top
                            } else {
                                fill
                            };
                        } else if k > 63 {
                            col[k] = clay;
                        }
                    }
                }
            }
            Painter::Canyon { top, fill, raise } => {
                let cliff = c > 1.3;
                for k in (0..hi).rev() {
                    let b = col[k];
                    if b == AIR {
                        depth = -1;
                    } else if b == STONE {
                        depth += 1;
                        let ki = k as i32;
                        let clay = blk::clay(clay_for(x.clay, ki));
                        if depth < 12 {
                            if cliff || depth > 4 {
                                col[k] = clay;
                            } else if ki > 74 + raise {
                                col[k] = if r.next_int_bounded(5) == 0 {
                                    DIRT
                                } else if depth == 0 {
                                    top
                                } else {
                                    fill
                                };
                            } else if ki < 62 {
                                col[k] = DIRT;
                            } else if ki < 62 + raise {
                                col[k] = if depth == 0 { GRASS } else { DIRT };
                            } else if ki < 75 + raise {
                                if depth == 0 {
                                    let rr = ((ki - (62 + raise)) as f32 / 2.0) as i32;
                                    col[k] = if r.next_int_bounded(rr + 1) == 0 {
                                        GRASS
                                    } else if r.next_int_bounded((rr as f32 / 2.0) as i32 + 1) == 0
                                    {
                                        DIRT
                                    } else {
                                        top
                                    };
                                } else {
                                    col[k] = fill;
                                }
                            } else {
                                col[k] = if depth == 0 { top } else { fill };
                            }
                        } else if k > 63 {
                            col[k] = clay;
                        }
                    }
                }
            }
            Painter::Mesa { top, fill } => {
                let cliff = c > 1.3;
                for k in (0..hi).rev() {
                    let b = col[k];
                    if b == AIR {
                        depth = -1;
                    } else if b == STONE {
                        depth += 1;
                        let ki = k as i32;
                        let clay = blk::clay(clay_for(x.clay, ki));
                        if depth < 12 {
                            if cliff || depth > 4 {
                                col[k] = clay;
                            } else if ki > 77 {
                                col[k] = if r.next_int_bounded(5) == 0 {
                                    DIRT
                                } else if depth == 0 {
                                    top
                                } else {
                                    fill
                                };
                            } else if ki < 69 {
                                col[k] = DIRT;
                            } else if ki < 78 {
                                if depth == 0 {
                                    // Java: `k < 72 && nextInt(k - 68) == 0` → dirt, else `nextInt(5) == 0` → dirt
                                    let low = ki < 72 && r.next_int_bounded(ki - 69 + 1) == 0;
                                    col[k] = if low || r.next_int_bounded(5) == 0 {
                                        DIRT
                                    } else {
                                        top
                                    };
                                } else {
                                    col[k] = fill;
                                }
                            } else {
                                col[k] = if depth == 0 { top } else { fill };
                            }
                        } else if k > 63 {
                            col[k] = clay;
                        }
                    }
                }
            }
            Painter::RiverOasis => {
                if x.river > 0.05 && x.river + (per.noise2(x.i / 10.0, x.j / 10.0) * 0.15) > 0.8 {
                    for k in (0..hi).rev() {
                        let b = col[k];
                        if b == AIR {
                            depth = -1;
                        } else if b != WATER {
                            depth += 1;
                            if depth == 0 && k > 61 {
                                col[k] = GRASS;
                            } else if depth < 4 {
                                col[k] = DIRT;
                            } else if depth > 4 {
                                return;
                            }
                        }
                    }
                }
            }
            Painter::CoastSand { cliff: cl } => {
                let cliff = c > cl;
                for k in (0..hi).rev() {
                    let b = col[k];
                    if b == AIR {
                        depth = -1;
                    } else if b == STONE {
                        depth += 1;
                        if cliff {
                            if depth < 2 {
                                col[k] = if r.next_int_bounded(3) == 0 {
                                    COBBLE
                                } else {
                                    STONE
                                };
                            } else if depth < 10 {
                                col[k] = STONE;
                            }
                        } else if depth < 7 && k < 65 {
                            col[k] = SAND;
                        } else if depth == 0 {
                            col[k] = GRASS;
                        } else if depth < 4 {
                            col[k] = DIRT;
                        }
                    }
                }
            }
            Painter::CoastDunes {
                cliff: cl,
                grass_above,
            } => {
                let mut sand = false;
                for k in (0..hi).rev() {
                    let b = col[k];
                    if b == AIR {
                        depth = -1;
                    } else if b == STONE {
                        depth += 1;
                        if c > cl {
                            if depth < 2 {
                                col[k] = if r.next_int_bounded(3) == 0 {
                                    COBBLE
                                } else {
                                    STONE
                                };
                            } else if depth < 10 {
                                col[k] = STONE;
                            }
                        } else if depth == 0 {
                            if k as i32 > grass_above {
                                col[k] = GRASS;
                            } else {
                                col[k] = SAND;
                                sand = true;
                            }
                        } else if depth < 5 {
                            col[k] = if sand {
                                if depth < 4 {
                                    SAND
                                } else {
                                    SANDSTONE
                                }
                            } else {
                                DIRT
                            };
                        }
                    }
                }
            }
            Painter::CoastColdSlope => {
                let cliff = c > 0.5;
                let mut gravel = false;
                for k in (0..hi).rev() {
                    let b = col[k];
                    if b == AIR {
                        depth = -1;
                    } else if b == STONE {
                        depth += 1;
                        if cliff {
                            if depth < 2 {
                                col[k] = if r.next_int_bounded(3) == 0 {
                                    COBBLE
                                } else {
                                    STONE
                                };
                            } else if depth < 10 {
                                col[k] = STONE;
                            }
                        } else if depth == 0 {
                            if k > 64 {
                                col[k] = GRASS;
                            } else {
                                col[k] = GRAVEL;
                                gravel = true;
                            }
                        } else if depth < 4 {
                            col[k] = if gravel { GRAVEL } else { DIRT };
                        }
                    }
                }
            }
            Painter::CoastIce => {
                let cliff = c > 1.4;
                let mut ty = 0;
                for k in (0..hi).rev() {
                    let b = col[k];
                    if b == AIR {
                        depth = -1;
                    } else if b == STONE {
                        depth += 1;
                        if cliff {
                            if depth == 0 {
                                ty = if per.noise2(x.i / 5.0, x.j / 5.0) > 0.0 {
                                    1
                                } else {
                                    0
                                };
                            }
                            if depth < 10 {
                                col[k] = if ty == 0 { PACKED_ICE } else { ICE };
                            }
                        } else if depth < 5 && k > 61 {
                            col[k] = SNOW;
                            if depth == 0 && k < 254 {
                                snow_layer(col, k, x.height);
                            }
                        } else if depth < 3 {
                            col[k] = GRAVEL;
                        }
                    }
                }
            }
            Painter::Ocean { block } => {
                for k in (0..hi).rev() {
                    let b = col[k];
                    if b == AIR || b == WATER {
                        depth = -1;
                    } else if b == STONE {
                        depth += 1;
                        if depth < 6 {
                            col[k] = block;
                        }
                    }
                }
            }
            Painter::OceanTest => {
                for k in (0..hi).rev() {
                    let b = col[k];
                    if b == AIR {
                        depth = -1;
                    } else if b == STONE {
                        depth += 1;
                        if depth < 6 {
                            col[k] = SAND;
                        }
                    }
                }
            }
            Painter::FungiPools { top, fill } => {
                if c > 0.45 {
                    return;
                }
                let pool =
                    per.noise2(x.i / 32.0, x.j / 32.0) + per.noise2(x.i / 9.0, x.j / 9.0) * 0.25;
                if pool < 0.5 {
                    return;
                }
                for h in (2..256).rev() {
                    if col[h] == AIR && col[h - 1] == top {
                        col[h - 1] = WATER;
                        if pool > 0.82 && col[h - 2] == fill {
                            col[h - 2] = WATER;
                        }
                        return;
                    }
                }
            }
            Painter::VolcanoAsh { depth: sd } => {
                let mut ash = false;
                for k in (0..hi).rev() {
                    let b = col[k];
                    if b == AIR {
                        depth = -1;
                        ash = false;
                    } else if b == STONE {
                        depth += 1;
                        if depth == 0 {
                            ash = k >= 61;
                            if ash {
                                col[k] = BOP_ASH;
                            }
                        } else if ash && depth < 6 && depth < sd {
                            col[k] = if depth < 2 { BOP_ASH } else { BOP_ASH_STONE };
                        }
                    }
                }
            }
        }
    }
}

fn clay_for(t: &[u8; 100], k: i32) -> u8 {
    t[(k - 60).clamp(0, 99) as usize]
}

/// `SurfaceCanyon` / `SurfaceMesa` clay colours by height (`perlin(2)`, colours {1, 8, 0}).
fn clay_table() -> [u8; 100] {
    let perlin = PerlinNoise::new(2);
    let c = [1u8, 8, 0];
    let mut t = [0u8; 100];
    for (i, v) in t.iter_mut().enumerate() {
        let mut n = perlin.noise1(i as f32 / 3.0) * 3.0 + perlin.noise1(i as f32) * 0.3 + 1.5;
        n = if n >= 3.0 {
            2.9
        } else if n < 0.0 {
            0.0
        } else {
            n
        };
        *v = c[n as usize];
    }
    t
}

// ---------------------------------------------------------------- per-biome tables

#[allow(clippy::too_many_arguments)]
const fn mtn(
    top: u8,
    fill: u8,
    beach: Option<u8>,
    min: f32,
    s_cliff: f32,
    s_height: f32,
    s_strength: f32,
    c_cliff: f32,
) -> Mountain {
    Mountain {
        top,
        fill,
        beach,
        min,
        s_cliff,
        s_height,
        s_strength,
        c_cliff,
        snow: None,
        mix: None,
        island: None,
    }
}

fn stone(top: u8, fill: u8, beach: Option<u8>, min: f32) -> Painter {
    Painter::Mountain(mtn(top, fill, beach, min, 1.5, 60.0, 65.0, 1.5))
}

#[allow(clippy::too_many_arguments)]
fn snowy(
    top: u8,
    fill: u8,
    beach: Option<u8>,
    min: f32,
    sh: f32,
    ss: f32,
    ic: f32,
    ih: f32,
) -> Painter {
    let mut m = mtn(top, fill, beach, min, 1.5, sh, ss, 1.5);
    m.snow = Some((ic, ih, 50.0));
    Painter::Mountain(m)
}

fn mix1(top: u8, fill: u8, mix: f32) -> Painter {
    let mut m = mtn(top, fill, None, 0.0, 1.5, 60.0, 65.0, 1.5);
    m.mix = Some((blk::STONE, mix));
    Painter::Mountain(m)
}

fn grassland(top: u8, fill: u8) -> Painter {
    Painter::Grassland {
        top,
        fill,
        c1: blk::STONE,
        c2: blk::COBBLE,
        mix: None,
    }
}

/// Native RWG2 biomes by class (constructors in `biomes/realistic/**`).
fn native_painters(class: &str, base: &str) -> Option<Vec<Painter>> {
    use blk::*;
    use Painter::*;
    let snow_beach = snowy(GRASS, DIRT, Some(SAND), 0.2, 60.0, 65.0, 0.3, 100.0);
    let ro = RiverOasis;
    Some(match class {
        "RealisticBiomeTest"
        | "RealisticBiomeSnowLakes"
        | "RealisticBiomeSnowRivers"
        | "RealisticBiomeRedwoodSnow"
        | "RealisticBiomeTaigaHills"
        | "RealisticBiomeWoodMountains" => vec![snow_beach],
        "RealisticBiomeSnowHills" => vec![snowy(GRASS, DIRT, None, 0.2, 60.0, 65.0, 0.3, 100.0)],
        "RealisticBiomeTestRiver" => vec![mix1(GRASS, DIRT, 0.08)],
        "RealisticBiomeTundraHills"
        | "RealisticBiomeTundraPlains"
        | "RealisticBiomeTaigaPlains" => vec![Tundra],
        "RealisticBiomeRedwood" => vec![stone(GRASS, DIRT, Some(SAND), 0.2)],
        "RealisticBiomeDarkRedwood" => vec![mix1(DIRT, DIRT, 0.15)],
        "RealisticBiomeDarkRedwoodPlains" => vec![mix1(GRASS, DIRT, 0.15)],
        "RealisticBiomeWoodHills" => vec![stone(GRASS, DIRT, None, 0.0)],
        "RealisticBiomeDuneValleyForest" => {
            vec![DuneValley {
                top: GRASS,
                fill: DIRT,
                valley: 220.0,
                dirt: false,
                mix: true,
            }]
        }
        "RealisticBiomeSavanna" => {
            vec![Grassland {
                top: GRASS,
                fill: DIRT,
                c1: STONE,
                c2: COBBLE,
                mix: Some((SAND, 13.0, 0.27)),
            }]
        }
        "RealisticBiomeSavannaForest" => vec![stone(GRASS, DIRT, None, 0.6)],
        "RealisticBiomeSavannaDunes" => {
            vec![
                DuneValley {
                    top: GRASS,
                    fill: DIRT,
                    valley: 300.0,
                    dirt: true,
                    mix: true,
                },
                ro,
            ]
        }
        "RealisticBiomeStoneMountains" => vec![mix1(GRASS, DIRT, 0.08), ro],
        "RealisticBiomeStoneMountainsCactus" => vec![mix1(GRASS, DIRT, 0.20), ro],
        "RealisticBiomeHotForest" | "RealisticBiomeHotRedwood" => vec![MixBig {
            top: SAND,
            fill: SAND,
            mtop: GRASS,
            mfill: DIRT,
            c1: STONE,
            c2: COBBLE,
            w: 60.0,
            h: -0.14,
            sw: 14.0,
            ss: 0.25,
        }],
        "RealisticBiomeCanyonForest" => vec![Canyon {
            top: RED_SAND,
            fill: RED_SAND,
            raise: 47,
        }],
        "RealisticBiomeMesaPlains" => vec![Canyon {
            top: RED_SAND,
            fill: RED_SAND,
            raise: 20,
        }],
        "RealisticBiomeCanyon" => vec![Canyon {
            top: RED_SAND,
            fill: RED_SAND,
            raise: 0,
        }],
        "RealisticBiomeMesa" => vec![
            Mesa {
                top: RED_SAND,
                fill: RED_SAND,
            },
            ro,
        ],
        "RealisticBiomeDesert" | "RealisticBiomeDesertMountains" => {
            vec![stone(SAND, SANDSTONE, None, 0.0), ro]
        }
        "RealisticBiomeDuneValley" => {
            vec![
                Desert {
                    top: SAND,
                    fill: SAND,
                    bottom: SANDSTONE,
                    c1: STONE,
                    c2: COBBLE,
                },
                ro,
            ]
        }
        "RealisticBiomeRedDesertMountains" => vec![RedDesert, ro],
        "RealisticBiomeOasis" => {
            vec![
                DesertOasis {
                    top: GRASS,
                    fill: DIRT,
                    c1: STONE,
                    c2: COBBLE,
                    sand: SAND,
                    clay_cliff: false,
                },
                ro,
            ]
        }
        "RealisticBiomeRedOasis" => vec![
            DesertOasis {
                top: GRASS,
                fill: DIRT,
                c1: blk::clay(0),
                c2: blk::clay(0),
                sand: RED_SAND,
                clay_cliff: true,
            },
            ro,
        ],
        "RealisticBiomeHighRainforest" | "RealisticBiomeRedwoodJungle" => {
            vec![grassland(GRASS, DIRT)]
        }
        "RealisticBiomeJungleHills" => vec![stone(GRASS, DIRT, None, 1.0)],
        "RealisticBiomeJungleCanyon" => vec![GrassCanyon {
            top: GRASS,
            fill: DIRT,
            clay: blk::clay(0),
        }],
        "RealisticBiomeCoastTest" | "RealisticBiomeCoastOasis" => vec![CoastSand { cliff: 1.4 }],
        "RealisticBiomeCoastColdSlope" => vec![CoastColdSlope],
        "RealisticBiomeCoastMangrove" => vec![CoastDunes {
            cliff: 1.3,
            grass_above: 67,
        }],
        "RealisticBiomeCoastDunes" => vec![CoastDunes {
            cliff: 1.3,
            grass_above: 68,
        }],
        "RealisticBiomeCoastColdCliff" => vec![],
        "RealisticBiomeCoastIce" => vec![CoastIce],
        "RealisticBiomeIslandTropical" | "RealisticBiomeIslandTundra" => {
            let mut m = mtn(GRASS, DIRT, Some(SAND), 0.0, 1.5, 60.0, 65.0, 1.5);
            m.island = Some(67);
            vec![Mountain(m)]
        }
        "RealisticBiomeOceanTest" => vec![OceanTest],
        "RealisticBiomePolar" => vec![Polar],
        "RealisticBiomeIslandVolcano" => vec![VolcanoAsh { depth: 6 }],
        "RealisticBiomeFungiCanyonIsland" => {
            let (t, f) = base_top_filler("Fungi Forest");
            vec![stone(t, f, None, 0.95), FungiPools { top: t, fill: f }]
        }
        "RealisticBiomeMountainChain" => {
            let (t, f) = base_top_filler(base);
            vec![mix1(t, f, 0.20)]
        }
        _ => return None,
    })
}

/// `RealisticBiomeSupport` entries by base-biome name and occurrence (SupportBOP / Support / SupportTC order).
fn support_painters(name: &str, occ: usize) -> Option<Vec<Painter>> {
    use blk::*;
    use Painter::*;
    let (t, f) = base_top_filler(name);
    let boreal = base_top_filler("Boreal Forest");
    Some(match (name, occ) {
        ("Bamboo Forest" | "Eucalyptus Forest" | "Fungi Forest" | "Jade Cliffs", _) => {
            vec![stone(t, f, None, 0.95)]
        }
        ("Boreal Forest", _) | ("Garden", 0) => {
            vec![snowy(t, f, Some(SAND), 0.45, 60.0, 65.0, 0.4, 130.0)]
        }
        ("Brushland", 0) => vec![Grassland {
            top: t,
            fill: f,
            c1: STONE,
            c2: COBBLE,
            mix: Some((SAND, 13.0, 0.27)),
        }],
        ("Brushland" | "Heathland", _) => {
            let (bt, bf) = base_top_filler("Brushland");
            vec![
                DuneValley {
                    top: bt,
                    fill: bf,
                    valley: 300.0,
                    dirt: false,
                    mix: true,
                },
                RiverOasis,
            ]
        }
        ("Outback", _) => vec![
            DuneValley {
                top: t,
                fill: f,
                valley: 300.0,
                dirt: false,
                mix: false,
            },
            RiverOasis,
        ],
        ("Canyon", _) => vec![Canyon {
            top: t,
            fill: f,
            raise: 0,
        }],
        ("Chaparral", _) => vec![Grassland {
            top: t,
            fill: f,
            c1: STONE,
            c2: COBBLE,
            mix: Some((SAND, 26.0, 0.35)),
        }],
        ("Cherry Blossom Grove", _) => vec![stone(boreal.0, boreal.1, Some(SAND), 0.45)],
        ("Coniferous Forest", _) => vec![snowy(GRASS, DIRT, None, 0.45, 60.0, 65.0, 0.3, 100.0)],
        ("Snowy Coniferous Forest", _) => {
            vec![snowy(GRASS, DIRT, None, 0.45, 50.0, 60.0, 0.4, 100.0)]
        }
        ("Highland", _) => vec![Painter::Mountain(mtn(
            t, f, None, 1.0, 1.5, 85.0, 20.0, 4.0,
        ))],
        ("Lavender Fields" | "Meadow", _) => vec![stone(t, f, None, 1.2)],
        ("Mountain", _) => vec![stone(t, f, Some(SAND), 0.75)],
        ("Rainforest", _) => vec![stone(t, f, None, 1.3)],
        ("Redwood Forest", _) => vec![stone(t, f, None, 0.4)],
        ("Temperate Rainforest", _) => vec![stone(t, f, None, 0.45)],
        (
            "Bayou"
            | "Bog"
            | "Dead Forest"
            | "Dead Swamp"
            | "Deciduous Forest"
            | "Fen"
            | "Flower Field"
            | "Frost Forest"
            | "Grove"
            | "Land of Lakes Marsh"
            | "Lush Desert"
            | "Lush Swamp"
            | "Maple Woods"
            | "Mangrove"
            | "Ominous Woods"
            | "Prairie"
            | "Quagmire"
            | "Sacred Springs"
            | "Seasonal Forest"
            | "Shield"
            | "Shrubland"
            | "Sludgepit"
            | "Tropical Rainforest"
            | "Tropics"
            | "Oasis"
            | "Tundra"
            | "Wetland"
            | "Woodland"
            | "Garden"
            | "MushroomIsland"
            | "Ice Plains Spikes"
            | "Sunflower Plains"
            | "Magical Forest"
            | "Tainted Land",
            _,
        ) => vec![grassland(t, f)],
        _ => return None,
    })
}

/// Surface painters of every RWG id, plus what the post-surface passes need.
#[derive(Clone, Debug)]
pub struct SurfaceTable {
    pub by_id: Vec<Vec<Painter>>,
    /// Ids not ported (painted with `RealisticBiomeBase.rReplace`).
    pub guessed: Vec<bool>,
    /// Base (gen) id → `topBlock`, `fillerBlock` for the cave `digBlock` fix (`isExceptionBiome`: grass/dirt).
    prov: Vec<(u8, u8)>,
    volcano: Option<BiomeId>,
    clay: [u8; 100],
}

impl SurfaceTable {
    pub fn new(table: &BiomeTable) -> Self {
        let mut seen = std::collections::HashMap::<String, usize>::new();
        let shallow: Vec<BiomeId> = table
            .ocean_shallow
            .iter()
            .copied()
            .chain([table.ocean_shallow_kelp, table.ocean_shallow_coral])
            .collect();
        let mut by_id = Vec::with_capacity(table.biomes.len());
        let mut guessed = Vec::with_capacity(table.biomes.len());
        let mut prov = vec![(blk::GRASS, blk::DIRT); 1 << 16];
        for b in &table.biomes {
            let class = b.class.rsplit('.').next().unwrap_or("");
            let found = match class {
                "RealisticBiomeSupport" => {
                    let occ = seen.entry(b.name.clone()).or_insert(0);
                    *occ += 1;
                    support_painters(&b.name, *occ - 1)
                }
                "RealisticBiomeOcean" | "RealisticBiomeBOPOcean" => Some(vec![Painter::Ocean {
                    block: if shallow.contains(&b.rwg_id) {
                        blk::SAND
                    } else {
                        blk::GRAVEL
                    },
                }]),
                _ => native_painters(class, &b.name),
            };
            guessed.push(found.is_none());
            by_id.push(found.unwrap_or_else(|| vec![Painter::Default]));
            prov[b.gen_id as usize] = base_top_filler(&b.name);
        }
        // isExceptionBiome: mushroomIsland, beach, desert dig as grass / dirt
        for id in [2usize, 14, 16] {
            prov[id] = (blk::GRASS, blk::DIRT);
        }
        SurfaceTable {
            by_id,
            guessed,
            prov,
            volcano: table.volcano_island,
            clay: clay_table(),
        }
    }

    fn painters(&self, id: BiomeId) -> &[Painter] {
        self.by_id
            .get(id as usize)
            .map(|v| v.as_slice())
            .unwrap_or(&[Painter::Default])
    }
}

impl Painter {
    /// Every block this painter can write (for search pre-filters).
    pub fn outputs(&self) -> Vec<u8> {
        use blk::*;
        let clays = |v: &mut Vec<u8>| v.extend([clay(0), clay(1), clay(8)]);
        let mut v = Vec::new();
        match *self {
            Painter::Default | Painter::RiverOasis => v.extend([GRASS, DIRT]),
            Painter::Grassland {
                top,
                fill,
                c1,
                c2,
                mix,
            } => {
                v.extend([top, fill, c1, c2]);
                if let Some((m, _, _)) = mix {
                    v.push(m);
                }
            }
            Painter::MixBig {
                top,
                fill,
                mtop,
                mfill,
                c1,
                c2,
                ..
            } => v.extend([top, fill, mtop, mfill, c1, c2]),
            Painter::Mountain(m) => {
                v.extend([m.top, m.fill, COBBLE, STONE, clay(9)]);
                if let Some(b) = m.beach {
                    v.push(b);
                }
                if m.snow.is_some() {
                    v.extend([SNOW, GRASS, DIRT, GRAVEL]);
                }
                if let Some((b, _)) = m.mix {
                    v.push(b);
                }
            }
            Painter::Tundra => {
                v.extend([COBBLE, STONE, clay(9), SNOW, PODZOL, GRAVEL, GRASS, DIRT])
            }
            Painter::Polar => v.extend([DIRT, STONE, COBBLE, SNOW, SNOW_LAYER, ICE]),
            Painter::DuneValley { top, fill, .. } => {
                v.extend([SAND, COARSE_DIRT, top, SANDSTONE, fill])
            }
            Painter::Desert {
                top,
                fill,
                bottom,
                c1,
                c2,
            } => v.extend([top, fill, bottom, c1, c2]),
            Painter::RedDesert => v.extend([clay(14), RED_SAND, SANDSTONE]),
            Painter::DesertOasis {
                top,
                fill,
                c1,
                c2,
                sand,
                ..
            } => v.extend([clay(14), c1, c2, top, sand, fill, SANDSTONE]),
            Painter::GrassCanyon { top, fill, clay } => v.extend([clay, top, fill]),
            Painter::Canyon { top, fill, .. } | Painter::Mesa { top, fill } => {
                clays(&mut v);
                v.extend([DIRT, GRASS, top, fill]);
            }
            Painter::CoastSand { .. } => v.extend([COBBLE, STONE, SAND, GRASS, DIRT]),
            Painter::CoastDunes { .. } => v.extend([COBBLE, STONE, GRASS, SAND, SANDSTONE, DIRT]),
            Painter::CoastColdSlope => v.extend([COBBLE, STONE, GRASS, GRAVEL, DIRT]),
            Painter::CoastIce => v.extend([PACKED_ICE, ICE, SNOW, SNOW_LAYER, GRAVEL]),
            Painter::Ocean { block } => v.push(block),
            Painter::OceanTest => v.push(SAND),
            Painter::FungiPools { .. } => v.push(WATER),
            Painter::VolcanoAsh { .. } => v.extend([BOP_ASH, BOP_ASH_STONE, LAVA, OBSIDIAN]),
        }
        v
    }
}

/// Blocks a column can show whatever biome paints it (bare stone of river tunnels / unpainted runs, caves, the
/// lava landmark, sea water, cave filler → grass).
pub const ANYWHERE: [u8; 7] = [
    blk::STONE,
    blk::LAVA,
    blk::OBSIDIAN,
    blk::WATER,
    blk::BEDROCK,
    blk::GRASS,
    blk::DIRT,
];

impl SurfaceTable {
    /// Per RWG id: can a column painted (or, for snow layers / ice, covered by the population snow pass) under it
    /// show any of `blocks`? `cold(id)`: the biome or its river biome is cold (temperature < 0.15), which the snow
    /// pass needs. `None` if one of `blocks` can appear anywhere.
    pub fn can_paint(&self, blocks: &[u8], cold: &dyn Fn(BiomeId) -> bool) -> Option<Vec<bool>> {
        if blocks.iter().any(|b| ANYWHERE.contains(b)) {
            return None;
        }
        let snowy = blocks
            .iter()
            .any(|b| [blk::SNOW_LAYER, blk::ICE].contains(b));
        Some(
            self.by_id
                .iter()
                .enumerate()
                .map(|(id, ps)| {
                    // volcano columns can also be painted by the underlying biome: never prune them
                    Some(id as BiomeId) == self.volcano
                        || (snowy && cold(id as BiomeId))
                        || ps
                            .iter()
                            .any(|p| p.outputs().iter().any(|o| blocks.contains(o)))
                })
                .collect(),
        )
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

/// One chunk's columns after `provideChunk` (pre-population). Index `lz * 16 + lx`, `cols[idx][y]`.
pub struct SurfaceChunk {
    pub cols: Vec<[u8; 256]>,
    /// Highest non-air y (-1: none).
    pub top: [i16; 256],
}

impl SurfaceChunk {
    pub fn top_block(&self, idx: usize) -> u8 {
        let y = self.top[idx];
        if y < 0 {
            blk::AIR
        } else {
            self.cols[idx][y as usize]
        }
    }
}

const VOLCANO_LAVA_FILL_RADIUS: f32 = 37.5;

/// Paints chunk (cx, cz). `cc` is the chunk's carve ([`crate::spawn::carved_chunk`]).
pub fn surface_chunk(
    p: &BiomeProvider,
    h: &dyn Heights,
    t: &SurfaceTable,
    c: &Chunk,
    cx: i32,
    cz: i32,
    cc: &CarvedChunk,
) -> SurfaceChunk {
    use blk::*;
    let per = p.perlin();
    let mut r = JavaRandom::new(
        (cx as i64)
            .wrapping_mul(0x4f9939f508)
            .wrapping_add((cz as i64).wrapping_mul(0x1ef1565bd5)),
    );
    let mut cols = vec![[AIR; 256]; 256];
    for lz in 0..16usize {
        for lx in 0..16usize {
            let idx = lz * 16 + lx;
            let (x, z) = (cx * 16 + lx as i32, cz * 16 + lz as i32);
            let col = &mut cols[idx];
            cc.fill_pre(lx as i32, lz as i32, col, STONE, WATER, AIR);
            let id = c.rwg[idx];
            let volcano = t.volcano == Some(id);
            // riverStrength (cell noise) only for the painters that read it
            let river = if volcano
                || t.painters(id)
                    .iter()
                    .any(|pt| matches!(pt, Painter::Polar | Painter::RiverOasis))
            {
                -p.river_strength(x, z)
            } else {
                0.0
            };
            let ctx = Ctx {
                perlin: per,
                clay: &t.clay,
                i: x as f32,
                j: z as f32,
                cliff: cliff(c, lx, lz),
                river,
                height: c.height[idx],
            };
            if volcano {
                paint_volcano(p, h, t, c.volcano_depth[idx], col, &mut r, &ctx, x, z);
            } else {
                for pt in t.painters(id) {
                    pt.paint(col, &mut r, &ctx);
                }
            }
            col[0] = BEDROCK;
            for b in 2..=5 {
                col[r.next_int_bounded(b) as usize] = BEDROCK;
            }
        }
    }
    // vanilla caves (Hodgepodge MapGenCaves, after the surface pass), replayed step by step in generation order:
    // per step and column top-down, `foundTop` once the column biome's top block is reached, `digBlock` digs only
    // stone or that biome's top / filler (provider biome; desert, beach and mushroom islands count as grass / dirt;
    // painted sandstone, clay, red sand, gravel … stay; lava below y 10) and turns the filler under a dug block into
    // the top once `foundTop` is set
    {
        let mut prov: [Option<(u8, u8)>; 256] = [None; 256];
        for &[lx, lz, y_top, y_bot] in cc.cave_runs() {
            let idx = lz as usize * 16 + lx as usize;
            let col = &mut cols[idx];
            let mut found = false;
            for y in (y_bot as usize..=y_top as usize).rev() {
                let b = col[y];
                if matches!(b, AIR | WATER | LAVA) {
                    continue;
                }
                let dug = if y < 10 { LAVA } else { AIR };
                if b == STONE && !found {
                    col[y] = dug;
                    continue;
                }
                // the provider biome (a full `getBiomeGenAt`) only once the block can be something other than stone
                let (ptop, pfill) = *prov[idx].get_or_insert_with(|| {
                    let (x, z) = (cx * 16 + lx as i32, cz * 16 + lz as i32);
                    t.prov[p.biome_gen_id(x, z, h) as usize]
                });
                if b == ptop {
                    found = true;
                }
                if b == STONE || b == ptop || b == pfill {
                    col[y] = dug;
                    if found && col[y - 1] == pfill {
                        col[y - 1] = ptop;
                    }
                }
            }
        }
    }
    let mut top = [-1i16; 256];
    for idx in 0..256usize {
        let (lx, lz) = ((idx & 15) as i32, (idx >> 4) as i32);
        let (x, z) = (cx * 16 + lx, cz * 16 + lz);
        let col = &mut cols[idx];
        // volcano magma chamber + vent (after caves; the vent and its shell lie inside the volcano biome)
        if t.volcano.is_some() && t.volcano == Some(c.rwg[idx]) {
            magma_vent(p, h, col, x, z);
        }
        if let Some(l) = cc.lava_column(lx, lz) {
            for y in (0..256usize).rev() {
                let falling = y < 255 && matches!(col[y + 1], SAND | RED_SAND | GRAVEL);
                match l.block(y as i32, falling) {
                    LavaBlock::Keep => {}
                    LavaBlock::Air => col[y] = AIR,
                    LavaBlock::Lava => col[y] = LAVA,
                    LavaBlock::Obsidian => col[y] = OBSIDIAN,
                    LavaBlock::Stone => col[y] = STONE,
                }
            }
            // RWG2 2.0.9: sand / gravel resting on the funnel's air (the neck's ledge underside) becomes stone.
            if let (true, Some((ltop, surf))) = (p.table.rwg2_209, l.funnel()) {
                for y in (ltop + 1).max(0) as usize..(surf as usize).min(255) {
                    if col[y] == AIR && matches!(col[y + 1], SAND | RED_SAND | GRAVEL) {
                        col[y + 1] = STONE;
                    }
                }
            }
        }
        top[idx] = (0..256)
            .rev()
            .find(|&y| col[y] != AIR)
            .map_or(-1, |y| y as i16);
    }
    SurfaceChunk { cols, top }
}

/// `replaceBlocksForBiome`'s volcano branch.
#[allow(clippy::too_many_arguments)]
fn paint_volcano(
    p: &BiomeProvider,
    h: &dyn Heights,
    t: &SurfaceTable,
    added: u8,
    col: &mut [u8; 256],
    r: &mut JavaRandom,
    ctx: &Ctx,
    x: i32,
    z: i32,
) {
    let coords = p.volcano_coordinates(x, z, h);
    let underlying = p.volcano_underlying_biome(x, z);
    if coords != continental::NONE {
        if added > 0 || underlying.is_none() {
            Painter::VolcanoAsh {
                depth: (added as i32).max(1),
            }
            .paint(col, r, ctx);
            // lava fill
            let (lx, lz) = (continental::unpack_x(coords), continental::unpack_y(coords));
            let mut d = ((lx as f64 * lx as f64 + lz as f64 * lz as f64).sqrt()) as f32;
            d += p.perlin().noise2(lx / 18.0, lz / 18.0) * 3.0;
            if d >= VOLCANO_LAVA_FILL_RADIUS {
                return;
            }
            let Some(base) = p.volcano_base_height(x, z, h) else {
                return;
            };
            let mut s = 255usize;
            while s > 1 && matches!(col[s], blk::AIR | blk::WATER | blk::LAVA) {
                s -= 1;
            }
            let lava = 254.min(java_round(base + 18.0)) as usize;
            if s >= lava {
                return;
            }
            col[s] = blk::OBSIDIAN;
            for v in col.iter_mut().take(lava + 1).skip(s + 1) {
                *v = blk::LAVA;
            }
        } else if let Some(u) = underlying {
            for pt in t.painters(u) {
                pt.paint(col, r, ctx);
            }
        }
    } else {
        let mut actual = p.biome_data(x, z, h).data;
        if Some(actual) == t.volcano {
            actual = underlying.unwrap_or(actual);
        }
        if Some(actual) == t.volcano {
            Painter::VolcanoAsh { depth: 6 }.paint(col, r, ctx);
        } else {
            for pt in t.painters(actual) {
                pt.paint(col, r, ctx);
            }
        }
    }
}

fn java_round(v: f32) -> i32 {
    (v + 0.5).floor() as i32
}

/// `RealisticBiomeIslandVolcano.generateMagmaChamber` for one column.
fn magma_vent(p: &BiomeProvider, h: &dyn Heights, col: &mut [u8; 256], x: i32, z: i32) {
    const VENT: f32 = 5.0;
    const VENT_SHELL: f32 = 10.0;
    const CENTER: i32 = 18;
    const RADIUS: f64 = 46.0;
    const HALF: f64 = 10.0;
    const SHELL: f64 = 3.0;
    let coords = p.volcano_coordinates(x, z, h);
    if coords == continental::NONE {
        return;
    }
    let (vx, vz) = (continental::unpack_x(coords), continental::unpack_y(coords));
    let d2 = vx * vx + vz * vz;
    let Some(base) = p.volcano_base_height(x, z, h) else {
        return;
    };
    let floor = 254.min(java_round(base + 4.0));
    let (outer_r, outer_h) = (RADIUS + SHELL, HALF + SHELL);
    for y in 5.max(CENTER - outer_h as i32)..=floor {
        let dv = (y - CENTER) as f64;
        let outer = d2 as f64 / (outer_r * outer_r) + dv * dv / (outer_h * outer_h);
        let inner = d2 as f64 / (RADIUS * RADIUS) + dv * dv / (HALF * HALF);
        let above = y >= CENTER;
        let vent = above && d2 <= VENT * VENT;
        let vent_shell = above && d2 <= VENT_SHELL * VENT_SHELL;
        if inner <= 1.0 || vent {
            col[y as usize] = blk::LAVA;
        } else if outer <= 1.0 || vent_shell {
            col[y as usize] = blk::OBSIDIAN;
        }
    }
}

// ---------------------------------------------------------------- population snow / ice

/// `ChunkGeneratorRealistic.populate` snow score of the populate of chunk (cx, cz): 81 `getBiomeDataAt` samples at
/// (x + 24 + 16·i, z + 24 + 16·j), i, j ∈ −4..4, each adding 0.01234569 to its RWG id (capped at 1), then
/// −0.6·w for base temperature < 0.15, else +0.6·w, summed in id order (f32 like Java).
pub fn snow_score(p: &BiomeProvider, h: &dyn Heights, cx: i32, cz: i32) -> f32 {
    let mut bn = vec![0f32; p.table.biomes.len().max(256)];
    let (x, z) = (cx.wrapping_mul(16), cz.wrapping_mul(16));
    for bx in -4..=4 {
        for by in -4..=4 {
            let id = p.biome_data(x + 24 + bx * 16, z + 24 + by * 16, h).data as usize;
            if let Some(v) = bn.get_mut(id) {
                *v += 0.012_345_69f32;
            }
        }
    }
    let mut snow = 0f32;
    for (id, &w) in bn.iter().enumerate() {
        if w > 0.0 {
            let w = if w >= 1.0 { 1.0 } else { w };
            let t = p.table.biomes.get(id).map_or(1.0, |b| b.temperature);
            if t < 0.15 {
                snow -= 0.6 * w;
            } else {
                snow += 0.6 * w;
            }
        }
    }
    snow
}

/// RWG2's populate snow pass over the columns of chunk (cx, cz) (each column is decided by the populate of chunk
/// ((x − 8) >> 4, (z − 8) >> 4)): where the score is < 0.59, the column's chunk biome is cold (temperature < 0.15)
/// and `s` = (score < −0.59 ? −1 : perlin(x/3, z/3) + score) < 0, water at the precipitation height freezes to ice
/// and opaque ground above y 62 gets a snow layer. Trees are not modelled (in game the layer sits on their leaves).
/// `temp`: base temperature of a chunk-biome (gen) id; `score(cx, cz)`: [`snow_score`] (callers may cache it).
/// Returns the number of columns changed.
pub fn apply_snow_cover_with(
    p: &BiomeProvider,
    temp: &dyn Fn(u16) -> Option<f32>,
    score: &mut dyn FnMut(i32, i32) -> f32,
    c: &Chunk,
    sc: &mut SurfaceChunk,
    cx: i32,
    cz: i32,
) -> u32 {
    use blk::*;
    let mut scores = [[f32::NAN; 2]; 2];
    let per = p.perlin();
    let mut changed = 0;
    for idx in 0..256usize {
        // cheap test first: most chunks have no cold column
        if temp(c.gen[idx]).map_or(true, |t| t >= 0.15) {
            continue;
        }
        let (lx, lz) = ((idx & 15) as i32, (idx >> 4) as i32);
        let (sx, sz) = ((lx >= 8) as usize, (lz >= 8) as usize);
        if scores[sx][sz].is_nan() {
            scores[sx][sz] = score(cx - 1 + sx as i32, cz - 1 + sz as i32);
        }
        let snow = scores[sx][sz];
        if snow >= 0.59 {
            continue;
        }
        let (x, z) = (cx * 16 + lx, cz * 16 + lz);
        let s = if snow < -0.59 {
            -1.0
        } else {
            per.noise2(x as f32 / 3.0, z as f32 / 3.0) + snow
        };
        if s >= 0.0 {
            continue;
        }
        let top = sc.top[idx] as i32;
        if top < 0 {
            continue;
        }
        let col = &mut sc.cols[idx];
        // precipitation height: above the highest block that blocks movement or is liquid (a snow layer doesn't)
        let sn3 = if col[top as usize] == SNOW_LAYER {
            top
        } else {
            top + 1
        };
        if !(1..256).contains(&sn3) {
            continue;
        }
        let b1 = col[sn3 as usize];
        let b2 = col[sn3 as usize - 1];
        if b2 == WATER {
            col[sn3 as usize - 1] = ICE;
            changed += 1;
            continue;
        }
        let opaque = !matches!(b2, AIR | WATER | LAVA | ICE | PACKED_ICE | SNOW_LAYER);
        if opaque && sn3 > 62 && b1 != SNOW_LAYER {
            col[sn3 as usize] = SNOW_LAYER;
            sc.top[idx] = sn3 as i16;
            changed += 1;
        }
    }
    changed
}

/// [`apply_snow_cover_with`] with uncached [`snow_score`]s.
pub fn apply_snow_cover(
    p: &BiomeProvider,
    h: &dyn Heights,
    temp: &dyn Fn(u16) -> Option<f32>,
    c: &Chunk,
    sc: &mut SurfaceChunk,
    cx: i32,
    cz: i32,
) -> u32 {
    apply_snow_cover_with(p, temp, &mut |x, z| snow_score(p, h, x, z), c, sc, cx, cz)
}
