//! Thaumcraft aura nodes in the Nether and the Twilight Forest, on the block terrain of [`crate::dimterrain`].
//!
//! `ThaumcraftWorldGenerator.worldGeneration`: dim −1 → `generateNether` = totem (needs dim 0/1: never, and the
//! dimension test short-circuits before its draw) + `generateWildNodes`; any other dimension (the Twilight Forest
//! is not on the pack's dimension blacklist, rwg2dump 2.9: `{55=0, 56=0, 173=0}`) → `generateSurface`:
//! vegetation (silverwood / greatwood / flowers), the scattered-feature node (a fresh `MapGenScatteredFeature`
//! knows no starts: none), wild nodes; obelisks, hilltop stones, mounds and totems are dim-0 only.
//!
//! The Forge per-chunk `Random` is seeded from the world seed alone, so a dimension's wild-node rolls sit at the
//! same x, z as the overworld's.

use crate::dimterrain::{nb, tb, NetherChunk, NetherTerrain, TfChunk, TfTerrain};
use crate::dungeons::forge_chunk_random;
use crate::forestry::ClimateTable;
use crate::java_random::JavaRandom;
use crate::jmath::FxMap;
use crate::tc_nodes::{create_random_node, NodeEnv, NodeSource};
use crate::thaumcraft::{TcConfig, TcKind, TcNode, TreeRule};
use std::rc::Rc;

pub enum DimWorld<'a> {
    Nether(&'a NetherTerrain),
    Twilight(&'a TfTerrain),
}

enum Chunk {
    N(NetherChunk),
    T(TfChunk),
}

/// Block class for the node logic.
#[derive(Clone, Copy, PartialEq, Eq)]
enum B {
    Air,
    Water,
    Lava,
    Stone,
    Other,
}

/// [`DimTc::peek_wild`]: a chunk's node contribution known without terrain.
#[derive(Clone, Debug)]
pub enum WildPeek {
    /// The chunk lists no node.
    Nothing,
    /// Only the full replay can tell (a growing silverwood reads the terrain).
    Unknown,
    /// One wild node at (x, z): `rng` is the state `createRandomNodeAt` starts from, `biome` its biome.
    Node {
        x: i32,
        z: i32,
        rng: JavaRandom,
        biome: u16,
    },
}

pub struct DimTc<'a> {
    pub w: DimWorld<'a>,
    pub climate: &'a ClimateTable,
    pub cfg: TcConfig,
    chunks: FxMap<(i32, i32), Rc<Chunk>>,
}

fn has(d: &[String], t: &str) -> bool {
    d.iter().any(|x| x == t)
}

fn greatwood_support(dict: &[String]) -> f32 {
    for t in dict {
        let v = match t.as_str() {
            "CONIFEROUS" | "SAVANNA" | "PLAINS" | "SWAMP" => Some(0.2),
            "FOREST" | "MAGICAL" => Some(1.0),
            "LUSH" => Some(0.5),
            "WATER" | "OCEAN" | "RIVER" | "WET" | "HOT" | "DESERT" | "NETHER" | "MESA"
            | "DENSE" | "SNOWY" | "COLD" | "FROZEN" | "MUSHROOM" | "SANDY" | "BEACH"
            | "MOUNTAIN" | "HILLS" | "DRY" | "SPARSE" | "WASTELAND" | "JUNGLE" | "END"
            | "SPOOKY" | "DEAD" => None,
            _ => return 0.0,
        };
        if let Some(v) = v {
            return v;
        }
    }
    0.0
}

impl<'a> DimTc<'a> {
    pub fn new(w: DimWorld<'a>, climate: &'a ClimateTable, cfg: TcConfig) -> Self {
        DimTc {
            w,
            climate,
            cfg,
            chunks: FxMap::default(),
        }
    }

    fn chunk(&mut self, cx: i32, cz: i32) -> Rc<Chunk> {
        if let Some(c) = self.chunks.get(&(cx, cz)) {
            return c.clone();
        }
        if self.chunks.len() > 512 {
            self.chunks.clear();
        }
        let c = Rc::new(match self.w {
            DimWorld::Nether(n) => Chunk::N(n.chunk(cx, cz)),
            DimWorld::Twilight(t) => Chunk::T(t.chunk(cx, cz)),
        });
        self.chunks.insert((cx, cz), c.clone());
        c
    }

