// GasmHost: the gasm ABI v0 host for JavaScript (browser and Node). Implements the
// "gasm", "gasm:gfx", "gasm:net" and "gasm:storage" imports and the WASI subset
// (lib/wasi.js) exactly like runners/native/src/host.rs.

import { FileSaves } from './files.js';
import { AssetTable, isAssetProvider, memoryAssets } from './assets.js';
import { GfxModel, NullGfx, clampRect } from './gfx.js';
import { GlHost, glImports } from './gl.js';
import { GAMEPAD_AXES, GAMEPAD_BUTTONS, GAMEPAD_BYTES, KEY_STATE_BYTES, POINTER_BYTES, framePosition } from './input.js';
import { FetchRequests } from './fetch.js';
import { NetConnections } from './net.js';
import { MemoryStorage, StorageError, STORAGE_ERR_IO } from './storage.js';
import { ProcExit, Splitmix, WASI_IMPLEMENTED, wasiImports } from './wasi.js';

export const ABI_VERSION = 0;

// FNV-1a 32-bit, identical to runners/native/src/host.rs
export function fnv32(h, bytes) {
  for (let i = 0; i < bytes.length; i++) h = Math.imul(h ^ bytes[i], 0x01000193) >>> 0;
  return h;
}
export const FNV_INIT = 0x811c9dc5;

/** Headless virtual time: derived from the frame number, monotonic when the guest
 *  changes its frame rate (same arithmetic as host.rs VirtualClock). */
export class VirtualClock {
  constructor() { this.baseMs = 0; this.baseFrame = 0; this.rate = 60; }
  at(frame, rate) {
    if (rate !== this.rate) {
      this.baseMs += (frame - this.baseFrame) * 1000 / this.rate;
      this.baseFrame = frame;
      this.rate = rate;
    }
    return this.baseMs + (frame - this.baseFrame) * 1000 / this.rate;
  }
}

const UTF8 = new TextDecoder('utf-8', { fatal: true });
const LOSSY = new TextDecoder();
const ENC = new TextEncoder();
const NO_INPUT = Object.freeze({ keys: null, keyEvents: [], pointer: null, gamepads: null });

export { ProcExit };

/** Longest title set_title keeps, in UTF-8 bytes. */
export const TITLE_MAX_BYTES = 256;
const BIDI_OR_CONTROL = /[\u0000-\u001f\u007f-\u009f\u202a-\u202e\u2066-\u2069]/gu;
/**
 * A guest's set_title text as runners show it: control characters (Unicode Cc)
 * and bidi controls removed, then cut to 256 UTF-8 bytes at a character
 * boundary; null if empty (the runner's default). Same rules as
 * runners/native/src/host.rs clean_title.
 */
export function cleanTitle(text) {
  let out = '', bytes = 0;
  for (const ch of text.replace(BIDI_OR_CONTROL, '')) {
    const cp = ch.codePointAt(0), n = cp < 0x80 ? 1 : cp < 0x800 ? 2 : cp < 0x10000 ? 3 : 4;
    if (bytes + n > TITLE_MAX_BYTES) break;
    out += ch; bytes += n;
  }
  return out || null;
}

/** The module's built-in title (custom section gasm.title), cleaned like set_title; null if none. */
export function staticTitle(module) {
  const [s] = WebAssembly.Module.customSections(module, 'gasm.title');
  if (!s) return null;
  try { return cleanTitle(new TextDecoder('utf-8', { fatal: true }).decode(s)); } catch { return null; }
}

/** Whether this JS engine can suspend wasm (JSPI): gasm_run guests, design/stack-switching.md. */
export const STACK_SWITCHING = typeof WebAssembly.Suspending === 'function' && typeof WebAssembly.promising === 'function';

/** The default cap on guest memory, as natively (1 GiB). */
export const DEFAULT_MEMORY_LIMIT = 1 << 30;
/** The largest text gasm:clipboard passes either way (bytes; UTF-16 units for pasted text). */
export const CLIPBOARD_MAX = 1 << 20;
/** The trap message when a guest needs more memory than allowed (the same natively). */
export const memoryLimitMessage = (limit) => `the game needs more memory than its limit (${Math.floor(limit / 1048576)} MiB; see --memory-limit)`;

