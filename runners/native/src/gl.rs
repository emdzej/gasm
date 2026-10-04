//! gasm:gl (OpenGL ES 3.0 with WebGL 2's rules, design/gasm-gl.md) on the native
//! runner: the shared model and the null GL. Mirrors runners/web/lib/gl.js (its
//! `ctx == null` paths) line by line: same names, same GL errors, same traps, same
//! hashes. Natively there is no GL backend yet, so gasm:gl guests run headless only.
//!
//! The imports are linked with `func_new` from the generated signature table
//! (gl_sigs.rs, from spec/abi.json) and dispatched by name in [`call`]. Every value
//! is read as the i32 the guest passed, as the JS runner sees it, so signed checks
//! (negative sizes, `end >= start`) agree.

use std::collections::{HashMap, HashSet};

use wasmtime::{Caller, FuncType, Linker, Val, ValType, bail};

use crate::host::{Host, guest_slice, guest_slice_mut, memory};

#[path = "gl_sigs.rs"]
mod sigs;

#[path = "gl_backend.rs"]
mod backend;
pub use backend::Backend;

const INVALID_ENUM: u32 = 0x0500;
const INVALID_VALUE: u32 = 0x0501;
const INVALID_OPERATION: u32 = 0x0502;
const ARRAY_BUFFER: u32 = 0x8892;
const ELEMENT_ARRAY_BUFFER: u32 = 0x8893;
const COPY_READ_BUFFER: u32 = 0x8F36;
const COPY_WRITE_BUFFER: u32 = 0x8F37;
const PIXEL_PACK_BUFFER: u32 = 0x88EB;
const PIXEL_UNPACK_BUFFER: u32 = 0x88EC;
const TRANSFORM_FEEDBACK_BUFFER: u32 = 0x8C8E;
const UNIFORM_BUFFER: u32 = 0x8A11;
const TEXTURE_2D: u32 = 0x0DE1;
const TEXTURE_CUBE_MAP: u32 = 0x8513;
const TEXTURE_3D: u32 = 0x806F;
const TEXTURE_2D_ARRAY: u32 = 0x8C1A;
const CUBE_POSITIVE_X: u32 = 0x8515;
const CUBE_NEGATIVE_Z: u32 = 0x851A;
const TEXTURE0: u32 = 0x84C0;
const FRAMEBUFFER: u32 = 0x8D40;
const READ_FRAMEBUFFER: u32 = 0x8CA8;
const DRAW_FRAMEBUFFER: u32 = 0x8CA9;
const RENDERBUFFER: u32 = 0x8D41;
const FRAMEBUFFER_COMPLETE: u32 = 0x8CD5;
const VERTEX_SHADER: u32 = 0x8B31;
const FRAGMENT_SHADER: u32 = 0x8B30;
const COMPILE_STATUS: u32 = 0x8B81;
const LINK_STATUS: u32 = 0x8B82;
const VALIDATE_STATUS: u32 = 0x8B83;
const SHADER_SOURCE_LENGTH: u32 = 0x8B88;
const SHADER_TYPE: u32 = 0x8B4F;
const TRANSFORM_FEEDBACK: u32 = 0x8E22;
const SYNC_STATUS: u32 = 0x9114;
const SIGNALED: u32 = 0x9119;
const UNSIGNALED: u32 = 0x9118;
const ALREADY_SIGNALED: u32 = 0x911A;
const TIMEOUT_EXPIRED: u32 = 0x911B;
const WAIT_FAILED: u32 = 0x911D;
const SYNC_GPU_COMMANDS_COMPLETE: u32 = 0x9117;
const QUERY_RESULT: u32 = 0x8866;
const QUERY_RESULT_AVAILABLE: u32 = 0x8867;
const CURRENT_QUERY: u32 = 0x8865;
const INVALID_INDEX: u32 = 0xFFFF_FFFF;

const BUFFER_TARGETS: &[u32] = &[
    ARRAY_BUFFER,
    ELEMENT_ARRAY_BUFFER,
    COPY_READ_BUFFER,
    COPY_WRITE_BUFFER,
    PIXEL_PACK_BUFFER,
    PIXEL_UNPACK_BUFFER,
    TRANSFORM_FEEDBACK_BUFFER,
    UNIFORM_BUFFER,
];
const TEXTURE_TARGETS: &[u32] = &[TEXTURE_2D, TEXTURE_CUBE_MAP, TEXTURE_3D, TEXTURE_2D_ARRAY];
const VOLUME_TARGETS: &[u32] = &[TEXTURE_3D, TEXTURE_2D_ARRAY];
const FB_TARGETS: &[u32] = &[FRAMEBUFFER, READ_FRAMEBUFFER, DRAW_FRAMEBUFFER];
const QUERY_TARGETS: &[u32] = &[0x8C2F, 0x8D6A, 0x8C88];
const INDEXED_TARGETS: &[u32] = &[UNIFORM_BUFFER, TRANSFORM_FEEDBACK_BUFFER];

/// Bytes per pixel of a (format, type) pair; 0 if the pair is invalid.
pub fn pixel_bytes(format: u32, ty: u32) -> u32 {
    match ty {
        0x8363 | 0x8033 | 0x8034 => return 2,
        0x8368 | 0x8C3B | 0x8C3E | 0x84FA => return 4,
        0x8DAD => return 8,
        _ => {}
    }
    let size = match ty {
        0x1400 | 0x1401 => 1,
        0x1402 | 0x1403 | 0x140B => 2,
        0x1404..=0x1406 => 4,
        _ => 0,
    };
    let comps = match format {
        0x1903 | 0x8D94 | 0x1906 | 0x1909 | 0x1902 => 1,
        0x8227 | 0x8228 | 0x190A | 0x84F9 => 2,
        0x1907 | 0x8D98 => 3,
        0x1908 | 0x8D99 => 4,
        _ => 0,
    };
    size * comps
}

#[derive(Clone, Copy)]
struct Store {
    alignment: i64,
    row_length: i64,
    image_height: i64,
    skip_pixels: i64,
    skip_rows: i64,
    skip_images: i64,
}

const STORE: Store = Store {
    alignment: 4,
    row_length: 0,
    image_height: 0,
    skip_pixels: 0,
    skip_rows: 0,
    skip_images: 0,
};

/// Bytes an image of w×h×d needs under a pixel-store state (GLES 3.0, 3.7.1).
fn image_bytes(w: i64, h: i64, d: i64, bpp: i64, s: &Store) -> i64 {
    if w <= 0 || h <= 0 || d <= 0 {
        return 0;
    }
    let row_len = if s.row_length > 0 { s.row_length } else { w };
    let a = s.alignment;
    let row = (row_len * bpp + a - 1) / a * a;
    let img_h = if s.image_height > 0 {
        s.image_height
    } else {
        h
    };
    let img = row * img_h;
    s.skip_images * img
        + (d - 1) * img
        + s.skip_rows * row
        + (h - 1) * row
        + s.skip_pixels * bpp
        + w * bpp
}

