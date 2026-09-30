//! Rendering through gasm:gfx (WebGPU subset). Reads the Sim, never writes it.

use std::f32::consts::PI;

use gasm::gfx::{self, BindGroup, Buffer, IndexFormat, Pipeline};

use crate::math::{Mat4, Vec3};
use crate::sim::{self, Phase, Sim, ARENA_R, BALL_R};

const MAX_OBJECTS: usize = 48;
const SLOT: usize = 256; // uniform slot stride (minUniformBufferOffsetAlignment)

/// Must match `struct U` in the shaders.
#[derive(Clone, Copy)]
#[repr(C)]
struct Uniforms {
    mvp: Mat4,
    model: Mat4,
    color: [f32; 4],
    eye: [f32; 4],
}

const LIT_SHADER: &str = r#"
struct U { mvp: mat4x4<f32>, model: mat4x4<f32>, color: vec4<f32>, eye: vec4<f32> };
@group(0) @binding(0) var<uniform> u: U;
struct VO { @builtin(position) pos: vec4<f32>, @location(0) n: vec3<f32>, @location(1) wp: vec3<f32> };
@vertex fn vs(@location(0) p: vec3<f32>, @location(1) n: vec3<f32>) -> VO {
  var o: VO;
  o.pos = u.mvp * vec4<f32>(p, 1.0);
  o.n = (u.model * vec4<f32>(n, 0.0)).xyz;
  o.wp = (u.model * vec4<f32>(p, 1.0)).xyz;
  return o;
}
@fragment fn fs(i: VO) -> @location(0) vec4<f32> {
  let n = normalize(i.n);
  let l = normalize(vec3<f32>(0.35, 1.0, 0.45));
  let v = normalize(u.eye.xyz - i.wp);
  let diff = max(dot(n, l), 0.0);
  let spec = pow(max(dot(n, normalize(l + v)), 0.0), 48.0) * 0.35;
  let rim = pow(1.0 - max(dot(n, v), 0.0), 3.0) * 0.30;
  let fog = clamp((length(u.eye.xyz - i.wp) - 18.0) / 40.0, 0.0, 1.0);
  let c = u.color.rgb * (0.28 + 0.8 * diff) + vec3<f32>(spec + rim);
  return vec4<f32>(mix(c, vec3<f32>(0.03, 0.04, 0.08), fog), u.color.a);
}
"#;

/// Unlit, alpha-blended (blob shadows).
const FLAT_SHADER: &str = r#"
struct U { mvp: mat4x4<f32>, model: mat4x4<f32>, color: vec4<f32>, eye: vec4<f32> };
@group(0) @binding(0) var<uniform> u: U;
@vertex fn vs(@location(0) p: vec3<f32>, @location(1) n: vec3<f32>) -> @builtin(position) vec4<f32> {
  return u.mvp * vec4<f32>(p, 1.0);
}
@fragment fn fs() -> @location(0) vec4<f32> { return u.color; }
"#;

#[derive(Clone, Copy)]
struct Mesh {
    vbuf: Buffer,
    ibuf: Buffer,
    count: u32,
}

#[derive(Clone, Copy)]
struct Draw {
    mesh: Mesh,
    pipe: Pipeline,
}

pub struct Renderer {
    lit: Pipeline,
    flat: Pipeline,
    ubuf: Buffer,
    /// One bind group per (pipeline, slot): "auto" layouts are pipeline-exclusive in WebGPU.
    bind_lit: Vec<BindGroup>,
    bind_flat: Vec<BindGroup>,
    sphere: Mesh,
    cylinder: Mesh,
    disc: Mesh,
    uniforms: Vec<u8>,
    draws: Vec<Draw>,
    viewproj: Mat4,
    eye: Vec3,
}

const COLORS: [[f32; 3]; 2] = [[0.95, 0.25, 0.22], [0.22, 0.50, 0.98]];

