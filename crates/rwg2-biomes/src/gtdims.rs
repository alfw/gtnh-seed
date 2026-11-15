//! GT ore-vein placement test in the Nether (dim −1) and the Twilight Forest (dim 7).
//!
//! Same machinery as the overworld (`GTWorldgenerator.WorldGenContainer`, FIFO tasks, `MAX_VEIN_SIZE` = 2,
//! `executeWorldgenChunkified` dry run): an oreseed is decided in the first populated chunk within ±2 chunks, and
//! a pick is rejected when the vein misses that chunk (+2 bias) and fewer than 5 of the 9 blocks at
//! (chunkX + 7, veinMinY + 0..9, chunkZ + 9) are stone, or when the dry run finds no block it could turn into ore.
//!
//! "Stone" is `StoneType.findStoneType(world, x, y, z) != null` (GT5-Unofficial 5.09.54.265 `StoneType`): vanilla
//! stone (meta 0), netherrack, end stone and GT's granites / marble / basalt (none generate in these dims), the
//! other types are tied to space dimensions. So: Nether = netherrack only (not soul sand, gravel, bedrock, lava or
//! the BOP surface blocks); Twilight Forest = vanilla stone only (not deadrock, dirt, gravel, ice).
//!
//! Terrain: the exact pre-population blocks of [`crate::dimterrain`]. Not modelled: blocks changed by population
//! before GT runs (Nether quartz ore, hidden lava, glowstone, fortress blocks; TF trees / structures) and GT's own
//! small ores and earlier veins.
//!
//! Population order in a dimension is set by how the player arrives: entering loads the view square around the
//! arrival point x outer / z inner (`PlayerManager.addPlayer`), which populates it in lexicographic order (an
//! oreseed inside is decided at its window's north-west chunk, clipped to the square); walking outward then
//! reaches new oreseeds from the side facing the arrival point. [`processing_chunk_dim`] implements that with
//! the arrival point as a parameter (the viewer uses the dimension's origin).

use crate::dimterrain::{nb, tb, NetherTerrain, TfTerrain};
use crate::gtorder::MAX_VEIN_SIZE;
use crate::gtores::{resolve_at_world_dim, BlockQuery, GtConfig, Placement, VeinTable};
use crate::jmath::FxMap;

/// Chunk radius of the square loaded around a player entering a dimension (server view distance 10).
pub const ARRIVAL_RADIUS: i32 = 10;

/// Stone bits of one chunk: `cols[x * 16 + z]` bit y (y < 256).
struct StoneChunk {
    cols: Box<[[u128; 2]; 256]>,
}

impl StoneChunk {
    #[inline]
    fn stone(&self, x: i32, y: i32, z: i32) -> bool {
        if !(0..256).contains(&y) {
            return false;
        }
        let c = &self.cols[((x & 15) * 16 + (z & 15)) as usize];
        (c[(y >> 7) as usize] >> (y & 127)) & 1 == 1
    }
}

enum Gen {
    Nether(Box<NetherTerrain>),
    Tf(Box<TfTerrain>),
}

/// GT "stone" in dim −1 or 7 on the exact pre-population terrain, chunk-cached as stone bitmasks.
pub struct DimStone {
    gen: Gen,
    cache: FxMap<(i32, i32), StoneChunk>,
    /// Chunks generated so far (cost accounting).
    pub generated: usize,
    /// Cache bound (chunks); the cache is cleared when exceeded.
    pub max_cached: usize,
}

impl DimStone {
    /// `seed` = the dimension's world seed (= the overworld seed in GTNH for both).
    pub fn new(dim: i32, seed: i64) -> Option<Self> {
        let gen = match dim {
            -1 => Gen::Nether(Box::new(NetherTerrain::new(seed))),
            7 => Gen::Tf(Box::new(TfTerrain::new(seed))),
            _ => return None,
        };
        Some(Self {
            gen,
            cache: FxMap::default(),
            generated: 0,
            max_cached: 4096,
        })
    }

