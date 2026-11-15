//! Thaumcraft 4.2.3.5 overworld surface generation (`ThaumcraftWorldGenerator.generateSurface`), as far as it is
//! determined by the seed: the per-chunk Random is Forge's (`dungeons::forge_chunk_random`) and the order of draws
//! is replayed exactly. Branches whose draw count depends on the live world (a greatwood/silverwood tree actually
//! growing, a wild aura node being created) make the rest of that chunk uncertain.
//!
//! Draw order per chunk (GTNH 2.9.0 config: ores off, aura on, structures on, nodeRarity 36):
//! 1. vegetation: `nextInt(60) == 3` silverwood (x, z; grows only in MAGICAL / forestHills / birchForestHills
//!    biomes), `nextInt(25) == 7` greatwood (x, z, nextFloat < greatwood support → grows), flower spot (x, z;
//!    `nextInt(30)` if the biome's top block is sand and temperature > 1; a flower patch is a fixed 108 draws)
//! 2. ores (cinnabar 18×3, amber 20×3, infused stone 8×(…)) — disabled in GTNH, not modelled
//! 3. wild aura node: `nextInt(nodeRarity) == 0` → x, z, `nextInt(4)`, node creation (variable)
//! 4. structures: x, z, then `nextInt(150) == 0` barrow mound, else `nextInt(66) == 0` eldritch ring + obelisk
//!    (w, h = 11 + nextInt(6)·2), else `nextInt(40) == 0` hilltop stones; each checks the terrain first
//! 5. totem: `nextInt(nodeRarity·10) == 0` (only if nothing above made an aura node) → x, z
//!
//! Validated on the pack (seed 1337, chunks ±24): the eldritch ring at (−123, 15) and the totem at (132, 137)
//! reproduce exactly (roll and position).
//!
//! **Aura nodes** (see [`crate::tc_nodes`] for type/modifier/aspects): wild nodes, ring, hilltop and mound nodes are
//! `blockAiry` nodes. GTNH's Gadomancy (`enableAdditionalNodeTypes=true`) replaces `blockAiry` with its
//! `BlockNode`, whose tile entity is `makeo.gadomancy.common.blocks.tiles.TileExtendedNode` (a `TileNode`
//! subclass) — rwg2dump ≤ 2.3 only listed `thaumcraft.*` tile entities, which is why no wild node showed up.
//! Totem nodes (inside `blockCosmeticSolid:8`) and silverwood nodes (inside `blockMagicalLog:2`) keep
//! Thaumcraft's own `TileNode`. Wild node height: `Utils.getFirstUncoveredY` scans up from y 6 for the first
//! air block, so most wild nodes sit in the lowest cave of the column (lava below y 11 is not air), else 1–4
//! blocks above the ground / sea.

use crate::dungeons::{forge_chunk_random, Terrain};
use crate::forestry::ClimateTable;
use crate::gtorder::{CaveMask, Caves};
use crate::java_random::JavaRandom;
pub use crate::tc_nodes::*;

#[derive(Clone, Copy, Debug)]
pub struct TcConfig {
    pub gen_trees: bool,
    pub gen_aura: bool,
    pub gen_structure: bool,
    pub node_rarity: i32,
    pub special_node_rarity: i32,
    /// Pack aspect registry (complex picks).
    pub aspects: TcAspectTable,
    /// Hodgepodge `fastRandom` cave RNG (GTNH default on).
    pub fast_cave_random: bool,
    /// [`TcGen::chunk`] also predicts wild node height/contents (cave carving + block census per wild node,
    /// roughly doubles the replay cost) and keeps replaying a chunk after a wild node whose census is clear.
    /// Off in [`TC_GTNH`] (viewer/search speed); [`TcGen::nodes`] always predicts.
    pub predict_nodes: bool,
    /// Whether Thaumcraft's own trees fit where they roll (decides how many draws they take).
    pub trees: TreeRule,
    /// [`TcGen::chunk`] stops at the structure roll unless it is an eldritch ring: no terrain for mounds,
    /// hilltops or totems (maze-conflict margins only need rings). Same rings as the full replay.
    pub rings_only: bool,
}

/// Silverwood growth decision. `WorldGenSilverwoodTrees` takes one draw when it does not fit and a fixed,
/// replayable sequence when it does; the fit depends on the populated world (other trees' logs and leaves).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TreeRule {
    /// The model's verdict (soil + space on terrain and estimated vegetation; uncertain → chunk stops).
    Model,
    /// Assume every silverwood fits (diagnostics).
    Grow,
    /// Assume none fits (diagnostics).
    Fail,
}

/// GTNH 2.9.0 `config/Thaumcraft.cfg`.
pub const TC_GTNH: TcConfig = TcConfig {
    gen_trees: true,
    gen_aura: true,
    gen_structure: true,
    node_rarity: 36,
    special_node_rarity: 18,
    aspects: TC_ASPECTS_GTNH,
    fast_cave_random: true,
    predict_nodes: false,
    trees: TreeRule::Model,
    rings_only: false,
};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(u8)]
pub enum TcKind {
    Mound = 1,
    Ring = 2,
    Hilltop = 3,
    Totem = 4,
    WildNode = 5,
    /// Silverwood tree node (only from [`TcGen::nodes`]; never in [`TcGen::chunk`]'s output).
    Silverwood = 6,
}

#[derive(Clone, Copy, Debug)]
pub struct TcFeature {
    pub kind: TcKind,
    pub x: i32,
    pub z: i32,
    /// Estimated y of the structure base (ring: surface; hilltop/mound: surface; totem: top block).
    pub y: i32,
    /// Terrain check on the model passed.
    pub valid: bool,
    /// Probability that the game builds it (rings: calibrated on the dumped worlds; others: `valid` as 0/1).
    pub p_valid: f32,
    /// The draws before this roll were fully determined (no world-dependent branch earlier in the chunk).
    pub exact: bool,
    /// Ring size (w, h) in Outer Lands chunks.
    pub size: (i32, i32),
    /// Chunk the roll happened in.
    pub chunk: (i32, i32),
    /// The aura node this feature creates (rings and totems; wild nodes only with [`TcConfig::predict_nodes`];
    /// `None` for hilltop stones and mounds — their chest loot consumes a world-dependent number of draws first —
    /// and for invalid features).
    pub node: Option<NodeInfo>,
    /// Node block y (`i32::MIN` without a node). Wild nodes: lowest cave / just above ground (estimate).
    pub node_y: i32,
}

/// One predicted aura node with its position ([`TcGen::nodes`]).
#[derive(Clone, Debug)]
pub struct TcNode {
    /// What made it: WildNode, Ring, Totem, Hilltop, Mound or Silverwood.
    pub kind: TcKind,
    pub x: i32,
    pub y: i32,
    pub z: i32,
    /// `y` is exact on the model (no vegetation / unmodelled blocks can move it).
    pub y_exact: bool,
    /// The structure behind it passed the model's checks (wild nodes always; rings before maze resolution).
    pub valid: bool,
    pub chunk: (i32, i32),
    /// `None` when the draws before the node are not modelled (hilltop stones, mounds).
    pub info: Option<NodeInfo>,
    /// Random state right before `createRandomNodeAt`, chunk biome and dictionary at the node: lets callers
    /// re-evaluate with other census verdicts ([`TcNode::with_env`]).
    pub rng: Option<JavaRandom>,
    pub source: NodeSource,
    pub biome: u16,
}

