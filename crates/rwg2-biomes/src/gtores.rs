//! GregTech 5.09.54 (GTNH 2.9.0) ore veins: which vein each 3×3-chunk oreseed gets, its bottom Y and extents.
//!
//! `GTWorldgenerator.WorldGenContainer.generateVein` (pure seed maths except the placement test):
//! - oreseeds: chunks with `floorMod(c, 3) == 1` on both axes (EQUAL_SPACING, every world created since 5.09.43)
//! - `oreveinSeed = (worldSeed << 16) ^ ((dim & 0xff) << 56 | (oreseedX & 0x0fffffff) << 28 | (oreseedZ & 0x0fffffff))`
//! - `XSTR(oreveinSeed).nextInt(100) < oreveinPercentage` (100 in GTNH) → up to `oreveinAttempts` (64) tries,
//!   at most `oreveinMaxPlacementAttempts` (8) failed placements:
//!   `seed = fnv1a64(fnv1a64(OFFSET, oreveinSeed), i)`, weighted pick over the dimension's veins (registry order),
//!   `placementSeed = fnv1a64(seed, primary.getId())` → `veinMinY = minY + nextInt(maxY − minY − 5)`,
//!   west/east = seedX − nextInt(size) / seedX + 16 + nextInt(size), north/south likewise.
//! - placement test, run once, from the *first* chunk GT processes within ±2 chunks of the oreseed
//!   ([`execute_chunkified`] dry run): no overlap with that chunk (+2 bias) → ≥ 5 of 9 stone blocks at
//!   (chunkX + 7, veinMinY + 0..9, chunkZ + 9); overlap → any ore placeable in the 9 layers (sampled with the vein
//!   RNG). Failure → next attempt. Which chunk comes first is the population order: see `gtorder`
//!   ([`crate::gtorder::resolve_ordered`] / [`crate::gtorder::resolve_ordered_approx`], ~87% right on the
//!   ground truth). [`terrain_stone_ok`] is the older order-free model (column top at the vein-box centre
//!   reaches `veinMinY + 4`, ~85%).

use crate::dungeons::Terrain;
pub use crate::gtorder::{resolve_ordered, resolve_ordered_approx, resolve_ordered_with};

/// GT's `XSTR` (xorshift, *not* java.util.Random semantics for `nextInt`).
#[derive(Clone, Debug)]
pub struct Xstr {
    seed: i64,
}

impl Xstr {
    pub fn new(seed: i64) -> Self {
        Xstr {
            seed: if seed == 0 { 42 } else { seed },
        }
    }

    pub fn set_seed(&mut self, seed: i64) {
        self.seed = if seed == 0 { 42 } else { seed };
    }

    #[inline]
    fn step(&mut self) -> i64 {
        let mut x = self.seed;
        x ^= x << 21;
        x ^= ((x as u64) >> 35) as i64;
        x ^= x << 4;
        self.seed = x;
        x
    }

    /// `nextInt(bound)`: `abs((int) state % bound)`.
    pub fn next_int(&mut self, bound: i32) -> i32 {
        let last = self.step();
        let out = (last as i32) % bound;
        if out < 0 {
            -out
        } else {
            out
        }
    }
}

const FNV_OFFSET: i64 = 0xCBF2_9CE4_8422_2325u64 as i64;
const FNV_PRIME: i64 = 0x100_0000_01B3;

/// GTNHLib `Fnv1a64.hashStep(state, byte)` — note the sign extension of the byte (kept on purpose upstream).
#[inline]
fn fnv_byte(state: i64, b: i8) -> i64 {
    (state ^ b as i64).wrapping_mul(FNV_PRIME)
}

fn fnv_long(mut s: i64, v: i64) -> i64 {
    for shift in (0..8).rev() {
        s = fnv_byte(s, (v >> (shift * 8)) as i8);
    }
    s
}

fn fnv_int(mut s: i64, v: i32) -> i64 {
    for shift in (0..4).rev() {
        s = fnv_byte(s, (v >> (shift * 8)) as i8);
    }
    s
}

