import { createRequire } from 'module';
const require = createRequire(new URL('../../viewer/', import.meta.url));
const pw = require('playwright');
const BROWSER = process.env.SMOKE_BROWSER ?? 'chromium'; // chromium | webkit | firefox
const chromium = pw[BROWSER];
const BASE = process.env.SMOKE_URL ?? 'http://localhost:4789'; // vite preview --port 4789 --strictPort
const browser = await chromium.launch({ args: BROWSER === 'chromium' ? ['--use-gl=swiftshader', '--enable-unsafe-swiftshader'] : [] });
const errs = [];
const p = await browser.newPage({ viewport: { width: 1440, height: 1000 } });
await p.addInitScript(() => { Object.defineProperty(navigator, 'hardwareConcurrency', { get: () => 8 }); });
p.on('pageerror', (e) => errs.push(e.message));
p.on('console', (m) => m.text().startsWith('lp') ? console.log(m.text()) : m.type() === 'error' && errs.push('console: ' + m.text()));
const seed = process.argv[2] ?? '1337', at = process.argv[3] ?? 'x=60&z=-40', dim = process.argv[4] ?? '';
await p.goto(`${BASE}/#seed=${seed}&${at}&zoom=1${dim}`);
await p.evaluate(() => (localStorage.clear(), localStorage.setItem('gtnh-seedmap:layers', JSON.stringify({ layers: { mode: 'advanced' } })))); await p.reload();
await p.waitForTimeout(1500);
for (let i = 0; i < 120; i++) { if ((await p.locator('.loadpill').count()) === 0) break; await p.waitForTimeout(500); }
await p.evaluate(() => [...document.querySelectorAll('.modes.tools button')].find((b) => b.textContent.includes('View')).click());
await p.waitForTimeout(300);
await p.mouse.click(560, 560);
await p.waitForTimeout(18000);
// long press low in the view (ground)
await p.mouse.move(900, 650); await p.mouse.down(); await p.waitForTimeout(700); console.log("during:", await p.locator(".sv-blockinfo").count()); await p.mouse.up();
await p.waitForTimeout(1500);
console.log('long:', await p.locator('.sv-blockinfo').innerText().catch(() => 'none'));
await p.screenshot({ path: '/tmp/svpick1.png' });
await p.mouse.click(+(process.argv[5] ?? 900), +(process.argv[6] ?? 650), { button: "right" });
await p.waitForTimeout(1500);
console.log('right:', await p.locator('.sv-blockinfo').innerText().catch(() => 'none'));
await p.screenshot({ path: '/tmp/svpick2.png' });
await p.mouse.click(300, 700); await p.waitForTimeout(500);
console.log('after tap:', await p.locator('.sv-blockinfo').count());
console.log('errors:', errs.join('\n') || 'none');
await browser.close();
