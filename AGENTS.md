# AGENTS.md

Guidance for coding agents working in this repository. Humans: see
[README.md](README.md) and [gasm.emdzej.pl](https://gasm.emdzej.pl).

## What this is

gasm is a portable game runtime on WebAssembly. **Guests** (games) are single
`.wasm` modules. **Runners** (native Rust, browser JS, headless Node) implement
the ABI in [`spec/ABI.md`](spec/ABI.md): the core `gasm` module plus optional
`gasm:gfx` (WebGPU subset), `gasm:gl` (OpenGL ES 3.0, WebGL 2 rules: WebGL 2 / ANGLE / null GL), `gasm:net` (WebSocket-style messages),
`gasm:fetch` (HTTP made by the runner), `gasm:storage` (per-game key/value),
`gasm:clipboard` (text; paste only in the paste key's frame) and `gasm:files`
(files saved for the player). Determinism across runners is the core
property: most tests assert bit-identical hashes.

## Layout

| Path | What |
|---|---|
| `spec/abi.json` | **Machine-readable ABI, source of truth.** `node scripts/gen-abi.mjs` regenerates `spec/gasm.h` + `guests/gasm/src/sys.rs`; `--check` verifies runners |
| `spec/ABI.md`, `spec/gasm.h` | Normative prose + generated C header (don't edit `gasm.h` or `sys.rs` by hand) |
| `CHANGELOG.md` | What each release added (guest authors: which runner a feature needs). Add to "Unreleased" with every ABI or behaviour change |
| `sdk/c/` | C/C++ SDK: CMake toolchain (wraps wasi-sdk) + `Gasm.cmake` + examples; `gasm_vfile.h` (`FILE*` over assets and storage, used by DOOM and SDL 3) |
| `guests/` | Rust workspace (`wasm32-unknown-unknown`): `gasm` (SDK + native stub host), `sumo`, `nes` (tetanes-core), `triangle`, `textured` (textures/layouts/offsets test), `inputtest` (raw input tester), `loopdemo` (`gasm::main_loop!`), `assetcheck` (asset providers; `watch=name` for replaced assets), `fetchtest` (gasm:fetch), `bricks` (the website's background, a self-playing brick breaker: `site/.vitepress/theme/components/BricksBackground.vue` runs it from `/play/`), `parity` (native harness) |
| `guests/test-pattern/` | C guest (wasi-sdk) |
| `guests/threadtest/` | C guest: cooperative threads (`gasm_thread.h`), deterministic schedule; `mode=many`, `mode=deadlock` |
| `guests/rthreadtest/` | Rust guest: `gasm::thread` / `gasm::sync` on the same scheduler (`threaded_main_loop!`); `mode=many`, `mode=deadlock` |
| `guests/pthreadtest/` | C guest: plain POSIX threads code on the same scheduler (`sdk/c/src/gasm_pthread.c`) |
| `sdk/c/src/gasm_loop.c`, `sdk/c/include/gasm_loop.h` | loop helper for games with their own main loop (`gasm_main` + `gasm_wait_frame`): exports `gasm_frame` (Asyncify inside the guest) and `gasm_run` (the runner switches stacks); Rust: `gasm::main_loop!`. Used by ScummVM and SDL3 classic `main()`; each builds as `game.wasm` (Asyncify) and `game-run.wasm` (without) |
| `sdk/sdl3/` | SDL 3 for gasm: SDL as a "private platform" (`SDL_PLATFORM_PRIVATE`), config + drivers (zlib; OpenGL ES on `gasm:gl` in `SDL_gasmopengles.c` for games linked with `lib/gasm_gl.o`). `scripts/fetch-sdl3.sh` puts SDL in `tools/SDL3-src`; `make sdl3` builds `build/sdl3/` (lib, headers, `find_package` config); `scripts/package-sdl3.sh` bundles it |
| `guests/godot/` | Godot 4.7 for gasm: `platform/gasm` (MIT: OS, display server with input, audio driver, FileAccess on assets/storage, entry points) + `godot.patch` (WebGL paths in `drivers/gles3` also for gasm; `HTTPClientTCP` left out for `http_client_gasm.cpp`, `HTTPClient` on gasm:fetch). Examples: hello2d, platformer, scene3d, ui, audio, http, net (`WebSocketPeer` on gasm:net in a relay room; `websocket_peer_gasm.cpp`), mods + modpack (resource packs from `--mods`; modpack has no main scene). `platform/gasm/api`: the `Gasm` singleton (`save_file`, `get_param`, `get_mods`). `scripts/fetch-godot.sh` puts the engine (MIT) in `tools/godot-src`, SCons in `tools/scons`, the editor in `tools/godot-editor`; `make godot` builds `build/godot.wasm`, `build/godot-2d.wasm` (no 3D, `GODOT_2D_FLAGS`, in `tools/godot-src-2d` kept in step by `scripts/sync-godot-variant.sh`) and exports `examples/*` to `build/godot/*.pck`; `make godot-custom GODOT_PROFILE=x.gdbuild` builds a game's own engine |
| `guests/scummvm/` | ScummVM: gasm backend (MIT, `backend/` -> `backends/platform/gasm`) + `configure.patch` (`wasm32-gasm` host). `scripts/fetch-scummvm.sh` puts ScummVM (GPL-3.0) in `tools/scummvm-src`; `scripts/build-scummvm-libs.sh` builds zlib, libmad, libogg/libvorbis, libFLAC (pinned release tarballs) into `tools/scummvm-libs`; `make scummvm` builds with wasi-sdk and runs `wasm-opt --asyncify` (`scripts/fetch-binaryen.sh`); `scripts/package-scummvm-src.sh` packs exactly the files the build used plus the library sources (verify: a clean `make scummvm` from the tarball is byte-identical) |
| `guests/doom/` | DOOM: gasm platform layer (MIT) for doomgeneric. `scripts/fetch-doom.sh` puts the GPL-2.0 engine (+ chocolate-doom OPL music) in `tools/doom-src` and applies `engine.patch`; `scripts/package-doom-src.sh` packs the complete source shipped with releases and the site |
| `runners/native/` | crate `gasm-host` (workspace root; one `target/`): the library has the host (`host.rs`, `switching.rs` stack switching for `gasm_run`, `wasi.rs` WASI subset, `gfx.rs`, `present.rs` 2D filters, `net.rs` (+ `NetPolicy`, the `--allow-net` host list), `fetch.rs` (gasm:fetch, record/replay), `files.rs` (gasm:files), `storage.rs`, `assets.rs`) and both runners (`headless.rs`, `window.rs` + `keymap.rs` behind the default `window` feature); `src/main.rs` (`gasm-run`) only parses arguments. `relay/`: crate `gasm-relay` (no runtime deps, not on crates.io); `relay.Dockerfile` |
| `runners/web/` | npm package `@emdzej/gasm-host`: `gasm-host.js` re-exports `lib/` (`host.js`, `wasi.js`, `gfx.js` GfxModel + NullGfx, `gl.js`, `net.js`, `fetch.js`, `files.js`, `storage.js`, `assets.js`, `input.js` keys/keymap/BrowserInput, `audio.js`) + `.d.ts`; `gasm-worker.js`: Worker mode; `webgpu-gfx.js`; `gasm-present.js` (2D frames on WebGL 2: letterbox + upscaling filters, GLSL ports of `runners/native/src/present.rs`); `headless.mjs` = `gasm-headless`. Not published: `app.js`/`index.html` (the player), `opfs.html`/`opfs.js` (csfs OPFS import; csfs is a devDependency vendored by `scripts/vendor-web.sh`), `testdata.js` |
| `runners/native/src/gl.rs`, `gl_backend.rs`, `angle.rs`, `gles.rs` | `gasm:gl` natively: the model (mirrors `runners/web/lib/gl.js`, null GL), the ANGLE backend it forwards passing calls to, the EGL loader, the generated GLES function table (`scripts/gen-gl-headers.py`, which also writes the C SDK's `GLES3/gl3.h` + `gasm_gl.c`). ANGLE comes from Electron 43.7.7 (`scripts/fetch-angle.sh` → `tools/angle/<platform>`, `package-angle.sh` for bundles) |
| `guests/gasm/src/gles.rs`, `gles_gen.rs`; `sdk/glow/` | Rust guests' GLES 3.0 C API on gasm:gl (`gasm::gles::get_proc_address`; the forwarding half generated by `gen-gl-headers.py`); glow 0.17 with its native backend on wasm32, made by `scripts/update-glow.sh` from crates.io + `sdk/glow.patch` (never edit `sdk/glow` by hand; CRLF sources, `-text`). The guests workspace patches it in; examples `glowtest`, `eguidemo` |
| `runners/native/src/assets.rs`, `keymap.rs` | file-backed assets, `--asset-dir`, case-insensitive lookup; keyboard layouts (`default-keymap.txt` must equal the JS `DEFAULT_KEYMAP`, checked by `gen-abi.mjs --check`) |
| `tests/golden/determinism.txt` | golden hashes of every determinism case; the same on every platform |
| `tests/fixtures/` | byte-exact inputs (`-text`): `cd/` (asset folders), `fetch/` (recorded gasm:fetch responses, from `scripts/fetch-server.mjs` on port 8787) |
| `scripts/` | toolchain/ROM fetchers (`lib.sh`: shared checksum helpers), test suites, packaging (`third-party-notices.sh`: license notices shipped with the games), site build, Linux container |
| `site/` | VitePress website (docs live here; `site/docs/abi.md` includes `spec/ABI.md`) |
| `site/docs/roadmap.md` | **The roadmap**: everything planned or known to be missing, in one place |
| `design/` | design documents with a status line: `threads.md` (part A implemented, B a proposal), and implemented ones kept as a record (`gasm-gl.md`, `fetch.md`, `presentation.md`, `stack-switching.md`) |
| `.github/workflows/` | `ci.yml`, `pages.yml`, `release.yml` |

## Build and test

```sh
make                          # games -> build/*.wasm, native runner + relay (Godot too: its first build
                              # takes ~15 min and an LTO link that needs ~12 GB; `make godot` alone)
make roms                     # test ROMs, Freedoom, shareware doom1.wad into roms/ (needed by the determinism test)
scripts/determinism-test.sh   # 55 cases: wasmtime JIT == AOT == V8 == golden hashes, and the Godot
                              # examples log no errors on either null GL (must pass)
                              # UPDATE_GOLDEN=1 re-records after a change meant to alter output
scripts/net-test.sh           # lockstep sumo via gasm-relay, 3 runner pairs + TLS (must pass)
scripts/asset-test.sh         # folders, case-insensitive names, 200 MB streaming + RSS, --watch-asset (must pass)
node scripts/fetch-test.mjs   # gasm:fetch against scripts/fetch-server.mjs: native == Node == Chrome, denials, record/replay
scripts/gl-native-test.sh     # gasm:gl on ANGLE: gltest hashes == null GL, frame drawn
node scripts/gl-web-test.mjs  # Chrome: gltest/glowtest on WebGL 2 == golden hashes, no WebGL errors
node scripts/opfs-test.mjs    # Chrome: OPFS + Worker mode == Node, memory flat
node scripts/player-test.mjs    # Chrome: copy key (F2) for 2D/gl/gfx, gasm:clipboard paste+copy (Godot too), gasm:files downloads
make parity                   # NES native Rust build == wasm build
node scripts/present-test.mjs # Chrome: 2D filters, WebGL 2 == native wgpu == tests/golden/present (skips without a GPU)
                              # UPDATE_GOLDEN=1 re-records the golden images
scripts/build-site.sh         # website into site/.vitepress/dist (needs build/*.wasm)
scripts/linux-container.sh    # everything above on Linux arm64 (Apple `container`)
scripts/linux-container.sh clean   # remove its volumes, image and builder (several GB)
```

**Clean up containers when you're done.** The Linux container workflow leaves
gigabytes behind (the `gasm-linux-work` and `gasm-linux-cargo` volumes, the
`gasm-linux:dev` image, the `buildkit` builder container). Run
`scripts/linux-container.sh clean` (or remove them by hand, `container ls -a`,
`container volume ls`, `container image ls`) before you finish. Leave other
projects' containers and volumes alone.

Before claiming a change works, run the determinism and network suites. For
runner or ABI changes, also check the browser: `python3 -m http.server 8765`
from the repo root, then
`node scripts/web-smoke.mjs "http://localhost:8765/runners/web/?game=sumo.wasm&autostart" out.png 5`.
`gasm-run --headless --screenshot` renders GPU games offscreen: look at the PNG
(the Node runner has no GPU and only captures `video_present` frames).

## Toolchain traps (already solved: don't reintroduce)

- **Homebrew rust has no wasm targets.** Rust guests build with the rustup
  `stable` toolchain; the Makefile resolves it with `rustup which`. `rustup run`
  can still pick Homebrew's rustc, so `RUSTC` is set explicitly.
- **rust-lld can't find `libLLVM.dylib`** on some macOS rustup installs. Rust
  guests always link with wasi-sdk's `wasm-ld`
  (`CARGO_TARGET_WASM32_UNKNOWN_UNKNOWN_LINKER`).
- **macOS make is 3.81.** No grouped targets (`&:`); the Rust build uses a stamp
  file (`build/.rust-guests`). Keep the Makefile 3.81-compatible.
- **Incremental builds depend on stamps, not on the Makefile.** Flags go through
  flag stamps (`$(call flags,name)` under `build/.flags`, rewritten only when the
  text changes); fetched toolchains and sources have version stamps
  (`tools/*/.version`, `.gasm-configure`, `.gasm-libs`); the compiler (`$(CLANG)`)
  is a real prerequisite. Don't make targets depend on `Makefile` (one edit would
  rebuild ScummVM), and copy into `tools/scummvm-src` only what changed
  (`fetch-scummvm.sh` uses `cmp`).
- **Guests are the same bytes on every host.** wasi-sdk's per-host archives
  carry their own sysroot builds (with different source paths in libc++abi, which
  shift data and so Godot's address-keyed hash maps), so `fetch-wasi-sdk.sh`
  lays the host-independent `wasi-sysroot` and `libclang_rt` archives over them.
  Check with `scripts/linux-container.sh godot` (same sha256 as a macOS build).
- **Every download is pinned:** `download`/`sha256_ok` from `scripts/lib.sh` with
  a SHA-256 (GitHub release assets list theirs: `gh api …/releases/tags/<tag>`).
- **`-mexec-model=reactor` is link-only** for clang; passing it with `-c` errors.
- **`getrandom` on wasm32-unknown-unknown** needs a backend: `guests/.cargo/config.toml`
  selects `unsupported`. The NES guest uses `RamState::AllZeros`, so rand is never
  called.
- **tetanes-core leaves wasm-bindgen imports** in `nes.wasm`. Runners link unknown
  imports as traps (`define_unknown_imports_as_traps`, JS `Proxy`). Don't "fix"
  this by failing on unknown imports.
- **zsh doesn't word-split** `$VAR`; use `${=VAR}` or arrays in ad-hoc commands.
  Scripts are bash and must stay bash 3.2-safe (no empty-array expansion under
  `set -u`; use `${a[@]+"${a[@]}"}`).
- **Background `node` reading the terminal gets suspended** (SIGTTIN). Redirect
  `</dev/null` for background peers in scripts.
- **DOOM is GPL-2.0.** Never commit engine sources into the repo: change the
  engine only through `guests/doom/engine.patch`, which `fetch-doom.sh` applies
  (its checksum is part of the fetch stamp, so `make doom` re-fetches). Wherever
  `doom.wasm` is distributed (release bundles, games zip, site), its source
  archive must be too.
- **Own main loops: Asyncify inside the guest, or stack switching** (`sdk/c/src/gasm_loop.c`,
  Rust `gasm::main_loop`). Both entry points come from one link; `game.wasm`
  gets `--asyncify`, `game-run.wasm` doesn't (`wasm_opt_loop` in the Makefile).
  Runners prefer `gasm_run` when it's exported (natively always; JS with JSPI),
  so the Asyncify path is tested with `--no-stack-switching`. Asyncify rules (each was a bug, listed at
  the top of `gasm_loop.c`): yield through an indirect call; `--asyncify`
  before `-O2`; `gasm_loop_frame` on the remove list and reaching the game only
  through the noinline `run_main`; nothing instrumented between an unwind and
  `asyncify_stop_unwind`; `exit` is wrapped (`--wrap=exit`) when static
  destructors could yield. Rust also removelists `gasm_frame`.
- **Cooperative threads** (`gasm_loop.c` with `-DGASM_LOOP_THREADS`,
  `gasm_thread.c`): Asyncify leaves `__stack_pointer` alone on unwind and rewind,
  so the scheduler saves each thread's stack pointer when it unwinds, restores
  it before rewinding, and runs its own calls on `sched_stack`. Never run
  scheduler code on a thread's stack, and keep the non-thread build of
  `gasm_loop.c` unchanged (ScummVM and SDL use it). Threaded builds don't export
  `gasm_run`. `guests/threadtest` checks all of it. The Rust port
  (`guests/gasm/src/thread.rs`) follows the same rules; it reaches the stack
  pointer through `sp/libgasm_sp.a` (assembled from `gasm_sp.s` by
  `scripts/build-rust-sp.sh`, linked by `build.rs`), keeps the Asyncify flags in
  plain statics and resumes threads by id.
- **ScummVM is GPL-3.0** and runs on `gasm_loop` (copied in as `gasm-loop.cpp`).
  Change ScummVM only through `guests/scummvm/configure.patch` or the backend.
  `SOURCE_DATE_EPOCH` keeps builds reproducible (the in-game menu shows the
  build date).
- **SDL 3 is not patched.** gasm is an SDL private platform: config in
  `sdk/sdl3/include/SDL_build_config_private.h`, drivers in `sdk/sdl3/src`,
  entry points in `SDL_main_impl_private.h`. Files SDL replaces on gasm are
  left out of the build (`SDL_SKIP` in the Makefile), `fopen` in
  `SDL_iostream.c` is redirected to assets/storage. SDL time is virtual and
  per frame; callback apps need no Asyncify, classic `main()` apps do.
  Threads: `SDL_gasmthread.c` replaces all of `src/thread/generic` (in
  `SDL_SKIP`); `SDL_THREADS_DISABLED` stays defined because it selects the
  generic thread handle type. Threaded apps link `lib/gasm_loop_threads.o`;
  SDL's frame clock catches up with the scheduler lazily (`SyncFrames`).
- **SDL's OpenGL ES must not import `gasm:gl` into every SDL game:** a module that
  imports it is a GL game (the runner opens ANGLE / WebGL and ignores 2D frames).
  `SDL_gasmopengles.c` reaches GL only through `gasm_gl_get_proc_address`, a weak
  symbol, which `lib/gasm_gl.o` (an object: a weak reference doesn't pull an
  archive member) defines for GL games. SDL is built against its own GL headers
  (`SDL_USE_BUILTIN_OPENGL_DEFINITIONS`). SDL's GLES 2 renderer uses client-side
  arrays (VBOs only on Emscripten): `gasm_gl.c` emulates them; functions overridden
  only for that are in `C_ONLY` in `gen-gl-headers.py`, so Rust still forwards them.
- **Runners never call a guest after it exited or trapped** (the windowed
  runner drops the game in `stop()`; the event loop can tick once more;
  `GasmHost.dead` makes `frame()` throw without calling the guest).
- **DOOM's rendering depends on heap layout:** the renderer reads past the end
  of some lumps, as the original did. Output stays identical on every runner,
  but changing allocations in the glue (`gasm_doom.c`, `gasm_vfile.c`) changes
  DOOM's video hashes. That is expected: check the frames look right, then
  `UPDATE_GOLDEN=1 scripts/determinism-test.sh`.
- **The WASI subset is ours** (`runners/native/src/wasi.rs`, `runners/web/lib/wasi.js`,
  no `wasmtime-wasi`): keep both identical. Guest stdout goes to the log (stderr),
  never to the runner's stdout, which carries the hash lines.
- **`.cwasm` files are native code:** `gasm-run` loads them only with
  `--allow-precompiled`, and they must come from the same `host::engine()`
  configuration (epoch interruption on).
- **gasm:gl natively = model first, ANGLE second.** `gl.rs` checks every call
  and answers what every runner answers the same way; `gl_backend.rs` runs only
  calls that recorded no GL error (as WebGL in the browser). Hashes come from
  the model, so ANGLE never changes them (`scripts/gl-native-test.sh`). ANGLE
  is loaded at run time (never link it); the context is WebGL-compatible
  (`EGL_CONTEXT_WEBGL_COMPATIBILITY_ANGLE`). SwiftShader needs the Vulkan
  loader shipped next to ANGLE and `VK_ICD_FILENAMES` set before ANGLE starts;
  headless gl games never start wgpu (its Vulkan instance would come first).
  `c_char` is `u8` on arm64 Linux: use `c_char`, not `i8`. Electron 44+ no
  longer ships ANGLE as separate libraries.
- **Godot:** change the engine only through `guests/godot/godot.patch` and
  `platform/gasm` (fetch-godot.sh copies changed platform files in, so SCons
  rebuilds only those). setjmp/longjmp use wasm exceptions (`-mllvm
  -wasm-enable-sjlj`, also at link time for LTO; `-lsetjmp`); `wasm-opt` gets the
  module's exact features (`GODOT_FEATURES`; `--all-features` emits encodings the
  runners refuse). Godot reads the raw keyboard (`KEYS_RAW`): scripted input uses
  `KEY(...)`, not pad buttons. Example packs must export byte-identical everywhere:
  commit `.uid`/`.import` files and keep text scenes
  (`editor/export/convert_text_resources_to_binary=false`). Godot's hash maps key
  on addresses, so a change to the engine build can change the godot-* golden
  hashes (check the frames, then `UPDATE_GOLDEN=1`); hashes are only comparable
  on the null GL (Godot adapts to a real GPU's extensions).
- **`bool::then_some` evaluates its value first.** In the Rust SDK a handle type
  with a closing `Drop` must be built with `then(|| …)`: `(h > 0).then_some(Conn(h))`
  made and dropped `Conn(-1)` for a refusal, closing handle -1, which traps.
- **WebGL wants typed arrays matching the pixel type** (`Float32Array` for
  `FLOAT`, `Uint16Array` for half floats and packed shorts): `pixelView` in
  `lib/gl.js` makes them (copying when the guest pointer is unaligned); the model
  hashes the bytes. A WebGL-only error (Chrome's console says `WebGL: …`) means
  the shared model lacks a rule: add it to both models, `scripts/gl-web-test.mjs`
  catches it.
- **Headless Chrome tests share a debugging port and profile per script:** run
  them one at a time (two `opfs-test.mjs` at once time out). The player's `#log`
  shows only the last line; read guest logs from the console. `?hashframes=N`
  runs frames without yielding to the event loop, so it can't test gasm:net or
  gasm:fetch (they need real frames: `?autostart`).
- **A new Godot example needs a new `make` run** (the example list is a wildcard
  evaluated when make starts); commit the `.uid` files the editor writes in
  `build/godot-projects/<example>/` (their absence makes exports differ).
- **wasm checks indirect call signatures.** C that calls through a mismatched
  function pointer traps with "indirect call type mismatch"; fix it with a
  typed wrapper (see `engine.patch`).

## Invariants

- **ABI changes** start in `spec/abi.json` (then regenerate) and update `spec/ABI.md`, the `gasm` crate
  (`guests/gasm/src/{sys,lib,native}.rs`), both runners (`runners/native/src/host.rs`
  and friends, `runners/web/lib/host.js`), `CHANGELOG.md` and the site docs, in one
  change. Breaking changes bump `GASM_ABI_VERSION`. `gen-abi.mjs --check` compares
  every runner's and the stub's signatures (natively by type, in JS by arity), the
  size constants and `ABI_VERSION` with `abi.json`; annotate native closure return
  types so it can.
- **Hash output format** (`frames=… presented=… size=…` / `video_fnv32=… audio_fnv32=… audio_frames=…`)
  is a contract between runners and scripts. FNV-1a 32 over tightly packed RGBA
  rows, `audio_push` bytes and `gfx.write_buffer` payloads. Native, JS and the
  `gasm::native` stub must stay identical.
- **Determinism rules for game simulations:** only `+ - * / sqrt` on floats (no
  transcendental functions in `sim.rs`), no clocks, no OS randomness, no
  `HashMap` iteration. Rendering may use anything.
- **Headless = reproducible:** virtual time (`VirtualClock`: monotonic across
  frame rate changes, fixed within a frame; also the WASI clocks), a fixed
  `random_get` sequence, null GPU (unless `--screenshot`), in-memory empty storage
  (unless `--storage-dir`; both headless runners). Golden hashes in
  `tests/golden/determinism.txt` are the same on every platform.
- **Security:** guests get no filesystem, env or args. Network is opt-in natively
  (`--allow-net`); the window runner and the player ask the player about other
  hosts and (natively) about saves (`consent.rs`, `Consent` in `lib/fetch.js`):
  a waiting request stays connecting/pending, never blocks a guest call, and
  headless runs never ask. The question screen (`prompt.rs`) is drawn over the
  game with its GL state restored (`Angle::show_image_over_game`). Storage namespaces are chosen by the runner, never the guest.
  Every guest pointer, handle and string argument is validated; violations trap,
  never panic (a guest call that never returns traps after `--call-timeout`).
  Resource limits: 16 net connections with bounded queues; the relay caps clients
  and per-peer queues; guest memory 1 GiB by default (`--memory-limit`: natively
  a `ResourceLimiter`, in JS checked after each call, the same message).
- **gfx:** "auto" layouts are pipeline-exclusive (one bind group per pipeline);
  explicit layouts (`create_bind_group_layout`) are shared across pipelines.
  Both runners keep a per-object record and the render pass state (`Meta`/`Pass`
  in `gfx.rs`, `GfxModel` in `lib/gfx.js`) and validate every call against it
  with the same rules in the same order (handle kinds, destroyed handles, ranges,
  usages, layouts, dynamic offsets, draw ranges), so the null GPU traps exactly
  like the real one; JS backends only execute. Runners never decode images or
  generate mipmaps. `write_texture` hashes a header (`tex, mip, x, y, w, h`)
  before the payload; handles are numbered in creation order on every runner and
  never reused (`destroy` leaves a tombstone; hashes depend on it). The render
  pass always has depth24plus + 4× MSAA; pipelines without `depthStencil` get a
  no-op one. What only the GPU can check becomes a trap via error scopes
  (natively at the call, in browsers at the next gfx call). `tests/gfx-cases.json`
  is replayed by both (`cargo test --lib gfx`, `node scripts/gfx-model-test.mjs`):
  add a case with every new rule.
- **Assets:** never preload on native (positioned reads); same naming rules in
  `assets.rs` and `AssetTable` (exact first, explicit over folder, ASCII
  case-insensitive among folder entries, first sorted on collisions, hidden
  and symlinks skipped, 64-bit sizes; folder files opened lazily, 64 at most).
  `@emdzej/gasm-host` stays dependency-free: csfs belongs to pages (e.g.
  `opfs.html`), not the host. `--mods <dir>` mounts top-level `*.pck`/`*.zip` as
  `mods/<name>` the same way in `assets.rs` (`add_mods`) and `headless.mjs`
  (refusals in asset `mods.refused`). Assets change only between frames, through the
  embedder (`Game::set_asset`, `GasmHost.setAsset`, `--watch-asset`), and every
  change gives a new `asset_version` from one counter (0 = as launched), on both
  runners the same way.
- **Time zone:** `gasm.utc_offset_minutes` is the system's natively
  (`localtime_r`/`GetTimeZoneInformation`), `-getTimezoneOffset()` in JS, and 0
  in headless runs and the native stub (reproducible, like the clocks).
- **The splash screen** is drawn twice, in integer arithmetic only:
  `runners/native/src/splash.rs` and `runners/web/lib/splash.js` must give the same
  frames (`SPLASH_HASH` in splash.rs: its unit test checks Rust, `gen-abi.mjs
  --check` the JS). Change both, then the constant. It's for windows, the player and pages using `gasm-splash.js` (`playSplash`)
  only: never in headless runs or `--window-screenshot`; pages and scripts that
  screenshot the player pass `&nosplash`.
- **gasm:gl null GL limits** are WebGL 2's minimums, in `null_limit` (gl.rs) and
  `NULL_LIMITS` (gl.js): `gen-abi.mjs --check` compares the two tables.
- **Raw input:** keys are W3C `KeyboardEvent.code` names numbered in
  `abi.json` (`GASM_KEY_*`); `KEY_CODES` in `lib/input.js` and `keys.rs` must
  match it (`gen-abi.mjs --check`). Escape: a tap goes to the guest, holding it
  ~1 s quits (natively) / stops (browser); it can't be bound to a pad.
  `KEYS_RAW` turns the keymap off for that guest. Headless scripted input
  (`script.rs` and `input-script.mjs`) is parsed and computed identically,
  in f64 rounded to f32 once; the pointer's frame position uses
  `frame_position`/`framePosition` (same formula as the letterbox).
- **Presentation (2D frames):** display only, applied after hashing. The
  letterbox (`present::letterbox`, `letterbox` in `lib/input.js`) is shared
  with the pointer's frame position; with a display aspect
  (`video_set_aspect`) it multiplies whole numbers before dividing, so 4:3
  scales come out exact (scripted clicks hit the same pixels on both runners).
  `set_title` and `video_set_aspect` are not hashed. Filters exist twice (WGSL in `present.rs`,
  GLSL in `gasm-present.js`): change both, then `node scripts/present-test.mjs`.
  Both sample from the exact fragment position (plus a 1/1024 bias) and use
  whole-pixel viewports, so the runners render the same pixels.
- **Stack switching (`gasm_run`):** natively the guest's run is one async
  wasmtime call polled once per frame (`switching.rs`); the store lives inside
  that call, so runner code reaches the host only through `Game::with_host`
  (never `host_mut()` between frames) and `Host` must stay `Send` (`AudioOut:
  Send`; the cpal stream lives in the window runner). In JS, `yield_frame` is a
  JSPI `Suspending` import and frames are async (`frameAsync`/`runFramesAsync`;
  `frame()` throws for such guests). Same frames and hashes as `gasm_frame`:
  the determinism suite checks 8 variants of each own-loop game. CI uses Node
  24 (JSPI); Node 22 can only run the Asyncify builds.
- **Worker mode:** transferables only (no SharedArrayBuffer/COOP/COEP); main
  thread stays the default. `gasm:gfx` guests go to a worker only through a
  transferred `OffscreenCanvas` with WebGPU in the worker; otherwise they run
  on the main thread (the page falls back automatically).
- **fetch:** `runners/native/src/fetch.rs` and `runners/web/lib/fetch.js` accept and
  refuse the same descriptions, report headers the same way (sorted, joined,
  connection-level and cookies hidden, `content-encoding` dropped: bodies arrive
  decoded) and share the record format and key (FNV-1a 64 of method, URL, body);
  `gen-abi.mjs --check` compares their lists. Replayed requests complete at the
  frame after the request. Host lists (`--allow-net=a,b`) cover gasm:net too, and
  every redirect hop natively. `tests/fixtures/fetch` is recorded from
  `scripts/fetch-server.mjs` on port 8787 (no `Date` header): re-record with
  `--fetch-record` when the guests' requests change.
- **clipboard and files:** pasted text is offered only to the frame that carries
  the paste key press (natively the window runner reads the system clipboard on
  Ctrl/Cmd+V; in the player the page's `paste` event, passed as a step's `paste`,
  which Worker mode carries too); copied text and saves leave between frames.
  Headless runs never paste and write saves only with `--save-dir`, and a save is
  `SAVED` by the next frame on both runners (reproducible). `files.rs` and
  `lib/files.js` refuse the same saves and make names safe the same way (never
  overwriting: `name (2).ext`). Godot reaches gasm:files through the `Gasm`
  singleton (`platform/gasm/api`).
- **net:** runners flush queued messages and do a WebSocket close handshake on
  exit. Lockstep peers finish the frames they have inputs for after a leave notice.
  TLS is rustls with the **ring** backend (no cmake/NASM on CI); the crypto
  provider is installed once per process (`net::install_crypto_provider`). The
  public-endpoint test is opt-in: `cargo test --release -- --ignored`.

## Conventions

- **Version tags have no `v` prefix:** `0.1.0`. Pushing one runs `release.yml`:
  the whole CI suite and a rebuild of doom.wasm/scummvm.wasm from their source
  archives (must be byte-identical) first, then macOS universal + `.app`
  bundles, Linux x86_64/arm64, Windows, games zip (with `THIRD-PARTY.txt`), C SDK,
  then publishes `gasm-sdk` + `gasm-host` (crates.io), `@emdzej/gasm-host` (npm),
  `ghcr.io/emdzej/gasm-relay`. Workflow actions are pinned to commit SHAs. Registries use trusted publishing (OIDC, environment
  `release`); never add registry tokens as secrets. A release commit bumps the
  version in `guests/gasm/Cargo.toml`, `runners/native/Cargo.toml`,
  `runners/native/relay/Cargo.toml`, `runners/web/package.json` (+ both
  `Cargo.lock`s: `cargo update -w`), the `gasm-sdk = "0.x"` lines in
  `guests/gasm/README.md`, `site/dev/games.md`, `site/dev/packages.md`, and
  turns "Unreleased" in the CHANGELOG into the version and date. Tag only after
  CI on that commit's parent is green: a newer push to main cancels the running
  CI (`gh run list` to find runs: `--commit` needs the full SHA). npm can take a
  few minutes to show a published version.
- **Package names:** crates `gasm-sdk` (lib name `gasm`), `gasm-host` and
  `gasm-relay` (`publish = false` until its trusted publisher is set up); npm scope
  `@emdzej/*` (the unscoped `gasm` is taken on both registries). Demo game crates are
  `publish = false` and depend on the SDK by path only.
- **No emojis on the website** (`site/`), including feature tiles and cards.
- Docs are written for the site (`site/…`). Repo links use absolute
  `https://github.com/emdzej/gasm/blob/main/…` URLs so they work in both places.
  VitePress fails on dead links. Diagrams are Mermaid (```` ```mermaid ````, the
  site renders them with vitepress-plugin-mermaid, GitHub natively), not ASCII
  art; check them in a browser (they render client-side, so the build doesn't
  catch syntax errors) in light and dark mode.
- Never commit ROMs, WADs, `build/`, `dist/`, `tools/`, `target/`, `site/node_modules`,
  `site/public/play`.
- Measured numbers in docs (performance, sizes, memory) must come from an actual
  run. Re-measure rather than copy when things change.
- Commit messages: a short summary line, then what changed and why.
- **Plans and known gaps go in `site/docs/roadmap.md` only**, not in READMEs or
  the spec (they link to it). Larger items get a `design/` document; when one
  ships, update its status, take it off the roadmap and add it to the CHANGELOG.
