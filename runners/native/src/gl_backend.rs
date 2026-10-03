//! The GL backend of gasm:gl natively: ANGLE, executing the calls the model let
//! through (runners/web/lib/gl.js does the same with WebGL 2). The model has done the
//! checks every runner shares and answered the questions every runner answers the
//! same way (bindings, the pixel store, query and sync readiness); this part keeps the
//! model's names → GL objects, turns guest offsets into pointers and asks GL the rest.

use std::collections::HashMap;
use std::ffi::{c_char, c_void};

use wasmtime::Val;

use super::{guest_slice, guest_slice_mut, guest_str, text, Gl, Kind, PIXEL_PACK_BUFFER, PIXEL_UNPACK_BUFFER};
use crate::angle::{c_string, Angle};

/// WebGL extension names and the GLES extensions ANGLE enables for them.
const EXTENSIONS: &[(&str, &[&str])] = &[
    ("EXT_color_buffer_float", &["GL_EXT_color_buffer_float"]),
    ("EXT_color_buffer_half_float", &["GL_EXT_color_buffer_half_float"]),
    ("EXT_float_blend", &["GL_EXT_float_blend"]),
    ("EXT_texture_filter_anisotropic", &["GL_EXT_texture_filter_anisotropic"]),
    ("OES_texture_float_linear", &["GL_OES_texture_float_linear"]),
    (
        "WEBGL_compressed_texture_s3tc",
        &["GL_EXT_texture_compression_dxt1", "GL_ANGLE_texture_compression_dxt3", "GL_ANGLE_texture_compression_dxt5"],
    ),
    ("WEBGL_compressed_texture_s3tc_srgb", &["GL_EXT_texture_compression_s3tc_srgb"]),
    ("EXT_texture_compression_bptc", &["GL_EXT_texture_compression_bptc"]),
    ("EXT_texture_compression_rgtc", &["GL_EXT_texture_compression_rgtc"]),
    ("WEBGL_compressed_texture_etc", &["GL_ANGLE_compressed_texture_etc"]),
    ("WEBGL_compressed_texture_astc", &["GL_KHR_texture_compression_astc_ldr"]),
];

const GL_EXTENSIONS: u32 = 0x1F03;
const GL_NUM_EXTENSIONS: u32 = 0x821D;
const GL_REQUESTABLE_EXTENSIONS_ANGLE: u32 = 0x93A8;
const INVALID_ENUM: u32 = 0x0500;
const INVALID_VALUE: u32 = 0x0501;

/// ANGLE and the tables that connect the model to it.
pub struct Backend {
    pub angle: Angle,
    /// (kind, model name) → GL name, and back
    names: HashMap<(Kind, u32), u32>,
    rev: HashMap<(Kind, u32), u32>,
    syncs: HashMap<u32, usize>,
    /// (program, model location) → (GL location, components per element)
    uniforms: HashMap<(u32, i32), (i32, u32)>,
    /// the guest called present() this frame
    presented: bool,
}

impl Backend {
    pub fn new(angle: Angle) -> Backend {
        Backend { angle, names: HashMap::new(), rev: HashMap::new(), syncs: HashMap::new(), uniforms: HashMap::new(), presented: false }
    }
    fn obj(&self, k: Kind, n: u32) -> u32 {
        if n == 0 { 0 } else { self.names.get(&(k, n)).copied().unwrap_or(0) }
    }
    fn model_name(&self, k: Kind, gl: u32) -> u32 {
        if gl == 0 { 0 } else { self.rev.get(&(k, gl)).copied().unwrap_or(0) }
    }
    fn put_name(&mut self, k: Kind, n: u32, gl: u32) {
        self.names.insert((k, n), gl);
        self.rev.insert((k, gl), n);
    }
    fn take(&mut self, k: Kind, n: u32) -> Option<u32> {
        let gl = self.names.remove(&(k, n))?;
        self.rev.remove(&(k, gl));
        Some(gl)
    }
    /// The GLES extensions ANGLE has or can enable.
    fn gl_extensions(&self) -> Vec<String> {
        let g = &self.angle.gl;
        let mut out = Vec::new();
        unsafe {
            let mut n = 0;
            (g.glGetIntegerv)(GL_NUM_EXTENSIONS, &mut n as *mut i32 as *mut c_void);
            for i in 0..n.max(0) as u32 {
                out.push(cstr((g.glGetStringi)(GL_EXTENSIONS, i)));
            }
            out.extend(cstr((g.glGetString)(GL_REQUESTABLE_EXTENSIONS_ANGLE)).split_whitespace().map(str::to_owned));
        }
        out
    }
    /// The WebGL extensions available (what get_string(GL_EXTENSIONS) lists, as in browsers).
    fn webgl_extensions(&self) -> Vec<&'static str> {
        let have = self.gl_extensions();
        EXTENSIONS.iter().filter(|(_, gl)| gl.iter().all(|e| have.iter().any(|h| h == e))).map(|(w, _)| *w).collect()
    }
    fn enable_extension(&self, name: &str) -> bool {
        let Some((_, gl)) = EXTENSIONS.iter().find(|(w, _)| *w == name) else { return false };
        let have = self.gl_extensions();
        if !gl.iter().all(|e| have.iter().any(|h| h == e)) {
            return false;
        }
        for e in *gl {
            let c = c_string(e);
            unsafe { (self.angle.gl.glRequestExtensionANGLE)(c.as_ptr() as *const c_void) };
        }
        true
    }
    /// Move GL's pending errors into the model's queue (in order), so a query's own
    /// error can be told apart; returns the last one.
    fn drain(&self, model: &mut Gl) -> u32 {
        let mut last = 0;
        loop {
            let e = unsafe { (self.angle.gl.glGetError)() };
            if e == 0 {
                return last;
            }
            model.error(e);
            last = e;
        }
    }
    fn int(&self, f: impl FnOnce(*mut c_void)) -> i32 {
        let mut v = 0i32;
        f(&mut v as *mut i32 as *mut c_void);
        v
    }
    fn info_log(&self, shader: bool, obj: u32) -> String {
        let g = &self.angle.gl;
        unsafe {
            let len = self.int(|p| if shader { (g.glGetShaderiv)(obj, 0x8B84, p) } else { (g.glGetProgramiv)(obj, 0x8B84, p) });
            if len <= 0 {
                return String::new();
            }
            let mut buf = vec![0u8; len as usize];
            let mut got = 0;
            if shader {
                (g.glGetShaderInfoLog)(obj, len, &mut got as *mut i32 as *mut c_void, buf.as_mut_ptr() as *mut c_void);
            } else {
                (g.glGetProgramInfoLog)(obj, len, &mut got as *mut i32 as *mut c_void, buf.as_mut_ptr() as *mut c_void);
            }
            buf.truncate(got.max(0) as usize);
            String::from_utf8_lossy(&buf).into_owned()
        }
    }
    /// Components per element of the uniform `name` in `program` (for get_uniform*).
    fn uniform_components(&self, program: u32, name: &str) -> u32 {
        let g = &self.angle.gl;
        let base = name.split('[').next().unwrap_or(name);
        unsafe {
            let n = self.int(|p| (g.glGetProgramiv)(program, 0x8B86, p));
            let max = self.int(|p| (g.glGetProgramiv)(program, 0x8B87, p)).max(1);
            for i in 0..n.max(0) as u32 {
                let mut buf = vec![0u8; max as usize];
                let (mut len, mut size, mut ty) = (0i32, 0i32, 0u32);
                (g.glGetActiveUniform)(
                    program, i, max, &mut len as *mut i32 as *mut c_void, &mut size as *mut i32 as *mut c_void,
                    &mut ty as *mut u32 as *mut c_void, buf.as_mut_ptr() as *mut c_void,
                );
                let u = String::from_utf8_lossy(&buf[..len.max(0) as usize]).into_owned();
                if u == name || u.split('[').next() == Some(base) {
                    return type_components(ty);
                }
            }
        }
        16
    }
}