impl Renderer {
    pub fn new() -> Renderer {
        let lit = pipeline(gfx::create_shader(LIT_SHADER).0, false);
        let flat = pipeline(gfx::create_shader(FLAT_SHADER).0, true);
        let ubuf = gfx::create_buffer((MAX_OBJECTS * SLOT) as u32, gfx::UNIFORM | gfx::COPY_DST);
        let binds = |p: Pipeline| -> Vec<BindGroup> {
            (0..MAX_OBJECTS)
                .map(|i| {
                    gfx::create_bind_group(&format!(
                        r#"{{"pipeline":{},"group":0,"entries":[{{"binding":0,"buffer":{},"offset":{},"size":{}}}]}}"#,
                        p.0,
                        ubuf.0,
                        i * SLOT,
                        std::mem::size_of::<Uniforms>()
                    ))
                })
                .collect()
        };
        let (bind_lit, bind_flat) = (binds(lit), binds(flat));
        Renderer {
            lit,
            flat,
            ubuf,
            bind_lit,
            bind_flat,
            sphere: sphere(32, 20),
            cylinder: cylinder(72),
            disc: disc(40),
            uniforms: vec![0; MAX_OBJECTS * SLOT],
            draws: Vec::with_capacity(MAX_OBJECTS),
            viewproj: Mat4([0.0; 16]),
            eye: Vec3(0.0, 0.0, 0.0),
        }
    }

    fn object(&mut self, mesh: Mesh, pipe: Pipeline, model: Mat4, color: [f32; 4]) {
        let i = self.draws.len();
        if i >= MAX_OBJECTS {
            return;
        }
        let u = Uniforms {
            mvp: self.viewproj.mul(&model),
            model,
            color,
            eye: [self.eye.0, self.eye.1, self.eye.2, 1.0],
        };
        let bytes = unsafe {
            std::slice::from_raw_parts(&u as *const Uniforms as *const u8, std::mem::size_of::<Uniforms>())
        };
        self.uniforms[i * SLOT..i * SLOT + bytes.len()].copy_from_slice(bytes);
        self.draws.push(Draw { mesh, pipe });
    }

    fn lit(&mut self, mesh: Mesh, model: Mat4, rgb: [f32; 3]) {
        self.object(mesh, self.lit, model, [rgb[0], rgb[1], rgb[2], 1.0]);
    }

