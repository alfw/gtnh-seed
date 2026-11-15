//! Vanilla 1.7.10 Nether fortress layout (`MapGenNetherBridge.Start` + `StructureNetherBridgePieces`), pure seed
//! maths.
//!
//! Structure Random: `MapGenNetherBridge.canSpawnStructureAtCoords` reseeds `MapGenBase.rand` itself
//! (`setSeed((rx ^ rz << 4) ^ worldSeed); nextInt(); nextInt(3); nextInt(8); nextInt(8)`), and
//! `getStructureStart` continues on the same rand. In GTNH that rand is Hodgepodge's `StdLCG` (raw `setSeed`), see
//! [`crate::nether::fortress_in_region`].
//!
//! `Start(rand, cx*16+2, cz*16+2)`: a `Crossing3` (`coordBaseMode = nextInt(4)`, box 19×10×19 at y 64..73), fresh
//! weight counters, `buildComponent`, then random pops of the pending list until empty, `updateBoundingBox()`,
//! `setRandomHeight(48, 70)`: `n = 70 − 48 + 1 − ySize; y = n > 1 ? 48 + nextInt(n) : 48`, shift every piece by
//! `y − minY`.
//!
//! Faithful quirks: a failed weighted pick keeps iterating with the negative remainder (next entries are tried);
//! `getTotalWeight` is −1 once every remaining entry is unlimited (→ only `End` pieces); the `End` returned when a
//! connection point is more than 112 blocks from the start is created (and draws `nextInt()`) but never added;
//! `Corridor4` draws its two `nextInt(8) > 0` flags interleaved with the children they decide.

use crate::java_random::JavaRandom;

/// Piece class (`StructureNetherBridgePieces$…`).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum PieceKind {
    Start,
    Straight,
    Crossing3,
    Crossing,
    Stairs,
    Throne,
    Entrance,
    Corridor5,
    Crossing2,
    Corridor2,
    Corridor,
    Corridor3,
    Corridor4,
    NetherStalkRoom,
    End,
}

impl PieceKind {
    pub fn java_name(self) -> &'static str {
        match self {
            PieceKind::Start => "Start",
            PieceKind::Straight => "Straight",
            PieceKind::Crossing3 => "Crossing3",
            PieceKind::Crossing => "Crossing",
            PieceKind::Stairs => "Stairs",
            PieceKind::Throne => "Throne",
            PieceKind::Entrance => "Entrance",
            PieceKind::Corridor5 => "Corridor5",
            PieceKind::Crossing2 => "Crossing2",
            PieceKind::Corridor2 => "Corridor2",
            PieceKind::Corridor => "Corridor",
            PieceKind::Corridor3 => "Corridor3",
            PieceKind::Corridor4 => "Corridor4",
            PieceKind::NetherStalkRoom => "NetherStalkRoom",
            PieceKind::End => "End",
        }
    }

    /// Bridge (open-air walkway) vs corridor (enclosed castle part): what the viewer colours.
    pub fn is_bridge(self) -> bool {
        matches!(
            self,
            PieceKind::Start
                | PieceKind::Straight
                | PieceKind::Crossing3
                | PieceKind::Crossing
                | PieceKind::Stairs
                | PieceKind::Throne
                | PieceKind::Entrance
                | PieceKind::End
        )
    }

    /// `getComponentToAddBoundingBox` offsets and size: (ox, oy, oz, w, h, d).
    fn dims(self) -> (i32, i32, i32, i32, i32, i32) {
        match self {
            PieceKind::Straight => (-1, -3, 0, 5, 10, 19),
            PieceKind::Start | PieceKind::Crossing3 => (-8, -3, 0, 19, 10, 19),
            PieceKind::Crossing => (-2, 0, 0, 7, 9, 7),
            PieceKind::Stairs => (-2, 0, 0, 7, 11, 7),
            PieceKind::Throne => (-2, 0, 0, 7, 8, 9),
            PieceKind::Entrance => (-5, -3, 0, 13, 14, 13),
            PieceKind::Corridor5
            | PieceKind::Crossing2
            | PieceKind::Corridor2
            | PieceKind::Corridor => (-1, 0, 0, 5, 7, 5),
            PieceKind::Corridor3 => (-1, -7, 0, 5, 14, 10),
            PieceKind::Corridor4 => (-3, 0, 0, 9, 7, 9),
            PieceKind::NetherStalkRoom => (-5, -3, 0, 13, 14, 13),
            PieceKind::End => (-1, -3, 0, 5, 10, 8),
        }
    }
}

