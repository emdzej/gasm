// gasm:gfx validation, shared by every backend (NullGfx, WebGpuGfx in webgpu-gfx.js).
//
// GfxModel numbers handles in creation order (never reused; destroy leaves a
// tombstone), keeps a record of every object and tracks the render pass state, so
// handles, ranges, usages, bind group compatibility and draw ranges trap the same
// way on the null GPU, WebGPU and the native runner (runners/native/src/gfx.rs:
// same rules, same order of checks). Backends only execute what passed.

export const MAX_TEXTURE_SIZE = 8192;   // WebGPU's default maxTextureDimension2D
export const OFFSET_ALIGNMENT = 256;    // dynamic offsets (WebGPU's default alignment limits)
export const MAX_BIND_GROUPS = 4;
export const MAX_VERTEX_BUFFERS = 8;
const MAX_VERTEX_ATTRIBUTES = 16;
const MAX_VERTEX_STRIDE = 2048;
// GASM_BUF_* (spec/abi.json; WebGPU GPUBufferUsage bits)
export const BUF_COPY_DST = 0x08, BUF_INDEX = 0x10, BUF_VERTEX = 0x20, BUF_UNIFORM = 0x40, BUF_STORAGE = 0x80;

const VERTEX_FORMAT_SIZE = {
  float32: 4, uint32: 4, sint32: 4, uint8x4: 4, unorm8x4: 4, uint16x2: 4, float16x2: 4,
  float32x2: 8, uint32x2: 8, sint32x2: 8, uint16x4: 8, float16x4: 8,
  float32x3: 12, uint32x3: 12, sint32x3: 12,
  float32x4: 16, uint32x4: 16, sint32x4: 16,
};
const BLEND_FACTORS = ['zero', 'one', 'src', 'one-minus-src', 'src-alpha', 'one-minus-src-alpha', 'dst', 'one-minus-dst',
  'dst-alpha', 'one-minus-dst-alpha', 'src-alpha-saturated', 'constant', 'one-minus-constant'];
const BLEND_OPS = ['add', 'subtract', 'reverse-subtract', 'min', 'max'];
const COMPARE = ['never', 'less', 'equal', 'less-equal', 'greater', 'not-equal', 'greater-equal', 'always'];
const TOPOLOGIES = ['point-list', 'line-list', 'line-strip', 'triangle-list', 'triangle-strip'];

const isU32 = (v) => Number.isInteger(v) && v >= 0 && v <= 0xffffffff;
const uint = (o, k) => { const v = o?.[k]; if (!isU32(v)) throw new Error(`descriptor: missing number ${JSON.stringify(k)}`); return v; };
const handle = (v) => { if (!isU32(v)) throw new Error(`descriptor: ${JSON.stringify(v)} is not a handle`); return v; };
const oneOf = (v, list, what) => { if (!list.includes(v)) throw new Error(`${what} ${JSON.stringify(v)}`); return v; };
const sameEntries = (a, b) => a.length === b.length && a.every((e, i) => {
  const f = b[i];
  return e.binding === f.binding && e.visibility === f.visibility && e.slot === f.slot && e.dynamic === f.dynamic &&
    e.minBindingSize === f.minBindingSize && e.filterable === f.filterable;
});

// Clamp [x, x+w) x [y, y+h) to a width x height drawable -> [x, y, w, h].
export function clampRect(x, y, w, h, width, height) {
  const cl = (v, hi) => Math.min(Math.max(v, 0), hi);
  const x0 = cl(x, width), y0 = cl(y, height);
  return [x0, y0, cl(x + w, width) - x0, cl(y + h, height) - y0];
}

