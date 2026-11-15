//! HEE island interior after the caves, on the island's `consistentRand` (all deterministic, `ComponentIsland
//! .setupStructure`): `OreGenerator` (stardust / instability orb clusters) and, for Enchanted Island / Homeland, the
//! `StructureHiddenCellar` pass that opens `BiomeDecoratorEnchantedIsland.genHomeland` (4–6 underground persegrit
//! cellars: rooms, halls, room contents, wall patterns). Chest loot is filled later from another Random
//! (`onTileEntityRequested`), so the cellar draws are geometry only. The rest of `genHomeland` (goo lakes, obsidian
//! piles, roads, orchids, caches, endermen) is not modelled.
//!
//! Coordinates are structure space: x, z 0..208, y 0..140 (world y = y + 20 + bottomY, world x = start·16 + 8 + x).
//! Reads outside return air and writes outside are dropped (`LargeStructureChunk`).

use crate::end::{island_terrain, Island};
use crate::java_random::JavaRandom;

pub const SIZE: i32 = 208;
pub const HEIGHT: i32 = 140;

/// Block kinds the passes tell apart.
pub mod b {
    pub const AIR: u8 = 0;
    pub const END_STONE: u8 = 1;
    /// `end_terrain` (the island top)
    pub const TERRAIN: u8 = 2;
    pub const STARDUST: u8 = 3;
    pub const INSTABILITY: u8 = 4;
    pub const PERSEGRIT: u8 = 5;
    pub const BEDROCK: u8 = 6;
    pub const CHEST: u8 = 7;
    pub const TRAPPED_CHEST: u8 = 8;
    /// `endermanSolidBlocks` (grass, dirt, sand, gravel, TNT, clay, pumpkin, melon, mycelium)
    pub const ENDERMAN_SOLID: u8 = 9;
}

/// The island's structure blocks.
pub struct IslandBlocks {
    pub island: Island,
    pub blocks: Vec<u8>,
}

impl IslandBlocks {
    #[inline]
    fn idx(x: i32, y: i32, z: i32) -> Option<usize> {
        ((0..SIZE).contains(&x) && (0..HEIGHT).contains(&y) && (0..SIZE).contains(&z))
            .then(|| ((y * SIZE + z) * SIZE + x) as usize)
    }
    #[inline]
    pub fn get(&self, x: i32, y: i32, z: i32) -> u8 {
        Self::idx(x, y, z).map_or(b::AIR, |i| self.blocks[i])
    }
    #[inline]
    fn set(&mut self, x: i32, y: i32, z: i32, v: u8) {
        if let Some(i) = Self::idx(x, y, z) {
            self.blocks[i] = v;
        }
    }
    #[inline]
    fn air(&self, x: i32, y: i32, z: i32) -> bool {
        self.get(x, y, z) == b::AIR
    }
    /// (solid, end stone) world-Y masks per column (index z·208 + x), like [`crate::end::island_cols`]; solid =
    /// any non-air block (persegrit, ores, chests … included).
    pub fn cols(&self) -> Vec<crate::end::IslandCol> {
        let base = 20 + self.island.bottom_y;
        let mut out = vec![([0u128; 2], [0u128; 2]); (SIZE * SIZE) as usize];
        for y in 0..HEIGHT {
            let wy = base + y;
            if !(0..256).contains(&wy) {
                continue;
            }
            let (h, bit) = ((wy >> 7) as usize, 1u128 << (wy & 127));
            for z in 0..SIZE {
                for x in 0..SIZE {
                    let v = self.blocks[((y * SIZE + z) * SIZE + x) as usize];
                    if v != b::AIR {
                        let c = &mut out[(z * SIZE + x) as usize];
                        c.0[h] |= bit;
                        if v == b::END_STONE {
                            c.1[h] |= bit;
                        }
                    }
                }
            }
        }
        out
    }
    /// [`crate::end::IslandCompact`] of these blocks (structure y < 128; solid = non-air, stone = end stone), what
    /// GT's End vein test sees (HEE ores and Homeland cellars are not end stone).
    pub fn compact(&self) -> crate::end::IslandCompact {
        let mut cols = vec![(0u128, 0u128); (SIZE * SIZE) as usize];
        for y in 0..HEIGHT.min(128) {
            let bit = 1u128 << y;
            for z in 0..SIZE {
                for x in 0..SIZE {
                    let v = self.blocks[((y * SIZE + z) * SIZE + x) as usize];
                    if v != b::AIR {
                        let c = &mut cols[(z * SIZE + x) as usize];
                        c.0 |= bit;
                        if v == b::END_STONE {
                            c.1 |= bit;
                        }
                    }
                }
            }
        }
        crate::end::IslandCompact { base: 20 + self.island.bottom_y, cols }
    }
    /// `LargeStructureWorld.getHighestY`: highest non-air y > 0, else 0.
    pub fn highest_y(&self, x: i32, z: i32) -> i32 {
        (1..HEIGHT).rev().find(|&y| !self.air(x, y, z)).unwrap_or(0)
    }
}

