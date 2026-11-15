//! Zoomed-out Nether floor vs the exact chunk floor on every column (pillar rule applied to both, as the viewer does).
use rwg2_biomes::dimterrain::NetherTerrain;

fn view((y, lava): (i32, bool)) -> i32 {
    if lava { 31 } else if y <= 4 { 110 } else { y }
}

#[test]
fn nether_approx_floor_matches_exact() {
    let (mut n5, mut tot) = (0usize, 0usize);
    for seed in [1337i64, 3380521564351534918, 42] {
        let t = NetherTerrain::new(seed);
        for (cx, cz) in [(-60, -60), (30, -30), (-30, 60), (7, 7), (-1, 0)] {
            let c = t.chunk(cx, cz);
            for lz in 0..16 {
                for lx in 0..16 {
                    let e = view(c.floor(lx, lz));
                    let g = view(t.approx_floor(cx * 16 + lx, cz * 16 + lz));
                    tot += 1;
                    n5 += ((g - e).abs() <= 5) as usize;
                }
            }
        }
    }
    let share = n5 as f64 / tot as f64;
    eprintln!("within 5 blocks: {n5}/{tot} = {share:.3}");
    assert!(share > 0.88, "{share}");
}

/// The tile grid (one lattice field) agrees with per-sample `approx_floor` (batched noise may differ in the last
/// bits, so a rare threshold column can flip).
#[test]
fn nether_approx_grid_matches_samples() {
    let t = NetherTerrain::new(1337);
    for (x0, z0, stride) in [(-1022, -1022, 4), (512, -510, 8), (0, 0, 16), (-301, 77, 3)] {
        let (nx, nz) = (40, 30);
        let g = t.approx_floor_grid(x0, z0, nx, nz, stride);
        let mut same = 0;
        for j in 0..nz {
            for i in 0..nx {
                same += (g[(j * nx + i) as usize] == t.approx_floor(x0 + i * stride, z0 + j * stride)) as i32;
            }
        }
        eprintln!("stride {stride}: {same}/{}", nx * nz);
        assert!(same * 1000 >= nx * nz * 995, "stride {stride}: {same}");
    }
}
