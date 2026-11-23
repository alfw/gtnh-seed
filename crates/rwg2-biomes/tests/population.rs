//! RWG2 populate prefix (lakes, dungeons, dirt/gravel blobs) vs the real post-generation test columns of
//! rwg2dump 2.10 (`gtores.json` `testColumns`: blocks at (cx·16+7, y, cz·16+9)).
//! Data: `testdata/pack/gtnh-2.10.0/tune/<seed>/` (tuning world). Skips without it.
//! Env: `POP_LAKES=0`, `POP_LAVA=0`, `POP_VR=<chunks>` (village radius), `POP_OWN=1` (only the own chunk).

use rwg2_biomes::config::RwgConfig;
use rwg2_biomes::population::*;
use rwg2_biomes::provider::{BiomeProvider, BiomeTable, TableHeights, WorldType};
use serde_json::Value;
use std::collections::BTreeMap;
use std::path::PathBuf;

fn root() -> PathBuf {
    // under WASI the runner maps the repo root and sets RWG2_DATA_DIR (= <root>/data)
    std::env::var("RWG2_DATA_DIR")
        .map(|d| PathBuf::from(d).parent().unwrap().to_path_buf())
        .unwrap_or_else(|_| PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../.."))
}

#[test]
fn population_vs_test_columns() {
    // tuning world(s) and held-out world(s) (never used to fit anything)
    for set in ["tune", "heldout"] {
        let dir = root().join("testdata/pack/gtnh-2.10.0").join(set);
        let Ok(rd) = std::fs::read_dir(&dir) else {
            continue;
        };
        for w in rd.flatten() {
            eprintln!("== {set} world {}", w.file_name().to_string_lossy());
            check_world(w.path(), w.file_name().to_str().unwrap().parse().unwrap());
        }
    }
}

fn check_world(wdir: PathBuf, seed: i64) {
    let scan: Value =
        serde_json::from_str(&std::fs::read_to_string(wdir.join("gtores.json")).unwrap()).unwrap();
    let names: Vec<String> = scan["censusBlocks"]
        .as_array()
        .unwrap()
        .iter()
        .map(|b| b.as_str().unwrap().to_string())
        .collect();
    let table = BiomeTable::from_rwg2dump_json(
        &std::fs::read_to_string(root().join(pack_rel("biomes.json"))).unwrap(),
    )
    .unwrap();
    let p = BiomeProvider::new(seed, WorldType::Rwg2Continents, table, RwgConfig::default());
    let h = TableHeights;
    let mut m = PopModel::new(&p, &h, seed);
    let cj: Value = serde_json::from_str(
        &std::fs::read_to_string(root().join(pack_rel("climate.json"))).unwrap(),
    )
    .unwrap();
    m.gates = PreGates::new(cj["biomes"].as_array().unwrap().iter().map(|b| {
        (
            b["id"].as_u64().unwrap() as u16,
            b["name"].as_str().unwrap_or(""),
            b["dict"]
                .as_array()
                .map(|d| d.iter().filter_map(|x| x.as_str()).collect())
                .unwrap_or_default(),
        )
    }));
    m.pre = std::env::var("POP_PRE").as_deref() != Ok("0");
    let env = |k: &str| std::env::var(k).ok();
    m.lakes = env("POP_LAKES").as_deref() != Some("0");
    m.lava_lakes = env("POP_LAVA").as_deref() != Some("0");
    if let Some(v) = env("POP_VR") {
        m.village_radius = v.parse().unwrap();
    }
    let own = env("POP_OWN").is_some();
    let r = scan["rangeChunks"].as_i64().unwrap() as i32;
    m.load_structures(-r - 1, -r - 1, r, r);
    let mut outcomes: BTreeMap<(i32, i32), Outcome> = BTreeMap::new();
    let mut fl: BTreeMap<(i32, i32), u32> = BTreeMap::new();
    // per flag: (chunks with a predicted dirt/gravel block on a column, of which with a wrong one)
    let mut attrib = BTreeMap::<u32, (i32, i32)>::new();
    let mut why = BTreeMap::<&str, i32>::new();
    // (pred, real) counts over deep positions
    let mut conf = BTreeMap::<(String, String), i32>::new();
    let (mut cols, mut skipped) = (0, 0);
    let (mut base_dirt, mut base_stone) = (0, 0);
    let mut fn_shown = 0;
    let mut runs = BTreeMap::<(bool, bool, i32), i32>::new();
    let mut dung: BTreeMap<(i32, i32), Vec<rwg2_biomes::population::Dungeon>> = BTreeMap::new();
    // dungeon rooms on test columns: [covered, floor solid both, floor kind exact (mossy/cobble), spawner/chest
    // column exact, real mossy floor without a predicted room, of which a populating chunk was uncertain]
    let mut dstat = [0i32; 6];
    for e in scan["testColumns"].as_array().unwrap() {
        let (cx, cz) = (
            e["cx"].as_i64().unwrap() as i32,
            e["cz"].as_i64().unwrap() as i32,
        );
        let col: Vec<String> = e["ids"]
            .as_str()
            .unwrap()
            .split(' ')
            .map(|t| {
                if t == "s" {
                    "stone".into()
                } else {
                    let a = t.split(':').next().unwrap();
                    names[a.parse::<usize>().unwrap()].clone()
                }
            })
            .collect();
        let (x, z) = (cx * 16 + 7, cz * 16 + 9);
        if cx <= -r || cz <= -r {
            continue; // the -x/-z neighbours lie outside the dump's populated area
        }
        let pops: Vec<(i32, i32)> = if own {
            vec![(cx, cz)]
        } else {
            vec![(cx - 1, cz - 1), (cx, cz - 1), (cx - 1, cz), (cx, cz)]
        };
        let mut changes = BTreeMap::<i32, B>::new();
        let mut ok = true;
        for &(pcx, pcz) in &pops {
            if let std::collections::btree_map::Entry::Vacant(e) = outcomes.entry((pcx, pcz)) {
                e.insert(m.populate(pcx, pcz));
                fl.insert((pcx, pcz), m.flags);
                dung.insert((pcx, pcz), m.last_dungeons.clone());
            }
            let o = outcomes[&(pcx, pcz)].clone();
            match o {
                Outcome::Certain(v) => {
                    for (bx, by, bz, b) in v {
                        if bx == x && bz == z {
                            changes.insert(by, b);
                        }
                    }
                }
                Outcome::Uncertain(w, _) => {
                    *why.entry(w).or_default() += 1;
                    ok = false;
                }
            }
        }
        {
            let nm = |y: i32| col.get(y as usize).map(|s| s.as_str()).unwrap_or("");
            let mut covered = false;
            for c in &pops {
                for d in &dung[c] {
                    if (x - d.x).abs() > d.l + 1 || (z - d.z).abs() > d.i1 + 1 {
                        continue;
                    }
                    covered = true;
                    dstat[0] += 1;
                    let real = nm(d.y - 1);
                    let pred = outcomes[c].clone();
                    let log = match pred {
                        Outcome::Certain(v) | Outcome::Uncertain(_, v) => v,
                    };
                    let at = |yy: i32| {
                        log.iter()
                            .rev()
                            .find(|&&(a, b2, cc, _)| a == x && b2 == yy && cc == z)
                            .map(|e| e.3)
                    };
                    let pf = at(d.y - 1);
                    let real_floor = matches!(
                        real,
                        "minecraft:mossy_cobblestone" | "minecraft:cobblestone"
                    );
                    if real_floor && matches!(pf, Some(B::Mossy | B::Cobble)) {
                        dstat[1] += 1;
                        if (pf == Some(B::Mossy)) == (real == "minecraft:mossy_cobblestone") {
                            dstat[2] += 1;
                        }
                    }
                    let pm = at(d.y);
                    let rm = nm(d.y);
                    if (pm == Some(B::Spawner)) == (rm == "minecraft:mob_spawner")
                        && (pm == Some(B::Chest)) == (rm == "minecraft:chest")
                    {
                        dstat[3] += 1;
                    }
                }
            }
            if !covered
                && (5..100).any(|y| {
                    nm(y) == "minecraft:mossy_cobblestone"
                        && nm(y + 1) != "minecraft:mossy_cobblestone"
                })
            {
                dstat[4] += 1;
                if !ok {
                    dstat[5] += 1;
                }
            }
        }
        if !ok {
            skipped += 1;
            continue;
        }
        cols += 1;
        // attribution: each populating chunk's own blob blocks on this column vs the real column
        for &(pcx, pcz) in &pops {
            if let Outcome::Certain(v) = &outcomes[&(pcx, pcz)] {
                let mut any = false;
                let mut bad = false;
                for &(bx, by, bz, b) in v {
                    if bx == x
                        && bz == z
                        && matches!(b, B::Dirt | B::Gravel)
                        && by >= 10
                        && (by as usize) < col.len()
                    {
                        any = true;
                        let real = col[by as usize].as_str();
                        let want = if b == B::Dirt {
                            "minecraft:dirt"
                        } else {
                            "minecraft:gravel"
                        };
                        if real == "stone" {
                            bad = true;
                        }
                        let _ = want;
                    }
                }
                if any {
                    let f = fl[&(pcx, pcz)];
                    for k in (0..10)
                        .map(|b| 1u32 << b)
                        .chain([rwg2_biomes::population::F_DUNGEON, u32::MAX])
                    {
                        if k == u32::MAX || f & k != 0 {
                            let e = attrib.entry(k).or_default();
                            e.0 += 1;
                            e.1 += bad as i32;
                        }
                    }
                }
            }
        }
        let top = m.t.top(x, z).unwrap_or(0);
        // real dirt/gravel runs: matched by any predicted block of the same kind?
        for (kind, bk) in [("minecraft:dirt", B::Dirt), ("minecraft:gravel", B::Gravel)] {
            let mut y = 10;
            while y < (top - SURFACE - 2).min(110) {
                if col[y as usize] == kind {
                    let y0 = y;
                    while y < 127 && col[y as usize] == kind {
                        y += 1;
                    }
                    let hit = (y0..y).any(|yy| changes.get(&yy) == Some(&bk));
                    let deco = pops.iter().any(|c| fl[c] & F_DECO_UNKNOWN != 0) as i32 * 2
                        + pops.iter().any(|c| fl[c] & F_DECO_MODELLED != 0) as i32;
                    let e = runs
                        .entry((kind == "minecraft:dirt", hit, deco))
                        .or_insert(0);
                    *e += 1;
                } else {
                    y += 1;
                }
            }
        }
        for y in 10..(top - SURFACE - 2).min(110) {
            let base = m.base(x, y, z);
            if base != B::Stone {
                continue; // only positions that start as stone
            }
            let pred = changes.get(&y).copied().unwrap_or(B::Stone);
            let real = col[y as usize].clone();
            let real = match real.as_str() {
                "stone" | "minecraft:dirt" | "minecraft:gravel" | "minecraft:air"
                | "minecraft:water" => real,
                s if s.starts_with("gregtech:gt.blockores") => "stone".into(), // GT ores replace stone later
                _ => "other".into(),
            };
            if real == "minecraft:dirt" {
                base_dirt += 1;
            }
            if real == "stone" {
                base_stone += 1;
            }
            if pred == B::Stone && real == "minecraft:dirt" && fn_shown < 0 {
                fn_shown += 1;
                let run: String = (y - 3..=y + 3)
                    .map(|yy| {
                        if col[yy as usize] == "stone" {
                            's'
                        } else if col[yy as usize] == "minecraft:dirt" {
                            'd'
                        } else {
                            '?'
                        }
                    })
                    .collect();
                let fls: Vec<String> = pops
                    .iter()
                    .map(|c| format!("{:#x}", fl.get(c).copied().unwrap_or(0)))
                    .collect();
                let prun: String = (y - 3..=y + 3)
                    .map(|yy| match changes.get(&yy) {
                        Some(B::Dirt) => 'd',
                        Some(B::Gravel) => 'g',
                        Some(_) => '?',
                        None => 's',
                    })
                    .collect();
                // nearest predicted dirt in the 4 populating chunks' logs around (x, y, z)
                let mut near = i32::MAX;
                for c in &pops {
                    if let Outcome::Certain(v) = &outcomes[c] {
                        for &(bx, by, bz, b) in v {
                            if b == B::Dirt && by == y {
                                near = near.min((bx - x).abs() + (bz - z).abs());
                            }
                        }
                    }
                }
                eprintln!("  FN dirt col ({cx},{cz}) y {y} top {top} real {run} pred {prun} flags {fls:?} nearest pred dirt same y: {near}");
            }
            *conf.entry((format!("{pred:?}"), real)).or_default() += 1;
        }
    }
    // geodes: the own chunk's geode shell crosses the test column (offset −1, +1 from the centre) at |dy| ≤ 7
    let (mut g_tp, mut g_fp, mut g_fn, mut g_n) = (0, 0, 0, 0);
    let mut dark = [0u32; 3];
    let mut ys_ok = 0;
    for e in scan["testColumns"].as_array().unwrap() {
        let (cx, cz) = (
            e["cx"].as_i64().unwrap() as i32,
            e["cz"].as_i64().unwrap() as i32,
        );
        let col: Vec<&str> = e["ids"].as_str().unwrap().split(' ').collect();
        let cube_ys: Vec<i32> = (0..col.len() as i32)
            .filter(|&y| {
                let t = col[y as usize];
                t != "s"
                    && names[t.split(':').next().unwrap().parse::<usize>().unwrap()]
                        == "Railcraft:cube"
                    && y < 50
            })
            .collect();
        let g = m.geode_at(cx, cz);
        if let Some(g) = g {
            g_n += 1;
            dark[0] += g.diamond as u32;
            dark[1] += g.emerald as u32;
            dark[2] += g.lapis as u32;
        }
        match (g, cube_ys.is_empty()) {
            (Some(g), false) => {
                g_tp += 1;
                let (lo, hi) = (cube_ys[0], *cube_ys.last().unwrap());
                if (lo + hi) / 2 - g.y <= 1 && g.y - (lo + hi) / 2 <= 1 {
                    ys_ok += 1;
                }
            }
            (Some(_), true) => g_fp += 1,
            (None, false) => g_fn += 1,
            _ => {}
        }
    }
    let census_ore: Vec<String> = scan["census"]
        .as_object()
        .unwrap()
        .iter()
        .filter(|(k, _)| k.starts_with("Railcraft:ore"))
        .map(|(k, v)| {
            format!(
                "{k} {}",
                v.as_array()
                    .unwrap()
                    .iter()
                    .map(|x| x.as_i64().unwrap())
                    .sum::<i64>()
            )
        })
        .collect();
    eprintln!("geodes: {g_n} predicted; column has abyssal stone: tp {g_tp} fp {g_fp} fn {g_fn}; centre y ±1 {ys_ok}/{g_tp}; dark ores predicted d/e/l {dark:?}; census {census_ore:?}");
    eprintln!("columns {cols} modelled, {skipped} skipped {why:?}; real dirt {base_dirt} / stone {base_stone} at deep pre-stone positions");
    for ((pr, re), n) in &conf {
        eprintln!("  pred {pr:7} real {re:18} {n}");
    }
    eprintln!("real runs (dirt?, matched?, 2·unknown deco + modelled deco): {runs:?}");
    for (k, (a, b)) in &attrib {
        eprintln!("  flag {k:#x}: chunks with blob blocks on a column {a}, with a block where the real column is stone {b} ({:.0}%)", 100.0 * *b as f64 / *a as f64);
    }
    let g = |a: &str, b: &str| *conf.get(&(a.to_string(), b.to_string())).unwrap_or(&0);
    let (tp, fp, fnn) = (
        g("Dirt", "minecraft:dirt"),
        g("Dirt", "stone"),
        g("Stone", "minecraft:dirt"),
    );
    eprintln!(
        "dungeon rooms on test columns: covered {}, floor solid {}, floor mossy/cobble exact {}, y spawner/chest exact {}; real mossy floor without a room {} (uncertain chunk {})",
        dstat[0], dstat[1], dstat[2], dstat[3], dstat[4], dstat[5]
    );
    eprintln!(
        "dirt: precision {:.3} recall {:.3}; gravel: tp {} fp {} fn {}",
        tp as f64 / (tp + fp).max(1) as f64,
        tp as f64 / (tp + fnn).max(1) as f64,
        g("Gravel", "minecraft:gravel"),
        g("Gravel", "stone"),
        g("Stone", "minecraft:gravel")
    );
}

#[test]
fn population_timing() {
    let table = BiomeTable::from_rwg2dump_json(
        &std::fs::read_to_string(root().join(pack_rel("biomes.json"))).unwrap(),
    )
    .unwrap();
    let seed = 1337;
    let p = BiomeProvider::new(seed, WorldType::Rwg2Continents, table, RwgConfig::default());
    let h = TableHeights;
    let cj: Value = serde_json::from_str(
        &std::fs::read_to_string(root().join(pack_rel("climate.json"))).unwrap(),
    )
    .unwrap();
    let gates = PreGates::new(cj["biomes"].as_array().unwrap().iter().map(|b| {
        (
            b["id"].as_u64().unwrap() as u16,
            b["name"].as_str().unwrap_or(""),
            b["dict"]
                .as_array()
                .map(|d| d.iter().filter_map(|x| x.as_str()).collect())
                .unwrap_or_default(),
        )
    }));
    let t0 = std::time::Instant::now();
    let mut m = PopModel::new(&p, &h, seed);
    m.gates = gates.clone();
    let g = m.geodes_in_rect(0, 0, 63, 63);
    let tg = t0.elapsed();
    let t1 = std::time::Instant::now();
    let mut n = 0;
    for i in 0..10 {
        let mut m = PopModel::new(&p, &h, seed);
        m.gates = gates.clone();
        m.load_structures(i - 1, -1, i, 0);
        n += m.chunk_codes(i, 0, 192).iter().filter(|&&c| c != 0).count();
    }
    eprintln!(
        "geodes in 64×64 chunks: {} in {:?}; xray_pop 10 chunks (fresh model each) {:?}, {n} changed blocks",
        g.len(),
        tg,
        t1.elapsed()
    );
    // vanilla dungeons in a 16×16-chunk cell, one shared model
    let t2 = std::time::Instant::now();
    let mut m = PopModel::new(&p, &h, seed);
    m.gates = gates.clone();
    m.load_structures(-1, -1, 15, 15);
    let (mut nd, mut nc) = (0, 0);
    for cz in 0..16 {
        for cx in 0..16 {
            let d = m.dungeons_at(cx, cz);
            nd += d.len();
            nc += d.iter().map(|d| d.chests.len()).sum::<usize>();
        }
    }
    eprintln!(
        "vanilla dungeons in 16×16 chunks: {nd} rooms, {nc} chests in {:?}",
        t2.elapsed()
    );
}

/// Prints the vanilla dungeons with chests around `DUNGEON_AT=seed,x,z` (±8 chunks). Manual tool.
#[test]
fn dungeons_near() {
    let Ok(v) = std::env::var("DUNGEON_AT") else {
        return;
    };
    let a: Vec<i64> = v.split(',').map(|x| x.trim().parse().unwrap()).collect();
    let table = BiomeTable::from_rwg2dump_json(
        &std::fs::read_to_string(root().join(pack_rel("biomes.json"))).unwrap(),
    )
    .unwrap();
    let p = BiomeProvider::new(a[0], WorldType::Rwg2Continents, table, RwgConfig::default());
    let h = TableHeights;
    let cj: Value = serde_json::from_str(
        &std::fs::read_to_string(root().join(pack_rel("climate.json"))).unwrap(),
    )
    .unwrap();
    let mut m = PopModel::new(&p, &h, a[0]);
    m.gates = PreGates::new(cj["biomes"].as_array().unwrap().iter().map(|b| {
        (
            b["id"].as_u64().unwrap() as u16,
            b["name"].as_str().unwrap_or(""),
            b["dict"]
                .as_array()
                .map(|d| d.iter().filter_map(|x| x.as_str()).collect())
                .unwrap_or_default(),
        )
    }));
    let (cx, cz) = ((a[1] >> 4) as i32, (a[2] >> 4) as i32);
    m.load_structures(cx - 9, cz - 9, cx + 8, cz + 8);
    let t = rwg2_biomes::loot::LootTable::dungeon();
    for z in cz - 8..=cz + 8 {
        for x in cx - 8..=cx + 8 {
            for d in m.dungeons_at(x, z) {
                eprintln!(
                    "dungeon {} {} {} ({}x{}), {} chests",
                    d.x,
                    d.y,
                    d.z,
                    2 * d.l + 1,
                    2 * d.i1 + 1,
                    d.chests.len()
                );
                for (x, y, z, c) in &d.chests {
                    let s: Vec<String> = c
                        .slots
                        .iter()
                        .map(|&(_, k, n)| format!("{n}x{}", t.items[k as usize].name))
                        .collect();
                    eprintln!("  chest {x} {y} {z}: {}", s.join(", "));
                }
            }
        }
    }
}

/// `data/<PACK_DATA>/<name>` relative to the repo (`PACK_DATA` default `gtnh-2.9.0`; `gtnh-2.9.1` = RWG2 2.0.9 pack).
#[allow(dead_code)]
fn pack_rel(name: &str) -> String {
    format!("data/{}/{name}", std::env::var("PACK_DATA").unwrap_or_else(|_| "gtnh-2.9.0".into()))
}
