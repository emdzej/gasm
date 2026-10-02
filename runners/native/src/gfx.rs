//! gasm:gfx on wgpu, plus the 2D `video_present` blit.
//!
//! Handles index a single object table, numbered in creation order and never
//! reused (`destroy` leaves a tombstone). Creation descriptors are JSON that
//! mirrors WebGPU (see spec/ABI.md#gasmgfx).
//!
//! Every object has a [`Meta`] record (kind, sizes, layouts, vertex layout) and
//! the render pass state (pipeline, bind groups, vertex and index buffers) is
//! tracked the same way with and without a GPU, so handles, ranges, usages,
//! bind group compatibility and draw ranges are validated (and trap) identically
//! on the null GPU, the real GPU and the JS runner (`GfxModel` in gasm-host.js).
//! What only the GPU can check (WGSL, pipeline/shader interface) is caught with
//! wgpu error scopes and becomes `Err`, which traps the guest.

use std::sync::Arc;

use serde_json::Value;
use crate::present::{Present, Presenter, VideoFrame};

pub const SAMPLE_COUNT: u32 = 4;
const DEPTH_FORMAT: wgpu::TextureFormat = wgpu::TextureFormat::Depth24Plus;

/// Drawable size of headless runs (null GPU and offscreen screenshots).
pub const HEADLESS_SIZE: (u32, u32) = (1280, 720);
/// Largest texture side (WebGPU's default `maxTextureDimension2D`).
pub const MAX_TEXTURE_SIZE: u32 = 8192;
/// Dynamic offsets must be multiples of this (WebGPU's default alignment limits).
pub const OFFSET_ALIGNMENT: u32 = 256;
/// WebGPU default limits used for validation.
pub const MAX_BIND_GROUPS: u32 = 4;
pub const MAX_VERTEX_BUFFERS: u32 = 8;
const MAX_VERTEX_ATTRIBUTES: usize = 16;
const MAX_VERTEX_STRIDE: u64 = 2048;

// GASM_BUF_* (spec/abi.json; WebGPU GPUBufferUsage bits)
pub const BUF_COPY_DST: u32 = 0x08;
pub const BUF_INDEX: u32 = 0x10;
pub const BUF_VERTEX: u32 = 0x20;
pub const BUF_UNIFORM: u32 = 0x40;
pub const BUF_STORAGE: u32 = 0x80;

enum Obj {
    Shader(wgpu::ShaderModule),
    Buffer(wgpu::Buffer),
    Pipeline(wgpu::RenderPipeline),
    BindGroup(wgpu::BindGroup),
    Layout(wgpu::BindGroupLayout),
    Texture(wgpu::Texture, wgpu::TextureView),
    Sampler(wgpu::Sampler),
    /// null GPU, or destroyed
    Null,
}

/// Backend-independent description of an object, used for validation.
#[derive(Clone)]
enum Meta {
    Shader,
    Buffer { size: u64, usage: u32 },
    Pipeline(PipelineMeta),
    BindGroup { dynamic: Vec<DynEntry>, layout: GroupLayout },
    Layout { entries: Vec<LayoutEntry> },
    Texture { width: u32, height: u32, mips: u32 },
    Sampler,
    Destroyed(&'static str),
}

impl Meta {
    fn kind(&self) -> &'static str {
        match self {
            Meta::Shader => "shader",
            Meta::Buffer { .. } => "buffer",
            Meta::Pipeline(_) => "pipeline",
            Meta::BindGroup { .. } => "bind group",
            Meta::Layout { .. } => "bind group layout",
            Meta::Texture { .. } => "texture",
            Meta::Sampler => "sampler",
            Meta::Destroyed(_) => "destroyed object",
        }
    }
}

#[derive(Clone)]
struct PipelineMeta {
    /// explicit layouts: the entries of each group; None = "auto"
    groups: Option<Vec<Vec<LayoutEntry>>>,
    vertex: Vec<VertexSlot>,
    /// color targets of the fragment stage (the pass has exactly one)
    targets: usize,
}

/// One vertex buffer of a pipeline.
#[derive(Clone, Copy)]
struct VertexSlot {
    stride: u64,
    instance: bool,
    /// bytes the last element reads: max(attribute offset + size)
    last: u64,
}

/// What a bind group was created against.
#[derive(Clone)]
enum GroupLayout {
    Explicit(Vec<LayoutEntry>),
    /// from a pipeline with layout "auto": only compatible with that pipeline
    Auto { pipeline: u32, group: u32 },
}

#[derive(Clone, Copy)]
struct DynEntry {
    buffer_size: u64,
    offset: u64,
    size: u64,
}

#[derive(Clone, Copy, PartialEq)]
enum Slot {
    Uniform,
    Storage,
    Texture,
    Sampler,
}

#[derive(Clone, PartialEq)]
struct LayoutEntry {
    binding: u32,
    visibility: u32,
    slot: Slot,
    dynamic: bool,
    min_binding_size: u64,
    /// texture: "float" (filterable) or "unfilterable-float"; sampler: "filtering" or "non-filtering"
    filterable: bool,
}

/// Render pass state between begin_frame and end_frame, tracked in every mode.
#[derive(Default)]
struct Pass {
    pipeline: Option<u32>,
    groups: [Option<u32>; MAX_BIND_GROUPS as usize],
    vertex: [Option<(u32, u64)>; MAX_VERTEX_BUFFERS as usize],
    /// (buffer, index size in bytes, offset)
    index: Option<(u32, u64, u64)>,
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

/// GPU work of a shown frame.
struct Frame {
    pass: wgpu::RenderPass<'static>,
    encoder: wgpu::CommandEncoder,
    surface_texture: Option<wgpu::SurfaceTexture>,
    /// viewport or scissor clamped to nothing: draws are skipped
    empty_viewport: bool,
    empty_scissor: bool,
}

struct Object {
    obj: Obj,
    meta: Meta,
}

pub struct Gfx {
    gpu: Option<Gpu>,
    target: Option<Target>,
    width: u32,
    height: u32,
    objects: Vec<Object>,
    msaa: Option<wgpu::TextureView>,
    depth: Option<wgpu::TextureView>,
    /// between begin_frame and end_frame (shown or not)
    pass: Option<Pass>,
    frame: Option<Frame>,
    presenter: Option<Presenter>,
    /// how video_present frames are shown (filter, integer scaling)
    pub present: Present,
    /// set by the guest calling any gfx draw API this frame (suppresses the 2D blit)
    pub used: bool,
}

type R<T> = Result<T, String>;

fn add_u64(a: u64, b: u64, what: &str) -> R<u64> {
    a.checked_add(b).ok_or_else(|| format!("gfx: {what} overflows"))
}

impl Gfx {
    fn new(gpu: Option<Gpu>, target: Option<Target>, width: u32, height: u32) -> Gfx {
        let presenter = gpu.as_ref().map(Presenter::new);
        Gfx { gpu, target, width, height, objects: Vec::new(), msaa: None, depth: None, pass: None, frame: None, presenter, present: Present::default(), used: false }
    }

    /// No GPU: ids and validation only (headless runs without screenshots, CI).
    pub fn null() -> Gfx {
        Gfx::new(None, None, HEADLESS_SIZE.0, HEADLESS_SIZE.1)
    }

