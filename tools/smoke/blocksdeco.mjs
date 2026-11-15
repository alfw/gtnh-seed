import { createRequire } from 'module';
const require = createRequire(new URL('../../viewer/', import.meta.url));
const pw = require('playwright');
const BROWSER = process.env.SMOKE_BROWSER ?? 'chromium'; // chromium | webkit | firefox
const chromium = pw[BROWSER];
const BASE = process.env.SMOKE_URL ?? 'http://localhost:4789'; // vite preview --port 4789 --strictPort
const browser = await chromium.launch({ args: BROWSER === 'chromium' ? ['--use-gl=swiftshader', '--enable-unsafe-swiftshader'] : [] });
const errs = [];
const p = await browser.newPage({ viewport: { width: 1200, height: 800 } });
await p.addInitScript(() => { Object.defineProperty(navigator, 'hardwareConcurrency', { get: () => 8 }); });
p.on('pageerror', (e) => errs.push(e.message));
p.on('console', (m) => m.type() === 'error' && errs.push('console: ' + m.text()));
const seed = process.argv[2] ?? '1631302684446519836', at = process.argv[3] ?? 'x=768&z=-512';
await p.goto(`${BASE}/#seed=${seed}&${at}&zoom=2`);
await p.evaluate(() => (localStorage.clear(), localStorage.setItem('gtnh-seedmap:layers', JSON.stringify({ layers: { mode: 'advanced' } })))); await p.reload();
await p.waitForTimeout(1500);
await p.evaluate(() => [...document.querySelectorAll('button')].find((b) => b.textContent.trim().startsWith('Blocks'))?.click());
const t0 = Date.now();
await p.waitForTimeout(+(process.argv[4] ?? 40000));
await p.screenshot({ path: process.argv[5] ?? '/tmp/bd1.png' });
console.log('errors:', errs.join('\n') || 'none', (Date.now() - t0) / 1000);
await browser.close();
