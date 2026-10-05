// gasm:gl (OpenGL ES 3.0 with WebGL 2's rules, design/gasm-gl.md): the model every
// backend shares, and the null GL of headless runs. Mirrors runners/native/src/gl.rs.
//
// GlModel numbers object names (per kind, from 1, in creation order, never reused),
// keeps the bindings and pixel-store state, and checks what both runners must agree
// on: unknown or deleted names, missing bindings, invalid targets, negative sizes,
// data lengths against the unpack/pack state. A failed check records a GL error and
// the call doesn't reach the backend; a too-short (ptr, len) traps (a gasm boundary
// violation). Backends (NullGl here, WebGlBackend in webgl-gl.js) only execute.
// Hashing: every upload folds a header of little-endian u32s and its payload into the
// video hash (ABI.md, "gasm:gl hashing").

export const GL = {
  NO_ERROR: 0, INVALID_ENUM: 0x0500, INVALID_VALUE: 0x0501, INVALID_OPERATION: 0x0502,
  INVALID_FRAMEBUFFER_OPERATION: 0x0506,
  ARRAY_BUFFER: 0x8892, ELEMENT_ARRAY_BUFFER: 0x8893, COPY_READ_BUFFER: 0x8F36, COPY_WRITE_BUFFER: 0x8F37,
  PIXEL_PACK_BUFFER: 0x88EB, PIXEL_UNPACK_BUFFER: 0x88EC, TRANSFORM_FEEDBACK_BUFFER: 0x8C8E, UNIFORM_BUFFER: 0x8A11,
  TEXTURE_2D: 0x0DE1, TEXTURE_CUBE_MAP: 0x8513, TEXTURE_3D: 0x806F, TEXTURE_2D_ARRAY: 0x8C1A,
  TEXTURE_CUBE_MAP_POSITIVE_X: 0x8515, TEXTURE_CUBE_MAP_NEGATIVE_Z: 0x851A, TEXTURE0: 0x84C0,
  FRAMEBUFFER: 0x8D40, READ_FRAMEBUFFER: 0x8CA8, DRAW_FRAMEBUFFER: 0x8CA9, RENDERBUFFER: 0x8D41,
  FRAMEBUFFER_COMPLETE: 0x8CD5, FRAMEBUFFER_INCOMPLETE_MISSING_ATTACHMENT: 0x8CD7,
  VERTEX_SHADER: 0x8B31, FRAGMENT_SHADER: 0x8B30,
  DELETE_STATUS: 0x8B80, COMPILE_STATUS: 0x8B81, LINK_STATUS: 0x8B82, VALIDATE_STATUS: 0x8B83,
  INFO_LOG_LENGTH: 0x8B84, ATTACHED_SHADERS: 0x8B85, ACTIVE_UNIFORMS: 0x8B86, ACTIVE_ATTRIBUTES: 0x8B89,
  SHADER_SOURCE_LENGTH: 0x8B88, SHADER_TYPE: 0x8B4F, ACTIVE_UNIFORM_BLOCKS: 0x8A36,
  TRANSFORM_FEEDBACK_VARYINGS: 0x8C83,
  VENDOR: 0x1F00, RENDERER: 0x1F01, VERSION: 0x1F02, EXTENSIONS: 0x1F03, SHADING_LANGUAGE_VERSION: 0x8B8C,
  UNPACK_ALIGNMENT: 0x0CF5, PACK_ALIGNMENT: 0x0D05, UNPACK_ROW_LENGTH: 0x0CF2, UNPACK_IMAGE_HEIGHT: 0x806E,
  UNPACK_SKIP_PIXELS: 0x0CF4, UNPACK_SKIP_ROWS: 0x0CF3, UNPACK_SKIP_IMAGES: 0x806D,
  PACK_ROW_LENGTH: 0x0D02, PACK_SKIP_PIXELS: 0x0D04, PACK_SKIP_ROWS: 0x0D03,
  TRANSFORM_FEEDBACK: 0x8E22, SYNC_STATUS: 0x9114, SIGNALED: 0x9119, UNSIGNALED: 0x9118,
  ALREADY_SIGNALED: 0x911A, TIMEOUT_EXPIRED: 0x911B, CONDITION_SATISFIED: 0x911C, SYNC_GPU_COMMANDS_COMPLETE: 0x9117,
  QUERY_RESULT: 0x8866, QUERY_RESULT_AVAILABLE: 0x8867, CURRENT_QUERY: 0x8865,
  ANY_SAMPLES_PASSED: 0x8C2F, ANY_SAMPLES_PASSED_CONSERVATIVE: 0x8D6A, TRANSFORM_FEEDBACK_PRIMITIVES_WRITTEN: 0x8C88,
  INVALID_INDEX: 0xFFFFFFFF,
};

const BUFFER_TARGETS = [GL.ARRAY_BUFFER, GL.ELEMENT_ARRAY_BUFFER, GL.COPY_READ_BUFFER, GL.COPY_WRITE_BUFFER,
  GL.PIXEL_PACK_BUFFER, GL.PIXEL_UNPACK_BUFFER, GL.TRANSFORM_FEEDBACK_BUFFER, GL.UNIFORM_BUFFER];
const TEXTURE_TARGETS = [GL.TEXTURE_2D, GL.TEXTURE_CUBE_MAP, GL.TEXTURE_3D, GL.TEXTURE_2D_ARRAY];
const FB_TARGETS = [GL.FRAMEBUFFER, GL.READ_FRAMEBUFFER, GL.DRAW_FRAMEBUFFER];
const QUERY_TARGETS = [GL.ANY_SAMPLES_PASSED, GL.ANY_SAMPLES_PASSED_CONSERVATIVE, GL.TRANSFORM_FEEDBACK_PRIMITIVES_WRITTEN];

/** Bytes per pixel of a (format, type) pair; 0 if the pair is invalid. */
/** The typed array WebGL 2 wants for pixels of `type` (it rejects any other view). */
const PIXEL_VIEWS = {
  0x1400: Int8Array,      // BYTE
  0x1402: Int16Array,     // SHORT
  0x1403: Uint16Array,    // UNSIGNED_SHORT
  0x1404: Int32Array,     // INT
  0x1405: Uint32Array,    // UNSIGNED_INT
  0x1406: Float32Array,   // FLOAT
  0x140B: Uint16Array,    // HALF_FLOAT
  0x8033: Uint16Array,    // UNSIGNED_SHORT_4_4_4_4
  0x8034: Uint16Array,    // UNSIGNED_SHORT_5_5_5_1
  0x8363: Uint16Array,    // UNSIGNED_SHORT_5_6_5
  0x8368: Uint32Array,    // UNSIGNED_INT_2_10_10_10_REV
  0x84FA: Uint32Array,    // UNSIGNED_INT_24_8
  0x8C3B: Uint32Array,    // UNSIGNED_INT_10F_11F_11F_REV
  0x8C3E: Uint32Array,    // UNSIGNED_INT_5_9_9_9_REV
  0x8DAD: Uint32Array,    // FLOAT_32_UNSIGNED_INT_24_8_REV
};

/**
 * Guest bytes as the view WebGL 2 needs for `type` (bytes for UNSIGNED_BYTE). The
 * view shares guest memory when the pointer is aligned for it, else it's a copy
 * (`copied`: readPixels then copies the result back).
 */
export function pixelView(bytes, type) {
  const View = PIXEL_VIEWS[type];
  if (!View) return { view: bytes, copied: false };
  const n = Math.floor(bytes.byteLength / View.BYTES_PER_ELEMENT);
  if (bytes.byteOffset % View.BYTES_PER_ELEMENT === 0) return { view: new View(bytes.buffer, bytes.byteOffset, n), copied: false };
  return { view: new View(bytes.slice(0, n * View.BYTES_PER_ELEMENT).buffer), copied: true };
}

export function pixelBytes(format, type) {
  const packed = { 0x8363: 2, 0x8033: 2, 0x8034: 2, 0x8368: 4, 0x8C3B: 4, 0x8C3E: 4, 0x84FA: 4, 0x8DAD: 8 }[type];
  if (packed) return packed;
  const size = { 0x1400: 1, 0x1401: 1, 0x1402: 2, 0x1403: 2, 0x1404: 4, 0x1405: 4, 0x1406: 4, 0x140B: 2 }[type];
  const comps = { 0x1903: 1, 0x8D94: 1, 0x8227: 2, 0x8228: 2, 0x1907: 3, 0x8D98: 3, 0x1908: 4, 0x8D99: 4,
    0x1906: 1, 0x1909: 1, 0x190A: 2, 0x1902: 1, 0x84F9: 2 }[format];
  return size && comps ? size * comps : 0;
}

/** Bytes an image of w×h×d needs under a pixel-store state (GLES 3.0, 3.7.1). */
export function imageBytes(w, h, d, bpp, store) {
  if (w <= 0 || h <= 0 || d <= 0) return 0;
  const rowLen = store.rowLength > 0 ? store.rowLength : w;
  const align = store.alignment;
  const rowBytes = Math.ceil(rowLen * bpp / align) * align;
  const imgH = store.imageHeight > 0 ? store.imageHeight : h;
  const img = rowBytes * imgH;
  return store.skipImages * img + (d - 1) * img + store.skipRows * rowBytes + (h - 1) * rowBytes + store.skipPixels * bpp + w * bpp;
}

const KINDS = ['buffer', 'texture', 'vertexArray', 'sampler', 'framebuffer', 'renderbuffer', 'shader', 'program',
  'query', 'sync', 'transformFeedback'];

