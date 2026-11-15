//! Mineshafts: vanilla 1.7.10 `MapGenMineshaft` + `StructureMineshaftPieces`, as wrapped by Et Futurum Requiem's
//! `MapGenMesaMineshaft` (GTNH `enableMesaMineshaft=true`): same layout, but a start whose chunk-centre biome has
//! the MESA dictionary type is shifted to `63 - maxY + ySize/2 + 5` instead of `markAvailableHeight`.
//!
//! Structure Random (Hodgepodge `fastRandom` StdLCG, see `stronghold_pieces`):
//! `setSeed(seed); l, m = nextLong; setSeed(cx*l ^ cz*m ^ seed); nextInt(); nextDouble() < 0.004 &&
//! nextInt(80) < max(|cx|, |cz|)` → start = `Room(0, rand, cx*16+2, cz*16+2)` + recursive `buildComponent`.
//! Pure seed maths (no terrain). Verified piece-for-piece against rwg2dump (`tests/mineshaft.rs`).

use crate::java_random::JavaRandom;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Kind {
    Room,
    Corridor,
    Cross,
    Stairs,
}

impl Kind {
    pub fn java_name(self) -> &'static str {
        match self {
            Kind::Room => "Room",
            Kind::Corridor => "Corridor",
            Kind::Cross => "Cross",
            Kind::Stairs => "Stairs",
        }
    }
}

/// Bounding box `[minX, minY, minZ, maxX, maxY, maxZ]` (inclusive).
pub type BBox = [i32; 6];

#[derive(Clone, Debug)]
pub struct Piece {
    pub kind: Kind,
    pub bb: BBox,
    /// Corridor/Stairs `coordBaseMode`, Cross `corridorDirection`.
    pub dir: i32,
    pub depth: i32,
    /// Cross: two floors (box height > 3).
    pub multi: bool,
    /// Corridor: `hasRails` / `hasSpiders` (constructor draws)
    pub rails: bool,
    pub spiders: bool,
}

#[derive(Clone, Debug)]
pub struct Mineshaft {
    pub chunk: (i32, i32),
    pub bb: BBox,
    pub mesa: bool,
    pub pieces: Vec<Piece>,
}

const CHANCE: f64 = 0.004;

fn std_lcg(seed: i64) -> JavaRandom {
    JavaRandom::new(seed ^ 0x5DEECE66D)
}

/// Structure Random seeds for one world (`l`, `m` of `MapGenBase`).
#[derive(Clone, Copy)]
pub struct Seeds {
    seed: i64,
    l: i64,
    m: i64,
}

impl Seeds {
    pub fn new(world_seed: i64) -> Self {
        let mut r = std_lcg(world_seed);
        let l = r.next_long();
        let m = r.next_long();
        Seeds {
            seed: world_seed,
            l,
            m,
        }
    }

    fn rand(&self, cx: i32, cz: i32) -> JavaRandom {
        let mut r = std_lcg(
            (cx as i64).wrapping_mul(self.l) ^ (cz as i64).wrapping_mul(self.m) ^ self.seed,
        );
        r.next_int();
        r
    }
}

/// `canSpawnStructureAtCoords`; returns the Random positioned for the start when true.
pub fn start_rand(s: &Seeds, cx: i32, cz: i32) -> Option<JavaRandom> {
    let mut r = s.rand(cx, cz);
    if r.next_double() < CHANCE && r.next_int_bounded(80) < cx.abs().max(cz.abs()) {
        Some(r)
    } else {
        None
    }
}

pub fn is_start(s: &Seeds, cx: i32, cz: i32) -> bool {
    start_rand(s, cx, cz).is_some()
}

fn intersects(a: &BBox, b: &BBox) -> bool {
    a[3] >= b[0] && a[0] <= b[3] && a[5] >= b[2] && a[2] <= b[5] && a[4] >= b[1] && a[1] <= b[4]
}

struct Gen<'r> {
    r: &'r mut JavaRandom,
    list: Vec<Piece>,
    start_min: (i32, i32),
}

