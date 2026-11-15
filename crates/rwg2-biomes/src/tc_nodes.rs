//! Thaumcraft 4.2.3.5 aura node contents: `ThaumcraftWorldGenerator.createRandomNodeAt` replayed on the caller's
//! Random (type, modifier, aspect list and amounts), plus the world inputs it reads (biome dictionary types, the
//! 11×11×11 block census around the node).
//!
//! Draw order (GTNH 2.9.0: `special_node_rarity` 18, Salis Arcana node weights disabled in `enhancements.cfg`):
//! 1. type: silverwood → PURE, eerie (structures/totems) → DARK, else `nextInt(18) == 0` → `nextInt(10)`
//!    (0–2 DARK, 3–5 UNSTABLE, 6–8 PURE, 9 HUNGRY)
//! 2. modifier: `nextInt(9) == 0` → `nextInt(3)` (BRIGHT, PALE, FADING)
//! 3. biome aura = mean of the biome's dictionary types' aura levels (any unregistered type → 100); taint biome
//!    (non-PURE): ×1.5 and `nextBoolean` → TAINTED ×1.5; silverwood: /4. `value = nextInt(aura/2) + aura/2`
//! 4. `getRandomBiomeTag`: `types[nextInt(len)]` → that type's aspect (MAGICAL has none) → add ×2; none →
//!    random complex ×1 + random primal ×1
//! 5. 3× `nextBoolean` → (`nextInt(18) == 0` ? random complex : random primal) merged ×1
//! 6. HUNGRY: fames ×2 (+ `nextBoolean` lucrum); PURE: `nextBoolean` ? victus : ordo ×2;
//!    DARK: 4× `nextBoolean` → mortuus, exanimis, perditio, tenebrae ×1
//! 7. block census in ±5 (no draws): water > 100 → aqua; lava > 100 → ignis + terra; stone > 500 → terra;
//!    foliage > 100 → herba
//! 8. per aspect in tag order: ×2 → `50 + nextInt(25)`, else `25 + nextInt(50)`; final amount =
//!    max(base, (int)(spread / total · value))
//!
//! "Random complex" indexes the pack's full list of compound aspects in registration order (Thaumcraft's 42, then
//! every addon's), so its size matters for every complex pick; primal picks are exact.

use crate::java_random::JavaRandom;

/// Aspects in registration order (`Aspect.aspects` is a `LinkedHashMap`): Thaumcraft's own 48 (6 primal + 42
/// compound) come first; addon aspects follow in mod-load order.
pub const TC_ASPECT_TAGS: [&str; 48] = [
    "aer",
    "terra",
    "ignis",
    "aqua",
    "ordo",
    "perditio",
    "vacuos",
    "lux",
    "tempestas",
    "motus",
    "gelum",
    "vitreus",
    "victus",
    "venenum",
    "potentia",
    "permutatio",
    "metallum",
    "mortuus",
    "volatus",
    "tenebrae",
    "spiritus",
    "sano",
    "iter",
    "alienis",
    "praecantatio",
    "auram",
    "vitium",
    "limus",
    "herba",
    "arbor",
    "bestia",
    "corpus",
    "exanimis",
    "cognitio",
    "sensus",
    "humanus",
    "messis",
    "perfodio",
    "instrumentum",
    "meto",
    "telum",
    "tutamen",
    "fames",
    "lucrum",
    "fabrico",
    "pannus",
    "machina",
    "vinculum",
];

/// One aspect: index into the pack's registration order (`< 48`: [`TC_ASPECT_TAGS`]; `>= 48`: an addon aspect,
/// named by [`TcAspectTable::tag`] when known).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct Aspect(pub u8);

