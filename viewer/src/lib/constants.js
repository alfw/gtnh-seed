// Viewer constants and pure helpers shared by App.svelte (split out of it; no component state in here).
export const CLIMATES = ['Ocean', 'Snow', 'Cold', 'Hot', 'Wet'];

export const WORLD_TYPES = [
  { id: 1, label: 'RWG2_CONTINENTS (default)' },
  { id: 0, label: 'RWG2' },
];

export const DIMS = [{ id: 0, label: 'Overworld' }, { id: -1, label: 'Nether' }, { id: 7, label: 'Twilight Forest' }, { id: 1, label: 'The End' }];

export const GLOBAL_MARKS_KEY = 'gtnh-seedmap:markers:global';

// phone layout (narrow or short screens): panels collapse to one-line chips, one sheet open at a time
export const PHONE_MQ = '(max-width: 720px), (max-height: 480px)';

// marker types: icon + colour. Portals link across dimensions (vanilla Teleporter: overworld ÷8 / Nether ×8,
// existing-portal search ±128 blocks in 1.7.10; Twilight Forest TFTeleporter: x/z 1:1, spiral search up to 200).
export const MARK_TYPES = {
  pin: { name: 'Pin', icon: '📍', color: '#e8a33d' },
  base: { name: 'Base / home', icon: '🏠', color: '#4fc3f7' },
  portal: { name: 'Nether portal', icon: '🟪', color: '#b388ff' },
  tfportal: { name: 'Twilight portal', icon: '🌿', color: '#69f0ae' },
  note: { name: 'Note', icon: '📝', color: '#fff176' },
  loot: { name: 'Loot / resource', icon: '💎', color: '#ff8a65' },
};

export const PORTAL_LINKS = {
  portal: { range: 128, to: { 0: { dim: -1, f: 1 / 8 }, '-1': { dim: 0, f: 8 } } },
  tfportal: { range: 200, to: { 0: { dim: 7, f: 1 }, 7: { dim: 0, f: 1 } } },
};

export const DIM_LABEL = { 0: 'Overworld', '-1': 'Nether', 7: 'Twilight Forest', 1: 'The End' };

// street-view settings (persisted): biome tint + legend scope, marker groups, water, minimap
export const SV_DEFAULTS = {
  tint: false, heightCol: false, legend: 'near', water: true, trees: true, mini: true, miniBig: false, miniZoom: -2, miniTerrain: false, panel: false,
  groups: { pins: true, villages: true, landmarks: true, dungeons: true, mineshafts: false, dimFeatures: true, thaum: true, nodes: false, geodes: false, lakes: false, ores: false, fluids: false },
  xray: { on: false, caves: true, pop: true, structures: true, ores: true, rooms: true },
};

export const LAYER_DEFAULTS = { strongholds: true, spawn: true, villages: true, mineshafts: false, dimFeatures: true, tcNodes: false, geodes: false, lakes: false, lakesLava: false, oil: true, vdungeons: false, mineChests: false, endKills: 0, deEnd: true, tfHideHills: false, loot: true, rogue: true, rogueLikely: true, tcRing: true, tcHilltop: true, tcMound: true, tcTotem: true, relief: false, blockDeco: true, biomeOverlay: 0, terrainPreset: 'default', ores: true, fluids: false, hits: true, pins: true, grid: true, groupMarkers: true, groupPx: 32, sideW: 300, sideHidden: false, markSort: 'added', devPresetsOpen: false };

export const NODE_KINDS = [['wild', 'Wild'], ['silverwood', 'Silverwood'], ['ring', 'Obelisk'], ['totem', 'Totem'], ['hilltop', 'Hilltop'], ['mound', 'Mound']];

export const NODE_TYPES = ['Normal', 'Pure', 'Dark', 'Unstable', 'Hungry', 'Tainted'];

// ---- worldgen features (LootGames, Roguelike): fetched per 2048-block cell, cached per seed/type ----------
export const CELL = 2048;

// aura nodes: smaller jobs (~1 s each) so results appear progressively and spread over the workers
export const NODE_CELL = 1024;