/// `[minX, minY, minZ, maxX, maxY, maxZ]`, inclusive.
pub type BBox = [i32; 6];

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Piece {
    pub kind: PieceKind,
    pub bbox: BBox,
    /// `coordBaseMode` (0 = +Z, 1 = −X, 2 = −Z, 3 = +X).
    pub orientation: i32,
    /// `componentType` (depth).
    pub depth: i32,
    /// `Corridor` / `Corridor2` chest flag (`nextInt(3) == 0` in the constructor)
    pub chest: bool,
}

impl Piece {
    /// `StructureComponent.getXWithOffset / getYWithOffset / getZWithOffset`.
    pub fn world(&self, x: i32, y: i32, z: i32) -> (i32, i32, i32) {
        let b = &self.bbox;
        let wx = match self.orientation {
            0 | 2 => b[0] + x,
            1 => b[3] - z,
            3 => b[0] + z,
            _ => x,
        };
        let wz = match self.orientation {
            0 => b[2] + z,
            1 | 3 => b[2] + x,
            2 => b[5] - z,
            _ => z,
        };
        (wx, y + b[1], wz)
    }
}

/// One fortress layout.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FortressLayout {
    pub chunk_x: i32,
    pub chunk_z: i32,
    /// Union of all piece boxes after the height shift.
    pub bbox: BBox,
    /// `StructureStart.components` order (Start first).
    pub pieces: Vec<Piece>,
}

/// A nether-wart room: the two soul-sand / wart patches (world coords, each an inclusive box of 2×5 wart blocks).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct WartRoom {
    pub room: BBox,
    /// Wart blocks (y = soul sand + 1): two inclusive boxes `[x0, y, z0, x1, y, z1]`.
    pub wart: [BBox; 2],
}

impl FortressLayout {
    /// Blaze spawners: one per `Throne` piece at local (3, 5, 5).
    pub fn spawners(&self) -> Vec<(i32, i32, i32)> {
        self.pieces
            .iter()
            .filter(|p| p.kind == PieceKind::Throne)
            .map(|p| p.world(3, 5, 5))
            .collect()
    }

    /// Nether-wart rooms (`NetherStalkRoom`: soul sand at local y 4 over x 3..4 and 8..9, z 4..8; wart at y 5).
    pub fn wart_rooms(&self) -> Vec<WartRoom> {
        self.pieces
            .iter()
            .filter(|p| p.kind == PieceKind::NetherStalkRoom)
            .map(|p| {
                let patch = |x0: i32, x1: i32| {
                    let a = p.world(x0, 5, 4);
                    let b = p.world(x1, 5, 8);
                    [
                        a.0.min(b.0),
                        a.1,
                        a.2.min(b.2),
                        a.0.max(b.0),
                        a.1,
                        a.2.max(b.2),
                    ]
                };
                WartRoom {
                    room: p.bbox,
                    wart: [patch(3, 4), patch(8, 9)],
                }
            })
            .collect()
    }

    /// Chest blocks (`Corridor` local (3, 2, 3), `Corridor2` local (1, 2, 3)) of pieces whose flag is set, in
    /// component order.
    pub fn chests(&self) -> Vec<(i32, i32, i32)> {
        self.pieces
            .iter()
            .filter(|p| p.chest)
            .map(|p| match p.kind {
                PieceKind::Corridor => p.world(3, 2, 3),
                _ => p.world(1, 2, 3),
            })
            .collect()
    }

    /// Centre of the bounding box (x, z).
    pub fn centre(&self) -> (i32, i32) {
        (
            (self.bbox[0] + self.bbox[3]) / 2,
            (self.bbox[2] + self.bbox[5]) / 2,
        )
    }
}

// ---------------------------------------------------------------------------------------------------------

