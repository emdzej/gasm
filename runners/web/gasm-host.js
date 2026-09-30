// gasm-host.js — gasm ABI v0 host for JavaScript (browser and Node).
//
//   const host = new GasmHost({ assets, params, gfx, allowNet, onPresent, onAudio, onLog, getPad });
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
  constructor({ assets = {}, params = {}, gfx = new NullGfx(), allowNet = false,
                onPresent = () => {}, onAudio = () => {}, onLog = console.log,
                getPad = () => 0, virtualTime = false } = {}) {
    this.assets = assets;            // name -> Uint8Array
    this.params = params;            // name -> string
    this.gfx = gfx;
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
      asset_size: (ptr, len) => { const a = this.assets[this.str(ptr, len)]; return a ? a.length : -1; },
      asset_read: (ptr, len, dst, cap) => {
        const a = this.assets[this.str(ptr, len)];
        if (!a) return -1;
        const n = Math.min(a.length, cap);
        this.bytes(dst, n).set(a.subarray(0, n));
        return n;
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
    this.exports.gasm_frame();
    this.frameIndex++;
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
}
