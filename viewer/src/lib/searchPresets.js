// Dev presets for the Search tab: ready-made condition lists, from single conditions to relations.
// Conditions use the Search condition fields; biomes and veins are named (`biome` / `biomeB` = in-game biome name,
// `vein` = GT vein display name) and resolved to ids when loaded. `from` = index of the parent condition (relation).
// `check` = dev regression run: seeds `from` (default 1) .. +n, RWG2_CONTINENTS, `expect` = matches found there;
// any other count means the search (or the data / model under it) changed.

export const SEARCH_PRESETS = [
  // ------------------------------------------------------------------ simple: one condition
  { id: 'tainted', check: { n: 3000, expect: 795 }, group: 'Simple', name: 'Tainted Land ≤ 500', conds: [{ what: 'biome', biome: 'Tainted Land', r: 500 }] },
  // expect rebased 2026-11-10 for the v1.49 exact spawn climb (seed 6's spawn moved; tests/spawn.rs village_preset_spawn_drift)
  { id: 'village', check: { n: 160, expect: 116 }, group: 'Simple', name: 'Village ≤ 600 of spawn', center: 'spawn', conds: [{ what: 'village', r: 600 }] },
  { id: 'stronghold', check: { n: 3000, expect: 14 }, group: 'Simple', name: 'Stronghold ≤ 600', conds: [{ what: 'stronghold', r: 600 }] },
  { id: 'volcano', check: { n: 1000, expect: 52 }, group: 'Simple', name: 'Volcano ≤ 2500', conds: [{ what: 'volcano', r: 2500 }] },
  { id: 'oil', check: { n: 2000, expect: 1441 }, group: 'Simple', name: 'Oil / gas field ≥ 600 L/op ≤ 1000', conds: [{ what: 'fluid:-1', minLpo: 600, r: 1000 }] },
  { id: 'diamondVein', check: { n: 160, expect: 65 }, group: 'Simple', name: 'Graphite & Diamond vein ≤ 100 of spawn', center: 'spawn', conds: [{ what: 'vein', vein: 'Graphite & Diamond', r: 100 }] },
  { id: 'blaze', check: { n: 20000, expect: 4779 }, group: 'Simple', name: 'Nether: fortress with 2 blaze spawners ≤ 150', dim: -1, conds: [{ what: 'fortress', spawners: 2, r: 150 }] },
  { id: 'hydra', check: { n: 2000, expect: 762 }, group: 'Simple', name: 'Twilight: Hydra Lair ≤ 700', dim: 7, conds: [{ what: 'tf', feat: 12, r: 700 }] },
  { id: 'burning', check: { n: 20000, expect: 16259 }, group: 'Simple', name: 'End: Burning Mountains island ≤ 1750', dim: 1, conds: [{ what: 'endIsland', ibiome: 1, ivar: '', r: 1750 }] },

  // ------------------------------------------------------------------ combined: several conditions
  {
    id: 'starter', check: { n: 160, expect: 129 }, group: 'Combined', name: 'Spawn starter kit', center: 'spawn',
    desc: 'Village ≤ 1000, LootGames ≤ 800, Roguelike area ≤ 1000 and a mineshaft ≤ 400 of spawn',
    conds: [{ what: 'village', r: 1000 }, { what: 'lootgames', r: 800 }, { what: 'roguelike', r: 1000 }, { what: 'mineshaft', r: 400 }],
  },
  {
    id: 'flatRain', check: { n: 200, expect: 105 }, group: 'Combined', name: 'Flat rainforest (humidity ≥ 120%)',
    desc: 'Humidity ≥ 120% biome patch with ≥ 80% of it in one ±10 band, ≥ 128² flat, within 2000',
    conds: [{ what: 'climate', hmin: 120, hmax: '', tmin: '', tmax: '', flat: true, ftol: 10, fpct: 80, farea: 128, fland: true, r: 2000 }],
  },
  {
    id: 'thaum', check: { n: 100, expect: 22 }, group: 'Combined', name: 'Thaumcraft base',
    desc: 'Tainted Land ≤ 1000 with an aura node ≤ 150 of it, Magical Forest ≤ 4000',
    conds: [{ what: 'biome', biome: 'Tainted Land', r: 1000 }, { what: 'aspect', aspect: '', amin: 1, r: 150, from: '0' }, { what: 'biome', biome: 'Magical Forest', r: 4000 }],
  },
  {
    id: 'dryStronghold', check: { n: 160, expect: 110 }, group: 'Combined', name: 'Land stronghold, no volcano near spawn', center: 'spawn',
    desc: 'Stronghold portal room under land ≤ 800, no volcano ≤ 1000',
    conds: [{ what: 'strongholdSurface', ocean: false, r: 800 }, { what: 'volcano', r: 1000, not: true }],
  },
  {
    id: 'adj', check: { n: 300, expect: 34 }, group: 'Combined', name: 'Tropics next to Hot Desert ≤ 2000',
    conds: [{ what: 'adj', biome: 'Tropics', biomeB: 'Hot Desert', dist: 64, r: 2000 }],
  },
  {
    id: 'geode', check: { n: 40, expect: 35 }, group: 'Combined', name: 'Diamond geode ≤ 1000 + oil ≤ 1000',
    desc: 'Abyssal geode with ≥ 1 dark diamond, any oil / gas field ≥ 200 L/op (slow: ~6 seeds/s)',
    conds: [{ what: 'geode', gmin: 1, gkind: '', r: 1000 }, { what: 'fluid:-1', minLpo: 200, r: 1000 }],
  },

  // ------------------------------------------------------------------ relations: measured from another hit
  {
    id: 'villageStronghold', check: { n: 1000, expect: 311 }, group: 'Relations', name: 'Village with a stronghold ≤ 200 of it',
    conds: [{ what: 'village', r: 3000 }, { what: 'stronghold', r: 200, from: '0' }],
  },
  {
    id: 'quietVillage', check: { n: 160, expect: 70 /* rebased 2026-11-10: v1.49 spawn climb */ }, group: 'Relations', name: 'Village with no Roguelike ≤ 500 of it', center: 'spawn',
    conds: [{ what: 'village', r: 1500 }, { what: 'roguelike', r: 500, not: true, from: '0' }],
  },
  {
    id: 'twinVillage', check: { n: 1000, expect: 159 }, group: 'Relations', name: 'Two villages ≤ 520 apart',
    desc: 'A village within 3000 with another village ≤ 520 of it (same type = another one)',
    conds: [{ what: 'village', r: 3000 }, { what: 'village', r: 520, from: '0' }],
  },
  {
    id: 'flatVillage', check: { n: 200, expect: 115 }, group: 'Relations', name: 'Flat Hot Plains with a village ≤ 300',
    desc: 'Hot Plains patch ≥ 50% in a ±10 band (≥ 256² flat) within 2500, a village ≤ 300 of the flat spot',
    conds: [{ what: 'biome', biome: 'Hot Plains', flat: true, ftol: 10, fpct: 50, farea: 256, fland: true, r: 2500 }, { what: 'village', r: 300, from: '0' }],
  },
  {
    id: 'volcanoDiamond', check: { n: 500, expect: 42 }, group: 'Relations', name: 'Volcano with a Graphite & Diamond vein ≤ 250',
    conds: [{ what: 'volcano', r: 4000 }, { what: 'vein', vein: 'Graphite & Diamond', r: 250, from: '0' }],
  },
  {
    id: 'fortressPair', check: { n: 20000, expect: 18851 }, group: 'Relations', name: 'Nether: two fortresses ≤ 180 apart', dim: -1,
    conds: [{ what: 'fortress', spawners: 0, r: 1500 }, { what: 'fortress', spawners: 0, r: 180, from: '0' }],
  },
  {
    id: 'flatRainTainted', check: { n: 20000, expect: 3 }, group: 'Relations', name: 'Flat rainforest at 0, 0 with Tainted Land ≤ 1000',
    desc: 'Temperate Rainforest patch ≥ 95% in one ±5 band (≥ 128² flat, land) within 10 of 0, 0; Tainted Land ≤ 1000 of that spot (~1 in 5000 seeds)',
    conds: [
      { what: 'biome', biome: 'Temperate Rainforest', flat: true, ftol: 5, fpct: 95, farea: 128, fland: true, r: 10 },
      { what: 'biome', biome: 'Tainted Land', r: 1000, from: '0' },
    ],
  },
  {
    id: 'surfaceRedSand', check: { n: 60, expect: 24 }, group: 'Simple', name: 'Red sand within 500',
    desc: 'Exact surface block: red sand on top of a column within 500 of 0, 0 (mesa / canyon / red desert)',
    conds: [{ what: 'surface', sblock: 'redsand', smin: 1, r: 500 }],
  },
  {
    id: 'lavaLakeCave', check: { n: 60, expect: 13 }, group: 'Simple', name: 'Lava lake open to a cave ≤ 150',
    desc: 'Underground lava lake (RWG2 lakes are never at the surface) touching a cave, top at Y20 or higher, within 150 of 0, 0',
    conds: [{ what: 'lake', lliq: 'lava', lopen: true, lminy: 20, r: 150 }],
  },
  {
    id: 'cartDiamond', check: { n: 60, expect: 11 }, group: 'Simple', name: 'Mineshaft cart with a diamond ≤ 400',
    desc: 'Minecart chest in a mineshaft corridor holding a diamond, within 400 of 0, 0 (exact populate replay)',
    conds: [{ what: 'loot', lsrc: 'cart', litem: 'diamond', lcount: 1, r: 400 }],
  },
  {
    id: 'roomBook', check: { n: 60, expect: 3 }, group: 'Simple', name: 'Monster room with an enchanted book ≤ 200',
    desc: 'Vanilla monster room whose chest holds an enchanted book, within 200 of 0, 0',
    conds: [{ what: 'loot', lsrc: 'room', litem: 'book', lcount: 1, r: 200 }],
  },
  {
    id: 'surfaceSnowStart', check: { n: 60, expect: 14 }, group: 'Combined', name: 'Snowy start (≥ 2000 snow columns ≤ 200)',
    desc: 'At least 2000 columns with a snow layer within 200 of 0, 0 (surface pass + population snow)',
    conds: [{ what: 'surface', sblock: 'snow', smin: 2000, r: 200 }],
  },
];

export const PRESET_GROUPS = ['Simple', 'Combined', 'Relations'];
