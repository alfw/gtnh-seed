import json, sys
import numpy as np
# usage: deco_run_vs_run.py [dump216-dir dump216b-dir] (defaults: the 1631302684446519836 held-out runs)
import os
_root = os.path.join(os.path.dirname(os.path.abspath(__file__)), '..')
_held = os.path.join(_root, 'testdata/pack/gtnh-2.10.0/heldout/1631302684446519836')
A = os.path.join(sys.argv[1] if len(sys.argv) > 2 else os.path.join(_held, 'dump216'), '')
B = os.path.join(sys.argv[2] if len(sys.argv) > 2 else os.path.join(_held, 'dump216b'), '')
MC=set("log log2 leaves leaves2 cactus tallgrass double_plant deadbush reeds pumpkin yellow_flower red_flower vine cocoa waterlily brown_mushroom red_mushroom".split())
BOP=set("foliage flowers flowers2 mushrooms plants logs1 logs2 logs3 logs4 leaves1 leaves2 leaves3 leaves4 colorizedLeaves1 colorizedLeaves2 appleLeaves persimmonLeaves petals bamboo ivy treeMoss flowerVine willow coral1 coral2".split())
def isd(n):
    if n.startswith('minecraft:'): return n[10:] in MC
    if n.startswith('BiomesOPlenty:'): return n[14:] in BOP
    return n.startswith('Botania:flower') or n.startswith('Botania:doubleFlower')
NY=151
def load(d):
    j=json.load(open(d+'deco216.json')); names=j['blocks']
    out={}
    for a in j['areas']:
        if 'file' not in a: continue
        raw=open(d+a['file'][:-3],'rb').read()
        n=a['chunks']**2*256*NY
        ids=np.frombuffer(raw[:2*n],dtype='<u2')
        meta=np.frombuffer(raw[2*n:3*n],dtype='u1')
        nm=np.array(names,dtype=object)[ids]
        out[a['name']]=(nm,meta)
    return out
a=load(A); b=load(B)
tot=[0,0,0]
for k in a:
    if k not in b: continue
    na,ma=a[k]; nb,mb=b[k]
    da=np.vectorize(isd,otypes=[bool])(na); db=np.vectorize(isd,otypes=[bool])(nb)
    both=(da&db).sum(); same=(da&db&(na==nb)&(ma==mb)).sum()
    print(f"{k:24} run1 {da.sum():6} run2 {db.sum():6}  agree {both/da.sum():.3f}/{both/max(1,db.sum()):.3f}  same block {same/da.sum():.3f}  all-voxels identical {(na==nb).mean():.5f}")