#[derive(Clone, Debug)]
pub struct Vein {
    pub name: String,
    pub weight: i32,
    pub min_y: i32,
    pub max_y: i32,
    pub size: i32,
    pub density: i32,
    pub primary_id: i32,
    /// primary, secondary, in-between, sporadic material names
    pub ores: [String; 4],
}

#[derive(Clone, Debug)]
pub struct GtConfig {
    pub percentage: i32,
    pub attempts: i32,
    pub max_placement_attempts: i32,
    /// EQUAL_SPACING (true) or the legacy AXISSYMMETRICAL grid.
    pub equal_spacing: bool,
}

pub const GT_GTNH: GtConfig = GtConfig {
    percentage: 100,
    attempts: 64,
    max_placement_attempts: 8,
    equal_spacing: true,
};

#[derive(Clone, Debug, Default)]
pub struct VeinTable {
    pub overworld: Vec<Vein>,
}

impl VeinTable {
    /// `data/<pack>/gregtech.json`: `{"veins":[{name, weight, minY, maxY, size, density, primaryId, ores:[4]}]}`
    /// (overworld-capable stone veins in registry order).
    #[cfg(feature = "json")]
    pub fn from_json(s: &str) -> Result<Self, String> {
        let v: serde_json::Value = serde_json::from_str(s).map_err(|e| e.to_string())?;
        let mut overworld = Vec::new();
        for x in v["veins"].as_array().ok_or("veins")? {
            let i = |k: &str| x[k].as_i64().map(|n| n as i32).ok_or(k.to_string());
            let ores = x["ores"].as_array().ok_or("ores")?;
            let ore = |n: usize| {
                ores.get(n)
                    .and_then(|o| o.as_str())
                    .unwrap_or("")
                    .to_string()
            };
            overworld.push(Vein {
                name: x["name"].as_str().unwrap_or("").to_string(),
                weight: i("weight")?,
                min_y: i("minY")?,
                max_y: i("maxY")?,
                size: i("size")?,
                density: i("density")?,
                primary_id: i("primaryId")?,
                ores: [ore(0), ore(1), ore(2), ore(3)],
            });
        }
        Ok(VeinTable { overworld })
    }
}

#[doc(hidden)]
pub fn fnv_offset() -> i64 {
    FNV_OFFSET
}
#[doc(hidden)]
pub fn fnv_long_pub(s: i64, v: i64) -> i64 {
    fnv_long(s, v)
}
#[doc(hidden)]
pub fn fnv_int_pub(s: i64, v: i32) -> i64 {
    fnv_int(s, v)
}

pub fn is_oreseed(cfg: &GtConfig, cx: i32, cz: i32) -> bool {
    if cfg.equal_spacing {
        cx.rem_euclid(3) == 1 && cz.rem_euclid(3) == 1
    } else {
        (cx.abs() % 3) == 1 && (cz.abs() % 3) == 1
    }
}

