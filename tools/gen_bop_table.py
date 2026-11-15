#!/usr/bin/env python3
"""Generate crates/rwg2-biomes/src/deco/bop_table.rs from the BiomesOPlenty 2.1.0 overworld biome sources.

usage: gen_bop_table.py <bop src/main/java/biomesoplenty/common/biome/overworld> <out.rs>

Per biome class: BiomeDecorator / bopFeatures counts, generatePumpkins, gem ore decorate, the heaviest weighted
flower / grass generator, and the `func_150567_a` tree selector as a draw tree (every `random.nextInt(n) == 0 ?`
is a shared-Random draw in evaluation order). Tree generators are mapped to approximate shapes (`deco::bop_trees`).
"""
import os
import re
import sys

src, out = sys.argv[1], sys.argv[2]
files = sorted(
    [os.path.join(src, f) for f in os.listdir(src) if f.endswith(".java")]
    + [os.path.join(src, "sub", f) for f in os.listdir(os.path.join(src, "sub")) if f.endswith(".java")]
    + [os.path.join(src, "ocean", f) for f in ("BiomeGenKelpForest.java", "BiomeGenCoralReef.java")]
)

DEC_FIELDS = {
    "treesPerChunk": "trees",
    "grassPerChunk": "grass",
    "flowersPerChunk": "flowers",
    "mushroomsPerChunk": "mushrooms",
    "reedsPerChunk": "reeds",
    "sandPerChunk": "sand",
    "sandPerChunk2": "sand2",
    "clayPerChunk": "clay",
    "deadBushPerChunk": "dead_bush",
    "cactiPerChunk": "cacti",
    "waterlilyPerChunk": "lily",
    "bigMushroomsPerChunk": "big_mush",
}
DEFAULTS = dict(trees=0, grass=0, flowers=0, mushrooms=0, reeds=0, sand=1, sand2=3, clay=1, dead_bush=0, cacti=0,
                lily=0, big_mush=0)


def block(expr):
    """Java block expression -> Rust db id expression (None if unknown)."""
    e = expr.strip()
    m = re.match(r"(?:Blocks|BOPCBlocks)\.(\w+)$", e)
    if not m:
        return None
    n = m.group(1)
    table = {
        "log": "db::LOG", "leaves": "db::LEAVES", "log2": "db::LOG2", "leaves2": "db::LEAVES2",
        "logs1": "db::BOP_LOGS1", "logs2": "db::BOP_LOGS2", "logs3": "db::BOP_LOGS3", "logs4": "db::BOP_LOGS4",
        "leaves1": "db::BOP_LEAVES1", "leaves3": "db::BOP_LEAVES3", "leaves4": "db::BOP_LEAVES4",
        "colorizedLeaves1": "db::BOP_CLEAVES1", "colorizedLeaves2": "db::BOP_CLEAVES2",
        "appleLeaves": "db::BOP_APPLE_LEAVES", "persimmonLeaves": "db::BOP_PERSIMMON_LEAVES",
        "petals": "db::BOP_PETALS", "bamboo": "db::BOP_BAMBOO",
        "ivy": "db::VINE", "treeMoss": "db::VINE", "flowerVine": "db::VINE", "willow": "db::VINE",
        "vine": "db::VINE",
    }
    if n == "leaves2" and e.startswith("BOPCBlocks"):
        return "db::BOP_LEAVES2"
    return table.get(n)


def split_args(s):
    out, depth, cur = [], 0, ""
    for ch in s:
        if ch == "(":
            depth += 1
        elif ch == ")":
            depth -= 1
        if ch == "," and depth == 0:
            out.append(cur.strip())
            cur = ""
        else:
            cur += ch
    if cur.strip():
        out.append(cur.strip())
    return out


def num(a, d=0):
    try:
        return int(a)
    except (ValueError, TypeError):
        return d


def mkb(b, m):
    return f"db::mk({b}, {m & 15})"


SOIL = {"grass": "SOIL_GRASS", "dirt": "SOIL_DIRT", "sand": "SOIL_SAND", "hardSand": "SOIL_HARD_SAND",
        "hardDirt": "SOIL_HARD_DIRT", "longGrass": "SOIL_LONG_GRASS", "mycelium": "SOIL_MYCELIUM"}