/// WebGL 2's guaranteed minimums (and fixed answers), reported by null GLs.
fn null_limit(pname: u32) -> &'static [i64] {
    // the same table as NULL_LIMITS in runners/web/lib/gl.js (gen-abi.mjs --check)
    match pname {
        0x0D33 | 0x851C | 0x84E8 => &[2048], // texture, cube map, renderbuffer size
        0x8073 | 0x88FF => &[256],           // 3D texture size, array texture layers
        0x8872 | 0x8B4C | 0x8869 => &[16],   // texture units (fragment, vertex), vertex attribs
        0x8B4D => &[32],                     // combined texture units
        0x8DFB => &[256],                    // vertex uniform vectors
        0x8DFD => &[224],                    // fragment uniform vectors
        0x8DFC => &[15],                     // varying vectors
        0x8B4B | 0x9125 => &[60],            // varying components, fragment input components
        0x9122 => &[64],                     // vertex output components
        0x8B4A => &[1024],                   // vertex uniform components
        0x8B49 => &[896],                    // fragment uniform components
        0x8CDF | 0x8824 | 0x8D57 => &[4],    // color attachments, draw buffers, samples
        0x8A2F => &[24],                     // uniform buffer bindings
        0x8A30 => &[16384],                  // uniform block size
        0x8A34 => &[256],                    // uniform buffer offset alignment
        0x8A2B | 0x8A2D => &[12],            // vertex, fragment uniform blocks
        0x8A2E => &[24],                     // combined uniform blocks
        0x8A31 => &[50176],                  // combined vertex uniform components
        0x8A33 => &[50048],                  // combined fragment uniform components
        0x8904 => &[-8],                     // min program texel offset
        0x8905 => &[7],                      // max program texel offset
        0x84FD => &[2],                      // texture LOD bias
        0x8D6B => &[16777215],               // element index (2^24 - 1)
        0x8C8A => &[64],                     // transform feedback interleaved components
        0x8C8B | 0x8C80 => &[4],             // transform feedback separate attribs, components
        0x0D3A => &[4096, 4096],             // viewport dims
        0x846D | 0x846E => &[1, 1],          // point size, line width ranges
        0x821B => &[3],                      // major version
        _ => &[0],
    }
}

fn null_string(name: u32) -> Option<&'static str> {
    Some(match name {
        0x1F00 => "gasm",
        0x1F01 => "gasm null GL",
        0x1F02 => "OpenGL ES 3.0 (gasm null GL)",
        0x8B8C => "OpenGL ES GLSL ES 3.00 (gasm null GL)",
        0x1F03 => "",
        _ => return None,
    })
}

#[derive(Clone, Copy, PartialEq, Eq, Hash)]
enum Kind {
    Buffer,
    Texture,
    VertexArray,
    Sampler,
    Framebuffer,
    Renderbuffer,
    Shader,
    Program,
    Query,
    Sync,
    TransformFeedback,
}

#[derive(Default)]
struct Uniforms {
    by_name: HashMap<String, i32>,
    count: i32,
}

/// The gasm:gl state of one guest (model + null GL).
pub struct Gl {
    next: HashMap<Kind, u32>,
    live: HashMap<Kind, HashSet<u32>>,
    errors: Vec<u32>,
    buffers: HashMap<u32, u32>,
    textures: HashMap<(u32, u32), u32>,
    /// the target each texture was first bound to (it can't be bound to another)
    texture_targets: HashMap<u32, u32>,
    unit: u32,
    framebuffers: HashMap<u32, u32>,
    renderbuffer: u32,
    program: u32,
    vertex_array: u32,
    vao_elements: HashMap<u32, u32>,
    unpack: Store,
    pack: Store,
    shaders: HashMap<u32, (u32, String)>,
    uniforms: HashMap<u32, Uniforms>,
    active: HashMap<u32, u32>,
    query_ended: HashMap<u32, u64>,
    sync_made: HashMap<u32, u64>,
    /// GL errors the model has recorded so far: a call that adds none reaches the backend
    error_count: u64,
    /// the GL that executes calls (ANGLE); None: the null GL
    backend: Option<Box<Backend>>,
}

impl Default for Gl {
    fn default() -> Self {
        Gl {
            next: HashMap::new(),
            live: HashMap::new(),
            errors: Vec::new(),
            buffers: HashMap::new(),
            textures: HashMap::new(),
            texture_targets: HashMap::new(),
            unit: 0,
            framebuffers: HashMap::new(),
            renderbuffer: 0,
            program: 0,
            vertex_array: 0,
            vao_elements: HashMap::from([(0, 0)]),
            unpack: STORE,
            pack: STORE,
            shaders: HashMap::new(),
            uniforms: HashMap::new(),
            active: HashMap::new(),
            query_ended: HashMap::new(),
            sync_made: HashMap::new(),
            error_count: 0,
            backend: None,
        }
    }
}

