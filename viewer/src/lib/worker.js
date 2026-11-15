// Tile worker: one wasm instance, one provider per (seed, worldType).
import wasmUrl from './rwg2.wasm?url';

let ex = null;
const ready = (async () => {
  const bytes = await (await fetch(wasmUrl)).arrayBuffer(); // no instantiateStreaming: tolerate any MIME type
  const { instance } = await WebAssembly.instantiate(bytes, {});
  ex = instance.exports;
})();

let current = { key: null, handle: -1 };

function providerFor(seed, worldType) {
  const key = `${seed}:${worldType}`;
  if (current.key !== key) {
    if (current.handle >= 0) ex.destroy(current.handle);
    current = { key, handle: ex.create(BigInt(seed), worldType) };
  }
  return current.handle;
}

function readString(ptr, len) {
  return new TextDecoder().decode(new Uint8Array(ex.memory.buffer, ptr, len));
}

self.onmessage = async (e) => {
  const m = e.data;
  try {
    await ready; // a CSP without 'wasm-unsafe-eval' fails here: report it instead of hanging
    if (m.type === 'legend') {
      const p = ex.legend_ptr(), l = ex.legend_len();
      const legend = JSON.parse(readString(p, l));
      const vp = ex.veins_ptr(), vl = ex.veins_len();
      legend.veins = JSON.parse(readString(vp, vl));
      legend.fluids = JSON.parse(readString(ex.fluids_ptr(), ex.fluids_len()));
      legend.blocks = JSON.parse(readString(ex.blocks_json(), ex.blocks_json_len()));
      self.postMessage({ id: m.id, legend });
    } else if (m.type === 'tile' && m.dim) {
      const t0 = performance.now();
      const ptr = ex.render_dim_mode(BigInt(m.seed), m.dim, m.x0, m.z0, m.w, m.h, m.step, m.mode ?? 0);
      const ids = new Uint16Array(ex.memory.buffer, ptr, m.w * m.h).slice();
      self.postMessage({ id: m.id, ids, ms: performance.now() - t0 }, [ids.buffer]);
    } else if (m.type === 'viewCols') {
      // street view: 3 u32 per sample [top | water<<8 | class<<16 | ceil<<24, rgb, biome gen id]
      const t0 = performance.now();
      const ptr = m.dim ? ex.view_cols_dim(BigInt(m.seed), m.dim, m.x0, m.z0, m.w, m.h, m.step) : ex.view_cols(providerFor(m.seed, m.worldType), m.x0, m.z0, m.w, m.h, m.step);
      const cols = new Uint32Array(ex.memory.buffer, ptr, 3 * m.w * m.h).slice();
      self.postMessage({ id: m.id, cols, ms: performance.now() - t0 }, [cols.buffer]);
    } else if (m.type === 'xrayBoxes') {
      const h = m.dim ? 0 : providerFor(m.seed, m.worldType);
      const ptr = ex.xray_boxes(h, BigInt(m.seed), m.dim ?? 0, m.x0, m.z0, m.x1, m.z1);
      const boxes = new Int32Array(ex.memory.buffer, ptr, ex.xray_len()).slice();
      self.postMessage({ id: m.id, boxes }, [boxes.buffer]);
    } else if (m.type === 'xrayCaves') {
      const h = m.dim ? 0 : providerFor(m.seed, m.worldType);
      const ptr = ex.xray_caves(h, BigInt(m.seed), m.dim ?? 0, m.cx, m.cz);
      const bits = new Uint8Array(ex.memory.buffer, ptr, 192 * 32).slice();
      self.postMessage({ id: m.id, bits }, [bits.buffer]);
    } else if (m.type === 'xrayPop') {
      const h = providerFor(m.seed, m.worldType);
      const ptr = ex.xray_pop(h, m.cx, m.cz);
      const codes = new Uint8Array(ex.memory.buffer, ptr, 192 * 256).slice();
      self.postMessage({ id: m.id, codes }, [codes.buffer]);
    } else if (m.type === 'deco') {
      const h = providerFor(m.seed, m.worldType);
      const ptr = ex.deco_chunk(h, m.cx, m.cz);
      const buf = new Uint8Array(ex.memory.buffer, ptr, ex.deco_len()).slice();
      self.postMessage({ id: m.id, buf }, [buf.buffer]);
    } else if (m.type === 'ringDeco') {
      const h = providerFor(m.seed, m.worldType);
      self.postMessage({ id: m.id, ok: ex.ring_deco(h, m.x, m.z) });
    } else if (m.type === 'decoTops') {
      const h = providerFor(m.seed, m.worldType);
      const ptr = ex.deco_tops(h, m.cx, m.cz, m.n);
      const buf = new Uint8Array(ex.memory.buffer, ptr, ex.deco_len()).slice();
      self.postMessage({ id: m.id, buf }, [buf.buffer]);
    } else if (m.type === 'spawnDeco') {
      const t0 = performance.now();
      const ptr = ex.spawn_deco_tops(providerFor(m.seed, m.worldType));
      const raw = new Uint8Array(ex.memory.buffer, ptr, ex.deco_len()).slice();
      const hd = new Int32Array(raw.buffer, 0, 4);
      const buf = raw.slice(16);
      self.postMessage({ id: m.id, cx0: hd[0], cz0: hd[1], n: hd[2], pops: hd[3], buf, ms: performance.now() - t0 }, [buf.buffer]);
    } else if (m.type === 'spawnDeco3d') {
      const t0 = performance.now();
      const ptr = ex.spawn_deco_voxels(providerFor(m.seed, m.worldType));
      const raw = new Uint8Array(ex.memory.buffer, ptr, ex.deco_len()).slice();
      self.postMessage({ id: m.id, buf: raw, ms: performance.now() - t0 }, [raw.buffer]);
    } else if (m.type === 'decoPalette') {
      const ptr = ex.deco_palette();
      const pal = new Uint8Array(ex.memory.buffer, ptr, ex.deco_len()).slice();
      self.postMessage({ id: m.id, pal }, [pal.buffer]);
    } else if (m.type === 'spawnOres') {
      const t0 = performance.now();
      const ptr = ex.spawn_ores(providerFor(m.seed, m.worldType));
      const recs = new Int32Array(ex.memory.buffer, ptr, ex.spawn_ores_len()).slice();
      self.postMessage({ id: m.id, recs, ms: performance.now() - t0 }, [recs.buffer]);
    } else if (m.type === 'geodes') {
      const t0 = performance.now();
      const p = ex.geodes_json(providerFor(m.seed, m.worldType), m.x0, m.z0, m.x1, m.z1);
      const l = ex.tc_json_len();
      self.postMessage({ id: m.id, geodes: JSON.parse(readString(p, l)), ms: performance.now() - t0 });
    } else if (m.type === 'lakes') {
      const t0 = performance.now();
      const p = ex.lakes_json(providerFor(m.seed, m.worldType), m.x0, m.z0, m.x1, m.z1);
      const l = ex.tc_json_len();
      self.postMessage({ id: m.id, lakes: JSON.parse(readString(p, l)), ms: performance.now() - t0 });
    } else if (m.type === 'spawnOil') {
      const t0 = performance.now();
      const p = ex.spawn_oil_json(providerFor(m.seed, m.worldType));
      const l = ex.tc_json_len();
      const j = JSON.parse(readString(p, l));
      self.postMessage({ id: m.id, oil: j.oil, sq: j.sq, ms: performance.now() - t0 });
    } else if (m.type === 'oilCell') {
      const t0 = performance.now();
      const p = ex.oil_cell_json(providerFor(m.seed, m.worldType), m.cx0, m.cz0);
      const l = ex.tc_json_len();
      self.postMessage({ id: m.id, oil: JSON.parse(readString(p, l)), ms: performance.now() - t0 });
    } else if (m.type === 'popColumn') {
      // populate-prefix codes of one column (y 0..191), from the chunk's x-ray population
      const ptr = ex.xray_pop(providerFor(m.seed, m.worldType), m.x >> 4, m.z >> 4);
      const all = new Uint8Array(ex.memory.buffer, ptr, 16 * 16 * 192);
      const lx = m.x & 15, lz = m.z & 15, col = new Uint8Array(192);
      for (let y = 0; y < 192; y++) col[y] = all[(y * 16 + lz) * 16 + lx];
      self.postMessage({ id: m.id, col }, [col.buffer]);
    } else if (m.type === 'vdungeons') {
      const t0 = performance.now();
      const p = ex.vanilla_dungeons_json(providerFor(m.seed, m.worldType), m.x0, m.z0, m.x1, m.z1);
      const l = ex.tc_json_len();
      self.postMessage({ id: m.id, dungeons: JSON.parse(readString(p, l)), ms: performance.now() - t0 });
    } else if (m.type === 'tcNodes') {
      const t0 = performance.now();
      const p = m.dim ? ex.tc_nodes_json_dim(BigInt(m.seed), m.dim, m.x0, m.z0, m.x1, m.z1) : ex.tc_nodes_json(providerFor(m.seed, m.worldType), m.x0, m.z0, m.x1, m.z1);
      const l = ex.tc_json_len();
      self.postMessage({ id: m.id, nodes: JSON.parse(readString(p, l)), ms: performance.now() - t0 });
    } else if (m.type === 'aspectTags') {
      const p = ex.tc_aspect_tags(), l = ex.tc_json_len();
      self.postMessage({ id: m.id, tags: JSON.parse(readString(p, l)) });
    } else if (m.type === 'dimLegend') {
      const p = ex.dim_legend(m.dim), l = ex.dim_legend_len();
      self.postMessage({ id: m.id, legend: JSON.parse(readString(p, l)) });
    } else if (m.type === 'tile') {
      const h = providerFor(m.seed, m.worldType);
      const t0 = performance.now();
      const ptr = ex.render_mode(h, m.x0, m.z0, m.w, m.h, m.step, m.mode ?? 0);
      const ids = new Uint16Array(ex.memory.buffer, ptr, m.w * m.h).slice(); // copy before memory can grow
      self.postMessage({ id: m.id, ids, ms: performance.now() - t0 }, [ids.buffer]);
    } else if (m.type === 'search') {
      const q = new TextEncoder().encode(m.query);
      const seeds = m.seeds; // BigInt64Array
      const total = q.length + seeds.length * 8;
      const ptr = ex.input_buf(total); // call first: may grow memory
      const buf = new Uint8Array(ex.memory.buffer, ptr, total);
      buf.set(q);
      buf.set(new Uint8Array(seeds.buffer, seeds.byteOffset, seeds.length * 8), q.length);
      const t0 = performance.now();
      const rp = ex.search(q.length, seeds.length);
      const rl = ex.result_len();
      const res = JSON.parse(readString(rp, rl));
      self.postMessage({ id: m.id, result: res, ms: performance.now() - t0 });
    } else if (m.type === 'flatPatch') {
      // flat-area patch of a search hit: approx cells (exact = false) or an exact-terrain check (exact = true)
      const h = providerFor(m.seed, m.worldType);
      const q = new TextEncoder().encode(m.query);
      const ptr = ex.input_buf(q.length);
      new Uint8Array(ex.memory.buffer, ptr, q.length).set(q);
      const t0 = performance.now();
      const rp = ex.flat_patch(h, q.length, m.exact ? 1 : 0);
      const res = JSON.parse(readString(rp, ex.result_len()));
      self.postMessage({ id: m.id, result: res, ms: performance.now() - t0 });
    } else if (m.type === 'dimBlock') {
      // street-view block pick in the Nether / Twilight Forest / End
      const ptr = ex.dim_block(BigInt(m.seed), m.dim, m.x, m.y, m.z);
      self.postMessage({ id: m.id, block: JSON.parse(readString(ptr, ex.dim_block_len())) });
    } else if (m.type === 'svBlock') {
      // street-view block pick (overworld): exact surface block + chunk biome
      const ptr = ex.surface_block(providerFor(m.seed, m.worldType), m.x, m.y, m.z);
      const r = Array.from(new Int32Array(ex.memory.buffer, ptr, 8));
      self.postMessage({ id: m.id, block: { block: r[0], top: r[1], gen: r[2], rwg: r[3], height: r[4], below: r[5], above: r[6], ok: r[7] === 1 } });
    } else if (m.type === 'inspect' && m.dim) {
      const ptr = ex.inspect_dim(BigInt(m.seed), m.dim, m.x, m.z);
      const r = Array.from(new Float64Array(ex.memory.buffer, ptr, 4));
      self.postMessage({ id: m.id, inspect: { dimBiome: r[0], height: r[1], top: r[2], lava: r[3] === 1 } });
    } else if (m.type === 'inspect') {
      const h = providerFor(m.seed, m.worldType);
      const ptr = ex.inspect(h, m.x, m.z);
      const r = Array.from(new Float64Array(ex.memory.buffer, ptr, 13));
      const fin = (v) => (Number.isFinite(v) ? v : null);
      self.postMessage({
        id: m.id,
        inspect: {
          rwgId: r[0], genId: r[1], meta: r[2], continent: r[3], marker: r[4] === 1, noiseHeight: fin(r[6]),
          // exact chunk data (what the world stores): dithered biome, blended terrain height, bee climate
          exactRwgId: fin(r[7]), exactGenId: fin(r[8]), height: fin(r[9]),
          beeTemp: fin(r[10]), beeHumidity: fin(r[11]), temperature: fin(r[12]),
        },
      });
    } else if (m.type === 'features') {
      const t0 = performance.now();
      const ptr = m.dim ? ex.features_dim(BigInt(m.seed), m.dim, m.x0, m.z0, m.x1, m.z1, m.flags) : ex.features(providerFor(m.seed, m.worldType), m.x0, m.z0, m.x1, m.z1, m.flags);
      const n = ex.features_len();
      const r = new Int32Array(ex.memory.buffer, ptr, n).slice();
      const U = -2147483648;
      const out = [];
      for (let i = 0; i + 5 < r.length; i += 6) {
        const v = (k) => (r[i + k] === U ? null : r[i + k]);
        const k = r[i];
        const kind = k === 1 ? 'loot' : k === 2 || k === 3 ? 'rogue' : k === 20 ? 'ore' : k === 30 ? 'fluid' : k === 40 ? 'village' : k === 50 ? 'tfFeature' : k === 51 ? 'fortress' : k === 52 ? 'fortressPiece' : k === 53 ? 'spawner' : k === 54 ? 'wart' : k === 55 ? 'fchest' : k === 56 ? 'fchestSlot' : k === 60 ? 'mineshaft' : k === 70 ? 'endIsland' : k === 71 ? 'endTower' : k === 72 ? 'deComet' : k === 73 ? 'deChaos' : k === 74 ? 'draconium' : ['', 'tcMound', 'tcRing', 'tcHilltop', 'tcTotem'][k - 10] ?? 'unknown';
        if (k === 2 || k === 3) {
          // Roguelike: y = ground Y | confidence% << 16
          const y = v(3);
          out.push({ kind, lowConf: k === 3, x: r[i + 1], z: r[i + 2], y: y === null ? null : y & 0xffff, conf: y === null ? null : y >> 16, a: v(4), b: v(5) });
          continue;
        }
        if (k === 12) {
          // eldritch ring: y = surface j | P(builds)% << 16
          const y = v(3);
          out.push({ kind, x: r[i + 1], z: r[i + 2], y: y === null ? null : y & 0xffff, p: y === null ? null : (y >> 16) / 100, a: v(4), b: v(5) });
          continue;
        }
        out.push({ kind, lowConf: k === 3, x: r[i + 1], z: r[i + 2], y: v(3), a: v(4), b: v(5) });
      }
      self.postMessage({ id: m.id, features: out, ms: performance.now() - t0 });
    } else if (m.type === 'spawn') {
      const h = providerFor(m.seed, m.worldType);
      const ptr = ex.spawn(h);
      const r = Array.from(new Int32Array(ex.memory.buffer, ptr, 5));
      self.postMessage({ id: m.id, spawn: { x: r[0], z: r[1], attempts: r[2], prob: r[3] / 1000, confident: !!r[4] } });
    } else if (m.type === 'strongholds') {
      const ptr = ex.strongholds(BigInt(m.seed));
      const r = Array.from(new Int32Array(ex.memory.buffer, ptr, 6));
      const rp = ex.portal_rooms(BigInt(m.seed));
      const rr = Array.from(new Int32Array(ex.memory.buffer, rp, 9));
      self.postMessage({
        id: m.id,
        strongholds: [0, 1, 2].map((i) => ({ chunkX: r[2 * i], chunkZ: r[2 * i + 1], room: rr[3 * i] === -2147483648 ? null : { x: rr[3 * i], y: rr[3 * i + 1], z: rr[3 * i + 2] } })),
      });
    }
  } catch (err) {
    self.postMessage({ id: m.id, error: String(err) });
  }
};
