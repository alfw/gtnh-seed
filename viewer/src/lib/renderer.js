// Canvas tile renderer. World units = blocks; zoom = log2(screen px per block) (0 → 1 px per block).
// A tile at sample step `s` holds TILE×TILE samples covering TILE*s blocks; level step = 2^max(0, floor(-zoom)).

import { LAVA_MARKER, FORESTRY_LUT } from './colors.js';

export const TILE = 256;
/** Blocks-mode decoration overlay: groups of DECO_N × DECO_N chunks, shown from zoom DECO_ZOOM. */
export const DECO_N = 4;
export const DECO_ZOOM = 1;
const DECO_CACHE = 600;
const CACHE_LIMIT = 900;
export const MIN_ZOOM = -9; // 512 blocks per px
export const MAX_ZOOM = 4; // 16 px per block

/** Marker kinds that never merge into groups (user markers, search hits, rare landmarks, footprints, ore dots). */
const UNGROUPED = new Set(['pin', 'hit', 'origin', 'spawn', 'stronghold', 'village', 'endIsland', 'deChaos', 'fortressPiece', 'rogueArea', 'fluid', 'ore']);
const GROUPABLE = (m) => !UNGROUPED.has(m.kind) && !m.selected;
/** Label priority 2 (after pins and hits). */
const LANDMARK = new Set(['spawn', 'stronghold', 'village', 'fortress', 'endIsland', 'endTower', 'deChaos', 'origin']);
/** Group badge colours per kind (the map icon's main colour). */
const KIND_COLOR = {
  loot: '#f9a825', rogue: '#e53935', mineshaft: '#8d6e63', tfFeature: '#2e7d32', fortress: '#ff7043', spawner: '#ffb300', fchest: '#c8a165', wart: '#ad1457',
  tcRing: '#7b1fa2', tcHilltop: '#9c4dcc', tcMound: '#9c4dcc', tcTotem: '#9c4dcc', node: '#ce93d8', geode: '#9575cd', lake: '#29b6f6', oil: '#5d4037', vdungeon: '#8fbf6a', minechest: '#c8a165',
  endTower: '#b39ddb', deComet: '#e53935', draconium: '#ef5350', ore: '#90a4ae',
};

/** Blocks mode (3) palette, ABGR by block code (shared by every renderer; `setBlockPalette`). */
const BLOCK_LUT = new Uint32Array(128).fill(0xffff00ff);
const WATER_RGB = [0x3f, 0x76, 0xe4];

/** `blocks`: wasm `blocks_json` (`[{id, rgb}]`). */
export function setBlockPalette(blocks) {
  for (const b of blocks ?? []) BLOCK_LUT[b.id & 127] = (255 << 24) | ((b.rgb & 255) << 16) | (b.rgb & 0xff00) | ((b.rgb >> 16) & 255);
}

/** Blocks-mode sample (block | under water << 7 | y << 8) → { block, wet, y }. */
export function blockSample(id) {
  return { block: id & 127, wet: !!(id & 128), y: id >> 8 };
}

/** Blocks-mode tile colours: block colour, hillshaded by the top y; under water the floor block seen through
 *  water, deeper = bluer. */
function blockPixels(ids, step, px) {
  const h = new Uint16Array(ids.length);
  for (let i = 0; i < ids.length; i++) h[i] = ids[i] & 128 ? 62 * 8 : (ids[i] >> 8) * 8;
  const shade = hillshade(h, step);
  for (let i = 0; i < ids.length; i++) {
    const id = ids[i], v = BLOCK_LUT[id & 127];
    let r = v & 255, g = (v >> 8) & 255, b = (v >> 16) & 255;
    if (id & 128) {
      const a = Math.min(0.92, 0.55 + (62 - (id >> 8)) * 0.025);
      r = r * (1 - a) + WATER_RGB[0] * a;
      g = g * (1 - a) + WATER_RGB[1] * a;
      b = b * (1 - a) + WATER_RGB[2] * a;
    } else {
      const k = shade[i];
      r = Math.min(255, r * k);
      g = Math.min(255, g * k);
      b = Math.min(255, b * k);
    }
    px[i] = (255 << 24) | ((b | 0) << 16) | ((g | 0) << 8) | (r | 0);
  }
}

export class Renderer {
  /** `ns`: pool namespace (a second renderer, e.g. the street-view minimap, must not prune the map's jobs).
   *  `mini`: minimap: marker icons without labels, no grid / origin / scale bar. */
  constructor(canvas, pool, { onStats, onTileTime, ns = null, mini = false } = {}) {
    this.ns = ns;
    this.mini = mini;
    this.canvas = canvas;
    this.ctx = canvas.getContext('2d');
    this.pool = pool;
    this.onStats = onStats;
    this.onTileTime = onTileTime;
    this.view = { x: 0, z: 0, zoom: -3 };
    this.world = null; // { seed: string, worldType: 0|1 }
    this.palette = null;
    this.highlight = null; // Set<number> | null
    this.flat = null; // flat-area patch overlay (setFlat)
    this.tiles = new Map(); // key → { ids, bmp, hl, s, tx, tz, used }
    this.inflight = new Set();
    this.dirty = true;
    this.statsTimer = 0;
    this.showGrid = true;
    this.markers = [];
    this.mode = 0; // 0 = biomes, 1 = Forestry bee climate, 2 = terrain height, 3 = surface blocks
    this.relief = false; // hillshade biome / bee maps with the height tiles
    this.decoOn = false; // Blocks mode: trees & plants from zoom DECO_ZOOM (overworld)
    this.decoPal = null; // Uint8Array r, g, b, kind per palette code
    this.decos = new Map(); // `${seed}|${type}|gx|gz` → { buf, bmp, used }
    this.decoInflight = new Set();
    // the spawn area replayed in the real load order (`spawnDeco` job): { key, cx0, cz0, n, buf } | 'pending'
    this.spawnDeco = null;
    this.raf = requestAnimationFrame(() => this.frame());
  }

  destroy() {
    cancelAnimationFrame(this.raf);
  }

  setWorld(seed, worldType, dim = 0) {
    const same = this.world && this.world.seed === seed && this.world.worldType === worldType && this.world.dim === dim;
    if (same) return;
    this.world = { seed, worldType, dim };
    this.tiles.clear();
    this.inflight.clear();
    this.pool.prune(() => false, this.ns);
    this.invalidate();
  }

  setMode(mode) {
    if (this.mode === mode) return;
    this.mode = mode;
    this.pool.prune(() => false, this.ns);
    this.inflight.clear();
    this.invalidate();
  }

  /** Terrain mode: biome colours over the height ramp at this opacity (0 = off). With a biome highlight only the
   *  highlighted biomes are overlaid. */
  setBiomeOverlay(a) {
    if (this.biomeOverlay === a) return;
    this.biomeOverlay = a;
    for (const t of this.tiles.values()) if (t.mode === 2) t.bmp = null;
    this.invalidate();
  }

  /** Colour table for another dimension's base biome ids (Uint32 ABGR, indexed by id). */
  setDimLut(lut) {
    this.dimLut = lut;
    for (const t of this.tiles.values()) t.bmp = null;
    this.invalidate();
  }

  /** Terrain colours changed (see `setTerrainPreset`): re-colour the height tiles. */
  recolourTerrain() {
    for (const t of this.tiles.values()) if (t.mode === 2) t.bmp = null;
    this.invalidate();
  }

  /** Blocks mode: trees, plants, boulders and canopy snow (wasm `deco_tops`) over the block tiles at zoom ≥ 1. */
  setDeco(on) {
    if (this.decoOn === on) return;
    this.decoOn = on;
    if (!on) this.pool.prune((k) => !k.startsWith('deco|'), this.ns);
    this.invalidate();
  }

  /** A queued decoration group still on screen (else pruned with the tiles). */
  decoWanted(k) {
    if (!this.decoOn || this.mode !== 3 || this.view.zoom < DECO_ZOOM || !this.world) return false;
    if (k.startsWith('spawn|')) return k.slice(6) === `${this.world.seed}|${this.world.worldType}` && this.world.dim === 0;
    const [seed, type, gx, gz] = k.split('|');
    if (seed !== String(this.world.seed) || type !== String(this.world.worldType)) return false;
    const { width, height } = this.cssSize(), span = 16 * DECO_N;
    const a = this.screenToWorld(-span * this.ppb, -span * this.ppb), b = this.screenToWorld(width + span * this.ppb, height + span * this.ppb);
    return gx * span <= b.x && (+gx + 1) * span >= a.x && gz * span <= b.z && (+gz + 1) * span >= a.z;
  }

  decoKey(gx, gz) {
    return `${this.world.seed}|${this.world.worldType}|${gx}|${gz}`;
  }

