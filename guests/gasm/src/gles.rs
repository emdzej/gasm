//! The OpenGL ES 3.0 C API on gasm:gl, for Rust guests: the same functions as the
//! C SDK's `gasm_gl.c` (there is no libc to link it), with C signatures, so GL
//! loaders can use them through [`get_proc_address`]. glow's native backend runs
//! on them (sdk/glow, design/gasm-gl.md):
//!
//! ```ignore
//! let gl = unsafe { glow::Context::from_loader_function_cstr(gasm::gles::get_proc_address) };
//! ```
//!
//! Most functions forward one to one (generated: gles_gen.rs); the ones here adapt
//! GL's C conventions to the ABI: name arrays, string arrays, the pixel store (upload
//! lengths are exact), cached glGetString pointers, glMapBufferRange in guest memory.
//! Unused functions cost nothing: only what a game (or its loader) references is linked.

#![allow(non_snake_case, clippy::missing_safety_doc, clippy::too_many_arguments)]

use std::ffi::{c_void, CStr};
use std::sync::Mutex;

use crate::sys;

include!("gles_gen.rs");

/// A GLES 3.0 entry point by name (null for anything else): the loader for glow.
pub fn get_proc_address(name: &CStr) -> *const c_void {
    lookup(name.to_bytes())
}

const GL_INVALID_ENUM: u32 = 0x0500;
const GL_INVALID_VALUE: u32 = 0x0501;
const GL_INVALID_OPERATION: u32 = 0x0502;
const GL_COLOR: u32 = 0x1800;
const GL_PIXEL_PACK_BUFFER: u32 = 0x88EB;
const GL_PIXEL_UNPACK_BUFFER: u32 = 0x88EC;
const GL_EXTENSIONS: u32 = 0x1F03;
const GL_NUM_EXTENSIONS: u32 = 0x821D;
const GL_MAP_READ_BIT: u32 = 0x0001;
const GL_MAP_WRITE_BIT: u32 = 0x0002;
const GL_MAP_INVALIDATE_RANGE_BIT: u32 = 0x0004;
const GL_MAP_INVALIDATE_BUFFER_BIT: u32 = 0x0008;
const GL_MAP_FLUSH_EXPLICIT_BIT: u32 = 0x0010;

#[derive(Clone, Copy)]
struct Store {
    alignment: i32,
    row_length: i32,
    image_height: i32,
    skip_pixels: i32,
    skip_rows: i32,
    skip_images: i32,
}
const STORE: Store = Store { alignment: 4, row_length: 0, image_height: 0, skip_pixels: 0, skip_rows: 0, skip_images: 0 };

struct Mapping {
    target: u32,
    data: Vec<u8>,
    offset: isize,
    access: u32,
}

struct State {
    /// errors found here (no binary formats, bad indices)
    error: u32,
    unpack: Store,
    pack: Store,
    unpack_buffer: u32,
    pack_buffer: u32,
    /// glGetString results: they stay valid
    strings: Vec<(u32, Box<[u8]>)>,
    extensions: Option<Vec<Box<[u8]>>>,
    maps: Vec<Mapping>,
}

static STATE: Mutex<State> = Mutex::new(State {
    error: 0,
    unpack: STORE,
    pack: STORE,
    unpack_buffer: 0,
    pack_buffer: 0,
    strings: Vec::new(),
    extensions: None,
    maps: Vec::new(),
});

fn state<R>(f: impl FnOnce(&mut State) -> R) -> R {
    f(&mut STATE.lock().unwrap_or_else(|e| e.into_inner()))
}

fn set_error(e: u32) {
    state(|s| s.error = e);
}