impl TcNode {
    pub fn with_env(
        &self,
        cfg: &TcConfig,
        climate: &ClimateTable,
        env: NodeEnv,
    ) -> Option<NodeInfo> {
        let mut r = self.rng.clone()?;
        let mut n = create_random_node(
            &mut r,
            cfg.special_node_rarity,
            &cfg.aspects,
            self.source,
            self.biome,
            climate.dict(self.biome),
            env,
        );
        n.env_certain = false;
        n.exact = false;
        Some(n)
    }
}

/// Mineshaft air boxes per chunk (`mineshaft::air_boxes` clipped to the chunk's x/z), per seed.
type MineCache = Option<(
    i64,
    crate::mineshaft::Seeds,
    crate::jmath::FxMap<(i32, i32), Vec<crate::mineshaft::BBox>>,
)>;

thread_local! {
    static MINES: std::cell::RefCell<MineCache> = const { std::cell::RefCell::new(None) };
}

type CaveCache = Option<(i64, bool, Caves, crate::jmath::FxMap<(i32, i32), CaveMask>)>;

thread_local! {
    static CAVES: std::cell::RefCell<CaveCache> =
        const { std::cell::RefCell::new(None) };
}

/// Per in-game biome id facts Thaumcraft reads (from the pack's BiomeDictionary order).
fn greatwood_support(dict: &[String]) -> f32 {
    // BiomeHandler.getBiomeSupportsGreatwood: first type (dictionary order) registered with greatwood = true
    for t in dict {
        let v = match t.as_str() {
            "CONIFEROUS" | "SAVANNA" | "PLAINS" | "SWAMP" => Some(0.2),
            "FOREST" | "MAGICAL" => Some(1.0),
            "LUSH" => Some(0.5),
            "WATER" | "OCEAN" | "RIVER" | "WET" | "HOT" | "DESERT" | "NETHER" | "MESA"
            | "DENSE" | "SNOWY" | "COLD" | "FROZEN" | "MUSHROOM" | "SANDY" | "BEACH"
            | "MOUNTAIN" | "HILLS" | "DRY" | "SPARSE" | "WASTELAND" | "JUNGLE" | "END"
            | "SPOOKY" | "DEAD" => None,
            _ => return 0.0, // unregistered type: NPE inside the try → 0
        };
        if let Some(v) = v {
            return v;
        }
    }
    0.0
}

fn has(dict: &[String], t: &str) -> bool {
    dict.iter().any(|d| d == t)
}

/// Test switch: turn the no-terrain pre-checks ([`TcGen::node_roll_possible`], [`TcGen::feature_roll_possible`],
/// the dimension node peek in search) off, so tests can compare the fast paths with the full replay.
pub static SLOW_PATHS: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);

pub fn slow_paths() -> bool {
    SLOW_PATHS.load(std::sync::atomic::Ordering::Relaxed)
}

/// Which biome-dependent vegetation branches are possible for a set of chunk biomes (no-terrain pre-checks).
#[derive(Clone, Copy, Debug)]
pub struct VegBounds {
    /// Some possible biome grows silverwood / some does not.
    pub silverwood_grows: bool,
    pub silverwood_absent: bool,
    /// Range of the greatwood support over the possible biomes.
    pub greatwood_min: f32,
    pub greatwood_max: f32,
    /// Some possible biome is hot and sandy (flower patch draw) / some is not.
    pub flowers_draw: bool,
    pub flowers_skip: bool,
}

impl VegBounds {
    /// No knowledge: every branch possible.
    pub const ANY: VegBounds = VegBounds {
        silverwood_grows: true,
        silverwood_absent: true,
        greatwood_min: 0.0,
        greatwood_max: 1.0,
        flowers_draw: true,
        flowers_skip: true,
    };

    /// Nothing possible yet (fold biomes in with [`VegBounds::add`]).
    pub const NONE: VegBounds = VegBounds {
        silverwood_grows: false,
        silverwood_absent: false,
        greatwood_min: f32::INFINITY,
        greatwood_max: f32::NEG_INFINITY,
        flowers_draw: false,
        flowers_skip: false,
    };

    /// The replay's per-biome rules: (silverwood grows, greatwood support, hot sandy flower biome).
    pub fn biome_rules(b: u16, climate: &ClimateTable) -> (bool, f32, bool) {
        let d = climate.dict(b);
        let grows = !(b == 192 || b == 193) && (has(d, "MAGICAL") || b == 18 || b == 28);
        let hot = climate.get(b).map(|c| c.temperature > 1.0).unwrap_or(false);
        (grows, greatwood_support(d), hot && has(d, "SANDY"))
    }

    /// Adds one possible biome (its [`VegBounds::biome_rules`]).
    pub fn add(&mut self, grows: bool, support: f32, sandy: bool) {
        self.silverwood_grows |= grows;
        self.silverwood_absent |= !grows;
        self.greatwood_min = self.greatwood_min.min(support);
        self.greatwood_max = self.greatwood_max.max(support);
        self.flowers_draw |= sandy;
        self.flowers_skip |= !sandy;
    }
}

pub struct TcGen<'t, 'a> {
    pub t: &'t mut Terrain<'a>,
    pub climate: &'t ClimateTable,
    pub cfg: TcConfig,
}

impl TcGen<'_, '_> {
    /// In-game biome from the exact chunk data (structure checks only: costs chunk generation).
    fn biome(&mut self, x: i32, z: i32) -> Option<u16> {
        self.t.column(x, z).map(|c| c.1)
    }

    /// Fast biome (provider, no border dithering) for the per-chunk vegetation lookups.
    fn biome_fast(&mut self, x: i32, z: i32) -> u16 {
        let p = self.t.p;
        match p.display(x, z, self.t.h) {
            Some(r) => p.table.biomes[r as usize].gen_id,
            None => p.table.lava_cave_marker_gen_id.unwrap_or(0),
        }
    }

    /// Surface "heightValue" on the model: first air above the top block (water surface 63 over oceans).
    fn height_value(&mut self, x: i32, z: i32) -> Option<i32> {
        self.t.top(x, z).map(|t| t.max(62) + 1)
    }

    /// `LocationIsValidSpawn` on the model: the point's surface must lie in [j-1, j+1], be land, with an allowed
    /// top block (`sand_ok`: rings accept sand/gravel; mounds and hilltop stones need stone/grass/dirt).
    fn spot_ok(&mut self, x: i32, z: i32, j: i32, sand_ok: bool) -> Option<bool> {
        let top = self.t.top(x, z)?;
        if top < 62 || top < j - 1 || top > j + 1 {
            return Some(false);
        }
        let b = self.biome(x, z)?;
        let d = self.climate.dict(b);
        let bad_top = has(d, "MESA") || (!sand_ok && (has(d, "SANDY") || has(d, "BEACH")));
        Some(!bad_top)
    }

    /// Trees put the heightmap on the canopy; with `swamp`, marsh ponds/vines count too (hilltop stones and mounds
    /// need stone/grass/dirt at every point; rings were seen succeeding in marsh).
    fn vegetated(&mut self, x: i32, z: i32, swamp: bool) -> bool {
        let Some(b) = self.biome(x, z) else {
            return false;
        };
        let d = self.climate.dict(b);
        has(d, "FOREST") || has(d, "JUNGLE") || has(d, "DENSE") || (swamp && has(d, "SWAMP"))
    }

    fn ring_prob_at(&mut self, x: i32, z: i32) -> f32 {
        let Some(b) = self.biome(x, z) else {
            return 0.0;
        };
        ring_prob(self.climate.dict(b))
    }