  /** Request and draw the decoration groups on screen (Blocks mode, overworld, zoom ≥ DECO_ZOOM). */
  drawDeco(width, height) {
    if (!this.decoOn || this.mini || this.mode !== 3 || this.world.dim !== 0 || this.view.zoom < DECO_ZOOM) return;
    if (!this.decoPal) {
      if (!this.decoInflight.has('pal')) {
        this.decoInflight.add('pal');
        this.pool
          .request({ type: 'decoPalette' }, { priority: 40, ns: this.ns })
          .then((r) => {
            this.decoPal = r.pal;
            this.invalidate();
          })
          .catch(() => this.decoInflight.delete('pal'));
      }
      return;
    }
    const span = 16 * DECO_N;
    const a = this.screenToWorld(0, 0), b = this.screenToWorld(width, height);
    const c = this.view, now = performance.now();
    this.requestSpawnDeco();
    for (let gz = Math.floor(a.z / span); gz <= Math.floor(b.z / span); gz++)
      for (let gx = Math.floor(a.x / span); gx <= Math.floor(b.x / span); gx++) {
        const k = this.decoKey(gx, gz);
        const d = this.decos.get(k);
        if (d) {
          d.used = now;
          const p = this.worldToScreen(gx * span, gz * span);
          const size = span * this.ppb;
          this.ctx.drawImage(this.decoBitmap(d), Math.floor(p.sx), Math.floor(p.sy), Math.ceil(size + 0.5), Math.ceil(size + 0.5));
          continue;
        }
        if (this.decoInflight.has(k)) continue;
        this.decoInflight.add(k);
        const { seed, worldType } = this.world;
        const dist = Math.hypot((gx + 0.5) * span - c.x, (gz + 0.5) * span - c.z) / span;
        this.pool
          .request({ type: 'decoTops', seed, worldType, cx: gx * DECO_N, cz: gz * DECO_N, n: DECO_N }, { priority: 30 + dist, key: 'deco|' + k, ns: this.ns })
          .then((r) => {
            this.decoInflight.delete(k);
            const d = { buf: r.buf, bmp: null, used: performance.now() };
            this.patchSpawnDeco(k, d);
            this.decos.set(k, d);
            if (this.decos.size > DECO_CACHE) {
              const e = [...this.decos.entries()].sort((x, y) => x[1].used - y[1].used);
              for (let i = 0; i < e.length - DECO_CACHE; i++) this.decos.delete(e[i][0]);
            }
            this.invalidate();
          })
          .catch(() => this.decoInflight.delete(k));
      }
  }

  /** Once per seed: the spawn area's decoration replayed in the fresh world's load order (wasm `spawn_deco_tops`),
   *  then laid over the 4×4-group tops (which replay each group on its own, z-major). */
  requestSpawnDeco() {
    const { seed, worldType } = this.world;
    const key = `${seed}|${worldType}`;
    if (this.spawnDeco && (this.spawnDeco === 'pending' ? this.spawnDecoKey === key : this.spawnDeco.key === key)) return;
    this.spawnDeco = 'pending';
    this.spawnDecoKey = key;
    this.pool
      .request({ type: 'spawnDeco', seed, worldType }, { priority: 29, key: 'deco|spawn|' + key, ns: this.ns })
      .then((r) => {
        if (this.spawnDecoKey !== key) return;
        this.spawnDeco = { key, cx0: r.cx0, cz0: r.cz0, n: r.n, buf: r.buf };
        for (const [k, d] of this.decos) if (this.patchSpawnDeco(k, d)) d.bmp = null;
        this.invalidate();
      })
      .catch(() => {
        if (this.spawnDecoKey === key) this.spawnDeco = null;
      });
  }

  /** Overwrites the columns of group `k` inside the spawn square with the ordered replay; true if any changed. */
  patchSpawnDeco(k, d) {
    const s = this.spawnDeco;
    if (!s || s === 'pending') return false;
    const [seed, type, gx, gz] = k.split('|');
    if (`${seed}|${type}` !== s.key) return false;
    const side = 16 * DECO_N, n = side * side, sw = 16 * s.n, sn = sw * sw;
    const ox = +gx * side - s.cx0 * 16, oz = +gz * side - s.cz0 * 16;
    if (ox >= sw || oz >= sw || ox + side <= 0 || oz + side <= 0) return false;
    for (let z = Math.max(0, -oz); z < Math.min(side, sw - oz); z++)
      for (let x = Math.max(0, -ox); x < Math.min(side, sw - ox); x++) {
        const i = z * side + x, j = (z + oz) * sw + (x + ox);
        d.buf[i] = s.buf[j];
        d.buf[n + i] = s.buf[sn + j];
      }
    return true;
  }

  /** Group bitmap: top decoration colour per column, lit from the north-west by its height steps; plants
   *  translucent, ground left transparent. */
  decoBitmap(d) {
    if (d.bmp) return d.bmp;
    const side = 16 * DECO_N, n = side * side, pal = this.decoPal, buf = d.buf;
    const c = new OffscreenCanvas(side, side);
    const g = c.getContext('2d');
    const img = g.createImageData(side, side);
    const px = new Uint32Array(img.data.buffer);
    const y = (i) => (buf[i] ? buf[n + i] : -1);
    for (let z = 0; z < side; z++)
      for (let x = 0; x < side; x++) {
        const i = z * side + x, code = buf[i];
        if (!code) continue;
        const kind = pal[code * 4 + 3];
        let k = 1;
        if (kind !== 3) {
          // step toward the light (north-west) lit, away from it shaded; canopy edges read as height
          const yy = buf[n + i];
          const yw = x > 0 ? y(i - 1) : yy, yn = z > 0 ? y(i - side) : yy;
          const ye = x < side - 1 ? y(i + 1) : yy, ys = z < side - 1 ? y(i + side) : yy;
          k += Math.max(-0.35, Math.min(0.3, (yy - (yw + yn) / 2) * 0.05));
          if (ye < 0 || ys < 0) k *= 0.82;
          if (yw < 0 || yn < 0) k *= 1.12;
        }
        const r = Math.min(255, pal[code * 4] * k), gg = Math.min(255, pal[code * 4 + 1] * k), bb = Math.min(255, pal[code * 4 + 2] * k);
        const a = kind === 3 ? 170 : 255;
        px[i] = (a << 24) | ((bb | 0) << 16) | ((gg | 0) << 8) | (r | 0);
      }
    g.putImageData(img, 0, 0);
    d.bmp = c;
    return c;
  }

  setRelief(on) {
    if (this.relief === on) return;
    this.relief = on;
    for (const t of this.tiles.values()) t.bmp = null;
    this.invalidate();
  }

  setPalette(p) {
    this.palette = p;
    for (const t of this.tiles.values()) t.bmp = null;
    this.invalidate();
  }

  setHighlight(set) {
    this.highlight = set && set.size ? set : null;
    for (const t of this.tiles.values()) t.bmp = null;
    this.invalidate();
  }

  /** Flat-area patch of a picked search hit: `{ cells: [i, j, top, …], step, lo, hi, land }` (tops = top block Y;
   * in band = lo ≤ top ≤ hi, and ≥ 63 if land) or null. Drawn under the markers. */
  setFlat(f) {
    this.flat = f;
    this.invalidate();
  }

  drawFlat(width, height) {
    const f = this.flat;
    if (!f || !f.cells?.length) return;
    const ctx = this.ctx;
    const s = f.step;
    const px = s * this.ppb;
    const half = s / 2;
    const c = f.cells;
    if (!f.keys) {
      f.keys = new Set();
      for (let k = 0; k < c.length; k += 3) f.keys.add(c[k] * 65536 + c[k + 1]);
    }
    // light veil = in the band, dark veil = rest of the patch: readable over any biome colour
    for (let pass = 0; pass < 2; pass++) {
      ctx.fillStyle = pass ? 'rgba(255,255,255,0.45)' : 'rgba(0,0,0,0.5)';
      for (let k = 0; k < c.length; k += 3) {
        const t = c[k + 2];
        const ok = t >= f.lo && t <= f.hi && (!f.land || t >= 63);
        if (ok !== !!pass) continue;
        const p = this.worldToScreen(c[k] * s - half, c[k + 1] * s - half);
        if (p.sx > width || p.sy > height || p.sx + px < 0 || p.sy + px < 0) continue;
        ctx.fillRect(p.sx, p.sy, Math.max(1, px), Math.max(1, px));
      }
    }
    // patch outline: cell edges without a patch neighbour
    ctx.beginPath();
    for (let k = 0; k < c.length; k += 3) {
      const i = c[k], j = c[k + 1];
      const p = this.worldToScreen(i * s - half, j * s - half);
      if (p.sx > width + px || p.sy > height + px || p.sx + 2 * px < 0 || p.sy + 2 * px < 0) continue;
      const has = (di, dj) => f.keys.has((i + di) * 65536 + (j + dj));
      if (!has(0, -1)) { ctx.moveTo(p.sx, p.sy); ctx.lineTo(p.sx + px, p.sy); }
      if (!has(0, 1)) { ctx.moveTo(p.sx, p.sy + px); ctx.lineTo(p.sx + px, p.sy + px); }
      if (!has(-1, 0)) { ctx.moveTo(p.sx, p.sy); ctx.lineTo(p.sx, p.sy + px); }
      if (!has(1, 0)) { ctx.moveTo(p.sx + px, p.sy); ctx.lineTo(p.sx + px, p.sy + px); }
    }
    ctx.lineWidth = 2;
    ctx.strokeStyle = '#ffe08a';
    ctx.stroke();
  }

  setTools(t) {
    this.tools = t;
    this.invalidate();
  }

  /** Marker grouping: `on`, `px` = merge distance on screen. */
  setGrouping(on, px = 32) {
    this.groupOn = on;
    this.groupPx = px;
    this.invalidate();
  }
  /** The drawn marker (or group, kind 'group') under a screen point, or null. */
  pickAt(sx, sy, radius = 11) {
    let best = null, bestD = radius;
    for (const d of this.drawn ?? []) {
      if (d.m.kind === 'fortressPiece') continue;
      const r = d.m.kind === 'group' ? Math.max(radius, d.r + 2) : radius;
      const dist = Math.hypot(d.sx - sx, d.sy - sy) - (d.m.kind === 'pin' ? 3 : 0);
      if (dist < r && (best == null || dist < bestD)) (best = d.m), (bestD = dist);
    }
    return best;
  }
  setMarkers(list) {
    this.markers = list;
    this.invalidate();
  }

