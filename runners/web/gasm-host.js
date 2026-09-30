// gasm-host.js — gasm ABI v0 host for JavaScript (browser and Node).
//
//   const host = new GasmHost({ assets, params, gfx, storage, allowNet, onPresent, onAudio, onLog, getPad });
//   await host.load(wasmBytes);
//   host.frame();              // call at host.frameRate Hz
//
// Platform concerns (canvas, GPU, audio device, input) are injected, so the same
// file runs the browser runner and the headless Node runner. `gfx` is a backend
// object (NullGfx here; WebGpuGfx in webgpu-gfx.js). Networking uses the global
// WebSocket, which exists in browsers and Node >= 22.

export const ABI_VERSION = 0;

// FNV-1a 32-bit, identical to runners/native/src/host.rs
export function fnv32(h, bytes) {
  for (let i = 0; i < bytes.length; i++) h = Math.imul(h ^ bytes[i], 0x01000193) >>> 0;
  return h;
}
export const FNV_INIT = 0x811c9dc5;

export class ProcExit extends Error {
  constructor(code) { super(`guest called proc_exit(${code})`); this.code = code; }
}

export class GasmHost {
  constructor({ assets = {}, params = {}, gfx = new NullGfx(), storage = new MemoryStorage(), allowNet = false,
                onPresent = () => {}, onAudio = () => {}, onLog = console.log,
                getPad = () => 0, virtualTime = false } = {}) {
    // GasmAssetProvider ({ size(name), readAt(name, offset, dst) }), or a plain
    // { name: Uint8Array } record (wrapped as an in-memory provider).
    this.assets = isAssetProvider(assets) ? assets : memoryAssets(assets);
    this.params = params;            // name -> string
    this.gfx = gfx;
    this.storage = storage;          // MemoryStorage (headless) or IdbStorage (browser)
    this.net = new NetConnections(allowNet, (m) => this.onLog(m));
    this.showFrame = true;           // false during catch-up frames: begin_frame returns 0
    this.onPresent = onPresent;      // (rgba: Uint8ClampedArray, w, h)
    this.onAudio = onAudio;          // (samples: Float32Array interleaved, rate, channels)
    this.onLog = onLog;
    this.getPad = getPad;            // (player) -> bitmask
    this.virtualTime = virtualTime;  // true: time_ms derived from frame count
    this.frameRate = 60;
    this.audioRate = 44100;
    this.audioChannels = 2;
    this.frameIndex = 0;
    this.width = 0;
    this.height = 0;
    this.rgba = new Uint8ClampedArray(0);
    this.framesPresented = 0;
    this.videoHash = FNV_INIT;
    this.audioHash = FNV_INIT;
    this.audioFrames = 0;
    this.hashing = virtualTime;      // hash only when asked (costs CPU)
    this.memory = null;
    this.t0 = performance.now();
    this.stdio = { 1: '', 2: '' };
  }

  // ---- memory helpers ------------------------------------------------------
  bytes(ptr, len) {
    ptr >>>= 0; len >>>= 0;
    const buf = this.memory.buffer;
    if (ptr < 0 || len < 0 || ptr + len > buf.byteLength) throw new RangeError(`guest access out of bounds: ${ptr}+${len}`);
    return new Uint8Array(buf, ptr, len);
  }
  str(ptr, len) { return new TextDecoder().decode(this.bytes(ptr, len)); }
  view() { return new DataView(this.memory.buffer); }

