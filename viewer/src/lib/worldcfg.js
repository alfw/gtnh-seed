// RWG2 world settings (`config/rwg2.cfg`, category "continental rwg"). Index = wasm `set_config`. Like the pack, the
// settings are fixed for the page's lifetime: Apply reloads with them in the URL (`cfg=`) and in local storage.
export const CFG_FIELDS = [
  { i: 0, key: 'minCont', name: 'Minimum Continent Width', def: 2700, min: 1, max: 100000, step: 100, unit: 'blocks', help: 'Distance from a continent’s Voronoi centre to its coast.' },
  { i: 1, key: 'maxCont', name: 'Maximum Continent Width', def: 2700, min: 1, max: 100000, step: 100, unit: 'blocks', help: 'Continents get a width between minimum and maximum.' },
  { i: 2, key: 'avgOcean', name: 'Average Ocean Width', def: 100, min: 1, max: 100000, step: 10, unit: 'blocks', help: 'Average ocean band; continent plus ocean width sets the Voronoi radius.' },
  { i: 3, key: 'minOcean', name: 'Minimum Ocean Width', def: 50, min: 0, max: 100000, step: 10, unit: 'blocks', help: 'Minimum water gap between two maximum-sized continents.' },
  { i: 4, key: 'oceanFrac', name: 'Maximum Ocean Fraction', def: 0.4, min: 0, max: 1, step: 0.05, help: 'Upper limit on the ocean share of the world (1 = off); lower values dilate continents.' },
  { i: 5, key: 'minIsland', name: 'Minimum Island Width', def: 300, min: 1, max: 100000, step: 50, unit: 'blocks' },
  { i: 6, key: 'maxIsland', name: 'Maximum Island Width', def: 600, min: 1, max: 100000, step: 50, unit: 'blocks' },
  { i: 7, key: 'islandChance', name: 'Island Placement Chance', def: 0.3, min: 0, max: 1, step: 0.05 },
  { i: 8, key: 'volcanoChance', name: 'Large Island Volcano Chance', def: 0.15, min: 0, max: 1, step: 0.05 },
  { i: 9, key: 'landmarks', name: 'Average Landmarks Per Type And Continent', def: 0.25, min: 0, max: 100, step: 0.05, help: 'Volcanoes and lava caves per continent.' },
  { i: 10, key: 'biomeSize', name: 'Biome Size Multiplier', def: 1, min: 0.25, max: 10, step: 0.25, help: 'Scales biome and climate sizes, like vanilla Large Biomes.' },
  { i: 11, key: 'taintPer', name: 'Per Continent Tainted Land', def: 1, bool: true, since: '2.0.9', help: 'Tainted Land from a per-continent budget (continents world type).' },
  { i: 12, key: 'taintAvg', name: 'Average Tainted Land Per Continent', def: 0.75, min: 0, max: 16, step: 0.25, since: '2.0.9', help: '0 removes Tainted Land; the game’s estimate runs high (1.0 gives about 2 patches).' },
];
const KEY = 'gtnh-seedmap.cfg';

/** `{key: value}` of the non-default values in a `cfg=` string (`key:value,…`), clamped. */
export function parseCfg(s) {
  const out = {};
  for (const part of (s ?? '').split(',')) {
    const [k, v] = part.split(':');
    const f = CFG_FIELDS.find((x) => x.key === k);
    const n = Number(v);
    if (!f || !Number.isFinite(n)) continue;
    const val = f.bool ? (n ? 1 : 0) : Math.min(f.max, Math.max(f.min, n));
    if (val !== f.def) out[k] = val;
  }
  return out;
}
export function cfgString(c) {
  return CFG_FIELDS.filter((f) => c[f.key] !== undefined && c[f.key] !== f.def).map((f) => `${f.key}:${c[f.key]}`).join(',');
}

function resolve() {
  try {
    const p = new URLSearchParams(location.hash.slice(1));
    if (p.has('cfg')) return parseCfg(p.get('cfg'));
    if (p.get('seed')) return {}; // a shared link without cfg = defaults
  } catch {}
  try {
    return parseCfg(localStorage.getItem(KEY));
  } catch {}
  return {};
}

/** Non-default settings in force on this page. */
export const CFG = resolve();
export const CFG_STR = cfgString(CFG);
export const CFG_CUSTOM = CFG_STR !== '';

export function cfgValue(c, f) {
  return c[f.key] ?? f.def;
}

/** Apply settings: remember them, put them in the URL (with the rest of `params`) and reload. */
export function switchConfig(c, params) {
  const s = cfgString(c);
  try {
    localStorage.setItem(KEY, s);
  } catch {}
  const p = new URLSearchParams(params);
  if (s) p.set('cfg', s);
  else p.delete('cfg');
  try {
    history.replaceState(null, '', `#${p}`);
  } catch {}
  location.reload();
}