def soil(args):
    bits = [SOIL[x.split(".")[-1]] for x in args if x.split(".")[-1] in SOIL]
    return " | ".join(bits) if bits else "0"


def tree_gen(cls, args):
    """`new cls(args)` -> Rust `TG` expression."""
    a = args
    nums = [num(x, None) for x in a]

    def lb(i_log=0, i_leaf=1, i_lm=2, i_fm=3, dl="db::LOG", dv="db::LEAVES"):
        log = block(a[i_log]) if len(a) > i_log else None
        leaf = block(a[i_leaf]) if len(a) > i_leaf else None
        lm = nums[i_lm] if len(a) > i_lm and nums[i_lm] is not None else 0
        fm = nums[i_fm] if len(a) > i_fm and nums[i_fm] is not None else 0
        return mkb(log or dl, lm), mkb(leaf or dv, fm)

    def hv(i, dh, dv):
        h = nums[i] if len(a) > i and nums[i] is not None else dh
        v = nums[i + 1] if len(a) > i + 1 and nums[i + 1] is not None else dv
        return max(h, 1), max(v, 0)

    if cls == "WorldGenShrub":
        return f"TG::G(G::Shrub({num(a[0])}, {num(a[1])}))"
    if cls == "WorldGenOriginalTree":
        log, leaf = lb()
        if len(a) == 4:
            return f"TG::Orig({log}, {leaf}, 4, 3, false, false)"
        if len(a) == 5:
            return f"TG::Orig({log}, {leaf}, 5, 4, false, true)"
        return f"TG::Orig({log}, {leaf}, {num(a[5], 4)}, {max(num(a[6], 3), 1)}, {str(a[7] == 'true').lower()}, false)"
    if cls in ("WorldGenBOPShrub", "WorldGenMiniShrub"):
        log, leaf = lb()
        rest = a[4:]
        if cls == "WorldGenBOPShrub":
            miny, maxy = 0, 256
            if len(rest) >= 2 and num(rest[0], None) is not None:
                miny, maxy, rest = num(rest[0]), num(rest[1]), rest[2:]
            return f"TG::BopShrub({log}, {leaf}, {miny}, {maxy}, {soil(rest)})"
        return f"TG::MiniShrub({log}, {leaf}, {soil(rest)})"
    if cls == "WorldGenBOPJungle":
        return f"TG::G(G::BopJungle({num(a[1], 10)}, {num(a[2], 20)}, {num(a[3]) & 15}, {num(a[4]) & 15}))"
    if cls == "WorldGenTrees":
        vines = len(a) > 4 and a[4] == "true"
        return f"TG::G(G::Trees({num(a[1], 4)}, {num(a[2]) & 15}, {num(a[3]) & 15}, {str(vines).lower()}))"
    if cls == "WorldGenRainforestTree1":
        log, leaf = lb()
        return f"TG::Rainforest1({log}, {leaf}, {num(a[5])}, {num(a[6])})"
    if cls == "WorldGenTropicsShrub":
        return "TG::TropicsShrub"
    if cls in ("WorldGenBogTree1", "WorldGenBogTree2"):
        log, leaf = lb()
        if cls == "WorldGenOriginalTree":
            h, v = hv(5, 4, 3) if len(a) >= 7 else (4, 3)
        else:
            h, v = hv(5, 5, 4)
        return f"TG::Round({log}, {leaf}, {h}, {v}, AIR)"
    if cls == "WorldGenMixedTree":
        log, leaf = lb()
        l2 = block(a[4]) or "db::LEAVES"
        return f"TG::Mixed({log}, {leaf}, {mkb(l2, num(a[5]))})"
    if cls in ("WorldGenBOPBigTree", "WorldGenBOPHugeTree"):
        log, leaf = lb()
        return f"TG::Big({log}, {leaf}, {12 if cls.endswith('HugeTree') else 8})"
    if cls == "WorldGenBOPTaiga2":
        # (wood, leaves, metaWood, metaLeaves, notify, minHeight, randomHeight, altNo, metaFruit)
        log, leaf = lb()
        return f"TG::Taiga2({log}, {leaf}, {num(a[5])}, {num(a[6])}, {num(a[7])}, {num(a[8])})"
    if cls == "WorldGenBOPTaiga3":
        # same constructor as Taiga2
        log, leaf = lb()
        return f"TG::Taiga3({log}, {leaf}, {num(a[5])}, {num(a[6])}, {num(a[7])}, {num(a[8])})"
    if cls == "WorldGenBOPSwampTree":
        # (wood, leaves, metaWood, metaLeaves, minHeight, randomHeight[, vineBlock, vineMeta])
        log, leaf = lb()
        vine = block(a[6]) if len(a) > 6 else None
        vb = mkb(vine, max(num(a[7]), 0)) if vine and vine != "db::VINE" else "AIR"
        return f"TG::Swamp({log}, {leaf}, {num(a[4])}, {num(a[5])}, {vb})"
    if cls in ("WorldGenBOPTaiga1", "WorldGenBOPTaiga3", "WorldGenRedwoodTree",
               "WorldGenRedwoodTree2", "WorldGenRedwoodTree3", "WorldGenCypress"):
        log, leaf = lb()
        h, v = hv(5, 6, 4)
        if cls == "WorldGenBOPTaiga3" or cls.startswith("WorldGenRedwood"):
            h, v = min(h, 26), min(v, 10)
        width = 1 if cls in ("WorldGenCypress", "WorldGenBOPTaiga1") else 2
        return f"TG::Cone({log}, {leaf}, {h}, {v}, {width})"
    if cls == "WorldGenBOPSwampTree":
        log, leaf = lb()
        h, v = hv(4, 5, 3)
        vine = block(a[6]) if len(a) > 6 else None
        vb = mkb(vine, max(num(a[7]), 0)) if vine and vine != "db::VINE" else ("VINE" if vine else "AIR")
        return f"TG::Round({log}, {leaf}, {h}, {v}, {vb})"
    if cls in ("WorldGenBayou1", "WorldGenBayou2"):
        log, leaf = lb()
        return f"TG::Round({log}, {leaf}, 7, 4, VINE)"
    if cls == "WorldGenBayou3":
        return f"TG::Round({mkb(block(a[0]) or 'db::LOG', num(a[1]))}, {mkb('db::BOP_CLEAVES2', 0)}, 8, 4, VINE)"
    if cls == "WorldGenChaparral3":
        return "TG::Chaparral3"
    if cls in ("WorldGenTropicsShrub", "WorldGenBrush1", "WorldGenBrush2",
               "WorldGenChaparral3", "WorldGenBogBush"):
        if len(a) >= 4 and block(a[0]):
            log, leaf = lb()
        else:
            log, leaf = mkb("db::LOG", 0), mkb("db::LEAVES", 0)
        return f"TG::Bush({log}, {leaf}, {0 if cls == 'WorldGenMiniShrub' else 1})"
    if cls == "WorldGenBulbTree":
        log, leaf = lb()
        # WorldGenBulbTree(wood, leaves, metaWood, metaLeaves, notify, minHeight, randomHeight, vines)
        return f"TG::Bulb({log}, {leaf}, {num(a[5])}, {num(a[6])}, {str(a[7].strip() == 'true').lower()})"
    if cls == "WorldGenPineTree":
        return f"TG::Cone({mkb('db::BOP_LOGS4', 0)}, {mkb('db::BOP_CLEAVES2', 1)}, 8, 6, 1)"
    if cls in ("WorldGenPoplar", "WorldGenPoplar2"):
        return f"TG::Cone({mkb('db::LOG', 2)}, {mkb('db::BOP_LEAVES1', 0 if cls == 'WorldGenPoplar2' else 0)}, 8, 4, 1)"
    if cls in ("WorldGenTaiga2",):
        return f"TG::Cone({mkb('db::LOG', 1)}, {mkb('db::LEAVES', 1)}, 6, 3, 2)"
    if cls == "WorldGenTaiga1":
        return f"TG::Cone({mkb('db::LOG', 1)}, {mkb('db::LEAVES', 1)}, 7, 4, 1)"
    if cls == "WorldGenDeadTree":
        return f"TG::Dead({mkb('db::BOP_LOGS3', 2)})"
    if cls == "WorldGenPalmTree1":
        return "TG::Palm1"
    if cls == "WorldGenMangrove":
        return f"TG::Round({mkb('db::BOP_LOGS2', 2)}, {mkb('db::BOP_CLEAVES1', 1)}, 5, 3, AIR)"
    if cls in ("WorldGenBOPJungleUnused",):
        h = nums[1] if len(a) > 1 and nums[1] is not None else (4 if cls == "WorldGenTrees" else 5)
        lm = nums[3] if cls == "WorldGenBOPJungle" else nums[2] if len(a) > 2 else 0
        fm = nums[4] if cls == "WorldGenBOPJungle" else nums[3] if len(a) > 3 else 0
        vines = len(a) > 4 and a[-1] == "true" or cls == "WorldGenBOPJungle"
        return f"TG::Round({mkb('db::LOG', lm or 0)}, {mkb('db::LEAVES', fm or 0)}, {h}, 3, {'VINE' if vines else 'AIR'})"
    if cls == "WorldGenGiantFlower":
        return f"TG::Flower({num(a[0])})"
    if cls == "WorldGenSacredOak":
        return f"TG::Big({mkb('db::BOP_LOGS1', 0)}, {mkb('db::BOP_CLEAVES1', 0)}, 20)"
    return None


