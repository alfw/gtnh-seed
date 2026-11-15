//! Raw C-ABI exports for the browser (no wasm-bindgen). One instance per Web Worker.
//!
//! JS contract (all integers little-endian, memory = `instance.exports.memory`):
//! - `create(seed: i64 /*BigInt*/, world_type: u32 /*0 = RWG2, 1 = RWG2_CONTINENTS*/) -> handle`
//! - `render(handle, x0, z0, w, h, step) -> *u16` — row-major `j*w + i`, sample at `(x0 + i*step, z0 + j*step)`;
//!   value = RWG biome id, or `LAVA_MARKER` (0xFFFF). Valid until the next `render`; copy it out.
//! - `inspect(handle, x, z) -> *f64[7]` = [rwgId, genId, meta, continentValue, isLavaMarker, assumedGates,
//!   terrainHeight (`getNoiseAt`, NaN if unknown)]
//! - `render_mode(handle, x0, z0, w, h, step, mode) -> *u16`: mode 0 = biome ids as `render` (at step 1 from the
//!   exact chunk generator: dithered in-game biome); mode 1 = Forestry climate at the surface
//!   (`temp | humidity << 4`, temp 1..6 = icy..hellish, humidity 0..2 = arid..damp; exact at step 1); mode 2 =
//!   terrain height × 8 (u16; exact chunk terrain at step 1, `getNoiseAt` estimate when zoomed out).
//! - `features(handle, x0, z0, x1, z1, flags) -> *i32`, length `features_len()` (in i32): records of 6
//!   `[kind, x, z, y, a, b]`; kind 1 = LootGames (x,z = chunk centre, y = surface estimate, a,b = chunk),
//!   kind 2 = Roguelike anchor (x,z = search centre, y = predicted entrance y, a,b = predicted spot; kind 3 = same
//!   but the spot is a low-confidence pick);
//!   kind 11..14 = Thaumcraft barrow mound / eldritch ring + obelisk / hilltop stones / totem (x, z, y; a,b =
//!   ring maze size w,h — resolve maze conflicts across cells on the JS side);
//!   kind 20 = GregTech ore vein at an oreseed (x, z = oreseed chunk centre, y = vein bottom Y (layers y-1..y+7),
//!   a = vein index into `veins` JSON, b = extents packed as west | east << 8 | north << 16 | south << 24, each the
//!   distance beyond the oreseed chunk);
//!   i32::MIN = unknown. flags: 1 LootGames, 2 Roguelike, 4 terrain estimates (slower), 8 Thaumcraft, 16 ore veins.
//! - `veins_ptr()/veins_len()` — UTF-8 JSON of the overworld vein table.
//! - `inspect` additionally returns [7] exact RWG id, [8] exact in-game biome id, [9] exact terrain height,
//!   [10] Forestry temperature class at the surface, [11] humidity class, [12] temperature value there.
//! - `strongholds(seed: i64) -> *i32[6]` = stronghold start chunks (x0, z0, x1, z1, x2, z2)
//! - `legend_ptr()/legend_len()` — UTF-8 JSON describing biomes (see `legend_json`).
//! - `pack_ptr()/pack_len()` — UTF-8 pack identifier.

use std::cell::{OnceCell, RefCell};

use rwg2_biomes::chunkgen::{Chunk, ChunkGen};
use rwg2_biomes::config::RwgConfig;
use rwg2_biomes::dungeons::{self, Terrain, ROGUE_GTNH};
use rwg2_biomes::forestry::{ClimateTable, TemperatureNoise};
use rwg2_biomes::provider::{
    BiomeProvider, BiomeTable, Heights, NoHeights, TableHeights, WorldType,
};

pub const LAVA_MARKER: u16 = 0xFFFF;
const BIOMES_JSON: &str = include_str!("../../../data/gtnh-2.9.0/biomes.json");
const GREGTECH_JSON: &str = include_str!("../../../data/gtnh-2.9.0/gregtech.json");
const GTFLUIDS_JSON: &str = include_str!("../../../data/gtnh-2.9.0/gtfluids.json");
const CLIMATE_JSON: &str = include_str!("../../../data/gtnh-2.9.0/climate.json");
const PACK: &str = "GTNH 2.9.0 / RWG2 2.0.6";

thread_local! {
    static TABLE: OnceCell<BiomeTable> = const { OnceCell::new() };
    static PROVIDERS: RefCell<Vec<Option<BiomeProvider>>> = const { RefCell::new(Vec::new()) };
    static OUT: RefCell<Vec<u16>> = const { RefCell::new(Vec::new()) };
    static INSPECT: RefCell<[f64; 13]> = const { RefCell::new([0.0; 13]) };
    static GENS: RefCell<Vec<Option<ChunkGen>>> = const { RefCell::new(Vec::new()) };
    static FEATURES: RefCell<Vec<i32>> = const { RefCell::new(Vec::new()) };
    static RING: RefCell<[i32; 6]> = const { RefCell::new([0; 6]) };
    static LEGEND: OnceCell<String> = const { OnceCell::new() };
}

/// Exact terrain when the embedded table has height models (rwg2dump v2), else assume-open gates.
fn heights() -> &'static dyn Heights {
    if TABLE.with(|t| t.get().map(|t| t.heights.is_empty()).unwrap_or(true)) {
        &NoHeights
    } else {
        &TableHeights
    }
}

fn table() -> BiomeTable {
    TABLE.with(|t| {
        t.get_or_init(|| BiomeTable::from_rwg2dump_json(BIOMES_JSON).expect("embedded biomes.json"))
            .clone()
    })
}

#[no_mangle]
pub extern "C" fn create(seed: i64, world_type: u32) -> u32 {
    let wt = if world_type == 0 {
        WorldType::Rwg2
    } else {
        WorldType::Rwg2Continents
    };
    let p = BiomeProvider::new(seed, wt, table(), RwgConfig::default());
    PROVIDERS.with(|ps| {
        let mut ps = ps.borrow_mut();
        if let Some(i) = ps.iter().position(|p| p.is_none()) {
            ps[i] = Some(p);
            i as u32
        } else {
            ps.push(Some(p));
            (ps.len() - 1) as u32
        }
    })
}

#[no_mangle]
pub extern "C" fn destroy(handle: u32) {
    PROVIDERS.with(|ps| {
        if let Some(slot) = ps.borrow_mut().get_mut(handle as usize) {
            *slot = None;
        }
    });
}

/// Pure-Rust entry point (also used by tests): fills `out` with display ids.
pub fn render_into(
    p: &BiomeProvider,
    x0: i32,
    z0: i32,
    w: u32,
    h: u32,
    step: i32,
    out: &mut Vec<u16>,
) {
    out.clear();
    out.reserve((w * h) as usize);
    for j in 0..h as i32 {
        let z = z0.wrapping_add(j.wrapping_mul(step));
        for i in 0..w as i32 {
            let x = x0.wrapping_add(i.wrapping_mul(step));
            out.push(p.display(x, z, heights()).unwrap_or(LAVA_MARKER));
        }
    }
}

/// Zoomed out, point sampling can miss small-biome patches (~75-block radius around `smallBiomePoints`): stamp
/// every patch centre in the tile (at least one pixel) with the biome there, if it is a small biome.
fn stamp_small_biomes(
    p: &BiomeProvider,
    x0: i32,
    z0: i32,
    w: u32,
    h: u32,
    step: i32,
    out: &mut [u16],
) {
    let (bx, bz) = p.biome_offset();
    let span = (w as i32 * step, h as i32 * step);
    let (fx, fz) = (x0.wrapping_add(bx) as f64, z0.wrapping_add(bz) as f64);
    let pts = p
        .small_points()
        .points_in_rect(fx, fz, fx + span.0 as f64, fz + span.1 as f64);
    let r = (60 / step).max(0);
    for (px, pz) in pts {
        let (wx, wz) = ((px as i32).wrapping_sub(bx), (pz as i32).wrapping_sub(bz));
        let Some(id) = p.display(wx, wz, heights()) else {
            continue;
        };
        if !p.table.small.iter().flatten().any(|&s| s == id) {
            continue;
        }
        let (ci, cj) = ((wx - x0).div_euclid(step), (wz - z0).div_euclid(step));
        for dj in -r..=r {
            for di in -r..=r {
                if di * di + dj * dj > r * r {
                    continue;
                }
                let (i, j) = (ci + di, cj + dj);
                if i >= 0 && j >= 0 && i < w as i32 && j < h as i32 {
                    out[(j as u32 * w + i as u32) as usize] = id;
                }
            }
        }
    }
}

#[no_mangle]
pub extern "C" fn render(handle: u32, x0: i32, z0: i32, w: u32, h: u32, step: i32) -> *const u16 {
    PROVIDERS.with(|ps| {
        let ps = ps.borrow();
        let Some(Some(p)) = ps.get(handle as usize) else {
            return std::ptr::null();
        };
        OUT.with(|o| {
            let mut o = o.borrow_mut();
            render_into(p, x0, z0, w, h, step, &mut o);
            o.as_ptr()
        })
    })
}

static CLIMATE: std::sync::OnceLock<ClimateTable> = std::sync::OnceLock::new();
static VEINS: std::sync::OnceLock<rwg2_biomes::gtores::VeinTable> = std::sync::OnceLock::new();

fn veins() -> &'static rwg2_biomes::gtores::VeinTable {
    VEINS.get_or_init(|| {
        rwg2_biomes::gtores::VeinTable::from_json(GREGTECH_JSON).expect("embedded gregtech.json")
    })
}

#[no_mangle]
pub extern "C" fn veins_ptr() -> *const u8 {
    GREGTECH_JSON.as_ptr()
}

#[no_mangle]
pub extern "C" fn veins_len() -> u32 {
    GREGTECH_JSON.len() as u32
}

static FLUIDS: std::sync::OnceLock<(rwg2_biomes::gtfluids::FluidTable, String)> =
    std::sync::OnceLock::new();

/// Overworld underground-fluid table (GT pick order) + its JSON for the viewer (`[{name, registry, displayName,
/// rgb, minAmount, maxAmount}]`, index = `fluid_index`).
pub fn fluids() -> &'static rwg2_biomes::gtfluids::FluidTable {
    &fluid_data().0
}

fn fluid_data() -> &'static (rwg2_biomes::gtfluids::FluidTable, String) {
    FLUIDS.get_or_init(|| {
        let t = rwg2_biomes::gtfluids::FluidTable::from_json(GTFLUIDS_JSON, "0")
            .expect("embedded gtfluids.json");
        let raw: serde_json::Value = serde_json::from_str(GTFLUIDS_JSON).unwrap();
        let src = raw["dimensions"]["0"]["fluids"]
            .as_array()
            .cloned()
            .unwrap_or_default();
        let list: Vec<_> = t
            .fluids
            .iter()
            .map(|f| {
                let r = src
                    .iter()
                    .find(|x| x["registry"].as_str() == Some(f.registry.as_str()));
                serde_json::json!({
                    "name": f.name, "registry": f.registry,
                    "displayName": r.and_then(|r| r["displayName"].as_str()).unwrap_or(&f.name),
                    "rgb": r.and_then(|r| r["rgb"].as_str()).unwrap_or("#888888"),
                    "minAmount": f.min_amount, "maxAmount": f.max_amount,
                })
            })
            .collect();
        let json = serde_json::json!({ "provisional": t.provisional, "fluids": list }).to_string();
        (t, json)
    })
}

#[no_mangle]
pub extern "C" fn fluids_ptr() -> *const u8 {
    fluid_data().1.as_ptr()
}

#[no_mangle]
pub extern "C" fn fluids_len() -> u32 {
    fluid_data().1.len() as u32
}
static TNOISE: std::sync::OnceLock<TemperatureNoise> = std::sync::OnceLock::new();

fn climate() -> &'static ClimateTable {
    CLIMATE.get_or_init(|| ClimateTable::from_json(CLIMATE_JSON).expect("embedded climate.json"))
}

fn tnoise() -> &'static TemperatureNoise {
    TNOISE.get_or_init(TemperatureNoise::new)
}

fn with_gen<R>(handle: u32, p: &BiomeProvider, f: impl FnOnce(&mut ChunkGen) -> R) -> R {
    GENS.with(|gs| {
        let mut gs = gs.borrow_mut();
        let i = handle as usize;
        if gs.len() <= i {
            gs.resize_with(i + 1, || None);
        }
        let g = gs[i].get_or_insert_with(|| ChunkGen::new(p));
        f(g)
    })
}

/// Bee-housing surface y: one above the top block (or above the water surface).
fn surface_y(height: f32) -> i32 {
    (height as i32).max(62) + 1
}

fn forestry_code(gen: u16, x: i32, y: i32, z: i32) -> u16 {
    match climate().classify(tnoise(), gen, x, y, z) {
        Some((t, h)) => t as u16 | (h as u16) << 4,
        None => 0,
    }
}

fn height_code(h: f32) -> u16 {
    (h * 8.0).clamp(0.0, 65535.0) as u16
}

/// Pure-Rust entry point for all render modes.
#[allow(clippy::too_many_arguments)]
pub fn render_mode_into(
    p: &BiomeProvider,
    g: &mut ChunkGen,
    x0: i32,
    z0: i32,
    w: u32,
    h: u32,
    step: i32,
    mode: u32,
    out: &mut Vec<u16>,
) {
    let hs = heights();
    let marker = p.table.lava_cave_marker_gen_id;
    let exact = step == 1 && !p.table.heights.is_empty();
    if !exact {
        if mode == 0 {
            render_into(p, x0, z0, w, h, step, out);
            if step >= 16 {
                // display only (the raw `render` export stays point-sampled for the dump checks)
                stamp_small_biomes(p, x0, z0, w, h, step, out);
            }
            return;
        }
        // Blocks mode: the point-sampled biome of the Biomes map (not `display`, which differs on the height grid)
        let mut ids = Vec::new();
        if mode == 3 {
            render_into(p, x0, z0, w, h, step, &mut ids);
        }
        out.clear();
        out.reserve((w * h) as usize);
        let hv = smoothed_heights(p, x0, z0, w, h, step, BLEND_RADIUS);
        for j in 0..h as i32 {
            let z = z0.wrapping_add(j.wrapping_mul(step));
            for i in 0..w as i32 {
                let x = x0.wrapping_add(i.wrapping_mul(step));
                let height = hv[(j as u32 * w + i as u32) as usize];
                out.push(if mode == 2 {
                    height_code(height)
                } else if mode == 3 {
                    let r = ids[(j as u32 * w + i as u32) as usize];
                    let gen = if r == LAVA_MARKER {
                        marker.unwrap_or(0)
                    } else {
                        p.table.biomes.get(r as usize).map_or(0, |b| b.gen_id)
                    };
                    let y = height as i32;
                    let b = zoomed_top(gen, y);
                    (b as u16) | if y < 62 { 0x80 } else { 0 } | ((y.clamp(0, 255) as u16) << 8)
                } else {
                    let gen = match p.display(x, z, hs) {
                        Some(r) => p.table.biomes[r as usize].gen_id,
                        None => marker.unwrap_or(0),
                    };
                    forestry_code(gen, x, surface_y(height), z)
                });
            }
        }
        return;
    }
    out.clear();
    out.resize((w * h) as usize, 0);
    let (x1, z1) = (x0 + w as i32 - 1, z0 + h as i32 - 1);
    if mode == 3 {
        let seed = p.seed();
        CAVES.with(|cv| {
            let mut cv = cv.borrow_mut();
            if cv.as_ref().map(|c| c.0) != Some(seed) {
                *cv = Some((seed, rwg2_biomes::gtorder::Caves::new(seed, true)));
            }
            let caves = &cv.as_ref().unwrap().1;
            for cz in (z0 >> 4)..=(z1 >> 4) {
                for cx in (x0 >> 4)..=(x1 >> 4) {
                    let Some(c) = g.generate(p, hs, cx, cz) else {
                        continue;
                    };
                    let sc = surface_of(p, hs, &c, cx, cz, Some(caves));
                    for lz in 0..16 {
                        let j = cz * 16 + lz - z0;
                        if j < 0 || j >= h as i32 {
                            continue;
                        }
                        for lx in 0..16 {
                            let i = cx * 16 + lx - x0;
                            if i < 0 || i >= w as i32 {
                                continue;
                            }
                            out[(j as u32 * w + i as u32) as usize] =
                                blocks_code(&sc, (lz * 16 + lx) as usize);
                        }
                    }
                }
            }
        });
        return;
    }
    for cz in (z0 >> 4)..=(z1 >> 4) {
        for cx in (x0 >> 4)..=(x1 >> 4) {
            let Some(c) = g.generate(p, hs, cx, cz) else {
                continue;
            };
            fill_chunk(&c, cx, cz, x0, z0, w, h, mode, marker, out);
        }
    }
}

/// Radius (blocks) of RWG2's height blend: `getNewNoise` mixes neighbour biomes' heights over 17×17 samples at
/// 8-block spacing.
pub const BLEND_RADIUS: i32 = 48;

/// Zoomed-out terrain height: per sample the column's own biome height with the chunk generator's inputs
/// (`chunkgen::approx_parts`), blurred with a separable triangle of `radius` blocks to stand in for the neighbour
/// blend, then the generator's post-blend steps (volcano, lava cave, river carving, mountain fade). Exact chunk
/// heights are only used at 1 block/px.
pub fn smoothed_heights(
    p: &BiomeProvider,
    x0: i32,
    z0: i32,
    w: u32,
    h: u32,
    step: i32,
    radius: i32,
) -> Vec<f32> {
    use rwg2_biomes::chunkgen::{approx_finish, approx_parts};
    let hs: &dyn Heights = if p.table.heights.is_empty() {
        &NoHeights
    } else {
        &TableHeights
    };
    let r = (radius.max(0) / step.max(1)) as usize;
    let (w, h) = (w as usize, h as usize);
    let (pw, ph) = (w + 2 * r, h + 2 * r);
    let at = |i: usize, j: usize| {
        (
            x0.wrapping_add((i as i32 - r as i32).wrapping_mul(step)),
            z0.wrapping_add((j as i32 - r as i32).wrapping_mul(step)),
        )
    };
    let mut a = vec![(64f32, 0f32); pw * ph];
    for j in 0..ph {
        for i in 0..pw {
            let (x, z) = at(i, j);
            a[j * pw + i] = approx_parts(p, hs, x, z).unwrap_or((64.0, 0.0));
        }
    }
    let k: Vec<f32> = (0..=2 * r)
        .map(|i| (r + 1 - i.abs_diff(r)) as f32)
        .collect();
    let ks: f32 = k.iter().sum();
    let mut b = vec![(0f32, 0f32); ph * w];
    for j in 0..ph {
        for i in 0..w {
            let (mut s0, mut s1) = (0f32, 0f32);
            for (t, kv) in k.iter().enumerate() {
                let v = a[j * pw + i + t];
                s0 += v.0 * kv;
                s1 += v.1 * kv;
            }
            b[j * w + i] = (s0 / ks, s1 / ks);
        }
    }
    let mut out = vec![0f32; w * h];
    for j in 0..h {
        for i in 0..w {
            let (mut s0, mut s1) = (0f32, 0f32);
            for (t, kv) in k.iter().enumerate() {
                let v = b[(j + t) * w + i];
                s0 += v.0 * kv;
                s1 += v.1 * kv;
            }
            let (x, z) = at(i + r, j + r);
            out[j * w + i] = approx_finish(p, hs, x, z, s0 / ks, s1 / ks);
        }
    }
    out
}

#[allow(clippy::too_many_arguments)]
fn fill_chunk(
    c: &Chunk,
    cx: i32,
    cz: i32,
    x0: i32,
    z0: i32,
    w: u32,
    h: u32,
    mode: u32,
    marker: Option<u16>,
    out: &mut [u16],
) {
    for lz in 0..16 {
        let z = cz * 16 + lz;
        let j = z - z0;
        if j < 0 || j >= h as i32 {
            continue;
        }
        for lx in 0..16 {
            let x = cx * 16 + lx;
            let i = x - x0;
            if i < 0 || i >= w as i32 {
                continue;
            }
            let idx = (lz * 16 + lx) as usize;
            let v = if mode == 2 {
                height_code(c.height[idx])
            } else if mode == 0 {
                if Some(c.gen[idx]) == marker {
                    LAVA_MARKER
                } else {
                    c.rwg[idx]
                }
            } else {
                forestry_code(c.gen[idx], x, surface_y(c.height[idx]), z)
            };
            out[(j as u32 * w + i as u32) as usize] = v;
        }
    }
}

#[no_mangle]
#[allow(clippy::too_many_arguments)]
pub extern "C" fn render_mode(
    handle: u32,
    x0: i32,
    z0: i32,
    w: u32,
    h: u32,
    step: i32,
    mode: u32,
) -> *const u16 {
    PROVIDERS.with(|ps| {
        let ps = ps.borrow();
        let Some(Some(p)) = ps.get(handle as usize) else {
            return std::ptr::null();
        };
        with_gen(handle, p, |g| {
            OUT.with(|o| {
                let mut o = o.borrow_mut();
                render_mode_into(p, g, x0, z0, w, h, step, mode, &mut o);
                o.as_ptr()
            })
        })
    })
}

