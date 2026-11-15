//! `pack-tool`: keeps the seed map honest across GTNH pack releases.
//!
//! * `pack-tool diff <rwg2dump-dir> [--data data/gtnh-2.9.0]` — extracts every pack fact the port depends on from a
//!   fresh rwg2dump and compares it with the data files, the recorded `pack-facts.json` and the constants compiled
//!   into `rwg2-biomes`. Prints a report grouped by area (with the affected modules) and exits 1 if anything
//!   model-relevant changed, 0 if not, 2 on errors.
//! * `pack-tool import <rwg2dump-dir> <out-dir> [--prev data/gtnh-2.9.0] [--pack 2.9.1]` — writes `biomes.json`,
//!   `climate.json`, `gregtech.json`, `gtfluids.json` and `pack-facts.json` for a new pack from a dump (the same
//!   extraction used for 2.9.0). Vein colours/names not derivable from the dump are carried over from `--prev`;
//!   new materials need `tools/pack-tool/gt_enrich.py` (see PACK_UPDATE.md).
//!
//! See PACK_UPDATE.md at the repo root for the full procedure.

use serde_json::{json, Map, Value};
use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::ExitCode;

use rwg2_biomes::dungeons::ROGUE_GTNH;
use rwg2_biomes::gtfluids::FluidTable;
use rwg2_biomes::gtores::GT_GTNH;
use rwg2_biomes::nether::{RawVein, BOP_NETHER_BIOMES_GTNH, GT_NETHER_VEINS, GT_TF_VEINS};
use rwg2_biomes::tc_nodes::{GTNH_ADDONS, GTNH_COMPLEX_COUNT};
use rwg2_biomes::thaumcraft::TC_GTNH;
use rwg2_biomes::twilight::TfBiomeIds;
use rwg2_biomes::villages::{GTNH_290_SPAWN_BIOME_NAMES, SEPARATION, SPACING};

type Res<T> = Result<T, String>;

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let r = match args.first().map(|s| s.as_str()) {
        Some("diff") => cmd_diff(&args[1..]),
        Some("import") => cmd_import(&args[1..]),
        _ => {
            eprintln!(
                "usage:\n  pack-tool diff <rwg2dump-dir> [--data data/gtnh-2.9.0]\n  pack-tool import <rwg2dump-dir> <out-dir> [--prev data/gtnh-2.9.0] [--pack 2.9.1]"
            );
            return ExitCode::from(2);
        }
    };
    match r {
        Ok(code) => ExitCode::from(code),
        Err(e) => {
            eprintln!("error: {e}");
            ExitCode::from(2)
        }
    }
}

fn opt(args: &[String], name: &str) -> Option<String> {
    args.iter().position(|a| a == name).and_then(|i| args.get(i + 1).cloned())
}

fn read_json(p: &Path) -> Res<Value> {
    let s = fs::read_to_string(p).map_err(|e| format!("{}: {e}", p.display()))?;
    serde_json::from_str(&s).map_err(|e| format!("{}: {e}", p.display()))
}

fn try_json(dir: &Path, f: &str) -> Option<Value> {
    read_json(&dir.join(f)).ok()
}

// ============================================================================================ normalisation

/// Drop per-session values that change on every run and never matter to the port.
fn strip_volatile(v: &mut Value) {
    match v {
        Value::Object(m) => {
            for k in ["largeTreeSeed", "worldSeed", "gtDef", "millis", "totalMillis"] {
                m.remove(k);
            }
            for (_, x) in m.iter_mut() {
                strip_volatile(x);
            }
        }
        Value::Array(a) => a.iter_mut().for_each(strip_volatile),
        _ => {}
    }
}

/// `{name, weight, minY, maxY, size, density, primaryId, ores: [primary, secondary, between, sporadic]}`.
fn vein_from_dump(v: &Value) -> Value {
    let ore = |k: &str| v[k]["name"].clone();
    vein_obj(&v["name"], &v["weight"], &v["minY"], &v["maxY"], &v["size"], &v["density"], &v["primary"]["id"],
        json!([ore("primary"), ore("secondary"), ore("between"), ore("sporadic")]))
}

fn vein_from_data(v: &Value) -> Value {
    vein_obj(&v["name"], &v["weight"], &v["minY"], &v["maxY"], &v["size"], &v["density"], &v["primaryId"], v["ores"].clone())
}

fn vein_from_const(v: &RawVein) -> Value {
    vein_obj(&json!(v.0), &json!(v.1), &json!(v.2), &json!(v.3), &json!(v.4), &json!(v.5), &json!(v.6), json!(v.7))
}

#[allow(clippy::too_many_arguments)]
fn vein_obj(name: &Value, weight: &Value, min_y: &Value, max_y: &Value, size: &Value, density: &Value, primary: &Value, ores: Value) -> Value {
    json!({"name": name, "weight": weight, "minY": min_y, "maxY": max_y, "size": size, "density": density,
           "primaryId": primary, "ores": ores})
}

/// `climate.json` biome entry from a `forestry.json` biome (rule verified 246/246 on 2.9.0).
fn climate_entry(b: &Value) -> Value {
    let dict: Vec<String> = b["dictionary"]
        .as_array()
        .map(|a| a.iter().filter_map(|x| x.as_str().map(String::from)).collect())
        .unwrap_or_default();
    let has = |t: &str| dict.iter().any(|d| d == t);
    json!({
        "id": b["id"], "name": b["name"], "t": b["temperature"]["bits"], "r": b["rainfall"]["bits"],
        "hellish": b["hellish"], "dict": dict,
        "rogueInvalid": has("RIVER") || has("BEACH") || has("MUSHROOM") || has("OCEAN"),
        "rogueVegetation": has("JUNGLE") || has("DENSE"),
    })
}

