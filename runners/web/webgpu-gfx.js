// gasm:gfx on the browser's WebGPU (navigator.gpu). Mirrors runners/native/src/gfx.rs:
// same handle table, JSON descriptors, runner-owned MSAA (4x) and depth24plus.
// Descriptors arrive already validated by GfxModel (gasm-host.js). The canvas may be
// an HTMLCanvasElement (main thread) or an OffscreenCanvas (Worker mode): offscreen,
// the page reports the display size with setSize().

export const SAMPLE_COUNT = 4;

export class WebGpuGfx {
  static async create(canvas, log = console.log) {
    if (!globalThis.navigator?.gpu) throw new Error('WebGPU is not available in this browser' + (typeof document === 'undefined' ? ' (worker)' : ''));
    const adapter = await navigator.gpu.requestAdapter({ powerPreference: 'high-performance' });
    if (!adapter) throw new Error('no WebGPU adapter');
    const device = await adapter.requestDevice();
    device.addEventListener('uncapturederror', (e) => log(`[gasm] gpu error: ${e.error.message}`));
    return new WebGpuGfx(canvas, device, log);
  }

  constructor(canvas, device, log) {
    this.canvas = canvas;
    this.device = device;
    this.log = log;
    this.format = navigator.gpu.getPreferredCanvasFormat();
    this.context = canvas.getContext('webgpu');
    this.context.configure({ device, format: this.format, alphaMode: 'opaque' });
    this.objects = [null]; // handle 0 is never valid
    this.pass = null;
    this.w = 0; this.h = 0;
    this.sizeHint = null;
    this.resize();
  }

  /** OffscreenCanvas: the display size in device pixels, sent by the page. */
  setSize(w, h) { this.sizeHint = [Math.max(1, w | 0), Math.max(1, h | 0)]; }

  // Match the drawing buffer to the canvas' CSS size (in device pixels).
  resize() {
    let w, h;
    if (this.sizeHint) [w, h] = this.sizeHint;
    else if ('clientWidth' in this.canvas) {
      const dpr = globalThis.devicePixelRatio || 1;
      w = Math.max(1, Math.round(this.canvas.clientWidth * dpr)) || this.canvas.width;
      h = Math.max(1, Math.round(this.canvas.clientHeight * dpr)) || this.canvas.height;
    } else { w = this.canvas.width; h = this.canvas.height; }
    if (w === this.w && h === this.h) return;
    this.w = this.canvas.width = w;
    this.h = this.canvas.height = h;
    this.msaa?.destroy(); this.depth?.destroy();
    const tex = (format) => this.device.createTexture({
      size: [w, h], sampleCount: SAMPLE_COUNT, format, usage: GPUTextureUsage.RENDER_ATTACHMENT,
    });
    this.msaa = tex(this.format);
    this.depth = tex('depth24plus');
  }

  add(obj) { this.objects.push(obj); return this.objects.length - 1; }
  get(h, kind) {
    const o = this.objects[h];
    if (!o || (kind && o.kind !== kind)) throw new Error(`gfx: handle ${h} is not a ${kind ?? 'valid object'}`);
    return o.value;
  }

  width() { return this.w; }
  height() { return this.h; }

  createShader(code) {
    return this.add({ kind: 'shader', value: this.device.createShaderModule({ code }) });
  }

  createBuffer(size, usage) {
    if (!size || size % 4) throw new Error(`gfx.create_buffer: size ${size} must be a non-zero multiple of 4`);
    const buf = this.device.createBuffer({ size, usage: usage | GPUBufferUsage.COPY_DST });
    return this.add({ kind: 'buffer', value: buf });
  }

  writeBuffer(h, offset, bytes) {
    this.device.queue.writeBuffer(this.get(h, 'buffer'), offset, bytes.slice());
  }

