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
        /// 1 if the runner provides an import module ("gasm:gfx") or a function in one ("gasm.asset_size64", "gasm:gfx.destroy"), else 0. Calling an import the runner lacks traps, so probe optional features first.
        pub fn has(name: *const u8, name_len: u32) -> i32;
        /// Monotonic time in milliseconds (virtual, frame-derived in headless runs).
        pub fn time_ms() -> f64;
        /// Rate (Hz) at which the runner calls gasm_frame(). Default 60; 1-1000.
        pub fn set_frame_rate(hz: f64);
        /// Present RGBA8 pixels (bytes R,G,B,A), stride bytes per row, w,h <= 4096. Copied before returning; letterboxed by the runner.
        pub fn video_present(rgba: *const u8, width: u32, height: u32, stride: u32);
        /// Show video_present frames at display aspect num:den (4:3 for 320x200 DOS games) instead of square pixels; 0:0 resets. Both 1..65535 with 1/8 <= num/den <= 8, else traps. Display and the pointer frame position only (not hashed). Newer than has: probe has("gasm.video_set_aspect") first.
        pub fn video_set_aspect(num: u32, den: u32);
        /// Format for audio_push: 8-192 kHz, 1 or 2 channels. Default 44100/2.
        pub fn audio_config(sample_rate: u32, channels: u32);
        /// Queue frames x channels interleaved f32 samples in [-1, 1]; the runner resamples.
        pub fn audio_push(samples: *const f32, frames: u32);
        /// Bitmask of GASM_BTN_* held on virtual pad player (0..3), stable within a frame.
        pub fn input_pad(player: u32) -> u32;
        /// UTF-8 text typed since the previous frame (backspace = \b, enter = \n), stable within a frame. Returns its length (copied only if length <= cap; cap = 0 queries), or -1 if the runner has no keyboard.
        pub fn text_input(dst: *mut u8, cap: u32) -> i32;
        /// GASM_INPUT_* flags: KEYS_RAW (the runner stops mapping the keyboard to pads; gamepads still map), POINTER_HIDDEN (hide the system cursor over the game), POINTER_LOCKED (capture the pointer for relative motion; best effort, the browser needs a click).
        pub fn input_mode(flags: u32);
        /// Held keys as a bitset indexed by GASM_KEY_* (bit k%8 of byte k/8), stable within a frame. Copies min(len, GASM_KEY_STATE_BYTES) bytes; returns GASM_KEY_STATE_BYTES, or -1 if the runner has no keyboard.
        pub fn key_state(dst: *mut u8, len: u32) -> i32;
        /// Key presses and releases since the previous frame, in order: 4 bytes each (u16 GASM_KEY_* code, u8 1 = down / 0 = up, u8 0). Returns the byte length (copied only if <= cap; cap = 0 queries), or -1 if the runner has no keyboard.
        pub fn key_events(dst: *mut u8, cap: u32) -> i32;
        /// Mouse/touch state for this frame as GASM_POINTER_BYTES bytes (see ABI.md: position in drawable and frame pixels, relative motion, wheel, buttons held/pressed/released, flags). Copied only if cap is large enough; returns GASM_POINTER_BYTES, or -1 if the runner has no pointer.
        pub fn pointer(dst: *mut u8, cap: u32) -> i32;
        /// Raw gamepad/joystick in slot 0-3 (connection order) as GASM_GAMEPAD_BYTES bytes: u32 flags (GASM_GAMEPAD_CONNECTED, GASM_GAMEPAD_STANDARD), u32 button count, u32 axis count, f32 buttons[32] (0-1), f32 axes[16] (-1..1). Standard mapping: W3C button and axis order. Copied only if cap is large enough; returns GASM_GAMEPAD_BYTES, or -1 if slot > 3 or the runner has no gamepad support.
        pub fn gamepad(slot: u32, dst: *mut u8, cap: u32) -> i32;
        /// Device name of the gamepad in slot: its length (copied only if <= cap), or -1 if the slot is empty.
        pub fn gamepad_name(slot: u32, dst: *mut u8, cap: u32) -> i32;
        /// Size in bytes of asset name, -1 if it does not exist, or -2 if it is 2 GiB or larger (use asset_size64).
        pub fn asset_size(name: *const u8, name_len: u32) -> i32;
        /// Size in bytes of asset name (any size), or -1 if it does not exist.
        pub fn asset_size64(name: *const u8, name_len: u32) -> i64;
        /// Copy up to cap bytes of asset name into dst. Bytes copied, or -1 if missing.
        pub fn asset_read(name: *const u8, name_len: u32, dst: *mut u8, cap: u32) -> i32;
        /// Copy up to len bytes of asset name starting at offset (streaming). Bytes copied (0 at the end), or -1 if missing.
        pub fn asset_read_at(name: *const u8, name_len: u32, offset: u32, dst: *mut u8, len: u32) -> i32;
        /// asset_read_at with a 64-bit offset, for assets of 4 GiB and more.
        pub fn asset_read_at64(name: *const u8, name_len: u32, offset: u64, dst: *mut u8, len: u32) -> i32;
        /// Number of assets.
        pub fn asset_count() -> u32;
        /// Name of asset index (0 .. asset_count-1, sorted by UTF-8 bytes; folder entries as named on disk). Its length (copied only if length <= cap; cap = 0 queries), or -1 if index is out of range.
        pub fn asset_name(index: u32, dst: *mut u8, cap: u32) -> i32;
        /// Launch parameter value length, or -1 if unset. Copied only if length <= cap (cap = 0 queries the length).
        pub fn param(name: *const u8, name_len: u32, dst: *mut u8, cap: u32) -> i32;
        /// Name the game's window or tab (the runner adds its own suffix). Control characters and bidi overrides are removed and the rest is cut to 256 bytes; empty resets to the default. Newer than has: probe has("gasm.set_title") first.
        pub fn set_title(title: *const u8, title_len: u32);
        /// End the frame inside gasm_run: the runner suspends the guest and resumes it at the start of the next frame. Traps outside gasm_run (and on runners without stack switching: probe has("gasm.yield_frame")).
        pub fn yield_frame();
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
        /// GPUBindGroupLayoutDescriptor subset: {"entries":[{"binding":B,"visibility":GASM_STAGE_*,"buffer":{"type":"uniform"|"read-only-storage","hasDynamicOffset":bool,"minBindingSize":N} | "texture":{"sampleType":"float"|"unfilterable-float","viewDimension":"2d"} | "sampler":{"type":"filtering"|"non-filtering"}}]}.
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
        /// slot 0-7; buffer with GASM_BUF_VERTEX; offset a multiple of 4, at most the buffer size.
        #[link_name = "set_vertex_buffer"]
        pub fn gfx_set_vertex_buffer(slot: u32, buffer: u32, offset: u32);
        /// buffer with GASM_BUF_INDEX; format GASM_INDEX_U16 or GASM_INDEX_U32 (anything else traps); offset a multiple of the index size, at most the buffer size.
        #[link_name = "set_index_buffer"]
        pub fn gfx_set_index_buffer(buffer: u32, format: u32, offset: u32);
        /// Needs a pipeline, and a vertex buffer in every slot the pipeline reads, large enough for the vertices and instances drawn (else traps).
        #[link_name = "draw"]
        pub fn gfx_draw(vertex_count: u32, instance_count: u32, first_vertex: u32, first_instance: u32);
        /// Like draw, plus an index buffer holding first_index + index_count indices.
        #[link_name = "draw_indexed"]
        pub fn gfx_draw_indexed(index_count: u32, instance_count: u32, first_index: u32, base_vertex: i32, first_instance: u32);
        /// Submit and present.
        #[link_name = "end_frame"]
        pub fn gfx_end_frame();
        /// Release an object of any kind. The handle becomes invalid (later use, including another destroy, traps) and is never reused; objects created from it stay valid. GPU memory is freed once nothing in a submitted frame uses it.
        #[link_name = "destroy"]
        pub fn gfx_destroy(handle: u32);
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
        /// Close (flushing queued messages); closing again does nothing.
        #[link_name = "close"]
        pub fn net_close(conn: i32);
    }

    #[link(wasm_import_module = "gasm:storage")]
    unsafe extern "C" {
        /// Value length, or -1 if missing. Copied only if length <= cap.
        #[link_name = "get"]
        pub fn storage_get(key: *const u8, key_len: u32, dst: *mut u8, cap: u32) -> i32;
        /// 0, or a GASM_STORAGE_ERR_* code: invalid key, value too large, quota exceeded, I/O error.
        #[link_name = "set"]
        pub fn storage_set(key: *const u8, key_len: u32, data: *const u8, len: u32) -> i32;
        /// 0 if deleted, -1 if it did not exist.
        #[link_name = "delete"]
        pub fn storage_delete(key: *const u8, key_len: u32) -> i32;
        /// Number of keys in the namespace.
        #[link_name = "count"]
        pub fn storage_count() -> u32;
        /// Key index (0 .. count-1, sorted): its length (copied only if <= cap; cap = 0 queries), or -1 if out of range.
        #[link_name = "key"]
        pub fn storage_key(index: u32, dst: *mut u8, cap: u32) -> i32;
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

/// Every import module and `module.function` of this ABI version (what `gasm::has` can report).
pub const IMPORTS: [&str; 65] = [
    "gasm", "gasm.log", "gasm.has", "gasm.time_ms",
    "gasm.set_frame_rate", "gasm.video_present", "gasm.video_set_aspect", "gasm.audio_config",
    "gasm.audio_push", "gasm.input_pad", "gasm.text_input", "gasm.input_mode",
    "gasm.key_state", "gasm.key_events", "gasm.pointer", "gasm.gamepad",
    "gasm.gamepad_name", "gasm.asset_size", "gasm.asset_size64", "gasm.asset_read",
    "gasm.asset_read_at", "gasm.asset_read_at64", "gasm.asset_count", "gasm.asset_name",
    "gasm.param", "gasm.set_title", "gasm.yield_frame", "gasm:gfx",
    "gasm:gfx.width", "gasm:gfx.height", "gasm:gfx.create_shader", "gasm:gfx.create_buffer",
    "gasm:gfx.create_pipeline", "gasm:gfx.create_bind_group", "gasm:gfx.create_bind_group_layout", "gasm:gfx.create_texture",
    "gasm:gfx.write_texture", "gasm:gfx.create_sampler", "gasm:gfx.write_buffer", "gasm:gfx.begin_frame",
    "gasm:gfx.set_pipeline", "gasm:gfx.set_bind_group", "gasm:gfx.set_bind_group_offsets", "gasm:gfx.set_viewport",
    "gasm:gfx.set_scissor_rect", "gasm:gfx.set_vertex_buffer", "gasm:gfx.set_index_buffer", "gasm:gfx.draw",
    "gasm:gfx.draw_indexed", "gasm:gfx.end_frame", "gasm:gfx.destroy", "gasm:net",
    "gasm:net.open", "gasm:net.state", "gasm:net.send", "gasm:net.recv",
    "gasm:net.close", "gasm:storage", "gasm:storage.get", "gasm:storage.set",
    "gasm:storage.delete", "gasm:storage.count", "gasm:storage.key", "wasi_snapshot_preview1",
    "wasi_snapshot_preview1.proc_exit",
];

// ---- buttons
/// Virtual gamepad buttons (bit positions). Face buttons by position: East=A, South=B, North=X, West=Y.
pub const GASM_BTN_A: u32 = 1 << 0;
pub const GASM_BTN_B: u32 = 1 << 1;
pub const GASM_BTN_X: u32 = 1 << 2;
pub const GASM_BTN_Y: u32 = 1 << 3;
pub const GASM_BTN_L: u32 = 1 << 4;
pub const GASM_BTN_R: u32 = 1 << 5;
pub const GASM_BTN_SELECT: u32 = 1 << 6;
pub const GASM_BTN_START: u32 = 1 << 7;
pub const GASM_BTN_UP: u32 = 1 << 8;
pub const GASM_BTN_DOWN: u32 = 1 << 9;
pub const GASM_BTN_LEFT: u32 = 1 << 10;
pub const GASM_BTN_RIGHT: u32 = 1 << 11;

// ---- buffer usage
pub const GASM_BUF_COPY_DST: u32 = 0x08;
pub const GASM_BUF_INDEX: u32 = 0x10;
pub const GASM_BUF_VERTEX: u32 = 0x20;
pub const GASM_BUF_UNIFORM: u32 = 0x40;
pub const GASM_BUF_STORAGE: u32 = 0x80;

// ---- shader stages
/// Bind group layout entry visibility (WebGPU GPUShaderStage bits).
pub const GASM_STAGE_VERTEX: u32 = 0x1;
pub const GASM_STAGE_FRAGMENT: u32 = 0x2;

// ---- index formats
pub const GASM_INDEX_U16: u32 = 0;
pub const GASM_INDEX_U32: u32 = 1;

// ---- net states
pub const GASM_NET_CONNECTING: u32 = 0;
pub const GASM_NET_OPEN: u32 = 1;
pub const GASM_NET_CLOSED: u32 = 2;
pub const GASM_NET_ERROR: u32 = 3;

// ---- input modes
/// input_mode flags.
pub const GASM_INPUT_KEYS_RAW: u32 = 1 << 0;
pub const GASM_INPUT_POINTER_HIDDEN: u32 = 1 << 1;
pub const GASM_INPUT_POINTER_LOCKED: u32 = 1 << 2;

// ---- storage errors
/// gasm_storage_set results.
pub const GASM_STORAGE_OK: i32 = 0;
pub const GASM_STORAGE_ERR_KEY: i32 = -1;
pub const GASM_STORAGE_ERR_SIZE: i32 = -2;
pub const GASM_STORAGE_ERR_QUOTA: i32 = -3;
pub const GASM_STORAGE_ERR_IO: i32 = -4;

// ---- pointer
/// pointer() layout and bits: little-endian fields at the GASM_POINTER_OFF_* byte offsets: f32 x, y (drawable px), fx, fy (frame px), dx, dy (relative motion), wheel_x, wheel_y (lines; y > 0 = down), u32 buttons, pressed, released, flags.
pub const GASM_POINTER_BYTES: u32 = 48;
pub const GASM_POINTER_OFF_X: u32 = 0;
pub const GASM_POINTER_OFF_Y: u32 = 4;
pub const GASM_POINTER_OFF_FX: u32 = 8;
pub const GASM_POINTER_OFF_FY: u32 = 12;
pub const GASM_POINTER_OFF_DX: u32 = 16;
pub const GASM_POINTER_OFF_DY: u32 = 20;
pub const GASM_POINTER_OFF_WHEEL_X: u32 = 24;
pub const GASM_POINTER_OFF_WHEEL_Y: u32 = 28;
pub const GASM_POINTER_OFF_BUTTONS: u32 = 32;
pub const GASM_POINTER_OFF_PRESSED: u32 = 36;
pub const GASM_POINTER_OFF_RELEASED: u32 = 40;
pub const GASM_POINTER_OFF_FLAGS: u32 = 44;
pub const GASM_MOUSE_LEFT: u32 = 1 << 0;
pub const GASM_MOUSE_RIGHT: u32 = 1 << 1;
pub const GASM_MOUSE_MIDDLE: u32 = 1 << 2;
pub const GASM_MOUSE_BACK: u32 = 1 << 3;
pub const GASM_MOUSE_FORWARD: u32 = 1 << 4;
pub const GASM_POINTER_INSIDE: u32 = 1 << 0;
pub const GASM_POINTER_IS_HIDDEN: u32 = 1 << 1;
pub const GASM_POINTER_IS_LOCKED: u32 = 1 << 2;

// ---- gamepad
/// gamepad() layout: little-endian fields at the GASM_GAMEPAD_OFF_* byte offsets: u32 flags, u32 button count, u32 axis count, f32 button values (32), f32 axis values (16). Standard mapping (W3C): buttons 0 south, 1 east, 2 west, 3 north, 4/5 shoulders, 6/7 triggers, 8 select, 9 start, 10/11 stick clicks, 12-15 d-pad up/down/left/right, 16 home; axes 0/1 left stick x/y, 2/3 right stick x/y (y > 0 = down).
pub const GASM_GAMEPAD_BYTES: u32 = 204;
pub const GASM_GAMEPAD_BUTTONS: u32 = 32;
pub const GASM_GAMEPAD_AXES: u32 = 16;
pub const GASM_GAMEPAD_OFF_FLAGS: u32 = 0;
pub const GASM_GAMEPAD_OFF_BUTTON_COUNT: u32 = 4;
pub const GASM_GAMEPAD_OFF_AXIS_COUNT: u32 = 8;
pub const GASM_GAMEPAD_OFF_BUTTONS: u32 = 12;
pub const GASM_GAMEPAD_OFF_AXES: u32 = 140;
pub const GASM_GAMEPAD_CONNECTED: u32 = 1 << 0;
pub const GASM_GAMEPAD_STANDARD: u32 = 1 << 1;

// ---- keyboard
/// key_state() size, and the size of one key_events() record.
pub const GASM_KEY_STATE_BYTES: u32 = 32;
pub const GASM_KEY_EVENT_BYTES: u32 = 4;

/// Physical keys (W3C KeyboardEvent.code names; layout-independent). The third column is the W3C name.
pub mod keys {
    pub const ESCAPE: u32 = 1;
    pub const F1: u32 = 2;
    pub const F2: u32 = 3;
    pub const F3: u32 = 4;
    pub const F4: u32 = 5;
    pub const F5: u32 = 6;
    pub const F6: u32 = 7;
    pub const F7: u32 = 8;
    pub const F8: u32 = 9;
    pub const F9: u32 = 10;
    pub const F10: u32 = 11;
    pub const F11: u32 = 12;
    pub const F12: u32 = 13;
    pub const BACKQUOTE: u32 = 14;
    pub const DIGIT0: u32 = 15;
    pub const DIGIT1: u32 = 16;
    pub const DIGIT2: u32 = 17;
    pub const DIGIT3: u32 = 18;
    pub const DIGIT4: u32 = 19;
    pub const DIGIT5: u32 = 20;
    pub const DIGIT6: u32 = 21;
    pub const DIGIT7: u32 = 22;
    pub const DIGIT8: u32 = 23;
    pub const DIGIT9: u32 = 24;
    pub const MINUS: u32 = 25;
    pub const EQUAL: u32 = 26;
    pub const BACKSPACE: u32 = 27;
    pub const TAB: u32 = 28;
    pub const KEY_A: u32 = 29;
    pub const KEY_B: u32 = 30;
    pub const KEY_C: u32 = 31;
    pub const KEY_D: u32 = 32;
    pub const KEY_E: u32 = 33;
    pub const KEY_F: u32 = 34;
    pub const KEY_G: u32 = 35;
    pub const KEY_H: u32 = 36;
    pub const KEY_I: u32 = 37;
    pub const KEY_J: u32 = 38;
    pub const KEY_K: u32 = 39;
    pub const KEY_L: u32 = 40;
    pub const KEY_M: u32 = 41;
    pub const KEY_N: u32 = 42;
    pub const KEY_O: u32 = 43;
    pub const KEY_P: u32 = 44;
    pub const KEY_Q: u32 = 45;
    pub const KEY_R: u32 = 46;
    pub const KEY_S: u32 = 47;
    pub const KEY_T: u32 = 48;
    pub const KEY_U: u32 = 49;
    pub const KEY_V: u32 = 50;
    pub const KEY_W: u32 = 51;
    pub const KEY_X: u32 = 52;
    pub const KEY_Y: u32 = 53;
    pub const KEY_Z: u32 = 54;
    pub const BRACKET_LEFT: u32 = 55;
    pub const BRACKET_RIGHT: u32 = 56;
    pub const BACKSLASH: u32 = 57;
    pub const CAPS_LOCK: u32 = 58;
    pub const SEMICOLON: u32 = 59;
    pub const QUOTE: u32 = 60;
    pub const ENTER: u32 = 61;
    pub const SHIFT_LEFT: u32 = 62;
    pub const INTL_BACKSLASH: u32 = 63;
    pub const COMMA: u32 = 64;
    pub const PERIOD: u32 = 65;
    pub const SLASH: u32 = 66;
    pub const SHIFT_RIGHT: u32 = 67;
    pub const CONTROL_LEFT: u32 = 68;
    pub const META_LEFT: u32 = 69;
    pub const ALT_LEFT: u32 = 70;
    pub const SPACE: u32 = 71;
    pub const ALT_RIGHT: u32 = 72;
    pub const META_RIGHT: u32 = 73;
    pub const CONTEXT_MENU: u32 = 74;
    pub const CONTROL_RIGHT: u32 = 75;
    pub const PRINT_SCREEN: u32 = 76;
    pub const SCROLL_LOCK: u32 = 77;
    pub const PAUSE: u32 = 78;
    pub const INSERT: u32 = 79;
    pub const HOME: u32 = 80;
    pub const PAGE_UP: u32 = 81;
    pub const DELETE: u32 = 82;
    pub const END: u32 = 83;
    pub const PAGE_DOWN: u32 = 84;
    pub const ARROW_UP: u32 = 85;
    pub const ARROW_LEFT: u32 = 86;
    pub const ARROW_DOWN: u32 = 87;
    pub const ARROW_RIGHT: u32 = 88;
    pub const NUM_LOCK: u32 = 89;
    pub const NUMPAD_DIVIDE: u32 = 90;
    pub const NUMPAD_MULTIPLY: u32 = 91;
    pub const NUMPAD_SUBTRACT: u32 = 92;
    pub const NUMPAD_ADD: u32 = 93;
    pub const NUMPAD_ENTER: u32 = 94;
    pub const NUMPAD_DECIMAL: u32 = 95;
    pub const NUMPAD0: u32 = 96;
    pub const NUMPAD1: u32 = 97;
    pub const NUMPAD2: u32 = 98;
    pub const NUMPAD3: u32 = 99;
    pub const NUMPAD4: u32 = 100;
    pub const NUMPAD5: u32 = 101;
    pub const NUMPAD6: u32 = 102;
    pub const NUMPAD7: u32 = 103;
    pub const NUMPAD8: u32 = 104;
    pub const NUMPAD9: u32 = 105;
    pub const NUMPAD_EQUAL: u32 = 106;
    pub const NUMPAD_COMMA: u32 = 107;
    pub const INTL_RO: u32 = 108;
    pub const INTL_YEN: u32 = 109;
    pub const F13: u32 = 110;
    pub const F14: u32 = 111;
    pub const F15: u32 = 112;
    pub const F16: u32 = 113;
    pub const F17: u32 = 114;
    pub const F18: u32 = 115;
    pub const F19: u32 = 116;
    pub const F20: u32 = 117;
    pub const F21: u32 = 118;
    pub const F22: u32 = 119;
    pub const F23: u32 = 120;
    pub const F24: u32 = 121;
    /// W3C names by code (index 0 is unused).
    pub const NAMES: [&str; 122] = [
        "", "Escape", "F1", "F2", "F3", "F4", "F5", "F6", "F7", "F8", "F9", "F10", "F11", "F12", "Backquote", "Digit0", "Digit1", "Digit2", "Digit3", "Digit4", "Digit5", "Digit6", "Digit7", "Digit8", "Digit9", "Minus", "Equal", "Backspace", "Tab", "KeyA", "KeyB", "KeyC", "KeyD", "KeyE", "KeyF", "KeyG", "KeyH", "KeyI", "KeyJ", "KeyK", "KeyL", "KeyM", "KeyN", "KeyO", "KeyP", "KeyQ", "KeyR", "KeyS", "KeyT", "KeyU", "KeyV", "KeyW", "KeyX", "KeyY", "KeyZ", "BracketLeft", "BracketRight", "Backslash", "CapsLock", "Semicolon", "Quote", "Enter", "ShiftLeft", "IntlBackslash", "Comma", "Period", "Slash", "ShiftRight", "ControlLeft", "MetaLeft", "AltLeft", "Space", "AltRight", "MetaRight", "ContextMenu", "ControlRight", "PrintScreen", "ScrollLock", "Pause", "Insert", "Home", "PageUp", "Delete", "End", "PageDown", "ArrowUp", "ArrowLeft", "ArrowDown", "ArrowRight", "NumLock", "NumpadDivide", "NumpadMultiply", "NumpadSubtract", "NumpadAdd", "NumpadEnter", "NumpadDecimal", "Numpad0", "Numpad1", "Numpad2", "Numpad3", "Numpad4", "Numpad5", "Numpad6", "Numpad7", "Numpad8", "Numpad9", "NumpadEqual", "NumpadComma", "IntlRo", "IntlYen", "F13", "F14", "F15", "F16", "F17", "F18", "F19", "F20", "F21", "F22", "F23", "F24",
    ];
}