export class GlModel {
  constructor(backend) {
    this.backend = backend;
    this.next = Object.fromEntries(KINDS.map((k) => [k, 1]));
    this.live = Object.fromEntries(KINDS.map((k) => [k, new Set()]));   // created and not deleted
    this.errors = [];
    this.buffers = new Map();   // target -> name
    this.textures = new Map();  // `${unit}:${target}` -> name
    this.textureTargets = new Map();  // name -> the target it was first bound to (it can't be bound to another)
    this.unit = 0;
    this.framebuffers = new Map();
    this.renderbuffer = 0;
    this.program = 0;
    this.vertexArray = 0;
    this.locations = new Map();   // program -> Map(name -> location)
    this.unpack = { alignment: 4, rowLength: 0, imageHeight: 0, skipPixels: 0, skipRows: 0, skipImages: 0 };
    this.pack = { alignment: 4, rowLength: 0, imageHeight: 0, skipPixels: 0, skipRows: 0, skipImages: 0 };
    this.hash = null;   // (bytes) => void, set by the host when hashing
  }

  error(e) { if (this.errors.length < 32) this.errors.push(e); return false; }
  getError() { return this.errors.length ? this.errors.shift() : (this.backend.getError?.() ?? 0); }

  create(kind) {
    const n = this.next[kind]++;
    this.live[kind].add(n);
    return n;
  }
  /** name 0 is "none" (valid where unbinding is); an unknown or deleted name is GL_INVALID_OPERATION. */
  valid(kind, n, allowZero = true) {
    if (n === 0) return allowZero || this.error(GL.INVALID_VALUE);
    return this.live[kind].has(n) || this.error(GL.INVALID_OPERATION);
  }
  remove(kind, n) {
    if (n === 0 || !this.live[kind].has(n)) return false;   // deleting an unknown name is a no-op (GL)
    this.live[kind].delete(n);
    return true;
  }
  is(kind, n) { return n !== 0 && this.live[kind].has(n) ? 1 : 0; }

  target(list, t) { return list.includes(t) || this.error(GL.INVALID_ENUM); }
  bufferTarget(t) { return this.target(BUFFER_TARGETS, t); }
  textureTarget(t) { return this.target(TEXTURE_TARGETS, t); }
  imageTarget(t) {   // tex_image_2d targets: 2D or a cube face
    return t === GL.TEXTURE_2D || (t >= GL.TEXTURE_CUBE_MAP_POSITIVE_X && t <= GL.TEXTURE_CUBE_MAP_NEGATIVE_Z) || this.error(GL.INVALID_ENUM);
  }
  boundBuffer(t) { return this.buffers.get(t) ?? 0; }
  boundTexture(t) {
    const face = t >= GL.TEXTURE_CUBE_MAP_POSITIVE_X && t <= GL.TEXTURE_CUBE_MAP_NEGATIVE_Z;
    return this.textures.get(`${this.unit}:${face ? GL.TEXTURE_CUBE_MAP : t}`) ?? 0;
  }
  needBuffer(t) { return (this.bufferTarget(t) && this.boundBuffer(t) !== 0) || (BUFFER_TARGETS.includes(t) && this.error(GL.INVALID_OPERATION)); }
  needTexture(t) { return this.boundTexture(t) !== 0 || this.error(GL.INVALID_OPERATION); }
  nonNegative(...v) { return v.every((x) => x >= 0) || this.error(GL.INVALID_VALUE); }

  pixelStore(pname, v) {
    const map = { [GL.UNPACK_ALIGNMENT]: ['unpack', 'alignment'], [GL.PACK_ALIGNMENT]: ['pack', 'alignment'],
      [GL.UNPACK_ROW_LENGTH]: ['unpack', 'rowLength'], [GL.UNPACK_IMAGE_HEIGHT]: ['unpack', 'imageHeight'],
      [GL.UNPACK_SKIP_PIXELS]: ['unpack', 'skipPixels'], [GL.UNPACK_SKIP_ROWS]: ['unpack', 'skipRows'],
      [GL.UNPACK_SKIP_IMAGES]: ['unpack', 'skipImages'], [GL.PACK_ROW_LENGTH]: ['pack', 'rowLength'],
      [GL.PACK_SKIP_PIXELS]: ['pack', 'skipPixels'], [GL.PACK_SKIP_ROWS]: ['pack', 'skipRows'] };
    const m = map[pname];
    if (!m) return true;   // WebGL-only or ignored pnames go to the backend as they are
    if (m[1] === 'alignment' ? ![1, 2, 4, 8].includes(v) : v < 0) return this.error(GL.INVALID_VALUE);
    this[m[0]][m[1]] = v;
    return true;
  }

  /** The upload's hash: header u32s, then the payload (if any). */
  fold(header, payload) {
    if (!this.hash) return;
    const h = new Uint8Array(header.length * 4);
    const dv = new DataView(h.buffer);
    header.forEach((v, i) => dv.setUint32(i * 4, v >>> 0, true));
    this.hash(h);
    if (payload) this.hash(payload);
  }
}

/** The null GL of headless runs: nothing is drawn, every answer a guest can branch
 *  on is fixed (WebGL 2's minimum limits, no extensions, shaders that compile). */
export class NullGl {
  constructor() { this.width = 1280; this.height = 720; }
  getError() { return 0; }
}

/** WebGL 2's guaranteed minimums (and fixed answers), reported by null GLs. */
export const NULL_LIMITS = {
  0x0D33: [2048],          // MAX_TEXTURE_SIZE
  0x851C: [2048],          // MAX_CUBE_MAP_TEXTURE_SIZE
  0x8073: [256],           // MAX_3D_TEXTURE_SIZE
  0x88FF: [256],           // MAX_ARRAY_TEXTURE_LAYERS
  0x84E8: [2048],          // MAX_RENDERBUFFER_SIZE
  0x8872: [16],            // MAX_TEXTURE_IMAGE_UNITS
  0x8B4D: [32],            // MAX_COMBINED_TEXTURE_IMAGE_UNITS
  0x8B4C: [16],            // MAX_VERTEX_TEXTURE_IMAGE_UNITS
  0x8869: [16],            // MAX_VERTEX_ATTRIBS
  0x8DFB: [256],           // MAX_VERTEX_UNIFORM_VECTORS
  0x8DFD: [224],           // MAX_FRAGMENT_UNIFORM_VECTORS
  0x8DFC: [15],            // MAX_VARYING_VECTORS
  0x8B4B: [60],            // MAX_VARYING_COMPONENTS
  0x9122: [64],            // MAX_VERTEX_OUTPUT_COMPONENTS
  0x9125: [60],            // MAX_FRAGMENT_INPUT_COMPONENTS
  0x8B4A: [1024],          // MAX_VERTEX_UNIFORM_COMPONENTS
  0x8B49: [896],           // MAX_FRAGMENT_UNIFORM_COMPONENTS
  0x8CDF: [4],             // MAX_COLOR_ATTACHMENTS
  0x8824: [4],             // MAX_DRAW_BUFFERS
  0x8D57: [4],             // MAX_SAMPLES
  0x8A2F: [24],            // MAX_UNIFORM_BUFFER_BINDINGS
  0x8A30: [16384],         // MAX_UNIFORM_BLOCK_SIZE
  0x8A34: [256],           // UNIFORM_BUFFER_OFFSET_ALIGNMENT
  0x8A2B: [12],            // MAX_VERTEX_UNIFORM_BLOCKS
  0x8A2D: [12],            // MAX_FRAGMENT_UNIFORM_BLOCKS
  0x8A2E: [24],            // MAX_COMBINED_UNIFORM_BLOCKS
  0x8A31: [50176],         // MAX_COMBINED_VERTEX_UNIFORM_COMPONENTS (12 blocks × 16384 / 4 + 1024)
  0x8A33: [50048],         // MAX_COMBINED_FRAGMENT_UNIFORM_COMPONENTS (12 blocks × 16384 / 4 + 896)
  0x8904: [-8],            // MIN_PROGRAM_TEXEL_OFFSET
  0x8905: [7],             // MAX_PROGRAM_TEXEL_OFFSET
  0x84FD: [2],             // MAX_TEXTURE_LOD_BIAS
  0x8D6B: [16777215],      // MAX_ELEMENT_INDEX (2^24 - 1)
  0x8C8A: [64],            // MAX_TRANSFORM_FEEDBACK_INTERLEAVED_COMPONENTS
  0x8C8B: [4],             // MAX_TRANSFORM_FEEDBACK_SEPARATE_ATTRIBS
  0x8C80: [4],             // MAX_TRANSFORM_FEEDBACK_SEPARATE_COMPONENTS
  0x0D3A: [4096, 4096],    // MAX_VIEWPORT_DIMS
  0x846D: [1, 1],          // ALIASED_POINT_SIZE_RANGE
  0x846E: [1, 1],          // ALIASED_LINE_WIDTH_RANGE
  0x0D50: [0],             // SUBPIXEL_BITS (unused)
  0x86A2: [0],             // NUM_COMPRESSED_TEXTURE_FORMATS
  0x821B: [3],             // MAJOR_VERSION
  0x821C: [0],             // MINOR_VERSION
  0x8B8B: [0],             // FRAGMENT_SHADER_DERIVATIVE_HINT (unused)
};

/** Fixed strings of null GLs (and the shape of every runner's). */
export const NULL_STRINGS = {
  [GL.VENDOR]: 'gasm', [GL.RENDERER]: 'gasm null GL', [GL.VERSION]: 'OpenGL ES 3.0 (gasm null GL)',
  [GL.SHADING_LANGUAGE_VERSION]: 'OpenGL ES GLSL ES 3.00 (gasm null GL)', [GL.EXTENSIONS]: '',
};

