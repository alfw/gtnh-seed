// Blocks map mode (render mode 3, 1 block/px) per 256² tile in V8, single thread.
// usage: node tools/bench-blocks.mjs [wasm]
import fs from 'node:fs';
const wasmPath = process.argv[2] ?? 'viewer/src/lib/rwg2.wasm';
const { instance } = await WebAssembly.instantiate(fs.readFileSync(wasmPath), {});
const ex = instance.exports;
ex.legend_ptr();
for (const seed of [1337n, 92900795n]) {
  const h = ex.create(seed, 1);
  const ts = [];
  for (const [tx, tz] of [[0, 0], [1, 0], [-3, 2], [2, -2]]) {
    const t0 = performance.now();
    ex.render_mode(h, tx * 256, tz * 256, 256, 256, 1, 3);
    ts.push(performance.now() - t0);
  }
  console.log(`seed ${seed}: ${ts.map((t) => t.toFixed(0)).join(' / ')} ms per tile (mean ${(ts.reduce((a, b) => a + b) / ts.length).toFixed(0)})`);
  ex.destroy(h);
}