  // ---- gasm imports --------------------------------------------------------
  gasmImports() {
    return {
      log: (ptr, len) => this.onLog(`[guest] ${this.str(ptr, len)}`),
      time_ms: () => this.virtualTime ? this.frameIndex * 1000 / this.frameRate : performance.now() - this.t0,
      set_frame_rate: (hz) => { if (Number.isFinite(hz) && hz >= 1 && hz <= 1000) this.frameRate = hz; },
      video_present: (ptr, w, h, stride) => {
        if (!w || !h || w > 4096 || h > 4096 || stride < w * 4) throw new Error(`video_present: bad geometry ${w}x${h} stride ${stride}`);
        const src = this.bytes(ptr, stride * (h - 1) + w * 4);
        if (this.rgba.length !== w * h * 4) this.rgba = new Uint8ClampedArray(w * h * 4);
        for (let y = 0; y < h; y++) this.rgba.set(src.subarray(y * stride, y * stride + w * 4), y * w * 4);
        if (this.hashing) this.videoHash = fnv32(this.videoHash, this.rgba);
        this.width = w; this.height = h; this.framesPresented++;
        this.onPresent(this.rgba, w, h);
      },
      audio_config: (rate, channels) => {
        if (rate >= 8000 && rate <= 192000 && (channels === 1 || channels === 2)) {
          this.audioRate = rate; this.audioChannels = channels;
        }
      },
      audio_push: (ptr, frames) => {
        const bytes = this.bytes(ptr, frames * this.audioChannels * 4);
        if (this.hashing) this.audioHash = fnv32(this.audioHash, bytes);
        this.audioFrames += frames;
        // copy: guest memory may be reused or grow (detaching the view)
        const samples = new Float32Array(bytes.slice().buffer);
        this.onAudio(samples, this.audioRate, this.audioChannels);
      },
      input_pad: (player) => player < 4 ? (this.getPad(player) >>> 0) : 0,
      param: (ptr, len, dst, cap) => {
        const v = this.params[this.str(ptr, len)];
        if (v === undefined) return -1;
        const b = new TextEncoder().encode(String(v));
        if (b.length <= cap >>> 0) this.bytes(dst, b.length).set(b);
        return b.length;
      },
      asset_read_at: (ptr, len, offset, dst, cap) => {
        const name = this.str(ptr, len);
        const size = this.assets.size(name);
        if (size < 0) return -1;
        const n = Math.max(0, Math.min(size - (offset >>> 0), cap >>> 0));
        return this.assets.readAt(name, offset >>> 0, this.bytes(dst, n));
      },
      asset_size: (ptr, len) => this.assets.size(this.str(ptr, len)),
      asset_read: (ptr, len, dst, cap) => {
        const name = this.str(ptr, len);
        const size = this.assets.size(name);
        if (size < 0) return -1;
        return this.assets.readAt(name, 0, this.bytes(dst, Math.min(size, cap >>> 0)));
      },
    };
  }

  // ---- gasm:gfx --------------------------------------------------------------
  gfxImports() {
    const g = this.gfx;
    const json = (ptr, len) => {
      const text = this.str(ptr, len);
      try { return JSON.parse(text); } catch (e) { throw new Error(`gfx: invalid JSON descriptor: ${e.message}: ${text.slice(0, 200)}`); }
    };
    return {
      width: () => g.width(),
      height: () => g.height(),
      create_shader: (ptr, len) => g.createShader(this.str(ptr, len)),
      create_buffer: (size, usage) => g.createBuffer(size >>> 0, usage >>> 0),
      create_pipeline: (ptr, len) => g.createPipeline(json(ptr, len)),
      create_bind_group: (ptr, len) => g.createBindGroup(json(ptr, len)),
      write_buffer: (buf, offset, ptr, len) => {
        if ((offset | len) & 3) throw new Error(`gfx.write_buffer: offset ${offset} and length ${len} must be multiples of 4`);
        const bytes = this.bytes(ptr, len);
        if (this.hashing) this.videoHash = fnv32(this.videoHash, bytes);
        g.writeBuffer(buf, offset >>> 0, bytes);
      },
      begin_frame: (r, gr, b, a) => (g.beginFrame(r, gr, b, a, this.showFrame) ? 1 : 0),
      set_pipeline: (p) => g.setPipeline(p),
      set_bind_group: (i, bg) => g.setBindGroup(i, bg),
      set_vertex_buffer: (slot, buf, off) => g.setVertexBuffer(slot, buf, off >>> 0),
      set_index_buffer: (buf, fmt, off) => g.setIndexBuffer(buf, fmt, off >>> 0),
      draw: (vc, ic, fv, fi) => g.draw(vc >>> 0, ic >>> 0, fv >>> 0, fi >>> 0),
      draw_indexed: (ic, n, first, base, fi) => g.drawIndexed(ic >>> 0, n >>> 0, first >>> 0, base | 0, fi >>> 0),
      end_frame: () => { g.endFrame(); this.framesPresented++; },
    };
  }

