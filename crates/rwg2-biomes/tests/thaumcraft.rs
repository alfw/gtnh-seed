#![cfg(feature = "json")]
use rwg2_biomes::config::RwgConfig;
use rwg2_biomes::dungeons::Terrain;
use rwg2_biomes::forestry::ClimateTable;
use rwg2_biomes::provider::{BiomeProvider, BiomeTable, TableHeights, WorldType};
use rwg2_biomes::thaumcraft::*;

fn root() -> std::path::PathBuf {
    std::env::var("RWG2_DATA_DIR")
        .map(|d| std::path::PathBuf::from(d).parent().unwrap().to_path_buf())
        .unwrap_or_else(|_| std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../.."))
}

/// The two dumped worlds: (seed, thaumcraft.json, real spawn x/z). Both RWG2_CONTINENTS, chunks [-24, 24]^2
/// generated and scanned for tile entities (rwg2dump 2.3 / 2.7; `thaumcraft.*` classes only).
const WORLDS: [(i64, &str, (i32, i32)); 2] = [
    (
        1337,
        "testdata/pack/gtnh-2.9.0/dump23/thaumcraft.json",
        (40, -104),
    ),
    (
        92_900_795,
        "testdata/pack/gtnh-2.9.0/dump27b/thaumcraft.json",
        (-37, -350),
    ),
];

fn load(path: &str) -> Option<serde_json::Value> {
    serde_json::from_str(&std::fs::read_to_string(root().join(path)).ok()?).ok()
}

fn tables() -> (BiomeTable, ClimateTable) {
    let table = BiomeTable::from_rwg2dump_json(
        &std::fs::read_to_string(root().join(pack_rel("biomes.json"))).unwrap(),
    )
    .unwrap();
    let climate = ClimateTable::from_json(
        &std::fs::read_to_string(root().join(pack_rel("climate.json"))).unwrap(),
    )
    .unwrap();
    (table, climate)
}

fn i(v: &serde_json::Value) -> i32 {
    v.as_i64().unwrap() as i32
}

/// Thaumcraft surface features vs the pack (rwg2dump 2.3: seed 1337 CONTINENTS world, chunks [-24, 24]^2 generated
/// and scanned for TC tile entities).
#[test]
fn thaumcraft_matches_pack() {
    let Some(truth) = load(WORLDS[0].1) else {
        eprintln!("skip");
        return;
    };
    let (table, climate) = tables();
    let p = BiomeProvider::new(1337, WorldType::Rwg2Continents, table, RwgConfig::default());
    let mut t = Terrain::new(&p, &TableHeights);
    let mut g = TcGen {
        t: &mut t,
        climate: &climate,
        cfg: TC_GTNH,
    };
    let mut f = Vec::new();
    for cz in -24..=24 {
        for cx in -24..=24 {
            g.chunk(1337, cx, cz, &mut f).unwrap();
        }
    }
    let dropped = resolve_ring_mazes(&mut f);
    eprintln!("rings dropped by maze conflicts: {dropped}");
    for k in [
        TcKind::Mound,
        TcKind::Ring,
        TcKind::Hilltop,
        TcKind::Totem,
        TcKind::WildNode,
    ] {
        let all: Vec<_> = f.iter().filter(|x| x.kind == k).collect();
        let valid = all.iter().filter(|x| x.valid).count();
        eprintln!("{k:?}: rolled {} valid(model) {valid}", all.len());
        for x in all.iter().filter(|x| x.valid && k != TcKind::WildNode) {
            eprintln!(
                "   {:?} ({}, {}) y {} size {:?} node y {}",
                x.kind, x.x, x.z, x.y, x.size, x.node_y
            );
        }
    }
    let tiles = truth["tiles"].as_array().unwrap();
    let altar = tiles
        .iter()
        .find(|x| x["class"].as_str().unwrap().ends_with("TileEldritchAltar"))
        .unwrap();
    let (ax, az) = (i(&altar["x"]), i(&altar["z"]));
    let ring = f
        .iter()
        .find(|x| x.kind == TcKind::Ring && x.x == ax && x.z == az)
        .expect("ring rolled");
    assert!(ring.valid, "real ring rejected by the model");
    // altar at j + 1, node between altar and obelisk
    assert_eq!(ring.node_y, i(&altar["y"]) + 1, "ring node y");
    let totem = f
        .iter()
        .find(|x| x.kind == TcKind::Totem && x.x == 132 && x.z == 137)
        .expect("totem rolled");
    assert!(totem.valid);
    // the dumped area contains exactly one ring: it must be the only "likely" one (p ≥ 0.5; others are at most
    // "possible", see `ring_prob`)
    let likely: Vec<_> = f
        .iter()
        .filter(|x| x.kind == TcKind::Ring && x.valid && x.p_valid >= 0.5)
        .map(|x| (x.x, x.z, x.p_valid))
        .collect();
    assert_eq!(likely.len(), 1, "likely rings {likely:?}");
    assert_eq!((likely[0].0, likely[0].1), (ring.x, ring.z));
}

/// Both worlds: the real ring is rolled and survives maze resolution with the real spawn; combined counts.
#[test]
fn rings_both_worlds() {
    let (table, climate) = tables();
    let (mut real, mut kept, mut claimed) = (0, 0, 0);
    for (seed, path, spawn) in WORLDS {
        let Some(truth) = load(path) else {
            eprintln!("skip {seed}");
            continue;
        };
        let p = BiomeProvider::new(
            seed,
            WorldType::Rwg2Continents,
            table.clone(),
            RwgConfig::default(),
        );
        let mut t = Terrain::new(&p, &TableHeights);
        let mut g = TcGen {
            t: &mut t,
            climate: &climate,
            cfg: TC_GTNH,
        };
        let mut f = Vec::new();
        // ring mazes reach ~42 chunks: replay a margin so conflicts from outside the dumped area are seen
        for cz in -24..=24 {
            for cx in -24..=24 {
                g.chunk(seed, cx, cz, &mut f).unwrap();
            }
        }
        let dropped = resolve_ring_mazes_spawn(&mut f, spawn);
        let altars: Vec<_> = truth["tiles"]
            .as_array()
            .unwrap()
            .iter()
            .filter(|x| x["class"].as_str().unwrap().ends_with("TileEldritchAltar"))
            .map(|x| (i(&x["x"]), i(&x["y"]), i(&x["z"])))
            .collect();
        let valid: Vec<_> = f
            .iter()
            .filter(|x| x.kind == TcKind::Ring && x.valid)
            .map(|x| (x.x, x.z, x.y))
            .collect();
        eprintln!(
            "seed {seed}: real altars {altars:?}; model rings {valid:?} (maze-dropped {dropped})"
        );
        for &(ax, ay, az) in &altars {
            real += 1;
            let r = f
                .iter()
                .find(|x| x.kind == TcKind::Ring && x.x == ax && x.z == az)
                .expect("real ring rolled");
            assert!(r.valid, "seed {seed}: real ring ({ax},{az}) rejected");
            assert_eq!(r.node_y, ay + 1);
            kept += 1;
        }
        claimed += valid.len();
    }
    eprintln!("rings: real {real}, kept by model {kept}, model claims {claimed}");
}

fn parse(s: &str) -> Vec<(String, u16)> {
    s.split_whitespace()
        .map(|t| {
            let (a, b) = t.split_once(':').unwrap();
            (a.to_string(), b.parse().unwrap())
        })
        .collect()
}

/// Every Thaumcraft-class node in both dumps (silverwood PURE nodes, totem DARK nodes) is predicted at its exact
/// block with the exact type, modifier and aspect list (names, amounts and order). Wild/ring/hilltop/mound nodes
/// are Gadomancy `TileExtendedNode`s and were not dumped.
#[test]
fn node_aspects_match_dumps() {
    let (table, climate) = tables();
    let (mut total, mut matched) = (0, 0);
    let (mut wild, mut wild_cave, mut wild_exact, mut wild_yexact) = (0, 0, 0, 0);
    for (seed, path, _) in WORLDS {
        let Some(truth) = load(path) else {
            eprintln!("skip {seed}");
            continue;
        };
        let p = BiomeProvider::new(
            seed,
            WorldType::Rwg2Continents,
            table.clone(),
            RwgConfig::default(),
        );
        let mut t = Terrain::new(&p, &TableHeights);
        let mut g = TcGen {
            t: &mut t,
            climate: &climate,
            cfg: TC_GTNH,
        };
        let mut nodes = Vec::new();
        for cz in -24..=24 {
            for cx in -24..=24 {
                g.nodes(seed, cx, cz, &mut nodes).unwrap();
            }
        }
        for tl in truth["tiles"]
            .as_array()
            .unwrap()
            .iter()
            .filter(|x| x["class"].as_str().unwrap().ends_with("TileNode"))
        {
            total += 1;
            let (x, y, z) = (i(&tl["x"]), i(&tl["y"]), i(&tl["z"]));
            let obs = tl["aspects"].as_str().unwrap();
            let ty = tl["type"].as_str().unwrap();
            let n = nodes
                .iter()
                .find(|n| (n.x, n.y, n.z) == (x, y, z))
                .unwrap_or_else(|| panic!("no node predicted at ({x},{y},{z})"));
            let info = n.info.expect("node info");
            let pred = info.aspects_string(&TC_ASPECTS_GTNH);
            eprintln!(
                "seed {seed} {:?} ({x},{y},{z}) real {ty} {obs} | model {:?} {:?} {pred} (env {:?}, certain {})",
                n.kind, info.node_type, info.modifier, info.env, info.env_certain
            );
            assert_eq!(format!("{:?}", info.node_type).to_uppercase(), ty);
            assert_eq!(tl["modifier"].as_str().unwrap(), "null");
            assert!(info.modifier.is_none());
            assert_eq!(parse(&pred), parse(obs), "aspects at ({x},{y},{z})");
            matched += 1;
        }
        let top = |g: &mut TcGen, x, z| g.t.top(x, z).unwrap();
        for n in nodes.iter().filter(|n| n.kind == TcKind::WildNode) {
            wild += 1;
            wild_cave += (n.y <= top(&mut g, n.x, n.z)) as usize;
            wild_exact += n.info.is_some_and(|i| i.exact) as usize;
            wild_yexact += n.y_exact as usize;
        }
        for n in nodes.iter().filter(|n| n.kind == TcKind::WildNode).take(6) {
            let info = n.info.unwrap();
            eprintln!(
                "  wild ({},{},{}) y_exact {} {:?} {:?} {} exact {}",
                n.x,
                n.y,
                n.z,
                n.y_exact,
                info.node_type,
                info.modifier,
                info.aspects_string(&TC_ASPECTS_GTNH),
                info.exact
            );
        }
    }
    eprintln!("dumped TC nodes matched exactly: {matched}/{total}");
    eprintln!("wild nodes predicted: {wild} ({wild_cave} in caves; y exact {wild_yexact}; aspects exact {wild_exact})");
    assert_eq!(matched, total);
}

/// `createRandomNodeAt` draw accounting on a fixed stream (regression for the node RNG port).
#[test]
fn node_generation_fixed_stream() {
    let dict: Vec<String> = ["FOREST", "HILLS"].iter().map(|s| s.to_string()).collect();
    assert_eq!(biome_aura(&dict), 120);
    let mut r = rwg2_biomes::java_random::JavaRandom::new(42);
    let a = create_random_node(
        &mut r,
        18,
        &TC_ASPECTS_GTNH,
        NodeSource::Wild,
        4,
        &dict,
        NodeEnv::default(),
    );
    let mut r2 = rwg2_biomes::java_random::JavaRandom::new(42);
    let b = create_random_node(
        &mut r2,
        18,
        &TC_ASPECTS_GTNH,
        NodeSource::Wild,
        4,
        &dict,
        NodeEnv::default(),
    );
    assert_eq!(a, b);
    assert!(a.total() > 0 && !a.aspects().is_empty());
    // value = nextInt(aura/2) + aura/2 bounds the sum of the computed shares
    assert!(a.total() <= 120 + 2 * a.aspects().len() as u32);
}

/// `TcGen::chunk` with `predict_nodes` fills wild-node features like `TcGen::nodes`.
#[test]
fn chunk_predict_nodes_matches_nodes() {
    let (table, climate) = tables();
    let p = BiomeProvider::new(1337, WorldType::Rwg2Continents, table, RwgConfig::default());
    let mut t = Terrain::new(&p, &TableHeights);
    let cfg = TcConfig {
        predict_nodes: true,
        ..TC_GTNH
    };
    let mut g = TcGen {
        t: &mut t,
        climate: &climate,
        cfg,
    };
    let (mut f, mut n) = (Vec::new(), Vec::new());
    for cz in -8..=8 {
        for cx in -8..=8 {
            g.chunk(1337, cx, cz, &mut f).unwrap();
            g.nodes(1337, cx, cz, &mut n).unwrap();
        }
    }
    let wild: Vec<_> = f.iter().filter(|x| x.kind == TcKind::WildNode).collect();
    assert!(!wild.is_empty());
    for w in wild {
        let m = n
            .iter()
            .find(|m| m.kind == TcKind::WildNode && (m.x, m.z) == (w.x, w.z))
            .unwrap();
        assert_eq!(w.node_y, m.y);
        assert_eq!(w.node, m.info);
    }
}

/// rwg2dump 2.8 `tcnodes.json` (seed 92900795, chunks [-24, 24]^2): every TileNode subclass incl. Gadomancy's
/// `TileExtendedNode` (wild, ring, hilltop, mound nodes).
#[test]
fn all_nodes_vs_dump28() {
    let Some(truth) = load("testdata/pack/gtnh-2.9.0/dump28/tcnodes.json") else {
        eprintln!("skip");
        return;
    };
    let seed = 92_900_795;
    let (table, climate) = tables();
    let p = BiomeProvider::new(seed, WorldType::Rwg2Continents, table, RwgConfig::default());
    let mut t = Terrain::new(&p, &TableHeights);
    let mut g = TcGen {
        t: &mut t,
        climate: &climate,
        cfg: TC_GTNH,
    };
    let mut nodes = Vec::new();
    for cz in -24..=24 {
        for cx in -24..=24 {
            g.nodes(seed, cx, cz, &mut nodes).unwrap();
        }
    }
    let real = truth["nodes"].as_array().unwrap();
    let (mut xz, mut y, mut ty, mut asp, mut asp_given_y) = (0, 0, 0, 0, 0);
    let mut unmatched = Vec::new();
    for tl in real {
        let (x, ry, z) = (i(&tl["x"]), i(&tl["y"]), i(&tl["z"]));
        let obs = tl["aspects"].as_str().unwrap();
        let rty = tl["type"].as_str().unwrap();
        let rmod = tl["modifier"].as_str().unwrap();
        let Some(n) = nodes.iter().find(|n| (n.x, n.z) == (x, z)) else {
            unmatched.push((x, ry, z, rty.to_string(), obs.to_string()));
            continue;
        };
        xz += 1;
        let yok = n.y == ry;
        y += yok as usize;
        let (pty, pmod, pasp, exact) = match n.info {
            Some(info) => (
                format!("{:?}", info.node_type).to_uppercase(),
                info.modifier
                    .map(|m| format!("{m:?}").to_uppercase())
                    .unwrap_or("null".into()),
                info.aspects_string(&TC_ASPECTS_GTNH),
                info.exact,
            ),
            None => ("?".into(), "?".into(), String::new(), false),
        };
        let tok = pty == rty && pmod == rmod;
        ty += tok as usize;
        let aok = tok && parse(&pasp) == parse(obs);
        asp += aok as usize;
        asp_given_y += (aok && yok) as usize;
        eprintln!(
            "{:?} ({x},{z}) y real {ry} model {} {} | real {rty}/{rmod} {obs} | model {pty}/{pmod} {pasp} exact {exact} {}",
            n.kind,
            n.y,
            if yok { "ok" } else { "XX" },
            if aok { "OK" } else { "--" }
        );
    }
    for u in &unmatched {
        let ch = (u.0 >> 4, u.2 >> 4);
        let mut f = Vec::new();
        g.chunk(seed, ch.0, ch.1, &mut f).unwrap();
        let near: Vec<_> = nodes
            .iter()
            .filter(|n| n.chunk == ch)
            .map(|n| (n.kind, n.x, n.y, n.z))
            .collect();
        let feats: Vec<_> = f.iter().map(|x| (x.kind, x.x, x.z, x.valid)).collect();
        eprintln!(
            "unmatched real node {u:?} chunk {ch:?}: model nodes {near:?} features {feats:?}"
        );
    }
    let realxz: Vec<(i32, i32)> = real.iter().map(|t| (i(&t["x"]), i(&t["z"]))).collect();
    for n in nodes.iter().filter(|n| !realxz.contains(&(n.x, n.z))) {
        eprintln!(
            "extra model node {:?} ({},{},{}) valid {} exact {:?}",
            n.kind,
            n.x,
            n.y,
            n.z,
            n.valid,
            n.info.map(|i| i.exact)
        );
    }
    let wild = nodes.iter().filter(|n| n.kind == TcKind::WildNode).count();
    eprintln!(
        "dump28: real {} | predicted {} ({} wild) | xz {xz} | y {y} | type+mod {ty} | aspects {asp} (with y {asp_given_y})",
        real.len(),
        nodes.len(),
        wild
    );
    assert!(xz >= 76 && asp >= 69, "regression: xz {xz} aspects {asp}");
}

/// Diagnostic: per chunk where the silverwood fit decides the outcome, which hypothesis (grows / fails) reproduces
/// the real nodes of rwg2dump 2.8 (seed 92900795, chunks ±24).
#[test]
fn silverwood_hypotheses_dump28() {
    let Some(truth) = load("testdata/pack/gtnh-2.9.0/dump28/tcnodes.json") else {
        return;
    };
    let seed = 92_900_795;
    let (table, climate) = tables();
    let p = BiomeProvider::new(seed, WorldType::Rwg2Continents, table, RwgConfig::default());
    let real: Vec<(i32, i32, i32, String)> = truth["nodes"]
        .as_array()
        .unwrap()
        .iter()
        .map(|n| {
            (
                i(&n["x"]),
                i(&n["y"]),
                i(&n["z"]),
                n["aspects"].as_str().unwrap().to_string(),
            )
        })
        .collect();
    let run = |rule: TreeRule| {
        let mut t = Terrain::new(&p, &TableHeights);
        let mut g = TcGen {
            t: &mut t,
            climate: &climate,
            cfg: TcConfig {
                trees: rule,
                ..TC_GTNH
            },
        };
        let mut out = std::collections::BTreeMap::new();
        for cz in -24..=24 {
            for cx in -24..=24 {
                let mut v = Vec::new();
                g.nodes(seed, cx, cz, &mut v).unwrap();
                let v: Vec<(TcKind, i32, i32, String)> = v
                    .iter()
                    .filter(|n| !matches!(n.kind, TcKind::Ring | TcKind::Hilltop | TcKind::Mound))
                    .map(|n| {
                        (
                            n.kind,
                            n.x,
                            n.z,
                            n.info
                                .map(|i| i.aspects_string(&TC_ASPECTS_GTNH))
                                .unwrap_or_default(),
                        )
                    })
                    .collect();
                out.insert((cx, cz), v);
            }
        }
        out
    };
    let grow = run(TreeRule::Grow);
    let fail = run(TreeRule::Fail);
    let (mut g_ok, mut f_ok, mut both, mut none) = (0, 0, 0, 0);
    for (ch, gv) in &grow {
        let fv = &fail[ch];
        if gv == fv {
            continue;
        }
        let rc: Vec<_> = real
            .iter()
            .filter(|r| (r.0 >> 4, r.2 >> 4) == *ch)
            .collect();
        let score = |v: &Vec<(TcKind, i32, i32, String)>| {
            v.len() == rc.len()
                && v.iter()
                    .all(|n| rc.iter().any(|r| (r.0, r.2) == (n.1, n.2)))
        };
        let (gs, fs) = (score(gv), score(fv));
        match (gs, fs) {
            (true, true) => both += 1,
            (true, false) => g_ok += 1,
            (false, true) => f_ok += 1,
            _ => none += 1,
        }
        let b = p.biome_gen_id(ch.0 * 16 + 8, ch.1 * 16 + 8, &TableHeights);
        eprintln!(
            "chunk {ch:?} biome {b}: real {:?} | grow {:?} | fail {:?} → {}",
            rc.iter().map(|r| (r.0, r.1, r.2)).collect::<Vec<_>>(),
            gv.iter().map(|n| (n.0, n.1, n.2)).collect::<Vec<_>>(),
            fv.iter().map(|n| (n.0, n.1, n.2)).collect::<Vec<_>>(),
            match (gs, fs) {
                (true, true) => "both",
                (true, false) => "GROWS",
                (false, true) => "FAILS",
                _ => "neither",
            }
        );
    }
    eprintln!("decided chunks: grows {g_ok}, fails {f_ok}, both fit {both}, neither {none}");
    // every silverwood roll is decided unambiguously by the real nodes: 2 grew, 12 did not (all Sacred Springs)
    assert_eq!((g_ok, f_ok, none), (2, 12, 0));
}

/// Seed 1337 (rwg2dump 2.3): 3 of 6 silverwood rolls grew (Sacred Springs, Y63–78 either way: altitude does not
/// decide; BOP's 30 shrubs per chunk do). Together with 92900795: 5 of 20.
#[test]
fn silverwood_rolls_1337() {
    let Some(truth) = load(WORLDS[0].1) else {
        return;
    };
    let seed = 1337;
    let (table, climate) = tables();
    let p = BiomeProvider::new(seed, WorldType::Rwg2Continents, table, RwgConfig::default());
    let mut t = Terrain::new(&p, &TableHeights);
    let mut g = TcGen {
        t: &mut t,
        climate: &climate,
        cfg: TcConfig {
            trees: TreeRule::Grow,
            ..TC_GTNH
        },
    };
    let real: Vec<(i32, i32, i32)> = truth["tiles"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|x| {
            x["class"].as_str().unwrap().ends_with("TileNode") && x["type"].as_str() == Some("PURE")
        })
        .map(|x| (i(&x["x"]), i(&x["y"]), i(&x["z"])))
        .collect();
    let mut nodes = Vec::new();
    for cz in -24..=24 {
        for cx in -24..=24 {
            g.nodes(seed, cx, cz, &mut nodes).unwrap();
        }
    }
    let mut seen = std::collections::BTreeSet::new();
    for n in nodes.iter().filter(|n| n.kind == TcKind::Silverwood) {
        if !seen.insert((n.x, n.z)) {
            continue;
        }
        let top = g.t.top(n.x, n.z).unwrap();
        let (_, b) = g.t.column(n.x, n.z).unwrap();
        let hit = real.iter().any(|r| (r.0, r.2) == (n.x, n.z));
        eprintln!(
            "silverwood ({},{}) top {top} biome {b} real {hit}",
            n.x, n.z
        );
    }
    eprintln!("real PURE nodes {real:?}");
    assert_eq!((seen.len(), real.len()), (6, 3));
    assert!(real.iter().all(|r| seen.contains(&(r.0, r.2))));
}

/// `data/<PACK_DATA>/<name>` relative to the repo (`PACK_DATA` default `gtnh-2.9.0`; `gtnh-2.9.1` = RWG2 2.0.9 pack).
#[allow(dead_code)]
fn pack_rel(name: &str) -> String {
    format!("data/{}/{name}", std::env::var("PACK_DATA").unwrap_or_else(|_| "gtnh-2.9.0".into()))
}