/// Pure-Rust entry point for `features`.
pub fn features_into(
    p: &BiomeProvider,
    x0: i32,
    z0: i32,
    x1: i32,
    z1: i32,
    flags: u32,
    out: &mut Vec<i32>,
) {
    out.clear();
    let seed = p.seed();
    let (cx0, cz0, cx1, cz1) = (x0 >> 4, z0 >> 4, x1 >> 4, z1 >> 4);
    let hs = heights();
    let estimate = flags & 4 != 0 && !p.table.heights.is_empty();
    let mut t = Terrain::new(p, hs);
    const U: i32 = i32::MIN;
    if flags & 1 != 0 {
        for (cx, cz) in dungeons::lootgames_chunks(seed, 20, cx0, cz0, cx1, cz1) {
            let (x, z) = (cx * 16 + 8, cz * 16 + 8);
            let y = if estimate {
                dungeons::lootgames_surface(&mut t, x, z).unwrap_or(U)
            } else {
                U
            };
            out.extend_from_slice(&[1, x, z, y, cx, cz]);
        }
    }
    if flags & 8 != 0 && !p.table.heights.is_empty() {
        use rwg2_biomes::thaumcraft::{TcGen, TcKind, TC_GTNH};
        let mut f = Vec::new();
        {
            // flag 256: rings only (the viewer's maze-conflict margin cells)
            let mut g = TcGen {
                t: &mut t,
                climate: climate(),
                cfg: rwg2_biomes::thaumcraft::TcConfig {
                    rings_only: flags & 256 != 0,
                    ..TC_GTNH
                },
            };
            g.structures_in_rect(seed, cx0, cz0, cx1, cz1, &mut f);
        }
        for x in f.iter().filter(|x| x.valid && x.kind != TcKind::WildNode) {
            // rings: y = surface j | P(builds, before maze conflicts)% << 16 (mazes are resolved in JS across cells)
            let y = if x.kind == TcKind::Ring {
                x.y | (((x.p_valid * 100.0).round() as i32) << 16)
            } else {
                x.y
            };
            out.extend_from_slice(&[10 + x.kind as i32, x.x, x.z, y, x.size.0, x.size.1]);
        }
    }
    if flags & 64 != 0 {
        // villages (VillageNames, GTNH config): kind 40, x/z = start chunk centre, y = style index, a = biome gen id
        for v in rwg2_biomes::villages::villages_in_rect(p, hs, seed, cx0, cz0, cx1, cz1) {
            if v.viable_exact {
                out.extend_from_slice(&[40, v.x, v.z, v.style.index() as i32, v.biome as i32, 0]);
            }
        }
    }
    if flags & 128 != 0 {
        // mineshafts (exact layout + Y): kind 60, x/z = start chunk centre, y = start room floor,
        // a = piece count, b = minY | maxY << 8 of the whole mineshaft
        let s = rwg2_biomes::mineshaft::Seeds::new(seed);
        for (cx, cz) in rwg2_biomes::mineshaft::starts_in(&s, cx0, cz0, cx1, cz1) {
            let gid = p.biome_gen_id(cx * 16 + 8, cz * 16 + 8, hs);
            let mesa = climate().dict(gid).iter().any(|t| t == "MESA");
            if let Some(m) = rwg2_biomes::mineshaft::mineshaft(&s, cx, cz, mesa) {
                out.extend_from_slice(&[
                    60,
                    cx * 16 + 8,
                    cz * 16 + 8,
                    m.pieces[0].bb[1],
                    m.pieces.len() as i32,
                    m.bb[1] | m.bb[4] << 8,
                ]);
            }
        }
    }
    if flags & 32 != 0 {
        // GT underground fluid fields (8x8 chunks): kind 30, x/z = field corner, y = average L/op,
        // a = fluid index, b = best chunk L/op in the field
        let t = fluids();
        for (rx, rz) in
            rwg2_biomes::gtfluids::fields_in_rect(cx0 * 16, cz0 * 16, cx1 * 16 + 15, cz1 * 16 + 15)
        {
            if let Some((f, amounts)) = rwg2_biomes::gtfluids::field_chunks(seed, 0, rx, rz, t) {
                let lpo = |a: i32| {
                    if a <= rwg2_biomes::gtfluids::DIVIDER {
                        0
                    } else {
                        a / rwg2_biomes::gtfluids::DIVIDER
                    }
                };
                let best = amounts.iter().map(|&a| lpo(a)).max().unwrap_or(0);
                out.extend_from_slice(&[
                    30,
                    rx * 128,
                    rz * 128,
                    lpo(f.vein_average),
                    f.fluid_index as i32,
                    best,
                ]);
            }
        }
    }
    if flags & 16 != 0 {
        use rwg2_biomes::gtores::{alternative, is_oreseed, GT_GTNH};
        let table = veins();
        let spawn = spawn_xz(p, hs, seed);
        let mut approx = rwg2_biomes::gtorder::ApproxTerrain::new(p, hs);
        for cz in cz0..=cz1 {
            for cx in cx0..=cx1 {
                if !is_oreseed(&GT_GTNH, cx, cz) {
                    continue;
                }
                // order-aware placement test on approximate terrain (rwg2dump 2.6: 220/252 vs 213 before)
                let r = rwg2_biomes::gtorder::resolve_ordered_approx(
                    seed,
                    table,
                    &GT_GTNH,
                    cx,
                    cz,
                    spawn,
                    &mut approx,
                );
                if let Some(pl) = r {
                    let (sx, sz) = (cx * 16, cz * 16);
                    // load-order dependence: other outcomes from the ±2 window (most frequent different vein)
                    let outs = rwg2_biomes::gtorder::window_veins_approx(
                        seed,
                        table,
                        &GT_GTNH,
                        cx,
                        cz,
                        &mut approx,
                    );
                    let dep = outs.len() > 1;
                    let other = outs
                        .iter()
                        .filter_map(|(q, _)| q.as_ref())
                        .find(|q| q.vein != pl.vein)
                        .copied();
                    // a = vein | (alternative + 1) << 8 | alternative bottom Y << 16 (0 = none) | order-dependent << 24
                    // alternative = another window outcome if order-dependent, else the runner-up pick
                    let alt = match (dep, other) {
                        (true, Some(o)) => ((o.vein as i32 + 1) << 8) | ((o.min_y & 0xff) << 16),
                        _ => alternative(table, seed, cx, cz, Some(&pl))
                            .map_or(0, |a| ((a.vein as i32 + 1) << 8) | ((a.min_y & 0xff) << 16)),
                    } | ((dep as i32) << 24);
                    let packed = ((sx - pl.west) & 0xff)
                        | (((pl.east - sx - 16) & 0xff) << 8)
                        | (((sz - pl.north) & 0xff) << 16)
                        | (((pl.south - sz - 16) & 0x7f) << 24);
                    out.extend_from_slice(&[
                        20,
                        sx + 8,
                        sz + 8,
                        pl.min_y,
                        pl.vein as i32 | alt,
                        packed,
                    ]);
                }
            }
        }
    }
    if flags & 2 != 0 {
        // block-level Roguelike model (surface tops, caves, plant/vine probabilities); y = ground Y |
        // confidence% << 16 (confidence = P(the game picks this attempt) on the model)
        let mut rm = if estimate {
            Some(dungeons::RogueModel::new(p, hs, seed))
        } else {
            None
        };
        for (cx, cz) in dungeons::rogue_anchors(seed, &ROGUE_GTNH, cx0, cz0, cx1, cz1) {
            let (ax, az) = (cx * 16 + 4, cz * 16 + 4);
            let (kind, y, px, pz) = match rm.as_mut() {
                Some(m) => {
                    let d = m.dungeon(climate(), &ROGUE_GTNH, seed, cx, cz);
                    match d.pos {
                        Some((x, z)) => {
                            let conf = (d.confidence * 100.0).round() as i32;
                            (
                                if d.confidence >= 0.5 { 2 } else { 3 },
                                d.y | (conf << 16),
                                x,
                                z,
                            )
                        }
                        None => (2, U, U, U),
                    }
                }
                None => (2, U, U, U),
            };
            out.extend_from_slice(&[kind, ax, az, y, px, pz]);
        }
    }
}

#[no_mangle]
pub extern "C" fn features(
    handle: u32,
    x0: i32,
    z0: i32,
    x1: i32,
    z1: i32,
    flags: u32,
) -> *const i32 {
    PROVIDERS.with(|ps| {
        let ps = ps.borrow();
        let Some(Some(p)) = ps.get(handle as usize) else {
            return std::ptr::null();
        };
        FEATURES.with(|f| {
            let mut f = f.borrow_mut();
            features_into(p, x0, z0, x1, z1, flags, &mut f);
            f.as_ptr()
        })
    })
}

#[no_mangle]
pub extern "C" fn features_len() -> u32 {
    FEATURES.with(|f| f.borrow().len() as u32)
}

#[no_mangle]
pub extern "C" fn inspect(handle: u32, x: i32, z: i32) -> *const f64 {
    PROVIDERS.with(|ps| {
        let ps = ps.borrow();
        let Some(Some(p)) = ps.get(handle as usize) else {
            return std::ptr::null();
        };
        let h = heights();
        let exact = with_gen(handle, p, |g| g.generate(p, h, x >> 4, z >> 4));
        let (erwg, egen, eh, ft, fh, ftv) = match &exact {
            Some(c) => {
                let idx = (((z & 15) * 16) + (x & 15)) as usize;
                let y = surface_y(c.height[idx]);
                let cl = climate().get(c.gen[idx]);
                let (t, hu) = climate()
                    .classify(tnoise(), c.gen[idx], x, y, z)
                    .map(|(t, h)| (t as u8 as f64, h as u8 as f64))
                    .unwrap_or((f64::NAN, f64::NAN));
                let tv = cl
                    .map(|cl| tnoise().float_temperature(cl.temperature, x, y, z) as f64)
                    .unwrap_or(f64::NAN);
                (
                    c.rwg[idx] as f64,
                    c.gen[idx] as f64,
                    c.height[idx] as f64,
                    t,
                    hu,
                    tv,
                )
            }
            None => (f64::NAN, f64::NAN, f64::NAN, f64::NAN, f64::NAN, f64::NAN),
        };
        let s = p.biome_data(x, z, h);
        let gen = p.biome_gen_id(x, z, h);
        let marker = p.display(x, z, h).is_none();
        let height = p.noise_at(x, z, h).map(|v| v as f64).unwrap_or(f64::NAN);
        INSPECT.with(|r| {
            let mut r = r.borrow_mut();
            *r = [
                s.data as f64,
                gen as f64,
                s.meta as f64,
                p.continent_value(x, z) as f64,
                marker as u8 as f64,
                p.assumed_gates.get() as f64,
                height,
                erwg,
                egen,
                eh,
                ft,
                fh,
                ftv,
            ];
            r.as_ptr()
        })
    })
}

