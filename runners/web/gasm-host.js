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
    this.text = null;                // text typed since the previous frame (set per frame); null = no keyboard
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
      text_input: (dst, cap) => {
        if (this.text === null) return -1;
        const b = new TextEncoder().encode(this.text);
        if (b.length <= cap >>> 0) this.bytes(dst, b.length).set(b);
        return b.length;
      },
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
      asset_count: () => this.assetNames().length,
      asset_name: (index, dst, cap) => {
        const n = this.assetNames()[index >>> 0];
        if (n === undefined) return -1;
        const b = new TextEncoder().encode(n);
        if (b.length <= cap >>> 0) this.bytes(dst, b.length).set(b);
        return b.length;
      },
      asset_read: (ptr, len, dst, cap) => {
        const name = this.str(ptr, len);
        const size = this.assets.size(name);
        if (size < 0) return -1;
        return this.assets.readAt(name, 0, this.bytes(dst, Math.min(size, cap >>> 0)));
      },
    };
  }

  /** Asset names sorted by UTF-8 bytes (= code point order), like the native runner. */
  assetNames() {
    if (!this._assetNames) {
      const cp = (s) => Array.from(s, (c) => c.codePointAt(0));
      const cmp = (a, b) => { const x = cp(a), y = cp(b); for (let i = 0; i < Math.min(x.length, y.length); i++) if (x[i] !== y[i]) return x[i] - y[i]; return x.length - y.length; };
      this._assetNames = (typeof this.assets.names === 'function' ? this.assets.names() : []).slice().sort(cmp);
    }
    return this._assetNames;
  }

  // ---- gasm:gfx --------------------------------------------------------------
  gfxImports() {
    const g = this.gfx;
    const m = new GfxModel(g);   // validation shared by every backend (matches the native runner)
    const json = (ptr, len) => {
      const text = this.str(ptr, len);
      try { return JSON.parse(text); } catch (e) { throw new Error(`gfx: invalid JSON descriptor: ${e.message}: ${text.slice(0, 200)}`); }
    };
    const u32le = (vals) => { const b = new Uint8Array(vals.length * 4), dv = new DataView(b.buffer); vals.forEach((v, i) => dv.setUint32(i * 4, v >>> 0, true)); return b; };
    return {
      width: () => g.width(),
      height: () => g.height(),
      create_shader: (ptr, len) => m.add(g.createShader(this.str(ptr, len)), { kind: 'shader' }),
      create_buffer: (size, usage) => {
        const h = g.createBuffer(size >>> 0, usage >>> 0);
        return m.add(h, { kind: 'buffer', size: size >>> 0, usage: (usage | 0x08) >>> 0 });
      },
      create_pipeline: (ptr, len) => { const d = json(ptr, len); m.pipelineLayouts(d); return m.add(g.createPipeline(d), { kind: 'pipeline' }); },
      create_bind_group: (ptr, len) => { const d = json(ptr, len); const meta = m.bindGroup(d); return m.add(g.createBindGroup(d), meta); },
      create_bind_group_layout: (ptr, len) => { const d = json(ptr, len); const meta = m.layout(d); return m.add(g.createBindGroupLayout(d), meta); },
      create_texture: (ptr, len) => { const d = json(ptr, len); const meta = m.texture(d); return m.add(g.createTexture(d, meta), meta); },
      create_sampler: (ptr, len) => { const d = json(ptr, len); m.sampler(d); return m.add(g.createSampler(d), { kind: 'sampler' }); },
      write_buffer: (buf, offset, ptr, len) => {
        if ((offset | len) & 3) throw new Error(`gfx.write_buffer: offset ${offset} and length ${len} must be multiples of 4`);
        const bytes = this.bytes(ptr, len);
        if (this.hashing) this.videoHash = fnv32(this.videoHash, bytes);
        g.writeBuffer(buf, offset >>> 0, bytes);
      },
      write_texture: (tex, mip, x, y, w, h, ptr, len) => {
        const bytes = this.bytes(ptr, len);
        m.writeTexture(tex, mip >>> 0, x >>> 0, y >>> 0, w >>> 0, h >>> 0, bytes.length);
        g.writeTexture(tex, mip >>> 0, x >>> 0, y >>> 0, w >>> 0, h >>> 0, bytes);
        if (this.hashing) this.videoHash = fnv32(fnv32(this.videoHash, u32le([tex, mip, x, y, w, h])), bytes);
      },
      begin_frame: (r, gr, b, a) => (g.beginFrame(r, gr, b, a, this.showFrame) ? 1 : 0),
      set_pipeline: (p) => g.setPipeline(p),
      set_bind_group: (i, bg) => g.setBindGroup(i, bg),
      set_bind_group_offsets: (i, bg, ptr, count) => {
        const b = this.bytes(ptr, (count >>> 0) * 4), dv = new DataView(b.buffer, b.byteOffset, b.byteLength);
        const offsets = Uint32Array.from({ length: count >>> 0 }, (_, k) => dv.getUint32(k * 4, true));
        m.offsets(bg, offsets);
        g.setBindGroupOffsets(i, bg, offsets);
      },
      set_viewport: (x, y, w, h, min, max) => {
        if (![x, y, w, h, min, max].every(Number.isFinite) || w < 0 || h < 0) throw new Error(`gfx.set_viewport: invalid rectangle ${x},${y} ${w}x${h}`);
        if (min < 0 || max > 1 || min > max) throw new Error(`gfx.set_viewport: depth range ${min}..${max} must be within 0..1`);
        g.setViewport(...clampRect(x, y, w, h, g.width(), g.height()), min, max);
      },
      set_scissor_rect: (x, y, w, h) => g.setScissorRect(...clampRect(x >>> 0, y >>> 0, w >>> 0, h >>> 0, g.width(), g.height())),
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

// Clamp [x, x+w) x [y, y+h) to a width x height drawable -> [x, y, w, h].
function clampRect(x, y, w, h, width, height) {
  const cl = (v, hi) => Math.min(Math.max(v, 0), hi);
  const x0 = cl(x, width), y0 = cl(y, height);
  return [x0, y0, cl(x + w, width) - x0, cl(y + h, height) - y0];
}

export const MAX_TEXTURE_SIZE = 8192;   // WebGPU's default maxTextureDimension2D
export const OFFSET_ALIGNMENT = 256;    // dynamic offsets (WebGPU's default alignment limits)

// Backend-independent record of every gfx object, so textures, samplers, layouts and
// dynamic offsets are validated (and trap) the same way on WebGPU, the null backend
// and the native runner (runners/native/src/gfx.rs, Meta).
export class GfxModel {
  constructor(backend) { this.backend = backend; this.meta = new Map(); }
  add(h, meta) { this.meta.set(h, meta); return h; }
  expect(h, kind) {
    const m = this.meta.get(h);
    if (!m) throw new Error(`gfx: invalid handle ${h}`);
    if (m.kind !== kind) throw new Error(`gfx: handle ${h} is a ${m.kind}, not a ${kind}`);
    return m;
  }
  pipelineLayouts(d) {
    if (d.layout === undefined || d.layout === 'auto') return;
    if (!Array.isArray(d.layout)) throw new Error(`pipeline: layout must be "auto" or an array of bind group layout handles`);
    for (const h of d.layout) this.expect(h, 'bind group layout');
  }
  layout(d) {
    const entries = [];
    for (const e of d.entries ?? []) {
      const b = e.binding;
      if (!Number.isInteger(b) || b < 0) throw new Error('descriptor: missing number "binding"');
      if (entries.some((x) => x.binding === b)) throw new Error(`bind group layout: binding ${b} appears twice`);
      const v = e.visibility;
      if (!Number.isInteger(v) || v === 0 || (v & ~3)) throw new Error(`bind group layout: binding ${b}: visibility ${v} must be GASM_STAGE_VERTEX (1) and/or GASM_STAGE_FRAGMENT (2)`);
      if (e.buffer) {
        const t = e.buffer.type ?? 'uniform';
        if (t !== 'uniform' && t !== 'read-only-storage') throw new Error(`bind group layout: binding ${b}: unsupported buffer type ${JSON.stringify(t)}`);
        entries.push({ binding: b, slot: t === 'uniform' ? 'uniform' : 'storage', dynamic: !!e.buffer.hasDynamicOffset, minBindingSize: e.buffer.minBindingSize ?? 0 });
      } else if (e.texture) {
        const st = e.texture.sampleType ?? 'float';
        if (st !== 'float' && st !== 'unfilterable-float') throw new Error(`bind group layout: binding ${b}: unsupported sampleType ${JSON.stringify(st)}`);
        if ((e.texture.viewDimension ?? '2d') !== '2d') throw new Error(`bind group layout: binding ${b}: only viewDimension "2d" is supported`);
        if (e.texture.multisampled) throw new Error(`bind group layout: binding ${b}: multisampled textures are not supported`);
        entries.push({ binding: b, slot: 'texture' });
      } else if (e.sampler) {
        const st = e.sampler.type ?? 'filtering';
        if (st !== 'filtering' && st !== 'non-filtering') throw new Error(`bind group layout: binding ${b}: unsupported sampler type ${JSON.stringify(st)}`);
        entries.push({ binding: b, slot: 'sampler' });
      } else {
        throw new Error(`bind group layout: binding ${b} needs "buffer", "texture" or "sampler"`);
      }
    }
    entries.sort((a, b) => a.binding - b.binding);
    return { kind: 'bind group layout', entries };
  }
  bindGroup(d) {
    const resolved = [];
    for (const e of d.entries ?? []) {
      const b = e.binding;
      if (!Number.isInteger(b) || b < 0) throw new Error('descriptor: missing number "binding"');
      if (resolved.some((r) => r.binding === b)) throw new Error(`bind group: binding ${b} appears twice`);
      if (e.buffer !== undefined) {
        const buf = this.expect(e.buffer, 'buffer'), offset = e.offset ?? 0;
        if (offset + (e.size ?? 0) > buf.size || offset > buf.size) throw new Error(`bind group: binding ${b}: range ${offset}+${e.size ?? 0} exceeds buffer size ${buf.size}`);
        resolved.push({ binding: b, slot: 'buffer', buf, offset, size: e.size ?? buf.size - offset, handle: e.buffer });
      } else if (e.texture !== undefined) {
        this.expect(e.texture, 'texture'); resolved.push({ binding: b, slot: 'texture' });
      } else if (e.sampler !== undefined) {
        this.expect(e.sampler, 'sampler'); resolved.push({ binding: b, slot: 'sampler' });
      } else {
        throw new Error(`bind group: binding ${b} needs a "buffer", "texture" or "sampler"`);
      }
    }
    const dynamic = [];
    if (d.layout !== undefined) {
      const { entries } = this.expect(d.layout, 'bind group layout');
      if (resolved.length !== entries.length) throw new Error(`bind group: layout ${d.layout} has ${entries.length} entries, got ${resolved.length}`);
      for (const le of entries) {
        const r = resolved.find((x) => x.binding === le.binding);
        if (!r) throw new Error(`bind group: missing binding ${le.binding}`);
        const isBuffer = le.slot === 'uniform' || le.slot === 'storage';
        if ((isBuffer && r.slot !== 'buffer') || (!isBuffer && r.slot !== le.slot)) throw new Error(`bind group: binding ${le.binding} has the wrong resource kind for its layout`);
        if (isBuffer) {
          const need = le.slot === 'uniform' ? 0x40 : 0x80;
          if (!(r.buf.usage & need)) throw new Error(`bind group: binding ${le.binding}: buffer ${r.handle} lacks ${need === 0x40 ? 'UNIFORM' : 'STORAGE'} usage`);
          if (r.size < le.minBindingSize) throw new Error(`bind group: binding ${le.binding}: size ${r.size} is below minBindingSize ${le.minBindingSize}`);
          if (le.dynamic) dynamic.push({ bufferSize: r.buf.size, offset: r.offset, size: r.size });
        }
      }
    } else {
      if (d.pipeline === undefined) throw new Error('bind group: needs "layout" or "pipeline"');
      this.expect(d.pipeline, 'pipeline');
    }
    return { kind: 'bind group', dynamic };
  }
  texture(d) {
    const [w, h] = Array.isArray(d.size) ? d.size : [];
    const ok = (v) => Number.isInteger(v) && v >= 1 && v <= MAX_TEXTURE_SIZE;
    if (!Array.isArray(d.size) || d.size.length !== 2 || !ok(w) || !ok(h)) throw new Error(`texture: size must be [width, height], each 1-${MAX_TEXTURE_SIZE}`);
    const format = d.format ?? 'rgba8unorm';
    if (format !== 'rgba8unorm' && format !== 'rgba8unorm-srgb') throw new Error(`texture: unsupported format ${JSON.stringify(format)} (rgba8unorm, rgba8unorm-srgb)`);
    const maxMips = 32 - Math.clz32(Math.max(w, h)), mips = d.mipLevelCount ?? 1;
    if (!Number.isInteger(mips) || mips < 1 || mips > maxMips) throw new Error(`texture: mipLevelCount ${mips} must be 1-${maxMips} for ${w}x${h}`);
    return { kind: 'texture', width: w, height: h, mips, format };
  }
  writeTexture(h, mip, x, y, w, ht, len) {
    const t = this.expect(h, 'texture');
    if (mip >= t.mips) throw new Error(`gfx.write_texture: mip ${mip} out of range (texture has ${t.mips})`);
    const lw = Math.max(1, t.width >> mip), lh = Math.max(1, t.height >> mip);
    if (!w || !ht || x + w > lw || y + ht > lh) throw new Error(`gfx.write_texture: region ${x},${y} ${w}x${ht} is outside mip ${mip} (${lw}x${lh})`);
    if (len !== w * ht * 4) throw new Error(`gfx.write_texture: len ${len} must be width*height*4 = ${w * ht * 4}`);
  }
  sampler(d) {
    const addr = (k) => { const v = d[k] ?? 'clamp-to-edge'; if (!['clamp-to-edge', 'repeat', 'mirror-repeat'].includes(v)) throw new Error(`sampler: unsupported ${k} ${JSON.stringify(v)}`); };
    const lin = (k) => { const v = d[k] ?? 'nearest'; if (v !== 'nearest' && v !== 'linear') throw new Error(`sampler: unsupported ${k} ${JSON.stringify(v)}`); return v === 'linear'; };
    addr('addressModeU'); addr('addressModeV');
    const all = [lin('magFilter'), lin('minFilter'), lin('mipmapFilter')].every(Boolean);
    const lmin = d.lodMinClamp ?? 0, lmax = d.lodMaxClamp ?? 32;
    if (!(lmin >= 0 && lmax >= lmin)) throw new Error(`sampler: lodMinClamp ${lmin} / lodMaxClamp ${lmax} must satisfy 0 <= min <= max`);
    const a = d.maxAnisotropy ?? 1;
    if (!Number.isInteger(a) || a < 1 || a > 16) throw new Error(`sampler: maxAnisotropy ${a} must be 1-16`);
    if (a > 1 && !all) throw new Error('sampler: maxAnisotropy > 1 needs linear magFilter, minFilter and mipmapFilter');
  }
  offsets(h, offsets) {
    const { dynamic } = this.expect(h, 'bind group');
    if (offsets.length !== dynamic.length) throw new Error(`gfx.set_bind_group_offsets: bind group ${h} has ${dynamic.length} dynamic entries, got ${offsets.length} offsets`);
    offsets.forEach((o, i) => {
      const e = dynamic[i];
      if (o % OFFSET_ALIGNMENT) throw new Error(`gfx.set_bind_group_offsets: offset ${o} is not a multiple of ${OFFSET_ALIGNMENT}`);
      if (o + e.offset + e.size > e.bufferSize) throw new Error(`gfx.set_bind_group_offsets: offset ${o} + binding ${e.offset}+${e.size} exceeds buffer size ${e.bufferSize}`);
    });
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
  createBindGroupLayout() { return this.next++; }
  createTexture() { return this.next++; }
  createSampler() { return this.next++; }
  writeBuffer() {}
  writeTexture() {}
  beginFrame() { return false; }
  setPipeline() {} setBindGroup() {} setBindGroupOffsets() {} setVertexBuffer() {} setIndexBuffer() {}
  setViewport() {} setScissorRect() {}
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