    /// `local`: player on this machine (camera side). `waiting`: no opponent yet.
    pub fn frame(&mut self, s: &Sim, local: usize, waiting: bool, t: f32) {
        let (w, h) = gfx::size();
        if w == 0 || h == 0 {
            return;
        }
        // Camera behind the local player's side of the arena, looking at the center.
        let side = if local == 1 { -1.0 } else { 1.0 };
        self.eye = Vec3(0.0, 15.0, 17.0 * side);
        self.viewproj = Mat4::perspective(0.85, w as f32 / h as f32, 0.5, 120.0)
            .mul(&Mat4::look_at(self.eye, Vec3(0.0, 0.6, -0.8 * side), Vec3(0.0, 1.0, 0.0)));
        self.draws.clear();
        let (sphere, cylinder, disc) = (self.sphere, self.cylinder, self.disc);

        // arena: rim, inner surface, center ring, and a pillar into the void
        self.lit(cylinder, Mat4::trs(Vec3(0.0, 0.0, 0.0), 0.0, Vec3(ARENA_R, 1.2, ARENA_R)), [0.55, 0.42, 0.30]);
        let inner = ARENA_R - 0.45;
        self.lit(disc, Mat4::trs(Vec3(0.0, 0.01, 0.0), 0.0, Vec3(inner, 1.0, inner)), [0.86, 0.78, 0.60]);
        self.lit(disc, Mat4::trs(Vec3(0.0, 0.015, 0.0), 0.0, Vec3(2.2, 1.0, 2.2)), [0.80, 0.70, 0.52]);
        self.lit(cylinder, Mat4::trs(Vec3(0.0, -1.2, 0.0), 0.0, Vec3(3.0, 30.0, 3.0)), [0.30, 0.24, 0.20]);

        // players
        for i in 0..2 {
            let p = s.p[i];
            if waiting && i != local {
                continue;
            }
            let squash = if p.dash_t > 0 { 0.85 } else { 1.0 };
            let bob = if s.phase == Phase::RoundOver && s.round_winner == i as i32 {
                (t * 9.0).sin().abs() * 0.8
            } else {
                0.0
            };
            let pulse = if waiting { 0.75 + 0.25 * (t * 4.0).sin() } else { 1.0 };
            let c = COLORS[i].map(|v| v * pulse);
            self.lit(sphere, Mat4::trs(Vec3(p.x, p.y + BALL_R + bob, p.z), 0.0, Vec3(1.0 / squash, squash, 1.0 / squash)), c);
            // dash-ready indicator: small orb above the ball
            if s.phase == Phase::Fight && p.dash_cd == 0 && p.falling == 0 {
                self.lit(sphere, Mat4::trs(Vec3(p.x, p.y + 2.45 + bob, p.z), 0.0, Vec3(0.18, 0.18, 0.18)), [1.0, 1.0, 0.85]);
            }
        }

        // score pips floating behind the arena: red on the left, blue on the right (from either camera)
        for i in 0..2 {
            for k in 0..sim::WIN_SCORE {
                let x = if i == 0 { -1.0 } else { 1.0 } * (1.6 + k as f32);
                let lit = if k < s.score[i] { 1.0 } else { 0.22 };
                let pip = Mat4::trs(Vec3(x * side, 2.2, -(ARENA_R + 1.6) * side), 0.0, Vec3(0.38, 0.38, 0.38));
                self.lit(sphere, pip, COLORS[i].map(|v| v * lit));
            }
        }

        // countdown: three orbs over the center that go out one by one
        if s.phase == Phase::Countdown && !waiting {
            let left = 3 - s.phase_t * 3 / sim::COUNTDOWN_FRAMES;
            for k in 0..left {
                let orb = Mat4::trs(Vec3((k as f32 - 1.0) * 1.2, 4.0, 0.0), 0.0, Vec3(0.4, 0.4, 0.4));
                self.lit(sphere, orb, [1.0, 0.85, 0.25]);
            }
        }

        // blob shadows on the platform
        for i in 0..2 {
            let p = s.p[i];
            if (waiting && i != local) || p.x * p.x + p.z * p.z > ARENA_R * ARENA_R || p.y < 0.0 {
                continue;
            }
            let k = 1.0 / (1.0 + p.y * 0.3);
            let shadow = Mat4::trs(Vec3(p.x, 0.03, p.z), 0.0, Vec3(0.95 * k, 1.0, 0.95 * k));
            self.object(disc, self.flat, shadow, [0.0, 0.0, 0.0, 0.40 * k]);
        }

        gfx::write_buffer(self.ubuf, 0, &self.uniforms[..self.draws.len() * SLOT]);
        if gfx::begin_frame([0.03, 0.04, 0.08, 1.0]) {
            let mut current = None;
            for (i, d) in self.draws.iter().enumerate() {
                if current != Some(d.pipe) {
                    gfx::set_pipeline(d.pipe);
                    current = Some(d.pipe);
                }
                let bind = if d.pipe == self.lit { &self.bind_lit } else { &self.bind_flat };
                gfx::set_bind_group(0, bind[i]);
                gfx::set_vertex_buffer(0, d.mesh.vbuf, 0);
                gfx::set_index_buffer(d.mesh.ibuf, IndexFormat::U16, 0);
                gfx::draw_indexed(d.mesh.count, 1, 0, 0, 0);
            }
        }
        gfx::end_frame();
    }
}

fn pipeline(shader: u32, blend: bool) -> Pipeline {
    let blend_json = if blend {
        r#","blend":{"color":{"srcFactor":"src-alpha","dstFactor":"one-minus-src-alpha","operation":"add"},"alpha":{"srcFactor":"one","dstFactor":"one-minus-src-alpha","operation":"add"}}"#
    } else {
        ""
    };
    gfx::create_pipeline(&format!(
        r#"{{"vertex":{{"module":{shader},"entryPoint":"vs","buffers":[{{"arrayStride":24,"attributes":[{{"format":"float32x3","offset":0,"shaderLocation":0}},{{"format":"float32x3","offset":12,"shaderLocation":1}}]}}]}},"fragment":{{"module":{shader},"entryPoint":"fs","targets":[{{"format":"surface"{blend_json}}}]}},"primitive":{{"topology":"triangle-list","cullMode":"{cull}","frontFace":"ccw"}},"depthStencil":{{"format":"depth24plus","depthWriteEnabled":{depth},"depthCompare":"less"}}}}"#,
        cull = if blend { "none" } else { "back" },
        depth = !blend,
    ))
}

