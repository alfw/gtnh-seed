use rwg2_biomes::endtower::{tower, tower_centre, TB};
use serde_json::Value;
use std::collections::HashMap;
use std::path::PathBuf;

fn root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..")
}

/// 2.14 dumps: the fresh tower's real blocks vs the HEE tower layout (rotation, shell, island).
#[test]
fn tower_vs_dump() {
    let seed: i64 = -1053966461076653616;
    for (cx, cz) in rwg2_biomes::end::towers(seed, 0, -60, -60, 60, 60)
        .into_iter()
        .take(3)
    {
        let t = tower(seed, 0, cx, cz);
        eprintln!(
            "tower start {cx},{cz} centre {:?} mode {} ground Y{} rooms {:?} chest {}",
            tower_centre(seed, cx, cz),
            t.mode,
            32 + t.island_y,
            t.rooms,
            t.chest_room
        );
    }
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
        let tw = &g["checks214"]["tower"];
        if tw.is_null() {
            continue;
        }
        let seed: i64 = g["worldInfoSeed"].as_str().unwrap().parse().unwrap();
        let tseed: i64 = g["worldGetSeed"].as_str().unwrap().parse().unwrap();
        let names: Vec<String> = g["blocks"]
            .as_array()
            .unwrap()
            .iter()
            .map(|v| v.as_str().unwrap().to_string())
            .collect();
        let st: Vec<i32> = tw["startChunk"]
            .as_array()
            .unwrap()
            .iter()
            .map(|v| v.as_i64().unwrap() as i32)
            .collect();
        let t = tower(seed, (tseed - seed) as i32, st[0], st[1]);
        if let Some(c) = tw["start"]["components"].as_array().and_then(|c| c.first()) {
            eprintln!(
                "real coordBaseMode {} predicted {}",
                c["coordBaseMode"], t.mode
            );
        }
        let sc = &tw["scan"];
        let (x0, z0) = (
            sc["x0"].as_i64().unwrap() as i32,
            sc["z0"].as_i64().unwrap() as i32,
        );
        let mut real: HashMap<(i32, i32, i32), String> = HashMap::new();
        for (j, row) in sc["rows"].as_array().unwrap().iter().enumerate() {
            for (i, col) in row.as_array().unwrap().iter().enumerate() {
                for run in col.as_str().unwrap().split(' ').filter(|r| !r.is_empty()) {
                    let p: Vec<i32> = run.split(':').map(|v| v.parse().unwrap()).collect();
                    for y in p[0]..p[0] + p[1] {
                        real.insert(
                            (x0 + i as i32, y, z0 + j as i32),
                            names[p[2] as usize].clone(),
                        );
                    }
                }
            }
        }
        let pred = t.blocks();
        let obs = |n: &str| n.contains("obsidian");
        let (mut solid_ok, mut solid_n, mut obs_ok, mut obs_n, mut es_ok, mut es_n) =
            (0, 0, 0, 0, 0, 0);
        for &(x, y, z, b) in &pred {
            let r = real.get(&(x, y, z));
            if b.solid() {
                solid_n += 1;
                solid_ok += r.is_some() as i32;
            }
            if matches!(
                b,
                TB::Obsidian | TB::ObsidianSpecial | TB::ObsidianGlow | TB::ObsidianStairs
            ) {
                obs_n += 1;
                obs_ok += r.is_some_and(|n| obs(n)) as i32;
            }
            if b == TB::EndStone {
                es_n += 1;
                es_ok += r.is_some_and(|n| n.contains("end_stone")) as i32;
            }
        }
        let pset: std::collections::HashSet<_> = pred.iter().map(|p| (p.0, p.1, p.2)).collect();
        // real obsidian inside the tower component's box (the start's bb; HEE decorator obsidian nearby is not the
        // tower's and not predictable)
        let bb: Vec<i32> = tw["start"]["bb"]
            .as_str()
            .unwrap()
            .trim_matches(|c| c == '(' || c == ')')
            .split([',', ';'])
            .map(|v| v.trim().parse().unwrap())
            .collect();
        let in_bb = |k: &(i32, i32, i32)| {
            k.0 >= bb[0]
                && k.0 <= bb[3]
                && k.1 >= bb[1]
                && k.1 <= bb[4]
                && k.2 >= bb[2]
                && k.2 <= bb[5]
        };
        let real_obs = real.iter().filter(|(k, n)| obs(n) && in_bb(k)).count();
        let real_obs_hit = real
            .iter()
            .filter(|(k, n)| obs(n) && in_bb(k) && pset.contains(k))
            .count();
        eprintln!(
            "tower {st:?}: predicted solid {solid_ok}/{solid_n} present, obsidian {obs_ok}/{obs_n}, end stone {es_ok}/{es_n}; real obsidian in the tower box {real_obs_hit}/{real_obs} predicted"
        );
        // unpredicted real obsidian: by block name and Y (diagnostic)
        let mut miss: std::collections::BTreeMap<String, Vec<i32>> = Default::default();
        for (k, n) in real.iter().filter(|(k, n)| obs(n) && !pset.contains(k)) {
            let k = &(k.0, k.1, k.2);
            miss.entry(n.clone()).or_default().push(k.1);
        }
        for (n, ys) in &mut miss {
            ys.sort_unstable();
            eprintln!(
                "  unpredicted {n}: {} (Y {}..{})",
                ys.len(),
                ys[0],
                ys[ys.len() - 1]
            );
        }
        let mut cnt: std::collections::BTreeMap<String, usize> = Default::default();
        for &(x, y, z, b) in &pred {
            if let Some(n) = real.get(&(x, y, z)) {
                if format!("{b:?}").contains("Obsidian") != obs(n) {
                    *cnt.entry(format!("{b:?} → {n}")).or_default() += 1;
                }
            }
        }
        eprintln!("  kind mismatches: {cnt:?}");
        assert!(
            obs_ok as f64 >= 0.95 * obs_n as f64,
            "tower obsidian {obs_ok}/{obs_n}"
        );
        assert!(real_obs_hit as f64 >= 0.95 * real_obs as f64);
    }
}
