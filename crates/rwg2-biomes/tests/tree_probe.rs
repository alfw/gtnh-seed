//! Trees step 1 probe: which decorator runs first in RWG2's border-biome loop, and how often the stream reaches it.
#![cfg(feature = "json")]
use rwg2_biomes::config::RwgConfig;
use rwg2_biomes::provider::{BiomeProvider, BiomeTable, TableHeights, WorldType};
use std::collections::BTreeMap;

#[test]
#[ignore]
fn first_decorator_census() {
    let dir = std::env::var("RWG2_DATA_DIR")
        .unwrap_or_else(|_| concat!(env!("CARGO_MANIFEST_DIR"), "/../../data/gtnh-2.9.0").into());
    let js = std::fs::read_to_string(format!("{dir}/biomes.json")).unwrap();
    let table = BiomeTable::from_rwg2dump_json(&js).unwrap();
    let mut kinds: BTreeMap<String, usize> = BTreeMap::new();
    let mut first_names: BTreeMap<String, usize> = BTreeMap::new();
    let mut single = 0usize;
    let mut n = 0usize;
    for seed in [1337i64, 92900795, 1631302684446519836] {
        let p = BiomeProvider::new(
            seed,
            WorldType::Rwg2Continents,
            table.clone(),
            RwgConfig::default(),
        );
        let h = TableHeights;
        for cx in -60..60 {
            for cz in -60..60 {
                let (x, z) = (cx * 16, cz * 16);
                let mut cnt: BTreeMap<u16, i32> = BTreeMap::new();
                for bx in -4..=4 {
                    for bz in -4..=4 {
                        *cnt.entry(p.biome_data(x + 24 + bx * 16, z + 24 + bz * 16, &h).data)
                            .or_default() += 1;
                    }
                }
                n += 1;
                if cnt.len() == 1 {
                    single += 1;
                }
                // first non-ocean decorator in id order
                let mut kind = "only ocean".to_string();
                for (&id, &c) in &cnt {
                    let info = p.table.info(id as _);
                    let cl = &info.class;
                    if cl.contains("RealisticBiomeOcean") {
                        continue;
                    }
                    let k = if cl.contains("BOPOcean") {
                        "BOP ocean"
                    } else if cl.contains("Support") {
                        if c as f32 * 0.012345 <= 0.3 {
                            "support ≤0.3 (no-op unless oasis)"
                        } else if info.gen_id == 129 || info.gen_id == 140 {
                            "support vanilla-base"
                        } else {
                            "support BOP decorator"
                        }
                    } else {
                        "RWG2 native"
                    };
                    kind = k.to_string();
                    if k == "RWG2 native" || k.starts_with("support BOP") {
                        *first_names
                            .entry(format!("{k}: {}", info.name))
                            .or_default() += 1;
                    }
                    if !k.starts_with("support ≤") {
                        break;
                    }
                }
                *kinds.entry(kind).or_default() += 1;
            }
        }
    }
    eprintln!("chunks {n}, single-biome border {single}");
    for (k, c) in &kinds {
        eprintln!("  {:5.1}%  {k}", *c as f64 * 100.0 / n as f64);
    }
    let mut v: Vec<_> = first_names.into_iter().collect();
    v.sort_by_key(|e| std::cmp::Reverse(e.1));
    for (k, c) in v.iter().take(30) {
        eprintln!("  {:5.1}%  {k}", *c as f64 * 100.0 / n as f64);
    }
}

/// Which BOP decorators run how often (chunks where a support biome, or a mountain chain's backing, has border
/// strength > 0.3), by base class.
#[test]
#[ignore]
fn bop_decorator_census() {
    let dir = std::env::var("RWG2_DATA_DIR")
        .unwrap_or_else(|_| concat!(env!("CARGO_MANIFEST_DIR"), "/../../data/gtnh-2.9.0").into());
    let js = std::fs::read_to_string(format!("{dir}/biomes.json")).unwrap();
    let table = BiomeTable::from_rwg2dump_json(&js).unwrap();
    let mut by: BTreeMap<String, usize> = BTreeMap::new();
    let mut n = 0usize;
    for seed in [1337i64, 92900795, 1631302684446519836] {
        let p = BiomeProvider::new(
            seed,
            WorldType::Rwg2Continents,
            table.clone(),
            RwgConfig::default(),
        );
        let h = TableHeights;
        for cx in (-200..200).step_by(4) {
            for cz in (-200..200).step_by(4) {
                let (x, z) = (cx * 16, cz * 16);
                let mut cnt: BTreeMap<u16, i32> = BTreeMap::new();
                for bx in -4..=4 {
                    for bz in -4..=4 {
                        *cnt.entry(p.biome_data(x + 24 + bx * 16, z + 24 + bz * 16, &h).data)
                            .or_default() += 1;
                    }
                }
                n += 1;
                for (&id, &c) in &cnt {
                    let info = p.table.info(id as _);
                    let s = c as f32 * 0.012345;
                    if s <= 0.3 {
                        continue;
                    }
                    let cl = &info.class;
                    if cl.contains("Support") || cl.contains("MountainChain") {
                        let tag = if cl.contains("MountainChain") {
                            " (mc)"
                        } else {
                            ""
                        };
                        *by.entry(format!(
                            "{}{tag}",
                            info.base_class.rsplit('.').next().unwrap()
                        ))
                        .or_default() += 1;
                    } else if !cl.contains("Ocean") {
                        *by.entry("native".into()).or_default() += 1;
                    }
                }
            }
        }
    }
    let mut v: Vec<_> = by.into_iter().collect();
    v.sort_by_key(|e| std::cmp::Reverse(e.1));
    eprintln!("{n} chunks");
    for (k, c) in v {
        eprintln!("  {c:6} {:5.1}%  {k}", c as f64 * 100.0 / n as f64);
    }
}

