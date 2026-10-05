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
 *   gasm_run           optional  The game's whole run with its own loop: runners with stack switching call it on the first frame instead of gasm_frame; each yield_frame ends a frame. Returning ends the game with that exit code.
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
#define GASM_GL_IMPORT(name) __attribute__((import_module("gasm:gl"), import_name(name)))
#define GASM_NET_IMPORT(name) __attribute__((import_module("gasm:net"), import_name(name)))
#define GASM_FETCH_IMPORT(name) __attribute__((import_module("gasm:fetch"), import_name(name)))
#define GASM_STORAGE_IMPORT(name) __attribute__((import_module("gasm:storage"), import_name(name)))
#define GASM_CLIPBOARD_IMPORT(name) __attribute__((import_module("gasm:clipboard"), import_name(name)))
#define GASM_FILES_IMPORT(name) __attribute__((import_module("gasm:files"), import_name(name)))
#define GASM_EXPORT(name) __attribute__((export_name(name)))
#else
#define GASM_IMPORT(name)
#define GASM_GFX_IMPORT(name)
#define GASM_GL_IMPORT(name)
#define GASM_NET_IMPORT(name)
#define GASM_FETCH_IMPORT(name)
#define GASM_STORAGE_IMPORT(name)
#define GASM_CLIPBOARD_IMPORT(name)
#define GASM_FILES_IMPORT(name)
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

/* ---- fetch states ------------------------------------------------------------ */
enum {
    GASM_FETCH_PENDING = 0,
    GASM_FETCH_HEADERS = 1,
    GASM_FETCH_DONE = 2,
    GASM_FETCH_FAILED = 3,
};

/* ---- input modes ------------------------------------------------------------- */
/* input_mode flags. */
enum {
    GASM_INPUT_KEYS_RAW = 1u << 0,
    GASM_INPUT_POINTER_HIDDEN = 1u << 1,
    GASM_INPUT_POINTER_LOCKED = 1u << 2,
};

/* ---- file save states -------------------------------------------------------- */
/* gasm_files_state results. */
enum {
    GASM_FILES_PENDING = 0,
    GASM_FILES_SAVED = 1,
    GASM_FILES_FAILED = 2,
};

/* ---- storage errors ---------------------------------------------------------- */
/* gasm_storage_set results. */
enum {
    GASM_STORAGE_OK = 0,
    GASM_STORAGE_ERR_KEY = -1,
    GASM_STORAGE_ERR_SIZE = -2,
    GASM_STORAGE_ERR_QUOTA = -3,
    GASM_STORAGE_ERR_IO = -4,
};

/* ---- pointer ----------------------------------------------------------------- */
/* pointer() layout and bits: little-endian fields at the GASM_POINTER_OFF_*
 * byte offsets: f32 x, y (drawable px), fx, fy (frame px), dx, dy (relative
 * motion), wheel_x, wheel_y (lines; y > 0 = down), u32 buttons, pressed,
 * released, flags. */
