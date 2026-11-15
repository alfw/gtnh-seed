//! Flat areas in a biome (`Target::FlatBiome`): the patch scan on synthetic fields (band, share, nearest hit,
//! accept order, land rule, truncation) and against a direct enumeration (whole-box components, brute-force band)
//! on real seeds; the zoomed-out heights it uses vs exact chunk tops on the matched patches.
#![cfg(feature = "json")]
use rwg2_biomes::config::RwgConfig;
use rwg2_biomes::gtorder::ApproxTerrain;
use rwg2_biomes::provider::{BiomeId, BiomeProvider, BiomeTable, TableHeights, WorldType};
use rwg2_biomes::search::{
    flat_patch_search, Center, Cond, Flat, Query, SearchData, Searcher, Target, FLAT_PAD, FLAT_STEP,
};

fn data_dir() -> std::path::PathBuf {
    std::env::var("RWG2_DATA_DIR")
        .map(std::path::PathBuf::from)
        .unwrap_or_else(|_| std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../data"))
}

fn table() -> BiomeTable {
    BiomeTable::from_rwg2dump_json(
        &std::fs::read_to_string(data_dir().join("gtnh-2.9.0/biomes.json")).unwrap(),
    )
    .unwrap()
}

fn d2(x: i32, z: i32, c: (i32, i32)) -> i64 {
    let (dx, dz) = (x as i64 - c.0 as i64, z as i64 - c.1 as i64);
    dx * dx + dz * dz
}

#[test]
fn synthetic_patch() {
    let disc = |x: i32, z: i32| d2(x, z, (1000, 0)) <= 800 * 800;
    // west half flat at 70, east half rising 2 blocks per 32
    let top = |x: i32, _z: i32| if x < 1000 { 70 } else { 100 + (x - 1000) / 16 };
    let f = Flat {
        tol: 20,
        min_pct: 40,
        min_area: 0,
        land: true,
    };
    let (hit, info) = flat_patch_search(
        (0, 0),
        2000,
        &f,
        &mut |x, z| disc(x, z),
        &mut |x, z| top(x, z),
        &mut |_, _| true,
    )
    .expect("flat west half");
    assert_eq!(hit, (224, 0));
    assert_eq!((info.y_lo, info.y_hi), (71, 111));
    // band 70..110 = the west half + the east rim up to x ≈ 1160
    assert!((60..=66).contains(&info.pct), "{info:?}");
    assert!(!info.truncated);
    // accept order: next nearest in-band cells, ties by z then x
    let mut seen = Vec::new();
    let r = flat_patch_search(
        (0, 0),
        2000,
        &f,
        &mut |x, z| disc(x, z),
        &mut |x, z| top(x, z),
        &mut |x, z| {
            seen.push((x, z));
            seen.len() == 3
        },
    );
    assert_eq!(seen, vec![(224, 0), (224, -32), (224, 32)]);
    assert_eq!(r.unwrap().0, (224, 32));
    // stricter share: nothing
    let f60 = Flat { min_pct: 70, ..f };
    assert!(flat_patch_search(
        (0, 0),
        2000,
        &f60,
        &mut |x, z| disc(x, z),
        &mut |x, z| top(x, z),
        &mut |_, _| true
    )
    .is_none());
    // west half under water: land-only band moves east
    let wet = |x: i32, _z: i32| if x < 1000 { 50 } else { 100 + (x - 1000) / 16 };
    let (hit, info) = flat_patch_search(
        (0, 0),
        2000,
        &Flat { min_pct: 20, ..f },
        &mut |x, z| disc(x, z),
        &mut |x, z| wet(x, z),
        &mut |_, _| true,
    )
    .expect("east band");
    assert!(hit.0 >= 1000 && info.y_lo >= 100, "{hit:?} {info:?}");
    // min area: a 64×64 band can't hold 1000² blocks
    let fa = Flat {
        min_area: 1_000_000_000,
        ..f
    };
    assert!(flat_patch_search(
        (0, 0),
        2000,
        &fa,
        &mut |x, z| disc(x, z),
        &mut |x, z| top(x, z),
        &mut |_, _| true
    )
    .is_none());
    // everywhere inside: the patch runs into the scan box
    let (_, info) = flat_patch_search(
        (0, 0),
        500,
        &f,
        &mut |_, _| true,
        &mut |_, _| 70,
        &mut |_, _| true,
    )
    .unwrap();
    assert!(info.truncated && info.pct == 100);
}

/// Direct enumeration: every cell of the scan box, components by flood fill inside the box, band by trying every
/// window start, nearest in-band cell within r over all passing components (ties z, x).
/// Hit and (y_lo, y_hi, pct).
type Found = ((i32, i32), (i32, i32, u8));

fn brute(
    p: &BiomeProvider,
    h: &TableHeights,
    ids: &[BiomeId],
    c: (i32, i32),
    r: i32,
    f: &Flat,
) -> Option<Found> {
    let s = FLAT_STEP;
    let lim = r + FLAT_PAD;
    let (bi0, bj0, bi1, bj1) = (
        (c.0 - lim).div_euclid(s),
        (c.1 - lim).div_euclid(s),
        (c.0 + lim).div_euclid(s),
        (c.1 + lim).div_euclid(s),
    );
    let (w, hgt) = ((bi1 - bi0 + 1) as usize, (bj1 - bj0 + 1) as usize);
    let idx = |i: i32, j: i32| (j - bj0) as usize * w + (i - bi0) as usize;
    let mut inside = vec![false; w * hgt];
    for j in bj0..=bj1 {
        for i in bi0..=bi1 {
            inside[idx(i, j)] = ids.contains(&p.biome_data(i * s, j * s, h).data);
        }
    }
    let mut comp = vec![usize::MAX; w * hgt];
    let mut a = ApproxTerrain::new(p, h);
    #[allow(clippy::type_complexity)]
    let mut best: Option<(i64, i32, i32, (i32, i32, u8))> = None; // (d², z, x, band)
    let mut ncomp = 0;
    for j in bj0..=bj1 {
        for i in bi0..=bi1 {
            if !inside[idx(i, j)] || comp[idx(i, j)] != usize::MAX {
                continue;
            }
            let mut cells = vec![(i, j)];
            comp[idx(i, j)] = ncomp;
            let mut k = 0;
            while k < cells.len() {
                let (ci, cj) = cells[k];
                k += 1;
                for (ni, nj) in [(ci + 1, cj), (ci - 1, cj), (ci, cj + 1), (ci, cj - 1)] {
                    if ni >= bi0
                        && ni <= bi1
                        && nj >= bj0
                        && nj <= bj1
                        && inside[idx(ni, nj)]
                        && comp[idx(ni, nj)] == usize::MAX
                    {
                        comp[idx(ni, nj)] = ncomp;
                        cells.push((ni, nj));
                    }
                }
            }
            ncomp += 1;
            if !cells
                .iter()
                .any(|&(i, j)| d2(i * s, j * s, c) <= r as i64 * r as i64)
            {
                continue;
            }
            let tops: Vec<i32> = cells.iter().map(|&(i, j)| a.top(i * s, j * s)).collect();
            let mut cand: Vec<i32> = tops
                .iter()
                .copied()
                .filter(|&t| !f.land || t >= 63)
                .collect();
            cand.sort_unstable();
            cand.dedup();
            let (mut bc, mut blo) = (0usize, 0);
            for &lo in &cand {
                let n = tops
                    .iter()
                    .filter(|&&t| t >= lo && t <= lo + 2 * f.tol && (!f.land || t >= 63))
                    .count();
                if n > bc {
                    (bc, blo) = (n, lo);
                }
            }
            let area = (s * s) as i64;
            let pct = (bc as i64 * 100 / cells.len() as i64) as u8;
            if bc == 0
                || pct < f.min_pct
                || (bc as i64) * area < f.min_area
                || (cells.len() as i64) * area < f.min_area.max(1)
            {
                continue;
            }
            for (&(i, j), &t) in cells.iter().zip(&tops) {
                let (x, z) = (i * s, j * s);
                let d = d2(x, z, c);
                if d <= r as i64 * r as i64 && t >= blo && t <= blo + 2 * f.tol {
                    let key = (d, z, x);
                    if best.map_or(true, |b| key < (b.0, b.1, b.2)) {
                        best = Some((d, z, x, (blo + 1, blo + 2 * f.tol + 1, pct)));
                    }
                }
            }
        }
    }
    best.map(|b| ((b.2, b.1), b.3))
}

fn ids_for_gen(t: &BiomeTable, gens: &[u16]) -> Vec<BiomeId> {
    t.biomes
        .iter()
        .filter(|b| gens.contains(&b.gen_id))
        .map(|b| b.rwg_id)
        .collect()
}

#[test]
fn flat_matches_enumeration() {
    let t = table();
    let h = TableHeights;
    // Temperate Rainforest (humidity 120%) and Hot Plains
    let cases = [
        (
            ids_for_gen(&t, &[111]),
            2500,
            Flat {
                tol: 20,
                min_pct: 60,
                min_area: 128 * 128,
                land: true,
            },
        ),
        (
            ids_for_gen(&t, &[316]),
            1200,
            Flat {
                tol: 8,
                min_pct: 50,
                min_area: 0,
                land: true,
            },
        ),
    ];
    let (mut agree, mut hits) = (0, 0);
    for (ids, r, f) in cases {
        assert!(!ids.is_empty());
        let q = Query {
            world_type: WorldType::Rwg2Continents,
            land_at_spawn: false,
            conds: vec![Cond::new(
                Target::FlatBiome {
                    ids: ids.clone(),
                    flat: f,
                },
                r,
                false,
            )],
            step: 64,
            center: Center::Origin,
            dim: 0,
        };
        let sr = Searcher::new(q, t.clone(), RwgConfig::default(), SearchData::default());
        for seed in 1..=12i64 {
            let p = sr.provider(seed);
            let got = sr.check_with(&p, &h).map(|m| {
                let hit = m.hits[0];
                let fi = hit.flat.expect("flat info");
                ((hit.x, hit.z), (fi.y_lo, fi.y_hi, fi.pct))
            });
            let want = brute(&p, &h, &ids, (0, 0), r, &f);
            assert_eq!(got, want, "seed {seed} r {r}");
            agree += 1;
            hits += got.is_some() as i32;
        }
    }
    eprintln!("flat: {agree} seeds agree, {hits} matches");
    assert!(hits >= 3, "too few matches to mean anything: {hits}");
}

/// The band uses the zoomed-out heights; on matched patches the exact chunk tops put a similar share in that band.
#[test]
fn approx_band_vs_exact() {
    let t = table();
    let h = TableHeights;
    let ids = ids_for_gen(&t, &[316]);
    let f = Flat {
        tol: 10,
        min_pct: 50,
        min_area: 0,
        land: true,
    };
    let q = Query {
        world_type: WorldType::Rwg2Continents,
        land_at_spawn: false,
        conds: vec![Cond::new(
            Target::FlatBiome {
                ids: ids.clone(),
                flat: f,
            },
            1000,
            false,
        )],
        step: 64,
        center: Center::Origin,
        dim: 0,
    };
    let sr = Searcher::new(q, t.clone(), RwgConfig::default(), SearchData::default());
    let (mut n, mut diff) = (0, 0i32);
    for seed in 1..=30i64 {
        let p = sr.provider(seed);
        let Some(m) = sr.check_with(&p, &h) else {
            continue;
        };
        let fi = m.hits[0].flat.unwrap();
        if fi.patch_area > 2500 * 1024 {
            continue;
        }
        // the patch again (flood from the hit), exact tops
        let s = FLAT_STEP;
        let start = (m.hits[0].x / s, m.hits[0].z / s);
        let mut cells = vec![start];
        let mut seen = std::collections::HashSet::from([start]);
        let mut k = 0;
        while k < cells.len() {
            let (i, j) = cells[k];
            k += 1;
            for nb in [(i + 1, j), (i - 1, j), (i, j + 1), (i, j - 1)] {
                if !seen.contains(&nb) && ids.contains(&p.biome_data(nb.0 * s, nb.1 * s, &h).data) {
                    seen.insert(nb);
                    cells.push(nb);
                }
            }
        }
        let mut ex = rwg2_biomes::dungeons::Terrain::new(&p, &h);
        let inb = cells
            .iter()
            .filter(|&&(i, j)| {
                ex.top(i * s, j * s)
                    .is_some_and(|y| (fi.y_lo..=fi.y_hi).contains(&(y + 1)))
            })
            .count();
        let pe = (inb * 100 / cells.len()) as i32;
        eprintln!(
            "seed {seed}: approx {}% exact {pe}% band Y{}–{} patch {} cells",
            fi.pct,
            fi.y_lo,
            fi.y_hi,
            cells.len()
        );
        diff += (pe - fi.pct as i32).abs();
        n += 1;
        if n == 6 {
            break;
        }
    }
    assert!(n >= 3, "{n} patches");
    assert!(diff / n <= 12, "mean |exact − approx| share {}%", diff / n);
}

#[test]
#[ignore]
fn flat_speed() {
    let t = table();
    let h = TableHeights;
    for (gens, r) in [(vec![111u16], 3000), (vec![316], 3000), (vec![316], 1000)] {
        let ids = ids_for_gen(&t, &gens);
        let f = Flat {
            tol: 20,
            min_pct: 60,
            min_area: 128 * 128,
            land: true,
        };
        let q = Query {
            world_type: WorldType::Rwg2Continents,
            land_at_spawn: false,
            conds: vec![Cond::new(Target::FlatBiome { ids, flat: f }, r, false)],
            step: 64,
            center: Center::Origin,
            dim: 0,
        };
        let sr = Searcher::new(q, t.clone(), RwgConfig::default(), SearchData::default());
        let t0 = std::time::Instant::now();
        let n = 40;
        let m = (100..100 + n)
            .filter(|&s| sr.check_seed(s, &h).is_some())
            .count();
        let dt = t0.elapsed().as_secs_f64();
        eprintln!(
            "gens {gens:?} r {r}: {m}/{n} match, {:.1} seeds/s",
            n as f64 / dt
        );
    }
}

/// `flat_patch_cells` + `flat_patch_band` (map overlay, exact check) rebuild the patch and band the search reported.
#[test]
fn patch_rebuild_matches_search() {
    use rwg2_biomes::search::{flat_patch_band, flat_patch_cells};
    let t = table();
    let h = TableHeights;
    let ids = ids_for_gen(&t, &[111]);
    let f = Flat {
        tol: 20,
        min_pct: 60,
        min_area: 128 * 128,
        land: true,
    };
    let q = Query {
        world_type: WorldType::Rwg2Continents,
        land_at_spawn: false,
        conds: vec![Cond::new(
            Target::FlatBiome {
                ids: ids.clone(),
                flat: f,
            },
            2500,
            false,
        )],
        step: 64,
        center: Center::Origin,
        dim: 0,
    };
    let sr = Searcher::new(q, t.clone(), RwgConfig::default(), SearchData::default());
    let mut n = 0;
    for seed in 1..=10i64 {
        let p = sr.provider(seed);
        let Some(m) = sr.check_with(&p, &h) else {
            continue;
        };
        let hit = m.hits[0];
        let fi = hit.flat.unwrap();
        let (cells, trunc) = flat_patch_cells((0, 0), 2500, (hit.x, hit.z), &mut |x, z| {
            ids.contains(&p.biome_data(x, z, &h).data)
        })
        .unwrap();
        let mut a = ApproxTerrain::new(&p, &h);
        let tops: Vec<i32> = cells
            .iter()
            .map(|&(i, j)| a.top(i * FLAT_STEP, j * FLAT_STEP))
            .collect();
        let (info, (lo, hi)) = flat_patch_band(&tops, &f, trunc).expect("passes");
        assert_eq!(info, fi, "seed {seed}");
        assert_eq!((lo + 1, hi + 1), (fi.y_lo, fi.y_hi));
        n += 1;
    }
    assert!(n >= 3, "{n}");
}
