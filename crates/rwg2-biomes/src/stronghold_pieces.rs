//! Vanilla 1.7.10 stronghold layout (`StructureStrongholdPieces` + `MapGenStronghold.Start`), pure seed maths.
//!
//! Structure Random (`MapGenBase.func_151539_a` + `MapGenStructure.func_151538_a`):
//! ```text
//! rand.setSeed(worldSeed); l = rand.nextLong(); m = rand.nextLong();   // rand = Hodgepodge StdLCG, see below
//! rand.setSeed((long)cx * l ^ (long)cz * m ^ worldSeed); rand.nextInt();
//! do start = new Start(world, rand, cx, cz)                     // same rand, continues on retry
//! while (start.components.isEmpty() || stairs2.strongholdPortalRoom == null);
//! ```
//! `Start`: `prepareStructurePieces()`, `Stairs2(0, rand, cx*16+2, cz*16+2)` (box y 64..74, `coordBaseMode =
//! nextInt(4)`), `buildComponent`, then pop random entries of the pending list until empty, `updateBoundingBox()`,
//! `markAvailableHeight(world, rand, 10)`. In 1.7.10 the latter only uses the constant 63 (not terrain):
//! `h = ySize + 1; if (h < 53) h += nextInt(53 - h); shift = h - maxY`, applied to every piece.
//!
//! GTNH: Hodgepodge (`fastRandom=true`, default) replaces `MapGenBase.rand` with `StdLCG`, whose `setSeed` stores the
//! seed raw (no `^ 0x5DEECE66D`). Same LCG/`next(bits)` otherwise. With java.util.Random nothing matches the pack.
//!
//! Faithful quirks kept: the `RightTurn` weight spawns a `LeftTurn` (it inherits `LeftTurn.findValidPlacement`);
//! when a weighted pick fails placement the iterator keeps going with the negative remainder, trying the next
//! entries; a forced `strongComponentType` (Crossing after the start stairs) neither counts nor sets the
//! "last weight". Verified piece-for-piece against the pack dump (`tests/stronghold_pieces.rs`).

use crate::java_random::JavaRandom;

/// Piece class (`StructureStrongholdPieces$…`).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum PieceKind {
    Stairs2,
    Straight,
    Prison,
    LeftTurn,
    RoomCrossing,
    StairsStraight,
    Stairs,
    Crossing,
    ChestCorridor,
    Library,
    PortalRoom,
    Corridor,
}

