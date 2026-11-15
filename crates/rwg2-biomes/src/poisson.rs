//! Port of `rwg2.util.PoissonPointNoise`: deterministic fixed-round Poisson dart throwing on a hashed grid.
//!
//! Acceptance is a pure function of (cell, round, seed); Java memoizes it in size-capped caches that may be cleared
//! mid-computation, which never changes results. Here: unbounded-per-instance memo + a simple region cache.
//! The candidate order inside a region (ring traversal, then round) is kept identical, since `sample` breaks
//! distance ties by first-seen.

use std::cell::RefCell;
use std::rc::Rc;

use crate::jmath::{cell_hash, cell_key, floor_i, unit, FxMap};

const SQRT_TWO: f64 = std::f64::consts::SQRT_2; // Java literal 1.4142135623730951D, bit-identical
const REGION_CACHE_LIMIT: usize = 4096;
const CELL_CACHE_LIMIT: usize = 1 << 20;

#[derive(Clone, Copy, Debug)]
pub struct Candidate {
    pub cell_x: i32,
    pub cell_z: i32,
    pub round: i32,
    pub x: f64,
    pub z: f64,
    pub priority: i64,
}

/// `sample` output: distance, owner cell X/Z, point X/Z, round.
#[derive(Clone, Copy, Debug)]
pub struct PointSample {
    pub distance: f64,
    pub cell_x: i32,
    pub cell_z: i32,
    pub x: f64,
    pub z: f64,
    pub round: i32,
}

pub struct PoissonPointNoise {
    seed: i64,
    minimum_distance: f64,
    minimum_distance_squared: f64,
    cell_size: f64,
    neighbour_cells: i32,
    rounds: i32,
    /// acceptance memo: key = (cell_key, round) → accepted
    accepted: RefCell<FxMap<(i64, i32), bool>>,
    regions: RefCell<FxMap<i64, Rc<[Candidate]>>>,
}

impl PoissonPointNoise {
    pub fn new(seed: i64, minimum_distance: f64, rounds: i32) -> Self {
        assert!(
            minimum_distance > 0.0 && rounds >= 1,
            "minimumDistance and rounds must be positive"
        );
        let cell_size = minimum_distance / SQRT_TWO;
        Self {
            seed,
            minimum_distance,
            minimum_distance_squared: minimum_distance * minimum_distance,
            cell_size,
            neighbour_cells: (minimum_distance / cell_size).ceil() as i32 + 1,
            rounds,
            accepted: RefCell::default(),
            regions: RefCell::default(),
        }
    }

    pub fn minimum_distance(&self) -> f64 {
        self.minimum_distance
    }
    pub fn cell_size(&self) -> f64 {
        self.cell_size
    }
    pub fn rounds(&self) -> i32 {
        self.rounds
    }

    pub fn sample(&self, x: f64, z: f64) -> PointSample {
        let region = self.region(floor_i(x / self.cell_size), floor_i(z / self.cell_size));
        let mut best: Option<&Candidate> = None;
        let mut best_sq = f64::INFINITY;
        for c in region.iter() {
            let (dx, dz) = (c.x - x, c.z - z);
            let d = dx * dx + dz * dz;
            if d < best_sq {
                best_sq = d;
                best = Some(c);
            }
        }
        // Java would NPE if empty; region() never returns empty.
        let b = best.expect("non-empty region");
        PointSample {
            distance: best_sq.sqrt(),
            cell_x: b.cell_x,
            cell_z: b.cell_z,
            x: b.x,
            z: b.z,
            round: b.round,
        }
    }

    /// Nearest and second-nearest (second may be absent: then all-zero with infinite distance, like Java).
    pub fn sample_two(&self, x: f64, z: f64) -> [f64; 10] {
        let region = self.region(floor_i(x / self.cell_size), floor_i(z / self.cell_size));
        let (mut first, mut second): (Option<&Candidate>, Option<&Candidate>) = (None, None);
        let (mut fsq, mut ssq) = (f64::INFINITY, f64::INFINITY);
        for c in region.iter() {
            let (dx, dz) = (c.x - x, c.z - z);
            let d = dx * dx + dz * dz;
            if d < fsq {
                second = first;
                ssq = fsq;
                first = Some(c);
                fsq = d;
            } else if d < ssq {
                second = Some(c);
                ssq = d;
            }
        }
        let w = |c: Option<&Candidate>, d: f64| -> [f64; 5] {
            match c {
                Some(c) => [d.sqrt(), c.cell_x as f64, c.cell_z as f64, c.x, c.z],
                None => [d.sqrt(), 0.0, 0.0, 0.0, 0.0],
            }
        };
        let (a, b) = (w(first, fsq), w(second, ssq));
        [a[0], a[1], a[2], a[3], a[4], b[0], b[1], b[2], b[3], b[4]]
    }

    pub fn is_accepted_at(&self, cell_x: i32, cell_z: i32, round: i32) -> bool {
        self.is_accepted(&self.candidate(cell_x, cell_z, round))
    }

