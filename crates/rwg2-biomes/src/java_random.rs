//! Bit-exact `java.util.Random` (48-bit LCG), as of JDK 8–21.

const MULTIPLIER: i64 = 0x5DEECE66D;
const ADDEND: i64 = 0xB;
const MASK: i64 = (1 << 48) - 1;

#[derive(Clone, Debug)]
pub struct JavaRandom {
    seed: i64,
    next_next_gaussian: Option<f64>,
}

impl JavaRandom {
    pub fn new(seed: i64) -> Self {
        Self {
            seed: (seed ^ MULTIPLIER) & MASK,
            next_next_gaussian: None,
        }
    }

    /// The scrambled 48-bit state (what `Random.seed` holds; `new Random(state)` scrambles it again).
    pub fn state(&self) -> i64 {
        self.seed
    }

    /// Steps the LCG back `n` draws of `next` (inverse multiplier mod 2^48).
    pub fn rewind(&mut self, n: u32) {
        const INV: i64 = 0xDFE05BCB1365;
        for _ in 0..n {
            self.seed = (self.seed.wrapping_sub(ADDEND)).wrapping_mul(INV) & MASK;
        }
        self.next_next_gaussian = None;
    }

    #[inline]
    pub fn next(&mut self, bits: u32) -> i32 {
        self.seed = self.seed.wrapping_mul(MULTIPLIER).wrapping_add(ADDEND) & MASK;
        (self.seed >> (48 - bits)) as i32
    }

    #[inline]
    pub fn next_int(&mut self) -> i32 {
        self.next(32)
    }

    /// `nextInt(bound)`; panics on `bound <= 0` like Java throws.
    pub fn next_int_bounded(&mut self, bound: i32) -> i32 {
        assert!(bound > 0, "bound must be positive");
        let mut r = self.next(31);
        let m = bound - 1;
        if bound & m == 0 {
            ((bound as i64 * r as i64) >> 31) as i32
        } else {
            let mut u = r;
            loop {
                r = u % bound;
                if u.wrapping_sub(r).wrapping_add(m) >= 0 {
                    return r;
                }
                u = self.next(31);
            }
        }
    }

    #[inline]
    pub fn next_long(&mut self) -> i64 {
        let hi = self.next(32) as i64;
        let lo = self.next(32) as i64; // sign-extended, as in Java's int + long promotion
        (hi << 32).wrapping_add(lo)
    }

    #[inline]
    pub fn next_boolean(&mut self) -> bool {
        self.next(1) != 0
    }

    #[inline]
    pub fn next_float(&mut self) -> f32 {
        self.next(24) as f32 * (1.0f32 / (1 << 24) as f32)
    }

    #[inline]
    pub fn next_double(&mut self) -> f64 {
        let hi = (self.next(26) as i64) << 27;
        let lo = self.next(27) as i64;
        (hi + lo) as f64 * (1.0f64 / (1i64 << 53) as f64)
    }

    /// Uses ported fdlibm `log` to match `StrictMath` (musl/libm `log` differs by 1 ulp in ~1% of cases).
    pub fn next_gaussian(&mut self) -> f64 {
        if let Some(g) = self.next_next_gaussian.take() {
            return g;
        }
        loop {
            let v1 = 2.0 * self.next_double() - 1.0;
            let v2 = 2.0 * self.next_double() - 1.0;
            let s = v1 * v1 + v2 * v2;
            if s < 1.0 && s != 0.0 {
                let multiplier = crate::fdlibm::sqrt(-2.0 * crate::fdlibm::log(s) / s);
                self.next_next_gaussian = Some(v2 * multiplier);
                return v1 * multiplier;
            }
        }
    }
}