// Queries every runner answers from the model, so they agree (bindings and the like).
const MODEL_PARAMS = {
  0x8894: (m) => [m.boundBuffer(GL.ARRAY_BUFFER)],            // ARRAY_BUFFER_BINDING
  0x8895: (m) => [m.vaoElement()],                            // ELEMENT_ARRAY_BUFFER_BINDING
  0x8F36: (m) => [m.boundBuffer(GL.COPY_READ_BUFFER)],
  0x8F37: (m) => [m.boundBuffer(GL.COPY_WRITE_BUFFER)],
  0x88ED: (m) => [m.boundBuffer(GL.PIXEL_PACK_BUFFER)],       // PIXEL_PACK_BUFFER_BINDING
  0x88EF: (m) => [m.boundBuffer(GL.PIXEL_UNPACK_BUFFER)],
  0x8A28: (m) => [m.boundBuffer(GL.UNIFORM_BUFFER)],          // UNIFORM_BUFFER_BINDING
  0x8C8F: (m) => [m.boundBuffer(GL.TRANSFORM_FEEDBACK_BUFFER)],
  0x8B8D: (m) => [m.program],                                 // CURRENT_PROGRAM
  0x8069: (m) => [m.boundTexture(GL.TEXTURE_2D)],             // TEXTURE_BINDING_2D
  0x8514: (m) => [m.boundTexture(GL.TEXTURE_CUBE_MAP)],
  0x806A: (m) => [m.boundTexture(GL.TEXTURE_3D)],
  0x8C1D: (m) => [m.boundTexture(GL.TEXTURE_2D_ARRAY)],
  0x84E0: (m) => [GL.TEXTURE0 + m.unit],                      // ACTIVE_TEXTURE
  0x8CA6: (m) => [m.framebuffers.get(GL.DRAW_FRAMEBUFFER) ?? 0],   // DRAW_FRAMEBUFFER_BINDING (= FRAMEBUFFER_BINDING)
  0x8CAA: (m) => [m.framebuffers.get(GL.READ_FRAMEBUFFER) ?? 0],
  0x8CA7: (m) => [m.renderbuffer],                            // RENDERBUFFER_BINDING
  0x85B5: (m) => [m.vertexArray],                             // VERTEX_ARRAY_BINDING
  0x0CF5: (m) => [m.unpack.alignment], 0x0D05: (m) => [m.pack.alignment],
  0x0CF2: (m) => [m.unpack.rowLength], 0x806E: (m) => [m.unpack.imageHeight],
  0x0CF4: (m) => [m.unpack.skipPixels], 0x0CF3: (m) => [m.unpack.skipRows], 0x806D: (m) => [m.unpack.skipImages],
  0x0D02: (m) => [m.pack.rowLength], 0x0D04: (m) => [m.pack.skipPixels], 0x0D03: (m) => [m.pack.skipRows],
};

const KIND_OF_CREATE = {
  create_buffer: 'buffer', create_texture: 'texture', create_vertex_array: 'vertexArray', create_sampler: 'sampler',
  create_framebuffer: 'framebuffer', create_renderbuffer: 'renderbuffer', create_program: 'program',
  create_query: 'query', create_transform_feedback: 'transformFeedback',
};

/**
 * The gasm:gl imports. `host` gives guest memory (bytes, view, str, copyOut) and the
 * frame; `ctx` is a WebGL2RenderingContext or null (null GL). Methods are the imports.
 */
export class GlHost {
  constructor(host, ctx = null) {
    this.host = host;
    this.ctx = ctx;
    this.model = new GlModel(this);
    this.objs = Object.fromEntries(KINDS.map((k) => [k, new Map()]));   // name -> WebGL object
    this.names = new WeakMap();   // WebGL object -> name
    this.vaoElements = new Map([[0, 0]]);   // vertex array -> its ELEMENT_ARRAY_BUFFER
    // `${vertex array}:${index}` -> { enabled, buffer }: a draw with an enabled attribute
    // that has no buffer is INVALID_OPERATION (WebGL: no client-side arrays)
    this.attribs = new Map();
    this.uniforms = new Map();    // program -> [WebGLUniformLocation | true] by location
    this.programOfLocation = new Map();
    this.ended = new Map();       // query / sync name -> frame it ended / was made
    this.extensions = new Set();
    this.model.vaoElement = () => this.vaoElements.get(this.model.vertexArray) ?? 0;
  }
  getError() { return this.ctx ? this.ctx.getError() : 0; }

  // ---- helpers ---------------------------------------------------------------------
  obj(kind, n) { return n ? this.objs[kind].get(n) ?? null : null; }
  put(kind, n, o) { if (o) { this.objs[kind].set(n, o); this.names.set(o, n); } }
  frame() { return this.host.frameIndex; }
  bytesIn(ptr, len) { return ptr ? this.host.bytes(ptr, len) : null; }
  ints(dst, count, values, kind = 'i32') {
    const n = Math.min(count >>> 0, values.length);
    if (n) {
      const dv = this.host.view();
      this.host.bytes(dst, n * (kind === 'i64' ? 8 : 4));
      for (let i = 0; i < n; i++) {
        const v = values[i];
        if (kind === 'f32') dv.setFloat32(dst + i * 4, Number(v), true);
        else if (kind === 'i64') dv.setBigInt64(dst + i * 8, BigInt(Math.trunc(Number(v))), true);
        else dv.setInt32(dst + i * 4, typeof v === 'boolean' ? (v ? 1 : 0) : Number(v) | 0, true);
      }
    }
    return values.length;
  }
  text(dst, cap, s) { return this.host.copyIfFits(dst, cap, new TextEncoder().encode(s ?? '')); }
  /** A parameter's values as an array (getParameter-style results normalised). */
  values(v) {
    if (v === null || v === undefined) return null;
    if (typeof v === 'object' && !ArrayBuffer.isView(v) && !Array.isArray(v)) return [this.names.get(v) ?? 0];
    if (ArrayBuffer.isView(v) || Array.isArray(v)) return Array.from(v, (x) => (typeof x === 'boolean' ? (x ? 1 : 0) : x));
    return [typeof v === 'boolean' ? (v ? 1 : 0) : v];
  }
  params(pname) {
    const m = MODEL_PARAMS[pname];
    if (m) return m(this.model);
    if (!this.ctx) return NULL_LIMITS[pname] ?? [0];
    return this.values(this.ctx.getParameter(pname));
  }
  create(kind, make) {
    const n = this.model.create(kind);
    if (this.ctx) this.put(kind, n, make());
    return n;
  }
  remove(kind, n, del) {
    if (!this.model.remove(kind, n)) return;
    const o = this.objs[kind].get(n);
    this.objs[kind].delete(n);
    if (this.ctx && o) del(o);
  }

  // ---- frames, context ---------------------------------------------------------------
  width() { return this.ctx ? this.ctx.drawingBufferWidth : 1280; }
  height() { return this.ctx ? this.ctx.drawingBufferHeight : 720; }
  // 0 only on catch-up frames (the runner shows the batch's last one): headless runs draw too
  frame_shown() { return this.host.catchUp ? 0 : 1; }
  present() {}
  get_error() { return this.model.getError(); }
  get_string(name, dst, cap) {
    let s;
    if (!this.ctx) s = NULL_STRINGS[name];
    else if (name === GL.EXTENSIONS) s = (this.ctx.getSupportedExtensions() ?? []).join(' ');
    else s = this.ctx.getParameter(name);
    if (typeof s !== 'string') { this.model.error(GL.INVALID_ENUM); return -1; }
    return this.text(dst, cap, s);
  }
  enable_extension(ptr, len) {
    const name = this.host.str(ptr, len);
    if (!this.ctx || !this.ctx.getExtension(name)) return 0;
    this.extensions.add(name);
    return 1;
  }
  get_integerv(pname, dst, count) { const v = this.params(pname); return v ? this.ints(dst, count, v) : (this.model.error(GL.INVALID_ENUM), -1); }
  get_floatv(pname, dst, count) { const v = this.params(pname); return v ? this.ints(dst, count, v, 'f32') : (this.model.error(GL.INVALID_ENUM), -1); }
  get_integer64v(pname, dst, count) { const v = this.params(pname); return v ? this.ints(dst, count, v, 'i64') : (this.model.error(GL.INVALID_ENUM), -1); }
  get_integeri_v(target, index, dst, count) {
    const v = this.ctx ? this.values(this.ctx.getIndexedParameter(target, index)) : [0];
    return v ? this.ints(dst, count, v) : (this.model.error(GL.INVALID_ENUM), -1);
  }
  get_internalformativ(target, format, pname, dst, count) {
    const v = this.ctx ? this.values(this.ctx.getInternalformatParameter(target, format, pname)) : (pname === 0x9380 ? [1] : [4]);
    return v ? this.ints(dst, count, v) : (this.model.error(GL.INVALID_ENUM), -1);
  }
  get_shader_precision_format(shadertype, precisiontype, dst) {
    const f = this.ctx?.getShaderPrecisionFormat(shadertype, precisiontype);
    this.ints(dst, 3, f ? [f.rangeMin, f.rangeMax, f.precision] : [127, 127, 23]);
  }