impl Aspect {
    pub const AIR: Aspect = Aspect(0);
    pub const EARTH: Aspect = Aspect(1);
    pub const FIRE: Aspect = Aspect(2);
    pub const WATER: Aspect = Aspect(3);
    pub const ORDER: Aspect = Aspect(4);
    pub const ENTROPY: Aspect = Aspect(5);
    pub const VOID: Aspect = Aspect(6);
    pub const LIFE: Aspect = Aspect(12);
    pub const DEATH: Aspect = Aspect(17);
    pub const DARKNESS: Aspect = Aspect(19);
    pub const SOUL: Aspect = Aspect(20);
    pub const PLANT: Aspect = Aspect(28);
    pub const UNDEAD: Aspect = Aspect(32);
    pub const HUNGER: Aspect = Aspect(42);
    pub const GREED: Aspect = Aspect(43);

    pub fn is_primal(self) -> bool {
        self.0 < 6
    }

    /// Tag of a Thaumcraft aspect (`None` for addon aspects; see [`TcAspectTable::tag`]).
    pub fn tc_tag(self) -> Option<&'static str> {
        TC_ASPECT_TAGS.get(self.0 as usize).copied()
    }

    pub fn from_tag(tag: &str, table: &TcAspectTable) -> Option<Aspect> {
        if let Some(i) = TC_ASPECT_TAGS.iter().position(|&t| t == tag) {
            return Some(Aspect(i as u8));
        }
        table
            .addons
            .iter()
            .position(|&t| t == tag)
            .map(|i| Aspect((48 + i) as u8))
    }
}

/// The pack's aspect registry as far as node generation needs it.
#[derive(Clone, Copy, Debug)]
pub struct TcAspectTable {
    /// Number of compound aspects registered when the first node is created (Thaumcraft's 42 + addons). Decides
    /// which aspect every random complex pick lands on.
    pub complex_count: i32,
    /// Addon aspect tags in registration order (may be shorter than `complex_count - 42`; unknown entries are `""`;
    /// missing/unknown names print as `addon#k`).
    pub addons: &'static [&'static str],
}

impl TcAspectTable {
    pub fn tag(&self, a: Aspect) -> String {
        match a.tc_tag() {
            Some(t) => t.to_string(),
            None => match self.addons.get(a.0 as usize - 48) {
                Some(t) if !t.is_empty() => t.to_string(),
                _ => format!("addon#{}", a.0 as usize - 48),
            },
        }
    }

    /// The aspect's name is known (Thaumcraft's, or a fitted addon position).
    pub fn known(&self, a: Aspect) -> bool {
        a.tc_tag().is_some()
            || self
                .addons
                .get(a.0 as usize - 48)
                .is_some_and(|t| !t.is_empty())
    }

    /// Complex pick `i` (`nextInt(complex_count)`).
    fn complex(&self, i: i32) -> Aspect {
        Aspect((6 + i) as u8)
    }
}

/// GTNH 2.9.0: 63 compound aspects (Thaumcraft's 42 + 21 addon aspects). Fitted on the 7 nodes of rwg2dump 2.3
/// (seed 1337) and 2.7 (seed 92900795): over counts 42..=160 only 63 puts every complex pick on the observed
/// aspect (motus, fabrico, metallum, and Forbidden Magic's superbia at addon index 3). Forbidden Magic registers
/// luxuria, infernus, superbia, gula, invidia, desidia, ira in one call (`"No Lust"=false`), so addon indices 1..=7
/// are those. rwg2dump 2.8 (`tcnodes.json`, `Aspect.aspects` key order) confirmed all 21: terminus (Thaumic
/// Tinkerer), the 7 sins, tempus, GT's strontio/nebrisum/electrum/magneto/radio, custom1..5, caelum, tabernus.
pub const TC_ASPECTS_GTNH: TcAspectTable = TcAspectTable {
    complex_count: GTNH_COMPLEX_COUNT,
    addons: &GTNH_ADDONS,
};

