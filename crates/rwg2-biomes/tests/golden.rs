//! Diffs the port against `testdata/golden/*` (regenerate with `oracle/build.sh`). Zero mismatches required.

use std::path::PathBuf;

use rwg2_biomes::cell::{value_noise_2d, value_noise_3d};
use rwg2_biomes::{CellNoise, JavaRandom, NoiseGenerator, PerlinNoise};

fn golden(name: &str) -> String {
    // Under WASI the repo root is preopened as "/" (tools/run-wasi.mjs sets RWG2_GOLDEN_DIR).
    let dir = std::env::var("RWG2_GOLDEN_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(|_| {
            PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../testdata/golden")
        });
    let p = dir.join(name);
    std::fs::read_to_string(&p)
        .unwrap_or_else(|e| panic!("{}: {e} (run oracle/build.sh)", p.display()))
}

fn f(s: &str) -> f32 {
    f32::from_bits(u32::from_str_radix(s, 16).unwrap())
}
fn d(s: &str) -> f64 {
    f64::from_bits(u64::from_str_radix(s, 16).unwrap())
}

#[derive(Default)]
struct Tally {
    checked: usize,
    failures: Vec<String>,
}

impl Tally {
    fn check(&mut self, ok: bool, msg: impl FnOnce() -> String) {
        self.checked += 1;
        if !ok && self.failures.len() < 20 {
            self.failures.push(msg());
        } else if !ok {
            self.failures.push(String::new());
        }
    }
    fn finish(self, what: &str) {
        let n = self.failures.len();
        assert!(
            n == 0,
            "{what}: {n}/{} mismatches:\n{}",
            self.checked,
            self.failures
                .iter()
                .filter(|s| !s.is_empty())
                .cloned()
                .collect::<Vec<_>>()
                .join("\n")
        );
        assert!(self.checked > 0, "{what}: no vectors checked");
        eprintln!("{what}: {} vectors, 0 mismatches", self.checked);
    }
}

#[test]
fn perlin_matches_java() {
    let data = golden("perlin.txt");
    let mut t = Tally::default();
    let mut noise: Option<(i64, PerlinNoise)> = None;
    for line in data.lines() {
        let c: Vec<&str> = line.split(' ').collect();
        match c[0] {
            "seed" => {
                let s: i64 = c[1].parse().unwrap();
                noise = Some((s, PerlinNoise::new(s)));
            }
            kind => {
                let (seed, n) = noise.as_ref().unwrap();
                let (got, want) = match kind {
                    "n1" => (n.noise1(f(c[1])).to_bits() as u64, f(c[2]).to_bits() as u64),
                    "n2" => (
                        n.noise2(f(c[1]), f(c[2])).to_bits() as u64,
                        f(c[3]).to_bits() as u64,
                    ),
                    "n3" => (
                        n.noise3(f(c[1]), f(c[2]), f(c[3])).to_bits() as u64,
                        f(c[4]).to_bits() as u64,
                    ),
                    "imp" => (
                        n.improved_noise(d(c[1]), d(c[2]), d(c[3])).to_bits(),
                        d(c[4]).to_bits(),
                    ),
                    _ => panic!("unknown record {line}"),
                };
                t.check(got == want, || {
                    format!("seed {seed}: {line} -> got {got:x}")
                });
            }
        }
    }
    t.finish("perlin");
}

#[test]
fn java_random_matches_java() {
    let data = golden("java_random.txt");
    let mut t = Tally::default();
    let mut r = JavaRandom::new(0);
    for line in data.lines() {
        let c: Vec<&str> = line.split(' ').collect();
        let ok = match c[0] {
            "seed" => {
                r = JavaRandom::new(c[1].parse().unwrap());
                continue;
            }
            "int" => r.next_int() == c[1].parse::<i32>().unwrap(),
            "long" => r.next_long() == c[1].parse::<i64>().unwrap(),
            "double" => r.next_double().to_bits() == d(c[1]).to_bits(),
            "float" => r.next_float().to_bits() == f(c[1]).to_bits(),
            "bool" => r.next_boolean() == c[1].parse::<bool>().unwrap(),
            "intb" => r.next_int_bounded(c[1].parse().unwrap()) == c[2].parse::<i32>().unwrap(),
            "gauss" => r.next_gaussian().to_bits() == d(c[1]).to_bits(),
            _ => panic!("unknown record {line}"),
        };
        t.check(ok, || line.to_string());
    }
    t.finish("java_random");
}