export class GasmHost {
  constructor({ assets = {}, params = {}, gfx = new NullGfx(), storage = new MemoryStorage(), allowNet = false,
                onPresent = () => {}, onAudio = () => {}, onLog = console.log, onTitle = () => {}, onCopyText = () => {}, onSaveFile = () => true,
                getPad = () => 0, virtualTime = false, stackSwitching = STACK_SWITCHING, gl = null,
                fetchReplay = null, fetchRecord = null, memoryLimit = DEFAULT_MEMORY_LIMIT, userAgent = null } = {}) {
    // GasmAssetProvider ({ size(name), readAt(name, offset, dst), names() }), or a plain
    // { name: Uint8Array } record (wrapped as an in-memory provider).
    this.assets = isAssetProvider(assets) ? assets : memoryAssets(assets);
    this.params = params;            // name -> string
    this.gfx = gfx;
    // gasm:gl: a WebGL2RenderingContext, or null for the null GL (headless)
    this.glContext = gl;
    this.gl = null;
    this.storage = storage;          // MemoryStorage (headless) or IdbStorage (browser)
    // allowNet: false, true or a list of host names (gasm:net and gasm:fetch)
    this.net = new NetConnections(allowNet, (m) => this.onLog(m));
    this.fetch = new FetchRequests(allowNet, (m) => this.onLog(m), { replay: fetchReplay, record: fetchRecord, userAgent });
    this.showFrame = true;           // false during catch-up frames: begin_frame returns 0
    this.catchUp = false;            // a catch-up frame (not the last of a batch): gasm:gl frame_shown 0
    this.onPresent = onPresent;      // (rgba: Uint8ClampedArray, w, h)
    this.onAudio = onAudio;          // (samples: Float32Array interleaved, rate, channels)
    this.onLog = onLog;
    this.onTitle = onTitle;
    // gasm:clipboard: (text) after a frame that copied text; put it on the clipboard
    this.onCopyText = onCopyText;
    this.pasted = null;              // the player's pasted text, during the frame that carries the paste key
    this.copied = null;
    // gasm:files: (name, mime, bytes) -> boolean | Promise<boolean>, after the frame; null: refuse saves
    this.files = new FileSaves(onSaveFile, (m) => this.onLog(m));          // (title: string | null) after a frame that changed it; null = default
    this.title = null;               // set_title, cleaned
    // gasm_run guests (JSPI): the guest's run is one suspended call; frames are async
    this.stackSwitching = stackSwitching && STACK_SWITCHING;
    this.switching = false;          // after load(): this guest runs through gasm_run
    this.running = null;             // the gasm_run promise
    this.resume = null;              // continues a guest suspended in yield_frame
    this.frameEnd = null;            // [resolve, reject] of the frame in progress
    this.staticTitle = null;         // the module's gasm.title section (the default title), after load()
    this.aspect = null;              // video_set_aspect [num, den]; null = square pixels
    this.titleChanged = false;
    this.getPad = getPad;            // (player) -> bitmask
    this.text = null;                // text typed since the previous frame (set per frame); null = no keyboard
    // Raw input for the next frame (set per frame by the runner; null = no such device):
    //   keys: Uint8Array(KEY_STATE_BYTES) bitset, keyEvents: [[code, down], ...],
    //   pointer: { x, y, dx, dy, wheelX, wheelY, buttons, pressed, released, flags, drawable: [w, h] },
    //   gamepads: [{ connected, standard, buttons: number[], axes: number[], name }] x 4
    this.input = NO_INPUT;
    this.inputMode = 0;              // GASM_INPUT_* flags requested by the guest
    // reproducible (headless) mode: frame-derived time (also WASI clocks), fixed random_get
    this.virtualTime = virtualTime;
    this.clock = new VirtualClock();
    this.vtime = 0;
    this.random = virtualTime ? new Splitmix() : null;
    this.frameRate = 60;
    this.audioRate = 44100;
    this.audioChannels = 2;
    this.frameIndex = 0;
    this.width = 0;
    this.height = 0;
    this.rgba = new Uint8ClampedArray(0);
    this.framesPresented = 0;        // video_present calls + gfx frames
    this.videoFrames = 0;            // video_present calls
    this.videoHash = FNV_INIT;
    this.audioHash = FNV_INIT;
    this.audioFrames = 0;
    this.hashing = virtualTime;      // hash only when asked (costs CPU)
    this.memory = null;
    this.exports = null;
    this.dead = null;                // why the guest can't be called any more (trapped, exited)
    this.memoryLimit = memoryLimit;  // bytes (0: no limit); checked after each guest call
    this.provided = new Set();       // what `has` reports
    this.t0 = performance.now();
  }

