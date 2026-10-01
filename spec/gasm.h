/*
 * gasm.h — gasm ABI v0 (guest side, C/C++)
 *
 * GENERATED from spec/abi.json by scripts/gen-abi.mjs. Do not edit by hand.
 * Normative prose: spec/ABI.md · https://gasm.emdzej.pl/docs/abi
 *
 * A gasm game is a wasm32 module that imports the functions below (modules
 * "gasm", and optionally "gasm:gfx", "gasm:net", "gasm:storage") and exports:
 *   memory             required
 *   gasm_abi_version   required  Must return the ABI version (0).
 *   gasm_init          required  0 = ok; anything else aborts.
 *   gasm_frame         required  One simulation + render step, called at the frame rate.
 *   gasm_exit          optional  The player is quitting: flush saves (best effort).
 *   _initialize        optional  WASI reactor constructor hook, called first if present.
 *
 * All pointers are offsets into the guest's linear memory. Strings are UTF-8
 * (ptr, len), not NUL-terminated. Games using wasi-libc may also import a
 * WASI preview1 subset; there is no filesystem.
 */
#ifndef GASM_H
#define GASM_H

#include <stdint.h>

#define GASM_ABI_VERSION 0

#ifdef __wasm__
#define GASM_IMPORT(name) __attribute__((import_module("gasm"), import_name(name)))
#define GASM_GFX_IMPORT(name) __attribute__((import_module("gasm:gfx"), import_name(name)))
#define GASM_NET_IMPORT(name) __attribute__((import_module("gasm:net"), import_name(name)))
#define GASM_STORAGE_IMPORT(name) __attribute__((import_module("gasm:storage"), import_name(name)))
#define GASM_EXPORT(name) __attribute__((export_name(name)))
#else
#define GASM_IMPORT(name)
#define GASM_GFX_IMPORT(name)
#define GASM_NET_IMPORT(name)
#define GASM_STORAGE_IMPORT(name)
#define GASM_EXPORT(name)
#endif

