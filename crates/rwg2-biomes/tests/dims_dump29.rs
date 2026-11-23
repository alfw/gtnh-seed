#![cfg(feature = "json")]
//! Nether / Twilight Forest block terrain vs rwg2dump 2.9.0 (seed 92900795, chunks [-12, 12]^2, `dimterrain_*.bin`).
use rwg2_biomes::dimterrain::*;

fn root() -> std::path::PathBuf {
    std::env::var("RWG2_DATA_DIR")
        .map(|d| std::path::PathBuf::from(d).parent().unwrap().to_path_buf())
        .unwrap_or_else(|_| std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../.."))
}

const SEED: i64 = 92_900_795;
const N: usize = 256;
const COLS: usize = 625 * N;

/// One chunk's view into the section-major file: [preTop u8 ×C][preTopBlock u16 ×C][pre solid 32 B ×C]
/// [postTop u8 ×C][postTopBlock u16 ×C][post air 32 B ×C], C = 625·256 columns, column = chunk·256 + lz·16 + lx.
struct DumpChunk<'a>(&'a [u8], usize);
impl DumpChunk<'_> {
    fn g(&self, i: usize) -> usize {
        self.1 * N + i
    }
    fn pre_top(&self, i: usize) -> i32 {
        let v = self.0[self.g(i)];
        if v == 255 {
            -1
        } else {
            v as i32
        }
    }
    fn pre_solid(&self, i: usize, y: usize) -> bool {
        let o = COLS * 3 + self.g(i) * 32;
        self.0[o + (y >> 3)] >> (y & 7) & 1 != 0
    }
    #[allow(dead_code)]
    fn post_top(&self, i: usize) -> i32 {
        let v = self.0[COLS * 35 + self.g(i)];
        if v == 255 {
            -1
        } else {
            v as i32
        }
    }
    #[allow(dead_code)]
    fn post_air(&self, i: usize, y: usize) -> bool {
        let o = COLS * 38 + self.g(i) * 32;
        self.0[o + (y >> 3)] >> (y & 7) & 1 != 0
    }
}

fn load(name: &str) -> Option<Vec<u8>> {
    std::fs::read(root().join("testdata/pack/gtnh-2.9.0/dump29").join(name)).ok()
}

fn chunks(bin: &[u8]) -> impl Iterator<Item = (i32, i32, DumpChunk<'_>)> {
    (0..625usize).map(move |k| {
        (
            -12 + (k % 25) as i32,
            -12 + (k / 25) as i32,
            DumpChunk(bin, k),
        )
    })
}

#[test]
fn nether_terrain_vs_dump29() {
    let Some(bin) = load("dimterrain_m1.bin") else {
        eprintln!("skip");
        return;
    };
    let t = NetherTerrain::new(SEED);
    let (mut blocks, mut extra_solid, mut extra_air, mut cols_exact) = (0u64, 0u64, 0u64, 0u64);
    let t0 = std::time::Instant::now();
    for (cx, cz, d) in chunks(&bin) {
        let c = t.chunk(cx, cz);
        for lz in 0..16 {
            for lx in 0..16 {
                let i = lz * 16 + lx;
                let mut ok = true;
                for y in 0..128 {
                    let m = nb::solid(c.at(lx as i32, y as i32, lz as i32));
                    let r = d.pre_solid(i, y);
                    blocks += 1;
                    if m && !r {
                        extra_solid += 1;
                        ok = false;
                    }
                    if !m && r {
                        extra_air += 1;
                        ok = false;
                    }
                }
                cols_exact += ok as u64;
            }
        }
    }
    eprintln!("nether: {blocks} blocks, model solid/real air {extra_solid} (caves carve), model air/real solid {extra_air}; columns exact {cols_exact}/160000 ({:?})", t0.elapsed());
    assert_eq!((extra_solid, extra_air), (0, 0));
}

