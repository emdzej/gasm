# Changelog

What each release added, for guest authors (which runner a feature needs) and
embedders. The ABI version is still 0: additions keep it (see
[ABI.md, Versioning](https://github.com/emdzej/gasm/blob/main/spec/ABI.md#gasm-abi-v0));
guests can probe for newer imports with `gasm.has`. Versions are the git tags and
the package versions (`gasm-sdk`, `gasm-host`, `@emdzej/gasm-host`).

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