/// `MathHelper.cos` (sine table).
fn mcos(f: f32) -> f32 {
    static T: std::sync::OnceLock<Vec<f32>> = std::sync::OnceLock::new();
    let t = T.get_or_init(|| (0..65536).map(|i| (i as f64 * std::f64::consts::PI * 2.0 / 65536.0).sin() as f32).collect());
    t[((f * 10430.378 + 16384.0) as i32 & 65535) as usize]
}

/// Island blocks after terrain, caves, ores and (Homeland) the hidden cellars. ~0.3 s native.
pub fn island_blocks(seed: i64, cx: i32, cz: i32) -> IslandBlocks {
    let (island, mut r, sm, surface) = island_terrain(seed, cx, cz);
    let mut w = IslandBlocks { island, blocks: vec![b::AIR; (SIZE * SIZE * HEIGHT) as usize] };
    for z in 0..SIZE {
        for x in 0..SIZE {
            let k = (z * SIZE + x) as usize;
            let mut m = sm[k];
            while m != 0 {
                let y = m.trailing_zeros() as i32;
                m &= m - 1;
                let v = if surface[k] >> y & 1 != 0 { b::TERRAIN } else { b::END_STONE };
                w.set(x, y, z, v);
            }
        }
    }
    let isl = w.island.clone();
    ores(&mut w, &isl, &mut r);
    if w.island.biome == 2 && w.island.variation == 2 {
        let mut c = Cellar { pattern: Vec::new() };
        let mut placed = 0;
        let mut attempt = 0;
        while attempt < 20 && placed < 4 + r.next_int_bounded(3) {
            if c.generate(&mut w, &mut r) {
                placed += 1;
            }
            attempt += 1;
        }
    }
    w
}

/// What [`ores`] needs from the island: end stone test, ore placement.
trait OreWorld {
    fn end_stone(&self, x: i32, y: i32, z: i32) -> bool;
    fn place(&mut self, x: i32, y: i32, z: i32, v: u8);
}

impl OreWorld for IslandBlocks {
    fn end_stone(&self, x: i32, y: i32, z: i32) -> bool {
        self.get(x, y, z) == b::END_STONE
    }
    fn place(&mut self, x: i32, y: i32, z: i32, v: u8) {
        self.set(x, y, z, v)
    }
}

/// Compact structure-space columns `(solid, end stone)` (index z·208 + x, bit = structure y < 128).
struct CompactOres<'a>(&'a mut [(u128, u128)]);

impl OreWorld for CompactOres<'_> {
    fn end_stone(&self, x: i32, y: i32, z: i32) -> bool {
        (0..SIZE).contains(&x) && (0..SIZE).contains(&z) && (0..128).contains(&y) && self.0[(z * SIZE + x) as usize].1 >> y & 1 != 0
    }
    fn place(&mut self, x: i32, y: i32, z: i32, _v: u8) {
        self.0[(z * SIZE + x) as usize].1 &= !(1u128 << y);
    }
}

