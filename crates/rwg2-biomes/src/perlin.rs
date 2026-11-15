//! Port of `rwg2.util.PerlinNoise` (J3D.org, LGPL-2.1).
//!
//! Faithfulness notes (do not "fix"):
//! - The improved-noise permutation shuffle uses `nextLong() & 256` (bitwise AND, not modulo), so swaps only
//!   ever target index 0 or 256, and `p_imp[256..512]` is mostly zero.
//! - All `float` arithmetic stays in f32; Java's `(int)` casts saturate exactly like Rust's `as i32`.

use crate::java_random::JavaRandom;
use crate::noise::NoiseGenerator;

const B: usize = 0x1000;
const BM: i32 = 0xff;
const N: f32 = 0x1000 as f32;
const DEFAULT_SAMPLE_SIZE: usize = 256;

#[derive(Clone)]
pub struct PerlinNoise {
    p_imp: Box<[i32; DEFAULT_SAMPLE_SIZE << 1]>,
    p: Box<[i32]>,
    g1: Box<[f32]>,
    g2: Box<[f32]>,
    g3: Box<[f32]>,
}

#[inline(always)]
fn lerp_f(t: f32, a: f32, b: f32) -> f32 {
    a + t * (b - a)
}

#[inline(always)]
fn lerp_d(t: f64, a: f64, b: f64) -> f64 {
    a + t * (b - a)
}

#[inline(always)]
fn s_curve(t: f32) -> f32 {
    t * t * (3.0 - 2.0 * t)
}

#[inline(always)]
fn fade(t: f64) -> f64 {
    t * t * t * (t * (t * 6.0 - 15.0) + 10.0)
}

#[inline(always)]
fn grad(hash: i32, x: f64, y: f64, z: f64) -> f64 {
    let h = hash & 15;
    let u = if h < 8 || h == 12 || h == 13 { x } else { y };
    let v = if h < 4 || h == 12 || h == 13 { y } else { z };
    (if h & 1 == 0 { u } else { -u }) + (if h & 2 == 0 { v } else { -v })
}

/// Java `v % m` for `v >= 0` and `m` a power of two, without libm `fmod` (a slow software loop).
/// Exact: `v / m` and `k * m` are exact power-of-two scalings, and the remainder lies on `v`'s ulp grid below `m`.
#[inline(always)]
fn rem_pow2(v: f64, m: f64) -> f64 {
    debug_assert!(v >= 0.0 && m.to_bits() & ((1u64 << 52) - 1) == 0);
    v - (v / m).floor() * m
}

/// `(float)(((rand.nextDouble() * Integer.MAX_VALUE) % (B + B)) - B) / B`
#[inline(always)]
fn rand_coef(rand: &mut JavaRandom) -> f32 {
    ((rem_pow2(rand.next_double() * i32::MAX as f64, (B + B) as f64)) - B as f64) as f32 / B as f32
}

fn normalize2(v: &mut [f32], off: usize) {
    let s = (1.0 / ((v[off] * v[off] + v[off + 1] * v[off + 1]) as f64).sqrt()) as f32;
    v[off] *= s;
    v[off + 1] *= s;
}

fn normalize3(v: &mut [f32], off: usize) {
    let s = (1.0
        / ((v[off] * v[off] + v[off + 1] * v[off + 1] + v[off + 2] * v[off + 2]) as f64).sqrt())
        as f32;
    v[off] *= s;
    v[off + 1] *= s;
    v[off + 2] *= s;
}

/// Splits `x + N` into (bucket0, bucket1, frac0, frac1) exactly like the Java prologue.
#[inline(always)]
fn axis(x: f32) -> (usize, usize, f32, f32) {
    let t = (x + N).abs();
    let ti = t as i32;
    let b0 = ti & BM;
    let b1 = (b0 + 1) & BM;
    let r0 = t - ti as f32;
    (b0 as usize, b1 as usize, r0, r0 - 1.0)
}

impl PerlinNoise {
    pub fn new(seed: i64) -> Self {
        let mut rand = JavaRandom::new(seed);
        let mut p_imp = Box::new([0i32; DEFAULT_SAMPLE_SIZE << 1]);
        let mut i = 0usize;
        while i < DEFAULT_SAMPLE_SIZE {
            p_imp[i] = i as i32;
            i += 1;
        }
        // while (--i > 0)
        loop {
            i -= 1;
            if i == 0 {
                break;
            }
            let k = p_imp[i];
            let j = (rand.next_long() & DEFAULT_SAMPLE_SIZE as i64) as usize;
            p_imp[i] = p_imp[j];
            p_imp[j] = k;
        }

        let len = B + B + 2;
        let mut p = vec![0i32; len];
        let mut g1 = vec![0f32; len];
        let mut g2 = vec![0f32; len * 2];
        let mut g3 = vec![0f32; len * 3];

        for i in 0..B {
            p[i] = i as i32;
            g1[i] = rand_coef(&mut rand);
            for j in 0..2 {
                g2[i * 2 + j] = rand_coef(&mut rand);
            }
            normalize2(&mut g2, i * 2);
            for j in 0..3 {
                g3[i * 3 + j] = rand_coef(&mut rand);
            }
            normalize3(&mut g3, i * 3);
        }

        let mut i = B;
        loop {
            i -= 1;
            if i == 0 {
                break;
            }
            let k = p[i];
            let j = rem_pow2(rand.next_double() * i32::MAX as f64, B as f64) as i32 as usize;
            p[i] = p[j];
            p[j] = k;
        }

        for i in 0..B + 2 {
            p[B + i] = p[i];
            g1[B + i] = g1[i];
            for j in 0..2 {
                g2[(B + i) * 2 + j] = g2[i * 2 + j];
            }
            for j in 0..3 {
                g3[(B + i) * 3 + j] = g3[i * 3 + j];
            }
        }

        Self {
            p_imp,
            p: p.into_boxed_slice(),
            g1: g1.into_boxed_slice(),
            g2: g2.into_boxed_slice(),
            g3: g3.into_boxed_slice(),
        }
    }
}