class P:
    """Recursive-descent parser of the tree selector expression; emits Rust `TS` expressions."""

    def __init__(self, s, unknown):
        self.s = s
        self.i = 0
        self.unknown = unknown

    def ws(self):
        while self.i < len(self.s) and self.s[self.i].isspace():
            self.i += 1

    def eat(self, t):
        self.ws()
        if self.s.startswith(t, self.i):
            self.i += len(t)
            return True
        return False

    def expr(self):
        self.ws()
        m = re.compile(r"\(?\s*(?:rand|random)\.nextInt\((\d+)\)\s*==\s*0\s*\)?\s*\?").match(self.s, self.i)
        if m:
            self.i = m.end()
            n = int(m.group(1))
            a = self.expr()
            if not self.eat(":"):
                raise ValueError("expected : at " + self.s[self.i:self.i + 40])
            b = self.expr()
            return f"&TS::Pick({n}, {a}, {b})"
        return self.atom()

    def atom(self):
        self.ws()
        if self.eat("(WorldGenAbstractTree)"):
            return self.atom()
        if self.eat("("):
            e = self.expr()
            if not self.eat(")"):
                raise ValueError("expected ) at " + self.s[self.i:self.i + 40])
            return e
        m = re.compile(r"(?:this\.)?worldGenerator(BigTree|Trees)").match(self.s, self.i)
        if m:
            self.i = m.end()
            return "&TS::Leaf(TG::VanillaBig)" if m.group(1) == "BigTree" else "&TS::Leaf(TG::Vanilla)"
        m = re.compile(r"new\s+(\w+)\s*\(").match(self.s, self.i)
        if m:
            self.i = m.end()
            depth, start = 1, self.i
            while depth:
                c = self.s[self.i]
                depth += c == "("
                depth -= c == ")"
                self.i += 1
            args = split_args(self.s[start:self.i - 1])
            cls = m.group(1)
            # draws inside the constructor arguments (e.g. `4 + random.nextInt(7)`), in order
            draws = [int(d) for d in re.findall(r"nextInt\((\d+)\)", self.s[start:self.i - 1])]
            args = [re.sub(r"^(\d+)\s*\+\s*(?:rand|random)\.nextInt\(\d+\)$", r"\1", x) for x in args]
            g = tree_gen(cls, args)
            if g is None:
                self.unknown.add(cls)
                g = "TG::Vanilla"
            leaf = f"&TS::Leaf({g})"
            for d in reversed(draws):
                leaf = f"&TS::Draw({d}, {leaf})"
            return leaf
        raise ValueError("bad atom at " + self.s[self.i:self.i + 60])


