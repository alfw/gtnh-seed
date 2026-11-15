//! GregTech 5.09.54 (GTNH 2.9.0) underground fluids ("UO": oil/gas fields read by pumps, drilling rigs and
//! prospectors): fluid kind and pristine amount per chunk.
//!
//! `UndergroundOil.getPristineAmount(world, chunkX, chunkZ)` (pure seed maths, no terrain):
//! - fields are 8×8 chunks (128×128 blocks, aligned): `rx = chunkX >> 3`, `rz = chunkZ >> 3`
//! - `rng = XSTR(worldSeed + dim * 2L + rx + 8267L * rz)` (XSTR seed 0 → 42)
//! - fluid: `GTUODimension.getRandomFluid`: `r = nextInt(1000)`; over the fluids in **`HashBiMap` iteration
//!   order**: `c = chance * 1000 / sumChance`; `if r <= c` pick, else `r -= c` (note `<=`: the first fluid gets
//!   one extra value, so a dimension can end with "nothing" only through integer truncation)
//! - field average: `GTUOFluid.getRandomAmount`: `smax = (int) floor(pow(max * 100 * 5000, 0.2))`,
//!   `smin = pow(min * 100 * 5000, 0.2)`, `s = max(smin, nextInt(smax) + nextDouble())`,
//!   `avg = (int) (powInt(s, 5) / 100)` (`powInt` = square-and-multiply: `s * ((s*s)*(s*s))`)
//! - per chunk: skip `i = (chunkX & 7) << 3 | (chunkZ & 7)` draws of `next(24)`, then
//!   `amount = (int) ((float) avg * (0.75f + nextFloat() / 2f))` (f32 maths) — ±25 % around the field average
//! - output: a pump op at coefficient `k` gives `floor(amount * k / 5000)` L and removes `decrease * k` (randomly
//!   rounded) from `amount`; prospectors show `floor(amount / 5000)` (`readOrDrainCoefficient = -1`). Reading a chunk
//!   with `amount <= 5000` **sets it to 0** (GT quirk). Saved chunk data survives only while the vein hash
//!   (registry, max, min, chance, decrease) and key match the config; otherwise the pristine value is regenerated.
//!
//! Dimension: only the seed term `dim * 2L` and the table choice depend on it. Blacklist `{-1, 1}` (Nether, End) → no
//! fluids; other dimensions are matched by id string or provider class name (`GTUODimensionList.GetDimension`).
//!
//! Exactness: the `HashBiMap` order is Guava 17's (MC 1.7.10) bucket order (see [`guava17_order`]), verified by
//! running the verbatim GT classes against Guava 17 sources. `pow(x, 0.2)` uses `f64::powf`; HotSpot `Math.pow` and
//! `StrictMath.pow` agree with it on every amount in the default tables we checked, but `smin` sits exactly on an
//! integer boundary (`smin^5 / 100 == min * 5000`), so a 1-ulp libm difference changes the clamped average by 1.

/// `UndergroundOil.DIVIDER`: amount units per litre per operation.
pub const DIVIDER: i32 = 5000;

/// GT's `XSTR` with the `Random` surface `getPristineAmount` needs (`next(bits)`, `nextFloat`, `nextDouble`).
/// Same state machine as [`crate::gtores::Xstr`], which keeps `step` private.
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

    #[inline]
    fn step(&mut self) -> i64 {
        let mut x = self.seed;
        x ^= x << 21;
        x ^= ((x as u64) >> 35) as i64;
        x ^= x << 4;
        self.seed = x;
        x
    }

    /// `next(nbits)`: low `nbits` of the new state.
    #[inline]
    pub fn next(&mut self, nbits: u32) -> i32 {
        (self.step() & ((1i64 << nbits) - 1)) as i32
    }

    /// `nextInt(bound)`: `abs((int) state % bound)` (not `Random`'s rejection sampling).
    #[inline]
    pub fn next_int(&mut self, bound: i32) -> i32 {
        let out = (self.step() as i32) % bound;
        if out < 0 {
            -out
        } else {
            out
        }
    }

    #[inline]
    pub fn next_float(&mut self) -> f32 {
        self.next(24) as f32 * (1.0 / (1u32 << 24) as f32)
    }

    #[inline]
    pub fn next_double(&mut self) -> f64 {
        (((self.next(26) as i64) << 27) + self.next(27) as i64) as f64 * (1.0 / (1u64 << 53) as f64)
    }
}

