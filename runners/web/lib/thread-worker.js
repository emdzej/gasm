// One worker of a ThreadPool (lib/threads.js), in Node (worker_threads) and in browsers
// (a module Worker). It instantiates the game's module against the shared memory once,
// then runs threads on it: parked in Atomics.wait on its slot until thread-spawn hands
// it a thread id and start argument, then wasi_thread_start(tid, arg), then parked again.
// Imports here: the WASI subset, gasm.log/has/time_ms/max_threads and the assets (the
// same code as the game's thread, a GasmHost of its own); every other gasm import does
// nothing and answers 0, as on a native worker thread.

import { GasmHost } from './host.js';
import { AssetTable, bytesSource } from './assets.js';
import { ProcExit } from './wasi.js';
import { END_EXIT, END_TRAP, IDLE, RUN, STOP, reportEnd, slotIndex, spawnThread, waitForAck } from './threads.js';

const IS_NODE = typeof process !== 'undefined' && !!process.versions?.node && typeof self === 'undefined';
const WORKER_IMPORTS = new Set(['log', 'has', 'time_ms', 'max_threads', 'utc_offset_minutes']);

let port;
if (IS_NODE) {
  const { parentPort } = await import('node:worker_threads');
  port = { post: (m) => parentPort.postMessage(m), once: (f) => parentPort.once('message', f) };
} else {
  port = { post: (m) => self.postMessage(m), once: (f) => { self.onmessage = (e) => { self.onmessage = null; f(e.data); }; } };
}
port.once((m) => init(m).catch((e) => port.post({ failed: e?.message ?? String(e) })));

async function init({ module, memory, memoryImport, ctrl, slot, size, assets, provided, epoch }) {
  const host = new GasmHost({ assets: await sharedAssets(assets), onLog: (m) => port.post({ log: m }) });
  host.memory = memory;
  host.t0 = epoch - performance.timeOrigin;   // the game's clock origin (gasm.time_ms)
  for (const p of provided) host.provided.add(p);
  const all = host.gasmImports(), noop = () => 0;
  const gasm = { max_threads: () => size };
  for (const [name, f] of Object.entries(all)) if (!(name in gasm)) gasm[name] = WORKER_IMPORTS.has(name) || name.startsWith('asset_') ? f : noop;
  const wasi = host.wasiImports();
  const imports = new Proxy({}, {
    get: (_, mod) => {
      if (mod === 'wasi_snapshot_preview1') return wasi;
      if (mod === 'wasi') return { 'thread-spawn': (arg) => spawnThread(ctrl, size, arg) };
      if (mod === memoryImport.module) return { [memoryImport.name]: memory };
      const fns = mod === 'gasm' ? gasm : {};
      return new Proxy(fns, { get: (t, n) => t[n] ?? noop });
    },
  });
  const instance = await WebAssembly.instantiate(module, imports);
  const start = instance.exports.wasi_thread_start;
  if (typeof start !== 'function') throw new Error("the module imports wasi.thread-spawn but doesn't export wasi_thread_start");
  Atomics.store(ctrl, slotIndex(slot), IDLE);
  port.post({ ready: true });
  // let the message go before this thread blocks for good
  await new Promise((r) => setTimeout(r, 0));
  run(ctrl, slot, start);
}

function run(ctrl, slot, start) {
  const s = slotIndex(slot);
  for (;;) {
    Atomics.wait(ctrl, s, IDLE);
    const state = Atomics.load(ctrl, s);
    if (state === STOP) return;
    if (state !== RUN) continue;
    const tid = ctrl[s + 1], arg = ctrl[s + 2];
    try {
      start(tid, arg);
    } catch (e) {
      // wasi-threads: a trap or exit on any thread ends the game (the game's thread
      // reports it after its frame)
      const exit = e instanceof ProcExit;
      const message = exit ? `guest exited with code ${e.code} (on a worker thread)` : `thread ${tid}: ${e?.message ?? e}`;
      if (reportEnd(ctrl, exit ? END_EXIT : END_TRAP, exit ? e.code : 1, message)) watchdog(ctrl, exit, message);
      Atomics.store(ctrl, s, STOP);
      return;
    }
    Atomics.store(ctrl, s, IDLE);
  }
}

// The game's thread may be blocked waiting for the thread that died (no timeout ends an
// atomic wait), so in Node, as natively, the process ends if it hasn't seen the end in
// 2 s. In browsers the page ends the game's worker.
async function watchdog(ctrl, exit, message) {
  if (!IS_NODE || waitForAck(ctrl, 2000)) return;
  const { writeSync } = await import('node:fs');
  writeSync(2, exit ? `[gasm-node] ${message}\n` : `error: ${message}\n`);
  process.kill(process.pid, 'SIGKILL');
}

/** The game's assets in this worker, from AssetTable.share(). */
async function sharedAssets(shared) {
  if (!shared) return new AssetTable();
  let fs = null, reader = null;
  const fds = new Map();
  if (shared.entries.some((e) => e[1].kind === 'file')) fs = await import('node:fs');
  // OPFS files: this worker's own read-only handles, opened now (opening is asynchronous)
  const opfs = new Map();
  for (const [, d] of shared.entries) {
    if (d.kind === 'opfs' && !opfs.has(d.handle)) opfs.set(d.handle, await d.handle.createSyncAccessHandle({ mode: 'read-only' }));
  }
  return AssetTable.fromShared(shared, (d) => {
    if (d.kind === 'opfs') {
      const access = opfs.get(d.handle);
      return { size: () => d.size, readAt: (offset, dst) => (dst.length ? access.read(dst, { at: offset }) : 0) };
    }
    if (d.kind === 'bytes') return bytesSource(d.bytes);
    if (d.kind === 'blob') {
      reader ??= new FileReaderSync();
      return {
        size: () => d.blob.size,
        readAt: (offset, dst) => {
          const n = Math.max(0, Math.min(dst.length, d.blob.size - offset));
          if (n > 0) dst.set(new Uint8Array(reader.readAsArrayBuffer(d.blob.slice(offset, offset + n))));
          return n;
        },
      };
    }
    if (d.kind === 'file') {
      return {
        size: () => d.size,
        readAt: (offset, dst) => {
          let fd = fds.get(d.path), done = 0;
          if (fd === undefined) { fd = fs.openSync(d.path, 'r'); fds.set(d.path, fd); }
          while (done < dst.length) { const k = fs.readSync(fd, dst, done, dst.length - done, offset + done); if (!k) break; done += k; }
          return done;
        },
      };
    }
    return null;
  });
}
