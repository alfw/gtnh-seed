// Minimal WASI runner for `cargo test --target wasm32-wasip1` (set as the target runner in .cargo/config.toml).
// Preopens the repo root as "/" so tests can read testdata via CARGO_MANIFEST_DIR-relative paths remapped below.
import { readFile } from 'node:fs/promises';
import { WASI } from 'node:wasi';
import path from 'node:path';
import { fileURLToPath } from 'node:url';

const root = path.resolve(path.dirname(fileURLToPath(import.meta.url)), '..');
const [wasmPath, ...args] = process.argv.slice(2);
const wasi = new WASI({
  version: 'preview1',
  args: [wasmPath, ...args],
  env: { ...process.env, RWG2_GOLDEN_DIR: '/testdata/golden', RWG2_DUMP_DIR: '/testdata/pack/gtnh-2.9.0/dump', RWG2_DATA_DIR: '/data' },
  preopens: { '/': root },
  returnOnExit: true,
});
const mod = await WebAssembly.compile(await readFile(wasmPath));
const inst = await WebAssembly.instantiate(mod, wasi.getImportObject());
process.exitCode = wasi.start(inst);
