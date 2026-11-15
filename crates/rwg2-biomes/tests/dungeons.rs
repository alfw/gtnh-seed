#![cfg(feature = "json")]
use rwg2_biomes::config::RwgConfig;
use rwg2_biomes::dungeons::*;
use rwg2_biomes::forestry::ClimateTable;
use rwg2_biomes::provider::{BiomeProvider, BiomeTable, TableHeights, WorldType};

fn root() -> std::path::PathBuf {
    std::env::var("RWG2_DATA_DIR")
        .map(|d| std::path::PathBuf::from(d).parent().unwrap().to_path_buf())
        .unwrap_or_else(|_| std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../.."))
}

#[test]
fn dungeons_match_pack() {
    let Ok(s) =
        std::fs::read_to_string(root().join("testdata/pack/gtnh-2.9.0/dump22/dungeons.json"))
    else {
        eprintln!("skip");
        return;
    };
    let d: serde_json::Value = serde_json::from_str(&s).unwrap();
    let seed: i64 = d["worldSeed"].as_str().unwrap().parse().unwrap();
    // LootGames chunks: exact set within +-128
    let want: Vec<(i32, i32)> = d["lootgames"]["dungeons"]
        .as_array()
        .unwrap()
        .iter()
        .map(|e| {
            (
                e["chunkX"].as_i64().unwrap() as i32,
                e["chunkZ"].as_i64().unwrap() as i32,
            )
        })
        .collect();
    let mut got = lootgames_chunks(seed, 20, -128, -128, 128, 128);
    let mut w2 = want.clone();
    w2.sort_by_key(|&(x, z)| (z, x));
    got.sort_by_key(|&(x, z)| (z, x));
    assert_eq!(got, w2, "lootgames chunk set");
    for &(cx, cz) in &want {
        assert!(lootgames_can_spawn(seed, 20, cx, cz));
    }
    // Roguelike anchors: exact set
    let anchors: Vec<(i32, i32)> = d["roguelike"]["anchors"]
        .as_array()
        .unwrap()
        .iter()
        .map(|e| {
            (
                e["chunkX"].as_i64().unwrap() as i32,
                e["chunkZ"].as_i64().unwrap() as i32,
            )
        })
        .collect();
    let mut ga = rogue_anchors(seed, &ROGUE_GTNH, -128, -128, 128, 128);
    let mut wa = anchors.clone();
    ga.sort();
    wa.sort();
    assert_eq!(ga, wa, "roguelike anchors");

    // attempts (exact positions) + validLocation verdicts (terrain model)
    let table = BiomeTable::from_rwg2dump_json(
        &std::fs::read_to_string(root().join("data/gtnh-2.9.0/biomes.json")).unwrap(),
    )
    .unwrap();
    let climate = ClimateTable::from_json(
        &std::fs::read_to_string(root().join("data/gtnh-2.9.0/climate.json")).unwrap(),
    )
    .unwrap();
    let p = BiomeProvider::new(seed, WorldType::Rwg2Continents, table, RwgConfig::default());
    let mut t = Terrain::new(&p, &TableHeights);
    let (mut pos_ok, mut pos_n, mut v_ok, mut v_n, mut fp, mut fn_) = (0, 0, 0, 0, 0, 0);
    let (mut chosen_ok, mut chosen_n) = (0, 0);
    let (mut as_is_ok, mut loaded_before) = (0, 0);
    for a in d["roguelike"]["anchors"].as_array().unwrap() {
        let Some(att) = a["attempts"].as_array() else {
            continue;
        };
        let (cx, cz) = (
            a["chunkX"].as_i64().unwrap() as i32,
            a["chunkZ"].as_i64().unwrap() as i32,
        );
        let mine = rogue_attempts(seed, cx, cz);
        let mut want_pos = None;
        for (i, e) in att.iter().enumerate() {
            let e = e.as_array().unwrap();
            let (x, z, ok) = (
                e[0].as_i64().unwrap() as i32,
                e[1].as_i64().unwrap() as i32,
                e[2].as_bool().unwrap(),
            );
            if e.len() > 5 {
                as_is_ok += e[4].as_bool().unwrap() as i32;
                loaded_before += e[5].as_bool().unwrap() as i32;
            }
            pos_n += 1;
            if mine[i] == (x, z) {
                pos_ok += 1;
            }
            let v = rogue_valid(&mut t, &climate, &ROGUE_GTNH, x, z, true).unwrap();
            v_n += 1;
            if v == ok {
                v_ok += 1;
            } else if v {
                fp += 1;
            } else {
                fn_ += 1;
            }
            if ok {
                want_pos = Some((x, z));
            }
        }
        let r = rogue_dungeon(&mut t, &climate, &ROGUE_GTNH, seed, cx, cz);
        chosen_n += 1;
        if r.pos == want_pos {
            chosen_ok += 1;
        } else {
            eprintln!("anchor ({cx},{cz}): got {:?} want {want_pos:?}", r.pos);
        }
    }
    // LootGames surface y
    let (mut s_ok, mut s_n, mut s_hist) = (0, 0, std::collections::BTreeMap::<i32, i32>::new());
    for e in d["lootgames"]["dungeons"].as_array().unwrap() {
        let Some(sy) = e["surfaceY"].as_i64() else {
            continue;
        };
        let (x, z) = (
            e["x"].as_i64().unwrap() as i32,
            e["z"].as_i64().unwrap() as i32,
        );
        let my = lootgames_surface(&mut t, x, z).unwrap();
        s_n += 1;
        *s_hist.entry((my - sy as i32).clamp(-5, 5)).or_default() += 1;
        if my == sy as i32 {
            s_ok += 1;
        }
    }
    eprintln!(
        "lootgames {} chunks exact; roguelike {} anchors exact",
        want.len(),
        anchors.len()
    );
    eprintln!("attempt positions {pos_ok}/{pos_n}; validLocation {v_ok}/{v_n} (model says valid but game not: {fp}, reverse: {fn_}); chosen spot {chosen_ok}/{chosen_n}");
    eprintln!(
        "as-is (no force-load) valid {as_is_ok}, attempt chunk loaded before {loaded_before}"
    );
    eprintln!("lootgames surface y {s_ok}/{s_n} exact; diff hist {s_hist:?}");
    assert_eq!(pos_ok, pos_n);
    assert!(chosen_ok * 3 >= chosen_n, "chosen {chosen_ok}/{chosen_n}");
}