/// `{"pack":..,"lavaMarker":{id,name,color},"biomes":[{id,name,baseName,baseId,color,className,baseClass,
///  temperature,climates:[..],placements:[..]}]}` — only biomes reachable from the provider lists.
pub fn legend_json() -> String {
    use serde_json::{json, Value};
    let v: Value = serde_json::from_str(BIOMES_JSON).expect("embedded biomes.json");
    let t = table();
    let mut uses: std::collections::BTreeMap<
        u16,
        (
            std::collections::BTreeSet<&str>,
            std::collections::BTreeSet<&str>,
        ),
    > = Default::default();
    let climates = ["snow", "cold", "hot", "wet"];
    let mut add = |id: u16, climate: &'static str, placement: &'static str| {
        let e = uses.entry(id).or_default();
        e.0.insert(climate);
        e.1.insert(placement);
    };
    let groups: [(&'static str, &rwg2_biomes::provider::PerClimate); 11] = [
        ("core", &t.core),
        ("border", &t.border),
        ("coldBorder", &t.cold_border),
        ("hotBorder", &t.hot_border),
        ("veryColdBorder", &t.very_cold_border),
        ("veryHotBorder", &t.very_hot_border),
        ("littoral", &t.littoral),
        ("small", &t.small),
        ("island", &t.island),
        ("smallIsland", &t.small_island),
        ("largeIsland", &t.large_island),
    ];
    for (name, per) in groups {
        for (ci, list) in per.iter().enumerate() {
            for &id in list {
                add(id, climates[ci], name);
            }
        }
    }
    for (ci, &id) in t.ocean_shallow.iter().enumerate() {
        add(id, climates[ci], "ocean");
    }
    for (ci, &id) in t.ocean_deep.iter().enumerate() {
        add(id, climates[ci], "deepOcean");
    }
    add(t.ocean_shallow_kelp, "cold", "ocean");
    add(t.ocean_shallow_coral, "hot", "ocean");
    add(t.coast_ice, "snow", "coast");
    add(t.coast_dunes, "cold", "coast");
    if let Some(vi) = t.volcano_island {
        add(vi, "hot", "volcano");
    }
    let rb = v["realisticBiomes"].as_array().unwrap();
    let biomes: Vec<Value> = uses
        .iter()
        .filter_map(|(&id, (cl, pl))| {
            let b = rb.iter().find(|b| b["rwgId"].as_u64() == Some(id as u64))?;
            Some(json!({
                "id": id,
                "name": b["displayName"],
                "className": b["class"],
                "baseName": b["base"]["name"],
                "baseId": b["base"]["id"],
                "baseClass": b["base"]["class"],
                "color": b["base"]["color"],
                "temperature": b["base"]["temperature"],
                "forestry": b["base"]["id"].as_u64().and_then(|id| climate().classify(tnoise(), id as u16, 0, 64, 0)).map(|(t, h)| json!([t as u8, h as u8])),
                "climates": cl.iter().collect::<Vec<_>>(),
                "placements": pl.iter().collect::<Vec<_>>(),
            }))
        })
        .collect();
    let marker = &v["supportStatics"]["lavaCaveMarkerBiome"];
    json!({
        "pack": PACK,
        "lavaMarker": { "id": LAVA_MARKER, "genId": marker["id"], "name": marker["name"], "color": marker["color"],
            "forestry": marker["id"].as_u64().and_then(|id| climate().classify(tnoise(), id as u16, 0, 64, 0)).map(|(t, h)| json!([t as u8, h as u8])) },
        "biomes": biomes,
        // every registered biome's (temperature, rainfall = humidity) by gen id: `[[id, t, r], ...]`
        "climate": climate_rows(),
    })
    .to_string()
}

/// `[[gen id, temperature, rainfall], ...]` for all biomes in `climate.json` (exact floats from the pack dump).
fn climate_rows() -> Vec<serde_json::Value> {
    let v: serde_json::Value = serde_json::from_str(CLIMATE_JSON).expect("embedded climate.json");
    let f = |x: &serde_json::Value| {
        x.as_str()
            .and_then(|h| u32::from_str_radix(h, 16).ok())
            .map(f32::from_bits)
    };
    v["biomes"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(|b| {
            Some(serde_json::json!([
                b["id"].as_u64()?,
                f(&b["t"])?,
                f(&b["r"])?
            ]))
        })
        .collect()
}

#[no_mangle]
pub extern "C" fn strongholds(seed: i64) -> *const i32 {
    let r = rwg2_biomes::structures::stronghold_ring(seed);
    RING.with(|o| {
        let mut o = o.borrow_mut();
        *o = [r[0].0, r[0].1, r[1].0, r[1].1, r[2].0, r[2].1];
        o.as_ptr()
    })
}

thread_local! {
    static SPAWN_OUT: RefCell<[i32; 5]> = const { RefCell::new([0; 5]) };
}

/// Estimated world spawn: `[x, z, attempts, prob * 1000, confident]` (vanilla random walk from 0,0 to the first
/// column whose top block is grass; see `rwg2_biomes::spawn`).
#[no_mangle]
pub extern "C" fn spawn(handle: u32) -> *const i32 {
    PROVIDERS.with(|ps| {
        let ps = ps.borrow();
        let Some(Some(p)) = ps.get(handle as usize) else {
            return std::ptr::null();
        };
        let s = spawn_cached(p, heights(), p.seed());
        SPAWN_OUT.with(|o| {
            let mut o = o.borrow_mut();
            *o = [
                s.x,
                s.z,
                s.attempts,
                (s.prob * 1000.0) as i32,
                s.confident as i32,
            ];
            o.as_ptr()
        })
    })
}

thread_local! {
    static ROOMS: RefCell<[i32; 9]> = const { RefCell::new([0; 9]) };
}

/// End-portal frame centres of the 3 strongholds (`[x, y, z] * 3`, same order as `strongholds`; exact piece port).
#[no_mangle]
pub extern "C" fn portal_rooms(seed: i64) -> *const i32 {
    let ring = rwg2_biomes::structures::stronghold_ring(seed);
    ROOMS.with(|o| {
        let mut o = o.borrow_mut();
        for (i, &c) in ring.iter().enumerate() {
            let f = rwg2_biomes::stronghold_pieces::portal_room(seed, c)
                .map_or((i32::MIN, 0, i32::MIN), |r| r.eye_frame_centre);
            o[i * 3] = f.0;
            o[i * 3 + 1] = f.1;
            o[i * 3 + 2] = f.2;
        }
        o.as_ptr()
    })
}

#[no_mangle]
pub extern "C" fn legend_ptr() -> *const u8 {
    LEGEND.with(|l| l.get_or_init(legend_json).as_ptr())
}

#[no_mangle]
pub extern "C" fn legend_len() -> u32 {
    LEGEND.with(|l| l.get_or_init(legend_json).len() as u32)
}

#[no_mangle]
pub extern "C" fn pack_ptr() -> *const u8 {
    PACK.as_ptr()
}

#[no_mangle]
pub extern "C" fn pack_len() -> u32 {
    PACK.len() as u32
}

// ------------------------------------------------------------------------------------------------ search

use rwg2_biomes::search::{Center, Cond, Query, SearchData, Searcher, Target};

thread_local! {
    static SEARCHER: RefCell<Option<(String, Searcher)>> = const { RefCell::new(None) };
    static IN: RefCell<Vec<u8>> = const { RefCell::new(Vec::new()) };
    static RESULT: RefCell<String> = const { RefCell::new(String::new()) };
}

/// Scratch input buffer of `len` bytes (query JSON followed by seeds as i64 LE). Valid until the next call.
#[no_mangle]
pub extern "C" fn input_buf(len: u32) -> *mut u8 {
    IN.with(|b| {
        let mut b = b.borrow_mut();
        b.clear();
        b.resize(len as usize, 0);
        b.as_mut_ptr()
    })
}

/// Parse `{"worldType":0|1,"landAtSpawn":bool,"step":n,"center":"origin"|"spawn","conds":[{"t":..,"r":n,
/// "not":bool, ...}]}`; `t` = biome (`ids`: RWG ids) | volcano | lava | stronghold | lootgames | roguelike |
/// tc (`kind`: 1 mound, 2 obelisk, 3 hilltop, 4 totem) | vein (`vein`: index into the embedded vein table, or the
/// dimension's list) | adj (`a`, `b`: ids, `dist`) | tf (`ids`: TFFeature ids) | fortress. `"dim"`: 0 | -1 | 7 (ids
/// of biome/adj conditions are in-game biome ids there); `"center"` may also be `[x, z]`. A condition with
/// `"from": k` is a relation: its radius is measured from each candidate of condition `k` (see `search::Cond::near`).
pub fn parse_query(text: &str) -> Result<Query, String> {
    use rwg2_biomes::thaumcraft::TcKind;
    let v: serde_json::Value = serde_json::from_str(text).map_err(|e| e.to_string())?;
    let mut conds = Vec::new();
    for c in v["conds"].as_array().map(|a| a.as_slice()).unwrap_or(&[]) {
        let target = match c["t"].as_str().unwrap_or("") {
            "biome" => {
                let ids: Vec<u16> = c["ids"]
                    .as_array()
                    .map(|i| {
                        i.iter()
                            .filter_map(|x| x.as_u64())
                            .map(|x| x as u16)
                            .collect()
                    })
                    .unwrap_or_default();
                // `"flat": {tol, pct, area, land}`: flat area in the biome (overworld)
                match c["flat"].as_object() {
                    Some(f) => Target::FlatBiome {
                        ids,
                        flat: rwg2_biomes::search::Flat {
                            tol: f
                                .get("tol")
                                .and_then(|v| v.as_i64())
                                .unwrap_or(20)
                                .clamp(0, 128) as i32,
                            min_pct: f.get("pct").and_then(|v| v.as_u64()).unwrap_or(60).min(100)
                                as u8,
                            min_area: f
                                .get("area")
                                .and_then(|v| v.as_i64())
                                .unwrap_or(0)
                                .clamp(0, 1 << 40),
                            land: f.get("land").and_then(|v| v.as_bool()).unwrap_or(true),
                        },
                    },
                    None => Target::Biome(ids),
                }
            }
            "volcano" => Target::Volcano,
            "lava" => Target::LavaCave,
            "stronghold" => Target::Stronghold,
            "village" => Target::Village,
            "aspect" => Target::TcAspect {
                aspect: c["aspect"]
                    .as_i64()
                    .filter(|&a| (0..255).contains(&a))
                    .map_or(u8::MAX, |a| a as u8),
                min: c["min"].as_u64().unwrap_or(1).min(1000) as u16,
            },
            "lootgames" => Target::LootGames,
            "roguelike" => Target::Roguelike,
            "tc" => Target::Thaumcraft(match c["kind"].as_u64() {
                Some(1) => TcKind::Mound,
                Some(2) => TcKind::Ring,
                Some(3) => TcKind::Hilltop,
                Some(4) => TcKind::Totem,
                k => return Err(format!("bad tc kind {k:?}")),
            }),
            "vein" => {
                let v = c["vein"].as_u64().ok_or("vein index")? as usize;
                // fast: Nether / Twilight Forest first candidate only (no placement test)
                if c["fast"].as_bool() == Some(true) {
                    Target::OreVeinFast(v)
                } else {
                    Target::OreVein(v)
                }
            }
            "fluid" => Target::Fluid {
                fluid: c["fluid"]
                    .as_i64()
                    .filter(|&f| f >= 0)
                    .map_or(usize::MAX, |f| f as usize),
                min_lpo: c["minLpo"].as_i64().unwrap_or(0).clamp(0, 100_000) as i32,
            },
            "adj" => {
                let ids = |k: &str| -> Vec<u16> {
                    c[k].as_array()
                        .map(|i| {
                            i.iter()
                                .filter_map(|x| x.as_u64())
                                .map(|x| x as u16)
                                .collect()
                        })
                        .unwrap_or_default()
                };
                Target::Adjacent {
                    a: ids("a"),
                    b: ids("b"),
                    dist: c["dist"].as_i64().unwrap_or(64).clamp(4, 2000) as i32,
                }
            }
            "tf" => Target::TfFeature(
                c["ids"]
                    .as_array()
                    .map(|i| {
                        i.iter()
                            .filter_map(|x| x.as_u64())
                            .map(|x| x as u8)
                            .collect()
                    })
                    .unwrap_or_default(),
            ),
            "fortress" => match c["spawners"].as_u64() {
                Some(n) if n > 0 => Target::FortressSpawners(n.min(255) as u8),
                _ => Target::Fortress,
            },
            "villageStyle" => {
                use rwg2_biomes::villages::Style;
                Target::VillageStyle(match c["style"].as_u64() {
                    Some(0) => Style::Plains,
                    Some(1) => Style::Desert,
                    Some(2) => Style::Taiga,
                    Some(3) => Style::Savanna,
                    Some(4) => Style::Snowy,
                    Some(5) => Style::Jungle,
                    Some(6) => Style::Swamp,
                    s => return Err(format!("bad village style {s:?}")),
                })
            }
            "nodeType" => {
                use rwg2_biomes::tc_nodes::{NodeModifier, NodeType};
                Target::TcNodeType {
                    node_type: match c["type"].as_str().unwrap_or("") {
                        "Normal" => NodeType::Normal,
                        "Unstable" => NodeType::Unstable,
                        "Dark" => NodeType::Dark,
                        "Tainted" => NodeType::Tainted,
                        "Hungry" => NodeType::Hungry,
                        "Pure" => NodeType::Pure,
                        t => return Err(format!("bad node type {t:?}")),
                    },
                    // "" / missing = any modifier, "none" = no modifier
                    modifier: match c["mod"].as_str().unwrap_or("") {
                        "none" => Some(None),
                        "Bright" => Some(Some(NodeModifier::Bright)),
                        "Pale" => Some(Some(NodeModifier::Pale)),
                        "Fading" => Some(Some(NodeModifier::Fading)),
                        _ => None,
                    },
                }
            }
            "strongholdSurface" => Target::StrongholdSurface {
                ocean: c["ocean"].as_bool().unwrap_or(false),
            },
            // `{"t":"surface","blocks":[codes],"min":n,"water":bool}`: exact surface block (overworld)
            "surface" => Target::SurfaceBlock {
                blocks: c["blocks"]
                    .as_array()
                    .map(|a| {
                        a.iter()
                            .filter_map(|v| v.as_u64())
                            .map(|v| v as u8)
                            .collect()
                    })
                    .unwrap_or_default(),
                min: c["min"].as_u64().unwrap_or(1).clamp(1, 1 << 20) as u32,
                water: c["water"].as_bool().unwrap_or(false),
            },
            "mineshaft" => Target::Mineshaft,
            "geode" => Target::Geode {
                min_diamond: c["min"].as_u64().unwrap_or(0).min(64) as u16,
                kind: match c["kind"].as_str().unwrap_or("") {
                    "mountain" => 1,
                    "ocean" => 2,
                    _ => 0,
                },
            },
            // underground lake: liquid "lava" | "water" | any, top Y ≥ minY, ≥ min liquid blocks, open = touches a cave
            "lake" => Target::Lake {
                liquid: match c["liquid"].as_str().unwrap_or("") {
                    "lava" => 1,
                    "water" => 2,
                    _ => 0,
                },
                min_y: c["minY"].as_i64().unwrap_or(0).clamp(0, 255) as i32,
                min_cells: c["min"].as_u64().unwrap_or(0).min(4096) as u32,
                open: c["open"].as_bool().unwrap_or(false),
            },
            // chest loot: source "room" | "cart" | any; items = unlocalized-name fragments (any of), min = total count
            "loot" => Target::Loot {
                source: match c["source"].as_str().unwrap_or("") {
                    "room" => 1,
                    "cart" => 2,
                    _ => 0,
                },
                items: c["items"]
                    .as_array()
                    .map(|a| a.iter().filter_map(|v| v.as_str().map(str::to_string)).filter(|s| !s.is_empty()).take(16).collect())
                    .unwrap_or_default(),
                min_count: c["min"].as_u64().unwrap_or(1).clamp(1, 1728) as u32,
            },
            // End: HEE islands (biome 0..2 / variation id, absent = any), towers, DE comets (min size)
            "endIsland" => Target::EndIsland {
                biome: c["biome"].as_u64().map(|b| b.min(2) as u8),
                variation: c["variation"].as_u64().map(|v| v.min(6) as u8),
            },
            "endTower" => Target::EndTower,
            "endComet" => Target::EndComet {
                min_size: c["min"].as_u64().unwrap_or(2).clamp(2, 9) as u8,
            },
            t => return Err(format!("unknown condition type {t:?}")),
        };
        conds.push(Cond::new(
            target,
            c["r"].as_i64().unwrap_or(1000).clamp(0, 100_000) as i32,
            c["not"].as_bool().unwrap_or(false),
        ));
    }
    // relations: `"from": k` measures this condition from each candidate of condition k (one level only)
    let raw = v["conds"].as_array().map(|a| a.as_slice()).unwrap_or(&[]);
    let from: Vec<Option<usize>> = raw
        .iter()
        .map(|c| c["from"].as_u64().map(|k| k as usize))
        .collect();
    for (i, f) in from.iter().enumerate() {
        let Some(k) = *f else { continue };
        if k == i || k >= conds.len() || from[k].is_some() {
            return Err(format!(
                "condition {} is measured from condition {}, which is itself relative or missing",
                i + 1,
                k + 1
            ));
        }
        conds[i].helper = true;
        conds[k].near.push(i);
    }
    Ok(Query {
        world_type: if v["worldType"].as_u64() == Some(0) {
            WorldType::Rwg2
        } else {
            WorldType::Rwg2Continents
        },
        land_at_spawn: v["landAtSpawn"].as_bool().unwrap_or(false),
        conds,
        step: v["step"].as_i64().unwrap_or(64).clamp(4, 512) as i32,
        center: match (&v["center"], v["center"].as_array()) {
            (_, Some(a)) if a.len() == 2 => Center::Point(
                a[0].as_i64().unwrap_or(0).clamp(-30_000_000, 30_000_000) as i32,
                a[1].as_i64().unwrap_or(0).clamp(-30_000_000, 30_000_000) as i32,
            ),
            (c, _) if c.as_str() == Some("spawn") => Center::Spawn,
            _ => Center::Origin,
        },
        dim: match v["dim"].as_i64() {
            Some(-1) => -1,
            Some(7) => 7,
            Some(1) => 1,
            _ => 0,
        },
    })
}

fn search_data() -> SearchData {
    SearchData {
        climate: Some(climate().clone()),
        veins: Some(veins().clone()),
        fluids: Some(fluids().clone()),
        spawn: SPAWN_FN,
        pop_gates: Some(pop_gates().clone()),
    }
}

/// Spawn estimator used for `"center":"spawn"`.
const SPAWN_FN: Option<rwg2_biomes::search::SpawnFn> = Some(spawn_xz);

fn spawn_xz(p: &BiomeProvider, h: &dyn Heights, seed: i64) -> (i32, i32) {
    let s = spawn_cached(p, h, seed);
    (s.x, s.z)
}

thread_local! {
    /// (seed, continental) -> spawn estimate (5-500 ms each: chunk generation along the walk).
    static SPAWNS: RefCell<std::collections::HashMap<(i64, bool), rwg2_biomes::spawn::SpawnEstimate>> = RefCell::new(std::collections::HashMap::new());
}

fn spawn_cached(
    p: &BiomeProvider,
    h: &dyn Heights,
    seed: i64,
) -> rwg2_biomes::spawn::SpawnEstimate {
    let key = (seed, p.is_continental());
    if let Some(s) = SPAWNS.with(|m| m.borrow().get(&key).cloned()) {
        return s;
    }
    let s = rwg2_biomes::spawn::estimate_spawn(p, h, seed);
    SPAWNS.with(|m| {
        let mut m = m.borrow_mut();
        if m.len() > 4096 {
            m.clear();
        }
        m.insert(key, s.clone());
    });
    s
}

/// One search hit; flat-area hits add `"flat": [yLo, yHi, pct, patchArea, flatArea, truncated]`.
fn hit_json(h: &rwg2_biomes::search::Hit) -> serde_json::Value {
    let mut v = serde_json::json!({ "cond": h.cond, "x": h.x, "z": h.z });
    if let Some(f) = h.flat {
        v["flat"] = serde_json::json!([
            f.y_lo,
            f.y_hi,
            f.pct,
            f.patch_area,
            f.flat_area,
            f.truncated as u8
        ]);
    }
    v
}

/// Input buffer = query JSON (`query_len` bytes) + `count` seeds (i64 LE). Returns ptr to a UTF-8 JSON result:
/// `{"checked":n,"matches":[{"seed":"..","center":[x,z],"hits":[{"cond":i,"x":..,"z":..}]}]}` (hits only for
/// positive conditions); length via `result_len()`. The parsed query is cached while the JSON text is unchanged.
#[no_mangle]
pub extern "C" fn search(query_len: u32, count: u32) -> *const u8 {
    let out = IN.with(|b| {
        let b = b.borrow();
        let ql = query_len as usize;
        let qtext = std::str::from_utf8(&b[..ql]).unwrap_or("");
        SEARCHER.with(|s| {
            let mut s = s.borrow_mut();
            if s.as_ref().map(|(t, _)| t.as_str()) != Some(qtext) {
                match parse_query(qtext) {
                    Ok(q) => *s = Some((qtext.to_string(), Searcher::new(q, table(), RwgConfig::default(), search_data()))),
                    Err(e) => return format!("{{\"error\":{}}}", serde_json::Value::String(e)),
                }
            }
            let searcher = &s.as_ref().unwrap().1;
            let mut matches = Vec::new();
            for i in 0..count as usize {
                let o = ql + i * 8;
                let Some(bytes) = b.get(o..o + 8) else { break };
                let seed = i64::from_le_bytes(bytes.try_into().unwrap());
                if let Some(m) = searcher.check_seed(seed, heights()) {
                    let hits: Vec<_> =
                        m.hits.iter().map(hit_json).collect();
                    matches.push(serde_json::json!({ "seed": seed.to_string(), "center": [m.center.0, m.center.1], "hits": hits }));
                }
            }
            serde_json::json!({ "checked": count, "matches": matches }).to_string()
        })
    });
    RESULT.with(|r| {
        *r.borrow_mut() = out;
        r.borrow().as_ptr()
    })
}

#[no_mangle]
pub extern "C" fn result_len() -> u32 {
    RESULT.with(|r| r.borrow().len() as u32)
}

// ------------------------------------------------------------------------------------------------ other dimensions

thread_local! {
    static TF: RefCell<Option<(i64, rwg2_biomes::twilight::TwilightForest)>> = const { RefCell::new(None) };
    static NETHER: RefCell<Option<(i64, rwg2_biomes::nether::NetherBiomes)>> = const { RefCell::new(None) };
    static DIM_OUT: RefCell<Vec<u16>> = const { RefCell::new(Vec::new()) };
    static DIM_LEGEND: RefCell<String> = const { RefCell::new(String::new()) };
}

fn with_tf<R>(seed: i64, f: impl FnOnce(&rwg2_biomes::twilight::TwilightForest) -> R) -> R {
    TF.with(|t| {
        let mut t = t.borrow_mut();
        if t.as_ref().map(|x| x.0) != Some(seed) {
            *t = Some((
                seed,
                rwg2_biomes::twilight::TwilightForest::new(rwg2_biomes::twilight::tf_seed(
                    seed, "",
                )),
            ));
        }
        f(&t.as_ref().unwrap().1)
    })
}

thread_local! {
    static END_T: RefCell<Option<(i64, std::rc::Rc<rwg2_biomes::end::EndTerrain>)>> = const { RefCell::new(None) };
}

fn end_terrain(seed: i64) -> std::rc::Rc<rwg2_biomes::end::EndTerrain> {
    END_T.with(|t| {
        let mut t = t.borrow_mut();
        if t.as_ref().map(|x| x.0) != Some(seed) {
            *t = Some((
                seed,
                std::rc::Rc::new(rwg2_biomes::end::EndTerrain::new(seed)),
            ));
        }
        t.as_ref().unwrap().1.clone()
    })
}

/// The End's main island lies within ±END_R chunks (density ≤ −100 + noise beyond; `tests/end.rs`).
const END_R: i32 = 24;

thread_local! {
    static END_ISLANDS: RefCell<std::collections::HashMap<(i64, i32, i32), std::rc::Rc<rwg2_biomes::end::IslandShape>>> =
        RefCell::new(std::collections::HashMap::new());
}

/// HEE island shape (cached per worker; ~0.15 s each in wasm).
type CompactMap =
    std::collections::HashMap<(i64, i32, i32), (u64, std::rc::Rc<rwg2_biomes::end::IslandCompact>)>;
thread_local! {
    /// Island columns shared by the map land (`island_shape_cached`) and the End ore / draconium model
    /// (`EndBlocks`): each island is generated once per worker (~55 ms native). LRU, ~1.4 MB each.
    static ISLAND_COMPACT: RefCell<(u64, CompactMap)> = RefCell::new((0, std::collections::HashMap::new()));
}

fn island_compact_cached(
    seed: i64,
    cx: i32,
    cz: i32,
) -> std::rc::Rc<rwg2_biomes::end::IslandCompact> {
    if let Some(c) = ISLAND_COMPACT.with(|m| {
        let mut m = m.borrow_mut();
        m.0 += 1;
        let t = m.0;
        m.1.get_mut(&(seed, cx, cz)).map(|e| {
            e.0 = t;
            e.1.clone()
        })
    }) {
        return c;
    }
    // terrain + caves + HEE ores (+ Homeland cellars): what GT's vein test sees; solid columns = terrain's
    let c = std::rc::Rc::new(rwg2_biomes::end_homeland::island_compact_interior(seed, cx, cz));
    ISLAND_COMPACT.with(|m| {
        let mut m = m.borrow_mut();
        if m.1.len() >= 32 {
            let old = *m.1.iter().min_by_key(|(_, v)| v.0).unwrap().0;
            m.1.remove(&old);
        }
        m.0 += 1;
        let t = m.0;
        m.1.insert((seed, cx, cz), (t, c.clone()));
    });
    c
}

fn island_shape_cached(seed: i64, cx: i32, cz: i32) -> std::rc::Rc<rwg2_biomes::end::IslandShape> {
    if let Some(s) = END_ISLANDS.with(|m| m.borrow().get(&(seed, cx, cz)).cloned()) {
        return s;
    }
    let s = std::rc::Rc::new(rwg2_biomes::end::shape_from_compact(
        seed,
        cx,
        cz,
        &island_compact_cached(seed, cx, cz),
    ));
    END_ISLANDS.with(|m| {
        let mut m = m.borrow_mut();
        if m.len() > 400 {
            m.clear();
        }
        m.insert((seed, cx, cz), s.clone());
    });
    s
}

type TowerMap =
    std::collections::HashMap<(i64, i32, i32, i32), std::rc::Rc<rwg2_biomes::endtower::Tower>>;
thread_local! {
    static END_TOWERS: RefCell<TowerMap> = RefCell::new(std::collections::HashMap::new());
}

/// HEE tower layout (cached per worker).
fn tower_cached(
    seed: i64,
    resets: i32,
    cx: i32,
    cz: i32,
) -> std::rc::Rc<rwg2_biomes::endtower::Tower> {
    if let Some(t) = END_TOWERS.with(|m| m.borrow().get(&(seed, resets, cx, cz)).cloned()) {
        return t;
    }
    let t = std::rc::Rc::new(rwg2_biomes::endtower::tower(seed, resets, cx, cz));
    END_TOWERS.with(|m| {
        let mut m = m.borrow_mut();
        if m.len() > 200 {
            m.clear();
        }
        m.insert((seed, resets, cx, cz), t.clone());
    });
    t
}

/// End dimension code from the viewer: `1 | resets << 8` (HEE End resets shift the terrain seed only).
fn end_resets(dim: i32) -> Option<i64> {
    (dim & 0xff == 1).then_some((dim >> 8) as i64)
}

/// End column: (top Y or -1, display id: 9 main island, 100 + HEE island biome, 0 void).
fn end_col(seed: i64, dim: i32, x: i32, z: i32) -> (i32, u8) {
    let c = dim_cols(seed, dim, x >> 4, z >> 4)[((z & 15) * 16 + (x & 15)) as usize];
    (c.0 as i32, c.1)
}

/// [`end_col`] for map tiles: at ≥ 16 blocks per sample HEE island land is skipped (each worker would build
/// every island's 13×13-chunk terrain; the island markers stay), the main island is still exact.
fn end_col_tile(seed: i64, dim: i32, x: i32, z: i32, step: i32) -> (i32, u8) {
    if step >= 16 && ((x >> 4).abs() > END_R || (z >> 4).abs() > END_R) {
        return (-1, 0);
    }
    end_col(seed, dim, x, z)
}

fn with_nether<R>(seed: i64, f: impl FnOnce(&rwg2_biomes::nether::NetherBiomes) -> R) -> R {
    NETHER.with(|t| {
        let mut t = t.borrow_mut();
        if t.as_ref().map(|x| x.0) != Some(seed) {
            *t = Some((seed, rwg2_biomes::nether::NetherBiomes::new(seed)));
        }
        f(&t.as_ref().unwrap().1)
    })
}

/// Biome tile for dimension `dim` (-1 Nether (BOP), 7 Twilight Forest): base biome ids, `w*h` u16 sampled every
/// `step` blocks (exact voronoi at step 1, the 1:4 layer when zoomed out).
#[no_mangle]
pub extern "C" fn render_dim(
    seed: i64,
    dim: i32,
    x0: i32,
    z0: i32,
    w: u32,
    h: u32,
    step: i32,
) -> *const u16 {
    let ids: Vec<i32> = if end_resets(dim).is_some() {
        // The End: 9 main island, 100 + biome on HEE islands, 0 = void
        let mut v = Vec::with_capacity((w * h) as usize);
        for j in 0..h as i32 {
            for i in 0..w as i32 {
                v.push(end_col_tile(seed, dim, x0 + i * step, z0 + j * step, step).1 as i32);
            }
        }
        v
    } else if step == 1 {
        match dim {
            7 => with_tf(seed, |t| t.biomes(x0, z0, w as i32, h as i32)),
            _ => with_nether(seed, |t| t.biomes(x0, z0, w as i32, h as i32)),
        }
    } else {
        // sample the 1:4 layer: one cell per 4 blocks
        let s4 = (step / 4).max(1);
        let (x4, z4) = (x0.div_euclid(4), z0.div_euclid(4));
        let (w4, h4) = ((w as i32 - 1) * s4 + 1, (h as i32 - 1) * s4 + 1);
        let q = match dim {
            7 => with_tf(seed, |t| t.biomes_quarter(x4, z4, w4, h4)),
            _ => with_nether(seed, |t| t.biomes_quarter(x4, z4, w4, h4)),
        };
        let mut v = Vec::with_capacity((w * h) as usize);
        for j in 0..h as i32 {
            for i in 0..w as i32 {
                v.push(q[((j * s4) * w4 + i * s4) as usize]);
            }
        }
        v
    };
    DIM_OUT.with(|o| {
        let mut o = o.borrow_mut();
        o.clear();
        o.extend(ids.iter().map(|&b| b.clamp(0, 0xfffe) as u16));
        o.as_ptr()
    })
}

/// Features of another dimension in a block rectangle (records like `features`): kind 50 = TF major feature
/// (x/z centre, y = feature id, a = structure 0/1, b = biome), kind 51 = Nether fortress (x/z = start crossing
/// centre, y = fortress minY, a = maxY, b = pieces | blaze spawners << 12 | wart rooms << 16 | start-crossing minY << 20; exact layout,
/// `rwg2_biomes::fortress`), kind 53 = blaze spawner (x, z, y = block Y, a/b = its fortress x/z), kind 54 = nether
/// wart patch (x0, z0, y, x1, z1; 2×5 blocks), with flag 2 also kind 52 = fortress piece footprint (x0, z0, x1, z1,
/// b = piece kind | bridge << 5 | minY << 8 | maxY << 16), kind 20 = GT ore vein (live vein list, attempt 0; a = vein | (attempt-1 vein + 1) << 8 | its minY
/// << 16, b = packed extents).
#[no_mangle]
pub extern "C" fn features_dim(
    seed: i64,
    dim: i32,
    x0: i32,
    z0: i32,
    x1: i32,
    z1: i32,
    flags: u32,
) -> *const i32 {
    let (cx0, cz0, cx1, cz1) = (x0 >> 4, z0 >> 4, x1 >> 4, z1 >> 4);
    let mut out = Vec::new();
    if dim == 1 {
        // HEE islands (kind 70: centre x/z, y = islandBottomY, a = biome | variation << 4 | deviation bits << 8,
        // b = start chunk x | z << 16 low bits) and towers (kind 71: centre x/z); flags >> 8 = dragon kills
        let kills = (flags >> 8) as i32 & 0xff;
        if flags & 1 != 0 {
            // islands whose centre (start chunk · 16 + 104) lies in the rectangle (no duplicates across cells)
            for i in rwg2_biomes::end::islands(seed, kills, cx0 - 7, cz0 - 7, cx1, cz1) {
                let (x, z) = (i.cx * 16 + 104, i.cz * 16 + 104);
                if x < x0 || x > x1 || z < z0 || z > z1 {
                    continue;
                }
                let bits = i.deviations.iter().fold(0, |b, &d| b | 1 << d);
                out.extend_from_slice(&[
                    70,
                    i.cx * 16 + 104,
                    i.cz * 16 + 104,
                    i.bottom_y,
                    i.biome as i32 | (i.variation as i32) << 4 | bits << 8,
                    0,
                ]);
            }
            // towers: kind 71 axis x/z, y = ground floor Y, a = floor 1..3 room kinds (4 bits each) | chest room << 12
            // | rotation << 16, b = top platform Y
            for (tx, tz) in rwg2_biomes::end::towers(seed, kills, cx0 - 2, cz0 - 2, cx1, cz1) {
                let t = tower_cached(seed, kills, tx, tz);
                let (x, z) = t.centre();
                if x < x0 || x > x1 || z < z0 || z > z1 {
                    continue;
                }
                let a = t.rooms[0] as i32
                    | (t.rooms[1] as i32) << 4
                    | (t.rooms[2] as i32) << 8
                    | (t.chest_room as i32) << 12
                    | t.mode << 16;
                let gy = rwg2_biomes::endtower::Tower::world_y(t.island_y);
                out.extend_from_slice(&[71, x, z, gy, a, gy + 24]);
            }
        }
        if flags & (16 | 64) != 0 {
            // GT veins (kind 20, same record as the other dims): only where a window chunk processes ores (main
            // island zone, HEE islands, chaos islands); vein Y from the processing chunk's column (`gtend`).
            // a bit 24 = the window corners that process ores disagree (load-order dependent), alt = the other.
            use rwg2_biomes::gtend::*;
            use rwg2_biomes::gtores::{is_oreseed, GT_GTNH};
            let table = dim_veins(1);
            let zones = OreZones::new(seed, cx0 - 2, cz0 - 2, cx1 + 2, cz1 + 2);
            with_end_blocks(seed, kills, |w| {
                for cz in cz0..=cz1 {
                    for cx in cx0..=cx1 {
                        if !is_oreseed(&GT_GTNH, cx, cz) {
                            continue;
                        }
                        let Some((px, pz)) =
                            processing_chunk_end_with(cx, cz, |x, z| zones.processes(x, z))
                        else {
                            continue;
                        };
                        let pl = resolve_end_at(seed, table, &GT_GTNH, cx, cz, px, pz, w);
                        let mut others: Vec<Option<rwg2_biomes::gtores::Placement>> = Vec::new();
                        for (qx, qz) in [
                            (cx - 2, cz - 2),
                            (cx + 2, cz - 2),
                            (cx - 2, cz + 2),
                            (cx + 2, cz + 2),
                        ] {
                            if (qx, qz) != (px, pz) && zones.processes(qx, qz) {
                                others
                                    .push(resolve_end_at(seed, table, &GT_GTNH, cx, cz, qx, qz, w));
                            }
                        }
                        let key = |o: &Option<rwg2_biomes::gtores::Placement>| {
                            o.map(|p| p.vein as i32).unwrap_or(-1)
                        };
                        let differ: Vec<_> = others.iter().filter(|o| key(o) != key(&pl)).collect();
                        let Some(pl) = pl else {
                            if let Some(a) = differ.iter().find_map(|o| **o) {
                                // no vein from the likely side, but another side gives one: kind 21 + alternative
                                out.extend_from_slice(&[
                                    21,
                                    cx * 16 + 8,
                                    cz * 16 + 8,
                                    0,
                                    ((a.vein as i32 + 1) << 8) | ((a.min_y & 0xff) << 16) | 1 << 24,
                                    0,
                                ]);
                            }
                            continue;
                        };
                        let mut a = pl.vein as i32;
                        if let Some(o) = differ.first() {
                            a |= 1 << 24;
                            if let Some(q) = o {
                                a |= ((q.vein as i32 + 1) << 8) | ((q.min_y & 0xff) << 16);
                            }
                        }
                        let (sx, sz) = (cx * 16, cz * 16);
                        let packed = ((sx - pl.west) & 0xff)
                            | (((pl.east - sx - 16) & 0xff) << 8)
                            | (((sz - pl.north) & 0xff) << 16)
                            | (((pl.south - sz - 16) & 0x7f) << 24);
                        out.extend_from_slice(&[20, sx + 8, sz + 8, pl.min_y, a, packed]);
                    }
                }
            });
        }
        if flags & 64 != 0 && (cx1 - cx0 + 1) * (cz1 - cz0 + 1) <= 96 * 96 {
            // Draconic Evolution draconium (kind 74 per chunk with ore: centre x/z, y = count, a = minY | maxY << 8,
            // b = 1 exact / 0 a comet or chaos island drew first)
            let cfg = rwg2_biomes::draconium::DEFAULT_CFG;
            let tseed = seed.wrapping_add(kills as i64);
            with_end_blocks(seed, kills, |w| {
                let mut per: std::collections::BTreeMap<(i32, i32), (i32, i32, i32)> =
                    std::collections::BTreeMap::new();
                let mut inexact: Vec<(i32, i32)> = Vec::new();
                for cz in cz0 - 1..=cz1 {
                    for cx in cx0 - 1..=cx1 {
                        let o = rwg2_biomes::draconium::chunk_ore(tseed, &cfg, cx, cz, w);
                        if !o.exact {
                            inexact.push((cx, cz));
                        }
                        for (x, y, z) in o.blocks {
                            let e = per.entry((x >> 4, z >> 4)).or_insert((0, 255, 0));
                            e.0 += 1;
                            e.1 = e.1.min(y);
                            e.2 = e.2.max(y);
                        }
                    }
                }
                for ((cx, cz), (n, lo, hi)) in per {
                    if cx < cx0 || cx > cx1 || cz < cz0 || cz > cz1 {
                        continue;
                    }
                    let ex = !inexact
                        .iter()
                        .any(|&(ix, iz)| (ix == cx || ix == cx - 1) && (iz == cz || iz == cz - 1));
                    out.extend_from_slice(&[
                        74,
                        cx * 16 + 8,
                        cz * 16 + 8,
                        n,
                        lo | hi << 8,
                        ex as i32,
                    ]);
                }
            });
        }
        if flags & 4 != 0 {
            // Draconic Evolution: comets (kind 72: core x/z, y, a = size | tailY << 8, b = tail dx | dz << 16) on
            // Forge's per-chunk Random of the End's world seed (+ resets); chaos islands (kind 73) on the 10000 grid
            for c in
                rwg2_biomes::end::comets(seed.wrapping_add(kills as i64), 10000, cx0, cz0, cx1, cz1)
            {
                let (dx, dz) = (c.tail.0 - c.x, c.tail.2 - c.z);
                out.extend_from_slice(&[
                    72,
                    c.x,
                    c.z,
                    c.y,
                    c.size | c.tail.1 << 8,
                    (dx & 0xffff) | (dz & 0xffff) << 16,
                ]);
            }
            for (x, z) in rwg2_biomes::end::chaos_islands(10000, x0, z0, x1, z1) {
                out.extend_from_slice(&[73, x, z, 0, 0, 0]);
            }
        }
        return FEATURES.with(|f| {
            let mut f = f.borrow_mut();
            *f = out;
            f.as_ptr()
        });
    }
    if flags & 1 != 0 {
        if dim == 7 {
            for f in with_tf(seed, |t| t.features_in(cx0, cz0, cx1, cz1)) {
                if f.feature.id == 0 {
                    continue;
                }
                out.extend_from_slice(&[
                    50,
                    f.x,
                    f.z,
                    f.feature.id as i32,
                    f.feature.structure as i32,
                    f.biome,
                ]);
            }
        } else {
            for f in rwg2_biomes::nether::fortresses_in(seed, cx0, cz0, cx1, cz1) {
                let (x, z) = f.start_center();
                let Some(l) = rwg2_biomes::fortress::layout(seed, f.chunk_x, f.chunk_z) else {
                    continue;
                };
                let spawners = l.spawners();
                let warts = l.wart_rooms();
                out.extend_from_slice(&[
                    51,
                    x,
                    z,
                    l.bbox[1],
                    l.bbox[4],
                    l.pieces.len() as i32
                        | (spawners.len() as i32) << 12
                        | (warts.len() as i32) << 16
                        | (l.pieces[0].bbox[1] & 0xff) << 20,
                ]);
                if flags & 2 != 0 {
                    for p in &l.pieces {
                        let k = p.kind as i32
                            | (p.kind.is_bridge() as i32) << 5
                            | (p.bbox[1] & 0xff) << 8
                            | (p.bbox[4] & 0xff) << 16;
                        out.extend_from_slice(&[52, p.bbox[0], p.bbox[2], p.bbox[3], p.bbox[5], k]);
                    }
                }
                for (sx, sy, sz) in spawners {
                    out.extend_from_slice(&[53, sx, sz, sy, x, z]);
                }
                for w in warts {
                    for b in w.wart {
                        out.extend_from_slice(&[54, b[0], b[2], b[1], b[3], b[5]]);
                    }
                }
                // chests (`Corridor` / `Corridor2`) with their loot: 55 = [x, z, y, fortress x, fortress z], then one
                // 56 = [x, z, slot, item (`FORTRESS_LOOT` index), count] per filled slot
                for (cpos, slots) in l
                    .chests()
                    .into_iter()
                    .map(|c| rwg2_biomes::fortress::populating_chunk(c.0, c.2))
                    .collect::<std::collections::BTreeSet<_>>()
                    .into_iter()
                    .flat_map(|(pcx, pcz)| rwg2_biomes::fortress::chunk_chest_loot(seed, &[&l], pcx, pcz))
                {
                    out.extend_from_slice(&[55, cpos.0, cpos.2, cpos.1, x, z]);
                    for (sl, k, n) in slots {
                        out.extend_from_slice(&[56, cpos.0, cpos.2, sl as i32, k as i32, n as i32]);
                    }
                }
            }
        }
    }
    if flags & (16 | 64) != 0 {
        use rwg2_biomes::gtores::{is_oreseed, GT_GTNH};
        let table = dim_veins(dim);
        // flag 64: GT's placement test on the exact dimension terrain (`gtdims`, arrival at the origin;
        // ~1.5–2.5 s per 2048 cell native); flag 16 alone: attempt 0 (seed maths only)
        let tested = flags & 64 != 0;
        for cz in cz0..=cz1 {
            for cx in cx0..=cx1 {
                if !is_oreseed(&GT_GTNH, cx, cz) {
                    continue;
                }
                let (pl, alt) = if tested {
                    let Some(pl) = with_dim_stone(seed, dim, |s| {
                        rwg2_biomes::gtdims::resolve_dim(seed, dim, table, &GT_GTNH, cx, cz, s)
                    }) else {
                        continue;
                    };
                    let alt =
                        rwg2_biomes::gtdims::alternative_dim(table, seed, dim, cx, cz, Some(&pl));
                    (pl, alt)
                } else {
                    let c =
                        rwg2_biomes::nether::gt_candidates(seed, dim, table, &GT_GTNH, cx, cz, 2);
                    let Some(pl) = c.first().copied() else {
                        continue;
                    };
                    (pl, c.get(1).copied().filter(|a| a.vein != pl.vein))
                };
                let pl = &pl;
                let alt = alt.map_or(0, |a| ((a.vein as i32 + 1) << 8) | ((a.min_y & 0xff) << 16));
                let (sx, sz) = (cx * 16, cz * 16);
                let packed = ((sx - pl.west) & 0xff)
                    | (((pl.east - sx - 16) & 0xff) << 8)
                    | (((sz - pl.north) & 0xff) << 16)
                    | (((pl.south - sz - 16) & 0x7f) << 24);
                out.extend_from_slice(&[
                    20,
                    sx + 8,
                    sz + 8,
                    pl.min_y,
                    pl.vein as i32 | alt,
                    packed,
                ]);
            }
        }
    }
    FEATURES.with(|f| {
        let mut f = f.borrow_mut();
        *f = out;
        f.as_ptr()
    })
}

thread_local! {
    /// Chunk-cached GT stone of the last (seed, dim) asked for (the ore placement test in dims −1 / 7).
    static DIM_STONE: std::cell::RefCell<Option<(i64, i32, rwg2_biomes::gtdims::DimStone)>> =
        const { std::cell::RefCell::new(None) };
}

fn with_dim_stone<R>(
    seed: i64,
    dim: i32,
    f: impl FnOnce(&mut rwg2_biomes::gtdims::DimStone) -> R,
) -> R {
    DIM_STONE.with(|c| {
        let mut c = c.borrow_mut();
        if !matches!(&*c, Some((s, d, _)) if *s == seed && *d == dim) {
            let mut st = rwg2_biomes::gtdims::DimStone::new(dim, seed).expect("dim -1 / 7");
            st.max_cached = 1024; // 8 KB per chunk
            *c = Some((seed, dim, st));
        }
        f(&mut c.as_mut().unwrap().2)
    })
}

thread_local! {
    /// End blocks for the GT placement test of the last (seed, resets) asked for.
    static END_BLOCKS: std::cell::RefCell<Option<(i64, i32, rwg2_biomes::gtend::EndBlocks)>> =
        const { std::cell::RefCell::new(None) };
}

fn with_end_blocks<R>(
    seed: i64,
    resets: i32,
    f: impl FnOnce(&mut rwg2_biomes::gtend::EndBlocks) -> R,
) -> R {
    END_BLOCKS.with(|c| {
        let mut c = c.borrow_mut();
        if !matches!(&*c, Some((s, r, _)) if *s == seed && *r == resets) {
            *c = Some((
                seed,
                resets,
                rwg2_biomes::gtend::EndBlocks::with_source(
                    seed,
                    resets,
                    Box::new(move |x, z| island_compact_cached(seed, x, z)),
                ),
            ));
        }
        f(&mut c.as_mut().unwrap().2)
    })
}

static DIM_VEINS: std::sync::OnceLock<(
    rwg2_biomes::gtores::VeinTable,
    rwg2_biomes::gtores::VeinTable,
    rwg2_biomes::gtores::VeinTable,
)> = std::sync::OnceLock::new();

fn dim_veins(dim: i32) -> &'static rwg2_biomes::gtores::VeinTable {
    let t = DIM_VEINS.get_or_init(|| {
        (
            rwg2_biomes::nether::vein_table(&rwg2_biomes::nether::GT_NETHER_VEINS),
            rwg2_biomes::nether::vein_table(&rwg2_biomes::nether::GT_TF_VEINS),
            rwg2_biomes::nether::vein_table(&rwg2_biomes::gtend::GT_END_VEINS),
        )
    });
    match dim {
        7 => &t.1,
        1 => &t.2,
        _ => &t.0,
    }
}

thread_local! {
    static BLOCK_OUT: RefCell<[i32; 8]> = const { RefCell::new([0; 8]) };
}

/// One block of the exact pre-population overworld column at (x, y, z), as the street view draws it (surface pass,
/// river tunnels, lava landmark; no caves): 8 i32 `[block code (0 air), top y, chunk biome gen id, rwg id,
/// height (floor of testHeight), block below, block above, 1 = column found]`.
#[no_mangle]
pub extern "C" fn surface_block(handle: u32, x: i32, y: i32, z: i32) -> *const i32 {
    let mut out = [0i32; 8];
    PROVIDERS.with(|ps| {
        let ps = ps.borrow();
        let Some(Some(p)) = ps.get(handle as usize) else {
            return;
        };
        if p.table.heights.is_empty() {
            return;
        }
        let hs = heights();
        with_gen(handle, p, |g| {
            let (cx, cz) = (x >> 4, z >> 4);
            let Some(c) = g.generate(p, hs, cx, cz) else {
                return;
            };
            let sc = surface_of(p, hs, &c, cx, cz, None);
            let idx = ((z & 15) * 16 + (x & 15)) as usize;
            let col = &sc.cols[idx];
            let at = |y: i32| {
                if (0..256).contains(&y) {
                    col[y as usize] as i32
                } else {
                    0
                }
            };
            // chunk biome array = what F3 shows (lava-cave marker included)
            let gen = c.gen[idx];
            out = [
                at(y),
                sc.top[idx] as i32,
                gen as i32,
                c.rwg[idx] as i32,
                c.height[idx].floor() as i32,
                at(y - 1),
                at(y + 1),
                1,
            ];
        });
    });
    BLOCK_OUT.with(|o| {
        *o.borrow_mut() = out;
        o.as_ptr() as *const i32
    })
}

thread_local! {
    static DIM_BLOCK: RefCell<String> = const { RefCell::new(String::new()) };
}

/// Block at (x, y, z) in the Nether (-1), Twilight Forest (7) or End (1 | resets << 8), before population:
/// JSON `{"name","above","below","exact"}` (the End only knows end stone / island top / tower obsidian:
/// `exact` false). Length via `dim_block_len`.
#[no_mangle]
pub extern "C" fn dim_block(seed: i64, dim: i32, x: i32, y: i32, z: i32) -> *const u8 {
    let (cx, cz) = (x >> 4, z >> 4);
    let v = match dim & 0xff {
        255 => {
            let c = nether_terrain(seed).chunk(cx, cz);
            let n = |y: i32| rwg2_biomes::dimterrain::nb::name(c.at(x & 15, y, z & 15));
            serde_json::json!({"name": n(y), "above": n(y + 1), "below": n(y - 1), "exact": true})
        }
        7 => {
            let c = tf_terrain(seed).chunk(cx, cz);
            let n = |y: i32| rwg2_biomes::dimterrain::tb::name(c.at(x & 15, y, z & 15));
            serde_json::json!({"name": n(y), "above": n(y + 1), "below": n(y - 1), "exact": true})
        }
        1 => with_end_blocks(seed, dim >> 8, |e| {
            let (solid, stone) = e.col(x, z);
            let bit = |m: [u128; 2], y: i32| {
                (0..256).contains(&y) && m[(y >> 7) as usize] >> (y & 127) & 1 != 0
            };
            let n = |y: i32| {
                if !bit(solid, y) {
                    "minecraft:air"
                } else if bit(stone, y) {
                    "minecraft:end_stone"
                } else if !bit(solid, y + 1) {
                    "HardcoreEnderExpansion:end_stone_terrain"
                } else {
                    "minecraft:obsidian"
                }
            };
            serde_json::json!({"name": n(y), "above": n(y + 1), "below": n(y - 1), "exact": false})
        }),
        _ => serde_json::json!({"name": null}),
    };
    DIM_BLOCK.with(|o| {
        *o.borrow_mut() = v.to_string();
        o.borrow().as_ptr()
    })
}

#[no_mangle]
pub extern "C" fn dim_block_len() -> u32 {
    DIM_BLOCK.with(|o| o.borrow().len() as u32)
}

thread_local! {
    static BLOCKS_JSON: RefCell<String> = const { RefCell::new(String::new()) };
}

/// Surface block codes of map mode 3 / the street view: `[{id, name, label, rgb}]` (JSON, length via
/// `blocks_json_len`).
#[no_mangle]
pub extern "C" fn blocks_json() -> *const u8 {
    use rwg2_biomes::surface::{blk, block_color, block_label, block_name};
    let v: Vec<serde_json::Value> = (0..blk::COUNT)
        .chain(blk::CLAY0..blk::CLAY0 + 16)
        .map(|b| serde_json::json!({"id": b, "name": block_name(b), "label": block_label(b), "rgb": block_color(b)}))
        .collect();
    BLOCKS_JSON.with(|o| {
        *o.borrow_mut() = serde_json::Value::Array(v).to_string();
        o.borrow().as_ptr()
    })
}

#[no_mangle]
pub extern "C" fn blocks_json_len() -> u32 {
    BLOCKS_JSON.with(|o| o.borrow().len() as u32)
}

/// JSON legend for another dimension: `{"biomes":[{id,name,rgb}], "features":[{id,name,structure}], "veins":[{name,
/// minY,maxY,weight,ores:[..]}]}` (veins: live list, = rwg2dump 2.8).
#[no_mangle]
pub extern "C" fn dim_legend(dim: i32) -> *const u8 {
    if dim == 1 {
        let vars: Vec<serde_json::Value> = (0..3)
            .map(|b| {
                serde_json::json!({
                    "name": rwg2_biomes::end::ISLAND_BIOMES[b],
                    "variations": rwg2_biomes::end::ISLAND_VARIATIONS[b].iter().map(|v| serde_json::json!({"id": v.0, "name": v.1, "weight": v.2})).collect::<Vec<_>>(),
                    "deviations": rwg2_biomes::end::ISLAND_DEVIATIONS[b].iter().map(|d| d.0).collect::<Vec<_>>(),
                })
            })
            .collect();
        let s = serde_json::json!({
            "dim": 1,
            "biomes": [{"id": 9, "name": "End stone (main island)", "rgb": 0xdbd99c}, {"id": 100, "name": "Infested Forest island", "rgb": 0x6f9a3c}, {"id": 101, "name": "Burning Mountains island", "rgb": 0xb4471f}, {"id": 102, "name": "Enchanted Island", "rgb": 0x9b5cc4}, {"id": 110, "name": "HEE dungeon tower (3D view)", "rgb": 0x3a2d58}, {"id": 111, "name": "Tower island (3D view)", "rgb": 0xdbd99c}, {"id": 0, "name": "Void", "rgb": 0x0d0b14}],
            "features": [], "islands": vars,
            "veins": dim_veins(1).overworld.iter().map(|v| serde_json::json!({ "name": v.name, "minY": v.min_y, "maxY": v.max_y, "weight": v.weight, "ores": v.ores })).collect::<Vec<_>>(),
        })
        .to_string();
        return DIM_LEGEND.with(|l| {
            *l.borrow_mut() = s;
            l.borrow().as_ptr()
        });
    }
    let biomes: Vec<serde_json::Value> = if dim == 7 {
        let ids = rwg2_biomes::twilight::TfBiomeIds::GTNH_290;
        ids.names()
            .iter()
            .map(|&(id, name)| serde_json::json!({ "id": id, "name": name, "rgb": ids.color(id) }))
            .collect()
    } else {
        // display colours (0xRRGGBB); Phantasmagoric Inferno = BOP's own colour
        let colour = |id: i32| match id {
            8 => 0x7a2020,
            136 => 0x8a7445,
            137 => 0xd96616,
            128 => 0xd8d0b0,
            117 => 0xa02c48,
            138 => 0x55703a,
            94 => 0x9cc8e0,
            _ => 0x666666,
        };
        rwg2_biomes::nether::BOP_NETHER_BIOMES_GTNH
            .iter()
            .map(|&(id, w, name)| {
                let rgb = colour(id);
                serde_json::json!({ "id": id, "name": name, "weight": w, "rgb": rgb })
            })
            .collect()
    };
    let features: Vec<serde_json::Value> = rwg2_biomes::twilight::TF_FEATURES
        .iter()
        .map(|f| serde_json::json!({ "id": f.id, "name": f.name, "structure": f.structure }))
        .collect();
    let veins: Vec<serde_json::Value> = dim_veins(dim)
        .overworld
        .iter()
        .map(|v| serde_json::json!({ "name": v.name, "minY": v.min_y, "maxY": v.max_y, "weight": v.weight, "ores": v.ores }))
        .collect();
    let s = serde_json::json!({ "dim": dim, "biomes": biomes, "features": if dim == 7 { features } else { vec![] }, "veins": veins }).to_string();
    DIM_LEGEND.with(|l| {
        *l.borrow_mut() = s;
        l.borrow().as_ptr()
    })
}

#[no_mangle]
pub extern "C" fn dim_legend_len() -> u32 {
    DIM_LEGEND.with(|l| l.borrow().len() as u32)
}

// ------------------------------------------------------------------------------------------------ Thaumcraft nodes

thread_local! {
    static TC_JSON: RefCell<String> = const { RefCell::new(String::new()) };
}

fn nodes_json(nodes: &[rwg2_biomes::thaumcraft::TcNode]) -> String {
    use rwg2_biomes::thaumcraft::TcKind;
    let list: Vec<serde_json::Value> = nodes
            .iter()
            .map(|n| {
                let k = match n.kind {
                    TcKind::WildNode => "wild",
                    TcKind::Ring => "ring",
                    TcKind::Totem => "totem",
                    TcKind::Hilltop => "hilltop",
                    TcKind::Mound => "mound",
                    TcKind::Silverwood => "silverwood",
                };
                let info = n.info.as_ref();
                serde_json::json!({
                    "k": k, "x": n.x, "y": n.y, "z": n.z, "yExact": n.y_exact, "valid": n.valid,
                    "type": info.map(|i| format!("{:?}", i.node_type)),
                    "mod": info.and_then(|i| i.modifier.map(|m| format!("{m:?}"))),
                    "exact": info.map(|i| i.exact), "envCertain": info.map(|i| i.env_certain),
                    "aspects": info.map(|i| i.aspects().iter().map(|(a, c)| serde_json::json!([tc_tag(*a), c])).collect::<Vec<_>>()),
                })
            })
            .collect();
    serde_json::Value::Array(list).to_string()
}

fn tc_tag(a: rwg2_biomes::tc_nodes::Aspect) -> String {
    rwg2_biomes::tc_nodes::TC_ASPECTS_GTNH.tag(a)
}

/// Aura nodes in a block rectangle as JSON: `[{k: wild|ring|totem|hilltop|mound|silverwood, x, y, z, yExact, valid,
/// type, mod, exact, envCertain, aspects: [[tag, n]...]}]` (`type`/`aspects` null where the contents are not
/// predictable). Length via `tc_json_len()`.
#[no_mangle]
pub extern "C" fn tc_nodes_json(handle: u32, x0: i32, z0: i32, x1: i32, z1: i32) -> *const u8 {
    use rwg2_biomes::thaumcraft::{TcGen, TC_GTNH};
    let s = PROVIDERS.with(|ps| {
        let ps = ps.borrow();
        let Some(Some(p)) = ps.get(handle as usize) else {
            return "[]".to_string();
        };
        let hs = heights();
        let mut t = rwg2_biomes::dungeons::Terrain::new(p, hs);
        let mut g = TcGen {
            t: &mut t,
            climate: climate(),
            cfg: TC_GTNH,
        };
        let mut nodes = Vec::new();
        // same output as g.nodes per chunk; the rect's possible biomes prune the no-terrain pre-check
        g.nodes_in_rect(p.seed(), x0 >> 4, z0 >> 4, x1 >> 4, z1 >> 4, &mut nodes);
        nodes_json(&nodes)
    });
    TC_JSON.with(|j| {
        *j.borrow_mut() = s;
        j.borrow().as_ptr()
    })
}

#[no_mangle]
pub extern "C" fn tc_json_len() -> u32 {
    TC_JSON.with(|j| j.borrow().len() as u32)
}

/// All aspect tags (index = aspect id), for the search UI.
#[no_mangle]
pub extern "C" fn tc_aspect_tags() -> *const u8 {
    let tags: Vec<String> = (0..69u8)
        .map(|i| tc_tag(rwg2_biomes::tc_nodes::Aspect(i)))
        .collect();
    TC_JSON.with(|j| {
        *j.borrow_mut() = serde_json::json!(tags).to_string();
        j.borrow().as_ptr()
    })
}

// ------------------------------------------------------------------------------------------------ dimension terrain

type DimTerrains = (
    Option<(i64, std::rc::Rc<rwg2_biomes::dimterrain::NetherTerrain>)>,
    Option<(i64, std::rc::Rc<rwg2_biomes::dimterrain::TfTerrain>)>,
);
type DimCols = rwg2_biomes::jmath::FxMap<(i64, i32, i32, i32), std::rc::Rc<[(i16, u8); 256]>>;

thread_local! {
    static DIM_TERRAIN: RefCell<DimTerrains> = const { RefCell::new((None, None)) };
    /// exact chunk columns: (ground or floor y, flags: 1 = lava floor) per column lz*16+lx
    static DIM_COLS: RefCell<DimCols> = RefCell::new(Default::default());
    static DIM_INSPECT: RefCell<[f64; 4]> = const { RefCell::new([0.0; 4]) };
}

fn nether_terrain(seed: i64) -> std::rc::Rc<rwg2_biomes::dimterrain::NetherTerrain> {
    DIM_TERRAIN.with(|t| {
        let mut t = t.borrow_mut();
        if t.0.as_ref().map(|x| x.0) != Some(seed) {
            t.0 = Some((
                seed,
                std::rc::Rc::new(rwg2_biomes::dimterrain::NetherTerrain::new(seed)),
            ));
        }
        t.0.as_ref().unwrap().1.clone()
    })
}

fn tf_terrain(seed: i64) -> std::rc::Rc<rwg2_biomes::dimterrain::TfTerrain> {
    DIM_TERRAIN.with(|t| {
        let mut t = t.borrow_mut();
        if t.1.as_ref().map(|x| x.0) != Some(seed) {
            let s = rwg2_biomes::twilight::tf_seed(seed, "");
            t.1 = Some((
                seed,
                std::rc::Rc::new(rwg2_biomes::dimterrain::TfTerrain::new(s)),
            ));
        }
        t.1.as_ref().unwrap().1.clone()
    })
}

/// Exact columns of one chunk (provideChunk = before population): TF ground (no water, no canopy), Nether floor
/// of the highest open space below y 110.
fn dim_cols(seed: i64, dim: i32, cx: i32, cz: i32) -> std::rc::Rc<[(i16, u8); 256]> {
    if let Some(c) = DIM_COLS.with(|m| m.borrow().get(&(seed, dim, cx, cz)).cloned()) {
        return c;
    }
    let mut out = [(0i16, 0u8); 256];
    if let Some(resets) = end_resets(dim) {
        // main island: terrain seed = world seed + resets; HEE islands: raw world seed
        out = [(-1i16, 0u8); 256];
        if cx.abs() <= END_R && cz.abs() <= END_R {
            let c = end_terrain(seed.wrapping_add(resets)).chunk(cx, cz);
            for lz in 0..16 {
                for lx in 0..16 {
                    let t = c.top(lx, lz);
                    out[(lz * 16 + lx) as usize] = (t as i16, if t >= 0 { 9 } else { 0 });
                }
            }
        } else if let Some(i) = rwg2_biomes::end::island_covering(seed, cx, cz) {
            let sh = island_shape_cached(seed, i.cx, i.cz);
            for lz in 0..16 {
                for lx in 0..16 {
                    let t = sh.top_at(cx * 16 + lx, cz * 16 + lz);
                    out[(lz * 16 + lx) as usize] =
                        (t as i16, if t >= 0 { 100 + i.biome } else { 0 });
                }
            }
        }
    } else if dim == 7 {
        let c = tf_terrain(seed).chunk(cx, cz);
        for lz in 0..16 {
            for lx in 0..16 {
                out[(lz * 16 + lx) as usize] = (c.ground(lx, lz) as i16, 0);
            }
        }
    } else {
        let c = nether_terrain(seed).chunk(cx, cz);
        for lz in 0..16 {
            for lx in 0..16 {
                let (mut y, lava) = c.floor(lx, lz);
                if y <= 4 && !lava {
                    y = 110; // solid from 110 down: a pillar (same rule as the zoomed-out and Blocks paths)
                }
                out[(lz * 16 + lx) as usize] = (y as i16, lava as u8);
            }
        }
    }
    let rc = std::rc::Rc::new(out);
    DIM_COLS.with(|m| {
        let mut m = m.borrow_mut();
        if m.len() > 3000 {
            m.clear();
        }
        m.insert((seed, dim, cx, cz), rc.clone());
    });
    rc
}

/// Height code for the viewer's terrain ramp (height · 8, sea level at 63): TF ground + 32 (TF sea level 31);
/// Nether floor + 31, lava ocean drawn as deep water (code 40 · 8).
fn dim_height_code(dim: i32, y: i32, lava: bool) -> u16 {
    let h = if dim == 1 {
        if y < 0 {
            return 0;
        }
        y
    } else if dim == 7 {
        y + 32
    } else if lava {
        40
    } else {
        y + 31
    };
    (h.clamp(0, 8191) * 8) as u16
}

/// `render_dim` with a mode: 0 biomes (as `render_dim`), 2 terrain height codes (exact chunks at step 1; the
/// lattice approximation (±5 blocks) on a coarser grid when zoomed out).
#[no_mangle]
pub extern "C" fn render_dim_mode(
    seed: i64,
    dim: i32,
    x0: i32,
    z0: i32,
    w: u32,
    h: u32,
    step: i32,
    mode: u32,
) -> *const u16 {
    if mode != 2 {
        return render_dim(seed, dim, x0, z0, w, h, step);
    }
    let (w, h) = (w as i32, h as i32);
    let mut v = vec![0u16; (w * h) as usize];
    if end_resets(dim).is_some() {
        for j in 0..h {
            for i in 0..w {
                v[(j * w + i) as usize] = dim_height_code(
                    1,
                    end_col_tile(seed, dim, x0 + i * step, z0 + j * step, step).0,
                    false,
                );
            }
        }
    } else if step == 1 {
        for j in 0..h {
            for i in 0..w {
                let (x, z) = (x0 + i, z0 + j);
                let c = dim_cols(seed, dim, x >> 4, z >> 4);
                let (y, f) = c[((z & 15) * 16 + (x & 15)) as usize];
                v[(j * w + i) as usize] = dim_height_code(dim, y as i32, f & 1 != 0);
            }
        }
    } else {
        // ≤ 128² samples at step 2, 64² beyond
        let f = if step == 2 { 2 } else { 4 };
        let (nt, tt) = if dim == 7 {
            (None, Some(tf_terrain(seed)))
        } else {
            (Some(nether_terrain(seed)), None)
        };
        let (nx, nz) = ((w + f - 1) / f, (h + f - 1) / f);
        // Nether: one lattice density field for the whole tile (neighbouring samples share columns)
        let grid = nt
            .as_ref()
            .map(|n| n.approx_floor_grid(x0 + f / 2 * step, z0 + f / 2 * step, nx, nz, f * step));
        let mut j = 0;
        while j < h {
            let mut i = 0;
            while i < w {
                let (x, z) = (x0 + (i + f / 2) * step, z0 + (j + f / 2) * step);
                let code = match (&grid, &tt) {
                    (_, Some(t)) => dim_height_code(dim, t.approx_ground(x, z), false),
                    (Some(g), _) => {
                        let (mut y, lava) = g[((j / f) * nx + i / f) as usize];
                        if y <= 4 && !lava {
                            y = 110;
                        }
                        dim_height_code(dim, y, lava)
                    }
                    _ => 0,
                };
                for jj in j..(j + f).min(h) {
                    for ii in i..(i + f).min(w) {
                        v[(jj * w + ii) as usize] = code;
                    }
                }
                i += f;
            }
            j += f;
        }
    }
    DIM_OUT.with(|o| {
        let mut o = o.borrow_mut();
        *o = v;
        o.as_ptr()
    })
}

/// `[biome, ground/floor y, highest block y (TF incl. water and canopy; Nether 127 = roof), lava floor 0/1]` from the
/// exact chunk.
#[no_mangle]
pub extern "C" fn inspect_dim(seed: i64, dim: i32, x: i32, z: i32) -> *const f64 {
    let r = if end_resets(dim).is_some() {
        let (y, id) = end_col(seed, dim, x, z);
        [id as f64, y as f64, y as f64, 0.0]
    } else if dim == 7 {
        let t = tf_terrain(seed);
        let c = t.chunk(x >> 4, z >> 4);
        [
            t.layers.biome_at(x, z) as f64,
            c.ground(x, z) as f64,
            c.top(x, z) as f64,
            0.0,
        ]
    } else {
        let n = nether_terrain(seed);
        let c = n.chunk(x >> 4, z >> 4);
        let (y, lava) = c.floor(x, z);
        [n.biome_at(x, z) as f64, y as f64, 127.0, lava as u8 as f64]
    };
    DIM_INSPECT.with(|d| {
        *d.borrow_mut() = r;
        d.as_ptr() as *const f64
    })
}

fn pop_gates() -> &'static rwg2_biomes::population::PreGates {
    static G: std::sync::OnceLock<rwg2_biomes::population::PreGates> = std::sync::OnceLock::new();
    G.get_or_init(|| {
        let v: serde_json::Value =
            serde_json::from_str(CLIMATE_JSON).expect("embedded climate.json");
        rwg2_biomes::population::PreGates::new(v["biomes"].as_array().unwrap().iter().map(|b| {
            (
                b["id"].as_u64().unwrap() as u16,
                b["name"].as_str().unwrap_or(""),
                b["dict"]
                    .as_array()
                    .map(|d| d.iter().filter_map(|x| x.as_str()).collect())
                    .unwrap_or_default(),
            )
        }))
    })
}

