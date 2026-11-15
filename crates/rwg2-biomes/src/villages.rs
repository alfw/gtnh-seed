//! Villages in GTNH 2.9.0: VillageNames 4.5.21-GTNH `MapGenVillageVN` (replaces RWG2's `MapGenVillage`).
//!
//! RWG2's `ChunkGeneratorRealistic` builds `new MapGenVillage({size:0, distance:24})` and passes it through
//! `TerrainGen.getModdedMapGen(…, VILLAGE)`. VillageNames' `InitMapGenEvent` handler (`newVillageGenerator = true`
//! in GTNH) replaces it with `new MapGenVillageVN()` — the **no-arg** constructor, so the `{size, distance}` map
//! is dropped and spacing comes from VN's config (`config/VillageNames4/newvillages/village_gen.cfg`, GTNH 2.9.0):
//! `Village Spacing: Median = 40`, `Range = 10` ⇒ max distance `g = 50`, min `h = 30`; village-free radius 0;
//! size min = max = mode = 1, normal order 1.
//!
//! `canSpawnStructureAtCoords(cx, cz)`:
//! ```text
//! if cx² + cz² < radius² → false                       // radius 0
//! rx = floorDiv(cx, 50); rz = floorDiv(cz, 50)          // (c < 0 ? c - 49 : c) / 50
//! Random r = world.setRandomSeed(rx, rz, 10387312)      // rx·341873128712 + rz·132897987541 + seed + salt
//! candidate = (rx·50 + r.nextInt(20), rz·50 + r.nextInt(20))
//! (cx, cz) == candidate && world.getBiomeGenForCoords(cx·16+8, cz·16+8).biomeName ∈ "Spawn Biome Names"
//! ```
//! **VN never calls `areBiomesViable`**, so RWG2's terrain-based override (25 `getNoiseAt` ≥ 63, spread < 8) is
//! not used for villages. The only gate is the biome *name* list.
//!
//! `World.getBiomeGenForCoords` reads the chunk biome array if the chunk exists, else
//! `ChunkManagerRealistic.getBiomeGenAt` (= [`BiomeProvider::biome_gen_id`], lava-cave marker included). The
//! candidate is first evaluated while its chunk does not exist (provider biome). A failed check stores nothing, so
//! it is retried each time a chunk within 8 is generated; once the candidate chunk itself exists, the *dithered*
//! chunk biome at local (8, 8) is used ⇒ a second chance ([`Gate::ChunkBiome`], usually a partial village since
//! some of its chunks were populated before the start existed).
//!
//! Style: `Start` samples `getBiomeGenAt(cx·16+2, cz·16+2)`; config village type if set ("Heathland|savanna"),
//! else `getVillageTypeFromBiome` (name contains taiga / CONIFEROUS → taiga; savanna; SNOWY ∧ WASTELAND without
//! DENSE/FOREST/LUSH/SPARSE → snowy; desert / SANDY; jungle / JUNGLE; swamp / SWAMP; else plains).
//!
//! Sizeable (`> 2` non-road components): needs VN's piece generator (~100k Java lines of biome structure
//! classes, all constructed per start and drawing from the structure Random) — not ported. Every start in the
//! ground truth has 37–68 components, so `sizeable` is reported as unknown (`None`) and is practically always true.

use crate::chunkgen::ChunkGen;
use crate::java_random::JavaRandom;
use crate::provider::{BiomeProvider, Heights};

/// `newVillageSpacingMedian + newVillageSpacingSpread` (GTNH 40 + 10).
pub const SPACING: i32 = 50;
/// `newVillageSpacingMedian - newVillageSpacingSpread` (GTNH 40 − 10).
pub const SEPARATION: i32 = 30;
pub const SALT: i64 = 10387312;
/// `Village Spacing: Village-Free Radius` (chunks).
pub const NO_VILLAGES_RADIUS: i32 = 0;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Style {
    Plains,
    Desert,
    Taiga,
    Savanna,
    Snowy,
    Jungle,
    Swamp,
}