enum {
    GASM_POINTER_BYTES = 48,
    GASM_POINTER_OFF_X = 0,
    GASM_POINTER_OFF_Y = 4,
    GASM_POINTER_OFF_FX = 8,
    GASM_POINTER_OFF_FY = 12,
    GASM_POINTER_OFF_DX = 16,
    GASM_POINTER_OFF_DY = 20,
    GASM_POINTER_OFF_WHEEL_X = 24,
    GASM_POINTER_OFF_WHEEL_Y = 28,
    GASM_POINTER_OFF_BUTTONS = 32,
    GASM_POINTER_OFF_PRESSED = 36,
    GASM_POINTER_OFF_RELEASED = 40,
    GASM_POINTER_OFF_FLAGS = 44,
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
/* gamepad() layout: little-endian fields at the GASM_GAMEPAD_OFF_* byte
 * offsets: u32 flags, u32 button count, u32 axis count, f32 button values
 * (32), f32 axis values (16). Standard mapping (W3C): buttons 0 south, 1 east,
 * 2 west, 3 north, 4/5 shoulders, 6/7 triggers, 8 select, 9 start, 10/11 stick
 * clicks, 12-15 d-pad up/down/left/right, 16 home; axes 0/1 left stick x/y,
 * 2/3 right stick x/y (y > 0 = down). */
enum {
    GASM_GAMEPAD_BYTES = 204,
    GASM_GAMEPAD_BUTTONS = 32,
    GASM_GAMEPAD_AXES = 16,
    GASM_GAMEPAD_OFF_FLAGS = 0,
    GASM_GAMEPAD_OFF_BUTTON_COUNT = 4,
    GASM_GAMEPAD_OFF_AXIS_COUNT = 8,
    GASM_GAMEPAD_OFF_BUTTONS = 12,
    GASM_GAMEPAD_OFF_AXES = 140,
    GASM_GAMEPAD_CONNECTED = 1u << 0,
    GASM_GAMEPAD_STANDARD = 1u << 1,
};

/* ---- keyboard ---------------------------------------------------------------- */
/* key_state() size, and the size of one key_events() record. */
enum {
    GASM_KEY_STATE_BYTES = 32,
    GASM_KEY_EVENT_BYTES = 4,
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

/* The W3C KeyboardEvent.code name of GASM_KEY_* `code`, or "" for an unknown code. */
static inline const char *gasm_key_name(uint32_t code) {
    static const char *const names[] = {
        "",
        "Escape", "F1", "F2", "F3", "F4", "F5",
        "F6", "F7", "F8", "F9", "F10", "F11",
        "F12", "Backquote", "Digit0", "Digit1", "Digit2", "Digit3",
        "Digit4", "Digit5", "Digit6", "Digit7", "Digit8", "Digit9",
        "Minus", "Equal", "Backspace", "Tab", "KeyA", "KeyB",
        "KeyC", "KeyD", "KeyE", "KeyF", "KeyG", "KeyH",
        "KeyI", "KeyJ", "KeyK", "KeyL", "KeyM", "KeyN",
        "KeyO", "KeyP", "KeyQ", "KeyR", "KeyS", "KeyT",
        "KeyU", "KeyV", "KeyW", "KeyX", "KeyY", "KeyZ",
        "BracketLeft", "BracketRight", "Backslash", "CapsLock", "Semicolon", "Quote",
        "Enter", "ShiftLeft", "IntlBackslash", "Comma", "Period", "Slash",
        "ShiftRight", "ControlLeft", "MetaLeft", "AltLeft", "Space", "AltRight",
        "MetaRight", "ContextMenu", "ControlRight", "PrintScreen", "ScrollLock", "Pause",
        "Insert", "Home", "PageUp", "Delete", "End", "PageDown",
        "ArrowUp", "ArrowLeft", "ArrowDown", "ArrowRight", "NumLock", "NumpadDivide",
        "NumpadMultiply", "NumpadSubtract", "NumpadAdd", "NumpadEnter", "NumpadDecimal", "Numpad0",
        "Numpad1", "Numpad2", "Numpad3", "Numpad4", "Numpad5", "Numpad6",
        "Numpad7", "Numpad8", "Numpad9", "NumpadEqual", "NumpadComma", "IntlRo",
        "IntlYen", "F13", "F14", "F15", "F16", "F17",
        "F18", "F19", "F20", "F21", "F22", "F23",
        "F24",
    };
    return code < sizeof names / sizeof names[0] ? names[code] : "";
}

/* ---- gasm -------------------------------------------------------------- */

/* Write a line to the runner's log. */
GASM_IMPORT("log") void gasm_log(const char *msg, uint32_t msg_len);
/* 1 if the runner provides an import module ("gasm:gfx") or a function in one
 * ("gasm.asset_size64", "gasm:gfx.destroy"), else 0. Calling an import the
 * runner lacks traps, so probe optional features first. */
GASM_IMPORT("has") int32_t gasm_has(const char *name, uint32_t name_len);
/* Monotonic time in milliseconds (virtual, frame-derived in headless runs). */
GASM_IMPORT("time_ms") double gasm_time_ms(void);
/* The player's time zone now: minutes east of UTC, daylight saving included
 * (120 for CEST, -300 for EST). 0 in headless runs. Local time is the WASI
 * realtime clock plus this offset. */
GASM_IMPORT("utc_offset_minutes") int32_t gasm_utc_offset_minutes(void);
/* Rate (Hz) at which the runner calls gasm_frame(). Default 60; 1-1000. */
GASM_IMPORT("set_frame_rate") void gasm_set_frame_rate(double hz);
/* Present RGBA8 pixels (bytes R,G,B,A), stride bytes per row, w,h <= 4096.
 * Copied before returning; letterboxed by the runner. */
GASM_IMPORT("video_present") void gasm_video_present(const void *rgba, uint32_t width, uint32_t height, uint32_t stride);
/* Show video_present frames at display aspect num:den (4:3 for 320x200 DOS
 * games) instead of square pixels; 0:0 resets. Both 1..65535 with 1/8 <=
 * num/den <= 8, else traps. Display and the pointer frame position only (not
 * hashed). Newer than has: probe has("gasm.video_set_aspect") first. */
GASM_IMPORT("video_set_aspect") void gasm_video_set_aspect(uint32_t num, uint32_t den);
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
/* Held keys as a bitset indexed by GASM_KEY_* (bit k%8 of byte k/8), stable
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
/* Size in bytes of asset name, -1 if it does not exist, or -2 if it is 2 GiB
 * or larger (use asset_size64). */
GASM_IMPORT("asset_size") int32_t gasm_asset_size(const char *name, uint32_t name_len);
/* Size in bytes of asset name (any size), or -1 if it does not exist. */
GASM_IMPORT("asset_size64") int64_t gasm_asset_size64(const char *name, uint32_t name_len);
/* Copy up to cap bytes of asset name into dst. Bytes copied, or -1 if missing. */
GASM_IMPORT("asset_read") int32_t gasm_asset_read(const char *name, uint32_t name_len, void *dst, uint32_t cap);
/* Copy up to len bytes of asset name starting at offset (streaming). Bytes
 * copied (0 at the end), or -1 if missing. */
GASM_IMPORT("asset_read_at") int32_t gasm_asset_read_at(const char *name, uint32_t name_len, uint32_t offset, void *dst, uint32_t len);
/* asset_read_at with a 64-bit offset, for assets of 4 GiB and more. */
GASM_IMPORT("asset_read_at64") int32_t gasm_asset_read_at64(const char *name, uint32_t name_len, uint64_t offset, void *dst, uint32_t len);
/* Number of assets. */
GASM_IMPORT("asset_count") uint32_t gasm_asset_count(void);
/* Name of asset index (0 .. asset_count-1, sorted by UTF-8 bytes; folder
 * entries as named on disk). Its length (copied only if length <= cap; cap = 0
 * queries), or -1 if index is out of range. */
GASM_IMPORT("asset_name") int32_t gasm_asset_name(uint32_t index, char *dst, uint32_t cap);
/* Version of asset name, -1 if it does not exist: 0 for assets given at
 * launch, a new, larger number each time the embedder replaces it while the
 * game runs (between frames). Poll it to notice new data; a read spanning
 * frames should check it did not change. */
GASM_IMPORT("asset_version") int32_t gasm_asset_version(const char *name, uint32_t name_len);
/* Launch parameter value length, or -1 if unset. Copied only if length <= cap
 * (cap = 0 queries the length). */
GASM_IMPORT("param") int32_t gasm_param(const char *name, uint32_t name_len, char *dst, uint32_t cap);
/* Name the game's window or tab (the runner adds its own suffix). Control
 * characters and bidi overrides are removed and the rest is cut to 256 bytes;
 * empty resets to the default. Newer than has: probe has("gasm.set_title")
 * first. */
GASM_IMPORT("set_title") void gasm_set_title(const char *title, uint32_t title_len);
/* End the frame inside gasm_run: the runner suspends the guest and resumes it
 * at the start of the next frame. Traps outside gasm_run (and on runners
 * without stack switching: probe has("gasm.yield_frame")). */
GASM_IMPORT("yield_frame") void gasm_yield_frame(void);

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
 * | "texture":{"sampleType":"float"|"unfilterable-float","viewDimension":"2d"}
 * | "sampler":{"type":"filtering"|"non-filtering"}}]}. */
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
/* slot 0-7; buffer with GASM_BUF_VERTEX; offset a multiple of 4, at most the
 * buffer size. */
GASM_GFX_IMPORT("set_vertex_buffer") void gasm_gfx_set_vertex_buffer(uint32_t slot, uint32_t buffer, uint32_t offset);
/* buffer with GASM_BUF_INDEX; format GASM_INDEX_U16 or GASM_INDEX_U32
 * (anything else traps); offset a multiple of the index size, at most the
 * buffer size. */
GASM_GFX_IMPORT("set_index_buffer") void gasm_gfx_set_index_buffer(uint32_t buffer, uint32_t format, uint32_t offset);
/* Needs a pipeline, and a vertex buffer in every slot the pipeline reads,
 * large enough for the vertices and instances drawn (else traps). */
GASM_GFX_IMPORT("draw") void gasm_gfx_draw(uint32_t vertex_count, uint32_t instance_count, uint32_t first_vertex, uint32_t first_instance);
/* Like draw, plus an index buffer holding first_index + index_count indices. */
GASM_GFX_IMPORT("draw_indexed") void gasm_gfx_draw_indexed(uint32_t index_count, uint32_t instance_count, uint32_t first_index, int32_t base_vertex, uint32_t first_instance);
/* Submit and present. */
GASM_GFX_IMPORT("end_frame") void gasm_gfx_end_frame(void);
/* Release an object of any kind. The handle becomes invalid (later use,
 * including another destroy, traps) and is never reused; objects created from
 * it stay valid. GPU memory is freed once nothing in a submitted frame uses
 * it. */
GASM_GFX_IMPORT("destroy") void gasm_gfx_destroy(uint32_t handle);

/* ---- gasm:gl (optional) ----------------------------------------------------------- */
/* OpenGL ES 3.0 with WebGL 2's rules (design/gasm-gl.md): object names are u32
 * (0 = none), uniform locations i32 (-1 = none), data (ptr, len) in guest
 * memory with checked lengths. GL errors are GL's (get_error, the call has no
 * effect); out-of-bounds pointers and impossible lengths trap. A module
 * imports gasm:gl or gasm:gfx, not both. */