fn fluid_norm(name: &str, registry: &str, min: i64, max: i64, chance: i64, dec: i64) -> Value {
    json!({"registry": registry, "name": name, "minAmount": min, "maxAmount": max, "chance": chance, "decreasePerOperation": dec})
}

/// Forge-style config → `{"category.key": "value"}` (list items joined with U+001F). Also handles plain `key=value`.
fn parse_cfg(text: &str) -> BTreeMap<String, String> {
    let mut out = BTreeMap::new();
    let mut cats: Vec<String> = Vec::new();
    let mut list: Option<(String, Vec<String>)> = None;
    for raw in text.lines() {
        let l = raw.trim();
        if let Some((k, items)) = list.as_mut() {
            if l == ">" {
                out.insert(k.clone(), items.join("\u{1f}"));
                list = None;
            } else if !l.is_empty() {
                items.push(l.to_string());
            }
            continue;
        }
        if l.is_empty() || l.starts_with('#') || l.starts_with('~') {
            continue;
        }
        if l == "}" {
            cats.pop();
            continue;
        }
        if let Some(c) = l.strip_suffix('{') {
            cats.push(c.trim().trim_matches('"').to_string());
            continue;
        }
        let body = if l.len() > 2 && l.as_bytes()[1] == b':' { &l[2..] } else { l };
        let key_of = |k: &str| {
            let k = k.trim().trim_matches('"');
            if cats.is_empty() {
                k.to_string()
            } else {
                format!("{}.{k}", cats.join("."))
            }
        };
        if let Some(k) = body.strip_suffix('<') {
            list = Some((key_of(k), Vec::new()));
        } else if let Some((k, v)) = body.split_once('=') {
            out.insert(key_of(k), v.trim().to_string());
        }
    }
    out
}

// ============================================================================================ areas

struct Area {
    key: &'static str,
    title: &'static str,
    /// Port modules that read this fact (what to re-check / update when it changes).
    modules: &'static str,
    /// A change here changes model output (else informational).
    relevant: bool,
}

const AREAS: &[Area] = &[
    Area { key: "mods", title: "Modelled mod versions / jars", modules: "per mod, see the version list in PACK_UPDATE.md", relevant: true },
    Area { key: "rwg2Config", title: "RWG2 config (rwg2.cfg as generated)", modules: "config.rs, provider.rs, chunkgen.rs (everything overworld)", relevant: true },
    Area { key: "noise", title: "RWG2 noise implementation", modules: "noise.rs / provider (OpenSimplex not ported)", relevant: true },
    Area { key: "biomes", title: "RWG2 biomes.json (lists, order, ids, terrain params)", modules: "data biomes.json → provider, terrain, chunkgen, legend", relevant: true },
    Area { key: "climate", title: "Biome climate / dictionary (forestry.json → climate.json)", modules: "data climate.json → forestry, dungeons (Roguelike flags), thaumcraft, villages", relevant: true },
    Area { key: "gtConfig", title: "GT ore config", modules: "gtores.rs GT_GTNH", relevant: true },
    Area { key: "gtOverworld", title: "GT overworld vein list", modules: "data gregtech.json → gtores, gtorder, search, Ores tab", relevant: true },
    Area { key: "gtDims", title: "GT Nether / Twilight Forest vein lists", modules: "nether.rs GT_NETHER_VEINS / GT_TF_VEINS → gtdims, search", relevant: true },
    Area { key: "gtDimDefs", title: "GT dimension definitions (vein chance, heights)", modules: "gtdims.rs, gtores.rs", relevant: true },
    Area { key: "gtFluids", title: "GT underground fluids table", modules: "data gtfluids.json → gtfluids.rs", relevant: true },
    Area { key: "spawn", title: "Overworld provider / spawn rule", modules: "spawn.rs (BOP sand/stone rule)", relevant: true },
    Area { key: "structures", title: "Structure generators", modules: "structures.rs, stronghold_pieces.rs, villages.rs, mineshaft.rs", relevant: true },
    Area { key: "villages", title: "VillageNames spacing / biome list", modules: "villages.rs SPACING, SEPARATION, GTNH_290_SPAWN_BIOME_NAMES", relevant: true },
    Area { key: "roguelike", title: "Roguelike placement config", modules: "dungeons.rs ROGUE_GTNH", relevant: true },
    Area { key: "lootgames", title: "LootGames placement config", modules: "dungeons.rs (dimRhombs {0=20})", relevant: true },
    Area { key: "thaumcraft", title: "Thaumcraft worldgen config + aspect registry", modules: "thaumcraft.rs TC_GTNH, tc_nodes.rs GTNH_ADDONS / GTNH_COMPLEX_COUNT", relevant: true },
    Area { key: "dims", title: "Nether / TF providers and generators", modules: "nether.rs, twilight.rs, dimterrain.rs, tc_dims.rs", relevant: true },
    Area { key: "tfBiomes", title: "Twilight Forest biome ids", modules: "twilight.rs TfBiomeIds::GTNH_290", relevant: true },
    Area { key: "netherBiomes", title: "BOP Nether biome list", modules: "nether.rs BOP_NETHER_BIOMES_GTNH", relevant: true },
    Area { key: "tcDimBlacklist", title: "Thaumcraft dimension blacklist", modules: "tc_dims.rs (Nether/TF assumed not blacklisted)", relevant: true },
    Area { key: "configs", title: "Captured config files (model-relevant keys)", modules: "per file: see PACK_UPDATE.md", relevant: true },
    Area { key: "configsOther", title: "Other captured config files", modules: "not used by the port", relevant: false },
];

