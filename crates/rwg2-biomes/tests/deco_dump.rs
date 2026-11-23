#![cfg(feature = "json")]
//! rwg2dump 2.16 `deco216.json` decoration validation: replays the decoration of each dumped 7×7-chunk area in the
//! game's REAL populate order (from the recorded `PopulateChunkEvent.Pre` sequence) and compares every decoration
//! block (y 50..200) with the populated world: recall / precision per area and per biome, for
//!   - `real`: real order, the model's own border-loop entry Random,
//!   - `real+entry`: real order, border-loop entry Random taken from the recorded events (isolates the decorators),
//!   - `canonical`: the viewer's z-major order (what the 3D view / Blocks map do).
//!
//! Draw-offset diagnostics: model vs recorded entry state per chunk (LCG distance), native draws before the BOP
//! decorator (real vs model: the parked +2..+52), and the real draws between BOP decorator phases (Decorate events).
//!
//! Skips without a 2.16 dump. The `.bin.gz` files must be gunzipped next to themselves (`gunzip -k deco216_*.bin.gz`).
//!
//! `DECO_AREA=name` limits to one area; `DECO_CHUNKS=1` prints per-chunk diagnostics.
use rwg2_biomes::config::RwgConfig;
use rwg2_biomes::deco::view::replay_ordered;
use rwg2_biomes::deco::{db, DecoWorld};
use rwg2_biomes::population::{shaft_scan_order, PreGates};
use rwg2_biomes::provider::{BiomeProvider, BiomeTable, TableHeights, WorldType};
use serde_json::Value;
use std::collections::{BTreeMap, HashMap};
use std::path::PathBuf;

type Entry<'a> = &'a dyn Fn(i32, i32) -> Option<i64>;
type Variant<'a> = (&'a str, &'a [(i32, i32)], Entry<'a>);

const Y0: i32 = 50;
const Y1: i32 = 200;
const NY: usize = (Y1 - Y0 + 1) as usize;
const M: i64 = 0x5DEECE66D;
const MASK: i64 = (1 << 48) - 1;

