//! Build a [`BiomeTable`] from `rwg2dump`'s `biomes.json` (feature `json`).

use serde_json::Value;

use crate::provider::{BiomeId, BiomeInfo, BiomeTable, PerClimate};
use crate::terrain::{HeightModel, Terrain};

#[derive(Debug)]
pub struct TableError(pub String);

impl std::fmt::Display for TableError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "biomes.json: {}", self.0)
    }
}
impl std::error::Error for TableError {}

fn err<T>(m: impl Into<String>) -> Result<T, TableError> {
    Err(TableError(m.into()))
}

impl BiomeTable {
    pub fn from_rwg2dump_json(text: &str) -> Result<Self, TableError> {
        let v: Value = serde_json::from_str(text).map_err(|e| TableError(e.to_string()))?;
        let rb = v["realisticBiomes"]
            .as_array()
            .ok_or(TableError("realisticBiomes".into()))?;
        let max_id = rb
            .iter()
            .filter_map(|b| b["rwgId"].as_u64())
            .max()
            .unwrap_or(0) as usize;
        let mut biomes = vec![BiomeInfo::default(); max_id + 1];
        let mut by_class = std::collections::HashMap::new();
        for b in rb {
            let id = b["rwgId"].as_u64().unwrap() as usize;
            let base = &b["base"];
            let class = b["class"].as_str().unwrap_or("").to_string();
            let name = b["displayName"]
                .as_str()
                .or(base["name"].as_str())
                .unwrap_or("")
                .to_string();
            biomes[id] = BiomeInfo {
                rwg_id: id as BiomeId,
                gen_id: base["id"].as_u64().unwrap_or(0) as u16,
                river_gen_id: b["river"]["id"]
                    .as_u64()
                    .or(base["id"].as_u64())
                    .unwrap_or(0) as u16,
                temperature: base["temperature"].as_f64().unwrap_or(0.0) as f32,
                river_temperature: b["river"]["temperature"]
                    .as_f64()
                    .or(base["temperature"].as_f64())
                    .unwrap_or(0.0) as f32,
                base_class: base["class"].as_str().unwrap_or("").to_string(),
                name,
                color: base["color"].as_i64().unwrap_or(0) as u32,
                class: class.clone(),
            };
            by_class
                .entry(class.rsplit('.').next().unwrap_or("").to_string())
                .or_insert(id as BiomeId);
        }
        let pl = &v["providerLists"];
        let ids = |x: &Value| -> Result<Vec<BiomeId>, TableError> {
            x.as_array()
                .ok_or(TableError("list".into()))?
                .iter()
                .map(|i| {
                    i.as_u64()
                        .map(|i| i as BiomeId)
                        .ok_or(TableError("null in list".into()))
                })
                .collect()
        };
        let per = |name: &str| -> Result<PerClimate, TableError> {
            let a = pl[name]
                .as_array()
                .ok_or(TableError(format!("providerLists.{name}")))?;
            if a.len() != 4 {
                return err(format!("{name}: expected 4 climates"));
            }
            Ok([ids(&a[0])?, ids(&a[1])?, ids(&a[2])?, ids(&a[3])?])
        };
        let ss = &v["supportStatics"];
        let st = |k: &str| -> Result<BiomeId, TableError> {
            ss[k]
                .as_u64()
                .map(|i| i as BiomeId)
                .ok_or(TableError(format!("supportStatics.{k}")))
        };
        let core = [
            ids(&pl["biomes_snow"])?,
            ids(&pl["biomes_cold"])?,
            ids(&pl["biomes_hot"])?,
            ids(&pl["biomes_wet"])?,
        ];
        let coast = |cls: &str| {
            by_class
                .get(cls)
                .copied()
                .ok_or(TableError(format!("no {cls}")))
        };
        let volcano_island = ss["volcanoIsland"]
            .as_u64()
            .map(|i| i as BiomeId)
            .filter(|&i| {
                biomes[i as usize]
                    .class
                    .ends_with("RealisticBiomeIslandVolcano")
            });
        Ok(BiomeTable {
            core,
            border: per("borderBiomes")?,
            cold_border: per("coldBorderBiomes")?,
            hot_border: per("hotBorderBiomes")?,
            very_cold_border: per("veryColdBorderBiomes")?,
            very_hot_border: per("veryHotBorderBiomes")?,
            littoral: per("littoralBiomes")?,
            small: per("smallBiomes")?,
            island: per("islandBiomes")?,
            small_island: per("smallIslandBiomes")?,
            large_island: per("largeIslandBiomes")?,
            wet_enabled: pl["wetEnabled"]
                .as_bool()
                .ok_or(TableError("wetEnabled".into()))?,
            coast_ice: coast("RealisticBiomeCoastIce")?,
            coast_dunes: coast("RealisticBiomeCoastDunes")?,
            ocean_shallow: [
                st("oceanShallowSnow")?,
                st("oceanShallowCold")?,
                st("oceanShallowHot")?,
                st("oceanShallowWet")?,
            ],
            ocean_deep: [
                st("oceanDeepSnow")?,
                st("oceanDeepCold")?,
                st("oceanDeepHot")?,
                st("oceanDeepWet")?,
            ],
            ocean_shallow_kelp: st("oceanShallowKelp")?,
            ocean_shallow_coral: st("oceanShallowCoral")?,
            volcano_island,
            lava_cave_marker_gen_id: ss["lavaCaveMarkerBiome"]["id"].as_u64().map(|i| i as u16),
            heights: height_models(rb, biomes.len())?,
            biomes,
        })
    }
}

