#!/usr/bin/env python3
"""Browsable GPL source: <out>/index.html (file list, licence, build steps) + <out>/files/<path>.txt per file.
Usage: source-pages.py <staged source dir> <out dir>. Deterministic output."""
import html, os, shutil, sys

src, out = sys.argv[1], sys.argv[2]
files_dir = os.path.join(out, 'files')
shutil.rmtree(files_dir, ignore_errors=True)
rows = []
for dp, dn, fn in os.walk(src):
    dn.sort()
    for f in sorted(fn):
        full = os.path.join(dp, f)
        rel = os.path.relpath(full, src).replace(os.sep, '/')
        dst = os.path.join(files_dir, rel + '.txt')
        os.makedirs(os.path.dirname(dst), exist_ok=True)
        shutil.copyfile(full, dst)
        rows.append((rel, os.path.getsize(full)))
rows.sort()
total = sum(n for _, n in rows)
li = '\n'.join(
    f'<li><a href="files/{html.escape(r)}.txt">{html.escape(r)}</a> <span>{n:,} B</span></li>' for r, n in rows
)
page = f'''<!DOCTYPE html>
<html lang="en"><head><meta charset="utf-8"><meta name="viewport" content="width=device-width, initial-scale=1">
<title>GTNH Seed Map — source code</title>
<style>
body {{ font: 14px/1.5 system-ui, sans-serif; margin: 2rem auto; max-width: 900px; padding: 0 1rem; color: #ddd; background: #1b1d21; }}
a {{ color: #8ab4f8; }} code, pre {{ background: #2a2d33; padding: 0 .3em; border-radius: 3px; }} pre {{ padding: .6em; overflow-x: auto; }}
ul {{ list-style: none; padding: 0; columns: 2; font-family: ui-monospace, monospace; font-size: 12px; }}
li span {{ color: #888; }}
</style></head><body>
<h1>GTNH Seed Map — complete source of this build</h1>
<p>The worldgen port (<code>crates/rwg2-biomes</code>, <code>crates/rwg2-wasm</code>) is derived from
<a href="https://github.com/GTNewHorizons/Realistic-World-Gen-2">Realistic-World-Gen-2</a> and licensed
<b>GPL-3.0-or-later</b> (<a href="files/LICENSE.txt">LICENSE</a>). Below is every file needed to rebuild the
WebAssembly module and this site: Rust crates and tests, the embedded GTNH pack data and the viewer sources
({len(rows)} files, {total:,} bytes). Self-hosted builds also ship <code>source/gtnh-seedmap-src.zip</code>.</p>
<h2>Build</h2>
<pre>rustup target add wasm32-unknown-unknown
cd viewer &amp;&amp; npm ci &amp;&amp; npm run build    # wasm + static site in viewer/dist</pre>
<p>Each file below is the original file with <code>.txt</code> appended to its name.</p>
<ul>
{li}
</ul>
</body></html>
'''
open(os.path.join(out, 'index.html'), 'w').write(page)
print(f'source pages: {len(rows)} files')