/// What GT's End vein test sees of an island (solid, end stone): terrain + caves + HEE ores (not end stone), and
/// for Homeland the hidden cellars. Same layout as [`crate::end::island_cols_compact`]; non-Homeland islands cost
/// only the ore draws on top of it (Homeland: the full [`island_blocks`], ~0.3 s native).
pub fn island_compact_interior(seed: i64, cx: i32, cz: i32) -> crate::end::IslandCompact {
    let (island, mut r, sm, surface) = island_terrain(seed, cx, cz);
    if island.biome == 2 && island.variation == 2 {
        return island_blocks(seed, cx, cz).compact();
    }
    let mut cols: Vec<(u128, u128)> = sm.iter().zip(&surface).map(|(&c, &s)| (c, c & !s)).collect();
    ores(&mut CompactOres(&mut cols), &island, &mut r);
    crate::end::IslandCompact { base: 20 + island.bottom_y, cols }
}

/// `OreGenerator(8, 0, 8, 200, 55, 200)`: `setup` draws every cluster's positions, `generate` places them on end
/// stone (stardust draws its meta).
fn ores<W: OreWorld>(w: &mut W, island: &Island, r: &mut JavaRandom) {
    // `getOreAmountMultiplier`: Infested Forest 0.6 (Ravaged) / 1, Burning Mountains 1.4 (Mine) / 1.15, Enchanted 1.25
    let v = island.variation;
    let mult = match island.biome {
        0 => if v == 3 { 0.6f32 } else { 1.0 },
        1 => if v == 5 { 1.4 } else { 1.15 },
        _ => 1.25,
    };
    let (min_x, min_y, min_z, max_x, max_y, max_z) = (8, 0, 8, SIZE - 8, 55, SIZE - 8);
    let cluster = |r: &mut JavaRandom, amount: i32| -> (i32, Vec<(i32, i32, i32)>) {
        let xx = r.next_int_bounded(max_x - min_x + 1) + min_x;
        let mut yy = r.next_int_bounded(max_y - min_y + 1) + min_y;
        let zz = r.next_int_bounded(max_z - min_z + 1) + min_z;
        let s = (amount as f64 * 1.75).sqrt();
        if yy < min_y + 14 {
            yy = r.next_int_bounded(max_y - min_y + 1) + min_y;
        }
        let c = |r: &mut JavaRandom, base: i32| -> i32 {
            let a = mcos((r.next_double() * 2.0 * std::f64::consts::PI) as f32);
            (base as f64 + a as f64 * s * r.next_double()).floor() as i32
        };
        let list = (0..amount * 3)
            .map(|_| {
                let x = c(r, xx);
                let y = c(r, yy);
                let z = c(r, zz);
                (x, y, z)
            })
            .collect();
        (amount, list)
    };
    let n1 = (495f32 * mult) as i32;
    let star: Vec<_> = (0..n1)
        .map(|_| {
            let a = 3 + r.next_int_bounded(7);
            cluster(r, a)
        })
        .collect();
    let n2 = (260f32 * mult) as i32;
    let orb: Vec<_> = (0..n2)
        .map(|_| {
            let a = 1 + r.next_int_bounded(3);
            cluster(r, a)
        })
        .collect();
    for (lists, kind) in [(&star, b::STARDUST), (&orb, b::INSTABILITY)] {
        for (amount, list) in lists {
            let mut placed = 0;
            let mut attempt = 0;
            while attempt < amount * 3 && placed < *amount {
                let (x, y, z) = list[attempt as usize];
                if y >= 0 && w.end_stone(x, y, z) {
                    w.place(x, y, z, kind);
                    if kind == b::STARDUST {
                        r.next_int_bounded(15);
                    }
                    placed += 1;
                }
                attempt += 1;
            }
        }
    }
}