class GP(P):
    """Same draw-tree parser for `getRandomWorldGenForGrass` (leaves: tall grass / double flora); emits `FS`."""

    def expr(self):
        self.ws()
        m = re.compile(
            r"(?:\(\s*(?:rand|random)\.nextInt\((\d+)\)\s*==\s*0\s*\)|(?:rand|random)\.nextInt\((\d+)\)\s*==\s*0)\s*\?"
        ).match(self.s, self.i)
        if m:
            self.i = m.end()
            n = int(m.group(1) or m.group(2))
            a = self.expr()
            if not self.eat(":"):
                raise ValueError("expected : at " + self.s[self.i:self.i + 40])
            b = self.expr()
            return f"&FS::Pick({n}, {a}, {b})"
        return self.atom()

    def atom(self):
        self.ws()
        if self.eat("("):
            e = self.expr()
            if not self.eat(")"):
                raise ValueError("expected ) at " + self.s[self.i:self.i + 40])
            return e
        m = re.compile(r"new\s+(\w+)\s*\(([^()]*)\)").match(self.s, self.i)
        if not m:
            raise ValueError("bad grass atom at " + self.s[self.i:self.i + 60])
        self.i = m.end()
        g, _ = flora("Grass", m.group(1), m.group(2))
        if g is None:
            raise ValueError("unknown grass generator " + m.group(1))
        return f"&FS::Leaf({g})"