    /// Render into a window.
    #[cfg(feature = "window")]
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
        let gpu = open_device(&adapter, pick_format, &surface)?;
        let mut config = surface
            .get_default_config(&adapter, size.width.max(1), size.height.max(1))
            .ok_or("surface not supported by adapter")?;
        config.format = gpu.format;
        config.present_mode = wgpu::PresentMode::AutoVsync;
        surface.configure(&gpu.device, &config);
        eprintln!("[gasm] gpu: {} ({:?}), surface {:?}", adapter.get_info().name, adapter.get_info().backend, gpu.format);
        let mut g = Gfx::new(Some(gpu), Some(Target::Window { surface, config }), size.width.max(1), size.height.max(1));
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
        let mut g = Gfx::new(Some(Gpu { device, queue, format }), Some(Target::Offscreen { texture }), width, height);
        g.recreate_attachments();
        Ok(g)
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

    fn add(&mut self, obj: Obj, meta: Meta) -> u32 {
        self.objects.push(Object { obj, meta });
        self.objects.len() as u32
    }

    fn object(&self, h: u32) -> R<&Object> {
        let o = self.objects.get((h as usize).wrapping_sub(1)).ok_or_else(|| format!("gfx: invalid handle {h}"))?;
        if let Meta::Destroyed(kind) = o.meta {
            return Err(format!("gfx: {kind} {h} was destroyed"));
        }
        Ok(o)
    }

    fn meta(&self, h: u32) -> R<&Meta> {
        Ok(&self.object(h)?.meta)
    }

    /// Check that `h` is a live object of kind `want` (see [`Meta::kind`]).
    fn expect(&self, h: u32, want: &str) -> R<&Meta> {
        let m = self.meta(h)?;
        if m.kind() != want {
            return Err(format!("gfx: handle {h} is a {}, not a {want}", m.kind()));
        }
        Ok(m)
    }

    fn buffer_meta(&self, h: u32) -> R<(u64, u32)> {
        match self.expect(h, "buffer")? {
            Meta::Buffer { size, usage } => Ok((*size, *usage)),
            _ => unreachable!(),
        }
    }

    fn pipeline_meta(&self, h: u32) -> R<&PipelineMeta> {
        match self.expect(h, "pipeline")? {
            Meta::Pipeline(p) => Ok(p),
            _ => unreachable!(),
        }
    }

    fn obj(&self, h: u32) -> R<&Obj> {
        Ok(&self.object(h)?.obj)
    }

    fn shader(&self, h: u32) -> R<&wgpu::ShaderModule> {
        match self.obj(h)? { Obj::Shader(s) => Ok(s), _ => Err(format!("gfx: handle {h} is not a shader")) }
    }
    fn buffer(&self, h: u32) -> R<&wgpu::Buffer> {
        match self.obj(h)? { Obj::Buffer(b) => Ok(b), _ => Err(format!("gfx: handle {h} is not a buffer")) }
    }
    fn pipeline(&self, h: u32) -> R<&wgpu::RenderPipeline> {
        match self.obj(h)? { Obj::Pipeline(p) => Ok(p), _ => Err(format!("gfx: handle {h} is not a pipeline")) }
    }
    fn bind_group(&self, h: u32) -> R<&wgpu::BindGroup> {
        match self.obj(h)? { Obj::BindGroup(b) => Ok(b), _ => Err(format!("gfx: handle {h} is not a bind group")) }
    }
    fn layout(&self, h: u32) -> R<&wgpu::BindGroupLayout> {
        match self.obj(h)? { Obj::Layout(l) => Ok(l), _ => Err(format!("gfx: handle {h} is not a bind group layout")) }
    }

    /// Release an object: the handle becomes a tombstone (never reused). wgpu
    /// keeps the resource alive while bind groups or a recorded pass use it.
    pub fn destroy(&mut self, h: u32) -> R<()> {
        let kind = self.meta(h)?.kind();
        let o = &mut self.objects[h as usize - 1];
        o.obj = Obj::Null;
        o.meta = Meta::Destroyed(kind);
        Ok(())
    }

    /// Run `f` inside a validation error scope; wgpu errors become `Err`.
    fn scoped<T>(&self, what: &str, f: impl FnOnce(&Gpu) -> R<T>) -> R<T> {
        let gpu = self.gpu.as_ref().ok_or("gfx: no GPU")?;
        let scope = gpu.device.push_error_scope(wgpu::ErrorFilter::Validation);
        let out = f(gpu);
        if let Some(e) = pollster::block_on(scope.pop()) {
            return Err(format!("gfx: {what}: {e}"));
        }
        out
    }

    fn pass_mut(&mut self, what: &str) -> R<&mut Pass> {
        self.pass.as_mut().ok_or_else(|| format!("gfx.{what} called outside begin_frame/end_frame"))
    }

    // ---- creation ------------------------------------------------------------------------

    pub fn create_shader(&mut self, wgsl: &str) -> R<u32> {
        if self.gpu.is_none() {
            return Ok(self.add(Obj::Null, Meta::Shader));
        }
        let m = self.scoped("create_shader", |gpu| {
            Ok(gpu.device.create_shader_module(wgpu::ShaderModuleDescriptor {
                label: None,
                source: wgpu::ShaderSource::Wgsl(wgsl.into()),
            }))
        })?;
        Ok(self.add(Obj::Shader(m), Meta::Shader))
    }

    pub fn create_buffer(&mut self, size: u32, usage: u32) -> R<u32> {
        if size == 0 || size % 4 != 0 {
            return Err(format!("gfx.create_buffer: size {size} must be a non-zero multiple of 4"));
        }
        let meta = Meta::Buffer { size: size as u64, usage: usage | BUF_COPY_DST };
        let Some(gpu) = &self.gpu else { return Ok(self.add(Obj::Null, meta)) };
        // WebGPU usage bits == wgpu BufferUsages bits; COPY_DST is always added for write_buffer.
        let usage = wgpu::BufferUsages::from_bits_truncate(usage) | wgpu::BufferUsages::COPY_DST;
        let b = gpu.device.create_buffer(&wgpu::BufferDescriptor { label: None, size: size as u64, usage, mapped_at_creation: false });
        Ok(self.add(Obj::Buffer(b), meta))
    }

    pub fn write_buffer(&mut self, h: u32, offset: u32, data: &[u8]) -> R<()> {
        if offset % 4 != 0 || data.len() % 4 != 0 {
            return Err(format!("gfx.write_buffer: offset {offset} and length {} must be multiples of 4", data.len()));
        }
        let (size, _) = self.buffer_meta(h)?;
        if offset as u64 + data.len() as u64 > size {
            return Err(format!("gfx.write_buffer: {}+{} exceeds buffer size {size}", offset, data.len()));
        }
        if let Some(gpu) = &self.gpu {
            gpu.queue.write_buffer(self.buffer(h)?, offset as u64, data);
        }
        Ok(())
    }

    pub fn create_pipeline(&mut self, json: &str) -> R<u32> {
        let d: Value = serde_json::from_str(json).map_err(|e| format!("gfx.create_pipeline: invalid JSON: {e}"))?;
        let layouts = self.pipeline_layouts(&d)?;
        let v = d.get("vertex").ok_or("pipeline: missing vertex")?;
        self.expect(uint(v, "module")?, "shader")?;
        let f = d.get("fragment");
        if let Some(f) = f {
            self.expect(uint(f, "module")?, "shader")?;
        }
        let meta = PipelineMeta {
            groups: match &layouts {
                Some(hs) => Some(hs.iter().map(|h| self.layout_entries(*h)).collect::<R<Vec<_>>>()?),
                None => None,
            },
            vertex: vertex_slots(v)?,
            targets: f.and_then(|f| f.get("targets")).and_then(Value::as_array).map_or(0, Vec::len),
        };
        Ok(match self.build_pipeline(&d, layouts)? {
            Some(p) => self.add(Obj::Pipeline(p), Meta::Pipeline(meta)),
            None => self.add(Obj::Null, Meta::Pipeline(meta)),
        })
    }