#[allow(clippy::too_many_arguments)]
fn comp_box(
    x: i32,
    y: i32,
    z: i32,
    ox: i32,
    oy: i32,
    oz: i32,
    w: i32,
    h: i32,
    d: i32,
    mode: i32,
) -> BBox {
    match mode {
        1 => [
            x - d + 1 + oz,
            y + oy,
            z + ox,
            x + oz,
            y + h - 1 + oy,
            z + w - 1 + ox,
        ],
        2 => [
            x + ox,
            y + oy,
            z - d + 1 + oz,
            x + w - 1 + ox,
            y + h - 1 + oy,
            z + oz,
        ],
        3 => [
            x + oz,
            y + oy,
            z + ox,
            x + d - 1 + oz,
            y + h - 1 + oy,
            z + w - 1 + ox,
        ],
        _ => [
            x + ox,
            y + oy,
            z + oz,
            x + w - 1 + ox,
            y + h - 1 + oy,
            z + d - 1 + oz,
        ],
    }
}

#[inline]
fn intersects(a: &BBox, b: &BBox) -> bool {
    a[3] >= b[0] && a[0] <= b[3] && a[5] >= b[2] && a[2] <= b[5] && a[4] >= b[1] && a[1] <= b[4]
}

/// (kind, weight, max, allow in a row)
const PRIMARY: [(PieceKind, i32, i32, bool); 6] = [
    (PieceKind::Straight, 30, 0, true),
    (PieceKind::Crossing3, 10, 4, false),
    (PieceKind::Crossing, 10, 4, false),
    (PieceKind::Stairs, 10, 3, false),
    (PieceKind::Throne, 5, 2, false),
    (PieceKind::Entrance, 5, 1, false),
];
const SECONDARY: [(PieceKind, i32, i32, bool); 7] = [
    (PieceKind::Corridor5, 25, 0, true),
    (PieceKind::Crossing2, 15, 5, false),
    (PieceKind::Corridor2, 5, 10, false),
    (PieceKind::Corridor, 5, 10, false),
    (PieceKind::Corridor3, 10, 3, true),
    (PieceKind::Corridor4, 7, 2, false),
    (PieceKind::NetherStalkRoom, 5, 2, false),
];

/// Weight entry id: (secondary list?, index in its table).
type Wid = (bool, usize);

struct Gen {
    rand: JavaRandom,
    pieces: Vec<Piece>,
    pending: Vec<usize>,
    /// Remaining entries per list, table indices in list order.
    lists: [Vec<usize>; 2],
    counts: [[i32; 7]; 2],
    last: Option<Wid>,
    start_min: (i32, i32),
}

impl Gen {
    fn entry(w: Wid) -> (PieceKind, i32, i32, bool) {
        if w.0 {
            SECONDARY[w.1]
        } else {
            PRIMARY[w.1]
        }
    }

    /// `createValidComponent` of `kind` (incl. `End.func_74971_a`). Returns the piece without adding it.
    #[allow(clippy::too_many_arguments)]
    fn create(
        &mut self,
        kind: PieceKind,
        x: i32,
        y: i32,
        z: i32,
        mode: i32,
        depth: i32,
    ) -> Option<Piece> {
        let (ox, oy, oz, w, h, d) = kind.dims();
        let bb = comp_box(x, y, z, ox, oy, oz, w, h, d, mode);
        if bb[1] <= 10 || self.pieces.iter().any(|p| intersects(&p.bbox, &bb)) {
            return None;
        }
        let mut chest = false;
        match kind {
            PieceKind::Corridor | PieceKind::Corridor2 => {
                chest = self.rand.next_int_bounded(3) == 0;
            }
            PieceKind::End => {
                self.rand.next_int(); // fillSeed
            }
            _ => {}
        }
        Some(Piece {
            kind,
            bbox: bb,
            orientation: mode,
            depth,
            chest,
        })
    }

    fn total_weight(&self, list: usize) -> i32 {
        let mut any = false;
        let mut sum = 0;
        for &i in &self.lists[list] {
            let (_, w, max, _) = Self::entry((list == 1, i));
            if max > 0 && self.counts[list][i] < max {
                any = true;
            }
            sum += w;
        }
        if any {
            sum
        } else {
            -1
        }
    }

