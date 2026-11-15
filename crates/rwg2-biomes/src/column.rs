//! Pre-population block columns of an RWG2 overworld chunk, as far as air/solid goes: the terrain (stone up to
//! `(int) testHeight`, sea to y 62), `carveMountainChainRivers` (before the surface pass), vanilla caves
//! (Hodgepodge, after the surface pass) and `LavaCaveLandmark.generate` (last). Mineshafts/strongholds are not
//! included (they don't touch the surface layers the users of this module look at).

use crate::chunkgen::{Chunk, LavaColumn};
use crate::gtorder::{CaveMask, Caves};
use crate::provider::{BiomeProvider, Heights};

/// `hard` upper bound meaning "up to depth 12, and any depth above y 63" (canyon / mesa clay).
pub const HARD_CLAY_ABOVE_63: i32 = 1000;

/// Block class before the surface pass (what `replaceBlocksForBiome` sees).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Pre {
    Air,
    Water,
    Stone,
}

/// One chunk's carved columns.
pub struct CarvedChunk {
    heights: [i32; 256],
    river: [crate::chunkgen::RiverCave; 256],
    caves: Option<CaveMask>,
    /// the caves' dig steps in generation order (see [`crate::gtorder::Caves::carve_water_runs`])
    cave_runs: Vec<[u8; 4]>,
    lava: [Option<LavaColumn>; 256],
    /// Per column: depth range `[from, to)` of each surface-pass run that caves cannot cut (vanilla `digBlock`
    /// only replaces stone and the biome's top/filler block: snow, hardened clay, sandstone survive).
    hard: [(i32, i32); 256],
    /// `SnowheightCalculator`: a snow layer on top of the column's first surface run (SurfacePolar).
    snow: [i32; 256],
}

impl CarvedChunk {
    /// `caves`: the world's cave generator (`None`: no caves).
    pub fn new(
        p: &BiomeProvider,
        h: &dyn Heights,
        c: &Chunk,
        cx: i32,
        cz: i32,
        caves: Option<&Caves>,
    ) -> Self {
        Self::with_hard(p, h, c, cx, cz, caves, [(0, 0); 256])
    }

    /// [`CarvedChunk::new`] with per-column uncarvable painted depth ranges (see [`crate::spawn::hard_layers`]).
    pub fn with_hard(
        p: &BiomeProvider,
        _h: &dyn Heights,
        c: &Chunk,
        cx: i32,
        cz: i32,
        caves: Option<&Caves>,
        hard: [(i32, i32); 256],
    ) -> Self {
        let mut heights = [0i32; 256];
        for (i, v) in heights.iter_mut().enumerate() {
            *v = c.height[i] as i32;
        }
        let river = c.river_cave;
        let (mask, cave_runs) = match caves.map(|cv| {
            cv.carve_water_runs(
                cx,
                cz,
                &heights,
                &|lx, y, lz| y <= 62 && river[(lz * 16 + lx) as usize].carved(y),
                true,
            )
        }) {
            Some((m, r)) => (Some(m), r),
            None => (None, Vec::new()),
        };
        let mut out = CarvedChunk {
            heights,
            river,
            caves: mask,
            cave_runs,
            lava: [None; 256],
            hard,
            snow: [-1; 256],
        };
        if p.is_continental() && p.table.lava_cave_marker_gen_id.is_some() {
            for idx in 0..256usize {
                let (lx, lz) = ((idx & 15) as i32, (idx >> 4) as i32);
                let packed = c.lava_cave[idx];
                if packed != crate::continental::NONE {
                    let surface = out.surface_after_caves(lx, lz);
                    out.lava[idx] = crate::chunkgen::lava_cave_column_at(
                        p,
                        packed,
                        cx * 16 + lx,
                        cz * 16 + lz,
                        surface,
                    );
                }
            }
        }
        out
    }

    /// Puts a snow layer on the first surface run of each flagged column (call before reading).
    pub fn set_snow(&mut self, flags: &[bool; 256]) {
        for (idx, &f) in flags.iter().enumerate() {
            if f {
                let (lx, lz) = ((idx & 15) as i32, (idx >> 4) as i32);
                if let Some(&k) = self.runs(lx, lz).first() {
                    if k > 61 && k < 254 {
                        self.snow[idx] = k + 1;
                    }
                }
            }
        }
    }

    /// [`CarvedChunk::pre`] of a whole column, as `stone` / `water` / `air` codes.
    pub fn fill_pre(&self, lx: i32, lz: i32, out: &mut [u8; 256], stone: u8, water: u8, air: u8) {
        let idx = (lz * 16 + lx) as usize;
        if self.river[idx].is_none() {
            let s = (self.heights[idx] + 1).clamp(0, 256) as usize;
            out[..s].fill(stone);
            if s < 63 {
                out[s..63].fill(water);
            }
            out[s.max(63)..].fill(air);
        } else {
            for (y, v) in out.iter_mut().enumerate() {
                *v = match self.pre(lx, y as i32, lz) {
                    Pre::Stone => stone,
                    Pre::Water => water,
                    Pre::Air => air,
                };
            }
        }
    }