    fn block(&mut self, x: i32, y: i32, z: i32) -> B {
        let c = self.chunk(x >> 4, z >> 4);
        match &*c {
            Chunk::N(n) => match n.at(x, y, z) {
                nb::AIR => B::Air,
                nb::LAVA => B::Lava,
                _ => B::Other, // netherrack is not Blocks.stone
            },
            Chunk::T(t) => match t.at(x, y, z) {
                tb::AIR | tb::VOID => B::Air,
                tb::WATER => B::Water,
                tb::STONE => B::Stone,
                _ => B::Other,
            },
        }
    }

    fn top(&mut self, x: i32, z: i32) -> i32 {
        let c = self.chunk(x >> 4, z >> 4);
        match &*c {
            Chunk::N(n) => {
                let mut y = 127;
                while y > 0 && n.at(x, y, z) == nb::AIR {
                    y -= 1;
                }
                y
            }
            Chunk::T(t) => t.top(x, z),
        }
    }

    fn biome(&self, x: i32, z: i32) -> u16 {
        match self.w {
            DimWorld::Nether(n) => n.biome_at(x, z) as u16,
            DimWorld::Twilight(t) => t.layers.biome_at(x, z) as u16,
        }
    }

    /// The 11³ census on the pre-population blocks. Certain when no count is within 25 % of its threshold.
    fn census(&mut self, x: i32, y: i32, z: i32) -> (NodeEnv, bool) {
        let (mut water, mut lava, mut stone) = (0, 0, 0);
        for dx in -5..=5 {
            for dy in -5..=5 {
                for dz in -5..=5 {
                    match self.block(x + dx, y + dy, z + dz) {
                        B::Water => water += 1,
                        B::Lava => lava += 1,
                        B::Stone => stone += 1,
                        _ => {}
                    }
                }
            }
        }
        let env = NodeEnv {
            water: water > 100,
            lava: lava > 100,
            stone: stone > 500,
            foliage: false,
        };
        let certain = (water - 100i32).abs() > 25
            && (lava - 100i32).abs() > 25
            && (stone - 500i32).abs() > 60;
        (env, certain)
    }

    fn node(
        &mut self,
        r: &mut JavaRandom,
        src: NodeSource,
        x: i32,
        y: i32,
        z: i32,
    ) -> (crate::tc_nodes::NodeInfo, u16) {
        let b = self.biome(x, z);
        let (env, certain) = self.census(x, y, z);
        let d = self.climate.dict(b);
        let mut n = create_random_node(
            r,
            self.cfg.special_node_rarity,
            &self.cfg.aspects,
            src,
            b,
            d,
            env,
        );
        n.env_certain = certain;
        n.exact &= certain;
        (n, b)
    }

    /// Every aura node chunk `(cx, cz)`'s Thaumcraft pass creates whose draws are modelled (wild nodes; silverwood
    /// trunk nodes in the Twilight Forest). A greatwood takes `nextInt(8)` + `nextLong()` (its spider variant may
    /// take 150 more: the chunk stops there). A silverwood whose fit the model cannot decide is listed as possible
    /// and the chunk continues on the "does not fit" branch; nodes after it are flagged `valid = false`.
    /// What chunk `(cx, cz)` can contribute, without terrain (dimension biomes are cheap layer lookups, so the
    /// vegetation draws replay exactly). [`WildPeek::Unknown`] when a growing silverwood needs the terrain;
    /// [`WildPeek::Nothing`] means [`DimTc::nodes`] lists nothing for the chunk.
    pub fn peek_wild(&self, seed: i64, cx: i32, cz: i32) -> WildPeek {
        let mut r = forge_chunk_random(seed, cx, cz);
        let (bx, bz) = (cx * 16, cz * 16);
        let nether = matches!(self.w, DimWorld::Nether(_));
        if !nether && self.cfg.gen_trees {
            if r.next_int_bounded(60) == 3 {
                let x = bx + r.next_int_bounded(16);
                let z = bz + r.next_int_bounded(16);
                let b = self.biome(x, z);
                let d = self.climate.dict(b);
                if !(b == 192 || b == 193) && (has(d, "MAGICAL") || b == 18 || b == 28) {
                    match self.cfg.trees {
                        TreeRule::Fail => {
                            r.next_int_bounded(4);
                        }
                        _ => return WildPeek::Unknown, // fit / trunk nodes read the terrain
                    }
                }
            }
            if r.next_int_bounded(25) == 7 {
                let x = bx + r.next_int_bounded(16);
                let z = bz + r.next_int_bounded(16);
                let b = self.biome(x, z);
                if greatwood_support(self.climate.dict(b)) > r.next_float() {
                    let spiders = r.next_int_bounded(8) == 0;
                    r.next_long();
                    if spiders {
                        match self.cfg.trees {
                            TreeRule::Grow => return WildPeek::Unknown,
                            TreeRule::Fail => {}
                            TreeRule::Model => return WildPeek::Nothing,
                        }
                    }
                }
            }
            let x = bx + r.next_int_bounded(16);
            let z = bz + r.next_int_bounded(16);
            let b = self.biome(x, z);
            let d = self.climate.dict(b);
            let hot = self
                .climate
                .get(b)
                .map(|c| c.temperature > 1.0)
                .unwrap_or(false);
            if hot && has(d, "SANDY") && r.next_int_bounded(30) == 0 {
                for _ in 0..108 {
                    r.next_int_bounded(8);
                }
            }
        }
        if !(self.cfg.gen_aura && r.next_int_bounded(self.cfg.node_rarity) == 0) {
            return WildPeek::Nothing;
        }
        let x = bx + r.next_int_bounded(16);
        let z = bz + r.next_int_bounded(16);
        r.next_int_bounded(4); // the height draw (the height itself needs the terrain, the contents do not)
        WildPeek::Node {
            x,
            z,
            rng: r,
            biome: self.biome(x, z),
        }
    }

