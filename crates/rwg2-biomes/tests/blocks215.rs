#![cfg(feature = "json")]
//! rwg2dump 2.15 `blocks215.json` block areas (full columns, id + metadata, before and after population):
//! overworld pre-population blocks incl. metadata vs `surface_chunk` (stained clay, red sand, coarse dirt, podzol,
//! filler layers), the population snow / ice pass vs the populated chunks, and Nether / TF pre-population block
//! names vs `dimterrain`. Skips when no 2.15 dump is present. The `.bin.gz` files must be gunzipped next to
//! themselves first (`gunzip -k blocks215_*.bin.gz`; the crate has no inflate).
use rwg2_biomes::chunkgen::ChunkGen;
use rwg2_biomes::config::RwgConfig;
use rwg2_biomes::dimterrain::{nb, tb, NetherTerrain, TfTerrain};
use rwg2_biomes::gtorder::Caves;
use rwg2_biomes::provider::{BiomeProvider, BiomeTable, TableHeights, WorldType};
use rwg2_biomes::surface::{
    apply_snow_cover, blk, block_label, block_meta, block_name, surface_chunk, SurfaceTable,
};
use serde_json::Value;
use std::collections::BTreeMap;
use std::path::PathBuf;

fn root() -> PathBuf {
    std::env::var("RWG2_DATA_DIR")
        .map(|d| PathBuf::from(d).parent().unwrap().to_path_buf())
        .unwrap_or_else(|_| PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../.."))
}

fn dumps() -> Vec<PathBuf> {
    fn walk(d: &std::path::Path, out: &mut Vec<PathBuf>) {
        let Ok(rd) = std::fs::read_dir(d) else { return };
        for e in rd.flatten() {
            let p = e.path();
            if p.is_dir() && !skip_dump_dir(&p) {
                walk(&p, out);
            } else if p.file_name().is_some_and(|n| n == "blocks215.json") {
                out.push(p.parent().unwrap().to_path_buf());
            }
        }
    }
    let mut v = Vec::new();
    walk(&root().join(dumps_under()), &mut v);
    v.sort();
    v
}

/// One dumped area: `pre`/`post` (block index, meta) per chunk, column, y.
struct Area {
    name: String,
    dim: i64,
    cx0: i32,
    cz0: i32,
    w: i32,
    bin: Vec<u8>,
}

impl Area {
    fn n(&self) -> usize {
        (self.w * self.w) as usize * 65536
    }
    /// (block index, meta) at chunk ci, column idx (lz*16+lx), y; `post` selects the populated pass.
    fn get(&self, post: bool, ci: usize, idx: usize, y: usize) -> (usize, u8) {
        let n = self.n();
        let base = if post { n * 3 } else { 0 };
        let o = ci * 65536 + idx * 256 + y;
        let id = u16::from_le_bytes([self.bin[base + o * 2], self.bin[base + o * 2 + 1]]) as usize;
        (id, self.bin[base + n * 2 + o])
    }
}

fn load() -> Vec<(i64, String, Vec<String>, Vec<Area>)> {
    let mut out = Vec::new();
    for d in dumps() {
        let j: Value =
            serde_json::from_str(&std::fs::read_to_string(d.join("blocks215.json")).unwrap())
                .unwrap();
        let seed: i64 = j["worldSeed"]
            .as_str()
            .and_then(|s| s.parse().ok())
            .unwrap_or(0);
        let wt = j["worldType"]
            .as_str()
            .unwrap_or("RWG2_CONTINENTS")
            .to_string();
        let names: Vec<String> = j["blocks"]
            .as_array()
            .map(|a| {
                a.iter()
                    .map(|v| v.as_str().unwrap_or("?").to_string())
                    .collect()
            })
            .unwrap_or_default();
        let mut areas = Vec::new();
        for a in j["areas"].as_array().into_iter().flatten() {
            let Some(f) = a["file"].as_str() else {
                eprintln!("{}: area {} failed: {}", d.display(), a["name"], a["error"]);
                continue;
            };
            let raw = d.join(f.trim_end_matches(".gz"));
            let Ok(bin) = std::fs::read(&raw) else {
                eprintln!("{}: missing {} (gunzip -k {f})", d.display(), raw.display());
                continue;
            };
            areas.push(Area {
                name: a["name"].as_str().unwrap_or("?").into(),
                dim: a["dim"].as_i64().unwrap_or(0),
                cx0: a["chunkX0"].as_i64().unwrap() as i32,
                cz0: a["chunkZ0"].as_i64().unwrap() as i32,
                w: a["chunks"].as_i64().unwrap() as i32,
                bin,
            });
        }
        out.push((seed, wt, names, areas));
    }
    out
}

fn top_of(a: &Area, names: &[String], post: bool, ci: usize, idx: usize) -> i32 {
    (0..256)
        .rev()
        .find(|&y| {
            names.get(a.get(post, ci, idx, y).0).map(String::as_str) != Some("minecraft:air")
        })
        .map_or(-1, |y| y as i32)
}

#[test]
fn overworld_blocks_vs_215() {
    let data = load();
    if data.is_empty() {
        eprintln!("no rwg2dump 2.15 dump; skipped");
        return;
    }
    let table = BiomeTable::from_rwg2dump_json(
        &std::fs::read_to_string(root().join(pack_rel("biomes.json"))).unwrap(),
    )
    .unwrap();
    let clim = rwg2_biomes::forestry::ClimateTable::from_json(
        &std::fs::read_to_string(root().join(pack_rel("climate.json"))).unwrap(),
    )
    .unwrap();
    let st = SurfaceTable::new(&table);
    let (mut cols, mut top_name, mut top_full, mut layer_n, mut layer_ok) =
        (0usize, 0usize, 0usize, 0usize, 0usize);
    let mut miss: BTreeMap<(String, String), usize> = BTreeMap::new();
    let mut snow = [[0usize; 2]; 2];
    let mut ice = [[0usize; 2]; 2];
    let mut veg = 0usize;
    for (seed, wt, names, areas) in &data {
        let ty = if wt.contains("CONTINENTS") {
            WorldType::Rwg2Continents
        } else {
            WorldType::Rwg2
        };
        let p = BiomeProvider::new(*seed, ty, table.clone(), RwgConfig::default());
        let caves = Caves::new(*seed, true);
        let m = rwg2_biomes::spawn::SurfaceModel::new(&p.table);
        let mut g = ChunkGen::new(&p);
        let full = |id: usize, meta: u8| -> String {
            let n = names.get(id).map(String::as_str).unwrap_or("?");
            if n.starts_with("minecraft:") {
                format!("{n}:{meta}")
            } else {
                n.to_string()
            }
        };
        let mfull = |b: u8| -> String {
            let n = block_name(b);
            if n.starts_with("minecraft:") {
                format!("{n}:{}", block_meta(b))
            } else {
                n.to_string()
            }
        };
        for a in areas.iter().filter(|a| a.dim == 0) {
            let (c0, n0, f0) = (cols, top_name, top_full);
            let (s0, i0) = (snow, ice);
            for ci in 0..(a.w * a.w) as usize {
                let (cx, cz) = (a.cx0 + ci as i32 % a.w, a.cz0 + ci as i32 / a.w);
                let c = g.generate(&p, &TableHeights, cx, cz).unwrap();
                let cc = rwg2_biomes::spawn::carved_chunk(
                    &p,
                    &TableHeights,
                    &m,
                    &c,
                    cx,
                    cz,
                    Some(&caves),
                );
                let pre = surface_chunk(&p, &TableHeights, &st, &c, cx, cz, &cc);
                let mut sn = surface_chunk(&p, &TableHeights, &st, &c, cx, cz, &cc);
                apply_snow_cover(
                    &p,
                    &TableHeights,
                    &|g| clim.get(g).map(|b| b.temperature),
                    &c,
                    &mut sn,
                    cx,
                    cz,
                );
                for idx in 0..256 {
                    // pre-population: top block (name, name + meta) and the 8 blocks below it
                    let ry = top_of(a, names, false, ci, idx);
                    cols += 1;
                    if ry < 0 || pre.top[idx] as i32 != ry {
                        continue;
                    }
                    let (rid, rm) = a.get(false, ci, idx, ry as usize);
                    let mb = pre.cols[idx][ry as usize];
                    if names.get(rid).map(String::as_str) == Some(block_name(mb)) {
                        top_name += 1;
                    }
                    if full(rid, rm) == mfull(mb) {
                        top_full += 1;
                    } else {
                        *miss
                            .entry((
                                format!(
                                    "top {} | {}",
                                    full(rid, rm),
                                    table.biomes[c.rwg[idx] as usize].name
                                ),
                                mfull(mb),
                            ))
                            .or_default() += 1;
                    }
                    for y in (ry - 8).max(0)..ry {
                        let (rid, rm) = a.get(false, ci, idx, y as usize);
                        layer_n += 1;
                        if full(rid, rm) == mfull(pre.cols[idx][y as usize]) {
                            layer_ok += 1;
                        } else {
                            *miss
                                .entry((
                                    format!("layer {}", full(rid, rm)),
                                    mfull(pre.cols[idx][y as usize]),
                                ))
                                .or_default() += 1;
                        }
                    }
                    // population snow / ice
                    let col = &sn.cols[idx];
                    let model_ice =
                        (0..256).any(|y| col[y] == blk::ICE && pre.cols[idx][y] == blk::WATER);
                    let real_ice = (0..256).any(|y| {
                        names.get(a.get(true, ci, idx, y).0).map(String::as_str)
                            == Some("minecraft:ice")
                            && names.get(a.get(false, ci, idx, y).0).map(String::as_str)
                                == Some("minecraft:water")
                    });
                    ice[real_ice as usize][model_ice as usize] += 1;
                    if pre.cols[idx][ry as usize] == blk::WATER || ry >= 255 {
                        continue;
                    }
                    let above = names
                        .get(a.get(true, ci, idx, ry as usize + 1).0)
                        .map(String::as_str)
                        .unwrap_or("?");
                    let model_snow = col[ry as usize + 1] == blk::SNOW_LAYER;
                    let real_snow = above == "minecraft:snow_layer";
                    // a plant / tree / structure on the spot, or a canopy above it (the snow lands on the leaves)
                    let post_top = top_of(a, names, true, ci, idx);
                    if !real_snow && (above != "minecraft:air" || post_top > ry + 1) {
                        veg += 1;
                        continue;
                    }
                    if model_snow && !real_snow && std::env::var("SNOW_MISS").is_ok() {
                        let (lx, lz) = ((idx & 15) as i32, (idx >> 4) as i32);
                        eprintln!(
                            "  snow FP {} ({}, {}, {}) top {} biome {} above {above}",
                            a.name,
                            cx * 16 + lx,
                            ry,
                            cz * 16 + lz,
                            names.get(rid).map(String::as_str).unwrap_or("?"),
                            table.biomes[c.rwg[idx] as usize].name
                        );
                    }
                    snow[real_snow as usize][model_snow as usize] += 1;
                }
            }
            eprintln!(
                "seed {seed} {}: cols {}, top name {}, name+meta {}; snow {:?}; ice {:?}",
                a.name,
                cols - c0,
                top_name - n0,
                top_full - f0,
                [
                    [snow[0][0] - s0[0][0], snow[0][1] - s0[0][1]],
                    [snow[1][0] - s0[1][0], snow[1][1] - s0[1][1]]
                ],
                [
                    [ice[0][0] - i0[0][0], ice[0][1] - i0[0][1]],
                    [ice[1][0] - i0[1][0], ice[1][1] - i0[1][1]]
                ]
            );
        }
    }
    let mut v: Vec<_> = miss.into_iter().collect();
    v.sort_by_key(|e| std::cmp::Reverse(e.1));
    for ((real, model), k) in v.iter().take(40) {
        eprintln!("  {k:6}  real {real} / model {model}");
    }
    eprintln!(
        "ALL: cols {cols}, top name {top_name}, name+meta {top_full}, layers {layer_ok}/{layer_n}; \
         snow (real × model) {snow:?}, ice {ice:?}, vegetation-blocked {veg}"
    );
    let _ = block_label;
    let (real, wrong) = (
        snow[1][1] + snow[1][0] + snow[0][1] + snow[0][0],
        snow[0][1] + snow[1][0],
    );
    assert!(top_full * 10000 >= top_name * 9999, "metadata regressed");
    assert!(top_name * 10000 >= cols * 9999, "top blocks regressed");
    assert!(wrong * 200 <= real, "snow regressed: {snow:?}");
    assert!(
        (ice[0][1] + ice[1][0]) * 100 <= ice[1][1].max(1),
        "ice regressed: {ice:?}"
    );
}

#[test]
fn dim_blocks_vs_215() {
    let data = load();
    if data.is_empty() {
        eprintln!("no rwg2dump 2.15 dump; skipped");
        return;
    }
    for (seed, _, names, areas) in &data {
        for a in areas.iter().filter(|a| a.dim != 0) {
            let (nt, tt) = if a.dim == -1 {
                (Some(NetherTerrain::new(*seed)), None)
            } else {
                (None, Some(TfTerrain::new(*seed)))
            };
            let mut miss: BTreeMap<(String, &str), usize> = BTreeMap::new();
            let (mut n, mut ok, mut surf_n, mut surf_ok) = (0usize, 0usize, 0usize, 0usize);
            for ci in 0..(a.w * a.w) as usize {
                let (cx, cz) = (a.cx0 + ci as i32 % a.w, a.cz0 + ci as i32 / a.w);
                let nc = nt.as_ref().map(|t| t.chunk(cx, cz));
                let tc = tt.as_ref().map(|t| t.chunk(cx, cz));
                for idx in 0..256 {
                    let (lx, lz) = ((idx & 15) as i32, (idx >> 4) as i32);
                    let hy = if a.dim == -1 { 128 } else { 256 };
                    for y in 0..hy {
                        let real = names
                            .get(a.get(false, ci, idx, y as usize).0)
                            .map(String::as_str)
                            .unwrap_or("?");
                        let (model, air_above) = match (&nc, &tc) {
                            (Some(c), _) => {
                                (nb::name(c.at(lx, y, lz)), c.at(lx, y + 1, lz) == nb::AIR)
                            }
                            (_, Some(c)) => (
                                tb::name(c.at(lx, y, lz)),
                                matches!(c.at(lx, y + 1, lz), tb::AIR | tb::VOID),
                            ),
                            _ => unreachable!(),
                        };
                        n += 1;
                        let surface = air_above && real != "minecraft:air";
                        if surface {
                            surf_n += 1;
                        }
                        if real == model {
                            ok += 1;
                            if surface {
                                surf_ok += 1;
                            }
                        } else {
                            *miss.entry((real.to_string(), model)).or_default() += 1;
                            if std::env::var("DIM_MISS").is_ok() {
                                let col = |pre: bool| -> String {
                                    (y.saturating_sub(4)..(y + 4).min(hy))
                                        .rev()
                                        .map(|yy| {
                                            let nm = names
                                                .get(a.get(pre, ci, idx, yy as usize).0)
                                                .map(String::as_str)
                                                .unwrap_or("?");
                                            nm.rsplit(':')
                                                .next()
                                                .unwrap_or("?")
                                                .chars()
                                                .take(3)
                                                .collect::<String>()
                                        })
                                        .collect::<Vec<_>>()
                                        .join(" ")
                                };
                                let mcol: String = (y.saturating_sub(4)..(y + 4).min(hy))
                                    .rev()
                                    .map(|yy| {
                                        let nm = match (&nc, &tc) {
                                            (Some(c), _) => nb::name(c.at(lx, yy, lz)),
                                            (_, Some(c)) => tb::name(c.at(lx, yy, lz)),
                                            _ => "?",
                                        };
                                        nm.rsplit(':')
                                            .next()
                                            .unwrap_or("?")
                                            .chars()
                                            .take(3)
                                            .collect::<String>()
                                    })
                                    .collect::<Vec<_>>()
                                    .join(" ");
                                eprintln!(
                                    "  miss ({}, {y}, {}) real {real} model {model} | real {} | model {mcol}",
                                    cx * 16 + lx,
                                    cz * 16 + lz,
                                    col(false)
                                );
                            }
                        }
                    }
                }
            }
            let mut v: Vec<_> = miss.into_iter().collect();
            v.sort_by_key(|e| std::cmp::Reverse(e.1));
            assert_eq!(
                ok,
                n,
                "{} blocks regressed: {:?}",
                a.name,
                &v[..v.len().min(15)]
            );
            eprintln!(
                "seed {seed} {} (dim {}): blocks {ok}/{n}, surfaces (air above) {surf_ok}/{surf_n}; top misses {:?}",
                a.name,
                a.dim,
                &v[..v.len().min(15)]
            );
        }
    }
}

/// `data/<PACK_DATA>/<name>` relative to the repo (`PACK_DATA` default `gtnh-2.9.0`; `gtnh-2.9.1` = RWG2 2.0.9 pack).
#[allow(dead_code)]
fn pack_rel(name: &str) -> String {
    format!("data/{}/{name}", std::env::var("PACK_DATA").unwrap_or_else(|_| "gtnh-2.9.0".into()))
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
