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
/* Size in bytes of asset name, or -1 if it does not exist. */
GASM_IMPORT("asset_size") int32_t gasm_asset_size(const char *name, uint32_t name_len);
/* Copy up to cap bytes of asset name into dst. Bytes copied, or -1 if missing. */
GASM_IMPORT("asset_read") int32_t gasm_asset_read(const char *name, uint32_t name_len, void *dst, uint32_t cap);
/* Copy up to len bytes of asset name starting at offset (streaming). Bytes
 * copied (0 at the end), or -1 if missing. */
GASM_IMPORT("asset_read_at") int32_t gasm_asset_read_at(const char *name, uint32_t name_len, uint32_t offset, void *dst, uint32_t len);
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
/* GPURenderPipelineDescriptor as JSON (layout auto). */
GASM_GFX_IMPORT("create_pipeline") uint32_t gasm_gfx_create_pipeline(const char *json, uint32_t json_len);
/* 
 * {"pipeline":P,"group":G,"entries":[{"binding":B,"buffer":H,"offset":O,"size":S}]} */
GASM_GFX_IMPORT("create_bind_group") uint32_t gasm_gfx_create_bind_group(const char *json, uint32_t json_len);
/* Queue a write, applied before the frame's draws. offset/len: multiples of 4. */
GASM_GFX_IMPORT("write_buffer") void gasm_gfx_write_buffer(uint32_t buffer, uint32_t offset, const void *data, uint32_t len);
/* Start the frame (clears color and depth). 1 = will be shown, 0 = discarded
 * (the guest may skip draws). */
GASM_GFX_IMPORT("begin_frame") uint32_t gasm_gfx_begin_frame(float r, float g, float b, float a);
GASM_GFX_IMPORT("set_pipeline") void gasm_gfx_set_pipeline(uint32_t pipeline);
GASM_GFX_IMPORT("set_bind_group") void gasm_gfx_set_bind_group(uint32_t index, uint32_t bind_group);
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
static inline int32_t gasm_net_open_str(const char *url) { return gasm_net_open(url, gasm__strlen(url)); }

#endif /* GASM_H */