impl Gl {
    fn error(&mut self, e: u32) -> bool {
        self.error_count += 1;
        if self.errors.len() < 32 {
            self.errors.push(e);
        }
        false
    }
    fn get_error(&mut self) -> u32 {
        if self.errors.is_empty() {
            0
        } else {
            self.errors.remove(0)
        }
    }
    fn create(&mut self, k: Kind) -> u32 {
        let next = self.next.entry(k).or_insert(1);
        let n = *next;
        *next += 1;
        self.live.entry(k).or_default().insert(n);
        n
    }
    fn has(&self, k: Kind, n: u32) -> bool {
        self.live.get(&k).is_some_and(|s| s.contains(&n))
    }
    /// name 0 is "none" (valid where unbinding is); an unknown or deleted name is INVALID_OPERATION
    fn valid(&mut self, k: Kind, n: u32, allow_zero: bool) -> bool {
        if n == 0 {
            return allow_zero || self.error(INVALID_VALUE);
        }
        self.has(k, n) || self.error(INVALID_OPERATION)
    }
    fn remove(&mut self, k: Kind, n: u32) {
        if n != 0 {
            if let Some(s) = self.live.get_mut(&k) {
                s.remove(&n);
            }
        }
    }
    fn is(&self, k: Kind, n: u32) -> i32 {
        (n != 0 && self.has(k, n)) as i32
    }
    fn target(&mut self, list: &[u32], t: u32) -> bool {
        list.contains(&t) || self.error(INVALID_ENUM)
    }
    fn image_target(&mut self, t: u32) -> bool {
        t == TEXTURE_2D
            || (CUBE_POSITIVE_X..=CUBE_NEGATIVE_Z).contains(&t)
            || self.error(INVALID_ENUM)
    }
    fn bound_buffer(&self, t: u32) -> u32 {
        self.buffers.get(&t).copied().unwrap_or(0)
    }
    fn vao_element(&self) -> u32 {
        self.vao_elements
            .get(&self.vertex_array)
            .copied()
            .unwrap_or(0)
    }
    fn bound_texture(&self, t: u32) -> u32 {
        let t = if (CUBE_POSITIVE_X..=CUBE_NEGATIVE_Z).contains(&t) {
            TEXTURE_CUBE_MAP
        } else {
            t
        };
        self.textures.get(&(self.unit, t)).copied().unwrap_or(0)
    }
    fn need_texture(&mut self, t: u32) -> bool {
        self.bound_texture(t) != 0 || self.error(INVALID_OPERATION)
    }
    fn non_negative(&mut self, v: &[i32]) -> bool {
        v.iter().all(|&x| x >= 0) || self.error(INVALID_VALUE)
    }
    fn buffer_bound(&mut self, t: u32) -> bool {
        if !self.target(BUFFER_TARGETS, t) {
            return false;
        }
        let b = if t == ELEMENT_ARRAY_BUFFER {
            self.vao_element()
        } else {
            self.bound_buffer(t)
        };
        b != 0 || self.error(INVALID_OPERATION)
    }
    fn need_renderbuffer(&mut self, t: u32) -> bool {
        (self.target(&[RENDERBUFFER], t) && self.renderbuffer != 0)
            || (t == RENDERBUFFER && self.error(INVALID_OPERATION))
    }
    fn pixel_store(&mut self, pname: u32, v: i32) -> bool {
        let v = v as i64;
        let (unpack, field): (bool, fn(&mut Store) -> &mut i64) = match pname {
            0x0CF5 => (true, |s| &mut s.alignment),
            0x0D05 => (false, |s| &mut s.alignment),
            0x0CF2 => (true, |s| &mut s.row_length),
            0x806E => (true, |s| &mut s.image_height),
            0x0CF4 => (true, |s| &mut s.skip_pixels),
            0x0CF3 => (true, |s| &mut s.skip_rows),
            0x806D => (true, |s| &mut s.skip_images),
            0x0D02 => (false, |s| &mut s.row_length),
            0x0D04 => (false, |s| &mut s.skip_pixels),
            0x0D03 => (false, |s| &mut s.skip_rows),
            _ => return true,
        };
        let alignment = pname == 0x0CF5 || pname == 0x0D05;
        if if alignment {
            ![1, 2, 4, 8].contains(&v)
        } else {
            v < 0
        } {
            return self.error(INVALID_VALUE);
        }
        *field(if unpack {
            &mut self.unpack
        } else {
            &mut self.pack
        }) = v;
        true
    }
    /// Queries every runner answers from the model (bindings and the pixel store).
    fn model_param(&self, pname: u32) -> Option<i64> {
        let tex = |t| self.bound_texture(t) as i64;
        Some(match pname {
            0x8894 => self.bound_buffer(ARRAY_BUFFER) as i64,
            0x8895 => self.vao_element() as i64,
            0x8F36 => self.bound_buffer(COPY_READ_BUFFER) as i64,
            0x8F37 => self.bound_buffer(COPY_WRITE_BUFFER) as i64,
            0x88ED => self.bound_buffer(PIXEL_PACK_BUFFER) as i64,
            0x88EF => self.bound_buffer(PIXEL_UNPACK_BUFFER) as i64,
            0x8A28 => self.bound_buffer(UNIFORM_BUFFER) as i64,
            0x8C8F => self.bound_buffer(TRANSFORM_FEEDBACK_BUFFER) as i64,
            0x8B8D => self.program as i64,
            0x8069 => tex(TEXTURE_2D),
            0x8514 => tex(TEXTURE_CUBE_MAP),
            0x806A => tex(TEXTURE_3D),
            0x8C1D => tex(TEXTURE_2D_ARRAY),
            0x84E0 => (TEXTURE0 + self.unit) as i64,
            0x8CA6 => self
                .framebuffers
                .get(&DRAW_FRAMEBUFFER)
                .copied()
                .unwrap_or(0) as i64,
            0x8CAA => self
                .framebuffers
                .get(&READ_FRAMEBUFFER)
                .copied()
                .unwrap_or(0) as i64,
            0x8CA7 => self.renderbuffer as i64,
            0x85B5 => self.vertex_array as i64,
            0x0CF5 => self.unpack.alignment,
            0x0D05 => self.pack.alignment,
            0x0CF2 => self.unpack.row_length,
            0x806E => self.unpack.image_height,
            0x0CF4 => self.unpack.skip_pixels,
            0x0CF3 => self.unpack.skip_rows,
            0x806D => self.unpack.skip_images,
            0x0D02 => self.pack.row_length,
            0x0D04 => self.pack.skip_pixels,
            0x0D03 => self.pack.skip_rows,
            _ => return None,
        })
    }
    fn params(&self, pname: u32) -> Vec<i64> {
        match self.model_param(pname) {
            Some(v) => vec![v],
            None => null_limit(pname).to_vec(),
        }
    }
    /// The uniform a location names in the current program (None: ignored or an error).
    fn uniform_target(&mut self, loc: i32) -> bool {
        if loc == -1 {
            return false;
        }
        if self.program == 0 {
            return self.error(INVALID_OPERATION);
        }
        let known = self
            .uniforms
            .get(&self.program)
            .is_some_and(|u| loc >= 0 && loc < u.count);
        known || self.error(INVALID_OPERATION)
    }
}

/// Link every gasm:gl import (signatures from abi.json, via gl_sigs.rs).
pub(crate) fn add_gl_imports(linker: &mut Linker<Host>) -> wasmtime::Result<()> {
    let engine = linker.engine().clone();
    for &(name, params, result) in sigs::FUNCTIONS {
        let ty = |c: char| match c {
            'i' => ValType::I32,
            'I' => ValType::I64,
            'f' => ValType::F32,
            _ => unreachable!("gl_sigs.rs: type {c}"),
        };
        let ft = FuncType::new(&engine, params.chars().map(ty), result.chars().map(ty));
        let f32_result = result == "f";
        linker.func_new("gasm:gl", name, ft, move |mut c, args, results| {
            call(&mut c, name, f32_result, args, results)
        })?;
    }
    Ok(())
}