#[test]
#[ignore]
fn deco_view_timing() {
    let dir = concat!(env!("CARGO_MANIFEST_DIR"), "/../../data/gtnh-2.9.0");
    let table = BiomeTable::from_rwg2dump_json(
        &std::fs::read_to_string(format!("{dir}/biomes.json")).unwrap(),
    )
    .unwrap();
    let p = BiomeProvider::new(
        1631302684446519836,
        WorldType::Rwg2Continents,
        table,
        RwgConfig::default(),
    );
    let g = rwg2_biomes::population::PreGates::default();
    let t = std::time::Instant::now();
    let mut n = 0;
    let mut cells = 0;
    for cx in -3..3 {
        for cz in -3..3 {
            let (_, hh, v) = rwg2_biomes::deco::view::chunk_voxels(&p, &TableHeights, &g, cx, cz);
            n += 1;
            cells += v.iter().filter(|&&c| c != 0).count();
            let _ = hh;
        }
    }
    eprintln!("{n} chunks {:?} per chunk, {cells} cells", t.elapsed() / n);
}

/// Nearest chunk dominated (> 80 %) by each BOP biome class (seed `BOP_FIND_SEED`), for viewer checks.
#[test]
#[ignore]
fn bop_find() {
    let dir = concat!(env!("CARGO_MANIFEST_DIR"), "/../../data/gtnh-2.9.0");
    let table = BiomeTable::from_rwg2dump_json(
        &std::fs::read_to_string(format!("{dir}/biomes.json")).unwrap(),
    )
    .unwrap();
    let seed: i64 =
        std::env::var("BOP_FIND_SEED").map_or(1631302684446519836, |s| s.parse().unwrap());
    let p = BiomeProvider::new(seed, WorldType::Rwg2Continents, table, RwgConfig::default());
    let h = TableHeights;
    let mut found: BTreeMap<String, (i32, i32)> = BTreeMap::new();
    for r in 0..120i32 {
        for cx in -r..=r {
            for cz in -r..=r {
                if cx.abs() != r && cz.abs() != r {
                    continue;
                }
                let (x, z) = (cx * 64, cz * 64);
                let mut cnt: BTreeMap<u16, i32> = BTreeMap::new();
                for bx in -4..=4 {
                    for bz in -4..=4 {
                        *cnt.entry(p.biome_data(x + bx * 16, z + bz * 16, &h).data)
                            .or_default() += 1;
                    }
                }
                for (&id, &c) in &cnt {
                    let info = p.table.info(id as _);
                    if c >= 65 && info.base_class.contains("biomesoplenty") {
                        let k = info.base_class.rsplit('.').next().unwrap().to_string();
                        found.entry(k).or_insert((x, z));
                    }
                }
            }
        }
    }
    for (k, (x, z)) in found {
        eprintln!("{k:32} x={x}&z={z}");
    }
}

/// Time of `region_tops` (4 × 4 chunk groups) vs per-chunk `chunk_voxels`.
#[test]
#[ignore]
fn deco_tops_timing() {
    use rwg2_biomes::population::PreGates;
    let dir = concat!(env!("CARGO_MANIFEST_DIR"), "/../../data/gtnh-2.9.0");
    let table = BiomeTable::from_rwg2dump_json(
        &std::fs::read_to_string(format!("{dir}/biomes.json")).unwrap(),
    )
    .unwrap();
    let p = BiomeProvider::new(
        1631302684446519836,
        WorldType::Rwg2Continents,
        table,
        RwgConfig::default(),
    );
    let h = TableHeights;
    let g = PreGates::default();
    let t = std::time::Instant::now();
    let mut n = 0;
    for (cx, cz) in [(0, 0), (40, -8), (-24, 24)] {
        let v = rwg2_biomes::deco::view::region_tops(&p, &h, &g, cx, cz, 4);
        n += v.iter().filter(|c| c.0 != 0).count();
    }
    eprintln!(
        "region 4x4: {:.1} ms per chunk, {n} cols",
        t.elapsed().as_secs_f64() * 1000.0 / 48.0
    );
}