    /// `Piece.getNextComponent(start, list, components, rand, x, y, z, mode, depth)`.
    #[allow(clippy::too_many_arguments)]
    fn next_weighted(
        &mut self,
        list: usize,
        x: i32,
        y: i32,
        z: i32,
        mode: i32,
        depth: i32,
    ) -> Option<Piece> {
        let total = self.total_weight(list);
        let ok = total > 0 && depth <= 30;
        let mut tries = 0;
        while tries < 5 && ok {
            tries += 1;
            let mut r = self.rand.next_int_bounded(total);
            let entries = self.lists[list].clone();
            for i in entries {
                let wid = (list == 1, i);
                let (kind, w, max, in_row) = Self::entry(wid);
                r -= w;
                if r < 0 {
                    let can_add = max == 0 || self.counts[list][i] < max;
                    if !can_add || (self.last == Some(wid) && !in_row) {
                        break;
                    }
                    if let Some(p) = self.create(kind, x, y, z, mode, depth) {
                        self.counts[list][i] += 1;
                        self.last = Some(wid);
                        if !(max == 0 || self.counts[list][i] < max) {
                            self.lists[list].retain(|&e| e != i);
                        }
                        return Some(p);
                    }
                }
            }
        }
        self.create(PieceKind::End, x, y, z, mode, depth)
    }

    /// `Piece.getNextComponent(start, components, rand, x, y, z, mode, ctype, secondary)`.
    #[allow(clippy::too_many_arguments)]
    fn next(&mut self, x: i32, y: i32, z: i32, mode: i32, ctype: i32, secondary: bool) {
        if (x - self.start_min.0).abs() <= 112 && (z - self.start_min.1).abs() <= 112 {
            if let Some(p) = self.next_weighted(secondary as usize, x, y, z, mode, ctype + 1) {
                self.pieces.push(p);
                self.pending.push(self.pieces.len() - 1);
            }
        } else {
            let _ = self.create(PieceKind::End, x, y, z, mode, ctype); // created, never added
        }
    }

    fn normal(&mut self, i: usize, a: i32, b: i32, sec: bool) {
        let p = &self.pieces[i];
        let (bb, m, t) = (p.bbox, p.orientation, p.depth);
        match m {
            0 => self.next(bb[0] + a, bb[1] + b, bb[5] + 1, m, t, sec),
            1 => self.next(bb[0] - 1, bb[1] + b, bb[2] + a, m, t, sec),
            2 => self.next(bb[0] + a, bb[1] + b, bb[2] - 1, m, t, sec),
            3 => self.next(bb[3] + 1, bb[1] + b, bb[2] + a, m, t, sec),
            _ => {}
        }
    }

    fn left(&mut self, i: usize, a: i32, b: i32, sec: bool) {
        let p = &self.pieces[i];
        let (bb, m, t) = (p.bbox, p.orientation, p.depth);
        match m {
            0 | 2 => self.next(bb[0] - 1, bb[1] + a, bb[2] + b, 1, t, sec),
            1 | 3 => self.next(bb[0] + b, bb[1] + a, bb[2] - 1, 2, t, sec),
            _ => {}
        }
    }

    fn right(&mut self, i: usize, a: i32, b: i32, sec: bool) {
        let p = &self.pieces[i];
        let (bb, m, t) = (p.bbox, p.orientation, p.depth);
        match m {
            0 | 2 => self.next(bb[3] + 1, bb[1] + a, bb[2] + b, 3, t, sec),
            1 | 3 => self.next(bb[0] + b, bb[1] + a, bb[5] + 1, 0, t, sec),
            _ => {}
        }
    }