#ifdef __cplusplus
extern "C" {
#endif

/* ---- buttons ----------------------------------------------------------------- */
/* Virtual gamepad buttons (bit positions). Face buttons by position: East=A,
 * South=B, North=X, West=Y. */
enum {
    GASM_BTN_A = 1u << 0,
    GASM_BTN_B = 1u << 1,
    GASM_BTN_X = 1u << 2,
    GASM_BTN_Y = 1u << 3,
    GASM_BTN_L = 1u << 4,
    GASM_BTN_R = 1u << 5,
    GASM_BTN_SELECT = 1u << 6,
    GASM_BTN_START = 1u << 7,
    GASM_BTN_UP = 1u << 8,
    GASM_BTN_DOWN = 1u << 9,
    GASM_BTN_LEFT = 1u << 10,
    GASM_BTN_RIGHT = 1u << 11,
};

/* ---- buffer usage ------------------------------------------------------------ */
enum {
    GASM_BUF_COPY_DST = 0x08,
    GASM_BUF_INDEX = 0x10,
    GASM_BUF_VERTEX = 0x20,
    GASM_BUF_UNIFORM = 0x40,
    GASM_BUF_STORAGE = 0x80,
};

/* ---- shader stages ----------------------------------------------------------- */
/* Bind group layout entry visibility (WebGPU GPUShaderStage bits). */
enum {
    GASM_STAGE_VERTEX = 0x1,
    GASM_STAGE_FRAGMENT = 0x2,
};

/* ---- index formats ----------------------------------------------------------- */
enum {
    GASM_INDEX_U16 = 0,
    GASM_INDEX_U32 = 1,
};

/* ---- net states -------------------------------------------------------------- */
enum {
    GASM_NET_CONNECTING = 0,
    GASM_NET_OPEN = 1,
    GASM_NET_CLOSED = 2,
    GASM_NET_ERROR = 3,
};

/* ---- input modes ------------------------------------------------------------- */
/* input_mode flags. */
enum {
    GASM_INPUT_KEYS_RAW = 1u << 0,
    GASM_INPUT_POINTER_HIDDEN = 1u << 1,
    GASM_INPUT_POINTER_LOCKED = 1u << 2,
};

/* ---- pointer ----------------------------------------------------------------- */
/* pointer() layout and bits. Offsets of little-endian fields: f32 x 0, y 4
 * (drawable px), fx 8, fy 12 (frame px), dx 16, dy 20 (relative motion),
 * wheel_x 24, wheel_y 28 (lines; y > 0 = down), u32 buttons 32, pressed 36,
 * released 40, flags 44. */
enum {
    GASM_POINTER_BYTES = 48,
    GASM_MOUSE_LEFT = 1u << 0,
    GASM_MOUSE_RIGHT = 1u << 1,
    GASM_MOUSE_MIDDLE = 1u << 2,
    GASM_MOUSE_BACK = 1u << 3,
    GASM_MOUSE_FORWARD = 1u << 4,
    GASM_POINTER_INSIDE = 1u << 0,
    GASM_POINTER_IS_HIDDEN = 1u << 1,
    GASM_POINTER_IS_LOCKED = 1u << 2,
};

/* ---- gamepad ----------------------------------------------------------------- */
/* gamepad() layout: u32 flags 0, u32 buttons 4, u32 axes 8, f32 button values
 * 12 (32), f32 axis values 140 (16). Standard mapping (W3C): buttons 0 south,
 * 1 east, 2 west, 3 north, 4/5 shoulders, 6/7 triggers, 8 select, 9 start,
 * 10/11 stick clicks, 12-15 d-pad up/down/left/right, 16 home; axes 0/1 left
 * stick x/y, 2/3 right stick x/y (y > 0 = down). */
enum {
    GASM_GAMEPAD_BYTES = 204,
    GASM_GAMEPAD_BUTTONS = 32,
    GASM_GAMEPAD_AXES = 16,
    GASM_GAMEPAD_CONNECTED = 1u << 0,
    GASM_GAMEPAD_STANDARD = 1u << 1,
};

/* ---- keyboard ---------------------------------------------------------------- */
/* key_state() size. */
enum {
    GASM_KEY_STATE_BYTES = 32,
};

/* ---- keys -------------------------------------------------------------------- */
/* Physical keys (W3C KeyboardEvent.code names; layout-independent). The third
 * column is the W3C name. */
enum {
    GASM_KEY_ESCAPE = 1,
    GASM_KEY_F1 = 2,
    GASM_KEY_F2 = 3,
    GASM_KEY_F3 = 4,
    GASM_KEY_F4 = 5,
    GASM_KEY_F5 = 6,
    GASM_KEY_F6 = 7,
    GASM_KEY_F7 = 8,
    GASM_KEY_F8 = 9,
    GASM_KEY_F9 = 10,
    GASM_KEY_F10 = 11,
    GASM_KEY_F11 = 12,
    GASM_KEY_F12 = 13,
    GASM_KEY_BACKQUOTE = 14,
    GASM_KEY_DIGIT0 = 15,
    GASM_KEY_DIGIT1 = 16,
    GASM_KEY_DIGIT2 = 17,
    GASM_KEY_DIGIT3 = 18,
    GASM_KEY_DIGIT4 = 19,
    GASM_KEY_DIGIT5 = 20,
    GASM_KEY_DIGIT6 = 21,
    GASM_KEY_DIGIT7 = 22,
    GASM_KEY_DIGIT8 = 23,
    GASM_KEY_DIGIT9 = 24,
    GASM_KEY_MINUS = 25,
    GASM_KEY_EQUAL = 26,
    GASM_KEY_BACKSPACE = 27,
    GASM_KEY_TAB = 28,
    GASM_KEY_KEY_A = 29,
    GASM_KEY_KEY_B = 30,
    GASM_KEY_KEY_C = 31,
    GASM_KEY_KEY_D = 32,
    GASM_KEY_KEY_E = 33,
    GASM_KEY_KEY_F = 34,
    GASM_KEY_KEY_G = 35,
    GASM_KEY_KEY_H = 36,
    GASM_KEY_KEY_I = 37,
    GASM_KEY_KEY_J = 38,
    GASM_KEY_KEY_K = 39,
    GASM_KEY_KEY_L = 40,
    GASM_KEY_KEY_M = 41,
    GASM_KEY_KEY_N = 42,
    GASM_KEY_KEY_O = 43,
    GASM_KEY_KEY_P = 44,
    GASM_KEY_KEY_Q = 45,
    GASM_KEY_KEY_R = 46,
    GASM_KEY_KEY_S = 47,
    GASM_KEY_KEY_T = 48,
    GASM_KEY_KEY_U = 49,
    GASM_KEY_KEY_V = 50,
    GASM_KEY_KEY_W = 51,
    GASM_KEY_KEY_X = 52,
    GASM_KEY_KEY_Y = 53,
    GASM_KEY_KEY_Z = 54,
    GASM_KEY_BRACKET_LEFT = 55,
    GASM_KEY_BRACKET_RIGHT = 56,
    GASM_KEY_BACKSLASH = 57,
    GASM_KEY_CAPS_LOCK = 58,
    GASM_KEY_SEMICOLON = 59,
    GASM_KEY_QUOTE = 60,
    GASM_KEY_ENTER = 61,
    GASM_KEY_SHIFT_LEFT = 62,
    GASM_KEY_INTL_BACKSLASH = 63,
    GASM_KEY_COMMA = 64,
    GASM_KEY_PERIOD = 65,
    GASM_KEY_SLASH = 66,
    GASM_KEY_SHIFT_RIGHT = 67,
    GASM_KEY_CONTROL_LEFT = 68,
    GASM_KEY_META_LEFT = 69,
    GASM_KEY_ALT_LEFT = 70,
    GASM_KEY_SPACE = 71,
    GASM_KEY_ALT_RIGHT = 72,
    GASM_KEY_META_RIGHT = 73,
    GASM_KEY_CONTEXT_MENU = 74,
    GASM_KEY_CONTROL_RIGHT = 75,
    GASM_KEY_PRINT_SCREEN = 76,
    GASM_KEY_SCROLL_LOCK = 77,
    GASM_KEY_PAUSE = 78,
    GASM_KEY_INSERT = 79,
    GASM_KEY_HOME = 80,
    GASM_KEY_PAGE_UP = 81,
    GASM_KEY_DELETE = 82,
    GASM_KEY_END = 83,
    GASM_KEY_PAGE_DOWN = 84,
    GASM_KEY_ARROW_UP = 85,
    GASM_KEY_ARROW_LEFT = 86,
    GASM_KEY_ARROW_DOWN = 87,
    GASM_KEY_ARROW_RIGHT = 88,
    GASM_KEY_NUM_LOCK = 89,
    GASM_KEY_NUMPAD_DIVIDE = 90,
    GASM_KEY_NUMPAD_MULTIPLY = 91,
    GASM_KEY_NUMPAD_SUBTRACT = 92,
    GASM_KEY_NUMPAD_ADD = 93,
    GASM_KEY_NUMPAD_ENTER = 94,
    GASM_KEY_NUMPAD_DECIMAL = 95,
    GASM_KEY_NUMPAD0 = 96,
    GASM_KEY_NUMPAD1 = 97,
    GASM_KEY_NUMPAD2 = 98,
    GASM_KEY_NUMPAD3 = 99,
    GASM_KEY_NUMPAD4 = 100,
    GASM_KEY_NUMPAD5 = 101,
    GASM_KEY_NUMPAD6 = 102,
    GASM_KEY_NUMPAD7 = 103,
    GASM_KEY_NUMPAD8 = 104,
    GASM_KEY_NUMPAD9 = 105,
    GASM_KEY_NUMPAD_EQUAL = 106,
    GASM_KEY_NUMPAD_COMMA = 107,
    GASM_KEY_INTL_RO = 108,
    GASM_KEY_INTL_YEN = 109,
    GASM_KEY_F13 = 110,
    GASM_KEY_F14 = 111,
    GASM_KEY_F15 = 112,
    GASM_KEY_F16 = 113,
    GASM_KEY_F17 = 114,
    GASM_KEY_F18 = 115,
    GASM_KEY_F19 = 116,
    GASM_KEY_F20 = 117,
    GASM_KEY_F21 = 118,
    GASM_KEY_F22 = 119,
    GASM_KEY_F23 = 120,
    GASM_KEY_F24 = 121,
};

/* ---- gasm -------------------------------------------------------------- */

/* Write a line to the runner's log. */
GASM_IMPORT("log") void gasm_log(const char *msg, uint32_t msg_len);
/* Monotonic time in milliseconds (virtual, frame-derived in headless runs). */
GASM_IMPORT("time_ms") double gasm_time_ms(void);
/* Rate (Hz) at which the runner calls gasm_frame(). Default 60; 1-1000. */
GASM_IMPORT("set_frame_rate") void gasm_set_frame_rate(double hz);
/* Present RGBA8 pixels (bytes R,G,B,A), stride bytes per row, w,h <= 4096.
 * Copied before returning; letterboxed by the runner. */
GASM_IMPORT("video_present") void gasm_video_present(const void *rgba, uint32_t width, uint32_t height, uint32_t stride);
/* Format for audio_push: 8-192 kHz, 1 or 2 channels. Default 44100/2. */
GASM_IMPORT("audio_config") void gasm_audio_config(uint32_t sample_rate, uint32_t channels);
/* Queue frames x channels interleaved f32 samples in [-1, 1]; the runner
 * resamples. */
GASM_IMPORT("audio_push") void gasm_audio_push(const float *samples, uint32_t frames);
/* Bitmask of GASM_BTN_* held on virtual pad player (0..3), stable within a
 * frame. */
GASM_IMPORT("input_pad") uint32_t gasm_input_pad(uint32_t player);
/* UTF-8 text typed since the previous frame (backspace = \b, enter = \n),
 * stable within a frame. Returns its length (copied only if length <= cap; cap
 * = 0 queries), or -1 if the runner has no keyboard. */
GASM_IMPORT("text_input") int32_t gasm_text_input(char *dst, uint32_t cap);
/* GASM_INPUT_* flags: KEYS_RAW (the runner stops mapping the keyboard to pads;
 * gamepads still map), POINTER_HIDDEN (hide the system cursor over the game),
 * POINTER_LOCKED (capture the pointer for relative motion; best effort, the
 * browser needs a click). */
GASM_IMPORT("input_mode") void gasm_input_mode(uint32_t flags);
/* Held keys as a bitset indexed by GASM_KEY_* (bit k of byte k/8), stable
 * within a frame. Copies min(len, GASM_KEY_STATE_BYTES) bytes; returns
 * GASM_KEY_STATE_BYTES, or -1 if the runner has no keyboard. */
GASM_IMPORT("key_state") int32_t gasm_key_state(uint8_t *dst, uint32_t len);
/* Key presses and releases since the previous frame, in order: 4 bytes each
 * (u16 GASM_KEY_* code, u8 1 = down / 0 = up, u8 0). Returns the byte length
 * (copied only if <= cap; cap = 0 queries), or -1 if the runner has no
 * keyboard. */
GASM_IMPORT("key_events") int32_t gasm_key_events(uint8_t *dst, uint32_t cap);
/* Mouse/touch state for this frame as GASM_POINTER_BYTES bytes (see ABI.md:
 * position in drawable and frame pixels, relative motion, wheel, buttons
 * held/pressed/released, flags). Copied only if cap is large enough; returns
 * GASM_POINTER_BYTES, or -1 if the runner has no pointer. */
GASM_IMPORT("pointer") int32_t gasm_pointer(void *dst, uint32_t cap);
/* Raw gamepad/joystick in slot 0-3 (connection order) as GASM_GAMEPAD_BYTES
 * bytes: u32 flags (GASM_GAMEPAD_CONNECTED, GASM_GAMEPAD_STANDARD), u32 button
 * count, u32 axis count, f32 buttons[32] (0-1), f32 axes[16] (-1..1). Standard
 * mapping: W3C button and axis order. Copied only if cap is large enough;
 * returns GASM_GAMEPAD_BYTES, or -1 if slot > 3 or the runner has no gamepad
 * support. */
GASM_IMPORT("gamepad") int32_t gasm_gamepad(uint32_t slot, void *dst, uint32_t cap);
/* Device name of the gamepad in slot: its length (copied only if <= cap), or
 * -1 if the slot is empty. */
GASM_IMPORT("gamepad_name") int32_t gasm_gamepad_name(uint32_t slot, char *dst, uint32_t cap);
/* Size in bytes of asset name, or -1 if it does not exist. */
GASM_IMPORT("asset_size") int32_t gasm_asset_size(const char *name, uint32_t name_len);
/* Copy up to cap bytes of asset name into dst. Bytes copied, or -1 if missing. */
GASM_IMPORT("asset_read") int32_t gasm_asset_read(const char *name, uint32_t name_len, void *dst, uint32_t cap);
/* Copy up to len bytes of asset name starting at offset (streaming). Bytes
 * copied (0 at the end), or -1 if missing. */
GASM_IMPORT("asset_read_at") int32_t gasm_asset_read_at(const char *name, uint32_t name_len, uint32_t offset, void *dst, uint32_t len);
/* Number of assets. */
GASM_IMPORT("asset_count") uint32_t gasm_asset_count(void);
/* Name of asset index (0 .. asset_count-1, sorted by UTF-8 bytes; folder
 * entries as named on disk). Its length (copied only if length <= cap; cap = 0
 * queries), or -1 if index is out of range. */
GASM_IMPORT("asset_name") int32_t gasm_asset_name(uint32_t index, char *dst, uint32_t cap);
/* Launch parameter value length, or -1 if unset. Copied only if length <= cap
 * (cap = 0 queries the length). */
GASM_IMPORT("param") int32_t gasm_param(const char *name, uint32_t name_len, char *dst, uint32_t cap);

/* ---- gasm:gfx (optional) ---------------------------------------------------------- */
/* GPU rendering: a WebGPU subset. Handles are u32 (0 is never valid); creation
 * descriptors are JSON mirroring WebGPU with color format "surface" and depth
 * format "depth24plus". Invalid descriptors trap. */

/* Current drawable width in pixels. */
GASM_GFX_IMPORT("width") uint32_t gasm_gfx_width(void);
/* Current drawable height in pixels. */
GASM_GFX_IMPORT("height") uint32_t gasm_gfx_height(void);
/* Compile WGSL source. */
GASM_GFX_IMPORT("create_shader") uint32_t gasm_gfx_create_shader(const char *wgsl, uint32_t wgsl_len);
/* size: non-zero multiple of 4; usage: GASM_BUF_* (WebGPU GPUBufferUsage bits;
 * COPY_DST always added). */
GASM_GFX_IMPORT("create_buffer") uint32_t gasm_gfx_create_buffer(uint32_t size, uint32_t usage);
/* GPURenderPipelineDescriptor as JSON. layout: "auto" (default) or an array of
 * bind group layout handles. */
GASM_GFX_IMPORT("create_pipeline") uint32_t gasm_gfx_create_pipeline(const char *json, uint32_t json_len);
/* {"layout":L,"entries":[...]} or {"pipeline":P,"group":G,"entries":[...]};
 * entries {"binding":B,"buffer":H,"offset":O,"size":S},
 * {"binding":B,"texture":T} or {"binding":B,"sampler":S}. */
GASM_GFX_IMPORT("create_bind_group") uint32_t gasm_gfx_create_bind_group(const char *json, uint32_t json_len);
/* GPUBindGroupLayoutDescriptor subset:
 * {"entries":[{"binding":B,"visibility":GASM_STAGE_*,"buffer":{"type":"uniform"|"read-only-storage","hasDynamicOffset":bool,"minBindingSize":N}
 * | "texture":{"sampleType":"float"} | "sampler":{"type":"filtering"}}]}. */
GASM_GFX_IMPORT("create_bind_group_layout") uint32_t gasm_gfx_create_bind_group_layout(const char *json, uint32_t json_len);
/* 2D texture:
 * {"size":[w,h],"format":"rgba8unorm"|"rgba8unorm-srgb","mipLevelCount":n},
 * w,h 1-8192. Usage is TEXTURE_BINDING | COPY_DST. */
GASM_GFX_IMPORT("create_texture") uint32_t gasm_gfx_create_texture(const char *json, uint32_t json_len);
/* Upload a tightly packed RGBA8 region (len = width*height*4) of mip level mip
 * at (x, y). Queued like write_buffer. */
GASM_GFX_IMPORT("write_texture") void gasm_gfx_write_texture(uint32_t texture, uint32_t mip, uint32_t x, uint32_t y, uint32_t width, uint32_t height, const void *data, uint32_t len);
/* GPUSamplerDescriptor subset: addressModeU/V, magFilter, minFilter,
 * mipmapFilter, lodMinClamp, lodMaxClamp, maxAnisotropy (1-16). */
GASM_GFX_IMPORT("create_sampler") uint32_t gasm_gfx_create_sampler(const char *json, uint32_t json_len);
/* Queue a write, applied before the frame's draws. offset/len: multiples of 4. */
GASM_GFX_IMPORT("write_buffer") void gasm_gfx_write_buffer(uint32_t buffer, uint32_t offset, const void *data, uint32_t len);
/* Start the frame (clears color and depth). 1 = will be shown, 0 = discarded
 * (the guest may skip draws). */
GASM_GFX_IMPORT("begin_frame") uint32_t gasm_gfx_begin_frame(float r, float g, float b, float a);
GASM_GFX_IMPORT("set_pipeline") void gasm_gfx_set_pipeline(uint32_t pipeline);
GASM_GFX_IMPORT("set_bind_group") void gasm_gfx_set_bind_group(uint32_t index, uint32_t bind_group);
/* set_bind_group with count dynamic offsets (multiples of 256), one per
 * dynamic-offset entry of the layout, in binding order. */
GASM_GFX_IMPORT("set_bind_group_offsets") void gasm_gfx_set_bind_group_offsets(uint32_t index, uint32_t bind_group, const uint32_t *offsets, uint32_t count);
/* Viewport in drawable pixels (clamped to the drawable), depth range 0-1.
 * Reset to the whole drawable by begin_frame. */
GASM_GFX_IMPORT("set_viewport") void gasm_gfx_set_viewport(float x, float y, float width, float height, float min_depth, float max_depth);
/* Scissor rectangle in drawable pixels (clamped to the drawable). Reset to the
 * whole drawable by begin_frame. */
GASM_GFX_IMPORT("set_scissor_rect") void gasm_gfx_set_scissor_rect(uint32_t x, uint32_t y, uint32_t width, uint32_t height);
GASM_GFX_IMPORT("set_vertex_buffer") void gasm_gfx_set_vertex_buffer(uint32_t slot, uint32_t buffer, uint32_t offset);
/* format: GASM_INDEX_U16 or GASM_INDEX_U32. */
GASM_GFX_IMPORT("set_index_buffer") void gasm_gfx_set_index_buffer(uint32_t buffer, uint32_t format, uint32_t offset);
GASM_GFX_IMPORT("draw") void gasm_gfx_draw(uint32_t vertex_count, uint32_t instance_count, uint32_t first_vertex, uint32_t first_instance);
GASM_GFX_IMPORT("draw_indexed") void gasm_gfx_draw_indexed(uint32_t index_count, uint32_t instance_count, uint32_t first_index, int32_t base_vertex, uint32_t first_instance);
/* Submit and present. */
GASM_GFX_IMPORT("end_frame") void gasm_gfx_end_frame(void);

/* ---- gasm:net (optional) ---------------------------------------------------------- */
/* Message connections with WebSocket semantics (reliable, ordered, binary),
 * non-blocking. Runners may deny connections (native: --allow-net). */

/* Open a ws:// or wss:// URL. Handle > 0, or -1 if denied/invalid. */
GASM_NET_IMPORT("open") int32_t gasm_net_open(const char *url, uint32_t url_len);
/* GASM_NET_CONNECTING / OPEN / CLOSED / ERROR. */
GASM_NET_IMPORT("state") uint32_t gasm_net_state(int32_t conn);
/* Send one message (len > 0). 0, or -1 if not open. */
GASM_NET_IMPORT("send") int32_t gasm_net_send(int32_t conn, const void *data, uint32_t len);
/* Next message's length (copied only if <= cap, else it stays queued), 0 if
 * none, -1 if closed and drained. */
GASM_NET_IMPORT("recv") int32_t gasm_net_recv(int32_t conn, void *dst, uint32_t cap);
GASM_NET_IMPORT("close") void gasm_net_close(int32_t conn);

/* ---- gasm:storage (optional) ------------------------------------------------------ */
/* Persistent per-game key/value store; the runner chooses the namespace. Keys:
 * 1-128 bytes of [A-Za-z0-9._-]. Values up to 1 MiB, 16 MiB per game. Headless
 * runs start empty. */

/* Value length, or -1 if missing. Copied only if length <= cap. */
GASM_STORAGE_IMPORT("get") int32_t gasm_storage_get(const char *key, uint32_t key_len, void *dst, uint32_t cap);
/* 0, or -1 on invalid key, too large, quota exceeded or I/O error. */
GASM_STORAGE_IMPORT("set") int32_t gasm_storage_set(const char *key, uint32_t key_len, const void *data, uint32_t len);
/* 0 if deleted, -1 if it did not exist. */
GASM_STORAGE_IMPORT("delete") int32_t gasm_storage_delete(const char *key, uint32_t key_len);
/* Number of keys in the namespace. */
GASM_STORAGE_IMPORT("count") uint32_t gasm_storage_count(void);
/* Key index (0 .. count-1, sorted): its length (copied only if <= cap; cap = 0
 * queries), or -1 if out of range. */
GASM_STORAGE_IMPORT("key") int32_t gasm_storage_key(uint32_t index, char *dst, uint32_t cap);

#ifdef __cplusplus
}
#endif