#[test]
fn twilight_terrain_vs_dump29() {
    let Some(bin) = load("dimterrain_7.bin") else {
        eprintln!("skip");
        return;
    };
    let t = TfTerrain::new(SEED);
    let (mut blocks, mut extra_solid, mut extra_air, mut cols_exact, mut top_ok) =
        (0u64, 0u64, 0u64, 0u64, 0u64);
    let mut bad: Vec<(i32, i32, usize, usize)> = Vec::new();
    let t0 = std::time::Instant::now();
    for (cx, cz, d) in chunks(&bin) {
        let c = t.chunk(cx, cz);
        let mut chunk_bad = 0;
        for lz in 0..16 {
            for lx in 0..16 {
                let i = lz * 16 + lx;
                let mut ok = true;
                for y in 0..256 {
                    let m = tb::solid(c.at(lx as i32, y as i32, lz as i32));
                    let r = d.pre_solid(i, y);
                    blocks += 1;
                    if m && !r {
                        extra_solid += 1;
                        ok = false;
                    }
                    if !m && r {
                        extra_air += 1;
                        ok = false;
                        chunk_bad += 1;
                    }
                }
                cols_exact += ok as u64;
                top_ok += (c.top(lx as i32, lz as i32) == d.pre_top(i)) as u64;
            }
        }
        if chunk_bad > 0 {
            bad.push((cx, cz, chunk_bad, 0));
        }
    }
    eprintln!("tf: {blocks} blocks, model solid/real air {extra_solid}, model air/real solid {extra_air}; columns exact {cols_exact}/160000, pre top {top_ok}/160000 ({:?})", t0.elapsed());
    eprintln!(
        "chunks with missing solids: {} {:?}",
        bad.len(),
        &bad[..bad.len().min(12)]
    );
    assert_eq!((extra_solid, extra_air), (0, 0));
    assert_eq!(top_ok, 160_000);
}

fn tables() -> rwg2_biomes::forestry::ClimateTable {
    rwg2_biomes::forestry::ClimateTable::from_json(
        &std::fs::read_to_string(root().join(pack_rel("climate.json"))).unwrap(),
    )
    .unwrap()
}

fn parse(s: &str) -> Vec<(String, u16)> {
    s.split_whitespace()
        .map(|t| {
            let (a, b) = t.split_once(':').unwrap();
            (a.to_string(), b.parse().unwrap())
        })
        .collect()
}

/// Aura nodes of dims -1 / 7 vs rwg2dump 2.9 (`gtScan["<dim>:terrain"].tcNodes`, chunks [-12, 12]^2 populated).
#[test]
fn dim_nodes_vs_dump29() {
    use rwg2_biomes::tc_dims::{DimTc, DimWorld};
    use rwg2_biomes::thaumcraft::{TC_ASPECTS_GTNH, TC_GTNH};
    let Ok(s) = std::fs::read_to_string(root().join("testdata/pack/gtnh-2.9.0/dump29/dims.json"))
    else {
        eprintln!("skip");
        return;
    };
    let d: serde_json::Value = serde_json::from_str(&s).unwrap();
    let climate = tables();
    let nether = NetherTerrain::new(SEED);
    let tf = TfTerrain::new(SEED);
    for (dim, key) in [(-1, "-1:terrain"), (7, "7:terrain")] {
        let real = d["gtScan"][key]["tcNodes"].as_array().unwrap();
        let w = if dim == -1 {
            DimWorld::Nether(&nether)
        } else {
            DimWorld::Twilight(&tf)
        };
        let mut g = DimTc::new(w, &climate, TC_GTNH);
        let mut nodes = Vec::new();
        let t0 = std::time::Instant::now();
        for cz in -12..=12 {
            for cx in -12..=12 {
                g.nodes(SEED, cx, cz, &mut nodes);
            }
        }
        let dt = t0.elapsed();
        let (mut xz, mut y, mut asp) = (0, 0, 0);
        for n in real {
            let (x, ry, z) = (
                n["x"].as_i64().unwrap() as i32,
                n["y"].as_i64().unwrap() as i32,
                n["z"].as_i64().unwrap() as i32,
            );
            let obs = n["aspects"].as_str().unwrap();
            let ty = n["type"].as_str().unwrap();
            let md = n["modifier"].as_str().unwrap();
            let Some(m) = nodes.iter().find(|m| (m.x, m.z) == (x, z)) else {
                eprintln!("dim {dim}: real node ({x},{ry},{z}) {ty} {obs} not predicted");
                continue;
            };
            xz += 1;
            y += (m.y == ry) as usize;
            let i = m.info.unwrap();
            let pa = i.aspects_string(&TC_ASPECTS_GTNH);
            let pt = format!("{:?}", i.node_type).to_uppercase();
            let pm = i
                .modifier
                .map(|m| format!("{m:?}").to_uppercase())
                .unwrap_or("null".into());
            let ok = pt == ty && pm == md && parse(&pa) == parse(obs);
            asp += ok as usize;
            eprintln!("dim {dim} {:?} ({x},{z}) y real {ry} model {} | real {ty}/{md} {obs} | model {pt}/{pm} {pa} env {:?} certain {} {}",
                m.kind, m.y, i.env, i.env_certain, if ok { "OK" } else { "--" });
        }
        let extra = nodes
            .iter()
            .filter(|m| {
                !real.iter().any(|n| {
                    (
                        n["x"].as_i64().unwrap() as i32,
                        n["z"].as_i64().unwrap() as i32,
                    ) == (m.x, m.z)
                })
            })
            .count();
        eprintln!(
            "dim {dim}: real {} predicted {} (extra {extra}) | xz {xz} y {y} contents {asp} ({dt:?} for 625 chunks)",
            real.len(),
            nodes.len()
        );
        // regression floor (rwg2dump 2.9, seed 92900795): Nether 12/12 xz, 9 contents; TF 20/20, 20 (greatwood replay)
        let (min_xz, min_asp) = if dim == -1 { (12, 9) } else { (20, 20) };
        assert!(
            xz >= min_xz && asp >= min_asp && extra == 0,
            "dim {dim}: xz {xz} contents {asp} extra {extra}"
        );
    }
}