    fn build(&mut self, i: usize) {
        match self.pieces[i].kind {
            PieceKind::Start | PieceKind::Crossing3 => {
                self.normal(i, 8, 3, false);
                self.left(i, 3, 8, false);
                self.right(i, 3, 8, false);
            }
            PieceKind::Straight => self.normal(i, 1, 3, false),
            PieceKind::Crossing => {
                self.normal(i, 2, 0, false);
                self.left(i, 0, 2, false);
                self.right(i, 0, 2, false);
            }
            PieceKind::Stairs => self.right(i, 6, 2, false),
            PieceKind::Throne | PieceKind::End => {}
            PieceKind::Entrance => self.normal(i, 5, 3, true),
            PieceKind::Corridor5 | PieceKind::Corridor3 => self.normal(i, 1, 0, true),
            PieceKind::Crossing2 => {
                self.normal(i, 1, 0, true);
                self.left(i, 0, 1, true);
                self.right(i, 0, 1, true);
            }
            PieceKind::Corridor2 => self.right(i, 0, 1, true),
            PieceKind::Corridor => self.left(i, 0, 1, true),
            PieceKind::Corridor4 => {
                let m = self.pieces[i].orientation;
                let v = if m == 1 || m == 2 { 5 } else { 1 };
                let f = self.rand.next_int_bounded(8) > 0;
                self.left(i, 0, v, f);
                let f = self.rand.next_int_bounded(8) > 0;
                self.right(i, 0, v, f);
            }
            PieceKind::NetherStalkRoom => {
                self.normal(i, 5, 3, true);
                self.normal(i, 5, 11, true);
            }
        }
    }
}

/// The layout of the fortress starting in chunk (`cx`, `cz`), given the structure rand right after
/// `canSpawnStructureAtCoords` returned true.
pub fn layout_from_rand(mut rand: JavaRandom, cx: i32, cz: i32) -> FortressLayout {
    let (x, z) = ((cx << 4) + 2, (cz << 4) + 2);
    let mode = rand.next_int_bounded(4);
    let start = Piece {
        kind: PieceKind::Start,
        bbox: [x, 64, z, x + 18, 73, z + 18],
        orientation: mode,
        depth: 0,
        chest: false,
    };
    let mut g = Gen {
        rand,
        pieces: vec![start],
        pending: Vec::new(),
        lists: [(0..PRIMARY.len()).collect(), (0..SECONDARY.len()).collect()],
        counts: [[0; 7]; 2],
        last: None,
        start_min: (x, z),
    };
    g.build(0);
    while !g.pending.is_empty() {
        let k = g.rand.next_int_bounded(g.pending.len() as i32) as usize;
        let i = g.pending.remove(k);
        g.build(i);
    }
    let mut bb = g.pieces[0].bbox;
    for p in &g.pieces[1..] {
        bb = [
            bb[0].min(p.bbox[0]),
            bb[1].min(p.bbox[1]),
            bb[2].min(p.bbox[2]),
            bb[3].max(p.bbox[3]),
            bb[4].max(p.bbox[4]),
            bb[5].max(p.bbox[5]),
        ];
    }
    let n = 70 - 48 + 1 - (bb[4] - bb[1] + 1);
    let y = if n > 1 {
        48 + g.rand.next_int_bounded(n)
    } else {
        48
    };
    let dy = y - bb[1];
    bb[1] += dy;
    bb[4] += dy;
    for p in &mut g.pieces {
        p.bbox[1] += dy;
        p.bbox[4] += dy;
    }
    FortressLayout {
        chunk_x: cx,
        chunk_z: cz,
        bbox: bb,
        pieces: g.pieces,
    }
}

/// The fortress layout of a 16×16-chunk region (`chunk >> 4`), if the region has a fortress.
pub fn layout_in_region(seed: i64, region_x: i32, region_z: i32) -> Option<FortressLayout> {
    let (f, rand) = crate::nether::fortress_in_region_rand(seed, region_x, region_z)?;
    Some(layout_from_rand(rand, f.chunk_x, f.chunk_z))
}

/// Layout of the fortress starting in chunk (`cx`, `cz`), if it is a fortress start chunk.
pub fn layout(seed: i64, cx: i32, cz: i32) -> Option<FortressLayout> {
    let l = layout_in_region(seed, cx >> 4, cz >> 4)?;
    (l.chunk_x == cx && l.chunk_z == cz).then_some(l)
}

/// A fortress chest and its slots `(slot, FORTRESS_LOOT index, count)`.
pub type ChestLoot = ((i32, i32, i32), Vec<(u8, u8, u8)>);

