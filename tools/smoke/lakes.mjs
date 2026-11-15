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
await p.goto(BASE + '/#seed=1337&x=0&z=0&zoom=1');
await p.evaluate(() => (localStorage.clear(), localStorage.setItem('gtnh-seedmap:layers', JSON.stringify({ layers: { mode: 'advanced' } })))); await p.reload();
// console errors only after the reload (the first load's cancelled fetches log "Load failed" in WebKit)
p.on('console', (m) => m.type() === 'error' && errs.push('console: ' + m.text()));
await p.waitForTimeout(1500);
await p.evaluate(() => [...document.querySelectorAll('.tabs button')].find((b) => b.textContent === 'Layers').click());
await p.waitForTimeout(300);
await p.evaluate(() => { const l = [...document.querySelectorAll('label')].find((x) => x.textContent.includes('Underground lakes')); l.querySelector('input').click(); });
const t0 = Date.now(); let saw = false;
for (let i = 0; i < 120; i++) { const t = await p.evaluate(() => document.querySelector('.loadpill')?.textContent ?? ''); if (t.includes('lakes')) saw = true; if (saw && !t.includes('lakes')) break; await p.waitForTimeout(500); }
console.log('lakes pill seen', saw, 'done in', (Date.now() - t0) / 1000, 's');
await p.waitForTimeout(800);
await p.screenshot({ path: '/tmp/lakes.png' });
console.log('errors:', errs.join('\n') || 'none');
await browser.close();
