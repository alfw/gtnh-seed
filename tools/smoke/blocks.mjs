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
p.on('console', (m) => m.type() === 'error' && errs.push('console: ' + m.text()));
await p.goto(BASE + '/#seed=1337&x=60&z=-40&zoom=1');
await p.evaluate(() => localStorage.clear()); await p.reload();
await p.waitForTimeout(1500);
for (let i = 0; i < 120; i++) { if ((await p.locator('.loadpill').count()) === 0) break; await p.waitForTimeout(500); }
await p.evaluate(() => [...document.querySelectorAll('.modes button')].find((b) => b.textContent === 'Blocks').click());
const t0 = Date.now();
await p.waitForTimeout(8000);
await p.screenshot({ path: '/tmp/blocks_z1.png' });
await p.mouse.move(720, 500); await p.waitForTimeout(600);
console.log('hover:', (await p.locator('.readout .biome').first().innerText().catch(() => 'none')));
await p.mouse.move(400, 300); await p.waitForTimeout(600);
console.log('hover2:', (await p.locator('.readout .biome').first().innerText().catch(() => 'none')));
// zoomed out
await p.goto(BASE + '/#seed=1337&x=0&z=0&zoom=-3'); await p.waitForTimeout(6000);
await p.screenshot({ path: '/tmp/blocks_zm3.png' });
console.log('errors:', errs.join('\n') || 'none');
await browser.close();
