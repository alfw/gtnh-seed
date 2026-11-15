// 3D view rings: mid / far samples (view_cols / view_cols_dim at the street view's steps) vs the exact step-1 top
// at the same columns. Usage: node ringaudit.mjs <wasm>
import fs from 'node:fs';
const { instance } = await WebAssembly.instantiate(fs.readFileSync(process.argv[2]), {});
const ex = instance.exports;
const W = 256;
function cols(dim, seed, h, x0, z0, w, step) {
  const p = dim ? ex.view_cols_dim(seed, dim, x0, z0, w, w, step) : ex.view_cols(h, x0, z0, w, w, step);
  return new Uint32Array(ex.memory.buffer, p, 3 * w * w).slice();
}
const pct = (a, q) => a.slice().sort((x, y) => x - y)[Math.floor(q * (a.length - 1))];
for (const [dim, steps] of [[0, [-4, 16]], [7, [-4, 16]], [-1, [8]], [1, [16]]]) {
  for (const step of steps) {
    const d = [], cls = [0, 0];
    for (const seed of [1337n, 3380521564351534918n, 42n]) {
      const h = dim ? 0 : ex.create(seed, 1);
      for (const [x0, z0] of [[-1024, -1024], [512, -512], [-512, 1024], [0, 0]]) {
        const ex1 = cols(dim, seed, h, x0, z0, W, 1);
        const n = W / Math.abs(step);
        const ap = cols(dim, seed, h, x0, z0, n, step);
        for (let j = 0; j < n; j++) for (let i = 0; i < n; i++) {
          const e = ex1[3 * ((j * Math.abs(step)) * W + i * Math.abs(step))], a = ap[3 * (j * n + i)];
          const et = e & 255, at = a & 255;
          if (dim === 1 && (((e >> 16) & 255) === 19) !== (((a >> 16) & 255) === 19)) { cls[1]++; continue; }
          if (dim === 1 && ((e >> 16) & 255) === 19) continue;
          d.push(at - et); cls[0]++;
        }
      }
      if (h) ex.destroy(h);
    }
    const abs = d.map(Math.abs);
    console.log(`dim ${dim} step ${step}: n ${d.length} bias ${(d.reduce((s, v) => s + v, 0) / d.length).toFixed(2)} p50 ${pct(abs, 0.5)} p90 ${pct(abs, 0.9)} max ${Math.max(...abs)} ≤5 ${(abs.filter((v) => v <= 5).length / abs.length * 100).toFixed(1)}%${dim === 1 ? ` void mismatch ${cls[1]}` : ''}`);
  }
}
