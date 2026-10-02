# Writing runners

A runner makes every gasm game available on a new platform: a new OS, a
console, a microcontroller, a different wasm engine, or an embedding inside an
existing app. The contract is the [ABI spec](/docs/abi). Use the two
existing runners as reference implementations:

| | Native (Rust) | Web (JS) |
|---|---|---|
| core ABI | [`host.rs`](https://github.com/emdzej/gasm/blob/main/runners/native/src/host.rs) (wasmtime) | [`lib/host.js`](https://github.com/emdzej/gasm/blob/main/runners/web/lib/host.js) (`GasmHost`; `gasm-host.js` re-exports `lib/`) |
| WASI subset | [`wasi.rs`](https://github.com/emdzej/gasm/blob/main/runners/native/src/wasi.rs) | [`lib/wasi.js`](https://github.com/emdzej/gasm/blob/main/runners/web/lib/wasi.js) |
| `gasm:gfx` | [`gfx.rs`](https://github.com/emdzej/gasm/blob/main/runners/native/src/gfx.rs) (wgpu, with validation and the 2D blit) | [`lib/gfx.js`](https://github.com/emdzej/gasm/blob/main/runners/web/lib/gfx.js) (`GfxModel`: validation, handles) + [`webgpu-gfx.js`](https://github.com/emdzej/gasm/blob/main/runners/web/webgpu-gfx.js) (WebGPU backend) |
| `gasm:net` | [`net.rs`](https://github.com/emdzej/gasm/blob/main/runners/native/src/net.rs) (tungstenite threads) | [`lib/net.js`](https://github.com/emdzej/gasm/blob/main/runners/web/lib/net.js) (`NetConnections`, WebSocket) |
| `gasm:storage` | [`storage.rs`](https://github.com/emdzej/gasm/blob/main/runners/native/src/storage.rs) | [`lib/storage.js`](https://github.com/emdzej/gasm/blob/main/runners/web/lib/storage.js) (`MemoryStorage`, `IdbStorage`) |
| complete runners | [`headless.rs`](https://github.com/emdzej/gasm/blob/main/runners/native/src/headless.rs), [`window.rs`](https://github.com/emdzej/gasm/blob/main/runners/native/src/window.rs) | [`headless.mjs`](https://github.com/emdzej/gasm/blob/main/runners/web/headless.mjs), `app.js` (the player) |

To embed rather than write a runner, use the published hosts:
[`@emdzej/gasm-host`](/dev/packages#for-runners-and-embedders) (browser/Node) or
the [`gasm-host`](https://crates.io/crates/gasm-host) crate (native). The
machine-readable ABI ([`spec/abi.json`](https://github.com/emdzej/gasm/blob/main/spec/abi.json))
lists every function to implement; `scripts/gen-abi.mjs --check` verifies the
reference runners against it.

A minimal runner implements only the core `gasm` module (23 functions).
`gasm:gfx`, `gasm:net` and `gasm:storage` are optional: games that don't import
them run anyway, and games can ask with `has` before using them.

### Embedding the reference runners

The `gasm-host` crate contains complete runners, not just the host:

```rust
use gasm_host::{headless, host::LoadOptions, session::Session, storage::Storage};

let session = Session {
    name: "game".into(), wasm: std::fs::read("game.wasm")?,
    assets, params, allow_net: false, storage: Storage::memory(),
    load: LoadOptions::default(),   // allow_precompiled: false, call_timeout: 30 s
};
let report = headless::run(session, &opts)?;   // headless::Options: frames, screenshot, script, realtime, hash
report.print();                                // the hash lines
```

`gasm_host::window::run(session, opts)` opens a window with audio, gamepads and
a keyboard layout (feature `window`, on by default: winit, cpal, gilrs).
Without it (`cargo build --no-default-features`) the library and `gasm-run` are
headless-only. For your own frame loop, use `host::Game::load` and
`Game::frame` directly
([`examples/embed.rs`](https://github.com/emdzej/gasm/blob/main/runners/native/examples/embed.rs)), and plug in your
own audio output through the `audio::AudioOut` trait.

In JS, `GasmHost.runFrames(steps, show)` runs a catch-up batch (only the last
step shown), `shutdown()` calls `gasm_exit` and closes connections and storage,
and `dead` says why a guest can't be called any more. `GasmWorker.start` takes
the module as bytes or as a compiled `WebAssembly.Module`. Storage backends'
`set` throws `StorageError` with a `GASM_STORAGE_ERR_*` code. `GfxModel`
validates every `gasm:gfx` call and numbers the handles for any backend, so a
backend's methods receive the handle as their first argument and only execute
(types: [`gasm-host.d.ts`](https://github.com/emdzej/gasm/blob/main/runners/web/gasm-host.d.ts)).

## 1. Choose an engine

| Engine | Language | Notes |
|---|---|---|
| wasmtime | Rust, C API | JIT + AOT (`precompile_module`); gasm-run implements the WASI subset itself (`wasmtime-wasi` isn't used) |
| Browser `WebAssembly` | JS | Everywhere; the WASI subset is about 100 lines (`lib/wasi.js`) |
| WAMR | C | Interpreter/AOT/JIT, small footprint: embedded, iOS (AOT) |
| wasm3 | C | Pure interpreter, very portable, slowest |
| wasm2c (wabt) | C output | Translates the module to C at build time: no runtime, no JIT, works with any C toolchain (consoles) |
| wazero | Go | Pure Go, no cgo |

Requirements: wasm MVP plus sign-extension, mutable globals and bulk memory
(what rustc and clang emit by default), and `f32`/`f64` imports and exports.

## 2. Implementation steps

### Step 1: load and link

Provide imports for module `gasm` (the core functions), optionally `gasm:gfx`,
`gasm:net` and `gasm:storage`, and `wasi_snapshot_preview1`: exactly the
[WASI subset](/docs/abi#wasi-subset) (`fd_write` to your log, never to stdout;
`fd_prestat_get` answers `EBADF`; virtual clocks and a fixed `random_get`
sequence in headless runs; everything else `ENOSYS`). Both reference runners
implement the same table, so guests behave the same on both.

**Link everything else as a trap.** Guests may carry imports they never call,
e.g. wasm-bindgen glue from Rust crates that also target browsers
(`nes.wasm` has 17). wasmtime has `Linker::define_unknown_imports_as_traps`;
in JS, a `Proxy` over the import object does the same (`lib/host.js`). Unknown
WASI functions return `52` (ENOSYS) instead. `has(name)` answers `1` exactly
for the imports you really implement.

**`proc_exit(code)`** ends the game: treat it as a normal exit, not a crash
(wasmtime surfaces it as `I32Exit`; the JS shim throws `ProcExit`).

Grant nothing beyond the log: no preopened directories, env or args.

**Bound guest calls.** A guest that loops forever inside `gasm_init` or
`gasm_frame` shouldn't hang the runner. `gasm-run` traps a call that runs
longer than `--call-timeout` (30 s by default) with wasmtime's epoch
interruption.

### Step 2: start

```
if exports._initialize: call it
if exports.gasm_abi_version() != 0: refuse to run
rc = exports.gasm_init(); if rc != 0: refuse to run (print log)
```

Keep a handle to `exports.memory` and **re-derive views after each call**:
guests can grow memory, which in JS detaches old `ArrayBuffer` views.

### Step 3: implement the imports

For each import that takes guest pointers, **validate `ptr + len <=
memory.size`** and trap (throw / return an error) otherwise. Never clamp or
ignore silently. String arguments must be UTF-8; trap otherwise.

| Import | Implementation notes |
|---|---|
| `log(ptr,len)` | Decode UTF-8 lossily and print. |
| `has(name)` | `1` for the import modules and functions you implement (`"gasm:gfx"`, `"gasm:gfx.destroy"`), else `0`. |
| `time_ms()` | Monotonic ms. For deterministic mode, return `frame_index * 1000 / frame_rate`. |
| `set_frame_rate(hz)` | Accept 1–1000, ignore others. |
| `video_present(ptr,w,h,stride)` | Reject `w,h == 0`, `> 4096`, `stride < w*4`. Read `stride*(h-1) + w*4` bytes. Copy rows into your own buffer before returning. Display the **latest** frame at your own refresh rate. |
| `audio_config(rate,ch)` | Accept 8000–192000 Hz, 1–2 channels. |
| `audio_push(ptr,frames)` | Read `frames*ch*4` bytes of little-endian f32. Resample to the device rate and enqueue. |
| `input_pad(player)` | Return the mask sampled *before* this `gasm_frame`. `0` for players ≥ 4. |
| `input_mode(flags)` | Remember the flags: `KEYS_RAW` stops the keymap from feeding pads; `POINTER_HIDDEN`/`LOCKED` hide or capture the cursor (report what you achieved in the pointer flags). |
| `key_state` / `key_events` | Physical keys by `GASM_KEY_*` (the W3C `code` table in `abi.json`; `gen-abi.mjs --check` compares your table). Events: transitions only, no auto-repeat; a tap gives down and up in the same frame. A tap of Escape is a key; holding it ~1 s quits. |
| `pointer` | Position in drawable pixels, plus the frame position (undo your `video_present` letterbox with the formula in the spec), relative motion (raw device motion if you have it), wheel (~1 per notch, y > 0 down), held/pressed/released buttons. |
| `gamepad` / `gamepad_name` | First four connected devices, in the order your pads use. W3C standard mapping when you know the layout (flip stick y to point down), raw device order otherwise. |
| `text_input(dst, cap)` | Text typed since the previous frame (backspace `\b`, enter `\n`), collected before this `gasm_frame`; length, copied if it fits. `-1` without a keyboard. |
| `asset_count()` / `asset_name(i, dst, cap)` | The asset names sorted by UTF-8 bytes (compute once; the set is fixed at start-up). |
| `asset_size/read` | Flat map; `-1` if missing; copy `min(len, cap)`. `asset_size` returns `-2` for assets of 2 GiB and more. |
| `asset_size64` / `asset_read_at64` | Sizes and offsets as 64-bit integers (`i64`; `BigInt` in JS), so assets of any size work. |
| `param(name, dst, cap)` | String map (CLI `--param`, URL query); return length, copy only if it fits; `-1` if unset. |

### Step 4: the frame loop

```
next = now()
loop:
  wait until next
  steps = 0
  while now() >= next and steps < 4:
      pads = poll_input()
      exports.gasm_frame()
      next += 1 / frame_rate
      steps += 1
  if steps == 4: next = now() + 1 / frame_rate      # we fell behind: resync
  display(latest_frame)
```

Read `frame_rate` every iteration; the guest may change it. In browsers, drive
this from `requestAnimationFrame` with an accumulator, and clamp large gaps
(background tabs), as `app.js` does.

Tell the gfx layer whether the current step will be shown: `begin_frame`
returns 0 for catch-up steps, so the GPU renders only the last one. After the
steps, if the guest didn't use `gasm:gfx`, display the last `video_present`
frame.

### Step 5: GPU (`gasm:gfx`, optional)

Map the calls onto WebGPU (browser) or a WebGPU implementation (wgpu, Dawn):

1. **Handle table**: one array of objects (shader, buffer, pipeline, bind
   group, bind group layout, texture, sampler); handle = index + 1, numbered
   in creation order. Type-check every handle on use and trap on mismatch.
   Keep a small record per object (buffer size and usage, texture size and
   mip count, layout entries, a bind group's dynamic entries) and validate
   `write_texture` regions, dynamic offsets and bind groups against it, so
   your null backend traps exactly like the GPU one. Also check buffer usages
   (`VERTEX`, `INDEX`) and ranges, vertex buffer slots (0–7), bind group
   indices (0–3), index formats (0/1), that set and draw calls happen inside a
   frame, and that draws stay within their vertex and index buffers.
   `destroy(handle)` invalidates a handle for good (handles are never reused);
   objects created from it keep what they reference.
2. **Creation JSON**: parse, replace handle numbers with objects, then apply the
   runner-owned parts: `"surface"` → your swapchain format; add depth
   `depth24plus` (a no-op one if the guest omitted `depthStencil`); add your
   MSAA sample count. `layout` is `"auto"` or a list of layout handles (build a
   pipeline layout). Bind groups use the given `"layout"`, or
   `pipeline.getBindGroupLayout(group)`. Textures get usage
   `TEXTURE_BINDING | COPY_DST` and one default view (all mips).
3. **Errors**: capture validation errors (wgpu: `push_error_scope` +
   `pop`) and trap with the message; don't let them panic the runner.
4. **Frames**: `begin_frame` acquires the swapchain texture and opens a pass with
   an MSAA color target resolving into it, plus the depth buffer, both cleared.
   `end_frame` submits and presents. Resize the swapchain and attachments when
   the window changes; `width`/`height` report the current size.
5. **Colors**: prefer a non-sRGB 8-bit swapchain (`bgra8unorm`), as browsers use,
   so guests look the same everywhere.
6. **Headless**: a null backend that allocates handles and validates, but draws
   nothing. Fold every `write_buffer` payload into the video hash, and for
   `write_texture` the header (`tex, mip, x, y, w, h`, little-endian `u32`)
   followed by the payload.
7. **Viewport and scissor**: clamp to the drawable; skip draws while either is
   empty. `begin_frame` resets both to the whole drawable.

### Step 6: network (`gasm:net`, optional)

- WebSocket client per `open`, never blocking the guest: native runners use a
  thread per connection with message queues (`net.rs`); JS uses `WebSocket`
  events and a per-connection queue.
- `recv` returns the next message's length and copies only if it fits, so a
  guest can retry with a bigger buffer.
- Deny by default unless the user opts in (`--allow-net`). Log denials.
- Support `wss://` with the platform's trusted roots. The native runner uses
  rustls with OS certificates (`rustls-native-certs`, which honours `SSL_CERT_FILE`).

### Step 7: storage (`gasm:storage`, optional)

- Namespace chosen by the runner (default: game file stem); validate keys
  (`[A-Za-z0-9._-]{1,128}`, not `.`/`..`), 1 MiB per value, 16 MiB per namespace.
- `set` returns `0` or a `GASM_STORAGE_ERR_*` code: invalid key (`-1`), value
  too large (`-2`), quota (`-3`), I/O error (`-4`).
- Native: one file per key under `<data dir>/gasm/<namespace>/`, written to a
  temp file, synced and renamed ([`storage.rs`](https://github.com/emdzej/gasm/blob/main/runners/native/src/storage.rs)).
- Browser: load the namespace from IndexedDB *before* instantiating, serve
  reads from memory, persist writes asynchronously (`IdbStorage` in `lib/storage.js`).
- Headless: in-memory and empty unless told otherwise, to keep runs reproducible.
- Call the optional `gasm_exit` export when the user quits (window close, Esc held for a second,
  `pagehide`, end of a headless run), but not after `proc_exit` or a trap.

### Step 8: audio

Requirements for a good result:

1. Resample guest rate → device rate. Linear interpolation is enough for
   retro audio. `Resampler` in `lib/audio.js` and in `audio.rs` are the same
   algorithm, about 15 lines.
2. Use a ring buffer with a **target latency** (60 ms) and a **cap** (200 ms).
   On overflow, drop the oldest samples. On underrun, output silence until
   the target level is buffered again.
3. Never block the guest thread on audio (`gasm-run` hands samples to the
   device callback through a lock-free ring).

Browsers: use an `AudioWorklet` and post `Float32Array` chunks to it; this
avoids SharedArrayBuffer and the COOP/COEP headers it would need. Create or
resume the `AudioContext` from a user gesture.

### Step 9: input

Map every device to virtual pads through a keyboard layout in the shared text
format ([runner behaviour](/docs/abi#keyboard-layouts)): one binding per line,
`<pad 1-4> <button> <key code>...`. The default keeps muscle memory
consistent across runners, and gives player 2 its own keys (IJKL, `.`/`,`
etc.). Gamepads take pads in connection order, and keyboard bindings for pad
N ≥ 2 apply while fewer than N gamepads are connected. Parsers:
`parseKeymap`/`keyboardPads` in `@emdzej/gasm-host`,
[`keymap.rs`](https://github.com/emdzej/gasm/blob/main/runners/native/src/keymap.rs)
in `gasm-host`. Player-1 defaults:

| Bit | Button | Keyboard | Gamepad |
|---|---|---|---|
| 0 | A | X | East |
| 1 | B | Z | South |
| 2 | X | S | North |
| 3 | Y | A | West |
| 4/5 | L/R | Q/W | LB/RB |
| 6 | Select | Right Shift | Back/Select |
| 7 | Start | Enter | Start |
| 8–11 | Up/Down/Left/Right | Arrows | D-pad + left stick (±0.5) |

Clear held keys when the window loses focus.

### Assets: file-backed, folders, lazy providers

Don't preload. Serve `asset_size`/`asset_read`/`asset_read_at` from a provider:

- **Native:** open files at start and do positioned reads into guest memory
  ([`assets.rs`](https://github.com/emdzej/gasm/blob/main/runners/native/src/assets.rs):
  `pread`/`seek_read`, folders opened lazily on first read, case-insensitive
  index, 64-bit sizes and offsets, no panic if a file shrinks).
- **Browser:** the `GasmAssetProvider` interface
  (`size(name)`, `readAt(name, offset, dst)`, synchronous) with `AssetTable`
  for the naming rules. Lazy sources (OPFS sync access handles,
  `FileReaderSync`) exist only in workers, so run such guests with
  `GasmWorker`.
- Rules: exact names first (explicit over folder entries), then ASCII
  case-insensitive among folder entries (first in sorted order on
  collisions), with hidden entries and symlinks skipped.

### Step 10: headless mode (strongly recommended)

Add a mode that runs N frames without a display or audio device, uses
virtual time, reads scripted input, and prints:

```
frames=<N> presented=<count> size=<w>x<h>
video_fnv32=<hex8> audio_fnv32=<hex8> audio_frames=<count>
```

The hashes are FNV-1a 32-bit (offset `0x811c9dc5`, prime `0x01000193`),
folded over, in call order:
- video hash: each presented frame's **tightly packed** RGBA rows (`w*4`
  bytes per row, stride padding excluded), and each `gfx.write_buffer` payload;
- audio hash: each `audio_push` payload's raw bytes.

WASI clocks follow the same virtual time, and `random_get` returns a fixed
sequence (splitmix64 from 0). Also support `--param`, `--allow-net` (network
tests) and a clean exit on `proc_exit`, with the hash lines still printed.
Guest output (WASI stdout) goes to the log, never to stdout with the hashes.

A runner is conformant when its output matches `gasm-run --headless` for the
cases in `scripts/determinism-test.sh` (the expected lines are in
[`tests/golden/determinism.txt`](https://github.com/emdzej/gasm/blob/main/tests/golden/determinism.txt)).

## 3. Conformance checklist

- [ ] Loads `build/test-pattern.wasm`, `build/nes.wasm` (with a ROM), `build/triangle.wasm`, `build/textured.wasm`, `build/sumo.wasm`
- [ ] Rejects a guest whose `gasm_abi_version` isn't 0
- [ ] Traps (doesn't crash) on out-of-bounds pointers, invalid handles and non-UTF-8 strings
- [ ] `has` answers for exactly the imports you implement
- [ ] Calls `gasm_frame` at the guest's rate, independent of display refresh
- [ ] Resamples audio; no drift-induced buffer growth over 10+ minutes
- [ ] Keyboard + gamepad mapped as above
- [ ] Unknown imports link as traps; `proc_exit(0)` ends cleanly; `gasm_exit` is called on user quit
- [ ] Storage persists across runs (e.g. sumo's `record-bot`, NES battery saves)
- [ ] Folder assets resolve case-insensitively and `scripts/asset-test.sh` hashes match (`assetcheck.wasm`)
- [ ] Keyboard layouts load from the shared format; pad-2 keys yield to a second gamepad
- [ ] Headless hashes match `gasm-run` for every case in `scripts/determinism-test.sh`
- [ ] With `gasm:net`: a headless sumo peer on your runner reaches the same final
      state as a `gasm-run` peer through `gasm-relay` (`scripts/net-test.sh`)

## 4. Platform notes

- **iOS / consoles:** JIT isn't allowed. Use wasmtime AOT (`.cwasm`),
  WAMR AOT, or wasm2c. The guest `.wasm` stays the same.
- **Microcontrollers (ESP32-S3, RP2350):** WAMR or wasm3. Watch memory: the
  NES game grows to about 3 MiB of linear memory at runtime (sumo about 1 MiB).
  Present to an SPI display at a reduced rate; no `gasm:gfx` needed for 2D games.
- **Retro handhelds / Linux SBCs:** the native runner should build as is
  (winit supports X11/Wayland, wgpu Vulkan/GL, cpal ALSA/PulseAudio; install
  `libasound2-dev libudev-dev` for the build).
- **Embedding in an app:** the ABI has no global state, so several guests
  can run side by side in separate instances.
