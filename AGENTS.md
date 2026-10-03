# AGENTS.md

Guidance for coding agents working in this repository. Humans: see
[README.md](README.md) and [gasm.emdzej.pl](https://gasm.emdzej.pl).

## What this is

gasm is a portable game runtime on WebAssembly. **Guests** (games) are single
`.wasm` modules. **Runners** (native Rust, browser JS, headless Node) implement
the ABI in [`spec/ABI.md`](spec/ABI.md): the core `gasm` module plus optional
`gasm:gfx` (WebGPU subset), `gasm:net` (WebSocket-style messages) and
`gasm:storage` (per-game key/value). Determinism across runners is the core
property: most tests assert bit-identical hashes.

## Layout

| Path | What |
|---|---|
| `spec/abi.json` | **Machine-readable ABI, source of truth.** `node scripts/gen-abi.mjs` regenerates `spec/gasm.h` + `guests/gasm/src/sys.rs`; `--check` verifies runners |
| `spec/ABI.md`, `spec/gasm.h` | Normative prose + generated C header (don't edit `gasm.h` or `sys.rs` by hand) |
| `CHANGELOG.md` | What each release added (guest authors: which runner a feature needs). Add to "Unreleased" with every ABI or behaviour change |
| `sdk/c/` | C/C++ SDK: CMake toolchain (wraps wasi-sdk) + `Gasm.cmake` + examples; `gasm_vfile.h` (`FILE*` over assets and storage, used by DOOM and SDL 3) |
| `guests/` | Rust workspace (`wasm32-unknown-unknown`): `gasm` (SDK + native stub host), `sumo`, `nes` (tetanes-core), `triangle`, `textured` (textures/layouts/offsets test), `inputtest` (raw input tester), `loopdemo` (`gasm::main_loop!`), `assetcheck`, `parity` (native harness) |
| `guests/test-pattern/` | C guest (wasi-sdk) |
| `guests/threadtest/` | C guest: cooperative threads (`gasm_thread.h`), deterministic schedule; `mode=many`, `mode=deadlock` |
| `sdk/c/src/gasm_loop.c`, `sdk/c/include/gasm_loop.h` | loop helper for games with their own main loop (`gasm_main` + `gasm_wait_frame`): exports `gasm_frame` (Asyncify inside the guest) and `gasm_run` (the runner switches stacks); Rust: `gasm::main_loop!`. Used by ScummVM and SDL3 classic `main()`; each builds as `game.wasm` (Asyncify) and `game-run.wasm` (without) |
| `sdk/sdl3/` | SDL 3 for gasm: SDL as a "private platform" (`SDL_PLATFORM_PRIVATE`), config + drivers (zlib). `scripts/fetch-sdl3.sh` puts SDL in `tools/SDL3-src`; `make sdl3` builds `build/sdl3/` (lib, headers, `find_package` config); `scripts/package-sdl3.sh` bundles it |
| `guests/scummvm/` | ScummVM: gasm backend (MIT, `backend/` -> `backends/platform/gasm`) + `configure.patch` (`wasm32-gasm` host). `scripts/fetch-scummvm.sh` puts ScummVM (GPL-3.0) in `tools/scummvm-src`; `scripts/build-scummvm-libs.sh` builds zlib, libmad, libogg/libvorbis, libFLAC (pinned release tarballs) into `tools/scummvm-libs`; `make scummvm` builds with wasi-sdk and runs `wasm-opt --asyncify` (`scripts/fetch-binaryen.sh`); `scripts/package-scummvm-src.sh` packs exactly the files the build used plus the library sources (verify: a clean `make scummvm` from the tarball is byte-identical) |
| `guests/doom/` | DOOM: gasm platform layer (MIT) for doomgeneric. `scripts/fetch-doom.sh` puts the GPL-2.0 engine (+ chocolate-doom OPL music) in `tools/doom-src` and applies `engine.patch`; `scripts/package-doom-src.sh` packs the complete source shipped with releases and the site |
| `runners/native/` | crate `gasm-host` (workspace root; one `target/`): the library has the host (`host.rs`, `switching.rs` stack switching for `gasm_run`, `wasi.rs` WASI subset, `gfx.rs`, `present.rs` 2D filters, `net.rs`, `storage.rs`, `assets.rs`) and both runners (`headless.rs`, `window.rs` + `keymap.rs` behind the default `window` feature); `src/main.rs` (`gasm-run`) only parses arguments. `relay/`: crate `gasm-relay` (no runtime deps, not on crates.io); `relay.Dockerfile` |
| `runners/web/` | npm package `@emdzej/gasm-host`: `gasm-host.js` re-exports `lib/` (`host.js`, `wasi.js`, `gfx.js` GfxModel + NullGfx, `net.js`, `storage.js`, `assets.js`, `input.js` keys/keymap/BrowserInput, `audio.js`) + `.d.ts`; `gasm-worker.js`: Worker mode; `webgpu-gfx.js`; `gasm-present.js` (2D frames on WebGL 2: letterbox + upscaling filters, GLSL ports of `runners/native/src/present.rs`); `headless.mjs` = `gasm-headless`. Not published: `app.js`/`index.html` (the player), `opfs.html`/`opfs.js` (csfs OPFS import; csfs is a devDependency vendored by `scripts/vendor-web.sh`), `testdata.js` |
| `runners/native/src/assets.rs`, `keymap.rs` | file-backed assets, `--asset-dir`, case-insensitive lookup; keyboard layouts (`default-keymap.txt` must equal the JS `DEFAULT_KEYMAP`, checked by `gen-abi.mjs --check`) |
| `tests/golden/determinism.txt` | golden hashes of every determinism case; the same on every platform |
| `scripts/` | toolchain/ROM fetchers (`lib.sh`: shared checksum helpers), test suites, packaging (`third-party-notices.sh`: license notices shipped with the games), site build, Linux container |
| `site/` | VitePress website (docs live here; `site/docs/abi.md` includes `spec/ABI.md`) |
| `site/docs/roadmap.md` | **The roadmap**: everything planned or known to be missing, in one place |
| `design/` | design documents: proposals (`gasm-gl.md`, `threads.md`) and implemented ones kept as a record (`presentation.md`, `stack-switching.md`) |
| `.github/workflows/` | `ci.yml`, `pages.yml`, `release.yml` |

## Build and test

```sh
make                          # games -> build/*.wasm, native runner + relay
make roms                     # test ROMs, Freedoom, shareware doom1.wad into roms/ (needed by the determinism test)
scripts/determinism-test.sh   # 26 cases: wasmtime JIT == AOT == V8 == golden hashes (must pass)
                              # UPDATE_GOLDEN=1 re-records after a change meant to alter output
scripts/net-test.sh           # lockstep sumo via gasm-relay, 3 runner pairs + TLS (must pass)
scripts/asset-test.sh         # folders, case-insensitive names, 200 MB streaming + RSS (must pass)
node scripts/opfs-test.mjs    # Chrome: OPFS + Worker mode == Node, memory flat
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
  `gasm_run`. `guests/threadtest` checks all of it.
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
  (`--allow-net`). Storage namespaces are chosen by the runner, never the guest.
  Every guest pointer, handle and string argument is validated; violations trap,
  never panic (a guest call that never returns traps after `--call-timeout`).
  Resource limits: 16 net connections with bounded queues; the relay caps clients
  and per-peer queues.
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
  `opfs.html`), not the host.
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
  `release`); never add registry tokens as secrets.
- **Package names:** crates `gasm-sdk` (lib name `gasm`), `gasm-host` and
  `gasm-relay` (`publish = false` until its trusted publisher is set up); npm scope
  `@emdzej/*` (the unscoped `gasm` is taken on both registries). Demo game crates are
  `publish = false` and depend on the SDK by path only.
- **No emojis on the website** (`site/`), including feature tiles and cards.
- Docs are written for the site (`site/…`). Repo links use absolute
  `https://github.com/emdzej/gasm/blob/main/…` URLs so they work in both places.
  VitePress fails on dead links.
- Never commit ROMs, WADs, `build/`, `dist/`, `tools/`, `target/`, `site/node_modules`,
  `site/public/play`.
- Measured numbers in docs (performance, sizes, memory) must come from an actual
  run. Re-measure rather than copy when things change.
- Commit messages: a short summary line, then what changed and why.
- **Plans and known gaps go in `site/docs/roadmap.md` only**, not in READMEs or
  the spec (they link to it). Larger items get a `design/` document; when one
  ships, update its status, take it off the roadmap and add it to the CHANGELOG.
