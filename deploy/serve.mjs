// Minimal static server for a built viewer with the production headers (deploy/headers.json) and precompressed
// files (deploy/precompress.mjs). For checking a deploy locally, e.g. the smoke tests against real headers:
//   node deploy/serve.mjs [dir] [port]   →   SMOKE_URL=http://localhost:4790 tools/smoke/run.sh
import { createServer } from 'node:http';
import { readFileSync, existsSync, statSync } from 'node:fs';
import { join, normalize, extname } from 'node:path';
const dir = process.argv[2] ?? new URL('../viewer/dist', import.meta.url).pathname;
const port = +(process.argv[3] ?? 4790);
const H = JSON.parse(readFileSync(new URL('./headers.json', import.meta.url), 'utf8'));
const TYPES = { '.html': 'text/html; charset=utf-8', '.js': 'text/javascript', '.css': 'text/css', '.wasm': 'application/wasm',
  '.svg': 'image/svg+xml', '.png': 'image/png', '.json': 'application/json', '.txt': 'text/plain; charset=utf-8', '.zip': 'application/zip' };
createServer((req, res) => {
  let path = decodeURIComponent(new URL(req.url, 'http://x').pathname);
  if (path.endsWith('/')) path += 'index.html';
  const file = normalize(join(dir, path));
  if (!file.startsWith(normalize(dir)) || !existsSync(file) || statSync(file).isDirectory()) { res.writeHead(404, H.all); return res.end('not found'); }
  const enc = req.headers['accept-encoding'] ?? '';
  let body = file, ce = null;
  if (/\bbr\b/.test(enc) && existsSync(file + '.br')) [body, ce] = [file + '.br', 'br'];
  else if (/\bgzip\b/.test(enc) && existsSync(file + '.gz')) [body, ce] = [file + '.gz', 'gzip'];
  const h = { ...H.all, ...(path.startsWith('/assets/') ? H.assets : H.other), 'Content-Type': TYPES[extname(file)] ?? 'application/octet-stream', Vary: 'Accept-Encoding' };
  if (ce) h['Content-Encoding'] = ce;
  res.writeHead(200, h);
  res.end(readFileSync(body));
}).listen(port, () => console.log(`serving ${dir} on http://localhost:${port}`));