/// A float field written by rwg2dump v2 as `{"v":..,"bits":"hex"}` (exact) or a plain number.
fn f32_field(fields: &Value, name: &str) -> Result<f32, TableError> {
    let v = &fields[name];
    if let Some(b) = v["bits"].as_str() {
        return u32::from_str_radix(b, 16)
            .map(f32::from_bits)
            .map_err(|e| TableError(format!("{name}: {e}")));
    }
    v.as_f64()
        .map(|f| f as f32)
        .ok_or(TableError(format!("missing float field {name}")))
}

fn terrain_from(t: &Value) -> Result<Terrain, TableError> {
    let cls = t["class"].as_str().unwrap_or("");
    let f = &t["fields"];
    let simple = cls.rsplit('.').next().unwrap_or("");
    Ok(match simple {
        "TerrainBase" => Terrain::Base,
        "TerrainFlatLakes" => Terrain::FlatLakes,
        "TerrainSmallSupport" => Terrain::SmallSupport,
        "TerrainPolar" => Terrain::Polar,
        "TerrainMarsh" => Terrain::Marsh,
        "TerrainSwampRiver" => Terrain::SwampRiver,
        "TerrainDuneValley" => Terrain::DuneValley {
            valley: f32_field(f, "valley")?,
        },
        "TerrainGrasslandFlats" => Terrain::GrasslandFlats,
        "TerrainDunes" => Terrain::Dunes,
        "TerrainSmallIsland" => Terrain::SmallIsland,
        "TerrainGrasslandMountains" => Terrain::GrasslandMountains,
        "TerrainMountain" => Terrain::Mountain,
        "TerrainMountainSpikes" => Terrain::MountainSpikes,
        "TerrainHighland" => Terrain::Highland {
            start: f32_field(f, "start")?,
            height: f32_field(f, "height")?,
            base: f32_field(f, "base")?,
            width: f32_field(f, "width")?,
            flat_detail_strength: f32_field(f, "flatDetailStrength")?,
        },
        "TerrainMountainRiver" => Terrain::MountainRiver,
        "TerrainSwampMountain" => Terrain::SwampMountain {
            height: f32_field(f, "heigth")?,
            width: f32_field(f, "width")?,
        },
        "TerrainMesa" => Terrain::Mesa,
        "TerrainHilly" => Terrain::Hilly {
            width: f32_field(f, "width")?,
            strength: f32_field(f, "strength")?,
            lake_depth: f32_field(f, "lakeDepth")?,
            lake_width: f32_field(f, "lakeWidth")?,
            terrain_height: f32_field(f, "terrainHeight")?,
        },
        "TerrainGrasslandHills" => Terrain::GrasslandHills {
            h_height: f32_field(f, "hHeight")?,
            h_width: f32_field(f, "hWidth")?,
            v_height: f32_field(f, "vHeight")?,
            v_width: f32_field(f, "vWidth")?,
            l_height: f32_field(f, "lHeight")?,
            l_width: f32_field(f, "lWidth")?,
            b_height: f32_field(f, "bHeight")?,
        },
        "TerrainCanyon" => Terrain::Canyon {
            small_river: f["smallRiver"]
                .as_bool()
                .ok_or(TableError("smallRiver".into()))?,
            height: f["height"]
                .as_array()
                .ok_or(TableError("canyon height".into()))?
                .iter()
                .map(|v| match v["bits"].as_str() {
                    Some(b) => u32::from_str_radix(b, 16)
                        .map(f32::from_bits)
                        .map_err(|e| TableError(e.to_string())),
                    None => v
                        .as_f64()
                        .map(|x| x as f32)
                        .ok_or(TableError("canyon height value".into())),
                })
                .collect::<Result<_, _>>()?,
            strength: f32_field(f, "strength")?,
            base: f32_field(f, "base")?,
        },
        other => return err(format!("unknown terrain class {other}")),
    })
}