    /// The node `createRandomNodeAt` makes from `rng` in `biome` for every census outcome (water/lava/stone/
    /// foliage): the real node is one of these.
    pub fn possible_infos(&self, rng: &JavaRandom, biome: u16) -> Vec<crate::tc_nodes::NodeInfo> {
        let d = self.climate.dict(biome);
        NodeEnv::all()
            .map(|env| {
                let mut r = rng.clone();
                create_random_node(
                    &mut r,
                    self.cfg.special_node_rarity,
                    &self.cfg.aspects,
                    NodeSource::Wild,
                    biome,
                    d,
                    env,
                )
            })
            .collect()
    }

    pub fn nodes(&mut self, seed: i64, cx: i32, cz: i32, out: &mut Vec<TcNode>) {
        let start = out.len();
        let mut cond = None;
        self.nodes_inner(seed, cx, cz, out, &mut cond);
        if let Some(i) = cond {
            for n in &mut out[i.max(start)..] {
                n.valid = false;
                if let Some(info) = n.info.as_mut() {
                    info.exact = false;
                }
            }
        }
    }

    fn nodes_inner(
        &mut self,
        seed: i64,
        cx: i32,
        cz: i32,
        out: &mut Vec<TcNode>,
        cond: &mut Option<usize>,
    ) {
        let mut r = forge_chunk_random(seed, cx, cz);
        let (bx, bz) = (cx * 16, cz * 16);
        let nether = matches!(self.w, DimWorld::Nether(_));
        if !nether && self.cfg.gen_trees {
            if r.next_int_bounded(60) == 3 {
                let x = bx + r.next_int_bounded(16);
                let z = bz + r.next_int_bounded(16);
                let b = self.biome(x, z);
                let d = self.climate.dict(b);
                if !(b == 192 || b == 193) && (has(d, "MAGICAL") || b == 18 || b == 28) {
                    // fits: soil (not under water) and space; vegetation from earlier population is not modelled
                    let fits = match self.cfg.trees {
                        TreeRule::Grow => Some(true),
                        TreeRule::Fail => Some(false),
                        TreeRule::Model => {
                            let top = self.top(x, z);
                            if self.block(x, top, z) == B::Water || top < 31 {
                                Some(false)
                            } else {
                                None
                            }
                        }
                    };
                    match fits {
                        Some(true) => {
                            if !self.silverwood(&mut r, x, z, (cx, cz), out, true) {
                                return;
                            }
                        }
                        Some(false) => {
                            r.next_int_bounded(4);
                        }
                        None => {
                            let mut rr = r.clone();
                            self.silverwood(&mut rr, x, z, (cx, cz), out, false);
                            r.next_int_bounded(4);
                            *cond = Some(out.len());
                        }
                    }
                }
            }
            if r.next_int_bounded(25) == 7 {
                let x = bx + r.next_int_bounded(16);
                let z = bz + r.next_int_bounded(16);
                let b = self.biome(x, z);
                if greatwood_support(self.climate.dict(b)) > r.next_float() {
                    // WorldGenGreatwoodTrees: spiders = nextInt(8) == 0, nextLong() seeds its own Random; a fitting
                    // spider tree draws 150 more (cobwebs)
                    let spiders = r.next_int_bounded(8) == 0;
                    r.next_long();
                    if spiders {
                        match self.cfg.trees {
                            TreeRule::Grow => {
                                for _ in 0..50 {
                                    for bound in [14, 10, 14] {
                                        r.next_int_bounded(bound);
                                    }
                                }
                            }
                            TreeRule::Fail => {}
                            TreeRule::Model => return,
                        }
                    }
                }
            }
            let x = bx + r.next_int_bounded(16);
            let z = bz + r.next_int_bounded(16);
            let b = self.biome(x, z);
            let d = self.climate.dict(b);
            let hot = self
                .climate
                .get(b)
                .map(|c| c.temperature > 1.0)
                .unwrap_or(false);
            if hot && has(d, "SANDY") && r.next_int_bounded(30) == 0 {
                for _ in 0..108 {
                    r.next_int_bounded(8);
                }
            }
        }
        if !(self.cfg.gen_aura && r.next_int_bounded(self.cfg.node_rarity) == 0) {
            return;
        }
        let x = bx + r.next_int_bounded(16);
        let z = bz + r.next_int_bounded(16);
        // getFirstUncoveredY: first q ≥ 5 with air at q + 1; then q + 1 (air by construction)
        let mut q = 5;
        while q < 255 && self.block(x, q + 1, z) != B::Air {
            q += 1;
        }
        q += 1;
        let p = r.next_int_bounded(4);
        if matches!(self.block(x, q + p, z), B::Air | B::Water | B::Lava) {
            q += p;
        }
        let top = self.top(x, z);
        let before = r.clone();
        let (n, b) = self.node(&mut r, NodeSource::Wild, x, q, z);
        // surface nodes move with decoration (trees, grass; Nether: glowstone, fortresses, fire)
        let y_exact = q <= top;
        out.push(TcNode {
            kind: TcKind::WildNode,
            x,
            y: q,
            z,
            y_exact,
            valid: true,
            chunk: (cx, cz),
            info: Some(n),
            rng: Some(before),
            source: NodeSource::Wild,
            biome: b,
        });
    }