thread_local! {
    static POP_BUF: RefCell<Vec<u8>> = const { RefCell::new(Vec::new()) };
}

/// X-ray population of overworld chunk `(cx, cz)`: RWG2 populate prefix (lakes, dungeon rooms, geodes, dirt/gravel
/// pockets, emeralds) as one byte per block, `(y · 16 + lz) · 16 + lx`, y < 192: 0 unchanged, 1 water, 2 lava,
/// 3 air, 4 dirt, 5 gravel, 6 abyssal stone, 7 cobblestone, 8 emerald ore. Pointer to 49,152 bytes.
#[no_mangle]
pub extern "C" fn xray_pop(handle: u32, cx: i32, cz: i32) -> *const u8 {
    let v = PROVIDERS.with(|ps| {
        let ps = ps.borrow();
        let Some(Some(p)) = ps.get(handle as usize) else {
            return vec![0u8; 16 * 16 * 192];
        };
        let mut m = rwg2_biomes::population::PopModel::new(p, heights(), p.seed());
        m.gates = pop_gates().clone();
        m.load_structures(cx - 1, cz - 1, cx, cz);
        m.chunk_codes(cx, cz, 192)
    });
    POP_BUF.with(|b| {
        *b.borrow_mut() = v;
        b.borrow().as_ptr()
    })
}