pub fn orevein_seed(world_seed: i64, dim: i32, ox: i32, oz: i32) -> i64 {
    (world_seed << 16)
        ^ ((((dim as i64) & 0xff) << 56)
            | (((ox as i64) & 0x0fff_ffff) << 28)
            | ((oz as i64) & 0x0fff_ffff))
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Placement {
    pub vein: usize,
    pub min_y: i32,
    pub west: i32,
    pub east: i32,
    pub north: i32,
    pub south: i32,
    /// Picks rejected by the placement test before this one (0 = first pick).
    pub rejected: i32,
}

/// One candidate pick (attempt `i`) without the placement test.
pub fn candidate(
    table: &VeinTable,
    oreveinseed: i64,
    i: i32,
    ox: i32,
    oz: i32,
) -> Option<Placement> {
    candidate_rng(table, oreveinseed, i, ox, oz).map(|c| c.0)
}

/// [`candidate`] plus the vein RNG as GT leaves it after `resolveVeinPlacement` (the placement test's dry run
/// continues drawing from it).
pub fn candidate_rng(
    table: &VeinTable,
    oreveinseed: i64,
    i: i32,
    ox: i32,
    oz: i32,
) -> Option<(Placement, Xstr)> {
    let total: i32 = table.overworld.iter().map(|v| v.weight).sum();
    if total <= 0 {
        return None;
    }
    let seed = fnv_int(fnv_long(FNV_OFFSET, oreveinseed), i);
    let mut rng = Xstr::new(seed);
    let mut rem = rng.next_int(total);
    let mut pick = 0;
    for (k, v) in table.overworld.iter().enumerate() {
        rem -= v.weight;
        if rem < 0 {
            pick = k;
            break;
        }
    }
    let v = &table.overworld[pick];
    rng.set_seed(fnv_int(seed, v.primary_id));
    let (sx, sz) = (ox * 16, oz * 16);
    let min_y = v.min_y + rng.next_int(v.max_y - v.min_y - 5);
    let west = sx - rng.next_int(v.size);
    let east = sx + 16 + rng.next_int(v.size);
    let north = sz - rng.next_int(v.size);
    let south = sz + 16 + rng.next_int(v.size);
    Some((
        Placement {
            vein: pick,
            min_y,
            west,
            east,
            north,
            south,
            rejected: 0,
        },
        rng,
    ))
}

/// The vein an oreseed ends up with. `stone_ok(placement)` is the placement test (terrain model or exact).
pub fn resolve(
    world_seed: i64,
    table: &VeinTable,
    cfg: &GtConfig,
    ox: i32,
    oz: i32,
    mut stone_ok: impl FnMut(&Placement) -> bool,
) -> Option<Placement> {
    let ovs = orevein_seed(world_seed, 0, ox, oz);
    let mut roll = Xstr::new(ovs);
    if roll.next_int(100) >= cfg.percentage {
        return None;
    }
    let mut failed = 0;
    let mut i = 0;
    while i < cfg.attempts && failed < cfg.max_placement_attempts {
        let mut p = candidate(table, ovs, i, ox, oz)?;
        if stone_ok(&p) {
            p.rejected = failed;
            return Some(p);
        }
        failed += 1;
        i += 1;
    }
    None
}

/// The runner-up for the tooltip: the first pick if `chosen` came after it (the model may have rejected the real vein),
/// else the pick after `chosen` (the real placement test may have failed where the model passed).
pub fn alternative(
    table: &VeinTable,
    world_seed: i64,
    ox: i32,
    oz: i32,
    chosen: Option<&Placement>,
) -> Option<Placement> {
    let ovs = orevein_seed(world_seed, 0, ox, oz);
    let i = match chosen {
        Some(p) if p.rejected > 0 => 0,
        Some(p) => p.rejected + 1,
        None => 0,
    };
    candidate(table, ovs, i, ox, oz).filter(|c| chosen.map_or(true, |p| p.vein != c.vein))
}

/// Placement test on the terrain model: stone must reach the vein's bottom layers at the oreseed centre.
pub fn terrain_stone_ok(t: &mut Terrain, p: &Placement) -> bool {
    // the test runs at the first generated chunk near the oreseed; the oreseed's own centre is the best stand-in
    let (x, z) = (
        p.west + (p.east - p.west) / 2,
        p.north + (p.south - p.north) / 2,
    );
    match t.top(x, z) {
        Some(top) => top >= p.min_y + 4,
        None => true,
    }
}

/// Outcome of GT's placement test (`WorldgenGTOreLayer.executeWorldgenChunkified` with `dryRun`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PlacementTest {
    /// The vein overlaps the processing chunk (+2 bias) and the dry run found a block it could turn into ore.
    OrePlaced,
    /// No overlap, but ≥ 5 of the 9 blocks at (chunkX + 7, veinMinY + 0..9, chunkZ + 9) are stone: accepted.
    NoOverlap,
    /// No overlap and < 5 stone blocks there, or the dry run placed nothing: rejected (next attempt).
    NoOverlapAir,
}

impl PlacementTest {
    pub fn accepted(self) -> bool {
        self != PlacementTest::NoOverlapAir
    }
}