    /// Replays one chunk. Returns the features rolled in it (valid or not).
    pub fn chunk(&mut self, seed: i64, cx: i32, cz: i32, out: &mut Vec<TcFeature>) -> Option<()> {
        self.replay(seed, cx, cz, out, None)
    }

    /// Every aura node the chunk's Thaumcraft pass creates (wild, structure, totem and silverwood nodes), with
    /// type/modifier/aspects where the draws before them are modelled. Structure nodes are listed when the
    /// structure is model-valid (rings: before [`resolve_ring_mazes`]).
    pub fn nodes(&mut self, seed: i64, cx: i32, cz: i32, out: &mut Vec<TcNode>) -> Option<()> {
        // no node can roll here: skip the replay (it would generate this chunk's terrain for the biome lookups)
        if !self.node_roll_possible(seed, cx, cz, false) {
            return Some(());
        }
        let mut f = Vec::new();
        self.replay(seed, cx, cz, &mut f, Some(out))?;
        Some(())
    }

    /// [`TcGen::nodes`] for every chunk of `[cx0, cx1] × [cz0, cz1]` (z-major), same output. Computes once which
    /// biomes the rect's chunk biome arrays can hold ([`ChunkGen::possible_chunk_biomes`]) to drop impossible
    /// vegetation branches from the no-terrain pre-check, so far fewer chunks get generated.
    pub fn nodes_in_rect(
        &mut self,
        seed: i64,
        cx0: i32,
        cz0: i32,
        cx1: i32,
        cz1: i32,
        out: &mut Vec<TcNode>,
    ) {
        let grid =
            crate::chunkgen::ChunkGen::sample_window_grid(self.t.p, self.t.h, cx0, cz0, cx1, cz1);
        // per in-game id: (silverwood grows, greatwood support, hot sandy), cached
        let mut memo: crate::jmath::FxMap<u16, (bool, f32, bool)> = Default::default();
        for cz in cz0..=cz1 {
            for cx in cx0..=cx1 {
                let mut vb = VegBounds::NONE;
                grid.chunk_ids(cx, cz, |b| {
                    let &mut (g, s, sandy) = memo
                        .entry(b)
                        .or_insert_with(|| VegBounds::biome_rules(b, self.climate));
                    vb.add(g, s, sandy);
                });
                if !self.node_roll_possible_in(seed, cx, cz, false, &vb) {
                    continue;
                }
                let mut f = Vec::new();
                let _ = self.replay(seed, cx, cz, &mut f, Some(out));
            }
        }
    }

    /// [`TcGen::chunk`] for every chunk of `[cx0, cx1] × [cz0, cz1]` (z-major), without wild nodes and with the same
    /// structures/totems (only rings with [`TcConfig::rings_only`]): chunks where no vegetation branch can roll a mound, ring, hilltop or totem are skipped
    /// (their replay would only cost terrain for the biome lookups).
    pub fn structures_in_rect(
        &mut self,
        seed: i64,
        cx0: i32,
        cz0: i32,
        cx1: i32,
        cz1: i32,
        out: &mut Vec<TcFeature>,
    ) {
        for cz in cz0..=cz1 {
            for cx in cx0..=cx1 {
                if !self.structure_roll_possible(seed, cx, cz) {
                    continue;
                }
                let n = out.len();
                let _ = self.chunk(seed, cx, cz, out);
                let mut i = n;
                while i < out.len() {
                    if out[i].kind == TcKind::WildNode {
                        out.remove(i);
                    } else {
                        i += 1;
                    }
                }
            }
        }
    }

    /// Can any vegetation branch of the chunk roll a mound, ring, hilltop or totem? (One pass over
    /// [`TcGen::veg_states`]; same rules as [`TcGen::feature_roll_possible`].)
    pub fn structure_roll_possible(&self, seed: i64, cx: i32, cz: i32) -> bool {
        if slow_paths() || self.cfg.trees != TreeRule::Model || self.cfg.predict_nodes {
            return true;
        }
        if !self.cfg.gen_structure {
            return false;
        }
        for mut r in self.veg_states(seed, cx, cz) {
            if self.cfg.gen_aura && r.next_int_bounded(self.cfg.node_rarity) == 0 {
                continue; // the replay stops after an unpredicted wild node
            }
            r.next_int_bounded(16);
            r.next_int_bounded(16);
            // mound / ring (a ring's size draws come after; any ring counts) / hilltop, then the totem roll
            if self.cfg.rings_only {
                if r.next_int_bounded(150) != 0 && r.next_int_bounded(66) == 0 {
                    return true;
                }
                continue;
            }
            if r.next_int_bounded(150) == 0 || r.next_int_bounded(66) == 0 {
                return true;
            }
            if r.next_int_bounded(40) == 0 || r.next_int_bounded(self.cfg.node_rarity * 10) == 0 {
                return true;
            }
        }
        false
    }

    /// Cheap upper bound for [`TcGen::chunk`]: can the chunk roll a feature of `kind`? Same branching as
    /// [`TcGen::node_roll_possible`]; a wild node ends a [`TcGen::chunk`] replay unless `predict_nodes`.
    pub fn feature_roll_possible(&self, seed: i64, cx: i32, cz: i32, kind: TcKind) -> bool {
        if slow_paths() || self.cfg.trees != TreeRule::Model || self.cfg.predict_nodes {
            return true;
        }
        for mut r in self.veg_states(seed, cx, cz) {
            if self.cfg.gen_aura && r.next_int_bounded(self.cfg.node_rarity) == 0 {
                if kind == TcKind::WildNode {
                    return true;
                }
                continue; // the replay stops after an unpredicted wild node
            }
            if self.cfg.gen_structure {
                r.next_int_bounded(16);
                r.next_int_bounded(16);
                if r.next_int_bounded(150) == 0 {
                    if kind == TcKind::Mound {
                        return true;
                    }
                } else if r.next_int_bounded(66) == 0 {
                    if kind == TcKind::Ring {
                        return true;
                    }
                    r.next_int_bounded(6);
                    r.next_int_bounded(6);
                } else if r.next_int_bounded(40) == 0 && kind == TcKind::Hilltop {
                    return true;
                }
                if kind == TcKind::Totem && r.next_int_bounded(self.cfg.node_rarity * 10) == 0 {
                    return true;
                }
            }
        }
        false
    }

    /// Every RNG state the vegetation pass can leave (biome-dependent branches enumerated), before the wild-node
    /// roll; branches that stop the replay (spider greatwood under [`TreeRule::Model`]) are dropped. A rolled
    /// silverwood yields both "no draw" and "one draw" states.
    fn veg_states(&self, seed: i64, cx: i32, cz: i32) -> Vec<JavaRandom> {
        self.veg_states_in(seed, cx, cz, &VegBounds::ANY)
    }

