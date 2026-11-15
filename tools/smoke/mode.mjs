import { createRequire } from 'module';
const require = createRequire(new URL('../../viewer/', import.meta.url));
const pw = require('playwright');
const BROWSER = process.env.SMOKE_BROWSER ?? 'chromium'; // chromium | webkit | firefox
const chromium = pw[BROWSER];
const BASE = process.env.SMOKE_URL ?? 'http://localhost:4789'; // vite preview --port 4789 --strictPort
const browser = await chromium.launch();
const errs = [];
const p = await browser.newPage({ viewport: { width: 1440, height: 1000 } });
p.on('pageerror', (e) => errs.push(e.message));
await p.goto(BASE + '/#seed=1337&x=0&z=0&zoom=-3');
await p.evaluate(() => localStorage.clear()); await p.reload();
await p.waitForTimeout(2500);
const tabs = () => p.evaluate(() => [...document.querySelectorAll('.tabs button')].map((b) => b.textContent.trim()).filter(Boolean).join(','));
const labels = () => p.evaluate(() => [...document.querySelectorAll('.layer-list label')].map((l) => l.textContent.trim().slice(0, 28)));
console.log('fresh tabs:', await tabs());
await p.evaluate(() => [...document.querySelectorAll('.tabs button')].find((b) => b.textContent === 'Layers').click());
await p.waitForTimeout(300);
const simple = await labels();
console.log('simple layers:', simple.length, simple.some((l) => l.includes('GregTech')) ? 'HAS ORES (bad)' : 'no ores');
const simpleConds = await p.evaluate(() => [...document.querySelectorAll('select.what option')].map((o) => o.value));
await p.evaluate(() => [...document.querySelectorAll('.mode-row button')].find((b) => b.textContent === 'Advanced').click());
await p.waitForTimeout(500);
console.log('advanced tabs:', await tabs());
const adv = await labels();
console.log('advanced layers:', adv.length, adv.some((l) => l.includes('GregTech')) ? 'has ores' : 'NO ORES (bad)');
const advConds = await p.evaluate(() => [...document.querySelectorAll('select.what option')].map((o) => o.value));
console.log('search targets simple / advanced:', simpleConds.length, '/', advConds.length);
// back to simple, reload: choice kept
await p.evaluate(() => [...document.querySelectorAll('.mode-row button')].find((b) => b.textContent === 'Simple').click());
await p.reload();
await p.waitForTimeout(1500);
console.log('after reload tabs:', await tabs());
// phone: scale bar above the tab strip
const q = await browser.newPage({ viewport: { width: 390, height: 844 }, isMobile: BROWSER !== 'firefox', hasTouch: true });
q.on('pageerror', (e) => errs.push(e.message));
await q.goto(BASE + '/#seed=1337&x=0&z=0&zoom=-3');
await q.waitForTimeout(3000);
await q.screenshot({ path: '/tmp/phone-scale.png' });
console.log('errors:', errs.join('\n') || 'none');
await browser.close();