/* Drawable width in pixels (the default framebuffer follows it). */
GASM_GL_IMPORT("width") uint32_t gasm_gl_width(void);
/* Drawable height in pixels. */
GASM_GL_IMPORT("height") uint32_t gasm_gl_height(void);
/* 0 during catch-up frames (the runner runs several frames to catch up and
 * shows only the last; the guest may skip drawing); 1 otherwise, headless
 * included. */
GASM_GL_IMPORT("frame_shown") uint32_t gasm_gl_frame_shown(void);
/* Show the default framebuffer now; otherwise the runner presents at the end
 * of the frame. */
GASM_GL_IMPORT("present") void gasm_gl_present(void);
/* The oldest GL error flag (GL rules: a call with an error has no effect), or
 * 0. */
GASM_GL_IMPORT("get_error") uint32_t gasm_gl_get_error(void);
/* GL_VENDOR, GL_RENDERER, GL_VERSION, GL_SHADING_LANGUAGE_VERSION or
 * GL_EXTENSIONS (space-separated): its length, copied if it fits; -1 for an
 * invalid name (GL_INVALID_ENUM). */
GASM_GL_IMPORT("get_string") int32_t gasm_gl_get_string(uint32_t name, void *dst, uint32_t cap);
/* Turn on a WebGL extension listed in GL_EXTENSIONS (1), or 0 if it isn't
 * there. */
GASM_GL_IMPORT("enable_extension") uint32_t gasm_gl_enable_extension(const char *name, uint32_t name_len);
/* The parameter's values as i32s: how many it has (copied up to count), or -1
 * for an invalid pname (GL_INVALID_ENUM). */