  // ---- memory helpers ------------------------------------------------------
  /** Views on guest memory, recreated only when it grows (the buffer changes). */
  views() {
    const buf = this.memory.buffer;
    if (this._buf !== buf) { this._buf = buf; this._u8 = new Uint8Array(buf); this._dv = new DataView(buf); }
  }
  bytes(ptr, len) {
    ptr >>>= 0; len >>>= 0;
    this.views();
    if (ptr + len > this._u8.length) throw new RangeError(`guest access out of bounds: ${ptr}+${len} (memory is ${this._u8.length})`);
    return this._u8.subarray(ptr, ptr + len);
  }
  view() { this.views(); return this._dv; }
  str(ptr, len) {
    try { return UTF8.decode(this.bytes(ptr, len)); } catch (e) {
      if (e instanceof RangeError) throw e;
      throw new Error('string argument is not UTF-8');
    }
  }
  /** The "copied only if it fits" convention: copy if src.length <= cap; returns the length. */
  copyIfFits(dst, cap, src) {
    if (src.length <= cap >>> 0) this.bytes(dst, src.length).set(src);
    return src.length;
  }

  // ---- gasm imports --------------------------------------------------------
  gasmImports() {
    const readAsset = (ptr, len, offset, dst, cap) => {
      const name = this.str(ptr, len);
      const size = this.assets.size(name);
      if (size < 0) return -1;
      const n = Math.max(0, Math.min(size - offset, cap >>> 0));
      return this.assets.readAt(name, offset, this.bytes(dst, n));
    };
    return {
      log: (ptr, len) => this.onLog(`[guest] ${LOSSY.decode(this.bytes(ptr, len))}`),
      has: (ptr, len) => (this.provided.has(this.str(ptr, len)) ? 1 : 0),
      // replaced by a suspending import for gasm_run guests (load)
      yield_frame: () => { throw new Error('gasm.yield_frame: only inside gasm_run'); },
      set_title: (ptr, len) => {
        const t = cleanTitle(this.str(ptr, len));
        if (t !== this.title) { this.title = t; this.titleChanged = true; }
      },
      time_ms: () => (this.virtualTime ? this.vtime : performance.now() - this.t0),
      // minutes east of UTC (getTimezoneOffset is west); headless runs are UTC
      utc_offset_minutes: () => (this.virtualTime ? 0 : -new Date().getTimezoneOffset()),
      set_frame_rate: (hz) => { if (Number.isFinite(hz) && hz >= 1 && hz <= 1000) this.frameRate = hz; },
      video_present: (ptr, w, h, stride) => {
        w >>>= 0; h >>>= 0; stride >>>= 0;
        if (!w || !h || w > 4096 || h > 4096 || stride < w * 4) throw new Error(`video_present: bad geometry ${w}x${h} stride ${stride}`);
        const src = this.bytes(ptr, stride * (h - 1) + w * 4), row = w * 4;
        if (this.rgba.length !== row * h) this.rgba = new Uint8ClampedArray(row * h);
        if (stride === row) this.rgba.set(src);
        else for (let y = 0; y < h; y++) this.rgba.set(src.subarray(y * stride, y * stride + row), y * row);
        if (this.hashing) this.videoHash = fnv32(this.videoHash, this.rgba);
        this.width = w; this.height = h; this.framesPresented++; this.videoFrames++;
        this.onPresent(this.rgba, w, h, this.aspect);
      },
      video_set_aspect: (num, den) => {
        num >>>= 0; den >>>= 0;
        if (num === 0 && den === 0) { this.aspect = null; return; }
        if (!(num >= 1 && num <= 65535 && den >= 1 && den <= 65535 && num * 8 >= den && den * 8 >= num)) {
          throw new Error(`video_set_aspect: invalid ratio ${num}:${den}`);
        }
        this.aspect = [num, den];
      },
      audio_config: (rate, channels) => {
        if (rate >= 8000 && rate <= 192000 && (channels === 1 || channels === 2)) {
          this.audioRate = rate; this.audioChannels = channels;
        }
      },
      audio_push: (ptr, frames) => {
        const bytes = this.bytes(ptr, (frames >>> 0) * this.audioChannels * 4);
        if (this.hashing) this.audioHash = fnv32(this.audioHash, bytes);
        this.audioFrames += frames >>> 0;
        // one copy: guest memory may be reused or grow (detaching views)
        this.onAudio(new Float32Array(bytes.slice().buffer), this.audioRate, this.audioChannels);
      },
      input_pad: (player) => ((player >>> 0) < 4 ? (this.getPad(player) >>> 0) : 0),
      input_mode: (flags) => { this.inputMode = flags & 7; },
      key_state: (dst, len) => {
        const k = this.input.keys;
        if (!k) return -1;
        const n = Math.min(len >>> 0, KEY_STATE_BYTES);
        this.bytes(dst, n).set(k.subarray(0, n));
        return KEY_STATE_BYTES;
      },
      key_events: (dst, cap) => {
        if (!this.input.keys) return -1;
        const ev = this.input.keyEvents ?? [], len = ev.length * 4;
        if (len <= cap >>> 0) {
          const b = this.bytes(dst, len);
          ev.forEach(([code, down], i) => { b[i * 4] = code & 255; b[i * 4 + 1] = code >> 8; b[i * 4 + 2] = down ? 1 : 0; b[i * 4 + 3] = 0; });
        }
        return len;
      },
      pointer: (dst, cap) => {
        const p = this.input.pointer;
        if (!p) return -1;
        if (cap >>> 0 >= POINTER_BYTES) {
          this.bytes(dst, POINTER_BYTES);   // bounds check
          const [fx, fy] = framePosition(p.x, p.y, p.drawable, [this.width, this.height], p.integerScale, this.aspect);
          const dv = this.view(), at = dst >>> 0;
          [p.x, p.y, fx, fy, p.dx, p.dy, p.wheelX, p.wheelY].forEach((v, i) => dv.setFloat32(at + i * 4, v, true));
          [p.buttons, p.pressed, p.released, p.flags].forEach((v, i) => dv.setUint32(at + 32 + i * 4, v >>> 0, true));
        }
        return POINTER_BYTES;
      },
      gamepad: (slot, dst, cap) => {
        const pads = this.input.gamepads;
        if (!pads || slot >>> 0 > 3) return -1;
        if (cap >>> 0 >= GAMEPAD_BYTES) {
          this.bytes(dst, GAMEPAD_BYTES).fill(0);
          const g = pads[slot] ?? { connected: false, buttons: [], axes: [] };
          const nb = Math.min(g.buttons.length, GAMEPAD_BUTTONS), na = Math.min(g.axes.length, GAMEPAD_AXES);
          const dv = this.view(), at = dst >>> 0;
          dv.setUint32(at, g.connected ? (1 | (g.standard ? 2 : 0)) : 0, true);
          dv.setUint32(at + 4, nb, true); dv.setUint32(at + 8, na, true);
          for (let i = 0; i < nb; i++) dv.setFloat32(at + 12 + i * 4, g.buttons[i], true);
          for (let i = 0; i < na; i++) dv.setFloat32(at + 140 + i * 4, g.axes[i], true);
        }
        return GAMEPAD_BYTES;
      },
      gamepad_name: (slot, dst, cap) => {
        const g = this.input.gamepads?.[slot >>> 0];
        if (!g?.connected) return -1;
        return this.copyIfFits(dst, cap, ENC.encode(g.name ?? ''));
      },
      text_input: (dst, cap) => (this.text === null ? -1 : this.copyIfFits(dst, cap, ENC.encode(this.text))),
      param: (ptr, len, dst, cap) => {
        const v = this.params[this.str(ptr, len)];
        return v === undefined ? -1 : this.copyIfFits(dst, cap, ENC.encode(String(v)));
      },
      asset_read_at: (ptr, len, offset, dst, cap) => readAsset(ptr, len, offset >>> 0, dst, cap),
      asset_read_at64: (ptr, len, offset, dst, cap) => readAsset(ptr, len, Number(BigInt.asUintN(64, offset)), dst, cap),
      asset_size: (ptr, len) => { const n = this.assets.size(this.str(ptr, len)); return n > 0x7fffffff ? -2 : n; },
      asset_size64: (ptr, len) => BigInt(this.assets.size(this.str(ptr, len))),
      asset_count: () => this.assetNames().length,
      // providers without versions never change: 0 (or -1 if missing)
      asset_version: (ptr, len) => {
        const name = this.str(ptr, len);
        if (typeof this.assets.version === 'function') return this.assets.version(name);
        return this.assets.size(name) >= 0 ? 0 : -1;
      },
      asset_name: (index, dst, cap) => {
        const n = this.assetNames()[index >>> 0];
        return n === undefined ? -1 : this.copyIfFits(dst, cap, ENC.encode(n));
      },
      asset_read: (ptr, len, dst, cap) => readAsset(ptr, len, 0, dst, cap),
    };
  }