impl Style {
    pub fn name(self) -> &'static str {
        match self {
            Style::Plains => "plains",
            Style::Desert => "desert",
            Style::Taiga => "taiga",
            Style::Savanna => "savanna",
            Style::Snowy => "snowy",
            Style::Jungle => "jungle",
            Style::Swamp => "swamp",
        }
    }
    pub fn index(self) -> u8 {
        self as u8
    }
    #[cfg_attr(not(feature = "json"), allow(dead_code))]
    fn from_config(s: &str) -> Option<Style> {
        Some(match s.to_ascii_uppercase().as_str() {
            "PLAINS" => Style::Plains,
            "DESERT" => Style::Desert,
            "TAIGA" => Style::Taiga,
            "SAVANNA" => Style::Savanna,
            "SNOWY" => Style::Snowy,
            "JUNGLE" => Style::Jungle,
            "SWAMP" => Style::Swamp,
            _ => return None,
        })
    }
}

/// Which `getBiomeGenForCoords` path lets the village spawn.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Gate {
    /// Provider biome (`getBiomeGenAt`) at the chunk centre is in the list: the start is created as soon as any
    /// chunk within 8 of it generates. This is what a fresh world (and the dump) sees.
    Provider,
    /// Provider biome rejected, but the dithered chunk biome at local (8, 8) is allowed: the start appears only
    /// after the candidate chunk itself exists and another chunk within 8 generates (likely partial village).
    ChunkBiome,
    /// Neither: no village.
    Rejected,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Village {
    pub cx: i32,
    pub cz: i32,
    /// Chunk centre (the town-centre piece starts at `cx·16+2` and is ~8–15 blocks wide).
    pub x: i32,
    pub z: i32,
    /// From the biome at `(cx·16+2, cz·16+2)` (what VN uses).
    pub style: Style,
    /// Provider biome (gen id) at the chunk centre.
    pub biome: u16,
    /// Provider gate (exact): the village exists in a freshly generated world.
    pub viable_exact: bool,
    /// Full gate incl. the dithered chunk-biome second chance (`None` if not evaluated).
    pub gate: Option<Gate>,
    /// `isSizeableStructure` (> 2 non-road pieces): not ported (`None`); every observed start is sizeable.
    pub sizeable: Option<bool>,
}

/// Biome name list and village type per in-game biome id.
#[derive(Clone, Debug)]
pub struct VillageBiomes {
    pub allowed: Vec<bool>,
    pub style: Vec<Style>,
}

/// GTNH 2.9.0 `Spawn Biome Names` (VillageNames4/newvillages/village_gen.cfg), verbatim order.
pub const GTNH_290_SPAWN_BIOME_NAMES: &[&str] = &[
    "Plains",
    "Desert",
    "Forest",
    "Taiga",
    "Swampland",
    "Ice Plains",
    "MushroomIsland",
    "Jungle",
    "JungleEdge",
    "Birch Forest",
    "Roofed Forest",
    "Cold Taiga",
    "Mega Taiga",
    "Savanna",
    "Mesa",
    "Sunflower Plains",
    "Desert M",
    "Flower Forest",
    "Jungle Edge",
    "Birch Forest M",
    "Mega Spruce Taiga",
    "Bamboo Forest",
    "Bayou",
    "Bog",
    "Boreal Forest",
    "Chaparral",
    "Cherry Blossom Grove",
    "Coniferous Forest",
    "Snowy Coniferous Forest",
    "Deciduous Forest",
    "Dense Forest",
    "Eucalyptus Forest",
    "Flower Field",
    "Flower Island",
    "Frost Forest",
    "Fungi Forest",
    "Garden",
    "Grassland",
    "Grove",
    "Heathland|savanna|oak",
    "Highland",
    "Lavender Fields",
    "Lush Swamp",
    "Maple Woods",
    "Meadow",
    "Mountain",
    "Mystic Grove",
    "Orchard",
    "Outback",
    "Prairie",
    "Rainforest",
    "Redwood Forest",
    "RedwoodForest",
    "Sacred Springs",
    "Seasonal Forest",
    "Shield",
    "Shrubland",
    "Steppe",
    "Temperate Rainforest",
    "Tropical Island",
    "Tropical Rainforest",
    "Tundra",
    "Wetland",
    "Woodland",
    "Xeric Shrubland",
    "Meadow Forest",
    "Oasis",
    "Scrubland",
    "Seasonal Forest Clearing",
    "Spruce Woods",
    "Mountain Foothills",
    "MountainFoothills",
    "Rocky Steppe",
    "Autumn Woods",
    "Mini Jungle",
    "Mountain Taiga",
    "Pine Forest",
    "Redwood Lush",
    "Lush Redwoods",
    "Snowy Forest",
    "Snow Forest",
    "Snowy Rainforest",
    "Woodlands",
    "Autumn Forest",
    "Badlands",
    "Highlands",
    "Lowlands",
    "Outback",
    "Pinelands",
    "Sahel",
    "Tall Pine Forest",
    "Tropics",
    "BiomeAppleTree",
    "BiomeBarley",
    "BiomeCedarForest",
    "BiomeFertileForest",
    "BiomeFlower",
    "BiomeFlowerPall",
    "BiomeForestFX",
    "BiomeLavender",
    "BiomeMagic",
    "BiomeOasis",
    "BiomePineForest",
    "BiomeRichForest",
    "BiomeSnowmountain",
    "BiomeTallGrass",
];