  // ---- state (straight through) --------------------------------------------------------
  active_texture(t) {
    const u = t - GL.TEXTURE0;
    if (u < 0 || u >= 32) return this.model.error(GL.INVALID_ENUM);
    this.model.unit = u;
    this.ctx?.activeTexture(t);
  }
  blend_color(r, g, b, a) { this.ctx?.blendColor(r, g, b, a); }
  blend_equation(m) { this.ctx?.blendEquation(m); }
  blend_equation_separate(a, b) { this.ctx?.blendEquationSeparate(a, b); }
  blend_func(s, d) { this.ctx?.blendFunc(s, d); }
  blend_func_separate(a, b, c, d) { this.ctx?.blendFuncSeparate(a, b, c, d); }
  clear(mask) { this.ctx?.clear(mask); }
  clear_color(r, g, b, a) { this.ctx?.clearColor(r, g, b, a); }
  clear_depthf(d) { this.ctx?.clearDepth(d); }
  clear_stencil(s) { this.ctx?.clearStencil(s); }
  color_mask(r, g, b, a) { this.ctx?.colorMask(!!r, !!g, !!b, !!a); }
  cull_face(m) { this.ctx?.cullFace(m); }
  depth_func(f) { this.ctx?.depthFunc(f); }
  depth_mask(f) { this.ctx?.depthMask(!!f); }
  depth_rangef(n, f) { this.ctx?.depthRange(n, f); }
  disable(c) { this.ctx?.disable(c); }
  enable(c) { this.ctx?.enable(c); }
  is_enabled(c) { return this.ctx?.isEnabled(c) ? 1 : 0; }
  front_face(m) { this.ctx?.frontFace(m); }
  hint(t, m) { this.ctx?.hint(t, m); }
  line_width(w) { this.ctx?.lineWidth(w); }
  pixel_storei(pname, v) { if (this.model.pixelStore(pname, v)) this.ctx?.pixelStorei(pname, v); }
  polygon_offset(f, u) { this.ctx?.polygonOffset(f, u); }
  sample_coverage(v, inv) { this.ctx?.sampleCoverage(v, !!inv); }
  scissor(x, y, w, h) { if (this.model.nonNegative(w, h)) this.ctx?.scissor(x, y, w, h); }
  viewport(x, y, w, h) { if (this.model.nonNegative(w, h)) this.ctx?.viewport(x, y, w, h); }
  stencil_func(f, r, m) { this.ctx?.stencilFunc(f, r, m); }
  stencil_func_separate(face, f, r, m) { this.ctx?.stencilFuncSeparate(face, f, r, m); }
  stencil_mask(m) { this.ctx?.stencilMask(m); }
  stencil_mask_separate(f, m) { this.ctx?.stencilMaskSeparate(f, m); }
  stencil_op(a, b, c) { this.ctx?.stencilOp(a, b, c); }
  stencil_op_separate(f, a, b, c) { this.ctx?.stencilOpSeparate(f, a, b, c); }
  finish() { this.ctx?.finish(); }
  flush() { this.ctx?.flush(); }

  // ---- buffers -------------------------------------------------------------------------
  create_buffer() { return this.create('buffer', () => this.ctx.createBuffer()); }
  delete_buffer(n) {
    for (const [t, b] of this.model.buffers) if (b === n) this.model.buffers.delete(t);
    for (const [v, b] of this.vaoElements) if (b === n) this.vaoElements.set(v, 0);
    for (const a of this.attribs.values()) if (a.buffer === n) a.buffer = 0;
    this.remove('buffer', n, (o) => this.ctx.deleteBuffer(o));
  }
  is_buffer(n) { return this.model.is('buffer', n); }
  bind_buffer(t, n) {
    const m = this.model;
    if (!m.bufferTarget(t) || !m.valid('buffer', n)) return;
    if (t === GL.ELEMENT_ARRAY_BUFFER) this.vaoElements.set(m.vertexArray, n);
    else m.buffers.set(t, n);
    this.ctx?.bindBuffer(t, this.obj('buffer', n));
  }
  bind_buffer_base(t, i, n) {
    const m = this.model;
    if (!m.target([GL.UNIFORM_BUFFER, GL.TRANSFORM_FEEDBACK_BUFFER], t) || !m.valid('buffer', n)) return;
    m.buffers.set(t, n);
    this.ctx?.bindBufferBase(t, i, this.obj('buffer', n));
  }
  bind_buffer_range(t, i, n, off, size) {
    const m = this.model;
    if (!m.target([GL.UNIFORM_BUFFER, GL.TRANSFORM_FEEDBACK_BUFFER], t) || !m.valid('buffer', n)) return;
    m.buffers.set(t, n);
    this.ctx?.bindBufferRange(t, i, this.obj('buffer', n), off, size);
  }
  bufferBound(t) {
    const m = this.model;
    if (!m.bufferTarget(t)) return false;
    const b = t === GL.ELEMENT_ARRAY_BUFFER ? m.vaoElement() : m.boundBuffer(t);
    return b !== 0 || m.error(GL.INVALID_OPERATION);
  }
  buffer_data(t, ptr, len, usage) {
    const data = this.bytesIn(ptr, len);
    if (!this.bufferBound(t)) return;
    this.model.fold([1, t, 0, len], data);
    this.ctx?.bufferData(t, data ?? len, usage);
  }
  buffer_sub_data(t, off, ptr, len) {
    const data = this.host.bytes(ptr, len);
    if (!this.bufferBound(t)) return;
    this.model.fold([1, t, off, len], data);
    this.ctx?.bufferSubData(t, off, data);
  }
  copy_buffer_sub_data(rt, wt, ro, wo, size) {
    if (this.bufferBound(rt) && this.bufferBound(wt)) this.ctx?.copyBufferSubData(rt, wt, ro, wo, size);
  }
  get_buffer_sub_data(t, off, dst, len) {
    const out = this.host.bytes(dst, len);
    if (!this.bufferBound(t)) return;
    if (this.ctx) this.ctx.getBufferSubData(t, off, out);
    else out.fill(0);
  }
  get_buffer_parameteriv(t, pname) {
    if (!this.bufferBound(t)) return 0;
    return this.ctx ? Number(this.ctx.getBufferParameter(t, pname) ?? 0) : 0;
  }

  // ---- vertex arrays ---------------------------------------------------------------------
  create_vertex_array() { const n = this.create('vertexArray', () => this.ctx.createVertexArray()); this.vaoElements.set(n, 0); return n; }
  delete_vertex_array(n) {
    if (this.model.vertexArray === n) this.model.vertexArray = 0;
    this.vaoElements.delete(n);
    for (const k of [...this.attribs.keys()]) if (k.startsWith(`${n}:`)) this.attribs.delete(k);
    this.remove('vertexArray', n, (o) => this.ctx.deleteVertexArray(o));
  }
  is_vertex_array(n) { return this.model.is('vertexArray', n); }
  bind_vertex_array(n) {
    if (!this.model.valid('vertexArray', n)) return;
    this.model.vertexArray = n;
    this.ctx?.bindVertexArray(this.obj('vertexArray', n));
  }
  /** The attribute's record in the bound vertex array (null: index out of range, INVALID_VALUE). */
  attrib(i) {
    if (i >>> 0 >= NULL_LIMITS[0x8869][0]) { this.model.error(GL.INVALID_VALUE); return null; }
    const k = `${this.model.vertexArray}:${i}`;
    return this.attribs.get(k) ?? this.attribs.set(k, { enabled: false, buffer: 0 }).get(k);
  }
  enable_vertex_attrib_array(i) { const a = this.attrib(i); if (a) { a.enabled = true; this.ctx?.enableVertexAttribArray(i); } }
  disable_vertex_attrib_array(i) { const a = this.attrib(i); if (a) { a.enabled = false; this.ctx?.disableVertexAttribArray(i); } }
  vertex_attrib_pointer(i, size, type, norm, stride, off) {
    const a = this.attrib(i);
    if (!a) return;
    if (this.model.boundBuffer(GL.ARRAY_BUFFER) === 0 && off !== 0) return this.model.error(GL.INVALID_OPERATION);
    a.buffer = this.model.boundBuffer(GL.ARRAY_BUFFER);
    this.ctx?.vertexAttribPointer(i, size, type, !!norm, stride, off);
  }
  vertex_attrib_ipointer(i, size, type, stride, off) {
    const a = this.attrib(i);
    if (!a) return;
    if (this.model.boundBuffer(GL.ARRAY_BUFFER) === 0 && off !== 0) return this.model.error(GL.INVALID_OPERATION);
    a.buffer = this.model.boundBuffer(GL.ARRAY_BUFFER);
    this.ctx?.vertexAttribIPointer(i, size, type, stride, off);
  }
  /** A draw may go ahead: every enabled attribute of the bound vertex array has a buffer. */
  attribsReady() {
    const v = `${this.model.vertexArray}:`;
    for (const [k, a] of this.attribs) if (a.enabled && !a.buffer && k.startsWith(v)) return this.model.error(GL.INVALID_OPERATION);
    return true;
  }
  vertex_attrib_divisor(i, d) { this.ctx?.vertexAttribDivisor(i, d); }
  vertex_attrib4f(i, x, y, z, w) { this.ctx?.vertexAttrib4f(i, x, y, z, w); }
  vertex_attribi4i(i, x, y, z, w) { this.ctx?.vertexAttribI4i(i, x, y, z, w); }
  vertex_attribi4ui(i, x, y, z, w) { this.ctx?.vertexAttribI4ui(i, x, y, z, w); }
  get_vertex_attribiv(i, pname) { const v = this.ctx ? this.values(this.ctx.getVertexAttrib(i, pname)) : [0]; return v ? v[0] : 0; }
  get_vertex_attribfv(i, pname, dst, count) { const v = this.ctx ? this.values(this.ctx.getVertexAttrib(i, pname)) : [0, 0, 0, 1]; return v ? this.ints(dst, count, v, 'f32') : -1; }
  get_vertex_attrib_offset(i, pname) { return this.ctx ? this.ctx.getVertexAttribOffset(i, pname) : 0; }

