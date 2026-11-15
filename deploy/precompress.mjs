// Writes .br and .gz next to every compressible file of a built site (default viewer/dist), for servers that
// serve precompressed files (nginx gzip_static / brotli_static, Caddy precompressed, deploy/serve.mjs).
// Usage: node deploy/precompress.mjs [dir]
import { readdirSync, readFileSync, writeFileSync, statSync } from 'node:fs';
import { join } from 'node:path';
import { brotliCompressSync, gzipSync, constants } from 'node:zlib';
const dir = process.argv[2] ?? new URL('../viewer/dist', import.meta.url).pathname;
const EXT = /\.(html|js|css|wasm|svg|json|txt|map)$/;
let n = 0, raw = 0, br = 0;
const walk = (d) => {
  for (const f of readdirSync(d)) {
    const p = join(d, f);
    if (statSync(p).isDirectory()) { walk(p); continue; }
    if (!EXT.test(f)) continue;
    const b = readFileSync(p);
    if (b.length < 1024) continue;
    const bz = brotliCompressSync(b, { params: { [constants.BROTLI_PARAM_QUALITY]: 11, [constants.BROTLI_PARAM_SIZE_HINT]: b.length } });
    writeFileSync(p + '.br', bz);
    writeFileSync(p + '.gz', gzipSync(b, { level: 9 }));
    n++; raw += b.length; br += bz.length;
  }
};
walk(dir);
console.log(`${n} files: ${(raw / 1e6).toFixed(2)} MB → brotli ${(br / 1e6).toFixed(2)} MB`);