  setView(v) {
    this.view = { ...this.view, ...v };
    this.view.zoom = Math.max(MIN_ZOOM, Math.min(MAX_ZOOM, this.view.zoom));
    this.invalidate();
  }

  invalidate() {
    this.dirty = true;
  }

  get ppb() {
    return 2 ** this.view.zoom;
  }

  levelStep() {
    return 2 ** Math.max(0, Math.floor(-this.view.zoom + 1e-9));
  }

  screenToWorld(sx, sy) {
    const { width, height } = this.cssSize();
    return { x: this.view.x + (sx - width / 2) / this.ppb, z: this.view.z + (sy - height / 2) / this.ppb };
  }

  worldToScreen(x, z) {
    const { width, height } = this.cssSize();
    return { sx: (x - this.view.x) * this.ppb + width / 2, sy: (z - this.view.z) * this.ppb + height / 2 };
  }

  cssSize() {
    return { width: this.canvas.clientWidth, height: this.canvas.clientHeight };
  }

  key(s, tx, tz, mode = this.mode) {
    return `${this.world.seed}|${this.world.worldType}|${this.world.dim}|${mode}|${s}|${tx}|${tz}`;
  }

  /** Sample id at a world position from the finest cached tile (no worker round-trip). */
  idAt(x, z) {
    if (!this.world) return null;
    for (let s = 1; s <= 2 ** -MIN_ZOOM; s *= 2) {
      const span = TILE * s;
      const tx = Math.floor(x / span), tz = Math.floor(z / span);
      const t = this.tiles.get(this.key(s, tx, tz));
      if (!t) continue;
      const i = Math.floor((x - tx * span) / s), j = Math.floor((z - tz * span) / s);
      return t.ids[j * TILE + i];
    }
    return null;
  }

  visibleTiles(s) {
    const { width, height } = this.cssSize();
    const a = this.screenToWorld(0, 0), b = this.screenToWorld(width, height);
    const span = TILE * s;
    const out = [];
    for (let tz = Math.floor(a.z / span); tz <= Math.floor(b.z / span); tz++)
      for (let tx = Math.floor(a.x / span); tx <= Math.floor(b.x / span); tx++) out.push([tx, tz]);
    return out;
  }

  request(s, tx, tz, priority, mode = this.mode) {
    const k = this.key(s, tx, tz, mode);
    if (this.tiles.has(k) || this.inflight.has(k)) return;
    this.inflight.add(k);
    const { seed, worldType, dim } = this.world;
    this.pool
      .request({ type: 'tile', seed, worldType, dim, mode, x0: tx * TILE * s, z0: tz * TILE * s, w: TILE, h: TILE, step: s }, { priority, key: k, ns: this.ns })
      .then((r) => {
        this.inflight.delete(k);
        if (!this.world || !k.startsWith(`${this.world.seed}|${this.world.worldType}|${this.world.dim}|`)) return;
        this.tiles.set(k, { ids: r.ids, bmp: null, s, tx, tz, mode, used: performance.now() });
        if (mode === 2) {
          // a height tile arrived: re-shade the colour tile it belongs to
          const c = this.tiles.get(this.key(s, tx, tz));
          if (c && c !== this.tiles.get(k)) c.bmp = null;
        } else if (mode === 0 && this.mode === 2) {
          // a biome tile arrived for the terrain overlay
          const h = this.tiles.get(this.key(s, tx, tz, 2));
          if (h) h.bmp = null;
        }
        this.onTileTime?.(r.ms);
        this.evict();
        this.invalidate();
        this.scheduleStats();
      })
      .catch(() => this.inflight.delete(k));
  }

  evict() {
    if (this.tiles.size <= CACHE_LIMIT) return;
    const entries = [...this.tiles.entries()].sort((a, b) => a[1].used - b[1].used);
    for (let i = 0; i < entries.length - CACHE_LIMIT; i++) this.tiles.delete(entries[i][0]);
  }

  bitmap(t) {
    if (t.bmp) return t.bmp;
    const c = new OffscreenCanvas(TILE, TILE);
    const g = c.getContext('2d');
    const img = g.createImageData(TILE, TILE);
    const px = new Uint32Array(img.data.buffer);
    if (t.mode === 3) {
      blockPixels(t.ids, t.s, px);
      g.putImageData(img, 0, 0);
      t.bmp = c;
      return c;
    }
    const lut = this.world.dim ? this.dimLut ?? this.palette.lut : t.mode === 1 ? FORESTRY_LUT : this.palette.lut;
    const hl = t.mode === 2 ? null : this.highlight;
    const ht = t.mode === 2 ? t : this.relief ? this.tiles.get(this.key(t.s, t.tx, t.tz, 2)) : null;
    const shade = ht ? hillshade(ht.ids, t.s) : null;
    const oa = t.mode === 2 ? this.biomeOverlay || 0 : 0;
    const bt = oa > 0 ? this.tiles.get(this.key(t.s, t.tx, t.tz, 0)) : null;
    const blut = this.palette.lut, bhl = this.highlight;
    for (let i = 0; i < px.length; i++) {
      const id = t.ids[i];
      let v = t.mode === 2 ? terrainColor(id) : lut[id];
      if (bt) {
        const bid = bt.ids[i];
        const a = bhl ? (bhl.has(bid) ? Math.max(oa, 0.85) : 0) : oa;
        if (a > 0) {
          const c = blut[bid];
          const mix = (sh) => (((v >> sh) & 255) * (1 - a) + ((c >> sh) & 255) * a) | 0;
          v = (255 << 24) | (mix(16) << 16) | (mix(8) << 8) | mix(0);
        }
      }
      if (hl && hl.has(id)) {
        // lift highlighted biomes toward white so dark colours (e.g. Tainted Land) stand out on the dimmed map
        const r = v & 255, gg = (v >> 8) & 255, bb = (v >> 16) & 255;
        v = (255 << 24) | (((bb + 255) >> 1) << 16) | (((gg + 255) >> 1) << 8) | ((r + 255) >> 1);
      } else if (hl) {
        // dim hard so highlighted biomes pop even as single pixels when zoomed out
        const r = v & 255, gg = (v >> 8) & 255, b = (v >> 16) & 255;
        const grey = (r * 0.3 + gg * 0.59 + b * 0.11) * 0.16 + 14;
        v = (255 << 24) | ((grey | 0) << 16) | ((grey | 0) << 8) | (grey | 0);
      }
      if (shade) {
        const k = shade[i];
        const r = Math.min(255, (v & 255) * k), gg = Math.min(255, ((v >> 8) & 255) * k), b = Math.min(255, ((v >> 16) & 255) * k);
        v = (255 << 24) | ((b | 0) << 16) | ((gg | 0) << 8) | (r | 0);
      }
      px[i] = v;
    }
    g.putImageData(img, 0, 0);
    t.bmp = c;
    return c;
  }

  frame() {
    this.raf = requestAnimationFrame(() => this.frame());
    if (!this.dirty || !this.world || !this.palette) return;
    this.dirty = false;
    this.draw();
  }

  draw() {
    const dpr = window.devicePixelRatio || 1;
    const { width, height } = this.cssSize();
    if (this.canvas.width !== Math.round(width * dpr) || this.canvas.height !== Math.round(height * dpr)) {
      this.canvas.width = Math.round(width * dpr);
      this.canvas.height = Math.round(height * dpr);
    }
    const ctx = this.ctx;
    ctx.setTransform(dpr, 0, 0, dpr, 0, 0);
    ctx.imageSmoothingEnabled = false;
    ctx.fillStyle = '#101418';
    ctx.fillRect(0, 0, width, height);

    const s = this.levelStep();
    const now = performance.now();
    const center = this.view;
    const tiles = this.visibleTiles(s);
    // queue missing tiles, nearest to the view centre first; drop queued tiles of other levels/positions
    const wanted = new Set(tiles.flatMap(([tx, tz]) => [this.key(s, tx, tz), this.key(s, tx, tz, 2), this.key(s, tx, tz, 0)]));
    this.pool.prune((k) => wanted.has(k) || (k.startsWith('deco|') && this.decoWanted(k.slice(5))), this.ns);
    for (const [tx, tz] of tiles) {
      const span = TILE * s;
      const d = Math.hypot((tx + 0.5) * span - center.x, (tz + 0.5) * span - center.z) / span;
      this.request(s, tx, tz, d);
      if (this.relief && this.mode !== 2 && this.mode !== 3) this.request(s, tx, tz, d + 0.5, 2);
      if (this.mode === 2 && this.biomeOverlay > 0) this.request(s, tx, tz, d + 0.5, 0);
    }
    // draw: coarser cached levels first as placeholders, then this level
    for (let cs = s * 16; cs >= s; cs /= 2) {
      const span = TILE * cs;
      for (const [tx, tz] of this.visibleTiles(cs)) {
        if (cs !== s) {
          // coarser cached tile as a placeholder; finer tiles are drawn over it
          const t = this.tiles.get(this.key(cs, tx, tz));
          if (!t) continue;
          this.blit(t, span);
          continue;
        }
        const t = this.tiles.get(this.key(cs, tx, tz));
        if (t) {
          t.used = now;
          this.blit(t, span);
        }
      }
    }
    this.drawDeco(width, height);
    this.drawFlat(width, height);
    this.overlays(width, height);
    this.drawTools();
  }

