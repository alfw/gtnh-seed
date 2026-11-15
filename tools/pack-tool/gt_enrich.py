#!/usr/bin/env python3
"""Fill vein display fields in data/gtnh-<version>/gregtech.json from GT5-Unofficial source.

pack-tool import derives displayName (live mix names) and oreNames (live local names) from the dump and carries
colours over from the previous pack. Colours of new or changed veins are not in the dump: they come from GT's
MaterialsInit ARGB values, so run this when pack-tool import warns about them:

    git clone --depth 1 --branch <gt-version-tag> https://github.com/GTNewHorizons/GT5-Unofficial /tmp/gt5u
    python3 tools/pack-tool/gt_enrich.py data/gtnh-<version>/gregtech.json --gt-src /tmp/gt5u [--only-missing]

Rules (same as 2.9.0): displayName = primary + OreMixBuilder.setLocalizedName materials, "A, B & C";
iconRgb = the single setLocalizedName material's ARGB, else the primary's; oreNames/oreRgb = distinct materials in
primary, secondary, between, sporadic order (default local names; lang-file overrides are not applied).
--only-missing keeps existing values and only replaces the #808080 placeholders pack-tool writes.
"""
import argparse
import json
import re

ap = argparse.ArgumentParser()
ap.add_argument("gregtech_json")
ap.add_argument("--gt-src", required=True, help="GT5-Unofficial checkout (repo root)")
ap.add_argument("--only-missing", action="store_true")
a = ap.parse_args()

src = f"{a.gt_src}/src/main/java/gregtech"
mi = open(f"{src}/loaders/materials/MaterialsInit.java").read()
mats = {}
for m in re.finditer(r'new MaterialBuilder\(\)\.setName\("(\w+)"\)(.*?)\.constructMaterial\(\)', mi, re.S):
    name, body = m.group(1), m.group(2)
    loc = re.search(r'setDefaultLocalName\("([^"]*)"\)', body)
    argb = re.search(r"setARGB\((0x[0-9a-fA-F]+)\)", body)
    mats[name] = (loc.group(1) if loc else name, int(argb.group(1), 16) & 0xFFFFFF if argb else 0x808080)

om = open(f"{src}/api/enums/OreMixes.java").read()
mixes = {}
for chunk in om.split("new OreMixBuilder()")[1:]:
    nm = re.match(r'\.name\("([\w.]+)"\)', chunk.strip())
    if not nm:
        continue

    def g(k):
        r = re.search(rf"\.{k}\(Materials\.(\w+)\)", chunk)
        return r.group(1) if r else None

    loc = re.search(r"setLocalizedName\(([^)]*)\)", chunk)
    locs = [x.strip().replace("Materials.", "") for x in loc.group(1).split(",")] if loc else []
    mixes[nm.group(1)] = dict(primary=g("primary"), secondary=g("secondary"), between=g("inBetween"), sporadic=g("sporadic"), locs=locs)


def fmt(xs):
    return xs[0] if len(xs) == 1 else ", ".join(xs[:-1]) + " & " + xs[-1]


d = json.load(open(a.gregtech_json))
changed = 0
for v in d["veins"]:
    if a.only_missing and v.get("iconRgb") not in (None, "#808080") and v.get("displayName"):
        continue
    mx = mixes.get(v["name"])
    if mx is None:
        print("no OreMixes entry for", v["name"], "- left as is")
        continue
    keys = [mx["primary"]] + mx["locs"]
    if not v.get("displayName") or not a.only_missing:
        v["displayName"] = fmt([mats[k][0] for k in keys])
    rep = mx["locs"][0] if len(mx["locs"]) == 1 else mx["primary"]
    v["iconRgb"] = "#%06x" % mats[rep][1]
    seen = []
    for k in [mx["primary"], mx["secondary"], mx["between"], mx["sporadic"]]:
        if k and k not in seen:
            seen.append(k)
    if not a.only_missing:
        v["oreNames"] = [mats[k][0] for k in seen]
    v["oreRgb"] = ["#%06x" % mats[k][1] for k in seen]
    changed += 1
    print(v["name"], "|", v["displayName"], v["iconRgb"], v["oreRgb"])
json.dump(d, open(a.gregtech_json, "w"), indent=2)
print(f"updated {changed} veins")
