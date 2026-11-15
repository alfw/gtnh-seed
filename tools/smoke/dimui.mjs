import { createRequire } from 'node:module';
const require = createRequire(new URL('../../viewer/', import.meta.url));
const pw = require('playwright');
const BROWSER = process.env.SMOKE_BROWSER ?? 'chromium'; // chromium | webkit | firefox
const chromium = pw[BROWSER];
const BASE = process.env.SMOKE_URL ?? 'http://localhost:4789'; // vite preview --port 4789 --strictPort
const b = await chromium.launch(); const pg = await b.newPage({ viewport: { width: 1400, height: 900 } });
const errs = []; pg.on('pageerror', e => errs.push(e.message)); pg.on('console', m => m.type() === 'error' && errs.push(m.text()));
await pg.addInitScript(() => localStorage.setItem('gtnh-seedmap:layers', JSON.stringify({ layers: { tcNodes: true, ores: false, relief: true } })));
for (const [dim, name, x, z, zoom] of [[7, 'tf', -100, 0, 1], [-1, 'nether', 0, -80, 1], [7, 'tf_far', 0, 0, -3]]) {
  await pg.goto(`${BASE}/#seed=92900795&type=RWG2_CONTINENTS&dim=${dim}&x=${x}&z=${z}&zoom=${zoom}`);
  await pg.waitForTimeout(1500);
  await pg.getByRole('button', { name: 'Terrain' }).click();
  await pg.waitForTimeout(12000);
  await pg.mouse.move(500, 450);
  await pg.waitForTimeout(1500);
  const ro = await pg.locator('.readout').innerText().catch(() => '');
  await pg.screenshot({ path: `/tmp/dim_${name}.png` });
  console.log(name, JSON.stringify(ro.slice(0, 300)));
}
console.log('errors', errs);
await b.close();
