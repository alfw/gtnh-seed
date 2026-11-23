#![cfg(feature = "json")]
//! Villages (VillageNames `MapGenVillageVN`) vs rwg2dump v2 `structures.json` (seed 1337 RWG2_CONTINENTS).
use rwg2_biomes::config::RwgConfig;
use rwg2_biomes::provider::{BiomeProvider, BiomeTable, TableHeights, WorldType};
use rwg2_biomes::villages::*;

fn root() -> std::path::PathBuf {
    std::env::var("RWG2_DATA_DIR")
        .map(|d| std::path::PathBuf::from(d).parent().unwrap().to_path_buf())
        .unwrap_or_else(|_| std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../.."))
}

#[test]
fn builtin_table_matches_climate_json() {
    let s = std::fs::read_to_string(root().join(pack_rel("climate.json"))).unwrap();
    let derived = VillageBiomes::from_climate_json(&s, GTNH_290_SPAWN_BIOME_NAMES).unwrap();
    let builtin = VillageBiomes::gtnh_2_9_0();
    let n = derived.allowed.len().max(builtin.allowed.len());
    for id in 0..n as u16 {
        assert_eq!(
            derived.is_allowed(id),
            builtin.is_allowed(id),
            "allowed {id}"
        );
        assert_eq!(derived.style_of(id), builtin.style_of(id), "style {id}");
    }
}

fn style_of_piece(class: &str) -> Option<Style> {
    let c = class.rsplit('.').next()?;
    Some(match c.split('$').next()? {
        "PlainsStructures" => Style::Plains,
        "DesertStructures" => Style::Desert,
        "TaigaStructures" => Style::Taiga,
        "SavannaStructures" => Style::Savanna,
        "SnowyStructures" => Style::Snowy,
        "JungleStructures" => Style::Jungle,
        "SwampStructures" => Style::Swamp,
        _ => return None,
    })
}

#[test]
fn villages_match_pack() {
    check_dump("dump");
}

#[test]
fn villages_match_second_world() {
    check_dump("dump28");
}

fn check_dump(dir: &str) {
    let Ok(s) = std::fs::read_to_string(
        root().join(format!("testdata/pack/gtnh-2.9.0/{dir}/structures.json")),
    ) else {
        eprintln!("skip: no structures.json");
        return;
    };
    let d: serde_json::Value = serde_json::from_str(&s).unwrap();
    let seed: i64 = d["worldSeed"]
        .as_i64()
        .or_else(|| d["worldSeed"].as_str().and_then(|s| s.parse().ok()))
        .unwrap();
    assert!(d["continental"].as_bool().unwrap());
    let vg = &d["villageGenerator"];
    assert_eq!(
        vg["class"].as_str().unwrap(),
        "astrotibs.villagenames.village.MapGenVillageVN"
    );
    let r = d["scanRadiusChunks"].as_i64().unwrap() as i32;
    // rwg2dump calls func_151539_a at centres -r, -r+17, … ≤ r, each covering ±8 chunks.
    let last = -r + ((2 * r) / 17) * 17;
    let (lo, hi) = (-r - 8, last + 8);

    let mut want: Vec<(i32, i32, bool, Option<Style>, usize)> = vg["starts"]
        .as_array()
        .unwrap()
        .iter()
        .map(|e| {
            let pieces = e["pieces"].as_array().unwrap();
            (
                e["chunkX"].as_i64().unwrap() as i32,
                e["chunkZ"].as_i64().unwrap() as i32,
                e["sizeable"].as_bool().unwrap(),
                style_of_piece(pieces[0]["class"].as_str().unwrap()),
                e["components"].as_u64().unwrap() as usize,
            )
        })
        .collect();
    want.sort_by_key(|w| (w.0, w.1));

    let table = BiomeTable::from_rwg2dump_json(
        &std::fs::read_to_string(root().join(pack_rel("biomes.json"))).unwrap(),
    )
    .unwrap();
    let p = BiomeProvider::new(seed, WorldType::Rwg2Continents, table, RwgConfig::default());
    let h = TableHeights;
    let mut vs = Villages::gtnh_2_9_0();
    let t0 = std::time::Instant::now();
    let all = vs.in_rect(&p, &h, seed, lo, lo, hi, hi);
    let dt_rect = t0.elapsed();
    let mut got: Vec<_> = all.iter().filter(|v| v.viable_exact).cloned().collect();
    got.sort_by_key(|v| (v.cx, v.cz));

    eprintln!(
        "scan chunks [{lo}, {hi}]²: {} candidates, {} provider-viable, {} in dump ({:?})",
        all.len(),
        got.len(),
        want.len(),
        dt_rect
    );
    for v in &all {
        eprintln!(
            "  cand ({:4},{:4}) biome {:3} style {:8} viable {}",
            v.cx,
            v.cz,
            v.biome,
            v.style.name(),
            v.viable_exact
        );
    }
    let gs: Vec<(i32, i32)> = got.iter().map(|v| (v.cx, v.cz)).collect();
    let ws: Vec<(i32, i32)> = want.iter().map(|w| (w.0, w.1)).collect();
    assert_eq!(gs, ws, "village start set");
    let mut style_ok = 0;
    for (v, w) in got.iter().zip(&want) {
        if Some(v.style) == w.3 {
            style_ok += 1;
        } else {
            eprintln!(
                "  style mismatch at ({},{}): got {:?}, dump {:?}",
                v.cx, v.cz, v.style, w.3
            );
        }
    }
    let sizeable = want.iter().filter(|w| w.2).count();
    let comps: Vec<usize> = want.iter().map(|w| w.4).collect();
    eprintln!(
        "starts {}/{} exact, style {}/{}, sizeable in dump {}/{} (components {:?})",
        gs.len(),
        ws.len(),
        style_ok,
        want.len(),
        sizeable,
        want.len(),
        comps
    );
    assert_eq!(style_ok, want.len());

    // every candidate chunk passes is_candidate; neighbours don't
    for v in &all {
        assert!(is_candidate(seed, v.cx, v.cz));
        assert!(!is_candidate(seed, v.cx + 1, v.cz) || (v.cx + 1).rem_euclid(SPACING) == 0);
    }

    // chunk-biome second chance for the provider-rejected candidates
    let t1 = std::time::Instant::now();
    let mut late = 0;
    let mut rejected = 0;
    for v in all.iter().filter(|v| !v.viable_exact) {
        let mut v = *v;
        vs.resolve_chunk_gate(&p, &h, &mut v);
        match v.gate {
            Some(Gate::ChunkBiome) => {
                late += 1;
                eprintln!("  chunk-biome second chance: ({},{})", v.cx, v.cz);
            }
            _ => rejected += 1,
        }
    }
    eprintln!(
        "provider-rejected: {} chunk-biome-allowed, {} rejected ({:?})",
        late,
        rejected,
        t1.elapsed()
    );

    // nearest search agrees with the rect scan
    let t2 = std::time::Instant::now();
    let n = vs.nearest_within(&p, &h, seed, 0, 0, 2000);
    let dt_n = t2.elapsed();
    let best = got
        .iter()
        .map(|v| ((v.x as i64).pow(2) + (v.z as i64).pow(2), v.cx, v.cz))
        .filter(|b| b.0 <= 2000 * 2000)
        .min();
    assert_eq!(n.map(|(v, d)| (d, v.cx, v.cz)), best);
    eprintln!(
        "nearest within 2000 of origin: {:?} ({:?})",
        n.map(|(v, d)| (v.cx, v.cz, (d as f64).sqrt() as i32)),
        dt_n
    );

    // cost per 2048-block cell (128x128 chunks)
    let t3 = std::time::Instant::now();
    let mut k = 0;
    for i in 0..16 {
        let c0 = (i % 4 - 2) * 128;
        let c1 = (i / 4 - 2) * 128;
        k += vs.in_rect(&p, &h, seed, c0, c1, c0 + 127, c1 + 127).len();
    }
    eprintln!("16 cells: {k} candidates, {:?} per cell", t3.elapsed() / 16);
}

/// rwg2dump 2.8.1 `villages.json`: the game's own `canSpawnStructureAtCoords` for 2304 regions (±1200 chunks) of
/// seed 92900795, plus whether each start is sizeable.
#[test]
fn villages_scale_dump281() {
    let Ok(s) =
        std::fs::read_to_string(root().join("testdata/pack/gtnh-2.9.0/dump281/villages.json"))
    else {
        eprintln!("skip");
        return;
    };
    let d: serde_json::Value = serde_json::from_str(&s).unwrap();
    let seed: i64 = d["worldSeed"].as_str().unwrap().parse().unwrap();
    let table = BiomeTable::from_rwg2dump_json(
        &std::fs::read_to_string(root().join(pack_rel("biomes.json"))).unwrap(),
    )
    .unwrap();
    let p = BiomeProvider::new(seed, WorldType::Rwg2Continents, table, RwgConfig::default());
    let h = TableHeights;
    let vs = Villages::gtnh_2_9_0();
    let (mut pos_ok, mut verdict_ok, mut real, mut small) = (0, 0, 0, 0);
    let mut bad = Vec::new();
    for c in d["candidates"].as_array().unwrap() {
        let (cx, cz) = (
            c["chunkX"].as_i64().unwrap() as i32,
            c["chunkZ"].as_i64().unwrap() as i32,
        );
        let ok = c["canSpawn"].as_bool().unwrap();
        real += ok as usize;
        small += (ok && !c["sizeable"].as_bool().unwrap_or(true)) as usize;
        pos_ok += is_candidate(seed, cx, cz) as usize;
        let got = vs.in_rect(&p, &h, seed, cx, cz, cx, cz);
        let v = got.iter().find(|v| (v.cx, v.cz) == (cx, cz));
        let pred = v.is_some_and(|v| v.viable_exact);
        if pred == ok {
            verdict_ok += 1;
        } else {
            bad.push((
                cx,
                cz,
                ok,
                c["biomeName"].as_str().unwrap_or("").to_string(),
                v.map(|v| v.biome),
            ));
        }
    }
    for b in bad.iter().take(20) {
        eprintln!("  mismatch {b:?}");
    }
    eprintln!(
        "candidates {} | position {pos_ok} | verdict {verdict_ok} | real villages {real} (not sizeable {small})",
        d["candidates"].as_array().unwrap().len()
    );
    assert_eq!(verdict_ok, d["candidates"].as_array().unwrap().len());
}

/// `data/<PACK_DATA>/<name>` relative to the repo (`PACK_DATA` default `gtnh-2.9.0`; `gtnh-2.9.1` = RWG2 2.0.9 pack).
#[allow(dead_code)]
fn pack_rel(name: &str) -> String {
    format!("data/{}/{name}", std::env::var("PACK_DATA").unwrap_or_else(|_| "gtnh-2.9.0".into()))
}