impl Gen<'_> {
    fn free(&self, bb: &BBox) -> bool {
        !self.list.iter().any(|p| intersects(&p.bb, bb))
    }

    fn random_component(&mut self, x: i32, y: i32, z: i32, dir: i32, depth: i32) -> Option<usize> {
        let v = self.r.next_int_bounded(100);
        let piece = if v >= 80 {
            let mut bb = [x, y, z, x, y + 2, z];
            if self.r.next_int_bounded(4) == 0 {
                bb[4] += 4;
            }
            match dir {
                0 => {
                    bb[0] = x - 1;
                    bb[3] = x + 3;
                    bb[5] = z + 4;
                }
                1 => {
                    bb[0] = x - 4;
                    bb[2] = z - 1;
                    bb[5] = z + 3;
                }
                2 => {
                    bb[0] = x - 1;
                    bb[3] = x + 3;
                    bb[2] = z - 4;
                }
                3 => {
                    bb[3] = x + 4;
                    bb[2] = z - 1;
                    bb[5] = z + 3;
                }
                _ => {}
            }
            if !self.free(&bb) {
                return None;
            }
            Piece {
                kind: Kind::Cross,
                bb,
                dir,
                depth,
                multi: bb[4] - bb[1] + 1 > 3,
                rails: false,
                spiders: false,
            }
        } else if v >= 70 {
            let mut bb = [x, y - 5, z, x, y + 2, z];
            match dir {
                0 => {
                    bb[3] = x + 2;
                    bb[5] = z + 8;
                }
                1 => {
                    bb[0] = x - 8;
                    bb[5] = z + 2;
                }
                2 => {
                    bb[3] = x + 2;
                    bb[2] = z - 8;
                }
                3 => {
                    bb[3] = x + 8;
                    bb[5] = z + 2;
                }
                _ => {}
            }
            if !self.free(&bb) {
                return None;
            }
            Piece {
                kind: Kind::Stairs,
                bb,
                dir,
                depth,
                multi: false,
                rails: false,
                spiders: false,
            }
        } else {
            let mut bb = [x, y, z, x, y + 2, z];
            let mut n = self.r.next_int_bounded(3) + 2;
            while n > 0 {
                let len = n * 5;
                match dir {
                    0 => {
                        bb[3] = x + 2;
                        bb[5] = z + (len - 1);
                    }
                    1 => {
                        bb[0] = x - (len - 1);
                        bb[5] = z + 2;
                    }
                    2 => {
                        bb[3] = x + 2;
                        bb[2] = z - (len - 1);
                    }
                    3 => {
                        bb[3] = x + (len - 1);
                        bb[5] = z + 2;
                    }
                    _ => {}
                }
                if self.free(&bb) {
                    break;
                }
                n -= 1;
            }
            if n <= 0 {
                return None;
            }
            // constructor: hasRails = nextInt(3) == 0; hasSpiders = !hasRails && nextInt(23) == 0
            let rails = self.r.next_int_bounded(3) == 0;
            let spiders = !rails && self.r.next_int_bounded(23) == 0;
            Piece {
                kind: Kind::Corridor,
                bb,
                dir,
                depth,
                multi: false,
                rails,
                spiders,
            }
        };
        self.list.push(piece);
        Some(self.list.len() - 1)
    }

    fn next(&mut self, x: i32, y: i32, z: i32, dir: i32, depth: i32) {
        if depth > 8 {
            return;
        }
        if (x - self.start_min.0).abs() <= 80 && (z - self.start_min.1).abs() <= 80 {
            if let Some(i) = self.random_component(x, y, z, dir, depth + 1) {
                self.build(i);
            }
        }
    }

    fn build(&mut self, i: usize) {
        let p = self.list[i].clone();
        let bb = p.bb;
        let d = p.depth;
        match p.kind {
            Kind::Room => self.build_room(i),
            Kind::Cross => {
                let (a, b, c) = match p.dir {
                    0 => (
                        (bb[0] + 1, bb[5] + 1, 0),
                        (bb[0] - 1, bb[2] + 1, 1),
                        (bb[3] + 1, bb[2] + 1, 3),
                    ),
                    1 => (
                        (bb[0] + 1, bb[2] - 1, 2),
                        (bb[0] + 1, bb[5] + 1, 0),
                        (bb[0] - 1, bb[2] + 1, 1),
                    ),
                    2 => (
                        (bb[0] + 1, bb[2] - 1, 2),
                        (bb[0] - 1, bb[2] + 1, 1),
                        (bb[3] + 1, bb[2] + 1, 3),
                    ),
                    _ => (
                        (bb[0] + 1, bb[2] - 1, 2),
                        (bb[0] + 1, bb[5] + 1, 0),
                        (bb[3] + 1, bb[2] + 1, 3),
                    ),
                };
                for (x, z, dir) in [a, b, c] {
                    self.next(x, bb[1], z, dir, d);
                }
                if p.multi {
                    let y = bb[1] + 4;
                    if self.r.next_boolean() {
                        self.next(bb[0] + 1, y, bb[2] - 1, 2, d);
                    }
                    if self.r.next_boolean() {
                        self.next(bb[0] - 1, y, bb[2] + 1, 1, d);
                    }
                    if self.r.next_boolean() {
                        self.next(bb[3] + 1, y, bb[2] + 1, 3, d);
                    }
                    if self.r.next_boolean() {
                        self.next(bb[0] + 1, y, bb[5] + 1, 0, d);
                    }
                }
            }
            Kind::Stairs => match p.dir {
                0 => self.next(bb[0], bb[1], bb[5] + 1, 0, d),
                1 => self.next(bb[0] - 1, bb[1], bb[2], 1, d),
                2 => self.next(bb[0], bb[1], bb[2] - 1, 2, d),
                _ => self.next(bb[3] + 1, bb[1], bb[2], 3, d),
            },
            Kind::Corridor => {
                let v = self.r.next_int_bounded(4);
                let y = bb[1] - 1 + self.r.next_int_bounded(3);
                let (x, z, dir) = match p.dir {
                    0 => match v {
                        0 | 1 => (bb[0], bb[5] + 1, 0),
                        2 => (bb[0] - 1, bb[5] - 3, 1),
                        _ => (bb[3] + 1, bb[5] - 3, 3),
                    },
                    1 => match v {
                        0 | 1 => (bb[0] - 1, bb[2], 1),
                        2 => (bb[0], bb[2] - 1, 2),
                        _ => (bb[0], bb[5] + 1, 0),
                    },
                    2 => match v {
                        0 | 1 => (bb[0], bb[2] - 1, 2),
                        2 => (bb[0] - 1, bb[2], 1),
                        _ => (bb[3] + 1, bb[2], 3),
                    },
                    _ => match v {
                        0 | 1 => (bb[3] + 1, bb[2], 3),
                        2 => (bb[3] - 3, bb[2] - 1, 2),
                        _ => (bb[3] - 3, bb[5] + 1, 0),
                    },
                };
                self.next(x, y, z, dir, d);
                if d < 8 {
                    if p.dir != 2 && p.dir != 0 {
                        let mut x = bb[0] + 3;
                        while x + 3 <= bb[3] {
                            match self.r.next_int_bounded(5) {
                                0 => self.next(x, bb[1], bb[2] - 1, 2, d + 1),
                                1 => self.next(x, bb[1], bb[5] + 1, 0, d + 1),
                                _ => {}
                            }
                            x += 5;
                        }
                    } else {
                        let mut z = bb[2] + 3;
                        while z + 3 <= bb[5] {
                            match self.r.next_int_bounded(5) {
                                0 => self.next(bb[0] - 1, bb[1], z, 1, d + 1),
                                1 => self.next(bb[3] + 1, bb[1], z, 3, d + 1),
                                _ => {}
                            }
                            z += 5;
                        }
                    }
                }
            }
        }
    }

    fn build_room(&mut self, i: usize) {
        let bb = self.list[i].bb;
        let d = self.list[i].depth;
        let (xs, ys, zs) = (bb[3] - bb[0] + 1, bb[4] - bb[1] + 1, bb[5] - bb[2] + 1);
        let hy = (ys - 3 - 1).max(1);
        let mut k = 0;
        while k < xs {
            k += self.r.next_int_bounded(xs);
            if k + 3 > xs {
                break;
            }
            let y = bb[1] + self.r.next_int_bounded(hy) + 1;
            self.next(bb[0] + k, y, bb[2] - 1, 2, d);
            k += 4;
        }
        let mut k = 0;
        while k < xs {
            k += self.r.next_int_bounded(xs);
            if k + 3 > xs {
                break;
            }
            let y = bb[1] + self.r.next_int_bounded(hy) + 1;
            self.next(bb[0] + k, y, bb[5] + 1, 0, d);
            k += 4;
        }
        let mut k = 0;
        while k < zs {
            k += self.r.next_int_bounded(zs);
            if k + 3 > zs {
                break;
            }
            let y = bb[1] + self.r.next_int_bounded(hy) + 1;
            self.next(bb[0] - 1, y, bb[2] + k, 1, d);
            k += 4;
        }
        let mut k = 0;
        while k < zs {
            k += self.r.next_int_bounded(zs);
            if k + 3 > zs {
                break;
            }
            let y = bb[1] + self.r.next_int_bounded(hy) + 1;
            self.next(bb[3] + 1, y, bb[2] + k, 3, d);
            k += 4;
        }
    }
}