pub const GTNH_COMPLEX_COUNT: i32 = 63;
/// Addon aspects in registration order (see [`TC_ASPECTS_GTNH`]).
pub const GTNH_ADDONS: [&str; 21] = [
    "terminus", "luxuria", "infernus", "superbia", "gula", "invidia", "desidia", "ira", "tempus",
    "strontio", "nebrisum", "electrum", "magneto", "radio", "custom1", "custom2", "custom3",
    "custom4", "custom5", "caelum", "tabernus",
];

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum NodeType {
    Normal,
    Unstable,
    Dark,
    Tainted,
    Hungry,
    Pure,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum NodeModifier {
    Bright,
    Pale,
    Fading,
}

/// Which `createRandomNodeAt` flags the caller passed.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum NodeSource {
    /// `generateWildNodes` (and the scattered-feature bonus node): normal type roll.
    Wild,
    /// Inside a silverwood trunk: PURE, aura / 4.
    Silverwood,
    /// Totems, eldritch rings, hilltop stones, barrow mounds: DARK.
    Eerie,
}

/// The block census `createRandomNodeAt` takes in the 11×11×11 cube around the node (thresholds already applied).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub struct NodeEnv {
    /// > 100 water blocks → aqua
    pub water: bool,
    /// > 100 lava blocks → ignis + terra
    pub lava: bool,
    /// > 500 `Blocks.stone` → terra
    pub stone: bool,
    /// > 100 `isFoliage` blocks → herba
    pub foliage: bool,
}

impl NodeEnv {
    pub fn all() -> impl Iterator<Item = NodeEnv> {
        (0..16u8).map(|b| NodeEnv {
            water: b & 1 != 0,
            lava: b & 2 != 0,
            stone: b & 4 != 0,
            foliage: b & 8 != 0,
        })
    }
}

pub const MAX_NODE_ASPECTS: usize = 16;

/// A predicted aura node.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct NodeInfo {
    pub node_type: NodeType,
    pub modifier: Option<NodeModifier>,
    aspects: [(Aspect, u16); MAX_NODE_ASPECTS],
    n: u8,
    /// Every input was known: the draws before the node, the biome, and the block census (see `env_certain`).
    pub exact: bool,
    /// The block census verdicts used.
    pub env: NodeEnv,
    /// The census verdicts were clear on the model (no threshold close, no unmodelled blocks that matter).
    pub env_certain: bool,
}

impl NodeInfo {
    /// Aspects with amounts, in Thaumcraft's insertion order (what the node's NBT / a wand shows).
    pub fn aspects(&self) -> &[(Aspect, u16)] {
        &self.aspects[..self.n as usize]
    }

    pub fn amount(&self, a: Aspect) -> u16 {
        self.aspects().iter().find(|x| x.0 == a).map_or(0, |x| x.1)
    }

    pub fn total(&self) -> u32 {
        self.aspects().iter().map(|x| x.1 as u32).sum()
    }

    /// `"aer:12 ordo:7 …"` (the rwg2dump format).
    pub fn aspects_string(&self, table: &TcAspectTable) -> String {
        self.aspects()
            .iter()
            .map(|(a, n)| format!("{}:{}", table.tag(*a), n))
            .collect::<Vec<_>>()
            .join(" ")
    }
}

