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
| `sdk/c/` | C/C++ SDK: CMake toolchain (wraps wasi-sdk) + `Gasm.cmake` + example |
| `guests/` | Rust workspace (`wasm32-unknown-unknown`): `gasm` (SDK + native stub host), `sumo`, `nes` (tetanes-core), `triangle`, `textured` (textures/layouts/offsets test), `inputtest` (raw input tester), `assetcheck`, `parity` (native harness) |
| `guests/test-pattern/` | C guest (wasi-sdk) |
| `guests/doom/` | DOOM: gasm platform layer (MIT) for doomgeneric. `scripts/fetch-doom.sh` puts the GPL-2.0 engine (+ chocolate-doom OPL music) in `tools/doom-src` and applies `engine.patch`; `scripts/package-doom-src.sh` packs the complete source shipped with releases and the site |
| `runners/native/` | crate `gasm-host`: library (`src/lib.rs`) + bins `gasm-run` (`src/main.rs`) and `gasm-relay` (`src/bin/`); `relay.Dockerfile` |
| `runners/web/` | npm package `@emdzej/gasm-host` (`gasm-host.js` + `.d.ts`: host, asset providers, keymap; `gasm-worker.js`: Worker mode; `webgpu-gfx.js`; `headless.mjs` = `gasm-headless`). Not published: `app.js`/`index.html` (the player), `opfs.html`/`opfs.js` (csfs OPFS import; csfs is a devDependency vendored by `scripts/vendor-web.sh`), `testdata.js` |
| `runners/native/src/assets.rs`, `keymap.rs` | file-backed assets, `--asset-dir`, case-insensitive lookup; keyboard layouts (`default-keymap.txt` must equal the JS `DEFAULT_KEYMAP`, checked by `gen-abi.mjs --check`) |
| `scripts/` | toolchain/ROM fetchers, test suites, packaging, site build, Linux container |
| `site/` | VitePress website (docs live here; `site/docs/abi.md` includes `spec/ABI.md`) |
| `.github/workflows/` | `ci.yml`, `pages.yml`, `release.yml` |

## Build and test

```sh
make                          # games -> build/*.wasm, native runner + relay
make roms                     # test ROMs, Freedoom, shareware doom1.wad into roms/ (needed by the determinism test)
scripts/determinism-test.sh   # 14 cases: wasmtime JIT == AOT == V8 (must pass)
scripts/net-test.sh           # lockstep sumo via gasm-relay, 3 runner pairs + TLS (must pass)
scripts/asset-test.sh         # folders, case-insensitive names, 200 MB streaming + RSS (must pass)
node scripts/opfs-test.mjs    # Chrome: OPFS + Worker mode == Node, memory flat
make parity                   # NES native Rust build == wasm build
scripts/build-site.sh         # website into site/.vitepress/dist (needs build/*.wasm)
scripts/linux-container.sh    # everything above on Linux arm64 (Apple `container`)
```

Before claiming a change works, run the determinism and network suites. For
runner or ABI changes, also check the browser: `python3 -m http.server 8765`
from the repo root, then
`node scripts/web-smoke.mjs "http://localhost:8765/runners/web/?game=sumo.wasm&autostart" out.png 5`.
Headless runs with `--screenshot` render GPU games offscreen: look at the PNG.

## Toolchain traps (already solved: don't reintroduce)

- **Homebrew rust has no wasm targets.** Rust guests build with the rustup
  `stable` toolchain; the Makefile resolves it with `rustup which`. `rustup run`
  can still pick Homebrew's rustc, so `RUSTC` is set explicitly.
- **rust-lld can't find `libLLVM.dylib`** on some macOS rustup installs. Rust
  guests always link with wasi-sdk's `wasm-ld`
  (`CARGO_TARGET_WASM32_UNKNOWN_UNKNOWN_LINKER`).