/// Mods whose code the port reproduces (mod id in env.json → what it drives).
const MODELLED_MODS: &[(&str, &str)] = &[
    ("mcp", "vanilla 1.7.10 (structures, caves, spawn walk)"),
    ("Forge", "per-chunk Random, generator order"),
    ("rwg2", "biomes, terrain, chunk generation"),
    ("BiomesOPlenty", "biomes, Nether, spawn provider"),
    ("gregtech", "ore veins, fluids"),
    ("gtnhlib", "ore vein hashing (FNV)"),
    ("hodgepodge", "fastRandom (caves, structure RNG)"),
    ("Thaumcraft", "aura nodes, structures, trees"),
    ("gadomancy", "extended aura nodes"),
    ("salisarcana", "node type/modifier weights"),
    ("tc4tweak", "Thaumcraft tweaks"),
    ("VillageNames", "villages"),
    ("etfuturum", "mesa mineshafts"),
    ("TwilightForest", "TF biomes, terrain, features"),
    ("Roguelike", "Roguelike dungeons"),
    ("lootgames", "LootGames dungeons"),
    ("Forestry", "bee climate"),
];

/// Config files (path suffix, lower case) whose keys the port depends on.
const RELEVANT_CONFIGS: &[(&str, &str)] = &[
    ("roguelike_dungeons/roguelike.cfg", "dungeons.rs"),
    ("lootgames/general.cfg", "dungeons.rs"),
    ("config/thaumcraft.cfg", "thaumcraft.rs, tc_nodes.rs"),
    ("newvillages/village_gen.cfg", "villages.rs"),
    ("defaultworldgenerator.cfg", "default world type (RWG2_CONTINENTS)"),
    ("twilightforest.cfg", "twilight.rs (ids, seed)"),
    ("hodgepodge", "fastRandom (caves, structures)"),
    ("gregtech/worldgeneration.cfg", "gtores.rs"),
    ("gregtech/undergroundfluids.cfg", "gtfluids.rs"),
    ("etfuturum/world.cfg", "mineshaft.rs (mesa mineshafts)"),
    ("rwg2.cfg", "config.rs (absent in 2.9.0 = code defaults)"),
];

fn config_relevance(path: &str) -> Option<&'static str> {
    let p = path.to_lowercase();
    RELEVANT_CONFIGS.iter().find(|(s, _)| p.ends_with(s) || (!s.contains('.') && p.contains(s))).map(|(_, m)| *m)
}