thread_local! {
    static DECO_BUF: RefCell<Vec<u8>> = const { RefCell::new(Vec::new()) };
}

/// Decoration (trees, plants, boulders, canopy snow) of overworld chunk `(cx, cz)`: `[y0 i16 LE, h u16 LE]` then
/// `h · 256` palette codes, index `((y - y0) · 16 + lz) · 16 + lx` (`deco::view::PALETTE`, see `deco_palette`).
/// Length via `deco_len`.
#[no_mangle]
pub extern "C" fn deco_chunk(handle: u32, cx: i32, cz: i32) -> *const u8 {
    let v = PROVIDERS.with(|ps| {
        let ps = ps.borrow();
        let Some(Some(p)) = ps.get(handle as usize) else {
            return vec![0u8; 4];
        };
        let (y0, h, codes) =
            rwg2_biomes::deco::view::chunk_voxels(p, heights(), pop_gates(), cx, cz);
        let mut out = Vec::with_capacity(4 + codes.len());
        out.extend_from_slice(&(y0 as i16).to_le_bytes());
        out.extend_from_slice(&(h as u16).to_le_bytes());
        out.extend_from_slice(&codes);
        out
    });
    DECO_BUF.with(|b| {
        *b.borrow_mut() = v;
        b.borrow().as_ptr()
    })
}

thread_local! {
    /// decoration replays for `ring_deco` (per provider handle)
    static RING_OV: RefCell<Option<(u32, i64, bool, rwg2_biomes::deco::view::DecoOverlay)>> = const { RefCell::new(None) };
}

/// Thaumcraft eldritch ring at (x, z) (altar position): `WorldGenEldritchRing.LocationIsValidSpawn` on the decorated
/// columns (canopies, plants). 1 pass, 0 fail, -1 no provider. The map applies it as a Bayes update
/// (`RING_DECO_LR`) to rings still shown after its maze pass.
#[no_mangle]
pub extern "C" fn ring_deco(handle: u32, x: i32, z: i32) -> i32 {
    PROVIDERS.with(|ps| {
        let ps = ps.borrow();
        let Some(Some(p)) = ps.get(handle as usize) else {
            return -1;
        };
        let mut t = Terrain::new(p, heights());
        RING_OV.with(|o| {
            let mut o = o.borrow_mut();
            let key = (handle, p.seed(), p.is_continental());
            if o.as_ref().map_or(true, |e| (e.0, e.1, e.2) != key) {
                let ov = rwg2_biomes::deco::view::DecoOverlay::new(pop_gates().clone(), 1);
                *o = Some((key.0, key.1, key.2, ov));
            }
            let ov = &mut o.as_mut().unwrap().3;
            rwg2_biomes::thaumcraft::ring_deco_ok(&mut t, ov, x, z) as i32
        })
    })
}

/// Top view of the decoration of `n × n` overworld chunks from `(cx0, cz0)` (Blocks map overlay): per column,
/// z-major over `16n × 16n`, the palette code of the highest visible decoration block, then (second half) its y.
/// Length via `deco_len`.
#[no_mangle]
pub extern "C" fn deco_tops(handle: u32, cx0: i32, cz0: i32, n: i32) -> *const u8 {
    let n = n.clamp(1, 8);
    let side = (16 * n * 16 * n) as usize;
    let v = PROVIDERS.with(|ps| {
        let ps = ps.borrow();
        let Some(Some(p)) = ps.get(handle as usize) else {
            return vec![0u8; 2 * side];
        };
        let tops = rwg2_biomes::deco::view::region_tops(p, heights(), pop_gates(), cx0, cz0, n);
        let mut out = vec![0u8; 2 * side];
        for (i, (c, y)) in tops.into_iter().enumerate() {
            out[i] = c;
            out[side + i] = y;
        }
        out
    });
    DECO_BUF.with(|b| {
        *b.borrow_mut() = v;
        b.borrow().as_ptr()
    })
}

/// Decoration tops of a fresh world's spawn area in the real load order (spawn walk loads, then
/// `MinecraftServer.initialWorldChunkLoad`, as [`spawn_ores`]): 16-byte header (i32 cx0, cz0, n, populates),
/// then codes and ys as [`deco_tops`] over `16n × 16n` columns from chunk (cx0, cz0): the chunks of the populated
/// square whose writers all ran (its first row / column excluded). Length via `deco_len`. ~4 s native.
/// Fresh-world spawn area load order (spawn walk loads, then `MinecraftServer.initialWorldChunkLoad`, as
/// [`spawn_ores`]) and the complete square of it: chunks of the populated square whose writers all ran (its first
/// row / column excluded), `(order, cx0, cz0, n)`.
fn spawn_order(p: &BiomeProvider, hs: &dyn Heights) -> (Vec<(i32, i32)>, i32, i32, i32) {
    use rwg2_biomes::gtorder::{spawn_populated, LoadSim};
    let seed = p.seed();
    let spawn = spawn_xz(p, hs, seed);
    let walk = rwg2_biomes::spawn::walk_positions(seed);
    let step = walk.iter().position(|&q| q == spawn);
    let mut sim = LoadSim::new();
    for &(x, z) in walk.iter().take(step.map_or(1, |s| s + 1)) {
        sim.load(x >> 4, z >> 4);
    }
    sim.spawn_area(spawn.0, spawn.1);
    let (a, b, c, d) = spawn_populated(spawn);
    let (cx0, cz0, cx1, cz1) = (a + 1, b + 1, c, d);
    let n = (cx1 - cx0 + 1).min(cz1 - cz0 + 1);
    (sim.order, cx0, cz0, n)
}

/// Decoration tops of a fresh world's spawn area in the real load order ([`spawn_order`]): 16-byte header
/// (i32 cx0, cz0, n, populates), then codes and ys as [`deco_tops`] over `16n × 16n` columns from chunk (cx0, cz0).
/// Length via `deco_len`. ~4 s native.
#[no_mangle]
pub extern "C" fn spawn_deco_tops(handle: u32) -> *const u8 {
    let v = PROVIDERS.with(|ps| {
        let ps = ps.borrow();
        let Some(Some(p)) = ps.get(handle as usize) else {
            return Vec::new();
        };
        let hs = heights();
        let (order, cx0, cz0, n) = spawn_order(p, hs);
        let tops = rwg2_biomes::deco::view::ordered_region_tops(
            p,
            hs,
            pop_gates(),
            &order,
            (cx0, cz0, cx0 + n - 1, cz0 + n - 1),
        );
        let side = tops.len();
        let mut out = Vec::with_capacity(16 + 2 * side);
        for v in [cx0, cz0, n, order.len() as i32] {
            out.extend_from_slice(&v.to_le_bytes());
        }
        out.extend(tops.iter().map(|t| t.0));
        out.extend(tops.iter().map(|t| t.1));
        out
    });
    DECO_BUF.with(|b| {
        *b.borrow_mut() = v;
        b.borrow().as_ptr()
    })
}

