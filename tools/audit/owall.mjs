import fs from 'node:fs';
const { instance } = await WebAssembly.instantiate(fs.readFileSync(process.argv[2]), {});
const ex = instance.exports; const S = 512; const d = [];
for (const seed of [1337n, 3380521564351534918n, 1631302684446519836n, 42n, 777n]) { const h = ex.create(seed, 1);
  for (const [x0, z0] of [[-1024, -1024], [512, -512], [-512, 1024], [1536, 1536], [-2048, 0]]) {
    let p = ex.render_mode(h, x0, z0, S, S, 1, 3); const e = new Uint16Array(ex.memory.buffer, p, S * S).slice();
    p = ex.render_mode(h, x0, z0, S / 2, S / 2, 2, 3); const g = new Uint16Array(ex.memory.buffer, p, S * S / 4).slice();
    for (let j = 0; j < S / 2; j++) for (let i = 0; i < S / 2; i++) { const a = e[2 * j * S + 2 * i], b = g[j * (S / 2) + i];
      d.push([Math.abs((b >> 8) - (a >> 8)), (a & 0x80) || (b & 0x80)]); } }
  ex.destroy(h); }
for (const [name, f] of [['all', () => true], ['land', (x) => !x[1]], ['water', (x) => x[1]]]) {
  const v = d.filter(f).map((x) => x[0]).sort((a, b) => a - b);
  console.log(name, 'n', v.length, 'p50', v[v.length >> 1], 'p90', v[Math.floor(v.length * 0.9)], '≤5', (v.filter((x) => x <= 5).length / v.length * 100).toFixed(1) + '%');
}
