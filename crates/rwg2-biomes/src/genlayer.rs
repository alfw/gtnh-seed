//! Vanilla MC 1.7.10 `net.minecraft.world.gen.layer` primitives (`GenLayer` LCG + Forge's `nextLong`, `GenLayerZoom`,
//! `GenLayerSmooth`, `GenLayerVoronoiZoom`), as used by mod biome stacks (Twilight Forest's `GenLayerTF`, BOP's hell layers).
//!
//! Java keeps the per-cell RNG (`chunkSeed`) in the layer object; here it lives in a [`LayerRng`] local to each
//! `get_ints` call, which is equivalent because every Java layer re-seeds it (`initChunkSeed`) before drawing.
//! `IntCache` is pure allocation reuse and is replaced by `Vec`s.
//!
//! `initWorldGenSeed(seed)` on the top of a stack initialises every layer with the same world seed, so each layer here
//! takes the world seed at construction and stores its own `worldGenSeed`.

const MUL: i64 = 6364136223846793005;
const ADD: i64 = 1442695040888963407;

#[inline]
fn lcg(s: i64) -> i64 {
    s.wrapping_mul(s.wrapping_mul(MUL).wrapping_add(ADD))
}

/// `GenLayer(long)` constructor: the layer's `baseSeed`.
pub fn base_seed(seed: i64) -> i64 {
    let mut b = seed;
    for _ in 0..3 {
        b = lcg(b).wrapping_add(seed);
    }
    b
}

/// `initWorldGenSeed(world)`: the layer's `worldGenSeed` from its constructor seed.
pub fn world_gen_seed(layer_seed: i64, world_seed: i64) -> i64 {
    let base = base_seed(layer_seed);
    let mut w = world_seed;
    for _ in 0..3 {
        w = lcg(w).wrapping_add(base);
    }
    w
}

/// The per-cell LCG (`initChunkSeed` / `nextInt`).
#[derive(Clone, Copy, Debug)]
pub struct LayerRng {
    wgs: i64,
    cs: i64,
}

impl LayerRng {
    pub fn new(world_gen_seed: i64) -> Self {
        LayerRng {
            wgs: world_gen_seed,
            cs: 0,
        }
    }

    /// `initChunkSeed(x, z)` (Java passes `long`s; callers sign-extend their ints).
    #[inline]
    pub fn init_chunk(&mut self, x: i64, z: i64) {
        let mut c = self.wgs;
        c = lcg(c).wrapping_add(x);
        c = lcg(c).wrapping_add(z);
        c = lcg(c).wrapping_add(x);
        c = lcg(c).wrapping_add(z);
        self.cs = c;
    }

    /// `nextInt(n)`: `(int) ((chunkSeed >> 24) % n)`, made non-negative, then advance.
    #[inline]
    pub fn next_int(&mut self, n: i32) -> i32 {
        let mut r = ((self.cs >> 24) % (n as i64)) as i32;
        if r < 0 {
            r += n;
        }
        self.cs = lcg(self.cs).wrapping_add(self.wgs);
        r
    }

    /// Forge's `GenLayer.nextLong(n)` (1.7.10 patch): `nextInt` in `long` arithmetic.
    #[inline]
    pub fn next_long(&mut self, n: i64) -> i64 {
        let mut r = (self.cs >> 24) % n;
        if r < 0 {
            r += n;
        }
        self.cs = lcg(self.cs).wrapping_add(self.wgs);
        r
    }

    /// `func_151619_a(a, b)` (pick one of two).
    #[inline]
    pub fn choose2(&mut self, a: i32, b: i32) -> i32 {
        if self.next_int(2) == 0 {
            a
        } else {
            b
        }
    }

    /// `func_151619_a(a, b, c, d)`.
    #[inline]
    pub fn choose4(&mut self, a: i32, b: i32, c: i32, d: i32) -> i32 {
        match self.next_int(4) {
            0 => a,
            1 => b,
            2 => c,
            _ => d,
        }
    }

