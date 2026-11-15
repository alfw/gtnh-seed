# Deploying the viewer

The viewer is a static site (`viewer/dist`): HTML, JS, CSS, one `.wasm`, the source pages. No server code.

## GitHub Pages (simplest)

`.github/workflows/pages.yml` tests, builds and deploys on every push to `main` / `master`
(Settings → Pages → Source: GitHub Actions). It sets `VITE_SITE_URL` (link previews), `VITE_SOURCE_URL` (the exact
commit) and `VITE_ISSUES_URL` (the repo's issue form, pre-filled from the viewer's "Report a bug" link). Pages serves
`.wasm` with the right type and compresses on the fly; it cannot send the CSP / cache headers below.

## Own server (nginx or Caddy)

```sh
cd viewer
VITE_SITE_URL=https://seedmap.example/ VITE_ISSUES_URL=https://github.com/OWNER/REPO/issues npm run build
node ../deploy/precompress.mjs          # .br + .gz next to each file (6.4 MB → 1.4 MB brotli)
rsync -a --delete dist/ server:/srv/gtnh-seedmap/
```

`nginx.conf` / `Caddyfile`: precompressed files, `application/wasm`, `immutable` caching for `assets/` (file names
carry content hashes), `no-cache` for the rest, and a strict Content-Security-Policy. `headers.json` is the source of
truth for the headers; both configs repeat it. Change all three together.

## Checking a deploy locally

```sh
node deploy/serve.mjs                    # viewer/dist with the production headers, http://localhost:4790
SMOKE_URL=http://localhost:4790 tools/smoke/run.sh   # incl. csp.mjs: 0 policy violations expected
```

Don't publish the `.br` / `.gz` files to the claude.ai artifact (it compresses itself); build again without
`precompress.mjs` for that.
