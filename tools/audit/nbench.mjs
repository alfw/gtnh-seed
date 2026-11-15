import fs from 'node:fs';
const { instance } = await WebAssembly.instantiate(fs.readFileSync(process.argv[2]), {});
const ex = instance.exports;
for (const step of [2, 4, 16]) { let t = performance.now();
  for (let k = 0; k < 6; k++) ex.render_dim_mode(1337n, -1, k * 4096, 0, 256, 256, step, 2);
  console.log('step', step, ((performance.now() - t) / 6).toFixed(1), 'ms/256² tile'); }