    /// [`TcGen::veg_states`] with the branches limited to what the chunk's possible biomes allow.
    fn veg_states_in(&self, seed: i64, cx: i32, cz: i32, vb: &VegBounds) -> Vec<JavaRandom> {
        let mut states = vec![forge_chunk_random(seed, cx, cz)];
        if !self.cfg.gen_trees {
            return states;
        }
        let mut next = Vec::with_capacity(8);
        for mut r in states.drain(..) {
            if r.next_int_bounded(60) == 3 {
                r.next_int_bounded(16);
                r.next_int_bounded(16);
                if vb.silverwood_grows {
                    let mut a = r.clone();
                    a.next_int_bounded(4);
                    next.push(a);
                }
                if vb.silverwood_absent {
                    next.push(r);
                }
            } else {
                next.push(r);
            }
        }
        std::mem::swap(&mut states, &mut next);
        for mut r in states.drain(..) {
            if r.next_int_bounded(25) == 7 {
                r.next_int_bounded(16);
                r.next_int_bounded(16);
                let f = r.next_float();
                if vb.greatwood_max > f {
                    let mut a = r.clone();
                    let spiders = a.next_int_bounded(8) == 0;
                    a.next_long();
                    if !spiders {
                        next.push(a);
                    }
                }
                if vb.greatwood_min <= f {
                    next.push(r);
                }
            } else {
                next.push(r);
            }
        }
        std::mem::swap(&mut states, &mut next);
        for mut r in states.drain(..) {
            r.next_int_bounded(16);
            r.next_int_bounded(16);
            if vb.flowers_draw {
                let mut a = r.clone();
                if a.next_int_bounded(30) == 0 {
                    for _ in 0..108 {
                        a.next_int_bounded(8);
                    }
                }
                next.push(a);
            }
            if vb.flowers_skip {
                next.push(r);
            }
        }
        next
    }

    /// Cheap upper bound, no terrain: can chunk `(cx, cz)`'s replay produce a node? With `hittable_only`, only
    /// nodes with predicted contents count (wild, obelisk, totem; silverwood nodes are never certain under
    /// [`TreeRule::Model`], mound/hilltop contents are unknown). Branches over every biome-dependent decision
    /// before the rolls (silverwood fit, greatwood support, hot-desert flowers): at most 8 RNG states. Never
    /// false when [`TcGen::nodes`] would list such a node.
    pub fn node_roll_possible(&self, seed: i64, cx: i32, cz: i32, hittable_only: bool) -> bool {
        self.node_roll_possible_in(seed, cx, cz, hittable_only, &VegBounds::ANY)
    }

    /// [`TcGen::node_roll_possible`] given what the chunk's biomes can be ([`VegBounds`]).
    pub fn node_roll_possible_in(
        &self,
        seed: i64,
        cx: i32,
        cz: i32,
        hittable_only: bool,
        vb: &VegBounds,
    ) -> bool {
        if slow_paths() || self.cfg.trees != TreeRule::Model {
            return true;
        }
        // a rolled silverwood that can grow lists (possible) trunk nodes
        if !hittable_only
            && self.cfg.gen_trees
            && vb.silverwood_grows
            && forge_chunk_random(seed, cx, cz).next_int_bounded(60) == 3
        {
            return true;
        }
        let states = self.veg_states_in(seed, cx, cz, vb);
        for mut r in states {
            if self.cfg.gen_aura && r.next_int_bounded(self.cfg.node_rarity) == 0 {
                return true;
            }
            if self.cfg.gen_structure {
                r.next_int_bounded(16);
                r.next_int_bounded(16);
                if r.next_int_bounded(150) == 0 {
                    if !hittable_only {
                        return true;
                    }
                } else if r.next_int_bounded(66) == 0
                    || (r.next_int_bounded(40) == 0 && !hittable_only)
                {
                    return true; // obelisk; hilltop stones (contents unknown) only when any node counts
                }
                // totem (only when no structure set auraGen; assume it may roll)
                if r.next_int_bounded(self.cfg.node_rarity * 10) == 0 {
                    return true;
                }
            }
        }
        false
    }

    fn node_info(
        &mut self,
        r: &mut JavaRandom,
        src: NodeSource,
        x: i32,
        y: i32,
        z: i32,
        seed: i64,
    ) -> Option<(NodeInfo, u16)> {
        let b = self.biome(x, z)?;
        let (env, certain) = self.census(seed, x, y, z)?;
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
        Some((n, b))
    }

    #[allow(clippy::too_many_arguments)]
    fn push_node(
        nodes: &mut Option<&mut Vec<TcNode>>,
        kind: TcKind,
        (x, y, z): (i32, i32, i32),
        y_exact: bool,
        valid: bool,
        chunk: (i32, i32),
        info: Option<(NodeInfo, u16)>,
        rng: Option<JavaRandom>,
        source: NodeSource,
    ) {
        if let Some(v) = nodes.as_mut() {
            let biome = info.map_or(0, |i| i.1);
            v.push(TcNode {
                kind,
                x,
                y,
                z,
                y_exact,
                valid,
                chunk,
                info: info.map(|i| i.0),
                rng,
                source,
                biome,
            });
        }
    }

    /// Replays the chunk; everything after a silverwood whose fit the model cannot decide is replayed on the
    /// "does not fit" branch (15 of 20 real rolls, rwg2dump 2.3/2.8) and flagged conditional: features `exact =
    /// false`, nodes `valid = false` and contents not exact.
    fn replay(
        &mut self,
        seed: i64,
        cx: i32,
        cz: i32,
        out: &mut Vec<TcFeature>,
        mut nodes: Option<&mut Vec<TcNode>>,
    ) -> Option<()> {
        let mut cond = None;
        let res = self.replay_inner(seed, cx, cz, out, nodes.as_deref_mut(), &mut cond);
        if let Some((fi, ni)) = cond {
            for f in &mut out[fi..] {
                f.exact = false;
            }
            if let Some(v) = nodes {
                for n in &mut v[ni..] {
                    n.valid = false;
                    if let Some(i) = n.info.as_mut() {
                        i.exact = false;
                    }
                }
            }
        }
        res
    }