  // ---- drawing -----------------------------------------------------------------------------
  draw_arrays(mode, first, count) { if (this.model.nonNegative(first, count) && this.attribsReady()) this.ctx?.drawArrays(mode, first, count); }
  draw_elements(mode, count, type, off) { if (this.model.nonNegative(count) && this.attribsReady()) this.ctx?.drawElements(mode, count, type, off); }
  draw_arrays_instanced(mode, first, count, n) { if (this.model.nonNegative(first, count, n) && this.attribsReady()) this.ctx?.drawArraysInstanced(mode, first, count, n); }
  draw_elements_instanced(mode, count, type, off, n) { if (this.model.nonNegative(count, n) && this.attribsReady()) this.ctx?.drawElementsInstanced(mode, count, type, off, n); }
  draw_range_elements(mode, start, end, count, type, off) {
    if (this.model.nonNegative(count) && (end >= start || this.model.error(GL.INVALID_VALUE)) && this.attribsReady()) this.ctx?.drawRangeElements(mode, start, end, count, type, off);
  }
  draw_buffers(ptr, count) { const v = new Uint32Array(this.host.bytes(ptr, count * 4).slice().buffer); this.ctx?.drawBuffers(Array.from(v)); }
  clear_bufferiv(buf, db, ptr, count) { const v = new Int32Array(this.host.bytes(ptr, count * 4).slice().buffer); this.ctx?.clearBufferiv(buf, db, v); }
  clear_bufferuiv(buf, db, ptr, count) { const v = new Uint32Array(this.host.bytes(ptr, count * 4).slice().buffer); this.ctx?.clearBufferuiv(buf, db, v); }
  clear_bufferfv(buf, db, ptr, count) { const v = new Float32Array(this.host.bytes(ptr, count * 4).slice().buffer); this.ctx?.clearBufferfv(buf, db, v); }
  clear_bufferfi(buf, db, depth, stencil) { this.ctx?.clearBufferfi(buf, db, depth, stencil); }

  // ---- textures -------------------------------------------------------------------------------
  create_texture() { return this.create('texture', () => this.ctx.createTexture()); }
  delete_texture(n) {
    for (const [k, t] of this.model.textures) if (t === n) this.model.textures.delete(k);
    this.model.textureTargets.delete(n);
    this.remove('texture', n, (o) => this.ctx.deleteTexture(o));
  }
  is_texture(n) { return this.model.is('texture', n); }
  bind_texture(t, n) {
    const m = this.model;
    if (!m.textureTarget(t) || !m.valid('texture', n)) return;
    if (n && (m.textureTargets.get(n) ?? m.textureTargets.set(n, t).get(n)) !== t) return void m.error(GL.INVALID_OPERATION);
    m.textures.set(`${m.unit}:${t}`, n);
    this.ctx?.bindTexture(t, this.obj('texture', n));
  }
  tex_parameteri(t, p, v) { if (this.model.textureTarget(t) && this.model.needTexture(t)) this.ctx?.texParameteri(t, p, v); }
  tex_parameterf(t, p, v) { if (this.model.textureTarget(t) && this.model.needTexture(t)) this.ctx?.texParameterf(t, p, v); }
  get_tex_parameteriv(t, p) { return this.model.textureTarget(t) && this.model.needTexture(t) && this.ctx ? Number(this.ctx.getTexParameter(t, p) ?? 0) : 0; }
  get_tex_parameterfv(t, p) { return this.model.textureTarget(t) && this.model.needTexture(t) && this.ctx ? Number(this.ctx.getTexParameter(t, p) ?? 0) : 0; }
  /** Pixel data for an upload: null (no data), a byte view, or (unpack buffer bound) an offset. */
  pixelsIn(ptr, len, w, h, d, format, type) {
    const m = this.model;
    const unpack = m.boundBuffer(GL.PIXEL_UNPACK_BUFFER);
    if (unpack) return { offset: len };   // GLES: the pointer is an offset into the buffer
    if (!ptr) return { data: null };
    const bpp = pixelBytes(format, type);
    if (!bpp) { m.error(GL.INVALID_ENUM); return null; }
    const need = imageBytes(w, h, d, bpp, m.unpack);
    if (len < need) throw new Error(`gasm:gl: ${len} bytes of pixels for a ${w}x${h}x${d} image that needs ${need}`);
    const data = this.host.bytes(ptr, len);
    return { data, view: pixelView(data, type).view };   // hashed as bytes, given to WebGL typed
  }
  upload(header, px) { this.model.fold(header, px.data ?? null); }
  tex_image_2d(t, level, ifmt, w, h, border, format, type, ptr, len) {
    const m = this.model;
    if (!m.imageTarget(t) || !m.needTexture(t) || !m.nonNegative(level, w, h)) return;
    const px = this.pixelsIn(ptr, len, w, h, 1, format, type);
    if (!px) return;
    this.upload([2, t, level, ifmt, 0, 0, 0, w, h, 1, format, type, px.data ? len : 0], px);
    if (px.offset !== undefined) this.ctx?.texImage2D(t, level, ifmt, w, h, border, format, type, px.offset);
    else this.ctx?.texImage2D(t, level, ifmt, w, h, border, format, type, px.data && px.view);
  }
  tex_image_3d(t, level, ifmt, w, h, d, border, format, type, ptr, len) {
    const m = this.model;
    if (!m.target([GL.TEXTURE_3D, GL.TEXTURE_2D_ARRAY], t) || !m.needTexture(t) || !m.nonNegative(level, w, h, d)) return;
    const px = this.pixelsIn(ptr, len, w, h, d, format, type);
    if (!px) return;
    this.upload([2, t, level, ifmt, 0, 0, 0, w, h, d, format, type, px.data ? len : 0], px);
    if (px.offset !== undefined) this.ctx?.texImage3D(t, level, ifmt, w, h, d, border, format, type, px.offset);
    else this.ctx?.texImage3D(t, level, ifmt, w, h, d, border, format, type, px.data && px.view);
  }
  tex_sub_image_2d(t, level, x, y, w, h, format, type, ptr, len) {
    const m = this.model;
    if (!m.imageTarget(t) || !m.needTexture(t) || !m.nonNegative(level, x, y, w, h)) return;
    const px = this.pixelsIn(ptr, len, w, h, 1, format, type);
    if (!px) return;
    if (!px.data && px.offset === undefined) return m.error(GL.INVALID_VALUE);
    this.upload([2, t, level, 0, x, y, 0, w, h, 1, format, type, px.data ? len : 0], px);
    if (px.offset !== undefined) this.ctx?.texSubImage2D(t, level, x, y, w, h, format, type, px.offset);
    else this.ctx?.texSubImage2D(t, level, x, y, w, h, format, type, px.data && px.view);
  }
  tex_sub_image_3d(t, level, x, y, z, w, h, d, format, type, ptr, len) {
    const m = this.model;
    if (!m.target([GL.TEXTURE_3D, GL.TEXTURE_2D_ARRAY], t) || !m.needTexture(t) || !m.nonNegative(level, x, y, z, w, h, d)) return;
    const px = this.pixelsIn(ptr, len, w, h, d, format, type);
    if (!px) return;
    if (!px.data && px.offset === undefined) return m.error(GL.INVALID_VALUE);
    this.upload([2, t, level, 0, x, y, z, w, h, d, format, type, px.data ? len : 0], px);
    if (px.offset !== undefined) this.ctx?.texSubImage3D(t, level, x, y, z, w, h, d, format, type, px.offset);
    else this.ctx?.texSubImage3D(t, level, x, y, z, w, h, d, format, type, px.data && px.view);
  }
  tex_storage_2d(t, levels, ifmt, w, h) {
    if (this.model.target([GL.TEXTURE_2D, GL.TEXTURE_CUBE_MAP], t) && this.model.needTexture(t) && (levels > 0 && w > 0 && h > 0 || this.model.error(GL.INVALID_VALUE))) this.ctx?.texStorage2D(t, levels, ifmt, w, h);
  }
  tex_storage_3d(t, levels, ifmt, w, h, d) {
    if (this.model.target([GL.TEXTURE_3D, GL.TEXTURE_2D_ARRAY], t) && this.model.needTexture(t) && (levels > 0 && w > 0 && h > 0 && d > 0 || this.model.error(GL.INVALID_VALUE))) this.ctx?.texStorage3D(t, levels, ifmt, w, h, d);
  }
  compressed_tex_image_2d(t, level, ifmt, w, h, border, ptr, len) {
    const data = this.host.bytes(ptr, len);
    if (!this.model.imageTarget(t) || !this.model.needTexture(t) || !this.model.nonNegative(level, w, h)) return;
    this.model.fold([2, t, level, ifmt, 0, 0, 0, w, h, 1, ifmt, 0, len], data);
    this.ctx?.compressedTexImage2D(t, level, ifmt, w, h, border, data);
  }
  compressed_tex_image_3d(t, level, ifmt, w, h, d, border, ptr, len) {
    const data = this.host.bytes(ptr, len);
    if (!this.model.target([GL.TEXTURE_3D, GL.TEXTURE_2D_ARRAY], t) || !this.model.needTexture(t) || !this.model.nonNegative(level, w, h, d)) return;
    this.model.fold([2, t, level, ifmt, 0, 0, 0, w, h, d, ifmt, 0, len], data);
    this.ctx?.compressedTexImage3D(t, level, ifmt, w, h, d, border, data);
  }
  compressed_tex_sub_image_2d(t, level, x, y, w, h, format, ptr, len) {
    const data = this.host.bytes(ptr, len);
    if (!this.model.imageTarget(t) || !this.model.needTexture(t) || !this.model.nonNegative(level, x, y, w, h)) return;
    this.model.fold([2, t, level, 0, x, y, 0, w, h, 1, format, 0, len], data);
    this.ctx?.compressedTexSubImage2D(t, level, x, y, w, h, format, data);
  }
  compressed_tex_sub_image_3d(t, level, x, y, z, w, h, d, format, ptr, len) {
    const data = this.host.bytes(ptr, len);
    if (!this.model.target([GL.TEXTURE_3D, GL.TEXTURE_2D_ARRAY], t) || !this.model.needTexture(t) || !this.model.nonNegative(level, x, y, z, w, h, d)) return;
    this.model.fold([2, t, level, 0, x, y, z, w, h, d, format, 0, len], data);
    this.ctx?.compressedTexSubImage3D(t, level, x, y, z, w, h, d, format, data);
  }
  copy_tex_image_2d(t, level, ifmt, x, y, w, h, border) { if (this.model.imageTarget(t) && this.model.needTexture(t)) this.ctx?.copyTexImage2D(t, level, ifmt, x, y, w, h, border); }
  copy_tex_sub_image_2d(t, level, xo, yo, x, y, w, h) { if (this.model.imageTarget(t) && this.model.needTexture(t)) this.ctx?.copyTexSubImage2D(t, level, xo, yo, x, y, w, h); }
  copy_tex_sub_image_3d(t, level, xo, yo, zo, x, y, w, h) { if (this.model.needTexture(t)) this.ctx?.copyTexSubImage3D(t, level, xo, yo, zo, x, y, w, h); }
  generate_mipmap(t) { if (this.model.textureTarget(t) && this.model.needTexture(t)) this.ctx?.generateMipmap(t); }

