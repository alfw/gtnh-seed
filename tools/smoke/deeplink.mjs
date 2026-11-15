// Deep links (self-hosted): a #seed=…&type=…&dim=…&x=…&z=…&zoom=… URL opens that view, the URL follows the view,
// Copy link carries the dimension, and the copied link reopens the same view.
import { createRequire } from 'module';
const require = createRequire(new URL('../../viewer/', import.meta.url));
const pw = require('playwright');
const BROWSER = process.env.SMOKE_BROWSER ?? 'chromium'; // chromium | webkit | firefox
const chromium = pw[BROWSER];
const BASE = process.env.SMOKE_URL ?? 'http://localhost:4789'; // vite preview --port 4789 --strictPort
const b = await chromium.launch();
const p = await b.newPage({ viewport: { width: 1400, height: 900 } });
const errs = [];
p.on('pageerror', (e) => errs.push(e.message));
const state = () => p.evaluate(() => ({
  seed: document.querySelector('input.seed, header input, .topbar input')?.value,
  dim: [...document.querySelectorAll('select')].find((s) => [...s.options].some((o) => o.textContent === 'Nether'))?.value,
  hash: location.hash,
}));
await p.goto(BASE + '/#seed=hello&type=RWG2&dim=-1&x=1234&z=-567&zoom=1');
await p.waitForTimeout(2500);
const s1 = await state();
const h1 = new URLSearchParams(s1.hash.slice(1));
const okOpen = s1.seed === 'hello' && s1.dim === '-1' && h1.get('type') === 'RWG2' && h1.get('x') === '1234' && h1.get('z') === '-567' && h1.get('zoom') === '1';
// Copy link (clipboard stubbed: headless browsers differ in clipboard permission handling)
await p.evaluate(() => { navigator.clipboard.writeText = async (t) => { window.__copied = t; }; });
await p.evaluate(() => [...document.querySelectorAll('button')].find((b) => /copy link/i.test(b.textContent))?.click());
await p.waitForTimeout(400);
const copied = await p.evaluate(() => window.__copied ?? '');
const c = new URLSearchParams(copied.split('#')[1] ?? '');
const okCopy = c.get('dim') === '-1' && c.get('x') === '1234' && c.get('type') === 'RWG2' && /^-?\d+$/.test(c.get('seed') ?? '');
// pan with the keyboard-free API: drag the map, the hash follows
const box = await (await p.$('canvas')).boundingBox();
await p.mouse.move(box.x + box.width / 2, box.y + box.height / 2); await p.mouse.down();
await p.mouse.move(box.x + box.width / 2 - 200, box.y + box.height / 2, { steps: 8 }); await p.mouse.up();
await p.waitForTimeout(800);
const h2 = new URLSearchParams((await state()).hash.slice(1));
const okFollow = Number(h2.get('x')) > 1234 && h2.get('dim') === '-1';
// reopen the copied link in a fresh page
const q = await b.newPage({ viewport: { width: 1400, height: 900 } });
q.on('pageerror', (e) => errs.push(e.message));
await q.goto(BASE + '/#' + copied.split('#')[1]); await q.waitForTimeout(2000);
const s3 = await q.evaluate(() => location.hash);
const okReopen = new URLSearchParams(s3.slice(1)).get('dim') === '-1' && new URLSearchParams(s3.slice(1)).get('x') === '1234';
console.log({ open: okOpen, copy: okCopy, follow: okFollow, reopen: okReopen, copied, s1 });
if (!(okOpen && okCopy && okFollow && okReopen)) errs.push('deep link check failed');
console.log('errors:', errs.length ? errs.join(' | ') : 'none');
await b.close();
process.exit(errs.length ? 1 : 0);
