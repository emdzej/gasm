//! Raw ABI. On wasm32 these are the real imports (see spec/ABI.md). On other
//! targets they are backed by [`crate::native`], an in-process stub host used
//! for native parity tests and benchmarks.

#![allow(clippy::missing_safety_doc)]

#[cfg(target_arch = "wasm32")]
mod imports {
    #[link(wasm_import_module = "gasm")]
    unsafe extern "C" {
        pub fn log(ptr: *const u8, len: u32);
        pub fn time_ms() -> f64;
        pub fn set_frame_rate(hz: f64);
        pub fn video_present(ptr: *const u8, w: u32, h: u32, stride: u32);
        pub fn audio_config(rate: u32, channels: u32);
        pub fn audio_push(ptr: *const f32, frames: u32);
        pub fn input_pad(player: u32) -> u32;
        pub fn asset_size(name: *const u8, len: u32) -> i32;
        pub fn asset_read(name: *const u8, len: u32, dst: *mut u8, cap: u32) -> i32;
        pub fn param(name: *const u8, len: u32, dst: *mut u8, cap: u32) -> i32;
        pub fn asset_read_at(name: *const u8, len: u32, offset: u32, dst: *mut u8, cap: u32) -> i32;
    }

    #[link(wasm_import_module = "gasm:storage")]
    unsafe extern "C" {
        #[link_name = "get"]
        pub fn storage_get(key: *const u8, len: u32, dst: *mut u8, cap: u32) -> i32;
        #[link_name = "set"]
        pub fn storage_set(key: *const u8, len: u32, data: *const u8, data_len: u32) -> i32;
        #[link_name = "delete"]
        pub fn storage_delete(key: *const u8, len: u32) -> i32;
    }

    #[link(wasm_import_module = "gasm:gfx")]
    unsafe extern "C" {
        #[link_name = "width"]
        pub fn gfx_width() -> u32;
        #[link_name = "height"]
        pub fn gfx_height() -> u32;
        #[link_name = "create_shader"]
        pub fn gfx_create_shader(ptr: *const u8, len: u32) -> u32;
        #[link_name = "create_buffer"]
        pub fn gfx_create_buffer(size: u32, usage: u32) -> u32;
        #[link_name = "create_pipeline"]
        pub fn gfx_create_pipeline(ptr: *const u8, len: u32) -> u32;
        #[link_name = "create_bind_group"]
        pub fn gfx_create_bind_group(ptr: *const u8, len: u32) -> u32;
        #[link_name = "write_buffer"]
        pub fn gfx_write_buffer(buf: u32, offset: u32, ptr: *const u8, len: u32);
        #[link_name = "begin_frame"]
        pub fn gfx_begin_frame(r: f32, g: f32, b: f32, a: f32) -> u32;
        #[link_name = "set_pipeline"]
        pub fn gfx_set_pipeline(p: u32);
        #[link_name = "set_bind_group"]
        pub fn gfx_set_bind_group(index: u32, bg: u32);
        #[link_name = "set_vertex_buffer"]
        pub fn gfx_set_vertex_buffer(slot: u32, buf: u32, offset: u32);
        #[link_name = "set_index_buffer"]
        pub fn gfx_set_index_buffer(buf: u32, format: u32, offset: u32);
        #[link_name = "draw"]
        pub fn gfx_draw(vc: u32, ic: u32, fv: u32, fi: u32);
        #[link_name = "draw_indexed"]
        pub fn gfx_draw_indexed(ic: u32, inst: u32, first: u32, base: i32, fi: u32);
        #[link_name = "end_frame"]
        pub fn gfx_end_frame();
    }

    #[link(wasm_import_module = "gasm:net")]
    unsafe extern "C" {
        #[link_name = "open"]
        pub fn net_open(ptr: *const u8, len: u32) -> i32;
        #[link_name = "state"]
        pub fn net_state(c: i32) -> u32;
        #[link_name = "send"]
        pub fn net_send(c: i32, ptr: *const u8, len: u32) -> i32;
        #[link_name = "recv"]
        pub fn net_recv(c: i32, dst: *mut u8, cap: u32) -> i32;
        #[link_name = "close"]
        pub fn net_close(c: i32);
    }

    #[link(wasm_import_module = "wasi_snapshot_preview1")]
    unsafe extern "C" {
        pub fn proc_exit(code: i32) -> !;
    }
}

#[cfg(target_arch = "wasm32")]
pub use imports::*;

#[cfg(not(target_arch = "wasm32"))]
pub use crate::native::abi::*;
