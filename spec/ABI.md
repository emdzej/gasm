# gasm ABI v0

Status: **experimental proof of concept**. Bindings: Rust crate
[`guests/gasm`](https://github.com/emdzej/gasm/blob/main/guests/gasm/src/lib.rs),
C header [`spec/gasm.h`](https://github.com/emdzej/gasm/blob/main/spec/gasm.h).

A gasm game is a single WebAssembly module (wasm32, MVP + bulk-memory, sign-ext,
mutable-globals: what rustc and clang emit by default). A runner is any host that
implements the imports below. The ABI is intentionally core wasm (no Component
Model), so it runs unmodified in browsers, wasmtime, WAMR, wasm2c, etc.

It has three import modules:

| Module | Status | Contents |
|---|---|---|
| `gasm` | core | log, time, frame rate, 2D video, audio, input, assets, params |
| `gasm:gfx` | optional | GPU rendering: a WebGPU subset |
| `gasm:net` | optional | message connections (WebSocket semantics) |
| `gasm:storage` | optional | persistent per-game key/value store (saves, settings) |

Games import only what they use. A runner that lacks an optional module can
still run games that don't import it.

## Module shape

| Kind   | Name                                | Required | Notes |
|--------|-------------------------------------|----------|-------|
| export | `memory`                            | yes      | all pointers index into it |
| export | `gasm_abi_version() -> i32`         | yes      | must return `0` |
| export | `gasm_init() -> i32`                | yes      | `0` = ok, anything else aborts |
| export | `gasm_frame()`                      | yes      | one simulation + render step |
| export | `_initialize()`                     | no       | WASI reactor ctor hook; called first if present |
| export | `gasm_exit()`                       | no       | the player is quitting: flush saves (best effort) |
| import | `gasm.*`, `gasm:gfx.*`, `gasm:net.*`, `gasm:storage.*` | — | see below |
| import | `wasi_snapshot_preview1.*`          | —        | libc support subset, see below |

**Unknown imports.** Runners link imports they don't implement as functions
that trap when called. A module therefore always loads, and fails only if it
actually calls something missing. Some toolchains leave unused imports
behind, e.g. wasm-bindgen glue in Rust crates that also target browsers.

## Lifecycle

1. Instantiate. Call `_initialize` if exported.
2. Check `gasm_abi_version() == 0`, otherwise refuse to run.
3. `gasm_init()`. The guest configures frame rate and audio, loads assets and
   params, and creates GPU resources.
4. Call `gasm_frame()` at the rate from `set_frame_rate` (default 60 Hz) on a
   **fixed timestep**, independent of display refresh. Runners may run up to a
   few frames back to back to catch up (rendering only the last, see
   `begin_frame`) and must not call it re-entrantly. Input is sampled before
   each call.
5. The game ends when the user quits, when an export traps, or when the guest
   calls WASI `proc_exit(code)`. Code 0 is a normal exit; runners stop cleanly
   and report the code. When the *user* quits (window closed, Esc, page left,
   headless run finished), runners first call `gasm_exit()` if it's exported.
   This is best effort (a crash or killed process skips it), so games should
   also save periodically.

Runners must trap (not crash) on out-of-bounds pointers or invalid handles.

## `gasm` imports

All pointers are `i32` offsets into guest memory. Strings are UTF-8 `(ptr, len)`.

| Import | Signature | Semantics |
|--------|-----------|-----------|
| `log` | `(ptr, len)` | Log a line. |
| `time_ms` | `() -> f64` | Monotonic ms. Headless/deterministic runs return `frame_index * 1000 / frame_rate`. |
| `set_frame_rate` | `(hz: f64)` | 1–1000 Hz, otherwise ignored. |
| `video_present` | `(ptr, w, h, stride)` | RGBA8 pixels (byte order R,G,B,A), `stride` bytes per row, `w,h ≤ 4096`. The data is copied before the call returns. The runner letterboxes it into its output. Ignored for display when the guest renders with `gasm:gfx` in the same frame. |
| `audio_config` | `(rate, channels)` | Format for `audio_push`: 8–192 kHz, 1 or 2 channels. Default 44100/2. |
| `audio_push` | `(ptr, frames)` | `frames × channels` interleaved `f32` in [-1, 1]. The runner resamples and buffers (~60 ms target latency). It drops the oldest audio if the guest runs ahead and plays silence on underrun. |
| `input_pad` | `(player) -> u32` | Bitmask of buttons for virtual pad 0–3, stable within one `gasm_frame`. |
| `asset_size` | `(name_ptr, name_len) -> i32` | Byte size, or `-1` if missing. |
| `asset_read` | `(name_ptr, name_len, dst, cap) -> i32` | Copy ≤ `cap` bytes, return count or `-1`. |
| `asset_read_at` | `(name_ptr, name_len, offset, dst, len) -> i32` | Copy up to `len` bytes starting at byte `offset` (streaming large assets). Returns bytes copied (0 at or after the end), or `-1` if missing. |
| `param` | `(name_ptr, name_len, dst, cap) -> i32` | Launch parameter value: returns its byte length, or `-1` if unset. Copied only if length ≤ `cap`; call with `cap = 0` to query the length. |

Button bits: `A=0 B=1 X=2 Y=3 L=4 R=5 SELECT=6 START=7 UP=8 DOWN=9 LEFT=10 RIGHT=11`.
Physical mapping (face buttons by position, as on a SNES pad): East=A,
South=B, North=X, West=Y.

Assets are a flat, read-only name→bytes map chosen by whoever launches the
game (CLI `--asset name=path`, a file picker, a package, …). By convention
`rom` is the content file for emulator guests.

Params are a flat name→string map: CLI `--param name=value`, or URL query
parameters in the browser runner. Use them for things like a relay URL or a
difficulty setting, not for secrets.

## `gasm:gfx` (optional): GPU rendering

A deliberately small subset of WebGPU. The browser runner forwards it to
`navigator.gpu`; the native runner implements it with wgpu (Metal, Vulkan,
D3D12). Shaders are **WGSL**.

Objects are `u32` handles; `0` is never valid. Resources are created once
through JSON descriptors that mirror WebGPU dictionaries (handles as numbers).
Per-frame calls take only scalars.

| Import | Signature | Semantics |
|---|---|---|
| `width` / `height` | `() -> u32` | Current drawable size in pixels (changes on resize). |
| `create_shader` | `(ptr, len) -> u32` | WGSL source. |
| `create_buffer` | `(size, usage) -> u32` | `size` a non-zero multiple of 4; `usage` = WebGPU `GPUBufferUsage` bits (`COPY_DST` 0x08 is always added; `INDEX` 0x10, `VERTEX` 0x20, `UNIFORM` 0x40). |
| `write_buffer` | `(buf, offset, ptr, len)` | Queue a write; `offset`, `len` multiples of 4. All writes made before `end_frame` land before that frame's draws, so give each object its own buffer region. |
| `create_pipeline` | `(ptr, len) -> u32` | `GPURenderPipelineDescriptor` JSON, see below. |
| `create_bind_group` | `(ptr, len) -> u32` | `{"pipeline":P,"group":G,"entries":[{"binding":B,"buffer":H,"offset":O,"size":S}]}`. Uses the pipeline's automatic layout for group `G`. |
| `begin_frame` | `(r, g, b, a: f32) -> u32` | Start the frame's render pass, clearing color and depth. Returns `1` if the frame will be shown, `0` if the runner will discard it (catch-up frame, headless); the guest may then skip its draw calls. |
| `set_pipeline` | `(p)` | |
| `set_bind_group` | `(index, bg)` | |
| `set_vertex_buffer` | `(slot, buf, offset)` | |
| `set_index_buffer` | `(buf, format, offset)` | `format`: 0 = uint16, 1 = uint32. |
| `draw` | `(vertex_count, instance_count, first_vertex, first_instance)` | |
| `draw_indexed` | `(index_count, instance_count, first_index, base_vertex: i32, first_instance)` | |
| `end_frame` | `()` | Submit and present. |

Draw and set calls outside `begin_frame`/`end_frame`, or after `begin_frame`
returned 0, are validated and then ignored.

**Pipeline descriptor.** It is WebGPU's, with these rules:

- `layout` is always `"auto"` (omit it). Automatic layouts belong to one
  pipeline: create a bind group per pipeline, even if two pipelines declare
  the same bindings.
- Color targets use `"format": "surface"` (the runner's swapchain format).
- `depthStencil.format` must be `"depth24plus"` (the runner owns the depth buffer).
  Omitting `depthStencil` is fine: the runner then adds one that neither tests
  nor writes depth.
- Don't set `multisample`: the runner picks it (currently 4× MSAA on both runners).
- Supported: `vertex {module, entryPoint, buffers[{arrayStride, stepMode, attributes[{format, offset, shaderLocation}]}]}`,
  `fragment {module, entryPoint, targets[{format, blend{color,alpha}{srcFactor,dstFactor,operation}, writeMask}]}`,
  `primitive {topology, cullMode, frontFace, stripIndexFormat}`,
  `depthStencil {format, depthWriteEnabled, depthCompare}`.
  Common vertex formats (`float32…x4`, `uint32…x4`, `sint32…x4`, `unorm8x4`, `uint8x4`, `uint16x2/x4`, `float16x2/x4`).

Invalid descriptors and WGSL errors trap the guest with the validation message.
Textures, samplers, storage buffers and compute are **not in v0**.

Hashing: headless runners fold every `write_buffer` payload into the video
hash. Uniforms carry the scene state, so deterministic GPU games get checked
too, without comparing pixels (GPU output isn't bit-exact across vendors).

## `gasm:net` (optional): message connections

WebSocket semantics on every runner, because browsers cannot open raw sockets:
reliable, ordered, binary messages. Everything is non-blocking; poll once per
frame.

| Import | Signature | Semantics |
|---|---|---|
| `open` | `(url_ptr, len) -> i32` | Open `ws://` (native and web) or `wss://` (web). Handle > 0, or `-1` if denied or invalid. |
| `state` | `(conn) -> u32` | `0` connecting, `1` open, `2` closed, `3` error. |
| `send` | `(conn, ptr, len) -> i32` | One message (`len > 0`). `0` ok, `-1` not open. |
| `recv` | `(conn, dst, cap) -> i32` | Next message's length (copied only if ≤ `cap`; otherwise it stays queued), `0` if none waiting, `-1` if closed or failed and drained. |
| `close` | `(conn)` | |

**Permission.** Networking is off by default natively (`gasm-run --allow-net`)
and on in the browser runner, where the browser's own rules apply.

**Determinism.** Messages arrive at unpredictable times. Deterministic games
must only let *message contents* affect the simulation, never arrival
timing: lockstep games apply inputs at the frame number carried in the
message. See `guests/sumo` and `gasm-relay`.

## `gasm:storage` (optional): persistent key/value store

For saves, settings and high scores. Each game gets its own **namespace, chosen
by the runner**, never by the game, so one game can't read another's saves.
By default the namespace is the game file's name (`sumo.wasm` → `sumo`).

| Import | Signature | Semantics |
|---|---|---|
| `get` | `(key_ptr, key_len, dst, cap) -> i32` | Value length, or `-1` if the key doesn't exist. Copied only if length ≤ `cap` (call with `cap = 0` to query the size). |
| `set` | `(key_ptr, key_len, data_ptr, data_len) -> i32` | Store a value: `0`, or `-1` on an invalid key, size or quota violation, or I/O error. |
| `delete` | `(key_ptr, key_len) -> i32` | `0` if deleted, `-1` if it didn't exist. |

Limits: keys are 1–128 bytes of `[A-Za-z0-9._-]` (not `.` or `..`), values up
to 1 MiB, total 16 MiB per namespace. Durability: native writes are atomic and
complete when `set` returns; the browser persists to IndexedDB in the
background right after `set`. **Headless runs start with an empty in-memory
store** (unless given a directory), so runs are reproducible.

Where data lives: native: `<data dir>/gasm/<namespace>/<key>`, one file per key
(macOS `~/Library/Application Support`, Linux `$XDG_DATA_HOME` or
`~/.local/share`, Windows `%APPDATA%`), override with `--storage-dir`. Browser:
IndexedDB database `gasm`, keys `<namespace>/<key>`, per site origin.

## WASI subset

Guests may import `wasi_snapshot_preview1` (C with wasi-libc does; Rust on
`wasm32-unknown-unknown` only needs `proc_exit` via `gasm::exit`). Runners
guarantee: `fd_write` (fd 1/2 → log), `fd_close`, `fd_seek` (ESPIPE),
`fd_fdstat_get` (fds 0–2), `clock_time_get`, `random_get`, `args_*`,
`environ_*` (empty), `proc_exit`. Other WASI functions may exist but may
return `ENOSYS` (52). There is **no filesystem**; use assets.

## Determinism

Given the same module, assets, params and per-frame input, a guest that only
uses `gasm.*` (no WASI clocks or random) must produce bit-identical
`video_present`/`audio_push`/`write_buffer` streams on every runner. The
runners verify this with FNV-1a-32 hashes (`make test`). One caveat: wasm NaN
bit patterns are nondeterministic by spec. Guests that hash or store NaN
payloads can diverge.

## Roadmap (not in v0)

- `.gasm` packages: one file bundling `game.wasm`, assets and a manifest.
- `gasm:files`: a runner-provided file picker (the game only sees what the player picks).
- `gasm:gfx` v1: textures and samplers, storage buffers, instancing examples,
  render bundles.
- Capabilities manifest (custom section `gasm.manifest`) that declares required
  and optional imports, network hosts, and platform extensions (`gasm:ext/*`).
- Runner-level rollback netplay (snapshot/restore guest memory).
- Move to WIT/Component Model once browser support doesn't need transpiling.