/// Biome dictionary type → (aura level, aspect index or none), `Config.initMisc` registrations.
fn biome_info(t: &str) -> Option<(i32, Option<Aspect>)> {
    use Aspect as A;
    Some(match t {
        "WATER" => (100, Some(A::WATER)),
        "OCEAN" => (120, Some(A::WATER)),
        "RIVER" => (100, Some(A::WATER)),
        "WET" => (80, Some(A::WATER)),
        "HOT" => (100, Some(A::FIRE)),
        "DESERT" => (100, Some(A::FIRE)),
        "NETHER" => (120, Some(A::FIRE)),
        "MESA" => (80, Some(A::FIRE)),
        "DENSE" => (100, Some(A::ORDER)),
        "SNOWY" => (80, Some(A::ORDER)),
        "COLD" => (80, Some(A::ORDER)),
        "FROZEN" => (100, Some(A::ORDER)),
        "MUSHROOM" => (140, Some(A::ORDER)),
        "CONIFEROUS" => (100, Some(A::EARTH)),
        "FOREST" => (120, Some(A::EARTH)),
        "SANDY" => (80, Some(A::EARTH)),
        "BEACH" => (80, Some(A::EARTH)),
        "SAVANNA" => (80, Some(A::AIR)),
        "MOUNTAIN" => (100, Some(A::AIR)),
        "HILLS" => (120, Some(A::AIR)),
        "PLAINS" => (80, Some(A::AIR)),
        "DRY" => (80, Some(A::ENTROPY)),
        "SPARSE" => (80, Some(A::ENTROPY)),
        "SWAMP" => (120, Some(A::ENTROPY)),
        "WASTELAND" => (80, Some(A::ENTROPY)),
        "JUNGLE" => (100, Some(A::PLANT)),
        "LUSH" => (100, Some(A::PLANT)),
        "MAGICAL" => (100, None),
        "END" => (80, Some(A::VOID)),
        "SPOOKY" => (80, Some(A::SOUL)),
        "DEAD" => (50, Some(A::DEATH)),
        _ => return None,
    })
}

/// `BiomeHandler.getBiomeAura`.
pub fn biome_aura(dict: &[String]) -> i32 {
    let mut sum = 0;
    for t in dict {
        match biome_info(t) {
            Some((a, _)) => sum += a,
            None => return 100, // NPE inside the try
        }
    }
    if dict.is_empty() {
        100 // ArithmeticException (/ 0)
    } else {
        sum / dict.len() as i32
    }
}

/// `BiomeHandler.getRandomBiomeTag` (draws `nextInt(len)` unless the biome has no types).
fn random_biome_tag(r: &mut JavaRandom, dict: &[String]) -> Option<Aspect> {
    if dict.is_empty() {
        return None; // nextInt(0) throws before drawing
    }
    let t = &dict[r.next_int_bounded(dict.len() as i32) as usize];
    biome_info(t).and_then(|x| x.1)
}

/// Thaumcraft's taint biome id (`biome_taint` in GTNH's Thaumcraft.cfg).
pub const BIOME_TAINT: u16 = 193;

struct List {
    a: [(Aspect, u16); MAX_NODE_ASPECTS],
    n: usize,
}

impl List {
    fn pos(&self, x: Aspect) -> Option<usize> {
        self.a[..self.n].iter().position(|e| e.0 == x)
    }
    fn add(&mut self, x: Aspect, v: u16) {
        match self.pos(x) {
            Some(i) => self.a[i].1 += v,
            None => {
                self.a[self.n] = (x, v);
                self.n += 1;
            }
        }
    }
    fn merge(&mut self, x: Aspect, v: u16) {
        match self.pos(x) {
            Some(i) => self.a[i].1 = self.a[i].1.max(v),
            None => {
                self.a[self.n] = (x, v);
                self.n += 1;
            }
        }
    }
}

/// Tag used by `getAspectsSorted` (String.compareTo; addon tags unknown → sorted after by index, approximate).
fn sort_key(a: Aspect, table: &TcAspectTable) -> String {
    table.tag(a)
}

