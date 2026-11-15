// Speed + output digest of the main wasm workloads (biome / terrain / blocks tiles, a feature cell, two searches),
// to compare two builds of the module. usage: node tools/bench-wasm.mjs [wasm] [reps]
// Builds are equivalent if the printed digests match; run each build 2–3 times, single runs vary by ~10%.
import fs from 'node:fs';
const wasmPath = process.argv[2] ?? 'viewer/src/lib/rwg2.wasm';
const reps = +(process.argv[3] ?? 1);
const { instance } = await WebAssembly.instantiate(fs.readFileSync(wasmPath), {});
const ex = instance.exports;
const lp = ex.legend_ptr(), ll = ex.legend_len();
const legend = JSON.parse(new TextDecoder().decode(new Uint8Array(ex.memory.buffer, lp, ll)));

let digest = 0x811c9dc5;
const eat = (bytes) => {
  let h = digest;
  for (let i = 0; i < bytes.length; i++) h = Math.imul(h ^ bytes[i], 0x01000193) >>> 0;
  digest = h;
};
const results = [];
const run = (name, f) => {
  let best = Infinity;
  for (let r = 0; r < reps; r++) {
    const t0 = performance.now();
    f(r === 0);
    best = Math.min(best, performance.now() - t0);
  }
  results.push([name, best]);
  console.log(`${name.padEnd(34)} ${best.toFixed(0).padStart(6)} ms`);
};

const tile = (h, x, z, w, step, mode, eatIt) => {
  const p = ex.render_mode(h, x, z, w, w, step, mode);
  if (eatIt) eat(new Uint8Array(ex.memory.buffer, p, w * w * 2));
};

for (const seed of [1337n, 92900795n]) {
  const h = ex.create(seed, 1);
  run(`biomes step 1 ×4 (${seed})`, (e) => { for (const [x, z] of [[0, 0], [256, 0], [-768, 512], [512, -512]]) tile(h, x, z, 256, 1, 0, e); });
  run(`biomes step 16 ×4 (${seed})`, (e) => { for (const [x, z] of [[0, 0], [4096, 0], [-8192, 4096], [8192, -8192]]) tile(h, x, z, 256, 16, 0, e); });
  run(`terrain step 1 ×4 (${seed})`, (e) => { for (const [x, z] of [[0, 0], [256, 0], [-768, 512], [512, -512]]) tile(h, x + 1024, z, 256, 1, 2, e); });
  run(`terrain step 16 ×2 (${seed})`, (e) => { for (const [x, z] of [[0, 0], [-8192, 4096]]) tile(h, x, z + 4096, 256, 16, 2, e); });
  run(`blocks step 1 ×2 (${seed})`, (e) => { for (const [x, z] of [[0, 0], [-768, 512]]) tile(h, x - 2048, z, 256, 1, 3, e); });
  run(`features ores+tc cell (${seed})`, (e) => {
    const p = ex.features(h, 0, 0, 2048, 2048, 16 | 8);
    if (e) eat(new Uint8Array(ex.memory.buffer, p, ex.features_len() * 4));
  });
  ex.destroy(h);
}

const tainted = legend.biomes.filter((b) => b.baseId === 193).map((b) => b.id);
const search = (name, q, n) => run(name, (e) => {
  const qb = new TextEncoder().encode(JSON.stringify(q));
  const ptr = ex.input_buf(qb.length + n * 8);
  const buf = new Uint8Array(ex.memory.buffer, ptr, qb.length + n * 8);
  buf.set(qb);
  buf.set(new Uint8Array(new BigInt64Array(n).map((_, i) => BigInt(i)).buffer), qb.length);
  const rp = ex.search(qb.length, n), rl = ex.result_len();
  if (e) eat(new Uint8Array(ex.memory.buffer, rp, rl));
});
search('search tainted+lava+volcano ×300', { worldType: 1, landAtSpawn: true, step: 64, conds: [{ t: 'biome', ids: tainted, r: 1500 }, { t: 'lava', r: 4000 }, { t: 'volcano', r: 4000 }] }, 300);
search('search spawn-based stronghold ×40', { worldType: 1, center: 'spawn', conds: [{ t: 'stronghold', r: 1500 }] }, 40);

const total = results.reduce((a, [, t]) => a + t, 0);
console.log(`${'total'.padEnd(34)} ${total.toFixed(0).padStart(6)} ms`);
console.log(`digest ${digest.toString(16).padStart(8, '0')}`);
