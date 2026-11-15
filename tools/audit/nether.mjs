import fs from 'node:fs';
const { instance } = await WebAssembly.instantiate(fs.readFileSync(process.argv[2]), {});
const ex = instance.exports;
const hist = new Map(), cases = [];
for (const seed of [1337n, 3380521564351534918n, 42n]) for (const [x0, z0] of [[-1024, -1024], [512, -512], [-512, 1024]]) {
  const W = 256;
  let p = ex.render_dim_mode(seed, -1, x0, z0, W, W, 1, 2);
  const exact = new Uint16Array(ex.memory.buffer, p, W * W).slice();
  p = ex.render_dim_mode(seed, -1, x0, z0, W / 2, W / 2, 2, 2);
  const guess = new Uint16Array(ex.memory.buffer, p, W * W / 4).slice();
  for (let j = 0; j < W / 2; j += 2) for (let i = 0; i < W / 2; i += 2) {
    const e = exact[((j + 1) * 2) * W + (i + 1) * 2] / 8 - 31, g = guess[j * (W / 2) + i] / 8 - 31;
    const b = Math.round((g - e) / 10) * 10;
    hist.set(b, (hist.get(b) ?? 0) + 1);
    if (Math.abs(g - e) > 30 && cases.length < 12) cases.push(`seed ${seed} x ${x0 + (i + 1) * 2} z ${z0 + (j + 1) * 2}: exact ${e} guess ${g}`);
  }
}
console.log([...hist].sort((a, b) => a[0] - b[0]).map(([k, v]) => `${k}:${v}`).join(' '));
console.log(cases.join('\n'));