/// Write `values` to guest memory as i32 / f32 / i64 (up to `count` of them); returns how many there are.
fn put(mem: &mut [u8], dst: u32, count: u32, values: &[i64], kind: char) -> wasmtime::Result<i32> {
    let n = values.len().min(count as usize);
    let size = if kind == 'I' { 8 } else { 4 };
    if n > 0 {
        let out = guest_slice_mut(mem, dst, (n * size) as u64)?;
        for (i, &v) in values[..n].iter().enumerate() {
            match kind {
                'f' => out[i * 4..i * 4 + 4].copy_from_slice(&(v as f32).to_le_bytes()),
                'I' => out[i * 8..i * 8 + 8].copy_from_slice(&v.to_le_bytes()),
                _ => out[i * 4..i * 4 + 4].copy_from_slice(&(v as i32).to_le_bytes()),
            }
        }
    }
    Ok(values.len() as i32)
}

fn text(mem: &mut [u8], dst: u32, cap: u32, s: &str) -> wasmtime::Result<i32> {
    if s.len() <= cap as usize {
        guest_slice_mut(mem, dst, s.len() as u64)?.copy_from_slice(s.as_bytes());
    }
    Ok(s.len() as i32)
}

fn guest_str(mem: &[u8], ptr: u32, len: u32) -> wasmtime::Result<String> {
    let b = guest_slice(mem, ptr, len as u64)?;
    std::str::from_utf8(b)
        .map(str::to_owned)
        .map_err(|_| wasmtime::format_err!("string argument is not UTF-8"))
}

/// One gasm:gl call: the model checks it (and answers what it can); if it recorded
/// no GL error and didn't trap, the backend (if any) executes it, as WebGL does in
/// the browser runner.
fn call(c: &mut Caller<'_, Host>, name: &str, f32_result: bool, args: &[Val], results: &mut [Val]) -> wasmtime::Result<()> {
    let before = c.data().gl.error_count;
    model_call(c, name, f32_result, args, results)?;
    if c.data().gl.backend.is_none() || c.data().gl.error_count != before {
        return Ok(());
    }
    let mem = memory(c)?;
    let (mem, host) = mem.data_and_store_mut(c);
    let gl = &mut host.gl;
    let mut be = gl.backend.take().expect("backend");
    let r = backend::forward(&mut be, gl, name, args, results, mem);
    gl.backend = Some(be);
    r
}