  /** Map tools on top of everything: measure path (with leg lengths) and radius circle (with resize handle). */
  drawTools() {
    const t = this.tools;
    if (!t) return;
    const ctx = this.ctx;
    const at = (p) => this.worldToScreen(p.x + 0.5, p.z + 0.5);
    const fmt = (n) => Math.round(n).toLocaleString('en-US');
    const label = (text, x, y) => {
      ctx.font = '600 12px system-ui, sans-serif';
      const w = ctx.measureText(text).width;
      ctx.fillStyle = 'rgba(16,20,24,0.88)';
      ctx.fillRect(x - w / 2 - 5, y - 9, w + 10, 18);
      ctx.fillStyle = '#ffe08a';
      ctx.textAlign = 'center';
      ctx.textBaseline = 'middle';
      ctx.fillText(text, x, y);
      ctx.textAlign = 'start';
      ctx.textBaseline = 'alphabetic';
    };
    const c = t.radius;
    if (c) {
      const p = at(c);
      const rr = c.r * this.ppb;
      ctx.beginPath();
      ctx.arc(p.sx, p.sy, rr, 0, Math.PI * 2);
      ctx.fillStyle = 'rgba(255,224,138,0.08)';
      ctx.fill();
      ctx.setLineDash([8, 6]);
      ctx.lineWidth = 2;
      ctx.strokeStyle = 'rgba(0,0,0,0.6)';
      ctx.stroke();
      ctx.lineDashOffset = 7;
      ctx.strokeStyle = '#ffe08a';
      ctx.stroke();
      ctx.setLineDash([]);
      ctx.lineDashOffset = 0;
      // centre and the radius spoke with its handle
      ctx.beginPath();
      ctx.moveTo(p.sx, p.sy);
      ctx.lineTo(p.sx + rr, p.sy);
      ctx.strokeStyle = 'rgba(255,224,138,0.7)';
      ctx.lineWidth = 1.5;
      ctx.stroke();
      for (const [x, y, r] of [[p.sx, p.sy, 4], [p.sx + rr, p.sy, t.active === 'radius' ? 7 : 4]]) {
        ctx.beginPath();
        ctx.arc(x, y, r, 0, Math.PI * 2);
        ctx.fillStyle = '#ffe08a';
        ctx.fill();
        ctx.lineWidth = 2;
        ctx.strokeStyle = '#101418';
        ctx.stroke();
      }
      label(`r ${fmt(c.r)}${t.otherScale ? ` (${fmt(c.r * t.otherScale)})` : ''}`, p.sx + rr / 2, p.sy - 14);
    }
    const pts = t.measure ?? [];
    if (pts.length) {
      const sp = pts.map(at);
      ctx.beginPath();
      sp.forEach((q, i) => (i ? ctx.lineTo(q.sx, q.sy) : ctx.moveTo(q.sx, q.sy)));
      ctx.lineWidth = 4;
      ctx.strokeStyle = 'rgba(0,0,0,0.6)';
      ctx.stroke();
      ctx.lineWidth = 2;
      ctx.strokeStyle = '#ffe08a';
      ctx.stroke();
      sp.forEach((q, i) => {
        ctx.beginPath();
        ctx.arc(q.sx, q.sy, i === 0 || i === sp.length - 1 ? 5 : 3.5, 0, Math.PI * 2);
        ctx.fillStyle = i === 0 ? '#7ee787' : '#ffe08a';
        ctx.fill();
        ctx.lineWidth = 2;
        ctx.strokeStyle = '#101418';
        ctx.stroke();
      });
      let total = 0;
      for (let i = 1; i < pts.length; i++) {
        const d = Math.hypot(pts[i].x - pts[i - 1].x, pts[i].z - pts[i - 1].z);
        total += d;
        const len = Math.hypot(sp[i].sx - sp[i - 1].sx, sp[i].sy - sp[i - 1].sy);
        if (len > 60) label(fmt(d), (sp[i].sx + sp[i - 1].sx) / 2, (sp[i].sy + sp[i - 1].sy) / 2 - 12);
      }
      if (pts.length > 1) {
        const q = sp[sp.length - 1];
        label(`${fmt(total)} blocks${t.otherScale ? ` · ${fmt(total * t.otherScale)} ${t.otherScale > 1 ? 'overworld' : 'Nether'}` : ''}`, q.sx, q.sy + 18);
      }
    }
    // street view: where the person stood, with a view cone in the heading (0 = north = -z)
    const v = t.viewer;
    if (v) {
      const p = at(v);
      const b = (v.heading * Math.PI) / 180;
      const dx = Math.sin(b), dy = -Math.cos(b);
      ctx.beginPath();
      ctx.moveTo(p.sx, p.sy);
      ctx.arc(p.sx, p.sy, 34, Math.atan2(dy, dx) - 0.55, Math.atan2(dy, dx) + 0.55);
      ctx.closePath();
      ctx.fillStyle = 'rgba(255,224,138,0.28)';
      ctx.fill();
      ctx.beginPath();
      ctx.moveTo(p.sx + dx * 22, p.sy + dy * 22);
      ctx.lineTo(p.sx + dx * 8 - dy * 6, p.sy + dy * 8 + dx * 6);
      ctx.lineTo(p.sx + dx * 8 + dy * 6, p.sy + dy * 8 - dx * 6);
      ctx.closePath();
      ctx.fillStyle = '#ffe08a';
      ctx.fill();
      ctx.beginPath();
      ctx.arc(p.sx, p.sy, 6, 0, Math.PI * 2);
      ctx.fillStyle = '#ffe08a';
      ctx.fill();
      ctx.lineWidth = 2;
      ctx.strokeStyle = 'rgba(0,0,0,0.7)';
      ctx.stroke();
    }
  }

  blit(t, span) {
    const p = this.worldToScreen(t.tx * span, t.tz * span);
    const size = span * this.ppb;
    // +0.5 px overlap hides seams from fractional positions
    this.ctx.drawImage(this.bitmap(t), Math.floor(p.sx), Math.floor(p.sy), Math.ceil(size + 0.5), Math.ceil(size + 0.5));
  }

