//! Port of `rwg2.util.CellNoise` (libnoiseforjava Voronoi, GPL-3+).
//!
//! Faithfulness notes (do not "fix"):
//! - `valueNoise*`: `1619*x + 6971*z (+ 31337*y)` is evaluated in **i32 (wrapping)**, then widened to i64 for
//!   `+ 1013*seed`. Everything after that is wrapping i64.
//! - Cell index: `x > 0 ? (int)x : (int)x - 1` (saturating cast, wrapping subtract). Note `x == 0.0` → -1.
//! - `distanceMethod == 1` is `xDist + zDist` *without* abs (not Manhattan).
//! - Loop bounds `cur <= base + 2` use wrapping i32 like Java (an i32::MAX bound would loop forever in Java too;
//!   unreachable for real coordinates).

#![allow(clippy::approx_constant, clippy::excessive_precision)] // literals kept verbatim from the Java source

use crate::java_random::JavaRandom;

const SQRT_2: f64 = 1.4142135623730950488; // == std::f64::consts::SQRT_2 bitwise
const SQRT_3: f64 = 1.7320508075688772935;

#[derive(Clone, Debug)]
pub struct CellNoise {
    use_distance: bool,
    seed: i64,
    seed_offset: i64,
    distance_method: i16,
    /// The 5×5 feature points of recently used windows (pure memo: neighbouring samples share a window).
    windows: std::cell::RefCell<[Window; WINDOW_SLOTS]>,
}

const WINDOW_SLOTS: usize = 8;

/// Feature points of the 5×5 cell window around `(xi, zi)`, in the Java loop order (z outer, x inner).
#[derive(Clone, Copy, Debug)]
struct Window {
    xi: i32,
    zi: i32,
    valid: bool,
    /// points in the window (25, fewer only where the Java loop bounds wrap around i32)
    n: usize,
    pts: [(f64, f64); 25],
}

const EMPTY_WINDOW: Window = Window {
    xi: 0,
    zi: 0,
    valid: false,
    n: 0,
    pts: [(0.0, 0.0); 25],
};

#[inline(always)]
fn cell_index(v: f64) -> i32 {
    if v > 0.0 {
        v as i32
    } else {
        (v as i32).wrapping_sub(1)
    }
}

/// Inclusive `lo..=hi` with Java `int` loop semantics (`for (c = b - 2; c <= b + 2; c++)`).
#[inline(always)]
fn window(base: i32) -> impl Iterator<Item = i32> {
    let lo = base.wrapping_sub(2) as i64;
    let hi = base.wrapping_add(2) as i64;
    (lo..=hi).map(|c| c as i32)
}

#[inline(always)]
fn hash_tail(n: i64) -> f64 {
    let n = n & 0x7fff_ffff;
    let n = (n >> 13) ^ n;
    let v = n
        .wrapping_mul(n.wrapping_mul(n).wrapping_mul(60493).wrapping_add(19990303))
        .wrapping_add(1376312589)
        & 0x7fff_ffff;
    1.0 - (v as f64 / 1073741824.0)
}

/// `CellNoise.valueNoise2D`.
#[inline(always)]
pub fn value_noise_2d(x: i32, z: i32, seed: i64) -> f64 {
    let ix = 1619i32
        .wrapping_mul(x)
        .wrapping_add(6971i32.wrapping_mul(z)) as i64;
    hash_tail(ix.wrapping_add(1013i64.wrapping_mul(seed)))
}

/// `CellNoise.valueNoise3D`.
#[inline(always)]
pub fn value_noise_3d(x: i32, y: i32, z: i32, seed: i64) -> f64 {
    let ix = 1619i32
        .wrapping_mul(x)
        .wrapping_add(31337i32.wrapping_mul(y))
        .wrapping_add(6971i32.wrapping_mul(z)) as i64;
    hash_tail(ix.wrapping_add(1013i64.wrapping_mul(seed)))
}

/// Output of [`CellNoise::sample_two_2d`]: nearest distance/value, then second-nearest distance/value.
pub type TwoSample = [f64; 4];

impl CellNoise {
    pub fn new(seed: i64, distance_method: i16) -> Self {
        Self {
            use_distance: false,
            seed,
            seed_offset: JavaRandom::new(seed).next_long(),
            distance_method,
            windows: std::cell::RefCell::new([EMPTY_WINDOW; WINDOW_SLOTS]),
        }
    }

