// Map / 3D marker list from the loaded feature records (pure: the component's state comes in through `ctx`).
import { LOOT_RARE, MINE_RARE, TOWER_ROOMS, TOWER_CHEST_ROOMS, VILLAGE_STYLES, lootName } from './constants.js';
import { ADVANCED_KINDS } from './features.js';

const FORTRESS_LOOT_NAMES = ['Diamond', 'Iron Ingot', 'Gold Ingot', 'Golden Sword', 'Golden Chestplate', 'Flint and Steel', 'Nether Wart', 'Saddle', 'Gold Horse Armor', 'Iron Horse Armor', 'Diamond Horse Armor', 'Music Disc (Pigstep)'];

export function buildShown(list, lay, zoom, d, ctx) {
  const { layers, spawnPt, veinTitle, dimLegend, veinTable, veinColor, selectedPin, palette, nodeType, nodeAspect, nodeMin, markMarkers, hitMarkers, hitIdx, hiddenVeins, hiddenNodeKinds, hiddenFluids, fluidTable, fluidMin, strongholds, pins, LOOT_DISPLAY, MINE_DISPLAY } = ctx;
  const f = [];
  const ores = []; // drawn first: footprints go under everything else
  const fl = [];
  const vt = d ? dimLegend : veinTable; // vein list of the current dimension
  for (const m of list) {
    if (lay.advanced === false && ADVANCED_KINDS.has(m.kind)) continue;
    if (m.kind === 'node') {
      if (!lay.tcNodes) continue;
      if (hiddenNodeKinds.has(m.k)) continue;
      if (nodeType && m.type !== nodeType) continue;
      if (nodeAspect && !(m.aspects ?? []).some(([t, n]) => t === nodeAspect && n >= (Number(nodeMin) || 1))) continue;
      const asp = m.aspects ? m.aspects.map(([t, n]) => `${t} ${n}`) : null;
      const what = { wild: 'Wild aura node', ring: 'Obelisk node', totem: 'Totem node', hilltop: 'Hilltop node', mound: 'Mound node', silverwood: 'Silverwood node' }[m.k] ?? 'Aura node';
      f.push({
        x: m.x, z: m.z, kind: 'node', ntype: m.type, valid: m.valid,
        label: `${what}${m.type ? ` · ${m.type}${m.mod ? ' ' + m.mod : ''}` : ''} · Y${m.y}${m.yExact ? '' : '≈'}`,
        lines: asp ?? ['contents not predictable (chest loot draws first)'],
        detail: m.dimNode ? `${m.dimNode === -1 ? 'Nether' : 'Twilight Forest'} node (Thaumcraft generates wild nodes here too, same x/z rolls as the overworld). Checked on 8 worlds: position ~99%, contents ~87% (Nether) / 100% (Twilight Forest); Y can differ where decoration${m.dimNode === -1 ? ' or a small pocket' : ' (trees, grass)'} changes the column.${m.yExact ? '' : ' Surface node: Y approximate.'}` : m.k === 'silverwood' ? 'only if the silverwood tree actually grows: about half do (15 of 31 across 9 checked worlds; shrubs from decoration decide it, not modelled)' + (m.aspects ? ' · contents exact replay if it grows' : '') : m.valid === false ? `exists only if the silverwood tree rolled earlier in this chunk did not grow (about half the time)${m.aspects ? '; contents then as listed' : ''}` : m.aspects ? `${m.exact ? 'contents exact replay' : 'contains an addon aspect of unknown name'}${m.envCertain ? '' : ' · surroundings (water/stone/foliage) uncertain, contents may differ'}` : null,
      });
      continue;
    }
    if (m.kind === 'minechest') {
      if (!lay.mineChests) continue;
      const items = m.s.map(([, n, k, idx]) => `${k}× ${n.startsWith('item.enchantedBook') ? `Enchanted Book (level ${m.b})` : MINE_DISPLAY?.[idx] ?? lootName(n, m.b)}`);
      const rich = m.s.some(([, n]) => MINE_RARE.test(n));
      f.push({
        x: m.x, z: m.z, kind: 'minechest', y: m.y, tpY: m.y, rich,
        label: `Chest cart · Y${m.y} · ${m.s.length} stack${m.s.length === 1 ? '' : 's'}`,
        lines: items.length ? items : ['empty'],
        detail: 'Minecart with chest in a mineshaft corridor. Rolls and contents are an exact replay of the populating chunk (rwg2dump 2.16: 42 of 44 corridor chest rolls reproduced; the 2 others sat in a corridor the game dropped). The corridor itself can be missing where an earlier chunk removed it (liquid next to it), and where two mineshafts overlap their order depends on world history. Book enchantment not modelled.',
        box3: [m.x, m.y, m.z, m.x + 1, m.y + 1, m.z + 1], xk: 'minechest', color: '#c8a165',
      });
      continue;
    }
    if (m.kind === 'vdungeon') {
      if (!lay.vdungeons) continue;
      const chests = m.c.map((c) => {
        const items = c.s.map(([, n, k, idx]) => `${k}× ${n.startsWith('item.enchantedBook') ? `Enchanted Book (level ${c.b})` : LOOT_DISPLAY[idx] ?? lootName(n, c.b)}`);
        return `Chest ${c.x}, ${c.y}, ${c.z}: ${items.join(', ')}`;
      });
      const val = m.c.reduce((v, c) => v + c.s.filter(([, n]) => LOOT_RARE.test(n)).length, 0);
      f.push({
        x: m.x, z: m.z, kind: 'vdungeon', y: m.y, tpY: m.y, rich: val > 0,
        label: `Dungeon · Y${m.y} · ${m.c.length} chest${m.c.length === 1 ? '' : 's'}`,
        lines: chests.length ? chests : ['no chest (no floor spot with exactly one solid neighbour)'],
        detail: `Vanilla monster room ${2 * m.l + 1}×${2 * m.i + 1}, spawner at ${m.x}, ${m.y}, ${m.z}. Room, chests and loot are an exact replay once the room exists; whether it exists is read from estimated terrain and caves (3 checked worlds: 39 of 41 predicted rooms real, ~60% of real rooms found). Book enchantment not modelled.`,
        box3: [m.x - m.l - 1, m.y - 1, m.z - m.i - 1, m.x + m.l + 1, m.y + 4, m.z + m.i + 1], xk: 'vdungeon', color: '#6d8f5a',
      });
      continue;
    }
    if (m.kind === 'lake') {
      if (!lay.lakes || (lay.lakesLava && !m.lava)) continue;
      const liq = m.lava ? 'Lava' : 'Water', ys = m.y1 > m.y0 ? `Y${m.y0}–${m.y1}` : `Y${m.y0}`;
      f.push({
        x: m.x, z: m.z, kind: 'lake', y: m.y0, tpY: m.y1 + 2, lava: m.lava,
        label: `${liq} lake · ${ys}`,
        lines: [`${m.n} ${m.lava ? 'lava' : 'water'} blocks, ${ys}`, m.open ? 'opens into a cave' : 'sealed in rock (dig to it)'],
        detail: `Underground ${liq.toLowerCase()} lake (vanilla WorldGenLakes in RWG2 populate) · exact replay of the populate draws on the pre-population terrain and caves`,
        box3: [m.x - 8, m.y0, m.z - 8, m.x + 7, m.y1 + 4, m.z + 7], xk: 'lake', color: m.lava ? '#ff6d00' : '#29b6f6',
      });
      continue;
    }
    if (m.kind === 'oil') {
      if (!lay.oil) continue;
      const size = m.r >= 14 ? 'large' : m.r >= 11 ? 'medium' : 'small';
      const dy = Math.floor(Math.sqrt((m.r * m.r) / 3));
      const yLo = m.y - Math.min(m.r - 2, dy), yHi = m.y + Math.min(m.r - 2, dy);
      f.push({
        x: m.x, z: m.z, kind: 'oil', y: m.y, tpY: yHi + 2, size, conf: m.c,
        label: `Oil · Y${m.y} · r${m.r}${size !== 'small' ? ' ⛲' : ''}`,
        lines: [`${size} sphere, radius ${m.r}, Y${yLo}–${yHi}, up to ~${m.n} oil blocks`, size === 'large' ? 'fountain 16 above ground + 4 side columns' : size === 'medium' ? 'fountain 5 above ground' : 'no fountain (dig to it)', `populated by chunk ${m.cx}, ${m.cz} · ${m.c >= 0.7 ? 'likely (~80%): only RWG2 native biomes decorate before it (10 of 12 real)' : '~50%: BOP decorators run before it (15 of 30 real)'}`],
        detail: m.spawnOrder
          ? 'DreamMaster oil sphere predicted from the spawn area replayed in a fresh world\'s load order. 8 of 14 real on 7 checked worlds; worlds whose spawn chunks loaded differently will differ.'
          : 'DreamMaster oil sphere predicted from a canonical-order replay of the surrounding chunks (the real order depends on how the area was explored). 25 of 42 real on 7 checked worlds.',
        box3: [m.x - m.r, yLo, m.z - m.r, m.x + m.r, yHi, m.z + m.r], xk: 'ore', color: '#5d4037',
      });
      continue;
    }
    if (m.kind === 'geode') {
      if (!lay.geodes) continue;
      const ores = [m.d && `${m.d} dark diamond`, m.e && `${m.e} dark emerald`, m.l && `${m.l} dark lapis`].filter(Boolean);
      f.push({
        x: m.x, z: m.z, kind: 'geode', y: m.y, tpY: m.y, rich: m.d > 0,
        label: `Abyssal geode · Y${m.y}${m.d ? ` · ${m.d}💎` : ''}`,
        lines: [ores.length ? ores.join(' · ') : 'no dark ores', `hollow core r 4, abyssal stone shell r 8 (Y${m.y - 8}–${m.y + 7})`],
        detail: `${m.o ? 'Railcraft ocean geode (deep ocean floor)' : 'NHCore mountain geode'} · position and Y exact; dark ores from the exact populate replay (checked on one world: 151/151 geodes, ores 385 predicted vs 382 real)`,
        box3: [m.x - 8, m.y - 8, m.z - 8, m.x + 7, m.y + 7, m.z + 7], xk: 'geode', color: '#7e57c2',
      });
      continue;
    }
    if (m.kind === 'endIsland') {
      const bi = m.a & 15, vid = (m.a >> 4) & 15, bits = m.a >> 8;
      const info = dimLegend?.islands?.[bi];
      const vname = info?.variations.find((v) => v.id === vid)?.name ?? vid;
      const devs = (info?.deviations ?? []).filter((_, k) => bits & (1 << k));
      f.push({
        x: m.x, z: m.z, kind: 'endIsland', ibiome: bi, tpY: null,
        label: zoom >= -2 ? `${info?.name ?? 'Island'} · ${vname}` : null,
        lines: [devs.length ? `deviations: ${devs.join(', ')}` : 'no deviations', `208×208 footprint ${m.x - 104}…${m.x + 103}, ${m.z - 104}…${m.z + 103} · island base Y${m.y}`],
        detail: 'Hardcore Ender Expansion biome island. Where HEE would place it (position, biome, variation, deviations match HEE\'s own code exactly; shape from the HEE island generator incl. its caves (real island: solid blocks within 0.1%); trees and decoration not included). Checked in a real world: biome, base Y and size match. Can be missing: HEE saves an island as an empty shell if the End unloads (server restart, everyone leaves) after a chunk within 8 chunks of its corner was generated but before the island area itself was; such an island never appears.',
        box3: [m.x - 104, 20, m.z - 104, m.x + 103, 160, m.z + 103], xk: 'endIsland', color: ['#7cb342', '#e64a19', '#8e24aa'][bi] ?? '#ccc',
      });
      continue;
    }
    if (m.kind === 'deComet') {
      const size = m.a & 255, ty = m.a >> 8, dx = (m.b << 16) >> 16, dz = m.b >> 16;
      f.push({
        x: m.x, z: m.z, kind: 'deComet', y: m.y, tpY: m.y + size + 2, label: zoom >= -2 ? `Ender comet · Y${m.y}` : null,
        lines: [`core radius ${size} at ${m.x}, ${m.y}, ${m.z}: ~10% draconium ore, 30% obsidian, rest end stone`, `trail to ${m.x + dx}, ${ty}, ${m.z + dz} (${Math.round(Math.hypot(dx, dz))} blocks)`],
        detail: 'Draconic Evolution Ender comet (WorldGenEnderComet). Position, size and trail are an exact replay of Forge\'s per-chunk random for the End (world seed + End resets); rarity 1 in 10000 chunks (GTNH config, read from a real world).',
        tail: [m.x + dx, m.z + dz],
      });
      continue;
    }
    if (m.kind === 'deChaos') {
      f.push({
        x: m.x, z: m.z, kind: 'deChaos', tpY: 120, label: 'Chaos island',
        detail: 'Draconic Evolution chaos island (Chaos Guardian, ~180-block island). Fixed grid: every multiple of 10000 blocks on both axes except 0, 0, the same in every world (default config "Chaos Island Separation" assumed).',
      });
      continue;
    }
    if (m.kind === 'endTower') {
      if (zoom < -3) continue;
      const rooms = [m.a & 15, (m.a >> 4) & 15, (m.a >> 8) & 15], chest = (m.a >> 12) & 15;
      f.push({
        x: m.x, z: m.z, kind: 'endTower', y: m.y, label: zoom >= -1 ? `Dungeon tower · Y${m.y}–${m.b}` : null, tpY: m.b + 2,
        lines: [
          ...rooms.map((r, i) => `floor ${i + 1}: ${TOWER_ROOMS[r] ?? '?'}`),
          `floor 4: ${TOWER_CHEST_ROOMS[chest] ?? '?'}`,
          `ground floor Y${m.y}, roof Y${m.b} (spires to Y${m.b + 13}) · Ender Eye mini-boss on the roof`,
        ],
        detail: 'Hardcore Ender Expansion dungeon tower (MapGenTower: spacing 9–15 chunks, ≥ 350 blocks out, 29% per cell, rarer past 900, never within 125 blocks of an island centre). Position exact replay; layout (island, floors, room kinds, rotation) from HEE\'s tower builder, not yet checked against a real world. Loot and spawner effects use the world random: not predictable. /tp goes onto the roof.',
        box3: [m.x - 13, m.y - 30, m.z - 13, m.x + 13, m.b + 13, m.z + 13], xk: 'endTower', color: '#b39ddb',
      });
      continue;
    }
    if (m.kind === 'draconium') {
      if (!lay.ores) continue;
      f.push({
        x: m.x, z: m.z, kind: 'draconium', exact: !!m.b, y: m.a & 255, tpY: (m.a & 255) + 2, label: zoom >= 1 ? `Draconium ×${m.y} · Y${m.a & 255}–${m.a >> 8}` : null,
        lines: [`${m.y} draconium ore in this chunk, Y${m.a & 255}–${m.a >> 8}`],
        detail: `Draconic Evolution draconium ore (10 tries per chunk, size-4 blobs in end stone, Y1–69). Replay of Forge's per-chunk random on the predicted end stone (main island exact; HEE islands without their ores/decoration; tower islands).${m.b ? '' : ' A comet or chaos island drew first in a neighbouring chunk: positions not predictable here.'} Checked on one fresh island: 9 predicted vs 11 real in a 1-in-16 column sample; exact check pending.`,
        box3: [m.x - 8, m.a & 255, m.z - 8, m.x + 7, m.a >> 8, m.z + 7], xk: 'ore', color: '#ef5350',
      });
      continue;
    }
    if (m.kind === 'tfFeature') {
      if (lay.tfHideHills && /Hollow Hill/.test(dimLegend?.features.find((x) => x.id === m.y)?.name ?? '')) continue;
      const fe = dimLegend?.features.find((x) => x.id === m.y);
      const bn = dimLegend?.biomes.find((x) => x.id === m.b)?.name ?? m.b;
      f.push({ x: m.x, z: m.z, kind: 'tfFeature', structure: !!m.a, label: fe?.name ?? `feature ${m.y}`, detail: `feature chunk ${m.x >> 4}, ${m.z >> 4} · decided by the biome at the region centre (${bn}) · ${m.a ? 'structure' : 'terrain feature'}` });
      continue;
    }
    if (m.kind === 'fortress') {
      const pieces = m.b & 0xfff, sp = (m.b >> 12) & 0xf, wr = (m.b >> 16) & 0xf;
      const mySp = list.filter((q) => q.kind === 'spawner' && q.a === m.x && q.b === m.z);
      f.push({
        x: m.x, z: m.z, kind: 'fortress', tpY: ((m.b >> 20) & 0xff) + 5, // standing on the start crossing's deck
        label: `Nether fortress · Y${m.y}–${m.a}`,
        lines: [
          ...mySp.map((q) => `blaze spawner ${q.x}, ${q.y}, ${q.z}`),
          ...(sp === 0 ? ['no blaze spawner (no throne room)'] : []),
          `${wr} nether wart room${wr === 1 ? '' : 's'} · ${pieces} pieces`,
        ],
        detail: `start chunk ${m.x >> 4}, ${m.z >> 4} · layout exact (every piece box and Y)`,
      });
      continue;
    }
    if (m.kind === 'fortressPiece') {
      if (zoom < -1) continue;
      const bridge = (m.b >> 5) & 1;
      ores.push({ x: (m.x + m.y) >> 1, z: (m.z + m.a) >> 1, kind: 'fortressPiece', bridge, box: { x0: m.x, z0: m.z, x1: m.y + 1, z1: m.a + 1 } });
      continue;
    }
    if (m.kind === 'fchestSlot') continue;
    if (m.kind === 'fchest') {
      if (!lay.dimFeatures) continue;
      // slots: 56 records at the same x / z (y = slot, a = item, b = count)
      const slots = list.filter((q) => q.kind === 'fchestSlot' && q.x === m.x && q.z === m.z).sort((p, q) => p.y - q.y);
      const name = (k) => FORTRESS_LOOT_NAMES[k] ?? `item ${k}`;
      const rare = slots.some((q) => q.a === 0 || q.a === 10);
      f.push({
        x: m.x, z: m.z, kind: 'fchest', tpY: m.y + 1, rich: rare,
        label: `Fortress chest · Y${m.y}${rare ? ' · ✦' : ''}`,
        lines: slots.length ? slots.map((q) => `slot ${q.y}: ${q.b}× ${name(q.a)}`) : ['empty'],
        detail: `Nether fortress chest at ${m.x}, ${m.y}, ${m.z} (Corridor piece; position exact). Contents: BOP's per-chunk populate Random, only fortress chests draw from it, vanilla table + Et Futurum Pigstep (2–5 picks). Verified 11/11 chests slot-exact on a real world (rwg2dump 2.19).`,
      });
      continue;
    }
    if (m.kind === 'spawner') {
      if (zoom < -2) continue;
      f.push({ x: m.x, z: m.z, kind: 'spawner', tpY: m.y + 1, label: `Blaze spawner · Y${m.y}`, detail: `block ${m.x}, ${m.y}, ${m.z} (exact) · in the fortress throne room` });
      continue;
    }
    if (m.kind === 'wart') {
      if (zoom < 0) continue;
      f.push({ x: (m.x + m.a) >> 1, z: (m.z + m.b) >> 1, kind: 'wart', tpY: m.y, label: zoom >= 1 ? `Nether wart · Y${m.y}` : null, detail: `wart on soul sand ${m.x}..${m.a}, ${m.y}, ${m.z}..${m.b} (exact)` });
      continue;
    }
    if (m.kind === 'fluid') {
      const fd = fluidTable?.fluids[m.a];
      if (!lay.fluids || !fd || hiddenFluids.has(m.a) || m.y < fluidMin) continue;
      fl.push({
        x: m.x + 64, z: m.z + 64, kind: 'fluid', color: fd.rgb, lpo: m.y,
        box: { x0: m.x, x1: m.x + 128, z0: m.z, z1: m.z + 128 },
        label: `${fd.displayName} · ~${m.y} L/op`,
        detail: `field average ${m.y} L per pump operation (chunks vary ±25%; best chunk ${m.b} L/op) · 8×8 chunks from ${m.x}, ${m.z}`,
      });
      continue;
    }
    if (m.kind === 'mineshaft') {
      if (lay.mineshafts) {
        const lo = m.b & 0xff, hi = (m.b >> 8) & 0xff;
        f.push({ x: m.x, z: m.z, kind: 'mineshaft', label: `Mineshaft · Y${lo}–${hi}`, detail: `start chunk ${m.x >> 4}, ${m.z >> 4} · central room floor Y${m.y} · ${m.a} pieces, corridors up to ~100 blocks out · exact layout` });
      }
      continue;
    }
    if (m.kind === 'village') {
      if (lay.villages) {
        const style = VILLAGE_STYLES[m.y] ?? 'village';
        f.push({ x: m.x, z: m.z, kind: 'village', label: `Village · ${style}`, detail: `start chunk ${m.x >> 4}, ${m.z >> 4} · biome ${palette?.groups.get(m.a)?.name ?? m.a} · houses spread ~100 blocks around` });
      }
      continue;
    }
    if (m.kind === 'loot' && lay.loot) {
      f.push({ x: m.x, z: m.z, kind: 'loot', label: m.y != null ? `LootGames · room Y${m.y - 10}–${m.y - 2}` : 'LootGames', ...(m.y != null ? { box3: [m.x - 10, m.y - 10, m.z - 10, m.x + 10, m.y - 2, m.z + 10], xk: 'room', tpY: m.y - 6 } : {}) });
    } else if (m.kind === 'ore' && lay.ores) {
      const vi = m.a & 0xff, ai = ((m.a >> 8) & 0xff) - 1, ay = (m.a >> 16) & 0xff, dep = (!d || d === 1) && ((m.a >> 24) & 1) === 1;
      if ((!d && hiddenVeins.has(vi)) || !vt) continue;
      const v = vt.veins[vi];
      if (!v) continue;
      const alt = ai >= 0 ? vt.veins[ai] : null;
      const ext = { w: m.b & 0xff, e: (m.b >> 8) & 0xff, n: (m.b >> 16) & 0xff, s: (m.b >> 24) & 0x7f };
      ores.push({
        x: m.x, z: m.z, kind: 'ore', dep, color: veinColor(vi), icon: v.iconRgb ?? '#888', speck: v.oreRgb ?? [],
        name: veinTitle(v), yr: `Y${m.y - 1}–${m.y + 7}`,
        label: `${veinTitle(v)} · Y${m.y - 1}–${m.y + 7}`,
        lines: v.oreNames ?? v.ores,
        detail: `vein ${16 + ext.w + ext.e}×${16 + ext.n + ext.s} blocks`
          + (d === 1 ? ' · End: only where GT processes ores (main island zone within 16 chunks of 0,0, HEE islands, chaos islands); the vein Y is drawn from the solid column of the chunk GT processes it from, so it changes with the side you arrive from (this assumes you come from the exit platform at 100, 0). Checked on a fresh End: 9/9 veins right incl. Y.' + (dep ? ` Other sides give ${alt ? veinTitle(alt) + ' · Y' + (ay - 1) + '–' + (ay + 7) : 'no vein'}.` : '') : alt ? dep ? ` · depends on the side this area is first loaded from: can also be ${veinTitle(alt)} · Y${ay - 1}–${ay + 7} (this pick assumes you come from spawn; order-dependent veins like this were right ~62% on 7 checked worlds; the real one was this or one of the other sides' outcomes 93% of the time)` : ` · if not: ${veinTitle(alt)} · Y${ay - 1}–${ay + 7}` : '')
          + (!d && !dep && !m.spawnReplay ? ' · same from whichever side you arrive (~93% right on 7 checked worlds)' : '')
          + (m.spawnReplay ? ' · spawn area: decided while a fresh world loads it, replayed in load order on the exact terrain with caves and chunk population (right ~92% on 7 checked worlds)' : ''),
        box: { x0: m.x - 8, x1: m.x + 8, z0: m.z - 8, z1: m.z + 8 }, // oreseed chunk only
        box3: [m.x - 8 - ext.w, m.y - 1, m.z - 8 - ext.n, m.x + 7 + ext.e, m.y + 7, m.z + 7 + ext.s], xk: 'ore', tpY: m.y + 3,
      });
    } else if (m.kind.startsWith('tc') && layers[m.kind]) {
      const label = {
        tcRing: `Eldritch obelisk · ${(m.pf ?? 0) >= 0.5 ? 'likely' : 'possible'} (~${Math.round((m.pf ?? 0) * 100)}%) · altar Y${m.y + 1} · maze ${m.a}×${m.b}`,
        tcHilltop: `Hilltop stones + eerie node · Y≈${m.y} (est.)`,
        tcMound: `Barrow mound + eerie node · Y≈${m.y} (est.)`,
        tcTotem: `Totem + eerie node · Y≈${m.y} (est.)`,
      }[m.kind];
      f.push({ x: m.x, z: m.z, kind: m.kind, label });
    } else if (m.kind === 'rogue' && lay.rogue) {
      const found = m.a != null;
      // likely-only: drop areas the terrain model says get no dungeon, and low-confidence guesses
      if (lay.rogueLikely && m.est && (!found || m.lowConf)) continue;
      f.push({ x: m.x, z: m.z, kind: 'rogueArea', found, label: found ? null : m.y === null && m.a === null && zoom >= -2 ? 'Roguelike: no valid spot (est.)' : 'Roguelike search area' });
      if (found) f.push({ x: m.a, z: m.b, kind: 'rogue', lowConf: m.lowConf, label: `Roguelike · Y≈${m.y} · ${m.conf != null ? (m.conf >= 75 ? 'likely spot' : 'uncertain spot') : m.lowConf ? 'low-confidence guess' : 'likely spot'}`, detail: m.conf != null ? `${m.conf >= 75 ? 'likely: spots like this were right ~80% of the time' : 'uncertain: spots like this were right only ~35–55% of the time'} (9 checked worlds; the search area itself is exact); entrance at ground Y≈${m.y}` : null });
    }
  }
  return [
    ...fl,
    ...ores,
    ...f,
    ...(lay.strongholds && !d ? strongholds.map((s) => ({ x: s.x, z: s.z, label: s.room ? `Stronghold portal room · Y${s.y}` : 'Stronghold', kind: 'stronghold', detail: s.room ? `end portal frame centre at ${s.x}, ${s.y}, ${s.z}` : null })) : []),
    ...(lay.spawn && spawnPt && !d ? [{ x: spawnPt.x, z: spawnPt.z, kind: 'spawn', label: `Spawn (est.${spawnPt.confident ? '' : ', uncertain'})`, detail: `walk step ${spawnPt.attempts} · model probability ${Math.round(spawnPt.prob * 100)}% · right on 5 of 9 checked worlds (caves, mountain river tunnels and lava craters near the walk can move it) · players appear within ~20 blocks` }] : []),
    ...(lay.hits ? hitMarkers.map((h, i) => ({ x: h.x, z: h.z, label: `${i + 1}. ${h.label}`, kind: 'hit', n: i + 1, selected: i === hitIdx })) : []),
    ...(lay.pins ? markMarkers(pins, d, selectedPin) : []),
  ];
}
