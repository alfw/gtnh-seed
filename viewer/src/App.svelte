<script>
  import lootDisplayNames from './lib/lootnames.json';
  import chestDisplayNames from './lib/chestnames.json';
  import { onMount, untrack } from 'svelte';
  import { Pool } from './lib/pool.js';
  import { Renderer, MIN_ZOOM, MAX_ZOOM, terrainColor, TERRAIN_PRESETS, setTerrainPreset, setBlockPalette, blockSample } from './lib/renderer.js';
  import { buildPalette, LAVA_MARKER, BEE_CODES, BEE_TEMPS, BEE_HUMIDITY, beeRgb, beeName } from './lib/colors.js';
  import { parseSeed, randomSeed } from './lib/seed.js';
  import Search from './Search.svelte';
  import { SEARCH_PRESETS, PRESET_GROUPS } from './lib/searchPresets.js';
  import ZoomControl from './ui/ZoomControl.svelte';
  import ContextMenu from './ui/ContextMenu.svelte';
  import KeyHelp from './ui/KeyHelp.svelte';
  import { fmtBlocks } from './lib/blocks.js';
  import {
    CLIMATES, WORLD_TYPES, DIMS, GLOBAL_MARKS_KEY, PHONE_MQ, MARK_TYPES, PORTAL_LINKS, DIM_LABEL, SV_DEFAULTS, LAYER_DEFAULTS, NODE_KINDS, NODE_TYPES, CELL, NODE_CELL, LOOT_NAMES, GT_MAT, LOOT_RARE, OTHER_DIM, SV_GROUPS, SV_GROUP_OF, COMPASS, KIND_NAMES, KIND_NOTES, TOWER_ROOMS, TOWER_CHEST_ROOMS, VILLAGE_STYLES, FEATURE_KEYS, LAYER_PRESETS, lootName, mazeConflict,
  } from './lib/constants.js';
  import { buildShown as buildShownPure } from './lib/markers.js';
  import { DEFAULT_MODE, effectiveLayers, ADVANCED_LAYERS, BUILD, DEV_TOOLS, ISSUES_URL } from './lib/features.js';


  let canvas;
  let renderer;
  const pool = new Pool();

  let palette = $state(null);
  let pack = $state('');
  let seedText = $state('');
  let seedInfo = $derived(parseSeed(seedText));
  let worldType = $state(1);
  let dim = $state(0); // 0 overworld, -1 Nether (BOP), 7 Twilight Forest
  let dimLegend = $state(null); // { biomes: [{id, name, rgb}], features, veins }
  let hover = $state(null); // { x, z, id }
  let exact = $state(null); // inspect result for the hovered block
  let stats = $state([]);
  let highlight = $state(new Set());
  let filter = $state('');
  let showAll = $state(false);
  let paletteMode = $state('game');
  let legendData = null;
  let gotoText = $state('');
  let tileMs = $state(0);
  let copied = $state(false);
  let view = $state({ x: 0, z: 0, zoom: -1 });
  let tab = $state('legend');
  // user markers ("pins"): placed with the marker tool, all dimensions in one record per seed + world type
  // (browser storage; may be unavailable in sandboxed hosts)
  let pins = $state([]); // { x, z, dim, type, label, notes, name, genId, meta, continent, height, bees }
  let selectedPin = $state(-1);
  let markType = $state('pin'); // type the marker tool places
  let markScope = $state('seed'); // 'seed' (this seed + world type) | 'global' (shown on every seed)
  let markFilter = $state('here'); // markers list: 'here' (this dim, both scopes) | 'seed' | 'global' | 'all'
  let markIo = $state(''); // export/import status line
  let keepPlacing = $state(false); // marker tool stays active after a placement
  let marksOpen = $state(typeof window === 'undefined' || window.innerWidth > 720);
  let phone = $state(typeof window !== 'undefined' && !!window.matchMedia && window.matchMedia(PHONE_MQ).matches);
  let sheet = $state(null); // phone: 'tool' | 'marks' | 'result' | 'side' | null (expanded panel)
  function openSheet(s) {
    sheet = sheet === s ? null : s;
    if (sheet) popup = null;
  }
  let marksShown = $derived(phone ? sheet === 'marks' : marksOpen);
  let hitMarkers = $state([]); // from a picked search result
  let hitIdx = $state(-1); // last hit jumped to with next / previous
  let ctxMenu = $state(null); // right-click menu: { sx, sy, x, z, ground }
  let keyHelp = $state(false); // `?` overlay
  let veinFind = $state(null); // vein finder: { vi, list: [{x, z}], idx }
  let loading = $state([]); // [[layer name, cells in flight]] for the loading pill
  let pendingHits = null; // applied after the seed switch (which resets per-seed state)
  let spawnPt = $state(null);
  // ---- map tools (session only, kept per dimension): measure path, radius circle, Nether <-> overworld ×8
  let tool = $state(''); // '' | 'measure' | 'radius' | 'mark' | 'view'
  // street view (3D): open = { x, z, dim, heading }; last = where the person stood (heading arrow on the map)
  let sv = $state(null);
  let svEl = $state(null);
  let svHud = $state(null);
  let svLast = $state(null);
  let svErr = $state('');
  let svCtl = null;
  // biome temperature / humidity (rainfall) by gen id, every registered biome (overworld, Nether, Twilight Forest)
  let climateMap = $state.raw(new Map());
  /** "temp 0.80 · humidity 90%" for a biome gen id ('' if unknown). Humidity = the biome's rainfall, what
   *  Forestry, IC2 crops and GT machines read. */
  function climText(gen) {
    const c = gen == null ? null : climateMap.get(gen);
    return c ? `temp ${c[0].toFixed(2)} · humidity ${Math.round(c[1] * 100)}%` : '';
  }
  let svOpt = $state(structuredClone(SV_DEFAULTS));
  try {
    const s = JSON.parse(localStorage.getItem('gtnh-seedmap:sv') ?? 'null');
    if (s) svOpt = { ...structuredClone(SV_DEFAULTS), ...s, groups: { ...SV_DEFAULTS.groups, ...s.groups }, xray: { ...SV_DEFAULTS.xray, ...s.xray } };
  } catch {}
  $effect(() => {
    const v = JSON.stringify(svOpt);
    try {
      localStorage.setItem('gtnh-seedmap:sv', v);
    } catch {}
  });
  let svBiomes = $state.raw(null); // { near: [[gen, n]], far: [[gen, n]] }
  let svPick = $state(null); // marker under the pointer / tapped: { label, dist, rel, sx, sy, pinned, ... }
  let svBlock = $state(null); // long-pressed block: { x, y, z, approx, sx, sy, dim, info (overworld) | dimInfo }
  let svBlockSeq = 0;
  let miniCanvas = $state(null);
  let miniR = null;
  let dimLutCur = null;
  let measureByDim = $state({ 0: [], '-1': [], 7: [] }); // [{ x, z }]
  let radiusByDim = $state({ 0: null, '-1': null, 7: null }); // { x, z, r }
  let radiusR = $state(500);
  let radiusDrag = null; // { pointerId } while dragging the radius handle
  let shownMarkers = $state.raw([]); // markers currently drawn (for the radius contents list)
  let searchLoad = $state(null); // dev preset → Search: { preset, run, n }
  let searchCmp = $state(null); // Search component (checkPreset)
  let presetChecks = $state({}); // preset id → { running } | { found, n, ms } | { error }
  let checkingAll = $state(false);
  async function checkPreset(p) {
    if (!searchCmp) return;
    presetChecks = { ...presetChecks, [p.id]: { running: true } };
    const r = await searchCmp.checkPreset(p);
    presetChecks = { ...presetChecks, [p.id]: r };
  }
  async function checkAllPresets() {
    checkingAll = true;
    presetChecks = {};
    for (const p of SEARCH_PRESETS) {
      if (!checkingAll) break;
      await checkPreset(p);
    }
    checkingAll = false;
  }
  const checkState = (p) => {
    const c = presetChecks[p.id];
    if (!c) return null;
    if (c.running) return { cls: 'run', text: '…' };
    if (c.error) return { cls: 'bad', text: '✗', title: c.error };
    const exp = p.check?.expect;
    const t = `${c.found}/${c.n} seeds in ${(c.ms / 1000).toFixed(1)} s`;
    if (exp == null) return { cls: 'new', text: `${c.found}`, title: `${t} (no expectation yet)` };
    return c.found === exp ? { cls: 'ok', text: `✓ ${c.found}`, title: t } : { cls: 'bad', text: `⚠ ${c.found}≠${exp}`, title: `${t}; expected ${exp}` };
  };
  const checkSummary = () => {
    const done = SEARCH_PRESETS.filter((p) => presetChecks[p.id] && !presetChecks[p.id].running);
    if (!done.length) return '';
    const bad = done.filter((p) => checkState(p)?.cls === 'bad').length;
    return `${done.length - bad}/${done.length} as expected${bad ? ` · ${bad} changed` : ''}`;
  };
  function copyChecks() {
    const o = {};
    for (const p of SEARCH_PRESETS) if (presetChecks[p.id]?.found != null) o[p.id] = presetChecks[p.id].found;
    navigator.clipboard?.writeText(JSON.stringify(o)).catch(() => {});
  }
  let searchPreset = $state(null); // { x, z, r, n } handed to the Search tab // { x, z, attempts, prob, confident } estimated world spawn
  let strongholds = $state([]); // [{x, z}] block centres of stronghold start chunks
  let pinCopied = $state(-1);
  let pinsKey = '';
  // map mode + layer toggles (persisted)
  let blockById = $state(null); // Blocks mode: code → { id, name, label, rgb }
  let mapMode = $state(0); // 0 = biomes, 1 = Forestry bee climate, 2 = terrain, 3 = surface blocks
  const anyTcOf = (l) => l.tcRing || l.tcHilltop || l.tcMound || l.tcTotem;
  const anyTc = () => anyTcOf(layers);
  let layers = $state({ ...LAYER_DEFAULTS, mode: DEFAULT_MODE });
  let beeHighlight = $state(new Set());
  let aspectTags = $state([]); // Thaumcraft aspect tags, index = aspect id
  let nodeAspect = $state(''); // node layer filter: aspect tag ('' = all)
  let nodeMin = $state(1);
  let nodeType = $state(''); // node layer filter: node type ('' = all)
  let hiddenNodeKinds = $state(new Set()); // wild / silverwood / ring / totem / hilltop / mound
  function toggleNodeKind(k) {
    const n = new Set(hiddenNodeKinds);
    n.has(k) ? n.delete(k) : n.add(k);
    hiddenNodeKinds = n;
  }
  let fluidTable = $state(null); // { fluids: [{displayName, rgb, ...}], provisional }
  let fluidMin = $state(0); // hide fields below this many L/op
  let hiddenFluids = $state(new Set());
  let veinTable = $state(null); // { veins: [...], provisional }
  let hiddenVeins = $state(new Set()); // vein indices not shown
  const veinColor = (i) => `hsl(${(i * 137.508) % 360} 70% 55%)`;
  // GT's own vein name (primary + setLocalizedName materials, "A & B"), as JourneyMap / VisualProspecting show it
  const veinTitle = (v) => v.displayName ?? v.name.replace(/^ore\.mix\./, '').replace(/^./, (c) => c.toUpperCase());
  try {
    const saved = JSON.parse(localStorage.getItem('gtnh-seedmap:layers') ?? 'null');
    if (saved) layers = { ...LAYER_DEFAULTS, mode: DEFAULT_MODE, ...saved.layers };
    if (saved?.mapMode === 1 || saved?.mapMode === 2 || saved?.mapMode === 3) mapMode = saved.mapMode;
  } catch {}
  $effect(() => {
    const v = JSON.stringify({ layers, mapMode });
    try {
      localStorage.setItem('gtnh-seedmap:layers', v);
    } catch {}
  });

  // simple / advanced mode (lib/features.js): advanced-only layers are off in what the map and workers see
  let advanced = $derived(layers.mode === 'advanced');
  let effLayers = $derived(effectiveLayers(layers, advanced));
  $effect(() => {
    if (!advanced && tab === 'ores') tab = 'layers';
  });

  // in-game display names of the dungeon table entries (rwg2dump 2.12.0; music discs by registry id)
  const LOOT_DISPLAY = lootDisplayNames;
  const MINE_DISPLAY = chestDisplayNames.mineshaftCorridor;
  const featureCells = new Map(); // key → features[]
  // spawn-area oil per seed|worldType: { oil, sq } (kept apart from featureCells so its LRU never drops it)
  const spawnOilCache = new Map();
  const featureInflight = new Set();
  let featureList = $state([]);
  let featureTimer = 0;
  function featureKey(S, cx, cz, est, tc = false) {
    return `${S}|${worldType}|${cx}|${cz}|${tc ? 'tc' : est ? 1 : 0}`;
  }

  // Thaumcraft eldritch rings: MazeHandler.mazesInRange — a ring is refused if an earlier ring's maze (centre, corner
  // cells at ±w/±h, grid from (cx-(1+w/2), cz-(1+h/2))) has a cell within ±w', ±h' chunks. Earlier = initial spawn
  // area (x-major) first, then outward.
  function ringOrder(r, sp) {
    const c = [r.x >> 4, r.z >> 4];
    const s = sp && !dim ? [sp.x >> 4, sp.z >> 4] : [0, 0];
    const d = [c[0] - s[0], c[1] - s[1]];
    return Math.abs(d[0]) <= 12 && Math.abs(d[1]) <= 12 ? [0, 0, c[0], c[1]] : [1, d[0] * d[0] + d[1] * d[1], c[0], c[1]];
  }
  // decorated 5-point check per ring (wasm `ring_deco`), for rings still shown after the maze pass: Bayes update of
  // the ring's own P with RING_DECO_LR (pass ×4, fail ×0.6), then the mazes are resolved again
  const ringDeco = new Map(); // `${seed}|${type}|x,z` → 1 / 0 / 'pending'
  function ringP(r, S) {
    const p = r.p ?? 1;
    const d = S != null ? ringDeco.get(`${S}|${worldType}|${r.x},${r.z}`) : undefined;
    if (d !== 1 && d !== 0) return p;
    const l = d === 1 ? 4 : 0.6;
    return (p * l) / (p * l + 1 - p);
  }
  function resolveRings(list, sp = spawnPt, S = null, onUpdate = null) {
    const rings = list.filter((f) => f.kind === 'tcRing');
    rings.sort((p, q) => {
      const a = ringOrder(p, sp), b = ringOrder(q, sp);
      for (let i = 0; i < 4; i++) if (a[i] !== b[i]) return a[i] - b[i];
      return 0;
    });
    // probabilistic maze resolution (like rwg2_biomes::thaumcraft::resolve_ring_mazes_spawn): a ring builds with
    // P(own terrain/vegetation) × Π (1 − P(earlier conflicting ring builds)); below 5% it is dropped
    const done = [];
    const pf = new Map();
    for (const r of rings) {
      let p = ringP(r, S);
      for (const k of done) if (mazeConflict(k, r)) p *= 1 - pf.get(k);
      pf.set(r, p);
      done.push(r);
    }
    if (S != null && onUpdate) {
      const wt = worldType;
      for (const r of rings) {
        const key = `${S}|${wt}|${r.x},${r.z}`;
        if (pf.get(r) < 0.05 || ringDeco.has(key)) continue;
        ringDeco.set(key, 'pending');
        pool
          .request({ type: 'ringDeco', seed: S, worldType: wt, x: r.x, z: r.z }, { priority: 6 })
          .then((res) => {
            if (res.ok < 0) ringDeco.delete(key);
            else ringDeco.set(key, res.ok);
            onUpdate();
          })
          .catch(() => ringDeco.delete(key));
      }
    }
    return list.flatMap((f) => (f.kind !== 'tcRing' ? [f] : pf.get(f) >= 0.05 ? [{ ...f, pf: pf.get(f) }] : []));
  }
  function refreshFeatures() {
    clearTimeout(featureTimer);
    featureTimer = setTimeout(refreshFeaturesNow, 120);
  }
  function refreshFeaturesNow() {
    if (!renderer || !seedInfo || !canvas) return;
    const r = canvas.getBoundingClientRect();
    const a = renderer.screenToWorld(0, 0), b = renderer.screenToWorld(r.width, r.height);
    featureList = collectFeatures({ a, b, zoom: view.zoom, cx: view.x, cz: view.z, d: dim, lay: effLayers, onUpdate: refreshFeaturesNow });
  }
  /** Worldgen features in the rect `A.a`–`A.b` (cached per cell; missing cells are queued and `A.onUpdate` runs
   *  when one arrives). `A.lay`: layer flags, `A.zoom`: decides the terrain estimates, `A.cx/cz`: priority centre. */
  function collectFeatures(A) {
    const lay = A.lay;
    const S = A.seed ?? seedInfo.seed.toString(); // A.seed: another seed (compare view)
    if (A.d) return collectDimFeatures(A);
    if ((!lay.loot && !lay.rogue && !lay.villages && !lay.mineshafts && !anyTcOf(lay) && !lay.ores && !lay.fluids && !lay.tcNodes && !lay.geodes && !lay.lakes && !lay.oil && !lay.vdungeons && !lay.mineChests) || A.zoom < -6) {
      return [];
    }
    const { a, b } = A;
    const est = A.zoom >= -2; // terrain estimates (Y, Roguelike spot) only when zoomed in: they cost chunk gen
    const out = [];
    const want = [];
    for (let cz = Math.floor(a.z / CELL); cz <= Math.floor(b.z / CELL); cz++)
      for (let cx = Math.floor(a.x / CELL); cx <= Math.floor(b.x / CELL); cx++) {
        const k = featureKey(S, cx, cz, est);
        const got = featureCells.get(k) ?? featureCells.get(featureKey(S, cx, cz, true)) ?? (est ? featureCells.get(featureKey(S, cx, cz, false)) : null);
        if (got) out.push(...got);
        if (!featureCells.has(k)) want.push([cx, cz, k]);
      }
    // Thaumcraft structures (needs terrain: only when zoomed in); one more ring of cells for maze conflicts, where
    // only the rings matter (rings-only pass, flag 256: no terrain for mounds / hilltops / totems)
    const tcOut = [];
    if ((anyTcOf(lay) || lay.tcNodes) && est) {
      const [vx0, vz0, vx1, vz1] = [Math.floor(a.x / CELL), Math.floor(a.z / CELL), Math.floor(b.x / CELL), Math.floor(b.z / CELL)];
      for (let cz = vz0 - 1; cz <= vz1 + 1; cz++)
        for (let cx = vx0 - 1; cx <= vx1 + 1; cx++) {
          const k = featureKey(S, cx, cz, true, true);
          const got = featureCells.get(k);
          if (got) {
            tcOut.push(...got);
            continue;
          }
          const inner = cx >= vx0 && cx <= vx1 && cz >= vz0 && cz <= vz1;
          const kr = k + 'r';
          if (inner) {
            want.push([cx, cz, k, true]);
            const gotR = featureCells.get(kr); // a former margin cell: its rings keep the maze resolution steady
            if (gotR) tcOut.push(...gotR);
            continue;
          }
          const gotR = featureCells.get(kr);
          if (gotR) tcOut.push(...gotR);
          else want.push([cx, cz, kr, true, false, false, true]);
        }
    }
    // GregTech ore veins: cheap (seed maths + per-biome height), so also when zoomed further out
    const oreOut = [];
    if (lay.ores && A.zoom >= -4)
      for (let cz = Math.floor(a.z / CELL); cz <= Math.floor(b.z / CELL); cz++)
        for (let cx = Math.floor(a.x / CELL); cx <= Math.floor(b.x / CELL); cx++) {
          const k = `${S}|${worldType}|${cx}|${cz}|gt`;
          const got = featureCells.get(k);
          if (got) oreOut.push(...got);
          else want.push([cx, cz, k, false, true]);
        }
    // spawn area: the veins a fresh world decides while loading it, replayed in load order on exact terrain +
    // caves + chunk population (one worker job per seed, ~2–6 s); replaces the order-free estimate there
    if (lay.ores && A.zoom >= -4 && oreOut.length) {
      const k = `${S}|${worldType}|spawnOres`;
      const sp = featureCells.get(k);
      if (sp) {
        const kept = oreOut.filter((o) => !sp.has(`${o.x},${o.z}`));
        oreOut.length = 0;
        oreOut.push(...kept);
        for (const o of sp.values()) if (o && o.x >= a.x - 8 && o.x <= b.x + 8 && o.z >= a.z - 8 && o.z <= b.z + 8) oreOut.push(o);
      } else if (!featureInflight.has(k)) {
        featureInflight.add(k);
        const seedS = S, wt0 = worldType;
        pool
          .request({ type: 'spawnOres', seed: seedS, worldType: wt0 }, { priority: 1.5 })
          .then((res) => {
            featureInflight.delete(k);
            const m = new Map();
            for (let i = 0; i + 5 < res.recs.length; i += 6) {
              const r = res.recs;
              m.set(`${r[i + 1]},${r[i + 2]}`, r[i] === 20 ? { kind: 'ore', x: r[i + 1], z: r[i + 2], y: r[i + 3], a: r[i + 4], b: r[i + 5], est: true, spawnReplay: true } : null);
            }
            featureCells.set(k, m);
            if ((A.seed ?? seedInfo?.seed.toString()) === seedS && worldType === wt0) A.onUpdate();
          })
          .catch(() => featureInflight.delete(k));
      }
    }
    // GT underground fluid fields: pure seed maths, cheap at any zoom
    const fluidOut = [];
    if (lay.fluids && A.zoom >= -6)
      for (let cz = Math.floor(a.z / CELL); cz <= Math.floor(b.z / CELL); cz++)
        for (let cx = Math.floor(a.x / CELL); cx <= Math.floor(b.x / CELL); cx++) {
          const k = `${S}|${worldType}|${cx}|${cz}|fl`;
          const got = featureCells.get(k);
          if (got) fluidOut.push(...got);
          else want.push([cx, cz, k, false, false, true]);
        }
    // Thaumcraft aura nodes (contents replayed; needs chunk terrain + caves: zoomed in only)
    const nodeOut = [];
    if (lay.tcNodes && est)
      for (let cz = Math.floor(a.z / NODE_CELL); cz <= Math.floor(b.z / NODE_CELL); cz++)
        for (let cx = Math.floor(a.x / NODE_CELL); cx <= Math.floor(b.x / NODE_CELL); cx++) {
          const k = `${S}|${worldType}|${cx}|${cz}|nodes1k`;
          const got = featureCells.get(k);
          if (got) {
            nodeOut.push(...got);
            continue;
          }
          if (featureInflight.has(k)) continue;
          featureInflight.add(k);
          const seedS = S, wt0 = worldType;
          const dist = Math.hypot((cx + 0.5) * NODE_CELL - A.cx, (cz + 0.5) * NODE_CELL - A.cz) / CELL;
          pool
            .request({ type: 'tcNodes', seed: seedS, worldType: wt0, x0: cx * NODE_CELL, z0: cz * NODE_CELL, x1: cx * NODE_CELL + NODE_CELL - 1, z1: cz * NODE_CELL + NODE_CELL - 1 }, { priority: 3.5 + dist })
            .then((res) => {
              featureInflight.delete(k);
              featureCells.set(k, res.nodes.map((n) => ({ ...n, kind: 'node' })));
              if (featureCells.size > 600) featureCells.delete(featureCells.keys().next().value);
              if ((A.seed ?? seedInfo?.seed.toString()) === seedS && worldType === wt0) A.onUpdate();
            })
            .catch(() => featureInflight.delete(k));
        }
    // Railcraft / NHCore abyssal geodes (exact; chunk biome + terrain only where a geode can roll)
    const geoOut = [];
    if (lay.geodes && est)
      for (let cz = Math.floor(a.z / NODE_CELL); cz <= Math.floor(b.z / NODE_CELL); cz++)
        for (let cx = Math.floor(a.x / NODE_CELL); cx <= Math.floor(b.x / NODE_CELL); cx++) {
          const k = `${S}|${worldType}|${cx}|${cz}|geo`;
          const got = featureCells.get(k);
          if (got) {
            geoOut.push(...got);
            continue;
          }
          if (featureInflight.has(k)) continue;
          featureInflight.add(k);
          const seedS = S, wt0 = worldType;
          const dist = Math.hypot((cx + 0.5) * NODE_CELL - A.cx, (cz + 0.5) * NODE_CELL - A.cz) / CELL;
          pool
            .request({ type: 'geodes', seed: seedS, worldType: wt0, x0: cx * NODE_CELL, z0: cz * NODE_CELL, x1: cx * NODE_CELL + NODE_CELL - 1, z1: cz * NODE_CELL + NODE_CELL - 1 }, { priority: 3.6 + dist })
            .then((res) => {
              featureInflight.delete(k);
              featureCells.set(k, res.geodes.map((g) => ({ ...g, kind: 'geode' })));
              if (featureCells.size > 600) featureCells.delete(featureCells.keys().next().value);
              if ((A.seed ?? seedInfo?.seed.toString()) === seedS && worldType === wt0) A.onUpdate();
            })
            .catch(() => featureInflight.delete(k));
        }
    // underground lakes (populate replay up to the two lake rolls; RNG pre-check skips most chunks before terrain)
    if (lay.lakes && est)
      for (let cz = Math.floor(a.z / NODE_CELL); cz <= Math.floor(b.z / NODE_CELL); cz++)
        for (let cx = Math.floor(a.x / NODE_CELL); cx <= Math.floor(b.x / NODE_CELL); cx++) {
          const k = `${S}|${worldType}|${cx}|${cz}|lk`;
          const got = featureCells.get(k);
          if (got) {
            geoOut.push(...got);
            continue;
          }
          if (featureInflight.has(k)) continue;
          featureInflight.add(k);
          const seedS = S, wt0 = worldType;
          const dist = Math.hypot((cx + 0.5) * NODE_CELL - A.cx, (cz + 0.5) * NODE_CELL - A.cz) / CELL;
          pool
            .request({ type: 'lakes', seed: seedS, worldType: wt0, x0: cx * NODE_CELL, z0: cz * NODE_CELL, x1: cx * NODE_CELL + NODE_CELL - 1, z1: cz * NODE_CELL + NODE_CELL - 1 }, { priority: 3.7 + dist })
            .then((res) => {
              featureInflight.delete(k);
              featureCells.set(k, res.lakes.map((g) => ({ ...g, kind: 'lake' })));
              if (featureCells.size > 600) featureCells.delete(featureCells.keys().next().value);
              if ((A.seed ?? seedInfo?.seed.toString()) === seedS && worldType === wt0) A.onUpdate();
            })
            .catch(() => featureInflight.delete(k));
        }
    // DreamMaster oil deposits: the spawn area replayed in the fresh world's load order (one job per seed), and
    // zoomed in 8×8-chunk cells replayed in canonical order (their deposits inside the spawn square are dropped)
    if (lay.oil && est) {
      const k = `${S}|${worldType}|oil`;
      const sp = spawnOilCache.get(k);
      if (sp) geoOut.push(...sp.oil);
      else if (!featureInflight.has(k)) {
        featureInflight.add(k);
        const seedS = S, wt0 = worldType;
        pool
          .request({ type: 'spawnOil', seed: seedS, worldType: wt0 }, { priority: 4.5 })
          .then((res) => {
            featureInflight.delete(k);
            spawnOilCache.set(k, { oil: res.oil.map((g) => ({ ...g, kind: 'oil', spawnOrder: true })), sq: res.sq });
            if ((A.seed ?? seedInfo?.seed.toString()) === seedS && worldType === wt0) A.onUpdate();
          })
          .catch(() => featureInflight.delete(k));
      }
      const sq = sp?.sq;
      if (A.zoom >= 0 || A.sv)
        for (let gz = Math.floor(a.z / 128); gz <= Math.floor(b.z / 128); gz++)
          for (let gx = Math.floor(a.x / 128); gx <= Math.floor(b.x / 128); gx++) {
            const kc = `${S}|${worldType}|${gx}|${gz}|oilc`;
            const got = featureCells.get(kc);
            if (got) {
              for (const o of got) if (!sq || o.cx < sq[0] || o.cx > sq[2] || o.cz < sq[1] || o.cz > sq[3]) geoOut.push(o);
              continue;
            }
            if (featureInflight.has(kc)) continue;
            featureInflight.add(kc);
            const seedS = S, wt0 = worldType;
            const dist = Math.hypot((gx + 0.5) * 128 - A.cx, (gz + 0.5) * 128 - A.cz) / CELL;
            pool
              .request({ type: 'oilCell', seed: seedS, worldType: wt0, cx0: gx * 8, cz0: gz * 8 }, { priority: 4.6 + dist })
              .then((res) => {
                featureInflight.delete(kc);
                featureCells.set(kc, res.oil.map((g) => ({ ...g, kind: 'oil' })));
                if (featureCells.size > 600) featureCells.delete(featureCells.keys().next().value);
                if ((A.seed ?? seedInfo?.seed.toString()) === seedS && worldType === wt0) A.onUpdate();
              })
              .catch(() => featureInflight.delete(kc));
          }
    }
    // vanilla dungeons + chest loot (populate replay up to the dungeon attempts; 256-block cells, zoomed in only)
    const vdOut = [];
    if ((lay.vdungeons || lay.mineChests) && (A.zoom >= 0 || A.sv))
      for (let cz = Math.floor(a.z / 256); cz <= Math.floor(b.z / 256); cz++)
        for (let cx = Math.floor(a.x / 256); cx <= Math.floor(b.x / 256); cx++) {
          const k = `${S}|${worldType}|${cx}|${cz}|vd`;
          const got = featureCells.get(k);
          if (got) {
            vdOut.push(...got);
            continue;
          }
          if (featureInflight.has(k)) continue;
          featureInflight.add(k);
          const seedS = S, wt0 = worldType;
          const dist = Math.hypot((cx + 0.5) * 256 - A.cx, (cz + 0.5) * 256 - A.cz) / CELL;
          pool
            .request({ type: 'vdungeons', seed: seedS, worldType: wt0, x0: cx * 256, z0: cz * 256, x1: cx * 256 + 255, z1: cz * 256 + 255 }, { priority: 3.7 + dist })
            .then((res) => {
              featureInflight.delete(k);
              featureCells.set(k, res.dungeons.map((d) => ({ ...d, kind: d.m ? 'minechest' : 'vdungeon' })));
              if (featureCells.size > 600) featureCells.delete(featureCells.keys().next().value);
              if ((A.seed ?? seedInfo?.seed.toString()) === seedS && worldType === wt0) A.onUpdate();
            })
            .catch(() => featureInflight.delete(k));
        }
    const tcKept = resolveRings(tcOut, A.seed != null ? A.spawn : spawnPt, S, A.onUpdate);
    // obelisk nodes: only rings that survive maze resolution (needs the TC cells, loaded with the node layer)
    const keptRings = new Set(tcKept.filter((f) => f.kind === 'tcRing').map((f) => `${f.x},${f.z}`));
    const nodesKept = nodeOut.filter((n) => n.k !== 'ring' || keptRings.has(`${n.x},${n.z}`));
    const result = [...fluidOut, ...out, ...(anyTcOf(lay) ? tcKept : []), ...oreOut, ...nodesKept, ...geoOut, ...vdOut];
    const seed = S, wt = worldType;
    for (const [cx, cz, k, tc, gt, fl, ringsOnly] of want) {
      if (featureInflight.has(k)) continue;
      featureInflight.add(k);
      const d = Math.hypot((cx + 0.5) * CELL - A.cx, (cz + 0.5) * CELL - A.cz) / CELL;
      pool
        .request(
          { type: 'features', seed, worldType: wt, x0: cx * CELL, z0: cz * CELL, x1: cx * CELL + CELL - 1, z1: cz * CELL + CELL - 1, flags: fl ? 32 : gt ? 16 : tc ? 8 | (ringsOnly ? 256 : 0) : 1 | 2 | 64 | 128 | (est ? 4 : 0) },
          { priority: (fl ? 0.8 : gt ? 1 : tc ? 3 : est ? 2 : 0.5) + d },
        )
        .then((res) => {
          featureInflight.delete(k);
          const isEst = k.endsWith('|1') || k.endsWith('|tc') || k.endsWith('|tcr') || k.endsWith('|gt') || k.endsWith('|fl');
          featureCells.set(k, res.features.map((f) => ({ ...f, est: isEst })));
          if (featureCells.size > 600) featureCells.delete(featureCells.keys().next().value);
          if ((A.seed ?? seedInfo?.seed.toString()) === seed && worldType === wt) A.onUpdate();
        })
        .catch(() => featureInflight.delete(k));
    }
    return result;
  }

  function collectDimFeatures(A) {
    const lay = A.lay, d = A.d;
    const S = A.seed ?? seedInfo.seed.toString();
    const { a, b } = A;
    // flag 2: Nether fortress piece footprints (only drawn zoomed in)
    const flags = d === 1
      ? (lay.dimFeatures ? 1 : 0) | (lay.deEnd ? 4 : 0) | (lay.ores && A.zoom >= -4 ? 16 : 0) | (lay.ores && A.zoom >= -1 ? 64 : 0) | ((lay.dimFeatures || lay.deEnd || lay.ores) ? Math.max(0, Math.min(255, lay.endKills | 0)) << 8 : 0)
      : (lay.dimFeatures ? 1 : 0) | (lay.dimFeatures && d === -1 && A.zoom >= -1 ? 2 : 0) | (lay.ores && A.zoom >= -4 ? 16 : 0) | (lay.ores && A.zoom >= -2 ? 64 : 0);
    const wantNodes = lay.tcNodes && A.zoom >= -2 && d !== 1;
    if ((!flags && !wantNodes) || A.zoom < -7) {
      return [];
    }
    const out = [];
    const seed = S;
    for (let cz = Math.floor(a.z / CELL); cz <= Math.floor(b.z / CELL); cz++)
      for (let cx = Math.floor(a.x / CELL); cx <= Math.floor(b.x / CELL); cx++) {
        const k = `${seed}|d${d}|${cx}|${cz}|${flags}`;
        const got = featureCells.get(k);
        if (got) {
          out.push(...got);
          continue;
        }
        if (featureInflight.has(k)) continue;
        featureInflight.add(k);
        const dist = Math.hypot((cx + 0.5) * CELL - A.cx, (cz + 0.5) * CELL - A.cz) / CELL;
        pool
          .request({ type: 'features', seed, dim: d, x0: cx * CELL, z0: cz * CELL, x1: cx * CELL + CELL - 1, z1: cz * CELL + CELL - 1, flags }, { priority: 1 + dist })
          .then((res) => {
            featureInflight.delete(k);
            featureCells.set(k, res.features);
            if (featureCells.size > 600) featureCells.delete(featureCells.keys().next().value);
            if ((A.seed ?? seedInfo?.seed.toString()) === seed) A.onUpdate();
          })
          .catch(() => featureInflight.delete(k));
      }
    if (wantNodes)
      for (let cz = Math.floor(a.z / NODE_CELL); cz <= Math.floor(b.z / NODE_CELL); cz++)
        for (let cx = Math.floor(a.x / NODE_CELL); cx <= Math.floor(b.x / NODE_CELL); cx++) {
          const k = `${seed}|d${d}|${cx}|${cz}|nodes1k`;
          const got = featureCells.get(k);
          if (got) {
            out.push(...got);
            continue;
          }
          if (featureInflight.has(k)) continue;
          featureInflight.add(k);
          const dist = Math.hypot((cx + 0.5) * NODE_CELL - A.cx, (cz + 0.5) * NODE_CELL - A.cz) / CELL;
          pool
            .request({ type: 'tcNodes', seed, dim: d, x0: cx * NODE_CELL, z0: cz * NODE_CELL, x1: cx * NODE_CELL + NODE_CELL - 1, z1: cz * NODE_CELL + NODE_CELL - 1 }, { priority: 3.5 + dist })
            .then((res) => {
              featureInflight.delete(k);
              featureCells.set(k, res.nodes.map((n) => ({ ...n, kind: 'node', dimNode: d })));
              if (featureCells.size > 600) featureCells.delete(featureCells.keys().next().value);
              if ((A.seed ?? seedInfo?.seed.toString()) === seed) A.onUpdate();
            })
            .catch(() => featureInflight.delete(k));
        }
    return out;
  }

  function storageKey() {
    return seedInfo ? `gtnh-seedmap:marks:${seedInfo.seed}:${worldType}` : '';
  }
  const normMark = (p, scope) => ({ dim: 0, type: 'pin', label: '', notes: '', ...p, scope: scope ?? (p.scope === 'global' ? 'global' : 'seed') });
  function loadPins() {
    pinsKey = storageKey();
    let list = null, migrated = false, global = [];
    try {
      const raw = localStorage.getItem(pinsKey);
      if (raw != null) list = JSON.parse(raw);
      else {
        // migrate pre-v1.13 click pins (overworld only) to Pin markers
        const old = localStorage.getItem(`gtnh-seedmap:pins:${seedInfo.seed}:${worldType}`);
        if (old != null) (list = JSON.parse(old).map((p) => ({ ...p, dim: 0, type: 'pin' }))), (migrated = true);
      }
    } catch {}
    try {
      const g = JSON.parse(localStorage.getItem(GLOBAL_MARKS_KEY) ?? '[]');
      if (Array.isArray(g)) global = g;
    } catch {}
    pins = [...(Array.isArray(list) ? list.map((p) => normMark(p, 'seed')) : []), ...global.map((p) => normMark(p, 'global'))];
    selectedPin = -1;
    if (migrated) savePins();
    loadViews();
  }
  // ---- saved views: position + zoom + dimension + map mode + feature layers, per seed + world type
  let views = $state([]);
  let viewName = $state('');
  let viewsKey = '';
  function loadViews() {
    viewsKey = seedInfo ? `gtnh-seedmap:views:${seedInfo.seed}:${worldType}` : '';
    let list = [];
    try {
      const v = JSON.parse(localStorage.getItem(viewsKey) ?? '[]');
      if (Array.isArray(v)) list = v.filter((x) => x && Number.isFinite(x.x) && Number.isFinite(x.z));
    } catch {}
    views = list;
  }
  function persistViews() {
    try {
      if (viewsKey) localStorage.setItem(viewsKey, JSON.stringify(views));
    } catch {}
  }
  function saveView() {
    const x = Math.round(view.x), z = Math.round(view.z);
    const name = viewName.trim() || `${DIM_LABEL[dim] ?? 'dim ' + dim} ${x}, ${z}`;
    const v = { name, dim, x, z, zoom: Math.round(view.zoom * 4) / 4, mode: mapMode, lay: Object.fromEntries(FEATURE_KEYS.map((k) => [k, !!effLayers[k]])), saved: Date.now() };
    views = [...views.filter((o) => o.name !== name), v];
    viewName = '';
    persistViews();
  }
  function openView(v) {
    if (v.dim !== dim) dim = v.dim;
    if (Number.isFinite(v.mode)) mapMode = v.mode;
    if (v.lay) {
      applyPreset(v.lay);
      // a view saved with advanced-mode layers on needs Advanced to show them
      if (!advanced && ADVANCED_LAYERS.some((k) => v.lay[k])) layers.mode = 'advanced';
    }
    updateView({ x: v.x, z: v.z, zoom: v.zoom });
    if (phone) sheet = null;
  }
  function deleteView(i) {
    views = views.filter((_, j) => j !== i);
    persistViews();
  }

  /** Seed markers → this seed's key, global markers → the global key. */
  function savePins() {
    try {
      if (pinsKey) localStorage.setItem(pinsKey, JSON.stringify(pins.filter((p) => p.scope !== 'global')));
    } catch {}
    try {
      localStorage.setItem(GLOBAL_MARKS_KEY, JSON.stringify(pins.filter((p) => p.scope === 'global')));
    } catch {}
  }
  // ---- marker export / import (JSON; localStorage stays the store) ----
  function exportMarks(which) {
    const list = pins.filter((p) => which === 'both' || (which === 'global' ? p.scope === 'global' : p.scope !== 'global'));
    const doc = {
      format: 'gtnh-seedmap-markers', version: 1, seed: seedInfo?.seed.toString() ?? null, worldType,
      markers: list.map(({ x, z, dim: d, type, label, notes, scope, name, height }) => ({ x, z, dim: d, type, label, notes, scope, name, height })),
    };
    const text = JSON.stringify(doc, null, 1);
    try {
      const a = document.createElement('a');
      a.href = URL.createObjectURL(new Blob([text], { type: 'application/json' }));
      a.download = `gtnh-markers-${which}${which !== 'global' ? '-' + doc.seed : ''}.json`;
      document.body.appendChild(a);
      a.click();
      a.remove();
      setTimeout(() => URL.revokeObjectURL(a.href), 2000);
      markIo = `exported ${list.length} marker${list.length === 1 ? '' : 's'}`;
    } catch {
      prompt = text; // sandboxed: show the JSON to copy by hand
    }
  }
  function importMarksText(text) {
    let doc;
    try {
      doc = JSON.parse(text);
    } catch {
      markIo = 'import failed: not JSON';
      return;
    }
    const arr = Array.isArray(doc) ? doc : doc?.markers;
    if (!Array.isArray(arr)) {
      markIo = 'import failed: no markers';
      return;
    }
    const key = (p) => `${p.scope}|${p.dim}|${p.x}|${p.z}|${p.type}|${p.label}`;
    const have = new Set(pins.map(key));
    const add = [];
    for (const m of arr) {
      if (!Number.isFinite(m?.x) || !Number.isFinite(m?.z)) continue;
      const p = normMark({ x: Math.floor(m.x), z: Math.floor(m.z), dim: [0, -1, 7, 1].includes(m.dim) ? m.dim : 0, type: MARK_TYPES[m.type] ? m.type : 'pin', label: String(m.label ?? ''), notes: String(m.notes ?? ''), name: m.name ?? '', height: m.height ?? null, scope: m.scope });
      if (have.has(key(p))) continue;
      have.add(key(p));
      add.push(p);
    }
    pins = [...pins, ...add];
    savePins();
    const other = doc?.seed && seedInfo && doc.seed !== seedInfo.seed.toString() && add.some((p) => p.scope !== 'global');
    markIo = `imported ${add.length} (${arr.length - add.length} already present)${other ? ` · seed markers from seed ${doc.seed} added to this seed` : ''}`;
  }
  function importMarksFile(e) {
    const f = e.currentTarget.files?.[0];
    e.currentTarget.value = '';
    if (!f) return;
    f.text().then(importMarksText).catch(() => (markIo = 'import failed: could not read the file'));
  }

  // ---- URL state -------------------------------------------------------------------------------------------
  function readHash() {
    const p = new URLSearchParams(location.hash.slice(1));
    seedText = p.get('seed') ?? String(randomSeed());
    worldType = p.get('type') === 'RWG2' ? 0 : 1;
    dim = [0, -1, 7, 1].includes(Number(p.get("dim"))) ? Number(p.get('dim')) : 0;
    view = {
      x: Number(p.get('x') ?? 0) || 0,
      z: Number(p.get('z') ?? 0) || 0,
      zoom: Math.max(MIN_ZOOM, Math.min(MAX_ZOOM, Number.isFinite(Number(p.get('zoom') ?? 'x')) ? Number(p.get('zoom')) : -1)),
    };
  }
  /** URL state (also what Copy link shares): seed, world type, dimension, centre, zoom. */
  function hashParams(seed) {
    const p = new URLSearchParams();
    p.set('seed', seed);
    p.set('type', worldType === 0 ? 'RWG2' : 'RWG2_CONTINENTS');
    if (dim) p.set('dim', String(dim));
    p.set('x', String(Math.round(view.x)));
    p.set('z', String(Math.round(view.z)));
    p.set('zoom', String(Math.round(view.zoom * 4) / 4));
    return p;
  }
  // GitHub issue form (.github/ISSUE_TEMPLATE/bug.yml) prefilled from the current view; other trackers get the plain URL
  let bugUrl = $derived.by(() => {
    if (!/github\.com\/[^/]+\/[^/]+\/issues\/?$/.test(ISSUES_URL)) return ISSUES_URL;
    const q = new URLSearchParams({ template: 'bug.yml', version: `v${BUILD.version} · ${BUILD.date} · ${BUILD.source}` });
    q.set('pack', worldType === 0 ? 'RWG2' : 'RWG2_CONTINENTS');
    if (seedText.trim()) q.set('seed', seedText.trim());
    q.set('place', `${DIMS.find((d) => d.id === dim)?.label ?? `dim ${dim}`}, X ${Math.round(view.x)}, Z ${Math.round(view.z)}, zoom ${Math.round(view.zoom * 4) / 4}`);
    q.set('browser', navigator.userAgent);
    return `${ISSUES_URL.replace(/\/$/, '')}/new?${q}`;
  });
  let hashTimer = 0;
  function writeHash() {
    clearTimeout(hashTimer);
    hashTimer = setTimeout(() => {
      const p = hashParams(seedText.trim());
      try {
        history.replaceState(null, '', `#${p}`); // self-hosted: shareable URL state (ignored inside sandboxed hosts)
      } catch {}
    }, 250);
  }

  onMount(() => {
    const loadingTimer = setInterval(pollLoading, 500);
    if (window.matchMedia) {
      const mq = window.matchMedia(PHONE_MQ);
      const on = (e) => (phone = e.matches);
      mq.addEventListener?.('change', on);
    }
    readHash();
    renderer = new Renderer(canvas, pool, {
      onStats: (s) => (stats = s),
      onTileTime: (ms) => (tileMs = tileMs ? tileMs * 0.9 + ms * 0.1 : ms),
    });
    renderer.setView(view);
    pool
      .request({ type: 'legend' }, { priority: -10 })
      .then((r) => {
        pack = r.legend.pack;
        legendData = r.legend;
        climateMap = new Map((r.legend.climate ?? []).map(([id, t, h]) => [id, [t, h]]));
        veinTable = r.legend.veins;
        fluidTable = r.legend.fluids;
        setBlockPalette(r.legend.blocks);
        blockById = new Map((r.legend.blocks ?? []).map((b) => [b.id, b]));
        pool.request({ type: 'aspectTags' }, { priority: -9 }).then((t) => (aspectTags = t.tags)).catch(() => {});
        palette = buildPalette(legendData, paletteMode);
        renderer.setPalette(palette);
      })
      .catch((err) => (fatal = `Could not start the WebAssembly worldgen module: ${err.message}`));
    // phone: the bottom sheet / tab strip overlaps the canvas bottom → lift the scale bar above it
    const inset = () => {
      let px = 0;
      if (phone && asideEl) {
        const c = canvas.getBoundingClientRect(), a = asideEl.getBoundingClientRect();
        if (a.width > 0 && a.top < c.bottom && a.bottom > c.bottom - 40 && a.left < c.left + 160 && a.right > c.left) px = Math.max(0, c.bottom - a.top);
      }
      if (renderer.bottomInset !== px) {
        renderer.bottomInset = px;
        renderer.invalidate();
      }
    };
    const ro = new ResizeObserver(() => {
      inset();
      renderer.invalidate();
    });
    ro.observe(canvas);
    if (asideEl) ro.observe(asideEl);
    window.addEventListener('hashchange', onHashChange);
    return () => {
      clearInterval(loadingTimer);
      ro.disconnect();
      renderer.destroy();
      window.removeEventListener('hashchange', onHashChange);
    };
  });

  // other dimensions: legend + colour table, biome map only
  $effect(() => {
    const d = dim;
    if (!renderer) return;
    if (!d) {
      dimLegend = null;
      renderer.setDimLut(null);
      dimLutCur = null;
      return;
    }
    if (mapMode === 1 || mapMode === 3) mapMode = 0; // bee climate, blocks: overworld only
    pool
      .request({ type: 'dimLegend', dim: d }, { priority: -10 })
      .then((r) => {
        if (dim !== d) return;
        dimLegend = r.legend;
        const lut = new Uint32Array(65536).fill(0xff444444);
        for (const b of r.legend.biomes) lut[b.id] = 0xff000000 | ((b.rgb & 255) << 16) | (b.rgb & 0xff00) | ((b.rgb >> 16) & 255);
        renderer.setDimLut(lut);
        dimLutCur = lut;
        miniR?.setDimLut(lut);
      })
      .catch(() => {});
  });

  function onHashChange() {
    readHash();
    renderer?.setView(view);
    cmpR?.setView(view);
  }

  /** Dimension code for wasm tiles / inspect: the End carries HEE's reset count (`1 | resets << 8`): the main
   *  island's terrain seed is world seed + resets (DragonSavefile death count, raised only when the temple ritual
   *  deletes DIM1), HEE islands keep the raw seed. Other dims unchanged. */
  function dimCode(d) {
    return d === 1 ? 1 + 256 * Math.max(0, Math.min(255, layers.endKills | 0)) : d;
  }

  $effect(() => {
    if (!renderer || !seedInfo) return;
    layers.endKills;
    renderer.setWorld(seedInfo.seed.toString(), worldType, dimCode(dim));
    exact = null;
    if (storageKey() !== pinsKey) {
      loadPins();
      hitMarkers = pendingHits ?? [];
      pendingHits = null;
      const s = seedInfo.seed.toString();
      pool
        .request({ type: 'strongholds', seed: s }, { priority: -6 })
        .then((r) => {
          if (seedInfo?.seed.toString() === s) strongholds = r.strongholds.map((c) => (c.room ? { x: c.room.x, z: c.room.z, y: c.room.y, room: true } : { x: c.chunkX * 16 + 8, z: c.chunkZ * 16 + 8 }));
        })
        .catch(() => {});
      spawnPt = null;
      const wt = worldType;
      pool
        .request({ type: 'spawn', seed: s, worldType: wt }, { priority: -5 })
        .then((r) => {
          if (seedInfo?.seed.toString() === s && worldType === wt) spawnPt = r.spawn;
        })
        .catch(() => {});
    }
    refreshFeatures();
    writeHash();
  });

  /** Map / 3D markers from raw features (`lay`: layer flags, `zoom`: detail level, `d`: dimension). */
  // ---- seed compare: a second map of another seed beside the main one, same view / dimension / map mode
  let cmpOn = $state(false);
  let cmpText = $state('');
  let cmpInfo = $derived(cmpText.trim() ? parseSeed(cmpText) : null);
  let cmpCanvas = $state(null);
  let cmpR = null;
  let cmpVer = $state(0);
  let cmpSpawn = $state(null);
  let cmpHolds = $state([]);
  let cmpCursor = $state(null); // { sx, sy, x, z, side: 'a' | 'b' }
  function openCompare(seed) {
    if (seed != null) cmpText = String(seed);
    if (!cmpText.trim() && seedInfo) cmpText = String(randomSeed());
    cmpOn = true;
    tool = null;
  }
  function closeCompare() {
    cmpOn = false;
    cmpCursor = null;
  }
  function swapCompare() {
    if (!cmpInfo) return;
    const a = seedText;
    seedText = cmpText;
    cmpText = a;
  }
  // renderer lifecycle
  $effect(() => {
    const c = cmpCanvas;
    if (!c || !cmpOn) {
      cmpR?.destroy();
      cmpR = null;
      return;
    }
    if (cmpR) return;
    cmpR = new Renderer(c, pool, { ns: 'cmp' });
    if (palette) cmpR.setPalette(palette);
    cmpR.setView(view);
    const ro = new ResizeObserver(() => cmpR?.invalidate());
    ro.observe(c);
    untrack(() => cmpVer++);
    return () => ro.disconnect();
  });
  // world / dimension
  $effect(() => {
    cmpVer;
    const info = cmpInfo, d = dim;
    layers.endKills;
    if (!cmpR || !info) return;
    cmpR.setWorld(info.seed.toString(), worldType, dimCode(d));
    cmpR.setDimLut(d ? dimLutCur : null);
    cmpR.setView(view);
  });
  $effect(() => {
    cmpVer;
    const lut = dimLutCur;
    if (cmpR && dim) cmpR.setDimLut(lut);
  });
  // map mode, relief, palette, highlight: same as the main map
  $effect(() => {
    cmpVer;
    if (!cmpR || !palette) return;
    cmpR.setPalette(palette);
    cmpR.setMode(mapMode);
    cmpR.setRelief(layers.relief);
    cmpR.setBiomeOverlay(mapMode === 2 ? Number(layers.biomeOverlay) || 0 : 0);
    if (mapMode === 2 && !(Number(layers.biomeOverlay) > 0)) cmpR.setHighlight(null);
    else if (mapMode === 1) cmpR.setHighlight(new Set(beeHighlight));
    else {
      const rwg = new Set();
      for (const g of highlight) for (const id of palette.groups.get(g)?.rwgIds ?? []) rwg.add(id);
      cmpR.setHighlight(rwg);
    }
  });
  // landmarks of the compared seed: spawn + stronghold portal rooms (overworld)
  $effect(() => {
    const info = cmpInfo, wt = worldType;
    cmpSpawn = null;
    cmpHolds = [];
    if (!cmpOn || !info) return;
    const s = info.seed.toString();
    pool
      .request({ type: 'strongholds', seed: s }, { priority: -6 })
      .then((r) => {
        if (cmpInfo?.seed.toString() === s) cmpHolds = r.strongholds.map((c) => (c.room ? { x: c.room.x, z: c.room.z, y: c.room.y, room: true } : { x: c.chunkX * 16 + 8, z: c.chunkZ * 16 + 8 }));
      })
      .catch(() => {});
    pool
      .request({ type: 'spawn', seed: s, worldType: wt }, { priority: -5 })
      .then((r) => {
        if (cmpInfo?.seed.toString() === s && worldType === wt) cmpSpawn = r.spawn;
      })
      .catch(() => {});
  });
  // feature layers of the compared seed: same cells / cache as the main map, keyed by seed B
  let cmpFeat = $state.raw([]);
  let cmpTimer = 0;
  function refreshCmp() {
    if (!cmpOn) return;
    clearTimeout(cmpTimer);
    cmpTimer = setTimeout(refreshCmpNow, 150);
  }
  function refreshCmpNow() {
    if (!cmpR || !cmpInfo || !cmpCanvas || !cmpOn) return;
    const r = cmpCanvas.getBoundingClientRect();
    const a = cmpR.screenToWorld(0, 0), b = cmpR.screenToWorld(r.width, r.height);
    cmpFeat = collectFeatures({ a, b, zoom: view.zoom, cx: view.x, cz: view.z, d: dim, lay: effLayers, seed: cmpInfo.seed.toString(), spawn: cmpSpawn, onUpdate: refreshCmpNow });
  }
  $effect(() => {
    cmpVer;
    cmpInfo;
    cmpSpawn;
    dim;
    const _ = { ...layers };
    untrack(() => {
      cmpFeat = [];
      refreshCmp();
    });
  });
  $effect(() => {
    cmpVer;
    if (!cmpR) return;
    const lay = { ...effLayers, hits: false, pins: false };
    const shown = buildShownPure(cmpFeat, lay, view.zoom, dim, { layers: lay, spawnPt: cmpSpawn, veinTitle, dimLegend, veinTable, veinColor, selectedPin: -1, palette, nodeType, nodeAspect, nodeMin, markMarkers: () => [], hitMarkers: [], hitIdx: -1, hiddenVeins, hiddenNodeKinds, hiddenFluids, fluidTable, fluidMin, strongholds: cmpHolds, pins: [], LOOT_DISPLAY, MINE_DISPLAY });
    cmpR.setGrouping(layers.groupMarkers, layers.groupPx);
    cmpR.setMarkers(shown);
  });
  /** Blocks-mode sample → "Grass · Y72" / "Sand under water · Y55". */
  function blockText(id) {
    const b = blockSample(id);
    const l = blockById?.get(b.block)?.label ?? '…';
    return `${l}${b.wet ? ' under water' : ''} · Y${b.y}`;
  }
  /** Label of a raw tile id in the current map mode / dimension. */
  function tileLabel(id) {
    if (id == null) return '…';
    if (mapMode === 2) return `Y${Math.floor(id / 8)}`;
    if (mapMode === 3) return blockText(id);
    if (mapMode === 1) return `Bees: ${beeName(id)}`;
    if (dim) return dimLegend?.biomes.find((b) => b.id === id)?.name ?? '…';
    return palette?.byId.get(id)?.group?.name ?? '…';
  }
  let cmpA = $derived(cmpCursor && renderer ? tileLabel(renderer.idAt(cmpCursor.x, cmpCursor.z)) : null);
  let cmpB = $derived(cmpCursor && cmpR ? tileLabel(cmpR.idAt(cmpCursor.x, cmpCursor.z)) : null);
  // the compare pane pans / zooms the shared view
  let cmpDown = null;
  function cmpPointerDown(e) {
    if (e.button !== 0) return;
    cmpCanvas.setPointerCapture(e.pointerId);
    cmpDown = { x: e.clientX, y: e.clientY };
  }
  function cmpPointerMove(e) {
    const r = cmpCanvas.getBoundingClientRect();
    const sx = e.clientX - r.left, sy = e.clientY - r.top;
    if (cmpDown) {
      updateView({ x: view.x - (e.clientX - cmpDown.x) / renderer.ppb, z: view.z - (e.clientY - cmpDown.y) / renderer.ppb });
      cmpDown = { x: e.clientX, y: e.clientY };
    }
    const w = cmpR?.screenToWorld(sx, sy);
    if (w) cmpCursor = { sx, sy, x: Math.floor(w.x), z: Math.floor(w.z), side: 'b' };
  }
  function cmpPointerUp() {
    cmpDown = null;
  }
  function cmpWheel(e) {
    e.preventDefault();
    const r = cmpCanvas.getBoundingClientRect();
    const dz = -Math.sign(e.deltaY) * Math.min(1, Math.abs(e.deltaY) / 100) * 0.5;
    zoomAt(e.clientX - r.left, e.clientY - r.top, dz);
  }

  // ---- map export: the current view (both panes when comparing) as a PNG, with an info footer + biome legend
  let pngOpen = $state(false);
  let pngOpt = $state({ markers: true, legend: true, grid: false });
  let pngBusy = $state(false);
  /** Legend rows (colour, name, share) for what is on screen in renderer `r`; biome map modes only. */
  function pngLegendRows(r) {
    if (mapMode !== 0) return [];
    const by = new Map();
    for (const { id, share } of r.visibleStats()) {
      let name, rgb;
      if (dim) {
        const b = dimLegend?.biomes.find((x) => x.id === id);
        if (!b) continue;
        name = b.name;
        rgb = [(b.rgb >> 16) & 255, (b.rgb >> 8) & 255, b.rgb & 255];
      } else {
        const g = palette?.byId.get(id)?.group;
        if (!g) continue;
        name = g.name;
        rgb = g.rgb;
      }
      const e = by.get(name) ?? { name, rgb, share: 0 };
      e.share += share;
      by.set(name, e);
    }
    return [...by.values()].sort((a, b) => b.share - a.share).filter((e) => e.share >= 0.005).slice(0, 14);
  }
  /** Draw `r` into a fresh bitmap with or without markers / grid, then restore it. */
  function snapshot(r, markers, grid) {
    const keepM = r.markers, keepG = r.showGrid, keepT = r.tools;
    if (!markers) r.markers = [];
    r.showGrid = grid;
    if (!markers) r.tools = null;
    r.draw();
    const c = document.createElement('canvas');
    c.width = r.canvas.width;
    c.height = r.canvas.height;
    c.getContext('2d').drawImage(r.canvas, 0, 0);
    r.markers = keepM;
    r.showGrid = keepG;
    r.tools = keepT;
    r.draw();
    return c;
  }
  async function exportPng() {
    if (!renderer || pngBusy) return;
    pngBusy = true;
    try {
      const dpr = window.devicePixelRatio || 1;
      const panes = [{ r: renderer, seed: seedInfo?.seed.toString() ?? '', tag: cmpOn && cmpR ? 'A' : '' }];
      if (cmpOn && cmpR && cmpInfo) panes.push({ r: cmpR, seed: cmpInfo.seed.toString(), tag: 'B' });
      const shots = panes.map((p) => ({ ...p, c: snapshot(p.r, pngOpt.markers, pngOpt.grid), rows: pngOpt.legend ? pngLegendRows(p.r) : [] }));
      const phoneStack = phone && shots.length > 1; // phone compare: panes stacked
      const mapW = phoneStack ? shots[0].c.width : shots.reduce((w, s) => w + s.c.width, 0);
      const mapH = phoneStack ? shots.reduce((h, s) => h + s.c.height, 0) : Math.max(...shots.map((s) => s.c.height));
      const line = 18 * dpr, pad = 10 * dpr;
      const legendLines = Math.max(0, ...shots.map((s) => Math.ceil(s.rows.length / 2)));
      const footH = pad * 2 + line * (1 + shots.length + (legendLines ? legendLines + 0.4 : 0));
      const out = document.createElement('canvas');
      out.width = mapW;
      out.height = mapH + footH;
      const g = out.getContext('2d');
      g.fillStyle = '#101418';
      g.fillRect(0, 0, out.width, out.height);
      let ox = 0, oy = 0;
      for (const s of shots) {
        g.drawImage(s.c, ox, oy);
        if (s.tag) {
          g.font = `600 ${13 * dpr}px system-ui, sans-serif`;
          const t = `${s.tag} · ${s.seed}`;
          const w = g.measureText(t).width + 12 * dpr;
          g.fillStyle = 'rgba(16,20,24,0.85)';
          g.fillRect(ox + 8 * dpr, oy + 8 * dpr, w, 22 * dpr);
          g.fillStyle = '#e8a33d';
          g.fillText(t, ox + 14 * dpr, oy + 24 * dpr);
        }
        if (phoneStack) oy += s.c.height;
        else ox += s.c.width;
      }
      // footer
      let y = mapH + pad + line * 0.8;
      g.font = `600 ${13 * dpr}px system-ui, sans-serif`;
      g.fillStyle = '#e8a33d';
      g.fillText('GTNH Seed Map', pad, y);
      g.font = `${12 * dpr}px system-ui, sans-serif`;
      g.fillStyle = '#c9d1d9';
      const modeName = ['Biomes', 'Bee climate', 'Terrain', 'Blocks'][mapMode] ?? '';
      const blocks = view.zoom >= 0 ? `${2 ** view.zoom} px/block` : `${2 ** -view.zoom} blocks/px`;
      g.fillText(`${worldType === 1 ? 'RWG2_CONTINENTS' : 'RWG2'} · ${DIM_LABEL[dim] ?? 'dim ' + dim}${dim === 1 && layers.endKills ? ` (${layers.endKills} resets)` : ''} · ${modeName} · centre ${Math.round(view.x)}, ${Math.round(view.z)} · zoom ${view.zoom} (${blocks}) · GTNH 2.9.0 / RWG2`, pad + g.measureText('GTNH Seed Map').width + 60 * dpr, y);
      for (const s of shots) {
        y += line;
        g.fillStyle = s.tag === 'B' ? '#e8a33d' : '#c9d1d9';
        const sp = s.tag === 'B' ? cmpSpawn : spawnPt;
        g.fillText(`${s.tag ? s.tag + ' · ' : ''}seed ${s.seed}${sp && !dim ? ` · spawn ≈ ${sp.x}, ${sp.z}` : ''}`, pad, y);
      }
      if (legendLines) {
        y += line * 0.4;
        const colW = (phoneStack ? mapW : mapW / shots.length) / 2;
        shots.forEach((s, k) => {
          s.rows.forEach((e, i) => {
            const x = (phoneStack ? 0 : (k * mapW) / shots.length) + pad + (i % 2) * colW;
            const yy = y + line * (1 + Math.floor(i / 2));
            g.fillStyle = `rgb(${e.rgb[0]},${e.rgb[1]},${e.rgb[2]})`;
            g.fillRect(x, yy - 10 * dpr, 11 * dpr, 11 * dpr);
            g.fillStyle = '#c9d1d9';
            g.fillText(`${e.name}  ${Math.round(e.share * 100)}%`, x + 16 * dpr, yy);
          });
        });
      }
      const blob = await new Promise((res) => out.toBlob(res, 'image/png'));
      if (!blob) throw new Error('no image');
      const url = URL.createObjectURL(blob);
      const a = document.createElement('a');
      a.href = url;
      a.download = `gtnh-seedmap_${seedInfo?.seed ?? ''}${cmpOn && cmpInfo ? '_vs_' + cmpInfo.seed : ''}_${DIM_LABEL[dim]?.toLowerCase().replace(/\s+/g, '-') ?? dim}_${Math.round(view.x)}_${Math.round(view.z)}.png`;
      document.body.appendChild(a);
      a.click();
      a.remove();
      setTimeout(() => URL.revokeObjectURL(url), 30000);
      pngOpen = false;
    } catch (e) {
      console.error(e);
    } finally {
      pngBusy = false;
    }
  }

  // phone bottom sheet: grab handle drag (closed / half / full), tap cycles
  let asideEl = $state(null);
  let sheetFull = $state(false);
  let sheetDrag = $state(null); // { y0, h0, h, moved }
  function grabDown(e) {
    e.currentTarget.setPointerCapture?.(e.pointerId);
    const h0 = sheet === 'side' ? asideEl?.getBoundingClientRect().height ?? 0 : 44;
    sheetDrag = { y0: e.clientY, h0, h: h0, moved: false };
  }
  function grabMove(e) {
    if (!sheetDrag) return;
    const dy = e.clientY - sheetDrag.y0;
    if (!sheetDrag.moved && Math.abs(dy) < 6) return;
    if (!sheetDrag.moved && sheet !== 'side') { sheet = 'side'; popup = null; }
    const maxH = (asideEl?.parentElement?.clientHeight ?? window.innerHeight) - 16;
    sheetDrag = { ...sheetDrag, moved: true, h: Math.max(44, Math.min(maxH, sheetDrag.h0 - dy)) };
  }
  function grabUp() {
    const d = sheetDrag;
    sheetDrag = null;
    if (!d) return;
    if (!d.moved) {
      if (sheet !== 'side') { openSheet('side'); sheetFull = false; } else sheetFull = !sheetFull;
      return;
    }
    const f = d.h / ((asideEl?.parentElement?.clientHeight ?? window.innerHeight) || 1);
    if (f < 0.2) { sheet = null; sheetFull = false; } else { sheet = 'side'; sheetFull = f > 0.66; }
  }

  /** Distance from the map centre to a marker, in this dimension's blocks; Nether ↔ overworld scaled ×8, other dims null. */
  function markDist(p) {
    if (p.dim === dim) return Math.hypot(p.x - view.x, p.z - view.z);
    const f = p.dim === -1 && dim === 0 ? 8 : p.dim === 0 && dim === -1 ? 1 / 8 : 0;
    return f ? Math.hypot(p.x * f - view.x, p.z * f - view.z) : null;
  }
  function sortMarks(list, how) {
    if (how !== 'near') return list;
    return list.slice().sort((a, b) => (a[2] ?? Infinity) - (b[2] ?? Infinity));
  }

  function buildShown(list, lay, zoom, d) {
    return buildShownPure(list, lay, zoom, d, { layers, spawnPt, veinTitle, dimLegend, veinTable, veinColor, selectedPin, palette, nodeType, nodeAspect, nodeMin, markMarkers, hitMarkers, hitIdx, hiddenVeins, hiddenNodeKinds, hiddenFluids, fluidTable, fluidMin, strongholds, pins, LOOT_DISPLAY, MINE_DISPLAY });
  }

  $effect(() => {
    if (!renderer) return;
    renderer.showGrid = layers.grid;
    renderer.setGrouping(layers.groupMarkers, layers.groupPx);
    miniR?.setGrouping(layers.groupMarkers, layers.groupPx);
    const shown = buildShown(featureList, effLayers, view.zoom, dim);
    renderer.setMarkers(shown);
    shownMarkers = shown;
  });

  /** The selected marker if it is in the current dimension. */
  function selPinHere() {
    const p = pins[selectedPin];
    return p && p.dim === dim ? p : null;
  }
  function setTool(t) {
    tool = tool === t ? '' : t;
    popup = null;
    if (phone) sheet = tool === 'mark' ? 'tool' : null; // marker tool: show the type picker once
  }
  function snapPoint(sx, sy) {
    const m = markerAt(sx, sy); // snap onto a structure / vein / node marker
    if (m) return { x: m.x, z: m.z };
    const w = renderer.screenToWorld(sx, sy);
    return { x: Math.floor(w.x), z: Math.floor(w.z) };
  }
  function toolClick(sx, sy, add) {
    const p = snapPoint(sx, sy);
    if (tool === 'measure') {
      const cur = measureByDim[dim] ?? [];
      measureByDim[dim] = add || cur.length < 2 ? [...cur, p] : [p];
    } else if (tool === 'radius') {
      radiusByDim[dim] = { x: p.x, z: p.z, r: radiusByDim[dim]?.r ?? radiusR };
    } else if (tool === 'mark') {
      const w = renderer.screenToWorld(sx, sy);
      addPin(Math.floor(w.x), Math.floor(w.z), markType);
      if (!add && !keepPlacing) tool = '';
    } else if (tool === 'view') {
      const w = renderer.screenToWorld(sx, sy);
      tool = '';
      openStreetView(Math.floor(w.x), Math.floor(w.z));
    }
  }

  // ---- street view ----------------------------------------------------------------------------------------
  function openStreetView(x, z) {
    if (!seedInfo) return;
    popup = null;
    if (phone) sheet = null;
    svErr = '';
    svHud = null;
    svBiomes = null;
    svPick = null;
    svFeatAt = null;
    svFeat = [];
    sv = { x, z, dim, heading: svLast && svLast.dim === dim && Math.hypot(svLast.x - x, svLast.z - z) < 64 ? svLast.heading : 0, fly: dim === 1 ? true : undefined };
  }
  /** Jump inside the open 3D view (minimap click); far jumps rebuild the scene around the new point. */
  function svTeleport(x, z) {
    if (!svCtl || !sv) return;
    if (svCtl.teleport(x, z)) return;
    const h = svCtl.state;
    svCtl.destroy();
    svCtl = null;
    svBiomes = null;
    svPick = null;
    sv = { x, z, dim: sv.dim, heading: h.heading, fly: h.fly };
  }
  function closeStreetView() {
    const h = svCtl?.state;
    if (h && sv) {
      svLast = { x: Math.floor(h.x), z: Math.floor(h.z), heading: h.heading, dim: sv.dim };
      updateView({ x: h.x, z: h.z });
    }
    svCtl?.destroy();
    svCtl = null;
    sv = null;
    svHud = null;
    svPick = null;
    miniR?.destroy();
    miniR = null;
  }
  const svGroupOf = (k) => SV_GROUP_OF[k] ?? (k.startsWith('tc') ? 'thaum' : 'dimFeatures');
  const SV_ADV_GROUPS = new Set(['dungeons', 'mineshafts', 'thaum', 'nodes', 'geodes', 'lakes', 'ores', 'fluids']);
  const svGroupList = (o) => Object.entries(o.groups).filter(([k, v]) => v && (advanced || !SV_ADV_GROUPS.has(k))).map(([k]) => k);
  /** x-ray options as the 3D view gets them: population / ores / LootGames rooms are advanced-mode layers. */
  const svXray = (x) => (advanced ? { ...x } : { ...x, pop: false, ores: false, rooms: false });
  /** Layer flags for the 3D view's own feature fetch: its marker groups + what the x-ray needs; the map's
   *  sub-filters (hidden veins, node filters, likely-only Roguelike, hollow hills) still apply. */
  function svLayers(o) {
    const g = o.groups, x = o.xray;
    const adv = (v) => advanced && v;
    return {
      ...effLayers, hits: g.pins, pins: g.pins, villages: g.villages, strongholds: g.landmarks, spawn: g.landmarks,
      loot: adv(g.dungeons || (x.on && x.rooms)), rogue: adv(g.dungeons), mineshafts: adv(g.mineshafts), dimFeatures: g.dimFeatures, deEnd: adv(g.dimFeatures),
      tcRing: adv(g.thaum), tcHilltop: adv(g.thaum), tcMound: adv(g.thaum), tcTotem: adv(g.thaum), tcNodes: adv(g.nodes), geodes: adv(g.geodes || (x.on && x.pop)), lakes: adv(g.lakes || (x.on && x.pop)), oil: adv(g.lakes || g.fluids), vdungeons: adv(g.dungeons || (x.on && x.pop)), mineChests: adv(g.mineshafts),
      ores: adv(g.ores || (x.on && x.ores)), fluids: adv(g.fluids),
    };
  }
  // features around the 3D viewer (own fetch: follows walking / teleports; shares the map's cell cache)
  const SV_FEAT_R = 1024;
  let svFeat = $state.raw([]);
  let svFeatAt = null, svFeatTimer = 0;
  function svRefreshFeatures() {
    clearTimeout(svFeatTimer);
    svFeatTimer = 0;
    if (!sv || !seedInfo) return;
    const h = svCtl?.state;
    const c = h ? { x: h.x, z: h.z } : { x: sv.x, z: sv.z };
    svFeatAt = c;
    const R = SV_FEAT_R;
    svFeat = collectFeatures({
      a: { x: c.x - R, z: c.z - R }, b: { x: c.x + R, z: c.z + R }, zoom: -1, sv: true, cx: c.x, cz: c.z, d: sv.dim,
      lay: svLayers(untrack(() => $state.snapshot(svOpt))), onUpdate: () => (svFeatTimer ||= setTimeout(svRefreshFeatures, 300)),
    });
  }
  /** 3D marker records from the shown-marker list. */
  function svMarkerList(shown) {
    return shown
      .filter((m) => (m.label || m.box3) && !m.ghost && m.kind !== 'rogueArea' && m.kind !== 'inspect')
      .map((m) => ({
        x: m.x, z: m.z, y: Number.isFinite(m.tpY) ? m.tpY : undefined, label: m.label ? (m.icon && m.kind === 'pin' ? m.icon + ' ' : '') + m.label : '',
        color: m.color ?? '#ffe08a', group: svGroupOf(m.kind), detail: typeof m.detail === 'string' ? m.detail : null, box3: m.box3, xk: m.xk,
      }));
  }
  let svShown = $derived.by(() => (sv ? buildShown(svFeat, svLayers(svOpt), 0, sv.dim) : []));
  let svMarks = $derived(svMarkerList(svShown));
  // minimap: the same markers as map icons (no labels), filtered by the 3D view's groups
  let miniMarks = $derived.by(() => {
    const on = new Set(svGroupList(svOpt));
    return svShown.filter((m) => !m.ghost && m.kind !== 'rogueArea' && on.has(svGroupOf(m.kind)) && m.kind !== 'fortressPiece');
  });
  let miniVer = $state(0); // bumps when the minimap renderer is (re)created
  $effect(() => {
    const m = miniMarks;
    miniVer;
    miniR?.setMarkers(m);
  });
  $effect(() => {
    const m = svMarks;
    if (svReady && svCtl) svCtl.setMarkers(m);
  });
  // refetch when the viewer walks / jumps away, or the groups change
  $effect(() => {
    const h = svHud;
    if (!h || !sv) return;
    if (!svFeatAt || Math.hypot(h.x - svFeatAt.x, h.z - svFeatAt.z) > 256) untrack(svRefreshFeatures);
  });
  $effect(() => {
    JSON.stringify(svOpt.groups);
    advanced;
    svOpt.xray.on;
    svOpt.xray.ores;
    svOpt.xray.rooms;
    svOpt.xray.pop;
    if (untrack(() => svReady)) untrack(svRefreshFeatures);
  });
  /** Biome colour (0xRRGGBB) / name of a gen id in the street view's dimension. */
  function svBiome(gen, d) {
    if (d) {
      const b = dimLegend?.biomes.find((b) => b.id === gen);
      return b ? { name: b.name, rgb: b.rgb & 0xffffff } : null;
    }
    const g = palette?.groups.get(gen);
    return g ? { name: g.name, rgb: (g.rgb[0] << 16) | (g.rgb[1] << 8) | g.rgb[2] } : null;
  }
  let svBiomeRows = $derived.by(() => {
    if (!svBiomes || !sv) return [];
    const list = svBiomes[svOpt.legend] ?? [];
    let total = 0;
    for (const [, n] of list) total += n;
    if (!total) return [];
    return list
      .map(([gen, n]) => ({ gen, share: n / total, ...(svBiome(gen, sv.dim) ?? { name: `biome ${gen}`, rgb: 0x808080 }) }))
      .sort((a, b) => b.share - a.share);
  });
  let svHere = $derived(svHud?.biome != null && sv ? svBiome(svHud.biome, sv.dim) : null);
  // settings → live view
  let svReady = $derived(!!svHud);
  $effect(() => {
    const o = svOpt;
    const groups = svGroupList(o), tint = o.tint, water = o.water, xray = svXray(o.xray);
    if (!svReady || !svCtl) return;
    svCtl.setXray(xray);
    svCtl.setHeightColours(o.heightCol, false);
    svCtl.setTint(tint);
    svCtl.setWater(water);
    svCtl.setTrees?.(o.trees);
    svCtl.setGroups(groups);
  });
  // minimap: a second map renderer (own pool namespace) following the person
  $effect(() => {
    const c = miniCanvas, open = sv;
    if (!c || !open || !svOpt.mini) {
      if (miniR && (!c || !svOpt.mini || !open)) {
        miniR.destroy();
        miniR = null;
      }
      return;
    }
    if (miniR) return;
    miniR = new Renderer(c, pool, { ns: 'mini', mini: true });
    miniR.showGrid = false;
    if (palette) miniR.setPalette(palette);
    if (open.dim && dimLutCur) miniR.setDimLut(dimLutCur);
    miniR.setWorld(seedInfo.seed.toString(), worldType, open.dim);
    miniR.setRelief(layers.relief);
    miniR.setView({ x: open.x, z: open.z, zoom: svOpt.miniZoom });
    untrack(() => miniVer++);
  });
  $effect(() => {
    const m = svOpt.miniTerrain ? 2 : 0;
    miniVer;
    miniR?.setMode(m);
  });
  $effect(() => {
    const h = svHud, z = svOpt.miniZoom, big = svOpt.miniBig;
    if (!miniR || !h) return;
    big;
    miniR.setView({ x: h.x, z: h.z, zoom: z });
    miniR.setTools({ measure: [], radius: null, active: '', otherScale: null, viewer: { x: Math.floor(h.x), z: Math.floor(h.z), heading: h.heading } });
    miniR.invalidate();
  });
  function miniClick(e) {
    if (!miniR) return;
    const r = miniCanvas.getBoundingClientRect();
    const w = miniR.screenToWorld(e.clientX - r.left, e.clientY - r.top);
    svTeleport(Math.floor(w.x), Math.floor(w.z));
  }
  function miniZoom(d) {
    svOpt.miniZoom = Math.max(-6, Math.min(1, svOpt.miniZoom + d));
  }
  function miniWheel(e) {
    e.preventDefault();
    miniZoom(e.deltaY > 0 ? -1 : 1);
  }
  function svKey(k) {
    if (k === 'b') svOpt.tint = !svOpt.tint;
    else if (k === 'm') svOpt.mini = !svOpt.mini;
    else if (k === 'l') svOpt.panel = !svOpt.panel;
    else if (k === 'x') svOpt.xray.on = !svOpt.xray.on;
    else if (k === 'h') svOpt.heightCol = !svOpt.heightCol;
  }
  const svDir = (rel) => {
    const a = Math.abs(Math.round(rel));
    return a <= 10 ? 'ahead' : a >= 170 ? 'behind you' : `${a}° ${rel > 0 ? 'right' : 'left'}`;
  };
  $effect(() => {
    if (!sv || !svEl || svCtl) return;
    const open = sv, el = svEl;
    // start with what the map already shows; the 3D view's own fetch replaces it
    const marks = untrack(() => svMarkerList(shownMarkers));
    const o = untrack(() => $state.snapshot(svOpt));
    import('./lib/streetview.js')
      .then(({ createStreetView }) => {
        if (sv !== open || svCtl) return;
        svCtl = createStreetView(el, {
          pool, seed: seedInfo.seed.toString(), worldType, dim: open.dim, dimCode: open.dim === 1 ? dimCode(1) : open.dim, x: open.x, z: open.z, heading: open.heading, fly: open.fly, phone, spawn: open.dim === 0 ? untrack(() => spawnPt) : null, markers: marks,
          groups: svGroupList(o), tint: o.tint, heightColours: o.heightCol, water: o.water, trees: o.trees, xray: svXray(o.xray), biomeRgb: (g) => svBiome(g, open.dim)?.rgb ?? null,
          onState: (s) => (svHud = s),
          onBiomes: (b) => (svBiomes = b),
          onPick: (p) => (svPick = p),
          onBlock: (b) => svBlockPick(b, open.dim),
          onKey: svKey,
        });
      })
      .catch((e) => {
        const m = String(e?.message ?? e);
        svErr = /webgl/i.test(m)
          ? 'The 3D view needs WebGL, which this browser or device has turned off. Turning on hardware acceleration in the browser settings usually fixes it; the map works without it.'
          : `3D view failed to start: ${m}`;
      });
  });
  /** Street view long press / right click: look up the block (overworld: exact surface block + chunk biome;
   *  other dims: biome + ground Y). */
  function svBlockPick(b, d) {
    const seq = ++svBlockSeq;
    if (!b) {
      svBlock = null;
      return;
    }
    svBlock = { ...b, dim: d, info: null, dimInfo: null, copied: false };
    const S = seedInfo.seed.toString();
    const req = d
      ? Promise.all([
          pool.request({ type: 'inspect', seed: S, worldType, dim: dimCode(d), x: b.x, z: b.z }, { priority: -6 }),
          pool.request({ type: 'dimBlock', seed: S, dim: dimCode(d), x: b.x, y: b.y, z: b.z }, { priority: -6 }),
        ]).then(([r, k]) => ({ dimInfo: r.inspect, dimBlock: k.block }))
      : Promise.all([
          pool.request({ type: 'svBlock', seed: S, worldType, x: b.x, y: b.y, z: b.z }, { priority: -6 }),
          pool.request({ type: 'popColumn', seed: S, worldType, x: b.x, z: b.z }, { priority: -5 }),
        ]).then(([r, c]) => ({ info: r.block, under: popRuns(c.col) }));
    req.then((v) => seq === svBlockSeq && svBlock && (svBlock = { ...svBlock, ...v })).catch(() => {});
  }
  /** Populate-prefix features in one column (x-ray codes, y 0..191) as runs: lakes, monster rooms, geodes. */
  function popRuns(col) {
    const KIND = { 1: 'water lake', 2: 'lava lake', 6: 'abyssal geode', 7: 'monster room', 9: 'monster room', 10: 'monster room', 11: 'monster room' };
    const runs = [];
    for (let y = 0; y < col.length; y++) {
      const k = KIND[col[y]];
      if (!k) continue;
      const last = runs[runs.length - 1];
      if (last && last.k === k && last.y1 >= y - 4) last.y1 = y;
      else runs.push({ k, y0: y, y1: y });
    }
    return runs.reverse();
  }
  /** "BiomesOPlenty:overgrownNetherrack" → "Overgrown netherrack". */
  function prettyBlock(id) {
    const n = (id ?? '?').split(':').pop().replace(/^tile\./, '').replace(/^TF/, '');
    const w = n.replace(/_/g, ' ').replace(/([a-z])([A-Z])/g, '$1 $2').toLowerCase();
    return w.charAt(0).toUpperCase() + w.slice(1);
  }
  /** Registry id with metadata of a surface block code ("minecraft:sand:1"). */
  function blockId(code) {
    const n = blockById?.get(code)?.name ?? '?';
    const meta = code >= 64 && code < 80 ? code - 64 : code === 8 ? 1 : code === 4 ? 1 : code === 5 ? 2 : 0;
    return meta ? `${n}:${meta}` : n;
  }
  async function svCopyBlock() {
    const b = svBlock;
    const t = `${b.x} ${b.y} ${b.z}${b.info ? ` ${blockId(b.info.block)}` : b.dimBlock?.name ? ` ${b.dimBlock.name}` : ''}`;
    try {
      await navigator.clipboard.writeText(t);
      svBlock = { ...svBlock, copied: true };
    } catch {}
  }
  function svScreenshot() {
    const url = svCtl?.screenshot();
    if (!url) return;
    const a = document.createElement('a');
    a.href = url;
    a.download = `gtnh-view-${seedInfo.seed}-${Math.floor(svHud?.x ?? 0)}_${Math.floor(svHud?.z ?? 0)}.png`;
    a.click();
  }
  const compassName = (h) => COMPASS[Math.round(h / 45) % 8];
  let svTicks = $derived.by(() => {
    if (!svHud) return [];
    const out = [];
    for (let a = 0; a < 360; a += 15) {
      let d = a - svHud.heading;
      d = ((d + 540) % 360) - 180;
      if (Math.abs(d) <= 75) out.push({ d, label: a % 45 === 0 ? COMPASS[a / 45] : '', major: a % 45 === 0 });
    }
    return out;
  });
  function clearTool() {
    if (tool === 'measure') measureByDim[dim] = [];
    else if (tool === 'radius') radiusByDim[dim] = null;
    else if (tool === 'mark' || tool === 'view') tool = '';
  }
  function setRadiusCentre(p) {
    if (!p) return;
    radiusByDim[dim] = { x: Math.round(p.x), z: Math.round(p.z), r: radiusByDim[dim]?.r ?? radiusR };
    tool = 'radius';
  }
  function setRadiusR(v) {
    const r = Math.max(1, Math.min(100000, Math.round(Number(v) || 0)));
    radiusR = r;
    if (radiusByDim[dim]) radiusByDim[dim] = { ...radiusByDim[dim], r };
  }
  function radiusHandleAt(sx, sy) {
    const c = radiusByDim[dim];
    if (tool !== 'radius' || !c) return false;
    const h = renderer.worldToScreen(c.x + 0.5 + c.r, c.z + 0.5);
    return Math.hypot(h.sx - sx, h.sy - sy) < 14;
  }
  const fmt = (n) => Math.round(n).toLocaleString('en-US');
  let measureStats = $derived.by(() => {
    const pts = measureByDim[dim] ?? [];
    if (pts.length < 2) return null;
    let total = 0;
    for (let i = 1; i < pts.length; i++) total += Math.hypot(pts[i].x - pts[i - 1].x, pts[i].z - pts[i - 1].z);
    const a = pts[pts.length - 2], b = pts[pts.length - 1];
    return {
      total, segs: pts.length - 1, a, b, dx: b.x - a.x, dz: b.z - a.z,
      last: Math.hypot(b.x - a.x, b.z - a.z),
      dcx: Math.abs((b.x >> 4) - (a.x >> 4)), dcz: Math.abs((b.z >> 4) - (a.z >> 4)),
    };
  });
  // phone: one-line summary of the active tool (chip)
  let toolSummary = $derived.by(() => {
    if (tool === 'mark') return `📍 ${MARK_TYPES[markType]?.icon ?? ''} ${MARK_TYPES[markType]?.name ?? ''} · tap the map`;
    if (tool === 'view') return '👁 tap where to stand';
    if (tool === 'measure') {
      if (!measureStats) return (measureByDim[dim] ?? []).length ? '📏 tap point B' : '📏 tap point A';
      return `📏 ${fmt(measureStats.total)} blocks${OTHER_DIM[dim] ? ` · ${fmt(measureStats.total * OTHER_DIM[dim].f)} ${OTHER_DIM[dim].name}` : ''}`;
    }
    if (tool === 'radius') {
      const c = radiusByDim[dim];
      if (!c) return '◯ tap a centre';
      const n = radiusContents ? radiusContents.kinds.reduce((a, [, k]) => a + k, 0) + radiusContents.ores.reduce((a, [, k]) => a + k, 0) : 0;
      return `◯ r ${fmt(c.r)} · ${n} inside`;
    }
    return '';
  });
  let radiusContents = $derived.by(() => {
    const c = radiusByDim[dim];
    if (!c) return null;
    const counts = new Map(), ores = new Map();
    for (const m of shownMarkers) {
      if (['pin', 'origin', 'fortressPiece', 'rogueArea', 'hit'].includes(m.kind)) continue;
      if (Math.hypot(m.x - c.x, m.z - c.z) > c.r) continue;
      if (m.kind === 'ore') ores.set(m.name, (ores.get(m.name) ?? 0) + 1);
      else {
        const k = m.kind === 'node' && m.ntype ? `${KIND_NAMES.node} (${m.ntype.toLowerCase()})` : KIND_NAMES[m.kind] ?? m.kind;
        counts.set(k, (counts.get(k) ?? 0) + 1);
      }
    }
    return {
      kinds: [...counts].sort((a, b) => b[1] - a[1]),
      ores: [...ores].sort((a, b) => b[1] - a[1]),
      area: Math.PI * c.r * c.r,
    };
  });
  // the other dimension's coordinate of a point (overworld <-> Nether only)
  function otherCoord(p) {
    const o = OTHER_DIM[dim];
    return o && p ? { name: o.name, dim: o.dim, x: Math.floor(p.x * o.f), z: Math.floor(p.z * o.f) } : null;
  }
  function jumpOther(p) {
    const o = otherCoord(p ?? { x: view.x, z: view.z });
    if (!o) return;
    dim = o.dim;
    updateView({ x: o.x + 0.5, z: o.z + 0.5 });
  }
  function useAsSearch() {
    const c = radiusByDim[dim];
    if (!c) return;
    searchPreset = { x: c.x, z: c.z, r: c.r, n: (searchPreset?.n ?? 0) + 1 };
    tab = 'search';
    if (phone) sheet = 'side';
  }
  $effect(() => {
    if (!renderer) return;
    renderer.setTools({ measure: measureByDim[dim] ?? [], radius: radiusByDim[dim], active: tool, otherScale: OTHER_DIM[dim]?.f ?? null, viewer: svLast && svLast.dim === dim ? svLast : null });
  });

  /** Standing Y: the column is solid up to floor(height) (exact chunk terrain, before caves/trees); water to 62.
   *  +1 margin for grass/snow layers. */
  function surfaceY(h) {
    return h == null ? 200 : Math.max(Math.floor(h), 62) + 2;
  }
  function tpCommand(p) {
    return `/tp ${p.x} ${surfaceY(p.height)} ${p.z}`;
  }

  async function copyTp(i) {
    const text = tpCommand(pins[i]);
    try {
      await navigator.clipboard.writeText(text);
      pinCopied = i;
      setTimeout(() => (pinCopied = -1), 1200);
    } catch {
      prompt = text;
    }
  }

  // ---- user markers --------------------------------------------------------------------------------------------
  /** Where a portal marker leads: the other dimension's coordinate and the vanilla / TF existing-portal search range. */
  function portalTarget(p) {
    const L = PORTAL_LINKS[p?.type];
    const t = L?.to[p.dim];
    return t ? { dim: t.dim, x: Math.floor(p.x * t.f), z: Math.floor(p.z * t.f), range: L.range } : null;
  }
  /** The portal marker on the other side that this one would link to (nearest within the search box), if any. */
  function portalPartner(p, list = pins) {
    const t = portalTarget(p);
    if (!t) return null;
    let best = null, bd = Infinity;
    list.forEach((q, i) => {
      if (q.type !== p.type || q.dim !== t.dim) return;
      if (Math.max(Math.abs(q.x - t.x), Math.abs(q.z - t.z)) > t.range) return;
      const d = Math.hypot(q.x - t.x, q.z - t.z);
      if (d < bd) (best = i), (bd = d);
    });
    return best == null ? null : { idx: best, dist: bd };
  }
  function markName(p) {
    const t = MARK_TYPES[p.type] ?? MARK_TYPES.pin;
    return p.label || (p.type === 'pin' ? `${p.x}, ${p.z}` : t.name);
  }
  /** Map markers for the current dimension: own markers + "ghosts" where other-dimension portals land here. */
  function markMarkers(list, d, sel) {
    const out = [];
    list.forEach((p, i) => {
      if (p.dim !== d) return;
      const t = MARK_TYPES[p.type] ?? MARK_TYPES.pin;
      out.push({ x: p.x, z: p.z, kind: 'pin', idx: i, mtype: p.type, color: t.color, icon: t.icon, global: p.scope === 'global', selected: i === sel, label: markName(p) });
    });
    list.forEach((p, i) => {
      const tg = portalTarget(p);
      if (!tg || tg.dim !== d) return;
      const t = MARK_TYPES[p.type];
      const pr = portalPartner(p, list);
      out.push({
        x: tg.x, z: tg.z, kind: 'pin', ghost: true, idx: i, mtype: p.type, color: t.color, icon: t.icon,
        linkTo: pr ? { x: list[pr.idx].x, z: list[pr.idx].z } : null,
        label: pr ? null : `${markName(p)} (${DIM_LABEL[p.dim]}) lands here`, // linked: the dashed line says it
      });
    });
    return out;
  }
  function biomeNameOf(i, d = dim) {
    if (!i) return null;
    if (d) return dimLegend?.biomes.find((b) => b.id === i.dimBiome)?.name ?? null;
    const g = palette?.groups.get(i.exactGenId) ?? palette?.byId.get(i.marker ? LAVA_MARKER : i.rwgId)?.group;
    return g?.name ?? null;
  }
  function addPin(x, z, type = 'pin', label = '') {
    const d = dim;
    const pin = { x, z, dim: d, type, label, notes: '', scope: markScope, name: '', genId: null, meta: null, continent: null, height: null };
    pins = [...pins, pin];
    selectedPin = pins.length - 1;
    savePins();
    const idx = pins.length - 1;
    pool
      .request({ type: 'inspect', seed: seedInfo.seed.toString(), worldType, dim: dimCode(d), x, z }, { priority: -5 })
      .then((r) => {
        const i = r.inspect;
        if (!pins[idx] || pins[idx].x !== x || pins[idx].z !== z || pins[idx].dim !== d) return;
        pins[idx] = {
          ...pins[idx], name: biomeNameOf(i, d) ?? '?', genId: d ? null : i.exactGenId ?? null, meta: i.meta ?? null, continent: i.continent ?? null,
          height: i.height ?? null, bees: !d && i.beeTemp != null ? i.beeTemp | (i.beeHumidity << 4) : null,
        };
        savePins();
      })
      .catch(() => {});
    return idx;
  }
  function updateMark(i, patch) {
    if (!pins[i]) return;
    pins[i] = { ...pins[i], ...patch };
    savePins();
  }
  function goToMark(i) {
    const p = pins[i];
    if (!p) return;
    selectedPin = i;
    if (p.dim !== dim) dim = p.dim;
    updateView({ x: p.x + 0.5, z: p.z + 0.5 });
  }
  function markAsSearchCentre(p) {
    searchPreset = { x: p.x, z: p.z, r: null, n: (searchPreset?.n ?? 0) + 1 };
    tab = 'search';
    popup = null;
    if (phone) sheet = 'side';
  }
  function measureFrom(p) {
    measureByDim[dim] = [{ x: p.x, z: p.z }];
    tool = 'measure';
    popup = null;
  }

  function removePin(i) {
    pins = pins.filter((_, j) => j !== i);
    selectedPin = -1;
    if (popup?.m.kind === 'pin') popup = null;
    savePins();
  }

  let popup = $state(null); // { m, ground }
  let popupPos = $derived.by(() => {
    view; // reposition on pan/zoom
    if (!popup || !renderer) return null;
    return renderer.worldToScreen(popup.m.x + 0.5, popup.m.z + 0.5);
  });
  let popupCopied = $state(false);
  function openPopup(m) {
    if (phone) sheet = null;
    popup = { m, ground: null, info: null };
    popupCopied = false;
    const d = dim;
    pool
      .request({ type: 'inspect', seed: seedInfo.seed.toString(), worldType, dim: dimCode(d), x: m.x, z: m.z }, { priority: -5 })
      .then((r) => {
        // popup is a $state proxy: compare by identity of position/kind, not object
        if (popup && dim === d && popup.m.x === m.x && popup.m.z === m.z && popup.m.kind === m.kind) popup = { ...popup, ground: r.inspect.height != null && r.inspect.height >= 0 ? r.inspect.height : null, info: r.inspect };
      })
      .catch(() => {});
  }
  async function copyPopupTp() {
    const text = `/tp ${popup.m.x} ${popup.m.tpY ?? surfaceY(popup.ground)} ${popup.m.z}`;
    try {
      await navigator.clipboard.writeText(text);
      popupCopied = true;
    } catch {
      prompt = text;
    }
  }
  function markerAt(sx, sy) {
    // what the renderer actually drew (grouped members are not pickable on their own; user markers win ties)
    return renderer.pickAt(sx, sy);
  }
  /** Click on a group badge: zoom in about it until it splits; at max zoom list the members. */
  function openGroup(g, sx, sy) {
    if (view.zoom < MAX_ZOOM - 0.01) {
      zoomAt(sx, sy, Math.min(2, MAX_ZOOM - view.zoom));
      return;
    }
    openPopup(g);
  }

  function onClickMap(sx, sy, add = false) {
    if (tool) {
      toolClick(sx, sy, add);
      return;
    }
    const hitMarker = markerAt(sx, sy);
    if (hitMarker?.kind === 'group') {
      openGroup(hitMarker, sx, sy);
      return;
    }
    if (hitMarker) {
      if (hitMarker.kind === 'pin' && !hitMarker.ghost) selectedPin = hitMarker.idx;
      openPopup(hitMarker);
      return;
    }
    if (popup) {
      popup = null; // first click elsewhere just closes the popup
      return;
    }
    // plain click: inspect the spot (markers are only placed with the marker tool, P)
    const w = renderer.screenToWorld(sx, sy);
    openPopup({ x: Math.floor(w.x), z: Math.floor(w.z), kind: 'inspect' });
  }

  let activeResult = $state(null); // picked search result: { seed, center, hits, nots }
  // flat-area overlay of the picked result's flat hit (approx tops of the patch the search measured)
  let flatPatch = $state(null); // { key, cells, step, lo, hi, land, info } | { key, loading } | { key, error }
  let flatShown = $state(true);
  $effect(() => {
    const r = activeResult;
    const h = r?.hits.find((x) => x.flatQ);
    if (!h) {
      flatPatch = null;
      return;
    }
    const key = `${r.seed}|${h.x}|${h.z}`;
    if (flatPatch?.key === key) return;
    flatPatch = { key, loading: true };
    const { worldType: wt, ...fq } = h.flatQ;
    pool
      .request({ type: 'flatPatch', seed: r.seed, worldType: wt, query: JSON.stringify(fq), exact: false }, { priority: -5 })
      .then((m) => {
        if (flatPatch?.key !== key) return;
        const v = m.result;
        flatPatch = v.error || !v.band ? { key, error: v.error ?? 'patch no longer passes' } : { key, cells: v.cells, step: v.step, lo: v.band[0], hi: v.band[1], land: fq.flat.land, info: v.info, n: v.cells.length / 3 };
      })
      .catch((e) => flatPatch?.key === key && (flatPatch = { key, error: String(e) }));
  });
  $effect(() => {
    const on = flatShown && flatPatch?.cells && activeResult && seedInfo && activeResult.seed === seedInfo.seed.toString() && dim === (activeResult.dim ?? 0);
    renderer?.setFlat(on ? flatPatch : null);
  });
  // layer that shows a search condition's target
  const HIT_LAYER = { aspect: 'tcNodes', village: 'villages', lootgames: 'loot', roguelike: 'rogue', stronghold: 'strongholds', 'tc:2': 'tcRing', 'tc:3': 'tcHilltop', 'tc:1': 'tcMound', 'tc:4': 'tcTotem', tf: 'dimFeatures', fortress: 'dimFeatures', villageStyle: 'villages', strongholdSurface: 'strongholds', mineshaft: 'mineshafts', nodeType: 'tcNodes', geode: 'geodes', lake: 'lakes' };
  function hitLayer(what) {
    if (!what) return null;
    if (what.startsWith('vein:')) return 'ores';
    if (what.startsWith('fluid:')) return 'fluids';
    return HIT_LAYER[what] ?? null;
  }
  function goToHit(h) {
    hitIdx = activeResult?.hits.indexOf(h) ?? -1;
    updateView({ x: h.x, z: h.z, zoom: Math.max(view.zoom, 0) });
  }
  /** Next / previous search hit (wraps). */
  function stepHit(d) {
    const hs = activeResult?.hits ?? [];
    if (!hs.length) return;
    hitIdx = (((hitIdx < 0 ? (d > 0 ? -1 : 0) : hitIdx) + d) % hs.length + hs.length) % hs.length;
    updateView({ x: hs[hitIdx].x, z: hs[hitIdx].z, zoom: Math.max(view.zoom, 0) });
  }

  function applyPreset(on) {
    layers = { ...layers, ...Object.fromEntries(FEATURE_KEYS.map((k) => [k, !!on[k]])) };
  }
  const presetActive = (on) => FEATURE_KEYS.every((k) => !!layers[k] === !!on[k]);

  // ---- right-click menu
  function openCtx(e, force = false) {
    e.preventDefault();
    if ((phone && !force) || !renderer) return;
    const r = canvas.getBoundingClientRect();
    const sx = e.clientX - r.left, sy = e.clientY - r.top;
    const w = renderer.screenToWorld(sx, sy);
    const x = Math.floor(w.x), z = Math.floor(w.z);
    ctxMenu = { sx: Math.max(4, Math.min(sx, r.width - 250)), sy: Math.max(4, Math.min(sy, r.height - 300)), x, z, ground: null, copied: false };
    popup = null;
    pool
      .request({ type: 'inspect', seed: seedInfo.seed.toString(), worldType, dim: dimCode(dim), x, z }, { priority: -5 })
      .then((res) => {
        if (ctxMenu && ctxMenu.x === x && ctxMenu.z === z) ctxMenu = { ...ctxMenu, ground: res.inspect.height != null && res.inspect.height >= 0 ? res.inspect.height : null };
      })
      .catch(() => {});
  }
  async function ctxCopyTp() {
    const t = `/tp ${ctxMenu.x} ${surfaceY(ctxMenu.ground)} ${ctxMenu.z}`;
    try {
      await navigator.clipboard.writeText(t);
      ctxMenu = { ...ctxMenu, copied: true };
      setTimeout(() => (ctxMenu = null), 600);
    } catch {
      prompt = t;
      ctxMenu = null;
    }
  }
  function ctxAction(k) {
    if (k === 'tp') return ctxCopyTp();
    const p = { x: ctxMenu.x, z: ctxMenu.z };
    ctxMenu = null;
    ({
      mark: () => addPin(p.x, p.z),
      view: () => openStreetView(p.x, p.z),
      search: () => markAsSearchCentre(p),
      measure: () => measureFrom(p),
      radius: () => setRadiusCentre(p),
      centre: () => updateView({ x: p.x + 0.5, z: p.z + 0.5 }),
      other: () => jumpOther(p),
    })[k]?.();
  }

  // ---- vein finder: nearest loaded vein of a type from the map centre, then the next nearest
  function findVein(i) {
    if (!veinFind || veinFind.vi !== i) {
      const list = featureList
        .filter((f) => f.kind === 'ore' && (f.a & 0xff) === i)
        .map((f) => ({ x: f.x, z: f.z }))
        .sort((a, b) => Math.hypot(a.x - view.x, a.z - view.z) - Math.hypot(b.x - view.x, b.z - view.z));
      veinFind = { vi: i, list, idx: -1 };
    }
    if (!veinFind.list.length) return;
    if (hiddenVeins.has(i)) {
      const s = new Set(hiddenVeins);
      s.delete(i);
      hiddenVeins = s;
    }
    if (!layers.ores) layers.ores = true;
    const idx = (veinFind.idx + 1) % veinFind.list.length;
    veinFind = { ...veinFind, idx };
    const v = veinFind.list[idx];
    updateView({ x: v.x, z: v.z, zoom: Math.max(view.zoom, 1) });
    if (phone) sheet = null;
  }

  // ---- loading pill: feature cells still being computed, by layer
  const LOADING_NAMES = { gt: 'ore veins', fl: 'fluids', nodes1k: 'aura nodes', geo: 'geodes', lk: 'lakes', oil: 'oil deposits', oilc: 'oil deposits', vd: 'monster rooms', tc: 'Thaumcraft', tcr: 'Thaumcraft', spawnOres: 'spawn veins' };
  function pollLoading() {
    const n = new Map();
    for (const k of featureInflight) {
      const tail = k.slice(k.lastIndexOf('|') + 1);
      const name = LOADING_NAMES[tail] ?? (k.includes('|d') ? 'dimension features' : 'structures');
      n.set(name, (n.get(name) ?? 0) + 1);
    }
    const next = [...n.entries()].sort((a, b) => b[1] - a[1]);
    if (JSON.stringify(next) !== JSON.stringify(loading)) loading = next;
  }

  // ---- phone: long-press = the right-click menu
  let pressTimer = 0;
  function pressStart(e) {
    if (!phone || e.pointerType === 'mouse') return;
    clearTimeout(pressTimer);
    const { clientX, clientY } = e;
    pressTimer = setTimeout(() => {
      if (down && !down.moved && pointers.size === 1) {
        down = null; // no click after the menu
        openCtx({ preventDefault() {}, clientX, clientY }, true);
      }
    }, 550);
  }

  // ---- sidebar resize (desktop)
  let sideDrag = null;
  function sideDown(e) {
    sideDrag = { x0: e.clientX, w0: layers.sideW };
    e.currentTarget.setPointerCapture(e.pointerId);
  }
  function sideMove(e) {
    if (!sideDrag) return;
    layers.sideW = Math.max(240, Math.min(560, Math.round(sideDrag.w0 - (e.clientX - sideDrag.x0))));
  }
  function sideUp() {
    sideDrag = null;
  }
  function highlightHit(h) {
    if (h.genId == null) return;
    mapMode = mapMode === 1 || mapMode === 3 ? 0 : mapMode;
    highlight = new Set([h.genId]);
  }

  function pickResult(r) {
    if (phone) sheet = null; // show the map
    if (r.dim != null && r.dim !== dim) dim = r.dim; // a Nether / Twilight Forest match opens that dimension
    const hits = r.hits.map((h) => ({ ...h, label: h.label }));
    activeResult = { ...r, hits };
    // make sure the matched features are visible
    const on = { hits: true };
    for (const h of hits) {
      const l = hitLayer(h.what);
      if (l) on[l] = true;
      if (h.what === 'loot') on.vdungeons = on.mineChests = true;
    }
    layers = { ...layers, ...on };
    if (parseSeed(r.seed)?.seed === seedInfo?.seed) hitMarkers = hits;
    else pendingHits = hits;
    hitIdx = -1;
    seedText = r.seed;
    const [cx, cz] = r.center ?? [0, 0];
    const far = Math.max(256, ...r.hits.map((h) => Math.hypot(h.x - cx, h.z - cz)));
    const r0 = canvas.getBoundingClientRect();
    const zoom = Math.floor(Math.log2(Math.min(r0.width, r0.height) / 2.4 / far) * 2) / 2;
    updateView({ x: cx, z: cz, zoom });
  }

  // highlight holds in-game biome ids; the renderer works on rwg ids (bee mode: climate codes)
  $effect(() => {
    if (!renderer || !palette) return;
    renderer.setMode(mapMode);
    renderer.setRelief(layers.relief);
    renderer.setDeco(!!effLayers.blockDeco);
    renderer.setBiomeOverlay(mapMode === 2 ? Number(layers.biomeOverlay) || 0 : 0);
    if (mapMode === 2 && !(Number(layers.biomeOverlay) > 0)) {
      renderer.setHighlight(null);
      return;
    }
    if (mapMode === 1) {
      renderer.setHighlight(new Set(beeHighlight));
      return;
    }
    const rwg = new Set();
    for (const g of highlight) for (const id of palette.groups.get(g)?.rwgIds ?? []) rwg.add(id);
    renderer.setHighlight(rwg);
  });

  $effect(() => {
    advanced;
    layers.loot;
    layers.rogue;
    layers.tcRing;
    layers.tcHilltop;
    layers.tcMound;
    layers.tcTotem;
    layers.ores;
    layers.fluids;
    layers.villages;
    layers.mineshafts;
    layers.dimFeatures;
    layers.tcNodes;
    layers.geodes;
    layers.lakes;
    layers.lakesLava;
    layers.oil;
    layers.vdungeons;
    layers.mineChests;
    layers.endKills;
    layers.deEnd;
    layers.tfHideHills;
    nodeAspect;
    nodeMin;
    nodeType;
    hiddenNodeKinds;
    dim;
    dimLegend;
    hiddenVeins;
    hiddenFluids;
    fluidMin;
    refreshFeatures();
  });

  function toggleBee(code) {
    const s = new Set(beeHighlight);
    s.has(code) ? s.delete(code) : s.add(code);
    beeHighlight = s;
  }
  let dimShares = $derived.by(() => {
    const m = new Map();
    if (!dim) return m;
    for (const s of stats) m.set(s.id, (m.get(s.id) ?? 0) + s.share);
    return m;
  });
  let beeShares = $derived.by(() => {
    const m = new Map();
    if (mapMode !== 1) return m;
    for (const s of stats) m.set(s.id, (m.get(s.id) ?? 0) + s.share);
    return m;
  });

  $effect(() => {
    const mode = paletteMode;
    if (!renderer || !legendData) return;
    palette = buildPalette(legendData, mode);
    renderer.setPalette(palette);
  });

  // ---- interaction ----------------------------------------------------------------------------------------
  const pointers = new Map();
  let pinch = null;

  function updateView(v) {
    view = { ...view, ...v };
    view.zoom = Math.max(MIN_ZOOM, Math.min(MAX_ZOOM, view.zoom));
    renderer.setView(view);
    cmpR?.setView(view);
    refreshCmp();
    renderer.scheduleStats();
    refreshFeatures();
    writeHash();
  }

  /** Zoom about the map centre by `dz` levels, snapping to whole levels. */
  function zoomCentre(dz) {
    const r = canvas.getBoundingClientRect();
    const target = Math.abs(dz) === 1 ? (dz > 0 ? Math.floor(view.zoom + 1e-6) + 1 : Math.ceil(view.zoom - 1e-6) - 1) : view.zoom + dz;
    zoomAt(r.width / 2, r.height / 2, target - view.zoom);
  }

  function zoomAt(sx, sy, dz) {
    const before = renderer.screenToWorld(sx, sy);
    const zoom = Math.max(MIN_ZOOM, Math.min(MAX_ZOOM, view.zoom + dz));
    renderer.setView({ zoom });
    const after = renderer.screenToWorld(sx, sy);
    updateView({ zoom, x: view.x + before.x - after.x, z: view.z + before.z - after.z });
  }

  function onWheel(e) {
    e.preventDefault();
    const r = canvas.getBoundingClientRect();
    const dz = -Math.sign(e.deltaY) * Math.min(1, Math.abs(e.deltaY) / 100) * 0.5;
    zoomAt(e.clientX - r.left, e.clientY - r.top, dz);
  }

  let down = null;
  function onPointerDown(e) {
    if (e.button === 2) return; // right button: context menu
    if (ctxMenu) ctxMenu = null;
    canvas.setPointerCapture(e.pointerId);
    {
      const r = canvas.getBoundingClientRect();
      if (pointers.size === 0 && radiusHandleAt(e.clientX - r.left, e.clientY - r.top)) {
        radiusDrag = { pointerId: e.pointerId };
        return;
      }
    }
    down = pointers.size === 0 ? { x: e.clientX, y: e.clientY, moved: false } : null;
    pointers.set(e.pointerId, { x: e.clientX, y: e.clientY });
    pressStart(e);
    if (pointers.size === 2) {
      const [a, b] = [...pointers.values()];
      pinch = { d: Math.hypot(a.x - b.x, a.y - b.y), zoom: view.zoom };
    }
  }

  let inspectTimer = 0;
  function onPointerMove(e) {
    const r = canvas.getBoundingClientRect();
    const sx = e.clientX - r.left, sy = e.clientY - r.top;
    if (radiusDrag && radiusDrag.pointerId === e.pointerId) {
      const c = radiusByDim[dim];
      const w = renderer.screenToWorld(sx, sy);
      if (c) setRadiusR(Math.hypot(w.x - c.x - 0.5, w.z - c.z - 0.5));
      return;
    }
    const prev = pointers.get(e.pointerId);
    if (down && Math.hypot(e.clientX - down.x, e.clientY - down.y) > 4) down.moved = true;
    if (prev) {
      if (pointers.size === 1) {
        updateView({ x: view.x - (e.clientX - prev.x) / renderer.ppb, z: view.z - (e.clientY - prev.y) / renderer.ppb });
      } else if (pinch && pointers.size === 2) {
        pointers.set(e.pointerId, { x: e.clientX, y: e.clientY });
        const [a, b] = [...pointers.values()];
        const d = Math.hypot(a.x - b.x, a.y - b.y);
        const mid = { x: (a.x + b.x) / 2 - r.left, y: (a.y + b.y) / 2 - r.top };
        zoomAt(mid.x, mid.y, pinch.zoom + Math.log2(d / pinch.d) - view.zoom);
      }
      pointers.set(e.pointerId, { x: e.clientX, y: e.clientY });
    }
    const w = renderer.screenToWorld(sx, sy);
    const x = Math.floor(w.x), z = Math.floor(w.z);
    hover = { x, z, id: renderer.idAt(x, z) };
    if (cmpOn) cmpCursor = { sx, sy, x, z, side: 'a' };
    clearTimeout(inspectTimer);
    inspectTimer = setTimeout(() => inspect(x, z), 120);
  }

  function onPointerUp(e) {
    if (radiusDrag && radiusDrag.pointerId === e.pointerId) {
      radiusDrag = null;
      return;
    }
    if (down && !down.moved && pointers.size === 1 && e.type === 'pointerup') {
      const r = canvas.getBoundingClientRect();
      onClickMap(e.clientX - r.left, e.clientY - r.top, e.shiftKey);
    }
    down = null;
    pointers.delete(e.pointerId);
    if (pointers.size < 2) pinch = null;
  }

  function inspect(x, z) {
    if (!seedInfo) return;
    pool
      .request({ type: 'inspect', seed: seedInfo.seed.toString(), worldType, dim: dimCode(dim), x, z }, { priority: -5 })
      .then((r) => {
        if (hover && hover.x === x && hover.z === z) exact = { x, z, ...r.inspect };
      })
      .catch(() => {});
  }

  function onKey(e) {
    if (e.target instanceof HTMLInputElement || e.target instanceof HTMLTextAreaElement || e.target instanceof HTMLSelectElement || e.target?.isContentEditable) return;
    if (e.ctrlKey || e.metaKey || e.altKey) return;
    if (sv) {
      if (e.key === 'Escape') closeStreetView();
      return; // the 3D view owns the keyboard (WASD, space / shift, F)
    }
    if (ctxMenu && e.key === 'Escape') {
      ctxMenu = null;
      return;
    }
    if (e.key === '?') {
      keyHelp = !keyHelp;
      return;
    }
    if (keyHelp && e.key === 'Escape') {
      keyHelp = false;
      return;
    }
    if (activeResult && (e.key === ']' || e.key === '[')) {
      stepHit(e.key === ']' ? 1 : -1);
      return;
    }
    const r = canvas.getBoundingClientRect();
    if (e.key === '+' || e.key === '=') zoomAt(r.width / 2, r.height / 2, 0.5);
    else if (e.key === '-') zoomAt(r.width / 2, r.height / 2, -0.5);
    else if (e.key === '0') updateView({ x: 0, z: 0 });
    else if (e.key === 'm' || e.key === 'M') setTool('measure');
    else if (e.key === 'r' || e.key === 'R') setTool('radius');
    else if (e.key === 'p' || e.key === 'P') setTool('mark');
    else if (e.key === 'v' || e.key === 'V') setTool('view');
    else if ((e.key === 'n' || e.key === 'N') && OTHER_DIM[dim]) jumpOther(selPinHere());
    else if (e.key === 'Escape' && popup) popup = null;
    else if (e.key === 'Escape' && tool) {
      const empty = tool === 'mark' || tool === 'view' || (tool === 'measure' ? !(measureByDim[dim] ?? []).length : !radiusByDim[dim]);
      empty ? (tool = '') : clearTool();
    }
    else if (e.key === 'c' && selectedPin >= 0) copyTp(selectedPin);
    else if ((e.key === 'Delete' || e.key === 'Backspace') && selectedPin >= 0) removePin(selectedPin);
  }

  function goto() {
    const m = gotoText.match(/(-?\d+)\s*[, ]\s*(?:-?\d+\s*[, ]\s*)?(-?\d+)/);
    if (m) updateView({ x: Number(m[1]), z: Number(m[2]), zoom: Math.max(view.zoom, -1) });
  }

  function toggle(id) {
    const s = new Set(highlight);
    s.has(id) ? s.delete(id) : s.add(id);
    highlight = s;
  }

  // inside a sandboxed embed (e.g. the claude.ai artifact) URL state doesn't survive: copy plain text there
  const embedded = (() => {
    try {
      return window.top !== window;
    } catch {
      return true;
    }
  })();
  async function share() {
    const type = worldType === 0 ? 'RWG2' : 'RWG2_CONTINENTS';
    const p = hashParams(String(seedInfo?.seed ?? seedText)); // dimension included (it was dropped before v1.67)
    const dimName = DIMS.find((d) => d.id === dim)?.label;
    const text = embedded
      ? `seed ${seedInfo?.seed ?? seedText} · ${type}${dim ? ` · ${dimName ?? `dim ${dim}`}` : ''} · X ${Math.round(view.x)} Z ${Math.round(view.z)}`
      : `${location.origin}${location.pathname}#${p}`;
    try {
      await navigator.clipboard.writeText(text);
      copied = true;
    } catch {
      copied = false;
      prompt = text; // clipboard refused: show it for manual copy
    }
    setTimeout(() => (copied = false), 1500);
  }
  let prompt = $state('');
  let fatal = $state('');

  $effect(() => {
    const p = layers.terrainPreset;
    if (setTerrainPreset(p)) {
      renderer?.recolourTerrain();
      miniR?.recolourTerrain();
      svCtl?.setHeightColours(untrack(() => svOpt.heightCol), true);
    }
  });
  function rampCss(h, _preset) {
    const v = terrainColor(h * 8);
    return `rgb(${v & 255},${(v >> 8) & 255},${(v >> 16) & 255})`;
  }

  // full-height colour bar for the terrain legend (Y0 at the bottom, 255 at the top; colour codes = map heights)
  const RAMP_H = 384;
  let rampAt = $state(null);
  function drawRamp(cv, preset) {
    const paint = () => {
      const g = cv.getContext('2d');
      for (let y = 0; y < RAMP_H; y++) {
        const v = terrainColor(Math.round((255 - ((y + 0.5) / RAMP_H) * 255) * 8));
        g.fillStyle = `rgb(${v & 255},${(v >> 8) & 255},${(v >> 16) & 255})`;
        g.fillRect(0, y, 28, 1);
      }
    };
    // the preset effect (setTerrainPreset) runs after the template update: paint on the next frame
    requestAnimationFrame(paint);
    return { update: () => requestAnimationFrame(paint) };
  }
  /** Map height code → label (dims are shifted so their sea level / lava reads like Y63). */
  function rampLabel(h) {
    if (dim === 1) return h <= 0 ? 'void' : `Y${h}`;
    if (dim === 7) return h === 63 ? 'Y31 · sea' : `Y${h - 32}`;
    if (dim === -1) return h < 63 ? 'lava' : `Y${h - 31}`;
    return h === 63 ? 'Y63 · sea level' : `Y${h}`;
  }
  function rampTicks(p) {
    const step = p === 'bands16' ? 32 : p === 'contour8' ? 24 : 20;
    const t = [];
    for (let h = 0; h <= 255; h += step) if (Math.abs(h - 63) > 8) t.push(h);
    t.push(63);
    if (p === 'steps2' || p === 'steps4') t.push(40);
    return [...new Set(t)].filter((h) => h <= 250);
  }
  function rampHover(e) {
    const r = e.currentTarget.getBoundingClientRect();
    rampAt = Math.max(0, Math.min(255, Math.round(255 - ((e.clientY - r.top) / RAMP_H) * 255)));
  }
  function hex(rgb) {
    return `rgb(${rgb[0]},${rgb[1]},${rgb[2]})`;
  }

  let hoverBiome = $derived(hover && palette && hover.id != null && mapMode === 0 && !dim ? palette.byId.get(hover.id) : null);
  let hoverDimBiome = $derived.by(() => {
    if (!dim || !hover) return null;
    const id = exact && exact.x === hover.x && exact.z === hover.z && exact.dimBiome != null ? exact.dimBiome : mapMode === 0 ? hover.id : null;
    return id == null ? null : dimLegend?.biomes.find((b) => b.id === id) ?? null;
  });
  // exact = the chunk biome array (dithered near borders), falling back to the provider biome
  let exactBiome = $derived.by(() => {
    if (!exact || !palette) return null;
    if (exact.exactRwgId != null) {
      const g = palette.groups.get(exact.exactGenId);
      const v = palette.byId.get(exact.exactRwgId);
      if (g) return { group: g, genId: exact.exactGenId, variant: v?.genId === exact.exactGenId ? v.variant : '' };
    }
    return palette.byId.get(exact.marker ? LAVA_MARKER : exact.rwgId);
  });
  // per in-game biome share of the visible area
  let groupShares = $derived.by(() => {
    const m = new Map();
    if (!palette) return m;
    for (const s of stats) {
      const e = palette.byId.get(s.id);
      if (e) m.set(e.genId, (m.get(e.genId) ?? 0) + s.share);
    }
    return m;
  });
  let legendSort = $state('share');
  const humOf = (gen) => climateMap.get(gen)?.[1] ?? -1, tempOf = (gen) => climateMap.get(gen)?.[0] ?? -9;
  /** Legend order: share on screen, humidity (wettest first, then hottest), or name. */
  function legendCmp(a, b) {
    const ia = a.genId ?? a.id, ib = b.genId ?? b.id;
    if (legendSort === 'humidity') return humOf(ib) - humOf(ia) || tempOf(ib) - tempOf(ia) || a.name.localeCompare(b.name);
    if (legendSort === 'name') return a.name.localeCompare(b.name);
    return b.share - a.share || a.name.localeCompare(b.name);
  }
  let legendRows = $derived.by(() => {
    if (!palette) return [];
    const f = filter.trim().toLowerCase();
    const rows = showAll || f ? [...palette.groups.values()] : [...groupShares.keys()].map((g) => palette.groups.get(g));
    return rows
      .filter(Boolean)
      .filter((g) => !f || g.name.toLowerCase().includes(f) || g.mod.toLowerCase().includes(f) || g.variants.some((v) => v.toLowerCase().includes(f)))
      .map((g) => ({ ...g, share: groupShares.get(g.genId) ?? 0 }))
      .sort(legendCmp);
  });