// dungeon chest item names (unlocalized names from the pack dump; display names for the GT meta items pending)
export const LOOT_NAMES = {
  'item.saddle@0': 'Saddle', 'item.ingotIron@0': 'Iron Ingot', 'item.bread@0': 'Bread', 'item.wheat@0': 'Wheat', 'item.sulphur@0': 'Gunpowder',
  'item.string@0': 'String', 'item.bucket@0': 'Bucket', 'item.appleGold@0': 'Golden Apple', 'item.redstone@0': 'Redstone', 'item.record@0': 'Music Disc',
  'item.nameTag@0': 'Name Tag', 'item.horsearmorgold@0': 'Gold Horse Armor', 'item.horsearmormetal@0': 'Iron Horse Armor', 'item.horsearmordiamond@0': 'Diamond Horse Armor',
  'item.helmetChain@0': 'Chain Helmet', 'item.leggingsChain@0': 'Chain Leggings', 'item.bootsChain@0': 'Chain Boots', 'item.chestplateChain@0': 'Chain Chestplate',
  'item.helmetDiamond@0': 'Diamond Helmet', 'item.chestplateDiamond@0': 'Diamond Chestplate', 'item.leggingsDiamond@0': 'Diamond Leggings', 'item.bootsDiamond@0': 'Diamond Boots',
  'item.doorIron@0': 'Iron Door', 'item.blazeRod@0': 'Blaze Rod', 'item.emerald@0': 'Emerald', 'item.ItemThaumonomicon@0': 'Thaumonomicon',
  'item.ItemSwordThaumium@0': 'Thaumium Sword', 'item.ItemPickThaumium@0': 'Thaumium Pickaxe', 'item.ItemAxeThaumium@0': 'Thaumium Axe', 'item.ItemHoeThaumium@0': 'Thaumium Hoe',
  'item.ItemLootBag@0': 'Loot Bag (common)', 'item.ItemLootBag@1': 'Loot Bag (uncommon)', 'item.ItemLootBag@2': 'Loot Bag (rare)', 'item.ItemAmuletVis@0': 'Amulet of Vis',
  'item.railcraft.tool.steel.sword@0': 'Steel Sword', 'item.railcraft.armor.steel.helmet@0': 'Steel Helmet', 'item.railcraft.armor.steel.plate@0': 'Steel Chestplate',
  'item.railcraft.armor.steel.legs@0': 'Steel Leggings', 'item.railcraft.armor.steel.boots@0': 'Steel Boots', 'item.extrautils:divisionSigil@0': 'Division Sigil (inactive)',
  'item.ItemResource@2': 'Thaumium Ingot', 'item.ItemResource@6': 'Amber', 'item.ItemResource@9': 'Knowledge Fragment', 'tile.blockCustomPlant@2': 'Shimmerleaf',
  'tile.blockCustomPlant@3': 'Cinderpearl', 'item.ItemRingRunic@0': 'Runic Ring', 'item.knowledgeNote@0': 'Knowledge Note', 'item.standardBindingAgent@0': 'Standard Binding Agent',
  'item.for.beeDroneGE@0': 'Bee Drone', 'item.avaritia_stew@0': 'Ultimate Stew', 'item.avaritia_meatballs@0': 'Cosmic Meatballs', 'item.spagettiandmeatballsItem@0': 'Spaghetti and Meatballs',
  'item.deluxecheeseburgerItem@0': 'Deluxe Cheeseburger', 'item.delightedmealItem@0': 'Delighted Meal', 'item.heartybreakfastItem@0': 'Hearty Breakfast', 'item.curryItem@0': 'Curry',
  'item.rainbowcurryItem@0': 'Rainbow Curry', 'item.supremepizzaItem@0': 'Supreme Pizza', 'item.baconmushroomburgerItem@0': 'Bacon Mushroom Burger', 'item.sausageinbreadItem@0': 'Sausage in Bread',
  'item.beefwellingtonItem@0': 'Beef Wellington', 'item.epicbaconItem@0': 'Epic Bacon', 'item.draconicevolution:dezilsMarshmallow@0': "Dezil's Marshmallow",
};