/* ---- convenience (hand-written template in scripts/gen-abi.mjs) ---------- */

/* NUL-terminated string helpers; no libc needed (work in freestanding builds). */
static inline uint32_t gasm__strlen(const char *s) {
    uint32_t n = 0;
    while (s[n]) n++;
    return n;
}
static inline void gasm_log_str(const char *s) { gasm_log(s, gasm__strlen(s)); }
static inline int32_t gasm_asset_size_str(const char *n) { return gasm_asset_size(n, gasm__strlen(n)); }
static inline int32_t gasm_asset_read_str(const char *n, void *dst, uint32_t cap) {
    return gasm_asset_read(n, gasm__strlen(n), dst, cap);
}
/* Copy parameter `name` into `dst` as a NUL-terminated string; returns 0 if unset or too long. */
static inline int gasm_param_str(const char *name, char *dst, uint32_t cap) {
    if (cap == 0) return 0;
    int32_t n = gasm_param(name, gasm__strlen(name), dst, cap - 1);
    if (n < 0 || (uint32_t)n > cap - 1) { dst[0] = 0; return 0; }
    dst[n] = 0;
    return 1;
}
static inline uint32_t gasm_gfx_create_shader_str(const char *wgsl) {
    return gasm_gfx_create_shader(wgsl, gasm__strlen(wgsl));
}
static inline uint32_t gasm_gfx_create_pipeline_str(const char *json) {
    return gasm_gfx_create_pipeline(json, gasm__strlen(json));
}
static inline uint32_t gasm_gfx_create_bind_group_str(const char *json) {
    return gasm_gfx_create_bind_group(json, gasm__strlen(json));
}
static inline uint32_t gasm_gfx_create_bind_group_layout_str(const char *json) {
    return gasm_gfx_create_bind_group_layout(json, gasm__strlen(json));
}
static inline uint32_t gasm_gfx_create_texture_str(const char *json) {
    return gasm_gfx_create_texture(json, gasm__strlen(json));
}
static inline uint32_t gasm_gfx_create_sampler_str(const char *json) {
    return gasm_gfx_create_sampler(json, gasm__strlen(json));
}
static inline int32_t gasm_net_open_str(const char *url) { return gasm_net_open(url, gasm__strlen(url)); }

#endif /* GASM_H */
