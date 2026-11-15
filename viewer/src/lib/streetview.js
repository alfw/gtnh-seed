// Street view: a first-person 3D look at the terrain around a point. Lazy-loaded (three.js stays out of the main
// bundle). Near: exact block columns (top block + water + Nether ceiling) in 32×32 tiles, nearest first through the
// worker pool. Far: the zoomed-out heights as a smooth heightfield, sunk out of sight under loaded near tiles.
// No textures: flat colours per top block (grass tinted by biome), baked per-face shading, fog, sky gradient.
// Columns are 3 u32 per sample: [top | water<<8 | class<<16 | ceil<<24, rgb, biome gen id]. Every mesh carries two
// colour sets (block / biome tint) so the biome overlay toggles without rebuilding.
import * as THREE from 'three';
import { fmtBlocks } from './blocks.js';
import { terrainColor } from './renderer.js';

// flat block colours are display values: no linear/sRGB round trip
THREE.ColorManagement.enabled = false;

const T = 32; // near tile size (blocks)
const FAR_N = 64; // far tile samples per side (+1 overlap)
const EYE = 1.62;
const NS = 'sv';
// spawn area decoration replayed in the fresh world's load order (wasm `spawn_deco_voxels`), kept across opens:
// { key: 'seed|worldType', chunks: Map("cx,cz" -> Uint8Array deco_chunk record) } | { key, pending: Promise }
let spawnVox = null;
const TINT = 0.72; // biome overlay: share of the biome colour
const MARKERS_PER_GROUP = 40;

// classes (match crates/rwg2-wasm view_cols)
const C = { GRASS: 0, STONE: 1, SAND: 2, SNOW: 3, DIRT: 4, CLAY: 5, GRAVEL: 6, ICE: 7, NETHERRACK: 8, SOUL: 9, LAVA: 10, DEADROCK: 11, BEDROCK: 12, MYCELIUM: 13, PACKED_ICE: 14, END_STONE: 17, END_SURFACE: 18, VOID: 19 };
const RGB_END_STONE = 0xdbde9e;
const RGB_DIRT = 0x866043, RGB_STONE = 0x7d7d7d, RGB_NETHERRACK = 0x6f3634, RGB_SANDSTONE = 0xd2c391;
// side layers of exact overworld columns (wasm `side_idx`): none, dirt, stone, sand, red sand, sandstone, gravel,
// stained clay (bands), snow, ice, packed ice, cobblestone, hardened clay, coarse dirt, mud / ash / mycelium, hard sand
const SIDE_RGB = [0, 0x866043, 0x7d7d7d, 0xdbd3a0, 0xa95821, 0xd2c391, 0x857f7e, 0x985f45, 0xf0fbfb, 0x91b6ff, 0xa6c3f2, 0x6e6e6e, 0x965d43, 0x77553b, 0x4c3a2b, 0xb3a17a];
const SIDE_CLAY = 7;
const MESA_BANDS = [0x985f45, 0xd1b1a1, 0xa15325, 0x764656, 0xba8523, 0x985f45, 0x87683c, 0x8f3d2e, 0xd1b1a1, 0x5a3c33];
const SHADE = { top: 1.0, bottom: 0.5, x: 0.8, z: 0.64 };

const DIM_LOOK = {
  0: { zenith: 0x5f95ee, horizon: 0xbfd6f6, sea: 62, water: 0x3f76e4 },
  7: { zenith: 0x24414f, horizon: 0x8aa9a0, sea: 30, water: 0x2f5f8a },
  [-1]: { zenith: 0x2a0905, horizon: 0x4a120a, sea: null, water: null },
  1: { zenith: 0x07060c, horizon: 0x2a2440, sea: null, water: null },
};

/** Sub-surface colours of a column's side from the top (block y counted from the surface down). */
function sideRgb(cls, rgb, depth, y) {
  if (depth === 0 && cls !== C.CLAY) return rgb;
  switch (cls) {
    case C.GRASS:
    case C.MYCELIUM:
    case C.DIRT:
    case C.SNOW:
      return depth <= 3 ? RGB_DIRT : RGB_STONE;
    case C.SAND:
      return depth <= 3 ? rgb : depth <= 6 ? RGB_SANDSTONE : RGB_STONE;
    case C.CLAY:
      return MESA_BANDS[((y % MESA_BANDS.length) + MESA_BANDS.length) % MESA_BANDS.length];
    case C.NETHERRACK:
    case C.SOUL:
    case C.LAVA:
      return depth <= 2 && cls === C.SOUL ? rgb : RGB_NETHERRACK;
    default:
      return rgb;
  }
}

