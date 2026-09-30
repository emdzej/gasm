/*
 * gasm.h — gasm ABI v0 (guest side)
 *
 * A gasm game is a WebAssembly module (wasm32) that:
 *   - imports the functions below from the "gasm" module,
 *   - may import WASI preview1 ("wasi_snapshot_preview1") for libc basics
 *     (runners provide a minimal subset: fd_write to stdout/stderr, clocks,
 *     random, args/environ stubs, proc_exit),
 *   - exports `memory`, `gasm_abi_version`, `gasm_init` and `gasm_frame`.
 *
 * Lifecycle (driven by the runner):
 *   1. instantiate; call `_initialize` if exported (WASI reactor / C++ ctors)
 *   2. check gasm_abi_version() == GASM_ABI_VERSION
 *   3. gasm_init()      -> 0 on success, non-zero aborts
 *   4. gasm_frame()     called at the rate set by gasm_set_frame_rate()
 *                       (default 60 Hz, fixed timestep, independent of vsync)
 *
 * All pointers are offsets into the guest's exported linear memory.
 * All strings are UTF-8, (ptr, len), not NUL-terminated.
 *
 * See spec/ABI.md for the normative description.
 */
#ifndef GASM_H
#define GASM_H

#include <stdint.h>

#define GASM_ABI_VERSION 0

#ifdef __wasm__
#define GASM_IMPORT(name) __attribute__((import_module("gasm"), import_name(name)))
#define GASM_GFX_IMPORT(name) __attribute__((import_module("gasm:gfx"), import_name(name)))
#define GASM_NET_IMPORT(name) __attribute__((import_module("gasm:net"), import_name(name)))
#define GASM_EXPORT(name) __attribute__((export_name(name)))
#else
#define GASM_IMPORT(name)
#define GASM_GFX_IMPORT(name)
#define GASM_NET_IMPORT(name)
#define GASM_EXPORT(name)
#endif