  // ---- gasm:storage -------------------------------------------------------------
  storageImports() {
    const st = this.storage;
    return {
      get: (kp, kl, dst, cap) => {
        const v = st.get(this.str(kp, kl));
        if (!v) return -1;
        if (v.length <= cap >>> 0) this.bytes(dst, v.length).set(v);
        return v.length;
      },
      set: (kp, kl, vp, vl) => {
        const err = st.set(this.str(kp, kl), this.bytes(vp, vl).slice());
        if (err) { this.onLog(`[gasm] storage: ${err}`); return -1; }
        return 0;
      },
      delete: (kp, kl) => (st.delete(this.str(kp, kl)) ? 0 : -1),
    };
  }

  // ---- gasm:net ----------------------------------------------------------------
  netImports() {
    const n = this.net;
    return {
      open: (ptr, len) => n.open(this.str(ptr, len)),
      state: (c) => n.state(c),
      send: (c, ptr, len) => ((len >>> 0) === 0 ? -1 : n.send(c, this.bytes(ptr, len).slice())),
      recv: (c, dst, cap) => {
        const r = n.peek(c);
        if (typeof r === 'number') return r;          // 0 = nothing, -1 = closed
        if (r.length > (cap >>> 0)) return r.length;  // too big: stays queued
        this.bytes(dst, r.length).set(r);
        n.pop(c);
        return r.length;
      },
      close: (c) => n.close(c),
    };
  }

  // ---- minimal WASI preview1 (enough for wasi-libc stdio/malloc/clocks) ----
  wasiImports() {
    const ENOSYS = 52, EBADF = 8, ESPIPE = 70, SUCCESS = 0;
    const impl = {
      fd_write: (fd, iovs, iovsLen, nwrittenPtr) => {
        if (fd !== 1 && fd !== 2) return EBADF;
        const dv = this.view();
        let total = 0;
        for (let i = 0; i < iovsLen; i++) {
          const ptr = dv.getUint32(iovs + i * 8, true), len = dv.getUint32(iovs + i * 8 + 4, true);
          this.stdio[fd] += this.str(ptr, len);
          total += len;
        }
        let nl;
        while ((nl = this.stdio[fd].indexOf('\n')) >= 0) {
          this.onLog(this.stdio[fd].slice(0, nl));
          this.stdio[fd] = this.stdio[fd].slice(nl + 1);
        }
        dv.setUint32(nwrittenPtr, total, true);
        return SUCCESS;
      },
      fd_close: () => SUCCESS,
      fd_seek: () => ESPIPE,
      fd_fdstat_get: (fd, ptr) => {
        if (fd > 2) return EBADF;
        const dv = this.view();
        for (let i = 0; i < 24; i++) dv.setUint8(ptr + i, 0);
        dv.setUint8(ptr, 2); // filetype: character device
        return SUCCESS;
      },
      clock_time_get: (id, _precision, outPtr) => {
        const ms = id === 0 ? Date.now() : performance.now();
        this.view().setBigUint64(outPtr, BigInt(Math.round(ms * 1e6)), true);
        return SUCCESS;
      },
      random_get: (ptr, len) => { crypto.getRandomValues(this.bytes(ptr, len)); return SUCCESS; },
      args_sizes_get: (a, b) => { const dv = this.view(); dv.setUint32(a, 0, true); dv.setUint32(b, 0, true); return SUCCESS; },
      args_get: () => SUCCESS,
      environ_sizes_get: (a, b) => { const dv = this.view(); dv.setUint32(a, 0, true); dv.setUint32(b, 0, true); return SUCCESS; },
      environ_get: () => SUCCESS,
      proc_exit: (code) => { throw new ProcExit(code); },
    };
    // Anything else a guest imports exists but reports "not supported".
    return new Proxy(impl, { get: (t, name) => t[name] ?? (() => ENOSYS) });
  }

