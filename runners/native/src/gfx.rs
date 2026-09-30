//! gasm:gfx on wgpu, plus the 2D `video_present` blit.
//!
//! Handles index a single object table. Creation descriptors are JSON that
//! mirrors WebGPU (see spec/ABI.md#gasmgfx). Validation errors from wgpu are
//! caught with error scopes and returned as `Err`, which traps the guest.
//!
//! Without a GPU (`Gfx::null`) every call still validates handles and
//! allocates ids, so guests behave identically in headless runs.

use std::sync::Arc;

use serde_json::Value;
use wgpu::util::DeviceExt;

pub const SAMPLE_COUNT: u32 = 4;
const DEPTH_FORMAT: wgpu::TextureFormat = wgpu::TextureFormat::Depth24Plus;

enum Obj {
    Shader(wgpu::ShaderModule),
    Buffer(wgpu::Buffer),
    Pipeline(wgpu::RenderPipeline),
    BindGroup(wgpu::BindGroup),
    Null,
}

pub struct Gpu {
    pub device: wgpu::Device,
    pub queue: wgpu::Queue,
    pub format: wgpu::TextureFormat,
}

pub enum Target {
    Window { surface: wgpu::Surface<'static>, config: wgpu::SurfaceConfiguration },
    Offscreen { texture: wgpu::Texture },
}

struct Frame {
    pass: wgpu::RenderPass<'static>,
    encoder: wgpu::CommandEncoder,
    surface_texture: Option<wgpu::SurfaceTexture>,
}

struct Blit {
    pipeline: wgpu::RenderPipeline,
    sampler: wgpu::Sampler,
    texture: Option<(wgpu::Texture, wgpu::BindGroup, u32, u32)>,
}

pub struct Gfx {
    gpu: Option<Gpu>,
    target: Option<Target>,
    width: u32,
    height: u32,
    objects: Vec<Obj>,
    msaa: Option<wgpu::TextureView>,
    depth: Option<wgpu::TextureView>,
    frame: Option<Frame>,
    blit: Option<Blit>,
    /// set by the guest calling any gfx draw API this frame (suppresses the 2D blit)
    pub used: bool,
}

type R<T> = Result<T, String>;

impl Gfx {
    /// No GPU: ids only (headless runs without screenshots, CI).
    pub fn null() -> Gfx {
        Gfx { gpu: None, target: None, width: 1280, height: 720, objects: Vec::new(), msaa: None, depth: None, frame: None, blit: None, used: false }
    }

    /// Render into a window.
    pub fn for_window(window: Arc<winit::window::Window>) -> R<Gfx> {
        let size = window.inner_size();
        let instance = wgpu::Instance::new(wgpu::InstanceDescriptor::new_without_display_handle());
        let surface = instance.create_surface(window).map_err(|e| e.to_string())?;
        let adapter = pollster::block_on(instance.request_adapter(&wgpu::RequestAdapterOptions {
            power_preference: wgpu::PowerPreference::HighPerformance,
            compatible_surface: Some(&surface),
            ..Default::default()
        }))
        .map_err(|e| format!("no GPU adapter: {e}"))?;
        let gpu = open_device(&adapter, |caps_formats| pick_format(caps_formats), &surface)?;
        let mut config = surface
            .get_default_config(&adapter, size.width.max(1), size.height.max(1))
            .ok_or("surface not supported by adapter")?;
        config.format = gpu.format;
        config.present_mode = wgpu::PresentMode::AutoVsync;
        surface.configure(&gpu.device, &config);
        eprintln!("[gasm] gpu: {} ({:?}), surface {:?}", adapter.get_info().name, adapter.get_info().backend, gpu.format);
        let mut g = Gfx::with_gpu(gpu, Target::Window { surface, config }, size.width.max(1), size.height.max(1));
        g.recreate_attachments();
        Ok(g)
    }

