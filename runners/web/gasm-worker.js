// gasm-worker.js — run a gasm guest in a dedicated Worker. The page keeps input,
// display and audio; the worker runs the guest with lazy synchronous asset providers
// (OPFS, File/Blob) that only exist in workers. Messages carry transferables, so no
// SharedArrayBuffer and therefore no cross-origin isolation (COOP/COEP) is needed:
// works on GitHub Pages.
//
// gasm:gfx guests: pass `canvas` (from canvas.transferControlToOffscreen()) and its
// display `size`; the worker renders with WebGPU into it. Start rejects with an error
// mentioning "WebGPU" if the worker has none (e.g. browsers without WebGPU in
// workers); run the guest on the main thread then.
//
// Page side:
//   import { GasmWorker } from '@emdzej/gasm-host/worker';
//   const w = await GasmWorker.start({
//     wasm,                                          // ArrayBuffer / Uint8Array
//     assets: [{ kind: 'opfs', dir: 'openrf-cd' }],  // and/or 'memory' | 'files'
//     params, storage: 'openrf', allowNet: false,
//     onLog, onAudio: (samples, rate, channels) => …,
//   });
//   const r = await w.frames([pads0, pads1, …], true);  // one entry per frame: [p0,p1,p2,p3]
//   if (r.frame) draw(r.frame.rgba, r.frame.width, r.frame.height);   // 2D guests only
//   // optional per batch: { texts: [text per frame] (text_input), size: [w, h] (gfx canvas) }
//
// Asset specs (in order; the first 'memory' entries are explicit, the rest are folder entries):
//   { kind: 'memory', record: { name: Uint8Array } }
//   { kind: 'files', entries: [[name, File], ...], prefix? }   lazy, FileReaderSync
//   { kind: 'opfs', dir: 'path/in/opfs', prefix? }            lazy, FileSystemSyncAccessHandle

import {
  AssetTable, GasmHost, IdbStorage, MemoryStorage, ProcExit, bytesSource, fileAssets, opfsAssets,
} from './gasm-host.js';
import { WebGpuGfx } from './webgpu-gfx.js';

const inWorker = typeof WorkerGlobalScope !== 'undefined' && globalThis instanceof WorkerGlobalScope;

// ---- page side ------------------------------------------------------------------------

export class GasmWorker {
  static async start({
    wasm, assets = [], params = {}, storage = null, allowNet = false, keyboard = false,
    hashing = false, virtualTime = false, onLog = console.log, onAudio = () => {},
    canvas = null, size = null,
    url = new URL('./gasm-worker.js', import.meta.url),
  }) {
    const w = new GasmWorker(new Worker(url, { type: 'module', name: 'gasm-guest' }), onLog, onAudio);
    const bytes = wasm instanceof ArrayBuffer ? wasm : wasm.buffer.slice(wasm.byteOffset, wasm.byteOffset + wasm.byteLength);
    const ready = w.next('ready');
    w.worker.postMessage({ type: 'init', wasm: bytes, assets, params, storage, allowNet, keyboard, hashing, virtualTime, canvas, size },
      canvas ? [bytes, canvas] : [bytes]);
    try {
      const r = await ready;
      w.frameRate = r.frameRate;
      return w;
    } catch (e) {
      w.worker.terminate();
      throw e;
    }
  }

  constructor(worker, onLog, onAudio) {
    this.worker = worker;
    this.onLog = onLog;
    this.onAudio = onAudio;
    this.frameRate = 60;
    this.inputMode = 0;   // GASM_INPUT_* flags the guest asked for (cursor, keymap)
    this.waiters = [];
    this.stats = null;
    worker.onmessage = (e) => this.message(e.data);
    worker.onerror = (e) => this.fail(new Error(e.message || 'worker error'));
  }

  message(m) {
    if (m.type === 'log') return this.onLog(m.msg);
    if (m.type === 'error') return this.fail(new Error(m.message));
    if (m.type === 'exit') return this.fail(new ProcExit(m.code));
    if (m.type === 'done') {
      for (const a of m.audio) this.onAudio(a.samples, a.rate, a.channels);
      this.frameRate = m.frameRate;
      this.stats = m.stats;
      this.inputMode = m.inputMode;
      if (m.exit !== null) { this.resolve('done', m); return this.fail(new ProcExit(m.exit)); }
      if (m.error !== null) return this.fail(new Error(m.error));
    }
    this.resolve(m.type, m);
  }

  next(type) { return new Promise((resolve, reject) => this.waiters.push({ type, resolve, reject })); }
  resolve(type, m) {
    const i = this.waiters.findIndex((w) => w.type === type);
    if (i >= 0) this.waiters.splice(i, 1)[0].resolve(m);
  }
  fail(err) {
    this.failed = err;
    for (const w of this.waiters.splice(0)) w.reject(err);
  }

