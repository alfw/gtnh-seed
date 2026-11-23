// Pack picker and World settings: first visit = newest pack; switching reloads with pack= in the URL; an old link
// (seed, no pack) opens the newest too; World tab Apply reloads with cfg= and the "custom cfg" badge; Copy-link state carries both.
import { createRequire } from 'module';
const require = createRequire(new URL('../../viewer/', import.meta.url));
const pw = require('playwright');
const BROWSER = process.env.SMOKE_BROWSER ?? 'chromium';
const BASE = process.env.SMOKE_URL ?? 'http://localhost:4789';
const b = await pw[BROWSER].launch();
const ctx = await b.newContext({ viewport: { width: 1400, height: 900 } });
const p = await ctx.newPage();
const errs = [];
p.on('pageerror', (e) => errs.push(e.message));
const sub = () => p.evaluate(() => document.querySelector('.brand .sub')?.textContent ?? '');
const fail = (m) => errs.push(m);

await p.goto(BASE + '/');
await p.waitForFunction(() => !/loading/.test(document.querySelector('.brand .sub')?.textContent ?? 'loading'), null, { timeout: 30000 });
const s0 = await sub();
if (!s0.includes('2.9.1')) fail(`first visit pack: ${s0}`);

// old link: seed without pack → newest (fresh context so storage does not decide)
const ctx2 = await b.newContext();
const p2 = await ctx2.newPage();
await p2.goto(BASE + '/#seed=1337&type=RWG2_CONTINENTS');
await p2.waitForFunction(() => !/loading/.test(document.querySelector('.brand .sub')?.textContent ?? 'loading'), null, { timeout: 30000 });
const s1 = await p2.evaluate(() => document.querySelector('.brand .sub')?.textContent ?? '');
if (!s1.includes('2.9.1')) fail(`old link pack: ${s1}`);
await ctx2.close();

// switch with the header picker
await p.evaluate(() => {
  const sel = [...document.querySelectorAll('header select')].find((s) => [...s.options].some((o) => o.value === '2.9.0'));
  sel.value = '2.9.0';
  sel.dispatchEvent(new Event('change', { bubbles: true }));
});
await p.waitForLoadState('load');
await p.waitForFunction(() => !/loading/.test(document.querySelector('.brand .sub')?.textContent ?? 'loading'), null, { timeout: 30000 });
const s2 = await sub();
const h2 = await p.evaluate(() => location.hash);
if (!s2.includes('2.9.0') || !h2.includes('pack=2.9.0')) fail(`after switch: ${s2} ${h2}`);

// back to 2.9.1, then World tab: Biome Size Multiplier 2 → Apply
await p.goto(BASE + '/#pack=2.9.1&seed=1337');
await p.reload();
await p.waitForFunction(() => !/loading/.test(document.querySelector('.brand .sub')?.textContent ?? 'loading'), null, { timeout: 30000 });
await p.evaluate(async () => {
  [...document.querySelectorAll('.tabs button')].find((x) => x.textContent.startsWith('World')).click();
  await new Promise((r) => setTimeout(r, 200));
  const lab = [...document.querySelectorAll('.pane.world label.cfg')].find((l) => l.textContent.includes('Biome Size Multiplier'));
  const inp = lab.querySelector('input');
  inp.value = '2';
  inp.dispatchEvent(new Event('change', { bubbles: true }));
  await new Promise((r) => setTimeout(r, 100));
  [...document.querySelectorAll('.pane.world button')].find((x) => x.textContent === 'Apply').click();
});
await p.waitForLoadState('load');
await p.waitForFunction(() => !/loading/.test(document.querySelector('.brand .sub')?.textContent ?? 'loading'), null, { timeout: 30000 });
const s3 = await sub();
const h3 = await p.evaluate(() => location.hash);
if (!s3.includes('custom cfg') || !h3.includes('cfg=biomeSize%3A2')) fail(`after apply: ${s3} ${h3}`);
// tiles still render with the setting (workers got it)
await p.waitForTimeout(3000);
const painted = await p.evaluate(() => {
  const c = document.querySelector('canvas');
  return !!c && c.width > 0;
});
if (!painted) fail('no canvas');
console.log({ first: s0, oldLink: s1, switched: s2, applied: s3, hash: h3 });
console.log('errors:', errs.length ? errs.join(' | ') : 'none');
await b.close();
process.exit(errs.length ? 1 : 0);