GASM_GL_IMPORT("get_integerv") int32_t gasm_gl_get_integerv(uint32_t pname, void *dst, uint32_t count);
/* As get_integerv, as f32s. */
GASM_GL_IMPORT("get_floatv") int32_t gasm_gl_get_floatv(uint32_t pname, void *dst, uint32_t count);
/* As get_integerv, as i64s. */
GASM_GL_IMPORT("get_integer64v") int32_t gasm_gl_get_integer64v(uint32_t pname, void *dst, uint32_t count);
/* Indexed parameters (e.g. GL_UNIFORM_BUFFER_BINDING), as i32s. */
GASM_GL_IMPORT("get_integeri_v") int32_t gasm_gl_get_integeri_v(uint32_t target, uint32_t index, void *dst, uint32_t count);
/* GL_SAMPLES or GL_NUM_SAMPLE_COUNTS of a renderbuffer format. */
GASM_GL_IMPORT("get_internalformativ") int32_t gasm_gl_get_internalformativ(uint32_t target, uint32_t internalformat, uint32_t pname, void *dst, uint32_t count);
/* Writes 3 i32s: range min, range max, precision. */
GASM_GL_IMPORT("get_shader_precision_format") void gasm_gl_get_shader_precision_format(uint32_t shadertype, uint32_t precisiontype, void *dst);
GASM_GL_IMPORT("active_texture") void gasm_gl_active_texture(uint32_t texture);
GASM_GL_IMPORT("blend_color") void gasm_gl_blend_color(float red, float green, float blue, float alpha);
GASM_GL_IMPORT("blend_equation") void gasm_gl_blend_equation(uint32_t mode);
GASM_GL_IMPORT("blend_equation_separate") void gasm_gl_blend_equation_separate(uint32_t mode_rgb, uint32_t mode_alpha);
GASM_GL_IMPORT("blend_func") void gasm_gl_blend_func(uint32_t sfactor, uint32_t dfactor);
GASM_GL_IMPORT("blend_func_separate") void gasm_gl_blend_func_separate(uint32_t src_rgb, uint32_t dst_rgb, uint32_t src_alpha, uint32_t dst_alpha);
GASM_GL_IMPORT("clear") void gasm_gl_clear(uint32_t mask);
GASM_GL_IMPORT("clear_color") void gasm_gl_clear_color(float red, float green, float blue, float alpha);
GASM_GL_IMPORT("clear_depthf") void gasm_gl_clear_depthf(float depth);
GASM_GL_IMPORT("clear_stencil") void gasm_gl_clear_stencil(int32_t s);
GASM_GL_IMPORT("color_mask") void gasm_gl_color_mask(uint32_t red, uint32_t green, uint32_t blue, uint32_t alpha);
GASM_GL_IMPORT("cull_face") void gasm_gl_cull_face(uint32_t mode);
GASM_GL_IMPORT("depth_func") void gasm_gl_depth_func(uint32_t func);
GASM_GL_IMPORT("depth_mask") void gasm_gl_depth_mask(uint32_t flag);
GASM_GL_IMPORT("depth_rangef") void gasm_gl_depth_rangef(float near, float far);
GASM_GL_IMPORT("disable") void gasm_gl_disable(uint32_t cap);
GASM_GL_IMPORT("enable") void gasm_gl_enable(uint32_t cap);
GASM_GL_IMPORT("is_enabled") uint32_t gasm_gl_is_enabled(uint32_t cap);
GASM_GL_IMPORT("front_face") void gasm_gl_front_face(uint32_t mode);
GASM_GL_IMPORT("hint") void gasm_gl_hint(uint32_t target, uint32_t mode);
GASM_GL_IMPORT("line_width") void gasm_gl_line_width(float width);
GASM_GL_IMPORT("pixel_storei") void gasm_gl_pixel_storei(uint32_t pname, int32_t param);
GASM_GL_IMPORT("polygon_offset") void gasm_gl_polygon_offset(float factor, float units);
GASM_GL_IMPORT("sample_coverage") void gasm_gl_sample_coverage(float value, uint32_t invert);
GASM_GL_IMPORT("scissor") void gasm_gl_scissor(int32_t x, int32_t y, int32_t width, int32_t height);
GASM_GL_IMPORT("viewport") void gasm_gl_viewport(int32_t x, int32_t y, int32_t width, int32_t height);
GASM_GL_IMPORT("stencil_func") void gasm_gl_stencil_func(uint32_t func, int32_t ref, uint32_t mask);
GASM_GL_IMPORT("stencil_func_separate") void gasm_gl_stencil_func_separate(uint32_t face, uint32_t func, int32_t ref, uint32_t mask);
GASM_GL_IMPORT("stencil_mask") void gasm_gl_stencil_mask(uint32_t mask);
GASM_GL_IMPORT("stencil_mask_separate") void gasm_gl_stencil_mask_separate(uint32_t face, uint32_t mask);
GASM_GL_IMPORT("stencil_op") void gasm_gl_stencil_op(uint32_t fail, uint32_t zfail, uint32_t zpass);
GASM_GL_IMPORT("stencil_op_separate") void gasm_gl_stencil_op_separate(uint32_t face, uint32_t sfail, uint32_t dpfail, uint32_t dppass);
GASM_GL_IMPORT("finish") void gasm_gl_finish(void);
GASM_GL_IMPORT("flush") void gasm_gl_flush(void);
/* A new buffer name (glGenBuffers). */
GASM_GL_IMPORT("create_buffer") uint32_t gasm_gl_create_buffer(void);
GASM_GL_IMPORT("delete_buffer") void gasm_gl_delete_buffer(uint32_t buffer);
GASM_GL_IMPORT("is_buffer") uint32_t gasm_gl_is_buffer(uint32_t buffer);
GASM_GL_IMPORT("bind_buffer") void gasm_gl_bind_buffer(uint32_t target, uint32_t buffer);
GASM_GL_IMPORT("bind_buffer_base") void gasm_gl_bind_buffer_base(uint32_t target, uint32_t index, uint32_t buffer);
GASM_GL_IMPORT("bind_buffer_range") void gasm_gl_bind_buffer_range(uint32_t target, uint32_t index, uint32_t buffer, uint32_t offset, uint32_t size);
/* len bytes from data, or zeros if data is 0. */
GASM_GL_IMPORT("buffer_data") void gasm_gl_buffer_data(uint32_t target, const void *data, uint32_t len, uint32_t usage);
GASM_GL_IMPORT("buffer_sub_data") void gasm_gl_buffer_sub_data(uint32_t target, uint32_t offset, const void *data, uint32_t len);
GASM_GL_IMPORT("copy_buffer_sub_data") void gasm_gl_copy_buffer_sub_data(uint32_t read_target, uint32_t write_target, uint32_t read_offset, uint32_t write_offset, uint32_t size);
/* Read back len bytes (glMapBufferRange for reading). */
GASM_GL_IMPORT("get_buffer_sub_data") void gasm_gl_get_buffer_sub_data(uint32_t target, uint32_t offset, void *dst, uint32_t len);
GASM_GL_IMPORT("get_buffer_parameteriv") int32_t gasm_gl_get_buffer_parameteriv(uint32_t target, uint32_t pname);
GASM_GL_IMPORT("create_vertex_array") uint32_t gasm_gl_create_vertex_array(void);
GASM_GL_IMPORT("delete_vertex_array") void gasm_gl_delete_vertex_array(uint32_t array);
GASM_GL_IMPORT("is_vertex_array") uint32_t gasm_gl_is_vertex_array(uint32_t array);
GASM_GL_IMPORT("bind_vertex_array") void gasm_gl_bind_vertex_array(uint32_t array);
GASM_GL_IMPORT("enable_vertex_attrib_array") void gasm_gl_enable_vertex_attrib_array(uint32_t index);
GASM_GL_IMPORT("disable_vertex_attrib_array") void gasm_gl_disable_vertex_attrib_array(uint32_t index);
/* offset into the bound GL_ARRAY_BUFFER (no client-side arrays). */
GASM_GL_IMPORT("vertex_attrib_pointer") void gasm_gl_vertex_attrib_pointer(uint32_t index, int32_t size, uint32_t type, uint32_t normalized, int32_t stride, uint32_t offset);
GASM_GL_IMPORT("vertex_attrib_ipointer") void gasm_gl_vertex_attrib_ipointer(uint32_t index, int32_t size, uint32_t type, int32_t stride, uint32_t offset);
GASM_GL_IMPORT("vertex_attrib_divisor") void gasm_gl_vertex_attrib_divisor(uint32_t index, uint32_t divisor);
GASM_GL_IMPORT("vertex_attrib4f") void gasm_gl_vertex_attrib4f(uint32_t index, float x, float y, float z, float w);
GASM_GL_IMPORT("vertex_attribi4i") void gasm_gl_vertex_attribi4i(uint32_t index, int32_t x, int32_t y, int32_t z, int32_t w);
GASM_GL_IMPORT("vertex_attribi4ui") void gasm_gl_vertex_attribi4ui(uint32_t index, uint32_t x, uint32_t y, uint32_t z, uint32_t w);
GASM_GL_IMPORT("get_vertex_attribiv") int32_t gasm_gl_get_vertex_attribiv(uint32_t index, uint32_t pname);
/* GL_CURRENT_VERTEX_ATTRIB: 4 f32s. */
GASM_GL_IMPORT("get_vertex_attribfv") int32_t gasm_gl_get_vertex_attribfv(uint32_t index, uint32_t pname, void *dst, uint32_t count);
GASM_GL_IMPORT("get_vertex_attrib_offset") uint32_t gasm_gl_get_vertex_attrib_offset(uint32_t index, uint32_t pname);
GASM_GL_IMPORT("draw_arrays") void gasm_gl_draw_arrays(uint32_t mode, int32_t first, int32_t count);
/* Indices from the bound GL_ELEMENT_ARRAY_BUFFER at offset. */
GASM_GL_IMPORT("draw_elements") void gasm_gl_draw_elements(uint32_t mode, int32_t count, uint32_t type, uint32_t offset);
GASM_GL_IMPORT("draw_arrays_instanced") void gasm_gl_draw_arrays_instanced(uint32_t mode, int32_t first, int32_t count, int32_t instances);
GASM_GL_IMPORT("draw_elements_instanced") void gasm_gl_draw_elements_instanced(uint32_t mode, int32_t count, uint32_t type, uint32_t offset, int32_t instances);
GASM_GL_IMPORT("draw_range_elements") void gasm_gl_draw_range_elements(uint32_t mode, uint32_t start, uint32_t end, int32_t count, uint32_t type, uint32_t offset);
/* count GLenums. */
GASM_GL_IMPORT("draw_buffers") void gasm_gl_draw_buffers(const void *bufs, uint32_t count);
/* count i32s (4 for GL_COLOR, 1 for GL_STENCIL). */
GASM_GL_IMPORT("clear_bufferiv") void gasm_gl_clear_bufferiv(uint32_t buffer, int32_t drawbuffer, const void *value, uint32_t count);
GASM_GL_IMPORT("clear_bufferuiv") void gasm_gl_clear_bufferuiv(uint32_t buffer, int32_t drawbuffer, const void *value, uint32_t count);
GASM_GL_IMPORT("clear_bufferfv") void gasm_gl_clear_bufferfv(uint32_t buffer, int32_t drawbuffer, const void *value, uint32_t count);
GASM_GL_IMPORT("clear_bufferfi") void gasm_gl_clear_bufferfi(uint32_t buffer, int32_t drawbuffer, float depth, int32_t stencil);
GASM_GL_IMPORT("create_texture") uint32_t gasm_gl_create_texture(void);
GASM_GL_IMPORT("delete_texture") void gasm_gl_delete_texture(uint32_t texture);
GASM_GL_IMPORT("is_texture") uint32_t gasm_gl_is_texture(uint32_t texture);
GASM_GL_IMPORT("bind_texture") void gasm_gl_bind_texture(uint32_t target, uint32_t texture);
GASM_GL_IMPORT("tex_parameteri") void gasm_gl_tex_parameteri(uint32_t target, uint32_t pname, int32_t param);
GASM_GL_IMPORT("tex_parameterf") void gasm_gl_tex_parameterf(uint32_t target, uint32_t pname, float param);
GASM_GL_IMPORT("get_tex_parameteriv") int32_t gasm_gl_get_tex_parameteriv(uint32_t target, uint32_t pname);
GASM_GL_IMPORT("get_tex_parameterfv") float gasm_gl_get_tex_parameterfv(uint32_t target, uint32_t pname);
/* pixels 0: no data (or, with a bound GL_PIXEL_UNPACK_BUFFER, len is the
 * offset into it); otherwise len must cover the image. */