/// Height model per RWG id. Inline `rNoise` overrides are keyed by biome class; otherwise the biome's terrain
/// object; otherwise `RealisticBiomeBase`'s constant 63. Empty when the dump has no terrain info (v1).
fn height_models(rb: &[Value], len: usize) -> Result<Vec<HeightModel>, TableError> {
    if !rb.iter().any(|b| b.get("terrain").is_some()) {
        return Ok(Vec::new());
    }
    let mut out = vec![HeightModel::Const(63.0); len];
    for b in rb {
        let id = b["rwgId"].as_u64().unwrap() as usize;
        let cls = b["class"].as_str().unwrap_or("");
        let simple = cls.rsplit('.').next().unwrap_or("");
        let fields = &b["fields"]["fields"];
        out[id] = match simple {
            "RealisticBiomeCoastColdCliff" => HeightModel::CoastColdCliff,
            "RealisticBiomeCoastColdSlope" => HeightModel::CoastColdSlope,
            "RealisticBiomeCoastDunes" => HeightModel::CoastDunes,
            "RealisticBiomeCoastIce" => HeightModel::CoastIce,
            "RealisticBiomeCoastMangrove" => HeightModel::CoastMangrove,
            "RealisticBiomeCoastOasis" | "RealisticBiomeCoastTest" => HeightModel::CoastOasis,
            "RealisticBiomeIslandTropical" => HeightModel::IslandTropical,
            "RealisticBiomeIslandTundra" => HeightModel::IslandTundra,
            "RealisticBiomeIslandVolcano" => HeightModel::IslandVolcano,
            "RealisticBiomeOcean" | "RealisticBiomeBOPOcean" => HeightModel::Ocean {
                shallow: fields["shallow"]
                    .as_bool()
                    .ok_or(TableError(format!("{simple}.shallow missing")))?,
            },
            "RealisticBiomeOceanTest" => HeightModel::OceanTest,
            "RealisticBiomeTest" => HeightModel::Const(70.0),
            "RealisticBiomeTestRiver" => HeightModel::Const(64.1),
            _ => match b.get("terrain") {
                Some(t) if !t.is_null() => HeightModel::Terrain(terrain_from(t)?),
                _ => HeightModel::Const(63.0),
            },
        };
    }
    Ok(out)
}