/// Full layout of the mineshaft starting in chunk `(cx, cz)` (`None` if no start there). `mesa`: the chunk-centre
/// biome `(cx*16+8, cz*16+8)` has the MESA dictionary type (Et Futurum height rule).
pub fn mineshaft(s: &Seeds, cx: i32, cz: i32, mesa: bool) -> Option<Mineshaft> {
    let mut r = start_rand(s, cx, cz)?;
    let (x, z) = (cx * 16 + 2, cz * 16 + 2);
    let mx = x + 7 + r.next_int_bounded(6);
    let my = 54 + r.next_int_bounded(6);
    let mz = z + 7 + r.next_int_bounded(6);
    let room = Piece {
        kind: Kind::Room,
        bb: [x, 50, z, mx, my, mz],
        dir: 0,
        depth: 0,
        multi: false,
        rails: false,
        spiders: false,
    };
    let mut g = Gen {
        r: &mut r,
        list: vec![room],
        start_min: (x, z),
    };
    g.build(0);
    let mut bb = [i32::MAX, i32::MAX, i32::MAX, i32::MIN, i32::MIN, i32::MIN];
    for p in &g.list {
        for k in 0..3 {
            bb[k] = bb[k].min(p.bb[k]);
            bb[k + 3] = bb[k + 3].max(p.bb[k + 3]);
        }
    }
    let ysize = bb[4] - bb[1] + 1;
    let dy = if mesa {
        63 - bb[4] + ysize / 2 + 5
    } else {
        let mut h = ysize + 1;
        if h < 53 {
            h += g.r.next_int_bounded(53 - h);
        }
        h - bb[4]
    };
    bb[1] += dy;
    bb[4] += dy;
    let mut pieces = g.list;
    for p in &mut pieces {
        p.bb[1] += dy;
        p.bb[4] += dy;
    }
    Some(Mineshaft {
        chunk: (cx, cz),
        bb,
        mesa,
        pieces,
    })
}