/// What the placement code asks the world about a block.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BlockQuery {
    /// `StoneType.findStoneType(..) != null` (the no-overlap stone count).
    Stone,
    /// Could ore slot `k` (0 primary, 1 secondary, 2 in-between, 3 sporadic) go here: stone, or (dry run only)
    /// the same natural ore material already there (`canSetOreForWorldGenOrAlreadySet`).
    Ore(usize),
}

/// `WorldgenGTOreLayer.executeWorldgenChunkified` for processing chunk `(pcx, pcz)` and oreseed `(ox, oz)`.
/// `rng` is the vein RNG from [`candidate_rng`] (`generationSeed`). Dry run: stops at the first block
/// `block(x, y, z, Ore(k))` accepts. Real run: calls `place(x, y, z, k)` for every accepted block.
/// Returns the GT result code.
#[allow(clippy::too_many_arguments)]
pub fn execute_chunkified(
    vein: &Vein,
    p: &Placement,
    mut rng: Xstr,
    ox: i32,
    oz: i32,
    pcx: i32,
    pcz: i32,
    dry_run: bool,
    mut block: impl FnMut(i32, i32, i32, BlockQuery) -> bool,
    mut place: impl FnMut(i32, i32, i32, usize),
) -> PlacementTest {
    let (chunk_x, chunk_z) = (pcx * 16, pcz * 16);
    let limit_west = p.west.max(chunk_x + 2);
    let limit_east = p.east.min(chunk_x + 2 + 16);
    let limit_north = p.north.max(chunk_z + 2);
    let limit_south = p.south.min(chunk_z + 2 + 16);
    if limit_west >= limit_east || limit_north >= limit_south {
        let hits = (0..9)
            .filter(|&i| block(chunk_x + 7, p.min_y + i, chunk_z + 9, BlockQuery::Stone))
            .count();
        return if hits >= 5 {
            PlacementTest::NoOverlap
        } else {
            PlacementTest::NoOverlapAir
        };
    }
    // generateWithPlacement
    let (dx, dz) = ((pcx - ox) as f64, (pcz - oz) as f64);
    let local_density = 1.max(vein.density / (2.0 + dx * dx + dz * dz).sqrt() as i32);
    let has = |k: usize| !vein.ores[k].is_empty();
    // (secondary, between, primary) per layer, bottom (veinMinY - 1) first
    const LAYERS: [(bool, bool, bool); 9] = [
        (true, false, false),
        (true, false, false),
        (true, false, false),
        (true, true, false),
        (false, true, false),
        (false, true, true),
        (false, true, true),
        (false, false, true),
        (false, false, true),
    ];
    let mut placed = false;
    for (layer, &(secondary, between, primary)) in LAYERS.iter().enumerate() {
        let y = p.min_y - 1 + layer as i32;
        let ok_y = (0..256).contains(&y);
        for tx in limit_west..limit_east {
            let chance_x = 1.max((p.west - tx).abs().max((p.east - tx).abs()) / local_density);
            for tz in limit_north..limit_south {
                let chance_z =
                    1.max((p.south - tz).abs().max((p.north - tz).abs()) / local_density);
                let mut put = |k: usize| {
                    if ok_y && block(tx, y, tz, BlockQuery::Ore(k)) {
                        placed = true;
                        if !dry_run {
                            place(tx, y, tz, k);
                        }
                        true
                    } else {
                        false
                    }
                };
                if primary && (rng.next_int(chance_z) == 0 || rng.next_int(chance_x) == 0) && has(0)
                {
                    if put(0) && dry_run {
                        return PlacementTest::OrePlaced;
                    }
                    continue;
                }
                if between && (rng.next_int(chance_z) == 0 || rng.next_int(chance_x) == 0) && has(2)
                {
                    if put(2) && dry_run {
                        return PlacementTest::OrePlaced;
                    }
                    continue;
                }
                if secondary
                    && (rng.next_int(chance_z) == 0 || rng.next_int(chance_x) == 0)
                    && has(1)
                {
                    if put(1) && dry_run {
                        return PlacementTest::OrePlaced;
                    }
                    continue;
                }
                if rng.next_int(7) == 0
                    && (rng.next_int(chance_z) == 0 || rng.next_int(chance_x) == 0)
                    && has(3)
                    && put(3)
                    && dry_run
                {
                    return PlacementTest::OrePlaced;
                }
            }
        }
    }
    if placed {
        PlacementTest::OrePlaced
    } else {
        PlacementTest::NoOverlapAir
    }
}

