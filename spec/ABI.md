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
| `gasm` | core | log, time, frame rate, 2D video, audio, input, text input, assets, params |
| `gasm:gfx` | optional | GPU rendering: a WebGPU subset |
| `gasm:net` | optional | message connections (WebSocket semantics) |
| `gasm:storage` | optional | persistent per-game key/value store (saves, settings) |

Games import only what they use. A runner that lacks an optional module can
still run games that don't import it.

**Versioning.** `GASM_ABI_VERSION` changes only for breaking changes.
Additions (new imports, new descriptor fields) keep it: runners link unknown
imports as traps, so an older runner still loads a newer guest and fails only
if the guest calls something it lacks, and an older guest never calls the
new imports. Textures, samplers, explicit layouts, viewport/scissor, text
input and asset enumeration were added this way; the version is still 0.

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
| `text_input` | `(dst, cap) -> i32` | UTF-8 text typed since the previous frame, stable within one `gasm_frame`: backspace is `\b` (0x08), enter is `\n`. Returns its length (copied only if length ≤ `cap`; `cap = 0` queries), or `-1` if the runner has no keyboard. Keys bound to pads still produce text; the guest decides what it wants. Headless: from the `--input` script (`FRAME:"text"`). |
| `asset_size` | `(name_ptr, name_len) -> i32` | Byte size, or `-1` if missing. |
| `asset_read` | `(name_ptr, name_len, dst, cap) -> i32` | Copy ≤ `cap` bytes, return count or `-1`. |
| `asset_read_at` | `(name_ptr, name_len, offset, dst, len) -> i32` | Copy up to `len` bytes starting at byte `offset` (streaming large assets). Returns bytes copied (0 at or after the end), or `-1` if missing. |
| `asset_count` | `() -> u32` | Number of assets. |
| `asset_name` | `(index, dst, cap) -> i32` | Name of asset `index` (0 … `asset_count`−1), sorted by UTF-8 bytes; folder entries as named on disk. Returns its length (copied only if length ≤ `cap`; `cap = 0` queries), or `-1` if `index` is out of range. |
| `param` | `(name_ptr, name_len, dst, cap) -> i32` | Launch parameter value: returns its byte length, or `-1` if unset. Copied only if length ≤ `cap`; call with `cap = 0` to query the length. |

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
| `set_pipeline` | `(p)` | |
| `set_bind_group` | `(index, bg)` | |
| `set_bind_group_offsets` | `(index, bg, ptr, count)` | Like `set_bind_group`, with `count` `u32` dynamic offsets read from guest memory: one per dynamic-offset entry of the bind group's layout, in binding order, each a multiple of 256, and the bound range must stay inside the buffer. |
| `set_viewport` | `(x, y, w, h, min_depth, max_depth: f32)` | Viewport in drawable pixels, clamped to the drawable; depth range within 0–1. |
| `set_scissor_rect` | `(x, y, w, h)` | Scissor rectangle in drawable pixels, clamped to the drawable. |
| `set_vertex_buffer` | `(slot, buf, offset)` | |
| `set_index_buffer` | `(buf, format, offset)` | `format`: 0 = uint16, 1 = uint32. |
| `draw` | `(vertex_count, instance_count, first_vertex, first_instance)` | |
| `draw_indexed` | `(index_count, instance_count, first_index, base_vertex: i32, first_instance)` | |
| `end_frame` | `()` | Submit and present. |

Draw and set calls outside `begin_frame`/`end_frame`, or after `begin_frame`
returned 0, are validated and then ignored. If a viewport or scissor
rectangle is empty after clamping, draws are skipped until it is set again.

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

**Validation.** Invalid descriptors, WGSL errors, out-of-range mip levels or
regions, a `len` that doesn't match `w*h*4`, handles of the wrong kind,
misaligned or out-of-bounds dynamic offsets, and bind groups that don't match
their layout all trap the guest with a message. The runners check textures,
samplers, layouts and offsets against their own record of every object, so
this happens identically with the null GPU of headless runs. Not in this
subset: render targets, cube maps, storage textures, compute, stencil.

**Hashing.** Headless runners fold every `write_buffer` payload into the video
hash, and for `write_texture` first a header (`tex, mip, x, y, w, h` as
little-endian `u32`s) and then the payload, so the same texels written to a
different place change the hash. Uniforms and textures carry the scene state,
so deterministic GPU games get checked too, without comparing pixels (GPU
output isn't bit-exact across vendors). Handles are numbered 1, 2, 3, … in
creation order on every runner. Draw calls, offsets, viewports and scissors
aren't hashed.

## `gasm:net` (optional): message connections

WebSocket semantics on every runner, because browsers cannot open raw sockets:
reliable, ordered, binary messages. Everything is non-blocking; poll once per
frame.

| Import | Signature | Semantics |
|---|---|---|
| `open` | `(url_ptr, len) -> i32` | Open a `ws://` or `wss://` URL. Handle > 0, or `-1` if denied or invalid. TLS uses the platform's trusted certificates. |
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
`video_present`/`audio_push`/`write_buffer`/`write_texture` streams on every runner. The
runners verify this with FNV-1a-32 hashes (`make test`). One caveat: wasm NaN
bit patterns are nondeterministic by spec. Guests that hash or store NaN
payloads can diverge.

## Runner behaviour

Not part of the import/export contract, but common to the reference runners
(`gasm-run`, the browser runner, the Node headless runner) so games behave the
same everywhere.

### Assets

- **File-backed, never preloaded (native, Node folders):** assets given by path
  are opened, not read. `asset_size` comes from file metadata; `asset_read`
  and `asset_read_at` do positioned reads straight into guest memory. A
  200 MB asset streamed at random offsets costs no extra RAM (measured:
  23.8 MB max RSS vs 23.7 MB with a tiny asset). A file that shrinks or
  disappears while running yields fewer bytes (possibly 0) or `-1`; runners
  never crash.
- **Size limit:** sizes are 32-bit (`asset_size -> i32`). Files over
  2 GiB − 1 are refused at start-up with an error (folder entries that large
  are skipped with a warning).
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

### Worker mode (browser runner)

Games can run in a dedicated Worker (`@emdzej/gasm-host/worker`). The page
keeps input, display and audio.
- **Per frame:** input (pads and typed text) for each frame is sent before
  it runs and is stable within it. The worker returns the latest RGBA frame
  and the audio. Buffers are *transferred*, so there's no
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
200 MB OPFS file in Worker mode, the renderer's resident memory grew about
20 MB (about 40 MB with a 1 MB file: it doesn't grow with the data read),
and the hashes matched the Node runner reading the same files from disk.

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
  player has a "keys…" editor, stored in `localStorage`. Escape is reserved
  for quitting.

## Roadmap (not in v0)

- `.gasm` packages: one file bundling `game.wasm`, assets and a manifest.
- `gasm:files`: a runner-provided file picker (the game only sees what the player picks).
- `gasm:gfx`: render targets (render-to-texture), cube maps, render bundles.
- Capabilities manifest (custom section `gasm.manifest`) that declares required
  and optional imports, network hosts, and platform extensions (`gasm:ext/*`).
- Runner-level rollback netplay (snapshot/restore guest memory).
- Move to WIT/Component Model once browser support doesn't need transpiling.