GASM_GL_IMPORT("tex_image_2d") void gasm_gl_tex_image_2d(uint32_t target, int32_t level, int32_t internalformat, int32_t width, int32_t height, int32_t border, uint32_t format, uint32_t type, const void *pixels, uint32_t len);
GASM_GL_IMPORT("tex_image_3d") void gasm_gl_tex_image_3d(uint32_t target, int32_t level, int32_t internalformat, int32_t width, int32_t height, int32_t depth, int32_t border, uint32_t format, uint32_t type, const void *pixels, uint32_t len);
GASM_GL_IMPORT("tex_sub_image_2d") void gasm_gl_tex_sub_image_2d(uint32_t target, int32_t level, int32_t x, int32_t y, int32_t width, int32_t height, uint32_t format, uint32_t type, const void *pixels, uint32_t len);
GASM_GL_IMPORT("tex_sub_image_3d") void gasm_gl_tex_sub_image_3d(uint32_t target, int32_t level, int32_t x, int32_t y, int32_t z, int32_t width, int32_t height, int32_t depth, uint32_t format, uint32_t type, const void *pixels, uint32_t len);
GASM_GL_IMPORT("tex_storage_2d") void gasm_gl_tex_storage_2d(uint32_t target, int32_t levels, uint32_t internalformat, int32_t width, int32_t height);
GASM_GL_IMPORT("tex_storage_3d") void gasm_gl_tex_storage_3d(uint32_t target, int32_t levels, uint32_t internalformat, int32_t width, int32_t height, int32_t depth);
GASM_GL_IMPORT("compressed_tex_image_2d") void gasm_gl_compressed_tex_image_2d(uint32_t target, int32_t level, uint32_t internalformat, int32_t width, int32_t height, int32_t border, const void *data, uint32_t len);
GASM_GL_IMPORT("compressed_tex_image_3d") void gasm_gl_compressed_tex_image_3d(uint32_t target, int32_t level, uint32_t internalformat, int32_t width, int32_t height, int32_t depth, int32_t border, const void *data, uint32_t len);
GASM_GL_IMPORT("compressed_tex_sub_image_2d") void gasm_gl_compressed_tex_sub_image_2d(uint32_t target, int32_t level, int32_t x, int32_t y, int32_t width, int32_t height, uint32_t format, const void *data, uint32_t len);
GASM_GL_IMPORT("compressed_tex_sub_image_3d") void gasm_gl_compressed_tex_sub_image_3d(uint32_t target, int32_t level, int32_t x, int32_t y, int32_t z, int32_t width, int32_t height, int32_t depth, uint32_t format, const void *data, uint32_t len);
GASM_GL_IMPORT("copy_tex_image_2d") void gasm_gl_copy_tex_image_2d(uint32_t target, int32_t level, uint32_t internalformat, int32_t x, int32_t y, int32_t width, int32_t height, int32_t border);
GASM_GL_IMPORT("copy_tex_sub_image_2d") void gasm_gl_copy_tex_sub_image_2d(uint32_t target, int32_t level, int32_t xoffset, int32_t yoffset, int32_t x, int32_t y, int32_t width, int32_t height);
GASM_GL_IMPORT("copy_tex_sub_image_3d") void gasm_gl_copy_tex_sub_image_3d(uint32_t target, int32_t level, int32_t xoffset, int32_t yoffset, int32_t zoffset, int32_t x, int32_t y, int32_t width, int32_t height);
/* Runners generate the levels (unlike gasm:gfx). */
GASM_GL_IMPORT("generate_mipmap") void gasm_gl_generate_mipmap(uint32_t target);
GASM_GL_IMPORT("create_sampler") uint32_t gasm_gl_create_sampler(void);
GASM_GL_IMPORT("delete_sampler") void gasm_gl_delete_sampler(uint32_t sampler);
GASM_GL_IMPORT("is_sampler") uint32_t gasm_gl_is_sampler(uint32_t sampler);
GASM_GL_IMPORT("bind_sampler") void gasm_gl_bind_sampler(uint32_t unit, uint32_t sampler);
GASM_GL_IMPORT("sampler_parameteri") void gasm_gl_sampler_parameteri(uint32_t sampler, uint32_t pname, int32_t param);
GASM_GL_IMPORT("sampler_parameterf") void gasm_gl_sampler_parameterf(uint32_t sampler, uint32_t pname, float param);
GASM_GL_IMPORT("get_sampler_parameteriv") int32_t gasm_gl_get_sampler_parameteriv(uint32_t sampler, uint32_t pname);
GASM_GL_IMPORT("get_sampler_parameterfv") float gasm_gl_get_sampler_parameterfv(uint32_t sampler, uint32_t pname);
GASM_GL_IMPORT("create_framebuffer") uint32_t gasm_gl_create_framebuffer(void);
GASM_GL_IMPORT("delete_framebuffer") void gasm_gl_delete_framebuffer(uint32_t framebuffer);
GASM_GL_IMPORT("is_framebuffer") uint32_t gasm_gl_is_framebuffer(uint32_t framebuffer);
/* 0: the default framebuffer (the window). */
GASM_GL_IMPORT("bind_framebuffer") void gasm_gl_bind_framebuffer(uint32_t target, uint32_t framebuffer);
GASM_GL_IMPORT("check_framebuffer_status") uint32_t gasm_gl_check_framebuffer_status(uint32_t target);
GASM_GL_IMPORT("framebuffer_texture_2d") void gasm_gl_framebuffer_texture_2d(uint32_t target, uint32_t attachment, uint32_t textarget, uint32_t texture, int32_t level);
GASM_GL_IMPORT("framebuffer_texture_layer") void gasm_gl_framebuffer_texture_layer(uint32_t target, uint32_t attachment, uint32_t texture, int32_t level, int32_t layer);
GASM_GL_IMPORT("framebuffer_renderbuffer") void gasm_gl_framebuffer_renderbuffer(uint32_t target, uint32_t attachment, uint32_t renderbuffertarget, uint32_t renderbuffer);
GASM_GL_IMPORT("get_framebuffer_attachment_parameteriv") int32_t gasm_gl_get_framebuffer_attachment_parameteriv(uint32_t target, uint32_t attachment, uint32_t pname);
GASM_GL_IMPORT("blit_framebuffer") void gasm_gl_blit_framebuffer(int32_t src_x0, int32_t src_y0, int32_t src_x1, int32_t src_y1, int32_t dst_x0, int32_t dst_y0, int32_t dst_x1, int32_t dst_y1, uint32_t mask, uint32_t filter);
GASM_GL_IMPORT("invalidate_framebuffer") void gasm_gl_invalidate_framebuffer(uint32_t target, const void *attachments, uint32_t count);
GASM_GL_IMPORT("invalidate_sub_framebuffer") void gasm_gl_invalidate_sub_framebuffer(uint32_t target, const void *attachments, uint32_t count, int32_t x, int32_t y, int32_t width, int32_t height);
GASM_GL_IMPORT("read_buffer") void gasm_gl_read_buffer(uint32_t src);
/* len must cover the rectangle; with a bound GL_PIXEL_PACK_BUFFER, dst is the
 * offset into it. */