/// Why the populate prefix stops before the border loop (`PREFIX_CHUNKS="cx,cz;…"`).
#[test]
#[ignore]
fn prefix_reasons() {
    use rwg2_biomes::population::{Outcome, PopModel};
    let dir = concat!(env!("CARGO_MANIFEST_DIR"), "/../../data/gtnh-2.9.0");
    let table = BiomeTable::from_rwg2dump_json(
        &std::fs::read_to_string(format!("{dir}/biomes.json")).unwrap(),
    )
    .unwrap();
    let p = BiomeProvider::new(
        1631302684446519836,
        WorldType::Rwg2Continents,
        table,
        RwgConfig::default(),
    );
    let h = TableHeights;
    let mut m = PopModel::new(&p, &h, p.seed());
    let list =
        std::env::var("PREFIX_CHUNKS").unwrap_or("-892,356;-891,356;-276,-68;-161,-38".into());
    for c in list.split(';') {
        let v: Vec<i32> = c.split(',').map(|s| s.trim().parse().unwrap()).collect();
        m.load_structures(v[0] - 2, v[1] - 2, v[0] + 1, v[1] + 1);
        let o = m.populate(v[0], v[1]);
        let why = match &o {
            Outcome::Certain(_) => "certain".to_string(),
            Outcome::Uncertain(w, _) => format!("uncertain: {w}"),
        };
        eprintln!(
            "({}, {}) {why} flags {:#x} deco_start {}",
            v[0],
            v[1],
            m.flags,
            m.last_deco_start.is_some()
        );
    }
}

/// Time of an ordered spawn-area replay (`LoadSim::spawn_area`, 24² populates on one stateful world).
#[test]
#[ignore]
fn spawn_replay_timing() {
    use rwg2_biomes::gtorder::LoadSim;
    use rwg2_biomes::population::PreGates;
    let dir = concat!(env!("CARGO_MANIFEST_DIR"), "/../../data/gtnh-2.9.0");
    let table = BiomeTable::from_rwg2dump_json(
        &std::fs::read_to_string(format!("{dir}/biomes.json")).unwrap(),
    )
    .unwrap();
    let p = BiomeProvider::new(
        1631302684446519836,
        WorldType::Rwg2Continents,
        table,
        RwgConfig::default(),
    );
    let h = TableHeights;
    let g = PreGates::default();
    let mut sim = LoadSim::new();
    sim.spawn_area(0, 0);
    let t = std::time::Instant::now();
    let (w, _) = rwg2_biomes::deco::view::replay_ordered(&p, &h, &g, &sim.order, &|_, _| None, &|_, _| None);
    eprintln!(
        "spawn replay: {} populates, {:.0} ms ({:.1} ms/chunk), log {}",
        sim.order.len(),
        t.elapsed().as_secs_f64() * 1000.0,
        t.elapsed().as_secs_f64() * 1000.0 / sim.order.len() as f64,
        w.log.len()
    );
}

/// `ordered_region_voxels` agrees with `ordered_region_tops` (highest code per column) on a spawn-area replay.
#[test]
#[ignore]
fn spawn_voxels_match_tops() {
    use rwg2_biomes::gtorder::LoadSim;
    use rwg2_biomes::population::PreGates;
    let dir = concat!(env!("CARGO_MANIFEST_DIR"), "/../../data/gtnh-2.9.0");
    let table = BiomeTable::from_rwg2dump_json(
        &std::fs::read_to_string(format!("{dir}/biomes.json")).unwrap(),
    )
    .unwrap();
    let p = BiomeProvider::new(
        1631302684446519836,
        WorldType::Rwg2Continents,
        table,
        RwgConfig::default(),
    );
    let h = TableHeights;
    let g = PreGates::default();
    let mut sim = LoadSim::new();
    sim.spawn_area(0, 0);
    let rect = (-11, -11, 10, 10);
    let n = 22;
    let tops = rwg2_biomes::deco::view::ordered_region_tops(&p, &h, &g, &sim.order, rect);
    let vox = rwg2_biomes::deco::view::ordered_region_voxels(&p, &h, &g, &sim.order, rect);
    assert_eq!(vox.len(), (n * n) as usize);
    let (mut cols, mut bytes) = (0, 0);
    for (ci, (y0, hh, codes)) in vox.iter().enumerate() {
        bytes += 4 + codes.len();
        let (ccx, ccz) = (ci as i32 % n, ci as i32 / n);
        for lz in 0..16 {
            for lx in 0..16 {
                let mut top = (0u8, 0u8);
                for y in (0..*hh).rev() {
                    let c = codes[((y * 16 + lz) * 16 + lx) as usize];
                    if c != 0 {
                        top = (c, (y0 + y).clamp(0, 255) as u8);
                        break;
                    }
                }
                let i = ((ccz * 16 + lz) * 16 * n + ccx * 16 + lx) as usize;
                assert_eq!(top, tops[i], "chunk {ccx},{ccz} col {lx},{lz}");
                cols += (top.0 != 0) as u32;
            }
        }
    }
    eprintln!("voxels: {} chunks, {cols} decorated columns, {bytes} bytes", vox.len());
}
