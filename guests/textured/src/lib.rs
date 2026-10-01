//! gasm:gfx beyond buffers: textures and samplers, explicit bind group layouts,
//! dynamic uniform offsets, a storage buffer with instancing, depth bias, and a
//! 4:3 viewport + scissor inside whatever the drawable is.
//!
//! - One 256x256 texture with a full mip chain (built here, box filter); a 64x64
//!   region of mip 0 is rewritten every frame.
//! - Two samplers: repeat/linear (left half of the big quad) and clamp/nearest (right half).
//! - One uniform buffer, one bind group, 256-byte slots picked with dynamic offsets.
//! - One texture bind group shared by three pipelines (alpha, additive, instanced).
//! - Typed text (`text_input`) tints the big quad; the mouse wheel zooms it and
//!   dragging with the left button turns it (raw pointer).

use gasm::gfx::{self, BindGroup, Buffer, IndexFormat, Pipeline, Texture};

const TEX: u32 = 256;
const MIPS: u32 = 9; // 256 .. 1
const PATCH: u32 = 64;
const SLOT: u32 = 256; // uniform slot stride (dynamic offset alignment)
const ORBITERS: u32 = 12;
const SLOTS: u32 = 2 + ORBITERS; // background, big quad, orbiters
const SPRITES: u32 = 16;

const SHADER: &str = r#"
struct Obj { mvp: mat4x4<f32>, tint: vec4<f32>, uv: vec4<f32> };  // uv: repeat scale xy, clamp zoom z, repeat-only w
@group(0) @binding(0) var<uniform> obj: Obj;
@group(1) @binding(0) var tex: texture_2d<f32>;
@group(1) @binding(1) var s_repeat: sampler;
@group(1) @binding(2) var s_clamp: sampler;
struct VO { @builtin(position) pos: vec4<f32>, @location(0) uv: vec2<f32> };
@vertex fn vs(@location(0) p: vec2<f32>, @location(1) uv: vec2<f32>) -> VO {
  return VO(obj.mvp * vec4<f32>(p, 0.0, 1.0), uv);
}
@fragment fn fs(i: VO) -> @location(0) vec4<f32> {
  let a = textureSample(tex, s_repeat, i.uv * obj.uv.xy);
  let b = textureSample(tex, s_clamp, (i.uv - vec2<f32>(0.5)) * obj.uv.z + vec2<f32>(0.5));
  return select(b, a, i.uv.x < 0.5 || obj.uv.w > 0.5) * obj.tint;  // uv.w = 1: repeat only
}
"#;

const SPRITE_SHADER: &str = r#"
@group(0) @binding(0) var<storage, read> inst: array<vec4<f32>>;  // x, y, size, alpha (viewport NDC)
@group(1) @binding(0) var tex: texture_2d<f32>;
@group(1) @binding(1) var s_repeat: sampler;
struct VO { @builtin(position) pos: vec4<f32>, @location(0) uv: vec2<f32>, @location(1) alpha: f32 };
@vertex fn vs(@location(0) p: vec2<f32>, @location(1) uv: vec2<f32>, @builtin(instance_index) k: u32) -> VO {
  let d = inst[k];
  return VO(vec4<f32>(d.xy + p * vec2<f32>(d.z * 0.75, d.z), 0.2, 1.0), uv, d.w);
}
@fragment fn fs(i: VO) -> @location(0) vec4<f32> {
  let c = textureSample(tex, s_repeat, i.uv);
  return vec4<f32>(c.rgb, c.a * i.alpha);
}
"#;

struct Textured {
    frame: u32,
    alpha: Pipeline,
    additive: Pipeline,
    sprites: Pipeline,
    quad: Buffer,
    indices: Buffer,
    uniforms: Buffer,
    instances: Buffer,
    texture: Texture,
    obj_group: BindGroup,
    tex_group: BindGroup,
    inst_group: BindGroup,
    typed: String,
    zoom: f32,
    turn: f32,
}

type Mat = [f32; 16];

/// Column-major: scale by `k`, rotate by `a`, move to (tx, ty) at depth z, then squeeze
/// x by 3/4 so shapes keep their proportions in the 4:3 viewport.
fn object(a: f32, k: f32, tx: f32, ty: f32, z: f32) -> Mat {
    let (s, c) = a.sin_cos();
    let ax = 0.75;
    [ax * c * k, s * k, 0.0, 0.0, -ax * s * k, c * k, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0, ax * tx, ty, z, 1.0]
}