GASM_GL_IMPORT("read_pixels") void gasm_gl_read_pixels(int32_t x, int32_t y, int32_t width, int32_t height, uint32_t format, uint32_t type, void *dst, uint32_t len);
GASM_GL_IMPORT("create_renderbuffer") uint32_t gasm_gl_create_renderbuffer(void);
GASM_GL_IMPORT("delete_renderbuffer") void gasm_gl_delete_renderbuffer(uint32_t renderbuffer);
GASM_GL_IMPORT("is_renderbuffer") uint32_t gasm_gl_is_renderbuffer(uint32_t renderbuffer);
GASM_GL_IMPORT("bind_renderbuffer") void gasm_gl_bind_renderbuffer(uint32_t target, uint32_t renderbuffer);
GASM_GL_IMPORT("renderbuffer_storage") void gasm_gl_renderbuffer_storage(uint32_t target, uint32_t internalformat, int32_t width, int32_t height);
GASM_GL_IMPORT("renderbuffer_storage_multisample") void gasm_gl_renderbuffer_storage_multisample(uint32_t target, int32_t samples, uint32_t internalformat, int32_t width, int32_t height);
GASM_GL_IMPORT("get_renderbuffer_parameteriv") int32_t gasm_gl_get_renderbuffer_parameteriv(uint32_t target, uint32_t pname);
GASM_GL_IMPORT("create_shader") uint32_t gasm_gl_create_shader(uint32_t type);
GASM_GL_IMPORT("delete_shader") void gasm_gl_delete_shader(uint32_t shader);
GASM_GL_IMPORT("is_shader") uint32_t gasm_gl_is_shader(uint32_t shader);
/* GLSL ES 3.00 (or 1.00). */
GASM_GL_IMPORT("shader_source") void gasm_gl_shader_source(uint32_t shader, const char *source, uint32_t source_len);
GASM_GL_IMPORT("compile_shader") void gasm_gl_compile_shader(uint32_t shader);
GASM_GL_IMPORT("get_shaderiv") int32_t gasm_gl_get_shaderiv(uint32_t shader, uint32_t pname);
GASM_GL_IMPORT("get_shader_info_log") int32_t gasm_gl_get_shader_info_log(uint32_t shader, void *dst, uint32_t cap);
GASM_GL_IMPORT("get_shader_source") int32_t gasm_gl_get_shader_source(uint32_t shader, void *dst, uint32_t cap);
GASM_GL_IMPORT("create_program") uint32_t gasm_gl_create_program(void);
GASM_GL_IMPORT("delete_program") void gasm_gl_delete_program(uint32_t program);
GASM_GL_IMPORT("is_program") uint32_t gasm_gl_is_program(uint32_t program);
GASM_GL_IMPORT("attach_shader") void gasm_gl_attach_shader(uint32_t program, uint32_t shader);
GASM_GL_IMPORT("detach_shader") void gasm_gl_detach_shader(uint32_t program, uint32_t shader);
GASM_GL_IMPORT("link_program") void gasm_gl_link_program(uint32_t program);
GASM_GL_IMPORT("use_program") void gasm_gl_use_program(uint32_t program);
GASM_GL_IMPORT("validate_program") void gasm_gl_validate_program(uint32_t program);
GASM_GL_IMPORT("get_programiv") int32_t gasm_gl_get_programiv(uint32_t program, uint32_t pname);
GASM_GL_IMPORT("get_program_info_log") int32_t gasm_gl_get_program_info_log(uint32_t program, void *dst, uint32_t cap);
/* Shader names (u32), copied up to count; returns how many. */
GASM_GL_IMPORT("get_attached_shaders") int32_t gasm_gl_get_attached_shaders(uint32_t program, void *dst, uint32_t count);
GASM_GL_IMPORT("bind_attrib_location") void gasm_gl_bind_attrib_location(uint32_t program, uint32_t index, const char *name, uint32_t name_len);
GASM_GL_IMPORT("get_attrib_location") int32_t gasm_gl_get_attrib_location(uint32_t program, const char *name, uint32_t name_len);
GASM_GL_IMPORT("get_frag_data_location") int32_t gasm_gl_get_frag_data_location(uint32_t program, const char *name, uint32_t name_len);
/* info: 2 i32s (size, type). Returns the name's length (copied if it fits), or
 * -1. */