// GregTech material ids (GT5-Unofficial MaterialsIDMap) of the dungeon table's dusts/gems/ingots
export const GT_MAT = { 18: 'Magnesium', 19: 'Aluminium', 30: 'Chrome', 31: 'Manganese', 33: 'Cobalt', 34: 'Nickel', 35: 'Copper', 36: 'Zinc', 48: 'Molybdenum', 54: 'Silver', 57: 'Tin', 58: 'Antimony', 67: 'Neodymium', 89: 'Lead', 300: 'Bronze', 302: 'Invar', 303: 'Electrum', 304: 'Cast Iron', 305: 'Steel', 306: 'Stainless Steel', 307: 'Pig Iron', 308: 'Red Alloy', 310: 'Cupronickel', 335: 'Damascus Steel', 343: 'Cobalt Brass', 354: 'Magnetic Iron', 365: 'Electrical Steel', 382: 'Ardite', 502: 'Ruby', 503: 'Sapphire', 504: 'Green Sapphire', 505: 'Olivine', 527: 'Red Garnet', 528: 'Yellow Garnet' };

/** Search "Chest loot" items: [key, label, unlocalized-name fragments (any of; [] = any chest, null = custom text)]. */
export const LOOT_ITEMS = [
  ['any', 'any chest', []],
  ['diamond', 'Diamond', ['item.diamond@']],
  ['book', 'Enchanted Book', ['item.enchantedBook']],
  ['emerald', 'Emerald', ['item.emerald@']],
  ['gems', 'GT gems (ruby, sapphire, olivine, garnet)', ['gt.metaitem.01@850', 'gt.metaitem.01@8527', 'gt.metaitem.01@8528']],
  ['gapple', 'Golden Apple (monster rooms)', ['item.appleGold']],
  ['temple', 'Temple Caller', ['item.templeCaller']],
  ['thaumonomicon', 'Thaumonomicon', ['item.ItemThaumonomicon']],
  ['thaumium', 'Thaumium ingot / tools', ['item.ItemResource@2', 'Thaumium']],
  ['rings', 'Thaumcraft rings / Vis Stone', ['item.ItemRingRunic', 'item.ItemBaubleBlanks@3', 'item.ItemBaubleBlanks@4', 'item.ItemBaubleBlanks@5', 'item.ItemBaubleBlanks@6', 'item.ItemBaubleBlanks@7', 'item.ItemBaubleBlanks@8', 'item.ItemAmuletVis']],
  ['lootbag', 'Rare Treasure bag', ['item.ItemLootBag@2']],
  ['sigil', 'Division Sigil', ['divisionSigil']],
  ['onering', 'The One Ring (GTNH 2.9.1)', ['item.EMT.item.bauble@0']],
  ['disc', 'Music disc (monster rooms)', ['item.record']],
  ['diamondArmor', 'Diamond armour (monster rooms)', ['Diamond@']],
  ['horse', 'Horse armour', ['item.horsearmor']],
  ['saddle', 'Saddle', ['item.saddle']],
  ['nametag', 'Name Tag (monster rooms)', ['item.nameTag']],
  ['iron', 'Iron ingot', ['item.ingotIron']],
  ['gold', 'Gold ingot', ['item.ingotGold']],
  ['redstone', 'Redstone', ['item.redstone']],
  ['lapis', 'Lapis (carts)', ['item.dyePowder@4']],
  ['steel', 'GT steel ingot', ['gt.metaitem.01@11305']],
  ['custom', 'custom name fragment…', null],
];

/** mineshaft chest-cart items worth a gold dot (weight ≤ 3 picks: diamonds, books, Thaumcraft / Ender gear, Temple Caller, …) */
export const MINE_RARE = /diamond|enchantedBook|templeCaller|ItemLootBag@2|ItemThaumonomicon|Thaumium|ItemRingRunic|ItemBaubleBlanks@[3-8]|ItemAmuletVis|baubleEnderDisjunction|divisionSigil|technicolor|dezilsMarshmallow/;
export const LOOT_RARE = /enchantedBook|Diamond|appleGold|divisionSigil|ItemLootBag@2|ItemAmuletVis|Thaumonomicon|horsearmordiamond|record/;