def body_of(src, sig):
    i = src.find(sig)
    if i < 0:
        return None
    j = src.find("{", i)
    depth, k = 0, j
    while True:
        if src[k] == "{":
            depth += 1
        elif src[k] == "}":
            depth -= 1
            if depth == 0:
                return src[j + 1:k]
        k += 1


FLORA_RE = re.compile(r"weighted(Flower|Grass)Gen\.put\(new (\w+)\(([^)]*)\)\s*,\s*([\d.]+)D?\)")


def flora(kind, cls, args):
    """weighted flower / grass generator -> (Rust `FloraGen`, family key for the weight totals)"""
    a = split_args(args)
    ids = {"flowers": "db::BOP_FLOWERS", "flowers2": "db::BOP_FLOWERS2", "foliage": "db::BOP_FOLIAGE",
           "red_flower": "db::RED_FLOWER", "yellow_flower": "db::YELLOW_FLOWER", "plants": "db::BOP_PLANTS"}

    def fl(blockexpr, m):
        b = blockexpr.split(".")[-1]
        if b == "tallgrass":
            return f"Flora::TallGrass({m})", "tallgrass"
        if b == "mushrooms":
            return f"Flora::Mushroom({m})", "mushrooms"
        if b in ids:
            return f"Flora::Decor({ids[b]}, {m})", b
        return None, None

    if cls in ("WorldGenBOPTallGrass", "WorldGenTallGrass") and len(a) >= 2:
        f, fam = fl(a[0], num(a[1]))
        return (f"FloraGen::Grass({f})", "grass:" + fam) if f else (None, None)
    if cls == "WorldGenBOPFlora" and len(a) >= 2:
        f, fam = fl(a[0], num(a[1]))
        g = num(a[2], 64) if len(a) > 2 else 64
        return (f"FloraGen::Flora({f}, {g})", "flora:" + fam) if f else (None, None)
    if cls == "WorldGenBOPDoubleFlora":
        if len(a) <= 2:
            m = num(a[0])
            g = num(a[1], 64) if len(a) > 1 else 64
            return f"FloraGen::Double(db::mk(db::DOUBLE_PLANT, {m}), db::mk(db::DOUBLE_PLANT, 8), {g})", "double"
        b1, b2 = ids.get(a[0].split(".")[-1]), ids.get(a[1].split(".")[-1])
        if b1 and b2:
            g = num(a[4], 64) if len(a) > 4 else 64
            return f"FloraGen::Double(db::mk({b1}, {num(a[2])}), db::mk({b2}, {num(a[3])}), {g})", "bopdouble"
    return None, None