/// Decoration voxels of a fresh world's spawn area in the real load order (3D view): 16-byte header as
/// [`spawn_deco_tops`], then per chunk of the `n × n` square (z-major) the [`deco_chunk`] record
/// (`[y0 i16, h u16]` + `h · 256` codes). Length via `deco_len`.
#[no_mangle]
pub extern "C" fn spawn_deco_voxels(handle: u32) -> *const u8 {
    let v = PROVIDERS.with(|ps| {
        let ps = ps.borrow();
        let Some(Some(p)) = ps.get(handle as usize) else {
            return Vec::new();
        };
        let hs = heights();
        let (order, cx0, cz0, n) = spawn_order(p, hs);
        let vox = rwg2_biomes::deco::view::ordered_region_voxels(
            p,
            hs,
            pop_gates(),
            &order,
            (cx0, cz0, cx0 + n - 1, cz0 + n - 1),
        );
        let mut out = Vec::with_capacity(16 + vox.iter().map(|v| 4 + v.2.len()).sum::<usize>());
        for v in [cx0, cz0, n, order.len() as i32] {
            out.extend_from_slice(&v.to_le_bytes());
        }
        for (y0, h, codes) in vox {
            out.extend_from_slice(&(y0 as i16).to_le_bytes());
            out.extend_from_slice(&(h as u16).to_le_bytes());
            out.extend_from_slice(&codes);
        }
        out
    });
    DECO_BUF.with(|b| {
        *b.borrow_mut() = v;
        b.borrow().as_ptr()
    })
}

#[no_mangle]
pub extern "C" fn deco_len() -> u32 {
    DECO_BUF.with(|b| b.borrow().len() as u32)
}

/// Decoration palette: 4 bytes per code (r, g, b, kind: 1 cube, 2 leaves, 3 plant, 4 thin layer).
#[no_mangle]
pub extern "C" fn deco_palette() -> *const u8 {
    DECO_BUF.with(|b| {
        let mut v = b.borrow_mut();
        v.clear();
        for &(rgb, k) in rwg2_biomes::deco::view::PALETTE {
            v.extend_from_slice(&[(rgb >> 16) as u8, (rgb >> 8) as u8, rgb as u8, k]);
        }
        v.as_ptr()
    })
}

/// Flat-area patch of a search hit (input buffer = JSON `{"ids":[rwg ids],"flat":{tol,pct,area,land},"c":[x,z],
/// "r":n,"hit":[x,z],"maxExact":n}`), the same patch the search measured. `exact` = 0: approx tops of every cell →
/// `{"cells":[i,j,top,…],"band":[lo,hi]|null,"info":[yLo,yHi,pct,patchArea,flatArea,trunc]|null,"step":32}`
/// (band / tops = top block Y; ground = +1). `exact` = 1: exact chunk tops on an every-k-th-cell subsample of at
/// most `maxExact` cells (default 1200) → `{"n":…,"k":…,"inBand":pct of the sample in the approx band,
/// "best":[yLo,yHi,pct]}` (ground Y). Result via `result_len`.
#[no_mangle]
pub extern "C" fn flat_patch(handle: u32, len: u32, exact: u32) -> *const u8 {
    use rwg2_biomes::search::{flat_best_band, flat_patch_band, flat_patch_cells, Flat, FLAT_STEP};
    let out = IN.with(|b| {
        let b = b.borrow();
        let text = std::str::from_utf8(&b[..(len as usize).min(b.len())]).unwrap_or("");
        let v: serde_json::Value = serde_json::from_str(text).unwrap_or_default();
        let ids: Vec<u16> = v["ids"]
            .as_array()
            .map(|a| {
                a.iter()
                    .filter_map(|x| x.as_u64())
                    .map(|x| x as u16)
                    .collect()
            })
            .unwrap_or_default();
        let f = &v["flat"];
        let flat = Flat {
            tol: f["tol"].as_i64().unwrap_or(20).clamp(0, 128) as i32,
            min_pct: f["pct"].as_u64().unwrap_or(60).min(100) as u8,
            min_area: f["area"].as_i64().unwrap_or(0).clamp(0, 1 << 40),
            land: f["land"].as_bool().unwrap_or(true),
        };
        let pt = |k: &str| {
            (
                v[k][0].as_i64().unwrap_or(0) as i32,
                v[k][1].as_i64().unwrap_or(0) as i32,
            )
        };
        let (c, hit) = (pt("c"), pt("hit"));
        let r = v["r"].as_i64().unwrap_or(0).clamp(0, 1 << 20) as i32;
        let max_exact = v["maxExact"].as_u64().unwrap_or(1200).clamp(16, 20_000) as usize;
        PROVIDERS.with(|ps| {
            let ps = ps.borrow();
            let Some(Some(p)) = ps.get(handle as usize) else {
                return "{\"error\":\"no provider\"}".to_string();
            };
            let h = heights();
            let mut member = |x: i32, z: i32| ids.contains(&p.biome_data(x, z, h).data);
            let Some((cells, truncated)) = flat_patch_cells(c, r, hit, &mut member) else {
                return "{\"error\":\"hit is not inside the biome\"}".to_string();
            };
            let s = FLAT_STEP;
            let mut a = rwg2_biomes::gtorder::ApproxTerrain::new(p, h);
            let tops: Vec<i32> = cells.iter().map(|&(i, j)| a.top(i * s, j * s)).collect();
            let band = flat_patch_band(&tops, &flat, truncated);
            if exact == 0 {
                let mut flatv = Vec::with_capacity(cells.len() * 3);
                for (&(i, j), &t) in cells.iter().zip(&tops) {
                    flatv.extend([i, j, t]);
                }
                let (bj, ij) = match band {
                    Some((fi, (lo, hi))) => (
                        serde_json::json!([lo, hi]),
                        serde_json::json!([
                            fi.y_lo,
                            fi.y_hi,
                            fi.pct,
                            fi.patch_area,
                            fi.flat_area,
                            fi.truncated as u8
                        ]),
                    ),
                    None => (serde_json::Value::Null, serde_json::Value::Null),
                };
                return serde_json::json!({ "cells": flatv, "band": bj, "info": ij, "step": s })
                    .to_string();
            }
            // exact: every k-th cell on both grid axes (spatially even), at most max_exact cells
            let mut k = 1i32;
            while cells
                .iter()
                .filter(|&&(i, j)| i.rem_euclid(k) == 0 && j.rem_euclid(k) == 0)
                .count()
                > max_exact
            {
                k += 1;
            }
            let mut t = rwg2_biomes::dungeons::Terrain::new(p, h);
            let ex: Vec<i32> = cells
                .iter()
                .filter(|&&(i, j)| i.rem_euclid(k) == 0 && j.rem_euclid(k) == 0)
                .filter_map(|&(i, j)| t.top(i * s, j * s))
                .collect();
            let n = ex.len().max(1);
            let in_band = band.map_or(0, |(_, (lo, hi))| {
                ex.iter()
                    .filter(|&&y| y >= lo && y <= hi && (!flat.land || y >= 63))
                    .count()
                    * 100
                    / n
            });
            let (blo, bc) = flat_best_band(&ex, flat.tol, flat.land);
            let best = if bc > 0 {
                serde_json::json!([blo + 1, blo + 2 * flat.tol + 1, bc * 100 / n])
            } else {
                serde_json::Value::Null
            };
            serde_json::json!({ "n": ex.len(), "k": k, "inBand": in_band, "best": best })
                .to_string()
        })
    });
    RESULT.with(|r| {
        *r.borrow_mut() = out;
        r.borrow().as_ptr()
    })
}

thread_local! {
    static SPAWN_ORES: RefCell<Vec<i32>> = const { RefCell::new(Vec::new()) };
}

/// GT ore veins of the oreseeds a fresh world decides while loading its spawn area (spawn walk + spawn area, in
/// the server's load order), with GT's placement test on the exact terrain + caves + RWG2 populate prefix
/// (lakes, dungeons, geodes, dirt/gravel pockets): 92% vs 89% for the order-free model on 7 held-out worlds.
/// Records of 6 i32 like the `features` ore records (kind 20); kind 21 = this oreseed has no vein. ~2–6 s.
/// Length via `spawn_ores_len`.
#[no_mangle]
pub extern "C" fn spawn_ores(handle: u32) -> *const i32 {
    use rwg2_biomes::gtorder::{
        spawn_populated, Caves, LoadSim, OreWorld, Underground, SURFACE_DEPTH,
    };
    use rwg2_biomes::gtores::{alternative, is_oreseed, GT_GTNH};
    let out = PROVIDERS.with(|ps| {
        let ps = ps.borrow();
        let Some(Some(p)) = ps.get(handle as usize) else {
            return Vec::new();
        };
        let hs = heights();
        let seed = p.seed();
        let table = veins();
        let spawn = spawn_xz(p, hs, seed);
        let walk = rwg2_biomes::spawn::walk_positions(seed);
        let step = walk.iter().position(|&q| q == spawn);
        let mut sim = LoadSim::new();
        for &(x, z) in walk.iter().take(step.map_or(1, |s| s + 1)) {
            sim.load(x >> 4, z >> 4);
        }
        sim.spawn_area(spawn.0, spawn.1);
        let firsts = sim.first_processed(|x, z| is_oreseed(&GT_GTNH, x, z));
        let mut t = rwg2_biomes::dungeons::Terrain::new(p, hs);
        let mut u = Underground::new(&mut t, Some(Caves::new(seed, true)), SURFACE_DEPTH);
        let mut pm = rwg2_biomes::population::PopModel::new(p, hs, seed);
        pm.gates = pop_gates().clone();
        let (a, b, c, d) = spawn_populated(spawn);
        pm.load_structures(a - 4, b - 4, c + 4, d + 4);
        u.pop = Some(Box::new(pm));
        u.populated = Some(Default::default());
        let mut w = OreWorld::new(seed, table, &GT_GTNH, &mut u);
        for &(px, pz) in &sim.order {
            if let Some(dn) = w.u.populated.as_mut() {
                dn.insert((px, pz));
            }
            w.process(px, pz);
        }
        let mut out = Vec::new();
        let mut keys: Vec<_> = firsts.keys().copied().collect();
        keys.sort();
        for (cx, cz) in keys {
            let (sx, sz) = (cx * 16, cz * 16);
            match w.decided.get(&(cx, cz)) {
                Some(Some((pl, _))) => {
                    let alt = alternative(table, seed, cx, cz, Some(pl))
                        .map_or(0, |a| ((a.vein as i32 + 1) << 8) | ((a.min_y & 0xff) << 16));
                    let packed = ((sx - pl.west) & 0xff)
                        | (((pl.east - sx - 16) & 0xff) << 8)
                        | (((sz - pl.north) & 0xff) << 16)
                        | (((pl.south - sz - 16) & 0x7f) << 24);
                    out.extend_from_slice(&[
                        20,
                        sx + 8,
                        sz + 8,
                        pl.min_y,
                        pl.vein as i32 | alt,
                        packed,
                    ]);
                }
                Some(None) => out.extend_from_slice(&[21, sx + 8, sz + 8, 0, 0, 0]),
                None => {}
            }
        }
        out
    });
    SPAWN_ORES.with(|o| {
        *o.borrow_mut() = out;
        o.borrow().as_ptr()
    })
}

#[no_mangle]
pub extern "C" fn spawn_ores_len() -> u32 {
    SPAWN_ORES.with(|o| o.borrow().len() as u32)
}

/// Railcraft (ocean) and NHCore (mountain) abyssal geodes in a block rectangle (overworld), exact incl. dark ores:
/// JSON `[{"x","y","z","o":ocean,"d":diamond,"e":emerald,"l":lapis}]`; read with `tc_json_len`.
#[no_mangle]
pub extern "C" fn geodes_json(handle: u32, x0: i32, z0: i32, x1: i32, z1: i32) -> *const u8 {
    let s = PROVIDERS.with(|ps| {
        let ps = ps.borrow();
        let Some(Some(p)) = ps.get(handle as usize) else {
            return "[]".to_string();
        };
        let mut m = rwg2_biomes::population::PopModel::new(p, heights(), p.seed());
        m.gates = pop_gates().clone();
        let g = m.geodes_in_rect(x0 >> 4, z0 >> 4, x1 >> 4, z1 >> 4);
        let v: Vec<serde_json::Value> = g
            .iter()
            .map(|g| serde_json::json!({"x": g.x, "y": g.y, "z": g.z, "o": g.ocean, "d": g.diamond, "e": g.emerald, "l": g.lapis}))
            .collect();
        serde_json::Value::Array(v).to_string()
    });
    TC_JSON.with(|j| {
        *j.borrow_mut() = s;
        j.borrow().as_ptr()
    })
}

/// Underground lakes (`WorldGenLakes` in RWG2 populate: water from y < 50, 1 in 10 chunks; lava from y < 52, 1 in 18;
/// water lakes inside a lava-cave landmark are lava) whose populating chunk lies in a block rectangle (overworld):
/// JSON `[{"x","z","y0","y1","lava","n":liquid blocks,"open":touches a cave}]`; read with `tc_json_len`. Exact
/// replay of the draws; skipped where a structure piece or a village makes the chunk's populate uncertain.
#[no_mangle]
pub extern "C" fn lakes_json(handle: u32, x0: i32, z0: i32, x1: i32, z1: i32) -> *const u8 {
    let s = PROVIDERS.with(|ps| {
        let ps = ps.borrow();
        let Some(Some(p)) = ps.get(handle as usize) else {
            return "[]".to_string();
        };
        let mut m = rwg2_biomes::population::PopModel::new(p, heights(), p.seed());
        m.gates = pop_gates().clone();
        let v: Vec<serde_json::Value> = m
            .lakes_in_rect(x0 >> 4, z0 >> 4, x1 >> 4, z1 >> 4)
            .iter()
            .map(|l| serde_json::json!({"x": l.x, "z": l.z, "y0": l.y0, "y1": l.y1, "lava": l.lava, "n": l.cells, "open": l.open}))
            .collect();
        serde_json::Value::Array(v).to_string()
    });
    TC_JSON.with(|j| {
        *j.borrow_mut() = s;
        j.borrow().as_ptr()
    })
}

fn oil_json(cx: i32, cz: i32, d: &rwg2_biomes::deco::oil::OilDeposit, c: f32) -> serde_json::Value {
    serde_json::json!({"cx": cx, "cz": cz, "x": d.x, "y": d.y, "z": d.z, "r": d.r,
        "c": (c as f64 * 100.0).round() / 100.0, "n": d.cells()})
}

/// DreamMaster oil deposits populated by the 8 × 8 chunks from (cx0, cz0), from one canonical-order replay of the
/// group (`deco::view::region_oil`; the real order outside a fresh spawn area depends on exploration). Same JSON
/// items as [`spawn_oil_json`]'s `oil`; read with `tc_json_len`. 7 checked worlds (±24 chunks): 25 of 44 checkable
/// deposits real, 42 of 74 real oil test columns predicted. ~1 s native per group.
#[no_mangle]
pub extern "C" fn oil_cell_json(handle: u32, cx0: i32, cz0: i32) -> *const u8 {
    let s = PROVIDERS.with(|ps| {
        let ps = ps.borrow();
        let Some(Some(p)) = ps.get(handle as usize) else {
            return "[]".to_string();
        };
        let v: Vec<serde_json::Value> =
            rwg2_biomes::deco::view::region_oil(p, heights(), pop_gates(), (cx0, cz0, cx0 + 7, cz0 + 7))
                .iter()
                .map(|((cx, cz), d, c)| oil_json(*cx, *cz, d, *c))
                .collect();
        serde_json::Value::Array(v).to_string()
    });
    TC_JSON.with(|j| {
        *j.borrow_mut() = s;
        j.borrow().as_ptr()
    })
}

/// DreamMaster oil deposits of a fresh world's spawn area: the populates replayed in the real load order
/// ([`spawn_order`], as [`spawn_deco_tops`]) carry the shared Random through decoration, the springs, ANIMALS and the
/// Post listeners (`deco::oil`). JSON `{"sq":[cx0,cz0,cx1,cz1] (populated in the real order), "oil":[{"cx","cz",
/// "x","y","z","r","c":confidence,"n":sphere cells}]}`; read with `tc_json_len`. Spawn-area checks (7 worlds): 8 of 15 predicted deposits confirmed by real oil, 11 of 15 real oil
/// test columns predicted. ~4 s native.
#[no_mangle]
pub extern "C" fn spawn_oil_json(handle: u32) -> *const u8 {
    let s = PROVIDERS.with(|ps| {
        let ps = ps.borrow();
        let Some(Some(p)) = ps.get(handle as usize) else {
            return "[]".to_string();
        };
        let hs = heights();
        let (order, cx0, cz0, n) = spawn_order(p, hs);
        let (_, steps) =
            rwg2_biomes::deco::view::replay_ordered(p, hs, pop_gates(), &order, &|_, _| None, &|_, _| None);
        let v: Vec<serde_json::Value> = steps
            .iter()
            .filter_map(|st| {
                let (d, c) = st.oil?;
                Some(oil_json(st.cx, st.cz, &d, c))
            })
            .collect();
        // the square whose populates all ran in the real order (deposits of map cells there are replaced)
        serde_json::json!({"sq": [cx0 - 1, cz0 - 1, cx0 + n - 1, cz0 + n - 1], "oil": v}).to_string()
    });
    TC_JSON.with(|j| {
        *j.borrow_mut() = s;
        j.borrow().as_ptr()
    })
}

/// Vanilla dungeons (`WorldGenDungeons`, RWG2 populate) whose populating chunk lies in a block rectangle, with
/// chest loot: `[{x,y,z,l,i,c:[{x,y,z,b,s:[[slot,"item",n],..]}]}]` (b = enchanted-book roll level). Room + loot
/// are draw-exact once the room's verdict is right; the verdict reads approximate terrain/caves (3 checked worlds:
/// 39/41 predicted rooms real, moss pattern 39/39; ~60% of real rooms found). ~1.3 ms/chunk native.
/// Also the minecart chests placed by mineshaft corridors in the same populates: `{x,y,z,m:1,b,s}` (slots name the
/// `mineshaftCorridor` table entries).
#[no_mangle]
pub extern "C" fn vanilla_dungeons_json(
    handle: u32,
    x0: i32,
    z0: i32,
    x1: i32,
    z1: i32,
) -> *const u8 {
    let s = PROVIDERS.with(|ps| {
        let ps = ps.borrow();
        let Some(Some(p)) = ps.get(handle as usize) else {
            return "[]".to_string();
        };
        let (cx0, cz0, cx1, cz1) = (x0 >> 4, z0 >> 4, x1 >> 4, z1 >> 4);
        let mut m = rwg2_biomes::population::PopModel::new(p, heights(), p.seed());
        m.gates = pop_gates().clone();
        m.load_structures(cx0 - 1, cz0 - 1, cx1, cz1);
        let t = rwg2_biomes::loot::LootTable::dungeon();
        let mt = rwg2_biomes::loot::LootTable::mineshaft();
        let mt_name = |k: u32| mt.and_then(|t| t.items.get(k as usize)).map_or("?", |i| i.name.as_str());
        let mut v = Vec::new();
        for cz in cz0..=cz1 {
            for cx in cx0..=cx1 {
                for d in m.dungeons_at(cx, cz) {
                    let c: Vec<serde_json::Value> = d
                        .chests
                        .iter()
                        .map(|(x, y, z, ch)| {
                            let s: Vec<serde_json::Value> = ch
                                .slots
                                .iter()
                                .map(|&(sl, k, n)| serde_json::json!([sl, t.items[k as usize].name, n, k]))
                                .collect();
                            serde_json::json!({"x": x, "y": y, "z": z, "b": ch.book_level, "s": s})
                        })
                        .collect();
                    v.push(serde_json::json!({"x": d.x, "y": d.y, "z": d.z, "l": d.l, "i": d.i1, "c": c}));
                }
                // minecart chests the same populate placed (mineshaft corridors, before the dungeons)
                for (x, y, z, ch) in std::mem::take(&mut m.mine_chests) {
                    let s: Vec<serde_json::Value> = ch
                        .slots
                        .iter()
                        .map(|&(sl, k, n)| serde_json::json!([sl, mt_name(k as u32), n, k]))
                        .collect();
                    v.push(serde_json::json!({"x": x, "y": y, "z": z, "m": 1, "b": ch.book_level, "s": s}));
                }
            }
        }
        serde_json::Value::Array(v).to_string()
    });
    TC_JSON.with(|j| {
        *j.borrow_mut() = s;
        j.borrow().as_ptr()
    })
}

/// Aura nodes of the Nether (-1) / Twilight Forest (7) in a block rectangle, JSON like `tc_nodes_json`.
#[no_mangle]
pub extern "C" fn tc_nodes_json_dim(
    seed: i64,
    dim: i32,
    x0: i32,
    z0: i32,
    x1: i32,
    z1: i32,
) -> *const u8 {
    use rwg2_biomes::tc_dims::{DimTc, DimWorld};
    use rwg2_biomes::thaumcraft::TC_GTNH;
    let (nt, tt) = if dim == 7 {
        (None, Some(tf_terrain(seed)))
    } else {
        (Some(nether_terrain(seed)), None)
    };
    let w = match (&nt, &tt) {
        (_, Some(t)) => DimWorld::Twilight(t),
        (Some(n), _) => DimWorld::Nether(n),
        _ => unreachable!(),
    };
    let mut g = DimTc::new(w, climate(), TC_GTNH);
    let mut nodes = Vec::new();
    for cz in (z0 >> 4)..=(z1 >> 4) {
        for cx in (x0 >> 4)..=(x1 >> 4) {
            g.nodes(seed, cx, cz, &mut nodes);
        }
    }
    let s = nodes_json(&nodes);
    TC_JSON.with(|j| {
        *j.borrow_mut() = s;
        j.borrow().as_ptr()
    })
}

// ============================================================================ street view (3D terrain viewer)

