// gasm:gfx on the browser's WebGPU (navigator.gpu). Mirrors runners/native/src/gfx.rs:
// same handle table, JSON descriptors, runner-owned MSAA (4x) and depth24plus.

export const SAMPLE_COUNT = 4;

export class WebGpuGfx {
  static async create(canvas, log = console.log) {
    if (!navigator.gpu) throw new Error('WebGPU is not available in this browser');
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
    this.resize();
  }

  // Match the drawing buffer to the canvas' CSS size (in device pixels).
  resize() {
    const dpr = globalThis.devicePixelRatio || 1;
    const w = Math.max(1, Math.round(this.canvas.clientWidth * dpr)) || this.canvas.width;
    const h = Math.max(1, Math.round(this.canvas.clientHeight * dpr)) || this.canvas.height;
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
    const desc = {
      layout: 'auto',
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
    const pipeline = this.get(d.pipeline, 'pipeline');
    const bg = this.device.createBindGroup({
      layout: pipeline.getBindGroupLayout(d.group ?? 0),
      entries: (d.entries ?? []).map((e) => ({
        binding: e.binding,
        resource: { buffer: this.get(e.buffer, 'buffer'), offset: e.offset ?? 0, size: e.size },
      })),
    });
    return this.add({ kind: 'bindgroup', value: bg });
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
    return true;
  }

  setPipeline(h) { const p = this.get(h, 'pipeline'); this.pass?.setPipeline(p); }
  setBindGroup(i, h) { const bg = this.get(h, 'bindgroup'); this.pass?.setBindGroup(i, bg); }
  setVertexBuffer(slot, h, off) { const b = this.get(h, 'buffer'); this.pass?.setVertexBuffer(slot, b, off); }
  setIndexBuffer(h, fmt, off) { const b = this.get(h, 'buffer'); this.pass?.setIndexBuffer(b, fmt === 1 ? 'uint32' : 'uint16', off); }
  draw(vc, ic, fv, fi) { this.pass?.draw(vc, ic, fv, fi); }
  drawIndexed(ic, n, first, base, fi) { this.pass?.drawIndexed(ic, n, first, base, fi); }

  endFrame() {
    if (!this.pass) return;
    this.pass.end();
    this.device.queue.submit([this.encoder.finish()]);
    this.pass = null;
  }
}