/// Zoomed-out approximations vs the exact columns (overview quality, not exactness).
#[test]
fn approx_heights() {
    let n = NetherTerrain::new(SEED);
    let t = TfTerrain::new(SEED);
    let (mut dn, mut dt, mut k) = (0i64, 0i64, 0i64);
    let t0 = std::time::Instant::now();
    for cz in -6..6 {
        for cx in -6..6 {
            let c = n.chunk(cx, cz);
            let tc = t.chunk_basic(cx, cz);
            for (lx, lz) in [(0, 0), (5, 9), (12, 3)] {
                let (x, z) = (cx * 16 + lx, cz * 16 + lz);
                dn += (c.floor(lx, lz).0 - n.approx_floor(x, z).0).abs() as i64;
                dt += (tc.ground(lx, lz) - t.approx_ground(x, z)).abs() as i64;
                k += 1;
            }
        }
    }
    eprintln!(
        "approx mean |err|: nether floor {:.1}, tf ground {:.1} ({} samples, {:?})",
        dn as f64 / k as f64,
        dt as f64 / k as f64,
        k,
        t0.elapsed()
    );
    let t1 = std::time::Instant::now();
    for i in 0..1000 {
        std::hint::black_box(t.approx_ground(i * 37, i * 91));
    }
    let t2 = std::time::Instant::now();
    for i in 0..1000 {
        std::hint::black_box(n.approx_floor(i * 37, i * 91));
    }
    eprintln!(
        "approx cost: tf {:?}, nether {:?} per sample",
        (t2 - t1) / 1000,
        t2.elapsed() / 1000
    );
}

/// Pre-population top block names (TF): `tb::name` of the model's top block vs the dump's `preTopBlock`.
#[test]
fn tf_top_names_vs_dump29() {
    let Some(bin) = load("dimterrain_7.bin") else {
        return;
    };
    let js: serde_json::Value = serde_json::from_str(
        &std::fs::read_to_string(root().join("testdata/pack/gtnh-2.9.0/dump29/dims.json")).unwrap(),
    )
    .unwrap();
    let names: Vec<String> = js["gtScan"]["7:terrain"]["blocks"]
        .as_array()
        .unwrap()
        .iter()
        .map(|v| v.as_str().unwrap().into())
        .collect();
    let t = TfTerrain::new(SEED);
    let (mut n, mut ok) = (0usize, 0usize);
    let mut miss = std::collections::BTreeMap::<(String, &str), usize>::new();
    for (cx, cz, d) in chunks(&bin) {
        let c = t.chunk(cx, cz);
        for lz in 0..16 {
            for lx in 0..16 {
                let i = lz * 16 + lx;
                let y = d.pre_top(i);
                if y < 0 {
                    continue;
                }
                let o = COLS + d.g(i) * 2;
                let real = names
                    .get(u16::from_le_bytes([bin[o], bin[o + 1]]) as usize)
                    .cloned()
                    .unwrap_or_default();
                let model = tb::name(c.at(lx as i32, y, lz as i32));
                n += 1;
                if real == model {
                    ok += 1;
                } else {
                    *miss.entry((real, model)).or_default() += 1;
                }
            }
        }
    }
    eprintln!("tf top names {ok}/{n}; misses {miss:?}");
    assert_eq!(ok, n, "TF top names: {miss:?}");
}

/// `data/<PACK_DATA>/<name>` relative to the repo (`PACK_DATA` default `gtnh-2.9.0`; `gtnh-2.9.1` = RWG2 2.0.9 pack).
#[allow(dead_code)]
fn pack_rel(name: &str) -> String {
    format!("data/{}/{name}", std::env::var("PACK_DATA").unwrap_or_else(|_| "gtnh-2.9.0".into()))
}