    fn layout_entries(&self, h: u32) -> R<Vec<LayoutEntry>> {
        match self.expect(h, "bind group layout")? {
            Meta::Layout { entries } => Ok(entries.clone()),
            _ => unreachable!(),
        }
    }

    /// `"layout"`: omitted / `"auto"` (None), or an array of bind group layout handles.
    fn pipeline_layouts(&self, d: &Value) -> R<Option<Vec<u32>>> {
        match d.get("layout") {
            None => Ok(None),
            Some(Value::String(s)) if s == "auto" => Ok(None),
            Some(Value::Array(a)) => {
                if a.len() > MAX_BIND_GROUPS as usize {
                    return Err(format!("pipeline: at most {MAX_BIND_GROUPS} bind group layouts, got {}", a.len()));
                }
                let mut out = Vec::new();
                for v in a {
                    let h = v.as_u64().and_then(|h| u32::try_from(h).ok()).ok_or("pipeline: layout entries must be bind group layout handles")?;
                    self.expect(h, "bind group layout")?;
                    out.push(h);
                }
                Ok(Some(out))
            }
            Some(o) => Err(format!("pipeline: layout must be \"auto\" or an array of bind group layout handles, got {o}")),
        }
    }

    pub fn create_bind_group(&mut self, json: &str) -> R<u32> {
        #[derive(PartialEq)]
        enum Res {
            Buffer,
            Texture,
            Sampler,
        }
        let d: Value = serde_json::from_str(json).map_err(|e| format!("gfx.create_bind_group: invalid JSON: {e}"))?;
        let entries_json = d.get("entries").and_then(Value::as_array).cloned().unwrap_or_default();
        // Resolve and check every entry against the object table (both modes).
        let mut resolved: Vec<(u32, Res, u32, u64, Option<u64>)> = Vec::new(); // (binding, resource, handle, offset, size)
        for e in &entries_json {
            let binding = uint(e, "binding")?;
            if resolved.iter().any(|r| r.0 == binding) {
                return Err(format!("bind group: binding {binding} appears twice"));
            }
            if let Some(h) = e.get("buffer") {
                let h = handle(h)?;
                let (size, _) = self.buffer_meta(h)?;
                let offset = e.get("offset").and_then(Value::as_u64).unwrap_or(0);
                let bsize = e.get("size").and_then(Value::as_u64);
                if offset > size || add_u64(offset, bsize.unwrap_or(0), "bind group range")? > size || bsize == Some(0) {
                    return Err(format!("bind group: binding {binding}: range {offset}+{} exceeds buffer size {size}", bsize.unwrap_or(0)));
                }
                resolved.push((binding, Res::Buffer, h, offset, bsize));
            } else if let Some(h) = e.get("texture") {
                let h = handle(h)?;
                self.expect(h, "texture")?;
                resolved.push((binding, Res::Texture, h, 0, None));
            } else if let Some(h) = e.get("sampler") {
                let h = handle(h)?;
                self.expect(h, "sampler")?;
                resolved.push((binding, Res::Sampler, h, 0, None));
            } else {
                return Err(format!("bind group: binding {binding} needs a \"buffer\", \"texture\" or \"sampler\""));
            }
        }
        let mut dynamic = Vec::new();
        let explicit = match d.get("layout") {
            Some(h) => Some(handle(h)?),
            None => None,
        };
        // the layout's entries: an explicit layout, or group G of a pipeline with explicit layouts
        let from_pipeline = match explicit {
            Some(_) => None,
            None => {
                let p = uint(&d, "pipeline").map_err(|_| "bind group: needs \"layout\" or \"pipeline\"".to_string())?;
                let group = d.get("group").and_then(Value::as_u64).unwrap_or(0);
                if group >= MAX_BIND_GROUPS as u64 {
                    return Err(format!("bind group: group {group} must be below {MAX_BIND_GROUPS}"));
                }
                Some((p, group as u32))
            }
        };
        let explicit_entries = match (explicit, from_pipeline) {
            (Some(lh), _) => Some(self.layout_entries(lh)?),
            (None, Some((p, g))) => match &self.pipeline_meta(p)?.groups {
                Some(groups) => Some(groups.get(g as usize).cloned().ok_or_else(|| format!("bind group: pipeline {p} has no group {g}"))?),
                None => None,
            },
            _ => unreachable!(),
        };
        let layout_meta = if let Some(entries) = explicit_entries {
            let lh = explicit.map_or_else(|| "of the pipeline".to_string(), |h| h.to_string());
            if resolved.len() != entries.len() {
                return Err(format!("bind group: layout {lh} has {} entries, got {}", entries.len(), resolved.len()));
            }
            for le in &entries {
                let r = resolved.iter().find(|r| r.0 == le.binding).ok_or_else(|| format!("bind group: missing binding {}", le.binding))?;
                let want = match le.slot {
                    Slot::Uniform | Slot::Storage => Res::Buffer,
                    Slot::Texture => Res::Texture,
                    Slot::Sampler => Res::Sampler,
                };
                if r.1 != want {
                    return Err(format!("bind group: binding {} has the wrong resource kind for its layout", le.binding));
                }
                if want == Res::Buffer {
                    let (size, usage) = self.buffer_meta(r.2)?;
                    let need = if le.slot == Slot::Uniform { BUF_UNIFORM } else { BUF_STORAGE };
                    if usage & need == 0 {
                        return Err(format!("bind group: binding {}: buffer {} lacks {} usage", le.binding, r.2, if need == BUF_UNIFORM { "UNIFORM" } else { "STORAGE" }));
                    }
                    let bsize = r.4.unwrap_or(size - r.3);
                    if bsize < le.min_binding_size {
                        return Err(format!("bind group: binding {}: size {bsize} is below minBindingSize {}", le.binding, le.min_binding_size));
                    }
                    if le.dynamic {
                        dynamic.push((le.binding, DynEntry { buffer_size: size, offset: r.3, size: bsize }));
                    }
                }
            }
            dynamic.sort_by_key(|d| d.0);
            GroupLayout::Explicit(entries)
        } else {
            let (pipeline, group) = from_pipeline.unwrap();
            GroupLayout::Auto { pipeline, group }
        };
        let meta = Meta::BindGroup { dynamic: dynamic.into_iter().map(|d| d.1).collect(), layout: layout_meta };
        if self.gpu.is_none() {
            return Ok(self.add(Obj::Null, meta));
        }
        let layout = match explicit {
            Some(lh) => self.layout(lh)?.clone(),
            None => self.pipeline(uint(&d, "pipeline")?)?.get_bind_group_layout(d.get("group").and_then(Value::as_u64).unwrap_or(0) as u32),
        };
        let mut entries = Vec::new();
        for (binding, res, h, offset, size) in &resolved {
            let resource = match res {
                Res::Texture => match self.obj(*h)? { Obj::Texture(_, v) => wgpu::BindingResource::TextureView(v), _ => unreachable!() },
                Res::Sampler => match self.obj(*h)? { Obj::Sampler(s) => wgpu::BindingResource::Sampler(s), _ => unreachable!() },
                Res::Buffer => wgpu::BindingResource::Buffer(wgpu::BufferBinding {
                    buffer: self.buffer(*h)?,
                    offset: *offset,
                    size: size.and_then(wgpu::BufferSize::new),
                }),
            };
            entries.push(wgpu::BindGroupEntry { binding: *binding, resource });
        }
        let bg = self.scoped("create_bind_group", |gpu| {
            Ok(gpu.device.create_bind_group(&wgpu::BindGroupDescriptor { label: None, layout: &layout, entries: &entries }))
        })?;
        Ok(self.add(Obj::BindGroup(bg), meta))
    }
    pub fn create_bind_group_layout(&mut self, json: &str) -> R<u32> {
        let d: Value = serde_json::from_str(json).map_err(|e| format!("gfx.create_bind_group_layout: invalid JSON: {e}"))?;
        let mut entries: Vec<LayoutEntry> = Vec::new();
        for e in d.get("entries").and_then(Value::as_array).cloned().unwrap_or_default() {
            let binding = uint(&e, "binding")?;
            if entries.iter().any(|x| x.binding == binding) {
                return Err(format!("bind group layout: binding {binding} appears twice"));
            }
            let visibility = uint(&e, "visibility")?;
            if visibility == 0 || visibility & !3 != 0 {
                return Err(format!("bind group layout: binding {binding}: visibility {visibility} must be GASM_STAGE_VERTEX (1) and/or GASM_STAGE_FRAGMENT (2)"));
            }
            let entry = if let Some(b) = e.get("buffer") {
                let slot = match b.get("type").and_then(Value::as_str).unwrap_or("uniform") {
                    "uniform" => Slot::Uniform,
                    "read-only-storage" => Slot::Storage,
                    o => return Err(format!("bind group layout: binding {binding}: unsupported buffer type {o:?}")),
                };
                LayoutEntry {
                    binding, visibility, slot,
                    dynamic: b.get("hasDynamicOffset").and_then(Value::as_bool).unwrap_or(false),
                    min_binding_size: b.get("minBindingSize").and_then(Value::as_u64).unwrap_or(0),
                    filterable: false,
                }
            } else if let Some(t) = e.get("texture") {
                let filterable = match t.get("sampleType").and_then(Value::as_str).unwrap_or("float") {
                    "float" => true,
                    "unfilterable-float" => false,
                    o => return Err(format!("bind group layout: binding {binding}: unsupported sampleType {o:?}")),
                };
                if let Some(v) = t.get("viewDimension").and_then(Value::as_str)
                    && v != "2d"
                {
                    return Err(format!("bind group layout: binding {binding}: only viewDimension \"2d\" is supported, got {v:?}"));
                }
                if t.get("multisampled").and_then(Value::as_bool).unwrap_or(false) {
                    return Err(format!("bind group layout: binding {binding}: multisampled textures are not supported"));
                }
                LayoutEntry { binding, visibility, slot: Slot::Texture, dynamic: false, min_binding_size: 0, filterable }
            } else if let Some(sm) = e.get("sampler") {
                let filterable = match sm.get("type").and_then(Value::as_str).unwrap_or("filtering") {
                    "filtering" => true,
                    "non-filtering" => false,
                    o => return Err(format!("bind group layout: binding {binding}: unsupported sampler type {o:?}")),
                };
                LayoutEntry { binding, visibility, slot: Slot::Sampler, dynamic: false, min_binding_size: 0, filterable }
            } else {
                return Err(format!("bind group layout: binding {binding} needs \"buffer\", \"texture\" or \"sampler\""));
            };
            entries.push(entry);
        }
        entries.sort_by_key(|e| e.binding);
        let meta = Meta::Layout { entries: entries.clone() };
        if self.gpu.is_none() {
            return Ok(self.add(Obj::Null, meta));
        }
        let wentries: Vec<wgpu::BindGroupLayoutEntry> = entries
            .iter()
            .map(|e| wgpu::BindGroupLayoutEntry {
                binding: e.binding,
                visibility: wgpu::ShaderStages::from_bits_truncate(e.visibility),
                ty: match e.slot {
                    Slot::Uniform | Slot::Storage => wgpu::BindingType::Buffer {
                        ty: if e.slot == Slot::Uniform {
                            wgpu::BufferBindingType::Uniform
                        } else {
                            wgpu::BufferBindingType::Storage { read_only: true }
                        },
                        has_dynamic_offset: e.dynamic,
                        min_binding_size: wgpu::BufferSize::new(e.min_binding_size),
                    },
                    Slot::Texture => wgpu::BindingType::Texture {
                        sample_type: wgpu::TextureSampleType::Float { filterable: e.filterable },
                        view_dimension: wgpu::TextureViewDimension::D2,
                        multisampled: false,
                    },
                    Slot::Sampler => wgpu::BindingType::Sampler(if e.filterable {
                        wgpu::SamplerBindingType::Filtering
                    } else {
                        wgpu::SamplerBindingType::NonFiltering
                    }),
                },
                count: None,
            })
            .collect();
        let l = self.scoped("create_bind_group_layout", |gpu| {
            Ok(gpu.device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor { label: None, entries: &wentries }))
        })?;
        Ok(self.add(Obj::Layout(l), meta))
    }

