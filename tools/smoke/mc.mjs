import { createRequire } from 'module';
const require = createRequire(new URL('../../viewer/', import.meta.url));
const pw = require('playwright');
const BROWSER = process.env.SMOKE_BROWSER ?? 'chromium'; // chromium | webkit | firefox
const chromium = pw[BROWSER];
const BASE = process.env.SMOKE_URL ?? 'http://localhost:4789'; // vite preview --port 4789 --strictPort
const browser = await chromium.launch({ args: BROWSER === 'chromium' ? ['--use-gl=angle', '--use-angle=swiftshader'] : [] });
const errs = [];
const page = await browser.newPage({ viewport: { width: 1280, height: 800 } });
page.on('pageerror', (e) => errs.push(e.message));
page.on('console', (m) => (m.type() === 'error') && errs.push('console: ' + m.text()));
await page.addInitScript(() => Object.defineProperty(navigator, 'hardwareConcurrency', { get: () => 8 }));
await page.addInitScript(() => { if (!localStorage.getItem('gtnh-seedmap:layers')) localStorage.setItem('gtnh-seedmap:layers', JSON.stringify({ layers: { mode: 'advanced' } })); });
await page.goto(`${BASE}/#seed=1631302684446519836&x=-475&z=-268&zoom=2`);
await page.waitForTimeout(3000);
await page.locator('.tabs button', { hasText: 'Layers' }).click();
await page.locator('label', { hasText: 'Mineshaft chest carts' }).locator('input').check();
await page.waitForTimeout(12000);
const c = await page.locator('canvas').first().boundingBox();
await page.mouse.click(c.x + c.width / 2, c.y + c.height / 2);
await page.waitForTimeout(800);
await page.screenshot({ path: '/tmp/mc_map.png', timeout: 120000 });
console.log((await page.locator('.popup, .pop').first().innerText().catch(() => 'no popup')).slice(0, 600));
console.log('errors:', errs.slice(0, 8).join('\n') || 'none');
await browser.close();