</script>

<svelte:window onkeydown={onKey} />

<div class="app" class:phone>
  <header>
    <div class="brand">
      <strong>GTNH Seed Map</strong>
      <span class="sub">{pack || 'loading…'}</span>
    </div>
    <label class="seed">
      <span>Seed</span>
      <input bind:value={seedText} spellcheck="false" placeholder="number or text" />
      {#if seedInfo?.kind === 'text'}<span class="hint" title="Text seeds use Java String.hashCode()">= {seedInfo.seed}</span>{/if}
      <button onclick={() => (seedText = String(randomSeed()))} title="Random seed">🎲</button>
    </label>
    <label>
      <span>World type</span>
      <select bind:value={worldType}>
        {#each WORLD_TYPES as t}<option value={t.id}>{t.label}</option>{/each}
      </select>
    </label>
    <label>
      <span>Dimension</span>
      <select bind:value={dim}>
        {#each DIMS as d}<option value={d.id}>{d.label}</option>{/each}
      </select>
    </label>
    <form class="goto" onsubmit={(e) => (e.preventDefault(), goto())}>
      <input bind:value={gotoText} placeholder="go to x, z" spellcheck="false" />
    </form>
    <button onclick={share} title={embedded ? 'Copy seed, world type and the map centre' : 'Copy a link to this seed and view'}>{copied ? 'Copied!' : embedded ? 'Copy position' : 'Copy link'}</button>
    {#if prompt}<input class="manual" readonly value={prompt} onfocus={(e) => e.currentTarget.select()} />{/if}
  </header>

  <main>
    <div class="mapwrap" class:cmp={cmpOn}>
    {#if fatal}<div class="fatal">{fatal}</div>{/if}
    <canvas
      bind:this={canvas}
      onwheel={onWheel}
      onpointerdown={onPointerDown}
      onpointermove={onPointerMove}
      onpointerup={onPointerUp}
      onpointercancel={onPointerUp}
      onpointerleave={() => (hover = null)}
      oncontextmenu={openCtx}
      ondblclick={(e) => {
        const r = canvas.getBoundingClientRect();
        zoomAt(e.clientX - r.left, e.clientY - r.top, 1);
      }}
    ></canvas>
    {#if cmpOn}
      <canvas
        class="cmp-canvas"
        bind:this={cmpCanvas}
        onwheel={cmpWheel}
        onpointerdown={cmpPointerDown}
        onpointermove={cmpPointerMove}
        onpointerup={cmpPointerUp}
        onpointercancel={cmpPointerUp}
        onpointerleave={() => (cmpCursor = null)}
        ondblclick={(e) => {
          const r = cmpCanvas.getBoundingClientRect();
          zoomAt(e.clientX - r.left, e.clientY - r.top, 1);
        }}
      ></canvas>
      <div class="cmp-bar">
        <span class="cmp-a" title="The main map (left / top)">A {seedInfo?.seed.toString() ?? ''}</span>
        <button class="small" onclick={swapCompare} disabled={!cmpInfo} title="Swap: make B the main seed">⇆</button>
        <label class="cmp-b">B <input bind:value={cmpText} spellcheck="false" placeholder="seed to compare" /></label>
        <button class="small" onclick={() => (cmpText = String(randomSeed()))} title="Random seed">🎲</button>
        <button class="small" onclick={closeCompare} title="Close the comparison">×</button>
      </div>
      {#if cmpCursor}
        <div class="cmp-cross" class:on-b={cmpCursor.side === 'a'} style:--cx={`${cmpCursor.sx}px`} style:--cy={`${cmpCursor.sy}px`}></div>
        <div class="cmp-read">
          <span>{cmpCursor.x}, {cmpCursor.z}</span>
          <span><b>A</b> {cmpA}</span>
          <span><b>B</b> {cmpB}</span>
          {#if cmpA && cmpB && cmpA !== '…' && cmpA === cmpB}<span class="dim">same</span>{/if}
        </div>
      {/if}
    {/if}

    {#if pngOpen}
      <div class="png-panel">
        <b>Save map as PNG</b>
        <label><input type="checkbox" bind:checked={pngOpt.markers} /> markers, labels &amp; tools</label>
        <label><input type="checkbox" bind:checked={pngOpt.grid} /> chunk / region grid</label>
        <label class:dimmed={mapMode !== 0}><input type="checkbox" bind:checked={pngOpt.legend} disabled={mapMode !== 0} /> biome legend (on-screen shares)</label>
        <div class="dim tiny">Exactly what is on screen{cmpOn ? ', both seeds' : ''}, at screen resolution, with seed, world type, dimension, centre and zoom underneath. Zoom / pan first; wait for the loading pill to clear.</div>
        <div class="row-btns">
          <button onclick={exportPng} disabled={pngBusy}>{pngBusy ? 'Saving…' : '⤓ Save PNG'}</button>
          <button class="small" onclick={() => (pngOpen = false)}>Cancel</button>
        </div>
      </div>
    {/if}

    {#if ctxMenu}
      <ContextMenu menu={{ ...ctxMenu, tp: `/tp ${ctxMenu.x} ${surfaceY(ctxMenu.ground)} ${ctxMenu.z}` }} other={otherCoord(ctxMenu)} onaction={ctxAction} />
    {/if}

    {#if popup && popupPos}
      {@const m = popup.m}
      {@const oc = OTHER_DIM[dim] ? otherCoord(m) : null}
      <div class="popup" class:popup-wide={m.kind === 'pin' && !m.ghost} class:sheet-popup={phone} style:left={phone ? null : `${popupPos.sx}px`} style:top={phone ? null : `${popupPos.sy}px`}>
        {#if m.kind === 'inspect'}
          <div class="popup-head">
            <strong>X {m.x} · Z {m.z}</strong>
            <button class="small" onclick={() => (popup = null)} title="Close (Esc)">×</button>
          </div>
          <div class="dim">chunk {m.x >> 4}, {m.z >> 4}{oc ? ` · ${oc.name} ${oc.x}, ${oc.z}` : ''}</div>
          {#if popup.info}
            {@const bn = biomeNameOf(popup.info)}
            {#if bn}<div><strong>{bn}</strong></div>{/if}
            {@const pg = dim ? popup.info.dimBiome : popup.info.exactGenId ?? palette?.byId.get(popup.info.marker ? LAVA_MARKER : popup.info.rwgId)?.genId}
            {#if climText(pg)}<div class="clim">{climText(pg)}</div>{/if}
            <div class="dim">
              {#if dim === -1}{popup.info.lava ? 'lava ocean (Y31) below the roof' : popup.ground != null ? `cave floor Y${popup.ground}` : ''}
              {:else if popup.ground != null}ground Y{Math.floor(popup.ground)}{/if}
              {#if !dim && popup.info.beeTemp != null} · bees {beeName(popup.info.beeTemp | (popup.info.beeHumidity << 4))}{/if}
            </div>
          {:else}
            <div class="dim tiny">reading the chunk…</div>
          {/if}
          <div class="popup-actions">
            <button class="small" onclick={copyPopupTp}>{popupCopied ? 'copied' : `/tp ${m.x} ${surfaceY(popup.ground)} ${m.z}`}</button>
            <select class="small-select" bind:value={markType} title="Marker type">
              {#each Object.entries(MARK_TYPES) as [k, t]}{#if (k !== 'portal' || dim !== 7) && (k !== 'tfportal' || dim !== -1)}<option value={k}>{t.icon} {t.name}</option>{/if}{/each}
            </select>
            <button class="small" onclick={() => { addPin(m.x, m.z, markType); popup = null; }} title="Place a marker of this type here">+ marker</button>
            <button class="small" onclick={() => openStreetView(m.x, m.z)} title="Stand here and look around in 3D (V)">👁 view from here</button>
          </div>
        {:else if m.kind === 'pin' && m.ghost}
          {@const src = pins[m.idx]}
          {#if src}
            {@const pr = portalPartner(src)}
            <div class="popup-head">
              <strong>{MARK_TYPES[src.type].icon} {markName(src)} <span class="dim">· {DIM_LABEL[src.dim]}</span></strong>
              <button class="small" onclick={() => (popup = null)} title="Close (Esc)">×</button>
            </div>
            <div class="dim">A {MARK_TYPES[src.type].name.toLowerCase()} at {src.x}, {src.z} in the {DIM_LABEL[src.dim]} leads to X {m.x} · Z {m.z} here{src.type === 'portal' ? ` (${src.dim ? '×8' : '÷8'})` : ' (1:1)'}.</div>
            {#if pr}
              <div>↔ links to <strong>{markName(pins[pr.idx])}</strong> <span class="dim">({Math.round(pr.dist)} blocks away)</span></div>
            {:else}
              <div class="warn">No {src.type === 'portal' ? 'Nether' : 'Twilight'} portal marker within {PORTAL_LINKS[src.type].range} blocks here: the game would build a new portal near this spot.</div>
            {/if}
            <div class="popup-actions">
              {#if !pr}<button class="small" onclick={() => { addPin(m.x, m.z, src.type, src.label ? src.label + ' (linked)' : ''); popup = null; }}>place linked portal here</button>{/if}
              <button class="small" onclick={() => { goToMark(m.idx); popup = null; }}>go to source</button>
            </div>
          {/if}
        {:else if m.kind === 'pin'}
          {@const p = pins[m.idx]}
          {#if p}
            {@const tg = portalTarget(p)}
            {@const pr = tg ? portalPartner(p) : null}
            <div class="popup-head">
              <strong>{MARK_TYPES[p.type]?.icon} {markName(p)}{#if p.scope === 'global'} <span title="Global marker: shown on every seed">🌐</span>{/if}</strong>
              <button class="small" onclick={() => (popup = null)} title="Close (Esc)">×</button>
            </div>
            <input class="mark-input" placeholder="label" value={p.label} oninput={(e) => updateMark(m.idx, { label: e.currentTarget.value })} />
            <div class="sub-row tool-row">
              <select class="small-select" value={p.type} onchange={(e) => updateMark(m.idx, { type: e.currentTarget.value })} title="Type">
                {#each Object.entries(MARK_TYPES) as [k, t]}{#if k === p.type || ((k !== 'portal' || p.dim !== 7) && (k !== 'tfportal' || p.dim !== -1))}<option value={k}>{t.icon} {t.name}</option>{/if}{/each}
              </select>
              <select class="small-select" value={p.scope} onchange={(e) => updateMark(m.idx, { scope: e.currentTarget.value })} title="Seed: only on this seed · Global: shown on every seed">
                <option value="seed">this seed</option>
                <option value="global">🌐 global</option>
              </select>
            </div>
            <textarea class="mark-input" rows="2" placeholder="notes" value={p.notes} oninput={(e) => updateMark(m.idx, { notes: e.currentTarget.value })}></textarea>
            <div class="dim">X {p.x} · Z {p.z} · {DIM_LABEL[p.dim]}{p.name ? ` · ${p.name}` : ''}{p.height != null ? ` · ground Y${Math.floor(p.height)}` : ''}{p.bees != null ? ` · bees ${beeName(p.bees)}` : ''}{p.genId != null && climateMap.get(p.genId) ? ` · humidity ${Math.round(climateMap.get(p.genId)[1] * 100)}%` : ''}</div>
            {#if tg}
              <div class="dim">→ {DIM_LABEL[tg.dim]} X {tg.x} · Z {tg.z}{p.type === 'tfportal' ? ' (1:1)' : ''}</div>
              {#if pr}<div>↔ links to <strong>{markName(pins[pr.idx])}</strong> <span class="dim">({Math.round(pr.dist)} blocks from the converted point)</span></div>
              {:else}<div class="warn">no portal marker within {PORTAL_LINKS[p.type].range} blocks there: would create a new portal</div>{/if}
            {/if}
            <div class="popup-actions">
              <button class="small" onclick={() => copyTp(m.idx)}>{pinCopied === m.idx ? 'copied' : tpCommand(p)}</button>
              <button class="small" onclick={() => measureFrom(p)}>measure from here</button>
              <button class="small" onclick={() => { setRadiusCentre(p); popup = null; }}>radius from here</button>
              <button class="small" onclick={() => openStreetView(p.x, p.z)}>👁 view from here</button>
              <button class="small" onclick={() => markAsSearchCentre(p)}>set as search centre</button>
              {#if tg}<button class="small" onclick={() => { dim = tg.dim; updateView({ x: tg.x + 0.5, z: tg.z + 0.5 }); popup = null; }}>open {DIM_LABEL[tg.dim]}</button>{/if}
              <button class="small danger" onclick={() => removePin(m.idx)}>delete</button>
            </div>
          {/if}
        {:else if m.kind === 'group'}
          <div class="popup-head">
            <strong>{m.members.length} markers</strong>
            <button class="small" onclick={() => (popup = null)} title="Close">×</button>
          </div>
          <div class="dim">{m.kinds.map(([k, n]) => `${n} ${KIND_NAMES[k] ?? k}`).join(' · ')}</div>
          <ul class="ore-lines group-list">
            {#each m.members.slice(0, 40) as g}
              <li><button class="linkish" onclick={() => openPopup(g)}>{g.label ?? KIND_NAMES[g.kind] ?? g.kind}</button> <span class="dim">{g.x}, {g.z}</span></li>
            {/each}
            {#if m.members.length > 40}<li class="dim">… {m.members.length - 40} more: zoom in</li>{/if}
          </ul>
        {:else}
          <div class="popup-head">
            <strong>{KIND_NAMES[m.kind] ?? m.kind}</strong>
            <button class="small" onclick={() => (popup = null)} title="Close">×</button>
          </div>
          {#if m.label && m.label !== KIND_NAMES[m.kind]}<div>{m.label}</div>{/if}
          <div class="dim">X {m.x} · Z {m.z}{popup.ground != null ? ` · ground Y${Math.floor(popup.ground)}` : ''}</div>
          {#if m.lines}<ul class="ore-lines">{#each m.lines as l}<li>{l}</li>{/each}</ul>{/if}
          {#if m.detail}<div class="dim">{m.detail}</div>{/if}
          {#if KIND_NOTES[m.kind]}<div class="dim tiny">{KIND_NOTES[m.kind]}</div>{/if}
          <div class="popup-actions">
            <button class="small" onclick={copyPopupTp}>{popupCopied ? 'copied' : `/tp ${m.x} ${m.tpY ?? surfaceY(popup.ground)} ${m.z}`}</button>
            <button class="small" onclick={() => { addPin(m.x, m.z, 'pin', m.label ?? ''); popup = null; }} title="Place a Pin marker on this feature">+ marker</button>
            <button class="small" onclick={() => openStreetView(m.x, m.z)} title="Stand here and look around in 3D (V)">👁 view</button>
          </div>
        {/if}
      </div>
    {/if}

    <div class="layers">
      <div class="modes">
        <button class:active={mapMode === 0} onclick={() => (mapMode = 0)}>Biomes</button>
        <button class:active={mapMode === 1} disabled={!!dim} onclick={() => (mapMode = 1)} title="Forestry temperature / humidity a bee house on the surface gets">Bee climate</button>
        <button class:active={mapMode === 2} onclick={() => (mapMode = 2)} title="Terrain height with hillshading (exact at 1 block/px)">Terrain</button>
        <button class:active={mapMode === 3} disabled={!!dim} onclick={() => (mapMode = 3)} title="Surface blocks: grass, dirt, sand, stone, clay, snow… (exact at 1 block/px: surface pass, caves and the population snow / ice; trees and plants not)">Blocks</button>
      </div>
      <div class="modes tools">
        <button class:active={tool === 'mark'} onclick={() => setTool('mark')} title="Place a marker (P): pick a type, click the map; shift-click places several; Esc cancels">📍<span class="lbl"> Marker</span></button>
        <button class:active={tool === 'measure'} onclick={() => setTool('measure')} title="Measure distance (M): click A, then B; shift-click adds points; Esc clears">📏<span class="lbl"> Measure</span></button>
        <button class:active={tool === 'radius'} onclick={() => setTool('radius')} title="Radius circle (R): click a centre, drag the handle or type a radius; Esc clears">◯<span class="lbl"> Radius</span></button>
        <button class:active={tool === 'view'} onclick={() => setTool('view')} title="Street view (V): click the map to stand there and look around in 3D">👁<span class="lbl"> View</span></button>
        {#if OTHER_DIM[dim]}<button onclick={() => jumpOther(selPinHere())} title="Open the {OTHER_DIM[dim].name} at the matching coordinate of the selected marker or the map centre (N)">⇄<span class="lbl"> {OTHER_DIM[dim].name} ×{dim ? 8 : '⅛'}</span></button>{/if}
        <button class:active={pngOpen} onclick={() => (pngOpen = !pngOpen)} title="Save the current map view as a PNG (both seeds when comparing)">📷<span class="lbl"> PNG</span></button>
        <button class:active={cmpOn} onclick={() => (cmpOn ? closeCompare() : openCompare())} title="Compare with another seed side by side (same view, dimension and map mode)">◫<span class="lbl"> Compare</span></button>
      </div>
    </div>

    <div class="left-col">
    <div class="readout">
      {#if hover}
        <div class="coords">X {hover.x} · Z {hover.z} <span class="dim">· chunk {hover.x >> 4}, {hover.z >> 4}</span></div>
        {#if OTHER_DIM[dim]}{@const o = otherCoord(hover)}<div class="dim">{o.name}: X {o.x} · Z {o.z}</div>{/if}
        {#if tool === 'measure' && (measureByDim[dim] ?? []).length}{@const pts = measureByDim[dim]}{@const l = pts[pts.length - 1]}<div class="dim">from last point: {fmt(Math.hypot(hover.x - l.x, hover.z - l.z))} blocks</div>{/if}
        {#if mapMode === 3 && !dim && hover.id != null}{@const b = blockSample(hover.id)}<div class="biome"><i style:background={'#' + (blockById?.get(b.block)?.rgb ?? 0).toString(16).padStart(6, '0')}></i>{blockText(hover.id)}{#if view.zoom < 0}<span class="dim"> · approx. (zoom in to 1 block/px)</span>{/if}</div>{/if}
        {#if exactBiome && exact.x === hover.x && exact.z === hover.z}
          <div class="biome"><i style:background={hex(exactBiome.group.rgb)}></i>{exactBiome.group.name} <span class="dim">#{exactBiome.genId}</span></div>
          {#if climText(exactBiome.genId)}<div class="clim">{climText(exactBiome.genId)}</div>{/if}
          {#if exactBiome.variant}<div class="dim">RWG terrain: {exactBiome.variant}</div>{/if}
          <div class="dim">
            {exactBiome.group.mod} · climate {CLIMATES[exact.meta]}{exact.height != null ? ` · ground Y${Math.floor(exact.height)}` : ''}
            {#if worldType === 1}· {exact.continent >= 0 ? `${Math.round(exact.continent)} from shore` : `${Math.round(-exact.continent)} out to sea`}{/if}
          </div>
          {#if exact.beeTemp != null}
            <div class="dim"><i class="sw" style:background={hex(beeRgb(exact.beeTemp | (exact.beeHumidity << 4)))}></i>Bees at surface: {beeName(exact.beeTemp | (exact.beeHumidity << 4))} <span class="dim">({exact.temperature.toFixed(3)})</span></div>
          {/if}
        {:else if hoverDimBiome}
          <div class="biome"><i style:background={'#' + hoverDimBiome.rgb.toString(16).padStart(6, '0')}></i>{hoverDimBiome.name} <span class="dim">#{hoverDimBiome.id}</span></div>
          {#if climText(hoverDimBiome.id)}<div class="clim">{climText(hoverDimBiome.id)}</div>{/if}
          {#if exact && exact.x === hover.x && exact.z === hover.z && exact.height != null}
            <div class="dim">{dim === -1 ? (exact.lava ? 'lava ocean (Y31) below the roof' : `cave floor Y${exact.height} (highest open space below Y110)`) : `ground Y${exact.height}${exact.top > exact.height ? ` · top Y${exact.top} (water / canopy)` : ''}`}</div>
          {/if}
        {:else if hoverBiome}
          <div class="biome"><i style:background={hex(hoverBiome.group.rgb)}></i>{hoverBiome.group.name}</div>
          {#if climText(hoverBiome.group.genId)}<div class="clim">{climText(hoverBiome.group.genId)}</div>{/if}
        {:else if mapMode === 2 && hover.id != null}
          <div class="biome">Terrain ≈ Y{Math.floor(hover.id / 8)}</div>
        {:else if mapMode === 1 && hover.id != null}
          <div class="biome"><i style:background={hex(beeRgb(hover.id))}></i>Bees: {beeName(hover.id)}</div>
        {/if}
      {:else}
        <div class="dim">drag to pan · wheel / pinch to zoom · click = inspect · P = place marker · V = street view · 0 = origin</div>
      {/if}
    </div>

    {#if tool && phone && sheet !== 'tool'}
      <div class="chip-row">
        <button class="chip" onclick={() => openSheet('tool')} title="Show tool details">{toolSummary} <span class="dim">▴</span></button>
        {#if tool !== 'mark' && tool !== 'view'}<button class="chip small" onclick={clearTool} title="Clear">clear</button>{/if}
        <button class="chip small" onclick={() => (tool = '')} title="Close tool">×</button>
      </div>
    {:else if tool === 'view'}
      <div class="tool-panel">
        <div class="pins-head">
          <span>👁 Street view <span class="dim">· click where to stand</span></span>
          <span><button class="small" onclick={() => (tool = '')} title="Cancel (Esc)">×</button></span>
        </div>
        <div class="dim tiny">A 3D look at the terrain from eye height: exact blocks within {phone ? 128 : 256} blocks, the approximate heights beyond. No trees or structures.</div>
      </div>
    {:else if tool === 'mark'}
      <div class="tool-panel">
        <div class="pins-head">
          <span>📍 Place marker <span class="dim">· click the map</span></span>
          <span>{#if phone}<button class="small" onclick={() => (sheet = null)} title="Collapse">▾</button> {/if}<button class="small" onclick={() => (tool = '')} title="Cancel (Esc)">×</button></span>
        </div>
        <div class="mark-types">
          {#each Object.entries(MARK_TYPES) as [k, t]}
            {#if (k !== 'portal' || dim !== 7) && (k !== 'tfportal' || dim !== -1)}
              <button class="small" class:active={markType === k} onclick={() => (markType = k)} style:border-color={markType === k ? t.color : null}>{t.icon} {t.name}</button>
            {/if}
          {/each}
        </div>
        <div class="sub-row tool-row">
          <label title="Seed: stored for this seed + world type · Global: shown on every seed"><input type="radio" bind:group={markScope} value="seed" /> this seed</label>
          <label><input type="radio" bind:group={markScope} value="global" /> 🌐 global</label>
          <label title="Stay in placing mode after each click (shift-click does the same once)"><input type="checkbox" bind:checked={keepPlacing} /> keep placing</label>
        </div>
        {#if markType === 'portal'}<div class="dim tiny">Nether portals link overworld ÷8 ↔ Nether ×8 (Y kept); the game reuses a portal within ±128 blocks of the converted point, else builds a new one. A ghost marker shows where it lands in the other dimension.</div>
        {:else if markType === 'tfportal'}<div class="dim tiny">Twilight Forest portals keep X/Z 1:1 (TF 2.7.45 TFTeleporter; existing portals within ~200 blocks are reused).</div>{/if}
      </div>
    {:else if tool === 'measure'}
      <div class="tool-panel">
        <div class="pins-head">
          <span>📏 Measure <span class="dim">· click A, then B · shift-click adds</span></span>
          <span><button class="small" onclick={clearTool} title="Clear points (Esc)">clear</button> {#if phone}<button class="small" onclick={() => (sheet = null)} title="Collapse">▾</button> {/if}<button class="small" onclick={() => (tool = '')} title="Close tool (M)">×</button></span>
        </div>
        {#if measureStats}
          <div class="tool-big">{fmt(measureStats.total)} blocks{measureStats.segs > 1 ? ` total (${measureStats.segs} legs)` : ''}</div>
          <div class="dim">{measureStats.segs > 1 ? 'last leg ' + fmt(measureStats.last) + ' · ' : ''}ΔX {measureStats.dx} · ΔZ {measureStats.dz} · {measureStats.dcx}×{measureStats.dcz} chunks ({(measureStats.total / 16).toFixed(1)} chunk lengths)</div>
          {#if OTHER_DIM[dim]}<div class="dim">= {fmt(measureStats.total * OTHER_DIM[dim].f)} blocks in the {OTHER_DIM[dim].name}</div>{/if}
          <div class="dim tiny">A {measureStats.a.x}, {measureStats.a.z} → B {measureStats.b.x}, {measureStats.b.z}</div>
        {:else}
          <div class="dim">{(measureByDim[dim] ?? []).length ? 'now click point B' : 'click point A on the map (snaps to markers)'}</div>
        {/if}
      </div>
    {:else if tool === 'radius'}
      {@const c = radiusByDim[dim]}
      <div class="tool-panel">
        <div class="pins-head">
          <span>◯ Radius</span>
          <span><button class="small" onclick={clearTool} title="Clear (Esc)">clear</button> {#if phone}<button class="small" onclick={() => (sheet = null)} title="Collapse">▾</button> {/if}<button class="small" onclick={() => (tool = '')} title="Close tool (R)">×</button></span>
        </div>
        <div class="sub-row tool-row">
          <span class="dim">r</span><input type="number" min="1" step="50" value={c?.r ?? radiusR} onchange={(e) => setRadiusR(e.currentTarget.value)} style="width: 8ch" /> <span class="dim">blocks</span>
        </div>
        <div class="sub-row tool-row">
          <span class="dim">centre</span>
          {#if selPinHere()}<button class="small" onclick={() => setRadiusCentre(selPinHere())}>marker</button>{/if}
          {#if spawnPt && !dim}<button class="small" onclick={() => setRadiusCentre(spawnPt)}>spawn</button>{/if}
          <button class="small" onclick={() => setRadiusCentre({ x: 0, z: 0 })}>0, 0</button>
          <button class="small" onclick={() => setRadiusCentre({ x: view.x, z: view.z })}>view</button>
        </div>
        {#if c}
          <div class="dim">centre {c.x}, {c.z} · r {fmt(c.r)}{OTHER_DIM[dim] ? ` (= ${fmt(c.r * OTHER_DIM[dim].f)} in the ${OTHER_DIM[dim].name})` : ''} · drag the handle to resize</div>
          {#if radiusContents && (radiusContents.kinds.length || radiusContents.ores.length)}
            <ul class="tool-list">
              {#each radiusContents.kinds as [k, n]}<li><span class="name">{k}</span><span class="pct">{n}</span></li>{/each}
              {#each radiusContents.ores as [k, n]}<li><span class="name"><span class="dim">ore</span> {k}</span><span class="pct">{n}</span></li>{/each}
            </ul>
          {:else}
            <div class="dim tiny">no markers of the enabled layers inside (only loaded / visible layers are counted)</div>
          {/if}
          <button class="small" onclick={useAsSearch} title="Search tab: centre this point, radius on every condition">use as search centre</button>
        {:else}
          <div class="dim">click the map to place the centre, or pick one above</div>
        {/if}
      </div>
    {/if}

    {#if activeResult && seedInfo && activeResult.seed === seedInfo.seed.toString() && phone && sheet !== 'result'}
      <div class="chip-row">
        <button class="chip" onclick={() => openSheet('result')}>🔎 match · {activeResult.hits.length} hit{activeResult.hits.length === 1 ? '' : 's'} <span class="dim">▴</span></button>
        <button class="chip small" onclick={() => ((activeResult = null), (hitMarkers = []))} title="Clear">×</button>
      </div>
    {:else if activeResult && seedInfo && activeResult.seed === seedInfo.seed.toString()}
      <div class="result-panel">
        <div class="pins-head">
          <span>Search match <span class="dim">· seed {activeResult.seed}</span></span>
          {#if activeResult.hits.length > 1}
            <button class="small" onclick={() => stepHit(-1)} title="Previous hit ([)">◀</button>
            <span class="dim tiny">{hitIdx >= 0 ? hitIdx + 1 : '–'}/{activeResult.hits.length}</span>
            <button class="small" onclick={() => stepHit(1)} title="Next hit (])">▶</button>
          {/if}
          <button class="small" onclick={() => ((activeResult = null), (hitMarkers = []), (hitIdx = -1))} title="Clear">×</button>
        </div>
        {#if activeResult.center && (activeResult.center[0] || activeResult.center[1])}
          <div class="dim tiny">distances from spawn {activeResult.center[0]}, {activeResult.center[1]}</div>
        {/if}
        <ul>
          {#each activeResult.hits as h, i (i)}
            <li>
              <span class="name" class:hit-on={i === hitIdx}><b class="hitn">{i + 1}</b>{h.label} <span class="dim">{h.dist} m</span></span>
              <button class="small" onclick={() => goToHit(h)} title="Centre the map on it">go</button>
              {#if h.genId != null}<button class="small" onclick={() => highlightHit(h)} title="Highlight this biome">highlight</button>{/if}
            </li>
          {/each}
          {#each activeResult.nots ?? [] as n}
            <li><span class="name dim">✓ {n}</span></li>
          {/each}
        </ul>
        {#if flatPatch}
          <div class="flatnote dim tiny">
            {#if flatPatch.loading}flat area: loading the patch…
            {:else if flatPatch.error}flat area: {flatPatch.error}
            {:else}
              <label><input type="checkbox" bind:checked={flatShown} /> flat area</label>
              <span class="sw" style="background: rgba(255,255,255,0.75)"></span> in band Y {flatPatch.lo + 1}–{flatPatch.hi + 1}
              <span class="sw" style="background: rgba(0,0,0,0.7); outline: 1px solid #ffe08a"></span> rest of the patch ({flatPatch.info?.[2]}% in band, {flatPatch.n.toLocaleString('en-US')} cells of 32²)
            {/if}
          </div>
        {/if}
      </div>
    {/if}

    </div>

    {#if pins.length}
      {@const shownMarks = sortMarks(pins.map((p, i) => [p, i, markDist(p)]).filter(([p]) => markFilter === 'all' || (markFilter === 'here' ? p.dim === dim : markFilter === 'global' ? p.scope === 'global' : p.scope !== 'global')), layers.markSort)}
      <div class="pins" class:collapsed={!marksShown}>
        <div class="pins-head">
          <button class="link-btn" onclick={() => (phone ? openSheet('marks') : (marksOpen = !marksOpen))} title="Show / hide the list">{marksShown ? '▾' : '▸'} {phone ? '📌 ' : ''}Markers <span class="dim">({pins.length})</span></button>
          {#if marksShown}
            <select class="small-select" bind:value={markFilter} title="Which markers to list">
              <option value="here">this dimension</option>
              <option value="seed">this seed (all dims)</option>
              <option value="global">🌐 global</option>
              <option value="all">all</option>
            </select>
            <select class="small-select" bind:value={layers.markSort} title="List order">
              <option value="added">by time added</option>
              <option value="near">nearest to map centre</option>
            </select>
          {/if}
        </div>
        {#if marksShown}
          <ul>
            {#each shownMarks as [p, i, d] (i + ':' + p.x + ':' + p.z + ':' + p.dim)}
              <li class:sel={i === selectedPin}>
                <button class="goto-pin" onclick={() => goToMark(i)} title="Centre on marker{p.dim !== dim ? ' (switches dimension)' : ''}">
                  <span class="xy">{MARK_TYPES[p.type]?.icon} {markName(p)}{p.scope === 'global' ? ' 🌐' : ''}{#if d != null}<span class="mdist" title={p.dim !== dim ? 'distance at the linked scale (Nether ×8)' : 'from the map centre'}>{fmtBlocks(d).replace(' blocks', '')}{p.dim !== dim ? '*' : ''}</span>{/if}</span>
                  <span class="dim">{p.x}, {p.z}{p.dim !== dim ? ` · ${DIM_LABEL[p.dim]}` : ''}{p.name ? ` · ${p.name}` : ''}{p.height != null ? ` · Y${Math.floor(p.height)}` : ''}</span>
                </button>
                <button class="small" onclick={() => copyTp(i)} title="Copy {tpCommand(p)}">{pinCopied === i ? 'copied' : '/tp'}</button>
                <button class="small" onclick={() => removePin(i)} title="Delete marker">×</button>
              </li>
            {:else}
              <li class="dim tiny">no markers in this view</li>
            {/each}
          </ul>
          <div class="sub-row tool-row io-row">
            <span class="dim">export</span>
            <button class="small" onclick={() => exportMarks('seed')}>seed</button>
            <button class="small" onclick={() => exportMarks('global')}>global</button>
            <button class="small" onclick={() => exportMarks('both')}>both</button>
            <label class="small file-btn">import<input type="file" accept="application/json,.json" onchange={importMarksFile} hidden /></label>
          </div>
          {#if markIo}<div class="dim tiny">{markIo}</div>{/if}
          <div class="dim tiny">click a marker on the map to edit · /tp Y = exact ground + 2 · c = copy · Del = delete</div>
        {/if}
      </div>
    {:else}
      <div class="pins collapsed no-marks">
        <div class="pins-head">
          <span class="dim">No markers · P or 📍 to place</span>
          <label class="small file-btn">import<input type="file" accept="application/json,.json" onchange={importMarksFile} hidden /></label>
        </div>
        {#if markIo}<div class="dim tiny">{markIo}</div>{/if}
      </div>
    {/if}

      <ZoomControl zoom={view.zoom} min={MIN_ZOOM} max={MAX_ZOOM} onzoom={zoomCentre} />
      {#if loading.length}
        <div class="loadpill" title="Feature cells still being computed (the map tiles load separately)">
          <span class="spin"></span>{loading.map(([n, c]) => `${n} ${c}`).join(' · ')}
        </div>
      {/if}
      {#if !phone}<button class="khbtn" onclick={() => (keyHelp = true)} title="Keyboard shortcuts (?)">?</button>{/if}
      {#if keyHelp}<KeyHelp onclose={() => (keyHelp = false)} />{/if}
    </div>
    {#if !phone && layers.sideHidden}
      <button class="side-show" onclick={() => (layers.sideHidden = false)} title="Show the side panel">‹ panel</button>
    {/if}
    {#if !phone && !layers.sideHidden}
      <div class="side-grip" role="separator" aria-orientation="vertical" onpointerdown={sideDown} onpointermove={sideMove} onpointerup={sideUp} ondblclick={() => (layers.sideW = 300)} title="Drag to resize · double-click: default width"></div>
    {/if}
    <aside bind:this={asideEl} class:sheet-closed={phone && sheet !== 'side'} class:sheet-full={phone && sheet === 'side' && sheetFull && !sheetDrag} class:sheet-drag={!!sheetDrag?.moved} class:side-hidden={!phone && layers.sideHidden} style:width={phone ? null : `${layers.sideW}px`} style:height={sheetDrag?.moved ? `${sheetDrag.h}px` : null}>
      {#if phone}<div class="grab" role="separator" aria-orientation="horizontal" onpointerdown={grabDown} onpointermove={grabMove} onpointerup={grabUp} onpointercancel={grabUp} title="Drag up / down · tap: bigger / smaller"><span></span></div>{/if}
      <div class="tabs">
        {#if !phone}<button class="side-hide" onclick={() => (layers.sideHidden = true)} title="Hide the side panel (full-width map)">›</button>{/if}
        {#each [['legend', 'Legend'], ['layers', 'Layers'], ...(advanced ? [['ores', 'Ores']] : []), ['search', 'Search']] as [k, l]}
          <button class:active={tab === k && (!phone || sheet === 'side')} onclick={() => { if (phone) { if (tab === k) openSheet('side'); else { tab = k; if (sheet !== 'side') openSheet('side'); } } else tab = k; }}>{l}</button>
        {/each}
        {#if phone && sheet === 'side'}<button class="sheet-close" onclick={() => (sheet = null)} title="Collapse">▾</button>{/if}
      </div>
      <div class="pane" class:hidden={tab !== 'ores'}>
        <div class="legend-head dim">GregTech veins on a 3×3-chunk grid (Overworld). {veinTable?.provisional ? 'Provisional vein data: types/odds right, exact Y pending a pack dump.' : ''}</div>
        <div class="legend-opts">
          <button class="small" onclick={() => (hiddenVeins = new Set())}>all</button>
          <button class="small" onclick={() => (hiddenVeins = new Set(veinTable?.veins.map((_, i) => i) ?? []))}>none</button>
          {#if !layers.ores}<span class="dim tiny">layer is off</span>{/if}
        </div>
        <ul>
          {#each veinTable?.veins ?? [] as v, i (v.name)}
            <li class:on={!hiddenVeins.has(i)}>
              <button
                onclick={() => {
                  const s = new Set(hiddenVeins);
                  s.has(i) ? s.delete(i) : s.add(i);
                  hiddenVeins = s;
                }}
                title={v.ores.join(' · ')}
              >
                <i style:background={hiddenVeins.has(i) ? 'transparent' : veinColor(i)}></i>
                <span class="name" title={(v.oreNames ?? v.ores).join(', ')}>{veinTitle(v)} <span class="dim">Y{v.minY}–{v.maxY}</span></span>
                <span class="pct">{featureList.filter((f) => f.kind === 'ore' && (f.a & 0xff) === i).length || ''}</span>
              </button>
              <button class="vfind" onclick={() => findVein(i)} title="Go to the nearest loaded {veinTitle(v)} vein from the map centre; click again for the next nearest">{veinFind?.vi === i && veinFind.idx >= 0 ? `${veinFind.idx + 1}/${veinFind.list.length}` : '⌖'}</button>
            </li>
          {/each}
        </ul>
        <div class="legend-head dim">Underground fluids (GT pumps / prospector): one fluid per 8×8-chunk field, exact. {fluidTable?.provisional ? 'Default GTNH table.' : ''}</div>
        <div class="legend-opts">
          <label class="tiny"><input type="checkbox" bind:checked={layers.fluids} /> show on map</label>
          <label class="tiny">min <input type="number" min="0" step="25" bind:value={fluidMin} style="width: 6ch" /> L/op</label>
        </div>
        <ul>
          {#each fluidTable?.fluids ?? [] as fd, i (fd.registry)}
            <li class:on={!hiddenFluids.has(i)}>
              <button
                onclick={() => {
                  const s = new Set(hiddenFluids);
                  s.has(i) ? s.delete(i) : s.add(i);
                  hiddenFluids = s;
                }}
              >
                <i style:background={hiddenFluids.has(i) ? 'transparent' : fd.rgb}></i>
                <span class="name">{fd.displayName} <span class="dim">≤ {Math.round(fd.maxAmount * 1.25)} L/op</span></span>
                <span class="pct">{featureList.filter((f) => f.kind === 'fluid' && f.a === i && f.y >= fluidMin).length || ''}</span>
              </button>
            </li>
          {/each}
        </ul>
      </div>
      <div class="pane" class:hidden={tab !== 'layers'}>
        <div class="views">
          <div class="views-row">
            <input class="grow" bind:value={viewName} placeholder="name this view (optional)" onkeydown={(e) => e.key === 'Enter' && saveView()} />
            <button class="small" onclick={saveView} title="Save the position, zoom, dimension, map mode and feature layers for this seed">★ save view</button>
          </div>
          {#if views.length}
            <ul class="view-list">
              {#each views as v, i (v.name)}
                <li>
                  <button class="goto-view" onclick={() => openView(v)} title="{DIM_LABEL[v.dim] ?? v.dim} · {v.x}, {v.z} · zoom {v.zoom}">
                    <span>{v.name}</span>
                    <span class="dim">{v.dim !== dim ? `${DIM_LABEL[v.dim] ?? v.dim} · ` : ''}zoom {v.zoom}</span>
                  </button>
                  <button class="small" onclick={() => deleteView(i)} title="Delete this view">×</button>
                </li>
              {/each}
            </ul>
          {/if}
        </div>
        <div class="mode-row" title="Simple: biomes, terrain, spawn, strongholds, villages and the other dimensions' structures. Advanced adds ores, fluids, oil, Thaumcraft, dungeons, mineshafts, population, chest loot and their searches. Hidden layers cost no computation.">
          <span class="dim">Mode</span>
          <button class="small" class:active={!advanced} onclick={() => (layers.mode = 'simple')}>Simple</button>
          <button class="small" class:active={advanced} onclick={() => (layers.mode = 'advanced')}>Advanced</button>
        </div>
        {#if advanced}<div class="presets">
          {#each LAYER_PRESETS as [name, title, on]}
            <button class="small" class:active={presetActive(on)} onclick={() => applyPreset(on)} {title}>{name}</button>
          {/each}
        </div>{/if}
        <div class="layer-list">
          {#if dim}<label><input type="checkbox" bind:checked={layers.dimFeatures} /> <i class="sw" style:background={dim === 7 ? '#2e7d32' : dim === 1 ? '#8e24aa' : '#ff7043'}></i>{dim === 7 ? 'Twilight Forest features' : dim === 1 ? 'HEE islands + towers' : 'Nether fortresses'} <span class="dim">exact placement</span></label>{/if}
          {#if dim === 1 && advanced}<label title="Draconic Evolution 1.5.35-GTNH: Ender comets (1 in 10000 chunks beyond 200 blocks; exact replay of Forge's per-chunk random) and chaos islands (Chaos Guardian, every 10000 blocks on both axes)"><input type="checkbox" bind:checked={layers.deEnd} /> <i class="sw" style:background="#c62828"></i>Draconic comets + chaos islands <span class="dim">exact placement</span></label>
          <label class="sub" title="HEE's dragon death counter. It only goes up when the End is reset with the Temple Caller ritual (kill the dragon, use the Temple Caller in the End holding the dragon egg, destroy the temple: DIM1 is deleted and regenerates). Killing the dragon alone does not change it. Each reset shifts the main island's terrain seed by one; islands and towers stay where they are (HEE's intended reroll draws one extra random number that never changes the result).">End resets <input type="number" min="0" max="255" step="1" style="width:4.5em" bind:value={layers.endKills} /></label>
          {#if dim === 7}<label class="sub"><input type="checkbox" bind:checked={layers.tfHideHills} /> hide hollow hills</label>{/if}
          {#if advanced}<div class="dim tiny">Ore veins here use GT's live vein list for this dimension. Zoomed in, GT's stone test runs on the exact terrain (assuming you arrive near 0, 0): right ~92% in both dimensions across 8 checked worlds (~95–97% with the real chunk load order). Zoomed out the map shows the first candidate (right ~69% Nether / ~83% Twilight Forest).</div>{/if}{/if}
          <label><input type="checkbox" bind:checked={layers.spawn} /> <i class="sw" style:background="#66bb6a"></i>World spawn <span class="dim">estimated</span></label>
          <label><input type="checkbox" bind:checked={layers.villages} /> <i class="sw" style:background="#a1887f"></i>Villages <span class="dim">exact start</span></label>
          {#if advanced}<label><input type="checkbox" bind:checked={layers.mineshafts} /> <i class="sw" style:background="#6d4c41"></i>Mineshafts <span class="dim">exact, with Y range</span></label>{/if}
          <label><input type="checkbox" bind:checked={layers.strongholds} /> <i class="sw" style:background="#7e57c2"></i>Strongholds <span class="dim">portal room, exact</span></label>
          {#if advanced}<label><input type="checkbox" bind:checked={layers.loot} /> <i class="sw" style:background="#f2c94c"></i>LootGames dungeons <span class="dim">exact chunk</span></label>{/if}
          {#if advanced}<label><input type="checkbox" bind:checked={layers.rogue} /> <i class="sw" style:background="#ef5350"></i>Roguelike dungeons <span class="dim">area exact, spot est.</span></label>{/if}
          {#if advanced}<label class="sub"><input type="checkbox" bind:checked={layers.rogueLikely} disabled={!layers.rogue} /> only likely spots <span class="dim">hide guesses / no-spot areas</span></label>{/if}
          {#if advanced}<label title="Vanilla monster rooms from the chunk population replay, with each chest's loot (exact once the room exists; ~95% of shown rooms are real, ~60% of real rooms shown)"><input type="checkbox" bind:checked={layers.vdungeons} /> <i class="sw" style:background="#6d8f5a"></i>Monster rooms + chest loot <span class="dim">zoomed in</span></label>{/if}
          {#if advanced}<label title="Minecarts with chests in mineshaft corridors, with their loot (exact replay of the populating chunk; the corridor can be missing where the game dropped it)"><input type="checkbox" bind:checked={layers.mineChests} /> <i class="sw" style:background="#c8a165"></i>Mineshaft chest carts + loot <span class="dim">zoomed in</span></label>{/if}
          {#if advanced}<div class="group dim">Thaumcraft</div>{/if}
          {#if advanced}<label><input type="checkbox" bind:checked={layers.tcRing} /> <i class="sw" style:background="#4a148c"></i>Eldritch obelisks <span class="dim">roll exact, terrain est.</span></label>{/if}
          {#if advanced}<label><input type="checkbox" bind:checked={layers.tcHilltop} /> <i class="sw" style:background="#9c4dcc"></i>Hilltop stones <span class="dim">est.</span></label>{/if}
          {#if advanced}<label><input type="checkbox" bind:checked={layers.tcMound} /> <i class="sw" style:background="#9c4dcc"></i>Barrow mounds <span class="dim">est.</span></label>{/if}
          {#if advanced}<label><input type="checkbox" bind:checked={layers.tcTotem} /> <i class="sw" style:background="#9c4dcc"></i>Totems <span class="dim">est.</span></label>{/if}
          {#if advanced}<label><input type="checkbox" bind:checked={layers.tcNodes} /> <i class="sw" style:background="#80deea"></i>Aura nodes + aspects <span class="dim">zoomed in · ~3 s per area</span></label>{/if}
          {#if effLayers.tcNodes}
            <div class="sub-row">
              <select bind:value={nodeAspect} title="Only nodes with this aspect">
                <option value="">all aspects</option>
                {#each aspectTags.filter((t) => !t.startsWith('addon#')) as t}<option value={t}>{t}</option>{/each}
              </select>
              ≥ <input type="number" min="1" step="5" bind:value={nodeMin} style="width: 6ch" />
              <select bind:value={nodeType} title="Only nodes of this type">
                <option value="">all types</option>
                {#each NODE_TYPES as t}<option value={t}>{t.toLowerCase()}</option>{/each}
              </select>
            </div>
            <div class="sub-row node-kinds">
              {#each NODE_KINDS as [k, name]}<label><input type="checkbox" checked={!hiddenNodeKinds.has(k)} onchange={() => toggleNodeKind(k)} />{name}</label>{/each}
            </div>
          {/if}
          {#if advanced}<div class="group dim">GregTech</div>{/if}
          {#if advanced}<label><input type="checkbox" bind:checked={layers.ores} /> <i class="sw" style:background="hsl(40 70% 55%)"></i>GregTech ore veins <span class="dim">see Ores tab</span></label>{/if}
          {#if advanced}<label><input type="checkbox" bind:checked={layers.fluids} /> <i class="sw" style:background="#c8a000"></i>Underground fluids (oil, gas) <span class="dim">exact · filters in Ores tab</span></label>{/if}
          {#if advanced}<div class="group dim">Railcraft</div>{/if}
          {#if advanced}<label><input type="checkbox" bind:checked={layers.geodes} /> <i class="sw" style:background="#7e57c2"></i>Abyssal geodes + dark ores <span class="dim">exact · zoomed in</span></label>{/if}
          {#if advanced}<div class="group dim">Population</div>{/if}
          {#if advanced}<label title="Vanilla lakes from RWG2 populate. All of them are underground in RWG2 (water starts below Y50, lava below Y52)."><input type="checkbox" bind:checked={layers.lakes} /> <i class="sw" style:background="#ff6d00"></i>Underground lakes (lava / water) <span class="dim">exact · zoomed in</span></label>{/if}
          {#if effLayers.lakes}<label class="sub"><input type="checkbox" bind:checked={layers.lakesLava} /> lava only</label>{/if}
          {#if advanced}<label title="DreamMaster oil spheres (BuildCraft oil, radius 8–15 at Y9–45; medium and large ones have a fountain to the surface). The deposit depends on the whole populate chain, so it is replayed: the spawn area in a fresh world's load order once per seed (a few seconds), elsewhere 128-block cells when zoomed in (zoom 0 and closer, ~2 s per cell). Each deposit is roughly a coin flip: on 7 checked worlds about half to 57% of predicted deposits were real, and 57–73% of real oil was predicted."><input type="checkbox" bind:checked={layers.oil} /> <i class="sw" style:background="#3e2723"></i>Oil deposits <span class="dim">~50% · zoomed in</span></label>{/if}
          <div class="group dim">Map</div>
          <label><input type="checkbox" bind:checked={layers.hits} /> <i class="sw" style:background="#4dd0e1"></i>Search hits</label>
          <label><input type="checkbox" bind:checked={layers.pins} /> <i class="sw" style:background="#e8a33d"></i>Markers</label>
          <label><input type="checkbox" bind:checked={layers.grid} /> <i class="sw" style:background="transparent"></i>Grid</label>
          <label title="Feature markers close together on screen merge into a badge with a count (your markers, search hits, strongholds, villages, spawn and ore veins never do). Click a badge to zoom in on it."><input type="checkbox" bind:checked={layers.groupMarkers} /> <i class="sw" style:background="transparent"></i>Group nearby markers</label>
          {#if layers.groupMarkers}<label class="sub">merge distance <select bind:value={layers.groupPx}><option value={22}>small</option><option value={32}>medium</option><option value={48}>large</option></select></label>{/if}
          <label><input type="checkbox" bind:checked={layers.relief} /> <i class="sw" style:background="transparent"></i>Relief shading <span class="dim">on Biomes / Bee climate</span></label>
          <label title="Trees, plants, boulders and canopy snow over the Blocks map from 2 px per block (overworld). Decorators replayed per 4×4 chunks in a fixed order: plausible, not block-exact (the game's order depends on how the world was explored). The spawn area (±192 blocks) is replayed once in a fresh world's real load order."><input type="checkbox" bind:checked={layers.blockDeco} /> <i class="sw" style:background="#3c7a2a"></i>Trees &amp; plants <span class="dim">on Blocks, zoom ≥ 1</span></label>
          {#if advanced}<div class="dim tiny">Dungeon Y and Roguelike spots are estimated when zoomed in (≤ 4 blocks/px).</div>{/if}
        </div>
      </div>
      <div class="pane" class:hidden={tab !== 'search'}>
        <Search {advanced} onadvanced={() => (layers.mode = 'advanced')} {pool} {palette} {worldType} veins={veinTable} fluids={fluidTable} {aspectTags} {embedded} {dim} {dimLegend} climate={climateMap} ondim={(d) => (dim = d)} onpick={pickResult} oncompare={(r) => { if ((r.dim ?? 0) !== dim) dim = r.dim ?? 0; openCompare(r.seed); }} preset={searchPreset} load={searchLoad} bind:this={searchCmp} />
      </div>
      <div class="pane" class:hidden={tab !== 'legend' || mapMode !== 2 || !dim}>
        <div class="legend-head dim">
          {#if dim === 1}The End (Hardcore Ender Expansion over the vanilla End generator): main island end stone, exact block for block before population (checked: 20.5M blocks, 0 mismatches). HEE biome islands (from 1600 blocks out) are drawn from HEE's island terrain generator (top of the island mass after HEE's caves; trees/decoration not included; not drawn at 16 blocks/px or more, i.e. zoom −4 and further out; the island markers stay).
          {:else if dim === 7}Twilight Forest ground (no water, no canopy), exact chunk terrain at 1 block/px (matches the game block for block before decoration; trees not included). Zoomed out: an approximation, 9 in 10 columns within 2 blocks. Sea level is Y31; colours are shifted so Y31 reads as sea level.
          {:else}Nether: floor of the highest open space below Y110 (what you reach digging down from the roof), exact at 1 block/px before decoration (glowstone, fortresses, lava falls not included). Blue = lava ocean (Y31). Zoomed out: an approximation (no caves; about 9 in 10 columns within 5 blocks, the rest a different cavern layer). Colours: Y32 reads like sea level.{/if}
        </div>
        <label class="overlay">Biome colours <input type="range" min="0" max="0.8" step="0.05" bind:value={layers.biomeOverlay} /> <span class="dim">{Math.round(layers.biomeOverlay * 100)}%</span></label>
        <label class="overlay">Colours <select bind:value={layers.terrainPreset}>{#each Object.entries(TERRAIN_PRESETS) as [k, p]}<option value={k}>{p.name}</option>{/each}</select></label>
        <div class="rampbar" onmousemove={rampHover} onmouseleave={() => (rampAt = null)} role="img" aria-label="Height colours">
          <canvas use:drawRamp={layers.terrainPreset} width="28" height={RAMP_H}></canvas>
          {#each rampTicks(layers.terrainPreset) as t (t)}<span class="tick" class:sea={t === 63} style:top={`${((255 - t) / 255) * RAMP_H}px`}>{rampLabel(t)}</span>{/each}
          {#if mapMode === 2 && hover?.id != null && rampAt == null}<span class="here" style:top={`${((255 - Math.min(255, hover.id / 8)) / 255) * RAMP_H}px`}>◀ {rampLabel(Math.floor(hover.id / 8))}</span>{/if}
          {#if rampAt != null}<span class="here at" style:top={`${((255 - rampAt) / 255) * RAMP_H}px`}>◀ {rampLabel(rampAt)}</span>{/if}
        </div>
      </div>
      <div class="pane" class:hidden={tab !== 'legend' || mapMode !== 2 || !!dim}>
        <div class="legend-head dim">Terrain height: blended chunk terrain, exact at 1 block/px (zoomed out: an approximate blend, median 2 blocks off, 4 in 5 land columns within 5; steep terrain and river cuts more). Sea level is Y63.</div>
        <label class="overlay">Biome colours <input type="range" min="0" max="0.8" step="0.05" bind:value={layers.biomeOverlay} /> <span class="dim">{Math.round(layers.biomeOverlay * 100)}%</span></label>
        {#if layers.biomeOverlay > 0}<div class="dim tiny">Click biomes in the list below to overlay only those (e.g. Tainted Land).</div>{/if}
        <label class="overlay">Colours <select bind:value={layers.terrainPreset}>{#each Object.entries(TERRAIN_PRESETS) as [k, p]}<option value={k}>{p.name}</option>{/each}</select></label>
        <div class="rampbar" onmousemove={rampHover} onmouseleave={() => (rampAt = null)} role="img" aria-label="Height colours">
          <canvas use:drawRamp={layers.terrainPreset} width="28" height={RAMP_H}></canvas>
          {#each rampTicks(layers.terrainPreset) as t (t)}<span class="tick" class:sea={t === 63} style:top={`${((255 - t) / 255) * RAMP_H}px`}>{rampLabel(t)}</span>{/each}
          {#if mapMode === 2 && hover?.id != null && rampAt == null}<span class="here" style:top={`${((255 - Math.min(255, hover.id / 8)) / 255) * RAMP_H}px`}>◀ {rampLabel(Math.floor(hover.id / 8))}</span>{/if}
          {#if rampAt != null}<span class="here at" style:top={`${((255 - rampAt) / 255) * RAMP_H}px`}>◀ {rampLabel(rampAt)}</span>{/if}
        </div>
      </div>
      <div class="pane" class:hidden={tab !== 'legend' || mapMode !== 1 || !!dim}>
        <div class="legend-head dim">Forestry bee climate where a bee house stands on the surface. Temperature drops above Y64, so mountains read colder.</div>
        {#if beeHighlight.size}
          <button class="clear" onclick={() => (beeHighlight = new Set())}>clear highlight ({beeHighlight.size})</button>
        {/if}
        <ul>
          {#each BEE_CODES.filter((c) => (beeShares.get(c) ?? 0) > 0 || showAll) as c (c)}
            <li class:on={beeHighlight.has(c)}>
              <button onclick={() => toggleBee(c)}>
                <i style:background={hex(beeRgb(c))}></i>
                <span class="name">{BEE_TEMPS[c & 15]} · {BEE_HUMIDITY[c >> 4]}</span>
                <span class="pct">{beeShares.get(c) ? ((beeShares.get(c) * 100).toFixed(beeShares.get(c) < 0.01 ? 1 : 0) + '%') : ''}</span>
              </button>
            </li>
          {/each}
        </ul>
        <label class="all dim"><input type="checkbox" bind:checked={showAll} /> show all classes</label>
      </div>
      <div class="pane" class:hidden={tab !== 'legend' || !dim}>
        <div class="legend-head dim">{dim === 1 ? 'The End: the main island (vanilla generator, exact) in the void, HEE biome islands as land where HEE would place them (checked in a real world; an island can be missing if the End unloaded while it was half explored), dungeon towers and Draconic Evolution comets / chaos islands as markers (Layers).' : dim === 7 ? 'Twilight Forest biomes (TF 2.7.45 GenLayer port; TF seed = world seed).' : 'Nether biomes (Biomes O\' Plenty nether, one biome per 4×4 chunks; Polar Chasm can never generate).'}{dim === 1 ? ' Main island checked against a real world: 20.5M blocks, 0 mismatches. HEE island land is not drawn at 16 blocks/px or more (zoom −4 and further out); the island markers stay.' : ' Exact: verified against real-world dumps (5.9M samples, 0 mismatches).'}
          <label class="dim">Sort <select bind:value={legendSort}><option value="share">on screen</option><option value="humidity">humidity</option><option value="name">name</option></select></label></div>
        <ul>
          {#each (dimLegend?.biomes ?? []).map((b) => ({ ...b, share: dimShares.get(b.id) ?? 0 })).sort(legendCmp) as b (b.id)}
            <li>
              <button>
                <i style:background={'#' + b.rgb.toString(16).padStart(6, '0')}></i>
                <span class="name" title={climText(b.id)}>{b.name} <span class="dim">#{b.id}</span></span>
                {#if dim !== 1 && climateMap.get(b.id)}<span class="hum" title={climText(b.id)}>💧{Math.round(climateMap.get(b.id)[1] * 100)}%</span>{/if}
                <span class="pct">{b.share ? (b.share * 100).toFixed(b.share < 0.01 ? 1 : 0) + '%' : ''}</span>
              </button>
            </li>
          {/each}
        </ul>
      </div>
      <div class="pane" class:hidden={tab !== 'legend' || !!dim || !(mapMode === 0 || (mapMode === 2 && layers.biomeOverlay > 0))}>
      <div class="legend-head">
        <input bind:value={filter} placeholder="filter biomes / mods" spellcheck="false" />
        <label class="all"><input type="checkbox" bind:checked={showAll} /> all</label>
      </div>
      <div class="legend-opts">
        <label class="dim">Colours
          <select bind:value={paletteMode}>
            <option value="game">in-game</option>
            <option value="distinct">distinct</option>
          </select>
        </label>
        <label class="dim">Sort
          <select bind:value={legendSort}>
            <option value="share">on screen</option>
            <option value="humidity">humidity</option>
            <option value="name">name</option>
          </select>
        </label>
      </div>
      {#if highlight.size}
        <button class="clear" onclick={() => (highlight = new Set())}>clear highlight ({highlight.size})</button>
      {/if}
      <ul>
        {#each legendRows as g (g.genId)}
          <li class:on={highlight.has(g.genId)}>
            <button onclick={() => toggle(g.genId)} title="{g.name} (#{g.genId}) · {g.mod}{climText(g.genId) ? '\n' + climText(g.genId) : ''}{g.variants.length ? '\nRWG terrain: ' + g.variants.join('; ') : ''}">
              <i style:background={hex(g.rgb)}></i>
              <span class="name">{g.name}</span>
              {#if climateMap.get(g.genId)}<span class="hum" title={climText(g.genId)}>💧{Math.round(climateMap.get(g.genId)[1] * 100)}%</span>{/if}
              <span class="pct">{g.share ? (g.share * 100).toFixed(g.share < 0.01 ? 1 : 0) + '%' : ''}</span>
            </button>
            {#if highlight.has(g.genId) && climText(g.genId)}<div class="clim sel">{climText(g.genId)}{g.bee != null ? ` · bees ${beeName(g.bee)}` : ''}</div>{/if}
          </li>
        {/each}
      </ul>
      </div>
      <footer class="dim">
        {pool.size} workers · {tileMs ? tileMs.toFixed(0) : '–'} ms/tile
        <span class="foot-long"><br />Biomes (incl. border dithering at 1 block/px), terrain height, bee climate, lava caves, volcanoes, strongholds and LootGames chunks match the game exactly (validated on GTNH 2.9.0).</span>
        <br /><span class="build" title="Version · build date · build id (start of the source bundle's SHA-256: names the exact source of this build)">v{BUILD.version} · {BUILD.date} · {BUILD.source}</span>
        <br /><a href="./about.html" title="What is exact and what is estimated, supported packs, privacy, licences">About</a> · <a href={import.meta.env.VITE_SOURCE_URL ?? './source/index.html'} title="The worldgen port (rwg2-biomes, rwg2-wasm) is GPL-3.0-or-later, derived from Realistic-World-Gen-2. Every file of this build: Rust crates, embedded pack data and the viewer.">Source code (GPL-3.0)</a>{#if ISSUES_URL} · <a href={bugUrl} target="_blank" rel="noopener" title="Opens an issue with the version, seed and place filled in">Report a bug</a>{/if}
      </footer>
      {#if tab === 'search' && advanced && DEV_TOOLS}
        <details class="devpresets" bind:open={layers.devPresetsOpen}>
          <summary class="dim">Dev · preset searches ({SEARCH_PRESETS.length})</summary>
          <div class="dp-checks">
            <button class="small" onclick={() => (checkingAll ? (checkingAll = false) : checkAllPresets())} title="Run every preset over its fixed seed range (seeds 1…n) and compare the match count with the expected one: any difference means the search, its data or a model under it changed">{checkingAll ? 'stop' : 'check all'}</button>
            <span class="dim tiny">{checkSummary()}</span>
            {#if Object.keys(presetChecks).length}<button class="small" onclick={copyChecks} title="Copy the counts as JSON (to update the expectations)">copy</button>{/if}
          </div>
          {#each PRESET_GROUPS as grp (grp)}
            <div class="dp-group dim">{grp}</div>
            {#each SEARCH_PRESETS.filter((p) => p.group === grp) as p (p.id)}
              {@const cs = checkState(p)}
              <div class="dp-row">
                <button class="dp-load" title={`${p.desc ?? p.name}\nClick: load the conditions · ▶: load and search`} onclick={() => (searchLoad = { preset: p, run: false, n: (searchLoad?.n ?? 0) + 1 })}>{p.name}</button>
                <button class={`dp-chk ${cs?.cls ?? ''}`} title={cs?.title ?? `Check: seeds 1…${p.check?.n ?? 400}${p.check?.expect != null ? `, expect ${p.check.expect} matches` : ''}`} onclick={() => checkPreset(p)}>{cs?.text ?? '✓?'}</button>
                <button class="dp-run" title="Load and search" onclick={() => (searchLoad = { preset: p, run: true, n: (searchLoad?.n ?? 0) + 1 })}>▶</button>
              </div>
            {/each}
          {/each}
        </details>
      {/if}
    </aside>
  </main>

  {#if sv}
    <div class="sv-overlay" role="dialog" aria-label="Street view">
      <div class="sv-stage" bind:this={svEl}></div>
      <div class="sv-top">
        <div class="sv-compass" title="Heading">
          {#each svTicks as t}<span class="sv-tick" class:major={t.major} style:left={`calc(50% + ${t.d * 1.6}px)`}>{t.label || '·'}</span>{/each}
          <span class="sv-caret">▼</span>
        </div>
        <button class="sv-close" onclick={closeStreetView} title="Back to the map (Esc)">×</button>
      </div>
      <div class="sv-info">
        {#if svHud}
          <div><strong>X {Math.floor(svHud.x)} · Y {Math.floor(svHud.y)} · Z {Math.floor(svHud.z)}</strong></div>
          <div class="dim">facing {compassName(svHud.heading)} {Math.round(svHud.heading)}° · FOV {Math.round(svHud.fov)}°{svHud.fly ? ' · flying' : ''}</div>
          {#if svHere}<div><i class="sw" style:background={`#${svHere.rgb.toString(16).padStart(6, '0')}`}></i>{svHere.name}{#if sv?.dim !== 1 && climText(svHud.biome)} <span class="dim">· {climText(svHud.biome)}</span>{/if}</div>{/if}
          {#if svHud.nearLoaded < svHud.nearTotal}<div class="dim">loading terrain {svHud.nearLoaded}/{svHud.nearTotal}{svHud.capped ? ' (triangle cap)' : ''}</div>{/if}
        {:else if svErr}
          <div class="warn">{svErr}</div>
        {:else}
          <div class="dim">starting 3D view…</div>
        {/if}
      </div>
      <div class="sv-actions">
        <button class="small" onclick={() => svCtl?.toggleFly()} title="Fly (F): space / shift (or E / Q) up and down">{svHud?.fly ? '⬇ walk' : '🕊 fly'}</button>
        <button class="small" class:on={svOpt.tint} onclick={() => (svOpt.tint = !svOpt.tint)} title="Biome colours on the terrain, with the share of each biome (B)">🎨</button>
        <button class="small" class:on={svOpt.mini} onclick={() => (svOpt.mini = !svOpt.mini)} title="Minimap: click it to jump there (M)">🗺</button>
        <button class="small" class:on={svOpt.xray.on} onclick={() => (svOpt.xray.on = !svOpt.xray.on)} title="X-ray: caves, structures and ore veins under the ground (X)">🩻</button>
        <button class="small" class:on={svOpt.panel} onclick={() => (svOpt.panel = !svOpt.panel)} title="What the 3D view shows (L)">☰</button>
        <button class="small" onclick={svScreenshot} title="Save a PNG of this view">📷</button>
      </div>
      {#if svOpt.panel}
        <div class="sv-panel">
          <label><input type="checkbox" bind:checked={svOpt.tint} /> Biome colours <span class="dim">B</span></label>
          <label title="Colour the terrain by height with the map's Terrain colour preset (Legend tab in Terrain mode)"><input type="checkbox" bind:checked={svOpt.heightCol} /> Height colours <span class="dim">H · {TERRAIN_PRESETS[layers.terrainPreset]?.name ?? ''}</span></label>
          {#if svOpt.heightCol}<label class="sub">preset <select bind:value={layers.terrainPreset}>{#each Object.entries(TERRAIN_PRESETS) as [k, p]}<option value={k}>{p.name}</option>{/each}</select></label>{/if}
          <label><input type="checkbox" bind:checked={svOpt.water} /> Water</label>
          {#if !sv?.dim}<label title="Trees, plants, boulders and snow on canopies: RWG2's biome decoration and the BiomesOPlenty decorator replayed per chunk (exact where the decorator is ported and the chunk order matches the game's; a plausible stand-in elsewhere)"><input type="checkbox" bind:checked={svOpt.trees} /> Trees &amp; plants</label>{/if}
          <label><input type="checkbox" bind:checked={svOpt.mini} /> Minimap <span class="dim">M</span></label>
          <div class="sv-ph dim">X-ray <span class="tiny">(see through the ground; fly below it)</span></div>
          <label><input type="checkbox" bind:checked={svOpt.xray.on} /> X-ray <span class="dim">X</span></label>
          {#if svOpt.xray.on}
            {#if sv?.dim !== -1}<label class="sub"><input type="checkbox" bind:checked={svOpt.xray.caves} /> Caves <span class="dim tiny">exact carve model · orange = lava level</span></label>{/if}
            {#if !sv?.dim && advanced}<label class="sub" title="RWG2 chunk population before GregTech's veins: water/lava lakes, dungeon rooms, Railcraft geodes, dirt and gravel pockets, emeralds (exact replay; skipped where a structure piece or a village makes the rest unknown; dungeon rooms with chests and spawner; later biome decoration not shown)"><input type="checkbox" bind:checked={svOpt.xray.pop} /> Lakes, dungeons, geodes, dirt/gravel <span class="dim tiny">exact replay</span></label>{/if}
            {#if sv?.dim !== 7}<label class="sub"><input type="checkbox" bind:checked={svOpt.xray.structures} /> {sv?.dim === -1 ? 'Fortress pieces & spawners' : 'Mineshafts & strongholds'} <span class="dim tiny">exact</span></label>{/if}
            {#if advanced}<label class="sub"><input type="checkbox" bind:checked={svOpt.xray.ores} /> Ore veins <span class="dim tiny">predicted vein</span></label>{/if}
            {#if !sv?.dim && advanced}<label class="sub"><input type="checkbox" bind:checked={svOpt.xray.rooms} /> LootGames rooms</label>{/if}
          {/if}
          <div class="sv-ph dim">Markers <span class="tiny">(nearest 40 of each, ~1000 blocks around)</span></div>
          {#each SV_GROUPS.filter(([k, , d]) => (d === 'all' || d === (sv?.dim ?? 0)) && (advanced || !SV_ADV_GROUPS.has(k))) as [k, l]}<label><input type="checkbox" bind:checked={svOpt.groups[k]} /> {l}</label>{/each}
        </div>
      {/if}
      {#if svOpt.tint && svBiomeRows.length}
        <div class="sv-biomes">
          <div class="sv-bh">
            <button class="tab" class:on={svOpt.legend === 'near'} onclick={() => (svOpt.legend = 'near')} title="Exact block columns of the loaded detail area">nearby</button>
            <button class="tab" class:on={svOpt.legend === 'far'} onclick={() => (svOpt.legend = 'far')} title="Every 16th block out to the horizon">to horizon</button>
          </div>
          {#each svBiomeRows.slice(0, phone ? 6 : 12) as b (b.gen)}
            <div class="sv-brow" class:here={svHud?.biome === b.gen}>
              <i class="sw" style:background={`#${b.rgb.toString(16).padStart(6, '0')}`}></i><span class="name" title={climText(b.gen)}>{b.name}</span>{#if climateMap.get(b.gen)}<span class="hum">💧{Math.round(climateMap.get(b.gen)[1] * 100)}%</span>{/if}<span class="pct">{(b.share * 100).toFixed(b.share < 0.01 ? 1 : 0)}%</span>
            </div>
          {/each}
          {#if svBiomeRows.length > (phone ? 6 : 12)}<div class="dim tiny">+{svBiomeRows.length - (phone ? 6 : 12)} more</div>{/if}
        </div>
      {/if}
      {#if svOpt.mini}
        <div class="sv-mini" class:big={svOpt.miniBig}>
          <canvas bind:this={miniCanvas} onclick={miniClick} onwheel={miniWheel} title="Click to jump there"></canvas>
          <div class="sv-mbtn">
            <button onclick={() => miniZoom(1)} title="Zoom in">+</button>
            <button onclick={() => miniZoom(-1)} title="Zoom out">−</button>
            <button onclick={() => (svOpt.miniBig = !svOpt.miniBig)} title={svOpt.miniBig ? 'Smaller' : 'Bigger'}>{svOpt.miniBig ? '↙' : '↗'}</button>
            <button class:on={svOpt.miniTerrain} onclick={() => (svOpt.miniTerrain = !svOpt.miniTerrain)} title={svOpt.miniTerrain ? 'Biome map' : 'Terrain height map'}>⛰</button>
          </div>
          <span class="sv-mn">N</span>
        </div>
      {/if}
      {#if svBlock}
        {@const bi = svBlock.info}
        {@const bm = bi ? blockById?.get(bi.block) : null}
        {@const gen = bi ? bi.gen : svBlock.dimInfo?.dimBiome}
        {@const bio = gen != null ? svBiome(gen, svBlock.dim) : null}
        <div class="sv-tip pinned sv-blockinfo" style:left={`${Math.min(Math.max(svBlock.sx, 150), (svEl?.clientWidth ?? 800) - 150)}px`} style:top={`${Math.min(Math.max(svBlock.sy, 90), (svEl?.clientHeight ?? 600) - 200)}px`}>
          {#if svBlock.dim}
            {@const db = svBlock.dimBlock}
            {#if db?.name}
              <div><strong>{prettyBlock(db.name)}</strong> <span class="dim">{db.name}</span>{#if db.name === "minecraft:air"} <span class="dim tiny">(a cave the 3D view draws as solid)</span>{/if}</div>
              <div>X {svBlock.x} · Y {svBlock.y} · Z {svBlock.z}</div>
              <div class="dim tiny">above: {prettyBlock(db.above)} · below: {prettyBlock(db.below)}</div>
              {#if !db.exact}<div class="dim tiny">End: end stone / island surface / tower obsidian only (HEE decoration not modelled)</div>{/if}
            {:else}
              <div><strong>Block at {svBlock.x}, {svBlock.y}, {svBlock.z}</strong></div>
            {/if}
            {#if svBlock.dimInfo?.height != null && svBlock.dimInfo.height >= 0}<div class="dim">ground Y{Math.floor(svBlock.dimInfo.height)}</div>{/if}
          {:else if !bi}
            <div class="dim">looking up {svBlock.x}, {svBlock.y}, {svBlock.z}…</div>
          {:else}
            <div><i class="sw" style:background={'#' + (bm?.rgb ?? 0).toString(16).padStart(6, '0')}></i><strong>{bi.block ? bm?.label ?? '?' : 'Air'}</strong> <span class="dim">{bi.block ? blockId(bi.block) : ''}</span></div>
            <div>X {svBlock.x} · Y {svBlock.y} · Z {svBlock.z}{bi.top > svBlock.y ? ` · ${bi.top - svBlock.y} below the top (Y${bi.top})` : ''}</div>
            {#if bi.block && (bi.above || bi.below)}<div class="dim tiny">above: {bi.above ? blockById?.get(bi.above)?.label : 'air'} · below: {bi.below ? blockById?.get(bi.below)?.label : 'air'}</div>{/if}
            {#if svBlock.under?.length}<div class="tiny" title="RWG2 populate replay for this column (underground)">under this column: {svBlock.under.map((u) => `${u.k} Y${u.y0}${u.y1 > u.y0 ? `–${u.y1}` : ''}`).join(' · ')}</div>{/if}
          {/if}
          {#if bio}
            <div><i class="sw" style:background={'#' + bio.rgb.toString(16).padStart(6, '0')}></i>{bio.name} <span class="dim">#{gen}</span>{#if bi && palette?.groups.get(gen)?.lava} <span class="dim">(lava-cave marker)</span>{/if}</div>
            {#if svBlock.dim !== 1 && climText(gen)}<div class="clim">{climText(gen)}</div>{/if}
            {#if bi && palette?.byId.get(bi.rwg)?.variant}<div class="dim tiny">RWG terrain: {palette.byId.get(bi.rwg).variant}</div>{/if}
          {/if}
          {#if svBlock.approx}<div class="warn tiny">far terrain: position approximate</div>{/if}
          <div class="dim tiny">{svBlock.dim ? 'before population: no plants, trees or ores' : 'before trees, plants and ores; with the population snow / ice; caves not cut'}</div>
          <div class="sv-tipb">
            <button class="small" onclick={svCopyBlock}>{svBlock.copied ? 'copied' : 'copy'}</button>
            <button class="small" onclick={() => svCtl?.clearBlock()}>×</button>
          </div>
        </div>
      {/if}
      {#if svPick}
        <div class="sv-tip" class:pinned={svPick.pinned} style:left={`${Math.min(Math.max(svPick.sx, 140), (svEl?.clientWidth ?? 800) - 140)}px`} style:top={`${Math.max(svPick.sy, 90)}px`}>
          <div><i class="sw" style:background={svPick.color}></i><strong>{svPick.label}</strong></div>
          <div>~{fmtBlocks(svPick.dist)} · {svDir(svPick.rel)} ({compassName(svPick.bearing)}){svPick.dy != null && Math.abs(svPick.dy) >= 3 ? ` · ground ${svPick.dy > 0 ? '+' : ''}${svPick.dy}` : ''}</div>
          {#if svPick.dist3 != null}<div class="dim">to Y{svPick.y}: ~{fmtBlocks(svPick.dist3)} in a straight line</div>{/if}
          <div class="dim">{svPick.x}, {svPick.z}</div>
          {#if svPick.detail}<div class="dim tiny">{svPick.detail}</div>{/if}
          {#if svPick.pinned}
            <div class="sv-tipb">
              <button class="small" onclick={() => { const p = svPick; svCtl?.setHeading(p.bearing); }}>face it</button>
              <button class="small" onclick={() => { const p = svPick; svCtl?.setHeading(p.bearing); const b = (p.bearing * Math.PI) / 180, back = Math.min(24, p.dist); svTeleport(Math.round(p.x - Math.sin(b) * back), Math.round(p.z + Math.cos(b) * back)); }}>go there</button>
              <button class="small" onclick={() => svCtl?.clearPick()}>×</button>
            </div>
          {/if}
        </div>
      {/if}
      {#if !phone && !(svOpt.mini && svOpt.miniBig)}<div class="sv-help dim">drag to look · wheel = zoom · WASD move · space / shift fly · F walk/fly · B biomes · H heights · X x-ray · M minimap · click a label · Esc back to map</div>{/if}
    </div>
  {/if}
</div>