    pub fn create_texture(&mut self, json: &str) -> R<u32> {
        let d: Value = serde_json::from_str(json).map_err(|e| format!("gfx.create_texture: invalid JSON: {e}"))?;
        let size = d.get("size").and_then(Value::as_array).ok_or("texture: missing \"size\": [width, height]")?;
        let dim = |i: usize| size.get(i).and_then(Value::as_u64).unwrap_or(0);
        let (w, h) = (dim(0), dim(1));
        if size.len() != 2 || w == 0 || h == 0 || w > MAX_TEXTURE_SIZE as u64 || h > MAX_TEXTURE_SIZE as u64 {
            return Err(format!("texture: size must be [width, height], each 1-{MAX_TEXTURE_SIZE}"));
        }
        let (w, h) = (w as u32, h as u32);
        let format = match d.get("format").and_then(Value::as_str).unwrap_or("rgba8unorm") {
            "rgba8unorm" => wgpu::TextureFormat::Rgba8Unorm,
            "rgba8unorm-srgb" => wgpu::TextureFormat::Rgba8UnormSrgb,
            o => return Err(format!("texture: unsupported format {o:?} (rgba8unorm, rgba8unorm-srgb)")),
        };
        let max_mips = 32 - w.max(h).leading_zeros();
        let mips = d.get("mipLevelCount").and_then(Value::as_u64).unwrap_or(1);
        if mips == 0 || mips > max_mips as u64 {
            return Err(format!("texture: mipLevelCount {mips} must be 1-{max_mips} for {w}x{h}"));
        }
        let meta = Meta::Texture { width: w, height: h, mips: mips as u32 };
        let Some(gpu) = &self.gpu else { return Ok(self.add(Obj::Null, meta)) };
        let t = gpu.device.create_texture(&wgpu::TextureDescriptor {
            label: None,
            size: wgpu::Extent3d { width: w, height: h, depth_or_array_layers: 1 },
            mip_level_count: mips as u32,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format,
            usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
            view_formats: &[],
        });
        let view = t.create_view(&Default::default());
        Ok(self.add(Obj::Texture(t, view), meta))
    }