impl NoiseGenerator for PerlinNoise {
    fn improved_noise(&self, x: f64, y: f64, z: f64) -> f64 {
        let fx = x.floor();
        let fy = y.floor();
        let fz = z.floor();
        let ucx = (fx as i32 & 255) as usize;
        let ucy = fy as i32 & 255;
        let ucz = fz as i32 & 255;

        let xo = x - fx;
        let yo = y - fy;
        let zo = z - fz;

        let u = fade(xo);
        let v = fade(yo);
        let w = fade(zo);

        let pi = &self.p_imp;
        let a = (pi[ucx] + ucy) as usize;
        let aa = (pi[a] + ucz) as usize;
        let ab = (pi[a + 1] + ucz) as usize;
        let b = (pi[ucx + 1] + ucy) as usize;
        let ba = (pi[b] + ucz) as usize;
        let bb = (pi[b + 1] + ucz) as usize;

        let c1 = grad(pi[aa], xo, yo, zo);
        let c2 = grad(pi[ba], xo - 1.0, yo, zo);
        let c3 = grad(pi[ab], xo, yo - 1.0, zo);
        let c4 = grad(pi[bb], xo - 1.0, yo - 1.0, zo);
        let c5 = grad(pi[aa + 1], xo, yo, zo - 1.0);
        let c6 = grad(pi[ba + 1], xo - 1.0, yo, zo - 1.0);
        let c7 = grad(pi[ab + 1], xo, yo - 1.0, zo - 1.0);
        let c8 = grad(pi[bb + 1], xo - 1.0, yo - 1.0, zo - 1.0);

        lerp_d(
            w,
            lerp_d(v, lerp_d(u, c1, c2), lerp_d(u, c3, c4)),
            lerp_d(v, lerp_d(u, c5, c6), lerp_d(u, c7, c8)),
        )
    }

    fn noise1(&self, x: f32) -> f32 {
        let (bx0, bx1, rx0, rx1) = axis(x);
        let sx = s_curve(rx0);
        let u = rx0 * self.g1[self.p[bx0] as usize];
        let v = rx1 * self.g1[self.p[bx1] as usize];
        lerp_f(sx, u, v)
    }

    fn noise2(&self, x: f32, y: f32) -> f32 {
        let (bx0, bx1, rx0, rx1) = axis(x);
        let (by0, by1, ry0, ry1) = axis(y);
        let p = &self.p;
        let g2 = &self.g2;

        let i = p[bx0] as usize;
        let j = p[bx1] as usize;
        let b00 = p[i + by0] as usize;
        let b10 = p[j + by0] as usize;
        let b01 = p[i + by1] as usize;
        let b11 = p[j + by1] as usize;

        let sx = s_curve(rx0);
        let sy = s_curve(ry0);

        let u = rx0 * g2[b00 * 2] + ry0 * g2[b00 * 2 + 1];
        let v = rx1 * g2[b10 * 2] + ry0 * g2[b10 * 2 + 1];
        let a = lerp_f(sx, u, v);

        let u = rx0 * g2[b01 * 2] + ry1 * g2[b01 * 2 + 1];
        let v = rx1 * g2[b11 * 2] + ry1 * g2[b11 * 2 + 1];
        let b = lerp_f(sx, u, v);

        lerp_f(sy, a, b)
    }

    fn noise3(&self, x: f32, y: f32, z: f32) -> f32 {
        let (bx0, bx1, rx0, rx1) = axis(x);
        let (by0, by1, ry0, ry1) = axis(y);
        let (bz0, bz1, rz0, rz1) = axis(z);
        let p = &self.p;
        let g3 = &self.g3;

        let i = p[bx0] as usize;
        let j = p[bx1] as usize;
        let b00 = p[i + by0] as usize;
        let b10 = p[j + by0] as usize;
        let b01 = p[i + by1] as usize;
        let b11 = p[j + by1] as usize;

        let t = s_curve(rx0);
        let sy = s_curve(ry0);
        let sz = s_curve(rz0);

        let dot = |gi: usize, a: f32, b: f32, c: f32| a * g3[gi] + b * g3[gi + 1] + c * g3[gi + 2];

        let u = dot((b00 + bz0) * 3, rx0, ry0, rz0);
        let v = dot((b10 + bz0) * 3, rx1, ry0, rz0);
        let a = lerp_f(t, u, v);
        let u = dot((b01 + bz0) * 3, rx0, ry1, rz0);
        let v = dot((b11 + bz0) * 3, rx1, ry1, rz0);
        let b = lerp_f(t, u, v);
        let c = lerp_f(sy, a, b);

        let u = dot((b00 + bz1) * 3, rx0, ry0, rz1);
        let v = dot((b10 + bz1) * 3, rx1, ry0, rz1);
        let a = lerp_f(t, u, v);
        let u = dot((b01 + bz1) * 3, rx0, ry1, rz1);
        let v = dot((b11 + bz1) * 3, rx1, ry1, rz1);
        let b = lerp_f(t, u, v);
        let d = lerp_f(sy, a, b);

        lerp_f(sz, c, d)
    }
}