  // ---- samplers --------------------------------------------------------------------------------
  create_sampler() { return this.create('sampler', () => this.ctx.createSampler()); }
  delete_sampler(n) { this.remove('sampler', n, (o) => this.ctx.deleteSampler(o)); }
  is_sampler(n) { return this.model.is('sampler', n); }
  bind_sampler(unit, n) { if (this.model.valid('sampler', n)) this.ctx?.bindSampler(unit, this.obj('sampler', n)); }
  sampler_parameteri(n, p, v) { if (this.model.valid('sampler', n, false)) this.ctx?.samplerParameteri(this.obj('sampler', n), p, v); }
  sampler_parameterf(n, p, v) { if (this.model.valid('sampler', n, false)) this.ctx?.samplerParameterf(this.obj('sampler', n), p, v); }
  get_sampler_parameteriv(n, p) { return this.model.valid('sampler', n, false) && this.ctx ? Number(this.ctx.getSamplerParameter(this.obj('sampler', n), p) ?? 0) : 0; }
  get_sampler_parameterfv(n, p) { return this.model.valid('sampler', n, false) && this.ctx ? Number(this.ctx.getSamplerParameter(this.obj('sampler', n), p) ?? 0) : 0; }

  // ---- framebuffers, renderbuffers ---------------------------------------------------------------
  create_framebuffer() { return this.create('framebuffer', () => this.ctx.createFramebuffer()); }
  delete_framebuffer(n) {
    for (const [t, f] of this.model.framebuffers) if (f === n) this.model.framebuffers.delete(t);
    this.remove('framebuffer', n, (o) => this.ctx.deleteFramebuffer(o));
  }
  is_framebuffer(n) { return this.model.is('framebuffer', n); }
  bind_framebuffer(t, n) {
    const m = this.model;
    if (!m.target(FB_TARGETS, t) || !m.valid('framebuffer', n)) return;
    if (t === GL.FRAMEBUFFER) { m.framebuffers.set(GL.DRAW_FRAMEBUFFER, n); m.framebuffers.set(GL.READ_FRAMEBUFFER, n); } else m.framebuffers.set(t, n);
    this.ctx?.bindFramebuffer(t, this.obj('framebuffer', n));
  }
  check_framebuffer_status(t) {
    if (!this.model.target(FB_TARGETS, t)) return 0;
    if (this.ctx) return this.ctx.checkFramebufferStatus(t);
    return GL.FRAMEBUFFER_COMPLETE;
  }
  framebuffer_texture_2d(t, att, tt, tex, level) {
    if (this.model.target(FB_TARGETS, t) && this.model.valid('texture', tex)) this.ctx?.framebufferTexture2D(t, att, tt, this.obj('texture', tex), level);
  }
  framebuffer_texture_layer(t, att, tex, level, layer) {
    if (this.model.target(FB_TARGETS, t) && this.model.valid('texture', tex)) this.ctx?.framebufferTextureLayer(t, att, this.obj('texture', tex), level, layer);
  }
  framebuffer_renderbuffer(t, att, rt, rb) {
    if (this.model.target(FB_TARGETS, t) && this.model.valid('renderbuffer', rb)) this.ctx?.framebufferRenderbuffer(t, att, rt, this.obj('renderbuffer', rb));
  }
  get_framebuffer_attachment_parameteriv(t, att, p) {
    if (!this.model.target(FB_TARGETS, t) || !this.ctx) return 0;
    const v = this.values(this.ctx.getFramebufferAttachmentParameter(t, att, p));
    return v ? v[0] : 0;
  }
  blit_framebuffer(a, b, c, d, e, f, g, h, mask, filter) { this.ctx?.blitFramebuffer(a, b, c, d, e, f, g, h, mask, filter); }
  invalidate_framebuffer(t, ptr, count) { const v = new Uint32Array(this.host.bytes(ptr, count * 4).slice().buffer); this.ctx?.invalidateFramebuffer(t, Array.from(v)); }
  invalidate_sub_framebuffer(t, ptr, count, x, y, w, h) { const v = new Uint32Array(this.host.bytes(ptr, count * 4).slice().buffer); this.ctx?.invalidateSubFramebuffer(t, Array.from(v), x, y, w, h); }
  read_buffer(src) { this.ctx?.readBuffer(src); }
  read_pixels(x, y, w, h, format, type, dst, len) {
    const m = this.model;
    if (!m.nonNegative(w, h)) return;
    const pack = m.boundBuffer(GL.PIXEL_PACK_BUFFER);
    if (pack) { this.ctx?.readPixels(x, y, w, h, format, type, dst); return; }
    const bpp = pixelBytes(format, type);
    if (!bpp) return m.error(GL.INVALID_ENUM);
    const need = imageBytes(w, h, 1, bpp, m.pack);
    if (len < need) throw new Error(`gasm:gl: read_pixels: ${len} bytes for ${w}x${h} that needs ${need}`);
    const out = this.host.bytes(dst, len);
    if (!this.ctx) return void out.fill(0, 0, need);
    const { view, copied } = pixelView(out, type);
    this.ctx.readPixels(x, y, w, h, format, type, view);
    if (copied) out.set(new Uint8Array(view.buffer, 0, view.byteLength));
  }
  create_renderbuffer() { return this.create('renderbuffer', () => this.ctx.createRenderbuffer()); }
  delete_renderbuffer(n) {
    if (this.model.renderbuffer === n) this.model.renderbuffer = 0;
    this.remove('renderbuffer', n, (o) => this.ctx.deleteRenderbuffer(o));
  }
  is_renderbuffer(n) { return this.model.is('renderbuffer', n); }
  bind_renderbuffer(t, n) {
    if (!this.model.target([GL.RENDERBUFFER], t) || !this.model.valid('renderbuffer', n)) return;
    this.model.renderbuffer = n;
    this.ctx?.bindRenderbuffer(t, this.obj('renderbuffer', n));
  }
  needRenderbuffer(t) { return (this.model.target([GL.RENDERBUFFER], t) && this.model.renderbuffer !== 0) || (t === GL.RENDERBUFFER && this.model.error(GL.INVALID_OPERATION)); }
  renderbuffer_storage(t, f, w, h) { if (this.needRenderbuffer(t) && this.model.nonNegative(w, h)) this.ctx?.renderbufferStorage(t, f, w, h); }
  renderbuffer_storage_multisample(t, s, f, w, h) { if (this.needRenderbuffer(t) && this.model.nonNegative(s, w, h)) this.ctx?.renderbufferStorageMultisample(t, s, f, w, h); }
  get_renderbuffer_parameteriv(t, p) { return this.needRenderbuffer(t) && this.ctx ? Number(this.ctx.getRenderbufferParameter(t, p) ?? 0) : 0; }