  /**
   * Run one frame per entry of `steps` (each: [pad0, pad1, pad2, pad3]). Only the
   * last is shown (catch-up rule) if `show`. Resolves with { frame?, stats, frameIndex }.
   * Rejects with ProcExit when the guest exits.
   */
  frames(steps, show = true, { texts = null, inputs = null, size = null } = {}) {
    if (this.failed) return Promise.reject(this.failed);
    const done = this.next('done');
    this.worker.postMessage({ type: 'frames', steps, show, texts, inputs, size });
    return done;
  }

  /** The player is quitting: gasm_exit (flush saves), close sockets, stop the worker. */
  async exit(timeoutMs = 1000) {
    if (!this.failed) {
      const closed = this.next('exited');
      this.worker.postMessage({ type: 'exit' });
      await Promise.race([closed, new Promise((r) => setTimeout(r, timeoutMs))]).catch(() => {});
    }
    this.worker.terminate();
  }
}

// ---- worker side ------------------------------------------------------------------------

async function buildAssets(specs, log) {
  const table = new AssetTable(log);
  for (const spec of specs) {
    if (spec.kind === 'memory') {
      for (const [name, bytes] of Object.entries(spec.record ?? {})) table.add(name, bytesSource(bytes));
    } else if (spec.kind === 'files') {
      table.merge(fileAssets(spec.entries, { prefix: spec.prefix, log }));
    } else if (spec.kind === 'opfs') {
      table.merge(await opfsAssets(spec.dir, { prefix: spec.prefix, log }));
    } else {
      throw new Error(`unknown asset source kind ${spec.kind}`);
    }
  }
  return table.finish();
}

if (inWorker) {
  let host = null, gfx = null, keyboard = false, audio = [], presented = 0;
  const post = (m, transfer = []) => globalThis.postMessage(m, transfer);
  const log = (msg) => post({ type: 'log', msg });
  const stats = () => ({
    frames: host.frameIndex, presented: host.framesPresented, width: host.width, height: host.height,
    videoHash: host.videoHash, audioHash: host.audioHash, audioFrames: host.audioFrames,
  });

  globalThis.onmessage = async (e) => {
    const m = e.data;
    if (m.type === 'init') {
      try {
        const assets = await buildAssets(m.assets, log);
        const storage = m.storage
          ? await IdbStorage.open(m.storage).catch((err) => { log(`storage unavailable (${err.message})`); return new MemoryStorage(); })
          : new MemoryStorage();
        if (m.canvas) {
          gfx = await WebGpuGfx.create(m.canvas, log);
          if (m.size) gfx.setSize(...m.size);
        }
        keyboard = m.keyboard;
        host = new GasmHost({
          assets, params: m.params, storage, allowNet: m.allowNet, virtualTime: m.virtualTime, onLog: log,
          onAudio: (samples, rate, channels) => audio.push({ samples, rate, channels }), ...(gfx ? { gfx } : {}),
        });
        host.hashing = m.hashing;
        await host.load(m.wasm);
        post({ type: 'ready', frameRate: host.frameRate });
      } catch (err) {
        post(err instanceof ProcExit ? { type: 'exit', code: err.code } : { type: 'error', message: err.message });
      }
    } else if (m.type === 'frames') {
      let exit = null, error = null;
      if (gfx && m.size) gfx.setSize(...m.size);
      for (let i = 0; i < m.steps.length; i++) {
        const pads = m.steps[i];
        host.getPad = (p) => pads[p] ?? 0;
        host.text = keyboard ? (m.texts?.[i] ?? '') : null;
        host.input = m.inputs?.[i] ?? { keys: null, keyEvents: [], pointer: null, gamepads: null };
        host.showFrame = m.show && i === m.steps.length - 1;
        if (gfx) gfx.used = false;
        try { host.frame(); } catch (err) {
          if (err instanceof ProcExit) exit = err.code; else error = err.message;
          break;
        }
      }
      // Send the latest 2D frame only if a new one was presented (transfer, no copy on arrival).
      // With a gfx canvas (it belongs to WebGPU here), blit it in the worker instead.
      let frame = null;
      if (gfx) {
        if (m.show && !gfx.used && host.width && host.framesPresented !== presented) gfx.presentVideo(host.rgba, host.width, host.height);
        presented = host.framesPresented;
      } else if (host.framesPresented !== presented && host.width) {
        presented = host.framesPresented;
        const rgba = host.rgba.slice();
        frame = { rgba, width: host.width, height: host.height };
      }
      const out = audio; audio = [];
      const transfer = [...(frame ? [frame.rgba.buffer] : []), ...out.map((a) => a.samples.buffer)];
      post({ type: 'done', frame, audio: out, stats: stats(), frameIndex: host.frameIndex, frameRate: host.frameRate, inputMode: host.inputMode, exit, error }, transfer);
    } else if (m.type === 'exit') {
      try { host?.exit(); await host?.net.closeAll(); } catch {}
      post({ type: 'exited' });
    }
  };
}
