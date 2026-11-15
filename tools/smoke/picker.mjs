// Searchable pickers in Search: filter + Enter picks the biome, the search runs with it; condition type picker
// switches the condition; Escape closes without changing.
import { createRequire } from 'module';
const require = createRequire(new URL('../../viewer/', import.meta.url));
const pw = require('playwright');
const BROWSER = process.env.SMOKE_BROWSER ?? 'chromium'; // chromium | webkit | firefox
const chromium = pw[BROWSER];
const BASE = process.env.SMOKE_URL ?? 'http://localhost:4789'; // vite preview --port 4789 --strictPort
const b = await chromium.launch();
const errs = [];
for (const phone of [false, true]) {
  const p = await b.newPage(phone ? { viewport: { width: 390, height: 844 }, isMobile: BROWSER !== 'firefox', hasTouch: true } : { viewport: { width: 1440, height: 1000 } });
  p.on('pageerror', (e) => errs.push(e.message));
  await p.goto(BASE + '/#seed=1337&x=0&z=0&zoom=-3');
  await p.evaluate(() => localStorage.setItem('gtnh-seedmap:layers', JSON.stringify({ layers: { mode: 'advanced' } }))); await p.reload();
  await p.waitForTimeout(2000);
  if (phone) await p.evaluate(() => document.querySelector('.grab')?.click());
  await p.evaluate(() => [...document.querySelectorAll('.tabs button')].find((b) => b.textContent.includes('Search'))?.click());
  await p.waitForTimeout(500);
  const open = (i) => p.evaluate((i) => document.querySelectorAll('.cond .picker .trigger')[i].click(), i);
  // biome picker: filter + Enter
  await open(1); await p.waitForTimeout(100);
  await p.keyboard.type('bamboo');
  await p.keyboard.press('Enter');
  const biome = await p.evaluate(() => document.querySelectorAll('.cond .picker .trigger .cur-label')[1].textContent);
  // Escape keeps the value
  await open(1); await p.waitForTimeout(100); await p.keyboard.type('xyz'); const none = await p.evaluate(() => !!document.querySelector('.picker .none'));
  await p.keyboard.press('Escape');
  const after = await p.evaluate(() => document.querySelectorAll('.cond .picker .trigger .cur-label')[1].textContent);
  // run the search
  await p.evaluate(() => document.querySelector('button.go').click());
  await p.waitForTimeout(6000);
  const status = await p.evaluate(() => [...document.querySelectorAll('.dim, span')].map((e) => e.textContent).find((t) => /checked ·/.test(t)));
  await p.evaluate(() => [...document.querySelectorAll('button')].find((b) => b.textContent.trim() === 'Stop')?.click());
  // condition type picker: arrow keys / click "Village"
  await open(0); await p.waitForTimeout(100); await p.keyboard.type('villag'); await p.keyboard.press('Enter');
  const what = await p.evaluate(() => document.querySelector('.cond .picker .trigger .cur-label').textContent);
  console.log(phone ? 'phone' : 'desk', '| biome:', biome, '| no-match shown:', none, '| after Esc:', after, '|', status, '| type:', what);
  if (biome !== 'Bamboo Forest' || after !== 'Bamboo Forest' || !none || what !== 'Village' || !/[1-9]\d* found/.test(status ?? '')) errs.push(`picker check failed (${phone ? 'phone' : 'desk'})`);
  await p.close();
}
console.log('errors:', errs.length ? errs.join(' | ') : 'none');
await b.close();
process.exit(errs.length ? 1 : 0);