  createPipeline(d) {
    const surface = (targets = []) => targets.map((t) => {
      if (t.format && t.format !== 'surface') throw new Error(`pipeline: color target format must be "surface", got ${t.format}`);
      return { ...t, format: this.format };
    });
    if (d.depthStencil?.format && d.depthStencil.format !== 'depth24plus') {
      throw new Error(`pipeline: depthStencil format must be "depth24plus", got ${d.depthStencil.format}`);
    }
    const layout = Array.isArray(d.layout)
      ? this.device.createPipelineLayout({ bindGroupLayouts: d.layout.map((h) => this.get(h, 'layout')) })
      : 'auto';
    const desc = {
      layout,
      vertex: { ...d.vertex, module: this.get(d.vertex.module, 'shader') },
      fragment: d.fragment && { ...d.fragment, module: this.get(d.fragment.module, 'shader'), targets: surface(d.fragment.targets) },
      primitive: d.primitive,
      // the pass always has a depth attachment: pipelines without depthStencil get a no-op one
      depthStencil: { depthWriteEnabled: false, depthCompare: 'always', ...d.depthStencil, format: 'depth24plus' },
      multisample: { count: SAMPLE_COUNT },
    };
    return this.add({ kind: 'pipeline', value: this.device.createRenderPipeline(desc) });
  }

  createBindGroup(d) {
    const layout = d.layout !== undefined ? this.get(d.layout, 'layout') : this.get(d.pipeline, 'pipeline').getBindGroupLayout(d.group ?? 0);
    const resource = (e) => {
      if (e.texture !== undefined) return this.objects[e.texture].view;
      if (e.sampler !== undefined) return this.get(e.sampler, 'sampler');
      return { buffer: this.get(e.buffer, 'buffer'), offset: e.offset ?? 0, size: e.size };
    };
    const bg = this.device.createBindGroup({
      layout, entries: (d.entries ?? []).map((e) => ({ binding: e.binding, resource: resource(e) })),
    });
    return this.add({ kind: 'bindgroup', value: bg });
  }

  createBindGroupLayout(d) {
    const entries = (d.entries ?? []).map((e) => {
      const out = { binding: e.binding, visibility: e.visibility };
      if (e.buffer) out.buffer = { type: e.buffer.type ?? 'uniform', hasDynamicOffset: !!e.buffer.hasDynamicOffset, minBindingSize: e.buffer.minBindingSize ?? 0 };
      else if (e.texture) out.texture = { sampleType: e.texture.sampleType ?? 'float', viewDimension: '2d' };
      else out.sampler = { type: e.sampler.type ?? 'filtering' };
      return out;
    });
    return this.add({ kind: 'layout', value: this.device.createBindGroupLayout({ entries }) });
  }

  createTexture(d, meta) {
    const texture = this.device.createTexture({
      size: [meta.width, meta.height], format: meta.format, mipLevelCount: meta.mips,
      usage: GPUTextureUsage.TEXTURE_BINDING | GPUTextureUsage.COPY_DST,
    });
    return this.add({ kind: 'texture', value: texture, view: texture.createView() });
  }

  writeTexture(h, mip, x, y, w, ht, bytes) {
    this.device.queue.writeTexture({ texture: this.get(h, 'texture'), mipLevel: mip, origin: [x, y] },
      bytes.slice(), { bytesPerRow: w * 4, rowsPerImage: ht }, [w, ht]);
  }

  createSampler(d) {
    const keys = ['addressModeU', 'addressModeV', 'magFilter', 'minFilter', 'mipmapFilter', 'lodMinClamp', 'lodMaxClamp', 'maxAnisotropy'];
    const desc = Object.fromEntries(keys.filter((k) => d[k] !== undefined).map((k) => [k, d[k]]));
    return this.add({ kind: 'sampler', value: this.device.createSampler(desc) });
  }

  beginFrame(r, g, b, a, show) {
    if (this.pass) throw new Error('gfx.begin_frame called twice without end_frame');
    this.used = true;
    if (!show) return false;
    this.resize();
    this.encoder = this.device.createCommandEncoder();
    this.pass = this.encoder.beginRenderPass({
      colorAttachments: [{
        view: this.msaa.createView(),
        resolveTarget: this.context.getCurrentTexture().createView(),
        clearValue: { r, g, b, a }, loadOp: 'clear', storeOp: 'discard',
      }],
      depthStencilAttachment: {
        view: this.depth.createView(), depthClearValue: 1, depthLoadOp: 'clear', depthStoreOp: 'discard',
      },
    });
    this.emptyViewport = this.emptyScissor = false;
    return true;
  }

