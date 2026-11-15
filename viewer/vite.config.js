import { defineConfig } from 'vite';
import { svelte } from '@sveltejs/vite-plugin-svelte';
import { readFileSync, writeFileSync, existsSync } from 'node:fs';
import { createHash } from 'node:crypto';
import { resolve } from 'node:path';

const pkg = JSON.parse(readFileSync(new URL('./package.json', import.meta.url), 'utf8'));
// build id = start of the source bundle's sha256 (the bundle is deterministic: same sources → same id), so a bug
// report names the exact Corresponding Source; "dev" when the bundle has not been built
const zip = new URL('./public/source/gtnh-seedmap-src.zip', import.meta.url);
const sourceId = existsSync(zip) ? createHash('sha256').update(readFileSync(zip)).digest('hex').slice(0, 8) : 'dev';
const date = new Date(process.env.SOURCE_DATE_EPOCH ? +process.env.SOURCE_DATE_EPOCH * 1000 : Date.now()).toISOString().slice(0, 10);

// absolute link-preview tags need the public address: VITE_SITE_URL=https://… at build time (left out otherwise)
const site = (process.env.VITE_SITE_URL ?? '').replace(/\/?$/, '/');
const ogTags = {
  name: 'og-absolute',
  transformIndexHtml: () => (site === '/' ? [] : [
    { tag: 'meta', attrs: { property: 'og:url', content: site }, injectTo: 'head' },
    { tag: 'meta', attrs: { property: 'og:image', content: `${site}og.png` }, injectTo: 'head' },
    { tag: 'link', attrs: { rel: 'canonical', href: site }, injectTo: 'head' },
  ]),
};

// beta while the version has a -beta pre-release tag: "(beta)" in the page titles; the About page's
// <!--beta-->…<!--/beta--> note is kept, otherwise removed from the built copy (public/ files skip Vite's transforms)
const beta = /-beta\b/.test(pkg.version);
let outDir = 'dist';
const betaMarks = {
  name: 'beta-marks',
  configResolved: (c) => { outDir = c.build.outDir; },
  transformIndexHtml: (html) => (beta ? html.replace(/<title>(.*?)<\/title>/, '<title>$1 (beta)</title>').replace(/(property="og:title" content=")([^"]*)/, '$1$2 (beta)') : html),
  closeBundle() {
    if (beta) return;
    const f = resolve(outDir, 'about.html'); // outDir is absolute after configResolved
    if (existsSync(f)) writeFileSync(f, readFileSync(f, 'utf8').replace(/<!--beta-->[\s\S]*?<!--\/beta-->\n?/, ''));
  },
};

export default defineConfig({
  base: './',
  plugins: [svelte(), ogTags, betaMarks],
  worker: { format: 'es' },
  define: {
    __APP_VERSION__: JSON.stringify(pkg.version),
    __BUILD_DATE__: JSON.stringify(date),
    __SOURCE_ID__: JSON.stringify(sourceId),
  },
  build: { target: 'es2022', assetsInlineLimit: 0, chunkSizeWarningLimit: 600 }, // the lazy three.js street-view chunk is ~540 kB
});