/// Street-view block classes (flat colours; the viewer only needs "what does the ground look like").
mod vc {
    pub const GRASS: u32 = 0;
    pub const STONE: u32 = 1;
    pub const SAND: u32 = 2;
    pub const SNOW: u32 = 3;
    pub const DIRT: u32 = 4;
    pub const CLAY: u32 = 5;
    pub const GRAVEL: u32 = 6;
    pub const ICE: u32 = 7;
    pub const NETHERRACK: u32 = 8;
    pub const SOUL: u32 = 9;
    pub const LAVA: u32 = 10;
    pub const DEADROCK: u32 = 11;
    pub const BEDROCK: u32 = 12;
    pub const MYCELIUM: u32 = 13;
    pub const PACKED_ICE: u32 = 14;
    pub const MUD: u32 = 15;
    pub const HARD_SAND: u32 = 16;
    /// End: main-island end stone / HEE island surface (`end_stone_terrain`, colour by island biome) / void (no
    /// column). End columns carry the lowest solid Y in the ceiling byte.
    pub const END_STONE: u32 = 17;
    pub const END_SURFACE: u32 = 18;
    pub const VOID: u32 = 19;
}

thread_local! {
    static VIEW_OUT: RefCell<Vec<u32>> = const { RefCell::new(Vec::new()) };
    static SURFACE_MODEL: OnceCell<rwg2_biomes::spawn::SurfaceModel> = const { OnceCell::new() };
    static SURFACE_TABLE: OnceCell<rwg2_biomes::surface::SurfaceTable> = const { OnceCell::new() };
}

/// Exact pre-population surface of chunk (cx, cz) (`caves`: with vanilla caves cut in).
fn surface_of(
    p: &BiomeProvider,
    hs: &dyn rwg2_biomes::provider::Heights,
    c: &Chunk,
    cx: i32,
    cz: i32,
    caves: Option<&rwg2_biomes::gtorder::Caves>,
) -> rwg2_biomes::surface::SurfaceChunk {
    SURFACE_MODEL.with(|sm| {
        let m = sm.get_or_init(|| rwg2_biomes::spawn::SurfaceModel::new(&p.table));
        SURFACE_TABLE.with(|st| {
            let t = st.get_or_init(|| rwg2_biomes::surface::SurfaceTable::new(&p.table));
            let cc = rwg2_biomes::spawn::carved_chunk(p, hs, m, c, cx, cz, caves);
            let mut sc = rwg2_biomes::surface::surface_chunk(p, hs, t, c, cx, cz, &cc);
            snow_cover(p, hs, c, &mut sc, cx, cz);
            sc
        })
    })
}

thread_local! {
    /// RWG2 populate snow scores by (provider uid, chunk) (81 biome samples each).
    static SNOW_SCORES: RefCell<rwg2_biomes::jmath::FxMap<(u64, i32, i32), f32>> = RefCell::new(Default::default());
}

/// The population snow / ice pass on a surface chunk (cached scores).
fn snow_cover(
    p: &BiomeProvider,
    hs: &dyn rwg2_biomes::provider::Heights,
    c: &Chunk,
    sc: &mut rwg2_biomes::surface::SurfaceChunk,
    cx: i32,
    cz: i32,
) {
    let uid = p.uid();
    rwg2_biomes::surface::apply_snow_cover_with(
        p,
        &|g| climate().get(g).map(|b| b.temperature),
        &mut |x, z| {
            if let Some(v) = SNOW_SCORES.with(|m| m.borrow().get(&(uid, x, z)).copied()) {
                return v;
            }
            let v = rwg2_biomes::surface::snow_score(p, hs, x, z);
            SNOW_SCORES.with(|m| {
                let mut m = m.borrow_mut();
                if m.len() > 1 << 16 {
                    m.clear();
                }
                m.insert((uid, x, z), v);
            });
            v
        },
        c,
        sc,
        cx,
        cz,
    );
}

/// Street-view class of a surface block.
fn block_vc(b: u8) -> u32 {
    use rwg2_biomes::surface::blk::*;
    match b {
        GRASS | BOP_LONG_GRASS | BOP_ORIGIN_GRASS => vc::GRASS,
        SAND | RED_SAND | SANDSTONE => vc::SAND,
        BOP_HARD_SAND => vc::HARD_SAND,
        SNOW | SNOW_LAYER => vc::SNOW,
        DIRT | COARSE_DIRT | PODZOL | BOP_HARD_DIRT | BOP_DRIED_DIRT | BOP_ASH => vc::DIRT,
        HARDENED_CLAY => vc::CLAY,
        GRAVEL => vc::GRAVEL,
        ICE | BOP_HARD_ICE => vc::ICE,
        PACKED_ICE => vc::PACKED_ICE,
        LAVA => vc::LAVA,
        MYCELIUM => vc::MYCELIUM,
        BOP_MUD => vc::MUD,
        _ if b >= CLAY0 => vc::CLAY,
        _ => vc::STONE,
    }
}

/// Side palette index of a block below the top (viewer `SIDE` table; 0 = none).
fn side_idx(b: u8) -> u32 {
    use rwg2_biomes::surface::blk::*;
    match b {
        DIRT | PODZOL | BOP_HARD_DIRT | BOP_DRIED_DIRT | GRASS | BOP_ORIGIN_GRASS
        | BOP_LONG_GRASS => 1,
        STONE | BOP_CRAG_ROCK | BOP_ASH_STONE | OBSIDIAN | BEDROCK => 2,
        SAND => 3,
        RED_SAND => 4,
        SANDSTONE => 5,
        GRAVEL => 6,
        SNOW | SNOW_LAYER => 8,
        ICE | BOP_HARD_ICE => 9,
        PACKED_ICE => 10,
        COBBLE => 11,
        HARDENED_CLAY => 12,
        COARSE_DIRT => 13,
        BOP_MUD | BOP_ASH | MYCELIUM => 14,
        BOP_HARD_SAND => 15,
        _ if b >= CLAY0 => 7,
        _ => 0,
    }
}

/// Side code of the column below `y`: palette index | run length (≤ 7) << 4 | sandstone follows (2 layers) << 7.
fn side_code(col: &[u8; 256], y: i32) -> u32 {
    if y < 1 {
        return 0;
    }
    let first = col[(y - 1) as usize];
    let f = side_idx(first);
    if f == 0 {
        return 0;
    }
    let mut n = 0;
    while n < 7 && y - 1 - n >= 0 && col[(y - 1 - n) as usize] == first {
        n += 1;
    }
    let ss = first != rwg2_biomes::surface::blk::SANDSTONE
        && y - 1 - n >= 0
        && col[(y - 1 - n) as usize] == rwg2_biomes::surface::blk::SANDSTONE;
    f | ((n as u32) << 4) | ((ss as u32) << 7)
}

/// Colour of a surface block in the street view (grass tinted by biome).
fn block_rgb(b: u8, gen: u16) -> u32 {
    use rwg2_biomes::surface::blk::*;
    match b {
        GRASS | BOP_LONG_GRASS => grass_rgb(gen),
        _ => rwg2_biomes::surface::block_color(b),
    }
}

/// Map class → block code (zoomed-out Blocks mode).
fn vc_block(class: u32) -> u8 {
    use rwg2_biomes::surface::blk::*;
    match class {
        vc::GRASS => GRASS,
        vc::SAND => SAND,
        vc::SNOW => SNOW,
        vc::DIRT => DIRT,
        vc::CLAY => HARDENED_CLAY,
        vc::GRAVEL => GRAVEL,
        vc::ICE => ICE,
        vc::PACKED_ICE => PACKED_ICE,
        vc::MYCELIUM => MYCELIUM,
        vc::MUD => BOP_MUD,
        vc::HARD_SAND => BOP_HARD_SAND,
        vc::LAVA => LAVA,
        _ => STONE,
    }
}

/// Blocks-mode sample: block code | under water << 7 | y << 8 (the floor y under water).
fn blocks_code(sc: &rwg2_biomes::surface::SurfaceChunk, idx: usize) -> u16 {
    use rwg2_biomes::surface::blk::*;
    let col = &sc.cols[idx];
    let mut y = sc.top[idx] as i32;
    if y < 0 {
        return 0;
    }
    let mut wet = 0u16;
    while y > 0 && col[y as usize] == WATER {
        y -= 1;
        wet = 0x80;
    }
    (col[y as usize] as u16 & 0x7f) | wet | ((y.clamp(0, 255) as u16) << 8)
}

fn has_dict(gen: u16, t: &str) -> bool {
    climate().dict(gen).iter().any(|d| d == t)
}

/// Vanilla grass colormap (triangle of the three corner colours) at the biome's temperature / rainfall; swamps
/// darker.
fn grass_rgb(gen: u16) -> u32 {
    let (t, r) = climate()
        .get(gen)
        .map_or((0.8, 0.4), |c| (c.temperature, c.rainfall));
    let t = t.clamp(0.0, 1.0);
    let r = r.clamp(0.0, 1.0) * t;
    let cold = [128.0, 180.0, 151.0];
    let dry = [191.0, 183.0, 85.0];
    let wet = [71.0, 205.0, 51.0];
    let mut c = [0f32; 3];
    for k in 0..3 {
        c[k] = cold[k] * (1.0 - t) + dry[k] * (t - r) + wet[k] * r;
    }
    if has_dict(gen, "SWAMP") {
        c = [
            c[0] * 0.55 + 76.0 * 0.45,
            c[1] * 0.55 + 112.0 * 0.45,
            c[2] * 0.55 + 60.0 * 0.45,
        ];
    }
    ((c[0] as u32) << 16) | ((c[1] as u32) << 8) | c[2] as u32
}

fn class_rgb(class: u32, gen: u16, dim: i32) -> u32 {
    match class {
        vc::GRASS => grass_rgb(gen),
        vc::STONE => 0x7d7d7d,
        vc::SAND => {
            if has_dict(gen, "MESA") {
                0xa95821
            } else {
                0xdbd3a0
            }
        }
        vc::SNOW => 0xf2fafa,
        vc::DIRT => 0x866043,
        vc::CLAY => 0x985f45,
        vc::GRAVEL => 0x837f7e,
        vc::ICE => 0x91b6f7,
        vc::PACKED_ICE => 0xa9c9fc,
        vc::NETHERRACK => 0x6f3634,
        vc::SOUL => match (dim, gen) {
            (-1, 136) => 0x51403a, // Corrupted Sands: soul sand
            (-1, 137) => 0x46403e, // Phantasmagoric Inferno: ash
            (-1, 128) => 0xc9c3a8, // Boneyard
            (-1, 117) => 0x8e3b3b, // Visceral Heap: flesh
            (-1, 138) => 0x4c6a2d, // Undergarden: overgrown netherrack
            _ => 0x51403a,
        },
        vc::LAVA => 0xd4590a,
        vc::DEADROCK => 0x5c5a55,
        vc::BEDROCK => 0x3a3a3a,
        vc::MYCELIUM => 0x6f6369,
        vc::MUD => 0x4a3b2c,
        vc::HARD_SAND => 0xb07a4c,
        _ => 0xff00ff,
    }
}

/// In-game biome name by gen id (base biomes of the RWG2 table).
fn gen_name(gen: u16) -> Option<&'static str> {
    thread_local! {
        static NAMES: OnceCell<&'static std::collections::HashMap<u16, String>> = const { OnceCell::new() };
    }
    let m = NAMES.with(|n| {
        *n.get_or_init(|| {
            let v: serde_json::Value =
                serde_json::from_str(BIOMES_JSON).expect("embedded biomes.json");
            let mut m = std::collections::HashMap::new();
            for b in v["realisticBiomes"].as_array().into_iter().flatten() {
                if let (Some(id), Some(name)) =
                    (b["base"]["id"].as_u64(), b["base"]["name"].as_str())
                {
                    m.insert(id as u16, name.to_string());
                }
            }
            Box::leak(Box::new(m))
        })
    });
    m.get(&gen).map(|s| s.as_str())
}

/// Surface class of a BOP 2.1 biome whose top block the surface model only knows as "other" (BOP grasses, mud,
/// hard sand, mycelium, ...). `None`: dirt (bare dirt / filler).
fn other_top(gen: u16) -> Option<u32> {
    Some(match gen_name(gen)? {
        "Sludgepit" | "Quagmire" => vc::MUD,
        "Outback" => vc::HARD_SAND,
        "Canyon" => vc::CLAY,
        "MushroomIsland" => vc::MYCELIUM,
        "Ice Plains Spikes" => vc::SNOW,
        "Bamboo Forest" => return None,
        // newBopGrass / longGrass / overgrown tops: grass-coloured
        _ => vc::GRASS,
    })
}

/// Sea-floor class of an RWG2 ocean column (`RealisticBiomeOcean.rReplace`: the top six stone layers become sand
/// in shallow oceans, gravel in deep ones; BOP kelp / coral oceans extend it).
fn ocean_floor(t: &BiomeTable, rwg: u16) -> Option<u32> {
    match t.heights.get(rwg as usize)? {
        rwg2_biomes::terrain::HeightModel::Ocean { shallow } => {
            Some(if *shallow { vc::SAND } else { vc::GRAVEL })
        }
        _ => None,
    }
}

/// Zoomed-out Blocks map top block of a land column (no exact chunk): the biome-dictionary class, with per-biome
/// corrections where it named a block the exact surface mostly lacks (v1.66 audit, 5 seeds × 5 areas, step-2 guess
/// vs exact top): Canyon is hard dirt (65%, sand never dominant), Brushland / Heathland / Oasis are mostly sand
/// (87 / 70 / 60%), and snowy tops are a snow layer, not a snow block.
fn zoomed_top(gen: u16, y: i32) -> u8 {
    use rwg2_biomes::surface::blk;
    if y >= 62 {
        match gen_name(gen) {
            Some("Canyon") => return blk::BOP_HARD_DIRT,
            Some("Brushland" | "Heathland" | "Oasis") => return blk::SAND,
            _ => {}
        }
    }
    match vc_block(ow_class(None, gen, y.max(62))) {
        blk::SNOW => blk::SNOW_LAYER,
        b => b,
    }
}

/// Biomes whose top block really is mycelium: only Mushroom Island and its shore (gen 14 / 15,
/// `surface::base_top_filler`). The MUSHROOM dictionary tag also covers BOP Fungi Forest and Twilight Forest's
/// mushroom forests, whose tops are grass.
fn mycelium_top(gen: u16) -> bool {
    matches!(gen, 14 | 15)
}

/// Top class of an overworld column from the surface model + the biome dictionary.
fn ow_class(t: Option<rwg2_biomes::spawn::TopProbs>, gen: u16, y: i32) -> u32 {
    let snowy = has_dict(gen, "SNOWY") || climate().get(gen).is_some_and(|c| c.temperature < 0.15);
    if let Some(t) = t {
        let m = t.grass.max(t.stone).max(t.sand);
        if m > 0.05 {
            if t.stone == m {
                return if snowy && y >= 63 {
                    vc::SNOW
                } else {
                    vc::STONE
                };
            }
            if t.sand == m {
                return vc::SAND;
            }
            return if snowy {
                vc::SNOW
            } else if mycelium_top(gen) {
                vc::MYCELIUM
            } else {
                vc::GRASS
            };
        }
        // an "other" top above the water: BOP's own surface blocks (under water the filler shows: dirt)
        if y >= 62 {
            if let Some(c) = other_top(gen) {
                return if c == vc::GRASS && snowy { vc::SNOW } else { c };
            }
        }
    }
    if y < 62 && has_dict(gen, "OCEAN") {
        // zoomed-out heights: shallow oceans sit around y 52, deep ones around 34
        return if y > 44 { vc::SAND } else { vc::GRAVEL };
    }
    if has_dict(gen, "MESA") {
        vc::CLAY
    } else if snowy {
        vc::SNOW
    } else if has_dict(gen, "SANDY") || has_dict(gen, "DESERT") || has_dict(gen, "BEACH") {
        vc::SAND
    } else if mycelium_top(gen) {
        vc::MYCELIUM
    } else if has_dict(gen, "MOUNTAIN") && y > 130 {
        vc::STONE
    } else if t.is_some() {
        vc::DIRT
    } else {
        vc::GRASS
    }
}

#[inline]
fn pack_col(top: i32, water: i32, class: u32, ceil: i32) -> u32 {
    (top.clamp(0, 255) as u32)
        | ((water.clamp(0, 255) as u32) << 8)
        | (class << 16)
        | ((ceil.clamp(0, 255) as u32) << 24)
}

/// Street-view columns of the overworld: `3·w·h` u32, per sample `[packed, rgb, biome gen id]` with packed = top y (highest solid
/// block, river tunnels / lava craters / snow layers carved; caves not) | water surface y << 8 (0 = dry) | class
/// << 16 | ceiling << 24 (0 = open sky). Exact chunk terrain at step 1; the blurred zoomed-out heights (median 2 blocks off, p90 8–9; `tools/audit/ringaudit.mjs`)
/// with dictionary-based colours at coarser steps.
#[no_mangle]
pub extern "C" fn view_cols(
    handle: u32,
    x0: i32,
    z0: i32,
    w: u32,
    h: u32,
    step: i32,
) -> *const u32 {
    PROVIDERS.with(|ps| {
        let ps = ps.borrow();
        let Some(Some(p)) = ps.get(handle as usize) else {
            return std::ptr::null();
        };
        let hs = heights();
        let (wi, hi) = (w as i32, h as i32);
        let mut v = vec![0u32; (3 * w * h) as usize];
        let marker = p.table.lava_cave_marker_gen_id;
        if step == 1 && !p.table.heights.is_empty() {
            SURFACE_MODEL.with(|sm| {
                let m = sm.get_or_init(|| rwg2_biomes::spawn::SurfaceModel::new(&p.table));
                with_gen(handle, p, |g| {
                    let (x1, z1) = (x0 + wi - 1, z0 + hi - 1);
                    for cz in (z0 >> 4)..=(z1 >> 4) {
                        for cx in (x0 >> 4)..=(x1 >> 4) {
                            let Some(c) = g.generate(p, hs, cx, cz) else {
                                continue;
                            };
                            let cc = rwg2_biomes::spawn::carved_chunk(p, hs, m, &c, cx, cz, None);
                            let mut sc = SURFACE_TABLE.with(|st| {
                                let t = st.get_or_init(|| {
                                    rwg2_biomes::surface::SurfaceTable::new(&p.table)
                                });
                                rwg2_biomes::surface::surface_chunk(p, hs, t, &c, cx, cz, &cc)
                            });
                            snow_cover(p, hs, &c, &mut sc, cx, cz);
                            for lz in 0..16 {
                                for lx in 0..16 {
                                    let (x, z) = (cx * 16 + lx, cz * 16 + lz);
                                    let (i, j) = (x - x0, z - z0);
                                    if i < 0 || j < 0 || i >= wi || j >= hi {
                                        continue;
                                    }
                                    let idx = (lz * 16 + lx) as usize;
                                    let mut y = cc.top(lx, lz);
                                    while y > 0
                                        && cc.pre(lx, y, lz) == rwg2_biomes::column::Pre::Water
                                        && !cc.lava_carved(lx, lz)
                                    {
                                        y -= 1;
                                    }
                                    let mut gen = c.gen[idx];
                                    if Some(gen) == marker {
                                        gen = p.table.biomes[c.rwg[idx] as usize].gen_id;
                                    }
                                    // exact surface block (water / ice: the y found above; ice stays on top)
                                    let col = &sc.cols[idx];
                                    // water only where the column really holds it (lava-cave funnels, carved
                                    // craters and dry pits below sea level stay dry)
                                    let water =
                                        if y < 62 && col[62] == rwg2_biomes::surface::blk::WATER {
                                            62
                                        } else {
                                            0
                                        };
                                    let (y, water) = if col[62] == rwg2_biomes::surface::blk::ICE
                                        && cc.pre(lx, 62, lz) == rwg2_biomes::column::Pre::Water
                                        && cc.top(lx, lz) <= 62
                                    {
                                        (62, 0)
                                    } else {
                                        (y, water)
                                    };
                                    let b = col[y.clamp(0, 255) as usize];
                                    let class = block_vc(b);
                                    let o = 3 * (j * wi + i) as usize;
                                    v[o] = pack_col(y, water, class, 0);
                                    v[o + 1] = block_rgb(b, gen) | (side_code(col, y) << 24);
                                    v[o + 2] = gen as u32;
                                }
                            }
                        }
                    }
                })
            });
        } else if step < 0 && !p.table.heights.is_empty() {
            // mid ring: exact chunk heights (no surface pass, no carving) every |step| blocks
            let s = -step;
            let (x1, z1) = (x0 + (wi - 1) * s, z0 + (hi - 1) * s);
            with_gen(handle, p, |g| {
                for cz in (z0 >> 4)..=(z1 >> 4) {
                    for cx in (x0 >> 4)..=(x1 >> 4) {
                        let Some(c) = g.generate(p, hs, cx, cz) else {
                            continue;
                        };
                        // samples inside this chunk
                        let i0 = ((cx * 16 - x0) + s - 1).div_euclid(s).max(0);
                        let j0 = ((cz * 16 - z0) + s - 1).div_euclid(s).max(0);
                        let mut j = j0;
                        while j < hi && z0 + j * s < cz * 16 + 16 {
                            let mut i = i0;
                            while i < wi && x0 + i * s < cx * 16 + 16 {
                                let (x, z) = (x0 + i * s, z0 + j * s);
                                let idx = (((z & 15) * 16) + (x & 15)) as usize;
                                let y = c.height[idx] as i32;
                                let mut gen = c.gen[idx];
                                if Some(gen) == marker {
                                    gen = p.table.biomes[c.rwg[idx] as usize].gen_id;
                                }
                                let class = match (y < 62)
                                    .then(|| ocean_floor(&p.table, c.rwg[idx]))
                                    .flatten()
                                {
                                    Some(f) => f,
                                    None => ow_class(None, gen, y),
                                };
                                let o = 3 * (j * wi + i) as usize;
                                v[o] = pack_col(y, if y < 62 { 62 } else { 0 }, class, 0);
                                v[o + 1] = class_rgb(class, gen, 0);
                                v[o + 2] = gen as u32;
                                i += 1;
                            }
                            j += 1;
                        }
                    }
                }
            });
        } else {
            let step = step.abs();
            let hv = smoothed_heights(p, x0, z0, w, h, step, BLEND_RADIUS);
            for j in 0..hi {
                for i in 0..wi {
                    let (x, z) = (x0 + i * step, z0 + j * step);
                    let y = hv[(j * wi + i) as usize] as i32;
                    let gen = match p.display(x, z, hs) {
                        Some(r) => p.table.biomes[r as usize].gen_id,
                        None => 0,
                    };
                    let class = ow_class(None, gen, y);
                    let o = 3 * (j * wi + i) as usize;
                    v[o] = pack_col(y, if y < 62 { 62 } else { 0 }, class, 0);
                    v[o + 1] = class_rgb(class, gen, 0);
                    v[o + 2] = gen as u32;
                }
            }
        }
        VIEW_OUT.with(|o| {
            let mut o = o.borrow_mut();
            *o = v;
            o.as_ptr()
        })
    })
}