  /**
   * Add or replace an asset while the game runs: the guest sees it from its next
   * call on, with a new gasm.asset_version (returned). Call between frames. Needs
   * an AssetTable (the default for a { name: bytes } record and every built-in provider).
   */
  setAsset(name, bytes) {
    if (typeof this.assets.set !== 'function') throw new Error('setAsset: this asset provider is read-only');
    return this.assets.set(name, bytes);
  }
  /** Remove an asset while the game runs. False if there was none. */
  removeAsset(name) {
    if (typeof this.assets.remove !== 'function') throw new Error('removeAsset: this asset provider is read-only');
    return this.assets.remove(name);
  }

  /** Asset names sorted by UTF-8 bytes (= code point order), like the native runner. */
  assetNames() {
    return typeof this.assets.names === 'function' ? this.assets.names() : [];
  }

  // ---- gasm:gfx --------------------------------------------------------------
  gfxImports() {
    const g = this.gfx;
    const m = new GfxModel(g);   // validation and handles, shared by every backend (as natively)
    this.gfxModel = m;
    const check = () => g.checkErrors?.();   // GPU validation errors reported since the last call
    const json = (ptr, len) => {
      const text = this.str(ptr, len);
      try { return JSON.parse(text); } catch (e) { throw new Error(`gfx: invalid JSON descriptor: ${e.message}: ${text.slice(0, 200)}`); }
    };
    const header = new DataView(new ArrayBuffer(24)), headerBytes = new Uint8Array(header.buffer);
    const offsetsOf = (ptr, count) => {
      const dv = this.view(), at = ptr >>> 0;
      this.bytes(ptr, count * 4);
      return Array.from({ length: count }, (_, k) => dv.getUint32(at + k * 4, true));
    };
    return {
      width: () => g.width(),
      height: () => g.height(),
      create_shader: (ptr, len) => { check(); const code = this.str(ptr, len); const h = m.shader(); g.createShader?.(h, code); return h; },
      create_buffer: (size, usage) => { check(); const h = m.buffer(size >>> 0, usage >>> 0); g.createBuffer?.(h, size >>> 0, usage >>> 0); return h; },
      create_pipeline: (ptr, len) => { check(); const d = json(ptr, len); const h = m.pipeline(d); g.createPipeline?.(h, d); return h; },
      create_bind_group: (ptr, len) => { check(); const d = json(ptr, len); const h = m.bindGroup(d); g.createBindGroup?.(h, d, m.objects[h]); return h; },
      create_bind_group_layout: (ptr, len) => { check(); const d = json(ptr, len); const h = m.layout(d); g.createBindGroupLayout?.(h, d); return h; },
      create_texture: (ptr, len) => { check(); const d = json(ptr, len); const h = m.texture(d); g.createTexture?.(h, d, m.objects[h]); return h; },
      create_sampler: (ptr, len) => { check(); const d = json(ptr, len); const h = m.sampler(d); g.createSampler?.(h, d); return h; },
      write_buffer: (buf, offset, ptr, len) => {
        check();
        const bytes = this.bytes(ptr, len);
        m.writeBuffer(buf, offset >>> 0, bytes.length);
        g.writeBuffer?.(buf, offset >>> 0, bytes);
        if (this.hashing) this.videoHash = fnv32(this.videoHash, bytes);
      },
      write_texture: (tex, mip, x, y, w, h, ptr, len) => {
        check();
        const bytes = this.bytes(ptr, len);
        m.writeTexture(tex, mip >>> 0, x >>> 0, y >>> 0, w >>> 0, h >>> 0, bytes.length);
        g.writeTexture?.(tex, mip >>> 0, x >>> 0, y >>> 0, w >>> 0, h >>> 0, bytes);
        if (this.hashing) {
          // header (little-endian u32s) + payload, so the target region counts too
          [tex, mip, x, y, w, h].forEach((v, i) => header.setUint32(i * 4, v >>> 0, true));
          this.videoHash = fnv32(fnv32(this.videoHash, headerBytes), bytes);
        }
      },
      begin_frame: (r, gr, b, a) => { check(); m.beginFrame(); g.used = true; return g.beginFrame(r, gr, b, a, this.showFrame) ? 1 : 0; },
      set_pipeline: (p) => { m.setPipeline(p); g.setPipeline?.(p); },
      set_bind_group: (i, bg) => { m.setBindGroup(i >>> 0, bg, []); g.setBindGroup?.(i, bg, []); },
      set_bind_group_offsets: (i, bg, ptr, count) => {
        if ((count >>> 0) > 32) throw new Error(`gfx.set_bind_group_offsets: ${count >>> 0} offsets (a bind group has at most 32)`);
        const offsets = offsetsOf(ptr, count >>> 0);
        m.setBindGroup(i >>> 0, bg, offsets);
        g.setBindGroup?.(i, bg, offsets);
      },
      set_viewport: (x, y, w, h, min, max) => {
        if (![x, y, w, h, min, max].every(Number.isFinite) || w < 0 || h < 0) throw new Error(`gfx.set_viewport: invalid rectangle ${x},${y} ${w}x${h}`);
        if (min < 0 || max > 1 || min > max) throw new Error(`gfx.set_viewport: depth range ${min}..${max} must be within 0..1`);
        m.setViewport();
        g.setViewport?.(...clampRect(x, y, w, h, g.width(), g.height()), min, max);
      },
      set_scissor_rect: (x, y, w, h) => { m.setScissorRect(); g.setScissorRect?.(...clampRect(x >>> 0, y >>> 0, w >>> 0, h >>> 0, g.width(), g.height())); },
      set_vertex_buffer: (slot, buf, off) => { m.setVertexBuffer(slot >>> 0, buf, off >>> 0); g.setVertexBuffer?.(slot, buf, off >>> 0); },
      set_index_buffer: (buf, fmt, off) => { m.setIndexBuffer(buf, fmt >>> 0, off >>> 0); g.setIndexBuffer?.(buf, fmt, off >>> 0); },
      draw: (vc, ic, fv, fi) => { m.draw(vc >>> 0, ic >>> 0, fv >>> 0, fi >>> 0); g.draw?.(vc >>> 0, ic >>> 0, fv >>> 0, fi >>> 0); },
      draw_indexed: (ic, n, first, base, fi) => {
        m.drawIndexed(ic >>> 0, n >>> 0, first >>> 0, base | 0, fi >>> 0);
        g.drawIndexed?.(ic >>> 0, n >>> 0, first >>> 0, base | 0, fi >>> 0);
      },
      end_frame: () => { m.endFrame(); g.endFrame?.(); this.framesPresented++; },
      destroy: (h) => { check(); m.destroy(h); g.destroy?.(h); },
    };
  }

