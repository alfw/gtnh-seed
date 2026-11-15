//! 3D view water: columns below sea level only carry water where the real column holds it.
use rwg2_wasm::*;

#[test]
fn dry_pits_have_no_water() {
    let h = create(1631302684446519836, 1);
    let (x0, z0, w, hh) = (-128, -128, 256u32, 256u32);
    let p = view_cols(h, x0, z0, w, hh, 1);
    let v = unsafe { std::slice::from_raw_parts(p, (3 * w * hh) as usize) };
    let (mut wet, mut dry) = (0, 0);
    let mut box_ = (i32::MAX, i32::MAX, i32::MIN, i32::MIN);
    for j in 0..hh as i32 {
        for i in 0..w as i32 {
            let c = v[3 * (j * w as i32 + i) as usize];
            let (y, water) = ((c & 255) as i32, (c >> 8) & 255);
            if y < 62 {
                if water == 0 {
                    dry += 1;
                    box_ = (
                        box_.0.min(x0 + i),
                        box_.1.min(z0 + j),
                        box_.2.max(x0 + i),
                        box_.3.max(z0 + j),
                    );
                } else {
                    wet += 1;
                }
            }
        }
    }
    println!("below sea: wet {wet}, dry {dry}, dry box {box_:?}");
    assert!(dry > 0, "seed has a dry lava-cave funnel near 0,0");
}