    /// `func_151617_b` (mode or random of four, vanilla 1.7.10 order of tests).
    #[inline]
    #[allow(clippy::if_same_then_else)] // mirrors the Java branch order exactly
    pub fn mode_or_random(&mut self, a: i32, b: i32, c: i32, d: i32) -> i32 {
        if b == c && c == d {
            b
        } else if a == b && a == c {
            a
        } else if a == b && a == d {
            a
        } else if a == c && a == d {
            a
        } else if a == b && c != d {
            a
        } else if a == c && b != d {
            a
        } else if a == d && b != c {
            a
        } else if b == c && a != d {
            b
        } else if b == d && a != c {
            b
        } else if c == d && a != b {
            c
        } else {
            self.choose4(a, b, c, d)
        }
    }
}

/// `GenLayer.getInts(x, z, width, depth)`: row-major `width × depth` values starting at (x, z).
pub trait GenLayer {
    fn get_ints(&self, x: i32, z: i32, w: i32, h: i32) -> Vec<i32>;
}

pub type BoxLayer = Box<dyn GenLayer>;

/// `GenLayerZoom` (×2, mode-or-random corner).
pub struct Zoom {
    parent: BoxLayer,
    wgs: i64,
}

impl Zoom {
    pub fn new(layer_seed: i64, parent: BoxLayer, world_seed: i64) -> Self {
        Zoom {
            parent,
            wgs: world_gen_seed(layer_seed, world_seed),
        }
    }

    /// `GenLayerZoom.magnify(seed, layer, n)`.
    pub fn magnify(layer_seed: i64, mut parent: BoxLayer, n: i32, world_seed: i64) -> BoxLayer {
        for k in 0..n {
            parent = Box::new(Zoom::new(layer_seed + k as i64, parent, world_seed));
        }
        parent
    }
}

impl GenLayer for Zoom {
    fn get_ints(&self, x: i32, z: i32, w: i32, h: i32) -> Vec<i32> {
        let px = x >> 1;
        let pz = z >> 1;
        let pw = (w >> 1) + 2;
        let ph = (h >> 1) + 2;
        let src = self.parent.get_ints(px, pz, pw, ph);
        let tw = (pw - 1) << 1;
        let th = (ph - 1) << 1;
        let mut tmp = vec![0i32; (tw * th) as usize];
        let mut rng = LayerRng::new(self.wgs);
        for j in 0..ph - 1 {
            let mut idx = ((j << 1) * tw) as usize;
            let mut a = src[(j * pw) as usize];
            let mut b = src[((j + 1) * pw) as usize];
            for i in 0..pw - 1 {
                rng.init_chunk(((i + px) << 1) as i64, ((j + pz) << 1) as i64);
                let c = src[(i + 1 + j * pw) as usize];
                let d = src[(i + 1 + (j + 1) * pw) as usize];
                tmp[idx] = a;
                tmp[idx + tw as usize] = rng.choose2(a, b);
                idx += 1;
                tmp[idx] = rng.choose2(a, c);
                tmp[idx + tw as usize] = rng.mode_or_random(a, c, b, d);
                idx += 1;
                a = c;
                b = d;
            }
        }
        let mut out = vec![0i32; (w * h) as usize];
        for r in 0..h {
            let s = ((r + (z & 1)) * tw + (x & 1)) as usize;
            out[(r * w) as usize..((r + 1) * w) as usize].copy_from_slice(&tmp[s..s + w as usize]);
        }
        out
    }
}

/// `GenLayerSmooth`.
pub struct Smooth {
    parent: BoxLayer,
    wgs: i64,
}

impl Smooth {
    pub fn new(layer_seed: i64, parent: BoxLayer, world_seed: i64) -> Self {
        Smooth {
            parent,
            wgs: world_gen_seed(layer_seed, world_seed),
        }
    }
}

impl GenLayer for Smooth {
    fn get_ints(&self, x: i32, z: i32, w: i32, h: i32) -> Vec<i32> {
        let pw = w + 2;
        let src = self.parent.get_ints(x - 1, z - 1, pw, h + 2);
        let at = |i: i32, j: i32| src[(i + j * pw) as usize];
        let mut out = vec![0i32; (w * h) as usize];
        let mut rng = LayerRng::new(self.wgs);
        for j in 0..h {
            for i in 0..w {
                let west = at(i, j + 1);
                let east = at(i + 2, j + 1);
                let north = at(i + 1, j);
                let south = at(i + 1, j + 2);
                let mut v = at(i + 1, j + 1);
                if west == east && north == south {
                    rng.init_chunk((i + x) as i64, (j + z) as i64);
                    v = if rng.next_int(2) == 0 { west } else { north };
                } else {
                    if west == east {
                        v = west;
                    }
                    if north == south {
                        v = north;
                    }
                }
                out[(i + j * w) as usize] = v;
            }
        }
        out
    }
}

