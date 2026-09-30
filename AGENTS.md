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
| `spec/ABI.md`, `spec/gasm.h` | Normative ABI + C header |
| `guests/` | Rust workspace (`wasm32-unknown-unknown`): `gasm` (SDK + native stub host), `sumo`, `nes` (tetanes-core), `triangle`, `parity` (native harness) |
| `guests/test-pattern/` | C guest (wasi-sdk) |
| `runners/native/` | `gasm-run` (wasmtime, wgpu, winit, cpal, gilrs, tungstenite) and `gasm-relay` (`src/bin/`) |
| `runners/web/` | `gasm-host.js` (shared browser + Node core), `webgpu-gfx.js`, `app.js`, `headless.mjs` |
| `scripts/` | toolchain/ROM fetchers, test suites, packaging, site build, Linux container |
| `site/` | VitePress website (docs live here; `site/docs/abi.md` includes `spec/ABI.md`) |
| `.github/workflows/` | `ci.yml`, `pages.yml`, `release.yml` |

## Build and test

```sh
make                          # games -> build/*.wasm, native runner + relay
make roms                     # test ROMs into roms/ (needed by the determinism test)
scripts/determinism-test.sh   # 8 cases: wasmtime JIT == AOT == V8 (must pass)
scripts/net-test.sh           # lockstep sumo via gasm-relay, 3 runner pairs (must pass)
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

## Invariants

- **ABI changes** update `spec/ABI.md`, `spec/gasm.h`, the `gasm` crate
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
- **gfx:** "auto" layouts are pipeline-exclusive (one bind group per pipeline).
  The render pass always has depth24plus + 4× MSAA; pipelines without
  `depthStencil` get a no-op one. Validation errors become traps via error
  scopes.
- **net:** runners flush queued messages and do a WebSocket close handshake on
  exit. Lockstep peers finish the frames they have inputs for after a leave notice.

## Conventions

- **Version tags have no `v` prefix:** `0.1.0`. Pushing one runs `release.yml`
  (macOS universal + `.app` bundles, Linux x86_64/arm64, Windows, games zip).
- **No emojis on the website** (`site/`), including feature tiles and cards.
- Docs are written for the site (`site/…`). Repo links use absolute
  `https://github.com/emdzej/gasm/blob/main/…` URLs so they work in both places.
  VitePress fails on dead links.
- Never commit ROMs, `build/`, `dist/`, `tools/`, `target/`, `site/node_modules`,
  `site/public/play`.
- Measured numbers in docs (performance, sizes, memory) must come from an actual
  run. Re-measure rather than copy when things change.
- Commit messages: a short summary line, then what changed and why.
