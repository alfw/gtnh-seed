// Content-Security-Policy check against a server that sends the production headers (node deploy/serve.mjs):
// map + every layer, a search, the 3D view and a PNG export must run without a single policy violation.
//   node deploy/serve.mjs & SMOKE_URL=http://localhost:4790 node tools/smoke/csp.mjs
import { createRequire } from 'module';
const require = createRequire(new URL('../../viewer/', import.meta.url));
const pw = require('playwright');
const BROWSER = process.env.SMOKE_BROWSER ?? 'chromium'; // chromium | webkit | firefox
const chromium = pw[BROWSER];
const BASE = process.env.SMOKE_URL ?? 'http://localhost:4790'; // deploy/serve.mjs
const b = await chromium.launch({ args: BROWSER === 'chromium' ? ['--use-gl=swiftshader', '--enable-unsafe-swiftshader'] : [] });
const p = await b.newPage({ viewport: { width: 1400, height: 900 }, acceptDownloads: true });
const errs = [], viol = [];
p.on('pageerror', (e) => errs.push(e.message));
p.on('console', (m) => { if (/content.security.policy|refused to/i.test(m.text())) viol.push(m.text().slice(0, 200)); });
await p.addInitScript(() => document.addEventListener('securitypolicyviolation', (e) => (window.__csp ??= []).push(`${e.violatedDirective} ${e.blockedURI}`)));
const head = await p.request.get(BASE + '/');
const csp = head.headers()['content-security-policy'] ?? '';
if (!csp) { console.log('skipped: the server sends no CSP header (vite preview); run against node deploy/serve.mjs'); console.log('errors: none'); await b.close(); process.exit(0); }
await p.goto(BASE + '/#seed=1337&x=0&z=0&zoom=0');
await p.evaluate(() => localStorage.setItem('gtnh-seedmap:layers', JSON.stringify({ layers: { mode: 'advanced' } }))); await p.reload();
await p.waitForTimeout(2500);
// every layer preset "Everything"
await p.evaluate(() => [...document.querySelectorAll('.tabs button')].find((b) => b.textContent.includes('Layers'))?.click());
await p.waitForTimeout(300);
await p.evaluate(() => [...document.querySelectorAll('button')].find((b) => b.textContent.trim() === 'Everything')?.click());
await p.waitForTimeout(8000);
// a search
await p.evaluate(() => [...document.querySelectorAll('.tabs button')].find((b) => b.textContent.includes('Search'))?.click());
await p.waitForTimeout(300);
await p.evaluate(() => document.querySelector('button.go')?.click());
await p.waitForTimeout(4000);
await p.evaluate(() => [...document.querySelectorAll('button')].find((b) => b.textContent.trim() === 'Stop')?.click());
// PNG export
await p.evaluate(() => [...document.querySelectorAll('button')].find((b) => /PNG/.test(b.textContent))?.click());
await p.waitForTimeout(400);
const dl = p.waitForEvent('download', { timeout: 15000 }).catch(() => null);
await p.evaluate(() => [...document.querySelectorAll('button')].find((b) => b.textContent.includes('Save PNG'))?.click());
const file = await dl;
// 3D view
await p.keyboard.press('Escape');
await p.evaluate(() => [...document.querySelectorAll('.modes.tools button')].find((b) => b.textContent.includes('View'))?.click());
await p.waitForTimeout(300);
await p.mouse.click(560, 560);
await p.waitForTimeout(6000);
const sv = await p.evaluate(() => !!document.querySelector('.sv-stage canvas'));
viol.push(...(await p.evaluate(() => window.__csp ?? [])));
console.log({ csp: csp.slice(0, 60) + '…', violations: viol.length, png: !!file, view3d: sv });
for (const v of viol.slice(0, 10)) console.log('  violation:', v);
if (viol.length) errs.push(`${viol.length} CSP violations`);
console.log('errors:', errs.length ? errs.join(' | ') : 'none');
await b.close();
process.exit(errs.length ? 1 : 0);