/// Biome ids whose name is in [`GTNH_290_SPAWN_BIOME_NAMES`] (derived from `data/gtnh-2.9.0/climate.json`; the
/// test re-derives it).
const GTNH_290_ALLOWED: &[u16] = &[
    1, 2, 4, 5, 6, 12, 14, 21, 23, 27, 29, 30, 32, 35, 37, 43, 44, 45, 47, 51, 52, 53, 59, 61, 63,
    64, 65, 66, 68, 69, 70, 71, 76, 79, 81, 83, 84, 86, 87, 88, 90, 92, 95, 97, 98, 99, 100, 101,
    102, 103, 104, 107, 109, 110, 111, 113, 114, 115, 120, 121, 122, 123, 129, 130, 132, 155, 160,
    196, 225, 232, 235, 313, 320, 323,
];

/// Non-plains village types per biome id (everything else is plains).
const GTNH_290_STYLES: &[(u16, Style)] = {
    use Style::*;
    &[
        (2, Desert),
        (5, Taiga),
        (6, Swamp),
        (12, Snowy),
        (17, Desert),
        (19, Taiga),
        (21, Jungle),
        (22, Jungle),
        (23, Jungle),
        (30, Taiga),
        (31, Taiga),
        (32, Taiga),
        (33, Taiga),
        (35, Savanna),
        (36, Savanna),
        (37, Desert),
        (38, Desert),
        (39, Desert),
        (42, Snowy),
        (43, Jungle),
        (44, Swamp),
        (45, Swamp),
        (47, Taiga),
        (48, Savanna),
        (49, Desert),
        (50, Desert),
        (53, Taiga),
        (58, Swamp),
        (60, Desert),
        (62, Swamp),
        (65, Swamp),
        (68, Swamp),
        (70, Savanna),
        (75, Swamp),
        (77, Savanna),
        (79, Swamp),
        (82, Swamp),
        (85, Swamp),
        (88, Desert),
        (89, Swamp),
        (92, Savanna),
        (94, Savanna),
        (96, Swamp),
        (97, Jungle),
        (98, Taiga),
        (100, Savanna),
        (103, Taiga),
        (105, Swamp),
        (106, Swamp),
        (107, Taiga),
        (108, Savanna),
        (109, Taiga),
        (110, Savanna),
        (111, Taiga),
        (113, Jungle),
        (114, Jungle),
        (120, Swamp),
        (122, Desert),
        (127, Desert),
        (130, Desert),
        (133, Taiga),
        (134, Swamp),
        (136, Desert),
        (138, Jungle),
        (142, Desert),
        (149, Jungle),
        (151, Jungle),
        (158, Taiga),
        (160, Taiga),
        (163, Savanna),
        (164, Savanna),
        (165, Desert),
        (166, Desert),
        (167, Desert),
        (175, Swamp),
        (176, Snowy),
        (177, Taiga),
        (182, Savanna),
        (185, Swamp),
        (196, Taiga),
        (202, Taiga),
        (207, Desert),
        (209, Jungle),
        (211, Jungle),
        (214, Taiga),
        (218, Desert),
        (223, Jungle),
        (224, Snowy),
        (225, Taiga),
        (227, Taiga),
        (228, Savanna),
        (229, Savanna),
        (230, Desert),
        (232, Jungle),
        (233, Taiga),
        (234, Jungle),
        (237, Jungle),
        (301, Taiga),
        (303, Desert),
        (304, Jungle),
        (305, Jungle),
        (307, Taiga),
        (309, Desert),
        (310, Jungle),
        (311, Jungle),
        (312, Snowy),
        (313, Taiga),
        (315, Taiga),
        (316, Savanna),
        (317, Savanna),
        (318, Desert),
        (320, Jungle),
        (321, Taiga),
        (322, Jungle),
        (325, Jungle),
        (326, Jungle),
        (327, Jungle),
        (328, Jungle),
        (329, Jungle),
    ]
};