    pub fn set_use_distance(&mut self, v: bool) {
        self.use_distance = v;
    }
    pub fn use_distance(&self) -> bool {
        self.use_distance
    }
    pub fn seed(&self) -> i64 {
        self.seed
    }
    pub fn set_seed(&mut self, seed: i64) {
        self.seed = seed;
        self.seed_offset = JavaRandom::new(seed).next_long();
        *self.windows.get_mut() = [EMPTY_WINDOW; WINDOW_SLOTS];
    }

    /// The 25 feature points of the window around cell `(xi, zi)` (memoised).
    #[inline]
    fn window_points(&self, xi: i32, zi: i32) -> std::cell::Ref<'_, [(f64, f64)]> {
        let slot = (xi.wrapping_mul(31).wrapping_add(zi) as u32 as usize) % WINDOW_SLOTS;
        {
            let mut w = self.windows.borrow_mut();
            let e = &mut w[slot];
            if !(e.valid && e.xi == xi && e.zi == zi) {
                let mut k = 0;
                for zcur in window(zi) {
                    for xcur in window(xi) {
                        e.pts[k] = self.point(xcur, zcur);
                        k += 1;
                    }
                }
                e.xi = xi;
                e.zi = zi;
                e.n = k;
                e.valid = true;
            }
        }
        std::cell::Ref::map(self.windows.borrow(), |w| &w[slot].pts[..w[slot].n])
    }
    pub fn distance_method(&self) -> i16 {
        self.distance_method
    }
    pub fn set_distance_method(&mut self, m: i16) {
        self.distance_method = m;
    }

    fn distance_2d(&self, xd: f64, zd: f64) -> f64 {
        match self.distance_method {
            0 => (xd * xd + zd * zd).sqrt() / SQRT_2,
            1 => xd + zd,
            _ => f64::NAN,
        }
    }

    fn distance_3d(&self, xd: f64, yd: f64, zd: f64) -> f64 {
        match self.distance_method {
            0 => (xd * xd + yd * yd + zd * zd).sqrt() / SQRT_3,
            1 => xd + yd + zd,
            _ => f64::NAN,
        }
    }

    #[inline(always)]
    fn point(&self, xc: i32, zc: i32) -> (f64, f64) {
        (
            xc as f64 + value_noise_2d(xc, zc, self.seed),
            zc as f64 + value_noise_2d(xc, zc, self.seed_offset),
        )
    }

    /// `float noise(double x, double z, double frequency)`.
    pub fn noise(&self, x: f64, z: f64, frequency: f64) -> f32 {
        let x = x * frequency;
        let z = z * frequency;
        let (xi, zi) = (cell_index(x), cell_index(z));
        let mut min_dist = 32000000.0f64;
        let (mut xc, mut zc) = (0.0f64, 0.0f64);
        let pts = self.window_points(xi, zi);
        for &(xp, zp) in pts.iter() {
            {
                let (xd, zd) = (xp - x, zp - z);
                let d = xd * xd + zd * zd;
                if d < min_dist {
                    min_dist = d;
                    xc = xp;
                    zc = zp;
                }
            }
        }
        if self.use_distance {
            self.distance_2d(xc - x, zc - z) as f32
        } else {
            value_noise_2d(xc.floor() as i32, zc.floor() as i32, self.seed) as f32
        }
    }

    /// `void sampleTwo2D(double x, double z, double frequency, double[] output)`.
    pub fn sample_two_2d(&self, x: f64, z: f64, frequency: f64) -> TwoSample {
        let x = x * frequency;
        let z = z * frequency;
        let (xi, zi) = (cell_index(x), cell_index(z));
        let (mut d1, mut d2) = (f64::INFINITY, f64::INFINITY);
        let (mut v1, mut v2) = (0.0f64, 0.0f64);
        let pts = self.window_points(xi, zi);
        for &(xp, zp) in pts.iter() {
            {
                let (xd, zd) = (xp - x, zp - z);
                let d = xd * xd + zd * zd;
                let v = value_noise_2d(xp.floor() as i32, zp.floor() as i32, self.seed);
                if d < d1 {
                    d2 = d1;
                    v2 = v1;
                    d1 = d;
                    v1 = v;
                } else if d < d2 {
                    d2 = d;
                    v2 = v;
                }
            }
        }
        [d1.sqrt(), v1, d2.sqrt(), v2]
    }

    /// `float border2(double x, double z, double width, float depth)`.
    pub fn border2(&self, x: f64, z: f64, width: f64, depth: f32) -> f32 {
        let (xi, zi) = (cell_index(x), cell_index(z));
        let (mut dc, mut xc, mut zc) = (32000000.0f64, 0.0f64, 0.0f64);
        let (mut dn, mut xn, mut zn) = (32000000.0f64, 0.0f64, 0.0f64);
        let pts = self.window_points(xi, zi);
        for &(xp, zp) in pts.iter() {
            {
                let (xd, zd) = (xp - x, zp - z);
                let d = xd * xd + zd * zd;
                if d < dc {
                    dn = dc;
                    xn = xc;
                    zn = zc;
                    dc = d;
                    xc = xp;
                    zc = zp;
                } else if d > dc && d < dn {
                    dn = d;
                    xn = xp;
                    zn = zp;
                }
            }
        }
        let mut dc = dc.sqrt();
        let dn = dn.sqrt();
        let diff = ((xc - xn) * (xc - xn) + (zc - zn) * (zc - zn)).sqrt();
        let total = (dc + dn) / diff;
        dc /= total; // Java also rescales dNeighbour here, but never reads it again
        let c = (diff / 2.0) - dc;
        if c < width {
            (((c / width) as f32) - 1.0) * depth
        } else {
            0.0
        }
    }

    /// `float border(double x, double z, double width, float depth)`.
    pub fn border(&self, x: f64, z: f64, width: f64, depth: f32) -> f32 {
        let (xi, zi) = (cell_index(x), cell_index(z));
        let sq = self.distance_method == 0;
        let (mut dc, mut dn) = (32000000.0f64, 32000000.0f64);
        let pts = self.window_points(xi, zi);
        for &(xp, zp) in pts.iter() {
            {
                let (xd, zd) = (xp - x, zp - z);
                let d = if sq {
                    xd * xd + zd * zd
                } else {
                    self.distance_2d(xd, zd)
                };
                if d < dc {
                    dn = dc;
                    dc = d;
                } else if d < dn {
                    dn = d;
                }
            }
        }
        if sq {
            dc = dc.sqrt() / SQRT_2;
            dn = dn.sqrt() / SQRT_2;
        }
        let c = dn - dc;
        if c < width {
            (((c / width) as f32) - 1.0) * depth
        } else {
            0.0
        }
    }

    /// `float junction(double x, double z, double width, float depth)`.
    pub fn junction(&self, x: f64, z: f64, width: f64, depth: f32) -> f32 {
        let (xi, zi) = (cell_index(x), cell_index(z));
        let (mut first, mut second, mut third) = (f64::INFINITY, f64::INFINITY, f64::INFINITY);
        let pts = self.window_points(xi, zi);
        for &(xp, zp) in pts.iter() {
            {
                let (xd, zd) = (xp - x, zp - z);
                let d = (xd * xd + zd * zd).sqrt() / SQRT_2;
                if d < first {
                    third = second;
                    second = first;
                    first = d;
                } else if d < second {
                    third = second;
                    second = d;
                } else if d < third {
                    third = d;
                }
            }
        }
        let diff = third - first;
        if diff < width {
            (((diff / width) as f32) - 1.0) * depth
        } else {
            0.0
        }
    }

    /// `double noise(double x, double y, double z, double frequency)` (3D; consumes a fresh `Random(seed)`).
    pub fn noise3(&self, x: f64, y: f64, z: f64, frequency: f64) -> f64 {
        let x = x * frequency;
        let y = y * frequency;
        let z = z * frequency;
        let (xi, yi, zi) = (cell_index(x), cell_index(y), cell_index(z));
        let mut min_dist = 32000000.0f64;
        let (mut xc, mut yc, mut zc) = (0.0f64, 0.0f64, 0.0f64);
        let mut rand = JavaRandom::new(self.seed);
        for zcur in window(zi) {
            for ycur in window(yi) {
                for xcur in window(xi) {
                    let xp = xcur as f64 + value_noise_3d(xcur, ycur, zcur, self.seed);
                    let yp = ycur as f64 + value_noise_3d(xcur, ycur, zcur, rand.next_long());
                    let zp = zcur as f64 + value_noise_3d(xcur, ycur, zcur, rand.next_long());
                    let (xd, yd, zd) = (xp - x, yp - y, zp - z);
                    let d = xd * xd + yd * yd + zd * zd;
                    if d < min_dist {
                        min_dist = d;
                        xc = xp;
                        yc = yp;
                        zc = zp;
                    }
                }
            }
        }
        if self.use_distance {
            self.distance_3d(xc - x, yc - y, zc - z)
        } else {
            value_noise_3d(
                xc.floor() as i32,
                yc.floor() as i32,
                zc.floor() as i32,
                self.seed,
            )
        }
    }
}