/// Components of a GLSL uniform type (samplers are one int).
fn type_components(ty: u32) -> u32 {
    match ty {
        0x8B50 | 0x8B53 | 0x8DC6 | 0x8B57 => 2, // vec2 ivec2 uvec2 bvec2
        0x8B51 | 0x8B54 | 0x8DC7 | 0x8B58 => 3,
        0x8B52 | 0x8B55 | 0x8DC8 | 0x8B59 | 0x8B5A => 4, // + mat2
        0x8B5B => 9,
        0x8B5C => 16,
        0x8B65 | 0x8B66 => 6, // mat2x3 mat2x4: 6, 8
        0x8B67 | 0x8B68 => 8,
        0x8B69 | 0x8B6A => 12,
        _ => 1,
    }
}

fn cstr(p: *const c_void) -> String {
    if p.is_null() {
        return String::new();
    }
    unsafe { std::ffi::CStr::from_ptr(p as *const c_char) }.to_string_lossy().into_owned()
}

/// How many values a glGet* of `pname` returns.
fn param_count(g: &crate::gles::Gles, pname: u32) -> usize {
    match pname {
        0x0D3A | 0x846D | 0x846E | 0x0B70 => 2,
        0x0BA2 | 0x0C10 | 0x0C22 | 0x0C23 | 0x8005 => 4,
        0x86A3 => {
            let mut n = 0i32;
            unsafe { (g.glGetIntegerv)(0x86A2, &mut n as *mut i32 as *mut c_void) };
            n.max(0) as usize
        }
        _ => 1,
    }
}

fn put_f(mem: &mut [u8], dst: u32, count: u32, v: &[f32]) -> wasmtime::Result<i32> {
    let n = v.len().min(count as usize);
    if n > 0 {
        let out = guest_slice_mut(mem, dst, n as u64 * 4)?;
        for (i, x) in v[..n].iter().enumerate() {
            out[i * 4..i * 4 + 4].copy_from_slice(&x.to_le_bytes());
        }
    }
    Ok(v.len() as i32)
}

fn put_i(mem: &mut [u8], dst: u32, count: u32, v: &[i64], wide: bool) -> wasmtime::Result<i32> {
    let n = v.len().min(count as usize);
    let size = if wide { 8 } else { 4 };
    if n > 0 {
        let out = guest_slice_mut(mem, dst, (n * size) as u64)?;
        for (i, x) in v[..n].iter().enumerate() {
            if wide {
                out[i * 8..i * 8 + 8].copy_from_slice(&x.to_le_bytes());
            } else {
                out[i * 4..i * 4 + 4].copy_from_slice(&(*x as i32).to_le_bytes());
            }
        }
    }
    Ok(v.len() as i32)
}

impl Gl {
    /// Execute calls on ANGLE from now on (the model stays in front of it).
    pub fn attach(&mut self, angle: Angle) {
        self.backend = Some(Box::new(Backend::new(angle)));
    }
    pub fn has_backend(&self) -> bool {
        self.backend.is_some()
    }
    /// The frame ended: show it if `show` (unless the guest did with present()).
    pub fn end_frame(&mut self, show: bool) {
        if let Some(b) = &mut self.backend {
            if show && !b.presented {
                b.angle.swap();
            }
            b.presented = false;
        }
    }
    /// The default framebuffer as RGBA8, top to bottom (headless screenshots).
    pub fn read_frame(&self) -> Option<(u32, u32, Vec<u8>)> {
        self.backend.as_ref().map(|b| b.angle.read_frame())
    }
    /// The drawable size, if a backend draws.
    pub fn size(&self) -> Option<(u32, u32)> {
        self.backend.as_ref().map(|b| b.angle.size())
    }
    pub fn renderer(&self) -> Option<&str> {
        self.backend.as_ref().map(|b| b.angle.renderer.as_str())
    }
}

