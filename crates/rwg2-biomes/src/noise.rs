//! `rwg2.util.NoiseGenerator`.
//!
//! RWG2's `NoiseGeneratorWrapper` switches impl via a *static* flag; here the choice is a per-instance value
//! ([`crate::NoiseImpl`]), so different worlds can coexist in one process/worker.

pub trait NoiseGenerator {
    fn improved_noise(&self, x: f64, y: f64, z: f64) -> f64;
    fn noise1(&self, x: f32) -> f32;
    fn noise2(&self, x: f32, y: f32) -> f32;
    fn noise3(&self, x: f32, y: f32, z: f32) -> f32;
}