/// Fills the viewport at depth z.
fn full(z: f32) -> Mat {
    [1.0, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, z, 1.0]
}

/// The texture: tiles of colour with a soft round alpha mask, plus a thin grid.
fn base_image() -> Vec<u8> {
    let mut img = vec![0u8; (TEX * TEX * 4) as usize];
    for y in 0..TEX {
        for x in 0..TEX {
            let i = ((y * TEX + x) * 4) as usize;
            let tile = ((x / 32) + (y / 32)) % 2;
            let (fx, fy) = (x as f32 / TEX as f32 - 0.5, y as f32 / TEX as f32 - 0.5);
            let r2 = fx * fx + fy * fy;
            let grid = x % 32 == 0 || y % 32 == 0;
            img[i] = if grid { 255 } else if tile == 0 { 230 } else { 40 + (x / 2) as u8 };
            img[i + 1] = if grid { 255 } else if tile == 0 { 120 + (y / 4) as u8 } else { 60 };
            img[i + 2] = if grid { 255 } else if tile == 0 { 40 } else { 200 };
            img[i + 3] = if r2 < 0.2 { 255 } else if r2 < 0.25 { ((0.25 - r2) / 0.05 * 255.0) as u8 } else { 0 };
        }
    }
    img
}

/// Next mip level: average of 2x2 blocks.
fn half(src: &[u8], w: u32) -> Vec<u8> {
    let n = (w / 2).max(1);
    let mut out = vec![0u8; (n * n * 4) as usize];
    for y in 0..n {
        for x in 0..n {
            for c in 0..4 {
                let at = |xx: u32, yy: u32| src[((yy.min(w - 1) * w + xx.min(w - 1)) * 4 + c) as usize] as u32;
                let s = at(2 * x, 2 * y) + at(2 * x + 1, 2 * y) + at(2 * x, 2 * y + 1) + at(2 * x + 1, 2 * y + 1);
                out[((y * n + x) * 4 + c) as usize] = (s / 4) as u8;
            }
        }
    }
    out
}

fn patch(frame: u32) -> Vec<u8> {
    let mut p = vec![0u8; (PATCH * PATCH * 4) as usize];
    for y in 0..PATCH {
        for x in 0..PATCH {
            let i = ((y * PATCH + x) * 4) as usize;
            let v = ((x + frame) ^ (y + frame / 2)) as u8;
            p[i] = v;
            p[i + 1] = 255 - v;
            p[i + 2] = (x * 4) as u8;
            p[i + 3] = 255;
        }
    }
    p
}

