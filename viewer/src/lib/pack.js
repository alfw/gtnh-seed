// Supported packs (index = rwg2_biomes::pack::Pack / wasm `set_pack`). The pack is fixed for the page's lifetime:
// every worker instantiates the wasm with it, and switching reloads the page (all caches are per pack).
export const PACKS = [
  { index: 0, id: '2.9.0', label: 'GTNH 2.9.0', rwg2: '2.0.6', note: 'RWG2 2.0.6' },
  { index: 1, id: '2.9.1', label: 'GTNH 2.9.1', rwg2: '2.0.9', note: 'RWG2 2.0.9: Tainted Land per continent, one more dungeon item' },
];
/** No pack in the URL and none saved (incl. links from before the picker): the newest pack. */
export const DEFAULT_PACK = PACKS[PACKS.length - 1];
const KEY = 'gtnh-seedmap.pack';

function resolve() {
  let p = null;
  try {
    p = new URLSearchParams(location.hash.slice(1));
  } catch {}
  const byId = (id) => PACKS.find((k) => k.id === id);
  if (p?.get('pack')) return byId(p.get('pack')) ?? DEFAULT_PACK;
  try {
    const saved = byId(localStorage.getItem(KEY));
    if (saved) return saved;
  } catch {}
  return DEFAULT_PACK;
}

export const PACK = resolve();

/** Switch packs: remember it, put it in the URL (with the rest of `params`) and reload. */
export function switchPack(id, params) {
  try {
    localStorage.setItem(KEY, id);
  } catch {}
  const p = new URLSearchParams(params);
  p.set('pack', id);
  try {
    history.replaceState(null, '', `#${p}`);
  } catch {}
  location.reload();
}