rows, unknown, failed = [], set(), []
parsed = {}
for f in files:
    s = open(f, encoding="utf-8").read()
    cls = os.path.basename(f)[:-5]
    ctor = body_of(s, f"public {cls}(") or ""
    d = dict(DEFAULTS)
    for k, v in re.findall(r"theBiomeDecorator\.(\w+)\s*=\s*(-?\d+)\s*;", ctor):
        if k in DEC_FIELDS:
            d[DEC_FIELDS[k]] = int(v)
    feats = []
    bop_flowers = bop_grass = 0
    for k, v in re.findall(r"bopFeatures\.(\w+)\s*=\s*(-?\d+|true|false)\s*;", ctor):
        v = 1 if v == "true" else 0 if v == "false" else int(v)
        if k == "bopFlowersPerChunk":
            bop_flowers = v
        elif k == "bopGrassPerChunk":
            bop_grass = v
        elif v:
            feats.append((k, v))
    pumpkins = not re.search(r"generatePumpkins\s*=\s*false", ctor)
    # `getRandomWeightedWorldGenerator` picks with Math.random(): take the block with the largest total weight (the
    # most likely registry name), then its heaviest meta
    cands = {"Flower": [], "Grass": []}
    for kind, fc, args, wgt in FLORA_RE.findall(ctor):
        g, fam = flora(kind, fc, args)
        if g:
            cands[kind].append((g, fam, float(wgt)))
    best = {}
    for kind, cs in cands.items():
        tot = {}
        for g, fam, wgt in cs:
            tot[fam] = tot.get(fam, 0.0) + wgt
        if not tot:
            best[kind] = (None, 0)
            continue
        fam = max(tot, key=lambda f: (tot[f], -list(tot).index(f)))
        g = max((c for c in cs if c[1] == fam), key=lambda c: c[2])
        best[kind] = (g[0], g[2])
    dec = body_of(s, "public void decorate(") or ""
    gem = "12 + random.nextInt(6)" in dec
    gb = body_of(s, "getRandomWorldGenForGrass(")
    gp = "None"
    if gb is not None:
        e = re.search(r"return\s+(.*?);\s*$", gb.strip(), re.S)
        try:
            gp = "Some(" + GP(e.group(1), unknown).expr() + ")"
        except (ValueError, AttributeError) as ex:
            failed.append((cls + " grass", str(ex)))
    tb = body_of(s, "func_150567_a(")
    if tb is None:
        ts = "&TS::Pick(10, &TS::Leaf(TG::VanillaBig), &TS::Leaf(TG::Vanilla))"
    else:
        e = re.search(r"return\s+(.*?);\s*$", tb.strip(), re.S)
        try:
            ts = P(e.group(1), unknown).expr()
        except (ValueError, AttributeError) as ex:
            failed.append((cls, str(ex)))
            ts = "&TS::Leaf(TG::Vanilla)"
    rows.append((cls, d, pumpkins, gem, feats, bop_flowers, bop_grass, best["Flower"][0] or "FloraGen::Flora(Flora::Decor(db::BOP_FLOWERS, 0), 64)",
                 best["Grass"][0] or "FloraGen::Grass(Flora::TallGrass(1))", gp, ts))

w = open(out, "w")
w.write("//! Generated by `tools/gen_bop_table.py` from the BiomesOPlenty 2.1.0 overworld biome sources. Do not edit.\n\n")
w.write("use super::bop::{Flora, FloraGen, Gen, FS};\nuse super::bop_trees::*;\nuse super::gens::G;\nuse super::{db, AIR};\n\n")
w.write("const VINE: super::Db = db::mk(db::VINE, 0);\n\n")
w.write("pub static TABLE: &[Gen] = &[\n")
for cls, d, pumpkins, gem, feats, bf, bg, fl, gr, gp, ts in rows:
    fs = ", ".join(f'("{k}", {v})' for k, v in feats)
    w.write(f"    Gen {{\n        class: \"{cls}\",\n")
    for k in DEFAULTS:
        w.write(f"        {k}: {d[k]},\n")
    w.write(f"        pumpkins: {str(pumpkins).lower()},\n        gem: {str(gem).lower()},\n")
    w.write(f"        feats: &[{fs}],\n        bop_flowers: {bf},\n        bop_grass: {bg},\n")
    w.write(f"        flower: {fl},\n        grass_gen: {gr},\n        grass_pick: {gp},\n        tree: {ts},\n    }},\n")
w.write("];\n")
w.close()
print(f"{len(rows)} biomes; unknown tree classes: {sorted(unknown)}; failed: {failed}", file=sys.stderr)