  overlays(width, height) {
    const ctx = this.ctx;
    // grid: chunk (16) / region (512) / 10k lines depending on zoom
    if (this.showGrid) {
      const spacing = [16, 512, 2048, 10000, 50000].find((g) => g * this.ppb >= 64) ?? 50000;
      const a = this.screenToWorld(0, 0), b = this.screenToWorld(width, height);
      ctx.strokeStyle = 'rgba(0,0,0,0.18)';
      ctx.lineWidth = 1;
      ctx.beginPath();
      for (let x = Math.ceil(a.x / spacing) * spacing; x <= b.x; x += spacing) {
        const sx = Math.round(this.worldToScreen(x, 0).sx) + 0.5;
        ctx.moveTo(sx, 0);
        ctx.lineTo(sx, height);
      }
      for (let z = Math.ceil(a.z / spacing) * spacing; z <= b.z; z += spacing) {
        const sy = Math.round(this.worldToScreen(0, z).sy) + 0.5;
        ctx.moveTo(0, sy);
        ctx.lineTo(width, sy);
      }
      ctx.stroke();
    }
    // origin, pins (user clicks) and search hits
    const all = this.mini ? this.markers : [{ x: 0, z: 0, label: '0, 0', kind: 'origin' }, ...this.markers];
    ctx.font = '12px system-ui, sans-serif';
    // screen positions of the visible markers, then grouping of nearby feature markers into count badges
    const vis = [];
    for (const m of all) {
      const { sx, sy } = this.worldToScreen(m.x + 0.5, m.z + 0.5);
      const mg = m.box ? Math.max(20, (m.box.x1 - m.box.x0) * this.ppb) : 20; // footprints reach past their centre
      if (sx < -mg || sy < -mg || sx > width + mg || sy > height + mg) continue;
      vis.push({ m, sx, sy });
    }
    const pts = [];
    const groups = [];
    if (this.groupOn !== false) {
      const G = this.groupPx ?? 32;
      const cells = new Map();
      const cand = [];
      for (const v of vis) (GROUPABLE(v.m) ? cand : pts).push(v);
      // stable under panning: fixed world order, anchors hashed in screen cells of G
      cand.sort((a, b) => a.m.x - b.m.x || a.m.z - b.m.z);
      for (const v of cand) {
        const cx = Math.floor(v.sx / G), cy = Math.floor(v.sy / G);
        let g = null, gd = G;
        for (let dy = -1; dy <= 1; dy++)
          for (let dx = -1; dx <= 1; dx++)
            for (const q of cells.get((cx + dx) * 65536 + (cy + dy)) ?? []) {
              const d = Math.hypot(q.ax - v.sx, q.ay - v.sy);
              if (d < gd) (g = q), (gd = d);
            }
        if (!g) {
          g = { ax: v.sx, ay: v.sy, list: [] };
          const k = cx * 65536 + cy;
          if (!cells.has(k)) cells.set(k, []);
          cells.get(k).push(g);
          groups.push(g);
        }
        g.list.push(v);
      }
      for (const g of groups) if (g.list.length === 1) pts.push(g.list[0]);
    } else pts.push(...vis);
    this.drawn = [];
    const labels = []; // drawn after all icons, nearest-to-centre first, skipping overlaps
    const cxs = width / 2, cys = height / 2;
    for (const { m, sx, sy } of pts) {
      this.drawn.push({ m, sx, sy });
      if (m.kind === 'pin') {
        // user marker: dark disc with a coloured ring and the type icon; ghosts (where an other-dimension portal
        // lands) are faded with a dashed ring and a dashed line to the portal they link to
        const r = m.selected ? 11 : 9;
        if (m.ghost && m.linkTo) {
          const q = this.worldToScreen(m.linkTo.x + 0.5, m.linkTo.z + 0.5);
          ctx.setLineDash([5, 4]);
          ctx.lineWidth = 2;
          ctx.strokeStyle = m.color ?? '#e8a33d';
          ctx.beginPath();
          ctx.moveTo(sx, sy);
          ctx.lineTo(q.sx, q.sy);
          ctx.stroke();
          ctx.setLineDash([]);
        }
        ctx.globalAlpha = m.ghost ? 0.55 : 1;
        ctx.beginPath();
        ctx.arc(sx, sy, r, 0, Math.PI * 2);
        ctx.fillStyle = 'rgba(16,20,24,0.92)';
        ctx.fill();
        if (m.ghost) ctx.setLineDash([3, 3]);
        ctx.lineWidth = m.selected ? 3 : 2;
        ctx.strokeStyle = m.selected ? '#fff' : m.color ?? '#e8a33d';
        ctx.stroke();
        ctx.setLineDash([]);
        if (m.selected) {
          ctx.beginPath();
          ctx.arc(sx, sy, r + 2.5, 0, Math.PI * 2);
          ctx.lineWidth = 1.5;
          ctx.strokeStyle = m.color ?? '#e8a33d';
          ctx.stroke();
        }
        ctx.font = `${m.selected ? 12 : 11}px system-ui, "Apple Color Emoji", "Segoe UI Emoji", "Noto Color Emoji", sans-serif`;
        ctx.textAlign = 'center';
        ctx.textBaseline = 'middle';
        ctx.fillStyle = '#fff';
        ctx.fillText(m.icon ?? '📍', sx, sy + 1);
        if (m.global) {
          ctx.font = '9px system-ui, "Apple Color Emoji", "Segoe UI Emoji", "Noto Color Emoji", sans-serif';
          ctx.fillText('🌐', sx + r - 1, sy - r + 1);
        }
        ctx.textAlign = 'start';
        ctx.textBaseline = 'alphabetic';
        ctx.font = '12px system-ui, sans-serif';
        ctx.globalAlpha = 1;
      } else if (m.kind === 'endIsland') {
        // HEE biome island: footprint square (208 blocks) when it is big enough on screen, else a disc
        const col = ['#7cb342', '#e64a19', '#8e24aa'][m.ibiome] ?? '#ccc';
        const half = 104 * this.ppb;
        if (half > 7 && this.view.zoom < -3) {
          ctx.globalAlpha = 0.22;
          ctx.fillStyle = col;
          ctx.fillRect(sx - half, sy - half, half * 2, half * 2);
          ctx.globalAlpha = 1;
          ctx.lineWidth = 1.5;
          ctx.strokeStyle = col;
          ctx.strokeRect(sx - half, sy - half, half * 2, half * 2);
        }
        ctx.beginPath();
        ctx.arc(sx, sy, 6, 0, Math.PI * 2);
        ctx.fillStyle = col;
        ctx.fill();
        ctx.lineWidth = 2;
        ctx.strokeStyle = '#1a1424';
        ctx.stroke();
      } else if (m.kind === 'deComet') {
        // Ender comet: trail line toward the tail, red-rimmed dark core
        if (m.tail) {
          const t = this.worldToScreen(m.tail[0], m.tail[1]);
          ctx.strokeStyle = 'rgba(239,154,154,0.6)';
          ctx.lineWidth = 2;
          ctx.beginPath();
          ctx.moveTo(sx, sy);
          ctx.lineTo(t.sx, t.sy);
          ctx.stroke();
        }
        ctx.beginPath();
        ctx.arc(sx, sy, 5, 0, Math.PI * 2);
        ctx.fillStyle = '#2a1830';
        ctx.fill();
        ctx.lineWidth = 2;
        ctx.strokeStyle = '#e53935';
        ctx.stroke();
      } else if (m.kind === 'deChaos') {
        ctx.beginPath();
        ctx.arc(sx, sy, 8, 0, Math.PI * 2);
        ctx.fillStyle = '#4a0e0e';
        ctx.fill();
        ctx.lineWidth = 3;
        ctx.strokeStyle = '#ff5252';
        ctx.stroke();
      } else if (m.kind === 'draconium') {
        // draconium ore chunk: small red diamond
        ctx.beginPath();
        ctx.moveTo(sx, sy - 4);
        ctx.lineTo(sx + 4, sy);
        ctx.lineTo(sx, sy + 4);
        ctx.lineTo(sx - 4, sy);
        ctx.closePath();
        ctx.fillStyle = m.exact ? '#e53935' : '#8d6e63';
        ctx.fill();
        ctx.lineWidth = 1;
        ctx.strokeStyle = '#1a1424';
        ctx.stroke();
      } else if (m.kind === 'endTower') {
        // dungeon tower: dark obsidian square with a purple rim
        ctx.fillStyle = '#1e1530';
        ctx.fillRect(sx - 4, sy - 7, 8, 14);
        ctx.lineWidth = 1.5;
        ctx.strokeStyle = '#b39ddb';
        ctx.strokeRect(sx - 4, sy - 7, 8, 14);
      } else if (m.kind === 'minechest') {
        // chest cart: brown chest with a dark band, gold dot if it holds something rare
        ctx.fillStyle = '#8d6e3f';
        ctx.fillRect(sx - 5, sy - 4, 10, 8);
        ctx.lineWidth = 1.5;
        ctx.strokeStyle = '#2b1d0e';
        ctx.strokeRect(sx - 5, sy - 4, 10, 8);
        ctx.beginPath();
        ctx.moveTo(sx - 5, sy - 1);
        ctx.lineTo(sx + 5, sy - 1);
        ctx.stroke();
        if (m.rich) {
          ctx.beginPath();
          ctx.arc(sx, sy + 1.5, 2, 0, Math.PI * 2);
          ctx.fillStyle = '#ffd54f';
          ctx.fill();
        }
      } else if (m.kind === 'vdungeon') {
        // monster room: mossy square, gold dot if a chest holds something rare
        ctx.fillStyle = '#3c4a35';
        ctx.fillRect(sx - 5, sy - 5, 10, 10);
        ctx.lineWidth = 2;
        ctx.strokeStyle = '#8fbf6a';
        ctx.strokeRect(sx - 5, sy - 5, 10, 10);
        if (m.rich) {
          ctx.beginPath();
          ctx.arc(sx, sy, 2, 0, Math.PI * 2);
          ctx.fillStyle = '#ffd54f';
          ctx.fill();
        }
      } else if (m.kind === 'lake') {
        // underground lake: rounded drop, orange (lava) / blue (water); dashed rim = sealed in rock
        ctx.beginPath();
        ctx.ellipse(sx, sy, 6, 4, 0, 0, Math.PI * 2);
        ctx.fillStyle = m.lava ? '#ff6d00' : '#29b6f6';
        ctx.fill();
        ctx.lineWidth = 1.5;
        ctx.strokeStyle = '#1b1b1b';
        ctx.stroke();
      } else if (m.kind === 'fchest') {
        // fortress chest: brown box with a lid line, gold dot if it holds a diamond / diamond horse armor
        ctx.fillStyle = '#7a5230';
        ctx.fillRect(sx - 5, sy - 4, 10, 8);
        ctx.lineWidth = 1.5;
        ctx.strokeStyle = '#1b1b1b';
        ctx.strokeRect(sx - 5, sy - 4, 10, 8);
        ctx.beginPath();
        ctx.moveTo(sx - 5, sy - 1);
        ctx.lineTo(sx + 5, sy - 1);
        ctx.stroke();
        if (m.rich) {
          ctx.beginPath();
          ctx.arc(sx, sy + 1, 2, 0, Math.PI * 2);
          ctx.fillStyle = '#4dd0e1';
          ctx.fill();
        }
      } else if (m.kind === 'oil') {
        // oil deposit: black drop, amber rim; opacity by confidence, ring for a fountain
        ctx.globalAlpha = 0.55 + 0.45 * Math.min(1, m.conf ?? 0.5);
        ctx.beginPath();
        ctx.moveTo(sx, sy - 7);
        ctx.quadraticCurveTo(sx + 6, sy + 1, sx, sy + 5);
        ctx.quadraticCurveTo(sx - 6, sy + 1, sx, sy - 7);
        ctx.fillStyle = '#1b1410';
        ctx.fill();
        ctx.lineWidth = 1.5;
        ctx.strokeStyle = '#ffb300';
        ctx.stroke();
        if (m.size !== 'small') {
          ctx.beginPath();
          ctx.arc(sx, sy, 9, 0, Math.PI * 2);
          ctx.strokeStyle = 'rgba(255,179,0,0.6)';
          ctx.stroke();
        }
        ctx.globalAlpha = 1;
      } else if (m.kind === 'geode') {
        // abyssal geode: dark disc, purple rim; gold dot if it holds dark diamond
        ctx.beginPath();
        ctx.arc(sx, sy, 5.5, 0, Math.PI * 2);
        ctx.fillStyle = '#2b2838';
        ctx.fill();
        ctx.lineWidth = 2;
        ctx.strokeStyle = '#9575cd';
        ctx.stroke();
        if (m.rich) {
          ctx.beginPath();
          ctx.arc(sx, sy, 2, 0, Math.PI * 2);
          ctx.fillStyle = '#80deea';
          ctx.fill();
        }
      } else if (m.kind === 'node') {
        // aura node: small diamond, colour by node type
        const col = { Normal: '#80deea', Pure: '#fff59d', Dark: '#5e35b1', Unstable: '#ff8a65', Hungry: '#8d6e63', Tainted: '#ab47bc' }[m.ntype] ?? '#b0bec5';
        ctx.beginPath();
        ctx.moveTo(sx, sy - 5); ctx.lineTo(sx + 5, sy); ctx.lineTo(sx, sy + 5); ctx.lineTo(sx - 5, sy); ctx.closePath();
        ctx.fillStyle = col;
        if (m.valid === false) ctx.globalAlpha = 0.45; // possible / conditional node
        ctx.fill();
        ctx.lineWidth = 1;
        ctx.strokeStyle = '#000';
        ctx.stroke();
        ctx.globalAlpha = 1;
      } else if (m.kind === 'tfFeature') {
        // TF major feature: green tower (structures) / ring (terrain features)
        ctx.beginPath();
        if (m.structure) {
          ctx.moveTo(sx - 5, sy + 6); ctx.lineTo(sx - 5, sy - 4); ctx.lineTo(sx - 2, sy - 4); ctx.lineTo(sx - 2, sy - 8); ctx.lineTo(sx + 2, sy - 8);
          ctx.lineTo(sx + 2, sy - 4); ctx.lineTo(sx + 5, sy - 4); ctx.lineTo(sx + 5, sy + 6); ctx.closePath();
        } else ctx.arc(sx, sy, 6, 0, Math.PI * 2);
        ctx.fillStyle = m.structure ? '#2e7d32' : 'rgba(46,125,50,0.5)';
        ctx.fill();
        ctx.lineWidth = 1.5;
        ctx.strokeStyle = '#000';
        ctx.stroke();
      } else if (m.kind === 'fortressPiece') {
        // fortress piece footprint: bridges lighter, enclosed corridors darker
        const p0 = this.worldToScreen(m.box.x0, m.box.z0), p1 = this.worldToScreen(m.box.x1, m.box.z1);
        ctx.fillStyle = m.bridge ? 'rgba(120,30,30,0.55)' : 'rgba(60,10,10,0.75)';
        ctx.fillRect(p0.sx, p0.sy, p1.sx - p0.sx, p1.sy - p0.sy);
        if (this.view.zoom >= 0) {
          ctx.strokeStyle = 'rgba(255,112,67,0.55)';
          ctx.lineWidth = 1;
          ctx.strokeRect(p0.sx + 0.5, p0.sy + 0.5, p1.sx - p0.sx - 1, p1.sy - p0.sy - 1);
        }
        continue;
      } else if (m.kind === 'spawner') {
        // blaze spawner: flame-coloured cage
        ctx.fillStyle = '#ffb300';
        ctx.fillRect(sx - 5, sy - 5, 10, 10);
        ctx.lineWidth = 1.5;
        ctx.strokeStyle = '#212121';
        ctx.strokeRect(sx - 5, sy - 5, 10, 10);
        ctx.beginPath();
        ctx.moveTo(sx - 5, sy); ctx.lineTo(sx + 5, sy); ctx.moveTo(sx, sy - 5); ctx.lineTo(sx, sy + 5);
        ctx.stroke();
      } else if (m.kind === 'wart') {
        ctx.fillStyle = '#b71c1c';
        ctx.beginPath();
        ctx.arc(sx, sy, 4, 0, Math.PI * 2);
        ctx.fill();
        ctx.lineWidth = 1;
        ctx.strokeStyle = '#000';
        ctx.stroke();
      } else if (m.kind === 'fortress') {
        ctx.fillStyle = '#4e0d0d';
        ctx.fillRect(sx - 7, sy - 7, 14, 14);
        ctx.lineWidth = 2;
        ctx.strokeStyle = '#ff7043';
        ctx.strokeRect(sx - 7, sy - 7, 14, 14);
      } else if (m.kind === 'mineshaft') {
        // mineshaft: crossed pickaxe-ish timber frame
        ctx.fillStyle = '#6d4c41';
        ctx.fillRect(sx - 6, sy - 6, 12, 12);
        ctx.lineWidth = 2;
        ctx.strokeStyle = '#ffcc80';
        ctx.beginPath();
        ctx.moveTo(sx - 4, sy + 5); ctx.lineTo(sx - 4, sy - 4); ctx.lineTo(sx + 4, sy - 4); ctx.lineTo(sx + 4, sy + 5);
        ctx.stroke();
      } else if (m.kind === 'village') {
        // village: little house
        ctx.beginPath();
        ctx.moveTo(sx - 6, sy + 5); ctx.lineTo(sx - 6, sy - 1); ctx.lineTo(sx, sy - 7); ctx.lineTo(sx + 6, sy - 1); ctx.lineTo(sx + 6, sy + 5); ctx.closePath();
        ctx.fillStyle = '#a1887f';
        ctx.fill();
        ctx.lineWidth = 1.5;
        ctx.strokeStyle = '#000';
        ctx.stroke();
      } else if (m.kind === 'spawn') {
        // world spawn: green diamond with a white core
        ctx.beginPath();
        ctx.moveTo(sx, sy - 9); ctx.lineTo(sx + 9, sy); ctx.lineTo(sx, sy + 9); ctx.lineTo(sx - 9, sy); ctx.closePath();
        ctx.fillStyle = '#43a047';
        ctx.fill();
        ctx.lineWidth = 2;
        ctx.strokeStyle = '#000';
        ctx.stroke();
        ctx.beginPath();
        ctx.arc(sx, sy, 2.5, 0, Math.PI * 2);
        ctx.fillStyle = '#fff';
        ctx.fill();
      } else if (m.kind === 'stronghold') {
        // purple square with an eye: End portal lives in the stronghold's portal room (within ~120 blocks)
        ctx.fillStyle = '#7e57c2';
        ctx.fillRect(sx - 7, sy - 7, 14, 14);
        ctx.lineWidth = 2;
        ctx.strokeStyle = '#000';
        ctx.strokeRect(sx - 7, sy - 7, 14, 14);
        ctx.fillStyle = '#c8f7c5';
        ctx.beginPath();
        ctx.arc(sx, sy, 3, 0, Math.PI * 2);
        ctx.fill();
      } else if (m.kind === 'loot') {
        // LootGames puzzle dungeon: gold square
        const r = this.view.zoom >= -2 ? 6 : 4;
        ctx.fillStyle = '#f2c94c';
        ctx.fillRect(sx - r, sy - r, 2 * r, 2 * r);
        ctx.lineWidth = 1.5;
        ctx.strokeStyle = '#000';
        ctx.strokeRect(sx - r, sy - r, 2 * r, 2 * r);
      } else if (m.kind === 'rogueArea') {
        // Roguelike anchor: dashed 40..100 block search ring (only when it is visible at this zoom)
        const r1 = 40 * this.ppb, r2 = 100 * this.ppb;
        if (r2 > 12) {
          ctx.setLineDash([4, 4]);
          ctx.lineWidth = 1.5;
          ctx.strokeStyle = 'rgba(239,83,80,0.8)';
          for (const r of [r1, r2]) {
            ctx.beginPath();
            ctx.arc(sx, sy, r, 0, Math.PI * 2);
            ctx.stroke();
          }
          ctx.setLineDash([]);
        }
        ctx.beginPath();
        ctx.arc(sx, sy, 3.5, 0, Math.PI * 2);
        ctx.fillStyle = m.found ? '#ef5350' : 'rgba(239,83,80,0.35)';
        ctx.fill();
        ctx.lineWidth = 1.5;
        ctx.strokeStyle = '#000';
        ctx.stroke();
      } else if (m.kind === 'rogue') {
        // Roguelike dungeon (predicted entrance): red triangle
        ctx.beginPath();
        ctx.moveTo(sx, sy - 8); ctx.lineTo(sx + 7, sy + 5); ctx.lineTo(sx - 7, sy + 5); ctx.closePath();
        ctx.fillStyle = m.lowConf ? 'rgba(239,83,80,0.45)' : '#ef5350';
        ctx.fill();
        ctx.lineWidth = 2;
        ctx.strokeStyle = '#000';
        ctx.stroke();
      } else if (m.kind === 'fluid') {
        // GT underground fluid field (128x128): tint, stronger for higher yields; centre dot carries the popup
        const p0 = this.worldToScreen(m.box.x0, m.box.z0), p1 = this.worldToScreen(m.box.x1, m.box.z1);
        const a = Math.min(0.4, 0.08 + m.lpo / 1500);
        ctx.globalAlpha = a;
        ctx.fillStyle = m.color;
        ctx.fillRect(p0.sx, p0.sy, p1.sx - p0.sx, p1.sy - p0.sy);
        ctx.globalAlpha = 1;
        if (this.view.zoom >= -3) {
          ctx.strokeStyle = 'rgba(0,0,0,0.35)';
          ctx.lineWidth = 1;
          ctx.strokeRect(p0.sx + 0.5, p0.sy + 0.5, p1.sx - p0.sx - 1, p1.sy - p0.sy - 1);
        }
        if (this.view.zoom >= -2) {
          ctx.beginPath();
          ctx.arc(sx, sy, 4, 0, Math.PI * 2);
          ctx.fillStyle = m.color;
          ctx.fill();
          ctx.strokeStyle = '#fff';
          ctx.lineWidth = 1.5;
          ctx.stroke();
        }
        if (this.view.zoom >= -1) {
          const t = `${m.label.split(' · ')[0]} ${m.lpo}`;
          ctx.font = '11px system-ui, sans-serif';
          ctx.textAlign = 'center';
          const tw = ctx.measureText(t).width;
          ctx.fillStyle = 'rgba(0,0,0,0.6)';
          ctx.fillRect(sx - tw / 2 - 3, sy + 7, tw + 6, 14);
          ctx.fillStyle = '#fff';
          ctx.fillText(t, sx, sy + 18);
          ctx.textAlign = 'start';
          ctx.font = '12px system-ui, sans-serif';
        }
        continue;
      } else if (m.kind === 'ore') {
        // oreseed chunk outline (vein centre) when zoomed in
        if (this.view.zoom >= 1 && m.box) {
          const p0 = this.worldToScreen(m.box.x0, m.box.z0), p1 = this.worldToScreen(m.box.x1, m.box.z1);
          ctx.strokeStyle = m.color.replace('hsl(', 'hsla(').replace(')', ' / 0.6)');
          ctx.lineWidth = 1;
          ctx.strokeRect(p0.sx + 0.5, p0.sy + 0.5, p1.sx - p0.sx - 1, p1.sy - p0.sy - 1);
        }
        // ore-block icon: stone square with specks in the vein's ore colours (JourneyMap / VisualProspecting style)
        if (this.view.zoom < -3) continue;
        if (this.view.zoom < 0) {
          // zoomed out: a small dot in the vein colour, so the distribution stays readable without hiding the map
          const d = this.view.zoom < -2 ? 1 : this.view.zoom < -1 ? 1.5 : 2;
          ctx.fillStyle = m.color;
          ctx.fillRect(sx - d, sy - d, 2 * d, 2 * d);
          continue;
        }
        const r = this.view.zoom >= 1 ? 7 : 5;
        ctx.fillStyle = '#7a7a7a';
        ctx.fillRect(sx - r, sy - r, 2 * r, 2 * r);
        if (r >= 4) {
          const sp = m.speck.length ? m.speck : [m.icon];
          const q = r / 2;
          [[-q, -q], [q, -q * 0.2], [-q * 0.4, q], [q * 0.9, q * 0.9]].forEach(([dx, dy], k) => {
            ctx.fillStyle = sp[k % sp.length];
            ctx.fillRect(sx + dx - q * 0.6, sy + dy - q * 0.6, q * 1.2, q * 1.2);
          });
        } else {
          ctx.fillStyle = m.icon;
          ctx.fillRect(sx - r + 1, sy - r + 1, 2 * r - 2, 2 * r - 2);
        }
        ctx.lineWidth = 1;
        ctx.strokeStyle = m.color;
        if (m.dep) ctx.setLineDash([2, 2]); // depends on the side the area is first loaded from
        ctx.strokeRect(sx - r - 0.5, sy - r - 0.5, 2 * r + 1, 2 * r + 1);
        ctx.setLineDash([]);
        if (m.dep && r >= 7) {
          ctx.font = 'bold 9px system-ui, sans-serif';
          ctx.fillStyle = '#ffd54f';
          ctx.fillText('?', sx + r - 1, sy - r + 3);
          ctx.font = '12px system-ui, sans-serif';
        }
      } else if (m.kind === 'tcRing') {
        // eldritch obelisk: tall dark-purple spire
        ctx.beginPath();
        ctx.moveTo(sx, sy - 11); ctx.lineTo(sx + 5, sy + 5); ctx.lineTo(sx - 5, sy + 5); ctx.closePath();
        ctx.fillStyle = '#4a148c';
        ctx.fill();
        ctx.lineWidth = 2;
        ctx.strokeStyle = '#e1bee7';
        ctx.stroke();
      } else if (m.kind === 'tcHilltop' || m.kind === 'tcMound' || m.kind === 'tcTotem') {
        // eerie-node structures: purple circle, letter for the kind
        ctx.beginPath();
        ctx.arc(sx, sy, 6.5, 0, Math.PI * 2);
        ctx.fillStyle = '#9c4dcc';
        ctx.fill();
        ctx.lineWidth = 1.5;
        ctx.strokeStyle = '#000';
        ctx.stroke();
        ctx.fillStyle = '#fff';
        ctx.font = 'bold 9px system-ui, sans-serif';
        ctx.textAlign = 'center';
        ctx.fillText({ tcHilltop: 'H', tcMound: 'M', tcTotem: 'T' }[m.kind], sx, sy + 3);
        ctx.textAlign = 'start';
        ctx.font = '12px system-ui, sans-serif';
      } else if (m.kind === 'hit') {
        // search hit: cyan diamond with its number (white ring = the one last jumped to)
        const r = m.n ? 10 : 7;
        ctx.beginPath();
        ctx.moveTo(sx, sy - r); ctx.lineTo(sx + r, sy); ctx.lineTo(sx, sy + r); ctx.lineTo(sx - r, sy); ctx.closePath();
        ctx.fillStyle = '#4dd0e1';
        ctx.fill();
        ctx.lineWidth = m.selected ? 3 : 2;
        ctx.strokeStyle = m.selected ? '#fff' : '#000';
        ctx.stroke();
        if (m.n) {
          ctx.fillStyle = '#062a30';
          ctx.font = 'bold 10px system-ui, sans-serif';
          ctx.textAlign = 'center';
          ctx.textBaseline = 'middle';
          ctx.fillText(String(m.n), sx, sy + 0.5);
          ctx.textAlign = 'start';
          ctx.textBaseline = 'alphabetic';
          ctx.font = '12px system-ui, sans-serif';
        }
      } else {
        ctx.strokeStyle = '#000';
        ctx.lineWidth = 3;
        ctx.beginPath();
        ctx.moveTo(sx - 7, sy); ctx.lineTo(sx + 7, sy); ctx.moveTo(sx, sy - 7); ctx.lineTo(sx, sy + 7);
        ctx.stroke();
        ctx.strokeStyle = '#fff';
        ctx.lineWidth = 1.5;
        ctx.stroke();
      }
      if (!m.label || this.mini) continue;
      // keep the map readable: feature labels only when zoomed in
      if ((m.kind === 'loot' || m.kind === 'rogue' || m.kind === 'rogueArea' || m.kind === 'village' || m.kind === 'mineshaft' || m.kind === 'tfFeature' || m.kind === 'fortress' || m.kind === 'spawner' || m.kind.startsWith('tc')) && this.view.zoom < -1) continue;
      if (m.kind === 'node' && this.view.zoom < 1) continue;
      if (m.kind === 'geode' && (this.view.zoom < 1 || (this.view.zoom < 2 && !m.rich))) continue; // dense in mountains: diamond ones first
      if ((m.kind === 'vdungeon' || m.kind === 'minechest') && this.view.zoom < 2) continue;
      if (m.kind === 'lake' && this.view.zoom < 2) continue;
      if (m.kind === 'ore') {
        // name centred above the icon, Y range below (labels need >= ~96 px between oreseeds)
        if (this.view.zoom < 1) continue;
        const room = 48 * this.ppb - 8;
        ctx.textAlign = 'center';
        ctx.font = '11px system-ui, sans-serif';
        let name = m.name, tw = ctx.measureText(name).width;
        while (tw > room && name.length > 4) {
          name = name.slice(0, -2).trimEnd() + '…';
          name = name.replace(/……$/, '…');
          tw = ctx.measureText(name).width;
        }
        const yw = ctx.measureText(m.yr).width;
        const hw = Math.max(tw, yw) / 2 + 3;
        labels.push({
          prio: 3, d: Math.hypot(sx - cxs, sy - cys), r: [sx - hw, sy - 24, sx + hw, sy + 24],
          draw: () => {
            ctx.textAlign = 'center';
            ctx.font = '11px system-ui, sans-serif';
            ctx.fillStyle = 'rgba(0,0,0,0.6)';
            ctx.fillRect(sx - tw / 2 - 3, sy - 24, tw + 6, 14);
            ctx.fillStyle = '#fff';
            ctx.fillText(name, sx, sy - 13);
            ctx.fillStyle = 'rgba(0,0,0,0.6)';
            ctx.fillRect(sx - yw / 2 - 3, sy + 10, yw + 6, 14);
            ctx.fillStyle = '#ffe082';
            ctx.fillText(m.yr, sx, sy + 21);
            ctx.textAlign = 'start';
            ctx.font = '12px system-ui, sans-serif';
          },
        });
        ctx.textAlign = 'start';
        ctx.font = '12px system-ui, sans-serif';
        continue;
      }
      const tw = ctx.measureText(m.label).width;
      const lx = m.kind === 'pin' ? sx + 12 : sx + 9;
      labels.push({
        prio: m.kind === 'pin' ? 0 : m.kind === 'hit' ? 1 : LANDMARK.has(m.kind) ? 2 : 3,
        d: Math.hypot(sx - cxs, sy - cys),
        r: [lx, sy - 18, lx + tw + 8, sy - 2],
        draw: () => {
          if (m.kind === 'pin' && m.ghost) ctx.globalAlpha = 0.7;
          ctx.fillStyle = 'rgba(0,0,0,0.65)';
          ctx.fillRect(lx, sy - 18, tw + 8, 16);
          ctx.fillStyle = m.kind === 'pin' ? m.color ?? '#fff' : '#fff';
          ctx.fillText(m.label, lx + 4, sy - 6);
          ctx.globalAlpha = 1;
        },
      });
    }
    // group badges: count in a dark disc, ring + up to 3 dots in the colours of the kinds inside
    for (const g of groups) {
      if (g.list.length < 2) continue;
      let x = 0, y = 0, wx = 0, wz = 0;
      const kinds = new Map();
      for (const v of g.list) {
        x += v.sx; y += v.sy; wx += v.m.x; wz += v.m.z;
        kinds.set(v.m.kind, (kinds.get(v.m.kind) ?? 0) + 1);
      }
      const n = g.list.length;
      x /= n; y /= n;
      const top = [...kinds.entries()].sort((a, b) => b[1] - a[1]);
      const r = n < 10 ? 10 : n < 100 ? 12 : 14;
      ctx.beginPath();
      ctx.arc(x, y, r, 0, Math.PI * 2);
      ctx.fillStyle = 'rgba(16,20,24,0.9)';
      ctx.fill();
      ctx.lineWidth = 2.5;
      ctx.strokeStyle = KIND_COLOR[top[0][0]] ?? '#cfd8dc';
      ctx.stroke();
      top.slice(0, 3).forEach(([k], i, arr) => {
        const a = -Math.PI / 2 + (i - (arr.length - 1) / 2) * 0.62;
        ctx.beginPath();
        ctx.arc(x + Math.cos(a) * r, y + Math.sin(a) * r, 3, 0, Math.PI * 2);
        ctx.fillStyle = KIND_COLOR[k] ?? '#cfd8dc';
        ctx.fill();
        ctx.lineWidth = 1;
        ctx.strokeStyle = '#000';
        ctx.stroke();
      });
      ctx.fillStyle = '#fff';
      ctx.font = 'bold 11px system-ui, sans-serif';
      ctx.textAlign = 'center';
      ctx.textBaseline = 'middle';
      ctx.fillText(String(n), x, y + 0.5);
      ctx.textAlign = 'start';
      ctx.textBaseline = 'alphabetic';
      ctx.font = '12px system-ui, sans-serif';
      const members = g.list.map((v) => v.m);
      this.drawn.push({ m: { kind: 'group', x: Math.round(wx / n), z: Math.round(wz / n), members, kinds: top }, sx: x, sy: y, r });
    }
    // labels: user markers, search hits and landmarks first, then nearest to the centre; overlaps are skipped
    labels.sort((a, b) => a.prio - b.prio || a.d - b.d);
    const placed = [];
    for (const l of labels) {
      const [x0, y0, x1, y1] = l.r;
      if (l.prio > 0 && placed.some((q) => x0 < q[2] && x1 > q[0] && y0 < q[3] && y1 > q[1])) continue;
      placed.push([x0 - 2, y0 - 1, x1 + 2, y1 + 1]);
      l.draw();
    }
    if (this.mini) return;
    // scale bar
    const target = 120 / this.ppb;
    const nice = [1, 2, 5].flatMap((m) => [1, 10, 100, 1000, 10000, 100000].map((p) => m * p)).sort((a, b) => a - b);
    const len = nice.reduce((best, n) => (n <= target ? n : best), 1);
    const px = len * this.ppb;
    // `bottomInset`: css px of the canvas bottom covered by the phone sheet / tab strip (set by App)
    const by = height - (this.bottomInset || 0);
    ctx.fillStyle = 'rgba(0,0,0,0.55)';
    ctx.fillRect(12, by - 34, px + 16, 24);
    ctx.fillStyle = '#fff';
    ctx.fillRect(20, by - 18, px, 3);
    ctx.fillText(len >= 1000 ? `${len / 1000}k blocks` : `${len} blocks`, 20, by - 22);
  }

