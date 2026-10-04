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
//     wasm,                     // WebAssembly.Module (shared, not copied) or bytes (transferred: detached)
//     assets: [{ kind: 'opfs', dir: 'gasm-assets/mygame' }],  // and/or 'memory' | 'files'
//     params, storage: 'mygame', allowNet: false,
//     onLog, onAudio: (samples, rate, channels) => …,
//   });
//   // one step per frame: { pads: [p0, p1, p2, p3], text, input } (GasmHost.runFrames), or just pads
//   const r = await w.frames([{ pads, text, input }, …], true);
//   if (r.frame) draw(r.frame.rgba, r.frame.width, r.frame.height);   // 2D guests only
//   // optional per batch: { size: [w, h] } (display size of a gfx canvas)
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
    hashing = false, virtualTime = false, onLog = console.log, onAudio = () => {}, onTitle = () => {},
    canvas = null, size = null, url = null,
  }) {
    // written out literally so bundlers (Vite, webpack) find and emit the worker
    const worker = url
      ? new Worker(url, { type: 'module', name: 'gasm-guest' })
      : new Worker(new URL('./gasm-worker.js', import.meta.url), { type: 'module', name: 'gasm-guest' });
    const w = new GasmWorker(worker, onLog, onAudio);
    w.onTitle = onTitle;
    // a compiled Module is shared with the worker (no copy, no second compile); bytes are transferred
    const module = wasm instanceof WebAssembly.Module;
    const bytes = module || wasm instanceof ArrayBuffer ? wasm : wasm.buffer.slice(wasm.byteOffset, wasm.byteOffset + wasm.byteLength);
    const transfer = [...(module ? [] : [bytes]), ...(canvas ? [canvas] : [])];
    const ready = w.next('ready');
    w.worker.postMessage({ type: 'init', wasm: bytes, assets, params, storage, allowNet, keyboard, hashing, virtualTime, canvas, size }, transfer);
    try {
      const r = await ready;
      w.frameRate = r.frameRate;
      w.switching = r.switching;
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
    this.title = null;    // the guest's set_title, as of the last batch
    this.onTitle = () => {};
    worker.onmessage = (e) => this.message(e.data);
    worker.onerror = (e) => this.fail(new Error(e.message || 'worker error'));
  }

  message(m) {
    if (m.type === 'log') return this.onLog(m.msg);
    if (m.type === 'error') return this.fail(new Error(m.message));
    if (m.type === 'exit') return this.fail(new ProcExit(m.code));
    if ((m.type === 'done' || m.type === 'ready') && m.title !== undefined && m.title !== this.title) {
      this.title = m.title;
      this.onTitle(m.title);
    }
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
   * Run one frame per entry of `steps` ({ pads: [p0, p1, p2, p3], text, input }, or
   * just the pads array). Only the last is shown (catch-up rule) if `show`. Resolves
   * with { frame?, stats, frameIndex }. Rejects with ProcExit when the guest exits;
   * after an exit or a trap every call rejects (the guest isn't called again).
   */
  frames(steps, show = true, { size = null } = {}) {
    if (this.failed) return Promise.reject(this.failed);
    const done = this.next('done');
    const norm = steps.map((s) => (Array.isArray(s) ? { pads: s } : s));
    this.worker.postMessage({ type: 'frames', steps: norm, show, size });
    return done;
  }

  /** GasmHost.setAsset in the worker, before the next batch of frames (the bytes are transferred). */
  setAsset(name, bytes) {
    if (this.failed) return;
    this.worker.postMessage({ type: 'setAsset', name, bytes }, [bytes.buffer]);
  }
  /** GasmHost.removeAsset in the worker, before the next batch of frames. */
  removeAsset(name) {
    if (!this.failed) this.worker.postMessage({ type: 'removeAsset', name });
  }

  /** The player is quitting: gasm_exit (flush saves), close sockets, stop the worker.
   *  Also after the guest exited or trapped (sockets and storage still get closed). */
  async exit(timeoutMs = 1000) {
    if (this.terminated) return;
    this.terminated = true;
    const closed = this.next('exited');
    this.worker.postMessage({ type: 'exit' });
    await Promise.race([closed, new Promise((r) => setTimeout(r, timeoutMs))]).catch(() => {});
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
  let host = null, gfx = null, keyboard = false, audio = [];
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
        // the worker reads storage on its own connection; closed by host.shutdown()
        keyboard = m.keyboard;
        host = new GasmHost({
          assets, params: m.params, storage, allowNet: m.allowNet, virtualTime: m.virtualTime, onLog: log,
          onAudio: (samples, rate, channels) => audio.push({ samples, rate, channels }), ...(gfx ? { gfx } : {}),
        });
        host.hashing = m.hashing;
        await host.load(m.wasm);
        post({ type: 'ready', frameRate: host.frameRate, title: host.title, switching: host.switching });
      } catch (err) {
        post(err instanceof ProcExit ? { type: 'exit', code: err.code } : { type: 'error', message: err.message });
      }
    } else if (m.type === 'frames') {
      let exit = null, error = null, video = false;
      if (gfx && m.size) gfx.setSize(...m.size);
      const steps = m.steps.map((s) => ({ ...s, text: keyboard ? (s.text ?? '') : null }));
      // a gfx canvas belongs to WebGPU here: runFrames blits 2D frames into it
      // gasm_run guests (stack switching) resume asynchronously
      try { ({ video } = await host.runFramesAsync(steps, m.show)); } catch (err) {
        if (err instanceof ProcExit) exit = err.code; else error = err.message;
      }
      // Send the latest 2D frame only if a new one was presented (transfer, no copy on arrival).
      let frame = null;
      if (!gfx && video) frame = { rgba: host.rgba.slice(), width: host.width, height: host.height, aspect: host.aspect };
      const out = audio; audio = [];
      const transfer = [...(frame ? [frame.rgba.buffer] : []), ...out.map((a) => a.samples.buffer)];
      post({ type: 'done', frame, audio: out, stats: stats(), frameIndex: host.frameIndex, frameRate: host.frameRate, inputMode: host.inputMode, title: host.title, exit, error }, transfer);
    } else if (m.type === 'setAsset') {
      host?.setAsset(m.name, m.bytes);
    } else if (m.type === 'removeAsset') {
      host?.removeAsset(m.name);
    } else if (m.type === 'exit') {
      try { await host?.shutdown(); } catch {}
      gfx?.device.destroy();
      post({ type: 'exited' });
    }
  };
}
