// Zoomed-out Blocks guess (step 2) vs exact top block (step 1) at the same columns, per in-game biome.
import fs from 'node:fs';
const { instance } = await WebAssembly.instantiate(fs.readFileSync(process.argv[2]), {});
const ex = instance.exports;
const lp = ex.legend_ptr(), ll = ex.legend_len();
const legend = JSON.parse(new TextDecoder().decode(new Uint8Array(ex.memory.buffer, lp, ll)));
const bp = ex.blocks_json(), bl = ex.blocks_json_len();
const blocks = JSON.parse(new TextDecoder().decode(new Uint8Array(ex.memory.buffer, bp, bl)));
const bname = (id) => blocks.find((b) => b.id === id)?.label ?? id;
const genOf = new Map(legend.biomes.map((b) => [b.id, { name: b.baseName ?? b.name }]));
const stats = new Map(); // gen name -> Map(guess|exact -> n)
const S = 512;
for (const seed of [1337n, 3380521564351534918n, 1631302684446519836n, 42n, 777n]) {
  const h = ex.create(seed, 1);
  for (const [x0, z0] of [[-1024, -1024], [512, -512], [-512, 1024], [1536, 1536], [-2048, 0]]) {
    let p = ex.render_mode(h, x0, z0, S, S, 1, 3);
    const exact = new Uint16Array(ex.memory.buffer, p, S * S).slice();
    p = ex.render_mode(h, x0, z0, S / 2, S / 2, 2, 3);
    const guess = new Uint16Array(ex.memory.buffer, p, S * S / 4).slice();
    p = ex.render(h, x0, z0, S / 2, S / 2, 2);
    const rwg = new Uint16Array(ex.memory.buffer, p, S * S / 4).slice();
    for (let j = 0; j < S / 2; j++) for (let i = 0; i < S / 2; i++) {
      const k = j * (S / 2) + i;
      const e = exact[(2 * j) * S + 2 * i];
      if ((e & 0x80) || (guess[k] & 0x80)) continue; // under water either way
      const b = genOf.get(rwg[k]);
      const name = b ? b.name : `rwg ${rwg[k]}`;
      const key = `${bname(guess[k] & 0x7f)} | ${bname(e & 0x7f)}`;
      if (!stats.has(name)) stats.set(name, new Map());
      const m = stats.get(name);
      m.set(key, (m.get(key) ?? 0) + 1);
    }
  }
  ex.destroy(h);
}
const out = [];
for (const [name, m] of stats) {
  const tot = [...m.values()].reduce((a, b) => a + b, 0);
  const guessTot = new Map(), agree = [...m].filter(([k]) => { const [g, e] = k.split(' | '); return g === e; }).reduce((a, [, n]) => a + n, 0);
  for (const [k, n] of m) { const g = k.split(' | ')[0]; guessTot.set(g, (guessTot.get(g) ?? 0) + n); }
  // per guessed block: how often the exact block anywhere in the biome is that block
  const exactTot = new Map();
  for (const [k, n] of m) { const e = k.split(' | ')[1]; exactTot.set(e, (exactTot.get(e) ?? 0) + n); }
  const top = (mm) => [...mm].sort((a, b) => b[1] - a[1]).slice(0, 3).map(([k, n]) => `${k} ${(100 * n / tot).toFixed(0)}%`).join(', ');
  out.push({ name, tot, agree: agree / tot, guess: top(guessTot), exact: top(exactTot), never: [...guessTot.keys()].filter((g) => !exactTot.has(g) && guessTot.get(g) / tot > 0.05) });
}
out.sort((a, b) => b.tot - a.tot);
for (const o of out) if (o.tot > 2000) console.log(`${o.name.padEnd(28)} n ${String(o.tot).padStart(7)} agree ${(100 * o.agree).toFixed(0).padStart(3)}% | guess ${o.guess} | exact ${o.exact}${o.never.length ? '  ** guess never real: ' + o.never.join(',') : ''}`);