    /// Render into an RGBA texture that can be read back (headless screenshots).
    pub fn offscreen(width: u32, height: u32) -> R<Gfx> {
        let instance = wgpu::Instance::new(wgpu::InstanceDescriptor::new_without_display_handle());
        let adapter = pollster::block_on(instance.request_adapter(&wgpu::RequestAdapterOptions::default()))
            .map_err(|e| format!("no GPU adapter: {e}"))?;
        let (device, queue) = pollster::block_on(adapter.request_device(&wgpu::DeviceDescriptor::default()))
            .map_err(|e| e.to_string())?;
        device.on_uncaptured_error(Arc::new(|e| eprintln!("[gasm] gpu error: {e}")));
        let format = wgpu::TextureFormat::Rgba8Unorm;
        let texture = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("offscreen"),
            size: wgpu::Extent3d { width, height, depth_or_array_layers: 1 },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format,
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::COPY_SRC,
            view_formats: &[],
        });
        let mut g = Gfx::with_gpu(Gpu { device, queue, format }, Target::Offscreen { texture }, width, height);
        g.recreate_attachments();
        Ok(g)
    }

    fn with_gpu(gpu: Gpu, target: Target, width: u32, height: u32) -> Gfx {
        let blit = Some(Blit::new(&gpu));
        Gfx { gpu: Some(gpu), target: Some(target), width, height, objects: Vec::new(), msaa: None, depth: None, frame: None, blit, used: false }
    }

    pub fn size(&self) -> (u32, u32) {
        (self.width, self.height)
    }

    pub fn resize(&mut self, width: u32, height: u32) {
        if width == 0 || height == 0 || (width, height) == (self.width, self.height) {
            return;
        }
        self.width = width;
        self.height = height;
        if let (Some(gpu), Some(Target::Window { surface, config })) = (&self.gpu, &mut self.target) {
            config.width = width;
            config.height = height;
            surface.configure(&gpu.device, config);
        }
        self.recreate_attachments();
    }

    fn recreate_attachments(&mut self) {
        let Some(gpu) = &self.gpu else { return };
        let make = |format, label| {
            gpu.device
                .create_texture(&wgpu::TextureDescriptor {
                    label: Some(label),
                    size: wgpu::Extent3d { width: self.width, height: self.height, depth_or_array_layers: 1 },
                    mip_level_count: 1,
                    sample_count: SAMPLE_COUNT,
                    dimension: wgpu::TextureDimension::D2,
                    format,
                    usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
                    view_formats: &[],
                })
                .create_view(&Default::default())
        };
        self.msaa = Some(make(gpu.format, "msaa"));
        self.depth = Some(make(DEPTH_FORMAT, "depth"));
    }

    // ---- object table ------------------------------------------------------------

    fn add(&mut self, obj: Obj) -> u32 {
        self.objects.push(obj);
        self.objects.len() as u32
    }

    fn get(&self, h: u32) -> R<&Obj> {
        self.objects.get((h as usize).wrapping_sub(1)).ok_or_else(|| format!("gfx: invalid handle {h}"))
    }

    fn shader(&self, h: u32) -> R<&wgpu::ShaderModule> {
        match self.get(h)? { Obj::Shader(s) => Ok(s), _ => Err(format!("gfx: handle {h} is not a shader")) }
    }
    fn buffer(&self, h: u32) -> R<&wgpu::Buffer> {
        match self.get(h)? { Obj::Buffer(b) => Ok(b), _ => Err(format!("gfx: handle {h} is not a buffer")) }
    }
    fn pipeline(&self, h: u32) -> R<&wgpu::RenderPipeline> {
        match self.get(h)? { Obj::Pipeline(p) => Ok(p), _ => Err(format!("gfx: handle {h} is not a pipeline")) }
    }
    fn bind_group(&self, h: u32) -> R<&wgpu::BindGroup> {
        match self.get(h)? { Obj::BindGroup(b) => Ok(b), _ => Err(format!("gfx: handle {h} is not a bind group")) }
    }

    /// Run `f` inside a validation error scope; wgpu errors become `Err`.
    fn scoped<T>(&self, what: &str, f: impl FnOnce(&Gpu) -> R<T>) -> R<T> {
        let gpu = self.gpu.as_ref().unwrap();
        let scope = gpu.device.push_error_scope(wgpu::ErrorFilter::Validation);
        let out = f(gpu);
        if let Some(e) = pollster::block_on(scope.pop()) {
            return Err(format!("gfx: {what}: {e}"));
        }
        out
    }

    // ---- creation ------------------------------------------------------------------------

    pub fn create_shader(&mut self, wgsl: &str) -> R<u32> {
        if self.gpu.is_none() {
            return Ok(self.add(Obj::Null));
        }
        let m = self.scoped("create_shader", |gpu| {
            Ok(gpu.device.create_shader_module(wgpu::ShaderModuleDescriptor {
                label: None,
                source: wgpu::ShaderSource::Wgsl(wgsl.into()),
            }))
        })?;
        Ok(self.add(Obj::Shader(m)))
    }

    pub fn create_buffer(&mut self, size: u32, usage: u32) -> R<u32> {
        if size == 0 || size % 4 != 0 {
            return Err(format!("gfx.create_buffer: size {size} must be a non-zero multiple of 4"));
        }
        let Some(gpu) = &self.gpu else { return Ok(self.add(Obj::Null)) };
        // WebGPU usage bits == wgpu BufferUsages bits; COPY_DST is always added for write_buffer.
        let usage = wgpu::BufferUsages::from_bits_truncate(usage) | wgpu::BufferUsages::COPY_DST;
        let b = gpu.device.create_buffer(&wgpu::BufferDescriptor { label: None, size: size as u64, usage, mapped_at_creation: false });
        Ok(self.add(Obj::Buffer(b)))
    }

    pub fn write_buffer(&mut self, h: u32, offset: u32, data: &[u8]) -> R<()> {
        if offset % 4 != 0 || data.len() % 4 != 0 {
            return Err(format!("gfx.write_buffer: offset {offset} and length {} must be multiples of 4", data.len()));
        }
        if self.gpu.is_none() {
            return self.get(h).map(|_| ());
        }
        let buf = self.buffer(h)?;
        if offset as u64 + data.len() as u64 > buf.size() {
            return Err(format!("gfx.write_buffer: {}+{} exceeds buffer size {}", offset, data.len(), buf.size()));
        }
        self.gpu.as_ref().unwrap().queue.write_buffer(buf, offset as u64, data);
        Ok(())
    }

    pub fn create_pipeline(&mut self, json: &str) -> R<u32> {
        let d: Value = serde_json::from_str(json).map_err(|e| format!("gfx.create_pipeline: invalid JSON: {e}"))?;
        if self.gpu.is_none() {
            return Ok(self.add(Obj::Null));
        }
        let p = self.build_pipeline(&d)?;
        Ok(self.add(Obj::Pipeline(p)))
    }

    pub fn create_bind_group(&mut self, json: &str) -> R<u32> {
        let d: Value = serde_json::from_str(json).map_err(|e| format!("gfx.create_bind_group: invalid JSON: {e}"))?;
        if self.gpu.is_none() {
            return Ok(self.add(Obj::Null));
        }
        let pipeline = self.pipeline(uint(&d, "pipeline")?)?;
        let layout = pipeline.get_bind_group_layout(d.get("group").and_then(Value::as_u64).unwrap_or(0) as u32);
        let entries_json = d.get("entries").and_then(Value::as_array).ok_or("bind group: missing entries")?;
        let mut entries = Vec::new();
        for e in entries_json {
            let buffer = self.buffer(uint(e, "buffer")?)?;
            let size = e.get("size").and_then(Value::as_u64).and_then(wgpu::BufferSize::new);
            entries.push(wgpu::BindGroupEntry {
                binding: uint(e, "binding")?,
                resource: wgpu::BindingResource::Buffer(wgpu::BufferBinding {
                    buffer,
                    offset: e.get("offset").and_then(Value::as_u64).unwrap_or(0),
                    size,
                }),
            });
        }
        let bg = self.scoped("create_bind_group", |gpu| {
            Ok(gpu.device.create_bind_group(&wgpu::BindGroupDescriptor { label: None, layout: &layout, entries: &entries }))
        })?;
        Ok(self.add(Obj::BindGroup(bg)))
    }

    fn build_pipeline(&self, d: &Value) -> R<wgpu::RenderPipeline> {
        let gpu = self.gpu.as_ref().unwrap();
        let v = d.get("vertex").ok_or("pipeline: missing vertex")?;
        let vmod = self.shader(uint(v, "module")?)?;
        let buffers_json = v.get("buffers").and_then(Value::as_array).cloned().unwrap_or_default();
        let mut attrs: Vec<Vec<wgpu::VertexAttribute>> = Vec::new();
        for b in &buffers_json {
            let mut list = Vec::new();
            for a in b.get("attributes").and_then(Value::as_array).cloned().unwrap_or_default() {
                list.push(wgpu::VertexAttribute {
                    format: vertex_format(string(&a, "format")?)?,
                    offset: a.get("offset").and_then(Value::as_u64).unwrap_or(0),
                    shader_location: uint(&a, "shaderLocation")?,
                });
            }
            attrs.push(list);
        }
        let layouts: Vec<Option<wgpu::VertexBufferLayout>> = buffers_json
            .iter()
            .zip(&attrs)
            .map(|(b, a)| {
                Some(wgpu::VertexBufferLayout {
                    array_stride: b.get("arrayStride").and_then(Value::as_u64).unwrap_or(0),
                    step_mode: match b.get("stepMode").and_then(Value::as_str) {
                        Some("instance") => wgpu::VertexStepMode::Instance,
                        _ => wgpu::VertexStepMode::Vertex,
                    },
                    attributes: a,
                })
            })
            .collect();

        let mut targets = Vec::new();
        let fragment_module;
        let f = d.get("fragment");
        if let Some(f) = f {
            fragment_module = Some(self.shader(uint(f, "module")?)?);
            for t in f.get("targets").and_then(Value::as_array).cloned().unwrap_or_default() {
                let format = match t.get("format").and_then(Value::as_str) {
                    Some("surface") | None => gpu.format,
                    Some(other) => return Err(format!("pipeline: color target format must be \"surface\", got {other:?}")),
                };
                let blend = match t.get("blend") {
                    Some(b) => Some(wgpu::BlendState {
                        color: blend_component(b.get("color"))?,
                        alpha: blend_component(b.get("alpha"))?,
                    }),
                    None => None,
                };
                let write_mask = t
                    .get("writeMask")
                    .and_then(Value::as_u64)
                    .map(|m| wgpu::ColorWrites::from_bits_truncate(m as u32))
                    .unwrap_or(wgpu::ColorWrites::ALL);
                targets.push(Some(wgpu::ColorTargetState { format, blend, write_mask }));
            }
        } else {
            fragment_module = None;
        }

        let p = d.get("primitive");
        let ps = |k: &str| p.and_then(|p| p.get(k)).and_then(Value::as_str);
        let primitive = wgpu::PrimitiveState {
            topology: match ps("topology").unwrap_or("triangle-list") {
                "point-list" => wgpu::PrimitiveTopology::PointList,
                "line-list" => wgpu::PrimitiveTopology::LineList,
                "line-strip" => wgpu::PrimitiveTopology::LineStrip,
                "triangle-list" => wgpu::PrimitiveTopology::TriangleList,
                "triangle-strip" => wgpu::PrimitiveTopology::TriangleStrip,
                o => return Err(format!("pipeline: unknown topology {o:?}")),
            },
            strip_index_format: match ps("stripIndexFormat") {
                Some("uint16") => Some(wgpu::IndexFormat::Uint16),
                Some("uint32") => Some(wgpu::IndexFormat::Uint32),
                _ => None,
            },
            front_face: match ps("frontFace").unwrap_or("ccw") {
                "cw" => wgpu::FrontFace::Cw,
                _ => wgpu::FrontFace::Ccw,
            },
            cull_mode: match ps("cullMode").unwrap_or("none") {
                "front" => Some(wgpu::Face::Front),
                "back" => Some(wgpu::Face::Back),
                _ => None,
            },
            ..Default::default()
        };

        let depth_stencil = match d.get("depthStencil") {
            Some(ds) => {
                if let Some(f) = ds.get("format").and_then(Value::as_str)
                    && f != "depth24plus"
                {
                    return Err(format!("pipeline: depthStencil format must be \"depth24plus\", got {f:?}"));
                }
                Some(wgpu::DepthStencilState {
                    format: DEPTH_FORMAT,
                    depth_write_enabled: Some(ds.get("depthWriteEnabled").and_then(Value::as_bool).unwrap_or(false)),
                    depth_compare: Some(compare(ds.get("depthCompare").and_then(Value::as_str).unwrap_or("always"))?),
                    stencil: Default::default(),
                    bias: Default::default(),
                })
            }
            // The render pass always has a depth attachment, so a pipeline without
            // depthStencil gets a no-op one (no test, no writes) to stay compatible.
            None => Some(wgpu::DepthStencilState {
                format: DEPTH_FORMAT,
                depth_write_enabled: Some(false),
                depth_compare: Some(wgpu::CompareFunction::Always),
                stencil: Default::default(),
                bias: Default::default(),
            }),
        };

        let entry = |s: &Value| s.get("entryPoint").and_then(Value::as_str).map(str::to_owned);
        let (v_entry, f_entry) = (entry(v), f.and_then(entry));
        self.scoped("create_pipeline", |gpu| {
            Ok(gpu.device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
                label: None,
                layout: None, // "auto"
                vertex: wgpu::VertexState {
                    module: vmod,
                    entry_point: v_entry.as_deref(),
                    compilation_options: Default::default(),
                    buffers: &layouts,
                },
                primitive,
                depth_stencil,
                multisample: wgpu::MultisampleState { count: SAMPLE_COUNT, ..Default::default() },
                fragment: fragment_module.map(|m| wgpu::FragmentState {
                    module: m,
                    entry_point: f_entry.as_deref(),
                    compilation_options: Default::default(),
                    targets: &targets,
                }),
                multiview_mask: None,
                cache: None,
            }))
        })
    }

    // ---- frame -----------------------------------------------------------------------------

    /// Start a frame. `show`: whether the runner will display it.
    pub fn begin_frame(&mut self, clear: [f32; 4], show: bool) -> R<bool> {
        if self.frame.is_some() {
            return Err("gfx.begin_frame called twice without end_frame".into());
        }
        self.used = true;
        let (Some(gpu), Some(target), true) = (&self.gpu, &self.target, show) else { return Ok(false) };
        let (resolve_view, surface_texture) = match target {
            Target::Window { surface, config } => {
                let st = match surface.get_current_texture() {
                    wgpu::CurrentSurfaceTexture::Success(t) | wgpu::CurrentSurfaceTexture::Suboptimal(t) => t,
                    wgpu::CurrentSurfaceTexture::Outdated | wgpu::CurrentSurfaceTexture::Lost => {
                        surface.configure(&gpu.device, config);
                        return Ok(false);
                    }
                    _ => return Ok(false), // timeout / occluded: skip this frame
                };
                (st.texture.create_view(&Default::default()), Some(st))
            }
            Target::Offscreen { texture } => (texture.create_view(&Default::default()), None),
        };
        let mut encoder = gpu.device.create_command_encoder(&Default::default());
        let c = clear.map(|v| v as f64);
        let pass = encoder
            .begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("guest"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: self.msaa.as_ref().unwrap(),
                    depth_slice: None,
                    resolve_target: Some(&resolve_view),
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Clear(wgpu::Color { r: c[0], g: c[1], b: c[2], a: c[3] }),
                        store: wgpu::StoreOp::Discard,
                    },
                })],
                depth_stencil_attachment: Some(wgpu::RenderPassDepthStencilAttachment {
                    view: self.depth.as_ref().unwrap(),
                    depth_ops: Some(wgpu::Operations { load: wgpu::LoadOp::Clear(1.0), store: wgpu::StoreOp::Discard }),
                    stencil_ops: None,
                }),
                timestamp_writes: None,
                occlusion_query_set: None,
                multiview_mask: None,
            })
            .forget_lifetime();
        self.frame = Some(Frame { pass, encoder, surface_texture });
        Ok(true)
    }

    pub fn set_pipeline(&mut self, h: u32) -> R<()> {
        if self.gpu.is_none() { return self.get(h).map(|_| ()) }
        let p = self.pipeline(h)?.clone();
        if let Some(f) = &mut self.frame { f.pass.set_pipeline(&p); }
        Ok(())
    }

    pub fn set_bind_group(&mut self, index: u32, h: u32) -> R<()> {
        if self.gpu.is_none() { return self.get(h).map(|_| ()) }
        let bg = self.bind_group(h)?.clone();
        if let Some(f) = &mut self.frame { f.pass.set_bind_group(index, &bg, &[]); }
        Ok(())
    }

    pub fn set_vertex_buffer(&mut self, slot: u32, h: u32, offset: u32) -> R<()> {
        if self.gpu.is_none() { return self.get(h).map(|_| ()) }
        let b = self.buffer(h)?.clone();
        if let Some(f) = &mut self.frame { f.pass.set_vertex_buffer(slot, b.slice(offset as u64..)); }
        Ok(())
    }

    pub fn set_index_buffer(&mut self, h: u32, format: u32, offset: u32) -> R<()> {
        if self.gpu.is_none() { return self.get(h).map(|_| ()) }
        let b = self.buffer(h)?.clone();
        let fmt = if format == 1 { wgpu::IndexFormat::Uint32 } else { wgpu::IndexFormat::Uint16 };
        if let Some(f) = &mut self.frame { f.pass.set_index_buffer(b.slice(offset as u64..), fmt); }
        Ok(())
    }

    pub fn draw(&mut self, vc: u32, ic: u32, fv: u32, fi: u32) {
        if let Some(f) = &mut self.frame { f.pass.draw(fv..fv + vc, fi..fi + ic); }
    }

    pub fn draw_indexed(&mut self, ic: u32, inst: u32, first: u32, base: i32, fi: u32) {
        if let Some(f) = &mut self.frame { f.pass.draw_indexed(first..first + ic, base, fi..fi + inst); }
    }

    pub fn end_frame(&mut self) -> R<()> {
        let Some(Frame { pass, encoder, surface_texture }) = self.frame.take() else { return Ok(()) };
        drop(pass);
        let gpu = self.gpu.as_ref().unwrap();
        gpu.queue.submit([encoder.finish()]);
        if let Some(st) = surface_texture {
            gpu.queue.present(st);
        }
        Ok(())
    }

    // ---- 2D path: show a video_present frame -----------------------------------------------

    /// Letterboxed nearest-neighbour blit of an RGBA frame to the window.
    pub fn present_video(&mut self, rgba: &[u8], w: u32, h: u32) {
        let (Some(gpu), Some(Target::Window { surface, config }), Some(blit)) = (&self.gpu, &self.target, &mut self.blit) else { return };
        let st = match surface.get_current_texture() {
            wgpu::CurrentSurfaceTexture::Success(t) | wgpu::CurrentSurfaceTexture::Suboptimal(t) => t,
            wgpu::CurrentSurfaceTexture::Outdated | wgpu::CurrentSurfaceTexture::Lost => {
                surface.configure(&gpu.device, config);
                return;
            }
            _ => return,
        };
        blit.upload(gpu, rgba, w, h);
        let view = st.texture.create_view(&Default::default());
        let mut encoder = gpu.device.create_command_encoder(&Default::default());
        {
            let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("blit"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: &view,
                    depth_slice: None,
                    resolve_target: None,
                    ops: wgpu::Operations { load: wgpu::LoadOp::Clear(wgpu::Color::BLACK), store: wgpu::StoreOp::Store },
                })],
                ..Default::default()
            });
            // letterbox: largest rect with the frame's aspect ratio
            let (sw, sh) = (self.width as f32, self.height as f32);
            let scale = (sw / w as f32).min(sh / h as f32);
            let (vw, vh) = (w as f32 * scale, h as f32 * scale);
            pass.set_viewport((sw - vw) / 2.0, (sh - vh) / 2.0, vw, vh, 0.0, 1.0);
            pass.set_pipeline(&blit.pipeline);
            pass.set_bind_group(0, &blit.texture.as_ref().unwrap().1, &[]);
            pass.draw(0..3, 0..1);
        }
        gpu.queue.submit([encoder.finish()]);
        gpu.queue.present(st);
    }

    /// Read back the offscreen target as tightly packed RGBA8 (headless screenshots).
    pub fn read_offscreen(&self) -> Option<(u32, u32, Vec<u8>)> {
        let (Some(gpu), Some(Target::Offscreen { texture })) = (&self.gpu, &self.target) else { return None };
        let (w, h) = (self.width, self.height);
        let row = (w * 4).div_ceil(256) * 256;
        let buf = gpu.device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("readback"),
            size: (row * h) as u64,
            usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
            mapped_at_creation: false,
        });
        let mut enc = gpu.device.create_command_encoder(&Default::default());
        enc.copy_texture_to_buffer(
            texture.as_image_copy(),
            wgpu::TexelCopyBufferInfo {
                buffer: &buf,
                layout: wgpu::TexelCopyBufferLayout { offset: 0, bytes_per_row: Some(row), rows_per_image: Some(h) },
            },
            wgpu::Extent3d { width: w, height: h, depth_or_array_layers: 1 },
        );
        gpu.queue.submit([enc.finish()]);
        buf.slice(..).map_async(wgpu::MapMode::Read, |_| {});
        gpu.device.poll(wgpu::PollType::wait_indefinitely()).ok()?;
        let data = buf.slice(..).get_mapped_range().ok()?;
        let mut out = Vec::with_capacity((w * h * 4) as usize);
        for y in 0..h as usize {
            out.extend_from_slice(&data[y * row as usize..y * row as usize + (w * 4) as usize]);
        }
        Some((w, h, out))
    }
}