  // ---- gasm:storage -------------------------------------------------------------
  clipboardImports() {
    return {
      set_text: (ptr, len) => {
        if (len > CLIPBOARD_MAX) return -1;
        this.copied = this.str(ptr, len);
        return 0;
      },
      get_text: (dst, cap) => (this.pasted === null ? -1 : this.copyIfFits(dst, cap, new TextEncoder().encode(this.pasted))),
    };
  }

  filesImports() {
    return {
      save: (np, nl, mp, ml, ptr, len) => this.files.save(this.str(np, nl), nl, this.str(mp, ml), this.bytes(ptr, len)),
      state: (h) => this.files.state(h),
    };
  }

  storageImports() {
    const st = this.storage;
    return {
      get: (kp, kl, dst, cap) => {
        const v = st.get(this.str(kp, kl));
        return v ? this.copyIfFits(dst, cap, v) : -1;
      },
      set: (kp, kl, vp, vl) => {
        const key = this.str(kp, kl);
        try { st.set(key, this.bytes(vp, vl).slice()); return 0; } catch (e) {
          this.onLog(`[gasm] storage: ${key}: ${e.message}`);
          return e instanceof StorageError ? e.code : STORAGE_ERR_IO;
        }
      },
      delete: (kp, kl) => (st.delete(this.str(kp, kl)) ? 0 : -1),
      count: () => st.keys().length,
      key: (index, dst, cap) => {
        const k = st.keys()[index >>> 0];
        return k === undefined ? -1 : this.copyIfFits(dst, cap, ENC.encode(k));
      },
    };
  }