    /// `WorldGenSilverwoodTrees.generate` on a fitting spot (see `thaumcraft.rs`): every draw replayed. Returns
    /// whether the draws after it stay exact (trunk node census clear).
    fn silverwood(
        &mut self,
        r: &mut JavaRandom,
        x: i32,
        z: i32,
        chunk: (i32, i32),
        out: &mut Vec<TcNode>,
        fits_known: bool,
    ) -> bool {
        let y = self.top(x, z) + 1;
        let height = r.next_int_bounded(4) + 7;
        if !(y >= 1 && y + height + 1 < 256) {
            return true;
        }
        let end = y + height + 3 + r.next_int_bounded(3);
        let start = y + height - 5;
        for _ in 0..(end - start + 1) * 121 {
            r.next_int_bounded(8);
        }
        let mut chance = (height as f64 * 1.5) as i32;
        let mut last = false;
        let mut certain = true;
        for k in 0..height {
            if k > 0 && !last && r.next_int_bounded(chance) == 0 {
                let before = r.clone();
                let (mut n, b) = self.node(r, NodeSource::Silverwood, x, y + k, z);
                certain &= n.env_certain;
                n.exact &= fits_known;
                out.push(TcNode {
                    kind: TcKind::Silverwood,
                    x,
                    y: y + k,
                    z,
                    y_exact: fits_known,
                    valid: fits_known,
                    chunk,
                    info: Some(n),
                    rng: Some(before),
                    source: NodeSource::Silverwood,
                    biome: b,
                });
                chance += height;
                last = true;
            } else {
                last = false;
            }
        }
        for _ in 0..8 {
            r.next_int_bounded(3);
        }
        for _ in 0..18 {
            for bound in [8, 8, 4, 4, 8, 8] {
                r.next_int_bounded(bound);
            }
        }
        certain
    }
}