    fn replay_inner(
        &mut self,
        seed: i64,
        cx: i32,
        cz: i32,
        out: &mut Vec<TcFeature>,
        mut nodes: Option<&mut Vec<TcNode>>,
        cond: &mut Option<(usize, usize)>,
    ) -> Option<()> {
        let want_nodes = nodes.is_some();
        let mut r: JavaRandom = forge_chunk_random(seed, cx, cz);
        let (bx, bz) = (cx * 16, cz * 16);
        let mut exact = true;
        // 1. vegetation
        if self.cfg.gen_trees {
            if r.next_int_bounded(60) == 3 {
                let x = bx + r.next_int_bounded(16);
                let z = bz + r.next_int_bounded(16);
                // world.getBiomeGenForCoords = chunk biome array (exact data only when nodes are wanted)
                let b = if want_nodes {
                    self.biome(x, z)?
                } else {
                    self.biome_fast(x, z)
                };
                let d = self.climate.dict(b);
                let grows = !(b == 192 || b == 193) && (has(d, "MAGICAL") || b == 18 || b == 28);
                if grows {
                    let fits = match self.cfg.trees {
                        TreeRule::Grow => Some(true),
                        TreeRule::Fail => Some(false),
                        TreeRule::Model => self.silverwood_fits(x, z)?,
                    };
                    match fits {
                        Some(false) => {
                            r.next_int_bounded(4); // the height draw, then the space/soil check fails
                        }
                        Some(true) if want_nodes || self.cfg.predict_nodes => {
                            exact =
                                self.silverwood(seed, &mut r, x, z, (cx, cz), &mut nodes, true)?;
                        }
                        Some(true) => exact = false, // fits, but the feature replay runs no node draws
                        None => {
                            // fit decided by unmodelled vegetation (BOP Sacred Springs shrubs): list the tree's
                            // nodes as possible, continue on the likelier "does not fit" branch, conditionally
                            if want_nodes {
                                let mut rr = r.clone();
                                self.silverwood(seed, &mut rr, x, z, (cx, cz), &mut nodes, false)?;
                            }
                            r.next_int_bounded(4);
                            *cond = Some((out.len(), nodes.as_ref().map_or(0, |v| v.len())));
                        }
                    }
                }
            }
            if exact && r.next_int_bounded(25) == 7 {
                let x = bx + r.next_int_bounded(16);
                let z = bz + r.next_int_bounded(16);
                // world.getBiomeGenForCoords = the chunk biome array (border dithering): exact when nodes are wanted
                let b = if want_nodes {
                    self.biome(x, z)?
                } else {
                    self.biome_fast(x, z)
                };
                let support = greatwood_support(self.climate.dict(b));
                if support > r.next_float() {
                    // WorldGenGreatwoodTrees.func_76484_a: spiders = nextInt(8) == 0, generate: nextLong() seeds its
                    // own Random; only a fitting spider tree draws again (50 cobweb tries × 3)
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
                            TreeRule::Model => exact = false,
                        }
                    }
                }
            }
            if exact {
                let x = bx + r.next_int_bounded(16);
                let z = bz + r.next_int_bounded(16);
                let b = if want_nodes {
                    self.biome(x, z)?
                } else {
                    self.biome_fast(x, z)
                };
                let d = self.climate.dict(b);
                let hot = self
                    .climate
                    .get(b)
                    .map(|c| c.temperature > 1.0)
                    .unwrap_or(false);
                // biome.topBlock == sand (approximated by the SANDY dictionary type)
                if hot && has(d, "SANDY") && r.next_int_bounded(30) == 0 {
                    for _ in 0..108 {
                        r.next_int_bounded(8);
                    }
                }
            }
        }
        // 3. wild aura node (the scattered-feature bonus node before it needs a desert/jungle temple or witch hut
        //    start: none are generated in RWG2 worlds, not modelled)
        let mut aura = false;
        if exact && self.cfg.gen_aura && r.next_int_bounded(self.cfg.node_rarity) == 0 {
            let x = bx + r.next_int_bounded(16);
            let z = bz + r.next_int_bounded(16);
            let p = r.next_int_bounded(4);
            let mut f = TcFeature {
                kind: TcKind::WildNode,
                x,
                z,
                y: i32::MIN,
                valid: true,
                p_valid: 1.0,
                exact,
                size: (0, 0),
                chunk: (cx, cz),
                node: None,
                node_y: i32::MIN,
            };
            exact = false;
            if want_nodes || self.cfg.predict_nodes {
                let (y, y_exact) = self.wild_node_y(seed, x, z, p)?;
                let before = r.clone();
                let (n, b) = self.node_info(&mut r, NodeSource::Wild, x, y, z, seed)?;
                f.y = y;
                f.node_y = y;
                f.node = Some(n);
                Self::push_node(
                    &mut nodes,
                    TcKind::WildNode,
                    (x, y, z),
                    y_exact,
                    true,
                    (cx, cz),
                    Some((n, b)),
                    Some(before),
                    NodeSource::Wild,
                );
                // the census decides how many spread draws follow: later draws stay exact only if it is certain
                exact = n.env_certain;
            }
            out.push(f);
            aura = true;
        }
        if !exact {
            return Some(());
        }
        // 4. structures
        if self.cfg.gen_structure {
            let x = bx + r.next_int_bounded(16);
            let z = bz + r.next_int_bounded(16);
            // heightValue only matters when a structure is rolled (computing it costs chunk generation)
            if r.next_int_bounded(150) == 0 {
                if self.cfg.rings_only {
                    return Some(());
                }
                let hv = self.height_value(x, z)?;
                // WorldGenMound at (x, hv - 9, z): checks at hv over an 18x18 footprint
                let mut ok = true;
                for (dx, dz) in [(9, 9), (0, 0), (18, 0), (18, 18), (0, 18)] {
                    ok &= self.spot_ok(x + dx, z + dz, hv, false)?;
                }
                let valid = ok && !self.vegetated(x, z, true);
                // node at (x + 9, hv - 1, z + 9) after the crypt's chest loot (world-dependent draws)
                let ny = if valid { hv - 1 } else { i32::MIN };
                out.push(TcFeature {
                    kind: TcKind::Mound,
                    x: x + 9,
                    z: z + 9,
                    y: hv - 1,
                    valid,
                    p_valid: valid as u8 as f32,
                    exact,
                    size: (0, 0),
                    chunk: (cx, cz),
                    node: None,
                    node_y: ny,
                });
                if valid {
                    Self::push_node(
                        &mut nodes,
                        TcKind::Mound,
                        (x + 9, hv - 1, z + 9),
                        false,
                        true,
                        (cx, cz),
                        None,
                        None,
                        NodeSource::Eerie,
                    );
                }
                aura |= valid;
            } else if r.next_int_bounded(66) == 0 {
                let j = self.height_value(x, z)? - 1;
                let w = 11 + r.next_int_bounded(6) * 2;
                let h = 11 + r.next_int_bounded(6) * 2;
                let mut ok = true;
                for (dx, dz) in [(-3, -3), (0, 0), (3, 0), (3, 3), (0, 3)] {
                    ok &= self.spot_ok(x + dx, z + dz, j, true)?;
                }
                // terrain shape exact; whether populated vegetation (trees, flowers, shrubs on the 5 points or a
                // canopy raising heightValue) lets it build is a calibrated probability by the centre biome
                // ([`ring_prob`]); maze conflicts are applied later ([`resolve_ring_mazes_spawn`])
                let pr = if ok { self.ring_prob_at(x, z) } else { 0.0 };
                let valid = pr >= RING_MIN_P;
                let mut f = TcFeature {
                    kind: TcKind::Ring,
                    x,
                    z,
                    y: j,
                    valid,
                    p_valid: pr,
                    exact,
                    size: (w, h),
                    chunk: (cx, cz),
                    node: None,
                    node_y: i32::MIN,
                };
                if valid {
                    // WorldGenEldritchRing: 45 columns × 9 blocks (j − 4 ..= j + 4) each draw nextInt(4) when the block
                    // is at/below j or air/replaceable/foliage above it, plus nextInt(10) for the altar; solid terrain
                    // above j draws nothing (✅ rwg2dump 2.8: the real 92900795 ring = 383 draws = 406 − 23 terrain
                    // blocks above j). Logs would also skip (not modelled). Node between altar (j + 1) and obelisk.
                    let mut above = 0;
                    for dx in -3..=3i32 {
                        for dz in -3..=3i32 {
                            if dx.abs() == 3 && dz.abs() == 3 {
                                continue;
                            }
                            above += (self.t.top(x + dx, z + dz)? - j).clamp(0, 4);
                        }
                    }
                    let mut rr = r.clone();
                    for _ in 0..45 * 9 - above {
                        rr.next_int_bounded(4);
                    }
                    rr.next_int_bounded(10);
                    let before = rr.clone();
                    let (mut n, b) =
                        self.node_info(&mut rr, NodeSource::Eerie, x, j + 2, z, seed)?;
                    n.exact = false; // open-ground assumption for the ring's draws
                    f.node = Some(n);
                    f.node_y = j + 2;
                    Self::push_node(
                        &mut nodes,
                        TcKind::Ring,
                        (x, j + 2, z),
                        true,
                        true,
                        (cx, cz),
                        Some((n, b)),
                        Some(before),
                        NodeSource::Eerie,
                    );
                }
                out.push(f);
                aura |= valid;
            } else if self.cfg.rings_only {
                return Some(());
            } else if r.next_int_bounded(40) == 0 {
                let j = self.height_value(x, z)?;
                let mut ok = j >= 85;
                if ok {
                    for (dx, dz) in [(-2, -2), (0, 0), (2, 0), (2, 2), (0, 2)] {
                        ok &= self.spot_ok(x + dx, z + dz, j, false)?;
                    }
                }
                let valid = ok && !self.vegetated(x, z, true);
                // node at (x, j + 4, z) above the stones, after their chest loot (world-dependent draws)
                let ny = if valid { j + 4 } else { i32::MIN };
                out.push(TcFeature {
                    kind: TcKind::Hilltop,
                    x,
                    z,
                    y: j - 1,
                    valid,
                    p_valid: valid as u8 as f32,
                    exact,
                    size: (0, 0),
                    chunk: (cx, cz),
                    node: None,
                    node_y: ny,
                });
                if valid {
                    Self::push_node(
                        &mut nodes,
                        TcKind::Hilltop,
                        (x, j + 4, z),
                        false,
                        true,
                        (cx, cz),
                        None,
                        None,
                        NodeSource::Eerie,
                    );
                }
                aura |= valid;
            }
            if self.cfg.rings_only {
                return Some(());
            }
            // 5. totem (a successful structure consumed draws while building and set auraGen)
            if !aura && r.next_int_bounded(self.cfg.node_rarity * 10) == 0 {
                let x = bx + r.next_int_bounded(16);
                let z = bz + r.next_int_bounded(16);
                let top = self.t.top(x, z)?;
                let b = self.biome(x, z)?;
                let d = self.climate.dict(b);
                let valid = top >= 62 && !has(d, "MESA");
                let mut f = TcFeature {
                    kind: TcKind::Totem,
                    x,
                    z,
                    y: top,
                    valid,
                    p_valid: valid as u8 as f32,
                    exact,
                    size: (0, 0),
                    chunk: (cx, cz),
                    node: None,
                    node_y: i32::MIN,
                };
                if valid {
                    // pillar of up to 4 blocks over the top block; from the 2nd block on each has a 1 in 4 chance
                    // to hold the node (draw nextInt(4)), else the node caps the 5th block (open sky assumed)
                    let mut c = 2;
                    while c < 5 && r.next_int_bounded(4) != 0 {
                        c += 1;
                    }
                    let ny = top + c;
                    let before = r.clone();
                    let (n, b) = self.node_info(&mut r, NodeSource::Eerie, x, ny, z, seed)?;
                    f.node = Some(n);
                    f.node_y = ny;
                    Self::push_node(
                        &mut nodes,
                        TcKind::Totem,
                        (x, ny, z),
                        false,
                        true,
                        (cx, cz),
                        Some((n, b)),
                        Some(before),
                        NodeSource::Eerie,
                    );
                }
                out.push(f);
            }
        }
        Some(())
    }

    /// Does `WorldGenSilverwoodTrees` fit at (x, z)? `Some(verdict)` or `None` when estimated vegetation decides.
    /// y = `getHeightValue` (opaque blocks: leaves and logs count); the block below must sustain a sapling
    /// (grass/dirt) and the trunk column plus a 7×7 crown (3 top layers) must be air/leaves/replaceable.
    fn silverwood_fits(&mut self, x: i32, z: i32) -> Option<Option<bool>> {
        let top = self.t.top(x, z)?;
        if top < 62 {
            return Some(Some(false)); // under water: water is not soil
        }
        let b = self.biome(x, z)?;
        let d = self.climate.dict(b);
        if has(d, "SANDY") || has(d, "BEACH") || has(d, "MESA") || has(d, "SNOWY") {
            return Some(Some(false));
        }
        Some(None)
    }

    /// `WorldGenSilverwoodTrees.generate` on a fitting spot: replays every draw (crown radius, end layer, trunk
    /// nodes with their `createRandomNodeAt`, 8 log draws, 18 flower tries). Returns whether the draws after it
    /// stay exact (each trunk node's census must be clear). `fits_known` = false: the tree may not exist.
    #[allow(clippy::too_many_arguments)]
    fn silverwood(
        &mut self,
        seed: i64,
        r: &mut JavaRandom,
        x: i32,
        z: i32,
        chunk: (i32, i32),
        nodes: &mut Option<&mut Vec<TcNode>>,
        fits_known: bool,
    ) -> Option<bool> {
        let y = self.height_value(x, z)?; // world.getHeightValue on bare ground (it fits: no canopy above)
        let height = r.next_int_bounded(4) + 7;
        if !(y >= 1 && y + height + 1 < 256) {
            return Some(true);
        }
        let end = y + height + 3 + r.next_int_bounded(3);
        let start = y + height - 5;
        for _ in 0..(end - start + 1) * 121 {
            r.next_int_bounded(8); // crown radius draw, taken for every position
        }
        let mut chance = (height as f64 * 1.5) as i32;
        let mut last = false;
        let mut certain = true;
        for k in 0..height {
            if k > 0 && !last && r.next_int_bounded(chance) == 0 {
                let before = r.clone();
                let mut info = self.node_info(r, NodeSource::Silverwood, x, y + k, z, seed)?;
                certain &= info.0.env_certain;
                info.0.exact &= fits_known;
                Self::push_node(
                    nodes,
                    TcKind::Silverwood,
                    (x, y + k, z),
                    fits_known,
                    fits_known,
                    chunk,
                    Some(info),
                    Some(before),
                    NodeSource::Silverwood,
                );
                chance += height;
                last = true;
            } else {
                last = false;
            }
        }
        for _ in 0..8 {
            r.next_int_bounded(3);
        }
        // WorldGenCustomFlowers: 18 tries × (x, y, z) = 6 draws each, no early exit
        for _ in 0..18 {
            for bound in [8, 8, 4, 4, 8, 8] {
                r.next_int_bounded(bound);
            }
        }
        Some(certain)
    }

    /// Cave mask of a chunk (thread-local cache per seed).
    fn carved(&mut self, seed: i64, x: i32, y: i32, z: i32) -> Option<bool> {
        let (cx, cz) = (x >> 4, z >> 4);
        let fast = self.cfg.fast_cave_random;
        let hit = CAVES.with(|c| {
            let c = c.borrow();
            match c.as_ref() {
                Some((s, f, _, m)) if *s == seed && *f == fast => {
                    m.get(&(cx, cz)).map(|m| m.carved(x & 15, y, z & 15))
                }
                _ => None,
            }
        });
        if let Some(h) = hit {
            return Some(h);
        }
        let mut h = [0i32; 256];
        {
            let ch = self.t.chunk(cx, cz)?;
            for (o, v) in h.iter_mut().zip(ch.height.iter()) {
                *o = *v as i32;
            }
        }
        CAVES.with(|c| {
            let mut c = c.borrow_mut();
            if !matches!(c.as_ref(), Some((s, f, _, _)) if *s == seed && *f == fast) {
                *c = Some((seed, fast, Caves::new(seed, fast), Default::default()));
            }
            let (_, _, caves, masks) = c.as_mut().unwrap();
            if masks.len() > 512 {
                masks.clear();
            }
            let m = caves.carve(cx, cz, &h);
            let v = m.carved(x & 15, y, z & 15);
            masks.insert((cx, cz), m);
            Some(v)
        })
    }

    /// Inside a mineshaft piece (Room/Corridor/Cross/Stairs box; `mineshaft.rs`, exact layout and Y).
    fn mine_air(&mut self, seed: i64, x: i32, y: i32, z: i32) -> bool {
        use crate::mineshaft::{air_boxes, is_start, mineshaft, Seeds};
        let (cx, cz) = (x >> 4, z >> 4);
        let cached = MINES.with(|m| {
            let m = m.borrow();
            match m.as_ref() {
                Some((s, _, map)) if *s == seed => map
                    .get(&(cx, cz))
                    .map(|v| v.iter().any(|b| inside(b, x, y, z))),
                _ => None,
            }
        });
        if let Some(v) = cached {
            return v;
        }
        let seeds = MINES.with(|m| {
            let mut m = m.borrow_mut();
            if !matches!(m.as_ref(), Some((s, _, _)) if *s == seed) {
                *m = Some((seed, Seeds::new(seed), Default::default()));
            }
            m.as_ref().unwrap().1
        });
        let (x0, z0) = (cx * 16, cz * 16);
        let mut boxes = Vec::new();
        for scz in cz - 8..=cz + 8 {
            for scx in cx - 8..=cx + 8 {
                if !is_start(&seeds, scx, scz) {
                    continue;
                }
                let gid = self.biome_fast(scx * 16 + 8, scz * 16 + 8);
                let mesa = has(self.climate.dict(gid), "MESA");
                if let Some(ms) = mineshaft(&seeds, scx, scz, mesa) {
                    boxes.extend(
                        air_boxes(&ms).filter(|b| {
                            b[3] >= x0 && b[0] < x0 + 16 && b[5] >= z0 && b[2] < z0 + 16
                        }),
                    );
                }
            }
        }
        let v = boxes.iter().any(|b| inside(b, x, y, z));
        MINES.with(|m| {
            let mut m = m.borrow_mut();
            let map = &mut m.as_mut().unwrap().2;
            if map.len() > 4096 {
                map.clear();
            }
            map.insert((cx, cz), boxes);
        });
        v
    }

    /// Air on the model: above the ground and the sea, a mineshaft piece, or a cave block at y ≥ 11 (caves below
    /// are lava).
    fn air(&mut self, seed: i64, x: i32, y: i32, z: i32) -> Option<bool> {
        let top = self.t.top(x, z)?;
        if y <= top && y > 0 && self.mine_air(seed, x, y, z) {
            return Some(true);
        }
        if y > top.max(62) {
            return Some(true);
        }
        if y > top || y < 11 {
            return Some(false);
        }
        self.carved(seed, x, y, z)
    }

    /// `generateWildNodes` height: first air above y 5 (`Utils.getFirstUncoveredY` + 1), then `+ p` if that block
    /// is air too. Returns (y, exact): surface nodes move with tall grass / trees / snow, so only cave and sea nodes
    /// are exact (mineshaft pieces included; RWG2 has no ravines; lakes/dungeons not modelled).
    fn wild_node_y(&mut self, seed: i64, x: i32, z: i32, p: i32) -> Option<(i32, bool)> {
        let top = self.t.top(x, z)?;
        let mut q = 5;
        while q < 255 && !self.air(seed, x, q + 1, z)? {
            q += 1;
        }
        q += 1;
        if self.air(seed, x, q + p, z)? {
            q += p;
        }
        let surface = q > top;
        let b = self.biome(x, z)?;
        let d = self.climate.dict(b);
        let y_exact =
            !surface || top < 62 || (has(d, "SANDY") && !has(d, "FOREST") && !has(d, "JUNGLE"));
        Some((q, y_exact))
    }

    /// `createRandomNodeAt`'s 11×11×11 block census on the model. Returns the verdicts and whether they are clear:
    /// a count near its threshold, the Et Futurum deepslate layer (stone ≤ y 22, generator order not known) or the
    /// unmodelled surface material make it uncertain. Foliage is taken as 0 (see below).
    fn census(&mut self, seed: i64, x: i32, y: i32, z: i32) -> Option<(NodeEnv, bool)> {
        const SURFACE: i32 = 4;
        let (mut water, mut lava, mut stone, mut deep, mut surf) = (0, 0, 0, 0, 0);
        let mut near_surface = false;
        for dz in -5..=5 {
            for dx in -5..=5 {
                let (xx, zz) = (x + dx, z + dz);
                let top = self.t.top(xx, zz)?;
                if (y - top).abs() <= 6 {
                    near_surface = true;
                }
                for yy in y - 5..=y + 5 {
                    if !(0..256).contains(&yy) {
                        continue;
                    }
                    if yy > top {
                        water += (yy <= 62) as i32;
                        continue;
                    }
                    if yy > 0 && self.mine_air(seed, xx, yy, zz) {
                        continue;
                    }
                    if self.carved(seed, xx, yy, zz)? {
                        lava += (yy <= 10) as i32;
                        continue;
                    }
                    if yy <= 4 {
                        continue; // bedrock (partly)
                    }
                    if yy > top - SURFACE {
                        surf += 1;
                    } else if yy <= 22 {
                        deep += 1;
                    } else {
                        stone += 1;
                    }
                }
            }
        }
        let b = self.biome(x, z)?;
        let d = self.climate.dict(b);
        let leafy = [
            "FOREST",
            "JUNGLE",
            "DENSE",
            "LUSH",
            "MAGICAL",
            "SWAMP",
            "CONIFEROUS",
        ]
        .iter()
        .any(|t| has(d, t));
        let rocky = ["MOUNTAIN", "HILLS", "SNOWY"].iter().any(|t| has(d, t));
        let env = NodeEnv {
            water: water > 100,
            lava: lava > 100,
            stone: stone + deep > 500,
            foliage: false,
        };
        let mut certain = (water - 100).abs() > 25 && (lava - 100).abs() > 25;
        // deepslate band: the verdict must not depend on it
        certain &= (stone > 500) == (stone + deep > 500) || deep == 0;
        // stone count margin; surface blocks may be stone on mountains
        let lo = stone;
        let hi = stone + deep + if rocky { surf } else { 0 };
        certain &= (lo > 560) || (hi < 440);
        // foliage: no dumped node (7, all surface nodes in leafy biomes, silverwood trunks inside their canopy)
        // shows herba from the census, so leaves do not count as `isFoliage` here; kept as an assumption
        let _ = (near_surface, leafy);
        Some((env, certain))
    }
}