fn open_device(
    adapter: &wgpu::Adapter,
    choose: impl FnOnce(&[wgpu::TextureFormat]) -> wgpu::TextureFormat,
    surface: &wgpu::Surface,
) -> R<Gpu> {
    let (device, queue) =
        pollster::block_on(adapter.request_device(&wgpu::DeviceDescriptor::default())).map_err(|e| e.to_string())?;
    device.on_uncaptured_error(Arc::new(|e| eprintln!("[gasm] gpu error: {e}")));
    let caps = surface.get_capabilities(adapter);
    let format = choose(&caps.formats);
    Ok(Gpu { device, queue, format })
}

/// Prefer a non-sRGB 8-bit format: browsers' canvas formats are non-sRGB, so
/// guests look the same on both runners.
fn pick_format(formats: &[wgpu::TextureFormat]) -> wgpu::TextureFormat {
    use wgpu::TextureFormat as F;
    [F::Bgra8Unorm, F::Rgba8Unorm].into_iter().find(|f| formats.contains(f)).unwrap_or(formats[0])
}

fn uint(v: &Value, key: &str) -> R<u32> {
    v.get(key).and_then(Value::as_u64).map(|n| n as u32).ok_or_else(|| format!("descriptor: missing number {key:?}"))
}

fn string<'a>(v: &'a Value, key: &str) -> R<&'a str> {
    v.get(key).and_then(Value::as_str).ok_or_else(|| format!("descriptor: missing string {key:?}"))
}