/// One `GTUOFluid` (config child category of a dimension).
#[derive(Clone, Debug)]
pub struct UoFluid {
    /// Config category name (`UndergroundFluidNames` enum name, e.g. `lightOil`).
    pub name: String,
    /// Forge fluid registry name (the `HashBiMap` key).
    pub registry: String,
    pub min_amount: i32,
    pub max_amount: i32,
    /// Weight (`Chance`).
    pub chance: i32,
    pub decrease_per_operation: i32,
    /// `(int) floor(pow(max * 100.0 * 5000, 0.2))`.
    pub smax: i32,
    /// `pow(min * 100.0 * 5000, 0.2)`.
    pub smin: f64,
}

impl UoFluid {
    pub fn new(
        name: &str,
        registry: &str,
        min_amount: i32,
        max_amount: i32,
        chance: i32,
        decrease: i32,
    ) -> Self {
        UoFluid {
            name: name.to_string(),
            registry: registry.to_string(),
            min_amount,
            max_amount,
            chance,
            decrease_per_operation: decrease,
            smax: pow02(max_amount as f64 * 100.0 * DIVIDER as f64).floor() as i32,
            smin: pow02(min_amount as f64 * 100.0 * DIVIDER as f64),
        }
    }

    /// `getRandomAmount`: the field average. `None` where Java would throw (`smax <= 0` → `nextInt` by zero).
    pub fn random_amount(&self, rng: &mut Xstr) -> Option<i32> {
        if self.smax <= 0 {
            return None;
        }
        let a = rng.next_int(self.smax) as f64 + rng.next_double();
        let s = if self.smin > a { self.smin } else { a }; // Math.max (no NaN/−0 cases here)
        Some((pow_int5(s) / 100.0) as i32)
    }
}

/// `Math.pow(x, 0.2d)`.
#[inline]
fn pow02(x: f64) -> f64 {
    x.powf(0.2)
}

/// `GTUtility.powInt(base, 5)`: square-and-multiply, incl. its exact shortcuts for bases 2 and 4.
#[inline]
pub fn pow_int5(b: f64) -> f64 {
    if b == 2.0 {
        return 32.0;
    }
    if b == 4.0 {
        return 1024.0;
    }
    let b2 = b * b;
    let b4 = b2 * b2;
    b * b4
}

/// A dimension's fluids **in `HashBiMap` iteration order** (what `getRandomFluid` walks).
#[derive(Clone, Debug, Default)]
pub struct FluidTable {
    pub fluids: Vec<UoFluid>,
    /// `maxChance` = sum of chances.
    pub max_chance: i32,
    /// Pack/config provenance note.
    pub source: String,
    pub provisional: bool,
}

impl FluidTable {
    /// `fluids` must already be in iteration order (use [`guava17_order`] for config order).
    pub fn new(fluids: Vec<UoFluid>) -> Self {
        let max_chance = fluids.iter().map(|f| f.chance).sum();
        FluidTable {
            fluids,
            max_chance,
            source: String::new(),
            provisional: false,
        }
    }

    /// GT 5.09.54 code defaults for the Overworld (`GTUODimensionList.setOverworldValues`), in Guava 17 order.
    /// GTNH 2.9.0 ships no `config/GregTech/UndergroundFluids.cfg`, so these are what a new world uses.
    pub fn overworld_default() -> Self {
        // config (insertion) order
        let cfg = [
            UoFluid::new("naturalGas", "gas_natural_gas", 10, 350, 20, 5),
            UoFluid::new("lightOil", "liquid_light_oil", 10, 350, 20, 5),
            UoFluid::new("mediumOil", "liquid_medium_oil", 0, 625, 20, 5),
            UoFluid::new("heavyOil", "liquid_heavy_oil", 0, 625, 20, 5),
            UoFluid::new("oil", "oil", 0, 625, 20, 5),
        ];
        let regs: Vec<&str> = cfg.iter().map(|f| f.registry.as_str()).collect();
        let order = guava17_order(&regs);
        let mut t = FluidTable::new(order.into_iter().map(|i| cfg[i].clone()).collect());
        t.source = "GT5-Unofficial 5.09.54.265 code defaults (setOverworldValues), Guava 17 HashBiMap order".into();
        t
    }

    /// `getRandomFluid`: index into `fluids`, or `None` ("nothing here").
    pub fn random_fluid(&self, rng: &mut Xstr) -> Option<usize> {
        let mut r = rng.next_int(1000);
        if self.max_chance == 0 {
            return None; // Java: empty list → null; all-zero chances → ArithmeticException
        }
        for (i, f) in self.fluids.iter().enumerate() {
            let c = f.chance.wrapping_mul(1000) / self.max_chance; // maxChance > 0 whenever the loop runs
            if r <= c {
                return Some(i);
            }
            r -= c;
        }
        None
    }

