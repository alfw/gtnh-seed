//! `TcGen::structures_in_rect` (skips chunks that cannot roll a structure) = the full per-chunk replay without
//! wild nodes, record for record.
#![cfg(feature = "json")]
use rwg2_biomes::config::RwgConfig;
use rwg2_biomes::dungeons::Terrain;
use rwg2_biomes::forestry::ClimateTable;
use rwg2_biomes::provider::{BiomeProvider, BiomeTable, TableHeights, WorldType};
use rwg2_biomes::thaumcraft::*;
use std::path::PathBuf;

fn root() -> PathBuf {
    std::env::var("RWG2_DATA_DIR")
        .map(|d| PathBuf::from(d).parent().unwrap().to_path_buf())
        .unwrap_or_else(|_| PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../.."))
}

fn key(f: &TcFeature) -> String {
    format!("{f:?}")
}

#[test]
fn structures_in_rect_matches_full_replay() {
    let table = BiomeTable::from_rwg2dump_json(
        &std::fs::read_to_string(root().join(pack_rel("biomes.json"))).unwrap(),
    )
    .unwrap();
    let climate = ClimateTable::from_json(
        &std::fs::read_to_string(root().join(pack_rel("climate.json"))).unwrap(),
    )
    .unwrap();
    let (mut full_ms, mut fast_ms, mut rings_ms, mut n) = (0.0, 0.0, 0.0, 0);
    // native: two whole 2048 cells per seed; wasi: a smaller rect (no threads, slower)
    let half = if cfg!(target_os = "wasi") { 24 } else { 64 };
    for (seed, wt) in [
        (1337i64, WorldType::Rwg2Continents),
        (92900795, WorldType::Rwg2Continents),
        (-7, WorldType::Rwg2),
    ] {
        let p = BiomeProvider::new(seed, wt, table.clone(), RwgConfig::default());
        for (ox, oz) in [(0, 0), (-2 * half, 0)] {
            let (cx0, cz0, cx1, cz1) = (ox - half, oz - half, ox + half - 1, oz + half - 1);
            let mut t = Terrain::new(&p, &TableHeights);
            let mut g = TcGen {
                t: &mut t,
                climate: &climate,
                cfg: TC_GTNH,
            };
            let t0 = std::time::Instant::now();
            let mut full = Vec::new();
            for cz in cz0..=cz1 {
                for cx in cx0..=cx1 {
                    let _ = g.chunk(seed, cx, cz, &mut full);
                }
            }
            full.retain(|f| f.kind != TcKind::WildNode);
            full_ms += t0.elapsed().as_secs_f64() * 1e3;
            let mut t2 = Terrain::new(&p, &TableHeights);
            let mut g2 = TcGen {
                t: &mut t2,
                climate: &climate,
                cfg: TC_GTNH,
            };
            let t1 = std::time::Instant::now();
            let mut fast = Vec::new();
            g2.structures_in_rect(seed, cx0, cz0, cx1, cz1, &mut fast);
            fast_ms += t1.elapsed().as_secs_f64() * 1e3;
            let (a, b): (Vec<String>, Vec<String>) = (
                full.iter().map(key).collect(),
                fast.iter().map(key).collect(),
            );
            assert_eq!(a, b, "seed {seed} rect {cx0},{cz0}");
            n += full.len();
            // rings-only pass (maze margins): the same rings
            let mut t3 = Terrain::new(&p, &TableHeights);
            let mut g3 = TcGen {
                t: &mut t3,
                climate: &climate,
                cfg: TcConfig {
                    rings_only: true,
                    ..TC_GTNH
                },
            };
            let t2 = std::time::Instant::now();
            let mut rings = Vec::new();
            g3.structures_in_rect(seed, cx0, cz0, cx1, cz1, &mut rings);
            rings_ms += t2.elapsed().as_secs_f64() * 1e3;
            let want: Vec<String> = full
                .iter()
                .filter(|f| f.kind == TcKind::Ring)
                .map(key)
                .collect();
            assert_eq!(
                want,
                rings.iter().map(key).collect::<Vec<_>>(),
                "rings-only seed {seed} rect {cx0},{cz0}"
            );
        }
    }
    println!("tc structures: {n} records; full replay {full_ms:.0} ms, skip path {fast_ms:.0} ms, rings only {rings_ms:.0} ms");
}

/// `data/<PACK_DATA>/<name>` relative to the repo (`PACK_DATA` default `gtnh-2.9.0`; `gtnh-2.9.1` = RWG2 2.0.9 pack).
#[allow(dead_code)]
fn pack_rel(name: &str) -> String {
    format!("data/{}/{name}", std::env::var("PACK_DATA").unwrap_or_else(|_| "gtnh-2.9.0".into()))
}