fn vertex_format(s: &str) -> R<wgpu::VertexFormat> {
    use wgpu::VertexFormat as V;
    Ok(match s {
        "float32" => V::Float32,
        "float32x2" => V::Float32x2,
        "float32x3" => V::Float32x3,
        "float32x4" => V::Float32x4,
        "uint32" => V::Uint32,
        "uint32x2" => V::Uint32x2,
        "uint32x3" => V::Uint32x3,
        "uint32x4" => V::Uint32x4,
        "sint32" => V::Sint32,
        "sint32x2" => V::Sint32x2,
        "sint32x3" => V::Sint32x3,
        "sint32x4" => V::Sint32x4,
        "uint8x4" => V::Uint8x4,
        "unorm8x4" => V::Unorm8x4,
        "uint16x2" => V::Uint16x2,
        "uint16x4" => V::Uint16x4,
        "float16x2" => V::Float16x2,
        "float16x4" => V::Float16x4,
        o => return Err(format!("unsupported vertex format {o:?}")),
    })
}

fn blend_component(v: Option<&Value>) -> R<wgpu::BlendComponent> {
    let Some(v) = v else { return Ok(wgpu::BlendComponent::REPLACE) };
    let factor = |k: &str, default: wgpu::BlendFactor| -> R<wgpu::BlendFactor> {
        use wgpu::BlendFactor as B;
        Ok(match v.get(k).and_then(Value::as_str) {
            None => default,
            Some("zero") => B::Zero,
            Some("one") => B::One,
            Some("src") => B::Src,
            Some("one-minus-src") => B::OneMinusSrc,
            Some("src-alpha") => B::SrcAlpha,
            Some("one-minus-src-alpha") => B::OneMinusSrcAlpha,
            Some("dst") => B::Dst,
            Some("one-minus-dst") => B::OneMinusDst,
            Some("dst-alpha") => B::DstAlpha,
            Some("one-minus-dst-alpha") => B::OneMinusDstAlpha,
            Some("src-alpha-saturated") => B::SrcAlphaSaturated,
            Some("constant") => B::Constant,
            Some("one-minus-constant") => B::OneMinusConstant,
            Some(o) => return Err(format!("unsupported blend factor {o:?}")),
        })
    };
    use wgpu::BlendOperation as O;
    Ok(wgpu::BlendComponent {
        src_factor: factor("srcFactor", wgpu::BlendFactor::One)?,
        dst_factor: factor("dstFactor", wgpu::BlendFactor::Zero)?,
        operation: match v.get("operation").and_then(Value::as_str).unwrap_or("add") {
            "add" => O::Add,
            "subtract" => O::Subtract,
            "reverse-subtract" => O::ReverseSubtract,
            "min" => O::Min,
            "max" => O::Max,
            o => return Err(format!("unsupported blend operation {o:?}")),
        },
    })
}

