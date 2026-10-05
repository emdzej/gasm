# gasm ABI v0

Status: **experimental proof of concept**. Bindings: Rust crate
[`guests/gasm`](https://github.com/emdzej/gasm/blob/main/guests/gasm/src/lib.rs),
C header [`spec/gasm.h`](https://github.com/emdzej/gasm/blob/main/spec/gasm.h).

A gasm game is a single WebAssembly module: wasm32 with the features current rustc
and clang emit by default (bulk-memory, sign-ext, mutable-globals,
nontrapping-float-to-int, multivalue, reference-types; clang also extended-const).
No threads, SIMD or exception handling. A runner is any host that implements the
imports below. The ABI is intentionally core wasm (no Component Model), so it runs
unmodified in browsers, wasmtime, WAMR, wasm2c, etc.

It has six import modules:

| Module | Status | Contents |
|---|---|---|
| `gasm` | core | log, time, frame rate, 2D video, audio, input (pads, text, raw keyboard, pointer, gamepads), assets, params |
| `gasm:gfx` | optional | GPU rendering: a WebGPU subset |
| `gasm:gl` | optional | GPU rendering: OpenGL ES 3.0 with WebGL 2's rules (a game uses `gasm:gfx` or `gasm:gl`) |
| `gasm:net` | optional | message connections (WebSocket semantics) |
| `gasm:fetch` | optional | HTTP requests made by the runner (TLS included) |
| `gasm:storage` | optional | persistent per-game key/value store (saves, settings) |

Games import only what they use. A runner that lacks an optional module can
still run games that don't import it.

**Versioning.** `GASM_ABI_VERSION` changes only for breaking changes.
Additions (new imports, new descriptor fields) keep it: runners link unknown
imports as traps, so an older runner still loads a newer guest and fails only
if the guest calls something it lacks, and an older guest never calls the
new imports. A guest that can do without a newer import asks first with
`has("module.function")`. Textures, samplers, explicit layouts, viewport/scissor,
text input, raw keyboard/pointer/gamepads, asset and storage enumeration, `has`,
64-bit assets, `gfx.destroy`, `set_title`, `video_set_aspect` and `gasm_run`
with `yield_frame` were added this way; the version is still 0.
[CHANGELOG.md](https://github.com/emdzej/gasm/blob/main/CHANGELOG.md) lists what
each release added.

## Module shape

| Kind   | Name                                | Required | Notes |
|--------|-------------------------------------|----------|-------|
| export | `memory`                            | yes      | all pointers index into it |
| export | `gasm_abi_version() -> i32`         | yes      | must return `0` |
| export | `gasm_init() -> i32`                | yes      | `0` = ok, anything else aborts |
| export | `gasm_frame()`                      | yes      | one simulation + render step |
| export | `gasm_run() -> i32`                 | no       | a game with its own loop, run by runners that switch stacks; see [Stack switching](#stack-switching) |
| export | `_initialize()`                     | no       | WASI reactor ctor hook; called first if present |
| export | `gasm_exit()`                       | no       | the player is quitting: flush saves (best effort) |
| import | `gasm.*`, `gasm:gfx.*` or `gasm:gl.*`, `gasm:net.*`, `gasm:storage.*` | — | see below |
| import | `wasi_snapshot_preview1.*`          | —        | libc support subset, see below |
| custom section | `gasm.title`                 | no       | the game's name (UTF-8), the default title before `set_title`; see [Window title](#window-title) |

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
   each call. (Guests that export `gasm_run` may be run through it instead,
   with the same frames: [Stack switching](#stack-switching).)
5. The game ends when the user quits, when an export traps, or when the guest
   calls WASI `proc_exit(code)`. Code 0 is a normal exit; runners stop cleanly
   and report the code. When the *user* quits (window closed, Esc held, page left,
   headless run finished), runners first call `gasm_exit()` if it's exported.
   This is best effort (a crash or killed process skips it), so games should
   also save periodically.

Runners must trap (not crash) on out-of-bounds pointers, invalid handles and
string arguments that aren't UTF-8.

**Conventions.**

- **Returning data of variable size** (`text_input`, `param`, `asset_name`,
  `gamepad_name`, `key_events`, `storage.get`, `storage.key`, `net.recv`): the
  call returns the full length and copies only if it fits in `cap`, so call once
  with `cap = 0` (or a guess), then again with a buffer that size. Fixed-size
  records (`pointer`, `gamepad`) are copied if `cap` is large enough;
  `key_state` copies `min(len, 32)`. Streaming reads (`asset_read`,
  `asset_read_at`) copy as much as fits and return that count.
- **Errors:** `-1` means "not there" (missing asset, key, parameter or device,
  index out of range). `storage.set` returns a `GASM_STORAGE_ERR_*` code.
  Misuse traps: bad pointers, handles that were never valid or were destroyed,
  invalid descriptors, arguments out of range where the ABI says so.
- **Handles:** gfx handles are `u32` numbered from 1 in creation order and never
  reused; net handles are positive `i32`s from `open`. Both trap when they were
  never issued; a closed connection keeps answering `CLOSED`.

### Why the runner drives the frames

Desktop engines own their main loop; a gasm guest gets called once per frame
instead. That is deliberate:

- **Browsers require it.** A page can't block: frames come from
  `requestAnimationFrame`, and a guest that never returns freezes the tab. A
  callback per frame works on every runner without stack switching (JSPI is
  not available everywhere yet).
- **Determinism and tests.** Input is sampled *between* calls, so a frame is a
  pure step: `--headless N` runs exactly N of them on virtual time, and
  scripted input lands on exact frame numbers. Lockstep netplay needs the same
  boundaries on both peers.
- **The runner keeps control.** Catch-up after a stall, pausing, Esc-to-quit,
  Worker mode and (later) rollback snapshots all happen between calls, where
  the guest's state is entirely in linear memory.

Games that keep their own loop still work: the SDKs' **loop helpers** (C/C++
`gasm_loop.h`, Rust `gasm::main_loop!`) export these entry points, run the
game's `main` on the first frame and suspend it in `wait_frame()`: with
Binaryen's Asyncify inside the module (an ordinary v0 guest to every runner), or,
on runners that switch stacks, through `gasm_run` and the `yield_frame` import
(see below). ScummVM and SDL 3 classic `main()` games run this way.

### Stack switching

A guest that exports **`gasm_run() -> i32`** keeps its whole run in one call.
Runners that can suspend a wasm stack (wasmtime's async calls natively, JSPI in
browsers and Node 24+) call it on the first frame, after `gasm_init`, instead
of `gasm_frame`; the guest ends each frame by calling **`gasm.yield_frame()`**,
and the runner resumes it at the start of the next one. Returning from
`gasm_run` ends the game with that exit code.

- Frames are exactly those of the `gasm_frame` model (frame 0 starts
  `gasm_run`): input, time, catch-up, `begin_frame` and hashing are the same,
  so a game gives the same hashes either way.
- `gasm_exit` (the player quits) is called while `gasm_run` is suspended; the
  run then ends. The call watchdog applies to each frame, not the whole run.
- `yield_frame` anywhere else (in `gasm_init`, `gasm_frame`, `gasm_exit`) traps.
- Every module still exports `gasm_frame`, for runners that can't switch
  stacks. A **run build** (made without the Asyncify pass: smaller and
  faster) can't suspend there: it exports `gasm_run` and still imports
  `asyncify.*`, and such runners refuse it with a message. The SDKs build both
  from the same link (`game.wasm` with Asyncify, `game-run.wasm` without).

## `gasm` imports

All pointers are `i32` offsets into guest memory. Strings are UTF-8 `(ptr, len)`.

| Import | Signature | Semantics |
|--------|-----------|-----------|
| `log` | `(ptr, len)` | Log a line. |
| `has` | `(name_ptr, name_len) -> i32` | `1` if the runner provides an import module (`"gasm:gfx"`) or a function in one (`"gasm.asset_size64"`, `"gasm:gfx.destroy"`, `"wasi_snapshot_preview1.random_get"`), else `0`. Probe optional features before calling them: a missing import traps. |
| `time_ms` | `() -> f64` | Monotonic ms. Headless runs use virtual time: the start of the frame on a clock that advances `1000 / frame_rate` per frame, at the rate in effect at that frame's start (so changing the rate never moves time backwards). The value is fixed for the whole frame. |
| `utc_offset_minutes` | `() -> i32` | The player's time zone now: minutes east of UTC, daylight saving included (`120` for CEST, `-300` for EST, `330` for India). Local time is the WASI realtime clock plus this. Headless runs answer `0` (UTC, like their clocks). Probe `has("gasm.utc_offset_minutes")` first (the SDK wrapper `gasm::utc_offset_minutes` does and answers 0 without it). |
| `set_frame_rate` | `(hz: f64)` | 1–1000 Hz, otherwise ignored. |
| `video_present` | `(ptr, w, h, stride)` | RGBA8 pixels (byte order R,G,B,A), `stride` bytes per row, `w,h ≤ 4096`. The data is copied before the call returns. The runner letterboxes it into its output. Ignored for display when the guest renders with `gasm:gfx` in the same frame. |
| `video_set_aspect` | `(num, den)` | Show later `video_present` frames at display aspect `num:den` (4:3 for a 320×200 game made for a CRT) instead of square pixels; `0, 0` resets. Otherwise both must be 1–65535 with 1/8 ≤ num/den ≤ 8, or it traps. Display and the pointer's frame position only: not hashed. Probe `has("gasm.video_set_aspect")` first (the SDK wrappers do). |
| `audio_config` | `(rate, channels)` | Format for `audio_push`: 8–192 kHz, 1 or 2 channels. Default 44100/2. |
| `audio_push` | `(ptr, frames)` | `frames × channels` interleaved `f32` in [-1, 1]. The runner resamples and buffers (~60 ms target latency). It drops the oldest audio if the guest runs ahead and plays silence on underrun. |
| `input_pad` | `(player) -> u32` | Bitmask of buttons for virtual pad 0–3, stable within one `gasm_frame`. |
| `input_mode` | `(flags)` | `GASM_INPUT_*` flags, see [Raw input](#raw-input). |
| `key_state` | `(dst, len) -> i32` | Held keys as a bitset indexed by `GASM_KEY_*` (bit `k % 8` of byte `k / 8`), stable within a frame. Copies min(`len`, 32) bytes; returns `GASM_KEY_STATE_BYTES` (32), or `-1` if the runner has no keyboard. |
| `key_events` | `(dst, cap) -> i32` | Presses and releases since the previous frame, in order, 4 bytes each: `u16` key code, `u8` 1 = down / 0 = up, `u8` 0. No auto-repeat. Returns the byte length (copied only if ≤ `cap`; `cap = 0` queries), or `-1` without a keyboard. |
| `pointer` | `(dst, cap) -> i32` | Mouse/touch state as 48 bytes, see [Raw input](#raw-input). Copied only if `cap` ≥ 48; returns 48, or `-1` if the runner has no pointer. |
| `gamepad` | `(slot, dst, cap) -> i32` | Gamepad or joystick in slot 0–3 as 204 bytes, see [Raw input](#raw-input). Copied only if `cap` ≥ 204; returns 204, or `-1` if `slot` > 3 or the runner has no gamepad support. |
| `gamepad_name` | `(slot, dst, cap) -> i32` | Device name: its length (copied only if ≤ `cap`), or `-1` if the slot is empty. |
| `text_input` | `(dst, cap) -> i32` | UTF-8 text typed since the previous frame, stable within one `gasm_frame`: backspace is `\b` (0x08), enter is `\n`. Returns its length (copied only if length ≤ `cap`; `cap = 0` queries), or `-1` if the runner has no keyboard. Keys bound to pads still produce text; the guest decides what it wants. Headless: from the `--input` script (`FRAME:"text"`). |
| `asset_size` | `(name_ptr, name_len) -> i32` | Byte size, `-1` if missing, or `-2` if it is 2 GiB or larger (use `asset_size64`). |
| `asset_size64` | `(name_ptr, name_len) -> i64` | Byte size (any size), or `-1` if missing. |
| `asset_read` | `(name_ptr, name_len, dst, cap) -> i32` | Copy ≤ `cap` bytes from the start, return count or `-1`. |
| `asset_read_at` | `(name_ptr, name_len, offset: u32, dst, len) -> i32` | Copy up to `len` bytes starting at byte `offset` (streaming large assets). Returns bytes copied (0 at or after the end), or `-1` if missing. |
| `asset_read_at64` | `(name_ptr, name_len, offset: u64, dst, len) -> i32` | `asset_read_at` with a 64-bit offset, for assets of 4 GiB and more. |
| `asset_version` | `(name_ptr, name_len) -> i32` | `0` for an asset given at launch, a new, larger number each time the embedder replaces it while the game runs, or `-1` if missing. See [Assets that change](#assets-that-change). Probe `has("gasm.asset_version")` (the SDK wrappers do, and answer 0 without it). |
| `asset_count` | `() -> u32` | Number of assets. |
| `asset_name` | `(index, dst, cap) -> i32` | Name of asset `index` (0 … `asset_count`−1), sorted by UTF-8 bytes; folder entries as named on disk. Returns its length (copied only if length ≤ `cap`; `cap = 0` queries), or `-1` if `index` is out of range. |
| `param` | `(name_ptr, name_len, dst, cap) -> i32` | Launch parameter value: returns its byte length, or `-1` if unset. Copied only if length ≤ `cap`; call with `cap = 0` to query the length. |
| `yield_frame` | `()` | End the frame inside `gasm_run`: the runner suspends the guest until the next frame ([Stack switching](#stack-switching)). Traps outside `gasm_run`. |
| `set_title` | `(ptr, len)` | Name the game's window or browser tab, see [Window title](#window-title). Probe `has("gasm.set_title")` first: the SDK wrappers (`gasm::set_title`, `gasm_set_title_str`) do, and do nothing without it. |

Button bits: `A=0 B=1 X=2 Y=3 L=4 R=5 SELECT=6 START=7 UP=8 DOWN=9 LEFT=10 RIGHT=11`.
Physical mapping (face buttons by position, as on a SNES pad): East=A,
South=B, North=X, West=Y.

Assets are a flat, read-only name→bytes map chosen by whoever launches the
game (CLI `--asset name=path`, a folder, a file picker, OPFS, a package, …). By
convention `rom` is the content file for emulator guests. How runners provide
them (file-backed reads, folders, case-insensitive names) is described in
[Runner behaviour](#runner-behaviour).

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
| `create_buffer` | `(size, usage) -> u32` | `size` a non-zero multiple of 4; `usage` = WebGPU `GPUBufferUsage` bits (`COPY_DST` 0x08 is always added; `INDEX` 0x10, `VERTEX` 0x20, `UNIFORM` 0x40, `STORAGE` 0x80). |
| `write_buffer` | `(buf, offset, ptr, len)` | Queue a write; `offset`, `len` multiples of 4. All writes made before `end_frame` land before that frame's draws, so give each object its own buffer region. |
| `create_texture` | `(ptr, len) -> u32` | `{"size":[w,h],"format":"rgba8unorm","mipLevelCount":n}`. 2D; `w`, `h` 1–8192; `format` `rgba8unorm` (default) or `rgba8unorm-srgb`; `mipLevelCount` 1 (default) up to a full chain. Usage is `TEXTURE_BINDING` \| `COPY_DST`. |
| `write_texture` | `(tex, mip, x, y, w, h, ptr, len)` | Upload a `w×h` RGBA8 region of level `mip` at (`x`, `y`), tightly packed (`len = w*h*4`, 4 bytes per texel, rows top to bottom). Queued like `write_buffer`; valid inside and outside a frame. Runners don't generate mipmaps: upload every level you use. |
| `create_sampler` | `(ptr, len) -> u32` | `GPUSamplerDescriptor` subset: `addressModeU`/`V` (`clamp-to-edge`, `repeat`, `mirror-repeat`), `magFilter`, `minFilter`, `mipmapFilter` (`nearest`, `linear`), `lodMinClamp`, `lodMaxClamp`, `maxAnisotropy` (1–16; above 1 all three filters must be `linear`). Omitted fields take WebGPU defaults. |
| `create_bind_group_layout` | `(ptr, len) -> u32` | `GPUBindGroupLayoutDescriptor` subset, see below. |
| `create_pipeline` | `(ptr, len) -> u32` | `GPURenderPipelineDescriptor` JSON, see below. |
| `create_bind_group` | `(ptr, len) -> u32` | `{"layout":L,"entries":[…]}` (explicit layout) or `{"pipeline":P,"group":G,"entries":[…]}` (that pipeline's automatic layout). Entries: `{"binding":B,"buffer":H,"offset":O,"size":S}`, `{"binding":B,"texture":T}` (all mip levels), `{"binding":B,"sampler":S}`. |
| `begin_frame` | `(r, g, b, a: f32) -> u32` | Start the frame's render pass, clearing color and depth. Viewport and scissor are the whole drawable. Returns `1` if the frame will be shown, `0` if the runner will discard it (catch-up frame, headless); the guest may then skip its draw calls. |
| `set_pipeline` | `(p)` | The pipeline must have one color target (the pass has one). |
| `set_bind_group` | `(index, bg)` | `index` 0–3. A bind group with dynamic offsets needs `set_bind_group_offsets`. |
| `set_bind_group_offsets` | `(index, bg, ptr, count)` | Like `set_bind_group`, with `count` `u32` dynamic offsets read from guest memory: one per dynamic-offset entry of the bind group's layout, in binding order, each a multiple of 256, and the bound range must stay inside the buffer. |
| `set_viewport` | `(x, y, w, h, min_depth, max_depth: f32)` | Viewport in drawable pixels, clamped to the drawable; depth range within 0–1. |
| `set_scissor_rect` | `(x, y, w, h)` | Scissor rectangle in drawable pixels, clamped to the drawable. |
| `set_vertex_buffer` | `(slot, buf, offset)` | `slot` 0–7; the buffer needs `VERTEX` usage; `offset` a multiple of 4, at most the buffer size. |
| `set_index_buffer` | `(buf, format, offset)` | `format`: 0 = uint16, 1 = uint32 (anything else traps); the buffer needs `INDEX` usage; `offset` a multiple of the index size, at most the buffer size. |
| `draw` | `(vertex_count, instance_count, first_vertex, first_instance)` | Needs a pipeline, the bind groups its explicit layout lists, and a vertex buffer in every slot it reads, large enough for the vertices (per-vertex buffers) and instances (per-instance buffers) drawn. |
| `draw_indexed` | `(index_count, instance_count, first_index, base_vertex: i32, first_instance)` | Like `draw` (per-instance buffers checked), plus an index buffer holding `first_index + index_count` indices. |
| `end_frame` | `()` | Submit and present. |
| `destroy` | `(handle)` | Release an object of any kind. The handle becomes invalid (using it, or destroying it again, traps) and is never reused. Objects created from it stay valid: a bind group keeps its buffers and textures, a pipeline its shaders. GPU memory is freed once nothing in a submitted frame uses it. |

Set and draw calls are only valid between `begin_frame` and `end_frame` (also
when `begin_frame` returned 0: they are validated, then nothing is drawn).
Outside a frame they trap. If a viewport or scissor rectangle is empty after
clamping, draws are skipped until it is set again.

**Pipeline descriptor.** It is WebGPU's, with these rules:

- `layout` is `"auto"` (or omitted), or an array of bind group layout
  handles, one per group (`"layout":[L0,L1]`).
  - **Automatic layouts belong to one pipeline:** create their bind groups
    with `"pipeline"`/`"group"`, one set per pipeline, even if two pipelines
    declare the same bindings.
  - **Explicit layouts are shared:** a bind group made with `"layout":L`
    works with every pipeline whose `layout` lists `L` at that index. Use them
    for per-material texture groups and for dynamic offsets.
- Color targets use `"format": "surface"` (the runner's swapchain format).
- `depthStencil.format` must be `"depth24plus"` (the runner owns the depth buffer).
  Omitting `depthStencil` is fine: the runner then adds one that neither tests
  nor writes depth.
- Don't set `multisample`: the runner picks it (currently 4× MSAA on both runners).
- Supported: `vertex {module, entryPoint, buffers[{arrayStride, stepMode, attributes[{format, offset, shaderLocation}]}]}`,
  `fragment {module, entryPoint, targets[{format, blend{color,alpha}{srcFactor,dstFactor,operation}, writeMask}]}`,
  `primitive {topology, cullMode, frontFace, stripIndexFormat}`,
  `depthStencil {format, depthWriteEnabled, depthCompare, depthBias, depthBiasSlopeScale, depthBiasClamp}`.
  Common vertex formats (`float32…x4`, `uint32…x4`, `sint32…x4`, `unorm8x4`, `uint8x4`, `uint16x2/x4`, `float16x2/x4`).

**Bind group layouts.** `{"entries":[{"binding":B,"visibility":V, …}]}` with
`visibility` = `GASM_STAGE_VERTEX` (1) and/or `GASM_STAGE_FRAGMENT` (2), and
exactly one of:

- `"buffer":{"type":"uniform"|"read-only-storage","hasDynamicOffset":bool,"minBindingSize":N}`.
  The buffer bound there needs `UNIFORM` or `STORAGE` usage.
- `"texture":{"sampleType":"float"|"unfilterable-float","viewDimension":"2d"}`
- `"sampler":{"type":"filtering"|"non-filtering"}`

A bind group for an explicit layout must provide every binding with the
matching resource kind.

**Colour.** The surface is a non-sRGB format on both runners (natively
`bgra8unorm`/`rgba8unorm`, in browsers `getPreferredCanvasFormat()`), so
shader output is written as is, without gamma conversion. Sampling an
`rgba8unorm-srgb` texture decodes to linear; `rgba8unorm` does not.

**Limits.** Textures up to 8192×8192 with full mip chains, `maxAnisotropy`
up to 16 (the device may clamp it), dynamic offsets aligned to 256. These are
WebGPU's default limits, so every WebGPU device provides them.

**Validation.** Invalid descriptors, out-of-range mip levels or regions, a
`len` that doesn't match `w*h*4`, handles of the wrong kind (or destroyed),
buffer ranges and usages, misaligned or out-of-bounds dynamic offsets, bind
groups that don't match their layout or pipeline, and draws that read past
their vertex or index buffers all trap the guest with a message. The runners
check these against their own record of every object and of the render pass
(same rules, same order), so they trap identically with the null GPU of
headless runs and on every runner. What only a GPU can check (WGSL errors, a
shader that doesn't match its pipeline or bind groups) traps on the GPU: at the
call natively, at the next gfx call in browsers (WebGPU reports errors
asynchronously). Not in this subset: render targets, cube maps, storage
textures, compute, stencil.

**Hashing.** Headless runners fold every `write_buffer` payload into the video
hash, and for `write_texture` first a header (`tex, mip, x, y, w, h` as
little-endian `u32`s) and then the payload, so the same texels written to a
different place change the hash. Uniforms and textures carry the scene state,
so deterministic GPU games get checked too, without comparing pixels (GPU
output isn't bit-exact across vendors). Handles are numbered 1, 2, 3, … in
creation order on every runner. Draw calls, offsets, viewports and scissors
aren't hashed.

## `gasm:gl` (optional): OpenGL ES 3.0

OpenGL ES 3.0 with WebGL 2's rules, for code written against GLES 3 / WebGL 2.
The browser runner forwards it to a WebGL 2 context; the native runner to
[ANGLE](https://chromium.googlesource.com/angle/angle) (Metal, Direct3D 11 or
Vulkan, SwiftShader without a GPU) in a WebGL compatibility context, the
validation Chrome uses for WebGL 2. Headless runs use a null GL (natively ANGLE
draws too when a screenshot is wanted; the hashes don't change). A module imports `gasm:gfx` or `gasm:gl`, not both
(runners refuse it). The full list (224 functions) is in
[`spec/abi.json`](https://github.com/emdzej/gasm/blob/main/spec/abi.json);
C and C++ games use the drop-in `<GLES3/gl3.h>` of the C SDK instead (below).

**Conventions.** The imports are the WebGL 2 API with C types:

- Object names are `u32`, numbered per kind from 1 in creation order and never
  reused; `0` is "none". `create_buffer()` etc. make one name (`glGen*` loops in
  the C SDK). Uniform locations are `i32`, numbered per program in the order the
  guest asks for them; `-1` is "none" (uniform calls with it are ignored).
- Data is `(ptr, len)` in guest memory. Lengths are exact: an image upload's
  `len` must cover the image under the current unpack state (`pixel_storei`),
  `read_pixels`' `len` the rectangle under the pack state. With a pixel unpack
  (pack) buffer bound, `len` (`dst`) is the offset into it.
- Queries for one value return it (`get_shaderiv(shader, pname) -> i32`);
  queries for several write up to `count` values and return how many there are
  (`get_integerv(pname, dst, count) -> i32`). Text comes back with the "copied
  if it fits" convention: the length is returned, the text copied only if it
  fits in `cap` (`get_string`, `get_shader_info_log`, ...).
- **GL errors are GL's.** A call that fails a GL rule records an error for
  `get_error` and has no effect; the guest continues. **Boundary violations
  trap**, as everywhere: out-of-bounds pointers, lengths too short for the
  data, strings that aren't UTF-8.
- Shaders are GLSL ES 3.00 (`#version 300 es`). Extensions are WebGL's:
  listed in `get_string(GL_EXTENSIONS)`, enabled with `enable_extension(name)`.
- Results that WebGL delivers asynchronously are ready **from the next frame
  on**, on every runner: `GL_QUERY_RESULT_AVAILABLE` and `client_wait_sync` /
  `GL_SYNC_STATUS` (`GL_TIMEOUT_EXPIRED` / `GL_UNSIGNALED` in the frame of the
  query or fence). The values themselves (an occlusion result) come from the GPU.

**Frames and the default framebuffer.**

| Import | Signature | Semantics |
|---|---|---|
| `width` / `height` | `() -> u32` | Drawable size; the default framebuffer follows it on resize. |
| `frame_shown` | `() -> u32` | `0` during catch-up frames (the runner runs several frames to catch up and shows only the last; the guest may skip drawing); `1` otherwise, headless included. |
| `present` | `()` | Show the default framebuffer now; otherwise the runner presents at the end of the frame. |

The default framebuffer has depth 24 + stencil 8, no alpha and no
multisampling (games multisample with their own renderbuffers); it isn't
preserved between frames.

Every runner checks a call against the shared model first (below); only calls
that pass reach WebGL or ANGLE, so the model's GL errors come first and
`get_error` then reports the backend's.

**Null GL** (headless runs): object names, bindings, the pixel store and every
GL error a guest can cause through names, targets, enums and sizes are tracked
exactly as in the browser runner (both runners share the model:
`runners/web/lib/gl.js`, `runners/native/src/gl.rs`). Everything a guest may
branch on is fixed:

- `GL_MAX_*` limits are WebGL 2's minimums; no extensions; the strings name the
  null GL (`GL_VERSION` "OpenGL ES 3.0 (gasm null GL)").
- Shaders compile and programs link; every uniform name gets a location,
  attribute and frag-data locations are 0, uniform indices and block indices
  0; `GL_ACTIVE_*` counts are 0.
- `read_pixels` and `get_buffer_sub_data` return zeros, occlusion queries
  "passed" (1), framebuffers are complete.

**Hashing.** Headless runners fold uploads into the video hash (FNV-1a 32, as
`write_buffer`): a header of little-endian `u32`s, then the payload.

| Upload | Header | Payload |
|---|---|---|
| `buffer_data`, `buffer_sub_data` | `1, target, offset, len` (`offset` 0 for `buffer_data`) | the bytes (none for `buffer_data` with no data) |
| `tex_image_2d/3d`, `tex_sub_image_2d/3d` | `2, target, level, internalformat (0 for sub), x, y, z, w, h, d, format, type, len (0 without data)` | the bytes (none from an unpack buffer) |
| `compressed_tex_*` | the same, `format` = internalformat (sub: format) and `type` 0 | the bytes |
| `uniform*` | `3, location, kind, count` (matrices: `, transpose`); kind 1–4 float, 11–14 int, 21–24 uint, 32–40 the matrices in ABI order | the values as stored (`f32` bits) |

Calls that fail a GL check hash nothing; draws and state aren't hashed (as in
`gasm:gfx`).

**C SDK.** `#include <GLES3/gl3.h>` (or `<GLES2/gl2.h>`) and link
`sdk/c/src/gasm_gl.c` (CMake: `${GASM_GL_SOURCE}`). Both are generated from the
Khronos registry by `scripts/gen-gl-headers.py` and give the whole GLES 3.0 API:
gen/delete loops, `glShaderSource` string arrays, `glGetString` /
`glGetStringi` (cached), the pixel store mirrored to compute exact lengths, and
`glMapBufferRange` emulated in guest memory (uploaded on unmap or flush). Program
binaries and `glShaderBinary` aren't available (`GL_INVALID_ENUM`, as WebGL).

## `gasm:net` (optional): message connections

WebSocket semantics on every runner, because browsers cannot open raw sockets:
reliable, ordered, binary messages. Everything is non-blocking; poll once per
frame.

| Import | Signature | Semantics |
|---|---|---|
| `open` | `(url_ptr, len) -> i32` | Open a `ws://` or `wss://` URL. Handle > 0, or `-1` if denied, invalid or too many are open (16). TLS uses the platform's trusted certificates. |
| `state` | `(conn) -> u32` | `0` connecting, `1` open, `2` closed, `3` error. A closed handle reports `2`. |
| `send` | `(conn, ptr, len) -> i32` | One message (`len > 0`). `0` ok, `-1` if not open or the send queue is full (4096 messages). |
| `recv` | `(conn, dst, cap) -> i32` | Next message's length (copied only if ≤ `cap`; otherwise it stays queued), `0` if none waiting, `-1` if closed or failed and drained. |
| `close` | `(conn)` | Close, after flushing what was sent; closing again does nothing. |

A handle `open` never returned traps (as gfx handles do).

**Permission.** Hosts allowed up front (`gasm-run --allow-net`, or
`--allow-net=host,host` for only those) are reached at once. For any other host
the window runner and the browser player ask the player
([Player consent](#player-consent)); the connection stays connecting until the
answer and ends in error if it's no. Headless runs never ask (refused unless
allowed). In browsers the browser's own rules apply on top.

**Determinism.** Messages arrive at unpredictable times. Deterministic games
must only let *message contents* affect the simulation, never arrival
timing: lockstep games apply inputs at the frame number carried in the
message. See `guests/sumo` and `gasm-relay`.

## `gasm:fetch` (optional): HTTP requests

The runner makes HTTP(S) requests for the guest: natively on background threads
(TLS with the platform's trusted certificates), in browsers with `fetch()`. The
guest starts a request and polls it once per frame; nothing blocks. Design:
[design/fetch.md](https://github.com/emdzej/gasm/blob/main/design/fetch.md).

| Import | Signature | Semantics |
|---|---|---|
| `request` | `(desc_ptr, desc_len, body_ptr, body_len) -> i32` | Start a request. `desc` is JSON: `{"method":"GET","url":"https://…","headers":{"accept":"application/json"}}` (`method` default `GET`; `GET HEAD POST PUT PATCH DELETE OPTIONS`; `url` absolute `http`/`https`). `body_len` bytes of body (`0`: none; at most 16 MiB; none for `GET`/`HEAD`). Handle > 0, or `-1` if denied, invalid, or too many are open (16). |
| `state` | `(req) -> u32` | `GASM_FETCH_PENDING` (0), `_HEADERS` (1: status and headers are in, the body is arriving), `_DONE` (2: the whole body has arrived), `_FAILED` (3: network error, timeout, body over 64 MiB, refused by the browser). A closed handle reports `3`. |
| `status` | `(req) -> i32` | HTTP status of the final response (redirects are followed), `0` before the headers or after a failure. |
| `headers` | `(req, dst, cap) -> i32` | Response headers as `name: value\n` lines: names lowercase and sorted, repeated names joined with `, ` (as `fetch()`'s `Headers`). Connection-level headers and cookies are never listed; bodies arrive decoded, so `content-encoding` isn't either (nor `content-length` if it applied). Length (copied only if ≤ `cap`; `cap = 0` queries), `-1` before state 1. Browsers list only what CORS exposes. |
| `read` | `(req, dst, cap) -> i32` | Copy up to `cap` bytes of body that have arrived: the count, `0` if none are waiting yet, `-1` once the body is done and drained, or after a failure. Reading is what makes room: a guest that stops reading pauses the download. |
| `close` | `(req)` | Cancel if still running and free the handle; closing again does nothing. |

A handle `request` never returned traps. Every runner refuses the same
descriptions: request headers browsers forbid (`host`, `cookie`, `origin`,
`referer`, `user-agent`, `accept-encoding`, `content-length`, `sec-*`,
`proxy-*`, …), invalid header names or values, unknown methods. Requests are
stateless: no cookies or credentials, no cache.

**Identity.** The runner chooses the `User-Agent`, never the guest. Browsers
send their own. Native and Node runners send `gasm-run/<version>` or
`gasm-headless/<version>`; whoever launches the game names it with `--app-id <text>`
(e.g. `--app-id 'mygame/1.0 (+https://mygame.example)'`), which goes in front:
`mygame/1.0 (+https://mygame.example) gasm-run/0.10.0`. Some APIs ask for that
(MET Norway's terms want an app name and contact).

**Permission.** As for `gasm:net`: hosts allowed up front by `gasm-run
--allow-net` (any host) or `--allow-net=api.example.org,*.example.org` (only
those; `*.` for subdomains; the list covers `gasm:net` too), any other host the
player's choice ([Player consent](#player-consent)): the request stays pending
until the answer and fails if it's no. Every redirect hop to a new host is
asked about the same way. In the browser the page decides (`GasmHost`'s
`allowNet` and `ask`), and CORS and mixed-content rules apply on top.

**Determinism.** Responses arrive whenever the network delivers them. Headless
runners can record them (`--fetch-record DIR`, with `--allow-net`) and replay
them (`--fetch-replay DIR`, no network): a replayed request completes at the
start of the next frame, on every runner, so runs are reproducible. Records are
`<key>.json` (status, headers) and `<key>.body`, keyed by FNV-1a 64 of method,
URL and request body (16 hex digits).

## `gasm:storage` (optional): persistent key/value store

For saves, settings and high scores. Each game gets its own **namespace, chosen
by the runner**, never by the game, so one game can't read another's saves.
By default the namespace is the game file's name (`sumo.wasm` → `sumo`).

| Import | Signature | Semantics |
|---|---|---|
| `get` | `(key_ptr, key_len, dst, cap) -> i32` | Value length, or `-1` if the key doesn't exist. Copied only if length ≤ `cap` (call with `cap = 0` to query the size). |
| `set` | `(key_ptr, key_len, data_ptr, data_len) -> i32` | Store a value: `0`, or `GASM_STORAGE_ERR_KEY` (-1) invalid key, `_SIZE` (-2) value over 1 MiB, `_QUOTA` (-3) namespace full, `_IO` (-4) the runner couldn't write it. |
| `delete` | `(key_ptr, key_len) -> i32` | `0` if deleted, `-1` if it didn't exist. |
| `count` | `() -> u32` | Number of keys in the namespace. |
| `key` | `(index, dst, cap) -> i32` | Key `index` (0 … `count`−1, sorted): its length (copied only if ≤ `cap`; `cap = 0` queries), or `-1` if out of range. Lets games list save slots. |

Limits: keys are 1–128 bytes of `[A-Za-z0-9._-]` (not `.` or `..`), values up
to 1 MiB, total 16 MiB per namespace. Durability: native writes are atomic and
complete when `set` returns; the browser persists to IndexedDB in the
background right after `set`. **Headless runs start with an empty in-memory
store** (unless given a directory), so runs are reproducible.

Where data lives: native: `<data dir>/gasm/<namespace>/<key>`, one file per key
(macOS `~/Library/Application Support`, Linux `$XDG_DATA_HOME` or
`~/.local/share`, Windows `%APPDATA%`), override with `--storage-dir`. Browser:
IndexedDB database `gasm`, keys `<namespace>/<key>`, per site origin.

## `gasm:clipboard` (optional): text on the system clipboard

Copy and paste for text fields and "copy link" buttons. **Copying is always
allowed; pasting is the player's choice:** a game can read the clipboard only
during the frame that carries the player's paste key press (Ctrl+V, or Cmd+V on
macOS), never otherwise, so a game can't read whatever happens to be on the
clipboard (a password, say) and send it somewhere. Text fields read the
clipboard when they handle that key, so paste works as players expect.

| Import | Signature | Semantics |
|---|---|---|
| `set_text` | `(ptr, len) -> i32` | Copy UTF-8 text: `0` (the runner puts it on the clipboard after this frame), or `-1` if refused (over 1 MiB, no clipboard). Invalid UTF-8 traps. |
| `get_text` | `(dst, cap) -> i32` | The pasted text's length in bytes (copied only if ≤ `cap`; `cap = 0` queries), or `-1` outside a paste frame or with nothing to paste. |

Natively the window runner uses the system clipboard; in the browser the page
listens for the `paste` event (games that read the raw keyboard still get the
paste shortcut's key events) and copies with `navigator.clipboard.writeText`.
**Headless runs have an empty clipboard** (copies are accepted and dropped), so
runs stay reproducible; neither direction is hashed. Images aren't supported:
the runners' copy key (F2) copies the game's frame.

## `gasm:files` (optional): files for the player

For a photo mode's pictures or an editor's exports: files the player keeps
outside the game, unlike `gasm:storage`, which only the game reads. The game
hands the runner a copy; **the runner decides where it goes and the game never
learns the path**.

| Import | Signature | Semantics |
|---|---|---|
| `save` | `(name_ptr, name_len, mime_ptr, mime_len, data, len) -> i32` | A handle > 0 (the runner saves the copy after this frame), or `-1`: refused (saving is off, the name empty or over 255 bytes, the type not `type/subtype`, over 256 MiB, or 16 saves still pending). Invalid UTF-8 traps. |
| `state` | `(handle) -> i32` | `GASM_FILES_PENDING` (0), `GASM_FILES_SAVED` (1) or `GASM_FILES_FAILED` (2: cancelled or not written). A handle `save` never returned traps. |

The name is a suggestion: runners keep its last path component only, replace
characters file systems refuse, drop leading dots, and never overwrite a file
(`photo.png`, then `photo (2).png`). Natively images (`image/*`) go to
`Pictures/<game>/` and everything else to `Downloads/<game>/` (the XDG user
directories on Linux), after the player agreed to the game saving files
([Player consent](#player-consent); the save stays pending until then);
`gasm-run --save-dir <dir>` picks the folder (no question) and `--no-save`
refuses every save. Browsers offer the file as a download.
**Headless runs write nothing** unless given `--save-dir`; either way a save is
`SAVED` by the next frame, so runs stay reproducible. Saves aren't hashed.

## WASI subset

Guests may import `wasi_snapshot_preview1` (C with wasi-libc does; Rust on
`wasm32-unknown-unknown` only needs `proc_exit` via `gasm::exit`). Every runner
implements exactly this, with the same results:

| Function | Result |
|---|---|
| `fd_write` | fd 1 and 2 go to the runner's **log** (never to its stdout, which carries the hash lines of headless runs); other fds `EBADF` (8) |
| `fd_close` | success |
| `fd_seek` | `ESPIPE` (70) |
| `fd_fdstat_get` | fds 0–2: a character device; others `EBADF` |
| `fd_prestat_get` | `EBADF`: no preopened directories (wasi-libc stops looking) |
| `clock_time_get`, `clock_res_get` | clocks 0–3 (`EINVAL` beyond). Headless: **virtual time** (as `time_ms`; the realtime clock counts from 1970-01-01 00:00 UTC). Otherwise real time; resolution 1 µs |
| `random_get` | OS randomness; headless: a **fixed sequence** (splitmix64 from 0, little-endian), so runs are reproducible |
| `args_*`, `environ_*` | empty |
| `sched_yield` | success |
| `proc_exit` | ends the game |

Anything else the guest imports from `wasi_snapshot_preview1` exists and
returns `ENOSYS` (52), so wasi-libc functions that need more (files, sockets,
`poll_oneoff` sleeps) fail without trapping. There is **no filesystem**; use
assets and storage (the C SDK's `gasm_vfile.h` gives both a `FILE*`).

## Raw input

Next to the four virtual pads (`input_pad`, mapped from keyboards and gamepads by
the runner's keymap), guests can read devices directly. Like pads, everything is
sampled before `gasm_frame` and stable within it.

**Keyboard.** Keys are physical keys, numbered by `GASM_KEY_*` (in `abi.json`, with
their W3C `KeyboardEvent.code` names: `GASM_KEY_SHIFT_LEFT` is `ShiftLeft`). A key
is the same key on every layout; typed characters come from `text_input`.
Modifiers are ordinary keys, so Shift+Left is `key_state` with both bits set.
`key_events` gives the order and catches taps shorter than a frame. Keys bound
to pads still show up as raw keys.

**`input_mode(flags)`:**

| Flag | Meaning |
|---|---|
| `GASM_INPUT_KEYS_RAW` (1) | The guest reads the keyboard itself: the runner stops mapping keys to pads (gamepads still map), so nothing arrives twice. |
| `GASM_INPUT_POINTER_HIDDEN` (2) | Hide the system cursor over the game (the game draws its own). |
| `GASM_INPUT_POINTER_LOCKED` (4) | Capture the pointer for relative motion (mouselook). Best effort: browsers lock on the next click; the pointer's flags say what happened. |

**Pointer** (48 bytes, little-endian):

| Offset | Field | |
|---|---|---|
| 0, 4 | `f32 x, y` | position in drawable pixels (the `width`/`height` space of `gasm:gfx`) |
| 8, 12 | `f32 fx, fy` | the same position in the last `video_present` frame's pixels (the runner undoes its letterboxing; outside the frame it's < 0 or ≥ the size) |
| 16, 20 | `f32 dx, dy` | relative motion since the previous frame, also while locked |
| 24, 28 | `f32 wheel_x, wheel_y` | wheel since the previous frame, about 1 per notch; `y` > 0 = down |
| 32 | `u32 buttons` | held: `GASM_MOUSE_LEFT` 1, `RIGHT` 2, `MIDDLE` 4, `BACK` 8, `FORWARD` 16 |
| 36, 40 | `u32 pressed, released` | buttons that went down / up since the previous frame (a click inside one frame shows in both) |
| 44 | `u32 flags` | `GASM_POINTER_INSIDE` 1, `IS_HIDDEN` 2, `IS_LOCKED` 4 |

The frame position uses the same arithmetic as the display, centred: with
square pixels the scale is s = min(dw / fw, dh / fh) on both axes. With a
display aspect `n:d` (`video_set_aspect`) the vertical scale is
s = min(dw·d / (fh·n), dh / fh) and the horizontal one s·fh·n / (d·fw)
(whole numbers multiplied before dividing, so common ratios come out exact).
When the player chose integer scaling (and the frame fits), s is rounded down
to a whole number before the horizontal scale is derived. Headless runs never
use integer scaling.

**Gamepads and joysticks** (204 bytes per slot, little-endian): `u32 flags`
(`GASM_GAMEPAD_CONNECTED` 1, `GASM_GAMEPAD_STANDARD` 2), `u32` button count,
`u32` axis count, `f32 buttons[32]` (0–1, analog triggers included), `f32
axes[16]` (−1…1). Slots follow connection order, like the pads. Known
controllers use the **W3C standard mapping**: buttons 0 south, 1 east, 2
west, 3 north, 4/5 shoulders, 6/7 triggers, 8 select, 9 start, 10/11 stick
clicks, 12–15 d-pad up/down/left/right, 16 home; axes 0/1 left stick, 2/3
right stick, `y` > 0 = down. Other devices (flight sticks, wheels, pedals)
report their buttons and axes in device order, without the `STANDARD` flag;
`gamepad_name` tells them apart.

## Determinism

Given the same module, assets, params and per-frame input, a guest must produce
bit-identical `video_present`/`audio_push`/`write_buffer`/`write_texture`
streams on every runner in headless runs (WASI clocks and `random_get` are
virtual there too). The runners verify this with FNV-1a-32 hashes against golden
values on Linux, macOS and Windows (`make test`, `tests/golden/determinism.txt`).
Caveats: wasm NaN bit patterns are nondeterministic by spec (guests that hash
or store NaN payloads can diverge), and a guest that reads uninitialized or
freed memory gets the same bytes on every runner, but different ones when its
own allocations change (the DOOM renderer reads past some lumps, so changes to
its glue can change its hashes).

## Runner behaviour

Not part of the import/export contract, but common to the reference runners
(`gasm-run`, the browser runner, the Node headless runner) so games behave the
same everywhere.

### Assets

- **File-backed, never preloaded (native, Node folders):** assets given by path
  are not read up front. `asset_size` comes from file metadata (folder
  entries: as found at start-up); `asset_read` and `asset_read_at` do
  positioned reads straight into guest memory. Folder entries are opened on
  first read and kept in a small cache (64 open files), so CD-sized trees
  don't hit the open-file limit. A 200 MB asset streamed at random offsets
  costs no extra RAM (measured: 24.9 MB max RSS vs 25.3 MB with a tiny
  asset). A file that shrinks, disappears or is replaced by a symlink while
  running yields fewer bytes (possibly 0); runners never crash.
- **Sizes** are 64-bit (`asset_size64`, `asset_read_at64`): any file works.
  `asset_size` reports assets of 2 GiB and more as `-2`.
- **Folders:** a folder exposes every regular file under it, recursively.
  The asset name is the `/`-separated path relative to the folder, as stored
  (`ART/ART.CAR`), optionally with a prefix (`cd/ART/ART.CAR`). The set of
  names is fixed at start-up.
  - Symlinks are skipped (nothing outside the folder is reachable).
  - Hidden entries are skipped: any path component starting with `.`, e.g.
    `.DS_Store`, `._foo`, `.git`.
  - Native: `--asset-dir [prefix=]dir`. Node: the same flag. Browser: a
    directory handle (`showDirectoryPicker`), a `webkitdirectory` file list
    (the leading root-folder segment of `webkitRelativePath` is stripped,
    so names match), an OPFS directory, or `File` objects.
- **Lookup:** an exact name always wins, and explicit assets (`--asset`)
  override folder entries with the same name. Folder entries also match
  **case-insensitively** (ASCII folding): `Art/art.car` finds
  `ART/ART.CAR`. If folder entries differ only in case, runners warn at
  start-up and resolve case-insensitive lookups to the first name in sorted
  order.

### Assets that change

Assets are fixed for a run unless the embedder replaces them, and only between
frames: a guest's reads within a frame always see one version.

- **Embedders:** `Game::set_asset(name, bytes)` / `remove_asset` (crate
  `gasm-host`), `GasmHost.setAsset(name, bytes)` / `removeAsset` and
  `GasmWorker.setAsset` (`@emdzej/gasm-host`). New names are listed by
  `asset_count`/`asset_name` from then on.
- **Launchers:** `gasm-run --watch-asset name=path` (and the Node runner's
  flag) re-reads the file whenever its size or modification time changes,
  checked before each frame, whether it was rewritten in place or replaced by
  a rename (the safe way to write it). The file may appear after the start.
- **Guests** poll `asset_version(name)`: `0` as launched, then a new, larger
  number for each replacement (one counter for all assets, so a removed and
  re-added asset gets a new one). A read spread over several frames should
  check the version didn't change in between. Godot games:
  `FileAccess.get_modified_time("res://<name>")` is the version.

Nothing else changes assets: a native file asset that is modified some other
way (without `--watch-asset`) has no new version, and what the guest reads is
undefined.

### Worker mode (browser runner)

Games can run in a dedicated Worker (`@emdzej/gasm-host/worker`). The page
keeps input, display and audio.
- **Per frame:** input (pads, typed text, raw keyboard/pointer/gamepads) for
  each frame is sent before it runs and is stable within it. The worker
  returns the latest RGBA frame and the audio. Buffers are *transferred*, so there's no
  `SharedArrayBuffer` and no COOP/COEP requirement (GitHub Pages works).
- **Unchanged:** frame pacing, the catch-up rule (only the last frame of a
  batch is shown) and `begin_frame` semantics.
- **Storage and net:** `gasm:storage` (IndexedDB) and `gasm:net` (WebSocket)
  run inside the worker.
- **`gasm:gfx` games:** the page transfers its canvas
  (`transferControlToOffscreen`) and reports its display size with each
  batch; the worker renders with WebGPU into the `OffscreenCanvas` (and blits
  `video_present` frames there too). This needs WebGPU in workers, which
  Chromium has; where it's missing the worker says so and the page runs the
  game on the main thread instead.
- **Lazy asset providers,** only available in workers:
  - **OPFS**, via `FileSystemSyncAccessHandle`: reads go straight from OPFS
    into guest memory. Handles are opened while loading, because
    `createSyncAccessHandle` is async and can't be awaited from a
    synchronous read.
  - **`File`/`Blob`**, via `FileReaderSync`.

  Main-thread mode remains the default.

Measured (Chrome, macOS): streaming 10 GB of random 64 KB reads from a
200 MB OPFS file in Worker mode, the renderer's resident memory peaked 3 MB
above where it started (11 MB with a 1 MB file: it doesn't grow with the data
read), and the hashes matched the Node runner reading the same files from disk.

### Keyboard layouts

Both runners map keys to virtual pads through the same text format. There's
one binding per line, `<pad 1-4> <button> <key code>...`. Buttons are
`a b x y l r select start up down left right`; key codes are W3C
`KeyboardEvent.code` names.
- **Default:** pad 1 on the arrows plus `X`/`Z`/`S`/`A`/`Q`/`W`, `Enter` and
  Right Shift; pad 2 on `IJKL` plus `.`/`,`/`M`/`N`/`U`/`O`, Right Ctrl or
  keypad Enter, and Backspace.
- **Gamepads** take pads in connection order. Keyboard bindings for pad
  N ≥ 2 apply while fewer than N gamepads are connected.
- **Changing it:** native uses `--keymap FILE` (default:
  `<data dir>/gasm/keymap.txt` if present) and `--print-keymap`. The web
  player has a "keys…" editor, stored in `localStorage`. Escape can't be
  bound to a pad.

### Window title

`set_title` names the game; the runner shows it in a frame of its own that the
guest can't remove, so a game can't pass itself off as the runner's or the
browser's UI:

- Text: control characters (Unicode Cc) and bidi controls (U+202A–U+202E,
  U+2066–U+2069) are removed, then the rest is cut to 256 bytes at a character
  boundary. Invalid UTF-8 traps, as for every string. An empty result restores
  the default: the module's `gasm.title` custom section (cleaned the same way;
  ignored if it isn't UTF-8), else its file name without `.wasm`. Launchers can
  read the section without running the game (SDKs: `gasm::title!("Sumo")`,
  `GASM_TITLE("Sumo")`).
- Native window: `<title> — gasm — <n> fps`. Browser player: the tab is
  `<title> — gasm` (the host reports it with `onTitle`; embedding pages decide
  what to do with it). Headless runs log `[gasm] title: …` when it changes;
  titles aren't hashed.
- Runners apply it after the frame that set it, and only when it changed, so
  a guest may call it every frame.

### Escape

A tap of Escape goes to the game (`key_state`, `key_events`). Holding it for a
second quits natively and stops the game in the browser, after `gasm_exit`.
Closing the window or the page also quits.

### Player consent

A game that wants to reach outside itself asks the player through the runner,
not through an import: the window runner and the browser player ask before a
game connects to a host nobody allowed up front (`gasm:net`, `gasm:fetch`; once
per host) and, natively, before its first save for the player (`gasm:files`;
once per game). Four answers: **allow this time** (until the game ends),
**always allow**, **not now** (asked again next run) and **never** (don't ask
again). The game keeps running until it needs the answer: natively it pauses
while the question is on screen (keys 1 to 4, Esc for not now), in the browser
a dialog asks while the request waits.

Remembered answers belong to the game (its id: the file name or
`--storage-id`). Natively they're in `<data dir>/gasm/consent/<game>.txt`;
`gasm-run --forget-consent <game>` (or `all`) clears them, and `--no-ask` refuses
instead of asking. The player keeps them in the page's `localStorage`; "forget
answers" clears them, and `?allownet` / `?allownet=a.org,b.org` (and a relay
typed into the page) allow hosts up front. Headless runs never ask, so they stay
reproducible: what the command line didn't allow is refused.

### Scripted input (headless)

`--input` (both headless runners) takes comma-separated `FRAMES:ACTION` items;
`FRAMES` is `N` or `FROM-TO` (inclusive). Commas inside quotes or parentheses
don't split items. Numbers are plain decimals (no hex, `inf` or `nan`); both
runners accept and reject the same scripts.

| Action | |
|---|---|
| `A+B+START` | pad 1 buttons (`A B X Y L R SELECT START UP DOWN LEFT RIGHT`) |
| `"text"` | typed text on frame `N` (escapes `\n` enter, `\b` backspace, `\\`, `\"`) |
| `KEY(ShiftLeft+ArrowLeft)` | raw keys held (W3C names); events come from the changes between frames |
| `PTR(x,y)`, `PTR(x,y,L+R)` | pointer position in drawable pixels (1280×720 headless) and buttons (`L R M BACK FWD`); the position stays until the next `PTR` |
| `MOVE(dx,dy)`, `WHEEL(x,y)` | relative motion, wheel, per frame |
| `GP0(B0+B9+A1=0.5)` | gamepad slot 0–3: buttons by index, axes `An=value`; slots used anywhere in the script are connected, standard mapping, named `scripted` |

Headless runs have a keyboard, a pointer and four gamepad slots, so the raw
imports never return `-1` there, and the cursor modes count as achieved.

## Roadmap

What is planned beyond this (`gasm:gl` natively, render targets, rollback
netplay, packages, a capabilities manifest, ...) is on the
[roadmap](https://github.com/emdzej/gasm/blob/main/site/docs/roadmap.md).
