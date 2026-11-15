//! Nether fortress layouts vs rwg2dump 2.8 (`dump28/dims.json` `nether.structures`: real starts with every piece
//! box after `setRandomHeight`), world seed 92900795.
#![cfg(feature = "json")]

use rwg2_biomes::fortress::*;
use serde_json::Value;

const SEED: i64 = 92_900_795;

fn root() -> std::path::PathBuf {
    std::env::var("RWG2_DATA_DIR")
        .map(|d| std::path::PathBuf::from(d).parent().unwrap().to_path_buf())
        .unwrap_or_else(|_| std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../.."))
}

fn i(v: &Value) -> i32 {
    v.as_i64().unwrap() as i32
}

#[test]
fn fortress_pieces_match_dump28() {
    let Ok(s) = std::fs::read_to_string(root().join("testdata/pack/gtnh-2.9.0/dump28/dims.json"))
    else {
        eprintln!("skip");
        return;
    };
    let d: Value = serde_json::from_str(&s).unwrap();
    let structs = d["nether"]["structures"].as_array().unwrap();
    let (mut starts_ok, mut pieces_total, mut pieces_ok, mut box_ok) = (0, 0, 0, 0);
    let t0 = std::time::Instant::now();
    for st in structs {
        let (cx, cz) = (i(&st["chunkX"]), i(&st["chunkZ"]));
        let l =
            layout(SEED, cx, cz).unwrap_or_else(|| panic!("({cx},{cz}) is not a fortress start"));
        let want: Vec<(String, BBox)> = st["pieces"]
            .as_array()
            .unwrap()
            .iter()
            .map(|p| {
                let a = p.as_array().unwrap();
                let name = a[0]
                    .as_str()
                    .unwrap()
                    .rsplit('$')
                    .next()
                    .unwrap()
                    .to_string();
                (
                    name,
                    [i(&a[1]), i(&a[2]), i(&a[3]), i(&a[4]), i(&a[5]), i(&a[6])],
                )
            })
            .collect();
        let sb = st["box"].as_array().unwrap();
        let wbox: BBox = [
            i(&sb[0]),
            i(&sb[1]),
            i(&sb[2]),
            i(&sb[3]),
            i(&sb[4]),
            i(&sb[5]),
        ];
        box_ok += (l.bbox == wbox) as usize;
        pieces_total += want.len();
        let same = l.pieces.len() == want.len();
        let mut first_bad = None;
        for (k, (p, w)) in l.pieces.iter().zip(&want).enumerate() {
            if p.kind.java_name() == w.0 && p.bbox == w.1 {
                pieces_ok += 1;
            } else if first_bad.is_none() {
                first_bad = Some((k, p.kind.java_name(), p.bbox, w.clone()));
            }
        }
        if same && first_bad.is_none() && l.bbox == wbox {
            starts_ok += 1;
        } else {
            eprintln!(
                "({cx},{cz}): pieces {} vs {} box {:?} vs {:?} first diff {:?}",
                l.pieces.len(),
                want.len(),
                l.bbox,
                wbox,
                first_bad
            );
        }
    }
    let dt = t0.elapsed() / structs.len() as u32;
    eprintln!(
        "fortresses exact {starts_ok}/{} | pieces {pieces_ok}/{pieces_total} | bounding box {box_ok}/{} | {dt:?} per fortress",
        structs.len(),
        structs.len()
    );
    assert_eq!(starts_ok, structs.len());

    // features: spawners in thrones, wart rooms
    let l = layout(SEED, i(&structs[0]["chunkX"]), i(&structs[0]["chunkZ"])).unwrap();
    for s in l.spawners() {
        assert!(l.pieces.iter().any(|p| p.kind == PieceKind::Throne
            && (p.bbox[0]..=p.bbox[3]).contains(&s.0)
            && (p.bbox[2]..=p.bbox[5]).contains(&s.2)));
    }
    for w in l.wart_rooms() {
        for b in w.wart {
            assert_eq!((b[3] - b[0] + 1) * (b[5] - b[2] + 1), 10);
        }
    }
}

/// Region search agrees with the dumped start list (all 350 starts within 256 chunks have a layout).
#[test]
fn every_dump_start_has_layout() {
    let Ok(s) = std::fs::read_to_string(root().join("testdata/pack/gtnh-2.9.0/dump28/dims.json"))
    else {
        return;
    };
    let d: Value = serde_json::from_str(&s).unwrap();
    let mut ys = std::collections::BTreeMap::new();
    for st in d["nether"]["starts"].as_array().unwrap() {
        let a = st.as_array().unwrap();
        let l = layout(SEED, i(&a[0]), i(&a[1])).expect("layout");
        assert!((48..=70).contains(&l.bbox[1]));
        *ys.entry(l.bbox[1]).or_insert(0) += 1;
    }
    eprintln!("start minY histogram {ys:?}");
}

/// Every `probe219.json` under testdata (rwg2dump 2.19 probe).
fn probes219() -> Vec<std::path::PathBuf> {
    fn walk(d: &std::path::Path, out: &mut Vec<std::path::PathBuf>) {
        let Ok(rd) = std::fs::read_dir(d) else { return };
        for e in rd.flatten() {
            let p = e.path();
            if p.is_dir() {
                walk(&p, out);
            } else if p.file_name().is_some_and(|n| n == "probe219.json") {
                out.push(p);
            }
        }
    }
    let mut v = Vec::new();
    walk(&root().join("testdata"), &mut v);
    v
}

/// Fortress chests (positions + contents) vs rwg2dump 2.19 `fortressChests` (Nether chunks ±24 force-loaded,
/// every chest tile entity with its slots). Skips without data.
#[test]
fn fortress_chests_vs_probe219() {
    let files = probes219();
    if files.is_empty() {
        eprintln!("no probe219.json: skipped");
        return;
    }
    for f in files {
        let j: Value = serde_json::from_str(&std::fs::read_to_string(&f).unwrap()).unwrap();
        // 2.19.0 nested it under spawnLists.biomes (the spawn-list writer threw mid-object)
        let fc = if j["fortressChests"].is_object() { &j["fortressChests"] } else { &j["spawnLists"]["biomes"]["fortressChests"] };
        let Some(seed) = fc["worldSeed"].as_str().and_then(|s| s.parse::<i64>().ok()) else {
            eprintln!("{}: no fortressChests ({})", f.display(), fc["error"]);
            continue;
        };
        let r = fc["range"].as_i64().unwrap_or(24) as i32;
        eprintln!("== {} seed {seed}: generator {} mapGen {} starts {}", f.display(), fc["generator"], fc["mapGen"], fc["starts"]);
        // model fortresses reaching the populated chunks
        let mut layouts = Vec::new();
        for rz in ((-r - 8) >> 4)..=((r + 8) >> 4) {
            for rx in ((-r - 8) >> 4)..=((r + 8) >> 4) {
                if let Some(l) = layout_in_region(seed, rx, rz) {
                    layouts.push(l);
                }
            }
        }
        // real chests: pos -> slots (slot, registry id, count)
        type Slots = Vec<(u8, String, u8)>;
        let mut real: std::collections::BTreeMap<(i32, i32, i32), Slots> = Default::default();
        for c in fc["chests"].as_array().into_iter().flatten() {
            let p: Vec<i32> = c["pos"].as_array().unwrap().iter().map(|v| v.as_i64().unwrap() as i32).collect();
            let slots = c["slots"]
                .as_array()
                .unwrap()
                .iter()
                .map(|s| {
                    let st = s[1].as_str().unwrap_or("");
                    let n: u8 = st.split('x').next().and_then(|v| v.parse().ok()).unwrap_or(0);
                    (s[0].as_u64().unwrap() as u8, s[2].as_str().unwrap_or("?").to_string(), n)
                })
                .collect();
            real.insert((p[0], p[1], p[2]), slots);
        }
        // predicted chests inside the populated chunks, with loot per populating chunk
        // populated (populating chunk within ±(r−1)) and inside the probe's tile-entity scan (chest chunk ±(r−1))
        let inner = |(x, _, z): &(i32, i32, i32)| {
            let (cx, cz) = populating_chunk(*x, *z);
            cx.abs() < r && cz.abs() < r && (x >> 4).abs() < r && (z >> 4).abs() < r
        };
        let mut by_chunk: std::collections::BTreeMap<(i32, i32), ()> = Default::default();
        for l in &layouts {
            for c in l.chests().iter().filter(|c| inner(c)) {
                by_chunk.insert(populating_chunk(c.0, c.2), ());
            }
        }
        let (mut pred, mut at, mut same) = (0, 0, 0);
        for &(cx, cz) in by_chunk.keys() {
            let reach: Vec<&FortressLayout> = layouts.iter().filter(|l| l.chests().iter().any(|c| populating_chunk(c.0, c.2) == (cx, cz))).collect();
            for (pos, slots) in chunk_chest_loot(seed, &reach, cx, cz) {
                pred += 1;
                let model: Vec<(u8, String, u8)> =
                    slots.iter().map(|&(s, k, n)| (s, FORTRESS_LOOT[k as usize].0.to_string(), n)).collect();
                match real.get(&pos) {
                    Some(rs) => {
                        at += 1;
                        let ok = *rs == model;
                        same += ok as u32;
                        if !ok {
                            eprintln!("  chest {pos:?}: real {rs:?}\n                    model {model:?}");
                        }
                    }
                    None => eprintln!("  predicted chest {pos:?} not found"),
                }
            }
        }
        let fortress_real = real.keys().filter(|p| layouts.iter().any(|l| l.chests().contains(p))).count();
        eprintln!(
            "fortress chests: predicted {pred}, real chest at the position {at}, identical contents {same}; real chests {} ({} at a model chest position)",
            real.len(),
            fortress_real
        );
        assert_eq!(at, pred, "every predicted chest exists");
        assert_eq!(same, at, "contents");
    }
}

/// Model chest count for the 2.19 owner run: fortress chests with populating chunk within ±23 of the Nether origin.
#[test]
#[ignore]
fn fortress_chests_preview() {
    let seed: i64 = std::env::var("SEED").ok().and_then(|s| s.parse().ok()).unwrap_or(3380521564351534918);
    let r = 24;
    let mut n = 0;
    for rz in ((-r - 8) >> 4)..=((r + 8) >> 4) {
        for rx in ((-r - 8) >> 4)..=((r + 8) >> 4) {
            if let Some(l) = layout_in_region(seed, rx, rz) {
                for c in l.chests() {
                    let (cx, cz) = populating_chunk(c.0, c.2);
                    if cx.abs() < r && cz.abs() < r {
                        n += 1;
                        let loot = chunk_chest_loot(seed, &[&l], cx, cz);
                        eprintln!("chest {c:?}: {:?}", loot.iter().find(|x| x.0 == c).map(|x| x.1.iter().map(|&(s, k, n)| (s, FORTRESS_LOOT[k as usize].0, n)).collect::<Vec<_>>()));
                    }
                }
            }
        }
    }
    eprintln!("{n} predicted fortress chests within ±{r} chunks of seed {seed}");
}

/// Raw draws behind each real fortress-chest slot (for fitting the modded loot table): prints
/// `OBS item count u_pick u_count` per slot, where u = raw `next(31)` of the pick / count draw.
#[test]
#[ignore]
fn fortress_loot_raw_draws() {
    for f in probes219() {
        let j: Value = serde_json::from_str(&std::fs::read_to_string(&f).unwrap()).unwrap();
        let fc = if j["fortressChests"].is_object() { &j["fortressChests"] } else { &j["spawnLists"]["biomes"]["fortressChests"] };
        let seed: i64 = fc["worldSeed"].as_str().unwrap().parse().unwrap();
        let mut layouts = Vec::new();
        for rz in -3..=3 {
            for rx in -3..=3 {
                if let Some(l) = layout_in_region(seed, rx, rz) {
                    layouts.push(l);
                }
            }
        }
        type Slots = Vec<(u8, String, u8)>;
        let mut real: std::collections::BTreeMap<(i32, i32, i32), Slots> = Default::default();
        for c in fc["chests"].as_array().into_iter().flatten() {
            let p: Vec<i32> = c["pos"].as_array().unwrap().iter().map(|v| v.as_i64().unwrap() as i32).collect();
            let slots = c["slots"].as_array().unwrap().iter().map(|s| {
                let st = s[1].as_str().unwrap_or("");
                (s[0].as_u64().unwrap() as u8, s[2].as_str().unwrap_or("?").to_string(), st.split('x').next().and_then(|v| v.parse().ok()).unwrap_or(0))
            }).collect();
            real.insert((p[0], p[1], p[2]), slots);
        }
        let mut chunks = std::collections::BTreeSet::new();
        for l in &layouts { for c in l.chests() { if real.contains_key(&c) { chunks.insert(populating_chunk(c.0, c.2)); } } }
        for (cx, cz) in chunks {
            let (x0, z0) = (cx * 16 + 8, cz * 16 + 8);
            let mut r = hell_rand(seed, cx, cz);
            for l in &layouts {
                for c in l.chests().into_iter().filter(|p| (x0..x0 + 16).contains(&p.0) && (z0..z0 + 16).contains(&p.2)) {
                    let n = 2 + r.next_int_bounded(4);
                    let mut last: [Option<(i32, i32)>; 27] = [None; 27];
                    for _ in 0..n {
                        let a = r.next(31);
                        let b = r.next(31);
                        let s = r.next_int_bounded(27);
                        last[s as usize] = Some((a, b));
                    }
                    let Some(rs) = real.get(&c) else { continue };
                    for (s, id, cnt) in rs {
                        if let Some((a, b)) = last[*s as usize] {
                            println!("OBS {id} {cnt} {a} {b}");
                        }
                    }
                }
            }
        }
    }
}