export const OTHER_DIM = { 0: { dim: -1, name: 'Nether', f: 1 / 8 }, '-1': { dim: 0, name: 'Overworld', f: 8 } };

export const SV_GROUPS = [
  ['pins', 'Pins & search hits', 'all'], ['villages', 'Villages', 0], ['landmarks', 'Spawn & strongholds', 0], ['dungeons', 'LootGames & Roguelike', 0],
  ['mineshafts', 'Mineshafts', 0], ['dimFeatures', 'Fortresses & spawners', -1], ['dimFeatures', 'Twilight Forest features', 7], ['dimFeatures', 'HEE islands, towers & comets', 1], ['thaum', 'Thaumcraft structures', 0],
  ['nodes', 'Aura nodes', 'all'], ['geodes', 'Abyssal geodes', 0], ['lakes', 'Underground lakes', 0], ['ores', 'Ore veins', 'all'], ['fluids', 'Oil fields', 0],
];

export const SV_GROUP_OF = { pin: 'pins', hit: 'pins', village: 'villages', stronghold: 'landmarks', spawn: 'landmarks', loot: 'dungeons', rogue: 'dungeons', mineshaft: 'mineshafts', fortress: 'dimFeatures', spawner: 'dimFeatures', fchest: 'dimFeatures', wart: 'dimFeatures', tfFeature: 'dimFeatures', endIsland: 'dimFeatures', endTower: 'dimFeatures', deComet: 'dimFeatures', deChaos: 'dimFeatures', draconium: 'ores', node: 'nodes', geode: 'geodes', lake: 'lakes', oil: 'fluids', vdungeon: 'dungeons', minechest: 'mineshafts', ore: 'ores', fluid: 'fluids' };

export const COMPASS = ['N', 'NE', 'E', 'SE', 'S', 'SW', 'W', 'NW'];

// ---- marker tooltip ------------------------------------------------------------------------------------------
export const KIND_NAMES = {
  stronghold: 'Stronghold', hit: 'Search hit', loot: 'LootGames dungeon', rogue: 'Roguelike dungeon',
  rogueArea: 'Roguelike search area', tcRing: 'Eldritch obelisk', tcHilltop: 'Hilltop stones', tcMound: 'Barrow mound',
  tcTotem: 'Totem', node: 'Thaumcraft aura node', geode: 'Abyssal geode', lake: 'Underground lake', oil: 'Oil deposit', vdungeon: 'Monster room', minechest: 'Mineshaft chest cart', endIsland: 'HEE biome island', endTower: 'HEE dungeon tower', deComet: 'Draconic Evolution Ender comet', deChaos: 'Draconic Evolution chaos island', draconium: 'Draconium ore', tfFeature: 'Twilight Forest feature', fortress: 'Nether fortress', spawner: 'Blaze spawner', fchest: 'Fortress chest', wart: 'Nether wart', ore: 'GregTech ore vein', fluid: 'Underground fluid', spawn: 'World spawn', village: 'Village', mineshaft: 'Mineshaft',
};