/** The vertex buffers of a pipeline's vertex stage, validated like createRenderPipeline. */
function vertexSlots(v) {
  const buffers = v.buffers ?? [];
  if (buffers.length > MAX_VERTEX_BUFFERS) throw new Error(`pipeline: at most ${MAX_VERTEX_BUFFERS} vertex buffers, got ${buffers.length}`);
  const locations = [];
  return buffers.map((b, i) => {
    const stride = b.arrayStride ?? 0;
    if (!Number.isInteger(stride) || stride < 0 || stride % 4 || stride > MAX_VERTEX_STRIDE) throw new Error(`pipeline: vertex buffer ${i}: arrayStride ${stride} must be a multiple of 4, at most ${MAX_VERTEX_STRIDE}`);
    const step = b.stepMode ?? 'vertex';
    if (step !== 'vertex' && step !== 'instance') throw new Error(`pipeline: vertex buffer ${i}: unknown stepMode ${JSON.stringify(step)}`);
    let last = 0;
    for (const a of b.attributes ?? []) {
      const size = VERTEX_FORMAT_SIZE[a.format];
      if (!size) throw new Error(`unsupported vertex format ${JSON.stringify(a.format)}`);
      const offset = a.offset ?? 0, loc = uint(a, 'shaderLocation');
      if (!Number.isInteger(offset) || offset < 0 || offset % Math.min(size, 4) || (stride > 0 && offset + size > stride) || offset + size > MAX_VERTEX_STRIDE) {
        throw new Error(`pipeline: vertex buffer ${i}: attribute at offset ${offset} doesn't fit (arrayStride ${stride})`);
      }
      if (loc >= MAX_VERTEX_ATTRIBUTES || locations.includes(loc)) throw new Error(`pipeline: shaderLocation ${loc} is used twice or is not below ${MAX_VERTEX_ATTRIBUTES}`);
      locations.push(loc);
      last = Math.max(last, offset + size);
    }
    return { stride, instance: step === 'instance', last };
  });
}

function checkBlend(c) {
  if (!c) return;
  if (c.srcFactor !== undefined) oneOf(c.srcFactor, BLEND_FACTORS, 'unsupported blend factor');
  if (c.dstFactor !== undefined) oneOf(c.dstFactor, BLEND_FACTORS, 'unsupported blend factor');
  if (c.operation !== undefined) oneOf(c.operation, BLEND_OPS, 'unsupported blend operation');
}

export class GfxModel {
  constructor(backend) {
    this.backend = backend;
    this.objects = [null];   // handle -> meta (handle 0 is never valid)
    this.pass = null;        // render pass state between beginFrame and endFrame
  }

  add(meta) { this.objects.push(meta); return this.objects.length - 1; }
  meta(h) {
    const m = this.objects[h];
    if (!m) throw new Error(`gfx: invalid handle ${h}`);
    if (m.kind === 'destroyed') throw new Error(`gfx: ${m.was} ${h} was destroyed`);
    return m;
  }
  expect(h, kind) {
    const m = this.meta(h);
    if (m.kind !== kind) throw new Error(`gfx: handle ${h} is a ${m.kind}, not a ${kind}`);
    return m;
  }
  passFor(what) {
    if (!this.pass) throw new Error(`gfx.${what} called outside begin_frame/end_frame`);
    return this.pass;
  }

  destroy(h) {
    const m = this.meta(h);
    this.objects[h] = { kind: 'destroyed', was: m.kind };
    return m;
  }

  // ---- creation (returns [handle, meta]) ------------------------------------------
  shader() { return this.add({ kind: 'shader' }); }

  buffer(size, usage) {
    if (!size || size % 4) throw new Error(`gfx.create_buffer: size ${size} must be a non-zero multiple of 4`);
    return this.add({ kind: 'buffer', size, usage: (usage | BUF_COPY_DST) >>> 0 });
  }

  writeBuffer(h, offset, len) {
    if ((offset | len) & 3) throw new Error(`gfx.write_buffer: offset ${offset} and length ${len} must be multiples of 4`);
    const b = this.expect(h, 'buffer');
    if (offset + len > b.size) throw new Error(`gfx.write_buffer: ${offset}+${len} exceeds buffer size ${b.size}`);
  }