impl VillageBiomes {
    /// Built-in GTNH 2.9.0 table (no data files needed; wasm-friendly).
    pub fn gtnh_2_9_0() -> Self {
        let n = 330usize;
        let mut allowed = vec![false; n];
        let mut style = vec![Style::Plains; n];
        for &id in GTNH_290_ALLOWED {
            allowed[id as usize] = true;
        }
        for &(id, s) in GTNH_290_STYLES {
            style[id as usize] = s;
        }
        VillageBiomes { allowed, style }
    }

    #[inline]
    pub fn is_allowed(&self, id: u16) -> bool {
        self.allowed.get(id as usize).copied().unwrap_or(false)
    }

    #[inline]
    pub fn style_of(&self, id: u16) -> Style {
        self.style
            .get(id as usize)
            .copied()
            .unwrap_or(Style::Plains)
    }

    /// `getVillageTypeFromBiome(biome)` from the biome name and its BiomeDictionary types.
    pub fn style_from_biome(name: &str, dict: &[String]) -> Style {
        let n = name.to_lowercase();
        let has = |t: &str| dict.iter().any(|d| d == t);
        if n.contains("taiga") || has("CONIFEROUS") {
            return Style::Taiga;
        }
        if n.contains("savanna") || has("SAVANNA") {
            return Style::Savanna;
        }
        let (mut snowy, mut waste) = (false, false);
        for d in dict {
            match d.as_str() {
                "SNOWY" => snowy = true,
                "WASTELAND" => waste = true,
                "DENSE" | "FOREST" | "LUSH" | "SPARSE" => {
                    snowy = false;
                    waste = false;
                    break;
                }
                _ => {}
            }
        }
        if snowy && waste {
            return Style::Snowy;
        }
        if n.contains("desert") || has("SANDY") {
            return Style::Desert;
        }
        if n.contains("jungle") || has("JUNGLE") {
            return Style::Jungle;
        }
        if n.contains("swamp") || has("SWAMP") {
            return Style::Swamp;
        }
        Style::Plains
    }

    /// Derive the table from `climate.json` (`biomes[].{id, name, dict}`) and a `Spawn Biome Names` list
    /// (`name|villageType|materialType|nosub`).
    #[cfg(feature = "json")]
    pub fn from_climate_json(s: &str, spawn_names: &[&str]) -> Result<Self, String> {
        let v: serde_json::Value = serde_json::from_str(s).map_err(|e| e.to_string())?;
        let arr = v["biomes"].as_array().ok_or("biomes missing")?;
        let entries: Vec<(String, String)> = spawn_names
            .iter()
            .filter_map(|e| {
                let mut it = e.split('|');
                let name = it.next().unwrap_or("").trim().to_string();
                let vt = it.next().unwrap_or("").trim().to_string();
                (!name.is_empty()).then_some((name, vt))
            })
            .collect();
        let n = arr
            .iter()
            .filter_map(|b| b["id"].as_u64())
            .max()
            .unwrap_or(0) as usize
            + 1;
        let mut allowed = vec![false; n];
        let mut style = vec![Style::Plains; n];
        for b in arr {
            let id = b["id"].as_u64().ok_or("id")? as usize;
            let name = b["name"].as_str().unwrap_or("");
            let dict: Vec<String> = b["dict"]
                .as_array()
                .map(|a| {
                    a.iter()
                        .filter_map(|t| t.as_str().map(String::from))
                        .collect()
                })
                .unwrap_or_default();
            // `indexOf` → first entry with that name
            let entry = entries.iter().find(|(nm, _)| nm == name);
            allowed[id] = entry.is_some();
            style[id] =
                match entry.and_then(|(_, vt)| (!vt.is_empty()).then(|| Style::from_config(vt))) {
                    Some(Some(s)) => s,
                    Some(None) => Style::Plains, // getVillageTypeFromName(unknown, PLAINS)
                    None => Self::style_from_biome(name, &dict),
                };
        }
        Ok(VillageBiomes { allowed, style })
    }
}

