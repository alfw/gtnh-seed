// Biome identity for players = the in-game biome (what F3 shows; what Thaumcraft/bees/GT checks): the base
// BiomeGenBase id. RWG2 maps several terrain variants onto one in-game biome (e.g. SnowHills, SnowRivers, SnowLakes,
// RedwoodSnow → "Snow Forest"), so the legend, colours and highlighting work per in-game biome; the RWG terrain
// variant is secondary detail.

export const LAVA_MARKER = 0xffff;

// RWG2's own base biomes have no colour set (0x000000) and a few others are unusable on a map; curated here.
const CURATED = {
  24: 0x18305e, // Deep Ocean
  307: 0x2d5a8a, // Cold Ocean
  308: 0xd9c78f, // Temperate Ocean → base of CoastDunes (beach strip)
  309: 0x2f6fa8, // Hot Ocean
  310: 0x2a7f9e, // Wet Ocean
  312: 0xeef3f7, // Snow Desert (Polar, CoastIce)
  313: 0xc4d6de, // Snow Forest
  314: 0x9db08a, // Cold Plains
  315: 0x3f6b4a, // Cold Forest
  316: 0xc2b25c, // Hot Plains
  317: 0x8e9a3c, // Hot Forest
  318: 0xe2c27a, // Hot Desert
  323: 0x4fbf8b, // Oasis
  324: 0x4f8a3e, // Temperate Forest
  325: 0xb56b3a, // Jungle Mesa
  326: 0x1f7a32, // High Rainforest
  327: 0x2e9a3a, // Jungle Hills
  328: 0x6f8f2a, // Jungle Canyon
  329: 0x2b5e2b, // Redwood Jungle
  54: 0x1e8fa6, // Coral Reef
  107: 0xb9cfd6, // Snowy Coniferous Forest (white)
  118: 0x4a3b3b, // Volcano
};
const LAVA_RGB = [255, 90, 31];
const OCEAN_IDS = new Set([24, 54, 73, 307, 309, 310]);

const rgbOf = (c) => [(c >> 16) & 255, (c >> 8) & 255, c & 255];

function hslToRgb(h, s, l) {
  const f = (n) => {
    const k = (n + h * 12) % 12;
    const a = s * Math.min(l, 1 - l);
    return Math.round(255 * (l - a * Math.max(-1, Math.min(k - 3, 9 - k, 1))));
  };
  return [f(0), f(8), f(4)];
}

/** Mod providing the in-game biome. */
export function modOf(baseClass = '') {
  if (baseClass.startsWith('biomesoplenty')) return "Biomes O' Plenty";
  if (baseClass.startsWith('thaumcraft')) return 'Thaumcraft';
  if (baseClass.startsWith('rwg2')) return 'RWG2';
  if (baseClass.startsWith('net.minecraft')) return 'Vanilla';
  return baseClass.split('.')[0] || '?';
}

const PLACEMENT_LABEL = {
  core: '',
  border: 'border',
  coldBorder: 'cold-side border',
  hotBorder: 'warm-side border',
  veryColdBorder: 'cold-side border',
  veryHotBorder: 'warm-side border',
  littoral: 'littoral',
  small: 'small patch',
  island: 'island',
  smallIsland: 'small island',
  largeIsland: 'large island',
  ocean: 'shallow',
  deepOcean: 'deep',
  coast: 'coast',
  volcano: 'volcano island',
};

/** RWG terrain-variant label for a provider biome (empty if it adds nothing to the in-game name). */
function variantName(b) {
  const cls = (b.className || '').split('.').pop().replace(/^RealisticBiome/, '');
  let name = b.name || (['Support', 'BOPOcean', 'Ocean'].includes(cls) ? '' : cls.replace(/([a-z])([A-Z])/g, '$1 $2'));
  if (name === b.baseName) name = '';
  const where = [...new Set(b.placements.map((p) => PLACEMENT_LABEL[p]).filter(Boolean))].join(', ');
  return [name, where].filter(Boolean).join(' · ');
}