  pipelineLayouts(d) {
    if (d.layout === undefined || d.layout === 'auto') return null;
    if (!Array.isArray(d.layout)) throw new Error(`pipeline: layout must be "auto" or an array of bind group layout handles, got ${JSON.stringify(d.layout)}`);
    if (d.layout.length > MAX_BIND_GROUPS) throw new Error(`pipeline: at most ${MAX_BIND_GROUPS} bind group layouts, got ${d.layout.length}`);
    return d.layout.map((h) => {
      if (!isU32(h)) throw new Error('pipeline: layout entries must be bind group layout handles');
      return this.expect(h, 'bind group layout').entries;
    });
  }

  /** Validate a pipeline descriptor (same checks as gfx.rs build_pipeline, without a GPU). */
  pipeline(d) {
    const groups = this.pipelineLayouts(d);
    const v = d.vertex;
    if (!v) throw new Error('pipeline: missing vertex');
    this.expect(uint(v, 'module'), 'shader');
    const vertex = vertexSlots(v);
    let targets = 0;
    if (d.fragment) {
      this.expect(uint(d.fragment, 'module'), 'shader');
      for (const t of d.fragment.targets ?? []) {
        if (t.format !== undefined && t.format !== 'surface') throw new Error(`pipeline: color target format must be "surface", got ${JSON.stringify(t.format)}`);
        if (t.blend) { checkBlend(t.blend.color); checkBlend(t.blend.alpha); }
        if (t.writeMask !== undefined && !(Number.isInteger(t.writeMask) && t.writeMask >= 0 && t.writeMask <= 15)) throw new Error(`pipeline: writeMask ${t.writeMask} must be 0-15`);
        targets++;
      }
    }
    const p = d.primitive ?? {};
    oneOf(p.topology ?? 'triangle-list', TOPOLOGIES, 'pipeline: unknown topology');
    if (p.stripIndexFormat !== undefined) oneOf(p.stripIndexFormat, ['uint16', 'uint32'], 'pipeline: unknown stripIndexFormat');
    oneOf(p.frontFace ?? 'ccw', ['ccw', 'cw'], 'pipeline: unknown frontFace');
    oneOf(p.cullMode ?? 'none', ['none', 'front', 'back'], 'pipeline: unknown cullMode');
    const ds = d.depthStencil;
    if (ds) {
      if (ds.format !== undefined && ds.format !== 'depth24plus') throw new Error(`pipeline: depthStencil format must be "depth24plus", got ${JSON.stringify(ds.format)}`);
      oneOf(ds.depthCompare ?? 'always', COMPARE, 'unsupported compare function');
    }
    return this.add({ kind: 'pipeline', groups, vertex, targets });
  }

  layout(d) {
    const entries = [];
    for (const e of d.entries ?? []) {
      const b = uint(e, 'binding');
      if (entries.some((x) => x.binding === b)) throw new Error(`bind group layout: binding ${b} appears twice`);
      const v = uint(e, 'visibility');
      if (v === 0 || (v & ~3)) throw new Error(`bind group layout: binding ${b}: visibility ${v} must be GASM_STAGE_VERTEX (1) and/or GASM_STAGE_FRAGMENT (2)`);
      const base = { binding: b, visibility: v, dynamic: false, minBindingSize: 0, filterable: false };
      if (e.buffer) {
        const t = e.buffer.type ?? 'uniform';
        if (t !== 'uniform' && t !== 'read-only-storage') throw new Error(`bind group layout: binding ${b}: unsupported buffer type ${JSON.stringify(t)}`);
        entries.push({ ...base, slot: t === 'uniform' ? 'uniform' : 'storage', dynamic: !!e.buffer.hasDynamicOffset, minBindingSize: e.buffer.minBindingSize ?? 0 });
      } else if (e.texture) {
        const st = e.texture.sampleType ?? 'float';
        if (st !== 'float' && st !== 'unfilterable-float') throw new Error(`bind group layout: binding ${b}: unsupported sampleType ${JSON.stringify(st)}`);
        if ((e.texture.viewDimension ?? '2d') !== '2d') throw new Error(`bind group layout: binding ${b}: only viewDimension "2d" is supported, got ${JSON.stringify(e.texture.viewDimension)}`);
        if (e.texture.multisampled) throw new Error(`bind group layout: binding ${b}: multisampled textures are not supported`);
        entries.push({ ...base, slot: 'texture', filterable: st === 'float' });
      } else if (e.sampler) {
        const st = e.sampler.type ?? 'filtering';
        if (st !== 'filtering' && st !== 'non-filtering') throw new Error(`bind group layout: binding ${b}: unsupported sampler type ${JSON.stringify(st)}`);
        entries.push({ ...base, slot: 'sampler', filterable: st === 'filtering' });
      } else {
        throw new Error(`bind group layout: binding ${b} needs "buffer", "texture" or "sampler"`);
      }
    }
    entries.sort((a, b) => a.binding - b.binding);
    return this.add({ kind: 'bind group layout', entries });
  }