const HOR_X: [i32; 8] = [-1, 0, 1, -1, 1, -1, 0, 1];
const HOR_Z: [i32; 8] = [-1, -1, -1, 0, 0, 1, 1, 1];
const OFF_X: [i32; 4] = [0, -1, 0, 1];
const OFF_Z: [i32; 4] = [1, 0, -1, 0];
const OPPOSITE: [usize; 4] = [2, 3, 0, 1];

struct Room {
    x: i32,
    z: i32,
    hw: i32,
    can_use: [bool; 4],
}

impl Room {
    fn useable_side(&self, r: &mut JavaRandom) -> Option<usize> {
        if !self.can_use.iter().any(|&b| b) {
            return None;
        }
        for _ in 0..8 {
            let s = r.next_int_bounded(4) as usize;
            if self.can_use[s] {
                return Some(s);
            }
        }
        (0..4).find(|&s| self.can_use[s])
    }
}

/// `StructureHiddenCellar` (Homeland variation).
struct Cellar {
    pattern: Vec<(i32, i32, i32)>,
}

impl Cellar {
    fn generate(&mut self, w: &mut IslandBlocks, r: &mut JavaRandom) -> bool {
        self.pattern.clear();
        let height = 4 + r.next_int_bounded(4);
        for _ in 0..50 {
            let x = r.next_int_bounded(SIZE - 64) + 32;
            let y = 10 + r.next_int_bounded(30);
            let z = r.next_int_bounded(SIZE - 64) + 32;
            let hw = 3 + r.next_int_bounded(6);
            let Some(first) = self.room(w, r, x, z, hw, y, height) else { continue };
            let mut rooms = vec![first];
            let attempts = 70 + r.next_int_bounded(110);
            let _rooms_left = 5 + r.next_int_bounded(16); // never decremented in HEE: all attempts run
            for _ in 0..attempts {
                let i = r.next_int_bounded(rooms.len() as i32) as usize;
                let Some(side) = rooms[i].useable_side(r) else { continue };
                let (ox, oz) = (OFF_X[side], OFF_Z[side]);
                let mut hw2 = 3 + r.next_int_bounded(6);
                if hw2 == 3 {
                    hw2 = 2;
                }
                let dist = rooms[i].hw + hw2 + 2 + r.next_int_bounded(7);
                let (rx, rz, rhw) = (rooms[i].x, rooms[i].z, rooms[i].hw);
                let Some(mut nr) = self.room(w, r, rx + ox * dist, rz + oz * dist, hw2, y, height) else { continue };
                rooms[i].can_use[side] = false;
                nr.can_use[OPPOSITE[side]] = false;
                let (nx, nz) = (nr.x, nr.z);
                rooms.push(nr);
                self.hall(
                    w,
                    r,
                    (rx + (rhw - 1) * ox - oz * 2, rz + (rhw - 1) * oz - ox * 2),
                    (nx - (hw2 - 1) * ox + oz * 2, nz - (hw2 - 1) * oz + ox * 2),
                    y,
                    height,
                );
            }
            let spots: Vec<(i32, i32, i32)> = rooms.iter().map(|rm| (rm.x, rm.z, rm.hw)).collect();
            for (rx, rz, rhw) in spots {
                self.content(w, r, rx, rz, rhw, y, height);
            }
            self.patterns(w, r);
            return true;
        }
        false
    }