fn compare(s: &str) -> R<wgpu::CompareFunction> {
    use wgpu::CompareFunction as C;
    Ok(match s {
        "never" => C::Never,
        "less" => C::Less,
        "equal" => C::Equal,
        "less-equal" => C::LessEqual,
        "greater" => C::Greater,
        "not-equal" => C::NotEqual,
        "greater-equal" => C::GreaterEqual,
        "always" => C::Always,
        o => return Err(format!("unsupported compare function {o:?}")),
    })
}

impl Blit {
    const SHADER: &str = r#"
@group(0) @binding(0) var t: texture_2d<f32>;
@group(0) @binding(1) var s: sampler;
struct VO { @builtin(position) pos: vec4<f32>, @location(0) uv: vec2<f32> };
@vertex fn vs(@builtin(vertex_index) i: u32) -> VO {
  let uv = vec2<f32>(f32((i << 1u) & 2u), f32(i & 2u));
  var o: VO;
  o.pos = vec4<f32>(uv.x * 2.0 - 1.0, 1.0 - uv.y * 2.0, 0.0, 1.0);
  o.uv = uv;
  return o;
}
@fragment fn fs(i: VO) -> @location(0) vec4<f32> { return textureSample(t, s, i.uv); }
"#;

    fn new(gpu: &Gpu) -> Blit {
        let module = gpu.device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("blit"),
            source: wgpu::ShaderSource::Wgsl(Self::SHADER.into()),
        });
        let pipeline = gpu.device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("blit"),
            layout: None,
            vertex: wgpu::VertexState { module: &module, entry_point: Some("vs"), compilation_options: Default::default(), buffers: &[] },
            primitive: Default::default(),
            depth_stencil: None,
            multisample: Default::default(),
            fragment: Some(wgpu::FragmentState {
                module: &module,
                entry_point: Some("fs"),
                compilation_options: Default::default(),
                targets: &[Some(gpu.format.into())],
            }),
            multiview_mask: None,
            cache: None,
        });
        let sampler = gpu.device.create_sampler(&wgpu::SamplerDescriptor {
            mag_filter: wgpu::FilterMode::Nearest,
            min_filter: wgpu::FilterMode::Nearest,
            ..Default::default()
        });
        Blit { pipeline, sampler, texture: None }
    }

    fn upload(&mut self, gpu: &Gpu, rgba: &[u8], w: u32, h: u32) {
        if !matches!(&self.texture, Some((_, _, tw, th)) if (*tw, *th) == (w, h)) {
            let tex = gpu.device.create_texture_with_data(
                &gpu.queue,
                &wgpu::TextureDescriptor {
                    label: Some("video"),
                    size: wgpu::Extent3d { width: w, height: h, depth_or_array_layers: 1 },
                    mip_level_count: 1,
                    sample_count: 1,
                    dimension: wgpu::TextureDimension::D2,
                    format: wgpu::TextureFormat::Rgba8Unorm,
                    usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
                    view_formats: &[],
                },
                Default::default(),
                rgba,
            );
            let bg = gpu.device.create_bind_group(&wgpu::BindGroupDescriptor {
                label: None,
                layout: &self.pipeline.get_bind_group_layout(0),
                entries: &[
                    wgpu::BindGroupEntry { binding: 0, resource: wgpu::BindingResource::TextureView(&tex.create_view(&Default::default())) },
                    wgpu::BindGroupEntry { binding: 1, resource: wgpu::BindingResource::Sampler(&self.sampler) },
                ],
            });
            self.texture = Some((tex, bg, w, h));
            return;
        }
        let tex = &self.texture.as_ref().unwrap().0;
        gpu.queue.write_texture(
            tex.as_image_copy(),
            rgba,
            wgpu::TexelCopyBufferLayout { offset: 0, bytes_per_row: Some(w * 4), rows_per_image: Some(h) },
            wgpu::Extent3d { width: w, height: h, depth_or_array_layers: 1 },
        );
    }
}
