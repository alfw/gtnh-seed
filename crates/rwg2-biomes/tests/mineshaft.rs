#![cfg(feature = "json")]
//! Mineshafts vs rwg2dump `structures.json` (start chunks + every piece box).
use rwg2_biomes::config::RwgConfig;
use rwg2_biomes::forestry::ClimateTable;
use rwg2_biomes::mineshaft::*;
use rwg2_biomes::provider::{BiomeProvider, BiomeTable, TableHeights, WorldType};

fn root() -> std::path::PathBuf {
    std::env::var("RWG2_DATA_DIR")
        .map(|d| std::path::PathBuf::from(d).parent().unwrap().to_path_buf())
        .unwrap_or_else(|_| std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../.."))
}

fn check(dir: &str) {
    let Ok(s) = std::fs::read_to_string(
        root().join(format!("testdata/pack/gtnh-2.9.0/{dir}/structures.json")),
    ) else {
        eprintln!("skip {dir}");
        return;
    };
    let d: serde_json::Value = serde_json::from_str(&s).unwrap();
    let seed: i64 = d["worldSeed"]
        .as_str()
        .map(|s| s.parse().unwrap())
        .or_else(|| d["worldSeed"].as_i64())
        .unwrap();
    let r = d["scanRadiusChunks"].as_i64().unwrap() as i32;
    let last = -r + ((2 * r) / 17) * 17;
    let (lo, hi) = (-r - 8, last + 8);
    let inside = |e: &&serde_json::Value| {
        let (x, z) = (
            e["chunkX"].as_i64().unwrap() as i32,
            e["chunkZ"].as_i64().unwrap() as i32,
        );
        (lo..=hi).contains(&x) && (lo..=hi).contains(&z)
    };
    // starts outside the scan window come from the save (explored chunks); they still must be starts
    let all = d["mineshaftGenerator"]["starts"].as_array().unwrap();
    let starts: Vec<&serde_json::Value> = all.iter().filter(inside).collect();
    let table = BiomeTable::from_rwg2dump_json(
        &std::fs::read_to_string(root().join("data/gtnh-2.9.0/biomes.json")).unwrap(),
    )
    .unwrap();
    let climate = ClimateTable::from_json(
        &std::fs::read_to_string(root().join("data/gtnh-2.9.0/climate.json")).unwrap(),
    )
    .unwrap();
    let p = BiomeProvider::new(seed, WorldType::Rwg2Continents, table, RwgConfig::default());
    let seeds = Seeds::new(seed);
    let t0 = std::time::Instant::now();
    let got = starts_in(&seeds, lo, lo, hi, hi);
    let dt = t0.elapsed();
    let mut want: Vec<(i32, i32)> = starts
        .iter()
        .map(|e| {
            (
                e["chunkX"].as_i64().unwrap() as i32,
                e["chunkZ"].as_i64().unwrap() as i32,
            )
        })
        .collect();
    want.sort();
    let mut g2 = got.clone();
    g2.sort();
    eprintln!(
        "{dir} seed {seed}: starts model {} dump {} ({dt:?} for {} chunks)",
        g2.len(),
        want.len(),
        (hi - lo + 1).pow(2)
    );
    assert_eq!(g2, want, "start set");
    let (mut exact, mut xz_ok, mut mesa_hits, mut total_p) = (0, 0, 0, 0);
    for e in &starts {
        let e = *e;
        let (cx, cz) = (
            e["chunkX"].as_i64().unwrap() as i32,
            e["chunkZ"].as_i64().unwrap() as i32,
        );
        let gid = p.biome_gen_id(cx * 16 + 8, cz * 16 + 8, &TableHeights);
        let mesa = climate.dict(gid).iter().any(|t| t == "MESA");
        let real_mesa = e["class"]
            .as_str()
            .unwrap()
            .ends_with("StructureMesaMineshaftStart");
        let wp: Vec<(String, Vec<i32>)> = e["pieces"]
            .as_array()
            .unwrap()
            .iter()
            .map(|q| {
                (
                    q["class"]
                        .as_str()
                        .unwrap()
                        .rsplit('$')
                        .next()
                        .unwrap()
                        .to_string(),
                    q["box"]
                        .as_array()
                        .unwrap()
                        .iter()
                        .map(|v| v.as_i64().unwrap() as i32)
                        .collect(),
                )
            })
            .collect();
        total_p += wp.len();
        let try_m = |mesa| {
            let m = mineshaft(&seeds, cx, cz, mesa).unwrap();
            let gp: Vec<(String, Vec<i32>)> = m
                .pieces
                .iter()
                .map(|q| (q.kind.java_name().to_string(), q.bb.to_vec()))
                .collect();
            gp
        };
        let gp = try_m(mesa);
        let strip = |v: &[(String, Vec<i32>)]| {
            v.iter()
                .map(|(k, b)| (k.clone(), b[0], b[2], b[3], b[5]))
                .collect::<Vec<_>>()
        };
        if strip(&gp) == strip(&wp) {
            xz_ok += 1;
        } else {
            eprintln!(
                "  ({cx},{cz}) layout differs: model {} pieces, dump {}; first diff {:?}",
                gp.len(),
                wp.len(),
                gp.iter().zip(&wp).find(|(a, b)| a != b)
            );
        }
        if gp == wp {
            exact += 1;
        } else if try_m(!mesa) == wp {
            mesa_hits += 1;
            eprintln!(
                "  ({cx},{cz}) matches with mesa = {} (model {mesa}, dump class mesa {real_mesa}, biome {gid})",
                !mesa
            );
        }
    }
    eprintln!("{dir}: layouts (x/z) {xz_ok}/{} | exact incl. Y {exact} | exact with flipped mesa rule {mesa_hits} | {total_p} pieces", starts.len());
    assert_eq!(xz_ok, starts.len());
}

#[test]
fn mineshafts_1337() {
    check("dump");
}

#[test]
fn mineshafts_92900795() {
    check("dump28");
}

#[test]
fn layout_cost() {
    let s = Seeds::new(92_900_795);
    let st = starts_in(&s, -200, -200, 200, 200);
    let t = std::time::Instant::now();
    let n: usize = st
        .iter()
        .map(|&(x, z)| mineshaft(&s, x, z, false).unwrap().pieces.len())
        .sum();
    eprintln!(
        "{} mineshafts, {n} pieces, {:?} per mineshaft",
        st.len(),
        t.elapsed() / st.len() as u32
    );
}