  bindGroup(d) {
    const resolved = [];
    for (const e of d.entries ?? []) {
      const b = uint(e, 'binding');
      if (resolved.some((r) => r.binding === b)) throw new Error(`bind group: binding ${b} appears twice`);
      if (e.buffer !== undefined) {
        const h = handle(e.buffer), buf = this.expect(h, 'buffer'), offset = e.offset ?? 0;
        if (offset > buf.size || offset + (e.size ?? 0) > buf.size || e.size === 0) throw new Error(`bind group: binding ${b}: range ${offset}+${e.size ?? 0} exceeds buffer size ${buf.size}`);
        resolved.push({ binding: b, res: 'buffer', h, buf, offset, size: e.size ?? buf.size - offset });
      } else if (e.texture !== undefined) {
        const h = handle(e.texture); this.expect(h, 'texture'); resolved.push({ binding: b, res: 'texture', h });
      } else if (e.sampler !== undefined) {
        const h = handle(e.sampler); this.expect(h, 'sampler'); resolved.push({ binding: b, res: 'sampler', h });
      } else {
        throw new Error(`bind group: binding ${b} needs a "buffer", "texture" or "sampler"`);
      }
    }
    // the layout's entries: an explicit layout, or group G of a pipeline with explicit layouts
    let fromPipeline = null;
    if (d.layout === undefined) {
      if (d.pipeline === undefined) throw new Error('bind group: needs "layout" or "pipeline"');
      const group = d.group ?? 0;
      if (!Number.isInteger(group) || group < 0 || group >= MAX_BIND_GROUPS) throw new Error(`bind group: group ${group} must be below ${MAX_BIND_GROUPS}`);
      fromPipeline = { pipeline: uint(d, 'pipeline'), group };
    }
    let entries = null;
    if (d.layout !== undefined) entries = this.expect(handle(d.layout), 'bind group layout').entries;
    else {
      const p = this.expect(fromPipeline.pipeline, 'pipeline');
      if (p.groups) {
        entries = p.groups[fromPipeline.group];
        if (!entries) throw new Error(`bind group: pipeline ${fromPipeline.pipeline} has no group ${fromPipeline.group}`);
      }
    }
    const dynamic = [];
    let layout;
    if (entries) {
      const lh = d.layout ?? 'of the pipeline';
      if (resolved.length !== entries.length) throw new Error(`bind group: layout ${lh} has ${entries.length} entries, got ${resolved.length}`);
      for (const le of entries) {
        const r = resolved.find((x) => x.binding === le.binding);
        if (!r) throw new Error(`bind group: missing binding ${le.binding}`);
        const want = le.slot === 'uniform' || le.slot === 'storage' ? 'buffer' : le.slot;
        if (r.res !== want) throw new Error(`bind group: binding ${le.binding} has the wrong resource kind for its layout`);
        if (want === 'buffer') {
          const need = le.slot === 'uniform' ? BUF_UNIFORM : BUF_STORAGE;
          if (!(r.buf.usage & need)) throw new Error(`bind group: binding ${le.binding}: buffer ${r.h} lacks ${need === BUF_UNIFORM ? 'UNIFORM' : 'STORAGE'} usage`);
          if (r.size < le.minBindingSize) throw new Error(`bind group: binding ${le.binding}: size ${r.size} is below minBindingSize ${le.minBindingSize}`);
          if (le.dynamic) dynamic.push({ bufferSize: r.buf.size, offset: r.offset, size: r.size });
        }
      }
      layout = { explicit: entries };
    } else {
      layout = { auto: fromPipeline };
    }
    const resources = resolved.filter((r) => r.res !== 'sampler').map((r) => r.h);
    return this.add({ kind: 'bind group', dynamic, layout, resources });
  }