    /// Pre-surface-pass block.
    #[inline]
    pub fn pre(&self, lx: i32, y: i32, lz: i32) -> Pre {
        let idx = (lz * 16 + lx) as usize;
        let open = y > self.heights[idx] || self.river[idx].carved(y);
        if !open {
            Pre::Stone
        } else if y <= 62 {
            Pre::Water
        } else {
            Pre::Air
        }
    }

    fn cave(&self, lx: i32, y: i32, lz: i32) -> bool {
        if !self.caves.as_ref().is_some_and(|m| m.carved(lx, y, lz)) {
            return false;
        }
        let (from, to) = self.hard[(lz * 16 + lx) as usize];
        if from >= to {
            return true;
        }
        // depth of y in its surface-pass run
        let mut k = y;
        while k < 255 && self.pre(lx, k + 1, lz) == Pre::Stone {
            k += 1;
        }
        let d = k - y;
        if to == HARD_CLAY_ABOVE_63 {
            return !(d >= from && (d < 12 || y > 63));
        }
        !(d >= from && d < to)
    }

    /// `LavaCaveLandmark.findSurface`: highest non-air block after caves.
    fn surface_after_caves(&self, lx: i32, lz: i32) -> i32 {
        let mut y = 255;
        while y > 1
            && (self.pre(lx, y, lz) == Pre::Air
                || (self.pre(lx, y, lz) == Pre::Stone && self.cave(lx, y, lz)))
        {
            y -= 1;
        }
        y
    }

    /// Air after every carve.
    pub fn air(&self, lx: i32, y: i32, lz: i32) -> bool {
        if !(0..256).contains(&y) {
            return y >= 256;
        }
        let pre = self.pre(lx, y, lz);
        if pre == Pre::Air {
            return self.snow[(lz * 16 + lx) as usize] != y
                || self.lava[(lz * 16 + lx) as usize].is_some_and(|l| l.air(y, false));
        }
        if pre == Pre::Stone && self.cave(lx, y, lz) {
            return true;
        }
        self.lava[(lz * 16 + lx) as usize].is_some_and(|l| l.air(y, false))
    }

    /// `WorldProviderSurfaceBOP.canCoordinateBeSpawn` climb: from y 63 up while the block above is not air.
    /// Returns (y, block at y is air).
    pub fn bop_climb(&self, lx: i32, lz: i32) -> (i32, bool) {
        let mut y = 63;
        while y < 255 && !self.air(lx, y + 1, lz) {
            y += 1;
        }
        (y, self.air(lx, y, lz))
    }

    /// A vanilla cave digs this block (stone before the surface pass, outside the uncarvable painted layers).
    pub fn cave_dug(&self, lx: i32, y: i32, lz: i32) -> bool {
        self.pre(lx, y, lz) == Pre::Stone && self.cave(lx, y, lz)
    }

    /// A vanilla cave tunnel passes through this block (stone before the surface pass), whatever the surface pass
    /// painted there; `digBlock` then decides by the painted block (stone / the biome's top / filler).
    pub fn cave_raw(&self, lx: i32, y: i32, lz: i32) -> bool {
        self.pre(lx, y, lz) == Pre::Stone
            && self.caves.as_ref().is_some_and(|m| m.carved(lx, y, lz))
    }

    /// The caves' dig steps in generation order: `[lx, lz, y_top, y_bottom]` per step and column.
    pub fn cave_runs(&self) -> &[[u8; 4]] {
        &self.cave_runs
    }

    /// Built with a cave generator.
    pub fn has_caves(&self) -> bool {
        self.caves.is_some()
    }

    /// The lava-cave landmark's carve of this column.
    pub fn lava_column(&self, lx: i32, lz: i32) -> Option<LavaColumn> {
        self.lava[(lz * 16 + lx) as usize]
    }

    /// The lava-cave landmark carves this column.
    pub fn lava_carved(&self, lx: i32, lz: i32) -> bool {
        self.lava[(lz * 16 + lx) as usize].is_some()
    }

    /// Highest non-air block (y, or -1).
    pub fn top(&self, lx: i32, lz: i32) -> i32 {
        let mut y = 255;
        while y >= 0 && self.air(lx, y, lz) {
            y -= 1;
        }
        y
    }

    /// Surface-pass runs of a column: y of each stone block painted at depth 0 (stone with only air, through
    /// water, above it), top first.
    pub fn runs(&self, lx: i32, lz: i32) -> Vec<i32> {
        let mut out = Vec::new();
        let mut reset = true; // depth == -1
        let top = self.heights[(lz * 16 + lx) as usize].clamp(-1, 255);
        for y in (0..=top.max(62)).rev() {
            match self.pre(lx, y, lz) {
                Pre::Air => reset = true,
                Pre::Water => {}
                Pre::Stone => {
                    if reset {
                        out.push(y);
                    }
                    reset = false;
                }
            }
        }
        out
    }
}