#[inline]
fn inside(b: &crate::mineshaft::BBox, x: i32, y: i32, z: i32) -> bool {
    x >= b[0] && x <= b[3] && y >= b[1] && y <= b[4] && z >= b[2] && z <= b[5]
}

/// Generation-order key: the initial spawn area (±12 chunks around the spawn chunk, loaded x-major by the server)
/// first, then outward from the spawn.
fn order_key(c: (i32, i32), s: (i32, i32)) -> (u8, i64, i32, i32) {
    let (dx, dz) = (c.0 - s.0, c.1 - s.1);
    if dx.abs() <= 12 && dz.abs() <= 12 {
        (0, 0, c.0, c.1)
    } else {
        (1, dx as i64 * dx as i64 + dz as i64 * dz as i64, c.0, c.1)
    }
}

/// Rings below this probability are not shown or predicted (no node).
pub const RING_MIN_P: f32 = 0.05;

/// P(an eldritch ring whose terrain shape passes `LocationIsValidSpawn` on the model actually builds), by the
/// chunk biome at its centre. What decides it is populated vegetation the model doesn't place: a canopy raising
/// `heightValue` at the centre, or flowers / shrubs / saplings on one of the 5 checked blocks (only tall grass and
/// snow layers over ground pass). Fitted on 9 dumped worlds (302 rolled rings, 7 real; rings not explained by a
/// real ring's maze), Laplace-smoothed:
/// - LUSH / MAGICAL / DENSE (Garden, Sacred Springs, rainforests, dense forests): 0 of 36 real → 0.014
/// - SWAMP / SAVANNA: 4 of 6 → 0.64
/// - anything else: 3 of 19 → 0.175
///
/// Leave-one-world-out: rings scored 0.5–0.8 were real 4/6, ~0.1–0.2 3/19, the first group 0/36.
pub fn ring_prob(dict: &[String]) -> f32 {
    if has(dict, "LUSH") || has(dict, "MAGICAL") || has(dict, "DENSE") {
        0.014
    } else if has(dict, "SWAMP") || has(dict, "SAVANNA") {
        0.64
    } else {
        0.175
    }
}