/// `GenLayerVoronoiZoom` (×4, jittered nearest cell). `mask_255` keeps vanilla's `& 255` on the right-hand
/// neighbours (biome ids > 255 need EndlessIDs' patched copy; Twilight Forest ids are all < 256).
pub struct Voronoi {
    parent: BoxLayer,
    wgs: i64,
    pub mask_255: bool,
}

impl Voronoi {
    pub fn new(layer_seed: i64, parent: BoxLayer, world_seed: i64) -> Self {
        Voronoi {
            parent,
            wgs: world_gen_seed(layer_seed, world_seed),
            mask_255: true,
        }
    }
}

impl GenLayer for Voronoi {
    fn get_ints(&self, x: i32, z: i32, w: i32, h: i32) -> Vec<i32> {
        let x = x - 2;
        let z = z - 2;
        let px = x >> 2;
        let pz = z >> 2;
        let pw = (w >> 2) + 2;
        let ph = (h >> 2) + 2;
        let src = self.parent.get_ints(px, pz, pw, ph);
        let tw = (pw - 1) << 2;
        let th = (ph - 1) << 2;
        let mut tmp = vec![0i32; (tw * th) as usize];
        let mut rng = LayerRng::new(self.wgs);
        let m = if self.mask_255 { 255 } else { -1 };
        let jit = |rng: &mut LayerRng| (rng.next_int(1024) as f64 / 1024.0 - 0.5) * 3.6;
        for j in 0..ph - 1 {
            let mut a = src[(j * pw) as usize];
            let mut b = src[((j + 1) * pw) as usize];
            for i in 0..pw - 1 {
                rng.init_chunk(((i + px) << 2) as i64, ((j + pz) << 2) as i64);
                let ax = jit(&mut rng);
                let az = jit(&mut rng);
                rng.init_chunk(((i + px + 1) << 2) as i64, ((j + pz) << 2) as i64);
                let cx = jit(&mut rng) + 4.0;
                let cz = jit(&mut rng);
                rng.init_chunk(((i + px) << 2) as i64, ((j + pz + 1) << 2) as i64);
                let bx = jit(&mut rng);
                let bz = jit(&mut rng) + 4.0;
                rng.init_chunk(((i + px + 1) << 2) as i64, ((j + pz + 1) << 2) as i64);
                let dx = jit(&mut rng) + 4.0;
                let dz = jit(&mut rng) + 4.0;
                let c = src[(i + 1 + j * pw) as usize] & m;
                let d = src[(i + 1 + (j + 1) * pw) as usize] & m;
                for r in 0..4 {
                    let mut idx = (((j << 2) + r) * tw + (i << 2)) as usize;
                    let rf = r as f64;
                    #[allow(clippy::explicit_counter_loop)]
                    for q in 0..4 {
                        let qf = q as f64;
                        let da = (rf - az) * (rf - az) + (qf - ax) * (qf - ax);
                        let dc = (rf - cz) * (rf - cz) + (qf - cx) * (qf - cx);
                        let db = (rf - bz) * (rf - bz) + (qf - bx) * (qf - bx);
                        let dd = (rf - dz) * (rf - dz) + (qf - dx) * (qf - dx);
                        tmp[idx] = if da < dc && da < db && da < dd {
                            a
                        } else if dc < da && dc < db && dc < dd {
                            c
                        } else if db < da && db < dc && db < dd {
                            b
                        } else {
                            d
                        };
                        idx += 1;
                    }
                }
                a = c;
                b = d;
            }
        }
        let mut out = vec![0i32; (w * h) as usize];
        for r in 0..h {
            let s = ((r + (z & 3)) * tw + (x & 3)) as usize;
            out[(r * w) as usize..((r + 1) * w) as usize].copy_from_slice(&tmp[s..s + w as usize]);
        }
        out
    }
}