  scheduleStats() {
    if (!this.onStats || this.statsTimer) return;
    this.statsTimer = setTimeout(() => {
      this.statsTimer = 0;
      this.onStats(this.visibleStats());
    }, 150);
  }

  /** Approximate share of each biome on screen (from the current level's cached tiles, clipped to the view). */
  visibleStats() {
    if (!this.world) return [];
    const s = this.levelStep();
    const { width, height } = this.cssSize();
    const a = this.screenToWorld(0, 0), b = this.screenToWorld(width, height);
    const counts = new Map();
    let total = 0;
    const stride = Math.max(1, Math.round(4 / Math.max(1e-9, s * this.ppb))); // ~every 4 px
    // terrain mode with the biome overlay: count the biome tiles (so the biome legend has shares)
    const m = this.mode === 2 && this.biomeOverlay > 0 ? 0 : this.mode;
    for (const [tx, tz] of this.visibleTiles(s)) {
      const t = this.tiles.get(this.key(s, tx, tz, m));
      if (!t) continue;
      const span = TILE * s;
      const i0 = Math.max(0, Math.floor((a.x - tx * span) / s)), i1 = Math.min(TILE, Math.ceil((b.x - tx * span) / s));
      const j0 = Math.max(0, Math.floor((a.z - tz * span) / s)), j1 = Math.min(TILE, Math.ceil((b.z - tz * span) / s));
      for (let j = j0; j < j1; j += stride)
        for (let i = i0; i < i1; i += stride) {
          const id = t.ids[j * TILE + i];
          counts.set(id, (counts.get(id) ?? 0) + 1);
          total++;
        }
    }
    return [...counts.entries()].map(([id, n]) => ({ id, share: n / total })).sort((x, y) => y.share - x.share);
  }
}