    /// `data/<pack>/gtfluids.json`: `{"provisional", "source", "dimensions": {"0": {"order": "hashbimap"|"config",
    /// "fluids": [{name, registry, minAmount, maxAmount, chance, decreasePerOperation}]}}}`.
    /// `order: "config"` → reorder with [`guava17_order`]; `"hashbimap"` → as listed (e.g. a live dump).
    #[cfg(feature = "json")]
    pub fn from_json(s: &str, dim: &str) -> Result<Self, String> {
        let v: serde_json::Value = serde_json::from_str(s).map_err(|e| e.to_string())?;
        let d = &v["dimensions"][dim];
        let arr = d["fluids"].as_array().ok_or("fluids")?;
        let mut fl = Vec::new();
        for x in arr {
            let i = |k: &str| x[k].as_i64().map(|n| n as i32).ok_or(k.to_string());
            let st = |k: &str| x[k].as_str().unwrap_or("").to_string();
            fl.push(UoFluid::new(
                &st("name"),
                &st("registry"),
                i("minAmount")?,
                i("maxAmount")?,
                i("chance")?,
                i("decreasePerOperation")?,
            ));
        }
        if d["order"].as_str() == Some("config") {
            let regs: Vec<&str> = fl.iter().map(|f| f.registry.as_str()).collect();
            let order = guava17_order(&regs);
            fl = order.into_iter().map(|i| fl[i].clone()).collect();
        }
        let mut t = FluidTable::new(fl);
        t.source = v["source"].as_str().unwrap_or("").to_string();
        t.provisional = v["provisional"].as_bool().unwrap_or(true);
        Ok(t)
    }
}

/// Java `String.hashCode` (UTF-16 code units).
pub fn java_string_hash(s: &str) -> i32 {
    s.encode_utf16()
        .fold(0i32, |h, c| h.wrapping_mul(31).wrapping_add(c as i32))
}

/// Guava 17 `Hashing.smear`.
#[inline]
pub fn guava_smear(h: i32) -> i32 {
    (h.wrapping_mul(0xcc9e2d51u32 as i32))
        .rotate_left(15)
        .wrapping_mul(0x1b873593)
}

/// Iteration order of a Guava 17 `HashBiMap.create()` after putting distinct `keys` in this order: buckets
/// `smear(hash) & 15` ascending, newest first within a bucket. Returns indices into `keys`.
/// Only up to 16 keys (no rehash; GT dimensions have ≤ 4–5 fluids).
pub fn guava17_order(keys: &[&str]) -> Vec<usize> {
    assert!(keys.len() <= 16, "guava17_order: rehash not modelled");
    let mut buckets: Vec<Vec<usize>> = vec![Vec::new(); 16];
    for (i, k) in keys.iter().enumerate() {
        buckets[(guava_smear(java_string_hash(k)) & 15) as usize].insert(0, i);
    }
    buckets.into_iter().flatten().collect()
}

/// The XSTR seed of field `(rx, rz)`: `worldSeed + dim * 2L + rx + 8267L * rz` (wrapping like Java `long`).
#[inline]
pub fn field_seed(world_seed: i64, dim: i32, rx: i32, rz: i32) -> i64 {
    world_seed
        .wrapping_add((dim as i64).wrapping_mul(2))
        .wrapping_add(rx as i64)
        .wrapping_add(8267i64.wrapping_mul(rz as i64))
}

/// One 8×8-chunk field: kind and average, before the per-chunk ±25 %.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct FieldSample {
    /// Index into [`FluidTable::fluids`].
    pub fluid_index: usize,
    /// `veinAverage` (amount units; `/ 5000` = L per op).
    pub vein_average: i32,
    pub region_x: i32,
    pub region_z: i32,
}

/// One chunk's pristine deposit.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct FluidSample {
    pub fluid_index: usize,
    /// Pristine `amount` (units; see [`FluidSample::litres_per_op`]).
    pub amount: i32,
    pub vein_average: i32,
    pub region_x: i32,
    pub region_z: i32,
}

impl FluidSample {
    /// What a prospector / pump at coefficient 1 reports: `floor(amount / 5000)`, 0 if `amount <= 5000`.
    pub fn litres_per_op(&self) -> i32 {
        if self.amount <= DIVIDER {
            0
        } else {
            self.amount / DIVIDER
        }
    }
}

fn field_rng(
    world_seed: i64,
    dim: i32,
    rx: i32,
    rz: i32,
    table: &FluidTable,
) -> Option<(FieldSample, Xstr)> {
    let mut rng = Xstr::new(field_seed(world_seed, dim, rx, rz));
    let fluid_index = table.random_fluid(&mut rng)?;
    let vein_average = table.fluids[fluid_index].random_amount(&mut rng)?;
    Some((
        FieldSample {
            fluid_index,
            vein_average,
            region_x: rx,
            region_z: rz,
        },
        rng,
    ))
}

