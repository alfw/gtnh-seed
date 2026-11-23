//! Dungeon chest loot vs the in-game probes of `dungeonloot.json` (rwg2dump 2.11.0). Skips without the dump.
use rwg2_biomes::java_random::JavaRandom;
use rwg2_biomes::loot::LootTable;

#[test]
fn dungeon_loot_probes() {
    let root = std::env::var("RWG2_DATA_DIR")
        .map(std::path::PathBuf::from)
        .unwrap_or_else(|_| std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../.."));
    let f = root.join("testdata/pack/gtnh-2.10.0/heldout/4285358484800856671/dungeonloot.json");
    let Ok(s) = std::fs::read_to_string(&f) else {
        eprintln!("skip: {f:?}");
        return;
    };
    let v: serde_json::Value = serde_json::from_str(&s).unwrap();
    let t = LootTable::dungeon();
    assert_eq!(t.items.len(), v["items"].as_array().unwrap().len());
    let (mut ok, mut n) = (0, 0);
    for p in v["probes"].as_array().unwrap() {
        let p = p.as_array().unwrap();
        let seed: i64 = p[0].as_str().unwrap().parse().unwrap();
        let mut r = JavaRandom::new(seed);
        let c = t.roll(&mut r);
        // count draws by replaying the LCG from the starting state
        let mut q = JavaRandom::new(seed);
        let mut calls = 0;
        while q.state() != r.state() {
            q.next(1);
            calls += 1;
            assert!(calls < 1000);
        }
        let got: Vec<String> = c
            .slots
            .iter()
            .map(|&(s, k, n)| format!("{s}:{n}x{}", t.items[k as usize].name))
            .collect();
        let exp: Vec<String> = p[4..]
            .iter()
            .map(|x| x.as_str().unwrap().to_string())
            .collect();
        n += 1;
        if calls == p[1].as_i64().unwrap() && got == exp {
            ok += 1;
        } else {
            eprintln!(
                "miss {seed}: calls {calls} vs {} ; {got:?} vs {exp:?}",
                p[1]
            );
        }
    }
    eprintln!("dungeon loot probes: {ok}/{n}");
    assert_eq!(ok, n);
}

/// `LootTable::category` on the compact multi-category form `tools/chestloot.py` writes.
#[test]
fn chest_loot_category_parse() {
    let s = "{\"source\":\"x\",\n\"mineshaftCorridor\":{\"count\":[3, 6],\"items\":[[\"item.saddle@0\",1,1,10,1],\n[\"item.enchantedBook@0\",1,1,1,1]]},\n\"dungeonChest\":{\"count\":[8, 8],\"items\":[[\"item.bread@0\",1,1,10,64]]}}";
    let m = LootTable::category(s, "mineshaftCorridor").unwrap();
    assert_eq!((m.items.len(), m.count, m.total), (2, (3, 6), 11));
    assert!(m.items[1].book && !m.items[0].book);
    let d = LootTable::category(s, "dungeonChest").unwrap();
    assert_eq!((d.items.len(), d.count), (1, (8, 8)));
    assert!(LootTable::category(s, "villageBlacksmith").is_none());
    assert!(LootTable::category("{}", "mineshaftCorridor").is_none());
}

/// Every category of a rwg2dump 2.17 `chestloot.json` vs its in-game probes (skips without one). The order of
/// draws on a mineshaft hit (getItems, getCount, then rail meta + contents only when placed) is checked against
/// dump 2.16 in `deco_dump.rs` (`mineshaft_chest_draws_216`).
#[test]
fn chest_loot_probes_217() {
    let root = std::env::var("RWG2_DATA_DIR")
        .map(std::path::PathBuf::from)
        .unwrap_or_else(|_| std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../.."));
    let mut files = Vec::new();
    fn walk(d: &std::path::Path, out: &mut Vec<std::path::PathBuf>) {
        for e in std::fs::read_dir(d).into_iter().flatten().flatten() {
            let p = e.path();
            if p.is_dir() && !skip_dump_dir(&p) {
                walk(&p, out);
            } else if p.file_name().is_some_and(|n| n == "chestloot.json") {
                out.push(p);
            }
        }
    }
    walk(&root.join(dumps_under()), &mut files);
    if files.is_empty() {
        eprintln!("skip: no rwg2dump 2.17 chestloot.json under testdata/pack");
        return;
    }
    for f in files {
        let v: serde_json::Value =
            serde_json::from_str(&std::fs::read_to_string(&f).unwrap()).unwrap();
        for (cat, c) in v["categories"].as_object().unwrap() {
            let Some(items) = c["items"].as_array() else {
                eprintln!("{cat}: {}", c["error"]);
                continue;
            };
            let mut s = format!(
                "{{\"count\":[{}, {}],\"items\":[",
                c["countMin"], c["countMax"]
            );
            for (k, i) in items.iter().enumerate() {
                let st = i["stack"].as_str().unwrap();
                let name = st.split_once('x').map_or(st, |x| x.1);
                s += &format!(
                    "{}[\"{name}\",{},{},{},{}]",
                    if k > 0 { "," } else { "" },
                    i["min"],
                    i["max"],
                    i["weight"],
                    i.get("maxStack").and_then(|m| m.as_i64()).unwrap_or(64)
                );
            }
            s += "]}";
            let t = LootTable::from_json(&s).unwrap();
            let (mut ok, mut n) = (0, 0);
            for p in c["probes"].as_array().into_iter().flatten() {
                let p = p.as_array().unwrap();
                let seed: i64 = p[0].as_str().unwrap().parse().unwrap();
                let mut r = JavaRandom::new(seed);
                let ch = t.roll(&mut r);
                let mut q = JavaRandom::new(seed);
                let mut calls = 0;
                while q.state() != r.state() && calls < 5000 {
                    q.next(1);
                    calls += 1;
                }
                let got: Vec<String> = ch
                    .slots
                    .iter()
                    .map(|&(s, k, n)| format!("{s}:{n}x{}", t.items[k as usize].name))
                    .collect();
                let exp: Vec<String> = p[4..]
                    .iter()
                    .map(|x| x.as_str().unwrap().to_string())
                    .collect();
                n += 1;
                if calls == p[1].as_i64().unwrap() && got == exp {
                    ok += 1;
                } else if n - ok <= 4 {
                    eprintln!("  {cat} miss {seed}: calls {calls} vs {}\n    got {got:?}\n    exp {exp:?}", p[1]);
                }
            }
            eprintln!("{} {cat}: probes {ok}/{n}", f.display());
            assert_eq!(ok, n, "{cat}");
            if cat == "mineshaftCorridor" {
                if let Some(e) = LootTable::mineshaft() {
                    assert_eq!(
                        e.items.len(),
                        t.items.len(),
                        "embedded mineshaft table differs from the dump"
                    );
                }
            }
        }
    }
}

/// Dump walk root: `DUMPS_UNDER` (e.g. `testdata/pack/gtnh-2.9.1` with `PACK_DATA=gtnh-2.9.1`), else `testdata/pack`
/// minus the RWG2 2.0.9 pack's folder (its worlds need the 2.0.9 data).
#[allow(dead_code)]
fn dumps_under() -> String {
    std::env::var("DUMPS_UNDER").unwrap_or_else(|_| "testdata/pack".into())
}

/// Without `DUMPS_UNDER`, the RWG2 2.0.9 pack's dumps (`testdata/pack/gtnh-2.9.1`) are left out of the walk.
#[allow(dead_code)]
fn skip_dump_dir(p: &std::path::Path) -> bool {
    std::env::var("DUMPS_UNDER").is_err() && p.file_name().is_some_and(|n| n == "gtnh-2.9.1")
}