export { LAVA_MARKER };

// ---- terrain height (render mode 2: value = height * 8) ------------------------------------------------------------
const RAMP = [
  // [height, r, g, b]
  [0, 8, 20, 48], [40, 18, 52, 102], [58, 40, 96, 150], [62.9, 70, 130, 180],
  [63, 74, 128, 72], [72, 98, 150, 80], [88, 140, 160, 92], [110, 160, 140, 96], [135, 140, 120, 104],
  [160, 150, 150, 150], [190, 220, 220, 225], [256, 255, 255, 255],
];
const RAINBOW = [
  [0, 20, 10, 60], [40, 40, 40, 160], [62.9, 40, 120, 220], [63, 30, 170, 90], [80, 120, 200, 40], [100, 230, 210, 40],
  [125, 240, 130, 30], [150, 220, 40, 40], [180, 170, 40, 160], [215, 230, 160, 240], [256, 255, 255, 255],
];
const SEA = [
  [0, 0, 0, 20], [20, 10, 20, 80], [35, 20, 60, 150], [45, 30, 110, 200], [53, 60, 170, 230], [58, 110, 210, 240], [62.9, 180, 240, 250],
  [63, 120, 120, 110], [100, 170, 170, 160], [256, 240, 240, 240],
];
const HIGH = [
  [0, 10, 20, 50], [62.9, 50, 90, 140], [63, 70, 90, 60], [85, 90, 110, 70], [95, 150, 170, 80], [110, 210, 190, 70],
  [125, 230, 140, 50], [140, 200, 70, 50], [160, 150, 60, 110], [185, 200, 160, 220], [256, 255, 255, 255],
];
function lerpRamp(R, h) {
  let i = 1;
  while (i < R.length - 1 && R[i][0] < h) i++;
  const a = R[i - 1], b = R[i];
  const t = Math.max(0, Math.min(1, (h - a[0]) / (b[0] - a[0] || 1)));
  return [a[1] + (b[1] - a[1]) * t, a[2] + (b[2] - a[2]) * t, a[3] + (b[3] - a[3]) * t];
}
/** Banded: colour of the band's bottom height, every other band darker so each step reads on its own. */
function banded(R, h, from, size, dark = 0.8) {
  if (h < from) return lerpRamp(R, h);
  const band = Math.floor((h - from) / size);
  const c = lerpRamp(R, from + band * size + size / 2);
  const k = band % 2 ? dark : 1;
  return [c[0] * k, c[1] * k, c[2] * k];
}
/** Terrain-mode colour presets (height h in blocks, sea level at 63 on the code scale). */
export const TERRAIN_PRESETS = {
  default: { name: 'Default (smooth)', fn: (h) => lerpRamp(RAMP, h) },
  steps2: { name: 'Aggressive: 2-block steps from Y40', fn: (h) => banded(RAINBOW, h, 40, 2, 0.72) },
  steps4: { name: '4-block steps from Y40', fn: (h) => banded(RAMP, h, 40, 4, 0.78) },
  contour8: { name: 'Contours every 8 blocks', fn: (h) => banded(RAMP, h, 0, 8, 0.86) },
  bands16: { name: '16-block bands', fn: (h) => banded(RAINBOW, h, 0, 16, 1) },
  rainbow: { name: 'Rainbow (full range)', fn: (h) => lerpRamp(RAINBOW, h) },
  sea: { name: 'Sea depth focus', fn: (h) => lerpRamp(SEA, h) },
  high: { name: 'Mountains focus (Y90+)', fn: (h) => lerpRamp(HIGH, h) },
};
let terrainFn = TERRAIN_PRESETS.default.fn;
/** Pick the terrain colour preset (all renderers share it). Returns true when it changed. */
export function setTerrainPreset(name) {
  const fn = (TERRAIN_PRESETS[name] ?? TERRAIN_PRESETS.default).fn;
  if (fn === terrainFn) return false;
  terrainFn = fn;
  return true;
}