  texture(d) {
    const [w, h] = Array.isArray(d.size) ? d.size : [];
    const ok = (v) => Number.isInteger(v) && v >= 1 && v <= MAX_TEXTURE_SIZE;
    if (!Array.isArray(d.size) || d.size.length !== 2 || !ok(w) || !ok(h)) throw new Error(`texture: size must be [width, height], each 1-${MAX_TEXTURE_SIZE}`);
    const format = d.format ?? 'rgba8unorm';
    if (format !== 'rgba8unorm' && format !== 'rgba8unorm-srgb') throw new Error(`texture: unsupported format ${JSON.stringify(format)} (rgba8unorm, rgba8unorm-srgb)`);
    const maxMips = 32 - Math.clz32(Math.max(w, h)), mips = d.mipLevelCount ?? 1;
    if (!Number.isInteger(mips) || mips < 1 || mips > maxMips) throw new Error(`texture: mipLevelCount ${mips} must be 1-${maxMips} for ${w}x${h}`);
    return this.add({ kind: 'texture', width: w, height: h, mips, format });
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
    return this.add({ kind: 'sampler' });
  }

  // ---- the pass -------------------------------------------------------------------
  beginFrame() {
    if (this.pass) throw new Error('gfx.begin_frame called twice without end_frame');
    this.pass = { pipeline: null, groups: new Array(MAX_BIND_GROUPS).fill(null), vertex: new Array(MAX_VERTEX_BUFFERS).fill(null), index: null };
  }
  endFrame() { this.pass = null; }

  setPipeline(h) {
    const { targets } = this.expect(h, 'pipeline');
    if (targets !== 1) throw new Error(`gfx.set_pipeline: pipeline ${h} has ${targets} color targets; the render pass has one ("surface")`);
    this.passFor('set_pipeline').pipeline = h;
  }

  setBindGroup(index, h, offsets) {
    if (index >= MAX_BIND_GROUPS) throw new Error(`gfx.set_bind_group: index ${index} must be below ${MAX_BIND_GROUPS}`);
    const { dynamic } = this.expect(h, 'bind group');
    if (offsets.length !== dynamic.length) throw new Error(`gfx.set_bind_group_offsets: bind group ${h} has ${dynamic.length} dynamic entries, got ${offsets.length} offsets`);
    offsets.forEach((o, i) => {
      const e = dynamic[i];
      if (o % OFFSET_ALIGNMENT) throw new Error(`gfx.set_bind_group_offsets: offset ${o} is not a multiple of ${OFFSET_ALIGNMENT}`);
      if (o + e.offset + e.size > e.bufferSize) throw new Error(`gfx.set_bind_group_offsets: offset ${o} + binding ${e.offset}+${e.size} exceeds buffer size ${e.bufferSize}`);
    });
    this.passFor('set_bind_group').groups[index] = h;
  }

  setViewport() { this.passFor('set_viewport'); }
  setScissorRect() { this.passFor('set_scissor_rect'); }

