//! Raw ABI. On wasm32 these are the real imports (see spec/ABI.md). On other
//! targets they are backed by [`crate::native`], an in-process stub host used
//! for native parity tests and benchmarks.
//!
//! GENERATED from spec/abi.json by scripts/gen-abi.mjs. Do not edit by hand.

#![allow(clippy::missing_safety_doc)]

#[cfg(target_arch = "wasm32")]
mod imports {
    #[link(wasm_import_module = "gasm")]
    unsafe extern "C" {
        /// Write a line to the runner's log.
        pub fn log(msg: *const u8, msg_len: u32);
        /// Monotonic time in milliseconds (virtual, frame-derived in headless runs).
        pub fn time_ms() -> f64;
        /// Rate (Hz) at which the runner calls gasm_frame(). Default 60; 1-1000.
        pub fn set_frame_rate(hz: f64);
        /// Present RGBA8 pixels (bytes R,G,B,A), stride bytes per row, w,h <= 4096. Copied before returning; letterboxed by the runner.
        pub fn video_present(rgba: *const u8, width: u32, height: u32, stride: u32);
        /// Format for audio_push: 8-192 kHz, 1 or 2 channels. Default 44100/2.
        pub fn audio_config(sample_rate: u32, channels: u32);
        /// Queue frames x channels interleaved f32 samples in [-1, 1]; the runner resamples.
        pub fn audio_push(samples: *const f32, frames: u32);
        /// Bitmask of GASM_BTN_* held on virtual pad player (0..3), stable within a frame.
        pub fn input_pad(player: u32) -> u32;
        /// UTF-8 text typed since the previous frame (backspace = \b, enter = \n), stable within a frame. Returns its length (copied only if length <= cap; cap = 0 queries), or -1 if the runner has no keyboard.
        pub fn text_input(dst: *mut u8, cap: u32) -> i32;
        /// Size in bytes of asset name, or -1 if it does not exist.
        pub fn asset_size(name: *const u8, name_len: u32) -> i32;
        /// Copy up to cap bytes of asset name into dst. Bytes copied, or -1 if missing.
        pub fn asset_read(name: *const u8, name_len: u32, dst: *mut u8, cap: u32) -> i32;
        /// Copy up to len bytes of asset name starting at offset (streaming). Bytes copied (0 at the end), or -1 if missing.
        pub fn asset_read_at(name: *const u8, name_len: u32, offset: u32, dst: *mut u8, len: u32) -> i32;
        /// Number of assets.
        pub fn asset_count() -> u32;
        /// Name of asset index (0 .. asset_count-1, sorted by UTF-8 bytes; folder entries as named on disk). Its length (copied only if length <= cap; cap = 0 queries), or -1 if index is out of range.
        pub fn asset_name(index: u32, dst: *mut u8, cap: u32) -> i32;
        /// Launch parameter value length, or -1 if unset. Copied only if length <= cap (cap = 0 queries the length).
        pub fn param(name: *const u8, name_len: u32, dst: *mut u8, cap: u32) -> i32;
    }

