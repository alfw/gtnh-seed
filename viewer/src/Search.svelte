<script>
  // Seed search: a list of conditions ("within R" or "not within R" of a centre) in the current dimension; batches
  // of seeds go to the worker pool at low priority (map tiles always win). Results can be saved, compared, shared.
  import { untrack } from 'svelte';
  import Picker from './ui/Picker.svelte';
  import { LOOT_ITEMS } from './lib/constants.js';
  import { isAdvancedCond } from './lib/features.js';
  import { randomSeed, parseSeed } from './lib/seed.js';
  import { LAVA_MARKER, BEE_CODES, beeName } from './lib/colors.js';

  let { advanced = true, onadvanced = null, pool, palette, worldType, veins, fluids, aspectTags = [], embedded = true, dim = 0, dimLegend = null, climate = new Map(), ondim, onpick, oncompare = null, preset = null, load = null } = $props();
  // map radius tool → "use as search centre": custom centre + that radius on every condition
  let lastPreset = 0;
  $effect(() => {
    const p = preset;
    if (!p || p.n === lastPreset) return;
    lastPreset = p.n;
    untrack(() => {
      center = 'point';
      cx = p.x;
      cz = p.z;
      if (p.r != null) conds = conds.map((c) => ({ ...c, r: p.r }));
    });
  });

  const DIM_NAMES = { 0: 'Overworld', '-1': 'Nether', 7: 'Twilight Forest', 1: 'The End' };
  const TC = { 2: 'Eldritch obelisk', 3: 'Hilltop stones', 1: 'Barrow mound', 4: 'Totem' };
  const HILLS = [1, 2, 3]; // TFFeature hollow hills (small / medium / large)

  // condition: { what, genId?, genIdB?, dist?, bee?, feat?, r, not, minLpo?, aspect?, amin?, spawners?, style?,
  //               ntype?, nmod?, ocean? }
  //   what = biome | bee | adj | stronghold | strongholdSurface | village | villageStyle | mineshaft | volcano | lava
  //        | lootgames | roguelike | tc:<kind> | aspect | nodeType | fluid:<i> | vein:<i> | fortress | tf
  // HEE island biomes and content variations (id, name) per biome; DE comet sizes 2..9
  const END_BIOMES = ['Infested Forest', 'Burning Mountains', 'Enchanted Island'];
  const END_VARS = [[[0, 'Deep'], [3, 'Ravaged']], [[1, 'Scorching'], [5, 'Mine']], [[2, 'Homeland'], [6, 'Laboratory']]];
  const STYLES = ['plains', 'desert', 'taiga', 'savanna', 'snowy', 'jungle', 'swamp'];
  const NODE_TYPES = ['Normal', 'Pure', 'Dark', 'Unstable', 'Hungry', 'Tainted'];
  const NODE_MODS = [['', 'any modifier'], ['none', 'no modifier'], ['Bright', 'bright'], ['Pale', 'pale'], ['Fading', 'fading']];
  const defaultConds = (d) =>
    d === 1 ? [{ what: 'endIsland', ibiome: '', ivar: '', r: 3000, not: false }] : d === 7 ? [{ what: 'tf', feat: 6, r: 1500, not: false }] : d === -1 ? [{ what: 'fortress', r: 400, not: false }] : [{ what: 'biome', genId: 193, r: 500, not: false }];
  let conds = $state(defaultConds(dim));
  let condsDim = $state(dim); // dimension the condition list belongs to
  // a loaded share code / preset / saved list with an advanced-mode condition switches the viewer to advanced
  $effect(() => {
    if (!advanced && conds.some((c) => isAdvancedCond(c.what))) untrack(() => onadvanced?.());
  });
  let center = $state('origin'); // origin | spawn | point
  let cx = $state(0);
  let cz = $state(0);
  let landAtSpawn = $state(true);
  let step = $state(64);
  let source = $state('random'); // random | sequential | range | list
  let startText = $state('0');
  let rangeTo = $state('9999');
  let listText = $state('');
  let maxResults = $state(50);

  let running = $state(false);
  let checked = $state(0);
  let total = $state(0); // finite sources: number of seeds
  let rate = $state(0);
  let results = $state([]);
  let resultsNote = $state(''); // what the list shows when it is not a live search (loaded / compared)
  let error = $state('');
  let next = 0n;
  let last = 0n;
  let listSeeds = [];
  let listPos = 0;
  let runId = 0;

  const BATCH = 24;

  // ------------------------------------------------------------------------------------------- results table
  let resView = $state('list'); // 'list' | 'table'
  let sortKey = $state(''); // '' = found order | 'seed' | 'worst' | 'sum' | column label
  let sortDir = $state(1);
  /** One column per positive condition (by label, in first-seen order); nearest hit per column. */
  let resCols = $derived.by(() => {
    const seen = [];
    for (const r of results) for (const h of r.hits) if (!seen.includes(h.label)) seen.push(h.label);
    return seen;
  });
  let resRows = $derived(
    results.map((r, i) => {
      const d = {};
      for (const h of r.hits) if (d[h.label] == null || h.dist < d[h.label]) d[h.label] = h.dist;
      const vals = resCols.map((c) => d[c]).filter((v) => v != null);
      return { r, i, d, worst: vals.length ? Math.max(...vals) : null, sum: vals.length ? vals.reduce((a, b) => a + b, 0) : null };
    }),
  );
  let sortedRows = $derived.by(() => {
    if (!sortKey) return resRows;
    const key = (row) => (sortKey === 'seed' ? row.r.seed : sortKey === 'worst' ? row.worst : sortKey === 'sum' ? row.sum : row.d[sortKey]);
    return resRows.slice().sort((a, b) => {
      const va = key(a), vb = key(b);
      if (va == null) return vb == null ? a.i - b.i : 1;
      if (vb == null) return -1;
      if (sortKey === 'seed') { const x = BigInt(va), y = BigInt(vb); return (x < y ? -1 : x > y ? 1 : 0) * sortDir; }
      return (va - vb) * sortDir || a.i - b.i;
    });
  });
  function sortBy(k) {
    if (sortKey === k) { if (sortDir === 1) sortDir = -1; else { sortKey = ''; sortDir = 1; } }
    else { sortKey = k; sortDir = 1; }
  }
  const arrow = (k) => (sortKey === k ? (sortDir === 1 ? ' ▲' : ' ▼') : '');
  function exportCsv() {
    const esc = (v) => (/[",\n]/.test(String(v)) ? `"${String(v).replaceAll('"', '""')}"` : String(v));
    const lines = [['seed', 'dim', 'centre_x', 'centre_z', ...resCols, 'worst', 'sum'].map(esc).join(',')];
    for (const row of sortedRows) lines.push([row.r.seed, row.r.dim, row.r.center[0], row.r.center[1], ...resCols.map((c) => row.d[c] ?? ''), row.worst ?? '', row.sum ?? ''].map(esc).join(','));
    const text = lines.join('\n');
    try { navigator.clipboard.writeText(text); note(`copied ${sortedRows.length} rows as CSV`); } catch { note('clipboard not available'); }
  }

  // ------------------------------------------------------------------------------------------- options per dim
  let biomeOptions = $derived(
    dim
      ? (dimLegend?.biomes ?? []).map((b) => ({ genId: b.id, name: b.name })).sort((a, b) => a.name.localeCompare(b.name))
      : palette
        ? [...palette.groups.values()].filter((g) => !g.lava).sort((a, b) => a.name.localeCompare(b.name))
        : [],
  );
  // biome dropdowns: grouped by humidity (rainfall), wettest first; inside a group hottest first
  let biomeGroups = $derived.by(() => {
    const groups = new Map();
    for (const g of biomeOptions) {
      const c = climate.get(g.genId);
      const h = c ? Math.round(c[1] * 100) : -1;
      if (!groups.has(h)) groups.set(h, []);
      groups.get(h).push({ ...g, t: c ? c[0] : null });
    }
    return [...groups.entries()]
      .sort((a, b) => b[0] - a[0])
      .map(([h, list]) => ({
        label: h < 0 ? 'Humidity unknown' : `Humidity ${h}%`,
        list: list.sort((a, b) => (b.t ?? -9) - (a.t ?? -9) || a.name.localeCompare(b.name)),
      }));
  });
  let beeOptions = $derived(palette ? BEE_CODES.filter((code) => [...palette.groups.values()].some((g) => !g.lava && g.bee === code)) : []);
  let veinList = $derived(dim ? (dimLegend?.veins ?? []) : (veins?.veins ?? []));
  let fluidList = $derived(fluids?.fluids ?? []);
  // searchable pickers (same options as the old <select>s)
  let biomePick = $derived(
    biomeGroups.map((grp) => ({ label: grp.label, items: grp.list.map((g) => ({ value: g.genId, label: g.name, hint: g.t != null ? `temp ${g.t.toFixed(2)}` : '' })) })),
  );
  let whatPick = $derived.by(() => {
    const G = [];
    const grp = (label, items) => { items = items.filter(Boolean); if (items.length) G.push({ label, items }); };
    const o = (value, label) => ({ value, label });
    if (dim !== 1) grp('Biomes', [o('biome', 'Biome'), o('adj', 'Biome next to biome'), !dim && o('bee', 'Bee climate'), o('climate', 'Biome by humidity / temperature')]);
    if (!dim) {
      grp('Structures', [o('stronghold', 'Stronghold (portal room)'), o('strongholdSurface', 'Stronghold under land / ocean'), o('village', 'Village'), o('villageStyle', 'Village of a style…'),
        advanced && o('mineshaft', 'Mineshaft'), advanced && o('geode', 'Abyssal geode (dark ores)'), advanced && o('lake', 'Underground lake (lava / water)'), advanced && o('loot', 'Chest loot (monster room / mineshaft cart)')]);
      grp('Terrain', [o('surface', 'Surface block (sand, clay, snow…)'), worldType === 1 && o('volcano', 'Volcano'), worldType === 1 && o('lava', 'Lava cave'), advanced && o('lootgames', 'LootGames dungeon'), advanced && o('roguelike', 'Roguelike area')]);
      if (advanced) grp('Thaumcraft', [...[2, 3, 1, 4].map((k) => o(`tc:${k}`, TC[k])), o('aspect', 'Aura node with aspect…'), o('nodeType', 'Aura node of a type…')]);
      if (fluidList.length && advanced) grp('Underground fluids', [o('fluid:-1', 'Any oil / gas'), ...fluidList.map((f, fi) => o(`fluid:${fi}`, f.displayName))]);
    } else {
      grp('Structures', [dim === -1 && o('fortress', 'Nether fortress (optionally ≥ N blaze spawners)'), dim === 7 && o('tf', 'Twilight Forest structure / feature'),
        dim === 1 && o('endIsland', 'HEE biome island (biome / variation)'), dim === 1 && o('endTower', 'HEE dungeon tower'), dim === 1 && advanced && o('endComet', 'Draconic Evolution comet')]);
      if (dim !== 1 && advanced) grp('Thaumcraft', [o('aspect', 'Aura node with aspect…'), o('nodeType', 'Aura node of a type…')]);
    }
    if (veinList.length && advanced) grp('GT ore veins', veinList.map((v, vi) => o(`vein:${vi}`, veinName(v))));
    return G;
  });
  const lootPick = [{ label: '', items: LOOT_ITEMS.map((it) => ({ value: it[0], label: it[1] })) }];
  let tfFeatures = $derived((dimLegend?.features ?? []).filter((f) => f.id !== 0 && !HILLS.includes(f.id)));

  const veinName = (v) => v?.displayName ?? (v?.name ?? '?').replace(/^ore\.mix\./, '').replace(/^./, (c) => c.toUpperCase());
  const biomeName = (id) => (dim ? dimLegend?.biomes.find((b) => b.id === Number(id))?.name : palette?.groups.get(Number(id))?.name) ?? '?';

  /** Ids for a biome condition: overworld RWG ids of an in-game biome (or a bee climate); other dims: the biome id. */
  // dimension the JSON helpers build for: the shown one, or a preset check's (`checkPreset`)
  let jsonDim = null;
  const jd = () => jsonDim ?? dim;
  function biomeIds(genId, bee) {
    if (jd()) return [Number(genId)];
    if (bee != null) {
      const ids = [];
      for (const g of palette.groups.values()) if (!g.lava && g.bee === Number(bee)) ids.push(...g.rwgIds);
      return ids;
    }
    return (palette.groups.get(Number(genId))?.rwgIds ?? []).filter((id) => id !== LAVA_MARKER);
  }

  // a condition switched to humidity / temperature starts at "humidity ≥ 85%" (Forestry's damp)
  $effect(() => {
    for (const c of conds) if (c.what === 'climate' && c.hmin === undefined) c.hmin = 85;
  });
  /** Biomes (of this dimension's list) whose humidity % / temperature lie in the condition's ranges. */
  function climateBiomes(c) {
    const hlo = num(c.hmin, 0), hhi = num(c.hmax, 1000), tlo = num(c.tmin, -10), thi = num(c.tmax, 10);
    const opts = jd() === dim ? biomeOptions : jd() === 0 && palette ? [...palette.groups.values()].filter((g) => !g.lava) : [];
    return opts.filter((g) => {
      const k = climate.get(g.genId);
      if (!k) return false;
      const h = Math.round(k[1] * 100);
      return h >= hlo && h <= hhi && k[0] >= tlo - 1e-6 && k[0] <= thi + 1e-6;
    });
  }
  const num = (v, d) => (v === '' || v == null || !Number.isFinite(Number(v)) ? d : Number(v));
  function climateIds(c) {
    const out = [];
    for (const g of climateBiomes(c)) out.push(...biomeIds(g.genId));
    return out;
  }
  /** Index this condition is measured from (a relation), or null: only a top-level condition can be a parent. */
  function relFrom(c, i, list = conds) {
    if (c.from === '' || c.from == null) return null;
    const k = Number(c.from);
    if (!Number.isInteger(k) || k === i || k < 0 || k >= list.length) return null;
    const p = list[k];
    return p.from === '' || p.from == null ? k : null;
  }
  /** Conditions this one may be measured from: the others that are not themselves relative. */
  function parentOptions(i) {
    if (conds.some((o, j) => j !== i && relFrom(o, j) === i)) return []; // already a parent: no chains
    return conds.map((o, k) => [o, k]).filter(([o, k]) => k !== i && (o.from === '' || o.from == null));
  }
  function removeCond(i) {
    conds = conds
      .filter((_, j) => j !== i)
      .map((o) => {
        if (o.from === '' || o.from == null) return o;
        const k = Number(o.from);
        return { ...o, from: k === i ? '' : k > i ? String(k - 1) : o.from };
      });
  }
  // flat area in a biome / bee climate / humidity condition (overworld)
  const FLAT_DEFAULTS = { flat: false, ftol: 20, fpct: 60, farea: 128, fland: true };
  /** Surface block groups (wasm `surface::blk` codes). */
  const SURFACE_GROUPS = [
    { k: 'sand', label: 'Sand', codes: [7] },
    { k: 'redsand', label: 'Red sand', codes: [8] },
    { k: 'sandstone', label: 'Sandstone', codes: [9] },
    { k: 'gravel', label: 'Gravel', codes: [10] },
    { k: 'clay', label: 'Clay (hardened / stained, any colour)', codes: [11, ...Array.from({ length: 16 }, (_, i) => 64 + i)] },
    { k: 'snow', label: 'Snow layer', codes: [13] },
    { k: 'snowblock', label: 'Snow block', codes: [12] },
    { k: 'ice', label: 'Ice / packed ice', codes: [14, 15] },
    { k: 'stone', label: 'Bare stone', codes: [1] },
    { k: 'cobble', label: 'Cobblestone', codes: [6] },
    { k: 'podzol', label: 'Podzol', codes: [5] },
    { k: 'coarse', label: 'Coarse dirt', codes: [4] },
    { k: 'mycelium', label: 'Mycelium', codes: [20] },
    { k: 'mud', label: 'Mud (BOP)', codes: [24] },
    { k: 'hardsand', label: 'Hard sand (BOP)', codes: [22] },
    { k: 'harddirt', label: 'Hard dirt (BOP)', codes: [23] },
    { k: 'longgrass', label: 'Long grass (BOP Garden)', codes: [21] },
    { k: 'ash', label: 'Volcanic ash (BOP)', codes: [29, 30] },
    { k: 'lava', label: 'Lava', codes: [17] },
    { k: 'obsidian', label: 'Obsidian', codes: [18] },
  ];
  const surfaceGroup = (c) => SURFACE_GROUPS.find((g) => g.k === c.sblock) ?? SURFACE_GROUPS[1];
  const canFlat = (c) => !jd() && ['biome', 'bee', 'climate'].includes(c.what);
  const isFlat = (c) => canFlat(c) && (c.flat === true || c.flat === 'true');
  // a missing / empty field means its default (conditions made before the option existed, loaded share codes)
  const fv = (c, k) => (c[k] === undefined || c[k] === '' || c[k] === null ? FLAT_DEFAULTS[k] : c[k]);
  const flatJson = (c) => {
    if (!isFlat(c)) return {};
    const side = Math.max(0, Number(fv(c, 'farea')) || 0);
    const land = fv(c, 'fland');
    return { flat: { tol: Math.max(0, Math.round(Number(fv(c, 'ftol')) || 0)), pct: Math.min(100, Math.max(0, Number(fv(c, 'fpct')) || 0)), area: Math.round(side * side), land: land !== false && land !== 'false' } };
  };
  const flatLabel = (c) => (isFlat(c) ? ` · flat ±${Number(fv(c, 'ftol')) || 0}, ≥ ${Number(fv(c, 'fpct')) || 0}%${Number(fv(c, 'farea')) ? `, ≥ ${fv(c, 'farea')}²` : ''}` : '');
  function toJson(c, i, list = conds) {
    const k = relFrom(c, i, list);
    const base = { r: Number(c.r) || 0, not: !!c.not, ...(k != null ? { from: k } : {}) };
    if (c.what === 'biome') return { t: 'biome', ids: biomeIds(c.genId), ...flatJson(c), ...base };
    if (c.what === 'bee') return { t: 'biome', ids: biomeIds(null, c.bee), ...flatJson(c), ...base };
    if (c.what === 'climate') return { t: 'biome', ids: climateIds(c), ...flatJson(c), ...base };
    if (c.what === 'adj') return { t: 'adj', a: biomeIds(c.genId), b: biomeIds(c.genIdB), dist: Number(c.dist) || 64, ...base };
    if (c.what === 'tf') return { t: 'tf', ids: c.feat === 'hills' ? HILLS : [Number(c.feat)], ...base };
    if (c.what.startsWith('tc:')) return { t: 'tc', kind: Number(c.what.slice(3)), ...base };
    if (c.what.startsWith('vein:')) return { t: 'vein', vein: Number(c.what.slice(5)), fast: !!jd() && c.veinFast === true, ...base };
    if (c.what === 'aspect') return { t: 'aspect', aspect: c.aspect === '' || c.aspect == null ? -1 : aspectTags.indexOf(c.aspect), min: Number(c.amin) || 1, ...base };
    if (c.what.startsWith('fluid:')) return { t: 'fluid', fluid: Number(c.what.slice(6)), minLpo: Number(c.minLpo) || 0, ...base };
    if (c.what === 'fortress') return { t: 'fortress', spawners: Number(c.spawners) || 0, ...base };
    if (c.what === 'villageStyle') return { t: 'villageStyle', style: Number(c.style) || 0, ...base };
    if (c.what === 'nodeType') return { t: 'nodeType', type: c.ntype || 'Normal', mod: c.nmod || '', ...base };
    if (c.what === 'strongholdSurface') return { t: 'strongholdSurface', ocean: c.ocean === true || c.ocean === 'true', ...base };
    if (c.what === 'endIsland') return { t: 'endIsland', ...(c.ibiome !== '' && c.ibiome != null ? { biome: Number(c.ibiome) } : {}), ...(c.ivar !== '' && c.ivar != null ? { variation: Number(c.ivar) } : {}), ...base };
    if (c.what === 'endComet') return { t: 'endComet', min: Number(c.cmin) || 2, ...base };
    if (c.what === 'loot') return { t: 'loot', source: c.lsrc || '', items: lootFrags(c), min: Math.max(1, Number(c.lcount) || 1), ...base };
    if (c.what === 'lake') return { t: 'lake', liquid: c.lliq || '', minY: Math.max(0, Number(c.lminy) || 0), min: Math.max(0, Number(c.lmin) || 0), open: !!c.lopen, ...base };
    if (c.what === 'geode') return { t: 'geode', min: Math.max(0, Number(c.gmin) || 0), kind: c.gkind || '', ...base };
    if (c.what === 'surface') return { t: 'surface', blocks: surfaceGroup(c).codes, min: Math.max(1, Number(c.smin) || 1), water: !!c.swater, ...base };
    return { t: c.what, ...base };
  }

  // chest-loot condition: the item fragments sent to the search (custom = the typed fragments, comma separated)
  function lootFrags(c) {
    const it = LOOT_ITEMS.find((i) => i[0] === c.litem) ?? LOOT_ITEMS[0];
    return it[2] ?? String(c.ltext || '').split(',').map((t) => t.trim()).filter(Boolean);
  }

  const centerJson = () => (center === 'point' ? [Math.round(Number(cx) || 0), Math.round(Number(cz) || 0)] : dim ? 'origin' : center);
  const query = () => JSON.stringify({ worldType, dim, landAtSpawn: !dim && landAtSpawn, step: Number(step), center: centerJson(), conds: conds.map((c, i) => toJson(c, i)) });
  const centerLabel = () => (center === 'point' ? `${Math.round(Number(cx) || 0)}, ${Math.round(Number(cz) || 0)}` : center === 'spawn' && !dim ? 'spawn' : '0, 0');

  // "(Y 64–104, 72% of 1.3 km²)": band (ground Y), share of the patch, patch area (+ if cut at the scan edge);
  // " · exact 66% ⚠" once the exact-terrain check is back (⚠ = below the condition's share)
  const flatHit = (h) => {
    const f = h.flat;
    if (!Array.isArray(f)) return '';
    const km = (a) => (a >= 1e6 ? `${(a / 1e6).toFixed(a >= 1e7 ? 0 : 1)} km²` : `${Math.round(a / 1e3)}k m²`);
    const e = h.exact;
    const ex = !e ? '' : e.pending ? ' · exact …' : e.error ? ' · exact ?' : ` · exact ${e.inBand}%${e.inBand < (h.flatQ?.flat?.pct ?? 0) ? ' ⚠' : ''}`;
    return ` (Y ${f[0]}–${f[1]}, ${f[2]}% of ${km(f[3])}${f[5] ? '+' : ''}${ex})`;
  };
  // exact-terrain check of flat hits: the first FLAT_CHECKS matches of a run, exact chunk tops on a subsample of
  // the patch (≤ 1200 cells, every k-th grid cell); the share of that sample inside the band the search found
  const FLAT_CHECKS = 15;
  let flatChecks = 0;
  function verifyFlat(res, runToken) {
    for (const h of res.hits) {
      if (!h.flatQ || flatChecks >= FLAT_CHECKS) continue;
      flatChecks++;
      setExact(res, h, { pending: true });
      const { worldType: wt, ...fq } = h.flatQ;
      pool
        .request({ type: 'flatPatch', seed: res.seed, worldType: wt, query: JSON.stringify({ ...fq, maxExact: 1200 }), exact: true }, { priority: 1e6 + 1, cls: 'search' })
        .then((r) => setExact(res, h, r.result))
        .catch((e) => setExact(res, h, { error: String(e) }));
    }
    void runToken;
  }
  function setExact(res, h, e) {
    results = results.map((r) => (r.seed === res.seed && r.dim === res.dim ? { ...r, hits: r.hits.map((x) => (x.cond === h.cond && x.x === h.x && x.z === h.z ? { ...x, exact: e } : x)) } : r));
  }
  const label = (c) => labelBase(c) + flatLabel(c);
  function labelBase(c) {
    if (c.what === 'biome') return biomeName(c.genId);
    if (c.what === 'bee') return `Bees ${beeName(Number(c.bee))}`;
    if (c.what === 'climate') {
      const p = [];
      if (c.hmin !== '' && c.hmin != null) p.push(`humidity ≥ ${c.hmin}%`);
      if (c.hmax !== '' && c.hmax != null) p.push(`humidity ≤ ${c.hmax}%`);
      if (c.tmin !== '' && c.tmin != null) p.push(`temp ≥ ${c.tmin}`);
      if (c.tmax !== '' && c.tmax != null) p.push(`temp ≤ ${c.tmax}`);
      return `Biome with ${p.join(', ') || 'any climate'} (${climateBiomes(c).length})`;
    }
    if (c.what === 'adj') return `${biomeName(c.genId)} next to ${biomeName(c.genIdB)} (≤${Number(c.dist) || 64})`;
    if (c.what === 'tf') return c.feat === 'hills' ? 'Hollow hill' : (dimLegend?.features.find((f) => f.id === Number(c.feat))?.name ?? 'TF feature');
    if (c.what === 'fortress') return Number(c.spawners) ? `Nether fortress with ≥ ${c.spawners} blaze spawner${Number(c.spawners) > 1 ? 's' : ''}` : 'Nether fortress';
    if (c.what === 'villageStyle') return `Village (${STYLES[Number(c.style) || 0]})`;
    if (c.what === 'nodeType') {
      const m = NODE_MODS.find(([v]) => v === (c.nmod || ''))?.[1];
      return `${c.ntype || 'Normal'} aura node${c.nmod ? ` (${m})` : ''}`;
    }
    if (c.what === 'strongholdSurface') return `Stronghold under ${c.ocean === true || c.ocean === 'true' ? 'ocean' : 'land'}`;
    if (c.what === 'endIsland') {
      const b = c.ibiome !== '' && c.ibiome != null ? END_BIOMES[Number(c.ibiome)] : 'HEE';
      const v = c.ivar !== '' && c.ivar != null ? END_VARS.flat().find(([id]) => id === Number(c.ivar))?.[1] : '';
      return `${b} island${v ? ` (${v})` : ''}`;
    }
    if (c.what === 'endTower') return 'Dungeon tower';
    if (c.what === 'endComet') return Number(c.cmin) > 2 ? `Draconic comet, size ≥ ${c.cmin}` : 'Draconic comet';
    if (c.what === 'surface') return `${Number(c.smin) > 1 ? `≥ ${c.smin} columns of ` : ''}${surfaceGroup(c).label.toLowerCase()} on the surface${c.swater ? ' (or under water)' : ''}`;
    if (c.what === 'loot') {
      const it = LOOT_ITEMS.find((i) => i[0] === c.litem) ?? LOOT_ITEMS[0];
      const src = c.lsrc === 'room' ? 'monster room' : c.lsrc === 'cart' ? 'mineshaft chest cart' : 'monster room or chest cart';
      if (!it[2] || it[2].length) return `${src} with ${Number(c.lcount) > 1 ? `≥ ${c.lcount} ` : ''}${it[2] ? it[1] : `"${c.ltext || '?'}"`}`;
      return src.charAt(0).toUpperCase() + src.slice(1);
    }
    if (c.what === 'lake') return `Underground ${c.lliq || 'lava or water'} lake${Number(c.lminy) ? ` (top ≥ Y${c.lminy})` : ''}${Number(c.lmin) ? ` ≥ ${c.lmin} blocks` : ''}${c.lopen ? ', open to a cave' : ''}`;
    if (c.what === 'geode') return `${c.gkind === 'ocean' ? 'Ocean ' : c.gkind === 'mountain' ? 'Mountain ' : ''}abyssal geode${Number(c.gmin) ? ` with ≥ ${c.gmin} dark diamond` : ''}`;
    if (c.what.startsWith('tc:')) return TC[c.what.slice(3)];
    if (c.what.startsWith('vein:')) return veinName(veinList[Number(c.what.slice(5))]);
    if (c.what === 'aspect') return c.aspect ? `Aura node with ${c.aspect} ≥ ${Number(c.amin) || 1}` : 'Aura node';
    if (c.what.startsWith('fluid:')) {
      const i = Number(c.what.slice(6));
      return `${i < 0 ? 'Any fluid' : fluidList[i]?.displayName ?? '?'}${Number(c.minLpo) ? ` ≥ ${c.minLpo} L/op` : ''}`;
    }
    return { stronghold: 'Stronghold', village: 'Village', mineshaft: 'Mineshaft', volcano: 'Volcano', lava: 'Lava cave', lootgames: 'LootGames', roguelike: 'Roguelike area' }[c.what] ?? c.what;
  }

  // slow conditions (terrain models over every chunk in range, or many biome lookups)
  const slow = (c) => isFlat(c) || c.what === 'geode' || c.what === 'lake' || c.what === 'loot' || c.what === 'surface' || c.what.startsWith('tc:') || c.what === 'aspect' || c.what === 'nodeType' || c.what === 'adj' || (!!dim && c.what.startsWith('vein:') && !c.veinFast);

  const blankCond = (d = dim) => {
    const g = biomeOptions[0]?.genId ?? 193;
    return { what: d === 1 ? 'endTower' : 'biome', ibiome: '', ivar: '', cmin: 2, genId: g, genIdB: biomeOptions[1]?.genId ?? g, dist: 64, bee: beeOptions[0] ?? 3, feat: 6, r: 1000, not: false, minLpo: 200, spawners: 0, style: 0, ntype: 'Pure', nmod: '', ocean: false, veinFast: false, hmin: 85, hmax: '', tmin: '', tmax: '', gmin: 0, gkind: '', lliq: 'lava', lminy: 0, lmin: 0, lopen: false, lsrc: '', litem: 'diamond', ltext: '', lcount: 1, sblock: 'redsand', smin: 1, swater: false, aspect: '', amin: 1, from: '', ...FLAT_DEFAULTS };
  };
  function addCond() {
    conds = [...conds, blankCond()];
  }

  // ------------------------------------------------------------------------------------------- dev presets
  // `load` = { preset, run, n } from the dev section: biome / vein names resolve to ids here (overworld palette,
  // dimension legend); a dimension switch is applied first, `run` starts once the dimension has followed
  let lastLoad = 0;
  let pendingRun = $state(false);
  let presetNote = $state('');
  function resolvePresetCond(pc, d) {
    const byName = (name) => {
      if (d) return dimLegend?.biomes.find((b) => b.name === name)?.id;
      for (const g of palette?.groups.values() ?? []) if (g.name === name && !g.lava) return g.genId;
      return undefined;
    };
    const c = { ...blankCond(d), ...pc };
    delete c.biome; delete c.biomeB; delete c.vein;
    if (pc.biome != null) {
      c.genId = byName(pc.biome);
      if (c.genId == null) throw new Error(`no biome "${pc.biome}"`);
    }
    if (pc.biomeB != null) {
      c.genIdB = byName(pc.biomeB);
      if (c.genIdB == null) throw new Error(`no biome "${pc.biomeB}"`);
    }
    if (pc.vein != null) {
      const list = d ? (dimLegend?.veins ?? []) : (veins?.veins ?? []);
      const i = list.findIndex((v) => veinName(v) === pc.vein);
      if (i < 0) throw new Error(`no vein "${pc.vein}"`);
      c.what = `vein:${i}`;
    }
    return c;
  }
  /** Dev check: run a preset over its fixed seed range (`check.from`, default 1, `check.n` seeds) without touching
   * the form; resolves to `{ found, n, ms }` (or `{ error }`). Same query the form would build, any dimension. */
  export async function checkPreset(p) {
    const d = p.dim ?? 0;
    let q;
    try {
      if (d && d !== dim && p.conds.some((c) => c.vein != null || c.biome != null)) throw new Error('needs its dimension shown');
      if (!d && !palette) throw new Error('palette not loaded');
      const list = p.conds.map((c) => resolvePresetCond(c, d));
      jsonDim = d;
      try {
        q = JSON.stringify({ worldType: 1, dim: d, landAtSpawn: !d && p.landAtSpawn !== false, step: 64, center: d ? 'origin' : (p.center ?? 'origin'), conds: list.map((c, i) => toJson(c, i, list)) });
      } finally {
        jsonDim = null;
      }
    } catch (e) {
      return { error: e.message };
    }
    const n = p.check?.n ?? 400;
    let s = BigInt(p.check?.from ?? 1);
    const end = s + BigInt(n);
    const BATCH_N = 64;
    const t0 = performance.now();
    let found = 0, err = null;
    const lane = async () => {
      while (!err && s < end) {
        const a = s;
        const b = a + BigInt(BATCH_N) < end ? a + BigInt(BATCH_N) : end;
        s = b;
        const seeds = new BigInt64Array(Number(b - a));
        for (let i = 0; i < seeds.length; i++) seeds[i] = a + BigInt(i);
        const r = await pool.request({ type: 'search', query: q, seeds }, { priority: 1e6, cls: 'search' });
        if (r.result.error) err = r.result.error;
        else found += r.result.matches.length;
      }
    };
    try {
      await Promise.all(Array.from({ length: Math.max(1, pool.size) }, lane));
    } catch (e) {
      err = String(e);
    }
    return err ? { error: err } : { found, n, ms: Math.round(performance.now() - t0) };
  }

  $effect(() => {
    const l = load;
    if (!l || l.n === lastLoad) return;
    lastLoad = l.n;
    untrack(() => {
      const p = l.preset;
      const d = p.dim ?? 0;
      try {
        if (d && d !== dim && !p.conds.every((c) => c.biome == null && c.vein == null)) throw new Error('switch dimension first');
        const list = p.conds.map((c) => resolvePresetCond(c, d));
        stop();
        conds = list;
        condsDim = d; // keep the loaded list when the dimension follows
        if (d !== dim) ondim?.(d);
        center = p.center ?? 'origin';
        if (d && center === 'spawn') center = 'origin';
        presetNote = `loaded "${p.name}"`;
        pendingRun = !!l.run;
      } catch (e) {
        presetNote = `preset "${p.name}": ${e.message}`;
        pendingRun = false;
      }
    });
  });
  $effect(() => {
    if (pendingRun && dim === condsDim && (dim ? dimLegend : palette)) {
      pendingRun = false;
      untrack(() => start());
    }
  });

  // the condition list belongs to one dimension: a dimension switch starts a fresh list
  $effect(() => {
    const d = dim;
    if (d !== condsDim) {
      stop();
      conds = defaultConds(d);
      condsDim = d;
      if (d && center === 'spawn') center = 'origin';
    }
  });

  // ------------------------------------------------------------------------------------------- seed sources
  function parseBig(t, dflt) {
    try {
      return BigInt(String(t).trim() || String(dflt));
    } catch {
      return BigInt(dflt);
    }
  }
  function parseList(text) {
    return text
      .split(/\r?\n/)
      .map((l) => l.trim())
      .filter(Boolean)
      .map((l) => parseSeed(l)?.seed)
      .filter((s) => s != null);
  }
  /** Up to n seeds; fewer (or none) once a finite source is exhausted. */
  function nextSeeds(n) {
    const out = [];
    for (let i = 0; i < n; i++) {
      if (source === 'random') out.push(randomSeed());
      else if (source === 'list') {
        if (listPos >= listSeeds.length) break;
        out.push(listSeeds[listPos++]);
      } else {
        if (source === 'range' && next > last) break;
        out.push(BigInt.asIntN(64, next));
        next += 1n;
      }
    }
    return BigInt64Array.from(out);
  }

  // ------------------------------------------------------------------------------------------- run
  async function start() {
    if ((!palette && !dim) || (!conds.length && (dim || !landAtSpawn))) return;
    error = '';
    if (conds.some((c) => c.what === 'climate' && !c.not && !climateBiomes(c).length)) {
      error = 'a humidity / temperature condition matches no biome';
      return;
    }
    results = [];
    resultsNote = '';
    checked = 0;
    rate = 0;
    next = parseBig(startText, 0);
    last = parseBig(rangeTo, 0);
    total = 0;
    if (source === 'range') {
      if (last < next) [next, last] = [last, next];
      const n = last - next + 1n;
      total = n > 1000000000n ? Infinity : Number(n);
    }
    if (source === 'list') {
      listSeeds = parseList(listText);
      listPos = 0;
      total = listSeeds.length;
      if (!total) {
        error = 'the seed list is empty';
        return;
      }
    }
    running = true;
    const id = ++runId;
    const q = query();
    const qdim = dim;
    const qj = JSON.parse(q);
    flatChecks = 0;
    const snapshot = conds.map((c, i) => ({ ...c, label: label(c), base: label(c), rel: relFrom(c, i) }));
    for (const c of snapshot) if (c.rel != null) c.label = `${c.base} near ${snapshot[c.rel].base}`;
    const t0 = performance.now();
    const lanes = Math.max(1, pool.size);
    const batch = snapshot.some(slow) ? 2 : BATCH;
    const lane = async () => {
      while (running && id === runId && results.length < maxResults) {
        const seeds = nextSeeds(batch);
        if (!seeds.length) return;
        let r;
        try {
          r = await pool.request({ type: 'search', query: q, seeds }, { priority: 1e6, cls: 'search' });
        } catch (e) {
          if (String(e).includes('cancelled')) continue;
          error = String(e);
          running = false;
          return;
        }
        if (id !== runId) return;
        if (r.result.error) {
          error = r.result.error;
          running = false;
          return;
        }
        checked += r.result.checked;
        rate = checked / ((performance.now() - t0) / 1000);
        for (const m of r.result.matches) {
          if (results.length >= maxResults) break;
          const mx = m.center?.[0] ?? 0, mz = m.center?.[1] ?? 0;
          results = [
            ...results,
            {
              seed: m.seed,
              dim: qdim,
              center: [mx, mz],
              hits: m.hits.map((h) => {
                const sc = snapshot[h.cond];
                // relation: distance from the parent's hit, not the centre
                const ph = sc.rel != null ? m.hits.find((q) => q.cond === sc.rel) : null;
                return {
                  ...h,
                  label: sc.label,
                  what: sc.what,
                  genId: ['biome', 'adj'].includes(sc.what) && !qdim ? Number(sc.genId) : null,
                  r: Number(sc.r),
                  dist: Math.round(ph ? Math.hypot(h.x - ph.x, h.z - ph.z) : Math.hypot(h.x - mx, h.z - mz)),
                  rel: sc.rel,
                  flat: Array.isArray(h.flat) ? h.flat : null,
                  // what the map overlay / exact check need to rebuild this hit's patch
                  flatQ: Array.isArray(h.flat) && qj.conds[h.cond]?.flat
                    ? { ids: qj.conds[h.cond].ids, flat: qj.conds[h.cond].flat, c: ph ? [ph.x, ph.z] : [mx, mz], r: Number(sc.r), hit: [h.x, h.z], worldType: qj.worldType }
                    : null,
                };
              }),
              nots: snapshot.filter((c) => c.not).map((c) => (c.rel != null ? `no ${c.base} ≤ ${c.r} of that ${snapshot[c.rel].base}` : `no ${c.label} ≤ ${c.r}`)),
            },
          ];
          verifyFlat(results[results.length - 1], id);
        }
      }
    };
    await Promise.all(Array.from({ length: lanes }, lane));
    if (id === runId) running = false;
  }

  function stop() {
    running = false;
    runId++;
  }

  // the query is world-type specific; stop if it changes underneath
  $effect(() => {
    worldType;
    stop();
  });

  // ------------------------------------------------------------------------------------------- share / load
  const SHARE_KEYS = ['what', 'genId', 'genIdB', 'dist', 'bee', 'feat', 'r', 'not', 'minLpo', 'aspect', 'amin', 'spawners', 'style', 'ntype', 'nmod', 'ocean', 'ibiome', 'ivar', 'cmin', 'veinFast', 'hmin', 'hmax', 'tmin', 'tmax', 'gmin', 'gkind', 'lliq', 'lminy', 'lmin', 'lopen', 'sblock', 'smin', 'swater', 'flat', 'ftol', 'fpct', 'farea', 'fland'];
  function encodeSearch() {
    const j = {
      // every per-condition field (a missing one falls back to its default when loaded)
      c: conds.map((c) => {
        const o = {};
        for (const k of SHARE_KEYS) if (c[k] !== undefined) o[k] = c[k];
        o.from = c.from ?? '';
        return o;
      }),
      dim,
      center,
      cx: Number(cx) || 0,
      cz: Number(cz) || 0,
      step,
      landAtSpawn,
      source,
      from: startText,
      to: rangeTo,
    };
    if (source === 'list' && listText.length <= 4000) j.list = listText;
    const s = JSON.stringify(j);
    return btoa(unescape(encodeURIComponent(s))).replace(/\+/g, '-').replace(/\//g, '_').replace(/=+$/, '');
  }
  function decodeSearch(code) {
    try {
      const j = JSON.parse(decodeURIComponent(escape(atob(code.replace(/-/g, '+').replace(/_/g, '/')))));
      const d = [0, -1, 7, 1].includes(Number(j.dim)) ? Number(j.dim) : 0;
      if (Array.isArray(j.c)) conds = j.c.map((c) => ({ ...FLAT_DEFAULTS, ...c }));
      condsDim = d; // keep the loaded list when the dimension follows
      if (d !== dim) ondim?.(d);
      if (j.center) center = j.center;
      if (j.cx != null) cx = j.cx;
      if (j.cz != null) cz = j.cz;
      if (j.step) step = j.step;
      if (typeof j.landAtSpawn === 'boolean') landAtSpawn = j.landAtSpawn;
      if (['random', 'sequential', 'range', 'list'].includes(j.source)) source = j.source;
      if (j.from != null) startText = String(j.from);
      if (j.to != null) rangeTo = String(j.to);
      if (typeof j.list === 'string') listText = j.list;
      return true;
    } catch {
      return false;
    }
  }
  let shareNote = $state('');
  function note(t, ms = 3500) {
    shareNote = t;
    setTimeout(() => (shareNote = ''), ms);
  }
  async function copyText(text, okNote) {
    try {
      await navigator.clipboard.writeText(text);
      note(okNote);
    } catch {
      window.prompt('Copy this', text);
    }
  }
  function shareSearch() {
    const code = encodeSearch();
    copyText(embedded ? code : `${location.origin}${location.pathname}#search=${code}`, embedded ? 'search code copied (paste it into "load")' : 'link copied');
  }
  function loadSearch() {
    const t = window.prompt('Paste a search link or code');
    if (!t) return;
    const code = t.includes('search=') ? t.split('search=')[1].split('&')[0] : t.trim();
    note(decodeSearch(code) ? 'search loaded' : 'not a valid search code');
  }

  // ------------------------------------------------------------------------------------------- saved lists
  const LISTS_KEY = 'gtnh-seedmap:searchLists';
  function readLists() {
    try {
      const v = JSON.parse(localStorage.getItem(LISTS_KEY) ?? '{}');
      return v && typeof v === 'object' ? v : {};
    } catch {
      return {};
    }
  }
  let lists = $state(readLists());
  let listName = $state('');
  let pickA = $state('');
  let pickB = $state('');
  function writeLists(v) {
    lists = v;
    try {
      localStorage.setItem(LISTS_KEY, JSON.stringify(v));
    } catch {
      note('could not store the list (browser storage unavailable or full)');
    }
  }
  function saveList() {
    if (!results.length) return note('nothing to save');
    const name = (listName.trim() || `${DIM_NAMES[dim]} ${new Date().toISOString().slice(0, 16).replace('T', ' ')}`).slice(0, 60);
    writeLists({ ...lists, [name]: { saved: Date.now(), dim, worldType, query: conds.map(label), results: $state.snapshot(results) } });
    pickA = name;
    listName = '';
    note(`saved "${name}" (${results.length} seeds)`);
  }
  function showList(name) {
    const l = lists[name];
    if (!l) return;
    stop();
    results = l.results ?? [];
    resultsNote = `saved list "${name}" · ${DIM_NAMES[l.dim ?? 0]} · ${(l.query ?? []).join(' · ')}`;
  }
  function deleteList(name) {
    if (!lists[name] || !window.confirm(`Delete the saved list "${name}"?`)) return;
    const v = { ...lists };
    delete v[name];
    writeLists(v);
    if (pickA === name) pickA = '';
    if (pickB === name) pickB = '';
  }
  function exportList(name) {
    const l = lists[name];
    if (l) copyText(JSON.stringify({ name, ...l }), `list "${name}" copied as JSON`);
  }
  function importList() {
    const t = window.prompt('Paste a saved list (JSON)');
    if (!t) return;
    try {
      const j = JSON.parse(t);
      if (!Array.isArray(j.results)) throw new Error('no results');
      const results = j.results.filter((r) => r && typeof r.seed === 'string').map((r) => ({ seed: r.seed, dim: Number(r.dim ?? j.dim) || 0, center: Array.isArray(r.center) ? r.center : [0, 0], hits: Array.isArray(r.hits) ? r.hits : [], nots: Array.isArray(r.nots) ? r.nots : [] }));
      let name = String(j.name || 'imported').slice(0, 60);
      while (lists[name]) name += ' (2)';
      writeLists({ ...lists, [name]: { saved: Date.now(), dim: Number(j.dim) || 0, worldType: j.worldType, query: Array.isArray(j.query) ? j.query.map(String) : [], results } });
      pickA = name;
      note(`imported "${name}" (${results.length} seeds)`);
    } catch {
      note('not a saved list');
    }
  }
  function compare(mode) {
    const a = lists[pickA], b = lists[pickB];
    if (!a || !b) return note('pick two saved lists');
    const inB = new Set((b.results ?? []).map((r) => r.seed));
    stop();
    if (mode === 'both') {
      const byB = new Map((b.results ?? []).map((r) => [r.seed, r]));
      results = (a.results ?? []).filter((r) => inB.has(r.seed)).map((r) => ({ ...r, hits: [...r.hits, ...(byB.get(r.seed)?.dim === r.dim ? byB.get(r.seed).hits : [])], nots: [...(r.nots ?? []), ...(byB.get(r.seed)?.nots ?? [])] }));
      resultsNote = `in both "${pickA}" and "${pickB}"`;
    } else {
      results = (a.results ?? []).filter((r) => !inB.has(r.seed));
      resultsNote = `in "${pickA}" but not in "${pickB}"`;
    }
  }
  let listNames = $derived(Object.keys(lists).sort((x, y) => (lists[y].saved ?? 0) - (lists[x].saved ?? 0)));

  // restore a shared search from the URL
  try {
    const m = location.hash.match(/search=([^&]+)/);
    if (m) decodeSearch(m[1]);
  } catch {}
</script>

<div class="search">
  <div class="row head">
    <span>Conditions <span class="dim small-n">· {DIM_NAMES[dim]}</span></span>
    <label class="dim from">from
      <select bind:value={center}>
        <option value="origin">0, 0</option>
        {#if !dim}<option value="spawn">spawn (est., slower)</option>{/if}
        <option value="point">a point…</option>
      </select>
    </label>
  </div>
  {#if center === 'point'}
    <div class="row">
      <span class="dim">x</span><input type="number" step="100" bind:value={cx} />
      <span class="dim">z</span><input type="number" step="100" bind:value={cz} />
    </div>
  {/if}
  {#each conds as c, i}
    {@const rel = relFrom(c, i)}
    <div class="cond" class:rel={rel != null}>
      {#if rel != null}<div class="relhead dim tiny">↳ #{i + 1}, measured from each #{rel + 1} {label(conds[rel])}</div>{:else}<div class="relhead dim tiny">#{i + 1}</div>{/if}
      <div class="row">
        <Picker class="what" bind:value={c.what} groups={whatPick} title="What to look for" placeholder="filter: biome, village, vein…" />
        <button class="x" onclick={() => removeCond(i)} title="Remove">×</button>
      </div>
      {#if c.what === 'biome'}
        <div class="row">
          <Picker class="grow" bind:value={c.genId} groups={biomePick} placeholder="filter biomes…" />
        </div>
      {:else if c.what === 'adj'}
        <div class="row">
          <Picker class="grow" bind:value={c.genId} groups={biomePick} title="Biome A (the hit)" placeholder="filter biomes…" />
        </div>
        <div class="row">
          <span class="dim">with</span>
          <Picker class="grow" bind:value={c.genIdB} groups={biomePick} title="Biome B" placeholder="filter biomes…" />
          <span class="dim">≤</span><input type="number" min="4" step="16" bind:value={c.dist} />
        </div>
      {:else if dim && c.what.startsWith('vein:')}
        <div class="row">
          <select class="grow" bind:value={c.veinFast} title="Which vein GT picks depends on the terrain: the accurate check runs GT's stone test on the exact terrain (~100–200 seeds/s); the fast one takes the first candidate (~25,000 seeds/s)">
            {#if dim === 1}
              <option value={false}>Accurate: GT's test on the End terrain (fresh End 9/9)</option>
              <option value={true}>Fast: first candidate where GT processes ores</option>
            {:else}
              <option value={false}>Accurate: GT's stone test (~92%, slower)</option>
              <option value={true}>Fast: first candidate (~{dim === 7 ? 83 : 69}%)</option>
            {/if}
          </select>
        </div>
      {:else if c.what === 'endIsland'}
        <div class="row">
          <select class="grow" bind:value={c.ibiome} onchange={() => (c.ivar = '')}>
            <option value="">any biome</option>
            {#each END_BIOMES as b, bi}<option value={String(bi)}>{b}</option>{/each}
          </select>
          <select class="grow" bind:value={c.ivar} title="HEE content variation of the island">
            <option value="">any variation</option>
            {#each (c.ibiome !== '' && c.ibiome != null ? END_VARS[Number(c.ibiome)] : END_VARS.flat()) as [id, n]}<option value={String(id)}>{n}</option>{/each}
          </select>
        </div>
      {:else if c.what === 'endComet'}
        <div class="row">
          <span class="dim">size ≥</span>
          <select bind:value={c.cmin} title="WorldGenEnderComet size 2..9 (core radius)">
            {#each [2, 3, 4, 5, 6, 7, 8, 9] as n}<option value={n}>{n}</option>{/each}
          </select>
        </div>
      {:else if c.what === 'tf'}
        <div class="row">
          <select class="grow" bind:value={c.feat}>
            {#each tfFeatures as f (f.id)}<option value={f.id}>{f.name}{f.structure ? '' : ' (terrain)'}</option>{/each}
            <option value="hills">Any hollow hill</option>
          </select>
        </div>
      {:else if c.what === 'aspect'}
        <div class="row">
          <select class="grow" bind:value={c.aspect}>
            <option value="">any node</option>
            {#each aspectTags.filter((t) => !t.startsWith('addon#')) as t}<option value={t}>{t}</option>{/each}
          </select>
          <span class="dim">≥</span>
          <input type="number" min="1" step="5" bind:value={c.amin} />
        </div>
      {:else if c.what === 'fortress'}
        <div class="row">
          <span class="dim">blaze spawners ≥</span>
          <select bind:value={c.spawners} title="Throne pieces (one spawner each); a fortress has at most 2">
            <option value={0}>any</option>
            <option value={1}>1</option>
            <option value={2}>2</option>
          </select>
        </div>
      {:else if c.what === 'villageStyle'}
        <div class="row">
          <select class="grow" bind:value={c.style}>
            {#each STYLES as st, si}<option value={si}>{st}</option>{/each}
          </select>
        </div>
      {:else if c.what === 'nodeType'}
        <div class="row">
          <select class="grow" bind:value={c.ntype}>
            {#each NODE_TYPES as t}<option value={t}>{t.toLowerCase()}</option>{/each}
          </select>
          <select class="grow" bind:value={c.nmod}>
            {#each NODE_MODS as [v, n]}<option value={v}>{n}</option>{/each}
          </select>
        </div>
      {:else if c.what === 'surface'}
        <div class="row">
          <select class="grow" bind:value={c.sblock} title="Exact top block before trees and plants: surface pass, caves, lava caves, and the population snow / ice pass">
            {#each SURFACE_GROUPS as g}<option value={g.k}>{g.label}</option>{/each}
          </select>
          ≥ <input type="number" min="1" max="100000" bind:value={c.smin} style="width: 7ch" title="Minimum number of matching columns (blocks) within the radius" /> cols
        </div>
        <label class="row tiny" title="Also count the sea / lake floor under water"><input type="checkbox" bind:checked={c.swater} /> also under water</label>
      {:else if c.what === 'geode'}
        <div class="row">
          <select class="grow" bind:value={c.gkind} title="NHCore geodes sit in mountain biomes (Y16–31), Railcraft ones under deep ocean">
            <option value="">mountain or ocean</option>
            <option value="mountain">mountain (NHCore)</option>
            <option value="ocean">deep ocean (Railcraft)</option>
          </select>
          ≥ <input type="number" min="0" max="20" bind:value={c.gmin} style="width: 5ch" title="Minimum dark diamond ores (0 = any geode)" /> 💎
        </div>
      {:else if c.what === 'loot'}
        <div class="row">
          <select bind:value={c.lsrc} title="Monster rooms: whole populate replay per chunk (slower). Mineshaft carts: only chunks a mineshaft reaches (faster).">
            <option value="">rooms or carts</option>
            <option value="room">monster rooms</option>
            <option value="cart">mineshaft carts</option>
          </select>
          <Picker class="grow" bind:value={c.litem} groups={lootPick} placeholder="filter items…" />
        </div>
        {#if c.litem === 'custom'}
          <input class="row" placeholder="name fragments, e.g. item.blazeRod, gt.metaitem.01@11054" bind:value={c.ltext} title="Unlocalized item names (as in the chest popups' source tables), any of these comma-separated fragments" />
        {/if}
        {#if c.litem !== 'any'}
          <label class="row tiny" title="Total count of the matching items in one chest">at least <input type="number" min="1" max="1728" bind:value={c.lcount} style="width: 6ch" /> in one chest</label>
        {/if}
      {:else if c.what === 'lake'}
        <div class="row">
          <select class="grow" bind:value={c.lliq} title="RWG2 lakes are all underground: water starts below Y50, lava below Y52">
            <option value="lava">lava</option>
            <option value="water">water</option>
            <option value="">lava or water</option>
          </select>
          top ≥ Y<input type="number" min="0" max="60" bind:value={c.lminy} style="width: 5ch" title="Liquid top at or above this Y (closer to the surface)" />
          ≥ <input type="number" min="0" max="2000" bind:value={c.lmin} style="width: 6ch" title="Minimum liquid blocks (0 = any)" /> blocks
        </div>
        <label class="row tiny" title="The lake touches a cave (reachable without digging)"><input type="checkbox" bind:checked={c.lopen} /> open to a cave</label>
      {:else if c.what === 'strongholdSurface'}
        <div class="row">
          <select class="grow" bind:value={c.ocean} title="Ocean: sea water above the portal frame column (exact terrain, before decoration)">
            <option value={false}>under land</option>
            <option value={true}>under ocean</option>
          </select>
        </div>
      {:else if c.what.startsWith('fluid:')}
        <div class="row">
          <span class="dim">field average ≥</span>
          <input type="number" min="0" step="25" bind:value={c.minLpo} /> <span class="dim">L/op</span>
        </div>
      {:else if c.what === 'climate'}
        {@const cb = climateBiomes(c)}
        <div class="row">
          <span class="dim">humidity</span>
          <input type="number" min="0" max="200" step="5" bind:value={c.hmin} placeholder="min" title="Minimum humidity % (biome rainfall × 100; empty = no limit)" />
          <span class="dim">–</span>
          <input type="number" min="0" max="200" step="5" bind:value={c.hmax} placeholder="max" title="Maximum humidity %" />
          <span class="dim">%</span>
        </div>
        <div class="row">
          <span class="dim">temp</span>
          <input type="number" min="-1" max="3" step="0.05" bind:value={c.tmin} placeholder="min" title="Minimum biome temperature (e.g. 0.8; empty = no limit)" />
          <span class="dim">–</span>
          <input type="number" min="-1" max="3" step="0.05" bind:value={c.tmax} placeholder="max" title="Maximum biome temperature" />
        </div>
        <div class="dim tiny" title={cb.map((g) => g.name).join(', ')}>{cb.length ? `${cb.length} biome${cb.length === 1 ? '' : 's'}: ${cb.slice(0, 6).map((g) => g.name).join(', ')}${cb.length > 6 ? ', …' : ''}` : 'no biome matches'}</div>
      {:else if c.what === 'bee'}
        <div class="row">
          <select class="grow" bind:value={c.bee} title="Any biome whose Forestry climate (at Y ≤ 64) is this">
            {#each beeOptions as code (code)}<option value={code}>{beeName(code)}</option>{/each}
          </select>
        </div>
      {/if}
      {#if canFlat(c)}
        <div class="row">
          <label class="dim" title="Only a connected patch of this biome where most of the ground lies in one height band (zoomed-out terrain model, median 2 blocks off; reads ~5% flatter than the exact terrain on average)"><input type="checkbox" checked={isFlat(c)} onchange={(e) => { for (const k of ['ftol', 'fpct', 'farea', 'fland']) c[k] = fv(c, k); c.flat = e.currentTarget.checked; }} /> flat area</label>
          {#if isFlat(c)}
            <span class="dim">±</span><input type="number" min="0" max="128" step="1" bind:value={c.ftol} title="Height band: ground within ± this many blocks of the band centre" />
            <span class="dim">≥</span><input type="number" min="0" max="100" step="5" bind:value={c.fpct} title="Minimum share of the biome patch inside the band, %" /><span class="dim">%</span>
          {/if}
        </div>
        {#if isFlat(c)}
          <div class="row">
            <span class="dim">band area ≥</span><input type="number" min="0" step="32" bind:value={c.farea} title="Minimum in-band area as a square side (128 = 128×128 blocks); 0 = any" /><span class="dim">² blocks</span>
            <label class="dim" title="Water (ground below sea level) never counts as in the band"><input type="checkbox" checked={c.fland !== false && c.fland !== 'false'} onchange={(e) => (c.fland = e.currentTarget.checked)} /> land only</label>
          </div>
        {/if}
      {/if}
      <div class="row">
        <select bind:value={c.not}>
          <option value={false}>within</option>
          <option value={true}>not within</option>
        </select>
        <input type="number" min="0" step="100" bind:value={c.r} /> <span class="dim">blocks</span>
        {#if slow(c)}<span class="dim tiny" title="Checks the terrain model in every chunk in range, or many biome samples">slow</span>{/if}
      </div>
      {#if conds.length > 1 && (rel != null || parentOptions(i).length)}
        <div class="row">
          <span class="dim">from</span>
          <select class="grow" bind:value={c.from} title="Measure this condition from the search centre, or from each candidate of another condition (a relation: e.g. a village within 2000 of the centre that has a stronghold within 300 of it)">
            <option value="">the search centre</option>
            {#each parentOptions(i) as [o, k] (k)}<option value={String(k)}>each #{k + 1} {label(o)}</option>{/each}
          </select>
        </div>
      {/if}
    </div>
  {/each}
  <div class="row">
    <button class="add" onclick={addCond}>+ condition</button>
    <span class="grow"></span>
    <button class="small" onclick={shareSearch} title="Copy these conditions (dimension, centre, seed source) as a link / code">share</button>
    <button class="small" onclick={loadSearch} title="Load conditions from a link / code">load</button>
  </div>
  {#if shareNote}<div class="dim tiny">{shareNote}</div>{/if}
  {#if presetNote}<div class="dim tiny">{presetNote}</div>{/if}
  {#if worldType === 1 && !dim}
    <label class="row"><input type="checkbox" bind:checked={landAtSpawn} /> land at {centerLabel()}</label>
  {/if}
  <div class="row">
    <label>Grid <select bind:value={step}><option value={32}>32</option><option value={64}>64</option><option value={128}>128</option></select></label>
    <label>Seeds
      <select bind:value={source}>
        <option value="random">random</option>
        <option value="sequential">counting from</option>
        <option value="range">range</option>
        <option value="list">my list</option>
      </select>
    </label>
  </div>
  {#if source === 'sequential'}
    <div class="row"><input class="start" bind:value={startText} title="First seed; counts up" /></div>
  {:else if source === 'range'}
    <div class="row"><input class="start" bind:value={startText} title="From (inclusive)" /> <span class="dim">to</span> <input class="start" bind:value={rangeTo} title="To (inclusive)" /></div>
  {:else if source === 'list'}
    <textarea class="list" rows="4" bind:value={listText} placeholder="one seed per line (numbers, or text like the seed box)"></textarea>
  {/if}
  <div class="row">
    {#if running}
      <button class="go stop" onclick={stop}>Stop</button>
    {:else}
      <button class="go" onclick={start} disabled={!palette && !dim}>Search</button>
    {/if}
    <span class="dim stats">
      {checked.toLocaleString()}{total && Number.isFinite(total) ? ` / ${total.toLocaleString()}` : ''} checked{rate ? ` · ${rate < 10 ? rate.toFixed(1) : Math.round(rate)}/s` : ''} · {results.length} found
    </span>
  </div>
  {#if error}<div class="err">{error}</div>{/if}
  {#if resultsNote}<div class="dim tiny">{resultsNote}</div>{/if}
  {#if results.length}
    <div class="row resbar">
      <span class="seg">
        <button class="small" class:on={resView === 'list'} onclick={() => (resView = 'list')}>list</button>
        <button class="small" class:on={resView === 'table'} onclick={() => (resView = 'table')}>table</button>
      </span>
      <select class="small-sel" bind:value={sortKey} onchange={() => (sortDir = 1)} title="Sort the results">
        <option value="">found order</option>
        <option value="worst">nearest worst condition</option>
        <option value="sum">smallest total distance</option>
        {#each resCols as c}<option value={c}>nearest {c}</option>{/each}
        <option value="seed">seed</option>
      </select>
      {#if resView === 'table'}<button class="small" onclick={exportCsv} title="Copy the table as CSV">CSV</button>{/if}
    </div>
  {/if}
  {#if resView === 'table' && results.length}
    <div class="tablewrap">
      <table class="restable">
        <thead>
          <tr>
            <th onclick={() => sortBy('seed')}>seed{arrow('seed')}</th>
            {#each resCols as c, k}<th class="num" onclick={() => sortBy(c)} title="distance from the search centre to the nearest {c}">{c}{arrow(c)}</th>{/each}
            {#if resCols.length > 1}<th class="num" onclick={() => sortBy('worst')} title="the furthest of the conditions">worst{arrow('worst')}</th>{/if}
            {#if oncompare}<th></th>{/if}
          </tr>
        </thead>
        <tbody>
          {#each sortedRows as row (row.r.seed + ':' + row.r.dim)}
            <tr onclick={() => onpick(row.r)} title={(row.r.nots ?? []).join(' · ') || 'open this seed'}>
              <td class="seed">{row.r.seed}{row.r.dim ? ` · ${DIM_NAMES[row.r.dim]}` : ''}</td>
              {#each resCols as c}<td class="num">{row.d[c] ?? '–'}</td>{/each}
              {#if resCols.length > 1}<td class="num worst">{row.worst ?? '–'}</td>{/if}
              {#if oncompare}<td><button class="cmpbtn" onclick={(e) => { e.stopPropagation(); oncompare(row.r); }} title="Compare this seed side by side with the one on the map">◫</button></td>{/if}
            </tr>
          {/each}
        </tbody>
      </table>
    </div>
  {:else}
    <ul class="results">
      {#each sortedRows as { r } (r.seed + ':' + r.dim)}
        <li>
          <button onclick={() => onpick(r)}>
            <span class="seed">{r.seed}{r.dim ? ` · ${DIM_NAMES[r.dim]}` : ''}{r.center[0] || r.center[1] ? ` · from ${r.center[0]}, ${r.center[1]}` : ''}</span>
            <span class="dim hits">{[...r.hits.map((h) => `${h.label} ${h.dist}m${flatHit(h)}`), ...(r.nots ?? [])].join(' · ')}</span>
          </button>
          {#if oncompare}<button class="cmpbtn" onclick={() => oncompare(r)} title="Compare this seed side by side with the one on the map">◫</button>{/if}
        </li>
      {/each}
    </ul>
  {/if}

  <details class="lists">
    <summary>Saved lists {listNames.length ? `(${listNames.length})` : ''}</summary>
    <div class="row">
      <input class="grow" bind:value={listName} placeholder="name for the current results" />
      <button class="small" onclick={saveList} disabled={!results.length}>save</button>
      <button class="small" onclick={importList} title="Paste a list exported as JSON">import</button>
    </div>
    {#if listNames.length}
      <div class="row">
        <select class="grow" bind:value={pickA}>
          <option value="">— list A —</option>
          {#each listNames as n}<option value={n}>{n} ({lists[n].results?.length ?? 0})</option>{/each}
        </select>
        <button class="small" onclick={() => showList(pickA)} disabled={!pickA}>show</button>
        <button class="small" onclick={() => exportList(pickA)} disabled={!pickA} title="Copy as JSON">export</button>
        <button class="small" onclick={() => deleteList(pickA)} disabled={!pickA}>delete</button>
      </div>
      <div class="row">
        <select class="grow" bind:value={pickB}>
          <option value="">— list B (compare) —</option>
          {#each listNames as n}<option value={n}>{n} ({lists[n].results?.length ?? 0})</option>{/each}
        </select>
      </div>
      <div class="row">
        <button class="small" onclick={() => compare('both')} disabled={!pickA || !pickB}>seeds in both</button>
        <button class="small" onclick={() => compare('onlyA')} disabled={!pickA || !pickB}>only in A</button>
      </div>
    {/if}
  </details>

  <details class="lists help">
    <summary>How exact is the search?</summary>
  <p class="dim note">
    "not within" passes only if nothing of that kind is in range. Exact: End: HEE islands by biome / variation (centre; the island can be missing if the End unloaded while it was half explored), dungeon towers, Draconic comets by size (default config), all ~2k–40k seeds/s; End ore veins only where GT processes ores (main island zone, near HEE islands). abyssal geodes and their dark ores (~10 seeds/s), underground lakes (all RWG2 lakes are underground; ~30 seeds/s, rare filters slower), surface blocks (exact top block before trees and plants, with caves, lava caves and the population snow / ice; ~5–30 seeds/s, slower for bare stone and large radii), strongholds (portal room; "under ocean" = sea water above the frame column, before decoration), villages and their style, mineshafts, fortress blaze spawners, LootGames chunks, Roguelike
    areas (the dungeon lands 40–100 blocks from the area centre if the terrain allows), volcanoes, lava caves, Nether fortresses (start
    crossing) and Twilight Forest features. Small biomes are checked at every patch centre; other biomes on the grid (Nether / Twilight
    Forest: the game's 1:4 biome grid). "Next to": biome B is looked for on a grid of a quarter of the distance (4–grid blocks).
    Underground fluid fields are exact. Ore veins: candidates exact, the chosen vein right for ~87% (Nether / Twilight Forest: GT's stone
    test on the exact terrain, arrival near 0, 0: right ~92%). Thaumcraft: rolls exact, terrain checks estimated; obelisk conditions count only "likely" obelisks (≥50%; possible ones are not counted). Bee climate uses each biome's base climate (Y ≤ 64). Flat area (biome, bee climate and humidity conditions, overworld): the connected patch of those biomes (sampled every 32 blocks, followed up to 2048 past the radius; larger patches are measured cut off, marked +) must have the given share of its ground in one ±N band, on the zoomed-out terrain model (no caves or trees; reads ~5% flatter than the exact terrain on average, up to ~20% on small hilly patches); the hit is the nearest in-band spot. ~40 seeds/s native for a 3000 radius.
  </p>
  </details>
</div>

<style>
  .search { display: flex; flex-direction: column; gap: 6px; padding: 8px; overflow: auto; flex: 1; min-height: 0; }
  .search > * { flex-shrink: 0; }
  .row { display: flex; align-items: center; gap: 6px; flex-wrap: wrap; }
  .head { color: var(--fg); font-weight: 600; justify-content: space-between; }
  .small-n { font-weight: 400; }
  .from { font-weight: 400; }
  .cond { display: flex; flex-direction: column; gap: 4px; padding: 6px; border: 1px solid #2a323c; border-radius: 6px; }
  .what, .grow { flex: 1; min-width: 0; }
  .row > :global(.picker) { flex: 1; min-width: 0; }
  .row:has(> :global(.picker)) { align-items: flex-start; }
  input[type='number'] { width: 9ch; }
  .start { width: 14ch; }
  .list { width: 100%; box-sizing: border-box; font: inherit; font-size: 12px; background: #11161c; color: var(--fg); border: 1px solid #2a323c; border-radius: 4px; }
  .x { padding: 2px 8px; }
  .add { align-self: flex-start; }
  .go { background: #3a2f1c; border-color: #6b532a; font-weight: 600; }
  .stop { background: #3a1d1d; border-color: #7a3434; }
  .stats { font-variant-numeric: tabular-nums; }
  .err { color: #ff8a80; }
  .tiny { font-size: 11px; }
  .results { list-style: none; margin: 0; padding: 0; flex: none; overflow: visible; }
  .results button {
    width: 100%; text-align: left; display: flex; flex-direction: column; align-items: flex-start; gap: 1px; border: 0;
    background: transparent; padding: 4px 6px; border-radius: 4px;
  }
  .results button:hover { background: #222a33; }
  .seed { font-variant-numeric: tabular-nums; }
  .hits { font-size: 11px; }
  .cond.rel { margin-left: 14px; border-left: 2px solid #e8a33d; }
  .relhead { margin: -2px 0 2px; }
  .resbar { gap: 4px; }
  .seg { display: inline-flex; }
  .seg button { border-radius: 0; }
  .seg button:first-child { border-radius: 4px 0 0 4px; }
  .seg button:last-child { border-radius: 0 4px 4px 0; margin-left: -1px; }
  .seg button.on { background: #3a2f1c; border-color: #6b532a; }
  .small-sel { flex: 1; min-width: 0; font-size: 12px; }
  .tablewrap { overflow-x: auto; max-width: 100%; }
  .restable { border-collapse: collapse; font-size: 12px; width: 100%; font-variant-numeric: tabular-nums; }
  .restable th { position: sticky; top: 0; background: #12161b; text-align: left; font-weight: 500; color: #aab3c0; cursor: pointer; padding: 3px 6px; white-space: nowrap; border-bottom: 1px solid #2a323c; user-select: none; max-width: 9em; overflow: hidden; text-overflow: ellipsis; }
  .restable td { padding: 3px 6px; border-bottom: 1px solid #1e252e; white-space: nowrap; }
  .restable tbody tr { cursor: pointer; }
  .restable tbody tr:hover { background: #222a33; }
  .restable .num { text-align: right; }
  .restable .worst { color: #e8c88a; }
  .cmpbtn { width: auto !important; flex: none; padding: 1px 6px !important; font-size: 12px; background: transparent; border: 1px solid #2a323c !important; border-radius: 4px; }
  .cmpbtn:hover { border-color: #e8a33d !important; }
  .restable th:first-child, .restable td:first-child { position: sticky; left: 0; background: #12161b; z-index: 1; }
  .restable th:first-child { z-index: 2; }
  .restable tbody tr:hover td:first-child { background: #222a33; }
  .lists { border-top: 1px solid #2a323c; padding-top: 6px; display: flex; flex-direction: column; gap: 6px; }
  .lists summary { cursor: pointer; color: var(--fg); }
  .lists > .row { margin-top: 6px; }
  .note { font-size: 11px; margin: 4px 0 0; }
</style>