    /// All accepted points inside the axis-aligned rectangle (inclusive), any order. Exact enumeration of the field.
    pub fn points_in_rect(&self, x0: f64, z0: f64, x1: f64, z1: f64) -> Vec<(f64, f64)> {
        let (cx0, cz0) = (floor_i(x0 / self.cell_size), floor_i(z0 / self.cell_size));
        let (cx1, cz1) = (floor_i(x1 / self.cell_size), floor_i(z1 / self.cell_size));
        let mut out = Vec::new();
        for cz in cz0..=cz1 {
            for cx in cx0..=cx1 {
                for round in 0..self.rounds {
                    let c = self.candidate(cx, cz, round);
                    if c.x >= x0 && c.x <= x1 && c.z >= z0 && c.z <= z1 && self.is_accepted(&c) {
                        out.push((c.x, c.z));
                    }
                }
            }
        }
        out
    }

    pub fn candidate_point(&self, cell_x: i32, cell_z: i32, round: i32) -> (f64, f64) {
        let c = self.candidate(cell_x, cell_z, round);
        (c.x, c.z)
    }

    fn region(&self, qx: i32, qz: i32) -> Rc<[Candidate]> {
        let key = cell_key(qx, qz);
        if let Some(r) = self.regions.borrow().get(&key) {
            return r.clone();
        }
        let mut accepted: Vec<Candidate> = Vec::new();
        let mut upper = f64::INFINITY;
        let mut ring: i32 = 0;
        loop {
            for oz in -ring..=ring {
                for ox in -ring..=ring {
                    if ring != 0 && ox.abs() != ring && oz.abs() != ring {
                        continue;
                    }
                    let (cx, cz) = (qx.wrapping_add(ox), qz.wrapping_add(oz));
                    for round in 0..self.rounds {
                        let c = self.candidate(cx, cz, round);
                        if self.is_accepted(&c) {
                            accepted.push(c);
                            upper = upper.min(self.farthest_cell_corner_squared(&c, qx, qz));
                        }
                    }
                }
            }
            let unsearched = ring as f64 * self.cell_size;
            if !accepted.is_empty() && unsearched * unsearched >= upper {
                let r: Rc<[Candidate]> = accepted.into();
                let mut regions = self.regions.borrow_mut();
                if regions.len() >= REGION_CACHE_LIMIT {
                    regions.clear();
                }
                regions.insert(key, r.clone());
                return r;
            }
            ring += 1;
        }
    }

    fn is_accepted(&self, c: &Candidate) -> bool {
        let key = (cell_key(c.cell_x, c.cell_z), c.round);
        if let Some(&a) = self.accepted.borrow().get(&key) {
            return a;
        }
        let result = self.compute_accepted(c);
        let mut memo = self.accepted.borrow_mut();
        if memo.len() >= CELL_CACHE_LIMIT {
            memo.clear();
        }
        memo.insert(key, result);
        result
    }

    fn compute_accepted(&self, c: &Candidate) -> bool {
        if !self.is_clear_of_earlier_rounds(c) {
            return false;
        }
        let n = self.neighbour_cells;
        for oz in -n..=n {
            for ox in -n..=n {
                if ox == 0 && oz == 0 {
                    continue;
                }
                let nb = self.candidate(
                    c.cell_x.wrapping_add(ox),
                    c.cell_z.wrapping_add(oz),
                    c.round,
                );
                if self.within_distance(c, &nb)
                    && self.lower_priority(&nb, c)
                    && self.is_clear_of_earlier_rounds(&nb)
                {
                    return false;
                }
            }
        }
        true
    }

    fn is_clear_of_earlier_rounds(&self, c: &Candidate) -> bool {
        let n = self.neighbour_cells;
        for round in 0..c.round {
            for oz in -n..=n {
                for ox in -n..=n {
                    let e =
                        self.candidate(c.cell_x.wrapping_add(ox), c.cell_z.wrapping_add(oz), round);
                    if self.within_distance(c, &e) && self.is_accepted(&e) {
                        return false;
                    }
                }
            }
        }
        true
    }

    #[inline]
    fn within_distance(&self, a: &Candidate, b: &Candidate) -> bool {
        let (dx, dz) = (a.x - b.x, a.z - b.z);
        dx * dx + dz * dz < self.minimum_distance_squared
    }

    #[inline]
    fn lower_priority(&self, a: &Candidate, b: &Candidate) -> bool {
        let (pa, pb) = (a.priority as u64, b.priority as u64);
        pa < pb || pa == pb && cell_key(a.cell_x, a.cell_z) < cell_key(b.cell_x, b.cell_z)
    }

    #[inline]
    fn candidate(&self, cell_x: i32, cell_z: i32, round: i32) -> Candidate {
        let salt = round.wrapping_mul(3);
        let x = (cell_x as f64 + unit(cell_hash(self.seed, cell_x, cell_z, salt))) * self.cell_size;
        let z = (cell_z as f64 + unit(cell_hash(self.seed, cell_x, cell_z, salt.wrapping_add(1))))
            * self.cell_size;
        Candidate {
            cell_x,
            cell_z,
            round,
            x,
            z,
            priority: cell_hash(self.seed, cell_x, cell_z, salt.wrapping_add(2)),
        }
    }

    fn farthest_cell_corner_squared(&self, c: &Candidate, cell_x: i32, cell_z: i32) -> f64 {
        let min_x = cell_x as f64 * self.cell_size;
        let max_x = min_x + self.cell_size;
        let min_z = cell_z as f64 * self.cell_size;
        let max_z = min_z + self.cell_size;
        let dx = (c.x - min_x).abs().max((c.x - max_x).abs());
        let dz = (c.z - min_z).abs().max((c.z - max_z).abs());
        dx * dx + dz * dz
    }
}
