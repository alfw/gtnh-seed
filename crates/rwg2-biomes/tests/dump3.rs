#![cfg(feature = "json")]
//! rwg2dump 2.16 sections other than the decoration replay (`deco_dump.rs`): parsers + a smoke test that loads each
//! section when present and prints what it holds. Skips without a 2.16 dump. Formats: `tools/rwg2dump/README216.md`.
use serde_json::Value;
use std::path::{Path, PathBuf};

fn root() -> PathBuf {
    std::env::var("RWG2_DATA_DIR")
        .map(|d| PathBuf::from(d).parent().unwrap().to_path_buf())
        .unwrap_or_else(|_| PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../.."))
}

fn dirs() -> Vec<PathBuf> {
    fn walk(d: &Path, out: &mut Vec<PathBuf>) {
        let Ok(rd) = std::fs::read_dir(d) else { return };
        for e in rd.flatten() {
            let p = e.path();
            if p.is_dir() {
                walk(&p, out);
            } else if p
                .file_name()
                .is_some_and(|n| n == "deco216.json" || n == "postpop216.json")
            {
                let d = p.parent().unwrap().to_path_buf();
                if !out.contains(&d) {
                    out.push(d);
                }
            }
        }
    }
    let mut v = Vec::new();
    walk(&root().join("testdata/pack"), &mut v);
    v.sort();
    v
}

fn json(d: &Path, f: &str) -> Option<Value> {
    serde_json::from_str(&std::fs::read_to_string(d.join(f)).ok()?).ok()
}

fn ints(v: &Value) -> Vec<i64> {
    v.as_array()
        .map(|a| a.iter().map(|x| x.as_i64().unwrap_or(-1)).collect())
        .unwrap_or_default()
}

/// A1: one Roguelike attempt.
#[derive(Debug)]
pub struct RogueAttempt {
    pub x: i32,
    pub z: i32,
    pub valid: bool,
    pub ground: i32,
    pub fresh: i64,
    /// 13×13 block indices at ground + 4 / ground − 3, row-major z then x from (x − 6, z − 6)
    pub above: Vec<i64>,
    pub below: Vec<i64>,
    /// 13×13 `getHeightValue`
    pub height: Vec<i64>,
}

pub fn rogue(pp: &Value) -> Vec<((i32, i32), Vec<RogueAttempt>)> {
    pp["roguelike"]["anchors"]
        .as_array()
        .into_iter()
        .flatten()
        .map(|a| {
            let at = a["attempts"]
                .as_array()
                .into_iter()
                .flatten()
                .map(|t| RogueAttempt {
                    x: t[0].as_i64().unwrap_or(0) as i32,
                    z: t[1].as_i64().unwrap_or(0) as i32,
                    valid: t[2].as_bool().unwrap_or(false),
                    ground: t[3].as_i64().unwrap_or(-1) as i32,
                    fresh: t[4].as_i64().unwrap_or(-1),
                    above: ints(&t[5]),
                    below: ints(&t[6]),
                    height: ints(&t[7]),
                })
                .collect();
            (
                (
                    a["chunkX"].as_i64().unwrap_or(0) as i32,
                    a["chunkZ"].as_i64().unwrap_or(0) as i32,
                ),
                at,
            )
        })
        .collect()
}

/// A4: one LootGames dungeon: (chunk, centre, surface found, surface y, fresh, 21×21 heights, 21×21 top blocks).
pub type Loot = (i32, i32, i32, i32, bool, i64, i64, Vec<i64>, Vec<i64>);
pub type Step = (i64, i32, i32, bool, i64, i64, Option<bool>, i64);

pub fn lootgames(pp: &Value) -> Vec<Loot> {
    pp["lootgames"]["dungeons"]
        .as_array()
        .into_iter()
        .flatten()
        .map(|d| {
            let i = |k: usize| d[k].as_i64().unwrap_or(0);
            (
                i(0) as i32,
                i(1) as i32,
                i(2) as i32,
                i(3) as i32,
                d[4].as_bool().unwrap_or(false),
                i(5),
                i(6),
                ints(&d[7]),
                ints(&d[8]),
            )
        })
        .collect()
}

/// A5: spawn walk steps: (i, x, z, loaded, heightValue, top block index, canSpawnNow, block under heightValue).
pub fn spawn_walk(pp: &Value) -> Vec<Step> {
    pp["spawnWalk"]["walk"]
        .as_array()
        .into_iter()
        .flatten()
        .map(|s| {
            (
                s[0].as_i64().unwrap_or(0),
                s[1].as_i64().unwrap_or(0) as i32,
                s[2].as_i64().unwrap_or(0) as i32,
                s[3].as_bool().unwrap_or(false),
                s[4].as_i64().unwrap_or(-1),
                s[5].as_i64().unwrap_or(-1),
                s[6].as_bool(),
                s[7].as_i64().unwrap_or(-1),
            )
        })
        .collect()
}

/// Node / tile columns: [x, y, z, [names y−6..y+6]].
pub fn node_columns(v: &Value) -> Vec<((i32, i32, i32), Vec<String>)> {
    v.as_array()
        .into_iter()
        .flatten()
        .map(|c| {
            (
                (
                    c[0].as_i64().unwrap_or(0) as i32,
                    c[1].as_i64().unwrap_or(0) as i32,
                    c[2].as_i64().unwrap_or(0) as i32,
                ),
                c[3].as_array()
                    .map(|a| {
                        a.iter()
                            .map(|n| n.as_str().unwrap_or("").to_string())
                            .collect()
                    })
                    .unwrap_or_default(),
            )
        })
        .collect()
}

#[test]
fn dump216_sections_smoke() {
    let ds = dirs();
    if ds.is_empty() {
        eprintln!("no rwg2dump 2.16 sections under testdata/pack: skipped");
        return;
    }
    for d in ds {
        eprintln!("== {}", d.display());
        if let Some(j) = json(&d, "deco216.json") {
            for a in j["areas"].as_array().into_iter().flatten() {
                let tiles = a["tiles"].as_array().map_or(0, |v| v.len());
                let nodes = node_columns(&a["nodeColumns"]).len();
                eprintln!(
                    "  deco area {:24} events {:6} tiles {:4} node columns {:3} file {} {}",
                    a["name"].as_str().unwrap_or("?"),
                    a["events"].as_array().map_or(0, |v| v.len()),
                    tiles,
                    nodes,
                    a["file"],
                    if a["error"].is_null() {
                        String::new()
                    } else {
                        format!("ERROR {}", a["error"])
                    }
                );
            }
        }
        if let Some(pp) = json(&d, "postpop216.json") {
            let r = rogue(&pp);
            let n: usize = r.iter().map(|a| a.1.len()).sum();
            let ok = r
                .iter()
                .filter(|a| a.1.last().is_some_and(|t| t.valid))
                .count();
            eprintln!(
                "  roguelike: {} anchors, {n} attempts, {ok} anchors with a valid attempt {}",
                r.len(),
                pp["roguelike"]["error"]
            );
            for a in &r {
                for t in &a.1 {
                    assert!(
                        t.ground < 0 || (t.above.len() == 169 && t.below.len() == 169),
                        "13x13 rows"
                    );
                    assert_eq!(t.height.len(), 169);
                }
            }
            let l = lootgames(&pp);
            eprintln!(
                "  lootgames: {} dungeons, surface found {} {}",
                l.len(),
                l.iter().filter(|x| x.4).count(),
                pp["lootgames"]["error"]
            );
            for x in &l {
                assert_eq!((x.7.len(), x.8.len()), (441, 441));
            }
            let s = spawn_walk(&pp);
            eprintln!(
                "  spawn walk: {} steps, {} loaded {}",
                s.len(),
                s.iter().filter(|x| x.3).count(),
                pp["spawnWalk"]["error"]
            );
        }
        if let Some(n) = json(&d, "nether216.json") {
            eprintln!(
                "  nether nodes: {} columns, {} loaded chunks {}",
                node_columns(&n["columns"]).len(),
                n["loadedChunks"],
                n["error"]
            );
        }
        if let Some(g) = json(&d, "gregtech216.json") {
            eprintln!(
                "  gregtech216: cacheSize {} keys {:?}",
                g["cacheSize"],
                g.as_object().map(|o| o.keys().cloned().collect::<Vec<_>>())
            );
        }
    }
}
