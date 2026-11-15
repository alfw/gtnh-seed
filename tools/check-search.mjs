// Runs the wasm `search` export over seeds 0..299 with the same query as tests/search.rs and prints matches.
import fs from 'node:fs';
const { instance } = await WebAssembly.instantiate(fs.readFileSync(process.argv[2] ?? 'target/wasm32-unknown-unknown/release/rwg2_wasm.wasm'), {});
const ex = instance.exports;
const lp = ex.legend_ptr(), ll = ex.legend_len();
const legend = JSON.parse(new TextDecoder().decode(new Uint8Array(ex.memory.buffer, lp, ll)));
const tainted = legend.biomes.filter((b) => b.baseId === 193).map((b) => b.id);
const q = new TextEncoder().encode(JSON.stringify({ worldType: 1, landAtSpawn: true, step: 64, conds: [{ t: 'biome', ids: tainted, r: 1500 }, { t: 'lava', r: 4000 }, { t: 'volcano', r: 4000 }] }));
const n = 300;
const t0 = performance.now();
const ptr = ex.input_buf(q.length + n * 8);
const buf = new Uint8Array(ex.memory.buffer, ptr, q.length + n * 8);
buf.set(q);
const seeds = new BigInt64Array(n).map((_, i) => BigInt(i));
buf.set(new Uint8Array(seeds.buffer), q.length);
const rp = ex.search(q.length, n), rl = ex.result_len();
const res = JSON.parse(new TextDecoder().decode(new Uint8Array(ex.memory.buffer, rp, rl)));
const ms = performance.now() - t0;
console.log(`${res.checked} seeds in ${ms.toFixed(0)} ms (${(res.checked / ms * 1000).toFixed(0)} seeds/s), matches:`, res.matches.map((m) => m.seed).join(','));
