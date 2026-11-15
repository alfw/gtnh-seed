// Renders dump regions through the browser module (raw exports) and diffs against rwg2dump grids.
// usage: node tools/check-wasm.mjs [wasm] [dumpDir]
import fs from 'node:fs';
// defaults are relative to the repo root, arguments to the current directory
const root = new URL('..', import.meta.url).pathname;
const wasmPath = process.argv[2] ?? `${root}target/wasm32-unknown-unknown/release/rwg2_wasm.wasm`;
const dump = process.argv[3] ?? `${root}testdata/pack/gtnh-2.9.0/dump`;
const { instance } = await WebAssembly.instantiate(fs.readFileSync(wasmPath), {});
const ex = instance.exports;
const lp = ex.legend_ptr(), ll = ex.legend_len(); // call first: may grow memory and detach old buffers
const legend = JSON.parse(new TextDecoder().decode(new Uint8Array(ex.memory.buffer, lp, ll)));
console.log(`legend: ${legend.biomes.length} biomes, pack "${legend.pack}"`);
const manifest = JSON.parse(fs.readFileSync(`${dump}/manifest.json`, 'utf8'));
let total = 0, bad = 0, markerOnly = 0;
const t0 = performance.now();
for (const f of manifest.files.filter(f => f.region !== 'heights' && (f.region === 'near' || f.region === 'wide'))) {
  const h = ex.create(BigInt(f.seed), f.worldType === 'RWG2' ? 0 : 1);
  const n = f.w * f.h;
  const ptr = ex.render(h, f.x0, f.z0, f.w, f.h, f.step);
  const got = new Uint16Array(ex.memory.buffer, ptr, n).slice();
  const buf = fs.readFileSync(`${dump}/${f.file}`);
  const rwg = new Uint16Array(buf.buffer, buf.byteOffset, n);
  const gen = new Uint16Array(buf.buffer, buf.byteOffset + 2 * n, n);
  for (let i = 0; i < n; i++) {
    const want = gen[i] === legend.lavaMarker.genId ? 0xffff : rwg[i];
    if (got[i] !== want) { bad++; if (got[i] === 0xffff) markerOnly++; }
  }
  total += n;
  ex.destroy(h);
}
const ms = performance.now() - t0;
console.log(`${total} samples, ${bad} mismatches (${markerOnly} of them extra lava markers), ${(ms * 1000 / total).toFixed(2)} us/sample`);
process.exitCode = bad === 0 ? 0 : 1;
