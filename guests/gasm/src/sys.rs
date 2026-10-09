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
        /// How many worker threads this run lets the game start at once (wasi-threads: thread-spawn); 0 when it allows none (headless runs, browsers), so a game sizes its thread pool from it instead of failing to spawn.
        pub fn max_threads() -> u32;
        /// The player's time zone now: minutes east of UTC, daylight saving included (120 for CEST, -300 for EST). 0 in headless runs. Local time is the WASI realtime clock plus this offset.
        pub fn utc_offset_minutes() -> i32;
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
        /// Version of asset name, -1 if it does not exist: 0 for assets given at launch, a new, larger number each time the embedder replaces it while the game runs (between frames). Poll it to notice new data; a read spanning frames should check it did not change.
        pub fn asset_version(name: *const u8, name_len: u32) -> i32;
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

    #[link(wasm_import_module = "gasm:gl")]
    unsafe extern "C" {
        /// Drawable width in pixels (the default framebuffer follows it).
        #[link_name = "width"]
        pub fn gl_width() -> u32;
        /// Drawable height in pixels.
        #[link_name = "height"]
        pub fn gl_height() -> u32;
        /// 0 during catch-up frames (the runner runs several frames to catch up and shows only the last; the guest may skip drawing); 1 otherwise, headless included.
        #[link_name = "frame_shown"]
        pub fn gl_frame_shown() -> u32;
        /// Show the default framebuffer now; otherwise the runner presents at the end of the frame.
        #[link_name = "present"]
        pub fn gl_present();
        /// The oldest GL error flag (GL rules: a call with an error has no effect), or 0.
        #[link_name = "get_error"]
        pub fn gl_get_error() -> u32;
        /// GL_VENDOR, GL_RENDERER, GL_VERSION, GL_SHADING_LANGUAGE_VERSION or GL_EXTENSIONS (space-separated): its length, copied if it fits; -1 for an invalid name (GL_INVALID_ENUM).
        #[link_name = "get_string"]
        pub fn gl_get_string(name: u32, dst: *mut u8, cap: u32) -> i32;
        /// Turn on a WebGL extension listed in GL_EXTENSIONS (1), or 0 if it isn't there.
        #[link_name = "enable_extension"]
        pub fn gl_enable_extension(name: *const u8, name_len: u32) -> u32;
        /// The parameter's values as i32s: how many it has (copied up to count), or -1 for an invalid pname (GL_INVALID_ENUM).
        #[link_name = "get_integerv"]
        pub fn gl_get_integerv(pname: u32, dst: *mut u8, count: u32) -> i32;
        /// As get_integerv, as f32s.
        #[link_name = "get_floatv"]
        pub fn gl_get_floatv(pname: u32, dst: *mut u8, count: u32) -> i32;
        /// As get_integerv, as i64s.
        #[link_name = "get_integer64v"]
        pub fn gl_get_integer64v(pname: u32, dst: *mut u8, count: u32) -> i32;
        /// Indexed parameters (e.g. GL_UNIFORM_BUFFER_BINDING), as i32s.
        #[link_name = "get_integeri_v"]
        pub fn gl_get_integeri_v(target: u32, index: u32, dst: *mut u8, count: u32) -> i32;
        /// GL_SAMPLES or GL_NUM_SAMPLE_COUNTS of a renderbuffer format.
        #[link_name = "get_internalformativ"]
        pub fn gl_get_internalformativ(target: u32, internalformat: u32, pname: u32, dst: *mut u8, count: u32) -> i32;
        /// Writes 3 i32s: range min, range max, precision.
        #[link_name = "get_shader_precision_format"]
        pub fn gl_get_shader_precision_format(shadertype: u32, precisiontype: u32, dst: *mut u8);
        #[link_name = "active_texture"]
        pub fn gl_active_texture(texture: u32);
        #[link_name = "blend_color"]
        pub fn gl_blend_color(red: f32, green: f32, blue: f32, alpha: f32);
        #[link_name = "blend_equation"]
        pub fn gl_blend_equation(mode: u32);
        #[link_name = "blend_equation_separate"]
        pub fn gl_blend_equation_separate(mode_rgb: u32, mode_alpha: u32);
        #[link_name = "blend_func"]
        pub fn gl_blend_func(sfactor: u32, dfactor: u32);
        #[link_name = "blend_func_separate"]
        pub fn gl_blend_func_separate(src_rgb: u32, dst_rgb: u32, src_alpha: u32, dst_alpha: u32);
        #[link_name = "clear"]
        pub fn gl_clear(mask: u32);
        #[link_name = "clear_color"]
        pub fn gl_clear_color(red: f32, green: f32, blue: f32, alpha: f32);
        #[link_name = "clear_depthf"]
        pub fn gl_clear_depthf(depth: f32);
        #[link_name = "clear_stencil"]
        pub fn gl_clear_stencil(s: i32);
        #[link_name = "color_mask"]
        pub fn gl_color_mask(red: u32, green: u32, blue: u32, alpha: u32);
        #[link_name = "cull_face"]
        pub fn gl_cull_face(mode: u32);
        #[link_name = "depth_func"]
        pub fn gl_depth_func(func: u32);
        #[link_name = "depth_mask"]
        pub fn gl_depth_mask(flag: u32);
        #[link_name = "depth_rangef"]
        pub fn gl_depth_rangef(near: f32, far: f32);
        #[link_name = "disable"]
        pub fn gl_disable(cap: u32);
        #[link_name = "enable"]
        pub fn gl_enable(cap: u32);
        #[link_name = "is_enabled"]
        pub fn gl_is_enabled(cap: u32) -> u32;
        #[link_name = "front_face"]
        pub fn gl_front_face(mode: u32);
        #[link_name = "hint"]
        pub fn gl_hint(target: u32, mode: u32);
        #[link_name = "line_width"]
        pub fn gl_line_width(width: f32);
        #[link_name = "pixel_storei"]
        pub fn gl_pixel_storei(pname: u32, param: i32);
        #[link_name = "polygon_offset"]
        pub fn gl_polygon_offset(factor: f32, units: f32);
        #[link_name = "sample_coverage"]
        pub fn gl_sample_coverage(value: f32, invert: u32);
        #[link_name = "scissor"]
        pub fn gl_scissor(x: i32, y: i32, width: i32, height: i32);
        #[link_name = "viewport"]
        pub fn gl_viewport(x: i32, y: i32, width: i32, height: i32);
        #[link_name = "stencil_func"]
        pub fn gl_stencil_func(func: u32, r#ref: i32, mask: u32);
        #[link_name = "stencil_func_separate"]
        pub fn gl_stencil_func_separate(face: u32, func: u32, r#ref: i32, mask: u32);
        #[link_name = "stencil_mask"]
        pub fn gl_stencil_mask(mask: u32);
        #[link_name = "stencil_mask_separate"]
        pub fn gl_stencil_mask_separate(face: u32, mask: u32);
        #[link_name = "stencil_op"]
        pub fn gl_stencil_op(fail: u32, zfail: u32, zpass: u32);
        #[link_name = "stencil_op_separate"]
        pub fn gl_stencil_op_separate(face: u32, sfail: u32, dpfail: u32, dppass: u32);
        #[link_name = "finish"]
        pub fn gl_finish();
        #[link_name = "flush"]
        pub fn gl_flush();
        /// A new buffer name (glGenBuffers).
        #[link_name = "create_buffer"]
        pub fn gl_create_buffer() -> u32;
        #[link_name = "delete_buffer"]
        pub fn gl_delete_buffer(buffer: u32);
        #[link_name = "is_buffer"]
        pub fn gl_is_buffer(buffer: u32) -> u32;
        #[link_name = "bind_buffer"]
        pub fn gl_bind_buffer(target: u32, buffer: u32);
        #[link_name = "bind_buffer_base"]
        pub fn gl_bind_buffer_base(target: u32, index: u32, buffer: u32);
        #[link_name = "bind_buffer_range"]
        pub fn gl_bind_buffer_range(target: u32, index: u32, buffer: u32, offset: u32, size: u32);
        /// len bytes from data, or zeros if data is 0.
        #[link_name = "buffer_data"]
        pub fn gl_buffer_data(target: u32, data: *const u8, len: u32, usage: u32);
        #[link_name = "buffer_sub_data"]
        pub fn gl_buffer_sub_data(target: u32, offset: u32, data: *const u8, len: u32);
        #[link_name = "copy_buffer_sub_data"]
        pub fn gl_copy_buffer_sub_data(read_target: u32, write_target: u32, read_offset: u32, write_offset: u32, size: u32);
        /// Read back len bytes (glMapBufferRange for reading).
        #[link_name = "get_buffer_sub_data"]
        pub fn gl_get_buffer_sub_data(target: u32, offset: u32, dst: *mut u8, len: u32);
        #[link_name = "get_buffer_parameteriv"]
        pub fn gl_get_buffer_parameteriv(target: u32, pname: u32) -> i32;
        #[link_name = "create_vertex_array"]
        pub fn gl_create_vertex_array() -> u32;
        #[link_name = "delete_vertex_array"]
        pub fn gl_delete_vertex_array(array: u32);
        #[link_name = "is_vertex_array"]
        pub fn gl_is_vertex_array(array: u32) -> u32;
        #[link_name = "bind_vertex_array"]
        pub fn gl_bind_vertex_array(array: u32);
        #[link_name = "enable_vertex_attrib_array"]
        pub fn gl_enable_vertex_attrib_array(index: u32);
        #[link_name = "disable_vertex_attrib_array"]
        pub fn gl_disable_vertex_attrib_array(index: u32);
        /// offset into the bound GL_ARRAY_BUFFER (no client-side arrays).
        #[link_name = "vertex_attrib_pointer"]
        pub fn gl_vertex_attrib_pointer(index: u32, size: i32, r#type: u32, normalized: u32, stride: i32, offset: u32);
        #[link_name = "vertex_attrib_ipointer"]
        pub fn gl_vertex_attrib_ipointer(index: u32, size: i32, r#type: u32, stride: i32, offset: u32);
        #[link_name = "vertex_attrib_divisor"]
        pub fn gl_vertex_attrib_divisor(index: u32, divisor: u32);
        #[link_name = "vertex_attrib4f"]
        pub fn gl_vertex_attrib4f(index: u32, x: f32, y: f32, z: f32, w: f32);
        #[link_name = "vertex_attribi4i"]
        pub fn gl_vertex_attribi4i(index: u32, x: i32, y: i32, z: i32, w: i32);
        #[link_name = "vertex_attribi4ui"]
        pub fn gl_vertex_attribi4ui(index: u32, x: u32, y: u32, z: u32, w: u32);
        #[link_name = "get_vertex_attribiv"]
        pub fn gl_get_vertex_attribiv(index: u32, pname: u32) -> i32;
        /// GL_CURRENT_VERTEX_ATTRIB: 4 f32s.
        #[link_name = "get_vertex_attribfv"]
        pub fn gl_get_vertex_attribfv(index: u32, pname: u32, dst: *mut u8, count: u32) -> i32;
        #[link_name = "get_vertex_attrib_offset"]
        pub fn gl_get_vertex_attrib_offset(index: u32, pname: u32) -> u32;
        #[link_name = "draw_arrays"]
        pub fn gl_draw_arrays(mode: u32, first: i32, count: i32);
        /// Indices from the bound GL_ELEMENT_ARRAY_BUFFER at offset.
        #[link_name = "draw_elements"]
        pub fn gl_draw_elements(mode: u32, count: i32, r#type: u32, offset: u32);
        #[link_name = "draw_arrays_instanced"]
        pub fn gl_draw_arrays_instanced(mode: u32, first: i32, count: i32, instances: i32);
        #[link_name = "draw_elements_instanced"]
        pub fn gl_draw_elements_instanced(mode: u32, count: i32, r#type: u32, offset: u32, instances: i32);
        #[link_name = "draw_range_elements"]
        pub fn gl_draw_range_elements(mode: u32, start: u32, end: u32, count: i32, r#type: u32, offset: u32);
        /// count GLenums.
        #[link_name = "draw_buffers"]
        pub fn gl_draw_buffers(bufs: *const u8, count: u32);
        /// count i32s (4 for GL_COLOR, 1 for GL_STENCIL).
        #[link_name = "clear_bufferiv"]
        pub fn gl_clear_bufferiv(buffer: u32, drawbuffer: i32, value: *const u8, count: u32);
        #[link_name = "clear_bufferuiv"]
        pub fn gl_clear_bufferuiv(buffer: u32, drawbuffer: i32, value: *const u8, count: u32);
        #[link_name = "clear_bufferfv"]
        pub fn gl_clear_bufferfv(buffer: u32, drawbuffer: i32, value: *const u8, count: u32);
        #[link_name = "clear_bufferfi"]
        pub fn gl_clear_bufferfi(buffer: u32, drawbuffer: i32, depth: f32, stencil: i32);
        #[link_name = "create_texture"]
        pub fn gl_create_texture() -> u32;
        #[link_name = "delete_texture"]
        pub fn gl_delete_texture(texture: u32);
        #[link_name = "is_texture"]
        pub fn gl_is_texture(texture: u32) -> u32;
        #[link_name = "bind_texture"]
        pub fn gl_bind_texture(target: u32, texture: u32);
        #[link_name = "tex_parameteri"]
        pub fn gl_tex_parameteri(target: u32, pname: u32, param: i32);
        #[link_name = "tex_parameterf"]
        pub fn gl_tex_parameterf(target: u32, pname: u32, param: f32);
        #[link_name = "get_tex_parameteriv"]
        pub fn gl_get_tex_parameteriv(target: u32, pname: u32) -> i32;
        #[link_name = "get_tex_parameterfv"]
        pub fn gl_get_tex_parameterfv(target: u32, pname: u32) -> f32;
        /// pixels 0: no data (or, with a bound GL_PIXEL_UNPACK_BUFFER, len is the offset into it); otherwise len must cover the image.
        #[link_name = "tex_image_2d"]
        pub fn gl_tex_image_2d(target: u32, level: i32, internalformat: i32, width: i32, height: i32, border: i32, format: u32, r#type: u32, pixels: *const u8, len: u32);
        #[link_name = "tex_image_3d"]
        pub fn gl_tex_image_3d(target: u32, level: i32, internalformat: i32, width: i32, height: i32, depth: i32, border: i32, format: u32, r#type: u32, pixels: *const u8, len: u32);
        #[link_name = "tex_sub_image_2d"]
        pub fn gl_tex_sub_image_2d(target: u32, level: i32, x: i32, y: i32, width: i32, height: i32, format: u32, r#type: u32, pixels: *const u8, len: u32);
        #[link_name = "tex_sub_image_3d"]
        pub fn gl_tex_sub_image_3d(target: u32, level: i32, x: i32, y: i32, z: i32, width: i32, height: i32, depth: i32, format: u32, r#type: u32, pixels: *const u8, len: u32);
        #[link_name = "tex_storage_2d"]
        pub fn gl_tex_storage_2d(target: u32, levels: i32, internalformat: u32, width: i32, height: i32);
        #[link_name = "tex_storage_3d"]
        pub fn gl_tex_storage_3d(target: u32, levels: i32, internalformat: u32, width: i32, height: i32, depth: i32);
        #[link_name = "compressed_tex_image_2d"]
        pub fn gl_compressed_tex_image_2d(target: u32, level: i32, internalformat: u32, width: i32, height: i32, border: i32, data: *const u8, len: u32);
        #[link_name = "compressed_tex_image_3d"]
        pub fn gl_compressed_tex_image_3d(target: u32, level: i32, internalformat: u32, width: i32, height: i32, depth: i32, border: i32, data: *const u8, len: u32);
        #[link_name = "compressed_tex_sub_image_2d"]
        pub fn gl_compressed_tex_sub_image_2d(target: u32, level: i32, x: i32, y: i32, width: i32, height: i32, format: u32, data: *const u8, len: u32);
        #[link_name = "compressed_tex_sub_image_3d"]
        pub fn gl_compressed_tex_sub_image_3d(target: u32, level: i32, x: i32, y: i32, z: i32, width: i32, height: i32, depth: i32, format: u32, data: *const u8, len: u32);
        #[link_name = "copy_tex_image_2d"]
        pub fn gl_copy_tex_image_2d(target: u32, level: i32, internalformat: u32, x: i32, y: i32, width: i32, height: i32, border: i32);
        #[link_name = "copy_tex_sub_image_2d"]
        pub fn gl_copy_tex_sub_image_2d(target: u32, level: i32, xoffset: i32, yoffset: i32, x: i32, y: i32, width: i32, height: i32);
        #[link_name = "copy_tex_sub_image_3d"]
        pub fn gl_copy_tex_sub_image_3d(target: u32, level: i32, xoffset: i32, yoffset: i32, zoffset: i32, x: i32, y: i32, width: i32, height: i32);
        /// Runners generate the levels (unlike gasm:gfx).
        #[link_name = "generate_mipmap"]
        pub fn gl_generate_mipmap(target: u32);
        #[link_name = "create_sampler"]
        pub fn gl_create_sampler() -> u32;
        #[link_name = "delete_sampler"]
        pub fn gl_delete_sampler(sampler: u32);
        #[link_name = "is_sampler"]
        pub fn gl_is_sampler(sampler: u32) -> u32;
        #[link_name = "bind_sampler"]
        pub fn gl_bind_sampler(unit: u32, sampler: u32);
        #[link_name = "sampler_parameteri"]
        pub fn gl_sampler_parameteri(sampler: u32, pname: u32, param: i32);
        #[link_name = "sampler_parameterf"]
        pub fn gl_sampler_parameterf(sampler: u32, pname: u32, param: f32);
        #[link_name = "get_sampler_parameteriv"]
        pub fn gl_get_sampler_parameteriv(sampler: u32, pname: u32) -> i32;
        #[link_name = "get_sampler_parameterfv"]
        pub fn gl_get_sampler_parameterfv(sampler: u32, pname: u32) -> f32;
        #[link_name = "create_framebuffer"]
        pub fn gl_create_framebuffer() -> u32;
        #[link_name = "delete_framebuffer"]
        pub fn gl_delete_framebuffer(framebuffer: u32);
        #[link_name = "is_framebuffer"]
        pub fn gl_is_framebuffer(framebuffer: u32) -> u32;
        /// 0: the default framebuffer (the window).
        #[link_name = "bind_framebuffer"]
        pub fn gl_bind_framebuffer(target: u32, framebuffer: u32);
        #[link_name = "check_framebuffer_status"]
        pub fn gl_check_framebuffer_status(target: u32) -> u32;
        #[link_name = "framebuffer_texture_2d"]
        pub fn gl_framebuffer_texture_2d(target: u32, attachment: u32, textarget: u32, texture: u32, level: i32);
        #[link_name = "framebuffer_texture_layer"]
        pub fn gl_framebuffer_texture_layer(target: u32, attachment: u32, texture: u32, level: i32, layer: i32);
        #[link_name = "framebuffer_renderbuffer"]
        pub fn gl_framebuffer_renderbuffer(target: u32, attachment: u32, renderbuffertarget: u32, renderbuffer: u32);
        #[link_name = "get_framebuffer_attachment_parameteriv"]
        pub fn gl_get_framebuffer_attachment_parameteriv(target: u32, attachment: u32, pname: u32) -> i32;
        #[link_name = "blit_framebuffer"]
        pub fn gl_blit_framebuffer(src_x0: i32, src_y0: i32, src_x1: i32, src_y1: i32, dst_x0: i32, dst_y0: i32, dst_x1: i32, dst_y1: i32, mask: u32, filter: u32);
        #[link_name = "invalidate_framebuffer"]
        pub fn gl_invalidate_framebuffer(target: u32, attachments: *const u8, count: u32);
        #[link_name = "invalidate_sub_framebuffer"]
        pub fn gl_invalidate_sub_framebuffer(target: u32, attachments: *const u8, count: u32, x: i32, y: i32, width: i32, height: i32);
        #[link_name = "read_buffer"]
        pub fn gl_read_buffer(src: u32);
        /// len must cover the rectangle; with a bound GL_PIXEL_PACK_BUFFER, dst is the offset into it.
        #[link_name = "read_pixels"]
        pub fn gl_read_pixels(x: i32, y: i32, width: i32, height: i32, format: u32, r#type: u32, dst: *mut u8, len: u32);
        #[link_name = "create_renderbuffer"]
        pub fn gl_create_renderbuffer() -> u32;
        #[link_name = "delete_renderbuffer"]
        pub fn gl_delete_renderbuffer(renderbuffer: u32);
        #[link_name = "is_renderbuffer"]
        pub fn gl_is_renderbuffer(renderbuffer: u32) -> u32;
        #[link_name = "bind_renderbuffer"]
        pub fn gl_bind_renderbuffer(target: u32, renderbuffer: u32);
        #[link_name = "renderbuffer_storage"]
        pub fn gl_renderbuffer_storage(target: u32, internalformat: u32, width: i32, height: i32);
        #[link_name = "renderbuffer_storage_multisample"]
        pub fn gl_renderbuffer_storage_multisample(target: u32, samples: i32, internalformat: u32, width: i32, height: i32);
        #[link_name = "get_renderbuffer_parameteriv"]
        pub fn gl_get_renderbuffer_parameteriv(target: u32, pname: u32) -> i32;
        #[link_name = "create_shader"]
        pub fn gl_create_shader(r#type: u32) -> u32;
        #[link_name = "delete_shader"]
        pub fn gl_delete_shader(shader: u32);
        #[link_name = "is_shader"]
        pub fn gl_is_shader(shader: u32) -> u32;
        /// GLSL ES 3.00 (or 1.00).
        #[link_name = "shader_source"]
        pub fn gl_shader_source(shader: u32, source: *const u8, source_len: u32);
        #[link_name = "compile_shader"]
        pub fn gl_compile_shader(shader: u32);
        #[link_name = "get_shaderiv"]
        pub fn gl_get_shaderiv(shader: u32, pname: u32) -> i32;
        #[link_name = "get_shader_info_log"]
        pub fn gl_get_shader_info_log(shader: u32, dst: *mut u8, cap: u32) -> i32;
        #[link_name = "get_shader_source"]
        pub fn gl_get_shader_source(shader: u32, dst: *mut u8, cap: u32) -> i32;
        #[link_name = "create_program"]
        pub fn gl_create_program() -> u32;
        #[link_name = "delete_program"]
        pub fn gl_delete_program(program: u32);
        #[link_name = "is_program"]
        pub fn gl_is_program(program: u32) -> u32;
        #[link_name = "attach_shader"]
        pub fn gl_attach_shader(program: u32, shader: u32);
        #[link_name = "detach_shader"]
        pub fn gl_detach_shader(program: u32, shader: u32);
        #[link_name = "link_program"]
        pub fn gl_link_program(program: u32);
        #[link_name = "use_program"]
        pub fn gl_use_program(program: u32);
        #[link_name = "validate_program"]
        pub fn gl_validate_program(program: u32);
        #[link_name = "get_programiv"]
        pub fn gl_get_programiv(program: u32, pname: u32) -> i32;
        #[link_name = "get_program_info_log"]
        pub fn gl_get_program_info_log(program: u32, dst: *mut u8, cap: u32) -> i32;
        /// Shader names (u32), copied up to count; returns how many.
        #[link_name = "get_attached_shaders"]
        pub fn gl_get_attached_shaders(program: u32, dst: *mut u8, count: u32) -> i32;
        #[link_name = "bind_attrib_location"]
        pub fn gl_bind_attrib_location(program: u32, index: u32, name: *const u8, name_len: u32);
        #[link_name = "get_attrib_location"]
        pub fn gl_get_attrib_location(program: u32, name: *const u8, name_len: u32) -> i32;
        #[link_name = "get_frag_data_location"]
        pub fn gl_get_frag_data_location(program: u32, name: *const u8, name_len: u32) -> i32;
        /// info: 2 i32s (size, type). Returns the name's length (copied if it fits), or -1.
        #[link_name = "get_active_attrib"]
        pub fn gl_get_active_attrib(program: u32, index: u32, name: *mut u8, cap: u32, info: *mut u8) -> i32;
        #[link_name = "get_active_uniform"]
        pub fn gl_get_active_uniform(program: u32, index: u32, name: *mut u8, cap: u32, info: *mut u8) -> i32;
        /// -1 if the program has no such uniform.
        #[link_name = "get_uniform_location"]
        pub fn gl_get_uniform_location(program: u32, name: *const u8, name_len: u32) -> i32;
        /// glGetUniformIndices for one name (GL_INVALID_INDEX if none).
        #[link_name = "get_uniform_index"]
        pub fn gl_get_uniform_index(program: u32, name: *const u8, name_len: u32) -> u32;
        /// count u32 indices in, count i32s out.
        #[link_name = "get_active_uniformsiv"]
        pub fn gl_get_active_uniformsiv(program: u32, indices: *const u8, count: u32, pname: u32, dst: *mut u8);
        #[link_name = "get_uniform_block_index"]
        pub fn gl_get_uniform_block_index(program: u32, name: *const u8, name_len: u32) -> u32;
        #[link_name = "get_active_uniform_block_name"]
        pub fn gl_get_active_uniform_block_name(program: u32, index: u32, dst: *mut u8, cap: u32) -> i32;
        #[link_name = "get_active_uniform_blockiv"]
        pub fn gl_get_active_uniform_blockiv(program: u32, index: u32, pname: u32, dst: *mut u8, count: u32) -> i32;
        #[link_name = "uniform_block_binding"]
        pub fn gl_uniform_block_binding(program: u32, index: u32, binding: u32);
        #[link_name = "get_uniformfv"]
        pub fn gl_get_uniformfv(program: u32, location: i32, dst: *mut u8, count: u32) -> i32;
        #[link_name = "get_uniformiv"]
        pub fn gl_get_uniformiv(program: u32, location: i32, dst: *mut u8, count: u32) -> i32;
        #[link_name = "get_uniformuiv"]
        pub fn gl_get_uniformuiv(program: u32, location: i32, dst: *mut u8, count: u32) -> i32;
        /// count names, each NUL-terminated, in len bytes.
        #[link_name = "transform_feedback_varyings"]
        pub fn gl_transform_feedback_varyings(program: u32, names: *const u8, len: u32, count: u32, buffer_mode: u32);
        #[link_name = "get_transform_feedback_varying"]
        pub fn gl_get_transform_feedback_varying(program: u32, index: u32, name: *mut u8, cap: u32, info: *mut u8) -> i32;
        #[link_name = "uniform1f"]
        pub fn gl_uniform1f(location: i32, x: f32);
        #[link_name = "uniform2f"]
        pub fn gl_uniform2f(location: i32, x: f32, y: f32);
        #[link_name = "uniform3f"]
        pub fn gl_uniform3f(location: i32, x: f32, y: f32, z: f32);
        #[link_name = "uniform4f"]
        pub fn gl_uniform4f(location: i32, x: f32, y: f32, z: f32, w: f32);
        #[link_name = "uniform1i"]
        pub fn gl_uniform1i(location: i32, x: i32);
        #[link_name = "uniform2i"]
        pub fn gl_uniform2i(location: i32, x: i32, y: i32);
        #[link_name = "uniform3i"]
        pub fn gl_uniform3i(location: i32, x: i32, y: i32, z: i32);
        #[link_name = "uniform4i"]
        pub fn gl_uniform4i(location: i32, x: i32, y: i32, z: i32, w: i32);
        #[link_name = "uniform1ui"]
        pub fn gl_uniform1ui(location: i32, x: u32);
        #[link_name = "uniform2ui"]
        pub fn gl_uniform2ui(location: i32, x: u32, y: u32);
        #[link_name = "uniform3ui"]
        pub fn gl_uniform3ui(location: i32, x: u32, y: u32, z: u32);
        #[link_name = "uniform4ui"]
        pub fn gl_uniform4ui(location: i32, x: u32, y: u32, z: u32, w: u32);
        /// count vectors (GL semantics).
        #[link_name = "uniform1fv"]
        pub fn gl_uniform1fv(location: i32, count: i32, value: *const u8);
        #[link_name = "uniform2fv"]
        pub fn gl_uniform2fv(location: i32, count: i32, value: *const u8);
        #[link_name = "uniform3fv"]
        pub fn gl_uniform3fv(location: i32, count: i32, value: *const u8);
        #[link_name = "uniform4fv"]
        pub fn gl_uniform4fv(location: i32, count: i32, value: *const u8);
        #[link_name = "uniform1iv"]
        pub fn gl_uniform1iv(location: i32, count: i32, value: *const u8);
        #[link_name = "uniform2iv"]
        pub fn gl_uniform2iv(location: i32, count: i32, value: *const u8);
        #[link_name = "uniform3iv"]
        pub fn gl_uniform3iv(location: i32, count: i32, value: *const u8);
        #[link_name = "uniform4iv"]
        pub fn gl_uniform4iv(location: i32, count: i32, value: *const u8);
        #[link_name = "uniform1uiv"]
        pub fn gl_uniform1uiv(location: i32, count: i32, value: *const u8);
        #[link_name = "uniform2uiv"]
        pub fn gl_uniform2uiv(location: i32, count: i32, value: *const u8);
        #[link_name = "uniform3uiv"]
        pub fn gl_uniform3uiv(location: i32, count: i32, value: *const u8);
        #[link_name = "uniform4uiv"]
        pub fn gl_uniform4uiv(location: i32, count: i32, value: *const u8);
        #[link_name = "uniform_matrix2fv"]
        pub fn gl_uniform_matrix2fv(location: i32, count: i32, transpose: u32, value: *const u8);
        #[link_name = "uniform_matrix3fv"]
        pub fn gl_uniform_matrix3fv(location: i32, count: i32, transpose: u32, value: *const u8);
        #[link_name = "uniform_matrix4fv"]
        pub fn gl_uniform_matrix4fv(location: i32, count: i32, transpose: u32, value: *const u8);
        #[link_name = "uniform_matrix2x3fv"]
        pub fn gl_uniform_matrix2x3fv(location: i32, count: i32, transpose: u32, value: *const u8);
        #[link_name = "uniform_matrix3x2fv"]
        pub fn gl_uniform_matrix3x2fv(location: i32, count: i32, transpose: u32, value: *const u8);
        #[link_name = "uniform_matrix2x4fv"]
        pub fn gl_uniform_matrix2x4fv(location: i32, count: i32, transpose: u32, value: *const u8);
        #[link_name = "uniform_matrix4x2fv"]
        pub fn gl_uniform_matrix4x2fv(location: i32, count: i32, transpose: u32, value: *const u8);
        #[link_name = "uniform_matrix3x4fv"]
        pub fn gl_uniform_matrix3x4fv(location: i32, count: i32, transpose: u32, value: *const u8);
        #[link_name = "uniform_matrix4x3fv"]
        pub fn gl_uniform_matrix4x3fv(location: i32, count: i32, transpose: u32, value: *const u8);
        #[link_name = "create_query"]
        pub fn gl_create_query() -> u32;
        #[link_name = "delete_query"]
        pub fn gl_delete_query(query: u32);
        #[link_name = "is_query"]
        pub fn gl_is_query(query: u32) -> u32;
        #[link_name = "begin_query"]
        pub fn gl_begin_query(target: u32, query: u32);
        #[link_name = "end_query"]
        pub fn gl_end_query(target: u32);
        /// GL_CURRENT_QUERY: the active query's name.
        #[link_name = "get_queryiv"]
        pub fn gl_get_queryiv(target: u32, pname: u32) -> u32;
        /// Results are available from the next frame on, on every runner.
        #[link_name = "get_query_objectuiv"]
        pub fn gl_get_query_objectuiv(query: u32, pname: u32) -> u32;
        /// A sync name (not a pointer).
        #[link_name = "fence_sync"]
        pub fn gl_fence_sync(condition: u32, flags: u32) -> u32;
        #[link_name = "is_sync"]
        pub fn gl_is_sync(sync: u32) -> u32;
        #[link_name = "delete_sync"]
        pub fn gl_delete_sync(sync: u32);
        /// Signaled from the next frame on; a timeout above 0 that isn't met gives GL_TIMEOUT_EXPIRED.
        #[link_name = "client_wait_sync"]
        pub fn gl_client_wait_sync(sync: u32, flags: u32, timeout: u64) -> u32;
        #[link_name = "wait_sync"]
        pub fn gl_wait_sync(sync: u32, flags: u32, timeout: u64);
        #[link_name = "get_synciv"]
        pub fn gl_get_synciv(sync: u32, pname: u32) -> i32;
        #[link_name = "create_transform_feedback"]
        pub fn gl_create_transform_feedback() -> u32;
        #[link_name = "delete_transform_feedback"]
        pub fn gl_delete_transform_feedback(tf: u32);
        #[link_name = "is_transform_feedback"]
        pub fn gl_is_transform_feedback(tf: u32) -> u32;
        #[link_name = "bind_transform_feedback"]
        pub fn gl_bind_transform_feedback(target: u32, tf: u32);
        #[link_name = "begin_transform_feedback"]
        pub fn gl_begin_transform_feedback(primitive_mode: u32);
        #[link_name = "end_transform_feedback"]
        pub fn gl_end_transform_feedback();
        #[link_name = "pause_transform_feedback"]
        pub fn gl_pause_transform_feedback();
        #[link_name = "resume_transform_feedback"]
        pub fn gl_resume_transform_feedback();
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

    #[link(wasm_import_module = "gasm:fetch")]
    unsafe extern "C" {
        /// Start a request described by JSON {"method":"GET","url":"https://...","headers":{"name":"value"}}, with body_len bytes of body (0: none). Handle > 0, or -1 if denied, invalid or too many are open (16).
        #[link_name = "request"]
        pub fn fetch_request(desc: *const u8, desc_len: u32, body: *const u8, body_len: u32) -> i32;
        /// GASM_FETCH_PENDING / HEADERS (status and headers are in) / DONE (the whole body arrived) / FAILED.
        #[link_name = "state"]
        pub fn fetch_state(req: i32) -> u32;
        /// HTTP status of the final response (after redirects), 0 before the headers or after a failure.
        #[link_name = "status"]
        pub fn fetch_status(req: i32) -> i32;
        /// Response headers as "name: value\n" lines (names lowercase). Length (copied only if <= cap; cap = 0 queries), -1 before GASM_FETCH_HEADERS.
        #[link_name = "headers"]
        pub fn fetch_headers(req: i32, dst: *mut u8, cap: u32) -> i32;
        /// Copy up to cap bytes of body that have arrived; returns the count, 0 if none are waiting yet, -1 once the body is done and drained or the request failed.
        #[link_name = "read"]
        pub fn fetch_read(req: i32, dst: *mut u8, cap: u32) -> i32;
        /// Cancel if still running and free the handle; closing again does nothing.
        #[link_name = "close"]
        pub fn fetch_close(req: i32);
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

    #[link(wasm_import_module = "gasm:clipboard")]
    unsafe extern "C" {
        /// 0: the runner puts it on the clipboard after this frame; -1: refused (over 1 MiB, or no clipboard). Invalid UTF-8 traps.
        #[link_name = "set_text"]
        pub fn clipboard_set_text(text: *const u8, text_len: u32) -> i32;
        /// The pasted text's length in bytes (copied only if <= cap; cap = 0 queries), or -1 outside a paste frame or with nothing to paste.
        #[link_name = "get_text"]
        pub fn clipboard_get_text(dst: *mut u8, cap: u32) -> i32;
    }

    #[link(wasm_import_module = "gasm:files")]
    unsafe extern "C" {
        /// A handle > 0 (the runner saves it after this frame), or -1: refused (saving is off, name empty or over 255 bytes, mime not type/subtype, over 256 MiB, or 16 saves still pending). name is a file name; the runner keeps only its last path component and makes it safe and unique. Invalid UTF-8 traps.
        #[link_name = "save"]
        pub fn files_save(name: *const u8, name_len: u32, mime: *const u8, mime_len: u32, data: *const u8, len: u32) -> i32;
        /// GASM_FILES_PENDING (0), GASM_FILES_SAVED (1) or GASM_FILES_FAILED (2: cancelled or not written). A handle save never returned traps.
        #[link_name = "state"]
        pub fn files_state(handle: i32) -> i32;
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
pub const IMPORTS: [&str; 306] = [
    "gasm", "gasm.log", "gasm.has", "gasm.time_ms",
    "gasm.max_threads", "gasm.utc_offset_minutes", "gasm.set_frame_rate", "gasm.video_present",
    "gasm.video_set_aspect", "gasm.audio_config", "gasm.audio_push", "gasm.input_pad",
    "gasm.text_input", "gasm.input_mode", "gasm.key_state", "gasm.key_events",
    "gasm.pointer", "gasm.gamepad", "gasm.gamepad_name", "gasm.asset_size",
    "gasm.asset_size64", "gasm.asset_read", "gasm.asset_read_at", "gasm.asset_read_at64",
    "gasm.asset_count", "gasm.asset_name", "gasm.asset_version", "gasm.param",
    "gasm.set_title", "gasm.yield_frame", "gasm:gfx", "gasm:gfx.width",
    "gasm:gfx.height", "gasm:gfx.create_shader", "gasm:gfx.create_buffer", "gasm:gfx.create_pipeline",
    "gasm:gfx.create_bind_group", "gasm:gfx.create_bind_group_layout", "gasm:gfx.create_texture", "gasm:gfx.write_texture",
    "gasm:gfx.create_sampler", "gasm:gfx.write_buffer", "gasm:gfx.begin_frame", "gasm:gfx.set_pipeline",
    "gasm:gfx.set_bind_group", "gasm:gfx.set_bind_group_offsets", "gasm:gfx.set_viewport", "gasm:gfx.set_scissor_rect",
    "gasm:gfx.set_vertex_buffer", "gasm:gfx.set_index_buffer", "gasm:gfx.draw", "gasm:gfx.draw_indexed",
    "gasm:gfx.end_frame", "gasm:gfx.destroy", "gasm:gl", "gasm:gl.width",
    "gasm:gl.height", "gasm:gl.frame_shown", "gasm:gl.present", "gasm:gl.get_error",
    "gasm:gl.get_string", "gasm:gl.enable_extension", "gasm:gl.get_integerv", "gasm:gl.get_floatv",
    "gasm:gl.get_integer64v", "gasm:gl.get_integeri_v", "gasm:gl.get_internalformativ", "gasm:gl.get_shader_precision_format",
    "gasm:gl.active_texture", "gasm:gl.blend_color", "gasm:gl.blend_equation", "gasm:gl.blend_equation_separate",
    "gasm:gl.blend_func", "gasm:gl.blend_func_separate", "gasm:gl.clear", "gasm:gl.clear_color",
    "gasm:gl.clear_depthf", "gasm:gl.clear_stencil", "gasm:gl.color_mask", "gasm:gl.cull_face",
    "gasm:gl.depth_func", "gasm:gl.depth_mask", "gasm:gl.depth_rangef", "gasm:gl.disable",
    "gasm:gl.enable", "gasm:gl.is_enabled", "gasm:gl.front_face", "gasm:gl.hint",
    "gasm:gl.line_width", "gasm:gl.pixel_storei", "gasm:gl.polygon_offset", "gasm:gl.sample_coverage",
    "gasm:gl.scissor", "gasm:gl.viewport", "gasm:gl.stencil_func", "gasm:gl.stencil_func_separate",
    "gasm:gl.stencil_mask", "gasm:gl.stencil_mask_separate", "gasm:gl.stencil_op", "gasm:gl.stencil_op_separate",
    "gasm:gl.finish", "gasm:gl.flush", "gasm:gl.create_buffer", "gasm:gl.delete_buffer",
    "gasm:gl.is_buffer", "gasm:gl.bind_buffer", "gasm:gl.bind_buffer_base", "gasm:gl.bind_buffer_range",
    "gasm:gl.buffer_data", "gasm:gl.buffer_sub_data", "gasm:gl.copy_buffer_sub_data", "gasm:gl.get_buffer_sub_data",
    "gasm:gl.get_buffer_parameteriv", "gasm:gl.create_vertex_array", "gasm:gl.delete_vertex_array", "gasm:gl.is_vertex_array",
    "gasm:gl.bind_vertex_array", "gasm:gl.enable_vertex_attrib_array", "gasm:gl.disable_vertex_attrib_array", "gasm:gl.vertex_attrib_pointer",
    "gasm:gl.vertex_attrib_ipointer", "gasm:gl.vertex_attrib_divisor", "gasm:gl.vertex_attrib4f", "gasm:gl.vertex_attribi4i",
    "gasm:gl.vertex_attribi4ui", "gasm:gl.get_vertex_attribiv", "gasm:gl.get_vertex_attribfv", "gasm:gl.get_vertex_attrib_offset",
    "gasm:gl.draw_arrays", "gasm:gl.draw_elements", "gasm:gl.draw_arrays_instanced", "gasm:gl.draw_elements_instanced",
    "gasm:gl.draw_range_elements", "gasm:gl.draw_buffers", "gasm:gl.clear_bufferiv", "gasm:gl.clear_bufferuiv",
    "gasm:gl.clear_bufferfv", "gasm:gl.clear_bufferfi", "gasm:gl.create_texture", "gasm:gl.delete_texture",
    "gasm:gl.is_texture", "gasm:gl.bind_texture", "gasm:gl.tex_parameteri", "gasm:gl.tex_parameterf",
    "gasm:gl.get_tex_parameteriv", "gasm:gl.get_tex_parameterfv", "gasm:gl.tex_image_2d", "gasm:gl.tex_image_3d",
    "gasm:gl.tex_sub_image_2d", "gasm:gl.tex_sub_image_3d", "gasm:gl.tex_storage_2d", "gasm:gl.tex_storage_3d",
    "gasm:gl.compressed_tex_image_2d", "gasm:gl.compressed_tex_image_3d", "gasm:gl.compressed_tex_sub_image_2d", "gasm:gl.compressed_tex_sub_image_3d",
    "gasm:gl.copy_tex_image_2d", "gasm:gl.copy_tex_sub_image_2d", "gasm:gl.copy_tex_sub_image_3d", "gasm:gl.generate_mipmap",
    "gasm:gl.create_sampler", "gasm:gl.delete_sampler", "gasm:gl.is_sampler", "gasm:gl.bind_sampler",
    "gasm:gl.sampler_parameteri", "gasm:gl.sampler_parameterf", "gasm:gl.get_sampler_parameteriv", "gasm:gl.get_sampler_parameterfv",
    "gasm:gl.create_framebuffer", "gasm:gl.delete_framebuffer", "gasm:gl.is_framebuffer", "gasm:gl.bind_framebuffer",
    "gasm:gl.check_framebuffer_status", "gasm:gl.framebuffer_texture_2d", "gasm:gl.framebuffer_texture_layer", "gasm:gl.framebuffer_renderbuffer",
    "gasm:gl.get_framebuffer_attachment_parameteriv", "gasm:gl.blit_framebuffer", "gasm:gl.invalidate_framebuffer", "gasm:gl.invalidate_sub_framebuffer",
    "gasm:gl.read_buffer", "gasm:gl.read_pixels", "gasm:gl.create_renderbuffer", "gasm:gl.delete_renderbuffer",
    "gasm:gl.is_renderbuffer", "gasm:gl.bind_renderbuffer", "gasm:gl.renderbuffer_storage", "gasm:gl.renderbuffer_storage_multisample",
    "gasm:gl.get_renderbuffer_parameteriv", "gasm:gl.create_shader", "gasm:gl.delete_shader", "gasm:gl.is_shader",
    "gasm:gl.shader_source", "gasm:gl.compile_shader", "gasm:gl.get_shaderiv", "gasm:gl.get_shader_info_log",
    "gasm:gl.get_shader_source", "gasm:gl.create_program", "gasm:gl.delete_program", "gasm:gl.is_program",
    "gasm:gl.attach_shader", "gasm:gl.detach_shader", "gasm:gl.link_program", "gasm:gl.use_program",
    "gasm:gl.validate_program", "gasm:gl.get_programiv", "gasm:gl.get_program_info_log", "gasm:gl.get_attached_shaders",
    "gasm:gl.bind_attrib_location", "gasm:gl.get_attrib_location", "gasm:gl.get_frag_data_location", "gasm:gl.get_active_attrib",
    "gasm:gl.get_active_uniform", "gasm:gl.get_uniform_location", "gasm:gl.get_uniform_index", "gasm:gl.get_active_uniformsiv",
    "gasm:gl.get_uniform_block_index", "gasm:gl.get_active_uniform_block_name", "gasm:gl.get_active_uniform_blockiv", "gasm:gl.uniform_block_binding",
    "gasm:gl.get_uniformfv", "gasm:gl.get_uniformiv", "gasm:gl.get_uniformuiv", "gasm:gl.transform_feedback_varyings",
    "gasm:gl.get_transform_feedback_varying", "gasm:gl.uniform1f", "gasm:gl.uniform2f", "gasm:gl.uniform3f",
    "gasm:gl.uniform4f", "gasm:gl.uniform1i", "gasm:gl.uniform2i", "gasm:gl.uniform3i",
    "gasm:gl.uniform4i", "gasm:gl.uniform1ui", "gasm:gl.uniform2ui", "gasm:gl.uniform3ui",
    "gasm:gl.uniform4ui", "gasm:gl.uniform1fv", "gasm:gl.uniform2fv", "gasm:gl.uniform3fv",
    "gasm:gl.uniform4fv", "gasm:gl.uniform1iv", "gasm:gl.uniform2iv", "gasm:gl.uniform3iv",
    "gasm:gl.uniform4iv", "gasm:gl.uniform1uiv", "gasm:gl.uniform2uiv", "gasm:gl.uniform3uiv",
    "gasm:gl.uniform4uiv", "gasm:gl.uniform_matrix2fv", "gasm:gl.uniform_matrix3fv", "gasm:gl.uniform_matrix4fv",
    "gasm:gl.uniform_matrix2x3fv", "gasm:gl.uniform_matrix3x2fv", "gasm:gl.uniform_matrix2x4fv", "gasm:gl.uniform_matrix4x2fv",
    "gasm:gl.uniform_matrix3x4fv", "gasm:gl.uniform_matrix4x3fv", "gasm:gl.create_query", "gasm:gl.delete_query",
    "gasm:gl.is_query", "gasm:gl.begin_query", "gasm:gl.end_query", "gasm:gl.get_queryiv",
    "gasm:gl.get_query_objectuiv", "gasm:gl.fence_sync", "gasm:gl.is_sync", "gasm:gl.delete_sync",
    "gasm:gl.client_wait_sync", "gasm:gl.wait_sync", "gasm:gl.get_synciv", "gasm:gl.create_transform_feedback",
    "gasm:gl.delete_transform_feedback", "gasm:gl.is_transform_feedback", "gasm:gl.bind_transform_feedback", "gasm:gl.begin_transform_feedback",
    "gasm:gl.end_transform_feedback", "gasm:gl.pause_transform_feedback", "gasm:gl.resume_transform_feedback", "gasm:net",
    "gasm:net.open", "gasm:net.state", "gasm:net.send", "gasm:net.recv",
    "gasm:net.close", "gasm:fetch", "gasm:fetch.request", "gasm:fetch.state",
    "gasm:fetch.status", "gasm:fetch.headers", "gasm:fetch.read", "gasm:fetch.close",
    "gasm:storage", "gasm:storage.get", "gasm:storage.set", "gasm:storage.delete",
    "gasm:storage.count", "gasm:storage.key", "gasm:clipboard", "gasm:clipboard.set_text",
    "gasm:clipboard.get_text", "gasm:files", "gasm:files.save", "gasm:files.state",
    "wasi_snapshot_preview1", "wasi_snapshot_preview1.proc_exit",
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

// ---- fetch states
pub const GASM_FETCH_PENDING: u32 = 0;
pub const GASM_FETCH_HEADERS: u32 = 1;
pub const GASM_FETCH_DONE: u32 = 2;
pub const GASM_FETCH_FAILED: u32 = 3;

// ---- input modes
/// input_mode flags.
pub const GASM_INPUT_KEYS_RAW: u32 = 1 << 0;
pub const GASM_INPUT_POINTER_HIDDEN: u32 = 1 << 1;
pub const GASM_INPUT_POINTER_LOCKED: u32 = 1 << 2;

// ---- file save states
/// gasm_files_state results.
pub const GASM_FILES_PENDING: i32 = 0;
pub const GASM_FILES_SAVED: i32 = 1;
pub const GASM_FILES_FAILED: i32 = 2;

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