/// Field `(rx, rz)` = chunks `rx*8 .. rx*8+7` × `rz*8 .. rz*8+7`. `None` = no fluid.
pub fn field_at(
    world_seed: i64,
    dim: i32,
    rx: i32,
    rz: i32,
    table: &FluidTable,
) -> Option<FieldSample> {
    field_rng(world_seed, dim, rx, rz, table).map(|(f, _)| f)
}

#[inline]
fn chunk_amount(avg: i32, f: f32) -> i32 {
    (avg as f32 * (0.75f32 + f / 2.0f32)) as i32
}

/// `getPristineAmount` for chunk `(chunk_x, chunk_z)`.
pub fn fluid_at(
    world_seed: i64,
    dim: i32,
    chunk_x: i32,
    chunk_z: i32,
    table: &FluidTable,
) -> Option<FluidSample> {
    let (f, mut rng) = field_rng(world_seed, dim, chunk_x >> 3, chunk_z >> 3, table)?;
    for _ in 0..(((chunk_x & 7) << 3) | (chunk_z & 7)) {
        rng.next(24);
    }
    Some(FluidSample {
        fluid_index: f.fluid_index,
        amount: chunk_amount(f.vein_average, rng.next_float()),
        vein_average: f.vein_average,
        region_x: f.region_x,
        region_z: f.region_z,
    })
}

/// All 64 chunk amounts of a field in one RNG pass; index `(lx << 3) | lz` with `lx = chunkX & 7`, `lz = chunkZ & 7`.
pub fn field_chunks(
    world_seed: i64,
    dim: i32,
    rx: i32,
    rz: i32,
    table: &FluidTable,
) -> Option<(FieldSample, [i32; 64])> {
    let (f, mut rng) = field_rng(world_seed, dim, rx, rz, table)?;
    let mut a = [0i32; 64];
    for v in a.iter_mut() {
        *v = chunk_amount(f.vein_average, rng.next_float());
    }
    Some((f, a))
}

/// Field coordinates covering the block rectangle `[x0, x1] × [z0, z1]` (inclusive), row-major.
pub fn fields_in_rect(x0: i32, z0: i32, x1: i32, z1: i32) -> impl Iterator<Item = (i32, i32)> {
    let (rx0, rx1) = (x0.min(x1) >> 7, x0.max(x1) >> 7);
    let (rz0, rz1) = (z0.min(z1) >> 7, z0.max(z1) >> 7);
    (rz0..=rz1).flat_map(move |rz| (rx0..=rx1).map(move |rx| (rx, rz)))
}

/// Squared distance (blocks) from `(x, z)` to the nearest block of field `(rx, rz)`.
pub fn field_dist2(x: i32, z: i32, rx: i32, rz: i32) -> i64 {
    let d = |p: i32, r: i32| -> i64 {
        let (lo, hi) = ((r as i64) << 7, ((r as i64) << 7) + 127);
        let p = p as i64;
        if p < lo {
            lo - p
        } else if p > hi {
            p - hi
        } else {
            0
        }
    };
    let (dx, dz) = (d(x, rx), d(z, rz));
    dx * dx + dz * dz
}

/// Fields with any block within `r` of `(x, z)`, nearest first (ties row-major).
pub fn fields_within(x: i32, z: i32, r: i32) -> Vec<(i32, i32)> {
    let r2 = r as i64 * r as i64;
    let mut v: Vec<(i64, i32, i32)> = fields_in_rect(
        x.saturating_sub(r),
        z.saturating_sub(r),
        x.saturating_add(r),
        z.saturating_add(r),
    )
    .map(|(rx, rz)| (field_dist2(x, z, rx, rz), rx, rz))
    .filter(|t| t.0 <= r2)
    .collect();
    v.sort_by_key(|t| t.0);
    v.into_iter().map(|t| (t.1, t.2)).collect()
}

/// Nearest field within `r` of `(x, z)` (blocks) whose sample satisfies `pred`.
pub fn nearest_field(
    world_seed: i64,
    dim: i32,
    table: &FluidTable,
    x: i32,
    z: i32,
    r: i32,
    mut pred: impl FnMut(&FieldSample) -> bool,
) -> Option<FieldSample> {
    fields_within(x, z, r)
        .into_iter()
        .find_map(|(rx, rz)| field_at(world_seed, dim, rx, rz, table).filter(|f| pred(f)))
}