/** Elevation colour (ABGR) for a height code. */
export function terrainColor(code) {
  const [r, g, bl] = terrainFn(code / 8);
  return (255 << 24) | ((Math.min(255, bl) | 0) << 16) | ((Math.min(255, g) | 0) << 8) | (Math.min(255, r) | 0);
}

/** Lambert-ish hillshade (light from the north-west) as a brightness factor per pixel; water stays flat. */
export function hillshade(ids, step) {
  const n = 256;
  const out = new Float32Array(n * n);
  const k = 2.2 / step; // blocks per sample
  for (let j = 0; j < n; j++)
    for (let i = 0; i < n; i++) {
      const h = (x, y) => Math.max(63, ids[Math.min(n - 1, Math.max(0, y)) * n + Math.min(n - 1, Math.max(0, x))] / 8);
      // central differences; one-sided at tile edges (a clamped central difference halves the slope there and
      // shows the tile seams)
      const i0 = i > 0 ? i - 1 : i, i1 = i < n - 1 ? i + 1 : i, j0 = j > 0 ? j - 1 : j, j1 = j < n - 1 ? j + 1 : j;
      const dx = (h(i1, j) - h(i0, j)) * k / (i1 - i0);
      const dz = (h(i, j1) - h(i, j0)) * k / (j1 - j0);
      // normal = (-dx, 1, -dz); light = normalize(-1, 1.4, -1)
      const len = Math.sqrt(dx * dx + 1 + dz * dz);
      const lambert = (dx * 1 + 1.4 + dz * 1) / (len * 1.98);
      out[j * n + i] = 0.55 + 0.6 * Math.max(0, lambert);
    }
  return out;
}