- **macOS make is 3.81.** No grouped targets (`&:`); the Rust build uses a stamp
  file (`build/.rust-guests`). Keep the Makefile 3.81-compatible.
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
- **wasm checks indirect call signatures.** C that calls through a mismatched
  function pointer traps with "indirect call type mismatch"; fix it with a
  typed wrapper (see `engine.patch`).

## Invariants

- **ABI changes** start in `spec/abi.json` (then regenerate) and update `spec/ABI.md`, the `gasm` crate
  (`guests/gasm/src/{sys,lib,native}.rs`), both runners (`runners/native/src/host.rs`
  and friends, `runners/web/gasm-host.js`) and the site docs, in one change.
  Breaking changes bump `GASM_ABI_VERSION`.
- **Hash output format** (`frames=… presented=… size=…` / `video_fnv32=… audio_fnv32=… audio_frames=…`)
  is a contract between runners and scripts. FNV-1a 32 over tightly packed RGBA
  rows, `audio_push` bytes and `gfx.write_buffer` payloads. Native, JS and the
  `gasm::native` stub must stay identical.
- **Determinism rules for game simulations:** only `+ - * / sqrt` on floats (no
  transcendental functions in `sim.rs`), no clocks, no OS randomness, no
  `HashMap` iteration. Rendering may use anything.
- **Headless = reproducible:** virtual time, null GPU (unless `--screenshot`),
  in-memory empty storage (unless `--storage-dir`).
- **Security:** guests get no filesystem, env or args. Network is opt-in natively
  (`--allow-net`). Storage namespaces are chosen by the runner, never the guest.
  Every guest pointer and handle is validated; violations trap, never panic.
- **gfx:** "auto" layouts are pipeline-exclusive (one bind group per pipeline);
  explicit layouts (`create_bind_group_layout`) are shared across pipelines.
  Both runners keep a per-object record (`Meta` in `gfx.rs`, `GfxModel` in
  `gasm-host.js`) and validate textures, samplers, layouts and dynamic offsets
  against it, so the null GPU traps exactly like the real one. Runners never
  decode images or generate mipmaps. `write_texture` hashes a header
  (`tex, mip, x, y, w, h`) before the payload; handles are numbered in
  creation order on every runner (hashes depend on it).
  The render pass always has depth24plus + 4× MSAA; pipelines without
  `depthStencil` get a no-op one. Validation errors become traps via error
  scopes.
- **Assets:** never preload on native (positioned reads); same naming rules in
  `assets.rs` and `AssetTable` (exact first, explicit over folder, ASCII
  case-insensitive among folder entries, first sorted on collisions, hidden
  and symlinks skipped, 2 GiB limit). `@emdzej/gasm-host` stays
  dependency-free: csfs belongs to pages (e.g. `opfs.html`), not the host.
- **Raw input:** keys are W3C `KeyboardEvent.code` names numbered in
  `abi.json` (`GASM_KEY_*`); `KEY_CODES` in `gasm-host.js` and `keymap.rs` must
  match it (`gen-abi.mjs --check`). Escape: a tap goes to the guest, holding it
  ~1 s quits (natively) / stops (browser); it can't be bound to a pad.
  `KEYS_RAW` turns the keymap off for that guest. Headless scripted input
  (`script.rs` and `input-script.mjs`) is parsed and computed identically,
  in f64 rounded to f32 once; the pointer's frame position uses
  `frame_position`/`framePosition` (same formula as the letterbox).
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
  macOS universal + `.app` bundles, Linux x86_64/arm64, Windows, games zip, C SDK,
  then publishes `gasm-sdk` + `gasm-host` (crates.io), `@emdzej/gasm-host` (npm),
  `ghcr.io/emdzej/gasm-relay`. Registries use trusted publishing (OIDC, environment
  `release`); never add registry tokens as secrets.
- **Package names:** crates `gasm-sdk` (lib name `gasm`) and `gasm-host`; npm scope
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