fn root() -> PathBuf {
    std::env::var("RWG2_DATA_DIR")
        .map(|d| PathBuf::from(d).parent().unwrap().to_path_buf())
        .unwrap_or_else(|_| PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../.."))
}

pub fn dumps(file: &str) -> Vec<PathBuf> {
    fn walk(d: &std::path::Path, file: &str, out: &mut Vec<PathBuf>) {
        let Ok(rd) = std::fs::read_dir(d) else { return };
        for e in rd.flatten() {
            let p = e.path();
            if p.is_dir() && !skip_dump_dir(&p) {
                walk(&p, file, out);
            } else if p.file_name().is_some_and(|n| n == file) {
                out.push(p.parent().unwrap().to_path_buf());
            }
        }
    }
    let mut v = Vec::new();
    walk(&root().join(dumps_under()), file, &mut v);
    v.sort();
    v
}

/// Decoration blocks compared (registry names).
fn is_deco(n: &str) -> bool {
    const MC: &[&str] = &[
        "log",
        "log2",
        "leaves",
        "leaves2",
        "cactus",
        "tallgrass",
        "double_plant",
        "deadbush",
        "reeds",
        "pumpkin",
        "yellow_flower",
        "red_flower",
        "vine",
        "cocoa",
        "waterlily",
        "brown_mushroom",
        "red_mushroom",
    ];
    const BOP: &[&str] = &[
        "foliage",
        "flowers",
        "flowers2",
        "mushrooms",
        "plants",
        "logs1",
        "logs2",
        "logs3",
        "logs4",
        "leaves1",
        "leaves2",
        "leaves3",
        "leaves4",
        "colorizedLeaves1",
        "colorizedLeaves2",
        "appleLeaves",
        "persimmonLeaves",
        "petals",
        "bamboo",
        "ivy",
        "treeMoss",
        "flowerVine",
        "willow",
        "coral1",
        "coral2",
    ];
    if let Some(s) = n.strip_prefix("minecraft:") {
        return MC.contains(&s);
    }
    if let Some(s) = n.strip_prefix("BiomesOPlenty:") {
        return BOP.contains(&s);
    }
    n.starts_with("Botania:flower") || n.starts_with("Botania:doubleFlower")
}

/// Recorded Random state → scrambled 48-bit state. `s:<hex>` direct; `o:a,b` = two `nextInt()` of a copy.
pub fn parse_state(s: &str) -> Option<i64> {
    let head = s.split('|').next()?;
    if let Some(h) = head.strip_prefix("s:") {
        return i64::from_str_radix(h, 16).ok();
    }
    let (a, b) = head.strip_prefix("o:")?.split_once(',')?;
    let (a, b): (i32, i32) = (a.parse().ok()?, b.parse().ok()?);
    from_outputs(a, b)
}

/// The shared Random inside a BOP `RandomForcedPositiveOwned` (`|f:<name>=<state>`).
pub fn shared_state(s: &str) -> Option<i64> {
    s.split('|').find_map(|p| {
        p.strip_prefix("f:")
            .and_then(|f| f.split_once('='))
            .and_then(|(_, v)| parse_state(v))
    })
}

/// State before two consecutive `nextInt()` outputs a, b (2^16 search).
fn from_outputs(a: i32, b: i32) -> Option<i64> {
    const INV: i64 = 0xDFE05BCB1365;
    for low in 0..1i64 << 16 {
        let s1 = (((a as i64) << 16) | low) & MASK;
        let s2 = (s1.wrapping_mul(M).wrapping_add(0xB)) & MASK;
        if (s2 >> 16) as i32 == b {
            return Some((s1.wrapping_sub(0xB)).wrapping_mul(INV) & MASK);
        }
    }
    None
}

/// Draws (`next` calls) from state a to state b, if within `max`.
pub fn lcg_distance(a: i64, b: i64, max: u32) -> Option<u32> {
    let mut s = a;
    for n in 0..=max {
        if s == b {
            return Some(n);
        }
        s = (s.wrapping_mul(M).wrapping_add(0xB)) & MASK;
    }
    None
}

fn signed_distance(a: i64, b: i64, max: u32) -> Option<i64> {
    lcg_distance(a, b, max)
        .map(|n| n as i64)
        .or_else(|| lcg_distance(b, a, max).map(|n| -(n as i64)))
}

/// One recorded event: (seq, kind, cx, cz, depth, type, rand).
#[derive(Clone, Debug)]
pub struct Ev {
    pub kind: String,
    pub cx: i32,
    pub cz: i32,
    pub depth: i64,
    pub ty: String,
    pub rand: Option<String>,
}

pub fn events(a: &Value) -> Vec<Ev> {
    a["events"]
        .as_array()
        .map(|v| {
            v.iter()
                .map(|e| Ev {
                    kind: e[1].as_str().unwrap_or("").into(),
                    cx: e[2].as_i64().unwrap_or(0) as i32,
                    cz: e[3].as_i64().unwrap_or(0) as i32,
                    depth: e[4].as_i64().unwrap_or(0),
                    ty: e[5].as_str().unwrap_or("").into(),
                    rand: e[6].as_str().map(String::from),
                })
                .collect()
        })
        .unwrap_or_default()
}

/// Per populated chunk, in populate-start order: (cx, cz, depth, events of its own segment).
pub fn segments(ev: &[Ev]) -> Vec<(i32, i32, i64, Vec<Ev>)> {
    let mut out: Vec<(i32, i32, i64, Vec<Ev>)> = Vec::new();
    let mut open: Vec<usize> = Vec::new(); // stack of indices into out
    for e in ev {
        match e.kind.as_str() {
            "popPre" => {
                out.push((e.cx, e.cz, e.depth, vec![e.clone()]));
                open.push(out.len() - 1);
            }
            "popPost" => {
                if let Some(i) = open.pop() {
                    out[i].3.push(e.clone());
                }
            }
            _ => {
                if let Some(&i) = open.last() {
                    out[i].3.push(e.clone());
                }
            }
        }
    }
    out
}

/// Recorded border-loop entry: RWG2's own `decPre` is followed by the SAND .. BIG_SHROOM events it fires on the
/// populate Random (no draws between them); the state at the last of that run is the border loop's entry.
fn loop_entry(seg: &[Ev]) -> Option<i64> {
    let i = seg.iter().position(|e| e.kind == "decPre")?;
    let mut last = None;
    for e in &seg[i + 1..] {
        let rand = e.rand.as_deref().unwrap_or("");
        if e.kind != "decorate" || rand.contains("|c:") {
            break;
        }
        last = parse_state(rand);
        if e.ty == "BIG_SHROOM" {
            break;
        }
    }
    last
}

/// Phases of the decorator starting at `seg[0]` (a `decPre`, BOP or vanilla).
fn decorator_phases(seg: &[Ev]) -> Option<(i64, Vec<(String, i64)>)> {
    let st = |e: &Ev| {
        e.rand
            .as_deref()
            .and_then(|r| shared_state(r).or_else(|| parse_state(r)))
    };
    let start = st(&seg[0])?;
    let mut ph = Vec::new();
    let mut post = false;
    for e in &seg[1..] {
        let bop = e.rand.as_deref().is_some_and(|r| r.contains("|c:"));
        if e.kind == "decPre" || (post && !bop) {
            // the first event after a BOP decorator's feature section: the shared state it left
            if post && seg[0].rand.as_deref().is_some_and(|r| r.contains("|c:")) {
                if let Some(s) = st(e) {
                    ph.push(("END".into(), s));
                }
            }
            break;
        }
        if e.kind == "decPost" {
            post = true;
            if let Some(s) = st(e) {
                ph.push(("decPost".into(), s));
            }
            continue;
        }
        if e.kind == "decorate" {
            if let Some(s) = st(e) {
                ph.push((
                    if post {
                        format!("{}2", e.ty)
                    } else {
                        e.ty.clone()
                    },
                    s,
                ));
            }
        }
    }
    Some((start, ph))
}

/// BOP decorator entry (shared Random at `decPre`) and the shared state at each later Decorate event.
fn bop_phases(seg: &[Ev]) -> Option<(i64, Vec<(String, i64)>)> {
    let i = seg
        .iter()
        .position(|e| e.kind == "decPre" && e.rand.as_deref().is_some_and(|r| r.contains("|c:")))?;
    let start = seg[i]
        .rand
        .as_deref()
        .and_then(|r| shared_state(r).or_else(|| parse_state(r)))?;
    let mut ph = Vec::new();
    let mut post = false;
    for e in &seg[i + 1..] {
        let bop = e.rand.as_deref().is_some_and(|r| r.contains("|c:"));
        if e.kind == "decPre" || (post && !bop) {
            break;
        }
        if e.kind == "decPost" {
            post = true;
            if let Some(s) = e
                .rand
                .as_deref()
                .and_then(|r| shared_state(r).or_else(|| parse_state(r)))
            {
                ph.push(("decPost".into(), s));
            }
            continue;
        }
        if e.kind == "decorate" && post {
            if let Some(s) = e
                .rand
                .as_deref()
                .and_then(|r| shared_state(r).or_else(|| parse_state(r)))
            {
                ph.push((format!("{}2", e.ty), s));
            }
            continue;
        }
        if e.kind == "decorate" {
            if let Some(s) = e
                .rand
                .as_deref()
                .and_then(|r| shared_state(r).or_else(|| parse_state(r)))
            {
                ph.push((e.ty.clone(), s));
            }
        }
    }
    Some((start, ph))
}

struct Area {
    name: String,
    cx0: i32,
    cz0: i32,
    w: i32,
    bin: Vec<u8>,
    ev: Vec<Ev>,
}

impl Area {
    fn nc(&self) -> usize {
        (self.w * self.w) as usize
    }
    /// block index at world (x, y, z) inside the area, y in Y0..=Y1
    fn block(&self, x: i32, y: i32, z: i32) -> usize {
        let (ci, idx) = self.ci(x, z);
        let o = (ci * 256 + idx) * NY + (y - Y0) as usize;
        u16::from_le_bytes([self.bin[o * 2], self.bin[o * 2 + 1]]) as usize
    }
    fn meta(&self, x: i32, y: i32, z: i32) -> u8 {
        let (ci, idx) = self.ci(x, z);
        let o = (ci * 256 + idx) * NY + (y - Y0) as usize;
        self.bin[self.nc() * 256 * NY * 2 + o]
    }
    fn height(&self, x: i32, z: i32) -> u8 {
        let (ci, idx) = self.ci(x, z);
        self.bin[self.nc() * 256 * NY * 3 + ci * 256 + idx]
    }
    fn ci(&self, x: i32, z: i32) -> (usize, usize) {
        let (cx, cz) = ((x >> 4) - self.cx0, (z >> 4) - self.cz0);
        (
            (cz * self.w + cx) as usize,
            ((z & 15) * 16 + (x & 15)) as usize,
        )
    }
}

fn gates() -> PreGates {
    let cj: Value = serde_json::from_str(
        &std::fs::read_to_string(root().join(pack_rel("climate.json"))).unwrap(),
    )
    .unwrap();
    PreGates::new(cj["biomes"].as_array().unwrap().iter().map(|b| {
        (
            b["id"].as_u64().unwrap() as u16,
            b["name"].as_str().unwrap_or(""),
            b["dict"]
                .as_array()
                .map(|d| d.iter().filter_map(|x| x.as_str()).collect())
                .unwrap_or_default(),
        )
    }))
}

/// Per BOP Decorate phase: real − model draws from the decorator's `decPre` (k-th BOP decorator of a chunk vs the
/// model's k-th), over chunks where both are known.
fn phase_deltas(segs: &[(i32, i32, i64, Vec<Ev>)], model: &[(i32, i32, &'static str, i64)]) {
    let mut by: BTreeMap<&str, Vec<i64>> = BTreeMap::new();
    for s in segs {
        // real: every BOP decorator of the segment
        let mut reals: Vec<(i64, Vec<(String, i64)>)> = Vec::new();
        let ev = &s.3;
        // every decorator's decPre except RWG2's own first one (BOP: wrapped Random; vanilla: plain)
        let first = ev.iter().position(|e| e.kind == "decPre");
        for i in 0..ev.len() {
            if ev[i].kind == "decPre" && Some(i) != first {
                if let Some(p) = decorator_phases(&ev[i..]) {
                    reals.push(p);
                }
            }
        }
        // model: split this chunk's marks at each decPre
        let mut models: Vec<Vec<(&str, i64)>> = Vec::new();
        for &(cx, cz, n, st) in model {
            if (cx, cz) != (s.0, s.1) {
                continue;
            }
            if n == "decPre" {
                models.push(vec![(n, st)]);
            } else if let Some(m) = models.last_mut() {
                m.push((n, st));
            }
        }
        if std::env::var("DECO_MODEL_LAST").is_ok() {
            if let Some(m) = models.last() {
                let line: Vec<String> = m
                    .iter()
                    .map(|(n, st)| format!("{n}:{:?}", lcg_distance(m[0].1, *st, 200_000)))
                    .collect();
                eprintln!("      model last ({}, {}): {}", s.0, s.1, line.join(" "));
            }
        }
        if std::env::var("DECO_LAST").is_ok() {
            if let (Some(r), Some(m)) = (reals.last(), models.last()) {
                eprintln!(
                    "      last decPre ({}, {}): real−model {:?} (reals {}, models {})",
                    s.0,
                    s.1,
                    signed_distance(m[0].1, r.0, 200_000),
                    reals.len(),
                    models.len()
                );
            }
        }
        for (k, (start, ph)) in reals.iter().enumerate() {
            let Some(m) = models.get(k) else { continue };
            let m0 = m[0].1;
            let mut line = String::new();
            for (ty, st) in ph {
                let Some(rn) = lcg_distance(*start, *st, 2_000_000) else {
                    continue;
                };
                if let Some(&(_, ms)) = m.iter().find(|e| e.0 == ty.as_str()) {
                    if let Some(mn) = lcg_distance(m0, ms, 2_000_000) {
                        let key: &str = Box::leak(ty.clone().into_boxed_str());
                        by.entry(key).or_default().push(rn as i64 - mn as i64);
                        line += &format!(" {ty}:{}", rn as i64 - mn as i64);
                    }
                }
            }
            if std::env::var("DECO_PHASE_CHUNKS").is_ok() {
                eprintln!("      phases ({}, {}) #{k}:{line}", s.0, s.1);
            }
        }
    }
    for (ty, mut v) in by {
        v.sort();
        let zero = v.iter().filter(|&&d| d == 0).count();
        eprintln!(
            "    phase {ty:12} real−model: zero {zero}/{} median {} min {} max {}",
            v.len(),
            v[v.len() / 2],
            v[0],
            v[v.len() - 1]
        );
    }
}

thread_local! {
    static PAIRS: std::cell::RefCell<BTreeMap<(String, String), usize>> = Default::default();
}

/// Prints and clears the most frequent (real, model) name mismatches (`DECO_PAIRS=1`).
fn dump_pairs() {
    PAIRS.with(|pp| {
        let mut v: Vec<_> = pp.borrow().iter().map(|(k, c)| (k.clone(), *c)).collect();
        v.sort_by_key(|e| std::cmp::Reverse(e.1));
        for ((r, m), c) in v.into_iter().take(12) {
            eprintln!("      pair {c:7} real {r:40} model {m}");
        }
        pp.borrow_mut().clear();
    });
}

#[derive(Default, Clone, Copy)]
struct Score {
    real: usize,
    model: usize,
    both: usize,
    both_name: usize,
}

impl Score {
    fn fmt(&self) -> String {
        let r = |a: usize, b: usize| {
            if b == 0 {
                f64::NAN
            } else {
                a as f64 / b as f64
            }
        };
        format!(
            "recall {:.3} precision {:.3} (same name {:.3}/{:.3}) real {} model {}",
            r(self.both, self.real),
            r(self.both, self.model),
            r(self.both_name, self.real),
            r(self.both_name, self.model),
            self.real,
            self.model
        )
    }
}

fn score(
    a: &Area,
    names: &[String],
    w: &mut DecoWorld,
    p: &BiomeProvider,
    by_biome: &mut BTreeMap<String, Score>,
) -> Score {
    let mut s = Score::default();
    let per = std::env::var("DECO_CHUNK_MISS").is_ok();
    for cz in a.cz0..a.cz0 + a.w {
        for cx in a.cx0..a.cx0 + a.w {
            let before = s;
            for lz in 0..16 {
                for lx in 0..16 {
                    let (x, z) = (cx * 16 + lx, cz * 16 + lz);
                    let biome = p
                        .table
                        .info(p.biome_data(x, z, &TableHeights).data as _)
                        .name
                        .clone();
                    let e = by_biome.entry(biome).or_default();
                    for y in Y0..=Y1 {
                        let rn = names
                            .get(a.block(x, y, z))
                            .map(String::as_str)
                            .unwrap_or("?");
                        let mn = db::name(w.get(x, y, z));
                        let (r, m) = (is_deco(rn), is_deco(mn));
                        if (r || m) && rn != mn && std::env::var("DECO_PAIRS").is_ok() {
                            PAIRS.with(|pp| {
                                *pp.borrow_mut()
                                    .entry((rn.to_string(), mn.to_string()))
                                    .or_insert(0) += 1
                            });
                        }
                        for t in [&mut s, &mut *e] {
                            t.real += r as usize;
                            t.model += m as usize;
                            t.both += (r && m) as usize;
                            t.both_name += (r && m && rn == mn) as usize;
                        }
                    }
                }
            }
            if per {
                eprintln!(
                    "      chunk ({cx}, {cz}) real {} model {} both {}",
                    s.real - before.real,
                    s.model - before.model,
                    s.both - before.both
                );
            }
        }
    }
    s
}

#[test]
fn deco_dump_216() {
    let dirs = dumps("deco216.json");
    if dirs.is_empty() {
        eprintln!("no rwg2dump 2.16 deco216.json under testdata/pack: skipped");
        return;
    }
    let table = BiomeTable::from_rwg2dump_json(
        &std::fs::read_to_string(root().join(pack_rel("biomes.json"))).unwrap(),
    )
    .unwrap();
    let g = gates();
    let only = std::env::var("DECO_AREA").ok();
    let per_chunk = std::env::var("DECO_CHUNKS").is_ok();
    for d in dirs {
        let j: Value =
            serde_json::from_str(&std::fs::read_to_string(d.join("deco216.json")).unwrap())
                .unwrap();
        let Some(seed) = j["worldSeed"].as_str().and_then(|s| s.parse::<i64>().ok()) else {
            eprintln!("{}: no worldSeed ({})", d.display(), j["error"]);
            continue;
        };
        eprintln!(
            "== {} seed {seed} hooks {} state {}",
            d.display(),
            j["hooks"],
            j["randomStateMethod"]
        );
        let names: Vec<String> = j["blocks"]
            .as_array()
            .map(|a| {
                a.iter()
                    .map(|v| v.as_str().unwrap_or("?").to_string())
                    .collect()
            })
            .unwrap_or_default();
        let p = BiomeProvider::new(
            seed,
            WorldType::Rwg2Continents,
            table.clone(),
            RwgConfig::default(),
        );
        let h = TableHeights;
        for a in j["areas"].as_array().into_iter().flatten() {
            let name = a["name"].as_str().unwrap_or("?").to_string();
            if only.as_deref().is_some_and(|o| o != name) {
                continue;
            }
            let Some(f) = a["file"].as_str() else {
                eprintln!(
                    "  {name}: {}",
                    if a["skipped"].is_null() {
                        a["error"].to_string()
                    } else {
                        a["skipped"].to_string()
                    }
                );
                continue;
            };
            let raw = d.join(f.trim_end_matches(".gz"));
            let Ok(bin) = std::fs::read(&raw) else {
                eprintln!("  {name}: missing {} (gunzip -k {f})", raw.display());
                continue;
            };
            let area = Area {
                name: name.clone(),
                cx0: a["chunkX0"].as_i64().unwrap() as i32,
                cz0: a["chunkZ0"].as_i64().unwrap() as i32,
                w: a["chunks"].as_i64().unwrap() as i32,
                bin,
                ev: events(a),
            };
            let segs = segments(&area.ev);
            let (x0, z0, x1, z1) = (
                area.cx0 - 1,
                area.cz0 - 1,
                area.cx0 + area.w - 1,
                area.cz0 + area.w - 1,
            );
            let order: Vec<(i32, i32)> = segs
                .iter()
                .filter(|s| (x0..=x1).contains(&s.0) && (z0..=z1).contains(&s.1))
                .map(|s| (s.0, s.1))
                .collect();
            let nested = segs.iter().filter(|s| s.2 > 1).count();
            // the deferred ocean decorator's entry (shared Random at its decPre, after the ICE event)
            // (after popPost, so not in the segments: the raw event list)
            let mut oceans: HashMap<(i32, i32), i64> = HashMap::new();
            {
                let mut after_ice: Option<(i32, i32)> = None;
                for e in &area.ev {
                    if e.kind == "populate" && e.ty == "ICE" {
                        after_ice = Some((e.cx, e.cz));
                    } else if e.kind == "decPre" && after_ice == Some((e.cx, e.cz)) {
                        if let Some(st) = e.rand.as_deref().and_then(shared_state) {
                            oceans.entry((e.cx, e.cz)).or_insert(st);
                        }
                        after_ice = None;
                    } else if e.kind == "popPre" {
                        after_ice = None;
                    }
                }
            }
            let entries: HashMap<(i32, i32), i64> = segs
                .iter()
                .filter_map(|s| loop_entry(&s.3).map(|e| ((s.0, s.1), e)))
                .collect();
            eprintln!(
                "  -- {} ({}, {}) {}²: {} populates in range (of {}), nested {}, recorded entries {}, fresh {} loadedBefore {} onDisk {}",
                area.name,
                area.cx0,
                area.cz0,
                area.w,
                order.len(),
                segs.len(),
                nested,
                entries.len(),
                a["freshChunks"],
                a["loadedBefore"],
                a["onDiskBefore"]
            );
            if std::env::var("DECO_ORDER").is_ok() {
                eprintln!("    order {:?}", order);
            }
            if order.is_empty() {
                continue;
            }
            let mut canonical = Vec::new();
            for cz in z0..=z1 {
                for cx in x0..=x1 {
                    canonical.push((cx, cz));
                }
            }
            // spawn-area load order (x outer, z inner): measures the replay's order sensitivity
            let mut xmajor = Vec::new();
            for cx in x0..=x1 {
                for cz in z0..=z1 {
                    xmajor.push((cx, cz));
                }
            }
            let none = |_: i32, _: i32| None;
            let rec = |cx: i32, cz: i32| entries.get(&(cx, cz)).copied();
            let mut variants: Vec<Variant> = vec![
                ("real", &order, &none),
                ("real+entry", &order, &rec),
                ("canonical", &canonical, &none),
            ];
            let xrev: Vec<(i32, i32)> = xmajor.iter().rev().copied().collect();
            let zrev: Vec<(i32, i32)> = canonical.iter().rev().copied().collect();
            if std::env::var("DECO_XMAJOR").is_ok() {
                variants.push(("x-major", &xmajor, &none));
                variants.push(("x-rev", &xrev, &none));
                variants.push(("z-rev", &zrev, &none));
            }
            // random permutations (order-noise level)
            let shuffles: Vec<Vec<(i32, i32)>> = (0..std::env::var("DECO_SHUFFLE")
                .ok()
                .and_then(|v| v.parse::<u64>().ok())
                .unwrap_or(0))
                .map(|k| {
                    let mut o = canonical.clone();
                    let mut r = rwg2_biomes::java_random::JavaRandom::new(k as i64 * 7919 + 1);
                    for i in (1..o.len()).rev() {
                        o.swap(i, r.next_int_bounded((i + 1) as i32) as usize);
                    }
                    o
                })
                .collect();
            for o in &shuffles {
                variants.push(("shuffle", o, &none));
            }
            // the viewer's tiling: independent 4×4-chunk group replays (`region_blocks`), merged
            if let Some(n) = std::env::var("DECO_GROUPS").ok().and_then(|v| v.parse::<i32>().ok()) {
                let mut w = rwg2_biomes::deco::DecoWorld::new(&p, &h, seed);
                let (a0, b0) = (area.cx0.div_euclid(n), area.cz0.div_euclid(n));
                let (a1, b1) = ((area.cx0 + area.w - 1).div_euclid(n), (area.cz0 + area.w - 1).div_euclid(n));
                for gz in b0..=b1 {
                    for gx in a0..=a1 {
                        let rect = (gx * n, gz * n, gx * n + n - 1, gz * n + n - 1);
                        for ((x, y, z), b) in rwg2_biomes::deco::view::region_blocks(&p, &h, &g, rect) {
                            w.set(x, y, z, b);
                        }
                    }
                }
                let mut by: BTreeMap<String, Score> = BTreeMap::new();
                let s = score(&area, &names, &mut w, &p, &mut by);
                eprintln!("    groups{n:<5} {}", s.fmt());
            }
            for (vn, ord, ent) in variants {
                let none2 = |_: i32, _: i32| None;
                let ocean = |cx: i32, cz: i32| oceans.get(&(cx, cz)).copied();
                let oe: &dyn Fn(i32, i32) -> Option<i64> =
                    if vn == "real" || vn == "real+entry" { &ocean } else { &none2 };
                let (mut w, steps) =
                    rwg2_biomes::deco::view::replay_ordered_with(&p, &h, &g, ord, ent, oe, vn == "real+entry");
                if vn == "real" && w.ocean_entry_check.1 > 0 {
                    eprintln!(
                        "    ocean entry (modelled post-populate tail == recorded): {}/{}",
                        w.ocean_entry_check.0, w.ocean_entry_check.1
                    );
                }
                let mut by: BTreeMap<String, Score> = BTreeMap::new();
                let s = score(&area, &names, &mut w, &p, &mut by);
                eprintln!("    {vn:11} {}", s.fmt());
                if let (Ok(want), true) = (std::env::var("DECO_MISSING"), vn == "real+entry") {
                    // real `want` blocks where the model has air: metadata / y histogram + sample positions
                    let (bx0, bz0) = (area.cx0 * 16, area.cz0 * 16);
                    let n = area.w * 16;
                    let mut hist: BTreeMap<(u8, i32), u32> = BTreeMap::new();
                    let mut sample = Vec::new();
                    for x in bx0..bx0 + n {
                        for z in bz0..bz0 + n {
                            for y in Y0..=Y1 {
                                let o = area.block(x, y, z);
                                if names[o] != want {
                                    continue;
                                }
                                if db::id(w.get(x, y, z)) != 0 {
                                    continue;
                                }
                                let m = area.meta(x, y, z);
                                *hist.entry((m, y)).or_default() += 1;
                                if sample.len() < 30 {
                                    sample.push((x, y, z, m));
                                }
                            }
                        }
                    }
                    let mut by_meta: BTreeMap<u8, u32> = BTreeMap::new();
                    for ((m, _), c) in &hist {
                        *by_meta.entry(*m).or_default() += c;
                    }
                    eprintln!("    missing {want} by meta {by_meta:?}");
                    let mut edge: BTreeMap<i32, u32> = BTreeMap::new();
                    for x in bx0..bx0 + n {
                        for z in bz0..bz0 + n {
                            for y in Y0..=Y1 {
                                if names[area.block(x, y, z)] == want && db::id(w.get(x, y, z)) == 0 {
                                    let d = (x - bx0).min(z - bz0);
                                    *edge.entry((d / 4) * 4).or_default() += 1;
                                }
                            }
                        }
                    }
                    eprintln!("    missing {want} by distance to the W/N edge (4-block bins) {edge:?}");
                    let mut per: BTreeMap<(i32, i32), u32> = BTreeMap::new();
                    for x in bx0 + 8..bx0 + n {
                        for z in bz0 + 8..bz0 + n {
                            for y in Y0..=Y1 {
                                if names[area.block(x, y, z)] == want && db::id(w.get(x, y, z)) == 0 {
                                    *per.entry((x >> 4, z >> 4)).or_default() += 1;
                                }
                            }
                        }
                    }
                    let mut v: Vec<_> = per.into_iter().collect();
                    v.sort_by_key(|e| std::cmp::Reverse(e.1));
                    eprintln!("    interior missing {want} per chunk (top): {:?}", &v[..v.len().min(8)]);
                    eprintln!("    sample {sample:?}");
                    for &(x, y, z, _) in sample.iter().take(4) {
                        let row: Vec<String> = (-2..=2)
                            .map(|d| format!("{}:{}", db::name(w.get(x + d, y, z)), db::meta(w.get(x + d, y, z))))
                            .collect();
                        eprintln!("    model x-2..x+2 at ({x},{y},{z}): {row:?}");
                    }
                }
                if std::env::var("KELP_SHIFT").is_ok() && vn == "real" {
                    // best (dx, dy, dz) between real and model coral1 voxels
                    let (bx0, bz0) = (area.cx0 * 16, area.cz0 * 16);
                    let n = area.w * 16;
                    let mut realv = std::collections::HashSet::new();
                    let mut modelv = Vec::new();
                    for x in bx0..bx0 + n {
                        for z in bz0..bz0 + n {
                            for y in Y0..63 {
                                if names[area.block(x, y, z)] == "BiomesOPlenty:coral1" {
                                    realv.insert((x, y, z));
                                }
                                if db::id(w.get(x, y, z)) == db::BOP_CORAL1 {
                                    modelv.push((x, y, z));
                                }
                            }
                        }
                    }
                    let mut best = Vec::new();
                    for dx in -17..=17 {
                        for dz in -17..=17 {
                            for dy in -3..=3 {
                                let hit = modelv.iter().filter(|&&(x, y, z)| realv.contains(&(x + dx, y + dy, z + dz))).count();
                                best.push((hit, dx, dy, dz));
                            }
                        }
                    }
                    best.sort();
                    eprintln!("    kelp shift (hits, dx, dy, dz) top: {:?} of real {} model {}", &best[best.len() - 5..], realv.len(), modelv.len());
                }
                dump_pairs();
                if vn == "real+entry" {
                    if let Ok(c) = std::env::var("DBG_CHUNK") {
                        let v: Vec<i32> = c.split(',').map(|t| t.parse().unwrap()).collect();
                        let (bx, bz) = (v[0] * 16 + 8, v[1] * 16 + 8);
                        for (n, x, y, z, _) in w.trace.clone() {
                            if !(bx..bx + 16).contains(&x) || !(bz..bz + 16).contains(&z) {
                                continue;
                            }
                            let mut col = String::new();
                            for yy in (Y0..=Y1).rev() {
                                let b = &names[area.block(x, yy, z)];
                                if b.contains("log") || yy == y || yy == y - 1 {
                                    col += &format!(" {yy}:{}", b.trim_start_matches("minecraft:"));
                                }
                                if col.len() > 160 {
                                    break;
                                }
                            }
                            let (m0, m1) = (w.get(x, y, z), w.get(x, y - 1, z));
                            eprintln!(
                                "      mark {n:10} ({x}, {y}, {z}) model {:?}/{:?} real{col}",
                                db::name(m0),
                                db::name(m1)
                            );
                        }
                    }
                    if let Ok(c) = std::env::var("TRUNKS") {
                        // trunk bases (log over non-log) in the chunk's +8 decoration area, real vs model
                        let v: Vec<i32> = c.split(',').map(|t| t.parse().unwrap()).collect();
                        let (bx, bz) = (v[0] * 16 + 8, v[1] * 16 + 8);
                        let (mut rt, mut mt) = (Vec::new(), Vec::new());
                        for x in bx..bx + 16 {
                            for z in bz..bz + 16 {
                                for y in Y0 + 1..=Y1 {
                                    let (r1, r0) = (
                                        names[area.block(x, y, z)].as_str(),
                                        names[area.block(x, y - 1, z)].as_str(),
                                    );
                                    if r1.ends_with(":log") && !r0.ends_with(":log") {
                                        let mut h = 0;
                                        while names[area.block(x, y + h, z)].ends_with(":log") {
                                            h += 1;
                                        }
                                        rt.push((
                                            x,
                                            y,
                                            z,
                                            h,
                                            r0.trim_start_matches("minecraft:").to_string(),
                                        ));
                                    }
                                    let (m1, m0) =
                                        (db::name(w.get(x, y, z)), db::name(w.get(x, y - 1, z)));
                                    if m1.ends_with(":log") && !m0.ends_with(":log") {
                                        let mut h = 0;
                                        while db::name(w.get(x, y + h, z)).ends_with(":log") {
                                            h += 1;
                                        }
                                        mt.push((
                                            x,
                                            y,
                                            z,
                                            h,
                                            m0.trim_start_matches("minecraft:").to_string(),
                                        ));
                                    }
                                }
                            }
                        }
                        for t in &rt {
                            let tag = if mt.iter().any(|m| (m.0, m.1, m.2) == (t.0, t.1, t.2)) {
                                "both"
                            } else {
                                "REAL"
                            };
                            eprintln!("      trunk {tag} {:?}", t);
                        }
                        for t in mt
                            .iter()
                            .filter(|m| !rt.iter().any(|t| (m.0, m.1, m.2) == (t.0, t.1, t.2)))
                        {
                            eprintln!("      trunk MODEL {:?}", t);
                        }
                        for (n, x, y, z, _) in w.trace.clone() {
                            if (bx..bx + 16).contains(&x)
                                && (bz..bz + 16).contains(&z)
                                && n.starts_with("bop")
                                && !n.contains("flora")
                                && !n.contains("grass")
                                && !n.contains("double")
                                && !n.contains("reeds")
                                && !n.contains("blob")
                            {
                                eprintln!(
                                    "      attempt {n} ({x}, {y}, {z}) real@y {} real@y-1 {}",
                                    names[area.block(x, y, z)],
                                    names[area.block(x, y - 1, z)]
                                );
                            }
                        }
                    }
                    if let Ok(c) = std::env::var("KELP_CHUNK") {
                        let v: Vec<i32> = c.split(',').map(|t| t.parse().unwrap()).collect();
                        let (bx, bz) = (v[0] * 16, v[1] * 16);
                        let (mut rc, mut mc) = (Vec::new(), Vec::new());
                        for x in bx..bx + 16 {
                            for z in bz..bz + 16 {
                                let rk = (Y0..=62)
                                    .filter(|&y| names[area.block(x, y, z)].ends_with("coral1"))
                                    .collect::<Vec<_>>();
                                let mk = (Y0..=62)
                                    .filter(|&y| db::name(w.get(x, y, z)).ends_with("coral1"))
                                    .collect::<Vec<_>>();
                                if !rk.is_empty() {
                                    rc.push((x - bx, z - bz, rk[0], rk.len()));
                                }
                                if !mk.is_empty() {
                                    mc.push((x - bx, z - bz, mk[0], mk.len()));
                                }
                            }
                        }
                        eprintln!("      kelp real {rc:?}\n      kelp model {mc:?}");
                        let floor = |w: &mut DecoWorld, x: i32, z: i32| {
                            (0..63)
                                .rev()
                                .find(|&y| {
                                    !db::name(w.get(x, y, z)).ends_with("water")
                                        && !db::name(w.get(x, y, z)).ends_with("coral1")
                                })
                                .unwrap_or(-1)
                        };
                        let fl: Vec<(i32, i32, i32, &str)> = rc
                            .iter()
                            .take(8)
                            .map(|&(x, z, y, _)| {
                                (
                                    x,
                                    z,
                                    floor(&mut w, bx + x, bz + z),
                                    names[area.block(bx + x, y - 1, bz + z)].as_str(),
                                )
                            })
                            .collect();
                        eprintln!("      model floor under real kelp (x, z, model floor y, real block below) {fl:?}");
                    }
                    phase_deltas(&segs, &w.phases);
                }
                if vn == "real" {
                    for (b, sc) in by.iter().filter(|e| e.1.real + e.1.model > 0) {
                        eprintln!("      {b:28} {}", sc.fmt());
                    }
                    // entry-state diagnostics
                    let (mut same, mut known, mut off) = (0, 0, Vec::new());
                    let mut bop_off = Vec::new();
                    for st in &steps {
                        let seg = segs.iter().find(|s| (s.0, s.1) == (st.cx, st.cz));
                        if let Some(e) = entries.get(&(st.cx, st.cz)) {
                            known += 1;
                            if *e == st.model_entry {
                                same += 1;
                            } else {
                                off.push(signed_distance(st.model_entry, *e, 200_000));
                                if std::env::var("DECO_OFF").is_ok() {
                                    eprintln!(
                                        "        off ({}, {}) prefix_modelled {} dist {:?}",
                                        st.cx,
                                        st.cz,
                                        st.prefix_modelled,
                                        signed_distance(st.model_entry, *e, 200_000)
                                    );
                                }
                            }
                            // natives before the BOP decorator: real draws (entry → decPre) vs model
                            if let Some((bop, _)) = seg.and_then(|s| bop_phases(&s.3)) {
                                let real_n = lcg_distance(*e, bop, 200_000);
                                let model_n = st
                                    .biomes
                                    .iter()
                                    .find(|b| p.table.info(b.0 as _).class.contains("Support"))
                                    .and_then(|b| lcg_distance(st.entry, b.2, 200_000));
                                bop_off.push((st.cx, st.cz, real_n, model_n));
                            }
                        }
                        if per_chunk {
                            eprintln!(
                                "      chunk ({}, {}) modelled {} model entry {:012x} recorded {:?} biomes {:?}",
                                st.cx,
                                st.cz,
                                st.prefix_modelled,
                                st.model_entry,
                                entries.get(&(st.cx, st.cz)).map(|e| format!("{e:012x}")),
                                st.biomes.iter().map(|b| (p.table.info(b.0 as _).name.clone(), b.1, b.3)).collect::<Vec<_>>()
                            );
                        }
                    }
                    eprintln!("      loop entry: model == recorded {same}/{known}; off by (draws, model→real) {off:?}");
                    if !bop_off.is_empty() {
                        let diffs: Vec<String> = bop_off
                            .iter()
                            .map(|(cx, cz, r, m)| match (r, m) {
                                (Some(r), Some(m)) => {
                                    format!("({cx},{cz}) {}", *r as i64 - *m as i64)
                                }
                                _ => format!("({cx},{cz}) real {r:?} model {m:?}"),
                            })
                            .collect();
                        eprintln!(
                            "      draws before the BOP decorator, real − model: {}",
                            diffs.join(", ")
                        );
                    }
                }
            }
            // real draws between BOP decorator phases (median per Decorate type)
            let mut phase: BTreeMap<String, Vec<u32>> = BTreeMap::new();
            for s in &segs {
                if let Some((start, ph)) = bop_phases(&s.3) {
                    for (ty, st) in ph {
                        if let Some(n) = lcg_distance(start, st, 2_000_000) {
                            phase.entry(ty).or_default().push(n);
                        }
                    }
                }
            }
            for (ty, mut v) in phase {
                v.sort();
                eprintln!(
                    "    BOP phase {ty:14} draws from decPre: median {} min {} max {} (n {})",
                    v[v.len() / 2],
                    v[0],
                    v[v.len() - 1],
                    v.len()
                );
            }
            let _ = area.height(area.cx0 * 16, area.cz0 * 16);
        }
    }
}

#[test]
fn state_helpers() {
    let mut r = rwg2_biomes::java_random::JavaRandom::new(42);
    let s0 = r.state();
    let (a, b) = (r.next_int(), r.next_int());
    assert_eq!(from_outputs(a, b), Some(s0));
    assert_eq!(
        parse_state(&format!("s:{s0:x}|c:x|f:shared=o:{a},{b}")),
        Some(s0)
    );
    assert_eq!(shared_state(&format!("s:1|f:shared=o:{a},{b}")), Some(s0));
    assert_eq!(lcg_distance(s0, r.state(), 10), Some(2));
    assert_eq!(signed_distance(r.state(), s0, 10), Some(-2));
}

/// Self-check of the bin layout + scoring: a synthetic area written in the Java writer's layout from a canonical
/// replay must score 1.0 / 1.0 against the same replay; segment parsing of a nested event log.
#[test]
fn selftest_layout_and_segments() {
    let table = BiomeTable::from_rwg2dump_json(
        &std::fs::read_to_string(root().join(pack_rel("biomes.json"))).unwrap(),
    )
    .unwrap();
    let p = BiomeProvider::new(
        1631302684446519836,
        WorldType::Rwg2Continents,
        table,
        RwgConfig::default(),
    );
    let h = TableHeights;
    let g = gates();
    let (cx0, cz0, w) = (-26, 22, 2);
    let mut order = Vec::new();
    for cz in cz0 - 1..cz0 + w {
        for cx in cx0 - 1..cx0 + w {
            order.push((cx, cz));
        }
    }
    let (mut world, _) = replay_ordered(&p, &h, &g, &order, &|_, _| None, &|_, _| None);
    let mut names: Vec<String> = Vec::new();
    let mut idx = HashMap::new();
    let nc = (w * w) as usize;
    let mut bin = vec![0u8; nc * 256 * NY * 3 + nc * 256 + nc * 128 * 3];
    let (mut ido, mut mo) = (0, nc * 256 * NY * 2);
    for cz in cz0..cz0 + w {
        for cx in cx0..cx0 + w {
            for lz in 0..16 {
                for lx in 0..16 {
                    for y in Y0..=Y1 {
                        let n = db::name(world.get(cx * 16 + lx, y, cz * 16 + lz)).to_string();
                        let k = *idx.entry(n.clone()).or_insert_with(|| {
                            names.push(n);
                            names.len() - 1
                        });
                        bin[ido] = k as u8;
                        bin[ido + 1] = (k >> 8) as u8;
                        ido += 2;
                        bin[mo] = 0;
                        mo += 1;
                    }
                }
            }
        }
    }
    let area = Area {
        name: "self".into(),
        cx0,
        cz0,
        w,
        bin,
        ev: Vec::new(),
    };
    let (mut w2, _) = replay_ordered(&p, &h, &g, &order, &|_, _| None, &|_, _| None);
    let s = score(&area, &names, &mut w2, &p, &mut BTreeMap::new());
    eprintln!("selftest {}", s.fmt());
    assert!(s.real > 0 && s.both == s.real && s.both == s.model && s.both_name == s.both);
    // nested populate: (1,0) starts inside (0,0)
    let mk = |k: &str, cx, cz, d| Ev {
        kind: k.into(),
        cx,
        cz,
        depth: d,
        ty: String::new(),
        rand: None,
    };
    let ev = vec![
        mk("popPre", 0, 0, 1),
        mk("decorate", 0, 0, 1),
        mk("popPre", 1, 0, 2),
        mk("decorate", 1, 0, 2),
        mk("popPost", 1, 0, 2),
        mk("decPre", 0, 0, 1),
        mk("popPost", 0, 0, 1),
    ];
    let s = segments(&ev);
    assert_eq!(
        s.iter()
            .map(|x| (x.0, x.1, x.2, x.3.len()))
            .collect::<Vec<_>>(),
        vec![(0, 0, 1, 4), (1, 0, 2, 3)]
    );
}

/// Mineshaft chest hits in dump 2.16: real draws per hit chunk (recorded LAKE-event state − the model's state after
/// the structure pieces with zero chest draws), with each hit's placement (inside the chunk box, no cobweb).
#[test]
fn mineshaft_chest_draws_216() {
    let dirs = dumps("deco216.json");
    if dirs.is_empty() {
        return;
    }
    let table = BiomeTable::from_rwg2dump_json(
        &std::fs::read_to_string(root().join(pack_rel("biomes.json"))).unwrap(),
    )
    .unwrap();
    let g = gates();
    for d in dirs {
        let j: Value =
            serde_json::from_str(&std::fs::read_to_string(d.join("deco216.json")).unwrap())
                .unwrap();
        let Some(seed) = j["worldSeed"].as_str().and_then(|s| s.parse::<i64>().ok()) else {
            continue;
        };
        let p = BiomeProvider::new(
            seed,
            WorldType::Rwg2Continents,
            table.clone(),
            RwgConfig::default(),
        );
        let h = TableHeights;
        let (mut n_hits, mut n_chunks, mut n_ok, mut n_full) = (0, 0, 0, 0);
        for a in j["areas"].as_array().into_iter().flatten() {
            let name = a["name"].as_str().unwrap_or("?");
            let ev = events(a);
            let segs = segments(&ev);
            let (cx0, cz0, w) = (
                a["chunkX0"].as_i64().unwrap() as i32,
                a["chunkZ0"].as_i64().unwrap() as i32,
                a["chunks"].as_i64().unwrap() as i32,
            );
            let mut m = rwg2_biomes::population::PopModel::new(&p, &h, seed);
            m.gates = g.clone();
            m.load_structures(cx0 - 2, cz0 - 2, cx0 + w, cz0 + w);
            m.chest_probe = Some(Vec::new());
            for s in &segs {
                let Some(lake) =
                    s.3.iter()
                        .find(|e| e.ty == "LAKE")
                        .and_then(|e| e.rand.as_deref().and_then(parse_state))
                else {
                    continue;
                };
                m.chest_probe = Some(Vec::new());
                let _ = m.populate(s.0, s.1);
                if m.chest_hits.is_empty() {
                    continue;
                }
                n_chunks += 1;
                n_hits += m.chest_hits.len();
                let first = m.chest_hits.clone();
                // contents draws per placed chest: search until the state after the pieces is the recorded one
                let mut found: Option<Vec<u32>> = None;
                let placed = first.iter().filter(|h| h.3).count();
                if placed == 0 {
                    if m.struct_state == lake {
                        found = Some(Vec::new());
                    }
                } else {
                    m.chest_probe = Some(vec![u32::MAX; 3]);
                    let _ = m.populate(s.0, s.1);
                    if m.struct_state == lake {
                        eprintln!(
                            "    ({}, {}) explained with the placed chests NOT placed",
                            s.0, s.1
                        );
                    }
                    let mut stack = vec![Vec::<u32>::new()];
                    'search: while let Some(pre) = stack.pop() {
                        for c in 0..120 {
                            let mut v = pre.clone();
                            v.push(c);
                            m.chest_probe = Some(v.clone());
                            let _ = m.populate(s.0, s.1);
                            let k = m.chest_hits.iter().filter(|h| h.3).count();
                            if m.struct_state == lake && k == v.len() {
                                found = Some(v);
                                break 'search;
                            }
                            if k > v.len() && v.len() < 3 && pre.is_empty() {
                                stack.push(v);
                            }
                        }
                    }
                }
                if found.is_some() {
                    n_ok += 1;
                }
                // with the embedded 2.17 table: the full model (no probe) must reach the recorded state
                if rwg2_biomes::loot::LootTable::mineshaft().is_some() {
                    m.chest_probe = None;
                    let _ = m.populate(s.0, s.1);
                    let full = m.struct_state == lake;
                    if full {
                        n_full += 1;
                    }
                    eprintln!(
                        "    full model {}",
                        if full { "matches" } else { "differs" }
                    );
                    m.chest_probe = Some(Vec::new());
                }
                eprintln!(
                    "  {name} ({}, {}) hits {:?} contents draws {found:?}",
                    s.0, s.1, first
                );
            }
        }
        eprintln!("{n_chunks} chunks, {n_hits} hits, explained {n_ok}");
        // ≤ 2 chunks per world unexplained (dump 2.16: 2 Kelp Forest chunks where the real game skips a corridor of
        // ≈300 draws the model keeps, liquid test; world 3380…: Jungle (80, −289), Garden (44, 156))
        assert!(n_chunks >= 20 && n_ok + 2 >= n_chunks, "{n_ok}/{n_chunks}");
        if rwg2_biomes::loot::LootTable::mineshaft().is_some() {
            // with the 2.17 table the contents are modelled: every explained chunk must now match unaided
            eprintln!("full model (2.17 table): {n_full}/{n_chunks}");
            assert_eq!(n_full, n_ok, "{n_hits} hits");
        }
    }
}

static ORDER_SIGN: std::sync::atomic::AtomicI64 = std::sync::atomic::AtomicI64::new(1);
fn order_key(s: (i32, i32)) -> i64 {
    // any total order; the sign flips it (two starts: both permutations)
    ORDER_SIGN.load(std::sync::atomic::Ordering::Relaxed)
        * (((s.0 as i64) << 32) | (s.1 as u32 as i64))
}

/// Chunks two mineshaft starts reach in dump 2.16: which processing order of the starts gives the recorded
/// LAKE-event state (contents draws of placed chests searched as in `mineshaft_chest_draws_216`).
#[test]
fn mineshaft_pair_order_216() {
    let dirs = dumps("deco216.json");
    if dirs.is_empty() {
        return;
    }
    let table = BiomeTable::from_rwg2dump_json(
        &std::fs::read_to_string(root().join(pack_rel("biomes.json"))).unwrap(),
    )
    .unwrap();
    let g = gates();
    for d in dirs {
        let j: Value =
            serde_json::from_str(&std::fs::read_to_string(d.join("deco216.json")).unwrap())
                .unwrap();
        let Some(seed) = j["worldSeed"].as_str().and_then(|s| s.parse::<i64>().ok()) else {
            continue;
        };
        let p = BiomeProvider::new(
            seed,
            WorldType::Rwg2Continents,
            table.clone(),
            RwgConfig::default(),
        );
        let h = TableHeights;
        let mut unexplained = 0;
        for a in j["areas"].as_array().into_iter().flatten() {
            let name = a["name"].as_str().unwrap_or("?");
            let ev = events(a);
            let segs = segments(&ev);
            let (cx0, cz0, w) = (
                a["chunkX0"].as_i64().unwrap() as i32,
                a["chunkZ0"].as_i64().unwrap() as i32,
                a["chunks"].as_i64().unwrap() as i32,
            );
            let mut m = rwg2_biomes::population::PopModel::new(&p, &h, seed);
            m.gates = g.clone();
            m.load_structures(cx0 - 2, cz0 - 2, cx0 + w, cz0 + w);
            for s in &segs {
                let Some(lake) =
                    s.3.iter()
                        .find(|e| e.ty == "LAKE")
                        .and_then(|e| e.rand.as_deref().and_then(parse_state))
                else {
                    continue;
                };
                m.shaft_order = None;
                m.chest_probe = Some(Vec::new());
                if !matches!(
                    m.populate(s.0, s.1),
                    rwg2_biomes::population::Outcome::Uncertain("structure piece", _)
                ) {
                    continue;
                }
                let starts = m.overlapping_starts(s.0, s.1);
                if starts.len() < 2 {
                    continue;
                }
                let mut res = Vec::new();
                for sign in [1i64, -1] {
                    ORDER_SIGN.store(sign, std::sync::atomic::Ordering::Relaxed);
                    m.shaft_order = Some(order_key);
                    let mut ok = None;
                    'c: for c0 in 0..100u32 {
                        for c1 in 0..100u32 {
                            m.chest_probe = Some(vec![c0, c1]);
                            let _ = m.populate(s.0, s.1);
                            let placed = m.chest_hits.iter().filter(|h| h.3).count();
                            if m.struct_state == lake {
                                ok = Some((placed, c0, c1));
                                break 'c;
                            }
                            if placed < 2 {
                                break;
                            }
                        }
                        let placed = m.chest_hits.iter().filter(|h| h.3).count();
                        if placed == 0 {
                            break;
                        }
                    }
                    res.push((sign, ok));
                }
                eprintln!(
                    "  {name} ({}, {}) starts {starts:?} asc/desc {res:?}",
                    s.0, s.1
                );
                // the modelled order (fastutil slot order, `shaft_scan_order`) must explain every pair
                let model_asc = shaft_scan_order(starts[0]).min(shaft_scan_order(starts[1])) == {
                    let mut v = starts.clone();
                    v.sort();
                    shaft_scan_order(v[0])
                };
                let want = if model_asc { 1 } else { -1 };
                if res.iter().all(|(_, ok)| ok.is_none()) {
                    // neither order explains it: not an order question (the chest-draw test misses it too)
                    unexplained += 1;
                    continue;
                }
                assert!(
                    res.iter().any(|(s, ok)| *s == want && ok.is_some()),
                    "model order fails at ({}, {})",
                    s.0,
                    s.1
                );
            }
        }
        eprintln!("pairs explained by neither order: {unexplained}");
        assert!(unexplained <= 2, "{unexplained} pair chunks unexplained");
    }
}

/// Diagnostic (ignored): for every dump-2.16 populate whose loop entry the model misses, where it diverges
/// (after the structure pieces = LAKE event, or later) and why the model gave up.
#[test]
#[ignore]
fn entry_misses_216() {
    let dirs = dumps("deco216.json");
    let table = BiomeTable::from_rwg2dump_json(
        &std::fs::read_to_string(root().join(pack_rel("biomes.json"))).unwrap(),
    )
    .unwrap();
    let g = gates();
    for d in dirs {
        let j: Value =
            serde_json::from_str(&std::fs::read_to_string(d.join("deco216.json")).unwrap())
                .unwrap();
        let Some(seed) = j["worldSeed"].as_str().and_then(|s| s.parse::<i64>().ok()) else {
            continue;
        };
        let p = BiomeProvider::new(
            seed,
            WorldType::Rwg2Continents,
            table.clone(),
            RwgConfig::default(),
        );
        let h = TableHeights;
        for a in j["areas"].as_array().into_iter().flatten() {
            let name = a["name"].as_str().unwrap_or("?");
            let ev = events(a);
            let segs = segments(&ev);
            let (cx0, cz0, w) = (
                a["chunkX0"].as_i64().unwrap() as i32,
                a["chunkZ0"].as_i64().unwrap() as i32,
                a["chunks"].as_i64().unwrap() as i32,
            );
            let mut m = rwg2_biomes::population::PopModel::new(&p, &h, seed);
            m.gates = g.clone();
            m.load_structures(cx0 - 2, cz0 - 2, cx0 + w, cz0 + w);
            m.stateful_shafts = true;
            m.stateful_world = std::env::var("NO_WORLD_MEM").is_err();
            for s in &segs {
                if !(cx0 - 1..cx0 + w).contains(&s.0) || !(cz0 - 1..cz0 + w).contains(&s.1) {
                    continue;
                }
                let Some(entry) = loop_entry(&s.3) else {
                    continue;
                };
                let lake =
                    s.3.iter()
                        .find(|e| e.ty == "LAKE")
                        .and_then(|e| e.rand.as_deref().and_then(parse_state));
                m.struct_state = 0;
                let out = m.populate(s.0, s.1);
                let me = m.last_deco_start.as_ref().map(|r| r.0.state());
                if me == Some(entry) && std::env::var("PRE_ALL").is_err() {
                    continue;
                }
                let g8 = p.biome_gen_id(s.0 * 16 + 8, s.1 * 16 + 8, &h);
                let cj: Value = serde_json::from_str(
                    &std::fs::read_to_string(root().join(pack_rel("climate.json"))).unwrap(),
                )
                .unwrap();
                let nm = |id: u16| {
                    cj["biomes"]
                        .as_array()
                        .unwrap()
                        .iter()
                        .find(|b| b["id"].as_u64() == Some(id as u64))
                        .map(|b| format!("{} {}", b["name"], b["dict"]))
                        .unwrap_or_default()
                };
                let mut t2 = rwg2_biomes::dungeons::Terrain::new(&p, &h);
                let cg = t2
                    .column(s.0 * 16 + 8, s.1 * 16 + 8)
                    .map(|c| c.1)
                    .unwrap_or(0);
                eprintln!(
                    "    provider gen at +8: {g8} {} | chunk array {cg} {}",
                    nm(g8),
                    nm(cg)
                );
                let why = match &out {
                    rwg2_biomes::population::Outcome::Certain(_) => "certain",
                    rwg2_biomes::population::Outcome::Uncertain(w, _) => w,
                };
                let at_lake = lake.map(|l| {
                    (
                        l == m.struct_state,
                        signed_distance(m.pre_state, l, 100_000),
                        signed_distance(m.struct_state, l, 100_000),
                        m.overlapping_starts(s.0, s.1).len(),
                        m.chest_hits.len(),
                    )
                });
                let late =
                    s.3.iter()
                        .find(|e| e.kind == "popPreLate")
                        .and_then(|e| e.rand.as_deref().and_then(parse_state));
                let pre_d = late.and_then(|l| signed_distance(m.pre_state, l, 200_000));
                // per Pre listener: recorded states (populate events of the Pre phase)
                let pre_list: Vec<&Ev> =
                    s.3.iter().take_while(|e| e.kind != "popPreLate").collect();
                let mut pre_ev: Vec<String> = Vec::new();
                for k in 0..pre_list.len() {
                    let a = pre_list[k].rand.as_deref().and_then(parse_state);
                    let b = if k + 1 < pre_list.len() {
                        pre_list[k + 1].rand.as_deref().and_then(parse_state)
                    } else {
                        late
                    };
                    if let (Some(a), Some(b)) = (a, b) {
                        pre_ev.push(format!(
                            "{}:{:?}",
                            pre_list[k].ty,
                            signed_distance(a, b, 10_000)
                        ));
                    }
                }
                let mut phases = Vec::new();
                for (tag, st) in &m.marks {
                    let real =
                        s.3.iter()
                            .find(|e| {
                                (e.kind == "populate" && e.ty == *tag)
                                    || (e.kind == *tag && *tag == "decPre")
                            })
                            .and_then(|e| e.rand.as_deref().and_then(parse_state));
                    phases.push(format!(
                        "{tag}:{:?}",
                        real.and_then(|r| signed_distance(*st, r, 100_000))
                    ));
                }
                eprintln!(
                    "    phases model→real {phases:?} dungeons {} lakes {} river {}",
                    m.last_dungeons.len(),
                    m.last_lakes.len(),
                    -p.river_strength(s.0 * 16 + 16, s.1 * 16 + 16)
                );
                let types: Vec<&str> =
                    s.3.iter()
                        .filter(|e| e.kind == "populate")
                        .map(|e| e.ty.as_str())
                        .collect();
                eprintln!(
                    "  {name} ({}, {}) {why} flags {:#x} pre dist {pre_d:?} {pre_ev:?} lake-state ok {at_lake:?} entry dist {:?} events {:?}",
                    s.0,
                    s.1,
                    m.flags,
                    me.and_then(|me| signed_distance(me, entry, 200_000)),
                    types
                );
            }
        }
    }
}

/// RWG2 populate tail ANIMALS → ICE (`deco::post_tail`) vs the recorded states of every traced ocean chunk
/// (rwg2dump 2.16 `deco216.json` ocean areas, 2.18 `probe218.json` oceans): ANIMALS draws by the chunk biome at
/// +16, +16, and the modelled state at ICE.
#[test]
fn post_tail_vs_recorded() {
    let table = BiomeTable::from_rwg2dump_json(
        &std::fs::read_to_string(root().join(pack_rel("biomes.json"))).unwrap(),
    )
    .unwrap();
    let mut files: Vec<(PathBuf, &str)> = dumps("deco216.json").into_iter().map(|d| (d, "deco216.json")).collect();
    files.extend(dumps("probe218.json").into_iter().map(|d| (d, "probe218.json")));
    files.extend(dumps("probe219.json").into_iter().map(|d| (d, "probe219.json")));
    if files.is_empty() {
        eprintln!("no traced ocean chunks: skipped");
        return;
    }
    let lcg = |a: i64, b: i64, lim: u32| -> Option<u32> {
        let mut s = a;
        for n in 0..=lim {
            if s == b {
                return Some(n);
            }
            s = (s.wrapping_mul(M).wrapping_add(0xB)) & MASK;
        }
        None
    };
    let (mut ok, mut all) = (0, 0);
    let mut anim: BTreeMap<(u16, Option<u32>), u32> = BTreeMap::new();
    let mut anim_ok: BTreeMap<u16, (u32, u32)> = BTreeMap::new();
    for (d, f) in files {
        let j: Value = serde_json::from_str(&std::fs::read_to_string(d.join(f)).unwrap()).unwrap();
        let Some(seed) = j["worldSeed"].as_str().and_then(|s| s.parse::<i64>().ok()) else { continue };
        let p = BiomeProvider::new(seed, WorldType::Rwg2Continents, table.clone(), RwgConfig::default());
        let h = TableHeights;
        let areas = if f.starts_with("probe") { &j["oceans"]["areas"] } else { &j["areas"] };
        for a in areas.as_array().into_iter().flatten() {
            let name = a["name"].as_str().unwrap_or("");
            if !name.starts_with("ocean") {
                continue;
            }
            let mut w = DecoWorld::new(&p, &h, seed);
            type Ev = (String, String, i64);
            let mut by: BTreeMap<(i32, i32), Vec<Ev>> = BTreeMap::new();
            for e in a["events"].as_array().into_iter().flatten() {
                let (Some(cx), Some(cz)) = (e[2].as_i64(), e[3].as_i64()) else { continue };
                let Some(st) = e[6].as_str().and_then(parse_state) else { continue };
                if e[6].as_str().is_some_and(|s| s.contains("|c:")) {
                    continue;
                }
                by.entry((cx as i32, cz as i32)).or_default().push((
                    e[1].as_str().unwrap_or("").to_string(),
                    e[5].as_str().unwrap_or("").to_string(),
                    st,
                ));
            }
            for ((cx, cz), ev) in by {
                let find = |k: &str, t: &str| ev.iter().find(|e| e.0 == k && e.1 == t).map(|e| e.2);
                let (Some(an), Some(pp), Some(ice)) =
                    (find("populate", "ANIMALS"), find("popPost", ""), find("populate", "ICE"))
                else {
                    continue;
                };
                let g = w.gen_id(cx * 16 + 16, cz * 16 + 16);
                *anim.entry((g, lcg(an, pp, 64))).or_default() += 1;
                {
                    let mut ra = rwg2_biomes::java_random::JavaRandom::new(an ^ M);
                    let (c, wet) = rwg2_biomes::deco::world_gen_spawning(&mut w, &mut ra, cx, cz);
                    let e = anim_ok.entry(g).or_insert((0, 0));
                    e.1 += 1;
                    if ra.state() == pp {
                        e.0 += 1;
                    } else if std::env::var("POST_MISS").is_ok() {
                        eprintln!("  animals {name} {cx},{cz} gen {g} real {:?} model {:?} conf {c} water {wet}", lcg(an, pp, 64), lcg(an, ra.state(), 200));
                        // which group sizes reproduce the recorded state (raw size draw u per group)
                        for seq in 0..8u32.pow(3) {
                            let ns = [seq % 8 + 1, seq / 8 % 8 + 1, seq / 64 + 1];
                            let mut us = Vec::new();
                            let mut rb = rwg2_biomes::java_random::JavaRandom::new(an ^ M);
                            rwg2_biomes::deco::world_gen_spawning_with(&mut w, &mut rb, cx, cz, &mut |gi, _, r| {
                                us.push(r.next(31));
                                ns[(gi as usize).min(2)] as i32
                            });
                            if rb.state() == pp {
                                let k = us.len();
                                eprintln!("    fits: sizes {:?} raw u {us:?} (u%3 {:?})", &ns[..k], us.iter().map(|u| u % 3).collect::<Vec<_>>());
                                break;
                            }
                        }
                    }
                }
                let mut r = rwg2_biomes::java_random::JavaRandom::new(an ^ M);
                rwg2_biomes::deco::post_tail(&mut w, &mut r, cx, cz);
                all += 1;
                if r.state() == ice {
                    ok += 1;
                } else if std::env::var("POST_MISS").is_ok() {
                    eprintln!("  miss {name} {cx},{cz} gen+16 {g} anim {:?}", lcg(an, pp, 64));
                }
            }
        }
    }
    eprintln!("ANIMALS draws by chunk biome at +16,+16 (gen, draws): {anim:?}");
    eprintln!("ANIMALS end state == recorded popPost, by gen (ok, all): {anim_ok:?}");
    eprintln!("post_tail state at ICE == recorded: {ok}/{all}");
}

/// DreamMaster oil chain vs the recorded Random states of every traced populate (dump 2.16 runs): Galacticraft
/// (popPost → 2nd CUSTOM), hives (25 / 33 draws), DreamMaster (its CUSTOM → ICE, deposits), and the model's
/// ANIMALS entry (decoration exit) from the real load-order replay. `OIL_LIST=1` prints deposits / misses.
#[test]
fn oil_chain_vs_recorded() {
    use rwg2_biomes::deco::oil;
    use rwg2_biomes::java_random::JavaRandom;
    let table = BiomeTable::from_rwg2dump_json(
        &std::fs::read_to_string(root().join(pack_rel("biomes.json"))).unwrap(),
    )
    .unwrap();
    let files = dumps("deco216.json");
    if files.is_empty() {
        eprintln!("no deco216 dumps: skipped");
        return;
    }
    let list = std::env::var("OIL_LIST").is_ok();
    let g = gates();
    // [gc ok, gc all, hive 25, hive 33, hive other, dm ok, dm all, deposits]
    let mut c = [0u32; 8];
    // per area name: (land chunks, model ANIMALS entry exact, exact & animals roll ≥ 0.1, real ANIMALS 1 draw)
    let mut per: BTreeMap<String, [u32; 4]> = BTreeMap::new();
    // model deposit vs real (even chunks with a modelled step): [both same, real only, model only, both differ]
    let mut dep = [0u32; 4];
    for d in files {
        let j: Value = serde_json::from_str(&std::fs::read_to_string(d.join("deco216.json")).unwrap()).unwrap();
        let Some(seed) = j["worldSeed"].as_str().and_then(|s| s.parse::<i64>().ok()) else { continue };
        let p = BiomeProvider::new(seed, WorldType::Rwg2Continents, table.clone(), RwgConfig::default());
        let h = TableHeights;
        for a in j["areas"].as_array().into_iter().flatten() {
            let name = a["name"].as_str().unwrap_or("?").to_string();
            let ev = events(a);
            if ev.is_empty() {
                continue;
            }
            let (cx0, cz0, wd) = (
                a["chunkX0"].as_i64().unwrap() as i32,
                a["chunkZ0"].as_i64().unwrap() as i32,
                a["chunks"].as_i64().unwrap() as i32,
            );
            let (x0, z0, x1, z1) = (cx0 - 1, cz0 - 1, cx0 + wd - 1, cz0 + wd - 1);
            let order: Vec<(i32, i32)> = segments(&ev)
                .iter()
                .filter(|s| (x0..=x1).contains(&s.0) && (z0..=z1).contains(&s.1))
                .map(|s| (s.0, s.1))
                .collect();
            let (mut w, steps) = replay_ordered(&p, &h, &g, &order, &|_, _| None, &|_, _| None);
            let steps: BTreeMap<(i32, i32), _> = steps.into_iter().map(|s| ((s.cx, s.cz), s)).collect();
            let mut by: BTreeMap<(i32, i32), Vec<&Ev>> = BTreeMap::new();
            for e in &ev {
                if e.rand.as_deref().is_some_and(|s| s.contains("|c:")) {
                    continue;
                }
                by.entry((e.cx, e.cz)).or_default().push(e);
            }
            for ((cx, cz), es) in by {
                let st = |k: &str, t: &str| {
                    es.iter().find(|e| e.kind == k && e.ty == t).and_then(|e| e.rand.as_deref()).and_then(parse_state)
                };
                let customs: Vec<i64> = es
                    .iter()
                    .filter(|e| e.kind == "populate" && e.ty == "CUSTOM")
                    .filter_map(|e| e.rand.as_deref().and_then(parse_state))
                    .collect();
                let (Some(an), Some(pp), Some(hv), Some(ice)) = (
                    st("populate", "ANIMALS"),
                    st("popPost", ""),
                    st("populate", "FORESTRY_HIVES"),
                    st("populate", "ICE"),
                ) else {
                    continue;
                };
                let even = cx % 2 == 0 && cz % 2 == 0;
                if customs.len() < 2 + even as usize {
                    continue;
                }
                let mut gen = |x: i32, z: i32| w.gen_id(x, z);
                // Galacticraft
                let mut r = JavaRandom::new(pp ^ M);
                oil::gc(&mut r, seed, &mut gen, cx, cz);
                c[1] += 1;
                c[0] += (r.state() == customs[1]) as u32;
                // hives
                let after_hives = if even { customs[2] } else { ice };
                let k = [false, true].iter().position(|&hit| {
                    let mut r = JavaRandom::new(hv ^ M);
                    oil::hives(&mut r, hit);
                    r.state() == after_hives
                });
                c[2 + k.unwrap_or(2)] += 1;
                // DreamMaster
                let mut real = None;
                if even {
                    let mut r = JavaRandom::new(customs[2] ^ M);
                    real = oil::dm(&mut r, seed, &mut gen, &rwg2_biomes::deco::oil_row, cx, cz);
                    c[6] += 1;
                    c[5] += (r.state() == ice) as u32;
                    if r.state() != ice && std::env::var("DM_MISS").is_ok() {
                        let g = w.gen_id(cx * 16 + 8, cz * 16 + 8);
                        eprintln!("  DM miss {name} chunk {cx},{cz} gen {g} model {real:?}");
                    }
                    if real.is_some() {
                        c[7] += 1;
                    }
                    if let (true, Some(r)) = (list, real) {
                        eprintln!("  deposit {name} chunk {cx},{cz}: {r:?}");
                    }
                }
                // model chain
                let Some(s) = steps.get(&(cx, cz)) else { continue };
                if name.starts_with("ocean") {
                    continue;
                }
                let e = per.entry(name.clone()).or_default();
                e[0] += 1;
                let exact = s.post_entry == an;
                e[1] += exact as u32;
                let roll_ok = JavaRandom::new(an ^ M).next_float() >= 0.1;
                e[2] += (exact && roll_ok) as u32;
                e[3] += roll_ok as u32;
                if even {
                    let model = s.oil.map(|o| o.0);
                    let i = match (real, model) {
                        (Some(a), Some(b)) if a == b => 0,
                        (Some(_), None) => 1,
                        (None, Some(_)) => 2,
                        (Some(_), Some(_)) => 3,
                        (None, None) => continue,
                    };
                    dep[i] += 1;
                    if list {
                        eprintln!("  {name} {cx},{cz}: real {real:?} model {model:?} (entry exact {exact})");
                    }
                }
            }
        }
    }
    eprintln!(
        "GC listener {}/{}; hives 25 draws {}, 33 draws {}, other {}; DreamMaster {}/{} exact, {} deposits",
        c[0], c[1], c[2], c[3], c[4], c[5], c[6], c[7]
    );
    eprintln!("model ANIMALS entry exact (land, real load order), per area: [chunks, exact, exact & no spawn, no spawn]");
    let mut tot = [0u32; 4];
    for (n, v) in &per {
        eprintln!("  {n:<26} {v:?}");
        for i in 0..4 {
            tot[i] += v[i];
        }
    }
    eprintln!("  total {tot:?}");
    eprintln!("deposits model vs real (even chunks): same {}, real only {}, model only {}, differ {}", dep[0], dep[1], dep[2], dep[3]);
    assert_eq!(c[0], c[1], "Galacticraft listener");
    assert_eq!(c[5], c[6], "DreamMaster listener");
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
