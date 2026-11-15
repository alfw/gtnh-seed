//! Port of `rwg2.util.ContinentLandmarkNoise`: landmark centres (volcanoes, lava caves) from raw continent darts.

use std::cell::RefCell;
use std::rc::Rc;

use crate::jmath::{cell_hash, cell_key, floor_i, unit, FxMap};
use crate::poisson::PoissonPointNoise;

const CACHE_LIMIT: usize = 4096;

#[derive(Clone, Copy, Debug)]
struct Candidate {
    x: f64,
    z: f64,
    cell_x: i32,
    cell_z: i32,
    round: i32,
}

/// `sample` output: distance, point X/Z, cell X/Z, round. `distance` is +inf when no candidate is in range.
#[derive(Clone, Copy, Debug)]
pub struct LandmarkSample {
    pub distance: f64,
    pub x: f64,
    pub z: f64,
    pub cell_x: i32,
    pub cell_z: i32,
    pub round: i32,
}

pub struct ContinentLandmarkNoise {
    continent_seed: i64,
    selection_seed: i64,
    minimum_continent_width: f64,
    continent_width_range: f64,
    voronoi_radius: f64,
    selection_chance: f64,
    search_cells: i32,
    regions: RefCell<FxMap<i64, Rc<[Candidate]>>>,
}

impl ContinentLandmarkNoise {
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        continent_seed: i64,
        selection_seed: i64,
        continents: &PoissonPointNoise,
        minimum_continent_width: f64,
        maximum_continent_width: f64,
        voronoi_radius: f64,
        average_per_continent: f64,
        search_radius: f64,
    ) -> Self {
        let avg = (minimum_continent_width + maximum_continent_width) * 0.5;
        let land_fraction = 0.01f64.max(1f64.min(avg * avg / (voronoi_radius * voronoi_radius)));
        Self {
            continent_seed,
            selection_seed,
            minimum_continent_width,
            continent_width_range: maximum_continent_width - minimum_continent_width,
            voronoi_radius,
            selection_chance: 1f64
                .min(average_per_continent / (continents.rounds() as f64 * land_fraction)),
            search_cells: (search_radius / continents.cell_size()).ceil() as i32 + 1,
            regions: RefCell::default(),
        }
    }

    pub fn sample(&self, continents: &PoissonPointNoise, x: f64, z: f64) -> LandmarkSample {
        let cs = continents.cell_size();
        let region = self.region(continents, floor_i(x / cs), floor_i(z / cs));
        let mut best: Option<&Candidate> = None;
        let mut best_sq = f64::INFINITY;
        for c in region.iter() {
            let (dx, dz) = (x - c.x, z - c.z);
            let d = dx * dx + dz * dz;
            if d < best_sq {
                best = Some(c);
                best_sq = d;
            }
        }
        match best {
            Some(b) => LandmarkSample {
                distance: best_sq.sqrt(),
                x: b.x,
                z: b.z,
                cell_x: b.cell_x,
                cell_z: b.cell_z,
                round: b.round,
            },
            None => LandmarkSample {
                distance: best_sq.sqrt(),
                x: 0.0,
                z: 0.0,
                cell_x: 0,
                cell_z: 0,
                round: 0,
            },
        }
    }

    fn region(&self, continents: &PoissonPointNoise, cx: i32, cz: i32) -> Rc<[Candidate]> {
        let key = cell_key(cx, cz);
        if let Some(r) = self.regions.borrow().get(&key) {
            return r.clone();
        }
        let n = self.search_cells;
        let mut selected = Vec::new();
        for oz in -n..=n {
            for ox in -n..=n {
                let (ccx, ccz) = (cx.wrapping_add(ox), cz.wrapping_add(oz));
                for round in 0..continents.rounds() {
                    if unit(cell_hash(
                        self.selection_seed,
                        ccx,
                        ccz,
                        round.wrapping_mul(2).wrapping_add(1),
                    )) >= self.selection_chance
                    {
                        continue;
                    }
                    let (px, pz) = continents.candidate_point(ccx, ccz, round);
                    let nearest = continents.sample(px, pz);
                    let width = self.minimum_continent_width
                        + unit(cell_hash(
                            self.continent_seed,
                            nearest.cell_x,
                            nearest.cell_z,
                            0,
                        )) * self.continent_width_range;
                    if self.voronoi_radius.min(width) - nearest.distance < 0.0 {
                        continue;
                    }
                    selected.push(Candidate {
                        x: px,
                        z: pz,
                        cell_x: ccx,
                        cell_z: ccz,
                        round,
                    });
                }
            }
        }
        let r: Rc<[Candidate]> = selected.into();
        let mut regions = self.regions.borrow_mut();
        if regions.len() >= CACHE_LIMIT {
            regions.clear();
        }
        regions.insert(key, r.clone());
        r
    }
}

/// `ContinentLandmarkNoise.candidateKey`.
pub fn candidate_key(seed: i64, cell_x: i32, cell_z: i32, round: i32) -> i64 {
    let v = cell_hash(seed, cell_x, cell_z, round.wrapping_mul(2).wrapping_add(2));
    if v == i64::MIN {
        i64::MIN + 1
    } else {
        v
    }
}