  // ---- shaders and programs ------------------------------------------------------------------------
  create_shader(type) {
    if (type !== GL.VERTEX_SHADER && type !== GL.FRAGMENT_SHADER) { this.model.error(GL.INVALID_ENUM); return 0; }
    const n = this.create('shader', () => this.ctx.createShader(type));
    this.shaderInfo ??= new Map();
    this.shaderInfo.set(n, { type, source: '' });
    return n;
  }
  delete_shader(n) { this.remove('shader', n, (o) => this.ctx.deleteShader(o)); }
  is_shader(n) { return this.model.is('shader', n); }
  shader_source(n, ptr, len) {
    const src = this.host.str(ptr, len);
    if (!this.model.valid('shader', n, false)) return;
    this.shaderInfo.get(n).source = src;
    this.ctx?.shaderSource(this.obj('shader', n), src);
  }
  compile_shader(n) { if (this.model.valid('shader', n, false)) this.ctx?.compileShader(this.obj('shader', n)); }
  get_shaderiv(n, p) {
    if (!this.model.valid('shader', n, false)) return 0;
    const info = this.shaderInfo.get(n);
    if (p === GL.SHADER_TYPE) return info.type;
    if (p === GL.SHADER_SOURCE_LENGTH) return info.source ? new TextEncoder().encode(info.source).length + 1 : 0;
    if (!this.ctx) return p === GL.COMPILE_STATUS ? 1 : 0;
    if (p === GL.INFO_LOG_LENGTH) { const l = this.ctx.getShaderInfoLog(this.obj('shader', n)) ?? ''; return l ? new TextEncoder().encode(l).length + 1 : 0; }
    const v = this.ctx.getShaderParameter(this.obj('shader', n), p);
    return v === null ? (this.model.error(GL.INVALID_ENUM), 0) : Number(v);
  }
  get_shader_info_log(n, dst, cap) { return this.model.valid('shader', n, false) ? this.text(dst, cap, this.ctx ? this.ctx.getShaderInfoLog(this.obj('shader', n)) : '') : -1; }
  get_shader_source(n, dst, cap) { return this.model.valid('shader', n, false) ? this.text(dst, cap, this.shaderInfo.get(n).source) : -1; }
  create_program() { return this.create('program', () => this.ctx.createProgram()); }
  delete_program(n) {
    if (this.model.program === n) this.model.program = 0;
    this.uniforms.delete(n);
    this.remove('program', n, (o) => this.ctx.deleteProgram(o));
  }
  is_program(n) { return this.model.is('program', n); }
  attach_shader(p, s) { if (this.model.valid('program', p, false) && this.model.valid('shader', s, false)) this.ctx?.attachShader(this.obj('program', p), this.obj('shader', s)); }
  detach_shader(p, s) { if (this.model.valid('program', p, false) && this.model.valid('shader', s, false)) this.ctx?.detachShader(this.obj('program', p), this.obj('shader', s)); }
  link_program(p) {
    if (!this.model.valid('program', p, false)) return;
    this.uniforms.delete(p);   // locations are the link's
    this.ctx?.linkProgram(this.obj('program', p));
  }
  use_program(p) {
    if (!this.model.valid('program', p)) return;
    this.model.program = p;
    this.ctx?.useProgram(this.obj('program', p));
  }
  validate_program(p) { if (this.model.valid('program', p, false)) this.ctx?.validateProgram(this.obj('program', p)); }
  get_programiv(p, pname) {
    if (!this.model.valid('program', p, false)) return 0;
    if (!this.ctx) return pname === GL.LINK_STATUS || pname === GL.VALIDATE_STATUS ? 1 : 0;
    const o = this.obj('program', p);
    if (pname === GL.INFO_LOG_LENGTH) { const l = this.ctx.getProgramInfoLog(o) ?? ''; return l ? new TextEncoder().encode(l).length + 1 : 0; }
    const v = this.ctx.getProgramParameter(o, pname);
    return v === null ? (this.model.error(GL.INVALID_ENUM), 0) : Number(v);
  }
  get_program_info_log(p, dst, cap) { return this.model.valid('program', p, false) ? this.text(dst, cap, this.ctx ? this.ctx.getProgramInfoLog(this.obj('program', p)) : '') : -1; }
  get_attached_shaders(p, dst, count) {
    if (!this.model.valid('program', p, false)) return 0;
    const list = this.ctx ? (this.ctx.getAttachedShaders(this.obj('program', p)) ?? []).map((s) => this.names.get(s) ?? 0) : [];
    return this.ints(dst, count, list);
  }
  bind_attrib_location(p, i, ptr, len) { const name = this.host.str(ptr, len); if (this.model.valid('program', p, false)) this.ctx?.bindAttribLocation(this.obj('program', p), i, name); }
  get_attrib_location(p, ptr, len) { const name = this.host.str(ptr, len); return this.model.valid('program', p, false) ? (this.ctx ? this.ctx.getAttribLocation(this.obj('program', p), name) : 0) : -1; }
  get_frag_data_location(p, ptr, len) { const name = this.host.str(ptr, len); return this.model.valid('program', p, false) ? (this.ctx ? this.ctx.getFragDataLocation(this.obj('program', p), name) : 0) : -1; }
  activeInfo(info, name, dst, cap, infoPtr) {
    if (!info) return -1;
    this.ints(infoPtr, 2, [info.size, info.type]);
    return this.text(dst, cap, info.name);
  }
  get_active_attrib(p, i, dst, cap, infoPtr) { return this.model.valid('program', p, false) && this.ctx ? this.activeInfo(this.ctx.getActiveAttrib(this.obj('program', p), i), 0, dst, cap, infoPtr) : -1; }
  get_active_uniform(p, i, dst, cap, infoPtr) { return this.model.valid('program', p, false) && this.ctx ? this.activeInfo(this.ctx.getActiveUniform(this.obj('program', p), i), 0, dst, cap, infoPtr) : -1; }
  /** Locations are numbered per program in the order the guest asks for them (every runner). */
  get_uniform_location(p, ptr, len) {
    const name = this.host.str(ptr, len);
    if (!this.model.valid('program', p, false)) return -1;
    let table = this.uniforms.get(p);
    if (!table) this.uniforms.set(p, (table = { byName: new Map(), objs: [] }));
    if (table.byName.has(name)) return table.byName.get(name);
    let o = true;
    if (this.ctx) { o = this.ctx.getUniformLocation(this.obj('program', p), name); if (!o) return -1; }
    const loc = table.objs.length;
    table.objs.push(o);
    table.byName.set(name, loc);
    return loc;
  }
  get_uniform_index(p, ptr, len) {
    const name = this.host.str(ptr, len);
    if (!this.model.valid('program', p, false)) return GL.INVALID_INDEX;
    if (!this.ctx) return 0;
    return (this.ctx.getUniformIndices(this.obj('program', p), [name]) ?? [GL.INVALID_INDEX])[0];
  }
  get_active_uniformsiv(p, ptr, count, pname, dst) {
    const idx = Array.from(new Uint32Array(this.host.bytes(ptr, count * 4).slice().buffer));
    if (!this.model.valid('program', p, false)) return;
    const v = this.ctx ? this.values(this.ctx.getActiveUniforms(this.obj('program', p), idx, pname)) ?? [] : idx.map(() => 0);
    this.ints(dst, count, v);
  }
  get_uniform_block_index(p, ptr, len) {
    const name = this.host.str(ptr, len);
    if (!this.model.valid('program', p, false)) return GL.INVALID_INDEX;
    return this.ctx ? this.ctx.getUniformBlockIndex(this.obj('program', p), name) : 0;
  }
  get_active_uniform_block_name(p, i, dst, cap) { return this.model.valid('program', p, false) ? this.text(dst, cap, this.ctx ? this.ctx.getActiveUniformBlockName(this.obj('program', p), i) : '') : -1; }
  get_active_uniform_blockiv(p, i, pname, dst, count) {
    if (!this.model.valid('program', p, false)) return -1;
    const v = this.ctx ? this.values(this.ctx.getActiveUniformBlockParameter(this.obj('program', p), i, pname)) : [0];
    return v ? this.ints(dst, count, v) : -1;
  }
  uniform_block_binding(p, i, b) { if (this.model.valid('program', p, false)) this.ctx?.uniformBlockBinding(this.obj('program', p), i, b); }
  uniformObj(p, loc) { return this.uniforms.get(p)?.objs[loc]; }
  getUniform(p, loc, dst, count, kind) {
    if (!this.model.valid('program', p, false)) return -1;
    const o = this.uniformObj(p, loc);
    if (o === undefined) { this.model.error(GL.INVALID_OPERATION); return -1; }
    const v = this.ctx ? this.values(this.ctx.getUniform(this.obj('program', p), o)) : [0];
    return v ? this.ints(dst, count, v, kind) : -1;
  }
  get_uniformfv(p, loc, dst, count) { return this.getUniform(p, loc, dst, count, 'f32'); }
  get_uniformiv(p, loc, dst, count) { return this.getUniform(p, loc, dst, count, 'i32'); }
  get_uniformuiv(p, loc, dst, count) { return this.getUniform(p, loc, dst, count, 'i32'); }
  transform_feedback_varyings(p, ptr, len, count, mode) {
    const names = new TextDecoder().decode(this.host.bytes(ptr, len)).split('\0').slice(0, count);
    if (names.length < count) throw new Error('gasm:gl: transform_feedback_varyings: fewer names than count');
    if (this.model.valid('program', p, false)) this.ctx?.transformFeedbackVaryings(this.obj('program', p), names, mode);
  }
  get_transform_feedback_varying(p, i, dst, cap, infoPtr) { return this.model.valid('program', p, false) && this.ctx ? this.activeInfo(this.ctx.getTransformFeedbackVarying(this.obj('program', p), i), 0, dst, cap, infoPtr) : -1; }