/// `World.setRandomSeed(a, b, salt)`.
fn world_random(seed: i64, a: i32, b: i32) -> JavaRandom {
    JavaRandom::new(
        (a as i64)
            .wrapping_mul(341873128712)
            .wrapping_add((b as i64).wrapping_mul(132897987541))
            .wrapping_add(seed)
            .wrapping_add(SALT),
    )
}

/// Candidate start chunk of region `(rx, rz)` (regions are `SPACING` chunks wide).
#[inline]
pub fn region_candidate(seed: i64, rx: i32, rz: i32) -> (i32, i32) {
    let mut r = world_random(seed, rx, rz);
    let ox = r.next_int_bounded(SPACING - SEPARATION);
    let oz = r.next_int_bounded(SPACING - SEPARATION);
    (rx.wrapping_mul(SPACING) + ox, rz.wrapping_mul(SPACING) + oz)
}

/// Position half of `canSpawnStructureAtCoords` (no biome test).
pub fn is_candidate(seed: i64, cx: i32, cz: i32) -> bool {
    let r2 = NO_VILLAGES_RADIUS as i64 * NO_VILLAGES_RADIUS as i64;
    if (cx as i64) * (cx as i64) + (cz as i64) * (cz as i64) < r2 {
        return false;
    }
    region_candidate(seed, cx.div_euclid(SPACING), cz.div_euclid(SPACING)) == (cx, cz)
}

/// Evaluates village candidates. Holds the biome table and (lazily) a chunk generator for the chunk-biome gate.
pub struct Villages {
    pub biomes: VillageBiomes,
    chunkgen: Option<ChunkGen>,
}

impl Villages {
    pub fn new(biomes: VillageBiomes) -> Self {
        Villages {
            biomes,
            chunkgen: None,
        }
    }

    pub fn gtnh_2_9_0() -> Self {
        Self::new(VillageBiomes::gtnh_2_9_0())
    }

    /// Provider gate + style for one candidate chunk (cheap: two `biome_gen_id` samples).
    pub fn evaluate(&self, p: &BiomeProvider, h: &dyn Heights, cx: i32, cz: i32) -> Village {
        let (x, z) = (cx.wrapping_mul(16) + 8, cz.wrapping_mul(16) + 8);
        let biome = p.biome_gen_id(x, z, h);
        let style_biome = p.biome_gen_id(cx.wrapping_mul(16) + 2, cz.wrapping_mul(16) + 2, h);
        let viable = self.biomes.is_allowed(biome);
        Village {
            cx,
            cz,
            x,
            z,
            style: self.biomes.style_of(style_biome),
            biome,
            viable_exact: viable,
            gate: viable.then_some(Gate::Provider),
            sizeable: None,
        }
    }

    /// Fills `gate` for a provider-rejected candidate with the dithered chunk biome at local (8, 8)
    /// (one chunk generation, ~1 ms). No-op if the table has no height models.
    pub fn resolve_chunk_gate(&mut self, p: &BiomeProvider, h: &dyn Heights, v: &mut Village) {
        if v.gate.is_some() {
            return;
        }
        let cg = self.chunkgen.get_or_insert_with(|| ChunkGen::new(p));
        if let Some(ch) = cg.generate(p, h, v.cx, v.cz) {
            let id = ch.gen[8 * 16 + 8];
            v.gate = Some(if self.biomes.is_allowed(id) {
                Gate::ChunkBiome
            } else {
                Gate::Rejected
            });
        }
    }