    #[allow(clippy::too_many_arguments)]
    pub fn write_texture(&mut self, h: u32, mip: u32, x: u32, y: u32, w: u32, ht: u32, data: &[u8]) -> R<()> {
        let Meta::Texture { width, height, mips } = *self.expect(h, "texture")? else { unreachable!() };
        if mip >= mips {
            return Err(format!("gfx.write_texture: mip {mip} out of range (texture has {mips})"));
        }
        let (lw, lh) = ((width >> mip).max(1), (height >> mip).max(1));
        if w == 0 || ht == 0 || x as u64 + w as u64 > lw as u64 || y as u64 + ht as u64 > lh as u64 {
            return Err(format!("gfx.write_texture: region {x},{y} {w}x{ht} is outside mip {mip} ({lw}x{lh})"));
        }
        if data.len() as u64 != w as u64 * ht as u64 * 4 {
            return Err(format!("gfx.write_texture: len {} must be width*height*4 = {}", data.len(), w as u64 * ht as u64 * 4));
        }
        let (Some(gpu), Obj::Texture(t, _)) = (&self.gpu, self.obj(h)?) else { return Ok(()) };
        gpu.queue.write_texture(
            wgpu::TexelCopyTextureInfo { texture: t, mip_level: mip, origin: wgpu::Origin3d { x, y, z: 0 }, aspect: wgpu::TextureAspect::All },
            data,
            wgpu::TexelCopyBufferLayout { offset: 0, bytes_per_row: Some(w * 4), rows_per_image: Some(ht) },
            wgpu::Extent3d { width: w, height: ht, depth_or_array_layers: 1 },
        );
        Ok(())
    }

    pub fn create_sampler(&mut self, json: &str) -> R<u32> {
        let d: Value = serde_json::from_str(json).map_err(|e| format!("gfx.create_sampler: invalid JSON: {e}"))?;
        let address = |k: &str| -> R<wgpu::AddressMode> {
            Ok(match d.get(k).and_then(Value::as_str).unwrap_or("clamp-to-edge") {
                "clamp-to-edge" => wgpu::AddressMode::ClampToEdge,
                "repeat" => wgpu::AddressMode::Repeat,
                "mirror-repeat" => wgpu::AddressMode::MirrorRepeat,
                o => return Err(format!("sampler: unsupported {k} {o:?}")),
            })
        };
        let linear = |k: &str| -> R<bool> {
            match d.get(k).and_then(Value::as_str).unwrap_or("nearest") {
                "nearest" => Ok(false),
                "linear" => Ok(true),
                o => Err(format!("sampler: unsupported {k} {o:?}")),
            }
        };
        let (u, v) = (address("addressModeU")?, address("addressModeV")?);
        let (mag, min, mip) = (linear("magFilter")?, linear("minFilter")?, linear("mipmapFilter")?);
        let lod_min = d.get("lodMinClamp").and_then(Value::as_f64).unwrap_or(0.0) as f32;
        let lod_max = d.get("lodMaxClamp").and_then(Value::as_f64).unwrap_or(32.0) as f32;
        if !(lod_min >= 0.0 && lod_max >= lod_min) {
            return Err(format!("sampler: lodMinClamp {lod_min} / lodMaxClamp {lod_max} must satisfy 0 <= min <= max"));
        }
        let aniso = d.get("maxAnisotropy").and_then(Value::as_u64).unwrap_or(1);
        if !(1..=16).contains(&aniso) {
            return Err(format!("sampler: maxAnisotropy {aniso} must be 1-16"));
        }
        if aniso > 1 && !(mag && min && mip) {
            return Err("sampler: maxAnisotropy > 1 needs linear magFilter, minFilter and mipmapFilter".into());
        }
        let Some(gpu) = &self.gpu else { return Ok(self.add(Obj::Null, Meta::Sampler)) };
        let f = |l: bool| if l { wgpu::FilterMode::Linear } else { wgpu::FilterMode::Nearest };
        let smp = gpu.device.create_sampler(&wgpu::SamplerDescriptor {
            label: None,
            address_mode_u: u,
            address_mode_v: v,
            address_mode_w: wgpu::AddressMode::ClampToEdge,
            mag_filter: f(mag),
            min_filter: f(min),
            mipmap_filter: if mip { wgpu::MipmapFilterMode::Linear } else { wgpu::MipmapFilterMode::Nearest },
            lod_min_clamp: lod_min,
            lod_max_clamp: lod_max,
            compare: None,
            anisotropy_clamp: aniso as u16,
            border_color: None,
        });
        Ok(self.add(Obj::Sampler(smp), Meta::Sampler))
    }