GASM_GL_IMPORT("get_active_attrib") int32_t gasm_gl_get_active_attrib(uint32_t program, uint32_t index, void *name, uint32_t cap, void *info);
GASM_GL_IMPORT("get_active_uniform") int32_t gasm_gl_get_active_uniform(uint32_t program, uint32_t index, void *name, uint32_t cap, void *info);
/* -1 if the program has no such uniform. */
GASM_GL_IMPORT("get_uniform_location") int32_t gasm_gl_get_uniform_location(uint32_t program, const char *name, uint32_t name_len);
/* glGetUniformIndices for one name (GL_INVALID_INDEX if none). */
GASM_GL_IMPORT("get_uniform_index") uint32_t gasm_gl_get_uniform_index(uint32_t program, const char *name, uint32_t name_len);
/* count u32 indices in, count i32s out. */
GASM_GL_IMPORT("get_active_uniformsiv") void gasm_gl_get_active_uniformsiv(uint32_t program, const void *indices, uint32_t count, uint32_t pname, void *dst);
GASM_GL_IMPORT("get_uniform_block_index") uint32_t gasm_gl_get_uniform_block_index(uint32_t program, const char *name, uint32_t name_len);
GASM_GL_IMPORT("get_active_uniform_block_name") int32_t gasm_gl_get_active_uniform_block_name(uint32_t program, uint32_t index, void *dst, uint32_t cap);
GASM_GL_IMPORT("get_active_uniform_blockiv") int32_t gasm_gl_get_active_uniform_blockiv(uint32_t program, uint32_t index, uint32_t pname, void *dst, uint32_t count);
GASM_GL_IMPORT("uniform_block_binding") void gasm_gl_uniform_block_binding(uint32_t program, uint32_t index, uint32_t binding);
GASM_GL_IMPORT("get_uniformfv") int32_t gasm_gl_get_uniformfv(uint32_t program, int32_t location, void *dst, uint32_t count);
GASM_GL_IMPORT("get_uniformiv") int32_t gasm_gl_get_uniformiv(uint32_t program, int32_t location, void *dst, uint32_t count);
GASM_GL_IMPORT("get_uniformuiv") int32_t gasm_gl_get_uniformuiv(uint32_t program, int32_t location, void *dst, uint32_t count);
/* count names, each NUL-terminated, in len bytes. */
GASM_GL_IMPORT("transform_feedback_varyings") void gasm_gl_transform_feedback_varyings(uint32_t program, const void *names, uint32_t len, uint32_t count, uint32_t buffer_mode);
GASM_GL_IMPORT("get_transform_feedback_varying") int32_t gasm_gl_get_transform_feedback_varying(uint32_t program, uint32_t index, void *name, uint32_t cap, void *info);
GASM_GL_IMPORT("uniform1f") void gasm_gl_uniform1f(int32_t location, float x);
GASM_GL_IMPORT("uniform2f") void gasm_gl_uniform2f(int32_t location, float x, float y);
GASM_GL_IMPORT("uniform3f") void gasm_gl_uniform3f(int32_t location, float x, float y, float z);
GASM_GL_IMPORT("uniform4f") void gasm_gl_uniform4f(int32_t location, float x, float y, float z, float w);
GASM_GL_IMPORT("uniform1i") void gasm_gl_uniform1i(int32_t location, int32_t x);
GASM_GL_IMPORT("uniform2i") void gasm_gl_uniform2i(int32_t location, int32_t x, int32_t y);
GASM_GL_IMPORT("uniform3i") void gasm_gl_uniform3i(int32_t location, int32_t x, int32_t y, int32_t z);
GASM_GL_IMPORT("uniform4i") void gasm_gl_uniform4i(int32_t location, int32_t x, int32_t y, int32_t z, int32_t w);
GASM_GL_IMPORT("uniform1ui") void gasm_gl_uniform1ui(int32_t location, uint32_t x);
GASM_GL_IMPORT("uniform2ui") void gasm_gl_uniform2ui(int32_t location, uint32_t x, uint32_t y);
GASM_GL_IMPORT("uniform3ui") void gasm_gl_uniform3ui(int32_t location, uint32_t x, uint32_t y, uint32_t z);
GASM_GL_IMPORT("uniform4ui") void gasm_gl_uniform4ui(int32_t location, uint32_t x, uint32_t y, uint32_t z, uint32_t w);
/* count vectors (GL semantics). */
GASM_GL_IMPORT("uniform1fv") void gasm_gl_uniform1fv(int32_t location, int32_t count, const void *value);
GASM_GL_IMPORT("uniform2fv") void gasm_gl_uniform2fv(int32_t location, int32_t count, const void *value);
GASM_GL_IMPORT("uniform3fv") void gasm_gl_uniform3fv(int32_t location, int32_t count, const void *value);
GASM_GL_IMPORT("uniform4fv") void gasm_gl_uniform4fv(int32_t location, int32_t count, const void *value);
GASM_GL_IMPORT("uniform1iv") void gasm_gl_uniform1iv(int32_t location, int32_t count, const void *value);
GASM_GL_IMPORT("uniform2iv") void gasm_gl_uniform2iv(int32_t location, int32_t count, const void *value);
GASM_GL_IMPORT("uniform3iv") void gasm_gl_uniform3iv(int32_t location, int32_t count, const void *value);
GASM_GL_IMPORT("uniform4iv") void gasm_gl_uniform4iv(int32_t location, int32_t count, const void *value);
GASM_GL_IMPORT("uniform1uiv") void gasm_gl_uniform1uiv(int32_t location, int32_t count, const void *value);
GASM_GL_IMPORT("uniform2uiv") void gasm_gl_uniform2uiv(int32_t location, int32_t count, const void *value);
GASM_GL_IMPORT("uniform3uiv") void gasm_gl_uniform3uiv(int32_t location, int32_t count, const void *value);
GASM_GL_IMPORT("uniform4uiv") void gasm_gl_uniform4uiv(int32_t location, int32_t count, const void *value);
GASM_GL_IMPORT("uniform_matrix2fv") void gasm_gl_uniform_matrix2fv(int32_t location, int32_t count, uint32_t transpose, const void *value);
GASM_GL_IMPORT("uniform_matrix3fv") void gasm_gl_uniform_matrix3fv(int32_t location, int32_t count, uint32_t transpose, const void *value);
GASM_GL_IMPORT("uniform_matrix4fv") void gasm_gl_uniform_matrix4fv(int32_t location, int32_t count, uint32_t transpose, const void *value);
GASM_GL_IMPORT("uniform_matrix2x3fv") void gasm_gl_uniform_matrix2x3fv(int32_t location, int32_t count, uint32_t transpose, const void *value);
GASM_GL_IMPORT("uniform_matrix3x2fv") void gasm_gl_uniform_matrix3x2fv(int32_t location, int32_t count, uint32_t transpose, const void *value);
GASM_GL_IMPORT("uniform_matrix2x4fv") void gasm_gl_uniform_matrix2x4fv(int32_t location, int32_t count, uint32_t transpose, const void *value);
GASM_GL_IMPORT("uniform_matrix4x2fv") void gasm_gl_uniform_matrix4x2fv(int32_t location, int32_t count, uint32_t transpose, const void *value);
GASM_GL_IMPORT("uniform_matrix3x4fv") void gasm_gl_uniform_matrix3x4fv(int32_t location, int32_t count, uint32_t transpose, const void *value);
GASM_GL_IMPORT("uniform_matrix4x3fv") void gasm_gl_uniform_matrix4x3fv(int32_t location, int32_t count, uint32_t transpose, const void *value);
GASM_GL_IMPORT("create_query") uint32_t gasm_gl_create_query(void);
GASM_GL_IMPORT("delete_query") void gasm_gl_delete_query(uint32_t query);
GASM_GL_IMPORT("is_query") uint32_t gasm_gl_is_query(uint32_t query);
GASM_GL_IMPORT("begin_query") void gasm_gl_begin_query(uint32_t target, uint32_t query);
GASM_GL_IMPORT("end_query") void gasm_gl_end_query(uint32_t target);
/* GL_CURRENT_QUERY: the active query's name. */
GASM_GL_IMPORT("get_queryiv") uint32_t gasm_gl_get_queryiv(uint32_t target, uint32_t pname);
/* Results are available from the next frame on, on every runner. */
GASM_GL_IMPORT("get_query_objectuiv") uint32_t gasm_gl_get_query_objectuiv(uint32_t query, uint32_t pname);
/* A sync name (not a pointer). */
GASM_GL_IMPORT("fence_sync") uint32_t gasm_gl_fence_sync(uint32_t condition, uint32_t flags);
GASM_GL_IMPORT("is_sync") uint32_t gasm_gl_is_sync(uint32_t sync);
GASM_GL_IMPORT("delete_sync") void gasm_gl_delete_sync(uint32_t sync);
/* Signaled from the next frame on; a timeout above 0 that isn't met gives
 * GL_TIMEOUT_EXPIRED. */
GASM_GL_IMPORT("client_wait_sync") uint32_t gasm_gl_client_wait_sync(uint32_t sync, uint32_t flags, uint64_t timeout);
GASM_GL_IMPORT("wait_sync") void gasm_gl_wait_sync(uint32_t sync, uint32_t flags, uint64_t timeout);
GASM_GL_IMPORT("get_synciv") int32_t gasm_gl_get_synciv(uint32_t sync, uint32_t pname);
GASM_GL_IMPORT("create_transform_feedback") uint32_t gasm_gl_create_transform_feedback(void);
GASM_GL_IMPORT("delete_transform_feedback") void gasm_gl_delete_transform_feedback(uint32_t tf);
GASM_GL_IMPORT("is_transform_feedback") uint32_t gasm_gl_is_transform_feedback(uint32_t tf);
GASM_GL_IMPORT("bind_transform_feedback") void gasm_gl_bind_transform_feedback(uint32_t target, uint32_t tf);
GASM_GL_IMPORT("begin_transform_feedback") void gasm_gl_begin_transform_feedback(uint32_t primitive_mode);
GASM_GL_IMPORT("end_transform_feedback") void gasm_gl_end_transform_feedback(void);
GASM_GL_IMPORT("pause_transform_feedback") void gasm_gl_pause_transform_feedback(void);
GASM_GL_IMPORT("resume_transform_feedback") void gasm_gl_resume_transform_feedback(void);

/* ---- gasm:net (optional) ---------------------------------------------------------- */
/* Message connections with WebSocket semantics (reliable, ordered, binary),
 * non-blocking. Runners may deny connections (native: --allow-net). A handle
 * that open never returned traps; a closed one reports GASM_NET_CLOSED. */