/// Everything the port depends on, extracted from one dump folder. Missing files → missing keys.
fn extract(dump: &Path) -> Res<Map<String, Value>> {
    let mut f = Map::new();
    let env = try_json(dump, "env.json").ok_or("env.json missing (not an rwg2dump folder?)")?;
    let mut mods = Map::new();
    if let Some(list) = env["mods"].as_array() {
        for (id, _) in MODELLED_MODS {
            if let Some(m) = list.iter().find(|m| m["id"].as_str() == Some(id)) {
                let mut e = Map::new();
                e.insert("version".into(), m["version"].clone());
                e.insert("jar".into(), m["source"].clone());
                if let Some(h) = m.get("sha256") {
                    e.insert("sha256".into(), h.clone());
                }
                mods.insert(id.to_string(), Value::Object(e));
            } else {
                mods.insert(id.to_string(), Value::Null);
            }
        }
    }
    f.insert("mods".into(), Value::Object(mods));
    f.insert("rwg2Config".into(), json!(parse_cfg(env["rwg2Config"].as_str().unwrap_or(""))));
    f.insert("noise".into(), env["noiseImplementation"].clone());
    f.insert("_dumpVersion".into(), env["rwg2dumpVersion"].clone());

    if let Some(mut b) = try_json(dump, "biomes.json") {
        strip_volatile(&mut b);
        f.insert("biomes".into(), b);
    }
    if let Some(fo) = try_json(dump, "forestry.json") {
        let c: Vec<Value> = fo["biomes"].as_array().map(|a| a.iter().map(climate_entry).collect()).unwrap_or_default();
        f.insert("climate".into(), Value::Array(c));
    }
    if let Some(g) = try_json(dump, "gregtech.json") {
        f.insert(
            "gtConfig".into(),
            json!({"oreveinPercentage": g["oreveinPercentage"], "oreveinAttempts": g["oreveinAttempts"],
                   "oreveinMaxPlacementAttempts": g["oreveinMaxPlacementAttempts"], "pattern": g["pattern"]}),
        );
        let ov: Vec<Value> = g["veins"]
            .as_array()
            .map(|a| a.iter().filter(|v| v["overworld"].as_bool() == Some(true)).map(vein_from_dump).collect())
            .unwrap_or_default();
        f.insert("gtOverworld".into(), Value::Array(ov));
    }
    if let Some(gf) = try_json(dump, "gtfluids.json") {
        let fl: Vec<Value> = gf["table"]["fluids"]
            .as_array()
            .map(|a| {
                a.iter()
                    .map(|x| {
                        let i = |k: &str| x[k].as_i64().unwrap_or(-1);
                        fluid_norm("", x["registry"].as_str().unwrap_or(""), i("minAmount"), i("maxAmount"), i("chance"), i("decreasePerOperation"))
                    })
                    .collect()
            })
            .unwrap_or_default();
        f.insert("gtFluids".into(), json!({"order": gf["table"]["order"], "fluids": fl, "blackList": gf["blackList"]}));
    }
    if let Some(s) = try_json(dump, "spawn.json") {
        f.insert(
            "spawn".into(),
            json!({"worldType": s["worldType"], "provider": s["provider"], "canSpawnDeclaredIn": s["canSpawnDeclaredIn"],
                   "chunkManager": s["chunkManager"], "spawnBiomes": s["spawnBiomes"]}),
        );
    }
    if let Some(s) = try_json(dump, "structures.json") {
        f.insert(
            "structures".into(),
            json!({"chunkGenerator": s["chunkGenerator"], "village": s["villageGenerator"]["class"],
                   "stronghold": s["strongholdGenerator"]["class"], "mineshaft": s["mineshaftGenerator"]["class"]}),
        );
    }
    let configs = try_json(dump, "dungeons.json").map(|d| d["configFiles"].clone()).unwrap_or(Value::Null);
    let cfg = |suffix: &str| -> Option<BTreeMap<String, String>> {
        configs.as_object()?.iter().find(|(k, _)| k.to_lowercase().ends_with(suffix)).map(|(_, v)| parse_cfg(v.as_str().unwrap_or("")))
    };
    let mut vil = Map::new();
    if let Some(v) = try_json(dump, "villages.json") {
        vil.insert("class".into(), v["generatorClass"].clone());
        vil.insert("spacing".into(), v["spacing"].clone());
    }
    if let Some(c) = cfg("newvillages/village_gen.cfg") {
        // `S:"Spawn Biome Names" <` list → order matters (gate list)
        if let Some((_, v)) = c.iter().find(|(k, _)| k.to_lowercase().ends_with("spawn biome names")) {
            vil.insert("spawnBiomeNames".into(), json!(v.split('\u{1f}').collect::<Vec<_>>()));
        }
    }
    if !vil.is_empty() {
        f.insert("villages".into(), Value::Object(vil));
    }
    if let Some(c) = cfg("roguelike_dungeons/roguelike.cfg") {
        let g = |k: &str| c.get(k).and_then(|v| v.parse::<i64>().ok());
        f.insert(
            "roguelike".into(),
            json!({"spawnFrequency": g("spawnFrequency"), "upperLimit": g("upperLimit"), "lowerLimit": g("lowerLimit"),
                   "dimensionWL": c.get("dimensionWL"), "dimensionBL": c.get("dimensionBL")}),
        );
    }
    if let Some(d) = try_json(dump, "dungeons.json") {
        f.insert(
            "lootgames".into(),
            json!({"disableDungeonGen": d["lootgames"]["disableDungeonGen"], "dimRhombs": d["lootgames"]["dimRhombs"]}),
        );
    }
    let mut tc = Map::new();
    if let Some(t) = try_json(dump, "tcnodes.json") {
        let aspects = t["aspects"].as_array().cloned().unwrap_or_default();
        tc.insert("addonAspects".into(), Value::Array(aspects.iter().skip(48).cloned().collect()));
        tc.insert("basic".into(), t["basic"].clone());
        tc.insert("complex".into(), t["complex"].clone());
    }
    if let Some(c) = cfg("config/thaumcraft.cfg") {
        let find = |k: &str| c.iter().find(|(kk, _)| kk.ends_with(&format!(".{k}")) || *kk == k).map(|(_, v)| v.clone());
        let int = |k: &str| find(k).and_then(|v| v.parse::<i64>().ok());
        let bool_ = |k: &str| find(k).map(|v| v == "true");
        tc.insert("nodeRarity".into(), json!(int("node_rarity")));
        tc.insert("specialNodeRarity".into(), json!(int("special_node_rarity")));
        tc.insert("generateTrees".into(), json!(bool_("generate_trees")));
        tc.insert("generateAuraNodes".into(), json!(bool_("generate_aura_nodes")));
        tc.insert("generateStructures".into(), json!(bool_("generate_structures")));
        for k in ["biome_eerie", "biome_magical_forest", "biome_taint", "outer_lands_dim"] {
            tc.insert(k.into(), json!(int(k)));
        }
        for k in ["generate_amber_ore", "generate_cinnibar_ore", "generate_infused_stone"] {
            tc.insert(k.into(), json!(bool_(k)));
        }
    }
    if !tc.is_empty() {
        f.insert("thaumcraft".into(), Value::Object(tc));
    }
    if let Some(mut d) = try_json(dump, "dims.json") {
        strip_volatile(&mut d);
        let mut dims = Map::new();
        if let Some(o) = d["dims"].as_object() {
            for (k, v) in o {
                let mut e = v.clone();
                if let Some(m) = e.as_object_mut() {
                    for x in ["getOreVeinChance", "respectsOreVeinHeights", "generatesOre"] {
                        m.remove(x);
                    }
                }
                dims.insert(k.clone(), e);
            }
        }
        let mut tfc = d["tfConfig"].clone();
        if let Some(m) = tfc.as_object_mut() {
            m.remove("biomes");
        }
        dims.insert("tfConfig".into(), tfc);
        f.insert("dims".into(), Value::Object(dims));
        if let Some(b) = d["tfConfig"]["biomes"].as_object() {
            let tf: Map<String, Value> = b.iter().filter(|(k, _)| !k.starts_with("field_")).map(|(k, v)| (k.clone(), v.clone())).collect();
            f.insert("tfBiomes".into(), Value::Object(tf));
        }
        f.insert("netherBiomes".into(), d["nether"]["bopNetherBiomes"].clone());
        let mut gd = Map::new();
        let mut defs = Map::new();
        for name in ["Nether", "Twilight Forest"] {
            if let Some(a) = d["gt"][name].as_array() {
                gd.insert(name.into(), Value::Array(a.iter().map(vein_from_dump).collect()));
            }
        }
        for name in ["Overworld", "Nether", "Twilight Forest"] {
            defs.insert(name.into(), d["gt"][format!("{name}:def")].clone());
        }
        f.insert("gtDims".into(), Value::Object(gd));
        f.insert("gtDimDefs".into(), Value::Object(defs));
        let bl = d["gtScan"]["-1:terrain"]["tcDimBlacklist"].clone();
        if !bl.is_null() {
            f.insert("tcDimBlacklist".into(), bl);
        }
    }
    if let Some(c) = configs.as_object() {
        let mut rel = Map::new();
        let mut other = Map::new();
        for (k, v) in c {
            if k == "error" {
                continue;
            }
            let parsed = json!(parse_cfg(v.as_str().unwrap_or("")));
            if config_relevance(k).is_some() {
                rel.insert(k.clone(), parsed);
            } else {
                // informational: content hash only (settings JSON / loot tables are large)
                other.insert(k.clone(), json!(fnv_hex(v.as_str().unwrap_or(""))));
            }
        }
        f.insert("configs".into(), Value::Object(rel));
        f.insert("configsOther".into(), Value::Object(other));
    }
    Ok(f)
}

