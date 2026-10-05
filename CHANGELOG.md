# Changelog

What each release added, for guest authors (which runner a feature needs) and
embedders. The ABI version is still 0: additions keep it (see
[ABI.md, Versioning](https://github.com/emdzej/gasm/blob/main/spec/ABI.md#gasm-abi-v0));
guests can probe for newer imports with `gasm.has`. Versions are the git tags and
the package versions (`gasm-sdk`, `gasm-host`, `@emdzej/gasm-host`).

## 0.10.0 (2026-10-05)

Website:
- The home page's background is a gasm game: `guests/bricks`, a brick breaker
  that plays itself in gasm's colours (26 KB), running on `@emdzej/gasm-host`
  behind the hero. It takes the hero's shape, pauses off screen and shows a still
  frame with reduced motion.

SDKs:
- SDL 3: OpenGL ES on `gasm:gl` for programs linked with `lib/gasm_gl.o`
  (CMake `SDL3::GL`): `SDL_GL_CreateContext` (ES up to 3.0), `SDL_GL_GetProcAddress`,
  `SDL_GL_SwapWindow`, GL windows at the drawable's size and following it, and
  `SDL_Renderer` on SDL's GLES 2 renderer (on the GPU). SDL itself doesn't import
  `gasm:gl` (it finds GL through a weak `gasm_gl_get_proc_address`), so programs
  without it are unchanged, hashes included. Examples: `sdk/sdl3/examples/gl`,
  SDL's snake on GLES 2 (`sdl3-snake-gl`); both are determinism cases.
- C SDK, GLES: `gasm_gl_get_proc_address(name)` (`src/gasm_gl_proc.c`, generated)
  for loaders; client-side vertex and index arrays (GLES 2 code, SDL's GLES 2
  renderer) are copied into buffers at the draw.

Runners:
- gasm:gl is much cheaper per call (NiP #10). The model kept vertex array
  attributes in one table that every draw and delete scanned, so Godot, which
  makes and frees a buffer and a vertex array per 2D polygon each frame, slowed
  down with the square of its draw count. Each vertex array now has its own
  record and buffers know which arrays use them. Natively, imports take their
  arguments without a heap allocation and the model hashes with FxHash; in JS
  the imports are bound methods. Godot drawing 660 polylines a frame: 22 -> 97
  fps on ANGLE (gasm-run), 19 -> 309 frames/s null GL natively, 9 -> 226 in Node.
  Hashes are unchanged.
- `gasm-run --gl-stats`: gasm:gl calls a frame, by name, printed when the game
  ends.
- `--app-id <text>` (`gasm-run`, `gasm-headless`): gasm:fetch requests identify
  the game, `User-Agent: <text> gasm-run/<version>` (NiP #8). Without it the
  runner sends `gasm-run/<version>` (`gasm-headless/<version>` in Node);
  browsers keep their own. `GasmHost` takes it as `userAgent` (Node only).
- Audio keeps playing when the game drops below its frame rate or the output
  device changes (NiP #12). Natively the stream is rebuilt when it fails or the
  default device changes (checked once a second; a macOS system sound could
  stall it), and the buffer grows from 60 ms by 30 ms steps, up to 150 ms, after
  repeated underruns. `gasm_gl.frame_shown` now answers 0 only during catch-up
  frames on both runners (1 headless).
- Godot: audio is mixed by elapsed time instead of one frame's worth per frame,
  so sound no longer stutters or stops when the game runs below 60 fps; it skips
  drawing on catch-up frames (`frame_shown`).
- Guest memory is capped: 1 GiB by default, `--memory-limit <MiB>` (0: none) on
  both runners, `memoryLimit` for `GasmHost` and `GasmWorker`. Natively a
  growth past it traps (a wasmtime resource limiter); browsers can't refuse a
  guest's own growth, so the JS runner checks after each guest call and traps
  with the same message. `gasm-host`: `LoadOptions::memory_limit`.
- gasm:gl model (both runners): a draw with an enabled attribute that has no
  buffer is `INVALID_OPERATION`, and attribute indices from 16 are
  `INVALID_VALUE`, as in WebGL (Chrome reported them, the null GL didn't).
- The gasm splash screen: a moment of Pong in the style of gasm's icon that
  turns into the logo and the name, about 1.6 s, while the game loads (natively
  the module compiles meanwhile, so a slow-to-compile game like Godot starts no
  later than before). The same frames on both runners
  (`runners/native/src/splash.rs`, `lib/splash.js`, compared by `gen-abi.mjs
  --check`), shown like 2D frames. A key or click shortens it; off with
  `gasm-run --no-splash` and the player's `?nosplash`, and never in headless runs
  or `--window-screenshot`. `@emdzej/gasm-host` exports `splashFrame` for pages
  that embed games. `gasm-host`: `Game::compile` / `load_module` and
  `Session::compile_in_background` / `start_compiled` split compiling from
  starting.

## 0.9.1 (2026-10-05)

Games:
- Godot: resizing the window (or the browser canvas) resizes the game. The gasm
  platform never told Godot the drawable's size had changed, so games kept
  rendering at their starting size in a corner of a larger window.

## 0.9.0 (2026-10-04)

ABI (additions, ABI version still 0):
- `gasm:fetch` (optional module): HTTP(S) requests made by the runner, TLS
  included, polled per frame: `request` (JSON method/URL/headers + body),
  `state`, `status`, `headers`, `read` (the body as it arrives), `close`. Both
  runners (natively ureq on rustls/ring with the OS certificates; in JS
  `fetch()`), the same refusals everywhere (forbidden headers, methods, URLs),
  16 requests, 64 MiB bodies. Off natively unless `--allow-net`;
  `--allow-net=host,*.domain` limits gasm:fetch and gasm:net to those hosts
  (every redirect hop is checked). Headless `--fetch-record DIR` /
  `--fetch-replay DIR` make runs reproducible (both runners, one format).
  Rust `gasm::fetch::Request`; Godot: `HTTPRequest`/`HTTPClient` work (a gasm
  `HTTPClient`), `https://` too. Design: design/fetch.md. Asked for by Nowhere
  in Particular.
- `gasm.utc_offset_minutes()`: the player's time zone, minutes east of UTC now
  (daylight saving included); 0 in headless runs. Both runners; Rust
  `gasm::utc_offset_minutes()` (0 on older runners), C `gasm_utc_offset_minutes()`
  (probe `gasm.has` first). Godot's `Time.get_datetime_*_from_system()` and
  `get_time_zone_from_system()` now give local time and the offset instead of UTC.

- `gasm.asset_version(name)`: assets the embedder replaces while the game runs
  get a new version (0 as launched). Embedders: `Game::set_asset` /
  `remove_asset` (`gasm-host`), `GasmHost.setAsset` / `removeAsset`,
  `GasmWorker.setAsset` (`@emdzej/gasm-host`). Launchers:
  `gasm-run --watch-asset name=path` (and the Node runner) re-reads a file
  whenever it changes, in place or by a rename. Rust `gasm::asset_version`,
  C `gasm_asset_version`; Godot: `FileAccess.get_modified_time("res://...")`.
  `gasm-host`: `Session` has a new field, `watch_assets`. Asked for by Nowhere in
  Particular.

SDKs:
- Rust: `gasm::net::Conn::open` returned `None` for a denied connection only
  after closing handle -1, which traps: a game without `--allow-net` crashed
  instead of seeing `None`.

Runners:
- Browser runner, gasm:gl: pixels that aren't bytes reach WebGL as the typed
  array it requires (`Float32Array` for `FLOAT`, `Uint16Array` for `HALF_FLOAT`
  and packed 16-bit types, `Uint32Array`, `Int*Array`), copied when the guest's
  pointer isn't aligned for it, in `tex(Sub)Image2D/3D` and `readPixels`. WebGL
  rejected them as bytes ("type FLOAT but ArrayBufferView not Float32Array"), so
  float textures (Godot's `FORMAT_RF`, half floats) sampled zeros in browsers.
  Reported from Nowhere in Particular.
- gasm:gl model (both runners): binding a texture to a target other than its
  first one is `INVALID_OPERATION`, as in GLES and WebGL (before, only WebGL
  reported it, so Chrome's hashes differed).
- gasm:gl null GL (headless, natively and in Node): reports every WebGL 2
  minimum limit. `GL_MAX_VERTEX_OUTPUT_COMPONENTS` and
  `GL_MAX_FRAGMENT_INPUT_COMPONENTS` were 0, so Godot rejected every shader
  with a `varying` in headless runs ("Too many varyings"); the uniform-block,
  uniform-component, texel-offset, LOD-bias and element-index limits were 0
  too, and `0x8C8A` answered 4 (it is
  `MAX_TRANSFORM_FEEDBACK_INTERLEAVED_COMPONENTS`, 64; `SEPARATE_ATTRIBS` is
  `0x8C8B`). `gen-abi.mjs --check` now compares the native and JS tables.
  Hashes of guests that query these limits change.

Games:
- Godot scene3d example: a custom spatial shader with a `varying`. The
  determinism suite also fails when a Godot example logs an error on either
  null GL.

## 0.8.0 (2026-10-04)

Games:
- Godot 4.7 (guests/godot): a gasm platform for Godot (MIT), so Godot games
  exported as a `.pck` run on every runner with the Compatibility renderer on
  gasm:gl: `gasm-run godot.wasm --asset game.pck=mygame.pck`, or the player
  (open or drop a `.pck`). GDScript, 2D and 3D, Godot Physics, audio (mixed per
  frame), keyboard, text, mouse, gamepads, `user://` on gasm:storage. Five example
  projects (hello2d, platformer, scene3d, ui, audio) are determinism cases and
  demos. `make godot` builds the engine (wasi-sdk, LTO, `wasm-opt -Oz`: 32 MB,
  8 MB gzipped) and exports the examples with the Godot editor
  (`scripts/fetch-godot.sh` pins the source, the editor and SCons). The release
  bundles have `godot.wasm`, the example packs and `run-godot`.
- C SDK: `gasm_key_name(code)` in `gasm.h` (the W3C name of a key code).

SDKs:
- Rust: OpenGL ES through glow. `gasm::gles` is the GLES 3.0 C API on gasm:gl
  (the same functions as the C SDK's `gasm_gl.c`), and `sdk/glow` is glow 0.17
  with its native backend on wasm32: with
  `[patch.crates-io] glow = { git = "https://github.com/emdzej/gasm" }`,
  `glow::Context::from_loader_function_cstr(gasm::gles::get_proc_address)`
  works, and so do crates built on glow, such as egui_glow (`guests/eguidemo`:
  egui's demo, unchanged). Examples and determinism cases: `glowtest`, `eguidemo`.
- Rust: cooperative threads. `gasm::thread` (`spawn`, `Builder::stack_size`,
  `JoinHandle::join`/`is_finished`, `yield_now`, `sleep`, `wait_frame`) and
  `gasm::sync` (`Mutex`, `Condvar` with timeouts, `Semaphore`) on the same
  deterministic scheduler as the C SDK, with `gasm::threaded_main_loop!` (an
  Asyncify build, as for `main_loop!`). Closures needn't be `Send`. `gasm-sdk`
  now has a build script: on wasm32 it links a 360-byte helper that reads and
  sets the wasm stack pointer (stable Rust can't). Test: `guests/rthreadtest`.
- Rust: a game no longer has to be `Send` on wasm32 (guests are
  single-threaded), so it can hold `Rc`, egui state and the like.

Runners:
- Native `gasm:gl`: gasm-run draws OpenGL ES games with ANGLE (Metal on macOS,
  Direct3D 11 on Windows, Vulkan on Linux, SwiftShader without a GPU or with
  `--gl-software`), in a WebGL compatibility context: the validation Chrome
  uses for WebGL 2. Calls pass the same model as in the browser first, so GL
  errors and hashes don't change. ANGLE is loaded at run time from next to
  gasm-run (`--gl-lib DIR`, `$GASM_ANGLE_DIR`); the release bundles ship it
  (from Electron 43.7.7, licenses in `ANGLE-NOTICES.txt`), and
  `scripts/fetch-angle.sh` fetches it for builds from the repository.
- `GASM_ANGLE_BACKEND` (metal, opengl, vulkan, d3d11) picks ANGLE's backend; on a
  macOS virtual machine's paravirtual GPU (no Metal argument buffers) gasm-run
  uses ANGLE's OpenGL backend by itself.
- Headless `--screenshot` renders `gasm:gl` games with ANGLE offscreen.
  `--window-screenshot <frames>:<out.png>` writes a frame as the window shows
  it and quits (gasm:gl games; for tests).
- Linux: gasm:gl games open X11 windows (XWayland on Wayland desktops).
- `gasm-host` library: `Session::start` takes the GL backend
  (`Option<angle::Angle>`; `Session::open_gl` makes one), `Session` has
  `gl_lib` and `gl_software`.
- The release bundles include `gltest.wasm` (`run-gltest`).

## 0.7.0 (2026-10-03)

ABI (additive):
- Stack switching: an optional export `gasm_run() -> i32` and the import
  `gasm.yield_frame()`. Runners that can suspend a wasm stack (natively with
  wasmtime's async calls; JSPI in Chromium and Node 24+) run such a guest's
  whole loop in one call and resume it every frame; the frames, input and
  hashes are those of `gasm_frame`. Runners without it keep calling
  `gasm_frame`.
- `gasm:gl`: OpenGL ES 3.0 with WebGL 2's rules (224 functions), an optional
  module next to `gasm:gfx` (a game imports one of them). The browser runner
  forwards it to WebGL 2; headless runs (native and Node) use a null GL that
  tracks names, bindings and the pixel store, reports the same GL errors and
  hashes every buffer, texture and uniform upload (ABI.md, `gasm:gl`).
  The native window refuses `gasm:gl` games until ANGLE lands (roadmap).

SDKs:
- Cooperative threads for games with their own loop (C SDK, `gasm_thread.h`):
  threads, mutexes, conditions, semaphores, thread-local keys, deterministic
  scheduling on the guest's one wasm thread (design/threads.md). Build with
  `gasm_add_game(<target> LOOP THREADS ...)` (gasm_loop.c with
  `-DGASM_LOOP_THREADS`); Asyncify builds only.
- POSIX threads on the same scheduler (`sdk/c/src/gasm_pthread.c`, added by
  `LOOP THREADS`): pthread code builds unchanged against wasi-libc's headers
  (threads, mutexes, conditions, rwlocks, once, keys, semaphores, spinlocks,
  `nanosleep`).
- SDL 3: threads work (`SDL_CreateThread`, `SDL_AddTimer`, mutexes,
  conditions, semaphores, read/write locks, TLS) on those cooperative threads,
  in classic `main()` apps linked with the threaded loop helper
  (`gasm_sdl3_app(<target> LOOP THREADS)`, `lib/gasm_loop_threads.o`). The SDL
  CMake helper now also writes `<target>-run.wasm` for `LOOP` apps.
- C SDK: drop-in `<GLES3/gl3.h>` / `<GLES2/gl2.h>` and `sdk/c/src/gasm_gl.c`
  (CMake: `${GASM_GL_SOURCE}`), generated from the Khronos registry: the whole
  GLES 3.0 API on `gasm:gl`, including string arrays, `glGetString` caching,
  exact upload lengths from the pixel store and `glMapBufferRange` emulated in
  guest memory. `guests/gltest` is the example and a determinism case.
- The loop helpers (`gasm_loop.h`, `gasm::main_loop!`) export `gasm_run` too.
  Own-loop games now build twice from one link: `game.wasm` with Asyncify
  (every runner) and `game-run.wasm` without (stack switching only): ScummVM
  10.6 MB instead of 16.1 MB, SDL 3 classic 0.81 MB instead of 1.15 MB. CMake's
  `gasm_add_game(... LOOP ...)` writes both.

Runners:
- Native: guests that export `gasm_run` run that way (`--no-stack-switching`
  forces `gasm_frame`); a run build is refused with a message when switching is
  off. `Game::with_host` reaches the host between frames for every guest
  (`host()`/`host_mut()` don't work while a `gasm_run` guest is suspended);
  `Host` is `Send` (`AudioOut: Send`; `AudioSink::open` returns the cpal
  stream separately). Loading ScummVM's run build peaks at 506 MB RSS instead
  of 904 MB for the Asyncify build.
- `@emdzej/gasm-host`: `GasmHost.frameAsync()` / `runFramesAsync()` (required
  for gasm_run guests, `host.switching`), `STACK_SWITCHING`, the
  `stackSwitching` option; Worker mode and `gasm-headless`
  (`--no-stack-switching`) use them. The player loads the run builds where the
  browser has JSPI (`?asyncify` forces the Asyncify build).
- Player: `gasm:gl` games get a WebGL 2 canvas at display size (main thread;
  Worker mode is a roadmap item).
- Native release bundles ship ScummVM's run build (`games/scummvm-run.wasm`).
- CI runs on Node 24.

## 0.6.0 (2026-10-02)

ABI (additive):
- `gasm.has(name)`: does the runner provide an import module or function.
- `gasm.set_title(title)`: name the window or browser tab (`<title> — gasm`;
  control and bidi characters removed, at most 256 bytes; headless runs log
  it). SDKs: `gasm::set_title`, `gasm_set_title_str`, no-ops on older runners.
  Used by DOOM, ScummVM and SDL 3 (`SDL_SetWindowTitle`).
- Custom section `gasm.title`: a game's built-in name, the default window
  title (and readable by launchers without running the game). SDKs:
  `gasm::title!("Sumo")`, `GASM_TITLE("Sumo")`. The demo games have one.
- `gasm.video_set_aspect(num, den)`: show frames at a display aspect instead of
  square pixels; the pointer's frame position follows. SDKs:
  `gasm::video_set_aspect`, `gasm_video_aspect` (return false/0 on older
  runners). DOOM now presents its 320×200 at 4:3 (a quarter of the bytes per
  frame) and ScummVM 320×200/640×400 games at 4:3, instead of scaling
  themselves; both still scale on older runners. Their video hashes changed.
- `gasm.asset_size64`, `gasm.asset_read_at64`: assets of any size (the 2 GiB limit
  is gone); `asset_size` reports assets of 2 GiB and more as `-2`.
- `gasm:gfx.destroy(handle)`: release GPU objects (handles are never reused).
- `gasm:storage.set` returns `GASM_STORAGE_ERR_KEY`/`_SIZE`/`_QUOTA`/`_IO`
  instead of `-1` for every failure.
- Constants for the `pointer`/`gamepad` field offsets (`GASM_POINTER_OFF_*`,
  `GASM_GAMEPAD_OFF_*`) and `GASM_KEY_EVENT_BYTES`.

Stricter (both runners, identically; previously accepted or a crash):
- gfx: wrong handle kinds and destroyed handles, buffer ranges and usages
  (`VERTEX`/`INDEX`), vertex buffer slots (0–7) and bind group indices (0–3),
  index formats other than 0/1, set/draw calls outside a frame, draws without a
  pipeline or reading past their vertex/index buffers, bind groups that don't
  match the pipeline, invalid pipeline enums. Natively a vertex/index buffer
  offset past the end used to crash the runner.
- net: handles `open` never returned trap; a closed handle reports `CLOSED`.
  At most 16 connections, bounded send/receive queues, connect timeouts.
- String arguments must be UTF-8.

Behaviour:
- Headless virtual time is monotonic when the guest changes its frame rate, and
  fixed within a frame on every runner. WASI clocks are virtual and `random_get`
  is a fixed sequence in headless runs.
- Both runners implement the same WASI subset (`wasmtime-wasi` is gone):
  `fd_prestat_get` answers `EBADF`, unknown functions `ENOSYS`; guest stdout goes
  to the log, never to the runner's stdout (the hash lines).
- Native: a precompiled `.cwasm` needs `--allow-precompiled`; a guest call that
  runs longer than `--call-timeout` (default 30 s) traps; folder assets are opened
  lazily; storage writes are synced and their temp files can't collide with keys;
  audio uses a lock-free ring and any device sample format; the mouse is released
  when the window loses focus.
- Upscaling filters for 2D frames: `sharp` (the new default: exactly nearest
  neighbour at whole factors, even pixels at other factors), `nearest`, `xbr`
  (edge-directed, for pixel art), `fsr` (AMD FSR 1: EASU upscaling + RCAS
  sharpening, for rendered or dithered content), `crt` (scanlines and an
  aperture grille), and integer scaling. Native:
  `--filter`, `--integer-scale`, `--screenshot-filtered` (saves what the window
  shows). Browser: `@emdzej/gasm-host/present` (`GlPresenter`, WebGL 2), in the
  player as `?filter=`/`?integer` and header controls. Both runners render the
  same pixels (`scripts/present-test.mjs`, with golden images of the test
  pattern in `tests/golden/present/`). Frames larger than the output are
  now shrunk with linear filtering. Display only: hashes don't change. With
  integer scaling the pointer's frame position follows the smaller letterbox
  (`framePosition`/`frame_position` take the flag; `letterbox` is exported).
- `gasm-relay` is its own crate (`runners/native/relay`): connection and queue
  limits, handshake timeouts, LEAVE ordered with JOIN.
- `gasm-host` (crate): runners in the library (`headless::run`, `window::run`),
  feature `window` (default) for winit/cpal/gilrs, `audio::AudioOut` trait.
- `@emdzej/gasm-host`: split into `lib/` modules; `GasmHost.runFrames`,
  `shutdown`, `dead`; storage `set` throws `StorageError`; GfxModel validates and
  numbers handles for every backend; `GasmWorker.start` takes a compiled
  `WebAssembly.Module`; `headless.mjs` has `--storage-dir`/`--storage-id` and
  rejects unknown options.
- Rust SDK: `gasm::has`, `asset_size`, 64-bit `asset_read_at`, `gfx::destroy`,
  `storage::try_set`; `main_loop!(run, on_exit)`; ordinary games no longer carry
  the main-loop export and Asyncify imports.
- C SDK: `gasm_vfile.h` (`FILE*` over assets and storage), used by DOOM and SDL 3.
- ScummVM reads assets through a 16 KB read-ahead buffer.
- Tests: golden hashes for the determinism suite on every CI platform
  (`tests/golden/determinism.txt`); releases run CI and rebuild the GPL games
  from their source archives before publishing. Every download is checksum-pinned.

## 0.5.0 (2026-10-01)

- Raw input: keyboard (`key_state`, `key_events`, `GASM_KEY_*`), pointer,
  gamepads and joysticks (`gamepad`, `gamepad_name`), `input_mode`; hold Escape
  to quit; storage listing (`storage.count`, `storage.key`).

## 0.4.0 (2026-09-30)

- `gasm:gfx`: textures, samplers, explicit bind group layouts, dynamic offsets,
  viewport and scissor; `text_input`; gfx games in Worker mode.
- DOOM (doomgeneric) with OPL music and saves; the "Built for gasm" badge.

## 0.3.0 (2026-09-30)

- File-backed assets, asset folders (case-insensitive names), `asset_count` and
  `asset_name`; Worker mode with OPFS and File assets; keyboard layouts.

## 0.2.0 (2026-09-30)

- Packages: `gasm-sdk`, `gasm-host` (crates.io), `@emdzej/gasm-host` (npm), C
  SDK; machine-readable ABI (`spec/abi.json`); TLS (`wss://`, `gasm-relay
  --tls-cert`).

## 0.1.0 (2026-09-30)

- The core ABI (video, audio, pads, assets, params), `gasm:gfx`, `gasm:net`,
  `gasm:storage`, `asset_read_at`, `gasm_exit`; native, browser and Node runners.