    fn chunk(&mut self, cx: i32, cz: i32) -> &StoneChunk {
        if !self.cache.contains_key(&(cx, cz)) {
            if self.cache.len() >= self.max_cached {
                self.cache.clear();
            }
            let mut cols = Box::new([[0u128; 2]; 256]);
            match &self.gen {
                Gen::Nether(n) => {
                    let c = n.chunk(cx, cz);
                    for x in 0..16 {
                        for z in 0..16 {
                            let col = &mut cols[(x * 16 + z) as usize];
                            for y in 0..128 {
                                if c.at(x, y, z) == nb::ROCK {
                                    col[0] |= 1u128 << y;
                                }
                            }
                        }
                    }
                }
                Gen::Tf(t) => {
                    let c = t.chunk(cx, cz);
                    for x in 0..16 {
                        for z in 0..16 {
                            let col = &mut cols[(x * 16 + z) as usize];
                            for y in 0..256 {
                                if c.at(x, y, z) == tb::STONE {
                                    col[(y >> 7) as usize] |= 1u128 << (y & 127);
                                }
                            }
                        }
                    }
                }
            }
            self.generated += 1;
            self.cache.insert((cx, cz), StoneChunk { cols });
        }
        &self.cache[&(cx, cz)]
    }

    /// `StoneType.findStoneType(world, x, y, z) != null` on the pre-population terrain.
    pub fn stone(&mut self, x: i32, y: i32, z: i32) -> bool {
        self.chunk(x >> 4, z >> 4).stone(x, y, z)
    }
}

/// The chunk GT decides oreseed `(ox, oz)` in for a player who entered the dimension at block `arrival` and
/// explored outward: inside the arrival square (chunks within `radius` of the arrival chunk, populated
/// `[a − r, a + r − 1]`) → the window's north-west chunk clipped to it; outside → the window corner facing the
/// arrival chunk (ties west / north).
pub fn processing_chunk_dim(ox: i32, oz: i32, arrival: (i32, i32), radius: i32) -> (i32, i32) {
    let r = MAX_VEIN_SIZE;
    let (ax, az) = (arrival.0 >> 4, arrival.1 >> 4);
    let (x0, z0, x1, z1) = (ax - radius, az - radius, ax + radius - 1, az + radius - 1);
    if ox + r >= x0 && ox - r <= x1 && oz + r >= z0 && oz - r <= z1 {
        return ((ox - r).max(x0), (oz - r).max(z0));
    }
    let px = if ax > ox + r { ox + r } else { ox - r };
    let pz = if az > oz + r { oz + r } else { oz - r };
    (px, pz)
}

/// The vein of oreseed `(ox, oz)` in dim −1 / 7 with GT's exact placement test run from processing chunk
/// `(pcx, pcz)` on the exact pre-population terrain.
#[allow(clippy::too_many_arguments)]
pub fn resolve_dim_at(
    world_seed: i64,
    dim: i32,
    table: &VeinTable,
    cfg: &GtConfig,
    ox: i32,
    oz: i32,
    pcx: i32,
    pcz: i32,
    s: &mut DimStone,
) -> Option<Placement> {
    resolve_at_world_dim(
        world_seed,
        dim,
        table,
        cfg,
        ox,
        oz,
        pcx,
        pcz,
        |_, x, y, z, q| match q {
            BlockQuery::Stone | BlockQuery::Ore(_) => s.stone(x, y, z),
        },
    )
    .map(|r| r.0)
}

/// [`resolve_dim_at`] from [`processing_chunk_dim`] with the arrival point at the dimension's origin.
pub fn resolve_dim(
    world_seed: i64,
    dim: i32,
    table: &VeinTable,
    cfg: &GtConfig,
    ox: i32,
    oz: i32,
    s: &mut DimStone,
) -> Option<Placement> {
    let (px, pz) = processing_chunk_dim(ox, oz, (0, 0), ARRIVAL_RADIUS);
    resolve_dim_at(world_seed, dim, table, cfg, ox, oz, px, pz, s)
}

/// The runner-up for the tooltip (like [`crate::gtores::alternative`], in dimension `dim`): the first pick if
/// `chosen` came after it, else the pick after `chosen`; `None` if it is the same vein type.
pub fn alternative_dim(
    table: &VeinTable,
    world_seed: i64,
    dim: i32,
    ox: i32,
    oz: i32,
    chosen: Option<&Placement>,
) -> Option<Placement> {
    let ovs = crate::gtores::orevein_seed(world_seed, dim, ox, oz);
    let i = match chosen {
        Some(p) if p.rejected > 0 => 0,
        Some(p) => p.rejected + 1,
        None => 0,
    };
    crate::gtores::candidate(table, ovs, i, ox, oz)
        .filter(|c| chosen.map_or(true, |p| p.vein != c.vein))
}
