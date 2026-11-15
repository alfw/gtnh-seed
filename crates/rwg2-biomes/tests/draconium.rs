use rwg2_biomes::draconium::{ore_in, DEFAULT_CFG};
use rwg2_biomes::gtend::EndBlocks;
use serde_json::Value;
use std::path::PathBuf;

fn root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..")
}

/// Fresh-island census (every 4th column) draconium count vs the prediction; 2.14 dumps: exact block lists.
#[test]
fn end_draconium_vs_dump() {
    let base = root().join("testdata/pack/gtnh-2.10.0/heldout");
    let Ok(rd) = std::fs::read_dir(&base) else {
        return;
    };
    for e in rd.flatten() {
        let Ok(s) = std::fs::read_to_string(e.path().join("dims.json")) else {
            continue;
        };
        let d: Value = serde_json::from_str(&s).unwrap();
        let g = &d["endGenerated"];
        let pf = &g["diag"]["probeFresh"];
        if pf.is_null() {
            continue;
        }
        let seed: i64 = g["worldInfoSeed"].as_str().unwrap().parse().unwrap();
        let tseed: i64 = g["worldGetSeed"].as_str().unwrap().parse().unwrap();
        let sc = pf["startChunk"].as_array().unwrap();
        let (sx, sz) = (
            sc[0].as_i64().unwrap() as i32,
            sc[1].as_i64().unwrap() as i32,
        );
        let mut w = EndBlocks::new(seed, (tseed - seed) as i32);
        let (v, inexact) = ore_in(tseed, &DEFAULT_CFG, sx, sz, sx + 13, sz + 13, &mut w);
        let (x0, z0) = (sx * 16 + 8, sz * 16 + 8);
        let pred = v
            .iter()
            .filter(|&&(x, _, z)| {
                x >= x0
                    && z >= z0
                    && x < x0 + 208
                    && z < z0 + 208
                    && (x - x0) % 4 == 0
                    && (z - z0) % 4 == 0
            })
            .count();
        let real = pf["censusAfter"]["DraconicEvolution:draconiumOre"]
            .as_i64()
            .unwrap_or(0);
        eprintln!("island {sx},{sz}: draconium every-4th census real {real} predicted {pred} (total predicted {}, inexact chunks {inexact})", v.len());
        let c = &g["checks214"];
        if !c.is_null() {
            for (k, area) in [
                ("main", &c["main"]),
                ("island", &c["island"]),
                ("tower", &c["tower"]),
            ] {
                if area.is_null() {
                    continue;
                }
                let r: Vec<i32> = area["chunkRect"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .map(|v| v.as_i64().unwrap() as i32)
                    .collect();
                let real: std::collections::BTreeSet<(i32, i32, i32)> = area["draconium"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .map(|p| {
                        let p: Vec<i32> = p
                            .as_array()
                            .unwrap()
                            .iter()
                            .map(|v| v.as_i64().unwrap() as i32)
                            .collect();
                        (p[0], p[1], p[2])
                    })
                    .collect();
                let (pv, _) = ore_in(tseed, &DEFAULT_CFG, r[0], r[1], r[2], r[3], &mut w);
                let pred: std::collections::BTreeSet<_> = pv.into_iter().collect();
                let hit = real.intersection(&pred).count();
                eprintln!(
                    "{k}: real {} predicted {} both {hit}",
                    real.len(),
                    pred.len()
                );
                if k == "main" {
                    assert!(
                        hit as f64 >= 0.9 * real.len() as f64,
                        "main island draconium {hit}/{}",
                        real.len()
                    );
                }
            }
        }
    }
}
