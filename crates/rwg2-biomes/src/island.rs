//! Port of `rwg2.util.IslandPointNoise`: island seeds chosen from unaccepted continent darts.

use std::cell::RefCell;
use std::rc::Rc;

use crate::jmath::{cell_hash, cell_key, floor_i, unit, FxMap};
use crate::poisson::PoissonPointNoise;

const CACHE_LIMIT: usize = 32768;
const MEMO_LIMIT: usize = 1 << 20;

#[derive(Clone, Copy, Debug)]
struct IslandCandidate {
    cell_x: i32,
    cell_z: i32,
    round: i32,
    x: f64,
    z: f64,
    width: f64,
    priority: i64,
}

/// `sample` output: winning island field value, radius, seed X/Z.
#[derive(Clone, Copy, Debug)]
pub struct IslandSample {
    pub value: f64,
    pub width: f64,
    pub x: f64,
    pub z: f64,
}

pub struct IslandPointNoise {
    seed: i64,
    maximum_continent_width: f64,
    minimum_island_width: f64,
    island_width_range: f64,
    minimum_ocean_width: f64,
    placement_chance: f64,
    island_search_cells: i32,
    neighbour_cells: i32,
    hole: RefCell<FxMap<(i64, i32), bool>>,
    identified: RefCell<FxMap<(i64, i32), bool>>,
    regions: RefCell<FxMap<i64, Rc<[IslandCandidate]>>>,
}

impl IslandPointNoise {
    pub fn new(
        seed: i64,
        continents: &PoissonPointNoise,
        maximum_continent_width: f64,
        first_island_width: f64,
        second_island_width: f64,
        minimum_ocean_width: f64,
        placement_chance: f64,
    ) -> Self {
        let min_w = first_island_width.min(second_island_width);
        let max_w = first_island_width.max(second_island_width);
        let cs = continents.cell_size();
        Self {
            seed,
            maximum_continent_width,
            minimum_island_width: min_w,
            island_width_range: max_w - min_w,
            minimum_ocean_width,
            placement_chance,
            island_search_cells: (max_w / cs).ceil() as i32 + 1,
            neighbour_cells: ((max_w * 2.0 + minimum_ocean_width) / cs).ceil() as i32 + 1,
            hole: RefCell::default(),
            identified: RefCell::default(),
            regions: RefCell::default(),
        }
    }

    pub fn sample(&self, continents: &PoissonPointNoise, x: f64, z: f64) -> IslandSample {
        let cs = continents.cell_size();
        let region = self.region(continents, floor_i(x / cs), floor_i(z / cs));
        let mut best = -f64::MAX;
        let (mut bw, mut bx, mut bz) = (0.0, 0.0, 0.0);
        for c in region.iter() {
            let (dx, dz) = (x - c.x, z - c.z);
            let d = dx * dx + dz * dz;
            if best != -f64::MAX {
                let r = c.width - best;
                if r <= 0.0 || d >= r * r {
                    continue;
                }
            }
            let v = c.width - d.sqrt();
            if v > best {
                best = v;
                bw = c.width;
                bx = c.x;
                bz = c.z;
            }
        }
        IslandSample {
            value: best,
            width: bw,
            x: bx,
            z: bz,
        }
    }

    fn region(&self, continents: &PoissonPointNoise, cx: i32, cz: i32) -> Rc<[IslandCandidate]> {
        let key = cell_key(cx, cz);
        if let Some(r) = self.regions.borrow().get(&key) {
            return r.clone();
        }
        let n = self.island_search_cells;
        let mut placed = Vec::new();
        for oz in -n..=n {
            for ox in -n..=n {
                for round in 0..continents.rounds() {
                    let c =
                        self.candidate(continents, cx.wrapping_add(ox), cz.wrapping_add(oz), round);
                    if self.is_placed(continents, &c) {
                        placed.push(c);
                    }
                }
            }
        }
        let r: Rc<[IslandCandidate]> = placed.into();
        let mut regions = self.regions.borrow_mut();
        if regions.len() >= CACHE_LIMIT {
            regions.clear();
        }
        regions.insert(key, r.clone());
        r
    }

    fn is_placed(&self, continents: &PoissonPointNoise, c: &IslandCandidate) -> bool {
        self.is_identified(continents, c)
            && unit(cell_hash(
                self.seed,
                c.cell_x,
                c.cell_z,
                c.round.wrapping_mul(4).wrapping_add(3),
            )) < self.placement_chance
    }

    fn is_identified(&self, continents: &PoissonPointNoise, c: &IslandCandidate) -> bool {
        let key = (cell_key(c.cell_x, c.cell_z), c.round);
        if let Some(&v) = self.identified.borrow().get(&key) {
            return v;
        }
        let v = self.compute_identified(continents, c);
        let mut m = self.identified.borrow_mut();
        if m.len() >= MEMO_LIMIT {
            m.clear();
        }
        m.insert(key, v);
        v
    }

    fn compute_identified(&self, continents: &PoissonPointNoise, c: &IslandCandidate) -> bool {
        if !self.is_hole_candidate(continents, c) {
            return false;
        }
        let n = self.neighbour_cells;
        for oz in -n..=n {
            for ox in -n..=n {
                for round in 0..continents.rounds() {
                    let (nx, nz) = (c.cell_x.wrapping_add(ox), c.cell_z.wrapping_add(oz));
                    // Java: `neighbour == candidate` (identity of the cached object for the same cell/round).
                    if nx == c.cell_x && nz == c.cell_z && round == c.round {
                        continue;
                    }
                    let nb = self.candidate(continents, nx, nz, round);
                    if !self.is_hole_candidate(continents, &nb) || !self.overlaps(c, &nb) {
                        continue;
                    }
                    if (nb.priority as u64) < (c.priority as u64) {
                        return false;
                    }
                }
            }
        }
        true
    }

    fn is_hole_candidate(&self, continents: &PoissonPointNoise, c: &IslandCandidate) -> bool {
        let key = (cell_key(c.cell_x, c.cell_z), c.round);
        if let Some(&v) = self.hole.borrow().get(&key) {
            return v;
        }
        let v = if continents.is_accepted_at(c.cell_x, c.cell_z, c.round) {
            false
        } else {
            let nearest = continents.sample(c.x, c.z);
            nearest.distance >= self.maximum_continent_width + c.width + self.minimum_ocean_width
        };
        let mut m = self.hole.borrow_mut();
        if m.len() >= MEMO_LIMIT {
            m.clear();
        }
        m.insert(key, v);
        v
    }

    fn overlaps(&self, a: &IslandCandidate, b: &IslandCandidate) -> bool {
        let (dx, dz) = (a.x - b.x, a.z - b.z);
        let m = a.width + b.width + self.minimum_ocean_width;
        dx * dx + dz * dz < m * m
    }

    fn candidate(
        &self,
        continents: &PoissonPointNoise,
        cell_x: i32,
        cell_z: i32,
        round: i32,
    ) -> IslandCandidate {
        let (x, z) = continents.candidate_point(cell_x, cell_z, round);
        let width = self.minimum_island_width
            + unit(cell_hash(self.seed, cell_x, cell_z, round.wrapping_mul(4)))
                * self.island_width_range;
        IslandCandidate {
            cell_x,
            cell_z,
            round,
            x,
            z,
            width,
            priority: cell_hash(
                self.seed,
                cell_x,
                cell_z,
                round.wrapping_mul(4).wrapping_add(1),
            ),
        }
    }
}