  // ---- uniforms: header [3, location, kind, count(, transpose)] + the values -----------------------
  uniformTarget(loc) {
    if (loc === -1) return undefined;   // GL: ignored
    const m = this.model;
    if (!m.program) { m.error(GL.INVALID_OPERATION); return undefined; }
    const o = this.uniformObj(m.program, loc);
    if (o === undefined) { m.error(GL.INVALID_OPERATION); return undefined; }
    return o;
  }
  scalar(loc, kind, values, call) {
    const o = this.uniformTarget(loc);
    if (o === undefined) return;
    const bytes = new Uint8Array(values.length * 4), dv = new DataView(bytes.buffer);
    values.forEach((v, i) => (kind < 10 ? dv.setFloat32(i * 4, v, true) : kind < 20 ? dv.setInt32(i * 4, v, true) : dv.setUint32(i * 4, v >>> 0, true)));
    this.model.fold([3, loc, kind, 1], bytes);
    if (this.ctx) call(o);
  }
  vector(loc, count, ptr, kind, comps, call, transpose) {
    if (count < 0) return this.model.error(GL.INVALID_VALUE);
    const bytes = this.host.bytes(ptr, count * comps * 4);
    const o = this.uniformTarget(loc);
    if (o === undefined) return;
    this.model.fold(transpose === undefined ? [3, loc, kind, count] : [3, loc, kind, count, transpose], bytes);
    if (this.ctx) call(o, bytes.slice().buffer);
  }
  uniform1f(l, x) { this.scalar(l, 1, [x], (o) => this.ctx.uniform1f(o, x)); }
  uniform2f(l, x, y) { this.scalar(l, 2, [x, y], (o) => this.ctx.uniform2f(o, x, y)); }
  uniform3f(l, x, y, z) { this.scalar(l, 3, [x, y, z], (o) => this.ctx.uniform3f(o, x, y, z)); }
  uniform4f(l, x, y, z, w) { this.scalar(l, 4, [x, y, z, w], (o) => this.ctx.uniform4f(o, x, y, z, w)); }
  uniform1i(l, x) { this.scalar(l, 11, [x], (o) => this.ctx.uniform1i(o, x)); }
  uniform2i(l, x, y) { this.scalar(l, 12, [x, y], (o) => this.ctx.uniform2i(o, x, y)); }
  uniform3i(l, x, y, z) { this.scalar(l, 13, [x, y, z], (o) => this.ctx.uniform3i(o, x, y, z)); }
  uniform4i(l, x, y, z, w) { this.scalar(l, 14, [x, y, z, w], (o) => this.ctx.uniform4i(o, x, y, z, w)); }
  uniform1ui(l, x) { this.scalar(l, 21, [x], (o) => this.ctx.uniform1ui(o, x)); }
  uniform2ui(l, x, y) { this.scalar(l, 22, [x, y], (o) => this.ctx.uniform2ui(o, x, y)); }
  uniform3ui(l, x, y, z) { this.scalar(l, 23, [x, y, z], (o) => this.ctx.uniform3ui(o, x, y, z)); }
  uniform4ui(l, x, y, z, w) { this.scalar(l, 24, [x, y, z, w], (o) => this.ctx.uniform4ui(o, x, y, z, w)); }
  uniform1fv(l, c, p) { this.vector(l, c, p, 1, 1, (o, b) => this.ctx.uniform1fv(o, new Float32Array(b))); }
  uniform2fv(l, c, p) { this.vector(l, c, p, 2, 2, (o, b) => this.ctx.uniform2fv(o, new Float32Array(b))); }
  uniform3fv(l, c, p) { this.vector(l, c, p, 3, 3, (o, b) => this.ctx.uniform3fv(o, new Float32Array(b))); }
  uniform4fv(l, c, p) { this.vector(l, c, p, 4, 4, (o, b) => this.ctx.uniform4fv(o, new Float32Array(b))); }
  uniform1iv(l, c, p) { this.vector(l, c, p, 11, 1, (o, b) => this.ctx.uniform1iv(o, new Int32Array(b))); }
  uniform2iv(l, c, p) { this.vector(l, c, p, 12, 2, (o, b) => this.ctx.uniform2iv(o, new Int32Array(b))); }
  uniform3iv(l, c, p) { this.vector(l, c, p, 13, 3, (o, b) => this.ctx.uniform3iv(o, new Int32Array(b))); }
  uniform4iv(l, c, p) { this.vector(l, c, p, 14, 4, (o, b) => this.ctx.uniform4iv(o, new Int32Array(b))); }
  uniform1uiv(l, c, p) { this.vector(l, c, p, 21, 1, (o, b) => this.ctx.uniform1uiv(o, new Uint32Array(b))); }
  uniform2uiv(l, c, p) { this.vector(l, c, p, 22, 2, (o, b) => this.ctx.uniform2uiv(o, new Uint32Array(b))); }
  uniform3uiv(l, c, p) { this.vector(l, c, p, 23, 3, (o, b) => this.ctx.uniform3uiv(o, new Uint32Array(b))); }
  uniform4uiv(l, c, p) { this.vector(l, c, p, 24, 4, (o, b) => this.ctx.uniform4uiv(o, new Uint32Array(b))); }
  matrix(l, c, t, p, kind, comps, fn) { this.vector(l, c, p, kind, comps, (o, b) => this.ctx[fn](o, !!t, new Float32Array(b)), t ? 1 : 0); }
  uniform_matrix2fv(l, c, t, p) { this.matrix(l, c, t, p, 32, 4, 'uniformMatrix2fv'); }
  uniform_matrix3fv(l, c, t, p) { this.matrix(l, c, t, p, 33, 9, 'uniformMatrix3fv'); }
  uniform_matrix4fv(l, c, t, p) { this.matrix(l, c, t, p, 34, 16, 'uniformMatrix4fv'); }
  uniform_matrix2x3fv(l, c, t, p) { this.matrix(l, c, t, p, 35, 6, 'uniformMatrix2x3fv'); }
  uniform_matrix3x2fv(l, c, t, p) { this.matrix(l, c, t, p, 36, 6, 'uniformMatrix3x2fv'); }
  uniform_matrix2x4fv(l, c, t, p) { this.matrix(l, c, t, p, 37, 8, 'uniformMatrix2x4fv'); }
  uniform_matrix4x2fv(l, c, t, p) { this.matrix(l, c, t, p, 38, 8, 'uniformMatrix4x2fv'); }
  uniform_matrix3x4fv(l, c, t, p) { this.matrix(l, c, t, p, 39, 12, 'uniformMatrix3x4fv'); }
  uniform_matrix4x3fv(l, c, t, p) { this.matrix(l, c, t, p, 40, 12, 'uniformMatrix4x3fv'); }

  // ---- queries, sync (results from the next frame on, on every runner), transform feedback ----------
  create_query() { return this.create('query', () => this.ctx.createQuery()); }
  delete_query(n) { this.remove('query', n, (o) => this.ctx.deleteQuery(o)); }
  is_query(n) { return this.model.is('query', n); }
  begin_query(t, n) {
    if (!this.model.target(QUERY_TARGETS, t) || !this.model.valid('query', n, false)) return;
    this.active ??= new Map();
    this.active.set(t, n);
    this.ended.delete(n);
    this.ctx?.beginQuery(t, this.obj('query', n));
  }
  end_query(t) {
    if (!this.model.target(QUERY_TARGETS, t)) return;
    const n = this.active?.get(t);
    if (!n) return this.model.error(GL.INVALID_OPERATION);
    this.active.delete(t);
    this.ended.set(n, this.frame());
    this.ctx?.endQuery(t);
  }
  get_queryiv(t, pname) { return this.model.target(QUERY_TARGETS, t) && pname === GL.CURRENT_QUERY ? this.active?.get(t) ?? 0 : 0; }
  get_query_objectuiv(n, pname) {
    if (!this.model.valid('query', n, false)) return 0;
    const ready = this.ended.has(n) && this.frame() > this.ended.get(n);
    if (pname === GL.QUERY_RESULT_AVAILABLE) return ready ? 1 : 0;
    if (pname !== GL.QUERY_RESULT) { this.model.error(GL.INVALID_ENUM); return 0; }
    if (!ready) return 0;
    if (!this.ctx) return 1;   // occlusion "passed"
    return Number(this.ctx.getQueryParameter(this.obj('query', n), GL.QUERY_RESULT) ?? 0);
  }
  fence_sync(cond, flags) {
    if (cond !== GL.SYNC_GPU_COMMANDS_COMPLETE || flags !== 0) { this.model.error(cond !== GL.SYNC_GPU_COMMANDS_COMPLETE ? GL.INVALID_ENUM : GL.INVALID_VALUE); return 0; }
    const n = this.create('sync', () => this.ctx.fenceSync(cond, flags));
    this.ended.set(-n, this.frame());
    return n;
  }
  is_sync(n) { return this.model.is('sync', n); }
  delete_sync(n) { this.ended.delete(-n); this.remove('sync', n, (o) => this.ctx.deleteSync(o)); }
  client_wait_sync(n, flags, timeout) {
    if (!this.model.valid('sync', n, false)) return 0x911D;   // WAIT_FAILED
    return this.frame() > this.ended.get(-n) ? GL.ALREADY_SIGNALED : GL.TIMEOUT_EXPIRED;
  }
  wait_sync(n, flags, timeout) { this.model.valid('sync', n, false); }
  get_synciv(n, pname) {
    if (!this.model.valid('sync', n, false)) return 0;
    if (pname === GL.SYNC_STATUS) return this.frame() > this.ended.get(-n) ? GL.SIGNALED : GL.UNSIGNALED;
    return 0;
  }
  create_transform_feedback() { return this.create('transformFeedback', () => this.ctx.createTransformFeedback()); }
  delete_transform_feedback(n) { this.remove('transformFeedback', n, (o) => this.ctx.deleteTransformFeedback(o)); }
  is_transform_feedback(n) { return this.model.is('transformFeedback', n); }
  bind_transform_feedback(t, n) { if (this.model.target([GL.TRANSFORM_FEEDBACK], t) && this.model.valid('transformFeedback', n)) this.ctx?.bindTransformFeedback(t, this.obj('transformFeedback', n)); }
  begin_transform_feedback(mode) { this.ctx?.beginTransformFeedback(mode); }
  end_transform_feedback() { this.ctx?.endTransformFeedback(); }
  pause_transform_feedback() { this.ctx?.pauseTransformFeedback(); }
  resume_transform_feedback() { this.ctx?.resumeTransformFeedback(); }
}

/** The import object for gasm:gl: every method of GlHost that is an import. */
export function glImports(gl) {
  const out = {};
  for (const name of Object.getOwnPropertyNames(GlHost.prototype)) {
    if (name === 'constructor' || !/^[a-z0-9_]+$/.test(name) || !name.includes('_') && !GL_SINGLE_WORD.has(name)) continue;
    out[name] = (...a) => gl[name](...a);
  }
  return out;
}
// imports whose names have no underscore (the helpers above all do or are camelCase)
const GL_SINGLE_WORD = new Set(['width', 'height', 'present', 'clear', 'disable', 'enable', 'hint', 'scissor', 'viewport',
  'finish', 'flush', 'uniform1f', 'uniform2f', 'uniform3f', 'uniform4f', 'uniform1i', 'uniform2i', 'uniform3i', 'uniform4i',
  'uniform1ui', 'uniform2ui', 'uniform3ui', 'uniform4ui', 'uniform1fv', 'uniform2fv', 'uniform3fv', 'uniform4fv',
  'uniform1iv', 'uniform2iv', 'uniform3iv', 'uniform4iv', 'uniform1uiv', 'uniform2uiv', 'uniform3uiv', 'uniform4uiv']);