impl PieceKind {
    /// Java simple name (`StructureStrongholdPieces$<name>`).
    pub fn java_name(self) -> &'static str {
        match self {
            PieceKind::Stairs2 => "Stairs2",
            PieceKind::Straight => "Straight",
            PieceKind::Prison => "Prison",
            PieceKind::LeftTurn => "LeftTurn",
            PieceKind::RoomCrossing => "RoomCrossing",
            PieceKind::StairsStraight => "StairsStraight",
            PieceKind::Stairs => "Stairs",
            PieceKind::Crossing => "Crossing",
            PieceKind::ChestCorridor => "ChestCorridor",
            PieceKind::Library => "Library",
            PieceKind::PortalRoom => "PortalRoom",
            PieceKind::Corridor => "Corridor",
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
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Stronghold {
    /// Start box (union of pieces) after the height shift.
    pub bbox: BBox,
    /// All pieces in `StructureStart.components` order (Stairs2 first).
    pub pieces: Vec<Piece>,
    /// Index into `pieces` of the portal room.
    pub portal_room: usize,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PortalRoom {
    pub bbox: BBox,
    pub orientation: i32,
    /// Centre block of the 5×5 end-portal-frame ring (frame y; the 3×3 portal interior is centred here).
    pub eye_frame_centre: (i32, i32, i32),
    /// `func_151553_a` of the room (box centre) — what `/locate`-style lookups and eyes of ender target.
    pub box_centre: (i32, i32, i32),
}

// ---------------------------------------------------------------------------------------------------------

#[derive(Clone, Copy)]
enum Extra {
    None,
    Straight {
        x: bool,
        z: bool,
    },
    Crossing {
        ll: bool,
        lh: bool,
        rl: bool,
        rh: bool,
    },
    Stairs {
        source: bool,
    },
}

#[derive(Clone)]
struct P {
    kind: PieceKind,
    bb: BBox,
    mode: i32,
    ctype: i32,
    extra: Extra,
}

/// `StructureBoundingBox.getComponentToAddBoundingBox`.
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

/// Weight table entries: (class, weight, limit). Index 3 is the `RightTurn` entry.
const W_STRAIGHT: usize = 0;
const W_PRISON: usize = 1;
const W_LEFT: usize = 2;
const W_RIGHT: usize = 3;
const W_ROOM: usize = 4;
const W_STAIRS_STRAIGHT: usize = 5;
const W_STAIRS: usize = 6;
const W_CROSSING: usize = 7;
const W_CHEST: usize = 8;
const W_LIBRARY: usize = 9;
const W_PORTAL: usize = 10;
const WEIGHTS: [(i32, i32); 11] = [
    (40, 0),
    (5, 5),
    (20, 0),
    (20, 0),
    (10, 6),
    (5, 5),
    (5, 5),
    (5, 4),
    (5, 4),
    (10, 2),
    (20, 1),
];

struct Gen<'a> {
    rand: &'a mut JavaRandom,
    comps: Vec<P>,
    pending: Vec<usize>,
    /// `structurePieceList` (indices into WEIGHTS), insertion order.
    list: Vec<usize>,
    spawned: [i32; 11],
    strong: Option<usize>,
    total_weight: i32,
    /// `Stairs2.strongholdPieceWeight`.
    last: Option<usize>,
    portal: Option<usize>,
}

impl<'a> Gen<'a> {
    fn can_spawn_type(&self, w: usize, ctype: i32) -> bool {
        let (_, lim) = WEIGHTS[w];
        let base = lim == 0 || self.spawned[w] < lim;
        match w {
            W_LIBRARY => base && ctype > 4,
            W_PORTAL => base && ctype > 5,
            _ => base,
        }
    }

    fn free(&self, bb: &BBox) -> bool {
        bb[1] > 10 && !self.comps.iter().any(|c| intersects(&c.bb, bb))
    }

    fn door(&mut self) {
        self.rand.next_int_bounded(5);
    }

    /// `getStrongholdComponentFromWeightedPiece`.
    #[allow(clippy::wrong_self_convention)]
    fn from_weight(
        &mut self,
        w: usize,
        x: i32,
        y: i32,
        z: i32,
        mode: i32,
        ctype: i32,
    ) -> Option<P> {
        let (kind, ox, oy, w_, h, d) = match w {
            W_STRAIGHT => (PieceKind::Straight, -1, -1, 5, 5, 7),
            W_PRISON => (PieceKind::Prison, -1, -1, 9, 5, 11),
            W_LEFT | W_RIGHT => (PieceKind::LeftTurn, -1, -1, 5, 5, 5),
            W_ROOM => (PieceKind::RoomCrossing, -4, -1, 11, 7, 11),
            W_STAIRS_STRAIGHT => (PieceKind::StairsStraight, -1, -7, 5, 11, 8),
            W_STAIRS => (PieceKind::Stairs, -1, -7, 5, 11, 5),
            W_CROSSING => (PieceKind::Crossing, -4, -3, 10, 9, 11),
            W_CHEST => (PieceKind::ChestCorridor, -1, -1, 5, 5, 7),
            W_LIBRARY => (PieceKind::Library, -4, -1, 14, 11, 15),
            W_PORTAL => (PieceKind::PortalRoom, -4, -1, 11, 8, 16),
            _ => unreachable!(),
        };
        let mut bb = comp_box(x, y, z, ox, oy, 0, w_, h, d, mode);
        if !self.free(&bb) {
            if kind != PieceKind::Library {
                return None;
            }
            bb = comp_box(x, y, z, -4, -1, 0, 14, 6, 15, mode);
            if !self.free(&bb) {
                return None;
            }
        }
        let extra = match kind {
            PieceKind::PortalRoom => Extra::None,
            PieceKind::Straight => {
                self.door();
                let ex = self.rand.next_int_bounded(2) == 0;
                let ez = self.rand.next_int_bounded(2) == 0;
                Extra::Straight { x: ex, z: ez }
            }
            PieceKind::RoomCrossing => {
                self.door();
                self.rand.next_int_bounded(5);
                Extra::None
            }
            PieceKind::Crossing => {
                self.door();
                let ll = self.rand.next_boolean();
                let lh = self.rand.next_boolean();
                let rl = self.rand.next_boolean();
                let rh = self.rand.next_int_bounded(3) > 0;
                Extra::Crossing { ll, lh, rl, rh }
            }
            PieceKind::Stairs => {
                self.door();
                Extra::Stairs { source: false }
            }
            _ => {
                self.door();
                Extra::None
            }
        };
        Some(P {
            kind,
            bb,
            mode,
            ctype,
            extra,
        })
    }

    /// `getNextComponent`.
    fn next_component(&mut self, x: i32, y: i32, z: i32, mode: i32, ctype: i32) -> Option<P> {
        // canAddStructurePieces
        let mut any = false;
        self.total_weight = 0;
        for &w in &self.list {
            let (wt, lim) = WEIGHTS[w];
            if lim > 0 && self.spawned[w] < lim {
                any = true;
            }
            self.total_weight += wt;
        }
        if !any {
            return None;
        }
        if let Some(s) = self.strong.take() {
            if let Some(p) = self.from_weight(s, x, y, z, mode, ctype) {
                return Some(p);
            }
        }
        for _ in 0..5 {
            let mut r = self.rand.next_int_bounded(self.total_weight);
            let mut i = 0;
            while i < self.list.len() {
                let w = self.list[i];
                i += 1;
                r -= WEIGHTS[w].0;
                if r < 0 {
                    if !self.can_spawn_type(w, ctype) || Some(w) == self.last {
                        break;
                    }
                    if let Some(p) = self.from_weight(w, x, y, z, mode, ctype) {
                        self.spawned[w] += 1;
                        self.last = Some(w);
                        let lim = WEIGHTS[w].1;
                        if !(lim == 0 || self.spawned[w] < lim) {
                            self.list.retain(|&e| e != w);
                        }
                        return Some(p);
                    }
                }
            }
        }
        // Corridor.func_74992_a
        let bb = comp_box(x, y, z, -1, -1, 0, 5, 5, 4, mode);
        let hit = self.comps.iter().find(|c| intersects(&c.bb, &bb))?.bb;
        if hit[1] == bb[1] {
            for k in (1..=3).rev() {
                let b = comp_box(x, y, z, -1, -1, 0, 5, 5, k - 1, mode);
                if !intersects(&hit, &b) {
                    let b = comp_box(x, y, z, -1, -1, 0, 5, 5, k, mode);
                    if b[1] > 1 {
                        return Some(P {
                            kind: PieceKind::Corridor,
                            bb: b,
                            mode,
                            ctype,
                            extra: Extra::None,
                        });
                    }
                    return None;
                }
            }
        }
        None
    }

    /// `getNextValidComponent`.
    fn next_valid(&mut self, x: i32, y: i32, z: i32, mode: i32, ctype: i32) {
        if ctype > 50 {
            return;
        }
        let s = self.comps[0].bb;
        if (x - s[0]).abs() <= 112 && (z - s[2]).abs() <= 112 {
            if let Some(p) = self.next_component(x, y, z, mode, ctype + 1) {
                let idx = self.comps.len();
                self.comps.push(p);
                self.pending.push(idx);
            }
        }
    }

    fn normal(&mut self, i: usize, a: i32, b: i32) {
        let P {
            bb, mode, ctype, ..
        } = self.comps[i];
        match mode {
            0 => self.next_valid(bb[0] + a, bb[1] + b, bb[5] + 1, mode, ctype),
            1 => self.next_valid(bb[0] - 1, bb[1] + b, bb[2] + a, mode, ctype),
            2 => self.next_valid(bb[0] + a, bb[1] + b, bb[2] - 1, mode, ctype),
            3 => self.next_valid(bb[3] + 1, bb[1] + b, bb[2] + a, mode, ctype),
            _ => {}
        }
    }

    fn comp_x(&mut self, i: usize, a: i32, b: i32) {
        let P {
            bb, mode, ctype, ..
        } = self.comps[i];
        match mode {
            0 | 2 => self.next_valid(bb[0] - 1, bb[1] + a, bb[2] + b, 1, ctype),
            1 | 3 => self.next_valid(bb[0] + b, bb[1] + a, bb[2] - 1, 2, ctype),
            _ => {}
        }
    }

    fn comp_z(&mut self, i: usize, a: i32, b: i32) {
        let P {
            bb, mode, ctype, ..
        } = self.comps[i];
        match mode {
            0 | 2 => self.next_valid(bb[3] + 1, bb[1] + a, bb[2] + b, 3, ctype),
            1 | 3 => self.next_valid(bb[0] + b, bb[1] + a, bb[5] + 1, 0, ctype),
            _ => {}
        }
    }

    fn build(&mut self, i: usize) {
        let (kind, mode, extra) = (self.comps[i].kind, self.comps[i].mode, self.comps[i].extra);
        match kind {
            PieceKind::Stairs2 | PieceKind::Stairs => {
                if let Extra::Stairs { source: true } = extra {
                    self.strong = Some(W_CROSSING);
                }
                self.normal(i, 1, 1);
            }
            PieceKind::Straight => {
                self.normal(i, 1, 1);
                if let Extra::Straight { x, z } = extra {
                    if x {
                        self.comp_x(i, 1, 2);
                    }
                    if z {
                        self.comp_z(i, 1, 2);
                    }
                }
            }
            PieceKind::Prison | PieceKind::ChestCorridor | PieceKind::StairsStraight => {
                self.normal(i, 1, 1)
            }
            PieceKind::RoomCrossing => {
                self.normal(i, 4, 1);
                self.comp_x(i, 1, 4);
                self.comp_z(i, 1, 4);
            }
            PieceKind::LeftTurn => {
                if mode != 2 && mode != 3 {
                    self.comp_z(i, 1, 1);
                } else {
                    self.comp_x(i, 1, 1);
                }
            }
            PieceKind::Crossing => {
                let (mut a, mut b) = (3, 5);
                if mode == 1 || mode == 2 {
                    a = 8 - a;
                    b = 8 - b;
                }
                self.normal(i, 5, 1);
                if let Extra::Crossing { ll, lh, rl, rh } = extra {
                    if ll {
                        self.comp_x(i, a, 1);
                    }
                    if lh {
                        self.comp_x(i, b, 7);
                    }
                    if rl {
                        self.comp_z(i, a, 1);
                    }
                    if rh {
                        self.comp_z(i, b, 7);
                    }
                }
            }
            PieceKind::PortalRoom => self.portal = Some(i),
            PieceKind::Library | PieceKind::Corridor => {}
        }
    }
}

/// One `MapGenStronghold.Start` attempt. Returns `(start box, pieces, portal index)`.
fn start_attempt(rand: &mut JavaRandom, cx: i32, cz: i32) -> (BBox, Vec<P>, Option<usize>) {
    let mut g = Gen {
        rand,
        comps: Vec::with_capacity(256),
        pending: Vec::with_capacity(64),
        list: (0..WEIGHTS.len()).collect(),
        spawned: [0; 11],
        strong: None,
        total_weight: 0,
        last: None,
        portal: None,
    };
    let (x, z) = ((cx << 4) + 2, (cz << 4) + 2);
    let mode = g.rand.next_int_bounded(4);
    g.comps.push(P {
        kind: PieceKind::Stairs2,
        bb: [x, 64, z, x + 4, 74, z + 4],
        mode,
        ctype: 0,
        extra: Extra::Stairs { source: true },
    });
    g.build(0);
    while !g.pending.is_empty() {
        let k = g.rand.next_int_bounded(g.pending.len() as i32) as usize;
        let i = g.pending.remove(k);
        g.build(i);
    }
    // updateBoundingBox
    let mut bb = [i32::MAX, i32::MAX, i32::MAX, i32::MIN, i32::MIN, i32::MIN];
    for c in &g.comps {
        for a in 0..3 {
            bb[a] = bb[a].min(c.bb[a]);
            bb[a + 3] = bb[a + 3].max(c.bb[a + 3]);
        }
    }
    // markAvailableHeight(world, rand, 10)
    let k = 63 - 10;
    let mut l = bb[4] - bb[1] + 1 + 1;
    if l < k {
        l += g.rand.next_int_bounded(k - l);
    }
    let dy = l - bb[4];
    bb[1] += dy;
    bb[4] += dy;
    for c in &mut g.comps {
        c.bb[1] += dy;
        c.bb[4] += dy;
    }
    let portal = g.portal;
    (bb, g.comps, portal)
}

/// Hodgepodge `StdLCG.setSeed` (GTNH `fastRandom=true`, mixin `fastload.rand.MixinMapGenBase`): stores the seed raw,
/// without java.util.Random's `^ 0x5DEECE66D` scramble. Same LCG otherwise.
fn std_lcg(seed: i64) -> JavaRandom {
    JavaRandom::new(seed ^ 0x5DEECE66D)
}

/// Structure Random for chunk `(cx, cz)` as `MapGenStructure` leaves it right before `getStructureStart`.
/// In GTNH `MapGenBase.rand` is Hodgepodge's `StdLCG`, so both `setSeed` calls skip the scramble.
fn structure_rand(world_seed: i64, cx: i32, cz: i32) -> JavaRandom {
    let mut r = std_lcg(world_seed);
    let l = r.next_long();
    let m = r.next_long();
    let mut r = std_lcg((cx as i64).wrapping_mul(l) ^ (cz as i64).wrapping_mul(m) ^ world_seed);
    r.next_int();
    r
}

/// Full stronghold layout for a start chunk from `structures::stronghold_ring`. `None` only if 100 consecutive
/// attempts lack a portal room (vanilla would loop forever; never observed).
pub fn stronghold(world_seed: i64, start_chunk: (i32, i32)) -> Option<Stronghold> {
    let mut rand = structure_rand(world_seed, start_chunk.0, start_chunk.1);
    for _ in 0..100 {
        let (bbox, comps, portal) = start_attempt(&mut rand, start_chunk.0, start_chunk.1);
        if let Some(pi) = portal {
            let pieces = comps
                .into_iter()
                .map(|c| Piece {
                    kind: c.kind,
                    bbox: c.bb,
                    orientation: c.mode,
                    depth: c.ctype,
                })
                .collect();
            return Some(Stronghold {
                bbox,
                pieces,
                portal_room: pi,
            });
        }
    }
    None
}

/// All piece boxes (for outlines), `components` order.
pub fn pieces(world_seed: i64, start_chunk: (i32, i32)) -> Vec<Piece> {
    stronghold(world_seed, start_chunk)
        .map(|s| s.pieces)
        .unwrap_or_default()
}

/// Portal room box and end-portal-frame ring centre.
pub fn portal_room(world_seed: i64, start_chunk: (i32, i32)) -> Option<PortalRoom> {
    let s = stronghold(world_seed, start_chunk)?;
    Some(portal_room_of(&s.pieces[s.portal_room]))
}

/// Derive the frame position from a `PortalRoom` piece. The frame ring is local x 3..7, z 8..12 at local y 3
/// (`addComponentParts`), so its centre is local (5, 3, 10), mapped with `getX/Y/ZWithOffset`.
pub fn portal_room_of(p: &Piece) -> PortalRoom {
    let bb = p.bbox;
    let (lx, ly, lz) = (5, 3, 10);
    let x = match p.orientation {
        1 => bb[3] - lz,
        3 => bb[0] + lz,
        _ => bb[0] + lx,
    };
    let z = match p.orientation {
        0 => bb[2] + lz,
        2 => bb[5] - lz,
        _ => bb[2] + lx,
    };
    let centre = |a: i32, b: i32| a + (b - a + 1) / 2;
    PortalRoom {
        bbox: bb,
        orientation: p.orientation,
        eye_frame_centre: (x, bb[1] + ly, z),
        box_centre: (
            centre(bb[0], bb[3]),
            centre(bb[1], bb[4]),
            centre(bb[2], bb[5]),
        ),
    }
}
