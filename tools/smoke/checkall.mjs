import { createRequire } from 'module';
const require = createRequire(new URL('../../viewer/', import.meta.url));
const pw = require('playwright');
const BROWSER = process.env.SMOKE_BROWSER ?? 'chromium'; // chromium | webkit | firefox
const chromium = pw[BROWSER];
const BASE = process.env.SMOKE_URL ?? 'http://localhost:4789'; // vite preview --port 4789 --strictPort
const browser = await chromium.launch();
const errs = [];
const p = await browser.newPage({ viewport: { width: 1440, height: 1000 } });
await p.addInitScript(() => { Object.defineProperty(navigator, 'hardwareConcurrency', { get: () => 8 }); });
p.on('pageerror', (e) => errs.push(e.message));
await p.goto(BASE + '/#seed=1337&x=0&z=0&zoom=-3');
await p.evaluate(() => (localStorage.clear(), localStorage.setItem('gtnh-seedmap:layers', JSON.stringify({ layers: { mode: 'advanced' } })))); await p.reload();
await p.waitForTimeout(1500);
for (let i = 0; i < 120; i++) { if ((await p.locator('.loadpill').count()) === 0) break; await p.waitForTimeout(500); }
await p.evaluate(() => [...document.querySelectorAll('.tabs button')].find((b) => b.textContent === 'Search').click());
await p.waitForTimeout(400);
const t0 = Date.now();
await p.evaluate(() => [...document.querySelectorAll('.dp-checks button')].find((b) => b.textContent === 'check all').click());
for (let i = 0; i < 1200; i++) {
  await p.waitForTimeout(1000);
  const st = await p.evaluate(() => [...document.querySelectorAll('.dp-checks button')].map((b) => b.textContent).join());
  if (!st.includes('stop')) break;
}
const rows = await p.evaluate(() => [...document.querySelectorAll('.dp-row')].map((r) => `${r.querySelector('.dp-load').textContent} | ${r.querySelector('.dp-chk').textContent} | ${r.querySelector('.dp-chk').title}`));
console.log(rows.join('\n'));
console.log('total s', (Date.now() - t0) / 1000);
console.log('errors:', errs.join('\n') || 'none');
await browser.close();