    #[allow(clippy::too_many_arguments)]
    fn room(&mut self, w: &mut IslandBlocks, r: &mut JavaRandom, x: i32, z: i32, hw: i32, by: i32, h: i32) -> Option<Room> {
        let hw = if hw == 3 { 2 } else { hw };
        let hh = h >> 1;
        if w.get(x, by + hh * 2 + 2, z) != b::END_STONE {
            return None;
        }
        for k in 0..8 {
            for yc in -1..=1 {
                if w.get(x + HOR_X[k] * hw, by + hh + yc * (hh + 1), z + HOR_Z[k] * hw) != b::END_STONE {
                    return None;
                }
            }
        }
        let (x1, z1, x2, z2) = (x - hw, z - hw, x + hw, z + hw);
        for xx in x1..=x2 {
            for zz in z1..=z2 {
                for yy in by..=by + h {
                    let edge = xx == x1 || xx == x2 || zz == z1 || zz == z2 || yy == by || yy == by + h;
                    w.set(xx, yy, zz, if edge { b::PERSEGRIT } else { b::AIR });
                    if edge && r.next_int_bounded(60) == 0 {
                        self.pattern.push((xx, yy, zz));
                    }
                }
            }
        }
        Some(Room { x, z, hw, can_use: [true; 4] })
    }

    fn hall(&mut self, w: &mut IslandBlocks, r: &mut JavaRandom, (x1, z1): (i32, i32), (x2, z2): (i32, i32), by: i32, h: i32) {
        for xx in x1.min(x2)..=x1.max(x2) {
            for zz in z1.min(z2)..=z1.max(z2) {
                for yy in by..=by + h {
                    let edge = xx == x1 || xx == x2 || zz == z1 || zz == z2 || yy == by || yy == by + h;
                    if !edge || !w.air(xx, yy, zz) {
                        w.set(xx, yy, zz, if edge { b::PERSEGRIT } else { b::AIR });
                        if edge && r.next_int_bounded(60) == 0 {
                            self.pattern.push((xx, yy, zz));
                        }
                    }
                }
            }
        }
    }