  setPipeline(h) { const p = this.get(h, 'pipeline'); this.pass?.setPipeline(p); }
  setBindGroup(i, h) { const bg = this.get(h, 'bindgroup'); this.pass?.setBindGroup(i, bg); }
  setBindGroupOffsets(i, h, offsets) { const bg = this.get(h, 'bindgroup'); this.pass?.setBindGroup(i, bg, offsets); }
  // Rectangles arrive clamped to the drawable; an empty one skips draws (as natively).
  setViewport(x, y, w, h, min, max) {
    if (!this.pass) return;
    this.emptyViewport = w <= 0 || h <= 0;
    if (!this.emptyViewport) this.pass.setViewport(x, y, w, h, min, max);
  }
  setScissorRect(x, y, w, h) {
    if (!this.pass) return;
    this.emptyScissor = w <= 0 || h <= 0;
    if (!this.emptyScissor) this.pass.setScissorRect(x, y, w, h);
  }
  setVertexBuffer(slot, h, off) { const b = this.get(h, 'buffer'); this.pass?.setVertexBuffer(slot, b, off); }
  setIndexBuffer(h, fmt, off) { const b = this.get(h, 'buffer'); this.pass?.setIndexBuffer(b, fmt === 1 ? 'uint32' : 'uint16', off); }
  get clipped() { return this.emptyViewport || this.emptyScissor; }
  draw(vc, ic, fv, fi) { if (!this.clipped) this.pass?.draw(vc, ic, fv, fi); }
  drawIndexed(ic, n, first, base, fi) { if (!this.clipped) this.pass?.drawIndexed(ic, n, first, base, fi); }

  endFrame() {
    if (!this.pass) return;
    this.pass.end();
    this.device.queue.submit([this.encoder.finish()]);
    this.pass = null;
  }

  /**
   * Show a video_present frame on this (WebGPU) canvas: letterboxed nearest-neighbour
   * blit, like the native runner. For frames where the guest didn't use gfx; needed
   * when the canvas has a WebGPU context (gfx guests, Worker mode).
   */
  presentVideo(rgba, w, h) {
    this.resize();
    const dev = this.device;
    if (!this.blit) {
      const module = dev.createShaderModule({ code: `
@group(0) @binding(0) var t: texture_2d<f32>;
@group(0) @binding(1) var s: sampler;
struct VO { @builtin(position) pos: vec4<f32>, @location(0) uv: vec2<f32> };
@vertex fn vs(@builtin(vertex_index) i: u32) -> VO {
  let uv = vec2<f32>(f32((i << 1u) & 2u), f32(i & 2u));
  var o: VO; o.pos = vec4<f32>(uv.x * 2.0 - 1.0, 1.0 - uv.y * 2.0, 0.0, 1.0); o.uv = uv; return o;
}
@fragment fn fs(i: VO) -> @location(0) vec4<f32> { return textureSample(t, s, i.uv); }` });
      const pipeline = dev.createRenderPipeline({
        layout: 'auto', vertex: { module, entryPoint: 'vs' },
        fragment: { module, entryPoint: 'fs', targets: [{ format: this.format }] },
      });
      this.blit = { pipeline, sampler: dev.createSampler(), tex: null, bg: null, w: 0, h: 0 };
    }
    const b = this.blit;
    if (b.w !== w || b.h !== h) {
      b.tex?.destroy();
      b.tex = dev.createTexture({ size: [w, h], format: 'rgba8unorm', usage: GPUTextureUsage.TEXTURE_BINDING | GPUTextureUsage.COPY_DST });
      b.bg = dev.createBindGroup({ layout: b.pipeline.getBindGroupLayout(0), entries: [
        { binding: 0, resource: b.tex.createView() }, { binding: 1, resource: b.sampler }] });
      b.w = w; b.h = h;
    }
    dev.queue.writeTexture({ texture: b.tex }, rgba, { bytesPerRow: w * 4 }, [w, h]);
    const enc = dev.createCommandEncoder();
    const pass = enc.beginRenderPass({ colorAttachments: [{
      view: this.context.getCurrentTexture().createView(), clearValue: { r: 0, g: 0, b: 0, a: 1 }, loadOp: 'clear', storeOp: 'store',
    }] });
    const scale = Math.min(this.w / w, this.h / h), vw = w * scale, vh = h * scale;
    pass.setViewport((this.w - vw) / 2, (this.h - vh) / 2, vw, vh, 0, 1);
    pass.setPipeline(b.pipeline);
    pass.setBindGroup(0, b.bg);
    pass.draw(3);
    pass.end();
    dev.queue.submit([enc.finish()]);
  }
}