    /// Every candidate start chunk in the chunk rectangle `[cx0, cx1] × [cz0, cz1]` (inclusive), viable or not,
    /// provider gate only (`gate` = `Some(Provider)` or `None`).
    #[allow(clippy::too_many_arguments)]
    pub fn in_rect(
        &self,
        p: &BiomeProvider,
        h: &dyn Heights,
        seed: i64,
        cx0: i32,
        cz0: i32,
        cx1: i32,
        cz1: i32,
    ) -> Vec<Village> {
        let mut out = Vec::new();
        for rz in cz0.div_euclid(SPACING)..=cz1.div_euclid(SPACING) {
            for rx in cx0.div_euclid(SPACING)..=cx1.div_euclid(SPACING) {
                let (cx, cz) = region_candidate(seed, rx, rz);
                if cx < cx0 || cx > cx1 || cz < cz0 || cz > cz1 || !is_candidate(seed, cx, cz) {
                    continue;
                }
                out.push(self.evaluate(p, h, cx, cz));
            }
        }
        out
    }

    /// Nearest provider-viable village whose chunk centre is within `radius` blocks of `(x, z)`.
    /// Returns the village and its distance². Cost: ~(2R/800 + 1)² regions, two biome samples each.
    pub fn nearest_within(
        &self,
        p: &BiomeProvider,
        h: &dyn Heights,
        seed: i64,
        x: i32,
        z: i32,
        radius: i32,
    ) -> Option<(Village, i64)> {
        self.nearest_within_where(p, h, seed, x, z, radius, &mut |_, _| true)
    }

    /// [`Villages::nearest_within`], skipping villages `accept` (block x, z of the village) refuses.
    #[allow(clippy::too_many_arguments)]
    pub fn nearest_within_where(
        &self,
        p: &BiomeProvider,
        h: &dyn Heights,
        seed: i64,
        x: i32,
        z: i32,
        radius: i32,
        accept: &mut dyn FnMut(i32, i32) -> bool,
    ) -> Option<(Village, i64)> {
        let r = radius.max(0) as i64;
        let r2 = r * r;
        let cx0 = ((x as i64 - r - 8) >> 4) as i32;
        let cx1 = ((x as i64 + r - 8) >> 4) as i32 + 1;
        let cz0 = ((z as i64 - r - 8) >> 4) as i32;
        let cz1 = ((z as i64 + r - 8) >> 4) as i32 + 1;
        let mut cands: Vec<(i64, i32, i32)> = Vec::new();
        for rz in cz0.div_euclid(SPACING)..=cz1.div_euclid(SPACING) {
            for rx in cx0.div_euclid(SPACING)..=cx1.div_euclid(SPACING) {
                let (cx, cz) = region_candidate(seed, rx, rz);
                if !is_candidate(seed, cx, cz) {
                    continue;
                }
                let dx = (cx as i64) * 16 + 8 - x as i64;
                let dz = (cz as i64) * 16 + 8 - z as i64;
                let d2 = dx * dx + dz * dz;
                if d2 <= r2 {
                    cands.push((d2, cx, cz));
                }
            }
        }
        cands.sort_unstable();
        for (d2, cx, cz) in cands {
            let v = self.evaluate(p, h, cx, cz);
            if v.viable_exact && accept(v.x, v.z) {
                return Some((v, d2));
            }
        }
        None
    }
}

/// Convenience wrapper with the GTNH 2.9.0 table: all candidates in a chunk rectangle (provider gate).
pub fn villages_in_rect(
    p: &BiomeProvider,
    h: &dyn Heights,
    seed: i64,
    cx0: i32,
    cz0: i32,
    cx1: i32,
    cz1: i32,
) -> Vec<Village> {
    Villages::gtnh_2_9_0().in_rect(p, h, seed, cx0, cz0, cx1, cz1)
}

/// Convenience wrapper with the GTNH 2.9.0 table: nearest provider-viable village within `radius` blocks.
pub fn nearest_village_within(
    p: &BiomeProvider,
    h: &dyn Heights,
    seed: i64,
    x: i32,
    z: i32,
    radius: i32,
) -> Option<(Village, i64)> {
    Villages::gtnh_2_9_0().nearest_within(p, h, seed, x, z, radius)
}

/// [`nearest_village_within`] with a filter on the village position.
#[allow(clippy::too_many_arguments)]
pub fn nearest_village_within_where(
    p: &BiomeProvider,
    h: &dyn Heights,
    seed: i64,
    x: i32,
    z: i32,
    radius: i32,
    accept: &mut dyn FnMut(i32, i32) -> bool,
) -> Option<(Village, i64)> {
    Villages::gtnh_2_9_0().nearest_within_where(p, h, seed, x, z, radius, accept)
}