    /// Parse and validate a pipeline descriptor (both modes); with a GPU, create it.
    fn build_pipeline(&self, d: &Value, group_layouts: Option<Vec<u32>>) -> R<Option<wgpu::RenderPipeline>> {
        let surface = self.gpu.as_ref().map_or(wgpu::TextureFormat::Bgra8Unorm, |g| g.format);
        let v = d.get("vertex").ok_or("pipeline: missing vertex")?;
        self.expect(uint(v, "module")?, "shader")?;
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
                        _ => wgpu::VertexStepMode::Vertex, // other values rejected by vertex_slots
                    },
                    attributes: a,
                })
            })
            .collect();

        let mut targets = Vec::new();
        let fragment_module;
        let f = d.get("fragment");
        if let Some(f) = f {
            fragment_module = Some(uint(f, "module")?);
            self.expect(fragment_module.unwrap(), "shader")?;
            for t in f.get("targets").and_then(Value::as_array).cloned().unwrap_or_default() {
                let format = match t.get("format").and_then(Value::as_str) {
                    Some("surface") | None => surface,
                    Some(other) => return Err(format!("pipeline: color target format must be \"surface\", got {other:?}")),
                };
                let blend = match t.get("blend") {
                    Some(b) => Some(wgpu::BlendState {
                        color: blend_component(b.get("color"))?,
                        alpha: blend_component(b.get("alpha"))?,
                    }),
                    None => None,
                };
                let write_mask = match t.get("writeMask").and_then(Value::as_u64) {
                    None => wgpu::ColorWrites::ALL,
                    Some(m) if m <= 0xf => wgpu::ColorWrites::from_bits_truncate(m as u32),
                    Some(m) => return Err(format!("pipeline: writeMask {m} must be 0-15")),
                };
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
                None => None,
                Some(o) => return Err(format!("pipeline: unknown stripIndexFormat {o:?}")),
            },
            front_face: match ps("frontFace").unwrap_or("ccw") {
                "cw" => wgpu::FrontFace::Cw,
                "ccw" => wgpu::FrontFace::Ccw,
                o => return Err(format!("pipeline: unknown frontFace {o:?}")),
            },
            cull_mode: match ps("cullMode").unwrap_or("none") {
                "front" => Some(wgpu::Face::Front),
                "back" => Some(wgpu::Face::Back),
                "none" => None,
                o => return Err(format!("pipeline: unknown cullMode {o:?}")),
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
                    bias: wgpu::DepthBiasState {
                        constant: ds.get("depthBias").and_then(Value::as_i64).unwrap_or(0) as i32,
                        slope_scale: ds.get("depthBiasSlopeScale").and_then(Value::as_f64).unwrap_or(0.0) as f32,
                        clamp: ds.get("depthBiasClamp").and_then(Value::as_f64).unwrap_or(0.0) as f32,
                    },
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
        if self.gpu.is_none() {
            return Ok(None);
        }
        let vmod = self.shader(uint(v, "module")?)?;
        let fragment_module = match fragment_module {
            Some(h) => Some(self.shader(h)?),
            None => None,
        };
        let bgls = match &group_layouts {
            Some(hs) => Some(hs.iter().map(|h| self.layout(*h)).collect::<R<Vec<_>>>()?),
            None => None,
        };
        self.scoped("create_pipeline", |gpu| {
            let layout = bgls.map(|ls| {
                let refs: Vec<Option<&wgpu::BindGroupLayout>> = ls.into_iter().map(Some).collect();
                gpu.device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor { label: None, bind_group_layouts: &refs, immediate_size: 0 })
            });
            Ok(gpu.device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
                label: None,
                layout: layout.as_ref(), // None = "auto"
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
        .map(Some)
    }

    // ---- frame -----------------------------------------------------------------------------

    /// Start a frame. `show`: whether the runner will display it.
    pub fn begin_frame(&mut self, clear: [f32; 4], show: bool) -> R<bool> {
        if self.pass.is_some() {
            return Err("gfx.begin_frame called twice without end_frame".into());
        }
        self.used = true;
        self.pass = Some(Pass::default());
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
        self.frame = Some(Frame { pass, encoder, surface_texture, empty_viewport: false, empty_scissor: false });
        Ok(true)
    }

    pub fn set_pipeline(&mut self, h: u32) -> R<()> {
        let targets = self.pipeline_meta(h)?.targets;
        if targets != 1 {
            return Err(format!("gfx.set_pipeline: pipeline {h} has {targets} color targets; the render pass has one (\"surface\")"));
        }
        self.pass_mut("set_pipeline")?.pipeline = Some(h);
        if self.frame.is_some() {
            let p = self.pipeline(h)?.clone();
            if let Some(f) = &mut self.frame {
                f.pass.set_pipeline(&p);
            }
        }
        Ok(())
    }

    pub fn set_bind_group(&mut self, index: u32, h: u32) -> R<()> {
        self.set_bind_group_offsets(index, h, &[])
    }

    pub fn set_bind_group_offsets(&mut self, index: u32, h: u32, offsets: &[u32]) -> R<()> {
        if index >= MAX_BIND_GROUPS {
            return Err(format!("gfx.set_bind_group: index {index} must be below {MAX_BIND_GROUPS}"));
        }
        let Meta::BindGroup { dynamic, .. } = self.expect(h, "bind group")? else { unreachable!() };
        if offsets.len() != dynamic.len() {
            return Err(format!("gfx.set_bind_group_offsets: bind group {h} has {} dynamic entries, got {} offsets", dynamic.len(), offsets.len()));
        }
        for (o, d) in offsets.iter().zip(dynamic) {
            if o % OFFSET_ALIGNMENT != 0 {
                return Err(format!("gfx.set_bind_group_offsets: offset {o} is not a multiple of {OFFSET_ALIGNMENT}"));
            }
            if add_u64(*o as u64 + d.offset, d.size, "dynamic offset")? > d.buffer_size {
                return Err(format!("gfx.set_bind_group_offsets: offset {o} + binding {}+{} exceeds buffer size {}", d.offset, d.size, d.buffer_size));
            }
        }
        self.pass_mut("set_bind_group")?.groups[index as usize] = Some(h);
        if self.frame.is_some() {
            let bg = self.bind_group(h)?.clone();
            if let Some(f) = &mut self.frame {
                f.pass.set_bind_group(index, &bg, offsets);
            }
        }
        Ok(())
    }

    /// Clamp `[x, x+w) x [y, y+h)` to the drawable.
    fn clamp_rect(&self, x: f64, y: f64, w: f64, h: f64) -> (f64, f64, f64, f64) {
        let (dw, dh) = (self.width as f64, self.height as f64);
        let (x0, y0) = (x.clamp(0.0, dw), y.clamp(0.0, dh));
        let (x1, y1) = ((x + w).clamp(0.0, dw), (y + h).clamp(0.0, dh));
        (x0, y0, x1 - x0, y1 - y0)
    }

    pub fn set_viewport(&mut self, x: f32, y: f32, w: f32, h: f32, min_depth: f32, max_depth: f32) -> R<()> {
        if ![x, y, w, h, min_depth, max_depth].iter().all(|v| v.is_finite()) || w < 0.0 || h < 0.0 {
            return Err(format!("gfx.set_viewport: invalid rectangle {x},{y} {w}x{h}"));
        }
        if !(0.0..=1.0).contains(&min_depth) || !(0.0..=1.0).contains(&max_depth) || min_depth > max_depth {
            return Err(format!("gfx.set_viewport: depth range {min_depth}..{max_depth} must be within 0..1"));
        }
        self.pass_mut("set_viewport")?;
        let (x, y, w, h) = self.clamp_rect(x as f64, y as f64, w as f64, h as f64);
        if let Some(f) = &mut self.frame {
            f.empty_viewport = w <= 0.0 || h <= 0.0;
            if !f.empty_viewport {
                f.pass.set_viewport(x as f32, y as f32, w as f32, h as f32, min_depth, max_depth);
            }
        }
        Ok(())
    }

    pub fn set_scissor_rect(&mut self, x: u32, y: u32, w: u32, h: u32) -> R<()> {
        self.pass_mut("set_scissor_rect")?;
        let (x, y, w, h) = self.clamp_rect(x as f64, y as f64, w as f64, h as f64);
        if let Some(f) = &mut self.frame {
            f.empty_scissor = w <= 0.0 || h <= 0.0;
            if !f.empty_scissor {
                f.pass.set_scissor_rect(x as u32, y as u32, w as u32, h as u32);
            }
        }
        Ok(())
    }

    pub fn set_vertex_buffer(&mut self, slot: u32, h: u32, offset: u32) -> R<()> {
        if slot >= MAX_VERTEX_BUFFERS {
            return Err(format!("gfx.set_vertex_buffer: slot {slot} must be below {MAX_VERTEX_BUFFERS}"));
        }
        let (size, usage) = self.buffer_meta(h)?;
        if usage & BUF_VERTEX == 0 {
            return Err(format!("gfx.set_vertex_buffer: buffer {h} lacks VERTEX usage"));
        }
        if offset % 4 != 0 || offset as u64 > size {
            return Err(format!("gfx.set_vertex_buffer: offset {offset} must be a multiple of 4 within the buffer ({size} bytes)"));
        }
        self.pass_mut("set_vertex_buffer")?.vertex[slot as usize] = Some((h, offset as u64));
        if self.frame.is_some() && (offset as u64) < size {
            let b = self.buffer(h)?.clone();
            if let Some(f) = &mut self.frame {
                f.pass.set_vertex_buffer(slot, b.slice(offset as u64..));
            }
        }
        Ok(())
    }

    pub fn set_index_buffer(&mut self, h: u32, format: u32, offset: u32) -> R<()> {
        let (fmt, isize) = match format {
            0 => (wgpu::IndexFormat::Uint16, 2),
            1 => (wgpu::IndexFormat::Uint32, 4),
            f => return Err(format!("gfx.set_index_buffer: format {f} must be GASM_INDEX_U16 (0) or GASM_INDEX_U32 (1)")),
        };
        let (size, usage) = self.buffer_meta(h)?;
        if usage & BUF_INDEX == 0 {
            return Err(format!("gfx.set_index_buffer: buffer {h} lacks INDEX usage"));
        }
        if offset as u64 % isize != 0 || offset as u64 > size {
            return Err(format!("gfx.set_index_buffer: offset {offset} must be a multiple of {isize} within the buffer ({size} bytes)"));
        }
        self.pass_mut("set_index_buffer")?.index = Some((h, isize, offset as u64));
        if self.frame.is_some() && (offset as u64) < size {
            let b = self.buffer(h)?.clone();
            if let Some(f) = &mut self.frame {
                f.pass.set_index_buffer(b.slice(offset as u64..), fmt);
            }
        }
        Ok(())
    }

    /// Validate a draw against the pass state: pipeline, compatible bind groups,
    /// vertex buffers large enough (`vertices`: first + count, None for indexed
    /// draws, whose per-vertex reads aren't checked, as in WebGPU).
    fn check_draw(&self, what: &str, vertices: Option<u64>, instances: u64) -> R<()> {
        let pass = self.pass.as_ref().ok_or_else(|| format!("gfx.{what} called outside begin_frame/end_frame"))?;
        let ph = pass.pipeline.ok_or_else(|| format!("gfx.{what}: no pipeline set"))?;
        let pm = self.pipeline_meta(ph)?;
        if let Some(groups) = &pm.groups {
            for (i, entries) in groups.iter().enumerate() {
                let bg = pass.groups[i].ok_or_else(|| format!("gfx.{what}: pipeline {ph} needs a bind group at index {i}"))?;
                let Meta::BindGroup { layout, .. } = self.expect(bg, "bind group")? else { unreachable!() };
                if !matches!(layout, GroupLayout::Explicit(e) if e == entries) {
                    return Err(format!("gfx.{what}: bind group {bg} at index {i} doesn't match pipeline {ph}'s layout"));
                }
            }
        } else {
            for (i, bg) in pass.groups.iter().enumerate() {
                let Some(bg) = *bg else { continue };
                let Meta::BindGroup { layout, .. } = self.expect(bg, "bind group")? else { unreachable!() };
                match layout {
                    GroupLayout::Auto { pipeline, group } if *group == i as u32 && *pipeline != ph => {
                        return Err(format!("gfx.{what}: bind group {bg} was made for pipeline {pipeline} (\"auto\" layouts are per pipeline), not {ph}"));
                    }
                    _ => {}
                }
            }
        }
        for (slot, vs) in pm.vertex.iter().enumerate() {
            let (b, offset) = pass.vertex[slot].ok_or_else(|| format!("gfx.{what}: pipeline {ph} needs a vertex buffer in slot {slot}"))?;
            let (size, _) = self.buffer_meta(b)?;
            let count = if vs.instance { Some(instances) } else { vertices };
            if let Some(n) = count.filter(|n| *n > 0) {
                let need = (n - 1).checked_mul(vs.stride).and_then(|x| x.checked_add(vs.last)).ok_or_else(|| format!("gfx.{what}: draw range overflows"))?;
                if need > size - offset {
                    return Err(format!("gfx.{what}: vertex buffer {b} in slot {slot} has {} bytes after its offset, the draw reads {need}", size - offset));
                }
            }
        }
        Ok(())
    }

    fn skip_draw(&self) -> bool {
        self.frame.as_ref().is_none_or(|f| f.empty_viewport || f.empty_scissor)
    }

    pub fn draw(&mut self, vc: u32, ic: u32, fv: u32, fi: u32) -> R<()> {
        self.check_draw("draw", Some(fv as u64 + vc as u64), fi as u64 + ic as u64)?;
        if !self.skip_draw() {
            let f = self.frame.as_mut().unwrap();
            f.pass.draw(fv..fv.saturating_add(vc), fi..fi.saturating_add(ic));
        }
        Ok(())
    }

    pub fn draw_indexed(&mut self, ic: u32, inst: u32, first: u32, base: i32, fi: u32) -> R<()> {
        self.check_draw("draw_indexed", None, fi as u64 + inst as u64)?;
        let pass = self.pass.as_ref().unwrap();
        let (b, isize, offset) = pass.index.ok_or("gfx.draw_indexed: no index buffer set")?;
        let (size, _) = self.buffer_meta(b)?;
        let need = (first as u64 + ic as u64) * isize;
        if need > size - offset {
            return Err(format!("gfx.draw_indexed: index buffer {b} has {} bytes after its offset, the draw reads {need}", size - offset));
        }
        if !self.skip_draw() {
            let f = self.frame.as_mut().unwrap();
            f.pass.draw_indexed(first..first.saturating_add(ic), base, fi..fi.saturating_add(inst));
        }
        Ok(())
    }

    /// End the frame: submit (inside an error scope, so GPU validation errors
    /// trap) and present.
    pub fn end_frame(&mut self) -> R<()> {
        self.pass.take();
        let Some(Frame { pass, encoder, surface_texture, .. }) = self.frame.take() else { return Ok(()) };
        self.scoped("end_frame", move |gpu| {
            drop(pass);
            gpu.queue.submit([encoder.finish()]);
            if let Some(st) = surface_texture {
                gpu.queue.present(st);
            }
            Ok(())
        })
    }

    // ---- 2D path: show a video_present frame -----------------------------------------------

    /// Show an RGBA frame in the window, letterboxed and filtered (`self.present`).
    pub fn present_video(&mut self, rgba: &[u8], w: u32, h: u32, aspect: Option<(u32, u32)>) {
        let (Some(gpu), Some(Target::Window { surface, config }), Some(presenter)) = (&self.gpu, &self.target, &mut self.presenter) else { return };
        let st = match surface.get_current_texture() {
            wgpu::CurrentSurfaceTexture::Success(t) | wgpu::CurrentSurfaceTexture::Suboptimal(t) => t,
            wgpu::CurrentSurfaceTexture::Outdated | wgpu::CurrentSurfaceTexture::Lost => {
                surface.configure(&gpu.device, config);
                return;
            }
            _ => return,
        };
        let view = st.texture.create_view(&Default::default());
        let mut encoder = gpu.device.create_command_encoder(&Default::default());
        presenter.draw(gpu, &mut encoder, &view, (self.width, self.height), VideoFrame { rgba, width: w, height: h, aspect }, self.present);
        gpu.queue.submit([encoder.finish()]);
        gpu.queue.present(st);
    }

    /// Render an RGBA frame as the window would show it at `size` and read it back
    /// (headless `--screenshot-filtered`). None without a GPU.
    pub fn render_video(&mut self, rgba: &[u8], w: u32, h: u32, aspect: Option<(u32, u32)>, size: (u32, u32)) -> Option<(u32, u32, Vec<u8>)> {
        let (Some(gpu), Some(presenter)) = (&self.gpu, &mut self.presenter) else { return None };
        let texture = gpu.device.create_texture(&wgpu::TextureDescriptor {
            label: Some("filtered"),
            size: wgpu::Extent3d { width: size.0, height: size.1, depth_or_array_layers: 1 },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: gpu.format,
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::COPY_SRC,
            view_formats: &[],
        });
        let mut encoder = gpu.device.create_command_encoder(&Default::default());
        presenter.draw(gpu, &mut encoder, &texture.create_view(&Default::default()), size, VideoFrame { rgba, width: w, height: h, aspect }, self.present);
        gpu.queue.submit([encoder.finish()]);
        read_texture(gpu, &texture, size.0, size.1).map(|px| (size.0, size.1, px))
    }

    /// Read back the offscreen target as tightly packed RGBA8 (headless screenshots).
    pub fn read_offscreen(&self) -> Option<(u32, u32, Vec<u8>)> {
        let (Some(gpu), Some(Target::Offscreen { texture })) = (&self.gpu, &self.target) else { return None };
        read_texture(gpu, texture, self.width, self.height).map(|px| (self.width, self.height, px))
    }
}

/// Copy a `w×h` 4-byte-per-texel texture back to the CPU, rows tightly packed.
fn read_texture(gpu: &Gpu, texture: &wgpu::Texture, w: u32, h: u32) -> Option<Vec<u8>> {
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
    // a BGRA surface format (window targets) reads back as BGRA
    if matches!(gpu.format, wgpu::TextureFormat::Bgra8Unorm | wgpu::TextureFormat::Bgra8UnormSrgb) {
        out.chunks_exact_mut(4).for_each(|p| p.swap(0, 2));
    }
    Some(out)
}

#[cfg(feature = "window")]
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

#[cfg(feature = "window")]
/// Prefer a non-sRGB 8-bit format: browsers' canvas formats are non-sRGB, so
/// guests look the same on both runners.
fn pick_format(formats: &[wgpu::TextureFormat]) -> wgpu::TextureFormat {
    use wgpu::TextureFormat as F;
    [F::Bgra8Unorm, F::Rgba8Unorm].into_iter().find(|f| formats.contains(f)).unwrap_or(formats[0])
}

fn uint(v: &Value, key: &str) -> R<u32> {
    v.get(key).and_then(Value::as_u64).and_then(|n| u32::try_from(n).ok()).ok_or_else(|| format!("descriptor: missing number {key:?}"))
}

/// An object handle in a descriptor.
fn handle(v: &Value) -> R<u32> {
    v.as_u64().and_then(|n| u32::try_from(n).ok()).ok_or_else(|| format!("descriptor: {v} is not a handle"))
}

/// Byte size of a vertex format.
fn vertex_format_size(s: &str) -> R<u64> {
    Ok(match s {
        "float32" | "uint32" | "sint32" | "uint8x4" | "unorm8x4" | "uint16x2" | "float16x2" => 4,
        "float32x2" | "uint32x2" | "sint32x2" | "uint16x4" | "float16x4" => 8,
        "float32x3" | "uint32x3" | "sint32x3" => 12,
        "float32x4" | "uint32x4" | "sint32x4" => 16,
        o => return Err(format!("unsupported vertex format {o:?}")),
    })
}

/// The vertex buffers of a pipeline's `vertex` stage, validated like WebGPU's
/// createRenderPipeline (strides, attribute offsets and locations).
fn vertex_slots(v: &Value) -> R<Vec<VertexSlot>> {
    let buffers = v.get("buffers").and_then(Value::as_array).cloned().unwrap_or_default();
    if buffers.len() > MAX_VERTEX_BUFFERS as usize {
        return Err(format!("pipeline: at most {MAX_VERTEX_BUFFERS} vertex buffers, got {}", buffers.len()));
    }
    let mut locations = Vec::new();
    let mut out = Vec::new();
    for (i, b) in buffers.iter().enumerate() {
        let stride = b.get("arrayStride").and_then(Value::as_u64).unwrap_or(0);
        if stride % 4 != 0 || stride > MAX_VERTEX_STRIDE {
            return Err(format!("pipeline: vertex buffer {i}: arrayStride {stride} must be a multiple of 4, at most {MAX_VERTEX_STRIDE}"));
        }
        let instance = match b.get("stepMode").and_then(Value::as_str).unwrap_or("vertex") {
            "vertex" => false,
            "instance" => true,
            o => return Err(format!("pipeline: vertex buffer {i}: unknown stepMode {o:?}")),
        };
        let mut last = 0;
        for a in b.get("attributes").and_then(Value::as_array).cloned().unwrap_or_default() {
            let size = vertex_format_size(string(&a, "format")?)?;
            let offset = a.get("offset").and_then(Value::as_u64).unwrap_or(0);
            let location = uint(&a, "shaderLocation")?;
            if offset % size.min(4) != 0 || (stride > 0 && offset + size > stride) || offset + size > MAX_VERTEX_STRIDE {
                return Err(format!("pipeline: vertex buffer {i}: attribute at offset {offset} doesn't fit (arrayStride {stride})"));
            }
            if location as usize >= MAX_VERTEX_ATTRIBUTES || locations.contains(&location) {
                return Err(format!("pipeline: shaderLocation {location} is used twice or is not below {MAX_VERTEX_ATTRIBUTES}"));
            }
            locations.push(location);
            last = last.max(offset + size);
        }
        out.push(VertexSlot { stride, instance, last });
    }
    Ok(out)
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

#[cfg(test)]
mod tests {
    use super::*;

    /// One call of tests/gfx-cases.json on `g`.
    fn call(g: &mut Gfx, c: &[Value], shader: &str) -> R<()> {
        let n = |i: usize| c[i].as_u64().unwrap() as u32;
        let json = |i: usize| c[i].to_string();
        match c[0].as_str().unwrap() {
            "create_shader" => g.create_shader(shader).map(drop),
            "create_buffer" => g.create_buffer(n(1), n(2)).map(drop),
            "create_pipeline" => g.create_pipeline(&json(1)).map(drop),
            "create_bind_group" => g.create_bind_group(&json(1)).map(drop),
            "create_bind_group_layout" => g.create_bind_group_layout(&json(1)).map(drop),
            "create_texture" => g.create_texture(&json(1)).map(drop),
            "write_buffer" => g.write_buffer(n(1), n(2), &vec![0; n(3) as usize]),
            "write_texture" => g.write_texture(n(1), n(2), n(3), n(4), n(5), n(6), &vec![0; n(7) as usize]),
            "begin_frame" => g.begin_frame([0.0; 4], false).map(drop),
            "set_pipeline" => g.set_pipeline(n(1)),
            "set_bind_group" => g.set_bind_group(n(1), n(2)),
            "set_vertex_buffer" => g.set_vertex_buffer(n(1), n(2), n(3)),
            "set_index_buffer" => g.set_index_buffer(n(1), n(2), n(3)),
            "draw" => g.draw(n(1), n(2), n(3), n(4)),
            "draw_indexed" => g.draw_indexed(n(1), n(2), n(3), n(4) as i32, n(5)),
            "end_frame" => g.end_frame(),
            "destroy" => g.destroy(n(1)),
            other => panic!("unknown call {other}"),
        }
    }

    /// The validation cases shared with the JS runner (scripts/gfx-model-test.mjs).
    #[test]
    fn shared_validation_cases() {
        let path = concat!(env!("CARGO_MANIFEST_DIR"), "/../../tests/gfx-cases.json");
        let doc: Value = serde_json::from_str(&std::fs::read_to_string(path).unwrap()).unwrap();
        let shader = doc["shader"].as_str().unwrap();
        for case in doc["cases"].as_array().unwrap() {
            let name = case["name"].as_str().unwrap();
            let mut g = Gfx::null();
            let mut failed = None;
            for (i, c) in case["calls"].as_array().unwrap().iter().enumerate() {
                if let Err(e) = call(&mut g, c.as_array().unwrap(), shader) {
                    failed = Some((i, e));
                    break;
                }
            }
            let want = case["fails"].as_u64().map(|i| i as usize);
            assert_eq!(failed.as_ref().map(|f| f.0), want, "{name}: {failed:?}");
        }
    }
}