/// Start chunks in `[cx0, cx1] × [cz0, cz1]`.
pub fn starts_in(s: &Seeds, cx0: i32, cz0: i32, cx1: i32, cz1: i32) -> Vec<(i32, i32)> {
    let mut v = Vec::new();
    for cz in cz0..=cz1 {
        for cx in cx0..=cx1 {
            if is_start(s, cx, cz) {
                v.push((cx, cz));
            }
        }
    }
    v
}

/// Blocks the pieces certainly turn to air (`addComponentParts`): corridor `minY..=minY+1` (the third layer is
/// air with probability 0.8), room `minY+1..=minY+3` (dirt floor; the dome above is random), cross floors,
/// stairs box (approximate). Ignores `isLiquidInStructureBoundingBox` skips.
pub fn air_boxes(m: &Mineshaft) -> impl Iterator<Item = BBox> + '_ {
    m.pieces.iter().map(piece_air)
}

/// [`air_boxes`] of one piece.
pub fn piece_air(p: &Piece) -> BBox {
    let b = p.bb;
    match p.kind {
        Kind::Room => [b[0], b[1] + 1, b[2], b[3], (b[1] + 3).min(b[4]), b[5]],
        Kind::Corridor => [b[0], b[1], b[2], b[3], b[1] + 1, b[5]],
        Kind::Cross if !p.multi => b,
        Kind::Cross => [b[0], b[1], b[2], b[3], b[1] + 2, b[5]],
        Kind::Stairs => b,
    }
}