// ---- procedural meshes (position + normal, u16 indices) ---------------------------

#[derive(Default)]
struct Builder {
    v: Vec<f32>,
    i: Vec<u16>,
}

impl Builder {
    fn vtx(&mut self, p: [f32; 3], n: [f32; 3]) -> u16 {
        self.v.extend_from_slice(&p);
        self.v.extend_from_slice(&n);
        (self.v.len() / 6 - 1) as u16
    }
    fn tri(&mut self, a: u16, b: u16, c: u16) {
        self.i.extend_from_slice(&[a, b, c]);
    }
    fn upload(mut self) -> Mesh {
        let count = self.i.len() as u32;
        if self.i.len() % 2 == 1 {
            self.i.push(0); // pad to a multiple of 4 bytes
        }
        let vbuf = gfx::create_buffer((self.v.len() * 4) as u32, gfx::VERTEX | gfx::COPY_DST);
        let ibuf = gfx::create_buffer((self.i.len() * 2) as u32, gfx::INDEX | gfx::COPY_DST);
        gfx::write_buffer(vbuf, 0, &self.v);
        gfx::write_buffer(ibuf, 0, &self.i);
        Mesh { vbuf, ibuf, count }
    }
}

fn sphere(seg: u16, rings: u16) -> Mesh {
    let mut b = Builder::default();
    for r in 0..=rings {
        let phi = PI * r as f32 / rings as f32;
        for s in 0..=seg {
            let th = 2.0 * PI * s as f32 / seg as f32;
            let p = [phi.sin() * th.cos(), phi.cos(), phi.sin() * th.sin()];
            b.vtx(p, p);
        }
    }
    for r in 0..rings {
        for s in 0..seg {
            let a = r * (seg + 1) + s;
            let c = a + seg + 1;
            b.tri(a, a + 1, c);
            b.tri(a + 1, c + 1, c);
        }
    }
    b.upload()
}

/// Radius 1, y from -1 to 0 (top face at y = 0).
fn cylinder(seg: u16) -> Mesh {
    let mut b = Builder::default();
    let c = b.vtx([0.0, 0.0, 0.0], [0.0, 1.0, 0.0]);
    for s in 0..=seg {
        let th = 2.0 * PI * s as f32 / seg as f32;
        b.vtx([th.cos(), 0.0, th.sin()], [0.0, 1.0, 0.0]);
    }
    for s in 0..seg {
        b.tri(c, c + 2 + s, c + 1 + s);
    }
    let base = (b.v.len() / 6) as u16;
    for s in 0..=seg {
        let th = 2.0 * PI * s as f32 / seg as f32;
        let (x, z) = (th.cos(), th.sin());
        b.vtx([x, 0.0, z], [x, 0.0, z]);
        b.vtx([x, -1.0, z], [x, 0.0, z]);
    }
    for s in 0..seg {
        let a = base + s * 2;
        b.tri(a, a + 2, a + 1);
        b.tri(a + 2, a + 3, a + 1);
    }
    b.upload()
}

fn disc(seg: u16) -> Mesh {
    let mut b = Builder::default();
    let c = b.vtx([0.0, 0.0, 0.0], [0.0, 1.0, 0.0]);
    for s in 0..=seg {
        let th = 2.0 * PI * s as f32 / seg as f32;
        b.vtx([th.cos(), 0.0, th.sin()], [0.0, 1.0, 0.0]);
    }
    for s in 0..seg {
        b.tri(c, c + 2 + s, c + 1 + s);
    }
    b.upload()
}