/// Execute one call the model passed. `results` holds the model's answer, which
/// stays unless GL has the real one.
pub(super) fn forward(
    be: &mut Backend,
    model: &mut Gl,
    name: &str,
    args: &[Val],
    results: &mut [Val],
    mem: &mut [u8],
) -> wasmtime::Result<()> {
    let a = |i: usize| -> i32 {
        match args[i] {
            Val::I32(v) => v,
            Val::I64(v) => v as i32,
            _ => 0,
        }
    };
    let u = |i: usize| a(i) as u32;
    let f = |i: usize| match args[i] {
        Val::F32(b) => f32::from_bits(b),
        _ => 0.0,
    };
    let model_result = match results.first() {
        Some(Val::I32(v)) => *v as i64,
        _ => 0,
    };
    let mut ret = |v: i64| {
        if let Some(r) = results.first_mut() {
            *r = Val::I32(v as i32);
        }
    };
    let base = mem.as_mut_ptr();
    let at = |p: u32| -> *const c_void { if p == 0 { std::ptr::null() } else { unsafe { base.add(p as usize) as *const c_void } } };
    let at_mut = |p: u32| -> *mut c_void { unsafe { base.add(p as usize) as *mut c_void } };
    let off = |o: u32| o as usize as *const c_void;
    let g = &be.angle.gl as *const crate::gles::Gles;
    let g = unsafe { &*g };
    use Kind::*;
    unsafe {
        match name {
            // ---- frames, context ----
            "width" => ret(be.angle.size().0 as i64),
            "height" => ret(be.angle.size().1 as i64),
            "present" => {
                be.angle.swap();
                be.presented = true;
            }
            "get_error" => {
                if model_result == 0 {
                    ret((g.glGetError)() as i64)
                }
            }
            "get_string" => {
                let s = if u(0) == GL_EXTENSIONS { be.webgl_extensions().join(" ") } else { cstr((g.glGetString)(u(0))) };
                ret(text(mem, u(1), u(2), &s)? as i64)
            }
            "enable_extension" => {
                let n = guest_str(mem, u(0), u(1))?;
                ret(be.enable_extension(&n) as i64)
            }
            "get_integerv" | "get_floatv" | "get_integer64v" => {
                let pname = u(0);
                if model.model_param(pname).is_some() {
                    return Ok(());
                }
                be.drain(model);
                let n = param_count(g, pname).max(1);
                let (dst, count) = (u(1), u(2));
                match name {
                    "get_floatv" => {
                        let mut v = vec![0f32; n.max(16)];
                        (g.glGetFloatv)(pname, v.as_mut_ptr() as *mut c_void);
                        if (g.glGetError)() == INVALID_ENUM {
                            model.error(INVALID_ENUM);
                            {
                    ret(-1);
                    return Ok(());
                }
                        }
                        v.truncate(n);
                        ret(put_f(mem, dst, count, &v)? as i64)
                    }
                    _ => {
                        let mut v = vec![0i64; n.max(16)];
                        (g.glGetInteger64v)(pname, v.as_mut_ptr() as *mut c_void);
                        if (g.glGetError)() == INVALID_ENUM {
                            model.error(INVALID_ENUM);
                            {
                    ret(-1);
                    return Ok(());
                }
                        }
                        v.truncate(n);
                        // objects come back as the guest's names
                        match pname {
                            0x8919 => v[0] = be.model_name(Sampler, v[0] as u32) as i64,
                            0x8E25 => v[0] = be.model_name(TransformFeedback, v[0] as u32) as i64,
                            _ => {}
                        }
                        ret(put_i(mem, dst, count, &v, name == "get_integer64v")? as i64)
                    }
                }
            }
            "get_integeri_v" => {
                be.drain(model);
                let mut v = 0i64;
                (g.glGetInteger64i_v)(u(0), u(1), &mut v as *mut i64 as *mut c_void);
                if (g.glGetError)() != 0 {
                    model.error(INVALID_ENUM);
                    {
                    ret(-1);
                    return Ok(());
                }
                }
                if u(0) == 0x8A28 || u(0) == 0x8C8F {
                    v = be.model_name(Buffer, v as u32) as i64;
                }
                ret(put_i(mem, u(2), u(3), &[v], false)? as i64)
            }
            "get_internalformativ" => {
                let (t, fmt, pname) = (u(0), u(1), u(2));
                let n = if pname == 0x80A9 { be.int(|p| (g.glGetInternalformativ)(t, fmt, 0x9380, 1, p)).max(0) as usize } else { 1 };
                let mut v = vec![0i32; n.max(1)];
                (g.glGetInternalformativ)(t, fmt, pname, n as i32, v.as_mut_ptr() as *mut c_void);
                v.truncate(n);
                let v: Vec<i64> = v.into_iter().map(i64::from).collect();
                ret(put_i(mem, u(3), u(4), &v, false)? as i64)
            }
            "get_shader_precision_format" => {
                let (mut range, mut precision) = ([0i32; 2], 0i32);
                (g.glGetShaderPrecisionFormat)(u(0), u(1), range.as_mut_ptr() as *mut c_void, &mut precision as *mut i32 as *mut c_void);
                put_i(mem, u(2), 3, &[range[0] as i64, range[1] as i64, precision as i64], false)?;
            }

            // ---- state ----
            "active_texture" => (g.glActiveTexture)(u(0)),
            "blend_color" => (g.glBlendColor)(f(0), f(1), f(2), f(3)),
            "blend_equation" => (g.glBlendEquation)(u(0)),
            "blend_equation_separate" => (g.glBlendEquationSeparate)(u(0), u(1)),
            "blend_func" => (g.glBlendFunc)(u(0), u(1)),
            "blend_func_separate" => (g.glBlendFuncSeparate)(u(0), u(1), u(2), u(3)),
            "clear" => (g.glClear)(u(0)),
            "clear_color" => (g.glClearColor)(f(0), f(1), f(2), f(3)),
            "clear_depthf" => (g.glClearDepthf)(f(0)),
            "clear_stencil" => (g.glClearStencil)(a(0)),
            "color_mask" => (g.glColorMask)((u(0) != 0) as u8, (u(1) != 0) as u8, (u(2) != 0) as u8, (u(3) != 0) as u8),
            "cull_face" => (g.glCullFace)(u(0)),
            "depth_func" => (g.glDepthFunc)(u(0)),
            "depth_mask" => (g.glDepthMask)((u(0) != 0) as u8),
            "depth_rangef" => (g.glDepthRangef)(f(0), f(1)),
            "disable" => (g.glDisable)(u(0)),
            "enable" => (g.glEnable)(u(0)),
            "is_enabled" => ret((g.glIsEnabled)(u(0)) as i64),
            "front_face" => (g.glFrontFace)(u(0)),
            "hint" => (g.glHint)(u(0), u(1)),
            "line_width" => (g.glLineWidth)(f(0)),
            "pixel_storei" => {
                // WebGL-only pnames (UNPACK_FLIP_Y_WEBGL, ...) have no GLES equivalent
                if !(0x9240..=0x9244).contains(&u(0)) {
                    (g.glPixelStorei)(u(0), a(1))
                }
            }
            "polygon_offset" => (g.glPolygonOffset)(f(0), f(1)),
            "sample_coverage" => (g.glSampleCoverage)(f(0), (u(1) != 0) as u8),
            "scissor" => (g.glScissor)(a(0), a(1), a(2), a(3)),
            "viewport" => (g.glViewport)(a(0), a(1), a(2), a(3)),
            "stencil_func" => (g.glStencilFunc)(u(0), a(1), u(2)),
            "stencil_func_separate" => (g.glStencilFuncSeparate)(u(0), u(1), a(2), u(3)),
            "stencil_mask" => (g.glStencilMask)(u(0)),
            "stencil_mask_separate" => (g.glStencilMaskSeparate)(u(0), u(1)),
            "stencil_op" => (g.glStencilOp)(u(0), u(1), u(2)),
            "stencil_op_separate" => (g.glStencilOpSeparate)(u(0), u(1), u(2), u(3)),
            "finish" => (g.glFinish)(),
            "flush" => (g.glFlush)(),

            // ---- object lifetimes (the model made or retired the name) ----
            "create_buffer" | "create_texture" | "create_vertex_array" | "create_sampler" | "create_framebuffer"
            | "create_renderbuffer" | "create_query" | "create_transform_feedback" => {
                let (kind, make): (Kind, unsafe extern "system" fn(i32, *mut c_void)) = match name {
                    "create_buffer" => (Buffer, g.glGenBuffers),
                    "create_texture" => (Texture, g.glGenTextures),
                    "create_vertex_array" => (VertexArray, g.glGenVertexArrays),
                    "create_sampler" => (Sampler, g.glGenSamplers),
                    "create_framebuffer" => (Framebuffer, g.glGenFramebuffers),
                    "create_renderbuffer" => (Renderbuffer, g.glGenRenderbuffers),
                    "create_query" => (Query, g.glGenQueries),
                    _ => (TransformFeedback, g.glGenTransformFeedbacks),
                };
                let mut x = 0u32;
                make(1, &mut x as *mut u32 as *mut c_void);
                be.put_name(kind, model_result as u32, x);
            }
            "delete_buffer" | "delete_texture" | "delete_vertex_array" | "delete_sampler" | "delete_framebuffer"
            | "delete_renderbuffer" | "delete_query" | "delete_transform_feedback" => {
                let (kind, del): (Kind, unsafe extern "system" fn(i32, *const c_void)) = match name {
                    "delete_buffer" => (Buffer, g.glDeleteBuffers),
                    "delete_texture" => (Texture, g.glDeleteTextures),
                    "delete_vertex_array" => (VertexArray, g.glDeleteVertexArrays),
                    "delete_sampler" => (Sampler, g.glDeleteSamplers),
                    "delete_framebuffer" => (Framebuffer, g.glDeleteFramebuffers),
                    "delete_renderbuffer" => (Renderbuffer, g.glDeleteRenderbuffers),
                    "delete_query" => (Query, g.glDeleteQueries),
                    _ => (TransformFeedback, g.glDeleteTransformFeedbacks),
                };
                if let Some(x) = be.take(kind, u(0)) {
                    del(1, &x as *const u32 as *const c_void);
                }
            }
            "create_shader" => {
                let x = (g.glCreateShader)(u(0));
                be.put_name(Shader, model_result as u32, x);
            }
            "create_program" => {
                let x = (g.glCreateProgram)();
                be.put_name(Program, model_result as u32, x);
            }
            "delete_shader" => {
                if let Some(x) = be.take(Shader, u(0)) {
                    (g.glDeleteShader)(x)
                }
            }
            "delete_program" => {
                be.uniforms.retain(|(p, _), _| *p != u(0));
                if let Some(x) = be.take(Program, u(0)) {
                    (g.glDeleteProgram)(x)
                }
            }
            "fence_sync" => {
                let s = (g.glFenceSync)(u(0), u(1));
                be.syncs.insert(model_result as u32, s as usize);
            }
            "delete_sync" => {
                if let Some(s) = be.syncs.remove(&u(0)) {
                    (g.glDeleteSync)(s as *const c_void)
                }
            }
            // the model answers these (names, readiness)
            "is_buffer" | "is_texture" | "is_vertex_array" | "is_sampler" | "is_framebuffer" | "is_renderbuffer"
            | "is_shader" | "is_program" | "is_query" | "is_sync" | "is_transform_feedback" | "frame_shown"
            | "get_shader_source" | "get_queryiv" | "client_wait_sync" | "wait_sync" | "get_synciv" => {}

            // ---- buffers ----
            "bind_buffer" => (g.glBindBuffer)(u(0), be.obj(Buffer, u(1))),
            "bind_buffer_base" => (g.glBindBufferBase)(u(0), u(1), be.obj(Buffer, u(2))),
            "bind_buffer_range" => (g.glBindBufferRange)(u(0), u(1), be.obj(Buffer, u(2)), u(3) as isize, u(4) as isize),
            "buffer_data" => (g.glBufferData)(u(0), u(2) as isize, at(u(1)), u(3)),
            "buffer_sub_data" => (g.glBufferSubData)(u(0), u(1) as isize, u(3) as isize, at(u(2))),
            "copy_buffer_sub_data" => (g.glCopyBufferSubData)(u(0), u(1), u(2) as isize, u(3) as isize, u(4) as isize),
            "get_buffer_sub_data" => {
                // GLES reads buffers back through a mapping
                let (t, o, dst, len) = (u(0), u(1), u(2), u(3));
                if len > 0 {
                    let p = (g.glMapBufferRange)(t, o as isize, len as isize, 0x0001);
                    if !p.is_null() {
                        let out = guest_slice_mut(mem, dst, len as u64)?;
                        std::ptr::copy_nonoverlapping(p as *const u8, out.as_mut_ptr(), len as usize);
                        (g.glUnmapBuffer)(t);
                    }
                }
            }
            "get_buffer_parameteriv" => ret(be.int(|p| (g.glGetBufferParameteriv)(u(0), u(1), p)) as i64),

            // ---- vertex arrays ----
            "bind_vertex_array" => (g.glBindVertexArray)(be.obj(VertexArray, u(0))),
            "enable_vertex_attrib_array" => (g.glEnableVertexAttribArray)(u(0)),
            "disable_vertex_attrib_array" => (g.glDisableVertexAttribArray)(u(0)),
            "vertex_attrib_pointer" => (g.glVertexAttribPointer)(u(0), a(1), u(2), (u(3) != 0) as u8, a(4), off(u(5))),
            "vertex_attrib_ipointer" => (g.glVertexAttribIPointer)(u(0), a(1), u(2), a(3), off(u(4))),
            "vertex_attrib_divisor" => (g.glVertexAttribDivisor)(u(0), u(1)),
            "vertex_attrib4f" => (g.glVertexAttrib4f)(u(0), f(1), f(2), f(3), f(4)),
            "vertex_attribi4i" => (g.glVertexAttribI4i)(u(0), a(1), a(2), a(3), a(4)),
            "vertex_attribi4ui" => (g.glVertexAttribI4ui)(u(0), u(1), u(2), u(3), u(4)),
            "get_vertex_attribiv" => {
                let v = be.int(|p| (g.glGetVertexAttribiv)(u(0), u(1), p));
                ret(if u(1) == 0x889F { be.model_name(Buffer, v as u32) as i64 } else { v as i64 })
            }
            "get_vertex_attribfv" => {
                let mut v = [0f32; 4];
                (g.glGetVertexAttribfv)(u(0), u(1), v.as_mut_ptr() as *mut c_void);
                let n = if u(1) == 0x8626 { 4 } else { 1 };
                ret(put_f(mem, u(2), u(3), &v[..n])? as i64)
            }
            "get_vertex_attrib_offset" => {
                let mut p: *mut c_void = std::ptr::null_mut();
                (g.glGetVertexAttribPointerv)(u(0), u(1), &mut p as *mut *mut c_void as *mut c_void);
                ret(p as usize as i64)
            }

            // ---- drawing ----
            "draw_arrays" => (g.glDrawArrays)(u(0), a(1), a(2)),
            "draw_elements" => (g.glDrawElements)(u(0), a(1), u(2), off(u(3))),
            "draw_arrays_instanced" => (g.glDrawArraysInstanced)(u(0), a(1), a(2), a(3)),
            "draw_elements_instanced" => (g.glDrawElementsInstanced)(u(0), a(1), u(2), off(u(3)), a(4)),
            "draw_range_elements" => (g.glDrawRangeElements)(u(0), u(1), u(2), a(3), u(4), off(u(5))),
            "draw_buffers" => (g.glDrawBuffers)(a(1), at(u(0))),
            "clear_bufferiv" => (g.glClearBufferiv)(u(0), a(1), at(u(2))),
            "clear_bufferuiv" => (g.glClearBufferuiv)(u(0), a(1), at(u(2))),
            "clear_bufferfv" => (g.glClearBufferfv)(u(0), a(1), at(u(2))),
            "clear_bufferfi" => (g.glClearBufferfi)(u(0), a(1), f(2), a(3)),

            // ---- textures ----
            "bind_texture" => (g.glBindTexture)(u(0), be.obj(Texture, u(1))),
            "tex_parameteri" => (g.glTexParameteri)(u(0), u(1), a(2)),
            "tex_parameterf" => (g.glTexParameterf)(u(0), u(1), f(2)),
            "get_tex_parameteriv" => ret(be.int(|p| (g.glGetTexParameteriv)(u(0), u(1), p)) as i64),
            "get_tex_parameterfv" | "get_sampler_parameterfv" => {
                let mut v = 0f32;
                if name == "get_tex_parameterfv" {
                    (g.glGetTexParameterfv)(u(0), u(1), &mut v as *mut f32 as *mut c_void);
                } else {
                    (g.glGetSamplerParameterfv)(be.obj(Sampler, u(0)), u(1), &mut v as *mut f32 as *mut c_void);
                }
                if let Some(r) = results.first_mut() {
                    *r = Val::F32(v.to_bits());
                }
            }
            "tex_image_2d" | "tex_image_3d" | "tex_sub_image_2d" | "tex_sub_image_3d" => {
                // pixels: none, guest memory, or (an unpack buffer bound) an offset into it
                let n = args.len();
                let (ptr, len) = (u(n - 2), u(n - 1));
                let px = if model.bound_buffer(PIXEL_UNPACK_BUFFER) != 0 { off(len) } else { at(ptr) };
                match name {
                    "tex_image_2d" => (g.glTexImage2D)(u(0), a(1), a(2), a(3), a(4), a(5), u(6), u(7), px),
                    "tex_image_3d" => (g.glTexImage3D)(u(0), a(1), a(2), a(3), a(4), a(5), a(6), u(7), u(8), px),
                    "tex_sub_image_2d" => (g.glTexSubImage2D)(u(0), a(1), a(2), a(3), a(4), a(5), u(6), u(7), px),
                    _ => (g.glTexSubImage3D)(u(0), a(1), a(2), a(3), a(4), a(5), a(6), a(7), u(8), u(9), px),
                }
            }
            "tex_storage_2d" => (g.glTexStorage2D)(u(0), a(1), u(2), a(3), a(4)),
            "tex_storage_3d" => (g.glTexStorage3D)(u(0), a(1), u(2), a(3), a(4), a(5)),
            "compressed_tex_image_2d" => (g.glCompressedTexImage2D)(u(0), a(1), u(2), a(3), a(4), a(5), a(7), at(u(6))),
            "compressed_tex_image_3d" => (g.glCompressedTexImage3D)(u(0), a(1), u(2), a(3), a(4), a(5), a(6), a(8), at(u(7))),
            "compressed_tex_sub_image_2d" => (g.glCompressedTexSubImage2D)(u(0), a(1), a(2), a(3), a(4), a(5), u(6), a(8), at(u(7))),
            "compressed_tex_sub_image_3d" => {
                (g.glCompressedTexSubImage3D)(u(0), a(1), a(2), a(3), a(4), a(5), a(6), a(7), u(8), a(10), at(u(9)))
            }
            "copy_tex_image_2d" => (g.glCopyTexImage2D)(u(0), a(1), u(2), a(3), a(4), a(5), a(6), a(7)),
            "copy_tex_sub_image_2d" => (g.glCopyTexSubImage2D)(u(0), a(1), a(2), a(3), a(4), a(5), a(6), a(7)),
            "copy_tex_sub_image_3d" => (g.glCopyTexSubImage3D)(u(0), a(1), a(2), a(3), a(4), a(5), a(6), a(7), a(8)),
            "generate_mipmap" => (g.glGenerateMipmap)(u(0)),

            // ---- samplers ----
            "bind_sampler" => (g.glBindSampler)(u(0), be.obj(Sampler, u(1))),
            "sampler_parameteri" => (g.glSamplerParameteri)(be.obj(Sampler, u(0)), u(1), a(2)),
            "sampler_parameterf" => (g.glSamplerParameterf)(be.obj(Sampler, u(0)), u(1), f(2)),
            "get_sampler_parameteriv" => ret(be.int(|p| (g.glGetSamplerParameteriv)(be.obj(Sampler, u(0)), u(1), p)) as i64),

            // ---- framebuffers, renderbuffers ----
            "bind_framebuffer" => (g.glBindFramebuffer)(u(0), be.obj(Framebuffer, u(1))),
            "check_framebuffer_status" => ret((g.glCheckFramebufferStatus)(u(0)) as i64),
            "framebuffer_texture_2d" => (g.glFramebufferTexture2D)(u(0), u(1), u(2), be.obj(Texture, u(3)), a(4)),
            "framebuffer_texture_layer" => (g.glFramebufferTextureLayer)(u(0), u(1), be.obj(Texture, u(2)), a(3), a(4)),
            "framebuffer_renderbuffer" => (g.glFramebufferRenderbuffer)(u(0), u(1), u(2), be.obj(Renderbuffer, u(3))),
            "get_framebuffer_attachment_parameteriv" => {
                let (t, att, p) = (u(0), u(1), u(2));
                let v = be.int(|q| (g.glGetFramebufferAttachmentParameteriv)(t, att, p, q));
                if p == 0x8CD1 {
                    // OBJECT_NAME: the guest's name of the texture or renderbuffer
                    let ty = be.int(|q| (g.glGetFramebufferAttachmentParameteriv)(t, att, 0x8CD0, q)) as u32;
                    let kind = if ty == 0x1702 { Texture } else { Renderbuffer };
                    ret(be.model_name(kind, v as u32) as i64)
                } else {
                    ret(v as i64)
                }
            }
            "blit_framebuffer" => (g.glBlitFramebuffer)(a(0), a(1), a(2), a(3), a(4), a(5), a(6), a(7), u(8), u(9)),
            "invalidate_framebuffer" => (g.glInvalidateFramebuffer)(u(0), a(2), at(u(1))),
            "invalidate_sub_framebuffer" => (g.glInvalidateSubFramebuffer)(u(0), a(2), at(u(1)), a(3), a(4), a(5), a(6)),
            "read_buffer" => (g.glReadBuffer)(u(0)),
            "read_pixels" => {
                let dst = if model.bound_buffer(PIXEL_PACK_BUFFER) != 0 { off(u(6)) as *mut c_void } else { at_mut(u(6)) };
                (g.glReadPixels)(a(0), a(1), a(2), a(3), u(4), u(5), dst)
            }
            "bind_renderbuffer" => (g.glBindRenderbuffer)(u(0), be.obj(Renderbuffer, u(1))),
            "renderbuffer_storage" => (g.glRenderbufferStorage)(u(0), u(1), a(2), a(3)),
            "renderbuffer_storage_multisample" => (g.glRenderbufferStorageMultisample)(u(0), a(1), u(2), a(3), a(4)),
            "get_renderbuffer_parameteriv" => ret(be.int(|p| (g.glGetRenderbufferParameteriv)(u(0), u(1), p)) as i64),

            // ---- shaders and programs ----
            "shader_source" => {
                let p = at(u(1));
                let len = a(2);
                (g.glShaderSource)(be.obj(Shader, u(0)), 1, &p as *const *const c_void as *const c_void, &len as *const i32 as *const c_void)
            }
            "compile_shader" => (g.glCompileShader)(be.obj(Shader, u(0))),
            "get_shaderiv" => {
                if u(1) != 0x8B4F && u(1) != 0x8B88 {
                    ret(be.int(|p| (g.glGetShaderiv)(be.obj(Shader, u(0)), u(1), p)) as i64)
                }
            }
            "get_shader_info_log" => {
                let log = be.info_log(true, be.obj(Shader, u(0)));
                ret(text(mem, u(1), u(2), &log)? as i64)
            }
            "attach_shader" => (g.glAttachShader)(be.obj(Program, u(0)), be.obj(Shader, u(1))),
            "detach_shader" => (g.glDetachShader)(be.obj(Program, u(0)), be.obj(Shader, u(1))),
            "link_program" => {
                be.uniforms.retain(|(p, _), _| *p != u(0));
                (g.glLinkProgram)(be.obj(Program, u(0)))
            }
            "use_program" => (g.glUseProgram)(be.obj(Program, u(0))),
            "validate_program" => (g.glValidateProgram)(be.obj(Program, u(0))),
            "get_programiv" => ret(be.int(|p| (g.glGetProgramiv)(be.obj(Program, u(0)), u(1), p)) as i64),
            "get_program_info_log" => {
                let log = be.info_log(false, be.obj(Program, u(0)));
                ret(text(mem, u(1), u(2), &log)? as i64)
            }
            "get_attached_shaders" => {
                let mut v = [0u32; 16];
                let mut n = 0i32;
                (g.glGetAttachedShaders)(be.obj(Program, u(0)), 16, &mut n as *mut i32 as *mut c_void, v.as_mut_ptr() as *mut c_void);
                let names: Vec<i64> = v[..n.max(0) as usize].iter().map(|&s| be.model_name(Shader, s) as i64).collect();
                ret(put_i(mem, u(1), u(2), &names, false)? as i64)
            }
            "bind_attrib_location" => {
                let n = c_string(&guest_str(mem, u(2), u(3))?);
                (g.glBindAttribLocation)(be.obj(Program, u(0)), u(1), n.as_ptr() as *const c_void)
            }
            "get_attrib_location" | "get_frag_data_location" => {
                let n = c_string(&guest_str(mem, u(1), u(2))?);
                let p = be.obj(Program, u(0));
                ret(if name == "get_attrib_location" {
                    (g.glGetAttribLocation)(p, n.as_ptr() as *const c_void)
                } else {
                    (g.glGetFragDataLocation)(p, n.as_ptr() as *const c_void)
                } as i64)
            }
            "get_active_attrib" | "get_active_uniform" | "get_transform_feedback_varying" => {
                let p = be.obj(Program, u(0));
                let (count_p, max_p, get): (u32, u32, unsafe extern "system" fn(u32, u32, i32, *mut c_void, *mut c_void, *mut c_void, *mut c_void)) =
                    match name {
                        "get_active_attrib" => (0x8B89, 0x8B8A, g.glGetActiveAttrib),
                        "get_active_uniform" => (0x8B86, 0x8B87, g.glGetActiveUniform),
                        _ => (0x8C83, 0x8C76, g.glGetTransformFeedbackVarying),
                    };
                let count = be.int(|q| (g.glGetProgramiv)(p, count_p, q));
                if u(1) as i64 >= count as i64 {
                    model.error(INVALID_VALUE);
                    {
                    ret(-1);
                    return Ok(());
                }
                }
                let max = be.int(|q| (g.glGetProgramiv)(p, max_p, q)).max(1);
                let mut buf = vec![0u8; max as usize];
                let (mut len, mut size, mut ty) = (0i32, 0i32, 0u32);
                get(
                    p, u(1), max, &mut len as *mut i32 as *mut c_void, &mut size as *mut i32 as *mut c_void,
                    &mut ty as *mut u32 as *mut c_void, buf.as_mut_ptr() as *mut c_void,
                );
                let s = String::from_utf8_lossy(&buf[..len.max(0) as usize]).into_owned();
                put_i(mem, u(4), 2, &[size as i64, ty as i64], false)?;
                ret(text(mem, u(2), u(3), &s)? as i64)
            }
            "get_uniform_location" => {
                let uname = guest_str(mem, u(1), u(2))?;
                let p = be.obj(Program, u(0));
                let c = c_string(&uname);
                let loc = (g.glGetUniformLocation)(p, c.as_ptr() as *const c_void);
                let model_loc = model_result as i32;
                if loc < 0 {
                    // not in the linked program: -1, and the name gets no location (as in browsers)
                    if let Some(t) = model.uniforms.get_mut(&u(0)) {
                        if t.by_name.get(&uname) == Some(&model_loc) && model_loc == t.count - 1 {
                            t.by_name.remove(&uname);
                            t.count -= 1;
                        }
                    }
                    {
                    ret(-1);
                    return Ok(());
                }
                }
                if !be.uniforms.contains_key(&(u(0), model_loc)) {
                    let comps = be.uniform_components(p, &uname);
                    be.uniforms.insert((u(0), model_loc), (loc, comps));
                }
            }
            "get_uniform_index" => {
                let c = c_string(&guest_str(mem, u(1), u(2))?);
                let ptr = c.as_ptr();
                let mut idx = 0u32;
                (g.glGetUniformIndices)(be.obj(Program, u(0)), 1, &ptr as *const *const c_char as *const c_void, &mut idx as *mut u32 as *mut c_void);
                ret(idx as i32 as i64)
            }
            "get_active_uniformsiv" => {
                (g.glGetActiveUniformsiv)(be.obj(Program, u(0)), a(2), at(u(1)), u(3), at_mut(u(4)))
            }
            "get_uniform_block_index" => {
                let c = c_string(&guest_str(mem, u(1), u(2))?);
                ret((g.glGetUniformBlockIndex)(be.obj(Program, u(0)), c.as_ptr() as *const c_void) as i32 as i64)
            }
            "get_active_uniform_block_name" => {
                let p = be.obj(Program, u(0));
                let n = be.int(|q| (g.glGetActiveUniformBlockiv)(p, u(1), 0x8A41, q)).max(1);
                let mut buf = vec![0u8; n as usize];
                let mut len = 0i32;
                (g.glGetActiveUniformBlockName)(p, u(1), n, &mut len as *mut i32 as *mut c_void, buf.as_mut_ptr() as *mut c_void);
                let s = String::from_utf8_lossy(&buf[..len.max(0) as usize]).into_owned();
                ret(text(mem, u(2), u(3), &s)? as i64)
            }
            "get_active_uniform_blockiv" => {
                let p = be.obj(Program, u(0));
                let n = if u(2) == 0x8A43 { be.int(|q| (g.glGetActiveUniformBlockiv)(p, u(1), 0x8A42, q)).max(0) as usize } else { 1 };
                let mut v = vec![0i32; n.max(1)];
                (g.glGetActiveUniformBlockiv)(p, u(1), u(2), v.as_mut_ptr() as *mut c_void);
                v.truncate(n);
                let v: Vec<i64> = v.into_iter().map(i64::from).collect();
                ret(put_i(mem, u(3), u(4), &v, false)? as i64)
            }
            "uniform_block_binding" => (g.glUniformBlockBinding)(be.obj(Program, u(0)), u(1), u(2)),
            "get_uniformfv" | "get_uniformiv" | "get_uniformuiv" => {
                let Some(&(loc, comps)) = be.uniforms.get(&(u(0), a(1))) else { return Ok(()) };
                let p = be.obj(Program, u(0));
                let n = comps.min(16) as usize;
                if name == "get_uniformfv" {
                    let mut v = [0f32; 16];
                    (g.glGetUniformfv)(p, loc, v.as_mut_ptr() as *mut c_void);
                    ret(put_f(mem, u(2), u(3), &v[..n])? as i64)
                } else {
                    let mut v = [0i32; 16];
                    if name == "get_uniformiv" {
                        (g.glGetUniformiv)(p, loc, v.as_mut_ptr() as *mut c_void);
                    } else {
                        (g.glGetUniformuiv)(p, loc, v.as_mut_ptr() as *mut c_void);
                    }
                    let v: Vec<i64> = v[..n].iter().map(|&x| x as i64).collect();
                    ret(put_i(mem, u(2), u(3), &v, false)? as i64)
                }
            }
            "transform_feedback_varyings" => {
                let bytes = guest_slice(mem, u(1), u(2) as u64)?;
                let names: Vec<std::ffi::CString> = bytes
                    .split(|&b| b == 0)
                    .take(u(3) as usize)
                    .map(|s| c_string(&String::from_utf8_lossy(s)))
                    .collect();
                let ptrs: Vec<*const c_char> = names.iter().map(|c| c.as_ptr()).collect();
                (g.glTransformFeedbackVaryings)(be.obj(Program, u(0)), ptrs.len() as i32, ptrs.as_ptr() as *const c_void, u(4))
            }

            // ---- uniforms (the model checked the location) ----
            n if n.starts_with("uniform") => {
                let Some(&(loc, _)) = be.uniforms.get(&(model.program, a(0))) else { return Ok(()) };
                match n {
                    "uniform1f" => (g.glUniform1f)(loc, f(1)),
                    "uniform2f" => (g.glUniform2f)(loc, f(1), f(2)),
                    "uniform3f" => (g.glUniform3f)(loc, f(1), f(2), f(3)),
                    "uniform4f" => (g.glUniform4f)(loc, f(1), f(2), f(3), f(4)),
                    "uniform1i" => (g.glUniform1i)(loc, a(1)),
                    "uniform2i" => (g.glUniform2i)(loc, a(1), a(2)),
                    "uniform3i" => (g.glUniform3i)(loc, a(1), a(2), a(3)),
                    "uniform4i" => (g.glUniform4i)(loc, a(1), a(2), a(3), a(4)),
                    "uniform1ui" => (g.glUniform1ui)(loc, u(1)),
                    "uniform2ui" => (g.glUniform2ui)(loc, u(1), u(2)),
                    "uniform3ui" => (g.glUniform3ui)(loc, u(1), u(2), u(3)),
                    "uniform4ui" => (g.glUniform4ui)(loc, u(1), u(2), u(3), u(4)),
                    "uniform1fv" => (g.glUniform1fv)(loc, a(1), at(u(2))),
                    "uniform2fv" => (g.glUniform2fv)(loc, a(1), at(u(2))),
                    "uniform3fv" => (g.glUniform3fv)(loc, a(1), at(u(2))),
                    "uniform4fv" => (g.glUniform4fv)(loc, a(1), at(u(2))),
                    "uniform1iv" => (g.glUniform1iv)(loc, a(1), at(u(2))),
                    "uniform2iv" => (g.glUniform2iv)(loc, a(1), at(u(2))),
                    "uniform3iv" => (g.glUniform3iv)(loc, a(1), at(u(2))),
                    "uniform4iv" => (g.glUniform4iv)(loc, a(1), at(u(2))),
                    "uniform1uiv" => (g.glUniform1uiv)(loc, a(1), at(u(2))),
                    "uniform2uiv" => (g.glUniform2uiv)(loc, a(1), at(u(2))),
                    "uniform3uiv" => (g.glUniform3uiv)(loc, a(1), at(u(2))),
                    "uniform4uiv" => (g.glUniform4uiv)(loc, a(1), at(u(2))),
                    _ => {
                        let m = match n {
                            "uniform_matrix2fv" => g.glUniformMatrix2fv,
                            "uniform_matrix3fv" => g.glUniformMatrix3fv,
                            "uniform_matrix4fv" => g.glUniformMatrix4fv,
                            "uniform_matrix2x3fv" => g.glUniformMatrix2x3fv,
                            "uniform_matrix3x2fv" => g.glUniformMatrix3x2fv,
                            "uniform_matrix2x4fv" => g.glUniformMatrix2x4fv,
                            "uniform_matrix4x2fv" => g.glUniformMatrix4x2fv,
                            "uniform_matrix3x4fv" => g.glUniformMatrix3x4fv,
                            _ => g.glUniformMatrix4x3fv,
                        };
                        m(loc, a(1), (u(2) != 0) as u8, at(u(3)))
                    }
                }
            }

            // ---- queries, transform feedback ----
            "begin_query" => (g.glBeginQuery)(u(0), be.obj(Query, u(1))),
            "end_query" => (g.glEndQuery)(u(0)),
            "get_query_objectuiv" => {
                // the model says when it's ready (the next frame); GL has the value
                if u(1) == 0x8866 && model_result != 0 {
                    let mut v = 0u32;
                    (g.glGetQueryObjectuiv)(be.obj(Query, u(0)), 0x8866, &mut v as *mut u32 as *mut c_void);
                    ret(v as i64)
                }
            }
            "bind_transform_feedback" => (g.glBindTransformFeedback)(u(0), be.obj(TransformFeedback, u(1))),
            "begin_transform_feedback" => (g.glBeginTransformFeedback)(u(0)),
            "end_transform_feedback" => (g.glEndTransformFeedback)(),
            "pause_transform_feedback" => (g.glPauseTransformFeedback)(),
            "resume_transform_feedback" => (g.glResumeTransformFeedback)(),
            _ => wasmtime::bail!("gasm:gl.{name}: no GL backend for it"),
        }
    }
    Ok(())
}
