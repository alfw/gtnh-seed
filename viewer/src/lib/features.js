// Simple / advanced mode: one registry of what the advanced mode adds.
// Simple = biomes, terrain / blocks, spawn, strongholds, villages, dimensions (TF features, fortress positions,
// HEE islands and towers), markers, biome / structure / terrain searches. Advanced adds everything else below.
// Hidden layers are switched off in the layer flags the workers see, so they cost no worker time.

/** Feature layers only shown (and computed) in advanced mode. */
export const ADVANCED_LAYERS = [
  'mineshafts', 'mineChests', 'vdungeons', 'loot', 'rogue', 'geodes', 'lakes', 'oil',
  'tcRing', 'tcHilltop', 'tcMound', 'tcTotem', 'tcNodes', 'ores', 'fluids', 'deEnd',
];

/** Marker kinds that are detail of a simple layer (fortress pieces, chests, spawners, wart rooms). */
export const ADVANCED_KINDS = new Set(['fortressPiece', 'fchest', 'fchestSlot', 'spawner', 'wart', 'draconium']);

/** Search condition targets (`c.what`) only offered in advanced mode. */
const ADVANCED_COND = /^(mineshaft|geode|lake|loot|lootgames|roguelike|tc:|aspect|nodeType|fluid:|endComet|vein:)/;
export const isAdvancedCond = (what) => ADVANCED_COND.test(what ?? '');

/** Mode for a visitor without a saved choice: `VITE_DEFAULT_MODE` at build time, else advanced in dev builds and
 *  simple in release builds. */
export const DEFAULT_MODE = import.meta.env.VITE_DEFAULT_MODE ?? (import.meta.env.DEV ? 'advanced' : 'simple');

/** The layer flags the map / workers use: advanced-only layers off in simple mode. */
export function effectiveLayers(layers, advanced) {
  if (advanced) return { ...layers, advanced: true };
  const out = { ...layers, advanced: false };
  for (const k of ADVANCED_LAYERS) out[k] = false;
  return out;
}

/** Build information shown in the footer (vite `define`). */
export const BUILD = { version: __APP_VERSION__, date: __BUILD_DATE__, source: __SOURCE_ID__ };
/** Beta while `viewer/package.json`'s version carries a `-beta` pre-release tag (header badge, titles, PNG export). */
export const BETA = /-beta\b/.test(BUILD.version);
/** Developer tools (preset searches, "check all"): dev builds, or a release built with `VITE_DEV_TOOLS=1`
 *  (the owner's artifact). Off in a plain `npm run build` (public beta). */
export const DEV_TOOLS = import.meta.env.DEV || import.meta.env.VITE_DEV_TOOLS === '1';
/** Where "Report a bug" points (`VITE_ISSUES_URL`); the link is hidden when unset. */
export const ISSUES_URL = import.meta.env.VITE_ISSUES_URL ?? '';
