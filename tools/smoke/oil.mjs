import { createRequire } from 'module';
const require = createRequire(new URL('../../viewer/', import.meta.url));
const pw = require('playwright');
const BROWSER = process.env.SMOKE_BROWSER ?? 'chromium'; // chromium | webkit | firefox
const chromium = pw[BROWSER];
const BASE = process.env.SMOKE_URL ?? 'http://localhost:4789'; // vite preview --port 4789 --strictPort
const browser = await chromium.launch({ args: BROWSER === 'chromium' ? ['--use-gl=swiftshader', '--enable-unsafe-swiftshader'] : [] });
const errs = [];
const p = await browser.newPage({ viewport: { width: 1440, height: 1000 } });
await p.addInitScript(() => {
  Object.defineProperty(navigator, 'hardwareConcurrency', { get: () => 8 });
  const W = window.Worker; window.__oil = [];
  window.Worker = class extends W {
    constructor(...a) { super(...a); const ids = new Map(); const pm = this.postMessage.bind(this);
      this.postMessage = (m, t) => { if (m?.type === 'spawnOil') ids.set(m.id, 1); return pm(m, t); };
      this.addEventListener('message', (e) => { if (ids.has(e.data.id)) window.__oil.push({ ms: e.data.ms, oil: e.data.oil }); }); }
  };
});
p.on('pageerror', (e) => errs.push(e.message));
p.on('console', (m) => m.type() === 'error' && errs.push('console: ' + m.text()));
const seed = process.argv[2] ?? '3302244597774558715';
await p.goto(`${BASE}/#seed=${seed}&x=-160&z=0&zoom=0`);
await p.evaluate(() => (localStorage.clear(), localStorage.setItem('gtnh-seedmap:layers', JSON.stringify({ layers: { mode: 'advanced' } })))); await p.reload();
let r = [];
for (let i = 0; i < 90; i++) { await p.waitForTimeout(1000); r = await p.evaluate(() => window.__oil); if (r.length) break; }
await p.waitForTimeout(2500);
console.log('spawnOil:', r.length ? `${Math.round(r[0].ms)} ms, ${r[0].oil.length} deposits ${JSON.stringify(r[0].oil.map(o => [o.x, o.y, o.z, o.r, o.c]))}` : 'none');
await p.screenshot({ path: '/tmp/oil1.png' });
// open the 3D view at the first deposit with x-ray
if (r.length && r[0].oil.length) {
  const o = r[0].oil[0];
  console.log('label check', await p.evaluate(() => document.body.innerText.includes('Oil deposits')));
}
console.log('errors:', errs.join('\n') || 'none');
await browser.close();