  // ---- gasm:net ----------------------------------------------------------------
  netImports() {
    const n = this.net;
    return {
      open: (ptr, len) => n.open(this.str(ptr, len)),
      state: (c) => n.state(c),
      send: (c, ptr, len) => {
        const bytes = this.bytes(ptr, len);
        if (!bytes.length) { n.check(c); return -1; }
        return n.send(c, bytes.slice());
      },
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

  // ---- gasm:fetch -------------------------------------------------------------
  fetchImports() {
    const f = this.fetch;
    return {
      request: (dp, dl, bp, bl) => f.request(this.str(dp, dl), this.bytes(bp, bl).slice(), this.frameIndex),
      state: (r) => f.state(r, this.frameIndex),
      status: (r) => f.status(r, this.frameIndex),
      headers: (r, dst, cap) => {
        const text = f.headers(r, this.frameIndex);
        return text === null ? -1 : this.copyIfFits(dst, cap, new TextEncoder().encode(text));
      },
      read: (r, dst, cap) => {
        this.bytes(dst, cap);   // the whole destination must be guest memory (traps otherwise)
        const b = f.read(r, cap >>> 0, this.frameIndex);
        if (b === -1) return -1;
        this.bytes(dst, b.length).set(b);
        return b.length;
      },
      close: (r) => f.close(r),
    };
  }

  wasiImports() { return wasiImports(this); }

  // ---- lifecycle -----------------------------------------------------------
  /** Instantiate a module (bytes or a compiled WebAssembly.Module) and run its init. */
  async load(wasm) {
    this.gl = new GlHost(this, this.glContext);
    this.gl.model.hash = (b) => { if (this.hashing) this.videoHash = fnv32(this.videoHash, b); };
    const known = {
      gasm: this.gasmImports(), 'gasm:gfx': this.gfxImports(), 'gasm:gl': glImports(this.gl), 'gasm:net': this.netImports(),
      'gasm:fetch': this.fetchImports(), 'gasm:storage': this.storageImports(), 'gasm:clipboard': this.clipboardImports(), 'gasm:files': this.filesImports(),
    };
    for (const [mod, fns] of Object.entries(known)) {
      this.provided.add(mod);
      for (const f of Object.keys(fns)) this.provided.add(`${mod}.${f}`);
    }
    this.provided.add('wasi_snapshot_preview1');
    for (const f of WASI_IMPLEMENTED) this.provided.add(`wasi_snapshot_preview1.${f}`);
    // Imports this runner doesn't know (a newer gasm function, JS glue some Rust
    // crates pull in) link as traps: harmless unless the guest calls them (as natively).
    const trapping = (mod, fns = {}) => new Proxy(fns, {
      get: (t, n) => t[n] ?? (() => { throw new Error(`unsupported import ${String(mod)}.${String(n)}`); }),
    });
    const module = wasm instanceof WebAssembly.Module ? wasm : await WebAssembly.compile(wasm);
    const gpuModules = new Set(WebAssembly.Module.imports(module).map((i) => i.module).filter((m) => m === 'gasm:gfx' || m === 'gasm:gl'));
    if (gpuModules.size > 1) throw new Error('a module imports gasm:gfx or gasm:gl, not both');
    this.usesGl = gpuModules.has('gasm:gl');
    const hasRun = WebAssembly.Module.exports(module).some((e) => e.name === 'gasm_run');
    this.switching = hasRun && this.stackSwitching;
    if (hasRun && !this.switching && WebAssembly.Module.imports(module).some((i) => i.module === 'asyncify')) {
      // made without wasm-opt --asyncify: its gasm_frame can't suspend
      throw new Error('this module is a run build (no Asyncify): it needs stack switching (JSPI); use the game\'s Asyncify build');
    }
    if (this.switching) {
      // ends the frame: the guest stays suspended here until the next frameAsync()
      known.gasm.yield_frame = new WebAssembly.Suspending(async () => {
        const [done] = this.frameEnd ?? [];
        this.frameEnd = null;
        await new Promise((r) => { this.resume = r; done?.(); });
      });
    }
    const imports = new Proxy({ ...known, wasi_snapshot_preview1: this.wasiImports() }, {
      get: (t, mod) => (mod === 'wasi_snapshot_preview1' ? t[mod] : trapping(mod, t[mod])),
    });
    this.staticTitle = staticTitle(module);
    const instance = await WebAssembly.instantiate(module, imports);
    const ex = instance.exports;
    if (!(ex.memory instanceof WebAssembly.Memory)) throw new Error('guest does not export `memory`');
    this.memory = ex.memory;
    try {
      if (ex._initialize) ex._initialize();
      const version = ex.gasm_abi_version();
      if (version !== ABI_VERSION) throw new Error(`guest targets gasm ABI v${version}, runner implements v${ABI_VERSION}`);
      const rc = ex.gasm_init();
      this.checkMemory();
      if (rc !== 0) throw new Error(`gasm_init failed with code ${rc}`);
    } catch (e) {
      this.dead = e;
      throw e;
    }
    this.exports = ex;
  }

  /** The guest's memory grew past memoryLimit: a trap (browsers can't refuse the growth
   *  itself, so this is checked after each call; natively the growth traps). */
  checkMemory() {
    if (this.memoryLimit > 0 && this.memory && this.memory.buffer.byteLength > this.memoryLimit) {
      throw new WebAssembly.RuntimeError(memoryLimitMessage(this.memoryLimit));
    }
  }

  /** Run one frame. After the guest trapped or exited, this throws without calling it.
   *  gasm_run guests (host.switching) need frameAsync(). */
  frame() {
    if (this.switching) throw new Error('this guest runs through gasm_run (stack switching): use frameAsync()');
    if (this.dead) throw this.dead instanceof ProcExit ? this.dead : new Error(`the guest is not running (${this.dead.message})`);
    if (this.virtualTime) this.vtime = this.clock.at(this.frameIndex, this.frameRate);
    try {
      this.gfx.checkErrors?.();
      this.exports.gasm_frame();
      this.checkMemory();
    } catch (e) {
      this.dead = e;
      throw e;
    } finally {
      this.frameIndex++; // a frame that exits or traps still counts (as in the other runners)
      if (this.titleChanged) { this.titleChanged = false; this.onTitle(this.title); }
      if (this.copied !== null) { const t = this.copied; this.copied = null; this.onCopyText(t); }
      this.files.flush();
    }
  }

  /**
   * Run one frame of any guest: frame(), or for a gasm_run guest, resume it
   * until its next yield_frame (the first frame starts gasm_run).
   */
  async frameAsync() {
    if (!this.switching) return this.frame();
    if (this.dead) throw this.dead instanceof ProcExit ? this.dead : new Error(`the guest is not running (${this.dead.message})`);
    if (this.virtualTime) this.vtime = this.clock.at(this.frameIndex, this.frameRate);
    const ended = new Promise((resolve, reject) => { this.frameEnd = [resolve, reject]; });
    try {
      this.gfx.checkErrors?.();
      if (!this.running) {
        this.running = WebAssembly.promising(this.exports.gasm_run)();
        this.running.then((code) => this.frameEnd?.[1](new ProcExit(code)), (e) => this.frameEnd?.[1](e));
      } else {
        const r = this.resume;
        this.resume = null;
        r();
      }
      await ended;
      this.checkMemory();
    } catch (e) {
      this.dead = e;
      throw e;
    } finally {
      this.frameIndex++;
      if (this.titleChanged) { this.titleChanged = false; this.onTitle(this.title); }
      if (this.copied !== null) { const t = this.copied; this.copied = null; this.onCopyText(t); }
      this.files.flush();
    }
  }

  /** runFrames() for any guest (gasm_run guests need it). */
  async runFramesAsync(steps, show = true) {
    if (!this.switching) return this.runFrames(steps, show);
    const before = this.videoFrames;
    for (let i = 0; i < steps.length; i++) {
      this.prepareStep(steps[i], show && i === steps.length - 1, i < steps.length - 1);
      await this.frameAsync();
    }
    return this.finishBatch(before, show);
  }

  prepareStep(s, show, catchUp = false) {
    this.catchUp = catchUp;
    this.getPad = (p) => s.pads?.[p] ?? 0;
    if (s.text !== undefined) this.text = s.text;
    this.input = s.input ?? NO_INPUT;
    this.pasted = typeof s.paste === 'string' && s.paste.length <= CLIPBOARD_MAX ? s.paste : null;
    this.showFrame = show;
    this.gfx.used = false;
  }

  finishBatch(before, show) {
    const video = this.videoFrames !== before && this.width > 0;
    if (video && show && this.gfx.presentVideo && !this.gfx.used) this.gfx.presentVideo(this.rgba, this.width, this.height, this.aspect);
    return { video };
  }

  /**
   * Run a batch of frames, as the runners do when catching up: one per step
   * ({ pads: [p0..p3], text, input }), only the last shown (if `show`). A 2D frame
   * from video_present is blitted with WebGPU when the canvas belongs to it (gfx
   * backend with presentVideo) and the guest drew nothing with gfx. Returns
   * { video: true if a new 2D frame was presented }. Throws like frame().
   */
  runFrames(steps, show = true) {
    const before = this.videoFrames;
    for (let i = 0; i < steps.length; i++) {
      this.prepareStep(steps[i], show && i === steps.length - 1, i < steps.length - 1);
      this.frame();
    }
    return this.finishBatch(before, show);
  }

  /** Best-effort "player is quitting" (optional gasm_exit export): games flush saves.
   *  The guest is not called again afterwards. */
  exit() {
    if (this.dead || !this.exports) return;
    this.dead = new Error('exited');
    const f = this.exports.gasm_exit;
    if (f) { try { f(); } catch (e) { if (!(e instanceof ProcExit)) this.onLog(`[gasm] gasm_exit trapped: ${e.message}`); } }
  }

  /** exit(), then close network connections (flushing them) and the storage. */
  async shutdown() {
    this.exit();
    this.files.flush();   // saved on the way out (gasm_exit)
    this.fetch.closeAll();
    await this.net.closeAll();
    await this.storage.flush?.();
    this.storage.close?.();
  }
}

export { AssetTable };