/// `ThaumcraftWorldGenerator.createRandomNodeAt` from the type roll on, on the caller's Random. `biome` is the
/// chunk biome array id at the node column, `dict` its dictionary types (Forge order), `env` the census verdicts.
#[allow(clippy::too_many_arguments)]
pub fn create_random_node(
    r: &mut JavaRandom,
    special_rarity: i32,
    table: &TcAspectTable,
    src: NodeSource,
    biome: u16,
    dict: &[String],
    env: NodeEnv,
) -> NodeInfo {
    let mut ty = match src {
        NodeSource::Silverwood => NodeType::Pure,
        NodeSource::Eerie => NodeType::Dark,
        NodeSource::Wild => {
            if r.next_int_bounded(special_rarity) == 0 {
                match r.next_int_bounded(10) {
                    0..=2 => NodeType::Dark,
                    3..=5 => NodeType::Unstable,
                    6..=8 => NodeType::Pure,
                    _ => NodeType::Hungry,
                }
            } else {
                NodeType::Normal
            }
        }
    };
    let modifier = if r.next_int_bounded(special_rarity / 2) == 0 {
        Some(match r.next_int_bounded(3) {
            0 => NodeModifier::Bright,
            1 => NodeModifier::Pale,
            _ => NodeModifier::Fading,
        })
    } else {
        None
    };
    let mut aura = biome_aura(dict);
    if ty != NodeType::Pure && biome == BIOME_TAINT {
        aura = (aura as f32 * 1.5) as i32;
        if r.next_boolean() {
            ty = NodeType::Tainted;
            aura = (aura as f32 * 1.5) as i32;
        }
    }
    if src == NodeSource::Silverwood {
        aura /= 4;
    }
    let value = r.next_int_bounded((aura / 2).max(1)) + aura / 2;
    let mut al = List {
        a: [(Aspect(0), 0); MAX_NODE_ASPECTS],
        n: 0,
    };
    match random_biome_tag(r, dict) {
        Some(a) => al.add(a, 2),
        None => {
            let c = table.complex(r.next_int_bounded(table.complex_count));
            al.add(c, 1);
            let b = Aspect(r.next_int_bounded(6) as u8);
            al.add(b, 1);
        }
    }
    for _ in 0..3 {
        if r.next_boolean() {
            if r.next_int_bounded(special_rarity) == 0 {
                let c = table.complex(r.next_int_bounded(table.complex_count));
                al.merge(c, 1);
            } else {
                let b = Aspect(r.next_int_bounded(6) as u8);
                al.merge(b, 1);
            }
        }
    }
    match ty {
        NodeType::Hungry => {
            al.merge(Aspect::HUNGER, 2);
            if r.next_boolean() {
                al.merge(Aspect::GREED, 1);
            }
        }
        NodeType::Pure => {
            if r.next_boolean() {
                al.merge(Aspect::LIFE, 2);
            } else {
                al.merge(Aspect::ORDER, 2);
            }
        }
        NodeType::Dark => {
            for a in [
                Aspect::DEATH,
                Aspect::UNDEAD,
                Aspect::ENTROPY,
                Aspect::DARKNESS,
            ] {
                if r.next_boolean() {
                    al.merge(a, 1);
                }
            }
        }
        _ => {}
    }
    if env.water {
        al.merge(Aspect::WATER, 1);
    }
    if env.lava {
        al.merge(Aspect::FIRE, 1);
        al.merge(Aspect::EARTH, 1);
    }
    if env.stone {
        al.merge(Aspect::EARTH, 1);
    }
    if env.foliage {
        al.merge(Aspect::PLANT, 1);
    }
    // spread in getAspectsSorted order (bubble sort by tag; ties impossible)
    let mut order: Vec<usize> = (0..al.n).collect();
    order.sort_by_key(|&i| sort_key(al.a[i].0, table));
    let mut spread = [0i32; MAX_NODE_ASPECTS];
    let mut total = 0f32;
    for (k, &i) in order.iter().enumerate() {
        spread[k] = if al.a[i].1 == 2 {
            50 + r.next_int_bounded(25)
        } else {
            25 + r.next_int_bounded(50)
        };
        total += spread[k] as f32;
    }
    for (k, &i) in order.iter().enumerate() {
        let v = (spread[k] as f32 / total * value as f32) as i32;
        let a = al.a[i].0;
        al.merge(a, v.max(0) as u16);
    }
    // an unnamed addon aspect sorts at an unknown place: the spread draws (amounts) are then uncertain
    let named = al.a[..al.n].iter().all(|x| table.known(x.0));
    NodeInfo {
        node_type: ty,
        modifier,
        aspects: al.a,
        n: al.n as u8,
        exact: named,
        env,
        env_certain: true,
    }
}