function put(K, o, rgb, shade) {
  const r = Math.min(255, ((rgb >> 16) & 255) * shade), g = Math.min(255, ((rgb >> 8) & 255) * shade), b = Math.min(255, (rgb & 255) * shade);
  for (let k = 0; k < 4; k++) {
    K[o + k * 3] = r;
    K[o + k * 3 + 1] = g;
    K[o + k * 3 + 2] = b;
  }
}
/** CSS colour (hex, rgb(), or the map's space-separated `hsl(h s% l%)`) → 0xRRGGBB. */
function cssHex(c) {
  if (typeof c === 'number') return c;
  const m = /^hsla?\(\s*([\d.]+)[\s,]+([\d.]+)%[\s,]+([\d.]+)%/.exec(c);
  if (m) return new THREE.Color().setHSL(Number(m[1]) / 360, Number(m[2]) / 100, Number(m[3]) / 100).getHex();
  return new THREE.Color(c).getHex();
}
/** Height-colour material: every fragment coloured by its block Y through the map's terrain preset (a 256-entry
 *  ramp texture), with flat shading from screen-space derivatives (`blocky`: top / side / bottom factors like the
 *  baked colours; else Lambert like the heightfields) and linear fog. */
function heightMaterial(ramp, fog, shift, blocky) {
  return new THREE.ShaderMaterial({
    uniforms: {
      ramp: { value: ramp }, shift: { value: shift }, opacity: { value: 1 },
      fogColor: { value: fog.color }, fogNear: { value: fog.near }, fogFar: { value: fog.far },
    },
    vertexShader: `
      varying vec3 vW; varying float vDepth;
      void main() {
        vec4 w = modelMatrix * vec4(position, 1.0);
        vW = w.xyz;
        vec4 mv = viewMatrix * w;
        vDepth = -mv.z;
        gl_Position = projectionMatrix * mv;
      }`,
    fragmentShader: `
      uniform sampler2D ramp; uniform float shift; uniform float opacity;
      uniform vec3 fogColor; uniform float fogNear; uniform float fogFar;
      varying vec3 vW; varying float vDepth;
      void main() {
        vec3 n = normalize(cross(dFdx(vW), dFdy(vW)));
        if (n.y < 0.0 && ${blocky ? 'false' : 'true'}) n = -n;
        float y = ${blocky ? 'floor(vW.y - (abs(n.y) > 0.5 ? 0.5 : 0.0) + 0.001)' : 'floor(vW.y - 0.5)'};
        vec3 c = texture2D(ramp, vec2((clamp(y + shift, 0.0, 255.0) + 0.5) / 256.0, 0.5)).rgb;
        float sh;
        ${blocky ? `if (abs(n.y) > 0.5) sh = n.y > 0.0 ? 1.0 : 0.5; else sh = abs(n.x) > 0.5 ? 0.8 : 0.64;` : `sh = 0.6 + 0.45 * max(0.0, dot(abs(n.y) > 0.0 ? n : -n, normalize(vec3(0.45, 1.0, 0.3))));`}
        c *= sh;
        float f = smoothstep(fogNear, fogFar, vDepth);
        gl_FragColor = vec4(mix(c, fogColor, f), opacity);
      }`,
    side: THREE.DoubleSide,
  });
}
/** 256×1 texture of the map's current terrain preset (index = height code / 8). */
function rampTexture(tex) {
  const data = tex?.image.data ?? new Uint8Array(256 * 4);
  for (let i = 0; i < 256; i++) {
    const v = terrainColor(i * 8 + 4);
    data.set([v & 255, (v >> 8) & 255, (v >> 16) & 255, 255], i * 4);
  }
  if (tex) {
    tex.needsUpdate = true;
    return tex;
  }
  const t = new THREE.DataTexture(data, 256, 1, THREE.RGBAFormat);
  t.magFilter = t.minFilter = THREE.NearestFilter;
  t.colorSpace = THREE.NoColorSpace;
  t.needsUpdate = true;
  return t;
}
function mixRgb(a, b, t) {
  const m = (sh) => Math.round(((a >> sh) & 255) * (1 - t) + ((b >> sh) & 255) * t) << sh;
  return m(16) | m(8) | m(0);
}

/** Growable quad buffer (two colour sets: block colours and the biome overlay). */
class Quads {
  constructor() {
    this.pos = new Float32Array(4096 * 12);
    this.col = new Uint8Array(4096 * 12);
    this.bcol = new Uint8Array(4096 * 12);
    this.n = 0; // quads
  }
  grow() {
    const p = new Float32Array(this.pos.length * 2);
    p.set(this.pos);
    this.pos = p;
    const c = new Uint8Array(this.col.length * 2);
    c.set(this.col);
    this.col = c;
    const b = new Uint8Array(this.bcol.length * 2);
    b.set(this.bcol);
    this.bcol = b;
  }
  quad(a, b, c, d, rgb, shade, brgb = rgb) {
    if ((this.n + 1) * 12 > this.pos.length) this.grow();
    const o = this.n * 12;
    const P = this.pos;
    P.set(a, o);
    P.set(b, o + 3);
    P.set(c, o + 6);
    P.set(d, o + 9);
    put(this.col, o, rgb, shade);
    put(this.bcol, o, brgb, shade);
    this.n++;
  }
  geometry() {
    if (!this.n) return null;
    const g = new THREE.BufferGeometry();
    g.setAttribute('position', new THREE.BufferAttribute(this.pos.slice(0, this.n * 12), 3));
    const ca = new THREE.BufferAttribute(this.col.slice(0, this.n * 12), 3, true);
    const cb = new THREE.BufferAttribute(this.bcol.slice(0, this.n * 12), 3, true);
    g.userData = { ca, cb };
    g.setAttribute('color', ca);
    const idx = new Uint32Array(this.n * 6);
    for (let q = 0; q < this.n; q++) {
      const v = q * 4, o = q * 6;
      idx[o] = v;
      idx[o + 1] = v + 1;
      idx[o + 2] = v + 2;
      idx[o + 3] = v;
      idx[o + 4] = v + 2;
      idx[o + 5] = v + 3;
    }
    g.setIndex(new THREE.BufferAttribute(idx, 1));
    g.computeBoundingSphere();
    return g;
  }
}

const KEYM = 16777216; // 2^24: packs two 24-bit colours (or a height/biome and a colour) into one double
const DIRS = [
  [1, 0],
  [-1, 0],
  [0, 1],
  [0, -1],
];
const KEY = new Float64Array(32 * 32);
let GRID = new Float64Array(32 * 256);
let DONE = new Uint8Array(32 * 256);
/** Greedy rectangles over a W×H grid of keys (NaN = empty): emit(i0, j0, w, h, key) per rectangle. */
function greedy(key, W, H, emit) {
  const n = W * H;
  if (DONE.length < n) DONE = new Uint8Array(n);
  DONE.fill(0, 0, n);
  for (let j = 0; j < H; j++)
    for (let i = 0; i < W; i++) {
      const k0 = j * W + i, kv = key[k0];
      if (DONE[k0] || kv !== kv) continue;
      let w = 1;
      while (i + w < W && !DONE[k0 + w] && key[k0 + w] === kv) w++;
      let h = 1;
      grow: while (j + h < H) {
        const r = (j + h) * W + i;
        for (let d = 0; d < w; d++) if (DONE[r + d] || key[r + d] !== kv) break grow;
        h++;
      }
      for (let jj = 0; jj < h; jj++) DONE.fill(1, (j + jj) * W + i, (j + jj) * W + i + w);
      emit(i, j, w, h, kv);
    }
}
/** Side walls of one orientation: per plane, a grid along × y of colour keys, merged greedily.
 *  lists[p] = flat [along, y0, y1, col, bcol, ...]; emit(p, along0, w, ya, yb, col, bcol). Planes are T long. */
function planes(lists, emit, W = 32) {
  for (let p = 0; p < lists.length; p++) {
    const L = lists[p];
    if (!L.length) continue;
    let y0 = 1e9, y1 = -1e9;
    for (let k = 0; k < L.length; k += 5) {
      if (L[k + 1] < y0) y0 = L[k + 1];
      if (L[k + 2] > y1) y1 = L[k + 2];
    }
    const H = y1 - y0, n = W * H;
    if (GRID.length < n) GRID = new Float64Array(n);
    GRID.fill(NaN, 0, n);
    for (let k = 0; k < L.length; k += 5) {
      const kv = L[k + 3] * KEYM + L[k + 4];
      for (let y = L[k + 1]; y < L[k + 2]; y++) GRID[(y - y0) * W + L[k]] = kv;
    }
    greedy(GRID, W, H, (i0, j0, w, h, kv) => {
      const col = Math.floor(kv / KEYM);
      emit(p, i0, w, y0 + j0, y0 + j0 + h, col, kv - col * KEYM);
    });
  }
}

function skyDome(zenith, horizon, radius) {
  const g = new THREE.SphereGeometry(radius, 32, 16);
  const pos = g.getAttribute('position');
  const cols = new Float32Array(pos.count * 3);
  const z = new THREE.Color(zenith), h = new THREE.Color(horizon), c = new THREE.Color();
  for (let i = 0; i < pos.count; i++) {
    const t = Math.max(0, pos.getY(i) / radius);
    c.copy(h).lerp(z, Math.pow(t, 0.6));
    cols.set([c.r, c.g, c.b], i * 3);
  }
  g.setAttribute('color', new THREE.BufferAttribute(cols, 3));
  const m = new THREE.MeshBasicMaterial({ vertexColors: true, side: THREE.BackSide, fog: false, depthWrite: false });
  const mesh = new THREE.Mesh(g, m);
  mesh.renderOrder = -10;
  return mesh;
}

function labelTexture(text, color) {
  const cv = document.createElement('canvas');
  const ctx = cv.getContext('2d');
  const font = '600 28px system-ui, sans-serif';
  ctx.font = font;
  const w = Math.ceil(ctx.measureText(text).width) + 24;
  cv.width = w;
  cv.height = 44;
  ctx.font = font;
  ctx.fillStyle = 'rgba(16,20,24,0.85)';
  ctx.fillRect(0, 0, w, 44);
  ctx.fillStyle = color;
  ctx.fillRect(0, 40, w, 4);
  ctx.fillStyle = '#fff';
  ctx.textBaseline = 'middle';
  ctx.fillText(text, 12, 21);
  return { tex: new THREE.CanvasTexture(cv), w };
}
function labelSprite(text, color) {
  const { tex, w } = labelTexture(text, color);
  const s = new THREE.Sprite(new THREE.SpriteMaterial({ map: tex, sizeAttenuation: false, depthTest: false, fog: false }));
  s.scale.set(w * 0.0011, 44 * 0.0011, 1);
  s.renderOrder = 10;
  return s;
}
function setLabel(s, text, color) {
  const { tex, w } = labelTexture(text, color);
  s.material.map.dispose();
  s.material.map = tex;
  s.scale.set(w * 0.0011, 44 * 0.0011, 1);
}

/**
 * opts: { pool, seed (string), worldType, dim, x, z, heading (deg, 0 = north), fly, phone,
 *         markers: [{x, z, y?, label, color, group, detail?}], groups: [group names shown], tint, water,
 *         biomeRgb(gen) -> 0xRRGGBB | null, onState(state), onBiomes({near: [[gen, n]], far: [[gen, n]]}), onPick(pick | null) }
 */
export function createStreetView(container, opts) {
  const { pool, seed, worldType, dim, phone, spawn } = opts;
  const look = DIM_LOOK[dim] ?? DIM_LOOK[0];
  const nether = dim === -1;
  // the End: floating land (top + lowest solid per column, void columns), no mid ring, no x-ray layers
  const end = dim === 1;
  const reqDim = opts.dimCode ?? dim;
  const nearR = (phone ? 8 : 16) * 16; // blocks
  const farStep = nether ? 8 : 16;
  const farR = nether ? 512 : phone ? 1024 : 2048;
  const farSpan = FAR_N * farStep;
  // mid ring: exact chunk heights every 4 blocks between the block tiles and the blurred horizon (not the Nether:
  // its fog hides it). The Twilight Forest's chunks are ~3× dearer, so its ring is narrower.
  const midR = nether || end ? 0 : phone ? 384 : dim === 7 ? 512 : 768;
  const triBudget = phone ? 450_000 : 2_000_000;
  const ox = Math.floor(opts.x), oz = Math.floor(opts.z); // scene origin (float precision)
  const t0 = performance.now();

  // ---- three setup
  const renderer = new THREE.WebGLRenderer({ antialias: !phone, preserveDrawingBuffer: true, powerPreference: 'high-performance', stencil: true });
  renderer.setPixelRatio(Math.min(window.devicePixelRatio || 1, phone ? 1.5 : 2));
  renderer.outputColorSpace = THREE.LinearSRGBColorSpace;
  container.appendChild(renderer.domElement);
  renderer.domElement.className = 'sv-canvas';
  const scene = new THREE.Scene();
  const fogColor = new THREE.Color(look.horizon);
  scene.background = fogColor;
  scene.fog = nether ? new THREE.Fog(fogColor, 24, 200) : end ? new THREE.Fog(fogColor, farR * 0.55, farR) : new THREE.Fog(fogColor, farR * 0.3, farR * 0.97);
  const camera = new THREE.PerspectiveCamera(70, 1, 0.1, nether ? 600 : farR * 1.6);
  camera.rotation.order = 'YXZ';
  const sky = skyDome(look.zenith, look.horizon, nether ? 500 : farR * 1.4);
  scene.add(sky);

  const nearMat = new THREE.MeshBasicMaterial({ vertexColors: true, side: THREE.DoubleSide });
  const farMat = new THREE.MeshBasicMaterial({ vertexColors: true, side: THREE.DoubleSide });
  const midMat = new THREE.MeshBasicMaterial({ vertexColors: true, side: THREE.DoubleSide });
  // height colours (map terrain preset): one ramp texture, block and heightfield variants
  let heightCol = !!opts.heightColours;
  const rampTex = rampTexture(null);
  const hShift = dim === 7 ? 32 : dim === -1 ? 31 : 0; // the map shifts dimension heights so sea / lava reads like Y63
  const hNearMat = heightMaterial(rampTex, scene.fog, hShift, true);
  const hFieldMat = heightMaterial(rampTex, scene.fog, hShift, false);
  const matNear = () => (heightCol ? hNearMat : nearMat);
  const matField = (L) => (heightCol ? hFieldMat : L.mat);
  const waterMat = look.water ? new THREE.MeshBasicMaterial({ color: look.water, transparent: true, opacity: 0.62, depthWrite: false, side: THREE.DoubleSide }) : null;
  if (waterMat) waterMat.visible = opts.water !== false;
  let tint = !!opts.tint;
  const bcache = new Map();
  const biomeRgb = (gen) => {
    let c = bcache.get(gen);
    if (c === undefined) {
      c = opts.biomeRgb?.(gen) ?? 0x808080;
      bcache.set(gen, c);
    }
    return c;
  };
  const tinted = (rgb, gen) => mixRgb(rgb, biomeRgb(gen), TINT);
  let sea = null, seaMat = null;
  const dryMat = new THREE.MeshBasicMaterial({
    colorWrite: false, depthWrite: false, side: THREE.DoubleSide,
    stencilWrite: true, stencilRef: 1, stencilFunc: THREE.AlwaysStencilFunc, stencilZPass: THREE.ReplaceStencilOp,
  });
  if (look.sea != null) {
    const g = new THREE.PlaneGeometry(farR * 3, farR * 3);
    g.rotateX(-Math.PI / 2);
    // the sea plane skips pixels where a dry column below sea level shows (stencil 1 from `dryMat` caps)
    seaMat = waterMat.clone();
    Object.assign(seaMat, { stencilWrite: true, stencilRef: 1, stencilFunc: THREE.NotEqualStencilFunc });
    seaMat.visible = waterMat.visible;
    sea = new THREE.Mesh(g, seaMat);
    sea.position.y = look.sea + 0.9;
    sea.renderOrder = 2;
    scene.add(sea);
  }
  let roof = null;
  if (nether) {
    const g = new THREE.PlaneGeometry(farR * 3, farR * 3);
    g.rotateX(Math.PI / 2);
    roof = new THREE.Mesh(g, new THREE.MeshBasicMaterial({ color: new THREE.Color(RGB_NETHERRACK).multiplyScalar(0.45), side: THREE.DoubleSide }));
    roof.position.y = 121;
    scene.add(roof);
  }

  // ---- state
  const st = {
    x: opts.x + 0.5, z: opts.z + 0.5, y: 80, heading: opts.heading ?? 0, pitch: -4, fov: 70,
    fly: !!opts.fly, groundKnown: false,
  };
  let closed = false, dirty = true, tris = 0, firstFrameMs = null, fullMs = null, capped = false;
  const near = new Map(); // "tx,tz" -> { tx, tz, data, mesh, water, tris }
  const nearPending = new Set();
  // heightfield levels: far (blurred zoomed-out heights) and mid (exact chunk heights); tiles "fx,fz" ->
  // { fx, fz, base: Float32Array, gens, mesh, tris }
  const mkLevel = (name, step, R, inner, prio, reqStep, mat) => ({ name, step, N: FAR_N, span: FAR_N * step, R, inner, prio, reqStep, mat, map: new Map(), pending: new Set(), centre: null });
  const farL = mkLevel('far', farStep, farR, 0, -900, farStep, farMat);
  // after the minimap tiles (priority ~0–2), before most feature cells
  const midL = midR ? mkLevel('mid', 4, midR, nearR - 32, 2, -4, midMat) : null;
  const levels = midL ? [midL, farL] : [farL];
  const far = farL.map;

  // ---- lookups
  function colAt(x, z) {
    const t = near.get(`${Math.floor(x / T)},${Math.floor(z / T)}`);
    if (!t) return -1;
    const lx = x - Math.floor(x / T) * T, lz = z - Math.floor(z / T) * T;
    return t.data[3 * (lz * T + lx)];
  }
  /** Biome gen id under (x, z): the exact column, else the nearest far sample. */
  function genAt(x, z) {
    x = Math.floor(x);
    z = Math.floor(z);
    const t = near.get(`${Math.floor(x / T)},${Math.floor(z / T)}`);
    if (t) return t.data[3 * ((z - Math.floor(z / T) * T) * T + (x - Math.floor(x / T) * T)) + 2];
    for (const L of levels) {
      const fx = Math.floor(x / L.span), fz = Math.floor(z / L.span);
      const f = L.map.get(`${fx},${fz}`);
      if (!f) continue;
      const i = Math.round((x - fx * L.span) / L.step), j = Math.round((z - fz * L.span) / L.step);
      return f.gens[j * (L.N + 1) + i];
    }
    return null;
  }
  function fieldAt(L, x, z) {
    const fx = Math.floor(x / L.span), fz = Math.floor(z / L.span);
    const f = L.map.get(`${fx},${fz}`);
    if (!f) return null;
    const u = (x - fx * L.span) / L.step, v = (z - fz * L.span) / L.step;
    const i = Math.min(L.N - 1, Math.floor(u)), j = Math.min(L.N - 1, Math.floor(v));
    const a = u - i, b = v - j, W = L.N + 1, H = f.base;
    return (H[j * W + i] * (1 - a) + H[j * W + i + 1] * a) * (1 - b) + (H[(j + 1) * W + i] * (1 - a) + H[(j + 1) * W + i + 1] * a) * b;
  }
  /** Top surface y (top face of the column) under (x, z), or null. */
  function groundAt(x, z) {
    const v = colAt(Math.floor(x), Math.floor(z));
    if (v >= 0) return end && ((v >> 16) & 255) === C.VOID ? null : (v & 255) + 1;
    for (const L of levels) {
      const f = fieldAt(L, x, z);
      if (f != null) return Number.isNaN(f) ? null : f;
    }
    return null;
  }

  // ---- biome shares (exact columns of the loaded near tiles; far samples out to farR)
  const nearCounts = new Map(), farCounts = new Map();
  let biomeVer = 0, biomeSent = -1;
  function countGens(m, data, w, h, sign) {
    for (let k = 0; k < w * h; k++) {
      const g = data[3 * k + 2];
      const n = (m.get(g) ?? 0) + sign;
      n > 0 ? m.set(g, n) : m.delete(g);
    }
    biomeVer++;
  }
  function countGensFar(gens, sign) {
    const W = FAR_N + 1;
    for (let j = 0; j < FAR_N; j++)
      for (let i = 0; i < FAR_N; i++) {
        const g = gens[j * W + i];
        const n = (farCounts.get(g) ?? 0) + sign;
        n > 0 ? farCounts.set(g, n) : farCounts.delete(g);
      }
    biomeVer++;
  }
  function applyTint() {
    const sw = (m) => m && m.geometry.setAttribute('color', tint ? m.geometry.userData.cb : m.geometry.userData.ca);
    for (const t of near.values()) sw(t.mesh);
    for (const L of levels) for (const f of L.map.values()) sw(f.mesh);
    dirty = true;
  }

  // ---- near meshes: greedy rectangles for tops (per height / colour / biome) and for the side walls of each
  // block-boundary plane (grid along × y, per colour), so cliffs and flat ground become a few big quads
  const X = (x) => x - ox, Z = (z) => z - oz;
  function buildNear(t) {
    if (t.mesh) {
      scene.remove(t.mesh);
      t.mesh.geometry.dispose();
      t.mesh = null;
    }
    for (const k of ['water', 'dry'])
      if (t[k]) {
        scene.remove(t[k]);
        t[k].geometry.dispose();
        t[k] = null;
      }
    tris -= t.tris;
    const q = new Quads(), wq = new Quads();
    const D = t.data;
    const bx = t.tx * T, bz = t.tz * T;
    const get = (lx, lz) => (lx >= 0 && lz >= 0 && lx < T && lz < T ? D[3 * (lz * T + lx)] : colAt(bx + lx, bz + lz));
    const N = T * T;
    // tops
    for (let k = 0; k < N; k++) {
      const v = D[3 * k];
      KEY[k] = end && ((v >> 16) & 255) === C.VOID ? NaN : (D[3 * k + 2] * 256 + ((v & 255) + 1)) * KEYM + D[3 * k + 1];
    }
    greedy(KEY, T, T, (i0, j0, w, h) => {
      const k = j0 * T + i0, v = D[3 * k], rgb = D[3 * k + 1], gen = D[3 * k + 2], y1 = (v & 255) + 1;
      const x0 = X(bx + i0), x1 = X(bx + i0 + w), z0 = Z(bz + j0), z1 = Z(bz + j0 + h);
      q.quad([x0, y1, z0], [x1, y1, z0], [x1, y1, z1], [x0, y1, z1], rgb, ((v >> 16) & 255) === C.LAVA ? 1.15 : SHADE.top, tinted(rgb, gen));
    });
    // water surfaces that are not the sea plane
    let anyWater = false;
    for (let k = 0; k < N; k++) {
      const w = (D[3 * k] >> 8) & 255;
      KEY[k] = !w || w === look.sea || end ? NaN : w; // End: the byte is the tower split
      anyWater ||= KEY[k] === KEY[k];
    }
    if (anyWater)
      greedy(KEY, T, T, (i0, j0, w, h, y) => {
        const yy = y + 0.9, x0 = X(bx + i0), x1 = X(bx + i0 + w), z0 = Z(bz + j0), z1 = Z(bz + j0 + h);
        wq.quad([x0, yy, z0], [x1, yy, z0], [x1, yy, z1], [x0, yy, z1], look.water, 1);
      });
    // dry columns below sea level (lava-cave funnels, dry pits): stencil caps at the sea plane keep it out
    const dq = new Quads();
    if (look.sea != null && !end) {
      let anyDry = false;
      for (let k = 0; k < N; k++) {
        const v = D[3 * k];
        KEY[k] = !((v >> 8) & 255) && (v & 255) + 1 < look.sea + 0.9 ? 1 : NaN;
        anyDry ||= KEY[k] === 1;
      }
      if (anyDry)
        greedy(KEY, T, T, (i0, j0, w, h) => {
          const yy = look.sea + 0.9, x0 = X(bx + i0), x1 = X(bx + i0 + w), z0 = Z(bz + j0), z1 = Z(bz + j0 + h);
          dq.quad([x0, yy, z0], [x1, yy, z0], [x1, yy, z1], [x0, yy, z1], 0, 1);
        });
    }
    // undersides (End): the lowest solid block's bottom face
    if (end) {
      for (let k = 0; k < N; k++) {
        const v = D[3 * k];
        KEY[k] = ((v >> 16) & 255) === C.VOID ? NaN : (v >>> 24) * 65536 + D[3 * k + 2];
      }
      greedy(KEY, T, T, (i0, j0, w, h, kv) => {
        const b = Math.floor(kv / 65536), gen = kv - b * 65536;
        const x0 = X(bx + i0), x1 = X(bx + i0 + w), z0 = Z(bz + j0), z1 = Z(bz + j0 + h);
        q.quad([x0, b, z0], [x1, b, z0], [x1, b, z1], [x0, b, z1], RGB_END_STONE, SHADE.bottom, tinted(RGB_END_STONE, gen));
      });
    }
    // End towers: top face of the lower run and underside of the upper run of two-run columns
    if (end && t.low) {
      for (let k = 0; k < N; k++) {
        const lo = t.low[k];
        if (!lo) continue;
        const v = D[3 * k], split = (v >> 8) & 255, gen = D[3 * k + 2];
        if (split <= lo) continue;
        const lx = k % T, lz = (k - lx) / T;
        const x0 = X(bx + lx), x1 = x0 + 1, z0 = Z(bz + lz), z1 = z0 + 1;
        q.quad([x0, lo, z0], [x1, lo, z0], [x1, lo, z1], [x0, lo, z1], RGB_END_STONE, SHADE.top, tinted(RGB_END_STONE, gen));
        q.quad([x0, split, z0], [x1, split, z0], [x1, split, z1], [x0, split, z1], D[3 * k + 1], SHADE.bottom, tinted(D[3 * k + 1], gen));
      }
    }
    // ceilings (Nether)
    if (nether) {
      for (let k = 0; k < N; k++) {
        const c = D[3 * k] >>> 24;
        KEY[k] = c ? c * 65536 + D[3 * k + 2] : NaN;
      }
      greedy(KEY, T, T, (i0, j0, w, h, kv) => {
        const c = Math.floor(kv / 65536), gen = kv - c * 65536;
        const x0 = X(bx + i0), x1 = X(bx + i0 + w), z0 = Z(bz + j0), z1 = Z(bz + j0 + h);
        q.quad([x0, c, z0], [x1, c, z0], [x1, c, z1], [x0, c, z1], RGB_NETHERRACK, SHADE.bottom, tinted(RGB_NETHERRACK, gen));
      });
    }
    // side walls, collected per plane: xp[p] = plane x = bx + p (along z), zp[p] = plane z = bz + p (along x)
    const xp = Array.from({ length: T + 1 }, () => []), zp = Array.from({ length: T + 1 }, () => []);
    for (let lz = 0; lz < T; lz++) {
      for (let lx = 0; lx < T; lx++) {
        const o = 3 * (lz * T + lx), v = D[o], rgb = D[o + 1], gen = D[o + 2];
        const top = v & 255, y1 = top + 1, cls = (v >> 16) & 255, ceil = v >>> 24;
        if (end) {
          // floating column [bottom, top]: walls where the neighbour column does not cover it
          if (cls === C.VOID) continue;
          const b = ceil;
          for (const [dx, dz] of DIRS) {
            const n = get(lx + dx, lz + dz);
            const list = dx !== 0 ? xp[dx === 1 ? lx + 1 : lx] : zp[dz === 1 ? lz + 1 : lz];
            const along = dx !== 0 ? lz : lx;
            const split = (v >> 8) & 255, lo = t.low ? t.low[lz * T + lx] : 0;
            const wall = (a, c) => {
              if (c <= a) return;
              if (lo && split > lo) {
                // two runs: [bottom, lo) end stone (tower island / rooms), [split, top] obsidian (spire / roof)
                if (Math.min(c, lo) > a) list.push(along, a, Math.min(c, lo), RGB_END_STONE, tinted(RGB_END_STONE, gen));
                if (c > Math.max(a, split)) list.push(along, Math.max(a, split), c, rgb, tinted(rgb, gen));
                return;
              }
              if (split) {
                // HEE tower column: obsidian from `split` up, the tower's end-stone island below
                const m = Math.max(a, Math.min(c, split));
                if (c > m) list.push(along, m, c, rgb, tinted(rgb, gen));
                if (m > a) list.push(along, a, m, RGB_END_STONE, tinted(RGB_END_STONE, gen));
                return;
              }
              // surface block on top (island biome colour), end stone below
              if (c === y1 && cls === C.END_SURFACE) {
                list.push(along, c - 1, c, rgb, tinted(rgb, gen));
                c -= 1;
              }
              if (c > a) list.push(along, a, c, RGB_END_STONE, tinted(RGB_END_STONE, gen));
            };
            if (n < 0 || ((n >> 16) & 255) === C.VOID) {
              wall(b, y1);
            } else {
              const nt1 = (n & 255) + 1, nb = n >>> 24;
              wall(Math.max(b, nt1), y1);
              wall(b, Math.min(y1, nb));
            }
          }
          continue;
        }
        for (const [dx, dz] of DIRS) {
          const n = get(lx + dx, lz + dz);
          const ny1 = n >= 0 ? (n & 255) + 1 : Math.max(0, y1 - 8);
          const list = dx !== 0 ? xp[dx === 1 ? lx + 1 : lx] : zp[dz === 1 ? lz + 1 : lz];
          const along = dx !== 0 ? lz : lx;
          const face = (a, b, col) => {
            if (b > a) list.push(along, a, b, col, tinted(col, gen));
          };
          if (ny1 < y1) {
            // overworld near tiles: exact layers below the top (wasm side code: block | run << 4 | sandstone << 7)
            const sc = t.low ? t.low[lz * T + lx] : 0;
            if (sc) {
              face(Math.max(ny1, y1 - 1), y1, rgb);
              const f = sc & 15, len = (sc >> 4) & 7;
              let y = y1 - 1;
              // a full clay run (mesa / canyon) is clay down to y 64 and below: bands to the wall's foot
              const a = f === SIDE_CLAY && len === 7 ? ny1 : Math.max(ny1, y - len);
              if (f === SIDE_CLAY) for (let yy = y - 1; yy >= a; yy--) face(yy, yy + 1, MESA_BANDS[((yy % MESA_BANDS.length) + MESA_BANDS.length) % MESA_BANDS.length]);
              else face(a, y, SIDE_RGB[f]);
              y = a;
              if (sc & 128 && y > ny1) {
                const b2 = Math.max(ny1, y - 2);
                face(b2, y, RGB_SANDSTONE);
                y = b2;
              }
              if (y > ny1) face(ny1, y, RGB_STONE);
            } else if (cls === C.CLAY) {
              for (let y = y1 - 1; y >= ny1; y--) face(y, y + 1, sideRgb(cls, rgb, y1 - 1 - y, y));
            } else {
              face(Math.max(ny1, y1 - 1), y1, rgb);
              if (y1 - 1 > ny1) {
                const s1 = Math.max(ny1, y1 - 4);
                face(s1, y1 - 1, sideRgb(cls, rgb, 1, 0));
                if (s1 > ny1) face(ny1, s1, sideRgb(cls, rgb, 9, 0));
              }
            }
          }
          if (ceil) {
            const nc = n >= 0 ? n >>> 24 || 128 : ceil + 8;
            if (nc > ceil) face(ceil, nc, RGB_NETHERRACK);
          }
        }
      }
    }
    planes(xp, (p, along0, w, ya, yb, col, bcol) => {
      const px = X(bx + p), z0 = Z(bz + along0), z1 = Z(bz + along0 + w);
      q.quad([px, ya, z0], [px, yb, z0], [px, yb, z1], [px, ya, z1], col, SHADE.x, bcol);
    });
    planes(zp, (p, along0, w, ya, yb, col, bcol) => {
      const pz = Z(bz + p), x0 = X(bx + along0), x1 = X(bx + along0 + w);
      q.quad([x0, ya, pz], [x0, yb, pz], [x1, yb, pz], [x1, ya, pz], col, SHADE.z, bcol);
    });
    const g = q.geometry();
    if (g) {
      if (tint) g.setAttribute('color', g.userData.cb);
      t.mesh = new THREE.Mesh(g, matNear());
      scene.add(t.mesh);
    }
    const wg = wq.geometry();
    if (wg) {
      t.water = new THREE.Mesh(wg, waterMat);
      t.water.renderOrder = 3;
      scene.add(t.water);
    }
    const dg = dq.geometry();
    if (dg) {
      t.dry = new THREE.Mesh(dg, dryMat);
      t.dry.renderOrder = 1;
      scene.add(t.dry);
    }
    t.tris = (q.n + wq.n + dq.n) * 2;
    tris += t.tris;
    dirty = true;
  }

  function disposeNear(key) {
    const t = near.get(key);
    if (!t) return;
    for (const m of [t.mesh, t.water, t.dry]) if (m) {
      scene.remove(m);
      m.geometry.dispose();
    }
    tris -= t.tris;
    near.delete(key);
    countGens(nearCounts, t.data, T, T, -1);
    resink(t.tx * T, t.tz * T, t.tx * T + T, t.tz * T + T);
  }

  // ---- heightfield meshes (mid + far)
  const LIGHT = new THREE.Vector3(0.45, 1, 0.3).normalize();
  function buildField(L, f, cols) {
    const N = L.N, W = N + 1, step = L.step, span = L.span;
    const base = new Float32Array(W * W);
    const pos = new Float32Array(W * W * 3), col = new Uint8Array(W * W * 3), bcol = new Uint8Array(W * W * 3);
    const gens = new Uint32Array(W * W);
    for (let k = 0; k < W * W; k++) {
      base[k] = end && ((cols[3 * k] >> 16) & 255) === C.VOID ? NaN : (cols[3 * k] & 255) + 1;
      gens[k] = cols[3 * k + 2];
    }
    const hb = (k, d) => (Number.isNaN(base[k]) ? d : base[k]);
    const n = new THREE.Vector3();
    for (let j = 0; j < W; j++) {
      for (let i = 0; i < W; i++) {
        const k = j * W + i;
        const b0 = hb(k, 0);
        const hx = hb(j * W + Math.min(W - 1, i + 1), b0) - hb(j * W + Math.max(0, i - 1), b0);
        const hz = hb(Math.min(W - 1, j + 1) * W + i, b0) - hb(Math.max(0, j - 1) * W + i, b0);
        n.set(-hx / (2 * step), 1, -hz / (2 * step)).normalize();
        const steep = 1 - n.y; // steep slopes show rock
        let rgb = cols[3 * k + 1];
        const cls = (cols[3 * k] >> 16) & 255;
        if (steep > 0.35 && (cls === C.GRASS || cls === C.SNOW || cls === C.DIRT)) rgb = RGB_STONE;
        const sh = 0.6 + 0.45 * Math.max(0, n.dot(LIGHT));
        const brgb = tinted(rgb, gens[k]);
        col[k * 3] = Math.min(255, ((rgb >> 16) & 255) * sh);
        col[k * 3 + 1] = Math.min(255, ((rgb >> 8) & 255) * sh);
        col[k * 3 + 2] = Math.min(255, (rgb & 255) * sh);
        bcol[k * 3] = Math.min(255, ((brgb >> 16) & 255) * sh);
        bcol[k * 3 + 1] = Math.min(255, ((brgb >> 8) & 255) * sh);
        bcol[k * 3 + 2] = Math.min(255, (brgb & 255) * sh);
        pos[k * 3] = f.fx * span + i * step - ox;
        pos[k * 3 + 1] = hb(k, -500);
        pos[k * 3 + 2] = f.fz * span + j * step - oz;
      }
    }
    let idx = new Uint32Array(N * N * 6);
    let o = 0;
    for (let j = 0; j < N; j++)
      for (let i = 0; i < N; i++) {
        const a = j * W + i;
        if (end && (Number.isNaN(base[a]) || Number.isNaN(base[a + 1]) || Number.isNaN(base[a + W]) || Number.isNaN(base[a + W + 1]))) continue;
        idx.set([a, a + W, a + 1, a + 1, a + W, a + W + 1], o);
        o += 6;
      }
    idx = idx.subarray(0, o);
    const g = new THREE.BufferGeometry();
    g.setAttribute('position', new THREE.BufferAttribute(pos, 3));
    const ca = new THREE.BufferAttribute(col, 3, true), cb = new THREE.BufferAttribute(bcol, 3, true);
    g.userData = { ca, cb };
    g.setAttribute('color', tint ? cb : ca);
    g.setIndex(new THREE.BufferAttribute(idx, 1));
    g.computeBoundingSphere();
    f.base = base;
    f.gens = gens;
    if (L === farL) countGensFar(gens, 1);
    f.mesh = new THREE.Mesh(g, matField(L));
    f.tris = o / 3;
    tris += f.tris;
    scene.add(f.mesh);
    resinkTile(L, f, -Infinity, -Infinity, Infinity, Infinity);
    if (L === midL) resinkLevel(farL, f.fx * L.span, f.fz * L.span, (f.fx + 1) * L.span, (f.fz + 1) * L.span);
    dirty = true;
  }
  function disposeField(L, key) {
    const f = L.map.get(key);
    if (!f) return;
    if (L === farL) countGensFar(f.gens, -1);
    scene.remove(f.mesh);
    f.mesh.geometry.dispose();
    tris -= f.tris;
    L.map.delete(key);
    if (L === midL) resinkLevel(farL, f.fx * L.span, f.fz * L.span, (f.fx + 1) * L.span, (f.fz + 1) * L.span);
  }

  /** Every tile of `map` (tile size `span`) touching the point is loaded (a point on a tile edge touches two). */
  function covers(map, span, x, z) {
    const txs = x % span === 0 ? [x / span - 1, x / span] : [Math.floor(x / span)];
    const tzs = z % span === 0 ? [z / span - 1, z / span] : [Math.floor(z / span)];
    for (const a of txs) for (const b of tzs) if (!map.has(`${a},${b}`)) return false;
    return true;
  }
  /** A heightfield vertex sinks out of sight under loaded finer tiles (near blocks; for the far level also mid). */
  function vertexCovered(L, x, z) {
    return covers(near, T, x, z) || (L === farL && midL != null && covers(midL.map, midL.span, x, z));
  }
  function resinkTile(L, f, x0, z0, x1, z1) {
    if (!f.mesh) return;
    const N = L.N, W = N + 1, span = L.span, step = L.step, pos = f.mesh.geometry.getAttribute('position');
    const i0 = Math.max(0, Math.floor((x0 - f.fx * span) / step)), i1 = Math.min(N, Math.ceil((x1 - f.fx * span) / step));
    const j0 = Math.max(0, Math.floor((z0 - f.fz * span) / step)), j1 = Math.min(N, Math.ceil((z1 - f.fz * span) / step));
    for (let j = j0; j <= j1; j++)
      for (let i = i0; i <= i1; i++) {
        const k = j * W + i;
        const bk = Number.isNaN(f.base[k]) ? -500 : f.base[k];
        pos.array[k * 3 + 1] = vertexCovered(L, f.fx * span + i * step, f.fz * span + j * step) ? bk - 80 : bk;
      }
    pos.needsUpdate = true;
    dirty = true;
  }
  function resinkLevel(L, x0, z0, x1, z1) {
    for (const f of L.map.values()) {
      if (x1 < f.fx * L.span || x0 > (f.fx + 1) * L.span || z1 < f.fz * L.span || z0 > (f.fz + 1) * L.span) continue;
      resinkTile(L, f, x0, z0, x1, z1);
    }
  }
  /** Near tile loaded / dropped: both heightfield levels re-sink there. */
  function resink(x0, z0, x1, z1) {
    for (const L of levels) resinkLevel(L, x0, z0, x1, z1);
  }

  // ---- loading
  const req = (msg, priority, key) =>
    pool.request({ type: 'viewCols', seed, worldType, dim: reqDim, ...msg }, { priority, key, ns: NS });

  function wantLevel(L) {
    const cx = st.x, cz = st.z;
    if (L.centre && Math.hypot(cx - L.centre.x, cz - L.centre.z) < Math.min(L.R / 3, L.span / 2)) return;
    L.centre = { x: cx, z: cz };
    const want = new Set();
    const a0 = Math.floor((cx - L.R) / L.span), a1 = Math.floor((cx + L.R) / L.span);
    const b0 = Math.floor((cz - L.R) / L.span), b1 = Math.floor((cz + L.R) / L.span);
    for (let fz = b0; fz <= b1; fz++)
      for (let fx = a0; fx <= a1; fx++) {
        const dx = Math.max(fx * L.span - cx, 0, cx - (fx + 1) * L.span), dz = Math.max(fz * L.span - cz, 0, cz - (fz + 1) * L.span);
        if (Math.hypot(dx, dz) > L.R) continue;
        // tiles fully inside the inner circle (covered by finer terrain) are not needed
        const ex = Math.max(Math.abs(fx * L.span - cx), Math.abs((fx + 1) * L.span - cx)), ez = Math.max(Math.abs(fz * L.span - cz), Math.abs((fz + 1) * L.span - cz));
        if (Math.hypot(ex, ez) < L.inner) continue;
        const key = `${fx},${fz}`;
        want.add(key);
        if (L.map.has(key) || L.pending.has(key)) continue;
        L.pending.add(key);
        req({ x0: fx * L.span, z0: fz * L.span, w: L.N + 1, h: L.N + 1, step: L.reqStep }, L.prio + Math.hypot(dx, dz) / (L === farL ? 64 : 256), `${L.name}|${key}`)
          .then((r) => {
            if (!L.pending.delete(key) || closed || L.map.has(key)) return;
            const f = { fx, fz, base: null, gens: null, mesh: null, tris: 0 };
            L.map.set(key, f);
            for (let k = 1; k < r.cols.length; k += 3) r.cols[k] &= 0xffffff; // End: tower gap byte
            buildField(L, f, r.cols);
            placeMarkers();
            if (!st.groundKnown && !st.fly) settle(true);
          })
          .catch(() => L.pending.delete(key));
      }
    pool.prune((k) => !k.startsWith(`${L.name}|`) || want.has(k.slice(L.name.length + 1)), NS);
    for (const k of [...L.pending]) if (!want.has(k)) L.pending.delete(k);
    for (const key of [...L.map.keys()]) if (!want.has(key)) disposeField(L, key);
  }
  const wantFar = () => {
    for (const L of levels) wantLevel(L);
  };

  let nearTotal = 0, failed = 0;
  function wantNear() {
    const cx = st.x, cz = st.z;
    const want = new Set();
    const t0x = Math.floor((cx - nearR) / T), t1x = Math.floor((cx + nearR) / T);
    const t0z = Math.floor((cz - nearR) / T), t1z = Math.floor((cz + nearR) / T);
    const jobs = [];
    for (let tz = t0z; tz <= t1z; tz++)
      for (let tx = t0x; tx <= t1x; tx++) {
        const d = Math.hypot(tx * T + T / 2 - cx, tz * T + T / 2 - cz);
        if (d > nearR + T / 2) continue;
        const key = `${tx},${tz}`;
        want.add(key);
        if (near.has(key) || nearPending.has(key)) continue;
        jobs.push({ tx, tz, key, d });
      }
    nearTotal = want.size;
    pool.prune((k) => !k.startsWith('near|') || want.has(k.slice(5)), NS);
    for (const k of [...nearPending]) if (!want.has(k)) nearPending.delete(k);
    for (const j of jobs) {
      if (capped || failed > 20) break;
      nearPending.add(j.key);
      // the tile under the player first, then the far terrain, then outwards
      req({ x0: j.tx * T, z0: j.tz * T, w: T, h: T, step: 1 }, j.d < T ? -1000 : -800 + j.d / 16, `near|${j.key}`)
        .then((r) => {
          if (!nearPending.delete(j.key) || closed || near.has(j.key)) return;
          if (tris > triBudget) {
            capped = true;
            return;
          }
          // End: rgb top byte = top + 1 of the part below a gap under the top run (tower spires / roof)
          let low = null;
          for (let k = 1; k < r.cols.length; k += 3) {
            const lo = r.cols[k] >>> 24;
            if (lo) (low ??= new Uint8Array(r.cols.length / 3))[(k - 1) / 3] = lo;
            r.cols[k] &= 0xffffff;
          }
          const t = { tx: j.tx, tz: j.tz, data: r.cols, low, mesh: null, water: null, tris: 0 };
          near.set(j.key, t);
          countGens(nearCounts, t.data, T, T, 1);
          buildNear(t);
          for (const [dx, dz] of [[1, 0], [-1, 0], [0, 1], [0, -1]]) {
            const nb = near.get(`${j.tx + dx},${j.tz + dz}`);
            if (nb) buildNear(nb);
          }
          resink(j.tx * T, j.tz * T, j.tx * T + T, j.tz * T + T);
          if (!st.fly) settle(!st.groundKnown);
          else if (end && !st.groundKnown && colAt(Math.floor(st.x), Math.floor(st.z)) >= 0) {
            // End (flying by default): start a few blocks above the land under the start point, if any
            const g = groundAt(st.x, st.z);
            if (g != null) st.y = g + 6;
            st.groundKnown = true;
            dirty = true;
          }
          if (fullMs == null && loadedNear() >= nearTotal) fullMs = performance.now() - t0;
        })
        .catch((e) => {
          nearPending.delete(j.key);
          if (e?.message !== 'cancelled') {
            failed++;
            console.warn('street view: tile failed', j.key, e?.message);
            if (!closed) setTimeout(() => !closed && wantNear(), 500);
          }
        });
    }
    // drop tiles well outside the radius
    for (const [key, t] of near) if (Math.hypot(t.tx * T + T / 2 - cx, t.tz * T + T / 2 - cz) > nearR + 3 * T) disposeNear(key);
  }
  const loadedNear = () => {
    let n = 0;
    for (const t of near.values()) if (Math.hypot(t.tx * T + T / 2 - st.x, t.tz * T + T / 2 - st.z) <= nearR + T / 2) n++;
    return n;
  };

  /** Put the eye on the ground (walk mode). `snap`: jump instead of easing. */
  function settle(snap) {
    const g = groundAt(st.x, st.z);
    if (g == null) return;
    const exact = colAt(Math.floor(st.x), Math.floor(st.z)) >= 0;
    const target = g + EYE;
    st.y = snap ? target : st.y + (target - st.y) * 0.35;
    if (exact) st.groundKnown = true;
    dirty = true;
  }

  // ---- markers (pillars + labels with a live distance), nearest MARKERS_PER_GROUP of each group
  const markerObjs = [];
  const beamGeo = new THREE.BoxGeometry(0.7, 260, 0.7);
  let groupsOn = new Set(opts.groups ?? ['pins', 'structures', 'thaum']);
  let markerList = opts.markers ?? [];
  let labelAt = { x: NaN, z: NaN };
  function buildMarkers() {
    for (const o of markerObjs) {
      scene.remove(o.beam, o.label);
      o.beam.material.dispose();
      o.label.material.map.dispose();
      o.label.material.dispose();
    }
    markerObjs.length = 0;
    const byGroup = new Map();
    for (const m of markerList) {
      if (!m.label) continue; // box-only (x-ray) entries
      const d = Math.hypot(m.x + 0.5 - st.x, m.z + 0.5 - st.z);
      if (d > farR) continue;
      const g = m.group ?? 'structures';
      if (!byGroup.has(g)) byGroup.set(g, []);
      byGroup.get(g).push([d, m]);
    }
    for (const [g, list] of byGroup)
      for (const [, m] of list.sort((a, b) => a[0] - b[0]).slice(0, MARKERS_PER_GROUP)) {
        const color = m.color ?? '#ffe08a';
        const beam = new THREE.Mesh(beamGeo, new THREE.MeshBasicMaterial({ color: cssHex(color), transparent: true, opacity: 0.55, fog: false, depthWrite: false }));
        beam.position.set(m.x + 0.5 - ox, 130, m.z + 0.5 - oz);
        beam.renderOrder = 4;
        const label = labelSprite(m.label, color);
        label.position.set(m.x + 0.5 - ox, 100, m.z + 0.5 - oz);
        beam.visible = label.visible = groupsOn.has(g);
        const o = { m, group: g, beam, label, text: m.label, color };
        beam.userData.marker = label.userData.marker = o;
        scene.add(beam, label);
        markerObjs.push(o);
      }
    labelAt = { x: NaN, z: NaN };
    updateLabels();
    placeMarkers();
  }
  function updateLabels() {
    if (Math.hypot(st.x - labelAt.x, st.z - labelAt.z) < 1) return;
    labelAt = { x: st.x, z: st.z };
    for (const o of markerObjs) {
      if (!groupsOn.has(o.group)) continue;
      const text = `${o.m.label} · ${fmtBlocks(Math.hypot(o.m.x + 0.5 - st.x, o.m.z + 0.5 - st.z))}`;
      if (text === o.text) continue;
      o.text = text;
      setLabel(o.label, text, o.color);
      dirty = true;
    }
  }
  // labels: nearest first, a label whose screen box overlaps one already shown is hidden (its pillar stays)
  const _v = new THREE.Vector3();
  function declutter() {
    if (!markerObjs.length) return;
    camera.updateMatrixWorld();
    const W = renderer.domElement.clientWidth || 1, H = renderer.domElement.clientHeight || 1;
    const k = H / 2 / Math.tan((camera.fov * Math.PI) / 360);
    const placed = [];
    const list = markerObjs
      .filter((o) => groupsOn.has(o.group))
      .map((o) => [Math.hypot(o.m.x + 0.5 - st.x, o.m.z + 0.5 - st.z), o])
      .sort((a, b) => a[0] - b[0]);
    for (const [, o] of list) {
      _v.copy(o.label.position).project(camera);
      let show = _v.z < 1 && Math.abs(_v.x) < 1.2 && Math.abs(_v.y) < 1.2;
      if (show) {
        const cx = ((_v.x + 1) / 2) * W, cy = ((1 - _v.y) / 2) * H;
        const hw = (o.label.scale.x * k) / 2 + 3, hh = (o.label.scale.y * k) / 2 + 2;
        const r = [cx - hw, cy - hh, cx + hw, cy + hh];
        show = !placed.some((q) => r[0] < q[2] && r[2] > q[0] && r[1] < q[3] && r[3] > q[1]);
        if (show) placed.push(r);
      }
      o.label.visible = show;
    }
  }
  function placeMarkers() {
    for (const o of markerObjs) {
      const g = groundAt(o.m.x + 0.5, o.m.z + 0.5);
      if (g == null) continue;
      o.beam.position.y = g + 130;
      o.label.position.y = g + (nether ? 6 : 14);
    }
    dirty = true;
  }
  function markerInfo(o) {
    const dx = o.m.x + 0.5 - st.x, dz = o.m.z + 0.5 - st.z;
    const bearing = ((Math.atan2(dx, -dz) * 180) / Math.PI + 360) % 360;
    const rel = ((bearing - st.heading + 540) % 360) - 180;
    const g = groundAt(o.m.x + 0.5, o.m.z + 0.5);
    return {
      x: o.m.x, z: o.m.z, y: o.m.y ?? null, label: o.m.label, detail: o.m.detail ?? null, color: o.color, group: o.group,
      dist: Math.hypot(dx, dz), dist3: o.m.y != null ? Math.hypot(dx, dz, o.m.y - (st.y - EYE)) : null,
      bearing, rel, groundY: g != null ? Math.floor(g) : null, dy: g != null ? Math.floor(g) - Math.floor(st.y - EYE) : null,
    };
  }
  const ray = new THREE.Raycaster();
  const ndc = new THREE.Vector2();
  /** The marker label / pillar under a screen point, or null. */
  function pickAt(clientX, clientY) {
    const r = el.getBoundingClientRect();
    ndc.set(((clientX - r.left) / r.width) * 2 - 1, -((clientY - r.top) / r.height) * 2 + 1);
    camera.updateMatrixWorld();
    ray.setFromCamera(ndc, camera);
    const objs = [];
    for (const o of markerObjs) if (o.label.visible) objs.push(o.label, o.beam);
    const hit = ray.intersectObjects(objs, false)[0];
    if (hit) return hit.object.userData.marker;
    // thin far pillars: nearest within 10 px on screen
    let best = null, bd = 10;
    const v = new THREE.Vector3();
    for (const o of markerObjs) {
      if (!o.beam.visible) continue;
      for (const y of [o.label.position.y, o.label.position.y + 40]) {
        v.set(o.beam.position.x, y, o.beam.position.z).project(camera);
        if (v.z > 1) continue;
        const sx = ((v.x + 1) / 2) * r.width + r.left, sy = ((1 - v.y) / 2) * r.height + r.top;
        const d = Math.hypot(sx - clientX, sy - clientY);
        if (d < bd) (bd = d), (best = o);
      }
    }
    return best;
  }
  let picked = null, pickedPinned = false;
  function sendPick(o, cx, cy, pinned) {
    if (o === picked && !pinned && !pickedPinned) {
      if (o) opts.onPick?.({ ...markerInfo(o), sx: cx, sy: cy, pinned: false });
      return;
    }
    picked = o;
    pickedPinned = !!(o && pinned);
    opts.onPick?.(o ? { ...markerInfo(o), sx: cx, sy: cy, pinned: pickedPinned } : null);
  }
  buildMarkers();

  // ---- block pick (long press / right click): the terrain block under the pointer, outlined
  const selBox = new THREE.LineSegments(
    new THREE.EdgesGeometry(new THREE.BoxGeometry(1.03, 1.03, 1.03)),
    new THREE.LineBasicMaterial({ color: 0xffe08a, depthTest: false, transparent: true, fog: false }),
  );
  selBox.visible = false;
  selBox.renderOrder = 20;
  scene.add(selBox);
  let selBlock = null;
  /** World block hit by the ray through a screen point: { x, y, z, approx, dist } (approx = a heightfield ring,
   *  not the exact block tiles) or null. */
  function blockAt(clientX, clientY) {
    const r = el.getBoundingClientRect();
    ndc.set(((clientX - r.left) / r.width) * 2 - 1, -((clientY - r.top) / r.height) * 2 + 1);
    camera.updateMatrixWorld();
    ray.setFromCamera(ndc, camera);
    const objs = [];
    for (const t of near.values()) if (t.mesh) objs.push(t.mesh);
    for (const L of levels) for (const f of L.map.values()) if (f.mesh) objs.push(f.mesh);
    const hit = ray.intersectObjects(objs, false)[0];
    if (!hit) return null;
    // step just inside the face that was hit
    const p = hit.point.clone().addScaledVector(ray.ray.direction, 0.02);
    const nearHit = [...near.values()].some((t) => t.mesh === hit.object);
    return { x: Math.floor(p.x) + ox, y: Math.floor(p.y), z: Math.floor(p.z) + oz, approx: !nearHit, dist: hit.distance };
  }
  function selectBlock(b, sx, sy) {
    selBlock = b;
    selBox.visible = !!b;
    if (b) selBox.position.set(b.x - ox + 0.5, b.y + 0.5, b.z - oz + 0.5);
    dirty = true;
    opts.onBlock?.(b ? { ...b, sx, sy } : null);
  }

  // ---- x-ray: see-through terrain with cave air (exact carve model), structure piece boxes and ore veins
  const XR_CAVE_H = 192;
  const caveR = phone ? 3 : 6; // chunks
  const xr = { on: false, caves: true, pop: true, structures: true, ores: true, rooms: true, ...(opts.xray ?? {}) };
  const xrGroup = new THREE.Group();
  xrGroup.visible = false;
  scene.add(xrGroup);
  const caveMat = new THREE.MeshBasicMaterial({ vertexColors: true, side: THREE.DoubleSide, fog: false });
  const boxMat = new THREE.MeshBasicMaterial({ vertexColors: true, transparent: true, opacity: 0.38, depthWrite: false, side: THREE.DoubleSide, fog: false });
  const edgeMat = new THREE.LineBasicMaterial({ vertexColors: true, fog: false });
  const caves = new Map(); // "cx,cz" -> { cx, cz, air: Uint8Array(16*16*H), mesh }
  const cavePending = new Set();
  let boxObjs = null, boxAt = null, boxSeq = 0, rustBoxes = [];
  const XR_COLORS = {
    1: 0xc8a060, 2: 0xa07040, 3: 0xd8b878, 4: 0xb08850, // mineshaft corridor / room / crossing / stairs
    10: 0xa8a8a8, 11: 0xb388ff, // stronghold piece / portal room
    20: 0xb03a3a, 21: 0x7a2424, 22: 0xffd54f, 23: 0xe53935, // fortress bridge / castle / spawner / wart
  };
  const xrPost = { on: [], off: [] };
  function setTerrainXray(on) {
    for (const m of [nearMat, midMat, farMat, hNearMat, hFieldMat]) {
      m.transparent = on;
      if (m.uniforms) m.uniforms.opacity.value = on ? 0.14 : 1;
      else m.opacity = on ? 0.14 : 1;
      m.depthWrite = !on;
      m.needsUpdate = true;
    }
    if (waterMat) {
      waterMat.opacity = on ? 0.12 : 0.62;
      waterMat.needsUpdate = true;
      if (seaMat) {
        seaMat.opacity = waterMat.opacity;
        seaMat.needsUpdate = true;
      }
    }
    if (roof) roof.visible = !on;
    xrGroup.visible = on;
    dirty = true;
  }
  const airIdx = (lx, y, lz) => (y * 16 + lz) * 16 + lx;
  /** Cave air at (x, y, z) world; unloaded neighbour chunks count as air (no false walls at their edge). */
  function caveAir(c, lx, y, lz) {
    if (y < 0 || y >= XR_CAVE_H) return false;
    if (lx >= 0 && lx < 16 && lz >= 0 && lz < 16) return c.air[airIdx(lx, y, lz)] === 1;
    const n = caves.get(`${c.cx + Math.floor(lx / 16)},${c.cz + Math.floor(lz / 16)}`);
    if (!n) return true;
    return n.air[airIdx(((lx % 16) + 16) % 16, y, ((lz % 16) + 16) % 16)] === 1;
  }
  function buildCave(c) {
    if (c.mesh) {
      xrGroup.remove(c.mesh);
      c.mesh.geometry.dispose();
      c.mesh = null;
    }
    const q = new Quads();
    const bx = c.cx * 16, bz = c.cz * 16;
    const colOf = (y) => (y <= 10 ? 0xff7a1a : 0xb9c6cf);
    const H = XR_CAVE_H;
    let ymin = H, ymax = -1;
    for (let y = 0; y < H; y++)
      for (let k = 0; k < 256; k++)
        if (c.air[y * 256 + k]) {
          if (y < ymin) ymin = y;
          if (y > ymax) ymax = y;
          break;
        }
    if (ymax < 0) return;
    const key = new Float64Array(16 * H);
    // floors / ceilings: plane y between cells y-1 and y
    for (let y = ymin; y <= ymax + 1; y++) {
      let any = false;
      for (let lz = 0; lz < 16; lz++)
        for (let lx = 0; lx < 16; lx++) {
          const a = caveAir(c, lx, y, lz), b = caveAir(c, lx, y - 1, lz);
          const k = lz * 16 + lx;
          key[k] = a && !b ? 1 : !a && b ? 2 : NaN; // 1 = floor under air, 2 = ceiling over air
          any ||= a !== b;
        }
      if (!any) continue;
      greedy(key, 16, 16, (i0, j0, w, h, kv) => {
        const yy = y, x0 = X(bx + i0), x1 = X(bx + i0 + w), z0 = Z(bz + j0), z1 = Z(bz + j0 + h);
        const rgb = colOf(kv === 1 ? y : y - 1);
        q.quad([x0, yy, z0], [x1, yy, z0], [x1, yy, z1], [x0, yy, z1], rgb, kv === 1 ? 0.95 : 0.55);
      });
    }
    // walls: x planes (along z × y) and z planes (along x × y)
    for (const ax of [0, 1]) {
      for (let p = 0; p <= 16; p++) {
        let any = false;
        for (let y = ymin; y <= ymax; y++)
          for (let t = 0; t < 16; t++) {
            const [lxA, lzA, lxB, lzB] = ax === 0 ? [p, t, p - 1, t] : [t, p, t, p - 1];
            // only this chunk's own cells make walls (the neighbour builds its side)
            const ownA = lxA >= 0 && lxA < 16 && lzA >= 0 && lzA < 16, ownB = lxB >= 0 && lxB < 16 && lzB >= 0 && lzB < 16;
            const a = caveAir(c, lxA, y, lzA), b = caveAir(c, lxB, y, lzB);
            const k = (y - ymin) * 16 + t;
            key[k] = a && !b && ownA ? colOf(y) : b && !a && ownB ? colOf(y) : NaN;
            any ||= key[k] === key[k];
          }
        if (!any) continue;
        greedy(key, 16, ymax - ymin + 1, (i0, j0, w, h, rgb) => {
          const ya = ymin + j0, yb = ya + h;
          if (ax === 0) {
            const px = X(bx + p), z0 = Z(bz + i0), z1 = Z(bz + i0 + w);
            q.quad([px, ya, z0], [px, yb, z0], [px, yb, z1], [px, ya, z1], rgb, 0.8);
          } else {
            const pz = Z(bz + p), x0 = X(bx + i0), x1 = X(bx + i0 + w);
            q.quad([x0, ya, pz], [x0, yb, pz], [x1, yb, pz], [x1, ya, pz], rgb, 0.68);
          }
        });
      }
    }
    const g = q.geometry();
    if (g) {
      c.mesh = new THREE.Mesh(g, caveMat);
      xrGroup.add(c.mesh);
    }
    dirty = true;
  }
  function wantCaves() {
    if (!xr.on || !xr.caves || nether || end) return;
    const pcx = Math.floor(st.x / 16), pcz = Math.floor(st.z / 16);
    const want = new Set();
    const jobs = [];
    for (let dz = -caveR; dz <= caveR; dz++)
      for (let dx = -caveR; dx <= caveR; dx++) {
        if (dx * dx + dz * dz > caveR * caveR + 1) continue;
        const key = `${pcx + dx},${pcz + dz}`;
        want.add(key);
        if (!caves.has(key) && !cavePending.has(key)) jobs.push([pcx + dx, pcz + dz, key, Math.hypot(dx, dz)]);
      }
    pool.prune((k) => !k.startsWith('cave|') || want.has(k.slice(5)), NS);
    for (const k of [...cavePending]) if (!want.has(k)) cavePending.delete(k);
    for (const [cx, cz, key, d] of jobs) {
      cavePending.add(key);
      pool
        .request({ type: 'xrayCaves', seed, worldType, dim, cx, cz }, { priority: -600 + d, key: `cave|${key}`, ns: NS })
        .then((r) => {
          if (!cavePending.delete(key) || closed || caves.has(key)) return;
          const air = new Uint8Array(16 * 16 * XR_CAVE_H);
          for (let i = 0; i < air.length; i++) air[i] = (r.bits[i >> 3] >> (i & 7)) & 1;
          const c = { cx, cz, air, mesh: null };
          caves.set(key, c);
          buildCave(c);
          for (const [ddx, ddz] of DIRS) {
            const n = caves.get(`${cx + ddx},${cz + ddz}`);
            if (n) buildCave(n);
          }
        })
        .catch(() => cavePending.delete(key));
    }
    for (const [key, c] of caves)
      if (Math.hypot(c.cx - pcx, c.cz - pcz) > caveR + 3) {
        if (c.mesh) {
          xrGroup.remove(c.mesh);
          c.mesh.geometry.dispose();
        }
        caves.delete(key);
      }
  }
  function showCaves(on) {
    for (const c of caves.values()) if (c.mesh) c.mesh.visible = on;
  }

  // ---- x-ray population (overworld): RWG2 populate prefix per chunk, codes per block (wasm `xray_pop`)
  // 1 water, 2 lava, 3 air, 4 dirt, 5 gravel, 6 abyssal stone, 7 cobblestone, 8 emerald ore
  const POP_COLORS = [0, 0x3f76e4, 0xff5a00, 0xd6f3ff, 0x8b5a2b, 0x9a948c, 0x4a4560, 0x8a8a8a, 0x17dd62, 0x5f7f4f, 0xb8860b, 0x2f2f3a];
  const pops = new Map(); // "cx,cz" -> { cx, cz, codes, mesh }
  const popPending = new Set();
  function buildPop(c) {
    if (c.mesh) {
      xrGroup.remove(c.mesh);
      c.mesh.geometry.dispose();
      c.mesh = null;
    }
    const H = XR_CAVE_H, codes = c.codes;
    const at = (lx, y, lz) => (lx < 0 || lx > 15 || lz < 0 || lz > 15 || y < 0 || y >= H ? 0 : codes[(y * 16 + lz) * 16 + lx]);
    let ymin = H, ymax = -1;
    for (let y = 0; y < H; y++)
      for (let k = 0; k < 256; k++)
        if (codes[y * 256 + k]) {
          if (y < ymin) ymin = y;
          if (y > ymax) ymax = y;
          break;
        }
    if (ymax < 0) return;
    const q = new Quads();
    const bx = c.cx * 16, bz = c.cz * 16;
    const key = new Float64Array(16 * H);
    // a face where the code changes, coloured by the side that has a code (air faces only towards solid)
    const face = (a, b) => (a && a !== b ? a : b && b !== a ? b : 0);
    for (let y = ymin; y <= ymax + 1; y++) {
      let any = false;
      for (let lz = 0; lz < 16; lz++)
        for (let lx = 0; lx < 16; lx++) {
          const f = face(at(lx, y, lz), at(lx, y - 1, lz));
          key[lz * 16 + lx] = f ? f : NaN;
          any ||= f !== 0;
        }
      if (!any) continue;
      greedy(key, 16, 16, (i0, j0, w, h, kv) => {
        const x0 = X(bx + i0), x1 = X(bx + i0 + w), z0 = Z(bz + j0), z1 = Z(bz + j0 + h);
        q.quad([x0, y, z0], [x1, y, z0], [x1, y, z1], [x0, y, z1], POP_COLORS[kv], 0.95);
      });
    }
    for (const ax of [0, 1]) {
      for (let p = 0; p <= 16; p++) {
        let any = false;
        for (let y = ymin; y <= ymax; y++)
          for (let t = 0; t < 16; t++) {
            const f = ax === 0 ? face(at(p, y, t), at(p - 1, y, t)) : face(at(t, y, p), at(t, y, p - 1));
            key[(y - ymin) * 16 + t] = f ? f : NaN;
            any ||= f !== 0;
          }
        if (!any) continue;
        greedy(key, 16, ymax - ymin + 1, (i0, j0, w, h, kv) => {
          const ya = ymin + j0, yb = ya + h, rgb = POP_COLORS[kv];
          if (ax === 0) {
            const px = X(bx + p), z0 = Z(bz + i0), z1 = Z(bz + i0 + w);
            q.quad([px, ya, z0], [px, yb, z0], [px, yb, z1], [px, ya, z1], rgb, 0.8);
          } else {
            const pz = Z(bz + p), x0 = X(bx + i0), x1 = X(bx + i0 + w);
            q.quad([x0, ya, pz], [x0, yb, pz], [x1, yb, pz], [x1, ya, pz], rgb, 0.68);
          }
        });
      }
    }
    const g = q.geometry();
    if (g) {
      c.mesh = new THREE.Mesh(g, caveMat);
      c.mesh.visible = xr.pop;
      xrGroup.add(c.mesh);
    }
    dirty = true;
  }
  function wantPops() {
    if (!xr.on || !xr.pop || dim) return;
    const pcx = Math.floor(st.x / 16), pcz = Math.floor(st.z / 16);
    const want = new Set();
    const jobs = [];
    for (let dz = -caveR; dz <= caveR; dz++)
      for (let dx = -caveR; dx <= caveR; dx++) {
        if (dx * dx + dz * dz > caveR * caveR + 1) continue;
        const key = `${pcx + dx},${pcz + dz}`;
        want.add(key);
        if (!pops.has(key) && !popPending.has(key)) jobs.push([pcx + dx, pcz + dz, key, Math.hypot(dx, dz)]);
      }
    pool.prune((k) => !k.startsWith('pop|') || want.has(k.slice(4)), NS);
    for (const k of [...popPending]) if (!want.has(k)) popPending.delete(k);
    for (const [cx, cz, key, d] of jobs) {
      popPending.add(key);
      pool
        .request({ type: 'xrayPop', seed, worldType, cx, cz }, { priority: -590 + d, key: `pop|${key}`, ns: NS })
        .then((r) => {
          if (!popPending.delete(key) || closed || pops.has(key)) return;
          const c = { cx, cz, codes: r.codes, mesh: null };
          pops.set(key, c);
          buildPop(c);
        })
        .catch(() => popPending.delete(key));
    }
    for (const [key, c] of pops)
      if (Math.hypot(c.cx - pcx, c.cz - pcz) > caveR + 3) {
        if (c.mesh) {
          xrGroup.remove(c.mesh);
          c.mesh.geometry.dispose();
        }
        pops.delete(key);
      }
  }
  function showPops(on) {
    for (const c of pops.values()) if (c.mesh) c.mesh.visible = on;
  }
  // ---- decoration (overworld): trees, plants, boulders, canopy snow per chunk (wasm `deco_chunk`); codes index the
  // palette from `deco_palette` (r, g, b, kind: 1 cube, 2 leaves, 3 plant, 4 thin layer)
  let trees = opts.trees !== false;
  const decoR = phone ? 4 : 7; // chunks
  const decos = new Map(); // "cx,cz" -> { cx, cz, mesh }
  const decoPending = new Set();
  let decoPal = null;
  const decoGroup = new THREE.Group();
  scene.add(decoGroup);
  decoGroup.visible = trees;
  function buildDeco(c, y0, H, codes) {
    const pal = decoPal;
    const kind = (k) => (k ? pal[k * 4 + 3] : 0);
    const rgbOf = (k) => (pal[k * 4] << 16) | (pal[k * 4 + 1] << 8) | pal[k * 4 + 2];
    const at = (lx, y, lz) => (lx < 0 || lx > 15 || lz < 0 || lz > 15 || y < 0 || y >= H ? 0 : codes[(y * 16 + lz) * 16 + lx]);
    // a cube face is hidden by a neighbouring cube or leaves block (leaves show their faces to each other only from outside)
    const hides = (k) => {
      const t = kind(k);
      return t === 1 || t === 2;
    };
    const q = new Quads();
    const bx = c.cx * 16, bz = c.cz * 16;
    for (let y = 0; y < H; y++)
      for (let lz = 0; lz < 16; lz++)
        for (let lx = 0; lx < 16; lx++) {
          const k = codes[(y * 16 + lz) * 16 + lx];
          if (!k) continue;
          const t = kind(k), rgb = rgbOf(k);
          const x0 = X(bx + lx), x1 = x0 + 1, z0 = Z(bz + lz), z1 = z0 + 1, ya = y0 + y, yb = ya + 1;
          if (t === 1 || t === 2) {
            if (!hides(at(lx, y + 1, lz))) q.quad([x0, yb, z0], [x1, yb, z0], [x1, yb, z1], [x0, yb, z1], rgb, SHADE.top);
            if (!hides(at(lx, y - 1, lz))) q.quad([x0, ya, z0], [x1, ya, z0], [x1, ya, z1], [x0, ya, z1], rgb, SHADE.bottom);
            if (!hides(at(lx - 1, y, lz))) q.quad([x0, ya, z0], [x0, yb, z0], [x0, yb, z1], [x0, ya, z1], rgb, SHADE.x);
            if (!hides(at(lx + 1, y, lz))) q.quad([x1, ya, z0], [x1, yb, z0], [x1, yb, z1], [x1, ya, z1], rgb, SHADE.x);
            if (!hides(at(lx, y, lz - 1))) q.quad([x0, ya, z0], [x0, yb, z0], [x1, yb, z0], [x1, ya, z0], rgb, SHADE.z);
            if (!hides(at(lx, y, lz + 1))) q.quad([x0, ya, z1], [x0, yb, z1], [x1, yb, z1], [x1, ya, z1], rgb, SHADE.z);
          } else if (t === 3) {
            const a = 0.2, b = 0.8, top = ya + 0.7;
            q.quad([x0 + a, ya, z0 + a], [x0 + a, top, z0 + a], [x0 + b, top, z0 + b], [x0 + b, ya, z0 + b], rgb, 0.9);
            q.quad([x0 + a, ya, z0 + b], [x0 + a, top, z0 + b], [x0 + b, top, z0 + a], [x0 + b, ya, z0 + a], rgb, 0.8);
          } else if (t === 4) {
            const yt = ya + 0.125;
            q.quad([x0, yt, z0], [x1, yt, z0], [x1, yt, z1], [x0, yt, z1], rgb, SHADE.top);
          }
        }
    const g = q.geometry();
    if (g) {
      c.mesh = new THREE.Mesh(g, nearMat);
      decoGroup.add(c.mesh);
      dirty = true;
    }
  }
  /** Mesh for chunk (cx, cz) from a `deco_chunk` record (replaces an existing one). */
  function placeDeco(cx, cz, key, b) {
    const old = decos.get(key);
    if (old?.mesh) {
      decoGroup.remove(old.mesh);
      old.mesh.geometry.dispose();
      dirty = true;
    }
    const c = { cx, cz, mesh: null };
    decos.set(key, c);
    if (b.length >= 4) {
      const y0 = (b[0] | (b[1] << 8)) << 16 >> 16, H = b[2] | (b[3] << 8);
      if (H) buildDeco(c, y0, H, b.subarray(4));
    }
  }
  /** Spawn-area chunks in the real load order, once the camera is near spawn: requested once per seed (~4–10 s on
   * one worker); on arrival the canonical-order meshes inside the square are rebuilt. Map or null. */
  let spawnVoxHooked = null; // the spawnVox.pending this view listens to
  function wantSpawnVox() {
    const key = `${seed}|${worldType}`;
    if (spawnVox?.key === key && spawnVox.chunks) return spawnVox.chunks;
    if (spawnVox?.key !== key) {
      if (!spawn || Math.max(Math.abs(st.x - spawn.x), Math.abs(st.z - spawn.z)) > 192 + 16 * (decoR + 4)) return null;
      // no view namespace: survives this view closing, so a reopen does not pay the replay again
      const pending = pool.request({ type: 'spawnDeco3d', seed, worldType }, { priority: -690 }).then((r) => {
        const b = r.buf;
        const [cx0, cz0, n] = new Int32Array(b.buffer, b.byteOffset, 4);
        const chunks = new Map();
        let o = 16;
        for (let i = 0; i < n * n && o + 4 <= b.length; i++) {
          const len = 4 + 256 * (b[o + 2] | (b[o + 3] << 8));
          chunks.set(`${cx0 + (i % n)},${cz0 + Math.floor(i / n)}`, b.subarray(o, o + len));
          o += len;
        }
        if (spawnVox?.pending === pending) spawnVox = { key, chunks };
        return chunks;
      });
      pending.catch(() => {
        if (spawnVox?.pending === pending) spawnVox = null;
      });
      spawnVox = { key, pending };
    }
    if (spawnVoxHooked !== spawnVox.pending) {
      spawnVoxHooked = spawnVox.pending;
      spawnVoxHooked
        .then((chunks) => {
          if (closed) return;
          for (const [k, c] of [...decos]) {
            const rec = chunks.get(k);
            if (rec) placeDeco(c.cx, c.cz, k, rec);
          }
          wantDeco();
        })
        .catch(() => {});
    }
    return null;
  }
  function wantDeco() {
    if (dim || !trees) return;
    if (!decoPal) {
      if (!decoPending.has('pal')) {
        decoPending.add('pal');
        pool
          .request({ type: 'decoPalette' }, { priority: -700, ns: NS })
          .then((r) => {
            decoPal = r.pal;
            decoPending.delete('pal');
            if (!closed) wantDeco();
          })
          .catch(() => decoPending.delete('pal'));
      }
      return;
    }
    const pcx = Math.floor(st.x / 16), pcz = Math.floor(st.z / 16);
    const sv = wantSpawnVox();
    const want = new Set();
    const jobs = [];
    for (let dz = -decoR; dz <= decoR; dz++)
      for (let dx = -decoR; dx <= decoR; dx++) {
        if (dx * dx + dz * dz > decoR * decoR + 1) continue;
        const key = `${pcx + dx},${pcz + dz}`;
        want.add(key);
        if (decos.has(key)) continue;
        const rec = sv?.get(key);
        if (rec) {
          decoPending.delete(key);
          placeDeco(pcx + dx, pcz + dz, key, rec);
        } else if (!decoPending.has(key)) jobs.push([pcx + dx, pcz + dz, key, Math.hypot(dx, dz)]);
      }
    pool.prune((k) => !k.startsWith('deco|') || want.has(k.slice(5)), NS);
    for (const k of [...decoPending]) if (k !== 'pal' && !want.has(k)) decoPending.delete(k);
    for (const [cx, cz, key, d] of jobs) {
      decoPending.add(key);
      pool
        .request({ type: 'deco', seed, worldType, cx, cz }, { priority: -700 + d, key: `deco|${key}`, ns: NS })
        .then((r) => {
          if (!decoPending.delete(key) || closed || decos.has(key)) return;
          placeDeco(cx, cz, key, r.buf);
        })
        .catch(() => decoPending.delete(key));
    }
    for (const [key, c] of decos)
      if (Math.hypot(c.cx - pcx, c.cz - pcz) > decoR + 3) {
        if (c.mesh) {
          decoGroup.remove(c.mesh);
          c.mesh.geometry.dispose();
        }
        decos.delete(key);
      }
  }
  function setTrees(on) {
    if (trees === !!on) return;
    trees = !!on;
    decoGroup.visible = trees;
    if (trees) wantDeco();
    dirty = true;
  }

  /** Boxes [kind, x0, y0, z0, x1, y1, z1] (inclusive) + vein / room boxes from the markers → one mesh + edges. */
  function rebuildBoxes() {
    if (boxObjs) {
      for (const o of boxObjs) {
        xrGroup.remove(o);
        o.geometry.dispose();
      }
      boxObjs = null;
    }
    if (!xr.on) return;
    const list = [];
    if (xr.structures) for (const b of rustBoxes) list.push([XR_COLORS[b[0]] ?? 0xffffff, b[1], b[2], b[3], b[4], b[5], b[6]]);
    for (const m of markerList) {
      if (!m.box3) continue;
      if (m.xk === 'geode' || m.xk === 'vdungeon' || m.xk === 'lake') {
        if (xr.pop && !dim) continue; // the population mesh shows the real shell
      } else if (m.xk === 'ore' ? !xr.ores : !xr.rooms) continue;
      const b = m.box3;
      if (Math.hypot((b[0] + b[3]) / 2 - st.x, (b[2] + b[5]) / 2 - st.z) > nearR + 160) continue;
      list.push([cssHex(m.color ?? '#ffb300'), ...b]);
    }
    if (!list.length) return;
    const q = new Quads();
    const lp = new Float32Array(list.length * 24 * 3), lc = new Float32Array(list.length * 24 * 3);
    let li = 0;
    const c3 = new THREE.Color();
    for (const [rgb, a0, b0, c0, a1, b1, c1] of list) {
      const x0 = X(a0), y0 = b0, z0 = Z(c0), x1 = X(a1 + 1), y1 = b1 + 1, z1 = Z(c1 + 1);
      q.quad([x0, y1, z0], [x1, y1, z0], [x1, y1, z1], [x0, y1, z1], rgb, 1);
      q.quad([x0, y0, z0], [x1, y0, z0], [x1, y0, z1], [x0, y0, z1], rgb, 0.5);
      q.quad([x0, y0, z0], [x0, y1, z0], [x0, y1, z1], [x0, y0, z1], rgb, 0.8);
      q.quad([x1, y0, z0], [x1, y1, z0], [x1, y1, z1], [x1, y0, z1], rgb, 0.8);
      q.quad([x0, y0, z0], [x0, y1, z0], [x1, y1, z0], [x1, y0, z0], rgb, 0.65);
      q.quad([x0, y0, z1], [x0, y1, z1], [x1, y1, z1], [x1, y0, z1], rgb, 0.65);
      c3.setHex(rgb);
      const P = [[x0, y0, z0], [x1, y0, z0], [x1, y0, z1], [x0, y0, z1], [x0, y1, z0], [x1, y1, z0], [x1, y1, z1], [x0, y1, z1]];
      for (const [i, j] of [[0, 1], [1, 2], [2, 3], [3, 0], [4, 5], [5, 6], [6, 7], [7, 4], [0, 4], [1, 5], [2, 6], [3, 7]]) {
        lp.set(P[i], li * 3);
        lp.set(P[j], li * 3 + 3);
        lc.set([c3.r, c3.g, c3.b, c3.r, c3.g, c3.b], li * 3);
        li += 2;
      }
    }
    const g = q.geometry();
    const mesh = new THREE.Mesh(g, boxMat);
    mesh.renderOrder = 5;
    const lg = new THREE.BufferGeometry();
    lg.setAttribute('position', new THREE.BufferAttribute(lp, 3));
    lg.setAttribute('color', new THREE.BufferAttribute(lc, 3));
    const lines = new THREE.LineSegments(lg, edgeMat);
    lines.renderOrder = 6;
    xrGroup.add(mesh, lines);
    boxObjs = [mesh, lines];
    dirty = true;
  }
  function wantBoxes(force) {
    if (!xr.on || !xr.structures || end) return rebuildBoxes();
    if (!force && boxAt && Math.hypot(st.x - boxAt.x, st.z - boxAt.z) < 128) return;
    boxAt = { x: st.x, z: st.z };
    const R = nearR + 160, my = ++boxSeq;
    pool
      .request({ type: 'xrayBoxes', seed, worldType, dim, x0: Math.floor(st.x - R), z0: Math.floor(st.z - R), x1: Math.floor(st.x + R), z1: Math.floor(st.z + R) }, { priority: -650, ns: NS })
      .then((r) => {
        if (closed || my !== boxSeq) return;
        rustBoxes = [];
        for (let i = 0; i + 6 < r.boxes.length; i += 7) rustBoxes.push(Array.from(r.boxes.subarray(i, i + 7)));
        rebuildBoxes();
      })
      .catch(() => {});
  }
  function setXray(o) {
    if (Object.keys(o).every((k) => xr[k] === o[k])) return;
    const was = xr.on;
    Object.assign(xr, o);
    if (xr.on !== was) setTerrainXray(xr.on);
    showCaves(xr.caves);
    showPops(xr.pop);
    if (xr.on) {
      wantCaves();
      wantPops();
      wantBoxes(true);
    } else rebuildBoxes();
    if (!xr.on && !st.fly) settle(false);
    dirty = true;
  }

  setTerrainXray(xr.on);
  setTimeout(() => !closed && wantDeco(), 0);
  if (xr.on)
    setTimeout(() => {
      if (closed) return;
      wantCaves();
      wantPops();
      wantBoxes(true);
    }, 0);

  // ---- input
  const el = renderer.domElement;
  const pointers = new Map();
  let pinchD = 0;
  let downAt = null;
  let lpTimer = 0;
  const LONG_PRESS_MS = 450;
  function onDown(e) {
    el.setPointerCapture(e.pointerId);
    pointers.set(e.pointerId, { x: e.clientX, y: e.clientY });
    downAt = pointers.size === 1 ? { x: e.clientX, y: e.clientY, moved: 0, long: false, t: performance.now() } : null;
    clearTimeout(lpTimer);
    if (downAt && e.button === 0) {
      const d = downAt;
      lpTimer = setTimeout(() => {
        if (downAt !== d || d.moved >= 6 || pointers.size !== 1) return;
        d.long = true;
        selectBlock(blockAt(d.x, d.y), d.x, d.y);
      }, LONG_PRESS_MS);
    }
    if (pointers.size === 2) {
      const [a, b] = [...pointers.values()];
      pinchD = Math.hypot(a.x - b.x, a.y - b.y);
    }
  }
  function onMove(e) {
    const p = pointers.get(e.pointerId);
    if (!p) {
      if (e.pointerType === 'mouse' && !pickedPinned) sendPick(pickAt(e.clientX, e.clientY), e.clientX, e.clientY, false);
      return;
    }
    if (downAt) downAt.moved = Math.max(downAt.moved, Math.hypot(e.clientX - downAt.x, e.clientY - downAt.y));
    if (downAt && downAt.moved >= 6) clearTimeout(lpTimer);
    if (downAt?.long) return; // the long press picked a block: no look-around until release
    if (pointers.size === 1) {
      const k = (st.fov / 70) * (phone ? 0.3 : 0.22);
      st.heading = (st.heading - (e.clientX - p.x) * k + 360) % 360;
      st.pitch = Math.max(-89, Math.min(89, st.pitch + (e.clientY - p.y) * k));
      dirty = true;
    }
    p.x = e.clientX;
    p.y = e.clientY;
    if (pointers.size === 2) {
      const [a, b] = [...pointers.values()];
      const d = Math.hypot(a.x - b.x, a.y - b.y);
      if (pinchD > 0) setFov(st.fov * (pinchD / d));
      pinchD = d;
    }
  }
  function onUp(e) {
    pointers.delete(e.pointerId);
    pinchD = 0;
    clearTimeout(lpTimer);
    // a busy main thread can starve the timer: a still press held long enough is a long press too
    if (e.type === 'pointerup' && e.button === 0 && downAt && !downAt.long && downAt.moved < 6 && performance.now() - downAt.t >= LONG_PRESS_MS) {
      downAt.long = true;
      selectBlock(blockAt(downAt.x, downAt.y), downAt.x, downAt.y);
    }
    // a tap / click without dragging: pin the marker under it (or clear), and drop the picked block
    if (e.type === 'pointerup' && e.button !== 2 && downAt && downAt.moved < 6 && !downAt.long) {
      sendPick(pickAt(e.clientX, e.clientY), e.clientX, e.clientY, true);
      if (selBlock) selectBlock(null);
    }
    downAt = null;
  }
  function onLeave(e) {
    if (e.pointerType === 'mouse' && !pickedPinned && picked) sendPick(null, 0, 0, false);
  }
  function setFov(f) {
    st.fov = Math.max(20, Math.min(110, f));
    dirty = true;
  }
  function onWheel(e) {
    e.preventDefault();
    setFov(st.fov * Math.exp(e.deltaY * 0.0012));
  }
  const keys = new Set();
  const MOVE_KEYS = ['w', 'a', 's', 'd', 'arrowup', 'arrowdown', 'arrowleft', 'arrowright', ' ', 'shift', 'q', 'e'];
  function onKeyDown(e) {
    if (e.target instanceof HTMLInputElement || e.target instanceof HTMLTextAreaElement) return;
    const k = e.key.toLowerCase();
    if (MOVE_KEYS.includes(k)) {
      keys.add(k);
      e.preventDefault();
    } else if (k === 'f') toggleFly();
    else opts.onKey?.(k);
  }
  function onKeyUp(e) {
    keys.delete(e.key.toLowerCase());
  }
  function onBlur() {
    keys.clear();
  }
  el.addEventListener('pointerdown', onDown);
  el.addEventListener('pointermove', onMove);
  el.addEventListener('pointerup', onUp);
  el.addEventListener('pointercancel', onUp);
  el.addEventListener('wheel', onWheel, { passive: false });
  el.addEventListener('pointerleave', onLeave);
  // right click picks a block at once; the browser menu (and touch long-press menu) stays away
  function onContext(e) {
    e.preventDefault();
    if (e.pointerType !== 'touch' && !(downAt?.long)) selectBlock(blockAt(e.clientX, e.clientY), e.clientX, e.clientY);
  }
  el.addEventListener('contextmenu', onContext);
  window.addEventListener('keydown', onKeyDown);
  window.addEventListener('keyup', onKeyUp);
  window.addEventListener('blur', onBlur);

  // phone joystick + up/down
  const stick = { x: 0, y: 0, up: 0 };
  let joyEl = null;
  if (phone || 'ontouchstart' in window) {
    joyEl = document.createElement('div');
    joyEl.className = 'sv-joy';
    joyEl.innerHTML = '<div class="sv-knob"></div>';
    const knob = joyEl.firstChild;
    let jid = null;
    const set = (e) => {
      const r = joyEl.getBoundingClientRect();
      let dx = (e.clientX - (r.left + r.width / 2)) / (r.width / 2), dy = (e.clientY - (r.top + r.height / 2)) / (r.height / 2);
      const l = Math.hypot(dx, dy);
      if (l > 1) (dx /= l), (dy /= l);
      stick.x = dx;
      stick.y = dy;
      knob.style.transform = `translate(${dx * 28}px, ${dy * 28}px)`;
    };
    joyEl.addEventListener('pointerdown', (e) => {
      jid = e.pointerId;
      joyEl.setPointerCapture(jid);
      set(e);
      e.stopPropagation();
    });
    joyEl.addEventListener('pointermove', (e) => e.pointerId === jid && set(e));
    const end = () => {
      jid = null;
      stick.x = stick.y = 0;
      knob.style.transform = '';
    };
    joyEl.addEventListener('pointerup', end);
    joyEl.addEventListener('pointercancel', end);
    container.appendChild(joyEl);
    for (const [cls, dir, txt] of [['sv-up', 1, '▲'], ['sv-down', -1, '▼']]) {
      const b = document.createElement('button');
      b.className = `sv-ud ${cls}`;
      b.textContent = txt;
      b.addEventListener('pointerdown', (e) => {
        e.stopPropagation();
        stick.up = dir;
        if (!st.fly) toggleFly();
      });
      b.addEventListener('pointerup', () => (stick.up = 0));
      b.addEventListener('pointercancel', () => (stick.up = 0));
      b.addEventListener('pointerleave', () => (stick.up = 0));
      container.appendChild(b);
    }
  }

  function toggleFly() {
    st.fly = !st.fly;
    if (!st.fly) settle(false);
    dirty = true;
    emit(true);
  }

  // ---- loop
  let raf = 0, last = performance.now(), lastLoadAt = { x: st.x, z: st.z }, lastEmit = 0;
  function resize() {
    const w = container.clientWidth || 1, h = container.clientHeight || 1;
    renderer.setSize(w, h, false);
    renderer.domElement.style.width = '100%';
    renderer.domElement.style.height = '100%';
    camera.aspect = w / h;
    dirty = true;
  }
  const ro = new ResizeObserver(resize);
  ro.observe(container);
  resize();

  function emit(force) {
    const now = performance.now();
    if (!force && now - lastEmit < 120) return;
    lastEmit = now;
    container.dataset.svStats = JSON.stringify({ tris, first: firstFrameMs && Math.round(firstFrameMs), full: fullMs && Math.round(fullMs), near: loadedNear(), of: nearTotal, far: far.size, mid: midL?.map.size ?? 0, midPend: midL?.pending.size ?? 0 });
    opts.onState?.({
      x: st.x, z: st.z, y: st.y - EYE, heading: st.heading, pitch: st.pitch, fov: st.fov, fly: st.fly,
      nearLoaded: loadedNear(), nearTotal, farLoaded: far.size, tris, firstFrameMs, fullMs, capped, biome: genAt(st.x, st.z),
    });
    if (biomeVer !== biomeSent && opts.onBiomes) {
      biomeSent = biomeVer;
      opts.onBiomes({ near: [...nearCounts], far: [...farCounts] });
    }
    if (picked) {
      // keep a shown tooltip's distance / direction current
      const r = el.getBoundingClientRect();
      const v = new THREE.Vector3(picked.label.position.x, picked.label.position.y, picked.label.position.z).project(camera);
      const on = v.z <= 1 && Math.abs(v.x) <= 1.05 && Math.abs(v.y) <= 1.05;
      if (!on && !pickedPinned) sendPick(null, 0, 0, false);
      else opts.onPick?.({ ...markerInfo(picked), sx: ((v.x + 1) / 2) * r.width + r.left, sy: ((1 - v.y) / 2) * r.height + r.top, pinned: pickedPinned, offscreen: !on });
    }
  }

  function frame(now) {
    raf = requestAnimationFrame(frame);
    const dt = Math.min(0.25, (now - last) / 1000);
    last = now;
    // movement
    let f = 0, s = 0, u = 0;
    if (keys.has('w') || keys.has('arrowup')) f += 1;
    if (keys.has('s') || keys.has('arrowdown')) f -= 1;
    if (keys.has('d') || keys.has('arrowright')) s += 1;
    if (keys.has('a') || keys.has('arrowleft')) s -= 1;
    if (keys.has(' ') || keys.has('e')) u += 1;
    if (keys.has('shift') || keys.has('q')) u -= 1;
    f -= stick.y;
    s += stick.x;
    u += stick.up;
    if (u && !st.fly) {
      st.fly = true;
      emit(true);
    }
    if (f || s || u) {
      const g = groundAt(st.x, st.z) ?? st.y;
      const speed = 12 + (st.fly ? Math.max(end && groundAt(st.x, st.z) == null ? 30 : 0, st.y - g) * 0.8 : 0);
      const b = (st.heading * Math.PI) / 180;
      st.x += (Math.sin(b) * f + Math.cos(b) * s) * speed * dt;
      st.z += (-Math.cos(b) * f + Math.sin(b) * s) * speed * dt;
      // End: the ground floor only holds above land; over the void / under an island down to Y1
      const floor = xr.on ? 1.5 : end ? (groundAt(st.x, st.z) != null && st.y >= g ? g + 0.5 : 1) : g + 0.5;
      if (st.fly) st.y = Math.max(floor, Math.min(nether ? 126 : 600, st.y + u * speed * dt));
      dirty = true;
    }
    if (!st.fly) {
      const g = groundAt(st.x, st.z);
      if (g != null && Math.abs(g + EYE - st.y) > 0.01) {
        st.y += (g + EYE - st.y) * Math.min(1, dt * 10);
        dirty = true;
      }
    }
    updateLabels();
    if (Math.hypot(st.x - lastLoadAt.x, st.z - lastLoadAt.z) > 16) {
      lastLoadAt = { x: st.x, z: st.z };
      wantNear();
      wantFar();
      wantDeco();
      if (xr.on) {
        wantCaves();
        wantPops();
        wantBoxes(false);
      }
    }
    if (dirty) {
      camera.position.set(st.x - ox, st.y, st.z - oz);
      camera.rotation.y = (-st.heading * Math.PI) / 180;
      camera.rotation.x = (st.pitch * Math.PI) / 180;
      camera.fov = st.fov;
      camera.updateProjectionMatrix();
      sky.position.copy(camera.position);
      if (sea) sea.position.set(camera.position.x, sea.position.y, camera.position.z);
      if (roof) roof.position.set(camera.position.x, roof.position.y, camera.position.z);
      declutter();
      renderer.render(scene, camera);
      if (firstFrameMs == null && tris > 0) firstFrameMs = performance.now() - t0;
      dirty = false;
      emit(false);
    }
  }

  wantNear();
  wantFar();
  raf = requestAnimationFrame(frame);

  return {
    get state() {
      return { ...st, tris, nearLoaded: loadedNear(), nearTotal, firstFrameMs, fullMs };
    },
    toggleFly,
    /** Drop the picked block (outline + `onBlock(null)`). */
    clearBlock() {
      selectBlock(null);
    },
    /** Colour the terrain by height with the map's terrain preset (`refresh`: the preset changed). */
    setHeightColours(on, refresh) {
      if (refresh) rampTexture(rampTex);
      if (heightCol !== !!on) {
        heightCol = !!on;
        for (const t of near.values()) if (t.mesh) t.mesh.material = matNear();
        for (const L of levels) for (const f of L.map.values()) if (f.mesh) f.mesh.material = matField(L);
      }
      dirty = true;
    },
    setTint(on) {
      if (tint === !!on) return;
      tint = !!on;
      applyTint();
    },
    setTrees(on) {
      setTrees(on);
    },
    setXray(o) {
      setXray(o);
    },
    setWater(on) {
      if (waterMat) waterMat.visible = !!on;
      if (seaMat) seaMat.visible = !!on;
      dirty = true;
    },
    setGroups(list) {
      if (list.length === groupsOn.size && list.every((g) => groupsOn.has(g))) return;
      groupsOn = new Set(list);
      for (const o of markerObjs) o.beam.visible = o.label.visible = groupsOn.has(o.group);
      if (picked && !groupsOn.has(picked.group)) sendPick(null, 0, 0, false);
      labelAt = { x: NaN, z: NaN };
      updateLabels();
      dirty = true;
    },
    setMarkers(list) {
      markerList = list;
      const was = picked?.m;
      buildMarkers();
      if (was) {
        const o = markerObjs.find((o) => o.m.x === was.x && o.m.z === was.z && o.m.label === was.label);
        if (o) picked = o;
        else sendPick(null, 0, 0, false);
      }
      if (xr.on) rebuildBoxes();
    },
    clearPick() {
      sendPick(null, 0, 0, false);
    },
    /** Move to (x, z) in this scene. False when it is too far from the scene origin (float precision): recreate. */
    teleport(x, z) {
      if (Math.hypot(x - ox, z - oz) > 6000) return false;
      st.x = x + 0.5;
      st.z = z + 0.5;
      st.groundKnown = false;
      if (!st.fly) {
        const g = groundAt(st.x, st.z);
        if (g != null) st.y = g + EYE;
      }
      if (picked) sendPick(null, 0, 0, false);
      lastLoadAt = { x: st.x, z: st.z };
      for (const L of levels) L.centre = null;
      wantNear();
      wantFar();
      wantDeco();
      buildMarkers();
      if (xr.on) {
        wantCaves();
        wantPops();
        wantBoxes(true);
      }
      dirty = true;
      emit(true);
      return true;
    },
    setHeading(h) {
      st.heading = h;
      dirty = true;
    },
    screenshot() {
      renderer.render(scene, camera);
      return renderer.domElement.toDataURL('image/png');
    },
    destroy() {
      closed = true;
      cancelAnimationFrame(raf);
      pool.prune(() => false, NS);
      ro.disconnect();
      el.removeEventListener('contextmenu', onContext);
      clearTimeout(lpTimer);
      el.removeEventListener('pointerdown', onDown);
      el.removeEventListener('pointermove', onMove);
      el.removeEventListener('pointerup', onUp);
      el.removeEventListener('pointercancel', onUp);
      el.removeEventListener('wheel', onWheel);
      el.removeEventListener('pointerleave', onLeave);
      window.removeEventListener('keydown', onKeyDown);
      window.removeEventListener('keyup', onKeyUp);
      window.removeEventListener('blur', onBlur);
      scene.traverse((o) => {
        if (o.geometry) o.geometry.dispose();
        if (o.material) {
          o.material.map?.dispose();
          o.material.dispose();
        }
      });
      renderer.dispose();
      renderer.forceContextLoss();
      renderer.domElement.remove();
      joyEl?.remove();
      for (const b of container.querySelectorAll('.sv-ud')) b.remove();
    },
  };
}