#[test]
fn cell_noise_matches_java() {
    let data = golden("cell.txt");
    let mut t = Tally::default();
    let mut c = CellNoise::new(0, 0);
    let mut ctx = String::new();
    for line in data.lines() {
        let a: Vec<&str> = line.split(' ').collect();
        let (got, want): (Vec<u64>, Vec<u64>) = match a[0] {
            "cell" => {
                c = CellNoise::new(a[1].parse().unwrap(), a[2].parse().unwrap());
                ctx = line.to_string();
                continue;
            }
            "usedist" => {
                c.set_use_distance(a[1] == "1");
                continue;
            }
            "v2" => (
                vec![value_noise_2d(
                    a[1].parse().unwrap(),
                    a[2].parse().unwrap(),
                    a[3].parse().unwrap(),
                )
                .to_bits()],
                vec![d(a[4]).to_bits()],
            ),
            "v3" => (
                vec![value_noise_3d(
                    a[1].parse().unwrap(),
                    a[2].parse().unwrap(),
                    a[3].parse().unwrap(),
                    a[4].parse().unwrap(),
                )
                .to_bits()],
                vec![d(a[5]).to_bits()],
            ),
            "noise" => (
                vec![c.noise(d(a[1]), d(a[2]), d(a[3])).to_bits() as u64],
                vec![f(a[4]).to_bits() as u64],
            ),
            "noise3" => (
                vec![c.noise3(d(a[1]), d(a[2]), d(a[3]), d(a[4])).to_bits()],
                vec![d(a[5]).to_bits()],
            ),
            "two" => (
                c.sample_two_2d(d(a[1]), d(a[2]), d(a[3]))
                    .iter()
                    .map(|v| v.to_bits())
                    .collect(),
                a[4..8].iter().map(|s| d(s).to_bits()).collect(),
            ),
            "border" | "border2" | "junction" => {
                let (x, z, w, dep) = (d(a[1]), d(a[2]), d(a[3]), f(a[4]));
                let g = match a[0] {
                    "border" => c.border(x, z, w, dep),
                    "border2" => c.border2(x, z, w, dep),
                    _ => c.junction(x, z, w, dep),
                };
                (vec![g.to_bits() as u64], vec![f(a[5]).to_bits() as u64])
            }
            _ => panic!("unknown record {line}"),
        };
        t.check(got == want, || {
            format!(
                "[{ctx} usedist={}] {line} -> got {got:x?}",
                c.use_distance()
            )
        });
    }
    t.finish("cell");
}

#[test]
fn poisson_matches_java() {
    use rwg2_biomes::poisson::PoissonPointNoise;
    let data = golden("poisson.txt");
    let mut t = Tally::default();
    let mut p: Option<PoissonPointNoise> = None;
    for line in data.lines() {
        let a: Vec<&str> = line.split(' ').collect();
        match a[0] {
            "poisson" => {
                p = Some(PoissonPointNoise::new(
                    a[1].parse().unwrap(),
                    d(a[2]),
                    a[3].parse().unwrap(),
                ))
            }
            "s" => {
                let s = p.as_ref().unwrap().sample(d(a[1]), d(a[2]));
                let got = format!(
                    "{:x} {} {} {:x} {:x} {}",
                    s.distance.to_bits(),
                    s.cell_x,
                    s.cell_z,
                    s.x.to_bits(),
                    s.z.to_bits(),
                    s.round
                );
                let want = a[3..9].join(" ");
                t.check(got == want, || format!("{line}\n   got {got}"));
            }
            "t" => {
                let o = p.as_ref().unwrap().sample_two(d(a[1]), d(a[2]));
                let ok = o
                    .iter()
                    .zip(&a[3..13])
                    .all(|(g, w)| g.to_bits() == d(w).to_bits());
                t.check(ok, || format!("{line}\n   got {o:?}"));
            }
            "a" => {
                let pp = p.as_ref().unwrap();
                let (cx, cz, rd): (i32, i32, i32) = (
                    a[1].parse().unwrap(),
                    a[2].parse().unwrap(),
                    a[3].parse().unwrap(),
                );
                let (x, z) = pp.candidate_point(cx, cz, rd);
                let ok = pp.is_accepted_at(cx, cz, rd).to_string() == a[4]
                    && x.to_bits() == d(a[5]).to_bits()
                    && z.to_bits() == d(a[6]).to_bits();
                t.check(ok, || line.to_string());
            }
            _ => panic!("unknown record {line}"),
        }
    }
    t.finish("poisson");
}

#[test]
fn continental_matches_java() {
    use rwg2_biomes::config::RwgConfig;
    use rwg2_biomes::continental::ContinentalNoise;
    let data = golden("continental.txt");
    let cfg = RwgConfig::default();
    let mut t = Tally::default();
    let mut c: Option<ContinentalNoise> = None;
    for line in data.lines() {
        let a: Vec<&str> = line.split(' ').collect();
        match a[0] {
            "cont" => c = Some(ContinentalNoise::new(a[1].parse().unwrap(), &cfg)),
            "found" => {}
            "c" => {
                let n = c.as_ref().unwrap();
                let (x, z): (i32, i32) = (a[1].parse().unwrap(), a[2].parse().unwrap());
                let got = format!(
                    "{:x} {} {} {} {} {} {} {} {} {} {} {} {}",
                    n.value(x, z).to_bits(),
                    n.is_island(x, z),
                    n.island_size_tier(x, z),
                    n.island_seed_coordinates(x, z),
                    n.island_center_coordinates(x, z),
                    n.volcano_coordinates(x, z),
                    n.volcano_vicinity_coordinates(x, z),
                    n.volcano_seed_key(x, z),
                    n.is_island_volcano(x, z),
                    n.volcano_center_coordinates(x, z),
                    n.lava_cave_coordinates(x, z),
                    n.lava_cave_seed_key(x, z),
                    n.lava_cave_center_coordinates(x, z),
                );
                let want = a[3..16].join(" ");
                t.check(got == want, || format!("{line}\n   got {got}"));
            }
            _ => panic!("unknown record {line}"),
        }
    }
    t.finish("continental");
}
