#!/usr/bin/env python3
"""rwg2dump 2.17 chestloot.json -> data/<pack>/chestloot.json (compact form read by crates/rwg2-biomes/src/loot.rs).

usage: python3 tools/chestloot.py <dump>/chestloot.json data/gtnh-2.9.0/chestloot.json [names.json]
Each category: {"count":[min,max],"items":[["name",min,max,weight,maxStack],...]}; name = the stack's
unlocalized name with damage (the part of ItemStack.toString() after "<n>x"), as in dungeonloot.json.
names.json (optional, e.g. viewer/src/lib/chestnames.json): {category: [in-game display name by table index]}.
"""
import json
import sys


def name(stack):
    return stack.split("x", 1)[1] if "x" in stack else stack


def main(src, dst, names_dst=None):
    d = json.load(open(src))
    out = {"source": "rwg2dump %s chestloot.json" % d.get("rwg2dumpVersion", "?")}
    for cat, c in d["categories"].items():
        if "error" in c:
            print("skip %s: %s" % (cat, c["error"]), file=sys.stderr)
            continue
        items = [[name(i["stack"]), i["min"], i["max"], i["weight"], i.get("maxStack", 64)] for i in c["items"]]
        out[cat] = {"count": [c["countMin"], c["countMax"]], "items": items}
    with open(dst, "w") as f:
        f.write("{" + json.dumps("source") + ":" + json.dumps(out.pop("source")))
        for cat, c in out.items():
            f.write(",\n" + json.dumps(cat) + ':{"count":' + json.dumps(c["count"]) + ',"items":[')
            f.write(",\n".join(json.dumps(i, separators=(",", ":")) for i in c["items"]) + "]}")
        f.write("}\n")
    print("%d categories -> %s" % (len(out), dst))
    if names_dst:
        names = {cat: [i.get("name") or name(i["stack"]) for i in c["items"]]
                 for cat, c in d["categories"].items() if "error" not in c}
        with open(names_dst, "w") as f:
            json.dump(names, f, separators=(",", ":"), ensure_ascii=False)
        print("display names -> %s" % names_dst)


if __name__ == "__main__":
    main(*sys.argv[1:4])
