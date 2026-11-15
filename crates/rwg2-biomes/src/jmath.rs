//! Small Java-semantics helpers shared by the RWG2 point fields.

/// The splitmix-style cell hash used by `PoissonPointNoise`, `IslandPointNoise`, `ContinentLandmarkNoise` and
/// `ContinentalNoise.random01` (all identical).
#[inline(always)]
pub fn cell_hash(seed: i64, cell_x: i32, cell_z: i32, salt: i32) -> i64 {
    let mut v = seed;
    v ^= (cell_x as i64).wrapping_mul(341873128712);
    v ^= (cell_z as i64).wrapping_mul(132897987541);
    v ^= (salt as i64).wrapping_mul(42317861);
    v ^= ((v as u64) >> 33) as i64;
    v = v.wrapping_mul(0xff51afd7ed558ccdu64 as i64);
    v ^= ((v as u64) >> 33) as i64;
    v = v.wrapping_mul(0xc4ceb9fe1a85ec53u64 as i64);
    v ^ ((v as u64) >> 33) as i64
}

/// `(value >>> 11) * 0x1.0p-53`
#[inline(always)]
pub fn unit(v: i64) -> f64 {
    ((v as u64) >> 11) as f64 * (1.0 / (1u64 << 53) as f64)
}

/// `(long) x & 0xffffffffL | (long) z << 32`
#[inline(always)]
pub fn cell_key(x: i32, z: i32) -> i64 {
    (x as u32 as i64) | ((z as i64) << 32)
}

/// `int i = (int) v; return v < i ? i - 1 : i;`
#[inline(always)]
pub fn floor_i(v: f64) -> i32 {
    let i = v as i32;
    if v < i as f64 {
        i.wrapping_sub(1)
    } else {
        i
    }
}

/// `java.lang.Math.round(double)`: round half up (towards +inf), NaN → 0, saturating.
#[inline(always)]
pub fn round_d(x: f64) -> i64 {
    let f = x.floor();
    // x - floor(x) is exact for all finite doubles.
    let r = if x - f >= 0.5 { f + 1.0 } else { f };
    r as i64
}

/// Pack two ints like `(long) a << 32 | b & 0xffffffffL`.
#[inline(always)]
pub fn pack(a: i32, b: i32) -> i64 {
    ((a as i64) << 32) | (b as u32 as i64)
}

/// Minimal FxHash-style hasher for i64-keyed caches (zero dependencies).
#[derive(Default, Clone, Copy)]
pub struct FxHasher(u64);

impl std::hash::Hasher for FxHasher {
    #[inline]
    fn finish(&self) -> u64 {
        self.0
    }
    #[inline]
    fn write(&mut self, bytes: &[u8]) {
        for &b in bytes {
            self.0 = (self.0.rotate_left(5) ^ b as u64).wrapping_mul(0x51_7c_c1_b7_27_22_0a_95);
        }
    }
    #[inline]
    fn write_i64(&mut self, i: i64) {
        self.0 = (self.0.rotate_left(5) ^ i as u64).wrapping_mul(0x51_7c_c1_b7_27_22_0a_95);
    }
    #[inline]
    fn write_u64(&mut self, i: u64) {
        self.write_i64(i as i64)
    }
}

pub type FxBuild = std::hash::BuildHasherDefault<FxHasher>;
pub type FxMap<K, V> = std::collections::HashMap<K, V, FxBuild>;

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn java_round() {
        assert_eq!(round_d(0.49999999999999994), 0);
        assert_eq!(round_d(-2.5), -2);
        assert_eq!(round_d(2.5), 3);
        assert_eq!(round_d(-0.5), 0);
        assert_eq!(round_d(f64::NAN), 0);
        assert_eq!(round_d(1e300), i64::MAX);
        assert_eq!(round_d(-1e300), i64::MIN);
    }
}