fn pixel_bytes(format: u32, ty: u32) -> u32 {
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

/// Bytes of a w×h×d image under a pixel store (GLES 3.0 3.7.1; the same formula in the runners).
fn image_bytes(s: &Store, w: i32, h: i32, d: i32, format: u32, ty: u32) -> u32 {
    let bpp = pixel_bytes(format, ty) as u64;
    if w <= 0 || h <= 0 || d <= 0 || bpp == 0 || s.alignment <= 0 {
        return 0;
    }
    let (w, h, d) = (w as u64, h as u64, d as u64);
    let row_len = if s.row_length > 0 { s.row_length as u64 } else { w };
    let a = s.alignment as u64;
    let row = (row_len * bpp).div_ceil(a) * a;
    let img_h = if s.image_height > 0 { s.image_height as u64 } else { h };
    let img = row * img_h;
    (s.skip_images as u64 * img + (d - 1) * img + s.skip_rows as u64 * row + (h - 1) * row + s.skip_pixels as u64 * bpp + w * bpp) as u32
}

/// An upload's pixels: (pointer, len); with an unpack buffer bound, the pointer is an offset.
fn unpack(pixels: *const c_void, w: i32, h: i32, d: i32, format: u32, ty: u32) -> (*const u8, u32) {
    state(|s| {
        if s.unpack_buffer != 0 {
            (std::ptr::null(), pixels as usize as u32)
        } else if pixels.is_null() {
            (std::ptr::null(), 0)
        } else {
            (pixels as *const u8, image_bytes(&s.unpack, w, h, d, format, ty))
        }
    })
}

unsafe fn cstr_len(p: *const c_void) -> u32 {
    unsafe { CStr::from_ptr(p as *const std::ffi::c_char) }.to_bytes().len() as u32
}

/// The "returns the full length, copies if it fits" imports, as GL's truncating copies.
unsafe fn truncated(get: impl Fn(*mut u8, u32) -> i32, size: i32, length: *mut c_void, out: *mut c_void) {
    let n = get(std::ptr::null_mut(), 0);
    let length = length as *mut i32;
    if n < 0 || size <= 0 {
        if !length.is_null() {
            unsafe { *length = 0 };
        }
        return;
    }
    let mut tmp = vec![0u8; n as usize];
    get(tmp.as_mut_ptr(), n as u32);
    let k = n.min(size - 1) as usize;
    unsafe {
        std::ptr::copy_nonoverlapping(tmp.as_ptr(), out as *mut u8, k);
        *(out as *mut u8).add(k) = 0;
        if !length.is_null() {
            *length = k as i32;
        }
    }
}

unsafe fn active_info(f: unsafe fn(u32, u32, *mut u8, u32, *mut u8) -> i32, program: u32, index: u32, size: i32, length: *mut c_void, out_size: *mut c_void, out_type: *mut c_void, name: *mut c_void) {
    let mut info = [0i32; 2];
    let n = unsafe { f(program, index, std::ptr::null_mut(), 0, info.as_mut_ptr() as *mut u8) };
    let length = length as *mut i32;
    if n < 0 {
        if !length.is_null() {
            unsafe { *length = 0 };
        }
        return;
    }
    let mut tmp = vec![0u8; n as usize];
    unsafe { f(program, index, tmp.as_mut_ptr(), n as u32, info.as_mut_ptr() as *mut u8) };
    let k = if size > 0 { n.min(size - 1) as usize } else { 0 };
    unsafe {
        if size > 0 {
            std::ptr::copy_nonoverlapping(tmp.as_ptr(), name as *mut u8, k);
            *(name as *mut u8).add(k) = 0;
        }
        if !length.is_null() {
            *length = k as i32;
        }
        if !out_size.is_null() {
            *(out_size as *mut i32) = info[0];
        }
        if !out_type.is_null() {
            *(out_type as *mut u32) = info[1] as u32;
        }
    }
}

// ---- errors and strings ----------------------------------------------------------------------

pub unsafe extern "C" fn glGetError() -> u32 {
    let e = state(|s| std::mem::take(&mut s.error));
    if e != 0 { e } else { unsafe { sys::gl_get_error() } }
}

fn string(name: u32) -> Option<Box<[u8]>> {
    let n = unsafe { sys::gl_get_string(name, std::ptr::null_mut(), 0) };
    if n < 0 {
        return None;
    }
    let mut v = vec![0u8; n as usize + 1];
    unsafe { sys::gl_get_string(name, v.as_mut_ptr(), n as u32) };
    Some(v.into_boxed_slice())
}

pub unsafe extern "C" fn glGetString(name: u32) -> *const c_void {
    if ![0x1F00, 0x1F01, 0x1F02, 0x8B8C, GL_EXTENSIONS].contains(&name) {
        set_error(GL_INVALID_ENUM);
        return std::ptr::null();
    }
    if let Some(p) = state(|s| s.strings.iter().find(|(n, _)| *n == name).map(|(_, b)| b.as_ptr())) {
        return p as *const c_void;
    }
    let Some(b) = string(name) else { return std::ptr::null() };
    state(|s| {
        s.strings.push((name, b));
        s.strings.last().map_or(std::ptr::null(), |(_, b)| b.as_ptr() as *const c_void)
    })
}

fn extensions() -> usize {
    if let Some(n) = state(|s| s.extensions.as_ref().map(Vec::len)) {
        return n;
    }
    let all = string(GL_EXTENSIONS).unwrap_or_default();
    let all = &all[..all.len().saturating_sub(1)];
    let list: Vec<Box<[u8]>> = all
        .split(|&b| b == b' ')
        .filter(|e| !e.is_empty())
        .map(|e| [e, &[0][..]].concat().into_boxed_slice())
        .collect();
    let n = list.len();
    state(|s| s.extensions = Some(list));
    n
}

pub unsafe extern "C" fn glGetStringi(name: u32, index: u32) -> *const c_void {
    if name != GL_EXTENSIONS {
        set_error(GL_INVALID_ENUM);
        return std::ptr::null();
    }
    if index as usize >= extensions() {
        set_error(GL_INVALID_VALUE);
        return std::ptr::null();
    }
    state(|s| s.extensions.as_ref().map_or(std::ptr::null(), |l| l[index as usize].as_ptr() as *const c_void))
}

// ---- queries -------------------------------------------------------------------------------------

pub unsafe extern "C" fn glGetIntegerv(pname: u32, data: *mut c_void) {
    if pname == GL_NUM_EXTENSIONS {
        unsafe { *(data as *mut i32) = extensions() as i32 };
        return;
    }
    unsafe { sys::gl_get_integerv(pname, data as *mut u8, 64) };
}
pub unsafe extern "C" fn glGetBooleanv(pname: u32, data: *mut c_void) {
    let mut v = [0i32; 64];
    let n = unsafe { sys::gl_get_integerv(pname, v.as_mut_ptr() as *mut u8, 64) };
    for (i, x) in v.iter().take(n.clamp(0, 64) as usize).enumerate() {
        unsafe { *(data as *mut u8).add(i) = (*x != 0) as u8 };
    }
}
pub unsafe extern "C" fn glGetFloatv(pname: u32, data: *mut c_void) {
    unsafe { sys::gl_get_floatv(pname, data as *mut u8, 64) };
}
pub unsafe extern "C" fn glGetInteger64v(pname: u32, data: *mut c_void) {
    unsafe { sys::gl_get_integer64v(pname, data as *mut u8, 64) };
}
pub unsafe extern "C" fn glGetIntegeri_v(target: u32, index: u32, data: *mut c_void) {
    unsafe { sys::gl_get_integeri_v(target, index, data as *mut u8, 16) };
}
pub unsafe extern "C" fn glGetInteger64i_v(target: u32, index: u32, data: *mut c_void) {
    let mut v = [0i32; 16];
    let n = unsafe { sys::gl_get_integeri_v(target, index, v.as_mut_ptr() as *mut u8, 16) };
    for (i, x) in v.iter().take(n.clamp(0, 16) as usize).enumerate() {
        unsafe { *(data as *mut i64).add(i) = *x as i64 };
    }
}
pub unsafe extern "C" fn glGetInternalformativ(target: u32, internalformat: u32, pname: u32, count: i32, params: *mut c_void) {
    unsafe { sys::gl_get_internalformativ(target, internalformat, pname, params as *mut u8, count.max(0) as u32) };
}
pub unsafe extern "C" fn glGetShaderPrecisionFormat(shadertype: u32, precisiontype: u32, range: *mut c_void, precision: *mut c_void) {
    let mut v = [0i32; 3];
    unsafe {
        sys::gl_get_shader_precision_format(shadertype, precisiontype, v.as_mut_ptr() as *mut u8);
        *(range as *mut i32) = v[0];
        *(range as *mut i32).add(1) = v[1];
        *(precision as *mut i32) = v[2];
    }
}

macro_rules! single {
    ($($name:ident($($p:ident: $t:ty),*) => $call:ident;)*) => {$(
        pub unsafe extern "C" fn $name($($p: $t,)* params: *mut c_void) {
            unsafe { *(params as *mut i32) = sys::$call($($p),*) as i32 };
        }
    )*};
}
single! {
    glGetShaderiv(shader: u32, pname: u32) => gl_get_shaderiv;
    glGetProgramiv(program: u32, pname: u32) => gl_get_programiv;
    glGetBufferParameteriv(target: u32, pname: u32) => gl_get_buffer_parameteriv;
    glGetRenderbufferParameteriv(target: u32, pname: u32) => gl_get_renderbuffer_parameteriv;
    glGetFramebufferAttachmentParameteriv(target: u32, attachment: u32, pname: u32) => gl_get_framebuffer_attachment_parameteriv;
    glGetTexParameteriv(target: u32, pname: u32) => gl_get_tex_parameteriv;
    glGetSamplerParameteriv(sampler: u32, pname: u32) => gl_get_sampler_parameteriv;
    glGetQueryiv(target: u32, pname: u32) => gl_get_queryiv;
    glGetQueryObjectuiv(id: u32, pname: u32) => gl_get_query_objectuiv;
    glGetVertexAttribiv(index: u32, pname: u32) => gl_get_vertex_attribiv;
    glGetVertexAttribIiv(index: u32, pname: u32) => gl_get_vertex_attribiv;
    glGetVertexAttribIuiv(index: u32, pname: u32) => gl_get_vertex_attribiv;
}
pub unsafe extern "C" fn glGetTexParameterfv(target: u32, pname: u32, params: *mut c_void) {
    unsafe { *(params as *mut f32) = sys::gl_get_tex_parameterfv(target, pname) };
}
pub unsafe extern "C" fn glGetSamplerParameterfv(sampler: u32, pname: u32, params: *mut c_void) {
    unsafe { *(params as *mut f32) = sys::gl_get_sampler_parameterfv(sampler, pname) };
}
pub unsafe extern "C" fn glGetBufferParameteri64v(target: u32, pname: u32, params: *mut c_void) {
    unsafe { *(params as *mut i64) = sys::gl_get_buffer_parameteriv(target, pname) as i64 };
}
pub unsafe extern "C" fn glGetVertexAttribfv(index: u32, pname: u32, params: *mut c_void) {
    unsafe { sys::gl_get_vertex_attribfv(index, pname, params as *mut u8, 4) };
}
pub unsafe extern "C" fn glGetVertexAttribPointerv(index: u32, pname: u32, pointer: *mut c_void) {
    unsafe { *(pointer as *mut usize) = sys::gl_get_vertex_attrib_offset(index, pname) as usize };
}
pub unsafe extern "C" fn glGetUniformfv(program: u32, location: i32, params: *mut c_void) {
    unsafe { sys::gl_get_uniformfv(program, location, params as *mut u8, 16) };
}
pub unsafe extern "C" fn glGetUniformiv(program: u32, location: i32, params: *mut c_void) {
    unsafe { sys::gl_get_uniformiv(program, location, params as *mut u8, 16) };
}
pub unsafe extern "C" fn glGetUniformuiv(program: u32, location: i32, params: *mut c_void) {
    unsafe { sys::gl_get_uniformuiv(program, location, params as *mut u8, 16) };
}

// ---- names ----------------------------------------------------------------------------------------

macro_rules! names {
    ($($gen:ident, $del:ident => $create:ident, $delete:ident;)*) => {$(
        pub unsafe extern "C" fn $gen(n: i32, out: *mut c_void) {
            for i in 0..n.max(0) as usize {
                unsafe { *(out as *mut u32).add(i) = sys::$create() };
            }
        }
        pub unsafe extern "C" fn $del(n: i32, names: *const c_void) {
            for i in 0..n.max(0) as usize {
                unsafe { sys::$delete(*(names as *const u32).add(i)) };
            }
        }
    )*};
}
names! {
    glGenBuffers, glDeleteBuffers => gl_create_buffer, gl_delete_buffer;
    glGenTextures, glDeleteTextures => gl_create_texture, gl_delete_texture;
    glGenFramebuffers, glDeleteFramebuffers => gl_create_framebuffer, gl_delete_framebuffer;
    glGenRenderbuffers, glDeleteRenderbuffers => gl_create_renderbuffer, gl_delete_renderbuffer;
    glGenQueries, glDeleteQueries => gl_create_query, gl_delete_query;
    glGenVertexArrays, glDeleteVertexArrays => gl_create_vertex_array, gl_delete_vertex_array;
    glGenSamplers, glDeleteSamplers => gl_create_sampler, gl_delete_sampler;
    glGenTransformFeedbacks, glDeleteTransformFeedbacks => gl_create_transform_feedback, gl_delete_transform_feedback;
}

// ---- buffers ----------------------------------------------------------------------------------------

pub unsafe extern "C" fn glBindBuffer(target: u32, buffer: u32) {
    state(|s| match target {
        GL_PIXEL_UNPACK_BUFFER => s.unpack_buffer = buffer,
        GL_PIXEL_PACK_BUFFER => s.pack_buffer = buffer,
        _ => {}
    });
    unsafe { sys::gl_bind_buffer(target, buffer) };
}
pub unsafe extern "C" fn glBufferData(target: u32, size: isize, data: *const c_void, usage: u32) {
    unsafe { sys::gl_buffer_data(target, data as *const u8, size as u32, usage) };
}
pub unsafe extern "C" fn glBufferSubData(target: u32, offset: isize, size: isize, data: *const c_void) {
    unsafe { sys::gl_buffer_sub_data(target, offset as u32, data as *const u8, size as u32) };
}
pub unsafe extern "C" fn glMapBufferRange(target: u32, offset: isize, length: isize, access: u32) -> *mut c_void {
    if length <= 0 || offset < 0 || state(|s| s.maps.iter().any(|m| m.target == target)) {
        set_error(GL_INVALID_OPERATION);
        return std::ptr::null_mut();
    }
    let mut data = vec![0u8; length as usize];
    if access & GL_MAP_READ_BIT != 0 || access & (GL_MAP_INVALIDATE_RANGE_BIT | GL_MAP_INVALIDATE_BUFFER_BIT) == 0 {
        unsafe { sys::gl_get_buffer_sub_data(target, offset as u32, data.as_mut_ptr(), length as u32) }; // the current contents
    }
    let p = data.as_mut_ptr() as *mut c_void;
    state(|s| s.maps.push(Mapping { target, data, offset, access }));
    p
}
pub unsafe extern "C" fn glFlushMappedBufferRange(target: u32, offset: isize, length: isize) {
    let ok = state(|s| match s.maps.iter().find(|m| m.target == target) {
        Some(m) if offset >= 0 && length >= 0 && (offset + length) as usize <= m.data.len() => {
            unsafe { sys::gl_buffer_sub_data(target, (m.offset + offset) as u32, m.data.as_ptr().add(offset as usize), length as u32) };
            true
        }
        _ => false,
    });
    if !ok {
        set_error(GL_INVALID_VALUE);
    }
}
pub unsafe extern "C" fn glUnmapBuffer(target: u32) -> u8 {
    let Some(m) = state(|s| s.maps.iter().position(|m| m.target == target).map(|i| s.maps.remove(i))) else {
        set_error(GL_INVALID_OPERATION);
        return 0;
    };
    if m.access & GL_MAP_WRITE_BIT != 0 && m.access & GL_MAP_FLUSH_EXPLICIT_BIT == 0 {
        unsafe { sys::gl_buffer_sub_data(target, m.offset as u32, m.data.as_ptr(), m.data.len() as u32) };
    }
    1
}
pub unsafe extern "C" fn glGetBufferPointerv(target: u32, _pname: u32, params: *mut c_void) {
    let p = state(|s| s.maps.iter().find(|m| m.target == target).map_or(std::ptr::null(), |m| m.data.as_ptr()));
    unsafe { *(params as *mut *const u8) = p };
}

// ---- vertex attributes, drawing ------------------------------------------------------------------------

pub unsafe extern "C" fn glVertexAttribPointer(index: u32, size: i32, ty: u32, normalized: u8, stride: i32, pointer: *const c_void) {
    unsafe { sys::gl_vertex_attrib_pointer(index, size, ty, normalized as u32, stride, pointer as usize as u32) };
}
pub unsafe extern "C" fn glVertexAttribIPointer(index: u32, size: i32, ty: u32, stride: i32, pointer: *const c_void) {
    unsafe { sys::gl_vertex_attrib_ipointer(index, size, ty, stride, pointer as usize as u32) };
}
pub unsafe extern "C" fn glVertexAttrib1f(index: u32, x: f32) {
    unsafe { sys::gl_vertex_attrib4f(index, x, 0.0, 0.0, 1.0) };
}
pub unsafe extern "C" fn glVertexAttrib2f(index: u32, x: f32, y: f32) {
    unsafe { sys::gl_vertex_attrib4f(index, x, y, 0.0, 1.0) };
}
pub unsafe extern "C" fn glVertexAttrib3f(index: u32, x: f32, y: f32, z: f32) {
    unsafe { sys::gl_vertex_attrib4f(index, x, y, z, 1.0) };
}
pub unsafe extern "C" fn glVertexAttrib1fv(index: u32, v: *const c_void) {
    let v = v as *const f32;
    unsafe { sys::gl_vertex_attrib4f(index, *v, 0.0, 0.0, 1.0) };
}
pub unsafe extern "C" fn glVertexAttrib2fv(index: u32, v: *const c_void) {
    let v = v as *const f32;
    unsafe { sys::gl_vertex_attrib4f(index, *v, *v.add(1), 0.0, 1.0) };
}
pub unsafe extern "C" fn glVertexAttrib3fv(index: u32, v: *const c_void) {
    let v = v as *const f32;
    unsafe { sys::gl_vertex_attrib4f(index, *v, *v.add(1), *v.add(2), 1.0) };
}
pub unsafe extern "C" fn glVertexAttrib4fv(index: u32, v: *const c_void) {
    let v = v as *const f32;
    unsafe { sys::gl_vertex_attrib4f(index, *v, *v.add(1), *v.add(2), *v.add(3)) };
}
pub unsafe extern "C" fn glVertexAttribI4iv(index: u32, v: *const c_void) {
    let v = v as *const i32;
    unsafe { sys::gl_vertex_attribi4i(index, *v, *v.add(1), *v.add(2), *v.add(3)) };
}
pub unsafe extern "C" fn glVertexAttribI4uiv(index: u32, v: *const c_void) {
    let v = v as *const u32;
    unsafe { sys::gl_vertex_attribi4ui(index, *v, *v.add(1), *v.add(2), *v.add(3)) };
}
pub unsafe extern "C" fn glDrawElements(mode: u32, count: i32, ty: u32, indices: *const c_void) {
    unsafe { sys::gl_draw_elements(mode, count, ty, indices as usize as u32) };
}
pub unsafe extern "C" fn glDrawRangeElements(mode: u32, start: u32, end: u32, count: i32, ty: u32, indices: *const c_void) {
    unsafe { sys::gl_draw_range_elements(mode, start, end, count, ty, indices as usize as u32) };
}
pub unsafe extern "C" fn glDrawElementsInstanced(mode: u32, count: i32, ty: u32, indices: *const c_void, instances: i32) {
    unsafe { sys::gl_draw_elements_instanced(mode, count, ty, indices as usize as u32, instances) };
}
pub unsafe extern "C" fn glDrawBuffers(n: i32, bufs: *const c_void) {
    unsafe { sys::gl_draw_buffers(bufs as *const u8, n.max(0) as u32) };
}
pub unsafe extern "C" fn glClearBufferiv(buffer: u32, drawbuffer: i32, value: *const c_void) {
    unsafe { sys::gl_clear_bufferiv(buffer, drawbuffer, value as *const u8, if buffer == GL_COLOR { 4 } else { 1 }) };
}
pub unsafe extern "C" fn glClearBufferuiv(buffer: u32, drawbuffer: i32, value: *const c_void) {
    unsafe { sys::gl_clear_bufferuiv(buffer, drawbuffer, value as *const u8, if buffer == GL_COLOR { 4 } else { 1 }) };
}
pub unsafe extern "C" fn glClearBufferfv(buffer: u32, drawbuffer: i32, value: *const c_void) {
    unsafe { sys::gl_clear_bufferfv(buffer, drawbuffer, value as *const u8, if buffer == GL_COLOR { 4 } else { 1 }) };
}

// ---- textures, pixels -------------------------------------------------------------------------------------

pub unsafe extern "C" fn glPixelStorei(pname: u32, param: i32) {
    state(|s| match pname {
        0x0CF5 => s.unpack.alignment = param,
        0x0CF2 => s.unpack.row_length = param,
        0x806E => s.unpack.image_height = param,
        0x0CF4 => s.unpack.skip_pixels = param,
        0x0CF3 => s.unpack.skip_rows = param,
        0x806D => s.unpack.skip_images = param,
        0x0D05 => s.pack.alignment = param,
        0x0D02 => s.pack.row_length = param,
        0x0D04 => s.pack.skip_pixels = param,
        0x0D03 => s.pack.skip_rows = param,
        _ => {}
    });
    unsafe { sys::gl_pixel_storei(pname, param) };
}
pub unsafe extern "C" fn glTexImage2D(target: u32, level: i32, ifmt: i32, w: i32, h: i32, border: i32, format: u32, ty: u32, pixels: *const c_void) {
    let (p, len) = unpack(pixels, w, h, 1, format, ty);
    unsafe { sys::gl_tex_image_2d(target, level, ifmt, w, h, border, format, ty, p, len) };
}
pub unsafe extern "C" fn glTexImage3D(target: u32, level: i32, ifmt: i32, w: i32, h: i32, d: i32, border: i32, format: u32, ty: u32, pixels: *const c_void) {
    let (p, len) = unpack(pixels, w, h, d, format, ty);
    unsafe { sys::gl_tex_image_3d(target, level, ifmt, w, h, d, border, format, ty, p, len) };
}
pub unsafe extern "C" fn glTexSubImage2D(target: u32, level: i32, x: i32, y: i32, w: i32, h: i32, format: u32, ty: u32, pixels: *const c_void) {
    let (p, len) = unpack(pixels, w, h, 1, format, ty);
    unsafe { sys::gl_tex_sub_image_2d(target, level, x, y, w, h, format, ty, p, len) };
}
pub unsafe extern "C" fn glTexSubImage3D(target: u32, level: i32, x: i32, y: i32, z: i32, w: i32, h: i32, d: i32, format: u32, ty: u32, pixels: *const c_void) {
    let (p, len) = unpack(pixels, w, h, d, format, ty);
    unsafe { sys::gl_tex_sub_image_3d(target, level, x, y, z, w, h, d, format, ty, p, len) };
}
pub unsafe extern "C" fn glCompressedTexImage2D(target: u32, level: i32, ifmt: u32, w: i32, h: i32, border: i32, size: i32, data: *const c_void) {
    unsafe { sys::gl_compressed_tex_image_2d(target, level, ifmt, w, h, border, data as *const u8, size as u32) };
}
pub unsafe extern "C" fn glCompressedTexImage3D(target: u32, level: i32, ifmt: u32, w: i32, h: i32, d: i32, border: i32, size: i32, data: *const c_void) {
    unsafe { sys::gl_compressed_tex_image_3d(target, level, ifmt, w, h, d, border, data as *const u8, size as u32) };
}
pub unsafe extern "C" fn glCompressedTexSubImage2D(target: u32, level: i32, x: i32, y: i32, w: i32, h: i32, format: u32, size: i32, data: *const c_void) {
    unsafe { sys::gl_compressed_tex_sub_image_2d(target, level, x, y, w, h, format, data as *const u8, size as u32) };
}
pub unsafe extern "C" fn glCompressedTexSubImage3D(
    target: u32, level: i32, x: i32, y: i32, z: i32, w: i32, h: i32, d: i32, format: u32, size: i32, data: *const c_void,
) {
    unsafe { sys::gl_compressed_tex_sub_image_3d(target, level, x, y, z, w, h, d, format, data as *const u8, size as u32) };
}
pub unsafe extern "C" fn glTexParameteriv(target: u32, pname: u32, params: *const c_void) {
    unsafe { sys::gl_tex_parameteri(target, pname, *(params as *const i32)) };
}
pub unsafe extern "C" fn glTexParameterfv(target: u32, pname: u32, params: *const c_void) {
    unsafe { sys::gl_tex_parameterf(target, pname, *(params as *const f32)) };
}
pub unsafe extern "C" fn glSamplerParameteriv(sampler: u32, pname: u32, params: *const c_void) {
    unsafe { sys::gl_sampler_parameteri(sampler, pname, *(params as *const i32)) };
}
pub unsafe extern "C" fn glSamplerParameterfv(sampler: u32, pname: u32, params: *const c_void) {
    unsafe { sys::gl_sampler_parameterf(sampler, pname, *(params as *const f32)) };
}
pub unsafe extern "C" fn glReadPixels(x: i32, y: i32, w: i32, h: i32, format: u32, ty: u32, pixels: *mut c_void) {
    let (pack_buffer, len) = state(|s| (s.pack_buffer, image_bytes(&s.pack, w, h, 1, format, ty)));
    unsafe { sys::gl_read_pixels(x, y, w, h, format, ty, pixels as *mut u8, if pack_buffer != 0 { 0 } else { len }) };
}
pub unsafe extern "C" fn glInvalidateFramebuffer(target: u32, n: i32, attachments: *const c_void) {
    unsafe { sys::gl_invalidate_framebuffer(target, attachments as *const u8, n.max(0) as u32) };
}
pub unsafe extern "C" fn glInvalidateSubFramebuffer(target: u32, n: i32, attachments: *const c_void, x: i32, y: i32, w: i32, h: i32) {
    unsafe { sys::gl_invalidate_sub_framebuffer(target, attachments as *const u8, n.max(0) as u32, x, y, w, h) };
}

// ---- shaders and programs ---------------------------------------------------------------------------------

pub unsafe extern "C" fn glShaderSource(shader: u32, count: i32, strings: *const c_void, lengths: *const c_void) {
    let strings = strings as *const *const c_void;
    let lengths = lengths as *const i32;
    let mut all = Vec::new();
    for i in 0..count.max(0) as usize {
        unsafe {
            let p = *strings.add(i);
            let n = if !lengths.is_null() && *lengths.add(i) >= 0 { *lengths.add(i) as u32 } else { cstr_len(p) };
            all.extend_from_slice(std::slice::from_raw_parts(p as *const u8, n as usize));
        }
    }
    unsafe { sys::gl_shader_source(shader, all.as_ptr(), all.len() as u32) };
}
pub unsafe extern "C" fn glGetShaderInfoLog(shader: u32, size: i32, length: *mut c_void, log: *mut c_void) {
    unsafe { truncated(|p, n| sys::gl_get_shader_info_log(shader, p, n), size, length, log) };
}
pub unsafe extern "C" fn glGetProgramInfoLog(program: u32, size: i32, length: *mut c_void, log: *mut c_void) {
    unsafe { truncated(|p, n| sys::gl_get_program_info_log(program, p, n), size, length, log) };
}
pub unsafe extern "C" fn glGetShaderSource(shader: u32, size: i32, length: *mut c_void, source: *mut c_void) {
    unsafe { truncated(|p, n| sys::gl_get_shader_source(shader, p, n), size, length, source) };
}
pub unsafe extern "C" fn glGetActiveUniformBlockName(program: u32, index: u32, size: i32, length: *mut c_void, name: *mut c_void) {
    unsafe { truncated(|p, n| sys::gl_get_active_uniform_block_name(program, index, p, n), size, length, name) };
}
pub unsafe extern "C" fn glGetActiveAttrib(program: u32, index: u32, size: i32, length: *mut c_void, out_size: *mut c_void, ty: *mut c_void, name: *mut c_void) {
    unsafe { active_info(|p, i, d, c, info| sys::gl_get_active_attrib(p, i, d, c, info), program, index, size, length, out_size, ty, name) };
}
pub unsafe extern "C" fn glGetActiveUniform(program: u32, index: u32, size: i32, length: *mut c_void, out_size: *mut c_void, ty: *mut c_void, name: *mut c_void) {
    unsafe { active_info(|p, i, d, c, info| sys::gl_get_active_uniform(p, i, d, c, info), program, index, size, length, out_size, ty, name) };
}
pub unsafe extern "C" fn glGetTransformFeedbackVarying(
    program: u32, index: u32, size: i32, length: *mut c_void, out_size: *mut c_void, ty: *mut c_void, name: *mut c_void,
) {
    unsafe { active_info(|p, i, d, c, info| sys::gl_get_transform_feedback_varying(p, i, d, c, info), program, index, size, length, out_size, ty, name) };
}
pub unsafe extern "C" fn glGetAttachedShaders(program: u32, max: i32, count: *mut c_void, shaders: *mut c_void) {
    let n = unsafe { sys::gl_get_attached_shaders(program, shaders as *mut u8, max.max(0) as u32) };
    if !count.is_null() {
        unsafe { *(count as *mut i32) = n.min(max) };
    }
}
pub unsafe extern "C" fn glBindAttribLocation(program: u32, index: u32, name: *const c_void) {
    unsafe { sys::gl_bind_attrib_location(program, index, name as *const u8, cstr_len(name)) };
}
pub unsafe extern "C" fn glGetAttribLocation(program: u32, name: *const c_void) -> i32 {
    unsafe { sys::gl_get_attrib_location(program, name as *const u8, cstr_len(name)) }
}
pub unsafe extern "C" fn glGetFragDataLocation(program: u32, name: *const c_void) -> i32 {
    unsafe { sys::gl_get_frag_data_location(program, name as *const u8, cstr_len(name)) }
}
pub unsafe extern "C" fn glGetUniformLocation(program: u32, name: *const c_void) -> i32 {
    unsafe { sys::gl_get_uniform_location(program, name as *const u8, cstr_len(name)) }
}
pub unsafe extern "C" fn glGetUniformBlockIndex(program: u32, name: *const c_void) -> u32 {
    unsafe { sys::gl_get_uniform_block_index(program, name as *const u8, cstr_len(name)) }
}
pub unsafe extern "C" fn glGetUniformIndices(program: u32, count: i32, names: *const c_void, indices: *mut c_void) {
    let names = names as *const *const c_void;
    for i in 0..count.max(0) as usize {
        unsafe {
            let n = *names.add(i);
            *(indices as *mut u32).add(i) = sys::gl_get_uniform_index(program, n as *const u8, cstr_len(n));
        }
    }
}
pub unsafe extern "C" fn glGetActiveUniformsiv(program: u32, count: i32, indices: *const c_void, pname: u32, params: *mut c_void) {
    unsafe { sys::gl_get_active_uniformsiv(program, indices as *const u8, count.max(0) as u32, pname, params as *mut u8) };
}
pub unsafe extern "C" fn glGetActiveUniformBlockiv(program: u32, index: u32, pname: u32, params: *mut c_void) {
    unsafe { sys::gl_get_active_uniform_blockiv(program, index, pname, params as *mut u8, 64) };
}
pub unsafe extern "C" fn glTransformFeedbackVaryings(program: u32, count: i32, varyings: *const c_void, mode: u32) {
    let varyings = varyings as *const *const c_void;
    let mut all = Vec::new();
    for i in 0..count.max(0) as usize {
        unsafe {
            let p = *varyings.add(i);
            all.extend_from_slice(std::slice::from_raw_parts(p as *const u8, cstr_len(p) as usize + 1));
        }
    }
    unsafe { sys::gl_transform_feedback_varyings(program, all.as_ptr(), all.len() as u32, count.max(0) as u32, mode) };
}
pub unsafe extern "C" fn glReleaseShaderCompiler() {}
pub unsafe extern "C" fn glShaderBinary(_count: i32, _shaders: *const c_void, _format: u32, _binary: *const c_void, _length: i32) {
    set_error(GL_INVALID_ENUM); // WebGL has no shader formats
}
pub unsafe extern "C" fn glGetProgramBinary(_program: u32, _size: i32, length: *mut c_void, _format: *mut c_void, _binary: *mut c_void) {
    if !length.is_null() {
        unsafe { *(length as *mut i32) = 0 };
    }
    set_error(GL_INVALID_OPERATION);
}
pub unsafe extern "C" fn glProgramBinary(_program: u32, _format: u32, _binary: *const c_void, _length: i32) {
    set_error(GL_INVALID_ENUM);
}
pub unsafe extern "C" fn glProgramParameteri(_program: u32, _pname: u32, _value: i32) {}

// ---- sync (a GLsync is a gasm:gl sync name) ------------------------------------------------------------------

pub unsafe extern "C" fn glFenceSync(condition: u32, flags: u32) -> *const c_void {
    unsafe { sys::gl_fence_sync(condition, flags) as usize as *const c_void }
}
pub unsafe extern "C" fn glIsSync(sync: *const c_void) -> u8 {
    unsafe { sys::gl_is_sync(sync as usize as u32) as u8 }
}
pub unsafe extern "C" fn glDeleteSync(sync: *const c_void) {
    unsafe { sys::gl_delete_sync(sync as usize as u32) };
}
pub unsafe extern "C" fn glClientWaitSync(sync: *const c_void, flags: u32, timeout: u64) -> u32 {
    unsafe { sys::gl_client_wait_sync(sync as usize as u32, flags, timeout) }
}
pub unsafe extern "C" fn glWaitSync(sync: *const c_void, flags: u32, timeout: u64) {
    unsafe { sys::gl_wait_sync(sync as usize as u32, flags, timeout) };
}
pub unsafe extern "C" fn glGetSynciv(sync: *const c_void, pname: u32, count: i32, length: *mut c_void, values: *mut c_void) {
    if count > 0 {
        unsafe { *(values as *mut i32) = sys::gl_get_synciv(sync as usize as u32, pname) };
    }
    if !length.is_null() {
        unsafe { *(length as *mut i32) = (count > 0) as i32 };
    }
}