/* Open a ws:// or wss:// URL. Handle > 0, or -1 if denied/invalid. */
GASM_NET_IMPORT("open") int32_t gasm_net_open(const char *url, uint32_t url_len);
/* GASM_NET_CONNECTING / OPEN / CLOSED / ERROR. */
GASM_NET_IMPORT("state") uint32_t gasm_net_state(int32_t conn);
/* Send one message (len > 0). 0, or -1 if not open. */
GASM_NET_IMPORT("send") int32_t gasm_net_send(int32_t conn, const void *data, uint32_t len);
/* Next message's length (copied only if <= cap, else it stays queued), 0 if
 * none, -1 if closed and drained. */
GASM_NET_IMPORT("recv") int32_t gasm_net_recv(int32_t conn, void *dst, uint32_t cap);
/* Close (flushing queued messages); closing again does nothing. */
GASM_NET_IMPORT("close") void gasm_net_close(int32_t conn);

/* ---- gasm:fetch (optional) -------------------------------------------------------- */
/* HTTP requests made by the runner (TLS included), non-blocking: poll each
 * frame. Runners may deny requests (native: --allow-net, optionally a host
 * list; browsers: the page and CORS). A handle that request never returned
 * traps; a closed one reports GASM_FETCH_FAILED. */

/* Start a request described by JSON
 * {"method":"GET","url":"https://...","headers":{"name":"value"}}, with
 * body_len bytes of body (0: none). Handle > 0, or -1 if denied, invalid or
 * too many are open (16). */
GASM_FETCH_IMPORT("request") int32_t gasm_fetch_request(const char *desc, uint32_t desc_len, const void *body, uint32_t body_len);
/* GASM_FETCH_PENDING / HEADERS (status and headers are in) / DONE (the whole
 * body arrived) / FAILED. */
GASM_FETCH_IMPORT("state") uint32_t gasm_fetch_state(int32_t req);
/* HTTP status of the final response (after redirects), 0 before the headers or
 * after a failure. */
GASM_FETCH_IMPORT("status") int32_t gasm_fetch_status(int32_t req);
/* Response headers as "name: value\n" lines (names lowercase). Length (copied
 * only if <= cap; cap = 0 queries), -1 before GASM_FETCH_HEADERS. */
GASM_FETCH_IMPORT("headers") int32_t gasm_fetch_headers(int32_t req, char *dst, uint32_t cap);
/* Copy up to cap bytes of body that have arrived; returns the count, 0 if none
 * are waiting yet, -1 once the body is done and drained or the request failed. */
GASM_FETCH_IMPORT("read") int32_t gasm_fetch_read(int32_t req, void *dst, uint32_t cap);
/* Cancel if still running and free the handle; closing again does nothing. */
GASM_FETCH_IMPORT("close") void gasm_fetch_close(int32_t req);

/* ---- gasm:storage (optional) ------------------------------------------------------ */
/* Persistent per-game key/value store; the runner chooses the namespace. Keys:
 * 1-128 bytes of [A-Za-z0-9._-]. Values up to 1 MiB, 16 MiB per game. Headless
 * runs start empty. */

/* Value length, or -1 if missing. Copied only if length <= cap. */
GASM_STORAGE_IMPORT("get") int32_t gasm_storage_get(const char *key, uint32_t key_len, void *dst, uint32_t cap);
/* 0, or a GASM_STORAGE_ERR_* code: invalid key, value too large, quota
 * exceeded, I/O error. */
GASM_STORAGE_IMPORT("set") int32_t gasm_storage_set(const char *key, uint32_t key_len, const void *data, uint32_t len);
/* 0 if deleted, -1 if it did not exist. */
GASM_STORAGE_IMPORT("delete") int32_t gasm_storage_delete(const char *key, uint32_t key_len);
/* Number of keys in the namespace. */
GASM_STORAGE_IMPORT("count") uint32_t gasm_storage_count(void);
/* Key index (0 .. count-1, sorted): its length (copied only if <= cap; cap = 0
 * queries), or -1 if out of range. */
GASM_STORAGE_IMPORT("key") int32_t gasm_storage_key(uint32_t index, char *dst, uint32_t cap);

/* ---- gasm:clipboard (optional) ---------------------------------------------------- */
/* Text on the system clipboard. Copying is always allowed; pasting is the
 * player's choice: the text is readable only during the frame that carries the
 * paste key press (Ctrl+V, Cmd+V on macOS), never otherwise. Headless runs
 * have an empty clipboard. Text is UTF-8, up to 1 MiB. */

/* 0: the runner puts it on the clipboard after this frame; -1: refused (over 1
 * MiB, or no clipboard). Invalid UTF-8 traps. */
GASM_CLIPBOARD_IMPORT("set_text") int32_t gasm_clipboard_set_text(const char *text, uint32_t text_len);
/* The pasted text's length in bytes (copied only if <= cap; cap = 0 queries),
 * or -1 outside a paste frame or with nothing to paste. */
GASM_CLIPBOARD_IMPORT("get_text") int32_t gasm_clipboard_get_text(char *dst, uint32_t cap);

/* ---- gasm:files (optional) -------------------------------------------------------- */
/* Files for the player, outside the game: save hands the runner a copy to keep
 * where the player finds it (natively Pictures/<game>/ for images,
 * Downloads/<game>/ otherwise, or --save-dir; a download in browsers). The
 * game never learns the path. Headless runs write nothing unless given
 * --save-dir. */

/* A handle > 0 (the runner saves it after this frame), or -1: refused (saving
 * is off, name empty or over 255 bytes, mime not type/subtype, over 256 MiB,
 * or 16 saves still pending). name is a file name; the runner keeps only its
 * last path component and makes it safe and unique. Invalid UTF-8 traps. */
GASM_FILES_IMPORT("save") int32_t gasm_files_save(const char *name, uint32_t name_len, const char *mime, uint32_t mime_len, const void *data, uint32_t len);
/* GASM_FILES_PENDING (0), GASM_FILES_SAVED (1) or GASM_FILES_FAILED (2:
 * cancelled or not written). A handle save never returned traps. */
GASM_FILES_IMPORT("state") int32_t gasm_files_state(int32_t handle);

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
static inline int32_t gasm_has_str(const char *name) { return gasm_has(name, gasm__strlen(name)); }
/* Show frames at display aspect num:den (0, 0: square pixels). Returns 1 if the
 * runner does; 0 on runners without gasm.video_set_aspect (correct it yourself). */
static inline int gasm_video_aspect(uint32_t num, uint32_t den) {
    static int supported = -1;
    if (supported < 0) supported = gasm_has_str("gasm.video_set_aspect");
    if (supported) gasm_video_set_aspect(num, den);
    return supported;
}
/* The game's built-in title (custom section gasm.title), shown before the game
 * runs and while it hasn't called set_title. Use once, at file scope, with a
 * plain string literal (no quotes or backslashes): GASM_TITLE("My Game"); */
#define GASM_TITLE(text) __asm__(".section .custom_section.gasm.title,\"\",@\n.ascii \"" text "\"\n")

/* Name the window or tab; does nothing on runners without gasm.set_title. */
static inline void gasm_set_title_str(const char *title) {
    static int supported = -1;
    if (supported < 0) supported = gasm_has_str("gasm.set_title");
    if (supported) gasm_set_title(title, gasm__strlen(title));
}
static inline int32_t gasm_asset_size_str(const char *n) { return gasm_asset_size(n, gasm__strlen(n)); }
static inline int64_t gasm_asset_size64_str(const char *n) { return gasm_asset_size64(n, gasm__strlen(n)); }
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