/// GT's placement test (dry run), exactly, run from processing chunk `(pcx, pcz)` for oreseed `(ox, oz)`,
/// on a world without other ores: `stone(x, y, z)` = `StoneType.findStoneType(..) != null`.
#[allow(clippy::too_many_arguments)]
pub fn placement_test(
    vein: &Vein,
    p: &Placement,
    rng: Xstr,
    ox: i32,
    oz: i32,
    pcx: i32,
    pcz: i32,
    mut stone: impl FnMut(i32, i32, i32) -> bool,
) -> PlacementTest {
    execute_chunkified(
        vein,
        p,
        rng,
        ox,
        oz,
        pcx,
        pcz,
        true,
        |x, y, z, _| stone(x, y, z),
        |_, _, _, _| {},
    )
}

/// The vein an oreseed gets when GT first reaches it from processing chunk `(pcx, pcz)` (within ±2 chunks),
/// with the exact placement test on `block` (see [`execute_chunkified`]). Also returns the vein RNG for placing it.
#[allow(clippy::too_many_arguments)]
pub fn resolve_at_world(
    world_seed: i64,
    table: &VeinTable,
    cfg: &GtConfig,
    ox: i32,
    oz: i32,
    pcx: i32,
    pcz: i32,
    block: impl FnMut(&Vein, i32, i32, i32, BlockQuery) -> bool,
) -> Option<(Placement, Xstr)> {
    resolve_at_world_dim(world_seed, 0, table, cfg, ox, oz, pcx, pcz, block)
}

/// [`resolve_at_world`] in dimension `dim` (the oreveinSeed carries `dim & 0xff`; `table` = that dimension's list).
#[allow(clippy::too_many_arguments)]
pub fn resolve_at_world_dim(
    world_seed: i64,
    dim: i32,
    table: &VeinTable,
    cfg: &GtConfig,
    ox: i32,
    oz: i32,
    pcx: i32,
    pcz: i32,
    mut block: impl FnMut(&Vein, i32, i32, i32, BlockQuery) -> bool,
) -> Option<(Placement, Xstr)> {
    let ovs = orevein_seed(world_seed, dim, ox, oz);
    if Xstr::new(ovs).next_int(100) >= cfg.percentage {
        return None;
    }
    let mut failed = 0;
    let mut i = 0;
    while i < cfg.attempts && failed < cfg.max_placement_attempts {
        let (mut p, rng) = candidate_rng(table, ovs, i, ox, oz)?;
        let v = &table.overworld[p.vein];
        let r = execute_chunkified(
            v,
            &p,
            rng.clone(),
            ox,
            oz,
            pcx,
            pcz,
            true,
            |x, y, z, q| block(v, x, y, z, q),
            |_, _, _, _| {},
        );
        if r.accepted() {
            p.rejected = failed;
            return Some((p, rng));
        }
        failed += 1;
        i += 1;
    }
    None
}

/// [`resolve_at_world`] on a world without other ores.
#[allow(clippy::too_many_arguments)]
pub fn resolve_at(
    world_seed: i64,
    table: &VeinTable,
    cfg: &GtConfig,
    ox: i32,
    oz: i32,
    pcx: i32,
    pcz: i32,
    mut stone: impl FnMut(i32, i32, i32) -> bool,
) -> Option<Placement> {
    resolve_at_world(world_seed, table, cfg, ox, oz, pcx, pcz, |_, x, y, z, _| {
        stone(x, y, z)
    })
    .map(|r| r.0)
}