export const KIND_NOTES = {
  stronghold: 'End portal frame position from an exact port of the stronghold layout (validated piece-for-piece on seed 1337).',
  mineshaft: 'Vanilla mineshaft (Et Futurum wrapper): start chunk, every piece and Y exact (validated piece-for-piece on two worlds).',
  minechest: 'Minecart with chest in a mineshaft corridor (StructureMineshaftPieces.Corridor): two 1-in-100 rolls per corridor section; the cart is placed on air inside the populating chunk, 4–8 picks from the GTNH mineshaftCorridor table (vanilla, Railcraft, IC2, Thaumcraft, HEE, GregTech, food mods).',
  vdungeon: 'Vanilla monster room (WorldGenDungeons): 8 attempts per chunk; a room needs solid floor and ceiling and 1–5 wall openings, so most sit next to caves. Up to two chests (8 loot picks each from the GTNH dungeon table, one enchanted book roll) and a spawner.',
  geode: 'Railcraft abyssal geode (NHCore in mountain biomes, Railcraft under deep ocean). 30% of eligible chunks, centred on the chunk centre. Dark diamond / emerald / lapis sit in the shell 4–5 blocks from the centre.',
  oil: 'DreamMaster OilGeneratorFix (NHCore): on chunks with even x and z, two 0.6% rolls (halved in high biomes, ×1.8 in oceans, ×3.5 in boost biomes, none in blacklisted ones) after reseeding at a jittered block. The jitter comes from the populate Random after all decoration, so the deposit is only as right as the replayed decoration chain: Galacticraft / DreamMaster steps exact on 2,672 / 668 recorded chunks; spawn areas of 7 worlds: 8 of 15 predicted deposits real.',
  lake: 'Vanilla WorldGenLakes from RWG2 populate: water 1 in 10 chunks (start below Y50), lava 1 in 18 (below Y52; water lakes inside a lava-cave landmark are lava). Exact replay of the populate draws; RWG2 never places them at the surface, so they are dug into or met in caves. Chunks with a structure piece or near a village are skipped (their draws are unknown).',
  village: 'VillageNames start chunk + biome check, exact (validated on seed 1337). Village style from the biome.',
  loot: 'Chunk exact; room Y estimated from the terrain.',
  rogue: 'Search area exact; this spot is the terrain model\'s pick.',
  rogueArea: 'The dungeon lands 40–100 blocks from here, if the terrain allows.',
  tcRing: 'Roll and terrain shape exact; whether it builds depends on trees/flowers on its 5 test blocks, so it is a probability by biome (swamp/savanna ~64%, lush/magical/dense ~1%, others ~18%) times the chance an earlier obelisk\'s maze blocks it. On 9 checked worlds: every real obelisk was shown; "likely" ones (≥50%) were real 3 of 4 times, "possible" ones about 1 in 16.',
  tcHilltop: 'Roll exact; terrain check estimated.',
  tcMound: 'Roll exact; terrain check estimated.',
  tcTotem: 'Roll exact; terrain check estimated.',
  spawn: 'Spawn search: vanilla random walk from 0, 0 (exact positions) until Biomes O\' Plenty\'s check passes: the top block must be stone or sand (grass does not count). Tops modelled from RWG2 surfaces incl. the exact stone/cobblestone cliff draws; caves and decoration estimated. Verified on one real world (seed 1337 → 40, -104).',
  node: 'Position and contents replayed from Thaumcraft\'s own generator (Gadomancy extended nodes in GTNH); node contents matched 7/7 real nodes in two worlds. Wild nodes usually sit in the lowest cave of the column.',
  tfFeature: 'Exact port of Twilight Forest 2.7.45 feature placement (TF seed = world seed in GTNH). Verified against a real world: 289/289 regions, 70/70 structure starts.',
  fortress: 'Exact fortress layout (vanilla MapGenNetherBridge + every piece, with GTNH\'s Hodgepodge RNG). Verified against a real world: 350/350 starts, 54/54 full layouts (6578 pieces, boxes and Y).',
  spawner: 'Exact: the blaze spawner of a fortress throne room (verified piece layouts).',
  wart: 'Exact: nether wart room patch (2×5 wart on soul sand).',
  fluid: 'Exact seed maths (GT 5.09.54 UndergroundOil, pristine amounts; pumping depletes chunks). Default GTNH fluid table; a server-edited UndergroundFluids.cfg would change it.',
  ore: 'Candidates, Y and extents are exact seed maths (GregTech 5.09.54). Which candidate passes GT\'s stone test is estimated from the terrain: right for ~87% of veins (order-aware model; Nether / Twilight Forest zoomed in ~92%, exact terrain). Ores sit in layers Y−1…Y+7 where there is stone.',
};

// HEE tower floors (endtower.rs ROOM_NAMES / CHEST_ROOM_NAMES)
export const TOWER_ROOMS = ['', 'enchanting table, bookshelves, 2 brewing stands', 'spawner, head dispensers, redstone', 'flower pot, 2 spawners, heads', '4 dispensers with heads, 8 corner spawners', 'anvils, heads, 4 spawners, 4 brewing stands', 'ceiling spawner, iron bars, flower pots', '8 spawners, anvils, redstone lamps', 'maze of pillars, spawners, 1 chest', 'enchanting table, 2 spawners, bookshelves'];

