// viewer/public/og.png (1200×630 link preview): the map of seed 1337 with the UI hidden and a title strip.
// Needs a running preview (cd viewer && npx vite preview --port 4789 --strictPort); SMOKE_URL overrides.
import { createRequire } from 'module';
const require = createRequire(new URL('../viewer/', import.meta.url));
const { chromium } = require('playwright');
const BASE = process.env.SMOKE_URL ?? 'http://localhost:4789';
const b = await chromium.launch();
const p = await b.newPage({ viewport: { width: 1200, height: 760 }, deviceScaleFactor: 1 });
await p.goto(BASE + '/#seed=1337&x=-200&z=150&zoom=-2');
await p.evaluate(() => localStorage.setItem('gtnh-seedmap:layers', JSON.stringify({ layers: { mode: 'simple', sideHidden: true } })));
await p.reload();
await p.waitForTimeout(9000);
await p.addStyleTag({ content: `* { visibility: hidden !important; } canvas, .og, .og * { visibility: visible !important; }
  .og { position: fixed; left: 0; right: 0; bottom: 0; padding: 26px 40px; background: linear-gradient(transparent, rgba(10,13,17,.92) 35%); color: #e8edf2; font: 600 54px/1.1 system-ui, sans-serif; }
  .og small { display: block; font: 400 26px/1.4 system-ui, sans-serif; color: #b9c4cf; margin-top: 8px; }` });
await p.evaluate(() => { const d = document.createElement('div'); d.className = 'og'; d.innerHTML = 'GTNH Seed Map<small>Biomes, structures, ore veins and seed search for GT New Horizons 2.9+</small>'; document.body.append(d); });
await p.waitForTimeout(300);
// the map canvas starts below the (hidden) top bar: clip 1200×630 from its top
const top = await p.evaluate(() => Math.max(...[...document.querySelectorAll('canvas')].map((c) => c.getBoundingClientRect()).filter((r) => r.width >= 1000).map((r) => r.top)));
await p.evaluate((top) => { document.querySelector('.og').style.bottom = `${760 - top - 630}px`; }, top);
await p.screenshot({ path: new URL('../viewer/public/og.png', import.meta.url).pathname, clip: { x: 0, y: top, width: 1200, height: 630 } });
await b.close();
console.log('og.png written');