/**
 * @param legend from the wasm module
 * @param mode 'game' (in-game colours, curated where missing) | 'distinct' (max-contrast categorical)
 * @returns {{ lut: Uint32Array, byId: Map<number, any>, groups: Map<number, any> }}
 *   lut: rwg id → ABGR; byId: rwg id → variant; groups: in-game id → { genId, name, mod, rgb, rwgIds, variants }
 */
export function buildPalette(legend, mode = 'game') {
  const groups = new Map();
  for (const b of legend.biomes) {
    let g = groups.get(b.baseId);
    if (!g) {
      g = { genId: b.baseId, name: b.baseName, mod: modOf(b.baseClass), color: b.color, rwgIds: [], variants: [], bee: b.forestry ? b.forestry[0] | (b.forestry[1] << 4) : null };
      groups.set(b.baseId, g);
    }
    g.rwgIds.push(b.id);
    const v = variantName(b);
    if (v) g.variants.push(v);
  }
  const m = legend.lavaMarker;
  groups.set(m.genId, { genId: m.genId, name: m.name, mod: "Biomes O' Plenty", rwgIds: [LAVA_MARKER], variants: ['lava cave marker'], color: 0, lava: true, bee: m.forestry ? m.forestry[0] | (m.forestry[1] << 4) : null });

  const sorted = [...groups.values()].sort((a, b) => a.genId - b.genId);
  let landIndex = 0;
  for (const g of sorted) {
    if (g.lava) g.rgb = LAVA_RGB;
    else if (mode === 'distinct' && !OCEAN_IDS.has(g.genId)) {
      const h = (landIndex++ * 0.618033988749895) % 1;
      const l = [0.5, 0.64, 0.38][landIndex % 3];
      g.rgb = hslToRgb(h, 0.62, l);
    } else g.rgb = rgbOf(CURATED[g.genId] ?? g.color);
  }

  const lut = new Uint32Array(65536).fill(0xff000000);
  const byId = new Map();
  for (const b of legend.biomes) {
    const g = groups.get(b.baseId);
    byId.set(b.id, { id: b.id, genId: g.genId, group: g, variant: variantName(b) });
  }
  byId.set(LAVA_MARKER, { id: LAVA_MARKER, genId: m.genId, group: groups.get(m.genId), variant: 'lava cave marker' });
  for (const [id, e] of byId) {
    const c = e.group.rgb;
    lut[id] = (255 << 24) | (c[2] << 16) | (c[1] << 8) | c[0];
  }
  return { lut, byId, groups };
}

// ---- Forestry bee climate (render mode 1): code = temperature (1..6) | humidity (0..2) << 4 ----------------
export const BEE_TEMPS = ['', 'Icy', 'Cold', 'Normal', 'Warm', 'Hot', 'Hellish'];
export const BEE_HUMIDITY = ['Arid', 'Normal', 'Damp'];
const TEMP_RGB = [null, [168, 216, 255], [84, 160, 214], [98, 184, 72], [230, 196, 70], [236, 128, 52], [192, 48, 40]];

/** RGB for a climate code: hue from temperature; arid = washed out toward sand, damp = deeper. */
export function beeRgb(code) {
  const t = code & 15, h = (code >> 4) & 15;
  const base = TEMP_RGB[t];
  if (!base) return [20, 22, 26];
  if (h === 0) return base.map((v, i) => Math.round(v * 0.55 + [232, 220, 190][i] * 0.45));
  if (h === 2) return base.map((v) => Math.round(v * 0.62));
  return base;
}

export const beeName = (code) => `${BEE_TEMPS[code & 15] || '?'} · ${BEE_HUMIDITY[(code >> 4) & 15] ?? '?'}`;

export const BEE_CODES = [];
for (let t = 1; t <= 6; t++) for (let h = 0; h <= 2; h++) BEE_CODES.push(t | (h << 4));

export const FORESTRY_LUT = (() => {
  const lut = new Uint32Array(65536).fill(0xff1a1614);
  for (const code of BEE_CODES) {
    const c = beeRgb(code);
    lut[code] = (255 << 24) | (c[2] << 16) | (c[1] << 8) | c[0];
  }
  return lut;
})();
