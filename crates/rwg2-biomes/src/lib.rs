//! Bit-exact port of the Realistic-World-Gen-2 biome provider (`rwg2.world.ChunkManagerRealistic`).
//!
//! Every module mirrors one Java class at the commit pinned in `oracle/RWG2_COMMIT` and is verified against
//! golden vectors produced by `oracle/` from the verbatim Java sources. Zero mismatches is the bar.

pub mod cell;
pub mod chunkgen;
pub mod climate;
pub mod column;
pub mod config;
pub mod continental;
pub mod deco;
pub mod dimterrain;
pub mod draconium;
pub mod dungeons;
pub mod end;
pub mod end_homeland;
pub mod endtower;
pub mod fdlibm;
pub mod forestry;
pub mod fortress;
pub mod genlayer;
pub mod gtdims;
pub mod gtend;
pub mod gtfluids;
pub mod gtorder;
pub mod gtores;
pub mod island;
pub mod java_random;
pub mod jmath;
pub mod landmark;
pub mod loot;
pub mod mineshaft;
pub mod nether;
pub mod pack;
pub mod noise;
pub mod perlin;
pub mod poisson;
pub mod population;
pub mod provider;
pub mod search;
pub mod spawn;
pub mod stronghold_pieces;
pub mod structures;
pub mod surface;
#[cfg(feature = "json")]
pub mod table_json;
pub mod tc_dims;
pub mod tc_nodes;
pub mod terrain;
pub mod thaumcraft;
pub mod twilight;
pub mod villages;

pub use cell::CellNoise;
pub use java_random::JavaRandom;
pub use noise::NoiseGenerator;
pub use perlin::PerlinNoise;

/// Per-world noise implementation (`rwg_data` → `noiseImplementation`). New 2.9 worlds stay `Unknown`,
/// which RWG2 treats as `DynamicPerlin`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum NoiseImpl {
    Unknown,
    Perlin,
    OpenSimplex,
    DynamicPerlin,
    DynamicOpenSimplex,
}