  setVertexBuffer(slot, h, offset) {
    if (slot >= MAX_VERTEX_BUFFERS) throw new Error(`gfx.set_vertex_buffer: slot ${slot} must be below ${MAX_VERTEX_BUFFERS}`);
    const b = this.expect(h, 'buffer');
    if (!(b.usage & BUF_VERTEX)) throw new Error(`gfx.set_vertex_buffer: buffer ${h} lacks VERTEX usage`);
    if (offset % 4 || offset > b.size) throw new Error(`gfx.set_vertex_buffer: offset ${offset} must be a multiple of 4 within the buffer (${b.size} bytes)`);
    this.passFor('set_vertex_buffer').vertex[slot] = [h, offset];
  }

  setIndexBuffer(h, format, offset) {
    if (format !== 0 && format !== 1) throw new Error(`gfx.set_index_buffer: format ${format} must be GASM_INDEX_U16 (0) or GASM_INDEX_U32 (1)`);
    const size = format === 1 ? 4 : 2, b = this.expect(h, 'buffer');
    if (!(b.usage & BUF_INDEX)) throw new Error(`gfx.set_index_buffer: buffer ${h} lacks INDEX usage`);
    if (offset % size || offset > b.size) throw new Error(`gfx.set_index_buffer: offset ${offset} must be a multiple of ${size} within the buffer (${b.size} bytes)`);
    this.passFor('set_index_buffer').index = [h, size, offset];
  }

  /** Validate a draw (same rules as gfx.rs check_draw). `vertices`: first + count, null for indexed draws. */
  checkDraw(what, vertices, instances) {
    const pass = this.passFor(what);
    if (pass.pipeline === null) throw new Error(`gfx.${what}: no pipeline set`);
    const ph = pass.pipeline, pm = this.expect(ph, 'pipeline');
    if (pm.groups) {
      pm.groups.forEach((entries, i) => {
        const bg = pass.groups[i];
        if (bg === null) throw new Error(`gfx.${what}: pipeline ${ph} needs a bind group at index ${i}`);
        const { layout } = this.expect(bg, 'bind group');
        if (!layout.explicit || !sameEntries(layout.explicit, entries)) throw new Error(`gfx.${what}: bind group ${bg} at index ${i} doesn't match pipeline ${ph}'s layout`);
      });
    } else {
      pass.groups.forEach((bg, i) => {
        if (bg === null) return;
        const { layout } = this.expect(bg, 'bind group');
        if (layout.auto && layout.auto.group === i && layout.auto.pipeline !== ph) {
          throw new Error(`gfx.${what}: bind group ${bg} was made for pipeline ${layout.auto.pipeline} ("auto" layouts are per pipeline), not ${ph}`);
        }
      });
    }
    pm.vertex.forEach((vs, slot) => {
      const vb = pass.vertex[slot];
      if (!vb) throw new Error(`gfx.${what}: pipeline ${ph} needs a vertex buffer in slot ${slot}`);
      const [b, offset] = vb, { size } = this.expect(b, 'buffer');
      const n = vs.instance ? instances : vertices;
      if (n) {
        const need = (n - 1) * vs.stride + vs.last;
        if (need > size - offset) throw new Error(`gfx.${what}: vertex buffer ${b} in slot ${slot} has ${size - offset} bytes after its offset, the draw reads ${need}`);
      }
    });
  }

  draw(vc, ic, fv, fi) { this.checkDraw('draw', fv + vc, fi + ic); }

  drawIndexed(ic, inst, first, _base, fi) {
    this.checkDraw('draw_indexed', null, fi + inst);
    const idx = this.pass.index;
    if (!idx) throw new Error('gfx.draw_indexed: no index buffer set');
    const [b, isize, offset] = idx, { size } = this.expect(b, 'buffer');
    const need = (first + ic) * isize;
    if (need > size - offset) throw new Error(`gfx.draw_indexed: index buffer ${b} has ${size - offset} bytes after its offset, the draw reads ${need}`);
  }
}

// gfx backend that draws nothing: headless runs and tests. GfxModel validates and
// numbers everything; writes are hashed by GasmHost.
export class NullGfx {
  constructor(width = 1280, height = 720) { this.w = width; this.h = height; this.used = false; }
  width() { return this.w; }
  height() { return this.h; }
  beginFrame() { return false; }
}