fn fnv_hex(s: &str) -> String {
    let mut h: u64 = 0xcbf29ce484222325;
    for b in s.bytes() {
        h ^= b as u64;
        h = h.wrapping_mul(0x100000001b3);
    }
    format!("{h:016x}")
}

/// What the port currently assumes: data files + `pack-facts.json` + compiled constants (constants win).
fn baseline(data: &Path) -> Res<(Map<String, Value>, Vec<String>)> {
    let mut b = Map::new();
    let mut notes = Vec::new();
    match try_json(data, "pack-facts.json") {
        Some(Value::Object(m)) => b.extend(m),
        _ => notes.push(format!("no {}/pack-facts.json: mods/configs/providers have no baseline", data.display())),
    }
    if let Some(mut x) = try_json(data, "biomes.json") {
        strip_volatile(&mut x);
        b.insert("biomes".into(), x);
    }
    if let Some(c) = try_json(data, "climate.json") {
        b.insert("climate".into(), c["biomes"].clone());
    }
    if let Some(g) = try_json(data, "gregtech.json") {
        b.insert(
            "gtOverworld".into(),
            Value::Array(g["veins"].as_array().map(|a| a.iter().map(vein_from_data).collect()).unwrap_or_default()),
        );
    }
    b.insert(
        "gtConfig".into(),
        json!({"oreveinPercentage": GT_GTNH.percentage, "oreveinAttempts": GT_GTNH.attempts,
               "oreveinMaxPlacementAttempts": GT_GTNH.max_placement_attempts,
               "pattern": if GT_GTNH.equal_spacing { "EQUAL_SPACING" } else { "AXISSYMMETRICAL" }}),
    );
    if let Ok(s) = fs::read_to_string(data.join("gtfluids.json")) {
        let t = FluidTable::from_json(&s, "0")?;
        let fl: Vec<Value> = t
            .fluids
            .iter()
            .map(|x| fluid_norm("", &x.registry, x.min_amount as i64, x.max_amount as i64, x.chance as i64, x.decrease_per_operation as i64))
            .collect();
        let v: Value = serde_json::from_str(&s).map_err(|e| e.to_string())?;
        let bl = v["blacklist"].as_array().map(|a| format!("{:?}", a.iter().filter_map(|x| x.as_i64()).collect::<Vec<_>>())).unwrap_or_default();
        b.insert("gtFluids".into(), json!({"order": "hashbimap", "fluids": fl, "blackList": bl}));
    }
    b.insert(
        "gtDims".into(),
        json!({"Nether": GT_NETHER_VEINS.iter().map(vein_from_const).collect::<Vec<_>>(),
               "Twilight Forest": GT_TF_VEINS.iter().map(vein_from_const).collect::<Vec<_>>()}),
    );
    let tf = TfBiomeIds::GTNH_290;
    b.insert(
        "tfBiomes".into(),
        json!({"tfLake": tf.lake, "twilightForest": tf.twilight_forest, "twilightForest2": tf.twilight_forest2,
               "highlands": tf.highlands, "mushrooms": tf.mushrooms, "tfSwamp": tf.swamp, "stream": tf.stream,
               "tfSnow": tf.snow, "glacier": tf.glacier, "clearing": tf.clearing, "oakSavanna": tf.oak_savanna,
               "fireflyForest": tf.firefly_forest, "deepMushrooms": tf.deep_mushrooms, "darkForest": tf.dark_forest,
               "enchantedForest": tf.enchanted_forest, "fireSwamp": tf.fire_swamp,
               "darkForestCenter": tf.dark_forest_center, "highlandsCenter": tf.highlands_center,
               "thornlands": tf.thornlands}),
    );
    b.insert(
        "netherBiomes".into(),
        Value::Array(BOP_NETHER_BIOMES_GTNH.iter().skip(1).map(|(i, w, n)| json!([i, w, n])).collect()),
    );
    let mut vil = b.get("villages").cloned().unwrap_or_else(|| json!({}));
    vil["spacing"] = json!({"max": SPACING, "min": SEPARATION});
    vil["spawnBiomeNames"] = json!(GTNH_290_SPAWN_BIOME_NAMES);
    b.insert("villages".into(), vil);
    let mut rg = b.get("roguelike").cloned().unwrap_or_else(|| json!({}));
    rg["spawnFrequency"] = json!(ROGUE_GTNH.spawn_frequency);
    rg["upperLimit"] = json!(ROGUE_GTNH.upper_limit);
    rg["lowerLimit"] = json!(ROGUE_GTNH.lower_limit);
    b.insert("roguelike".into(), rg);
    let mut tc = b.get("thaumcraft").cloned().unwrap_or_else(|| json!({}));
    tc["addonAspects"] = json!(GTNH_ADDONS);
    tc["complex"] = json!(GTNH_COMPLEX_COUNT);
    tc["basic"] = json!(6);
    tc["nodeRarity"] = json!(TC_GTNH.node_rarity);
    tc["specialNodeRarity"] = json!(TC_GTNH.special_node_rarity);
    tc["generateTrees"] = json!(TC_GTNH.gen_trees);
    tc["generateAuraNodes"] = json!(TC_GTNH.gen_aura);
    tc["generateStructures"] = json!(TC_GTNH.gen_structure);
    b.insert("thaumcraft".into(), tc);
    Ok((b, notes))
}

