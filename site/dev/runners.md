# Writing runners

A runner makes every gasm game available on a new platform: a new OS, a
console, a microcontroller, a different wasm engine, or an embedding inside an
existing app. The contract is the [ABI spec](/docs/abi). Use the two
existing runners as reference implementations:

| | Native (Rust) | Web (JS) |
|---|---|---|
| core ABI + WASI | [`host.rs`](https://github.com/emdzej/gasm/blob/main/runners/native/src/host.rs) (wasmtime) | [`gasm-host.js`](https://github.com/emdzej/gasm/blob/main/runners/web/gasm-host.js) (~310 lines incl. WASI shim, net) |
| `gasm:gfx` | [`gfx.rs`](https://github.com/emdzej/gasm/blob/main/runners/native/src/gfx.rs) (wgpu, ~770 lines incl. 2D blit) | [`webgpu-gfx.js`](https://github.com/emdzej/gasm/blob/main/runners/web/webgpu-gfx.js) (~130 lines) |
| `gasm:net` | [`net.rs`](https://github.com/emdzej/gasm/blob/main/runners/native/src/net.rs) (tungstenite threads) | `NetConnections` in `gasm-host.js` (WebSocket) |

To embed rather than write a runner, use the published hosts:
[`@emdzej/gasm-host`](/dev/packages#for-runners-and-embedders) (browser/Node) or
the [`gasm-host`](https://crates.io/crates/gasm-host) crate (native). The
machine-readable ABI ([`spec/abi.json`](https://github.com/emdzej/gasm/blob/main/spec/abi.json))
lists every function to implement; `scripts/gen-abi.mjs --check` verifies the
reference runners against it.

A minimal runner implements only the core `gasm` module (11 functions).
`gasm:gfx` and `gasm:net` are optional: games that don't import them run
anyway.

## 1. Choose an engine

| Engine | Language | Notes |
|---|---|---|
| wasmtime | Rust, C API | JIT + AOT (`precompile_module`), full WASI p1 via `wasmtime-wasi` |
| Browser `WebAssembly` | JS | Everywhere; write your own tiny WASI shim |
| WAMR | C | Interpreter/AOT/JIT, small footprint: embedded, iOS (AOT) |
| wasm3 | C | Pure interpreter, very portable, slowest |
| wasm2c (wabt) | C output | Translates the module to C at build time: no runtime, no JIT, works with any C toolchain (consoles) |
| wazero | Go | Pure Go, no cgo |

Requirements: wasm MVP plus sign-extension, mutable globals and bulk memory
(what rustc and clang emit by default), and `f32`/`f64` imports and exports.

## 2. Implementation steps

### Step 1: load and link

Provide imports for module `gasm` (the core functions), optionally `gasm:gfx`
and `gasm:net`, and `wasi_snapshot_preview1` (at least `fd_write`, `fd_close`,
`fd_seek`, `fd_fdstat_get`, `clock_time_get`, `random_get`, `args_*`,
`environ_*`, `proc_exit`).

**Link everything else as a trap.** Guests may carry imports they never call,
e.g. wasm-bindgen glue from Rust crates that also target browsers
(`nes.wasm` has 17). wasmtime has `Linker::define_unknown_imports_as_traps`;
in JS, a `Proxy` over the import object does the same (`gasm-host.js`). Unknown
WASI functions can return `52` (ENOSYS) instead.

**`proc_exit(code)`** ends the game: treat it as a normal exit, not a crash
(wasmtime surfaces it as `I32Exit`; the JS shim throws `ProcExit`).

Grant nothing beyond stdout/stderr: no preopened directories, env or args.

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
ignore silently.

| Import | Implementation notes |
|---|---|
| `log(ptr,len)` | Decode UTF-8 lossily and print. |
| `time_ms()` | Monotonic ms. For deterministic mode, return `frame_index * 1000 / frame_rate`. |
| `set_frame_rate(hz)` | Accept 1–1000, ignore others. |
| `video_present(ptr,w,h,stride)` | Reject `w,h == 0`, `> 4096`, `stride < w*4`. Read `stride*(h-1) + w*4` bytes. Copy rows into your own buffer before returning. Display the **latest** frame at your own refresh rate. |
| `audio_config(rate,ch)` | Accept 8000–192000 Hz, 1–2 channels. |
| `audio_push(ptr,frames)` | Read `frames*ch*4` bytes of little-endian f32. Resample to the device rate and enqueue. |
| `input_pad(player)` | Return the mask sampled *before* this `gasm_frame`. `0` for players ≥ 4. |
| `asset_size/read` | Flat map; `-1` if missing; copy `min(len, cap)`. |
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

1. **Handle table**: one array of objects (shader, buffer, pipeline, bind group);
   handle = index + 1. Type-check every handle on use and trap on mismatch.
2. **Creation JSON**: parse, replace handle numbers with objects, then apply the
   runner-owned parts: `"surface"` → your swapchain format; add depth
   `depth24plus` (a no-op one if the guest omitted `depthStencil`); add your
   MSAA sample count; `layout: "auto"`. Bind groups use
   `pipeline.getBindGroupLayout(group)`.
3. **Errors**: capture validation errors (wgpu: `push_error_scope` +
   `pop`) and trap with the message; don't let them panic the runner.
4. **Frames**: `begin_frame` acquires the swapchain texture and opens a pass with
   an MSAA color target resolving into it, plus the depth buffer, both cleared.
   `end_frame` submits and presents. Resize the swapchain and attachments when
   the window changes; `width`/`height` report the current size.
5. **Colors**: prefer a non-sRGB 8-bit swapchain (`bgra8unorm`), as browsers use,
   so guests look the same everywhere.
6. **Headless**: a null backend that allocates handles and validates, but draws
   nothing. Fold every `write_buffer` payload into the video hash.

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
- Native: one file per key under `<data dir>/gasm/<namespace>/`, written to a
  temp file and renamed ([`storage.rs`](https://github.com/emdzej/gasm/blob/main/runners/native/src/storage.rs)).
- Browser: load the namespace from IndexedDB *before* instantiating, serve
  reads from memory, persist writes asynchronously (`IdbStorage` in `gasm-host.js`).
- Headless: in-memory and empty unless told otherwise, to keep runs reproducible.
- Call the optional `gasm_exit` export when the user quits (window close, Esc,
  `pagehide`, end of a headless run), but not after `proc_exit` or a trap.

### Step 8: audio

Requirements for a good result:

1. Resample guest rate → device rate. Linear interpolation is enough for
   retro audio. `Resampler` in `gasm-host.js` and `push_le_f32` in
   `audio.rs` are the same algorithm, about 15 lines.
2. Use a ring buffer with a **target latency** (60 ms) and a **cap** (200 ms).
   On overflow, drop the oldest samples. On underrun, output silence until
   the target level is buffered again.
3. Never block the guest thread on audio.

Browsers: use an `AudioWorklet` and post `Float32Array` chunks to it; this
avoids SharedArrayBuffer and the COOP/COEP headers it would need. Create or
resume the `AudioContext` from a user gesture.

### Step 9: input

Map every device to virtual pads. Recommended mapping (keeps muscle memory
consistent across runners):

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

Also support `--param`, `--allow-net` (network tests) and a clean exit on
`proc_exit`, with the hash lines still printed.

A runner is conformant when its output matches `gasm-run --headless` for the
cases in `scripts/determinism-test.sh`.

## 3. Conformance checklist

- [ ] Loads `build/test-pattern.wasm`, `build/nes.wasm` (with a ROM), `build/triangle.wasm`, `build/sumo.wasm`
- [ ] Rejects a guest whose `gasm_abi_version` isn't 0
- [ ] Traps (doesn't crash) on out-of-bounds pointers
- [ ] Calls `gasm_frame` at the guest's rate, independent of display refresh
- [ ] Resamples audio; no drift-induced buffer growth over 10+ minutes
- [ ] Keyboard + gamepad mapped as above
- [ ] Unknown imports link as traps; `proc_exit(0)` ends cleanly; `gasm_exit` is called on user quit
- [ ] Storage persists across runs (e.g. sumo's `record-bot`, NES battery saves)
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