  // ---- lifecycle -----------------------------------------------------------
  async load(wasmBytes) {
    const imports = new Proxy({
      gasm: this.gasmImports(), 'gasm:gfx': this.gfxImports(), 'gasm:net': this.netImports(),
      'gasm:storage': this.storageImports(),
      wasi_snapshot_preview1: this.wasiImports(),
    }, {
      get: (t, mod) => t[mod] ?? new Proxy({}, { get: (_, n) => () => { throw new Error(`unsupported import ${String(mod)}.${String(n)}`); } }),
    });
    const { instance } = await WebAssembly.instantiate(wasmBytes, imports);
    const ex = instance.exports;
    if (!(ex.memory instanceof WebAssembly.Memory)) throw new Error('guest does not export `memory`');
    this.memory = ex.memory;
    if (ex._initialize) ex._initialize();
    const version = ex.gasm_abi_version();
    if (version !== ABI_VERSION) throw new Error(`guest targets gasm ABI v${version}, runner implements v${ABI_VERSION}`);
    const rc = ex.gasm_init();
    if (rc !== 0) throw new Error(`gasm_init failed with code ${rc}`);
    this.exports = ex;
  }

  frame() {
    try {
      this.exports.gasm_frame();
    } finally {
      this.frameIndex++; // a frame that exits or traps still counts (as in the other runners)
    }
  }

  // Best-effort "player is quitting" (optional gasm_exit export): games flush saves.
  exit() {
    const f = this.exports?.gasm_exit;
    this.exports = { ...this.exports, gasm_exit: undefined };
    if (f) { try { f(); } catch (e) { if (!(e instanceof ProcExit)) this.onLog(`[gasm] gasm_exit trapped: ${e.message}`); } }
  }
}

// Streaming linear resampler (same algorithm as runners/native/src/audio.rs).
export class Resampler {
  constructor(dstRate) { this.dstRate = dstRate; this.t = 0; this.prev = [0, 0]; }
  process(samples, srcRate, channels) {
    const step = srcRate / this.dstRate;
    const frames = samples.length / channels;
    const out = new Float32Array(Math.ceil((frames + 1) / step) * 2 + 4);
    let n = 0;
    for (let i = 0; i < frames; i++) {
      const l = samples[i * channels], r = channels === 2 ? samples[i * channels + 1] : l;
      while (this.t < 1) {
        out[n++] = this.prev[0] + (l - this.prev[0]) * this.t;
        out[n++] = this.prev[1] + (r - this.prev[1]) * this.t;
        this.t += step;
      }
      this.t -= 1;
      this.prev = [l, r];
    }
    return out.subarray(0, n);
  }
}

// gfx backend that draws nothing: headless runs and tests. Handles are real so
// guests behave identically; writes are hashed by GasmHost.
export class NullGfx {
  constructor(width = 1280, height = 720) { this.w = width; this.h = height; this.next = 1; }
  width() { return this.w; }
  height() { return this.h; }
  createShader() { return this.next++; }
  createBuffer() { return this.next++; }
  createPipeline() { return this.next++; }
  createBindGroup() { return this.next++; }
  writeBuffer() {}
  beginFrame() { return false; }
  setPipeline() {} setBindGroup() {} setVertexBuffer() {} setIndexBuffer() {}
  draw() {} drawIndexed() {}
  endFrame() {}
}