/// Likelihood ratios of [`ring_deco_ok`] (pass, fail) for a terrain-passing ring: real rings pass 4 / 7, non-real
/// rings that no real ring's maze explains 1 / 18 (9 dumped worlds, smoothed); 3…10 / 0.75…0.4 all score alike.
pub const RING_DECO_LR: (f32, f32) = (4.0, 0.6);

/// `WorldGenEldritchRing.LocationIsValidSpawn` at the 5 points on the decorated model: `j` = heightValue − 1 with
/// canopies; each point's surface (first air within 2 above j, minus 1) must be stone / sand / packed ice /
/// grass / gravel / dirt, or snow layer / tall grass on one, with air above. The pre-population top block is
/// taken as valid (the terrain check judged it).
pub fn ring_deco_ok(
    t: &mut Terrain,
    ov: &mut crate::deco::view::DecoOverlay,
    x: i32,
    z: i32,
) -> bool {
    use crate::deco::{db, mat, Mat};
    use crate::surface::blk;
    let (p, h) = (t.p, t.h);
    let class =
        |t: &mut Terrain, ov: &mut crate::deco::view::DecoOverlay, x: i32, y: i32, z: i32| -> u8 {
            if let Some(b) = ov.at(p, h, x, y, z) {
                return match db::id(b) {
                    blk::AIR => 0,
                    blk::STONE
                    | blk::DIRT
                    | blk::COARSE_DIRT
                    | blk::PODZOL
                    | blk::GRASS
                    | blk::SAND
                    | blk::RED_SAND
                    | blk::GRAVEL
                    | blk::PACKED_ICE => 1,
                    blk::SNOW_LAYER | db::TALLGRASS => 2,
                    _ if mat(b) == Mat::Water => 4,
                    _ => 3,
                };
            }
            let top = t.top(x, z).unwrap_or(0);
            if y <= top {
                1
            } else if y <= 62 {
                4
            } else {
                0
            }
        };
    let top = t.top(x, z).unwrap_or(0).max(62);
    let mut hv = top + 1;
    for y in top + 1..top + 48 {
        if let Some(b) = ov.at(p, h, x, y, z) {
            if !matches!(
                mat(b),
                Mat::Air | Mat::Plants | Mat::Vine | Mat::Snow | Mat::Cactus
            ) {
                hv = y + 1;
            }
        }
    }
    let j = hv - 1;
    [(-3, -3), (0, 0), (3, 0), (3, 3), (0, 3)]
        .iter()
        .all(|&(dx, dz)| {
            let (i, k) = (x + dx, z + dz);
            let mut d = 0;
            while class(t, ov, i, j + d, k) != 0 {
                d += 1;
                if d > 2 {
                    return false;
                }
            }
            let y = j + d - 1;
            if class(t, ov, i, y + 1, k) != 0 {
                return false;
            }
            match class(t, ov, i, y, k) {
                1 => true,
                2 => class(t, ov, i, y - 1, k) == 1,
                _ => false,
            }
        })
}