// ============================================================================================ diff

fn diff_values(a: &Value, b: &Value, path: &str, out: &mut Vec<String>) {
    const MAX: usize = 40;
    if out.len() > MAX {
        return;
    }
    match (a, b) {
        (Value::Object(x), Value::Object(y)) => {
            for (k, va) in x {
                match y.get(k) {
                    Some(vb) => diff_values(va, vb, &format!("{path}/{k}"), out),
                    None => out.push(format!("{path}/{k}: removed (was {})", short(va))),
                }
            }
            for (k, vb) in y {
                if !x.contains_key(k) {
                    out.push(format!("{path}/{k}: added = {}", short(vb)));
                }
            }
        }
        (Value::Array(x), Value::Array(y)) => {
            // name-keyed lists (veins, biomes): report by name when the order or membership changed
            if x.len() != y.len() {
                out.push(format!("{path}: length {} → {}", x.len(), y.len()));
            }
            for (i, (va, vb)) in x.iter().zip(y).enumerate() {
                let label = match va.get("name").or_else(|| va.get("displayName")).and_then(|n| n.as_str()) {
                    Some(n) => format!("{path}[{i} {n}]"),
                    None => format!("{path}[{i}]"),
                };
                diff_values(va, vb, &label, out);
            }
            if y.len() > x.len() {
                for (i, vb) in y.iter().enumerate().skip(x.len()) {
                    out.push(format!("{path}[{i}]: added {}", short(vb)));
                }
            }
        }
        _ => {
            if !same_scalar(a, b) {
                out.push(format!("{path}: {} → {}", short(a), short(b)));
            }
        }
    }
    if out.len() > MAX {
        out.truncate(MAX);
        out.push("… (more differences)".into());
    }
}

/// Numbers compare by value (dump ints vs data ints), everything else exactly.
fn same_scalar(a: &Value, b: &Value) -> bool {
    match (a.as_f64(), b.as_f64()) {
        (Some(x), Some(y)) => x == y,
        _ => a == b,
    }
}

fn short(v: &Value) -> String {
    let s = v.to_string();
    if s.len() > 120 {
        format!("{}…", &s[..s.char_indices().take_while(|(i, _)| *i < 117).last().map(|(i, c)| i + c.len_utf8()).unwrap_or(0)])
    } else {
        s
    }
}

fn cmd_diff(args: &[String]) -> Res<u8> {
    let dump = PathBuf::from(args.first().ok_or("diff: missing <rwg2dump-dir>")?);
    let data = PathBuf::from(opt(args, "--data").unwrap_or_else(|| "data/gtnh-2.9.0".into()));
    let new = extract(&dump)?;
    let (base, notes) = baseline(&data)?;
    println!("pack-diff: {} (rwg2dump {}) vs {}", dump.display(), new.get("_dumpVersion").map(short).unwrap_or_default(), data.display());
    for n in &notes {
        println!("  note: {n}");
    }
    let mut relevant_changes = 0;
    let mut skipped = Vec::new();
    let mut new_files = Vec::new();
    for area in AREAS {
        let (Some(a), Some(b)) = (base.get(area.key), new.get(area.key)) else {
            skipped.push(format!(
                "{} ({})",
                area.key,
                if new.get(area.key).is_none() { "not in this dump" } else { "no baseline" }
            ));
            continue;
        };
        let mut d = Vec::new();
        // area fields the dump does not capture (older rwg2dump, file not dumped) are not changes
        let mut a = a.clone();
        if let (Some(ao), Some(bo)) = (a.as_object_mut(), b.as_object()) {
            if area.key != "configs" && area.key != "configsOther" && area.key != "mods" {
                let missing: Vec<String> = ao.keys().filter(|k| !bo.contains_key(*k)).cloned().collect();
                for k in &missing {
                    ao.remove(k);
                    skipped.push(format!("{}.{k} (not in this dump)", area.key));
                }
            }
        }
        let a = &a;
        if area.key == "configs" {
            config_diff(a, b, &mut d, &mut new_files);
        } else if area.key == "mods" {
            mods_diff(a, b, &mut d);
        } else {
            diff_values(a, b, "", &mut d);
        }
        if d.is_empty() {
            println!("  ok     {}", area.title);
        } else {
            let tag = if area.relevant { "CHANGE" } else { "info" };
            println!("  {tag:<7}{}  → {}", area.title, area.modules);
            for l in &d {
                println!("          {l}");
            }
            if area.relevant {
                relevant_changes += 1;
            }
        }
    }
    if !skipped.is_empty() {
        println!("  skipped: {}", skipped.join(", "));
    }
    for f in &new_files {
        println!("  note   {f}");
    }
    if relevant_changes == 0 {
        println!("RESULT: no model-relevant changes");
        Ok(0)
    } else {
        println!("RESULT: {relevant_changes} area(s) with model-relevant changes — follow PACK_UPDATE.md");
        Ok(1)
    }
}