// gasm:net over the platform WebSocket. Handles > 0; messages are binary.
export class NetConnections {
  constructor(allowed, log) { this.allowed = allowed; this.log = log; this.conns = new Map(); this.next = 1; }
  open(url) {
    if (!this.allowed) { this.log(`[gasm] net: denied connection to ${url} (networking not enabled)`); return -1; }
    if (!/^wss?:\/\//.test(url) || typeof WebSocket === 'undefined') return -1;
    let ws;
    try { ws = new WebSocket(url); } catch (e) { this.log(`[gasm] net: ${url}: ${e.message}`); return -1; }
    ws.binaryType = 'arraybuffer';
    const c = { ws, queue: [], state: 0 };
    ws.onopen = () => { c.state = 1; };
    ws.onmessage = (e) => {
      if (e.data instanceof ArrayBuffer) c.queue.push(new Uint8Array(e.data));
      else c.queue.push(new TextEncoder().encode(String(e.data)));
    };
    ws.onerror = () => { if (c.state < 2) c.state = 3; };
    ws.onclose = () => { if (c.state !== 3) c.state = 2; };
    const h = this.next++;
    this.conns.set(h, c);
    return h;
  }
  state(h) { return this.conns.get(h)?.state ?? 3; }
  send(h, bytes) {
    const c = this.conns.get(h);
    if (!c || c.state !== 1) return -1;
    c.ws.send(bytes);
    return 0;
  }
  peek(h) {
    const c = this.conns.get(h);
    if (!c) return -1;
    if (c.queue.length) return c.queue[0];
    return c.state >= 2 ? -1 : 0;
  }
  pop(h) { this.conns.get(h)?.queue.shift(); }
  close(h) {
    const c = this.conns.get(h);
    if (c) { c.ws.close(); this.conns.delete(h); }
  }
  // Close every connection with a proper handshake and wait (bounded) for it,
  // so queued messages are delivered before a headless process exits.
  closeAll(timeoutMs = 1000) {
    const waits = [...this.conns.values()].map((c) => new Promise((resolve) => {
      if (c.ws.readyState >= 2) return resolve();
      c.ws.addEventListener('close', resolve, { once: true });
      c.ws.close();
    }));
    this.conns.clear();
    return Promise.race([Promise.all(waits), new Promise((r) => setTimeout(r, timeoutMs))]);
  }
}

// gasm:storage rules (same as runners/native/src/storage.rs).
export const STORAGE_MAX_VALUE = 1 << 20, STORAGE_QUOTA = 16 << 20;
export const validKey = (k) => /^[A-Za-z0-9._-]{1,128}$/.test(k) && k !== '.' && k !== '..';

// In-memory store: headless runs (reproducible) and the base for IdbStorage.
export class MemoryStorage {
  constructor(entries = []) { this.map = new Map(entries); }
  get(k) { return this.map.get(k); }
  set(k, v) {
    if (!validKey(k)) return `invalid key ${JSON.stringify(k)}`;
    if (v.length > STORAGE_MAX_VALUE) return `value for ${k} is ${v.length} bytes (max ${STORAGE_MAX_VALUE})`;
    let used = 0;
    for (const [key, val] of this.map) if (key !== k) used += key.length + val.length;
    if (used + k.length + v.length > STORAGE_QUOTA) return `storage quota of ${STORAGE_QUOTA} bytes exceeded`;
    this.map.set(k, v);
    this.persist?.('put', k, v);
    return null;
  }
  delete(k) {
    if (!this.map.delete(k)) return false;
    this.persist?.('delete', k);
    return true;
  }
}

// Browser store: IndexedDB database "gasm", one object store record per
// (namespace, key). Loaded fully before the game starts so reads are
// synchronous; writes are persisted in the background.
export class IdbStorage extends MemoryStorage {
  static async open(namespace) {
    const db = await new Promise((resolve, reject) => {
      const req = indexedDB.open('gasm', 1);
      req.onupgradeneeded = () => req.result.createObjectStore('kv');
      req.onsuccess = () => resolve(req.result);
      req.onerror = () => reject(req.error);
    });
    const prefix = `${namespace}/`;
    const entries = await new Promise((resolve, reject) => {
      const out = [];
      const range = IDBKeyRange.bound(prefix, `${prefix}\uffff`);
      const req = db.transaction('kv').objectStore('kv').openCursor(range);
      req.onsuccess = () => {
        const c = req.result;
        if (!c) return resolve(out);
        out.push([String(c.key).slice(prefix.length), new Uint8Array(c.value)]);
        c.continue();
      };
      req.onerror = () => reject(req.error);
    });
    const s = new IdbStorage(entries);
    s.persist = (op, k, v) => {
      const store = db.transaction('kv', 'readwrite').objectStore('kv');
      if (op === 'put') store.put(v, prefix + k); else store.delete(prefix + k);
    };
    return s;
  }
}

// ---- assets ------------------------------------------------------------------------
// A GasmAssetProvider is synchronous (the ABI is): { size(name) -> bytes or -1,
// readAt(name, offset, dst: Uint8Array) -> bytes copied or -1 }. AssetTable
// implements the same naming rules as the native runner (runners/native/src/assets.rs):
// exact names win (explicit entries over folder entries); folder entries also match
// case-insensitively (ASCII), ties resolved by the first name in sorted order.

export const isAssetProvider = (a) => a && typeof a.size === 'function' && typeof a.readAt === 'function';
const asciiLower = (s) => s.replace(/[A-Z]/g, (c) => c.toLowerCase());
const hidden = (segments) => segments.some((seg) => seg.startsWith('.'));

/** An asset source: { size() -> number, readAt(offset, dst) -> bytes copied }. */
export const bytesSource = (u8) => ({
  size: () => u8.length,
  readAt: (offset, dst) => {
    const n = Math.max(0, Math.min(dst.length, u8.length - offset));
    if (n > 0) dst.set(u8.subarray(offset, offset + n));
    return n;
  },
});

export class AssetTable {
  constructor(log = () => {}) { this.exact = new Map(); this.folded = new Map(); this.log = log; }
  /** Add a source. Explicit entries replace; folder entries never replace an existing name. */
  add(name, source, { fromDir = false } = {}) {
    if (fromDir && this.exact.has(name)) return false;
    this.exact.set(name, { source, fromDir });
    return true;
  }
  /** Merge another table's entries (as folder entries if `fromDir`). */
  merge(table, { fromDir = true } = {}) {
    for (const [name, e] of table.exact) this.add(name, e.source, { fromDir: fromDir || e.fromDir });
    return this.finish();
  }
  /** Build the case-insensitive index; warns about names that differ only in case. */
  finish() {
    this.folded.clear();
    for (const [name, e] of this.exact) {
      if (!e.fromDir) continue;
      const k = asciiLower(name);
      (this.folded.get(k) ?? this.folded.set(k, []).get(k)).push(name);
    }
    for (const names of this.folded.values()) {
      names.sort();
      if (names.length > 1) this.log(`[gasm] assets: ${names.join(', ')} differ only in case; case-insensitive lookups use "${names[0]}"`);
    }
    return this;
  }
  resolve(name) {
    const e = this.exact.get(name);
    if (e) return e;
    const k = this.folded.get(asciiLower(name));
    return k ? this.exact.get(k[0]) : undefined;
  }
  size(name) { const e = this.resolve(name); return e ? e.source.size() : -1; }
  readAt(name, offset, dst) { const e = this.resolve(name); return e ? e.source.readAt(offset, dst) : -1; }
  names() { return [...this.exact.keys()].sort(); }
}

/** In-memory provider from { name: Uint8Array } (explicit entries: exact names only). */
export function memoryAssets(record = {}) {
  const t = new AssetTable();
  for (const [name, bytes] of Object.entries(record)) t.add(name, bytesSource(bytes));
  return t.finish();
}

const joinName = (prefix, rel) => (prefix ? `${prefix.replace(/\/+$/, '')}/${rel}` : rel);

/**
 * Folder from showDirectoryPicker() (Chromium). Preloads every file into memory
 * (main-thread mode); in Worker mode prefer fileAssets()/opfsAssets() for lazy reads.
 * onProgress({ done, total, bytes, name }) is called per file.
 */
export async function directoryHandleAssets(handle, { prefix = '', onProgress, log } = {}) {
  return preload(await directoryHandleEntries(handle), prefix, onProgress, log);
}

/**
 * Folder from <input type="file" webkitdirectory> (all browsers). The leading
 * root-folder segment of webkitRelativePath is stripped, so names match the
 * native runner's --asset-dir. Preloads into memory (see directoryHandleAssets).
 */
export async function fileListAssets(fileList, { prefix = '', onProgress, log } = {}) {
  return preload(fileListEntries(fileList), prefix, onProgress, log);
}

/** [relative name, File] pairs from a webkitdirectory FileList (root segment stripped, hidden skipped). */
export function fileListEntries(fileList) {
  const out = [];
  for (const f of fileList) {
    const segs = (f.webkitRelativePath || f.name).split('/');
    const rel = segs.length > 1 ? segs.slice(1) : segs;
    if (!hidden(rel)) out.push([rel.join('/'), f]);
  }
  return out.sort((a, b) => (a[0] < b[0] ? -1 : a[0] > b[0] ? 1 : 0));
}

/** Preload [name, File|Blob] entries into memory as folder assets (main-thread mode). */
export async function preloadAssets(entries, { prefix = '', onProgress, log } = {}) {
  return preload(entries, prefix, onProgress, log);
}

/** [relative name, File] pairs from a showDirectoryPicker() handle (sorted, hidden skipped). */
export async function directoryHandleEntries(handle) {
  const files = [];
  const walk = async (dir, segs) => {
    const entries = [];
    for await (const [name, h] of dir.entries()) entries.push([name, h]);
    entries.sort((a, b) => (a[0] < b[0] ? -1 : a[0] > b[0] ? 1 : 0));
    for (const [name, h] of entries) {
      const s = [...segs, name];
      if (hidden(s)) continue;
      if (h.kind === 'directory') await walk(h, s);
      else files.push([s.join('/'), await h.getFile()]);
    }
  };
  await walk(handle, []);
  return files;
}

async function preload(files, prefix, onProgress, log) {
  const t = new AssetTable(log);
  let bytes = 0;
  for (let i = 0; i < files.length; i++) {
    const [rel, file] = files[i];
    const data = new Uint8Array(await file.arrayBuffer());
    bytes += data.length;
    t.add(joinName(prefix, rel), bytesSource(data), { fromDir: true });
    onProgress?.({ done: i + 1, total: files.length, bytes, name: rel });
  }
  return t.finish();
}

/**
 * Worker mode only: lazy, synchronous reads from File/Blob objects via FileReaderSync
 * (e.g. a picked folder posted to the worker). entries: [[name, File], ...].
 */
export function fileAssets(entries, { prefix = '', log } = {}) {
  if (typeof FileReaderSync === 'undefined') throw new Error('fileAssets needs a Worker (FileReaderSync)');
  const reader = new FileReaderSync();
  const t = new AssetTable(log);
  for (const [rel, file] of entries) {
    t.add(joinName(prefix, rel), {
      size: () => file.size,
      readAt: (offset, dst) => {
        const n = Math.max(0, Math.min(dst.length, file.size - offset));
        if (n > 0) dst.set(new Uint8Array(reader.readAsArrayBuffer(file.slice(offset, offset + n))));
        return n;
      },
    }, { fromDir: true });
  }
  return t.finish();
}

/**
 * Worker mode only: a directory in the origin private file system, read lazily and
 * synchronously through FileSystemSyncAccessHandle. Nothing is preloaded: reads go
 * straight from OPFS into guest memory. Handles are opened once, while loading:
 * createSyncAccessHandle() is async and a synchronous read can't await it without
 * SharedArrayBuffer (which would need COOP/COEP headers). Opening is cheap (~450 files
 * for a CD), and each handle is then reused for every read.
 * `dir` is a path like "openrf-cd" (relative to the OPFS root) or a directory handle.
 */
export async function opfsAssets(dir, { prefix = '', log } = {}) {
  let handle = dir;
  if (typeof dir === 'string') {
    handle = await navigator.storage.getDirectory();
    for (const seg of dir.split('/').filter(Boolean)) handle = await handle.getDirectoryHandle(seg);
  }
  const t = new AssetTable(log);
  const walk = async (d, segs) => {
    const entries = [];
    for await (const [name, h] of d.entries()) entries.push([name, h]);
    entries.sort((a, b) => (a[0] < b[0] ? -1 : a[0] > b[0] ? 1 : 0));
    for (const [name, h] of entries) {
      const s = [...segs, name];
      if (hidden(s)) continue;
      if (h.kind === 'directory') { await walk(h, s); continue; }
      const access = await h.createSyncAccessHandle();
      const size0 = access.getSize();
      t.add(joinName(prefix, s.join('/')), {
        size: () => { try { return access.getSize(); } catch { return size0; } },
        readAt: (offset, dst) => (dst.length ? access.read(dst, { at: offset }) : 0),
      }, { fromDir: true });
    }
  };
  await walk(handle, []);
  return t.finish();
}

// ---- keyboard layouts --------------------------------------------------------------
// One text format for both runners (gasm-run --keymap FILE, the web player's "keys"
// editor). A line per binding: <pad 1-4> <button> <key code> [<key code> ...].
// Buttons: a b x y l r select start up down left right. Key codes are
// KeyboardEvent.code names (KeyX, ArrowUp, Period, ControlRight, NumpadEnter...).
// Keyboard bindings for pad N >= 2 apply while fewer than N gamepads are connected.

export const BUTTONS = ['a', 'b', 'x', 'y', 'l', 'r', 'select', 'start', 'up', 'down', 'left', 'right'];

export const DEFAULT_KEYMAP = `# gasm keyboard layout: <pad> <button> <key code>...  (KeyboardEvent.code names)
# player 1
1 up ArrowUp
1 down ArrowDown
1 left ArrowLeft
1 right ArrowRight
1 a KeyX
1 b KeyZ
1 x KeyS
1 y KeyA
1 l KeyQ
1 r KeyW
1 select ShiftRight
1 start Enter
# player 2 (used while fewer than two gamepads are connected)
2 up KeyI
2 down KeyK
2 left KeyJ
2 right KeyL
2 a Period
2 b Comma
2 x KeyM
2 y KeyN
2 l KeyU
2 r KeyO
2 select Backspace
2 start ControlRight NumpadEnter
`;

/** Parse a keymap. Returns { bindings: Map<code, [{pad, bit}]>, errors: string[] }. */
export function parseKeymap(text) {
  const bindings = new Map(), errors = [];
  text.split('\n').forEach((raw, i) => {
    const line = raw.replace(/#.*/, '').trim();
    if (!line) return;
    const [pad, button, ...keys] = line.split(/\s+/);
    const p = Number(pad), bit = BUTTONS.indexOf((button ?? '').toLowerCase());
    if (!(p >= 1 && p <= 4) || bit < 0 || !keys.length) {
      errors.push(`line ${i + 1}: expected "<pad 1-4> <button> <key>...", got "${line}"`);
      return;
    }
    for (const code of keys) {
      if (code === 'Escape') { errors.push(`line ${i + 1}: Escape is reserved (quit)`); continue; }
      (bindings.get(code) ?? bindings.set(code, []).get(code)).push({ pad: p - 1, bit });
    }
  });
  return { bindings, errors };
}

/** Pads from held keys: pad N (N >= 1, zero-based) only while fewer than N+1 gamepads exist. */
export function keyboardPads(bindings, held, gamepads = 0) {
  const pads = [0, 0, 0, 0];
  for (const code of held) {
    for (const { pad, bit } of bindings.get(code) ?? []) {
      if (pad === 0 || gamepads <= pad) pads[pad] |= 1 << bit;
    }
  }
  return pads;
}