#ifdef __cplusplus
extern "C" {
#endif

/* ---- core --------------------------------------------------------------- */

/* Write a line to the runner's log. */
GASM_IMPORT("log") void gasm_log(const char *msg, uint32_t len);

/* Monotonic time in milliseconds since an arbitrary epoch. */
GASM_IMPORT("time_ms") double gasm_time_ms(void);

/* Rate (Hz) at which the runner calls gasm_frame(). Default 60. */
GASM_IMPORT("set_frame_rate") void gasm_set_frame_rate(double hz);

/* ---- video (software framebuffer) --------------------------------------- */

/* Present a frame. Pixels are RGBA8 (bytes R,G,B,A in memory order),
 * `stride` is the distance in bytes between rows. The runner copies the data
 * before returning; the guest may reuse the buffer immediately.
 * The runner scales the image to its window preserving aspect ratio. */
GASM_IMPORT("video_present")
void gasm_video_present(const void *rgba, uint32_t width, uint32_t height, uint32_t stride);

/* ---- audio -------------------------------------------------------------- */

/* Declare the format of samples passed to gasm_audio_push().
 * channels: 1 or 2. The runner resamples to the device rate. */
GASM_IMPORT("audio_config") void gasm_audio_config(uint32_t sample_rate, uint32_t channels);

/* Queue `frames` audio frames of interleaved float32 samples in [-1, 1]. */
GASM_IMPORT("audio_push") void gasm_audio_push(const float *samples, uint32_t frames);

/* ---- input -------------------------------------------------------------- */

enum {
    GASM_BTN_A      = 1u << 0,
    GASM_BTN_B      = 1u << 1,
    GASM_BTN_X      = 1u << 2,
    GASM_BTN_Y      = 1u << 3,
    GASM_BTN_L      = 1u << 4,
    GASM_BTN_R      = 1u << 5,
    GASM_BTN_SELECT = 1u << 6,
    GASM_BTN_START  = 1u << 7,
    GASM_BTN_UP     = 1u << 8,
    GASM_BTN_DOWN   = 1u << 9,
    GASM_BTN_LEFT   = 1u << 10,
    GASM_BTN_RIGHT  = 1u << 11,
};

/* Bitmask of GASM_BTN_* currently held on virtual gamepad `player` (0..3).
 * State is sampled once per gasm_frame() and stable within it. */
GASM_IMPORT("input_pad") uint32_t gasm_input_pad(uint32_t player);

/* ---- assets (read-only) ------------------------------------------------- */

/* Size in bytes of asset `name`, or -1 if it does not exist. */
GASM_IMPORT("asset_size") int32_t gasm_asset_size(const char *name, uint32_t name_len);

/* Copy up to `cap` bytes of asset `name` into `dst`.
 * Returns bytes copied, or -1 if it does not exist. */
GASM_IMPORT("asset_read")
int32_t gasm_asset_read(const char *name, uint32_t name_len, void *dst, uint32_t cap);

/* ---- launch parameters ---------------------------------------------------- */

/* Launch parameter `name` (CLI `--param name=value`, URL query `?name=value`).
 * Returns the value's length in bytes, or -1 if unset. The value is copied only
 * if it fits (length <= cap); call with cap = 0 to query the length. */
GASM_IMPORT("param")
int32_t gasm_param(const char *name, uint32_t name_len, char *dst, uint32_t cap);

/* ---- gasm:gfx — GPU rendering (optional; a WebGPU subset) -------------------
 *
 * Objects are u32 handles (0 is never valid). Creation calls take JSON
 * descriptors that mirror the WebGPU dictionaries, with handles as numbers and
 * two runner-owned values:
 *   - color target "format": "surface"     (the runner's swapchain format)
 *   - depthStencil "format": "depth24plus" (the runner-owned depth buffer)
 * The runner picks MSAA; do not set "multisample". Invalid descriptors trap.
 * See spec/ABI.md#gasmgfx for the supported fields. */

enum {
    GASM_BUF_COPY_DST = 0x08,
    GASM_BUF_INDEX    = 0x10,
    GASM_BUF_VERTEX   = 0x20,
    GASM_BUF_UNIFORM  = 0x40,
};
enum { GASM_INDEX_U16 = 0, GASM_INDEX_U32 = 1 };

/* Current drawable size in pixels (changes when the window is resized). */
GASM_GFX_IMPORT("width")  uint32_t gasm_gfx_width(void);
GASM_GFX_IMPORT("height") uint32_t gasm_gfx_height(void);

GASM_GFX_IMPORT("create_shader")     uint32_t gasm_gfx_create_shader(const char *wgsl, uint32_t len);
GASM_GFX_IMPORT("create_buffer")     uint32_t gasm_gfx_create_buffer(uint32_t size, uint32_t usage);
GASM_GFX_IMPORT("create_pipeline")   uint32_t gasm_gfx_create_pipeline(const char *json, uint32_t len);
GASM_GFX_IMPORT("create_bind_group") uint32_t gasm_gfx_create_bind_group(const char *json, uint32_t len);

/* Queue a write; applied before the next submitted frame. offset/len: multiples of 4. */
GASM_GFX_IMPORT("write_buffer")
void gasm_gfx_write_buffer(uint32_t buffer, uint32_t offset, const void *data, uint32_t len);

/* Start a frame: clears color and depth. Returns 1 if the frame will be shown,
 * 0 if the runner will discard it (catch-up frame, headless); the guest may
 * then skip its draw calls. Draw calls outside begin/end are ignored. */
GASM_GFX_IMPORT("begin_frame") uint32_t gasm_gfx_begin_frame(float r, float g, float b, float a);
GASM_GFX_IMPORT("set_pipeline")      void gasm_gfx_set_pipeline(uint32_t pipeline);
GASM_GFX_IMPORT("set_bind_group")    void gasm_gfx_set_bind_group(uint32_t index, uint32_t bind_group);
GASM_GFX_IMPORT("set_vertex_buffer") void gasm_gfx_set_vertex_buffer(uint32_t slot, uint32_t buffer, uint32_t offset);
GASM_GFX_IMPORT("set_index_buffer")  void gasm_gfx_set_index_buffer(uint32_t buffer, uint32_t format, uint32_t offset);
GASM_GFX_IMPORT("draw")
void gasm_gfx_draw(uint32_t vertex_count, uint32_t instance_count, uint32_t first_vertex, uint32_t first_instance);
GASM_GFX_IMPORT("draw_indexed")
void gasm_gfx_draw_indexed(uint32_t index_count, uint32_t instance_count, uint32_t first_index,
                           int32_t base_vertex, uint32_t first_instance);
/* Submit and present. */
GASM_GFX_IMPORT("end_frame") void gasm_gfx_end_frame(void);

/* ---- gasm:net — message connections (optional) ------------------------------
 *
 * WebSocket semantics on every runner (browsers cannot open raw sockets):
 * reliable, ordered, binary messages. Everything is non-blocking; poll once
 * per frame. Runners may deny connections (native: needs --allow-net). */

enum { GASM_NET_CONNECTING = 0, GASM_NET_OPEN = 1, GASM_NET_CLOSED = 2, GASM_NET_ERROR = 3 };

/* Open a connection to a ws:// or wss:// URL. Returns a handle > 0, or -1 if denied/invalid. */
GASM_NET_IMPORT("open")  int32_t  gasm_net_open(const char *url, uint32_t len);
GASM_NET_IMPORT("state") uint32_t gasm_net_state(int32_t conn);
/* Send one message (len > 0). Returns 0, or -1 if the connection is not open. */
GASM_NET_IMPORT("send")  int32_t  gasm_net_send(int32_t conn, const void *data, uint32_t len);
/* Receive the next message. Returns its length (copied only if <= cap, otherwise
 * it stays queued), 0 if none is waiting, -1 if closed/failed and drained. */
GASM_NET_IMPORT("recv")  int32_t  gasm_net_recv(int32_t conn, void *dst, uint32_t cap);
GASM_NET_IMPORT("close") void     gasm_net_close(int32_t conn);

#ifdef __cplusplus
}
#endif

/* ---- convenience ---------------------------------------------------------- */

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
