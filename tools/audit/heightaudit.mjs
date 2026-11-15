// Zoomed-out heights vs exact: overworld Blocks y per biome, Nether / TF terrain-mode heights.
import fs from 'node:fs';
const { instance } = await WebAssembly.instantiate(fs.readFileSync(process.argv[2]), {});
const ex = instance.exports;
const lp = ex.legend_ptr(), ll = ex.legend_len();
const legend = JSON.parse(new TextDecoder().decode(new Uint8Array(ex.memory.buffer, lp, ll)));
const genOf = new Map(legend.biomes.map((b) => [b.id, b.baseName ?? b.name]));
const S = 512;
const seeds = [1337n, 3380521564351534918n, 1631302684446519836n, 42n, 777n];
const areas = [[-1024, -1024], [512, -512], [-512, 1024], [1536, 1536], [-2048, 0]];
const agg = new Map();
const add = (k, d) => { if (!agg.has(k)) agg.set(k, []); agg.get(k).push(d); };
for (const seed of seeds) {
  const h = ex.create(seed, 1);
  for (const [x0, z0] of areas) {
    let p = ex.render_mode(h, x0, z0, S, S, 1, 3);
    const exact = new Uint16Array(ex.memory.buffer, p, S * S).slice();
    p = ex.render_mode(h, x0, z0, S / 2, S / 2, 2, 3);
    const guess = new Uint16Array(ex.memory.buffer, p, S * S / 4).slice();
    p = ex.render(h, x0, z0, S / 2, S / 2, 2);
    const rwg = new Uint16Array(ex.memory.buffer, p, S * S / 4).slice();
    for (let j = 0; j < S / 2; j++) for (let i = 0; i < S / 2; i++) {
      const k = j * (S / 2) + i, e = exact[2 * j * S + 2 * i];
      if ((e & 0x80) || (guess[k] & 0x80)) continue;
      add('OW ' + (genOf.get(rwg[k]) ?? rwg[k]), (guess[k] >> 8) - (e >> 8));
    }
  }
  ex.destroy(h);
  for (const dim of [-1, 7]) for (const [x0, z0] of areas.slice(0, 3)) {
    const W = 256;
    let p = ex.render_dim_mode(seed, dim, x0, z0, W, W, 1, 2);
    const exact = new Uint16Array(ex.memory.buffer, p, W * W).slice();
    p = ex.render_dim_mode(seed, dim, x0, z0, W / 2, W / 2, 2, 2);
    const guess = new Uint16Array(ex.memory.buffer, p, W * W / 4).slice();
    for (let j = 0; j < W / 2; j += 2) for (let i = 0; i < W / 2; i += 2) {
      // a step-2 block of 2×2 pixels is sampled at pixel (i+1, j+1) → world x0 + (i+1)·2
      const ex_ = exact[((j + 1) * 2) * W + (i + 1) * 2];
      add(dim === -1 ? 'Nether' : 'Twilight', guess[j * (W / 2) + i] / 8 - ex_ / 8);
    }
  }
}
const q = (a, f) => { const s = [...a].sort((x, y) => x - y); return s[Math.floor(f * (s.length - 1))]; };
const rows = [...agg].filter(([, a]) => a.length > 2000).map(([k, a]) => {
  const mean = a.reduce((x, y) => x + y, 0) / a.length;
  const abs = a.map(Math.abs);
  return { k, n: a.length, mean, p50: q(abs, 0.5), p90: q(abs, 0.9), within5: abs.filter((d) => d <= 5).length / a.length };
}).sort((a, b) => Math.abs(b.mean) - Math.abs(a.mean));
for (const r of rows) console.log(`${r.k.padEnd(30)} n ${String(r.n).padStart(7)} bias ${r.mean.toFixed(1).padStart(6)} |d| p50 ${String(r.p50).padStart(3)} p90 ${String(r.p90).padStart(3)} ≤5: ${(100 * r.within5).toFixed(0)}%`);
