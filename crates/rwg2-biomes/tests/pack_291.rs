//! The RWG2 2.0.9 pack (GTNH 2.9.1, `data/gtnh-2.9.1`, worlds in `testdata/pack/gtnh-2.9.1/heldout/<seed>/`): the
//! pack-specific tables against that pack's dumps. Own test binary: `pack::set` is process-wide.
use rwg2_biomes::java_random::JavaRandom;
use rwg2_biomes::loot::LootTable;
use rwg2_biomes::pack::{self, Pack};
use std::path::PathBuf;

fn root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..")
}

fn worlds() -> Vec<PathBuf> {
    let mut v: Vec<PathBuf> = std::fs::read_dir(root().join("testdata/pack/gtnh-2.9.1/heldout"))
        .map(|rd| rd.flatten().map(|e| e.path()).filter(|p| p.join("env.json").exists()).collect())
        .unwrap_or_default();
    v.sort();
    v
}

/// Dungeon chests: the 140-entry table (2.9's 139 + EMT "The One Ring") vs every world's in-game probes.
#[test]
fn dungeon_loot_probes_next() {
    pack::set(Pack::Gtnh291);
    let ws = worlds();
    if ws.is_empty() {
        eprintln!("skip: no testdata/pack/gtnh-2.9.1/heldout");
        return;
    }
    let t = LootTable::dungeon();
    assert_eq!(t.items.len(), 140);
    let (mut ok, mut n) = (0, 0);
    for w in ws {
        let Ok(s) = std::fs::read_to_string(w.join("dungeonloot.json")) else { continue };
        let v: serde_json::Value = serde_json::from_str(&s).unwrap();
        assert_eq!(t.items.len(), v["items"].as_array().unwrap().len());
        for p in v["probes"].as_array().unwrap() {
            let p = p.as_array().unwrap();
            let seed: i64 = p[0].as_str().unwrap().parse().unwrap();
            let mut r = JavaRandom::new(seed);
            let c = t.roll(&mut r);
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
            let exp: Vec<String> = p[4..].iter().map(|x| x.as_str().unwrap().to_string()).collect();
            n += 1;
            if calls == p[1].as_i64().unwrap() && got == exp {
                ok += 1;
            } else {
                eprintln!("miss {seed}: calls {calls} vs {}; {got:?} vs {exp:?}", p[1]);
            }
        }
    }
    eprintln!("2.9.1 dungeon loot probes: {ok}/{n}");
    assert!(n > 0);
    assert_eq!(ok, n);
}

/// The 2.9 table must not fit the new pack's probes (guards the pack switch actually changing the table).
#[test]
fn pack_tables_differ() {
    assert_ne!(
        Pack::Gtnh290.dungeon_loot_json().matches("[\"").count(),
        Pack::Gtnh291.dungeon_loot_json().matches("[\"").count()
    );
}
