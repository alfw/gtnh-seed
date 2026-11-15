use rwg2_biomes::config::RwgConfig;
use rwg2_biomes::provider::{BiomeProvider, BiomeTable, WorldType};
#[test]
fn tc_features_match_native() {
    let t = BiomeTable::from_rwg2dump_json(include_str!("../../../data/gtnh-2.9.0/biomes.json"))
        .unwrap();
    let p = BiomeProvider::new(1337, WorldType::Rwg2Continents, t, RwgConfig::default());
    let mut out = Vec::new();
    rwg2_wasm::features_into(
        &p,
        -24 * 16,
        -24 * 16,
        24 * 16 + 15,
        24 * 16 + 15,
        8,
        &mut out,
    );
    let mut c = std::collections::BTreeMap::new();
    for r in out.chunks(6) {
        *c.entry(r[0]).or_insert(0) += 1;
    }
    // pack (rwg2dump 2.3): 1 real ring + 1 real totem here; per cell the 3 extra rings lose maze conflicts in the
    // viewer (JS) and one totem is a known false positive
    // 2026-11-03 rings are a probability (`ring_prob`, record y = j | p% << 16); records = rings with p ≥ 5%
    // before maze conflicts (resolved in JS across cells; the native test `rings.rs` resolves them: only the real ring
    // at (-123, 15) stays "likely"). Here, pre-maze: 6 rings, 4 likely including the real one.
    let rings: Vec<&[i32]> = out.chunks(6).filter(|r| r[0] == 12).collect();
    let likely: Vec<(i32, i32)> = rings
        .iter()
        .filter(|r| (r[3] >> 16) >= 50)
        .map(|r| (r[1], r[2]))
        .collect();
    assert!(
        likely.contains(&(-123, 15)),
        "real ring not likely: {likely:?}"
    );
    assert_eq!((rings.len(), likely.len()), (6, 4));
    assert_eq!(c.get(&14), Some(&2));
}