/// Bayes update of every terrain-passing ring's `p_valid` (≥ `min_p`) by [`ring_deco_ok`] ([`RING_DECO_LR`]); call before
/// [`resolve_ring_mazes_spawn`]. Returns the number of rings checked (each may cost a decoration replay of its
/// chunk group).
pub fn apply_ring_deco(
    features: &mut [TcFeature],
    t: &mut Terrain,
    ov: &mut crate::deco::view::DecoOverlay,
    min_p: f32,
) -> usize {
    let mut n = 0;
    for q in features
        .iter_mut()
        .filter(|q| q.kind == TcKind::Ring && q.p_valid > 0.0 && q.p_valid >= min_p)
    {
        let l = if ring_deco_ok(t, ov, q.x, q.z) {
            RING_DECO_LR.0
        } else {
            RING_DECO_LR.1
        };
        q.p_valid = q.p_valid * l / (q.p_valid * l + 1.0 - q.p_valid);
        q.valid = q.p_valid >= RING_MIN_P;
        n += 1;
    }
    n
}

/// `MazeHandler.mazesInRange`: an earlier ring's maze (its centre, 4 corner cells at ±w/±h, and the w×h grid starting
/// at (cx − (1 + w/2), cz − (1 + h/2))) must not have a cell within ±w', ±h' chunks of a later ring.
pub fn maze_conflict(a: &TcFeature, b: &TcFeature) -> bool {
    let (ax, az, aw, ah) = (a.chunk.0, a.chunk.1, a.size.0, a.size.1);
    let (bx, bz, bw, bh) = (b.chunk.0, b.chunk.1, b.size.0, b.size.1);
    let inside = |x: i32, z: i32| (x - bx).abs() <= bw && (z - bz).abs() <= bh;
    if [(0, 0), (-aw, -ah), (aw, ah), (-aw, ah), (aw, -ah)]
        .iter()
        .any(|&(dx, dz)| inside(ax + dx, az + dz))
    {
        return true;
    }
    // interior grid [col, col + w) x [row, row + h) vs the check box [bx − bw, bx + bw] x [bz − bh, bz + bh]
    let (c0, r0) = (ax - (1 + aw / 2), az - (1 + ah / 2));
    let (c1, r1) = (c0 + aw - 1, r0 + ah - 1);
    c0 <= bx + bw && c1 >= bx - bw && r0 <= bz + bh && r1 >= bz - bh
}

/// Applies maze conflicts in generation order, probabilistically: a ring builds with
/// `p_valid × Π (1 − p_final(earlier conflicting ring))`. Rings that fall below [`RING_MIN_P`] become invalid
/// (no altar, no node). Returns how many were dropped. `rings` should cover a margin around the area of interest (conflicts reach
/// ~42 chunks).
pub fn resolve_ring_mazes(features: &mut [TcFeature]) -> usize {
    resolve_ring_mazes_spawn(features, (0, 0))
}

/// [`resolve_ring_mazes`] with the world spawn (block x, z; e.g. `spawn::estimate_spawn`): the server generates
/// the ±12-chunk area around the spawn chunk first, then the world grows outward from there.
pub fn resolve_ring_mazes_spawn(features: &mut [TcFeature], spawn: (i32, i32)) -> usize {
    let s = (spawn.0 >> 4, spawn.1 >> 4);
    let mut idx: Vec<usize> = (0..features.len())
        .filter(|&i| features[i].kind == TcKind::Ring && features[i].valid)
        .collect();
    idx.sort_by_key(|&i| order_key(features[i].chunk, s));
    let mut done: Vec<usize> = Vec::new();
    let mut dropped = 0;
    for i in idx {
        let mut p = features[i].p_valid;
        for &k in &done {
            if maze_conflict(&features[k], &features[i]) {
                p *= 1.0 - features[k].p_valid;
            }
        }
        features[i].p_valid = p;
        if p < RING_MIN_P {
            features[i].valid = false;
            features[i].node = None; // generate() refused: no altar, no node
            features[i].node_y = i32::MIN;
            dropped += 1;
        }
        done.push(i);
    }
    dropped
}