    #[allow(clippy::too_many_arguments)]
    fn content(&mut self, w: &mut IslandBlocks, r: &mut JavaRandom, x: i32, z: i32, hw: i32, by: i32, h: i32) {
        if hw == 2 {
            return;
        }
        // roomContentList: NONE 55, CONNECTING_LINES 9, SPIKES 9, PERSEGRIT_CUBE 8, LOTS_OF_CHESTS 7,
        // FLOATING_CUBES 5, CHEST_PILLARS 4, CHAOTIC_PERSEGRIT 4
        const W: [(u8, i32); 8] = [(0, 55), (1, 9), (2, 9), (3, 8), (4, 7), (5, 5), (7, 4), (6, 4)];
        let mut i = r.next_int_bounded(101);
        let mut ty = 0;
        for (t, wt) in W {
            i -= wt;
            if i < 0 {
                ty = t;
                break;
            }
        }
        let width = hw * 2 - 2;
        match ty {
            1 => {
                let mut to_persegrit = Vec::new();
                let mut lines = 3 + ((hw * (h - 2)) >> 1) + r.next_int_bounded(6 + hw * 2 + h);
                while lines > 0 {
                    let dir = r.next_int_bounded(4);
                    let (mut ax, mut ay, mut az) = (0, 0, 0);
                    if dir == 0 {
                        ax = if r.next_boolean() { -1 } else { 1 };
                    } else if dir == 1 {
                        az = if r.next_boolean() { -1 } else { 1 };
                    } else {
                        ay = if r.next_boolean() { -1 } else { 1 };
                    }
                    for _ in 0..10 {
                        let mut xx = x + r.next_int_bounded(width) - (width >> 1);
                        let mut yy = by + 1 + r.next_int_bounded(h - 1);
                        let mut zz = z + r.next_int_bounded(width) - (width >> 1);
                        let bed = |w: &IslandBlocks, a: i32, b2: i32, c: i32| w.get(a, b2, c) == b::BEDROCK;
                        if bed(w, xx - 1, yy, zz)
                            || bed(w, xx + 1, yy, zz)
                            || bed(w, xx, yy - 1, zz)
                            || bed(w, xx, yy + 1, zz)
                            || bed(w, xx, yy, zz - 1)
                            || bed(w, xx, yy, zz + 1)
                            || !w.air(xx, yy, zz)
                        {
                            continue;
                        }
                        let mut found = false;
                        for _ in 0..=width + 2 {
                            xx -= ax;
                            yy -= ay;
                            zz -= az;
                            if w.get(xx, yy, zz) == b::PERSEGRIT {
                                if (xx - x).abs() <= hw && (zz - z).abs() <= hw {
                                    found = true;
                                }
                                break;
                            }
                        }
                        if !found {
                            continue;
                        }
                        found = false;
                        for _ in 0..=width + 2 {
                            xx += ax;
                            yy += ay;
                            zz += az;
                            if w.get(xx, yy, zz) == b::PERSEGRIT {
                                if (xx - x).abs() <= hw && (zz - z).abs() <= hw {
                                    found = true;
                                }
                                break;
                            }
                        }
                        if !found {
                            continue;
                        }
                        for _ in 0..=width + 2 {
                            xx -= ax;
                            yy -= ay;
                            zz -= az;
                            if w.get(xx, yy, zz) == b::PERSEGRIT {
                                break;
                            }
                            w.set(xx, yy, zz, b::BEDROCK);
                            to_persegrit.push((xx, yy, zz));
                        }
                        break;
                    }
                    lines -= 1;
                }
                for (a, b2, c) in to_persegrit {
                    w.set(a, b2, c, b::PERSEGRIT);
                    if r.next_int_bounded(60) == 0 {
                        self.pattern.push((a, b2, c));
                    }
                }
            }
            2 => {
                let mut spikes = 2 + hw + r.next_int_bounded(4 + 3 * hw);
                while spikes > 0 {
                    let xx = x + r.next_int_bounded(width) - (width >> 1);
                    let mut yy = by + if r.next_boolean() { 1 } else { h - 1 };
                    let zz = z + r.next_int_bounded(width) - (width >> 1);
                    if w.air(xx, yy, zz) {
                        let ay = if yy == by + 1 { 1 } else { -1 };
                        let mut sh = 1 + r.next_int_bounded(h - 3);
                        while sh > 0 {
                            // HEE's checks as written (the +x and ±z edge tests compare with x − (hw ± 1))
                            if (xx == x - (hw - 1) || w.air(xx - 1, yy + ay, zz))
                                && (xx == x - (hw + 1) || w.air(xx + 1, yy + ay, zz))
                                && (zz == z - (hw - 1) || w.air(xx, yy + ay, zz - 1))
                                && (zz == z - (hw - 1) || w.air(xx, yy + ay, zz + 1))
                                && w.air(xx, yy + ay, zz)
                            {
                                w.set(xx, yy, zz, b::PERSEGRIT);
                                yy += ay;
                            } else {
                                break;
                            }
                            sh -= 1;
                        }
                    }
                    spikes -= 1;
                }
            }
            3 => {
                let c = (hw - 3 - r.next_int_bounded(2)).max(1);
                for yy in by + 1..=by + h - 1 {
                    for a in -c..=c {
                        for (px, pz) in [(x + a, z - c), (x + a, z + c), (x - c, z + a), (x + c, z + a)] {
                            w.set(px, yy, pz, b::PERSEGRIT);
                            if r.next_int_bounded(60) == 0 {
                                self.pattern.push((px, yy, pz));
                            }
                        }
                    }
                }
            }
            4 => {
                let mut chests = 10 + r.next_int_bounded(8 + 3 * hw) + 4 * hw;
                while chests > 0 {
                    for _ in 0..4 {
                        let xx = x + r.next_int_bounded(width) - (width >> 1);
                        let zz = z + r.next_int_bounded(width) - (width >> 1);
                        let mut yy = by + 1;
                        if !w.air(xx, yy, zz) {
                            for _ in 0..h - 3 {
                                yy += 1;
                                if w.air(xx, yy, zz) {
                                    break;
                                }
                            }
                            if !w.air(xx, yy, zz) {
                                continue;
                            }
                        }
                        let (mut normal, mut trapped) = (0, 0);
                        for d in 0..4 {
                            match w.get(xx + OFF_X[d], yy, zz + OFF_Z[d]) {
                                b::CHEST => normal += 1,
                                b::TRAPPED_CHEST => trapped += 1,
                                _ => {}
                            }
                        }
                        if normal == 0 && trapped == 0 {
                            let k = if r.next_boolean() { b::CHEST } else { b::TRAPPED_CHEST };
                            r.next_int_bounded(4);
                            w.set(xx, yy, zz, k);
                        } else if normal == 1 && trapped == 0 {
                            r.next_int_bounded(4);
                            w.set(xx, yy, zz, b::TRAPPED_CHEST);
                        } else if normal == 0 && trapped == 1 {
                            r.next_int_bounded(4);
                            w.set(xx, yy, zz, b::CHEST);
                        } else {
                            continue;
                        }
                        if r.next_int_bounded(10) == 0 {
                            r.next_int_bounded(6);
                        }
                        break;
                    }
                    chests -= 1;
                }
            }
            5 => {
                let mut cubes = 5 + r.next_int_bounded(10);
                while cubes > 0 {
                    cubes -= 1;
                    r.next_int_bounded(9); // block pick
                    let inner = 3 + r.next_int_bounded(2);
                    let size = 1 + r.next_int_bounded(inner);
                    let xz_space = hw * 2 - size;
                    if xz_space < 1 {
                        continue;
                    }
                    let y_space = h - size;
                    if y_space < 1 {
                        continue;
                    }
                    for _ in 0..20 {
                        let xx = x - hw + 1 + r.next_int_bounded(xz_space);
                        let yy = by + 1 + r.next_int_bounded(y_space);
                        let zz = z - hw + 1 + r.next_int_bounded(xz_space);
                        let mut ok = true;
                        'c: for px in xx - 1..=xx + size {
                            for py in yy - 1..=yy + size {
                                for pz in zz - 1..=zz + size {
                                    let v = w.get(px, py, pz);
                                    if v != b::AIR && v != b::PERSEGRIT {
                                        ok = false;
                                        break 'c;
                                    }
                                }
                            }
                        }
                        if ok {
                            for px in xx..xx + size {
                                for py in yy..yy + size {
                                    for pz in zz..zz + size {
                                        w.set(px, py, pz, b::ENDERMAN_SOLID);
                                    }
                                }
                            }
                            break;
                        }
                    }
                }
            }
            7 => {
                let mut amount = 2 + hw * (hw - 2) + r.next_int_bounded(5 + hw * 3);
                while amount > 0 {
                    let xx = x - hw + 1 + r.next_int_bounded(hw * 2 - 2);
                    let zz = z - hw + 1 + r.next_int_bounded(hw * 2 - 2);
                    let mut yy = by + 2 + r.next_int_bounded(h - 3);
                    if w.air(xx - 1, by + 1, zz) && w.air(xx + 1, by + 1, zz) && w.air(xx, by + 1, zz - 1) && w.air(xx, by + 1, zz + 1) {
                        w.set(xx, yy, zz, b::CHEST);
                        if r.next_int_bounded(5) <= 1 {
                            r.next_int_bounded(4);
                        }
                        yy -= 1;
                        while yy > by {
                            w.set(xx, yy, zz, b::PERSEGRIT);
                            yy -= 1;
                        }
                    }
                    amount -= 1;
                }
            }
            6 => {
                let n = ((hw * hw * (h - 1)) as f64).sqrt() * (0.85 + r.next_double() * 0.5);
                let mut amount = n.ceil() as i32;
                while amount > 0 {
                    let px = x + r.next_int_bounded(width) - (width >> 1);
                    let py = by + 1 + r.next_int_bounded(h - 1);
                    let pz = z + r.next_int_bounded(width) - (width >> 1);
                    w.set(px, py, pz, b::PERSEGRIT);
                    amount -= 1;
                }
            }
            _ => {}
        }
    }

    fn patterns(&mut self, w: &mut IslandBlocks, r: &mut JavaRandom) {
        let pg = |w: &IslandBlocks, x: i32, y: i32, z: i32| w.get(x, y, z) == b::PERSEGRIT;
        let is_wall = |w: &IslandBlocks, x: i32, y: i32, z: i32| pg(w, x, y - 1, z) || pg(w, x, y + 1, z);
        let is_corner = |w: &IslandBlocks, x: i32, y: i32, z: i32| {
            let (l, rr, u, d, bo, t) =
                (pg(w, x - 1, y, z), pg(w, x + 1, y, z), pg(w, x, y, z - 1), pg(w, x, y, z + 1), pg(w, x, y - 1, z), pg(w, x, y + 1, z));
            (((l && rr) || (u && d)) && ((bo && !t) || (!bo && t)))
                || (t && bo && ((l && !rr) || (u && !d) || (!l && rr) || (!u && d)))
        };
        let mut connections = 0u32;
        let pattern = std::mem::take(&mut self.pattern);
        for &(x0, y0, z0) in &pattern {
            let (mut x, mut y, mut z) = (x0, y0, z0);
            let (mut ax, mut ay, mut az) = (0, 0, 0);
            let inner = 30 + r.next_int_bounded(60);
            let iterations = 6 + r.next_int_bounded(inner);
            if !is_wall(w, x, y, z) {
                if r.next_boolean() {
                    ax = r.next_int_bounded(2) * 2 - 1;
                } else {
                    az = r.next_int_bounded(2) * 2 - 1;
                }
            } else if !pg(w, x - 1, y, z) || !pg(w, x + 1, y, z) {
                if r.next_int_bounded(3) != 0 {
                    ay = r.next_int_bounded(2) * 2 - 1;
                } else {
                    az = r.next_int_bounded(2) * 2 - 1;
                }
            } else if !pg(w, x, y, z - 1) || !pg(w, x, y, z + 1) {
                if r.next_int_bounded(3) != 0 {
                    ay = r.next_int_bounded(2) * 2 - 1;
                } else {
                    ax = r.next_int_bounded(2) * 2 - 1;
                }
            }
            for _ in 0..=iterations {
                x += ax;
                y += ay;
                z += az;
                if is_corner(w, x, y, z) {
                    if ax != 0 {
                        az = if pg(w, x, y, z - 1) { -1 } else { 1 };
                        ax = 0;
                    } else if az != 0 {
                        ax = if pg(w, x - 1, y, z) { -1 } else { 1 };
                        az = 0;
                    }
                } else if r.next_int_bounded(12) == 0 {
                    let (mut nx, mut ny, mut nz) = (0, 0, 0);
                    if !is_wall(w, x, y, z) {
                        if r.next_boolean() {
                            nx = if r.next_boolean() { -1 } else { 1 };
                        } else {
                            nz = if r.next_boolean() { -1 } else { 1 };
                        }
                    } else if !pg(w, x - 1, y, z) || !pg(w, x + 1, y, z) {
                        if r.next_boolean() {
                            ny = r.next_int_bounded(2) * 2 - 1;
                        } else {
                            nz = r.next_int_bounded(2) * 2 - 1;
                        }
                    } else if !pg(w, x, y, z - 1) || !pg(w, x, y, z + 1) {
                        if r.next_boolean() {
                            nx = r.next_int_bounded(2) * 2 - 1;
                        } else {
                            ny = r.next_int_bounded(2) * 2 - 1;
                        }
                    }
                    if !is_corner(w, x + nx, y + ny, z + nz) {
                        ax = nx;
                        ay = ny;
                        az = nz;
                    }
                }
                if pg(w, x, y, z) {
                    connections += 1; // meta 15 only: same block kind
                } else {
                    break;
                }
            }
        }
        // `BlockPersegrit.getConnectionMeta`: two `nextBoolean` per connection
        for _ in 0..connections {
            r.next_boolean();
            r.next_boolean();
        }
    }
}