/// (top, bottom, display id) of one chunk's 256 End columns.
type EndColsRc = std::rc::Rc<[(i16, i16, u8, i16, i16); 256]>;

thread_local! {
    static END_COLS: RefCell<std::collections::HashMap<(i64, i32, i32, i32), EndColsRc>> =
        RefCell::new(std::collections::HashMap::new());
}

/// End columns of chunk (`cx`, `cz`) for the street view: (top, bottom, display id) per column; void = (-1, -1, 0).
/// Main island exact (seed + resets), HEE islands after their caves (internal cave holes not represented).
fn end_cols(seed: i64, dim: i32, cx: i32, cz: i32) -> EndColsRc {
    if let Some(c) = END_COLS.with(|m| m.borrow().get(&(seed, dim, cx, cz)).cloned()) {
        return c;
    }
    let resets = end_resets(dim).unwrap_or(0);
    let mut out = [(-1i16, -1i16, 0u8, -1i16, 0i16); 256];
    if cx.abs() <= END_R && cz.abs() <= END_R {
        let c = end_terrain(seed.wrapping_add(resets)).chunk(cx, cz);
        for (k, col) in c.cols.iter().enumerate() {
            if *col != 0 {
                out[k] = (
                    (127 - col.leading_zeros()) as i16,
                    col.trailing_zeros() as i16,
                    9,
                    -1,
                    0,
                );
            }
        }
    } else if let Some(i) = rwg2_biomes::end::island_covering(seed, cx, cz) {
        let sh = island_shape_cached(seed, i.cx, i.cz);
        for lz in 0..16 {
            for lx in 0..16 {
                let (x, z) = (cx * 16 + lx, cz * 16 + lz);
                let t = sh.top_at(x, z);
                if t >= 0 {
                    out[(lz * 16 + lx) as usize] =
                        (t as i16, sh.bottom_at(x, z) as i16, 100 + i.biome, -1, 0);
                }
            }
        }
    }
    // HEE towers: shell / spires (id 110, obsidian) and their end-stone islands (111). 4th field: lowest tower
    // (obsidian) Y of the top run; 5th: top Y + 1 of the solid part below a gap under the top run (spires above
    // the tower island, roof over the rooms), 0 = none
    let mut tcols: std::collections::BTreeMap<usize, Vec<(i32, bool)>> =
        std::collections::BTreeMap::new();
    for (tx, tz) in rwg2_biomes::end::towers(seed, resets as i32, cx - 2, cz - 2, cx + 1, cz + 1) {
        let t = tower_cached(seed, resets as i32, tx, tz);
        for (x, y, z, b) in t.blocks() {
            if x >> 4 != cx || z >> 4 != cz || !b.solid() {
                continue;
            }
            tcols
                .entry((((z & 15) * 16) + (x & 15)) as usize)
                .or_default()
                .push((y, b == rwg2_biomes::endtower::TB::EndStone));
        }
    }
    for (k, mut ys) in tcols {
        ys.sort();
        let o = &mut out[k];
        let (mut top, mut bot) = (ys[ys.len() - 1].0, ys[0].0);
        if o.0 >= 0 {
            top = top.max(o.0 as i32);
            bot = bot.min(o.1 as i32);
        }
        // top run: walk down from the top while contiguous
        let mut rb = ys.len() - 1;
        while rb > 0 && ys[rb - 1].0 == ys[rb].0 - 1 {
            rb -= 1;
        }
        // two runs only for a floating top part over pure end stone (spires above the tower island); columns
        // with gaps inside the tower (decoration, rooms) stay one column, obsidian from their lowest tower block
        let two = rb > 0 && ys[..rb].iter().all(|p| p.1);
        let low = if two { ys[rb - 1].0 + 1 } else { 0 };
        let run = if two { &ys[rb..] } else { &ys[..] };
        let split = run.iter().find(|p| !p.1).map_or(-1, |p| p.0);
        let id = if ys[ys.len() - 1].1 { 111 } else { 110 };
        *o = (top as i16, bot as i16, id, split as i16, low as i16);
    }
    let rc = std::rc::Rc::new(out);
    END_COLS.with(|m| {
        let mut m = m.borrow_mut();
        if m.len() > 4000 {
            m.clear();
        }
        m.insert((seed, dim, cx, cz), rc.clone());
    });
    rc
}

/// End street-view sample: packed column (top | class << 16 | bottom << 24), rgb, display id. At ≥ 16 blocks per
/// sample HEE islands still come from the (per-worker cached) island shapes.
fn end_view_sample(seed: i64, dim: i32, x: i32, z: i32) -> [u32; 3] {
    let (t, b, id, split, low) =
        end_cols(seed, dim, x >> 4, z >> 4)[((z & 15) * 16 + (x & 15)) as usize];
    if t < 0 {
        return [pack_col(0, 0, vc::VOID, 0), 0, 0];
    }
    let (class, rgb) = match id {
        9 => (vc::END_STONE, 0xdbde9e),
        110 => (vc::END_SURFACE, 0x463766), // HEE tower obsidian (lightened to read against the void)
        111 => (vc::END_STONE, 0xdbde9e),   // tower island
        100 => (vc::END_SURFACE, 0x7d5b8f), // Infested Forest
        101 => (vc::END_SURFACE, 0xa5512f), // Burning Mountains
        _ => (vc::END_SURFACE, 0x5d74b5),   // Enchanted Island
    };
    // water byte: tower columns, lowest obsidian Y (walls from there up are obsidian), 0 = none
    let split = if split > 0 { split as i32 } else { 0 };
    // rgb top byte: top + 1 of the solid part under a gap below the top run (tower spires / roof), 0 = none
    [
        pack_col(t as i32, split, class, b as i32),
        rgb | ((low.clamp(0, 255) as u32) << 24),
        id as u32,
    ]
}

/// `view_cols` for the Nether (-1: floor of the highest open space below y 110 + the ceiling above it, lava ocean
/// as a lava top), the Twilight Forest (7: ground without canopy, water surface) and the End (1 | resets << 8:
/// top, class, lowest solid Y in the ceiling byte; void columns class `VOID`).
#[no_mangle]
pub extern "C" fn view_cols_dim(
    seed: i64,
    dim: i32,
    x0: i32,
    z0: i32,
    w: u32,
    h: u32,
    step: i32,
) -> *const u32 {
    use rwg2_biomes::dimterrain::{nb, tb};
    let (wi, hi) = (w as i32, h as i32);
    let mut v = vec![0u32; (3 * w * h) as usize];
    if end_resets(dim).is_some() {
        let s = step.abs().max(1);
        for j in 0..hi {
            for i in 0..wi {
                let o = 3 * (j * wi + i) as usize;
                v[o..o + 3].copy_from_slice(&end_view_sample(seed, dim, x0 + i * s, z0 + j * s));
            }
        }
    } else if dim == 7 {
        let t = tf_terrain(seed);
        if step == 1 {
            let (x1, z1) = (x0 + wi - 1, z0 + hi - 1);
            for cz in (z0 >> 4)..=(z1 >> 4) {
                for cx in (x0 >> 4)..=(x1 >> 4) {
                    let c = t.chunk(cx, cz);
                    for lz in 0..16 {
                        for lx in 0..16 {
                            let (x, z) = (cx * 16 + lx, cz * 16 + lz);
                            let (i, j) = (x - x0, z - z0);
                            if i < 0 || j < 0 || i >= wi || j >= hi {
                                continue;
                            }
                            let g = c.ground(lx, lz);
                            let top = c.top(lx, lz);
                            let mut wy = top;
                            while wy > g && c.at(lx, wy, lz) != tb::WATER {
                                wy -= 1;
                            }
                            let water = if wy > g && c.at(lx, wy, lz) == tb::WATER {
                                wy
                            } else {
                                0
                            };
                            let gen = t.layers.biome_at(x, z) as u16;
                            let class = match c.at(lx, g, lz) {
                                tb::STONE => vc::STONE,
                                tb::GRAVEL => vc::GRAVEL,
                                tb::ICE => vc::ICE,
                                tb::PACKED_ICE => vc::PACKED_ICE,
                                tb::DEADROCK => vc::DEADROCK,
                                tb::BEDROCK => vc::BEDROCK,
                                _ => {
                                    if has_dict(gen, "SNOWY")
                                        || climate().get(gen).is_some_and(|c| c.temperature < 0.15)
                                    {
                                        vc::SNOW
                                    } else if mycelium_top(gen) {
                                        vc::MYCELIUM
                                    } else {
                                        vc::GRASS
                                    }
                                }
                            };
                            let o = 3 * (j * wi + i) as usize;
                            v[o] = pack_col(g, water, class, 0);
                            v[o + 1] = class_rgb(class, gen, 7);
                            v[o + 2] = gen as u32;
                        }
                    }
                }
            }
        } else if step < 0 {
            // mid ring: exact ground every |step| blocks (whole chunks generated)
            let s = -step;
            let (x1, z1) = (x0 + (wi - 1) * s, z0 + (hi - 1) * s);
            for cz in (z0 >> 4)..=(z1 >> 4) {
                for cx in (x0 >> 4)..=(x1 >> 4) {
                    let c = t.chunk(cx, cz);
                    let i0 = ((cx * 16 - x0) + s - 1).div_euclid(s).max(0);
                    let j0 = ((cz * 16 - z0) + s - 1).div_euclid(s).max(0);
                    let mut j = j0;
                    while j < hi && z0 + j * s < cz * 16 + 16 {
                        let mut i = i0;
                        while i < wi && x0 + i * s < cx * 16 + 16 {
                            let (x, z) = (x0 + i * s, z0 + j * s);
                            let (lx, lz) = (x & 15, z & 15);
                            let g = c.ground(lx, lz);
                            let top = c.top(lx, lz);
                            let mut wy = top;
                            while wy > g && c.at(lx, wy, lz) != tb::WATER {
                                wy -= 1;
                            }
                            let water = if wy > g && c.at(lx, wy, lz) == tb::WATER {
                                wy
                            } else {
                                0
                            };
                            let gen = t.layers.biome_at(x, z) as u16;
                            let class = match c.at(lx, g, lz) {
                                tb::STONE => vc::STONE,
                                tb::GRAVEL => vc::GRAVEL,
                                tb::ICE => vc::ICE,
                                tb::PACKED_ICE => vc::PACKED_ICE,
                                tb::DEADROCK => vc::DEADROCK,
                                _ if has_dict(gen, "SNOWY")
                                    || climate().get(gen).is_some_and(|c| c.temperature < 0.15) =>
                                {
                                    vc::SNOW
                                }
                                _ => vc::GRASS,
                            };
                            let o = 3 * (j * wi + i) as usize;
                            v[o] = pack_col(g, water, class, 0);
                            v[o + 1] = class_rgb(class, gen, 7);
                            v[o + 2] = gen as u32;
                            i += 1;
                        }
                        j += 1;
                    }
                }
            }
        } else {
            let step = step.abs();
            for j in 0..hi {
                for i in 0..wi {
                    let (x, z) = (x0 + i * step, z0 + j * step);
                    let g = t.approx_ground(x, z);
                    let gen = t.layers.biome_at(x, z) as u16;
                    let class = if g > 110 {
                        vc::STONE
                    } else if has_dict(gen, "SNOWY") {
                        vc::SNOW
                    } else {
                        vc::GRASS
                    };
                    let o = 3 * (j * wi + i) as usize;
                    v[o] = pack_col(g, if g < 31 { 31 } else { 0 }, class, 0);
                    v[o + 1] = class_rgb(class, gen, 7);
                    v[o + 2] = gen as u32;
                }
            }
        }
    } else {
        let n = nether_terrain(seed);
        if step == 1 {
            let (x1, z1) = (x0 + wi - 1, z0 + hi - 1);
            for cz in (z0 >> 4)..=(z1 >> 4) {
                for cx in (x0 >> 4)..=(x1 >> 4) {
                    let c = n.chunk(cx, cz);
                    for lz in 0..16 {
                        for lx in 0..16 {
                            let (x, z) = (cx * 16 + lx, cz * 16 + lz);
                            let (i, j) = (x - x0, z - z0);
                            if i < 0 || j < 0 || i >= wi || j >= hi {
                                continue;
                            }
                            let (mut y, lava) = c.floor(lx, lz);
                            if y <= 4 && !lava {
                                y = 110; // solid from 110 down: a pillar
                            }
                            let mut ceil = y + 1;
                            while ceil < 127 && !nb::solid(c.at(lx, ceil, lz)) {
                                ceil += 1;
                            }
                            let gen = n.biome_at(x, z) as u16;
                            let class = if lava {
                                vc::LAVA
                            } else {
                                match c.at(lx, y, lz) {
                                    nb::GRAVEL => vc::GRAVEL,
                                    b if nb::surface(b) => vc::SOUL,
                                    nb::BEDROCK => vc::BEDROCK,
                                    _ if gen != 8 => vc::SOUL,
                                    _ => vc::NETHERRACK,
                                }
                            };
                            let o = 3 * (j * wi + i) as usize;
                            v[o] = pack_col(y, 0, class, ceil);
                            v[o + 1] = class_rgb(class, gen, -1);
                            v[o + 2] = gen as u32;
                        }
                    }
                }
            }
        } else {
            let step = step.abs();
            let grid = n.approx_floor_grid(x0, z0, wi, hi, step);
            for j in 0..hi {
                for i in 0..wi {
                    let (x, z) = (x0 + i * step, z0 + j * step);
                    let (mut y, lava) = grid[(j * wi + i) as usize];
                    if y <= 4 && !lava {
                        y = 110;
                    }
                    let gen = n.biome_at(x, z) as u16;
                    let class = if lava {
                        vc::LAVA
                    } else if gen != 8 {
                        vc::SOUL
                    } else {
                        vc::NETHERRACK
                    };
                    let o = 3 * (j * wi + i) as usize;
                    v[o] = pack_col(y, 0, class, 120);
                    v[o + 1] = class_rgb(class, gen, -1);
                    v[o + 2] = gen as u32;
                }
            }
        }
    }
    VIEW_OUT.with(|o| {
        let mut o = o.borrow_mut();
        *o = v;
        o.as_ptr()
    })
}

// ============================================================================ street view x-ray

thread_local! {
    static XRAY: RefCell<Vec<i32>> = const { RefCell::new(Vec::new()) };
    static XCAVE: RefCell<Vec<u8>> = const { RefCell::new(Vec::new()) };
    static CAVES: RefCell<Option<(i64, rwg2_biomes::gtorder::Caves)>> = const { RefCell::new(None) };
}

#[no_mangle]
pub extern "C" fn xray_len() -> u32 {
    XRAY.with(|o| o.borrow().len() as u32)
}

/// Underground structure boxes touching `[x0, x1] × [z0, z1]` for the street view's x-ray: 7 i32 per box
/// `[kind, minX, minY, minZ, maxX, maxY, maxZ]` (inclusive). Kinds: 1–4 mineshaft corridor / room / crossing /
/// stairs, 10 stronghold piece, 11 stronghold portal room, 20 fortress bridge piece, 21 fortress castle piece,
/// 22 blaze spawner, 23 nether wart patch. `handle` (overworld provider, for the Mesa mineshaft rule) is ignored in
/// other dimensions. Length via `xray_len`.
#[no_mangle]
pub extern "C" fn xray_boxes(
    handle: u32,
    seed: i64,
    dim: i32,
    x0: i32,
    z0: i32,
    x1: i32,
    z1: i32,
) -> *const i32 {
    let mut out: Vec<i32> = Vec::new();
    let touches = |b: &[i32; 6]| b[3] >= x0 && b[0] <= x1 && b[5] >= z0 && b[2] <= z1;
    // structures reach ~8 chunks (mineshaft corridors, fortress bridges) from their start chunk
    let (cx0, cz0, cx1, cz1) = ((x0 >> 4) - 8, (z0 >> 4) - 8, (x1 >> 4) + 8, (z1 >> 4) + 8);
    if dim == 0 {
        PROVIDERS.with(|ps| {
            let ps = ps.borrow();
            let Some(Some(p)) = ps.get(handle as usize) else {
                return;
            };
            let hs = heights();
            let s = rwg2_biomes::mineshaft::Seeds::new(seed);
            for (cx, cz) in rwg2_biomes::mineshaft::starts_in(&s, cx0, cz0, cx1, cz1) {
                let gid = p.biome_gen_id(cx * 16 + 8, cz * 16 + 8, hs);
                let mesa = climate().dict(gid).iter().any(|t| t == "MESA");
                let Some(m) = rwg2_biomes::mineshaft::mineshaft(&s, cx, cz, mesa) else {
                    continue;
                };
                if !touches(&m.bb) {
                    continue;
                }
                for pc in &m.pieces {
                    if touches(&pc.bb) {
                        use rwg2_biomes::mineshaft::Kind;
                        let k = match pc.kind {
                            Kind::Corridor => 1,
                            Kind::Room => 2,
                            Kind::Cross => 3,
                            Kind::Stairs => 4,
                        };
                        out.push(k);
                        out.extend_from_slice(&pc.bb);
                    }
                }
            }
        });
        for c in rwg2_biomes::structures::stronghold_ring(seed) {
            let Some(sh) = rwg2_biomes::stronghold_pieces::stronghold(seed, c) else {
                continue;
            };
            if !touches(&sh.bbox) {
                continue;
            }
            for (i, pc) in sh.pieces.iter().enumerate() {
                if touches(&pc.bbox) {
                    out.push(if i == sh.portal_room { 11 } else { 10 });
                    out.extend_from_slice(&pc.bbox);
                }
            }
        }
    } else if dim == -1 {
        for f in rwg2_biomes::nether::fortresses_in(seed, cx0, cz0, cx1, cz1) {
            let Some(l) = rwg2_biomes::fortress::layout(seed, f.chunk_x, f.chunk_z) else {
                continue;
            };
            if !touches(&l.bbox) {
                continue;
            }
            for pc in &l.pieces {
                if touches(&pc.bbox) {
                    out.push(if pc.kind.is_bridge() { 20 } else { 21 });
                    out.extend_from_slice(&pc.bbox);
                }
            }
            for (sx, sy, sz) in l.spawners() {
                out.extend_from_slice(&[22, sx, sy, sz, sx, sy, sz]);
            }
            for w in l.wart_rooms() {
                for b in w.wart {
                    out.push(23);
                    out.extend_from_slice(&b);
                }
            }
        }
    }
    XRAY.with(|o| {
        let mut o = o.borrow_mut();
        *o = out;
        o.as_ptr()
    })
}

/// Height of the x-ray cave masks (y 0..XRAY_CAVE_H).
pub const XRAY_CAVE_H: i32 = 192;

/// Underground air of chunk (cx, cz) for the x-ray: bit `(y * 256 + lz * 16 + lx)` set = carved air below the
/// original terrain surface (overworld: Hodgepodge caves, mountain-chain river tunnels, lava-cave chambers; the
/// Twilight Forest: its caves and ravines; the Nether: nothing). `XRAY_CAVE_H * 32` bytes.
#[no_mangle]
pub extern "C" fn xray_caves(handle: u32, seed: i64, dim: i32, cx: i32, cz: i32) -> *const u8 {
    let mut bits = vec![0u8; (XRAY_CAVE_H * 32) as usize];
    let mut set = |lx: i32, y: i32, lz: i32| {
        let b = (y * 256 + lz * 16 + lx) as usize;
        bits[b >> 3] |= 1 << (b & 7);
    };
    if dim == 0 {
        PROVIDERS.with(|ps| {
            let ps = ps.borrow();
            let Some(Some(p)) = ps.get(handle as usize) else {
                return;
            };
            let hs = heights();
            if p.table.heights.is_empty() {
                return;
            }
            CAVES.with(|cv| {
                let mut cv = cv.borrow_mut();
                if cv.as_ref().map(|c| c.0) != Some(seed) {
                    *cv = Some((seed, rwg2_biomes::gtorder::Caves::new(seed, true)));
                }
                let caves = &cv.as_ref().unwrap().1;
                with_gen(handle, p, |g| {
                    let Some(c) = g.generate(p, hs, cx, cz) else {
                        return;
                    };
                    let cc = rwg2_biomes::column::CarvedChunk::new(p, hs, &c, cx, cz, Some(caves));
                    for lz in 0..16 {
                        for lx in 0..16 {
                            let top =
                                (c.height[(lz * 16 + lx) as usize] as i32).min(XRAY_CAVE_H - 1);
                            for y in 1..=top {
                                if cc.air(lx, y, lz) {
                                    set(lx, y, lz);
                                }
                            }
                        }
                    }
                });
            });
        });
    } else if dim == 7 {
        use rwg2_biomes::dimterrain::tb;
        let t = tf_terrain(seed);
        let c = t.chunk(cx, cz);
        for lz in 0..16 {
            for lx in 0..16 {
                let g = c.ground(lx, lz).min(XRAY_CAVE_H - 1);
                for y in 1..g {
                    if matches!(c.at(lx, y, lz), tb::AIR | tb::VOID) {
                        set(lx, y, lz);
                    }
                }
            }
        }
    }
    XCAVE.with(|o| {
        let mut o = o.borrow_mut();
        *o = bits;
        o.as_ptr()
    })
}