/// BOP `ChunkProviderBOPHell.populate` Random of nether chunk (cx, cz), as handed to
/// `MapGenNetherBridge.generateStructuresInChunk`: `setSeed(seed)`, a = `nextLong()/2*2+1`, b likewise,
/// `setSeed(cx·a + cz·b ^ seed)` (plain `java.util.Random`, reseeded after the Pre event).
pub fn hell_rand(seed: i64, cx: i32, cz: i32) -> JavaRandom {
    let mut r = JavaRandom::new(seed);
    let a = r.next_long() / 2 * 2 + 1;
    let b = r.next_long() / 2 * 2 + 1;
    JavaRandom::new((cx as i64).wrapping_mul(a).wrapping_add((cz as i64).wrapping_mul(b)) ^ seed)
}

/// `StructureNetherBridgePieces.Piece.field_111019_a` as GTNH 2.8 runs it: the vanilla 11 entries plus
/// Et Futurum's Pigstep disc appended (weight 5, total 76) — fitted on the 2.19 probe (37 of 37 slots).
/// (name, min, max, weight).
pub const FORTRESS_LOOT: [(&str, i32, i32, i32); 12] = [
    ("minecraft:diamond", 1, 3, 5),
    ("minecraft:iron_ingot", 1, 5, 5),
    ("minecraft:gold_ingot", 1, 3, 15),
    ("minecraft:golden_sword", 1, 1, 5),
    ("minecraft:golden_chestplate", 1, 1, 5),
    ("minecraft:flint_and_steel", 1, 1, 5),
    ("minecraft:nether_wart", 3, 7, 5),
    ("minecraft:saddle", 1, 1, 10),
    ("minecraft:golden_horse_armor", 1, 1, 8),
    ("minecraft:iron_horse_armor", 1, 1, 5),
    ("minecraft:diamond_horse_armor", 1, 1, 3),
    ("etfuturum:pigstep_record", 1, 1, 5),
];

/// Contents of every fortress chest populated by nether chunk (cx, cz): the chunk's [`hell_rand`] runs through
/// `generateStructuresInChunk` (box chunk·16 + 8 ..= +23 on x / z); in fortress pieces only chests draw
/// (`2 + nextInt(4)` picks, then per pick `nextInt(76)`, `min + nextInt(max − min + 1)`, slot `nextInt(27)`).
/// `layouts` = the fortresses reaching the chunk, in structure-map order (two overlapping fortresses are rare;
/// their order would be the fastutil slot order). Returns (chest position, slots (slot, item index, count)).
pub fn chunk_chest_loot(
    seed: i64,
    layouts: &[&FortressLayout],
    cx: i32,
    cz: i32,
) -> Vec<ChestLoot> {
    let (x0, z0) = (cx * 16 + 8, cz * 16 + 8);
    let inside = |p: &(i32, i32, i32)| (x0..x0 + 16).contains(&p.0) && (z0..z0 + 16).contains(&p.2);
    let mut r = hell_rand(seed, cx, cz);
    let total: i32 = FORTRESS_LOOT.iter().map(|e| e.3).sum();
    let mut out = Vec::new();
    for l in layouts {
        for c in l.chests().into_iter().filter(inside) {
            let n = 2 + r.next_int_bounded(4);
            let mut slots = [None::<(u8, u8)>; 27];
            for _ in 0..n {
                let mut x = r.next_int_bounded(total);
                let mut k = 0;
                while x >= FORTRESS_LOOT[k].3 {
                    x -= FORTRESS_LOOT[k].3;
                    k += 1;
                }
                let (_, mn, mx, _) = FORTRESS_LOOT[k];
                let cnt = mn + r.next_int_bounded(mx - mn + 1);
                slots[r.next_int_bounded(27) as usize] = Some((k as u8, cnt as u8));
            }
            out.push((
                c,
                slots.iter().enumerate().filter_map(|(s, v)| v.map(|(k, n)| (s as u8, k, n))).collect(),
            ));
        }
    }
    out
}

/// The chunk that populates block (x, z) (its +8 box holds it).
pub fn populating_chunk(x: i32, z: i32) -> (i32, i32) {
    ((x - 8) >> 4, (z - 8) >> 4)
}