/// The model's part of a call. `a(i)` is argument i as the i32 the guest passed (`u(i)` as u32).
fn model_call(
    c: &mut Caller<'_, Host>,
    name: &str,
    f32_result: bool,
    args: &[Val],
    results: &mut [Val],
) -> wasmtime::Result<()> {
    let mem = memory(c)?;
    let (mem, host) = mem.data_and_store_mut(c);
    let hashing = host.hashing;
    let frame = host.frame_index;
    let show = host.show_frame;
    let (gl, hash) = (&mut host.gl, &mut host.video_hash);
    let a = |i: usize| -> i32 {
        match args[i] {
            Val::I32(v) => v,
            Val::I64(v) => v as i32,
            Val::F32(b) => b as i32,
            _ => 0,
        }
    };
    let u = |i: usize| a(i) as u32;
    let mut fold = |header: &[u32], payload: Option<&[u8]>| {
        if !hashing {
            return;
        }
        for v in header {
            hash.update(&v.to_le_bytes());
        }
        if let Some(p) = payload {
            hash.update(p);
        }
    };
    // the result: 0 unless set; an f32 for the f32 results
    if let Some(r) = results.first_mut() {
        *r = if f32_result { Val::F32(0) } else { Val::I32(0) };
    }
    let mut ret = |v: i64| {
        if let Some(r) = results.first_mut() {
            *r = match r {
                Val::F32(_) => Val::F32((v as f32).to_bits()),
                _ => Val::I32(v as i32),
            };
        }
    };
    use Kind::*;
    match name {
        // ---- frames, context ----
        "width" => ret(1280),
        "height" => ret(720),
        "frame_shown" => ret(show as i64),
        "get_error" => ret(gl.get_error() as i64),
        "get_string" => match null_string(u(0)) {
            Some(s) => ret(text(mem, u(1), u(2), s)? as i64),
            None => {
                gl.error(INVALID_ENUM);
                ret(-1)
            }
        },
        "enable_extension" => {
            guest_str(mem, u(0), u(1))?;
            ret(0)
        }
        "get_integerv" | "get_floatv" | "get_integer64v" => {
            let v = gl.params(u(0));
            let kind = match name {
                "get_floatv" => 'f',
                "get_integer64v" => 'I',
                _ => 'i',
            };
            ret(put(mem, u(1), u(2), &v, kind)? as i64)
        }
        "get_integeri_v" => ret(put(mem, u(2), u(3), &[0], 'i')? as i64),
        "get_internalformativ" => {
            ret(put(mem, u(3), u(4), &[if u(2) == 0x9380 { 1 } else { 4 }], 'i')? as i64)
        }
        "get_shader_precision_format" => {
            put(mem, u(2), 3, &[127, 127, 23], 'i')?;
        }
        "active_texture" => {
            let unit = u(0).wrapping_sub(TEXTURE0);
            if unit >= 32 {
                gl.error(INVALID_ENUM);
            } else {
                gl.unit = unit;
            }
        }
        "is_enabled" => ret(0),
        "pixel_storei" => {
            gl.pixel_store(u(0), a(1));
        }
        "scissor" | "viewport" => {
            gl.non_negative(&[a(2), a(3)]);
        }
        // state that only a real GL keeps
        "present"
        | "blend_color"
        | "blend_equation"
        | "blend_equation_separate"
        | "blend_func"
        | "blend_func_separate"
        | "clear"
        | "clear_color"
        | "clear_depthf"
        | "clear_stencil"
        | "color_mask"
        | "cull_face"
        | "depth_func"
        | "depth_mask"
        | "depth_rangef"
        | "disable"
        | "enable"
        | "front_face"
        | "hint"
        | "line_width"
        | "polygon_offset"
        | "sample_coverage"
        | "stencil_func"
        | "stencil_func_separate"
        | "stencil_mask"
        | "stencil_mask_separate"
        | "stencil_op"
        | "stencil_op_separate"
        | "finish"
        | "flush"
        | "enable_vertex_attrib_array"
        | "disable_vertex_attrib_array"
        | "vertex_attrib_divisor"
        | "vertex_attrib4f"
        | "vertex_attribi4i"
        | "vertex_attribi4ui"
        | "clear_bufferfi"
        | "blit_framebuffer"
        | "read_buffer"
        | "begin_transform_feedback"
        | "end_transform_feedback"
        | "pause_transform_feedback"
        | "resume_transform_feedback" => {}

        // ---- buffers ----
        "create_buffer" => ret(gl.create(Buffer) as i64),
        "delete_buffer" => {
            let n = u(0);
            gl.buffers.retain(|_, b| *b != n);
            for b in gl.vao_elements.values_mut() {
                if *b == n {
                    *b = 0;
                }
            }
            gl.remove(Buffer, n);
        }
        "is_buffer" => ret(gl.is(Buffer, u(0)) as i64),
        "bind_buffer" => {
            let (t, n) = (u(0), u(1));
            if gl.target(BUFFER_TARGETS, t) && gl.valid(Buffer, n, true) {
                if t == ELEMENT_ARRAY_BUFFER {
                    gl.vao_elements.insert(gl.vertex_array, n);
                } else {
                    gl.buffers.insert(t, n);
                }
            }
        }
        "bind_buffer_base" | "bind_buffer_range" => {
            let (t, n) = (u(0), u(2));
            if gl.target(INDEXED_TARGETS, t) && gl.valid(Buffer, n, true) {
                gl.buffers.insert(t, n);
            }
        }
        "buffer_data" => {
            let (t, ptr, len) = (u(0), u(1), u(2));
            let data = if ptr != 0 {
                Some(guest_slice(mem, ptr, len as u64)?)
            } else {
                None
            };
            if gl.buffer_bound(t) {
                fold(&[1, t, 0, len], data);
            }
        }
        "buffer_sub_data" => {
            let (t, off, ptr, len) = (u(0), u(1), u(2), u(3));
            let data = guest_slice(mem, ptr, len as u64)?;
            if gl.buffer_bound(t) {
                fold(&[1, t, off, len], Some(data));
            }
        }
        "copy_buffer_sub_data" => {
            let _ = gl.buffer_bound(u(0)) && gl.buffer_bound(u(1));
        }
        "get_buffer_sub_data" => {
            let out = guest_slice_mut(mem, u(2), u(3) as u64)?;
            if gl.buffer_bound(u(0)) {
                out.fill(0);
            }
        }
        "get_buffer_parameteriv" => {
            gl.buffer_bound(u(0));
            ret(0)
        }

        // ---- vertex arrays ----
        "create_vertex_array" => {
            let n = gl.create(VertexArray);
            gl.vao_elements.insert(n, 0);
            ret(n as i64)
        }
        "delete_vertex_array" => {
            let n = u(0);
            if gl.vertex_array == n {
                gl.vertex_array = 0;
            }
            gl.vao_elements.remove(&n);
            gl.remove(VertexArray, n);
        }
        "is_vertex_array" => ret(gl.is(VertexArray, u(0)) as i64),
        "bind_vertex_array" => {
            if gl.valid(VertexArray, u(0), true) {
                gl.vertex_array = u(0);
            }
        }
        "vertex_attrib_pointer" | "vertex_attrib_ipointer" => {
            let off = if name == "vertex_attrib_pointer" {
                a(5)
            } else {
                a(4)
            };
            if gl.bound_buffer(ARRAY_BUFFER) == 0 && off != 0 {
                gl.error(INVALID_OPERATION);
            }
        }
        "get_vertex_attribiv" | "get_vertex_attrib_offset" => ret(0),
        "get_vertex_attribfv" => ret(put(mem, u(2), u(3), &[0, 0, 0, 1], 'f')? as i64),

        // ---- drawing ----
        "draw_arrays" => {
            gl.non_negative(&[a(1), a(2)]);
        }
        "draw_elements" => {
            gl.non_negative(&[a(1)]);
        }
        "draw_arrays_instanced" => {
            gl.non_negative(&[a(1), a(2), a(3)]);
        }
        "draw_elements_instanced" => {
            gl.non_negative(&[a(1), a(4)]);
        }
        "draw_range_elements" => {
            let _ = gl.non_negative(&[a(3)]) && (a(2) >= a(1) || gl.error(INVALID_VALUE));
        }
        "draw_buffers" => {
            guest_slice(mem, u(0), u(1) as u64 * 4)?;
        }
        "clear_bufferiv" | "clear_bufferuiv" | "clear_bufferfv" => {
            guest_slice(mem, u(2), u(3) as u64 * 4)?;
        }

        // ---- textures ----
        "create_texture" => ret(gl.create(Texture) as i64),
        "delete_texture" => {
            let n = u(0);
            gl.textures.retain(|_, t| *t != n);
            gl.texture_targets.remove(&n);
            gl.remove(Texture, n);
        }
        "is_texture" => ret(gl.is(Texture, u(0)) as i64),
        "bind_texture" => {
            let (t, n) = (u(0), u(1));
            if gl.target(TEXTURE_TARGETS, t) && gl.valid(Texture, n, true) {
                if n != 0 && *gl.texture_targets.entry(n).or_insert(t) != t {
                    gl.error(INVALID_OPERATION);
                } else {
                    gl.textures.insert((gl.unit, t), n);
                }
            }
        }
        "tex_parameteri"
        | "tex_parameterf"
        | "get_tex_parameteriv"
        | "get_tex_parameterfv"
        | "generate_mipmap" => {
            let _ = gl.target(TEXTURE_TARGETS, u(0)) && gl.need_texture(u(0));
            ret(0)
        }
        "tex_image_2d" | "tex_image_3d" | "tex_sub_image_2d" | "tex_sub_image_3d" => {
            // (target, level, [ifmt], [x, y, [z]], w, h, [d], [border], format, type, ptr, len)
            let t = u(0);
            let sub = name.starts_with("tex_sub");
            let three = name.ends_with("3d");
            let (ifmt, x, y, z, w, h, d, fi) = match (sub, three) {
                (false, false) => (u(2), 0, 0, 0, a(3), a(4), 1, 6),
                (false, true) => (u(2), 0, 0, 0, a(3), a(4), a(5), 7),
                (true, false) => (0, a(2), a(3), 0, a(4), a(5), 1, 6),
                (true, true) => (0, a(2), a(3), a(4), a(5), a(6), a(7), 8),
            };
            let (format, ty, ptr, len) = (u(fi), u(fi + 1), u(fi + 2), u(fi + 3));
            let level = a(1);
            let ok = if three {
                gl.target(VOLUME_TARGETS, t)
            } else {
                gl.image_target(t)
            } && gl.need_texture(t)
                && gl.non_negative(&[level, x, y, z, w, h, d]);
            if !ok {
                return Ok(());
            }
            // pixels: none, guest bytes, or (unpack buffer bound) an offset into it
            let unpack = gl.bound_buffer(PIXEL_UNPACK_BUFFER) != 0;
            let mut data = None;
            if !unpack && ptr != 0 {
                let bpp = pixel_bytes(format, ty);
                if bpp == 0 {
                    gl.error(INVALID_ENUM);
                    return Ok(());
                }
                let need = image_bytes(w as i64, h as i64, d as i64, bpp as i64, &gl.unpack);
                if (len as i64) < need {
                    bail!(
                        "gasm:gl: {len} bytes of pixels for a {w}x{h}x{d} image that needs {need}"
                    );
                }
                data = Some(guest_slice(mem, ptr, len as u64)?);
            }
            if sub && data.is_none() && !unpack {
                gl.error(INVALID_VALUE);
                return Ok(());
            }
            let n = if data.is_some() { len } else { 0 };
            fold(
                &[
                    2,
                    t,
                    level as u32,
                    ifmt,
                    x as u32,
                    y as u32,
                    z as u32,
                    w as u32,
                    h as u32,
                    d as u32,
                    format,
                    ty,
                    n,
                ],
                data,
            );
        }
        "tex_storage_2d" | "tex_storage_3d" => {
            let t = u(0);
            let three = name.ends_with("3d");
            let list: &[u32] = if three {
                VOLUME_TARGETS
            } else {
                &[TEXTURE_2D, TEXTURE_CUBE_MAP]
            };
            let positive = a(1) > 0 && a(3) > 0 && a(4) > 0 && (!three || a(5) > 0);
            let _ =
                gl.target(list, t) && gl.need_texture(t) && (positive || gl.error(INVALID_VALUE));
        }
        "compressed_tex_image_2d"
        | "compressed_tex_image_3d"
        | "compressed_tex_sub_image_2d"
        | "compressed_tex_sub_image_3d" => {
            let t = u(0);
            let sub = name.contains("sub");
            let three = name.ends_with("3d");
            // header [2, t, level, ifmt|0, x, y, z, w, h, d, ifmt|format, 0, len]
            let (ifmt, x, y, z, w, h, d, fmt, pi) = match (sub, three) {
                (false, false) => (u(2), 0, 0, 0, a(3), a(4), 1, u(2), 6),
                (false, true) => (u(2), 0, 0, 0, a(3), a(4), a(5), u(2), 7),
                (true, false) => (0, a(2), a(3), 0, a(4), a(5), 1, u(6), 7),
                (true, true) => (0, a(2), a(3), a(4), a(5), a(6), a(7), u(8), 9),
            };
            let (ptr, len) = (u(pi), u(pi + 1));
            let data = guest_slice(mem, ptr, len as u64)?;
            let level = a(1);
            let ok = if three {
                gl.target(VOLUME_TARGETS, t)
            } else {
                gl.image_target(t)
            } && gl.need_texture(t)
                && {
                    let v: &[i32] = if sub {
                        &[level, x, y, z, w, h, d]
                    } else {
                        &[level, w, h, d]
                    };
                    gl.non_negative(v)
                };
            if ok {
                fold(
                    &[
                        2,
                        t,
                        level as u32,
                        ifmt,
                        x as u32,
                        y as u32,
                        z as u32,
                        w as u32,
                        h as u32,
                        d as u32,
                        fmt,
                        0,
                        len,
                    ],
                    Some(data),
                );
            }
        }
        "copy_tex_image_2d" | "copy_tex_sub_image_2d" => {
            let _ = gl.image_target(u(0)) && gl.need_texture(u(0));
        }
        "copy_tex_sub_image_3d" => {
            gl.need_texture(u(0));
        }

        // ---- samplers ----
        "create_sampler" => ret(gl.create(Sampler) as i64),
        "delete_sampler" => gl.remove(Sampler, u(0)),
        "is_sampler" => ret(gl.is(Sampler, u(0)) as i64),
        "bind_sampler" => {
            gl.valid(Sampler, u(1), true);
        }
        "sampler_parameteri"
        | "sampler_parameterf"
        | "get_sampler_parameteriv"
        | "get_sampler_parameterfv" => {
            gl.valid(Sampler, u(0), false);
            ret(0)
        }

        // ---- framebuffers, renderbuffers ----
        "create_framebuffer" => ret(gl.create(Framebuffer) as i64),
        "delete_framebuffer" => {
            let n = u(0);
            gl.framebuffers.retain(|_, f| *f != n);
            gl.remove(Framebuffer, n);
        }
        "is_framebuffer" => ret(gl.is(Framebuffer, u(0)) as i64),
        "bind_framebuffer" => {
            let (t, n) = (u(0), u(1));
            if gl.target(FB_TARGETS, t) && gl.valid(Framebuffer, n, true) {
                if t == FRAMEBUFFER {
                    gl.framebuffers.insert(DRAW_FRAMEBUFFER, n);
                    gl.framebuffers.insert(READ_FRAMEBUFFER, n);
                } else {
                    gl.framebuffers.insert(t, n);
                }
            }
        }
        "check_framebuffer_status" => ret(if gl.target(FB_TARGETS, u(0)) {
            FRAMEBUFFER_COMPLETE as i64
        } else {
            0
        }),
        "framebuffer_texture_2d" => {
            let _ = gl.target(FB_TARGETS, u(0)) && gl.valid(Texture, u(3), true);
        }
        "framebuffer_texture_layer" => {
            let _ = gl.target(FB_TARGETS, u(0)) && gl.valid(Texture, u(2), true);
        }
        "framebuffer_renderbuffer" => {
            let _ = gl.target(FB_TARGETS, u(0)) && gl.valid(Renderbuffer, u(3), true);
        }
        "get_framebuffer_attachment_parameteriv" => {
            gl.target(FB_TARGETS, u(0));
            ret(0)
        }
        "invalidate_framebuffer" | "invalidate_sub_framebuffer" => {
            guest_slice(mem, u(1), u(2) as u64 * 4)?;
        }
        "read_pixels" => {
            let (w, h, format, ty, dst, len) = (a(2), a(3), u(4), u(5), u(6), u(7));
            if !gl.non_negative(&[w, h]) || gl.bound_buffer(PIXEL_PACK_BUFFER) != 0 {
                return Ok(());
            }
            let bpp = pixel_bytes(format, ty);
            if bpp == 0 {
                gl.error(INVALID_ENUM);
                return Ok(());
            }
            let need = image_bytes(w as i64, h as i64, 1, bpp as i64, &gl.pack);
            if (len as i64) < need {
                bail!("gasm:gl: read_pixels: {len} bytes for {w}x{h} that needs {need}");
            }
            guest_slice_mut(mem, dst, len as u64)?[..need as usize].fill(0);
        }
        "create_renderbuffer" => ret(gl.create(Renderbuffer) as i64),
        "delete_renderbuffer" => {
            if gl.renderbuffer == u(0) {
                gl.renderbuffer = 0;
            }
            gl.remove(Renderbuffer, u(0));
        }
        "is_renderbuffer" => ret(gl.is(Renderbuffer, u(0)) as i64),
        "bind_renderbuffer" => {
            if gl.target(&[RENDERBUFFER], u(0)) && gl.valid(Renderbuffer, u(1), true) {
                gl.renderbuffer = u(1);
            }
        }
        "renderbuffer_storage" => {
            let _ = gl.need_renderbuffer(u(0)) && gl.non_negative(&[a(2), a(3)]);
        }
        "renderbuffer_storage_multisample" => {
            let _ = gl.need_renderbuffer(u(0)) && gl.non_negative(&[a(1), a(3), a(4)]);
        }
        "get_renderbuffer_parameteriv" => {
            gl.need_renderbuffer(u(0));
            ret(0)
        }

        // ---- shaders and programs ----
        "create_shader" => {
            let ty = u(0);
            if ty != VERTEX_SHADER && ty != FRAGMENT_SHADER {
                gl.error(INVALID_ENUM);
                ret(0);
                return Ok(());
            }
            let n = gl.create(Shader);
            gl.shaders.insert(n, (ty, String::new()));
            ret(n as i64)
        }
        "delete_shader" => gl.remove(Shader, u(0)),
        "is_shader" => ret(gl.is(Shader, u(0)) as i64),
        "shader_source" => {
            let src = guest_str(mem, u(1), u(2))?;
            if gl.valid(Shader, u(0), false) {
                if let Some(s) = gl.shaders.get_mut(&u(0)) {
                    s.1 = src;
                }
            }
        }
        "compile_shader" => {
            gl.valid(Shader, u(0), false);
        }
        "get_shaderiv" => {
            if !gl.valid(Shader, u(0), false) {
                ret(0);
                return Ok(());
            }
            let (ty, src) = gl.shaders.get(&u(0)).cloned().unwrap_or_default();
            ret(match u(1) {
                SHADER_TYPE => ty as i64,
                SHADER_SOURCE_LENGTH if !src.is_empty() => src.len() as i64 + 1,
                COMPILE_STATUS => 1,
                _ => 0,
            })
        }
        "get_shader_info_log" => ret(if gl.valid(Shader, u(0), false) {
            text(mem, u(1), u(2), "")? as i64
        } else {
            -1
        }),
        "get_shader_source" => {
            if gl.valid(Shader, u(0), false) {
                let src = gl
                    .shaders
                    .get(&u(0))
                    .map(|s| s.1.clone())
                    .unwrap_or_default();
                ret(text(mem, u(1), u(2), &src)? as i64)
            } else {
                ret(-1)
            }
        }
        "create_program" => ret(gl.create(Program) as i64),
        "delete_program" => {
            let n = u(0);
            if gl.program == n {
                gl.program = 0;
            }
            gl.uniforms.remove(&n);
            gl.remove(Program, n);
        }
        "is_program" => ret(gl.is(Program, u(0)) as i64),
        "attach_shader" | "detach_shader" => {
            let _ = gl.valid(Program, u(0), false) && gl.valid(Shader, u(1), false);
        }
        "link_program" => {
            if gl.valid(Program, u(0), false) {
                gl.uniforms.remove(&u(0)); // locations are the link's
            }
        }
        "use_program" => {
            if gl.valid(Program, u(0), true) {
                gl.program = u(0);
            }
        }
        "validate_program" | "uniform_block_binding" => {
            gl.valid(Program, u(0), false);
        }
        "get_programiv" => {
            let ok = gl.valid(Program, u(0), false);
            ret((ok && (u(1) == LINK_STATUS || u(1) == VALIDATE_STATUS)) as i64)
        }
        "get_program_info_log" | "get_active_uniform_block_name" => {
            let (dst, cap) = if name == "get_program_info_log" {
                (u(1), u(2))
            } else {
                (u(2), u(3))
            };
            ret(if gl.valid(Program, u(0), false) {
                text(mem, dst, cap, "")? as i64
            } else {
                -1
            })
        }
        "get_attached_shaders" => {
            if gl.valid(Program, u(0), false) {
                put(mem, u(1), u(2), &[], 'i')?;
            }
            ret(0)
        }
        "bind_attrib_location" => {
            guest_str(mem, u(2), u(3))?;
            gl.valid(Program, u(0), false);
        }
        "get_attrib_location" | "get_frag_data_location" => {
            guest_str(mem, u(1), u(2))?;
            ret(if gl.valid(Program, u(0), false) {
                0
            } else {
                -1
            })
        }
        "get_active_attrib" | "get_active_uniform" | "get_transform_feedback_varying" => {
            gl.valid(Program, u(0), false);
            ret(-1)
        }
        // locations are numbered per program in the order the guest asks for them
        "get_uniform_location" => {
            let name = guest_str(mem, u(1), u(2))?;
            if !gl.valid(Program, u(0), false) {
                ret(-1);
                return Ok(());
            }
            let table = gl.uniforms.entry(u(0)).or_default();
            let loc = match table.by_name.get(&name) {
                Some(&l) => l,
                None => {
                    let l = table.count;
                    table.count += 1;
                    table.by_name.insert(name, l);
                    l
                }
            };
            ret(loc as i64)
        }
        "get_uniform_index" | "get_uniform_block_index" => {
            guest_str(mem, u(1), u(2))?;
            ret(if gl.valid(Program, u(0), false) {
                0
            } else {
                INVALID_INDEX as i32 as i64
            })
        }
        "get_active_uniformsiv" => {
            let count = u(2);
            guest_slice(mem, u(1), count as u64 * 4)?;
            if gl.valid(Program, u(0), false) {
                put(mem, u(4), count, &vec![0; count as usize], 'i')?;
            }
        }
        "get_active_uniform_blockiv" => ret(if gl.valid(Program, u(0), false) {
            put(mem, u(3), u(4), &[0], 'i')? as i64
        } else {
            -1
        }),
        "get_uniformfv" | "get_uniformiv" | "get_uniformuiv" => {
            if !gl.valid(Program, u(0), false) {
                ret(-1);
                return Ok(());
            }
            let loc = a(1);
            if !gl
                .uniforms
                .get(&u(0))
                .is_some_and(|t| loc >= 0 && loc < t.count)
            {
                gl.error(INVALID_OPERATION);
                ret(-1);
                return Ok(());
            }
            ret(put(
                mem,
                u(2),
                u(3),
                &[0],
                if name == "get_uniformfv" { 'f' } else { 'i' },
            )? as i64)
        }
        "transform_feedback_varyings" => {
            let bytes = guest_slice(mem, u(1), u(2) as u64)?;
            let pieces = bytes.iter().filter(|&&b| b == 0).count() as i64 + 1;
            if a(3) as i64 > pieces {
                bail!("gasm:gl: transform_feedback_varyings: fewer names than count");
            }
            gl.valid(Program, u(0), false);
        }

        // ---- uniforms: header [3, location, kind, count(, transpose)] + the values ----
        n if n.starts_with("uniform") && n != "uniform_block_binding" => {
            let (kind, comps, vector, matrix) = uniform_kind(n);
            let loc = a(0);
            if !vector {
                if !gl.uniform_target(loc) {
                    return Ok(());
                }
                let mut bytes = Vec::with_capacity(16);
                for v in &args[1..] {
                    match *v {
                        Val::F32(b) => bytes.extend_from_slice(&b.to_le_bytes()),
                        Val::I32(i) => bytes.extend_from_slice(&i.to_le_bytes()),
                        _ => {}
                    }
                }
                fold(&[3, loc as u32, kind, 1], Some(&bytes));
                return Ok(());
            }
            let count = a(1);
            if count < 0 {
                gl.error(INVALID_VALUE);
                return Ok(());
            }
            let ptr = if matrix { u(3) } else { u(2) };
            let bytes = guest_slice(mem, ptr, count as u64 * comps as u64 * 4)?;
            if !gl.uniform_target(loc) {
                return Ok(());
            }
            if matrix {
                fold(
                    &[3, loc as u32, kind, count as u32, (a(2) != 0) as u32],
                    Some(bytes),
                );
            } else {
                fold(&[3, loc as u32, kind, count as u32], Some(bytes));
            }
        }

        // ---- queries, sync (results from the next frame on), transform feedback ----
        "create_query" => ret(gl.create(Query) as i64),
        "delete_query" => gl.remove(Query, u(0)),
        "is_query" => ret(gl.is(Query, u(0)) as i64),
        "begin_query" => {
            let (t, n) = (u(0), u(1));
            if gl.target(QUERY_TARGETS, t) && gl.valid(Query, n, false) {
                gl.active.insert(t, n);
                gl.query_ended.remove(&n);
            }
        }
        "end_query" => {
            let t = u(0);
            if gl.target(QUERY_TARGETS, t) {
                match gl.active.remove(&t) {
                    Some(n) if n != 0 => {
                        gl.query_ended.insert(n, frame);
                    }
                    _ => {
                        gl.error(INVALID_OPERATION);
                    }
                }
            }
        }
        "get_queryiv" => {
            let ok = gl.target(QUERY_TARGETS, u(0)) && u(1) == CURRENT_QUERY;
            ret(if ok {
                gl.active.get(&u(0)).copied().unwrap_or(0) as i64
            } else {
                0
            })
        }
        "get_query_objectuiv" => {
            let n = u(0);
            if !gl.valid(Query, n, false) {
                ret(0);
                return Ok(());
            }
            let ready = gl.query_ended.get(&n).is_some_and(|&e| frame > e);
            ret(match u(1) {
                QUERY_RESULT_AVAILABLE => ready as i64,
                QUERY_RESULT => ready as i64, // occlusion "passed"
                _ => {
                    gl.error(INVALID_ENUM);
                    0
                }
            })
        }
        "fence_sync" => {
            let (cond, flags) = (u(0), u(1));
            if cond != SYNC_GPU_COMMANDS_COMPLETE || flags != 0 {
                gl.error(if cond != SYNC_GPU_COMMANDS_COMPLETE {
                    INVALID_ENUM
                } else {
                    INVALID_VALUE
                });
                ret(0);
                return Ok(());
            }
            let n = gl.create(Sync);
            gl.sync_made.insert(n, frame);
            ret(n as i64)
        }
        "is_sync" => ret(gl.is(Sync, u(0)) as i64),
        "delete_sync" => {
            gl.sync_made.remove(&u(0));
            gl.remove(Sync, u(0));
        }
        "client_wait_sync" => {
            let n = u(0);
            if !gl.valid(Sync, n, false) {
                ret(WAIT_FAILED as i64);
                return Ok(());
            }
            let done = gl.sync_made.get(&n).is_some_and(|&m| frame > m);
            ret((if done {
                ALREADY_SIGNALED
            } else {
                TIMEOUT_EXPIRED
            }) as i64)
        }
        "wait_sync" => {
            gl.valid(Sync, u(0), false);
        }
        "get_synciv" => {
            let n = u(0);
            if !gl.valid(Sync, n, false) {
                ret(0);
                return Ok(());
            }
            let done = gl.sync_made.get(&n).is_some_and(|&m| frame > m);
            ret(if u(1) != SYNC_STATUS {
                0
            } else if done {
                SIGNALED as i64
            } else {
                UNSIGNALED as i64
            })
        }
        "create_transform_feedback" => ret(gl.create(TransformFeedback) as i64),
        "delete_transform_feedback" => gl.remove(TransformFeedback, u(0)),
        "is_transform_feedback" => ret(gl.is(TransformFeedback, u(0)) as i64),
        "bind_transform_feedback" => {
            let _ =
                gl.target(&[TRANSFORM_FEEDBACK], u(0)) && gl.valid(TransformFeedback, u(1), true);
        }
        _ => bail!("gasm:gl.{name}: not implemented by this runner"),
    }
    Ok(())
}

/// A uniform import's hash kind (1-4 float, 11-14 int, 21-24 uint, 32-40 matrices),
/// components per element, whether it takes (count, ptr), whether it is a matrix.
fn uniform_kind(name: &str) -> (u32, u32, bool, bool) {
    const MATRICES: [(&str, u32, u32); 9] = [
        ("2fv", 32, 4),
        ("3fv", 33, 9),
        ("4fv", 34, 16),
        ("2x3fv", 35, 6),
        ("3x2fv", 36, 6),
        ("2x4fv", 37, 8),
        ("4x2fv", 38, 8),
        ("3x4fv", 39, 12),
        ("4x3fv", 40, 12),
    ];
    if let Some(m) = name.strip_prefix("uniform_matrix") {
        let &(_, kind, comps) = MATRICES
            .iter()
            .find(|(s, _, _)| *s == m)
            .expect("matrix uniform");
        return (kind, comps, true, true);
    }
    let rest = &name["uniform".len()..];
    let n: u32 = rest[..1].parse().unwrap_or(1);
    let suffix = &rest[1..];
    let base = match suffix.trim_end_matches('v') {
        "f" => 0,
        "i" => 10,
        _ => 20, // "ui"
    };
    (base + n, n, suffix.ends_with('v'), false)
}