    #[link(wasm_import_module = "gasm:gfx")]
    unsafe extern "C" {
        /// Current drawable width in pixels.
        #[link_name = "width"]
        pub fn gfx_width() -> u32;
        /// Current drawable height in pixels.
        #[link_name = "height"]
        pub fn gfx_height() -> u32;
        /// Compile WGSL source.
        #[link_name = "create_shader"]
        pub fn gfx_create_shader(wgsl: *const u8, wgsl_len: u32) -> u32;
        /// size: non-zero multiple of 4; usage: GASM_BUF_* (WebGPU GPUBufferUsage bits; COPY_DST always added).
        #[link_name = "create_buffer"]
        pub fn gfx_create_buffer(size: u32, usage: u32) -> u32;
        /// GPURenderPipelineDescriptor as JSON. layout: "auto" (default) or an array of bind group layout handles.
        #[link_name = "create_pipeline"]
        pub fn gfx_create_pipeline(json: *const u8, json_len: u32) -> u32;
        /// {"layout":L,"entries":[...]} or {"pipeline":P,"group":G,"entries":[...]}; entries {"binding":B,"buffer":H,"offset":O,"size":S}, {"binding":B,"texture":T} or {"binding":B,"sampler":S}.
        #[link_name = "create_bind_group"]
        pub fn gfx_create_bind_group(json: *const u8, json_len: u32) -> u32;
        /// GPUBindGroupLayoutDescriptor subset: {"entries":[{"binding":B,"visibility":GASM_STAGE_*,"buffer":{"type":"uniform"|"read-only-storage","hasDynamicOffset":bool,"minBindingSize":N} | "texture":{"sampleType":"float"} | "sampler":{"type":"filtering"}}]}.
        #[link_name = "create_bind_group_layout"]
        pub fn gfx_create_bind_group_layout(json: *const u8, json_len: u32) -> u32;
        /// 2D texture: {"size":[w,h],"format":"rgba8unorm"|"rgba8unorm-srgb","mipLevelCount":n}, w,h 1-8192. Usage is TEXTURE_BINDING | COPY_DST.
        #[link_name = "create_texture"]
        pub fn gfx_create_texture(json: *const u8, json_len: u32) -> u32;
        /// Upload a tightly packed RGBA8 region (len = width*height*4) of mip level mip at (x, y). Queued like write_buffer.
        #[link_name = "write_texture"]
        pub fn gfx_write_texture(texture: u32, mip: u32, x: u32, y: u32, width: u32, height: u32, data: *const u8, len: u32);
        /// GPUSamplerDescriptor subset: addressModeU/V, magFilter, minFilter, mipmapFilter, lodMinClamp, lodMaxClamp, maxAnisotropy (1-16).
        #[link_name = "create_sampler"]
        pub fn gfx_create_sampler(json: *const u8, json_len: u32) -> u32;
        /// Queue a write, applied before the frame's draws. offset/len: multiples of 4.
        #[link_name = "write_buffer"]
        pub fn gfx_write_buffer(buffer: u32, offset: u32, data: *const u8, len: u32);
        /// Start the frame (clears color and depth). 1 = will be shown, 0 = discarded (the guest may skip draws).
        #[link_name = "begin_frame"]
        pub fn gfx_begin_frame(r: f32, g: f32, b: f32, a: f32) -> u32;
        #[link_name = "set_pipeline"]
        pub fn gfx_set_pipeline(pipeline: u32);
        #[link_name = "set_bind_group"]
        pub fn gfx_set_bind_group(index: u32, bind_group: u32);
        /// set_bind_group with count dynamic offsets (multiples of 256), one per dynamic-offset entry of the layout, in binding order.
        #[link_name = "set_bind_group_offsets"]
        pub fn gfx_set_bind_group_offsets(index: u32, bind_group: u32, offsets: *const u32, count: u32);
        /// Viewport in drawable pixels (clamped to the drawable), depth range 0-1. Reset to the whole drawable by begin_frame.
        #[link_name = "set_viewport"]
        pub fn gfx_set_viewport(x: f32, y: f32, width: f32, height: f32, min_depth: f32, max_depth: f32);
        /// Scissor rectangle in drawable pixels (clamped to the drawable). Reset to the whole drawable by begin_frame.
        #[link_name = "set_scissor_rect"]
        pub fn gfx_set_scissor_rect(x: u32, y: u32, width: u32, height: u32);
        #[link_name = "set_vertex_buffer"]
        pub fn gfx_set_vertex_buffer(slot: u32, buffer: u32, offset: u32);
        /// format: GASM_INDEX_U16 or GASM_INDEX_U32.
        #[link_name = "set_index_buffer"]
        pub fn gfx_set_index_buffer(buffer: u32, format: u32, offset: u32);
        #[link_name = "draw"]
        pub fn gfx_draw(vertex_count: u32, instance_count: u32, first_vertex: u32, first_instance: u32);
        #[link_name = "draw_indexed"]
        pub fn gfx_draw_indexed(index_count: u32, instance_count: u32, first_index: u32, base_vertex: i32, first_instance: u32);
        /// Submit and present.
        #[link_name = "end_frame"]
        pub fn gfx_end_frame();
    }

    #[link(wasm_import_module = "gasm:net")]
    unsafe extern "C" {
        /// Open a ws:// or wss:// URL. Handle > 0, or -1 if denied/invalid.
        #[link_name = "open"]
        pub fn net_open(url: *const u8, url_len: u32) -> i32;
        /// GASM_NET_CONNECTING / OPEN / CLOSED / ERROR.
        #[link_name = "state"]
        pub fn net_state(conn: i32) -> u32;
        /// Send one message (len > 0). 0, or -1 if not open.
        #[link_name = "send"]
        pub fn net_send(conn: i32, data: *const u8, len: u32) -> i32;
        /// Next message's length (copied only if <= cap, else it stays queued), 0 if none, -1 if closed and drained.
        #[link_name = "recv"]
        pub fn net_recv(conn: i32, dst: *mut u8, cap: u32) -> i32;
        #[link_name = "close"]
        pub fn net_close(conn: i32);
    }

    #[link(wasm_import_module = "gasm:storage")]
    unsafe extern "C" {
        /// Value length, or -1 if missing. Copied only if length <= cap.
        #[link_name = "get"]
        pub fn storage_get(key: *const u8, key_len: u32, dst: *mut u8, cap: u32) -> i32;
        /// 0, or -1 on invalid key, too large, quota exceeded or I/O error.
        #[link_name = "set"]
        pub fn storage_set(key: *const u8, key_len: u32, data: *const u8, len: u32) -> i32;
        /// 0 if deleted, -1 if it did not exist.
        #[link_name = "delete"]
        pub fn storage_delete(key: *const u8, key_len: u32) -> i32;
    }

    #[link(wasm_import_module = "wasi_snapshot_preview1")]
    unsafe extern "C" {
        /// End the game; 0 is a normal exit.
        pub fn proc_exit(code: i32) -> !;
    }
}

#[cfg(target_arch = "wasm32")]
pub use imports::*;

#[cfg(not(target_arch = "wasm32"))]
pub use crate::native::abi::*;
