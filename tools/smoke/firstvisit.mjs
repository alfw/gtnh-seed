// First visit (no saved settings): Simple mode, footer shows the build, About page and source pages load,
// dev presets only when the build has VITE_DEV_TOOLS=1 (set SMOKE_EXPECT_DEV=1 for such a build).
import { createRequire } from 'module';
const require = createRequire(new URL('../../viewer/', import.meta.url));
const pw = require('playwright');
const BROWSER = process.env.SMOKE_BROWSER ?? 'chromium'; // chromium | webkit | firefox
const chromium = pw[BROWSER];
const BASE = process.env.SMOKE_URL ?? 'http://localhost:4789'; // vite preview --port 4789 --strictPort
const b = await chromium.launch();
const ctx = await b.newContext({ viewport: { width: 1400, height: 900 } }); // fresh storage
const p = await ctx.newPage();
const errs = [];
p.on('pageerror', (e) => errs.push(e.message));
await p.goto(BASE + '/#seed=1337');
await p.waitForTimeout(2500);
const r = await p.evaluate(async () => {
  const tab = (t) => [...document.querySelectorAll('.tabs button')].find((b) => b.textContent.includes(t));
  const ores = !!tab('Ores');
  const simple = !ores && [...document.querySelectorAll('button')].some((b) => b.textContent.trim() === 'Simple');
  // dev presets are an Advanced-mode, Search-tab panel
  tab('Layers')?.click();
  await new Promise((res) => setTimeout(res, 300));
  [...document.querySelectorAll('button')].find((b) => b.textContent.trim() === 'Advanced')?.click();
  await new Promise((res) => setTimeout(res, 300));
  tab('Search')?.click();
  await new Promise((res) => setTimeout(res, 400));
  return {
    simple,
    build: document.querySelector('footer .build')?.textContent ?? '',
    dev: !!document.querySelector('.devpresets'),
    favicon: document.querySelector('link[rel=icon]')?.getAttribute('href'),
    desc: !!document.querySelector('meta[name=description]'),
  };
});
const about = await (await p.request.get(BASE + '/about.html')).text();
const lic = await p.request.get(BASE + '/source/files/THIRD_PARTY_LICENSES.txt.txt');
const fav = await p.request.get(BASE + '/favicon.svg');
const expectDev = process.env.SMOKE_EXPECT_DEV === '1';
const ok = r.simple && /^v\d+\.\d+\.\d+(-[0-9A-Za-z.]+)? · \d{4}-\d\d-\d\d · [0-9a-f]{8}$/.test(r.build) && r.dev === expectDev && r.desc
  && about.includes('What is exact') && lic.ok() && (await lic.text()).includes('three') && fav.ok();
console.log({ ...r, expectDev, about: about.includes('What is exact'), licences: lic.status(), favicon: fav.status() });
if (!ok) errs.push('first-visit check failed');
console.log('errors:', errs.length ? errs.join(' | ') : 'none');
await b.close();
process.exit(errs.length ? 1 : 0);