export const TOWER_CHEST_ROOMS = ['', 'loot: 2 chests (7–11 items each)', 'loot: 16 trapped chests + 8 furnaces', 'loot: 1 chest (17–21 items)'];

export const VILLAGE_STYLES = ['plains', 'desert', 'taiga', 'savanna', 'jungle', 'swamp', 'snowy'];

// ---- layer presets: only the feature layers change (relief, grid, colours, grouping stay)
export const FEATURE_KEYS = ['strongholds', 'spawn', 'villages', 'mineshafts', 'dimFeatures', 'tcNodes', 'geodes', 'lakes', 'vdungeons', 'mineChests', 'deEnd', 'loot', 'rogue', 'tcRing', 'tcHilltop', 'tcMound', 'tcTotem', 'ores', 'fluids'];

export const LAYER_PRESETS = [
  ['Default', 'the layers a fresh visit starts with', Object.fromEntries(FEATURE_KEYS.map((k) => [k, LAYER_DEFAULTS[k]]))],
  ['Clean', 'biomes with spawn and strongholds only', { strongholds: true, spawn: true }],
  ['Ores', 'GT ore veins (underground fluids stay a separate toggle)', { ores: true, spawn: true }],
  ['Structures', 'strongholds, villages, mineshafts, dungeons, dimension structures', { strongholds: true, spawn: true, villages: true, mineshafts: true, dimFeatures: true, loot: true, rogue: true, vdungeons: true, mineChests: true, deEnd: true }],
  ['Thaumcraft', 'obelisks, hilltops, mounds, totems and aura nodes', { spawn: true, tcRing: true, tcHilltop: true, tcMound: true, tcTotem: true, tcNodes: true }],
  ['Everything', 'every feature layer (slow when zoomed out)', Object.fromEntries(FEATURE_KEYS.map((k) => [k, true]))],
];

export function lootName(n, book) {
  if (n.startsWith('item.enchantedBook')) return `Enchanted Book (level ${book})`;
  if (LOOT_NAMES[n]) return LOOT_NAMES[n];
  const gt = /^gt\.metaitem\.0(\d)@(\d+)$/.exec(n);
  if (gt) {
    const id = +gt[2];
    if (gt[1] === '2') return { 32100: 'Purple Drink', 32137: 'Holy Water' }[id] ?? `GT item 02:${id}`;
    const mat = GT_MAT[id % 1000], pre = Math.floor(id / 1000);
    if (mat && pre === 2) return `${mat} Dust`;
    if (mat && pre === 8) return mat;
    if (mat && pre === 11) return `${mat} Ingot`;
    return `GT item 01:${id}`;
  }
  const bb = /^item\.ItemBaubleBlanks@(\d)$/.exec(n);
  if (bb) return ['Mundane Amulet', 'Mundane Ring', 'Mundane Belt'][+bb[1]] ?? `Bauble blank ${bb[1]}`;
  const t = n.replace(/^(item|tile)\./, '').replace(/@0$/, '').replace(/Item$/, '').replace(/^.*[.:]/, '').replace(/([a-z])([A-Z])/g, '$1 $2');
  return t.charAt(0).toUpperCase() + t.slice(1);
}

export function mazeConflict(a, b) {
  const [ax, az, aw, ah] = [a.x >> 4, a.z >> 4, a.a, a.b];
  const [bx, bz, bw, bh] = [b.x >> 4, b.z >> 4, b.a, b.b];
  const inside = (x, z) => Math.abs(x - bx) <= bw && Math.abs(z - bz) <= bh;
  for (const [dx, dz] of [[0, 0], [-aw, -ah], [aw, ah], [-aw, ah], [aw, -ah]]) if (inside(ax + dx, az + dz)) return true;
  const c0 = ax - (1 + (aw >> 1)), r0 = az - (1 + (ah >> 1));
  return c0 <= bx + bw && c0 + aw - 1 >= bx - bw && r0 <= bz + bh && r0 + ah - 1 >= bz - bh;
}
