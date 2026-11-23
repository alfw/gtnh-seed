//! Supported GTNH packs. Everything the port reads per pack that differs between the supported ones:
//! the biome table (`data/<dir>/biomes.json`, read by the caller) and the chest loot tables (embedded here).
//!
//! | pack | RWG2 | differs from the previous |
//! |---|---|---|
//! | `Gtnh290` (2.9.0; also the 2.10 test builds up to coremod 2.9.99) | 2.0.6 | — |
//! | `Gtnh291` (2.9.1) | 2.0.9 | per-continent Tainted Land, `DecoBigTree` fixes, lava-cave funnel ledge (all keyed on the biome table, `BiomeTable::rwg2_209`); dungeon chests +1 item (EMT "The One Ring", weight 5) |
//!
//! The current pack is process-wide (one per wasm instance): set it before building providers or rolling loot.

use std::sync::atomic::{AtomicU8, Ordering};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
#[repr(u8)]
pub enum Pack {
    Gtnh290 = 0,
    Gtnh291 = 1,
}

impl Pack {
    pub const ALL: [Pack; 2] = [Pack::Gtnh290, Pack::Gtnh291];

    pub fn from_index(i: u32) -> Option<Pack> {
        Self::ALL.get(i as usize).copied()
    }

    /// Short id used in URLs and file names.
    pub fn id(self) -> &'static str {
        match self {
            Pack::Gtnh290 => "2.9.0",
            Pack::Gtnh291 => "2.9.1",
        }
    }

    /// `data/<dir>` holding this pack's tables.
    pub fn data_dir(self) -> &'static str {
        match self {
            Pack::Gtnh290 => "gtnh-2.9.0",
            Pack::Gtnh291 => "gtnh-2.9.1",
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            Pack::Gtnh290 => "GTNH 2.9.0 (RWG2 2.0.6)",
            Pack::Gtnh291 => "GTNH 2.9.1 (RWG2 2.0.9)",
        }
    }

    /// `dungeonloot.json` (compact form, see `loot`).
    pub fn dungeon_loot_json(self) -> &'static str {
        match self {
            Pack::Gtnh290 => include_str!("../../../data/gtnh-2.9.0/dungeonloot.json"),
            Pack::Gtnh291 => include_str!("../../../data/gtnh-2.9.1/dungeonloot.json"),
        }
    }

    /// `chestloot.json` (other `ChestGenHooks` categories, `tools/chestloot.py`).
    pub fn chest_loot_json(self) -> &'static str {
        match self {
            Pack::Gtnh290 => include_str!("../../../data/gtnh-2.9.0/chestloot.json"),
            Pack::Gtnh291 => include_str!("../../../data/gtnh-2.9.1/chestloot.json"),
        }
    }
}

static CURRENT: AtomicU8 = AtomicU8::new(Pack::Gtnh290 as u8);

pub fn current() -> Pack {
    Pack::from_index(CURRENT.load(Ordering::Relaxed) as u32).unwrap_or(Pack::Gtnh290)
}

pub fn set(p: Pack) {
    CURRENT.store(p as u8, Ordering::Relaxed);
}