fn mods_diff(a: &Value, b: &Value, out: &mut Vec<String>) {
    for (id, what) in MODELLED_MODS {
        let (x, y) = (&a[id], &b[id]);
        if x == y {
            continue;
        }
        let fmt = |v: &Value| {
            if v.is_null() {
                "absent".to_string()
            } else {
                format!("{} ({})", v["version"].as_str().unwrap_or("?"), v["jar"].as_str().unwrap_or("?"))
            }
        };
        let hash = if !x.is_null() && !y.is_null() && x["version"] == y["version"] && x["jar"] == y["jar"] {
            " [same version, different jar hash]"
        } else {
            ""
        };
        out.push(format!("{id}: {} → {}{hash}  [{what}]", fmt(x), fmt(y)));
    }
}

/// Changed keys of config files present in both. Files only on one side go to `new_files` as notes: captured by a
/// newer rwg2dump (inspect by hand once; `pack-tool import` records them) or not captured by an older one.
fn config_diff(a: &Value, b: &Value, out: &mut Vec<String>, new_files: &mut Vec<String>) {
    let (Some(x), Some(y)) = (a.as_object(), b.as_object()) else { return };
    for (path, vb) in y {
        let what = config_relevance(path).unwrap_or("");
        match x.get(path) {
            None => new_files.push(format!("{path}: captured, no baseline yet (check by hand; re-import to record)  [{what}]")),
            Some(va) => {
                let mut d = Vec::new();
                diff_values(va, vb, "", &mut d);
                for l in d {
                    out.push(format!("{path}{l}  [{what}]"));
                }
            }
        }
    }
    for path in x.keys() {
        if !y.contains_key(path) {
            new_files.push(format!("{path}: not captured by this dump (older rwg2dump?), not compared"));
        }
    }
}

// ============================================================================================ import

