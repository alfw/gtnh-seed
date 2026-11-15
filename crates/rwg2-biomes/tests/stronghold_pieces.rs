#![cfg(feature = "json")]
use rwg2_biomes::stronghold_pieces::*;
use rwg2_biomes::structures::stronghold_ring;

fn root() -> std::path::PathBuf {
    std::env::var("RWG2_DATA_DIR")
        .map(|d| std::path::PathBuf::from(d).parent().unwrap().to_path_buf())
        .unwrap_or_else(|_| std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../.."))
}

fn bbox(v: &serde_json::Value) -> BBox {
    let a: Vec<i32> = v
        .as_array()
        .unwrap()
        .iter()
        .map(|x| x.as_i64().unwrap() as i32)
        .collect();
    [a[0], a[1], a[2], a[3], a[4], a[5]]
}

/// All strongholds of the pack dump (seed 1337 RWG2_CONTINENTS): start box, piece classes + boxes in order.
#[test]
fn strongholds_match_pack() {
    let Ok(s) =
        std::fs::read_to_string(root().join("testdata/pack/gtnh-2.9.0/dump/structures.json"))
    else {
        eprintln!("structures.json missing; skipping");
        return;
    };
    let truth: serde_json::Value = serde_json::from_str(&s).unwrap();
    let seed = match &truth["worldSeed"] {
        serde_json::Value::String(s) => s.parse::<i64>().unwrap(),
        v => v.as_i64().unwrap(),
    };
    let ring = stronghold_ring(seed);
    let starts = truth["strongholdGenerator"]["starts"].as_array().unwrap();
    assert_eq!(starts.len(), 3);
    let t0 = std::time::Instant::now();
    for st in starts {
        let c = (
            st["chunkX"].as_i64().unwrap() as i32,
            st["chunkZ"].as_i64().unwrap() as i32,
        );
        assert!(ring.contains(&c), "{c:?} not in ring {ring:?}");
        let sh = stronghold(seed, c).expect("no portal room");
        let want = st["pieces"].as_array().unwrap();
        for (i, (p, w)) in sh.pieces.iter().zip(want).enumerate() {
            let cls = w["class"].as_str().unwrap();
            let name = cls.rsplit('$').next().unwrap();
            assert_eq!(p.kind.java_name(), name, "{c:?} piece {i}");
            assert_eq!(p.bbox, bbox(&w["box"]), "{c:?} piece {i} {name}");
        }
        assert_eq!(sh.bbox, bbox(&st["box"]), "start box {c:?}");
        assert_eq!(sh.pieces.len(), want.len(), "piece count {c:?}");
        let pr = portal_room(seed, c).unwrap();
        let wpr = want
            .iter()
            .find(|w| w["class"].as_str().unwrap().ends_with("$PortalRoom"))
            .unwrap();
        assert_eq!(pr.bbox, bbox(&wpr["box"]));
        // Frame centre lies inside the room, 3 above the floor, 5/10 blocks in.
        let b = pr.bbox;
        let (x, y, z) = pr.eye_frame_centre;
        assert!(x >= b[0] && x <= b[3] && z >= b[2] && z <= b[5] && y == b[1] + 3);
        println!(
            "chunk {c:?}: {} pieces, start box {:?}, portal room {:?} orient {} frame centre {:?} box centre {:?}",
            sh.pieces.len(),
            sh.bbox,
            pr.bbox,
            pr.orientation,
            pr.eye_frame_centre,
            pr.box_centre
        );
    }
    let t1 = std::time::Instant::now();
    let n = 200;
    for i in 0..n {
        for c in stronghold_ring(i) {
            std::hint::black_box(portal_room(i, c));
        }
    }
    println!(
        "verify: {:?}; {} portal_room calls: {:?} each",
        t1 - t0,
        3 * n,
        t1.elapsed() / (3 * n as u32)
    );
}