impl gasm::Game for Textured {
    fn init() -> Result<Self, String> {
        gasm::set_frame_rate(60.0);
        let module = gfx::create_shader(SHADER);
        let sprite_module = gfx::create_shader(SPRITE_SHADER);

        let obj_layout = gfx::create_bind_group_layout(&format!(
            r#"{{"entries":[{{"binding":0,"visibility":{v},"buffer":{{"type":"uniform","hasDynamicOffset":true,"minBindingSize":96}}}}]}}"#,
            v = gfx::STAGE_VERTEX | gfx::STAGE_FRAGMENT
        ));
        let tex_layout = gfx::create_bind_group_layout(&format!(
            r#"{{"entries":[{{"binding":0,"visibility":{f},"texture":{{"sampleType":"float"}}}},
                {{"binding":1,"visibility":{f},"sampler":{{"type":"filtering"}}}},
                {{"binding":2,"visibility":{f},"sampler":{{"type":"filtering"}}}}]}}"#,
            f = gfx::STAGE_FRAGMENT
        ));
        let inst_layout = gfx::create_bind_group_layout(&format!(
            r#"{{"entries":[{{"binding":0,"visibility":{v},"buffer":{{"type":"read-only-storage"}}}}]}}"#,
            v = gfx::STAGE_VERTEX
        ));

        let vertex = |m: u32| {
            format!(
                r#"{{"module":{m},"entryPoint":"vs","buffers":[{{"arrayStride":16,"attributes":[
                {{"format":"float32x2","offset":0,"shaderLocation":0}},{{"format":"float32x2","offset":8,"shaderLocation":1}}]}}]}}"#
            )
        };
        let pipeline = |m: u32, layouts: [u32; 2], blend: &str, depth: &str| {
            gfx::create_pipeline(&format!(
                r#"{{"layout":[{l0},{l1}],"vertex":{v},
                  "fragment":{{"module":{m},"entryPoint":"fs","targets":[{{"format":"surface","blend":{blend}}}]}},
                  "primitive":{{"topology":"triangle-list"}},"depthStencil":{depth}}}"#,
                l0 = layouts[0], l1 = layouts[1], v = vertex(m)
            ))
        };
        let depth = r#"{"format":"depth24plus","depthWriteEnabled":false,"depthCompare":"less-equal"}"#;
        let alpha_blend = r#"{"color":{"srcFactor":"src-alpha","dstFactor":"one-minus-src-alpha"},"alpha":{"srcFactor":"one","dstFactor":"one-minus-src-alpha"}}"#;
        let add_blend = r#"{"color":{"srcFactor":"src-alpha","dstFactor":"one"},"alpha":{"srcFactor":"one","dstFactor":"one"}}"#;
        let alpha = pipeline(module.0, [obj_layout.0, tex_layout.0], alpha_blend, depth);
        let additive = pipeline(
            module.0,
            [obj_layout.0, tex_layout.0],
            add_blend,
            r#"{"format":"depth24plus","depthWriteEnabled":false,"depthCompare":"less-equal","depthBias":-2,"depthBiasSlopeScale":-1.0,"depthBiasClamp":0.0}"#,
        );
        let sprites = pipeline(sprite_module.0, [inst_layout.0, tex_layout.0], alpha_blend, depth);

        // unit quad: position (x, y), uv
        let quad_data: [f32; 16] = [-1.0, -1.0, 0.0, 1.0, 1.0, -1.0, 1.0, 1.0, 1.0, 1.0, 1.0, 0.0, -1.0, 1.0, 0.0, 0.0];
        let quad = gfx::create_buffer(64, gfx::VERTEX);
        gfx::write_buffer(quad, 0, &quad_data);
        let indices = gfx::create_buffer(12, gfx::INDEX);
        gfx::write_buffer(indices, 0, &[0u16, 1, 2, 0, 2, 3]);

        let uniforms = gfx::create_buffer(SLOTS * SLOT, gfx::UNIFORM);
        let instances = gfx::create_buffer(SPRITES * 16, gfx::STORAGE);

        let texture = gfx::create_texture(&format!(r#"{{"size":[{TEX},{TEX}],"format":"rgba8unorm","mipLevelCount":{MIPS}}}"#));
        let mut level = base_image();
        for mip in 0..MIPS {
            let w = (TEX >> mip).max(1);
            gfx::write_texture(texture, mip, 0, 0, w, w, &level);
            if mip + 1 < MIPS {
                level = half(&level, w);
            }
        }
        let repeat = gfx::create_sampler(
            r#"{"addressModeU":"repeat","addressModeV":"repeat","magFilter":"linear","minFilter":"linear","mipmapFilter":"linear","maxAnisotropy":4}"#,
        );
        let clamp = gfx::create_sampler(r#"{"addressModeU":"clamp-to-edge","addressModeV":"clamp-to-edge","magFilter":"nearest","minFilter":"nearest"}"#);

        let obj_group = gfx::create_bind_group(&format!(
            r#"{{"layout":{l},"entries":[{{"binding":0,"buffer":{b},"offset":0,"size":96}}]}}"#,
            l = obj_layout.0, b = uniforms.0
        ));
        let tex_group = gfx::create_bind_group(&format!(
            r#"{{"layout":{l},"entries":[{{"binding":0,"texture":{t}}},{{"binding":1,"sampler":{r}}},{{"binding":2,"sampler":{c}}}]}}"#,
            l = tex_layout.0, t = texture.0, r = repeat.0, c = clamp.0
        ));
        let inst_group = gfx::create_bind_group(&format!(
            r#"{{"layout":{l},"entries":[{{"binding":0,"buffer":{b}}}]}}"#,
            l = inst_layout.0, b = instances.0
        ));
        Ok(Textured {
            frame: 0, alpha, additive, sprites, quad, indices, uniforms, instances, texture,
            obj_group, tex_group, inst_group, typed: String::new(), zoom: 1.0, turn: 0.0,
        })
    }

    fn frame(&mut self) {
        let t = self.frame as f32 / 60.0;
        if let Some(text) = gasm::text_input() {
            for ch in text.chars() {
                match ch {
                    '\n' => self.typed.clear(),
                    '\u{8}' => {
                        self.typed.pop();
                    }
                    c if self.typed.len() < 16 => self.typed.push(c),
                    _ => {}
                }
            }
            if !text.is_empty() {
                gasm::log!("typed {:?} -> {:?}", text, self.typed);
            }
        }
        if let Some(p) = gasm::input::pointer() {
            self.zoom = (self.zoom * (1.0 - p.wheel_y * 0.1)).clamp(0.3, 2.0);
            if p.buttons & gasm::input::MOUSE_LEFT != 0 {
                self.turn += p.dx * 0.01;
            }
        }
        // tint from the typed text (white if none)
        let h = self.typed.bytes().fold(0u32, |h, b| h.wrapping_mul(31).wrapping_add(b as u32));
        let tint = if self.typed.is_empty() {
            [1.0, 1.0, 1.0, 1.0]
        } else {
            [0.4 + (h % 7) as f32 / 10.0, 0.4 + (h / 7 % 7) as f32 / 10.0, 0.4 + (h / 49 % 7) as f32 / 10.0, 1.0]
        };

        // uniforms: one 256-byte slot per object (mvp, tint, uv params)
        let mut u = vec![0f32; (SLOTS * SLOT / 4) as usize];
        let mut slot = |i: u32, m: Mat, tint: [f32; 4], uv: [f32; 4]| {
            let o = (i * SLOT / 4) as usize;
            u[o..o + 16].copy_from_slice(&m);
            u[o + 16..o + 20].copy_from_slice(&tint);
            u[o + 20..o + 24].copy_from_slice(&uv);
        };
        slot(0, full(0.9), [0.35, 0.35, 0.45, 1.0], [8.0, 6.0, 1.0, 1.0]);
        slot(1, object(t * 0.5 + self.turn, 0.45 * self.zoom, 0.0, 0.0, 0.5), tint, [2.0, 2.0, 1.4, 0.0]);
        for k in 0..ORBITERS {
            let a = t + k as f32 * std::f32::consts::TAU / ORBITERS as f32;
            let (s, c) = a.sin_cos();
            slot(2 + k, object(-a * 2.0, 0.08, c * 0.7, s * 0.7, 0.5), [0.9, 0.6, 0.3, 0.8], [1.0, 1.0, 1.0, 0.0]);
        }
        gfx::write_buffer(self.uniforms, 0, &u);

        let mut inst = [0f32; (SPRITES * 4) as usize];
        for k in 0..SPRITES {
            let x = -0.9 + k as f32 * 1.8 / (SPRITES - 1) as f32;
            let i = (k * 4) as usize;
            inst[i..i + 4].copy_from_slice(&[x, -0.85 + 0.05 * (t * 3.0 + k as f32).sin(), 0.05, 0.4 + 0.6 * k as f32 / SPRITES as f32]);
        }
        gfx::write_buffer(self.instances, 0, &inst);
        gfx::write_texture(self.texture, 0, 96, 96, PATCH, PATCH, &patch(self.frame));

        if gfx::begin_frame([0.12, 0.02, 0.03, 1.0]) {
            // 4:3, centred: pillar- or letterboxed inside any drawable
            let (w, h) = gfx::size();
            let (vw, vh) = if w * 3 > h * 4 { (h * 4 / 3, h) } else { (w, w * 3 / 4) };
            let (vx, vy) = ((w - vw) / 2, (h - vh) / 2);
            gfx::set_viewport(vx as f32, vy as f32, vw as f32, vh as f32, 0.0, 1.0);
            gfx::set_scissor_rect(vx, vy, vw, vh);

            gfx::set_vertex_buffer(0, self.quad, 0);
            gfx::set_index_buffer(self.indices, IndexFormat::U16, 0);
            gfx::set_pipeline(self.alpha);
            gfx::set_bind_group(1, self.tex_group);
            for i in 0..2 {
                gfx::set_bind_group_offsets(0, self.obj_group, &[i * SLOT]);
                gfx::draw_indexed(6, 1, 0, 0, 0);
            }
            gfx::set_pipeline(self.additive);
            gfx::set_bind_group(1, self.tex_group);
            for i in 2..SLOTS {
                gfx::set_bind_group_offsets(0, self.obj_group, &[i * SLOT]);
                gfx::draw_indexed(6, 1, 0, 0, 0);
            }
            gfx::set_pipeline(self.sprites);
            gfx::set_bind_group(0, self.inst_group);
            gfx::set_bind_group(1, self.tex_group);
            gfx::draw_indexed(6, SPRITES, 0, 0, 0);
        }
        gfx::end_frame();
        self.frame += 1;
    }
}

gasm::game!(Textured);