fn cmd_import(args: &[String]) -> Res<u8> {
    let dump = PathBuf::from(args.first().ok_or("import: missing <rwg2dump-dir>")?);
    let out = PathBuf::from(args.get(1).ok_or("import: missing <out-dir>")?);
    let prev = PathBuf::from(opt(args, "--prev").unwrap_or_else(|| "data/gtnh-2.9.0".into()));
    let pack = opt(args, "--pack").unwrap_or_else(|| "?".into());
    fs::create_dir_all(&out).map_err(|e| e.to_string())?;
    let env = read_json(&dump.join("env.json"))?;
    let ver = env["rwg2dumpVersion"].as_str().unwrap_or("?").to_string();
    let mut warn = Vec::new();

    // biomes.json: the dump's file verbatim (the port's loader reads the rwg2dump format directly)
    fs::copy(dump.join("biomes.json"), out.join("biomes.json")).map_err(|e| format!("biomes.json: {e}"))?;

    // climate.json from forestry.json
    let fo = read_json(&dump.join("forestry.json"))?;
    let biomes: Vec<Value> = fo["biomes"].as_array().ok_or("forestry.json: biomes")?.iter().map(climate_entry).collect();
    write(&out.join("climate.json"), &json!({"source": format!("rwg2dump {ver} forestry.json (GTNH {pack})"), "biomes": biomes}))?;

    // gregtech.json: overworld stone veins in registry order + enrichment
    let g = read_json(&dump.join("gregtech.json"))?;
    let prev_g = try_json(&prev, "gregtech.json");
    let prev_by_name = |name: &str| -> Option<Value> {
        prev_g.as_ref()?["veins"].as_array()?.iter().find(|v| v["name"].as_str() == Some(name)).cloned()
    };
    let mut veins = Vec::new();
    for v in g["veins"].as_array().ok_or("gregtech.json: veins")? {
        if v["overworld"].as_bool() != Some(true) {
            continue;
        }
        let name = v["name"].as_str().unwrap_or("");
        let parts = ["primary", "secondary", "between", "sporadic"];
        let mut e = json!({
            "name": name, "weight": v["weight"], "minY": v["minY"], "maxY": v["maxY"], "size": v["size"],
            "density": v["density"], "primaryId": v["primary"]["id"],
            "ores": parts.iter().map(|p| v[p]["name"].clone()).collect::<Vec<_>>(),
            "oreIds": parts.iter().map(|p| v[p]["id"].clone()).collect::<Vec<_>>(),
        });
        // displayName: the live localized mix name (rwg2dump ≥ 2.6 mixNames), else carried over
        let live = g["mixNames"][name].as_str().map(String::from);
        let p = prev_by_name(name);
        let display = live.or_else(|| p.as_ref().and_then(|p| p["displayName"].as_str().map(String::from)));
        // ore names: distinct materials in primary, secondary, between, sporadic order, local names
        let mut seen: Vec<(String, String)> = Vec::new();
        for part in parts {
            let n = v[part]["name"].as_str().unwrap_or("").to_string();
            if !n.is_empty() && !seen.iter().any(|(m, _)| *m == n) {
                seen.push((n, v[part]["local"].as_str().unwrap_or("").to_string()));
            }
        }
        let same_ores = p.as_ref().is_some_and(|p| p["ores"] == e["ores"]);
        e["displayName"] = json!(display);
        e["oreNames"] = json!(seen.iter().map(|(_, l)| l.clone()).collect::<Vec<_>>());
        match (&p, same_ores) {
            (Some(p), true) => {
                e["iconRgb"] = p["iconRgb"].clone();
                e["oreRgb"] = p["oreRgb"].clone();
            }
            _ => {
                warn.push(format!("vein {name}: new or changed ores — colours not derivable from the dump; run tools/pack-tool/gt_enrich.py"));
                e["iconRgb"] = json!("#808080");
                e["oreRgb"] = json!(vec!["#808080"; seen.len()]);
            }
        }
        if e["displayName"].is_null() {
            warn.push(format!("vein {name}: no live mix name (rwg2dump < 2.6?) and none in --prev"));
        }
        veins.push(e);
    }
    let gt_version = env["mods"]
        .as_array()
        .and_then(|a| a.iter().find(|m| m["id"].as_str() == Some("gregtech")))
        .and_then(|m| m["version"].as_str())
        .unwrap_or("?")
        .to_string();
    write(
        &out.join("gregtech.json"),
        &json!({
            "source": format!("rwg2dump {ver} gregtech.json (GTNH {pack}, GregTech {gt_version}): WorldgenGTOreLayer.sList, Overworld stone veins in registry order"),
            "provisional": false,
            "config": {"oreveinPercentage": g["oreveinPercentage"], "oreveinAttempts": g["oreveinAttempts"],
                       "oreveinMaxPlacementAttempts": g["oreveinMaxPlacementAttempts"], "pattern": g["pattern"]},
            "veins": veins,
        }),
    )?;

    // gtfluids.json: keep the previous file if the live table is unchanged, else write the live order
    let gf = try_json(&dump, "gtfluids.json");
    let prev_fluids = fs::read_to_string(prev.join("gtfluids.json")).ok();
    match (gf, prev_fluids) {
        (Some(gf), Some(pf)) => {
            let live: Vec<Value> = gf["table"]["fluids"].as_array().cloned().unwrap_or_default();
            let prev_t = FluidTable::from_json(&pf, "0")?;
            let same = live.len() == prev_t.fluids.len()
                && live.iter().zip(&prev_t.fluids).all(|(l, p)| {
                    l["registry"].as_str() == Some(p.registry.as_str())
                        && l["minAmount"].as_i64() == Some(p.min_amount as i64)
                        && l["maxAmount"].as_i64() == Some(p.max_amount as i64)
                        && l["chance"].as_i64() == Some(p.chance as i64)
                        && l["decreasePerOperation"].as_i64() == Some(p.decrease_per_operation as i64)
                });
            if same {
                fs::write(out.join("gtfluids.json"), pf).map_err(|e| e.to_string())?;
            } else {
                let pv: Value = serde_json::from_str(&pf).map_err(|e| e.to_string())?;
                let old: Vec<Value> = pv["dimensions"]["0"]["fluids"].as_array().cloned().unwrap_or_default();
                let fluids: Vec<Value> = live
                    .iter()
                    .map(|l| {
                        let reg = l["registry"].as_str().unwrap_or("");
                        let o = old.iter().find(|o| o["registry"].as_str() == Some(reg));
                        json!({
                            "name": o.map(|o| o["name"].clone()).unwrap_or_else(|| l["key"].clone()),
                            "registry": reg,
                            "displayName": l["displayName"],
                            "rgb": o.map(|o| o["rgb"].clone()).unwrap_or(json!("#808080")),
                            "minAmount": l["minAmount"], "maxAmount": l["maxAmount"], "chance": l["chance"],
                            "decreasePerOperation": l["decreasePerOperation"],
                        })
                    })
                    .collect();
                warn.push("gtfluids: live table differs from --prev; wrote the dump's HashBiMap order (check colours)".into());
                write(
                    &out.join("gtfluids.json"),
                    &json!({"source": format!("rwg2dump {ver} gtfluids.json (GTNH {pack}), live table"), "provisional": false,
                            "gtVersion": gt_version, "divider": pv["divider"], "fieldChunks": pv["fieldChunks"],
                            "blacklist": pv["blacklist"],
                            "dimensions": {"0": {"name": "Overworld", "order": "hashbimap", "fluids": fluids}}}),
                )?;
            }
        }
        (None, Some(pf)) => {
            warn.push("gtfluids: not in this dump, copied --prev".into());
            fs::write(out.join("gtfluids.json"), pf).map_err(|e| e.to_string())?;
        }
        (_, None) => warn.push("gtfluids: no --prev gtfluids.json to start from; write it by hand".into()),
    }

    // pack-facts.json: everything else pack-diff compares (not read by the viewer)
    let mut facts = extract(&dump)?;
    for k in ["biomes", "climate", "gtOverworld", "gtFluids", "_dumpVersion"] {
        facts.remove(k);
    }
    facts.insert("_source".into(), json!(format!("pack-tool import from rwg2dump {ver} (GTNH {pack})")));
    write(&out.join("pack-facts.json"), &Value::Object(facts))?;

    println!("wrote {} (biomes, climate, gregtech, gtfluids, pack-facts)", out.display());
    for w in &warn {
        println!("  warning: {w}");
    }
    Ok(if warn.is_empty() { 0 } else { 1 })
}

fn write(p: &Path, v: &Value) -> Res<()> {
    let s = serde_json::to_string_pretty(v).map_err(|e| e.to_string())?;
    fs::write(p, s + "\n").map_err(|e| format!("{}: {e}", p.display()))
}
