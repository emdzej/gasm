# gasm — game assembly

A proof of concept for a portable game runtime: **games are compiled once to
WebAssembly, and thin per-platform *runners* expose a small, stable binary
interface** (video, audio, input, assets, GPU, network). The same `.wasm` runs
natively (wasmtime + wgpu), in the browser (WebAssembly + WebGPU), and headless
in Node, with bit-identical game state.

Games in this repo (all Rust except one C example):

| Game | Shows | Size |
|---|---|---|
| `sumo.wasm` | 3D (`gasm:gfx`, WebGPU/WGSL) + online 2-player lockstep (`gasm:net`), with cross-play between native and browser | 70 KB |
| `nes.wasm` | NES emulator on [tetanes-core](https://crates.io/crates/tetanes-core): 2D video, audio, input, assets | 1.5 MB |
| `triangle.wasm` | Smallest `gasm:gfx` program (about 50 lines of Rust) | 25 KB |
| `test-pattern.wasm` | Minimal C guest (proves the ABI is language-agnostic) | 87 KB |

```
                ┌────────── game.wasm (one artifact) ──────────┐
                │ Rust/C game ── gasm crate / gasm.h bindings  │
                └───────────────────────┬──────────────────────┘
       gasm ABI v0: gasm.* (video, audio, input, assets, params)
                    gasm:gfx (WebGPU subset)   gasm:net (messages)
      ┌───────────────────┬─────────────┴────────┬──────────────────────┐
  gasm-run (Rust)     gasm-run --compile     browser runner         Node headless
  wasmtime + wgpu     AOT .cwasm (no JIT)    WebGPU + canvas +      (CI, hashing,
  winit/cpal/gilrs                           AudioWorklet+Gamepad   net tests)
                   ▲                                  ▲
                   └──────── gasm-relay (WebSocket rooms) ────────┘
```

**Website:** [gasm.emdzej.pl](https://gasm.emdzej.pl): docs and **playable demos** in your browser.
**Docs:** [user guide](site/guide/index.md) · [how it works](site/docs/how-it-works.md) ·
[writing games](site/dev/games.md) · [writing runners](site/dev/runners.md) ·
[testing](site/dev/testing.md) · [ABI spec](spec/ABI.md)

## Download

[Releases](https://github.com/emdzej/gasm/releases) have ready-to-run builds:
**macOS apps** (Sumo.app, NES.app, Triangle.app; unsigned, so right-click →
Open the first time) and `gasm-run` + `gasm-relay` + games for macOS
(universal), Linux and Windows. Or just [play in the browser](https://gasm.emdzej.pl/demos/).

## Quick start

Requirements (building from source): Rust via **rustup** (Homebrew's rust has no wasm targets; the
Makefile adds `wasm32-unknown-unknown` to the `stable` toolchain), Node ≥ 22,
Python 3 (dev web server), curl, git. `make` fetches
[wasi-sdk](https://github.com/WebAssembly/wasi-sdk) into `tools/` on first use.
It builds the C example and provides `wasm-ld`, which links the Rust games.
On Linux, the native runner also needs `libasound2-dev libudev-dev pkg-config`.

```sh
make                 # build games (build/*.wasm) + native runner + relay
make roms            # fetch freely distributable test ROMs / homebrew into roms/
make test            # determinism (JIT vs AOT vs V8) + network lockstep tests

R=runners/native/target/release/gasm-run

# 3D sumo vs. a bot
$R build/sumo.wasm

# 3D sumo online: start a relay, then two players (any mix of native/browser)
make relay                                                     # terminal 1
$R build/sumo.wasm --allow-net --param relay=ws://127.0.0.1:9000   # terminal 2
make web   # terminal 3, open http://localhost:8080/runners/web/, enter the relay URL, start

# NES
$R build/nes.wasm --rom roms/bladebuster.nes
```

Controls: arrows = D-pad, **X** = A, **Z** = B, **Enter** = Start,
**Right Shift** = Select (plus S/A = X/Y, Q/W = L/R). Gamepads work in both
runners. In sumo: move with the D-pad, **dash with A or B**, and push the other
ball off the platform. First to 5 wins.

## Results (Apple M1 Pro, this commit)

**Portability.** `make test` checks 8 single-player cases (test pattern, sumo
vs. bot, 5 NES test ROMs/demos, scripted Blade Buster gameplay). Each produces
**bit-identical video, audio and GPU-upload streams** on wasmtime JIT,
wasmtime AOT and V8. It also plays 3,600-frame online sumo matches through
`gasm-relay` for native↔native, native↔Node and Node↔native, all ending in the
identical game state. A headless-Chrome (WebGPU) player against a native
player was also verified (`fa43844c` on both sides after 1,200 frames).

**NES correctness** (tetanes-core, inside wasm): blargg `instr_test-v5` *All
16 tests passed*, `cpu_timing_test6` *PASSED*, `apu_test` *All 8 tests
passed*. `make parity` builds the same Rust game natively and gets identical
hashes.

**Performance.** 6,000 frames of Blade Buster, emulation only (`--no-hash`):

| build                          | time   | vs native | realtime |
|--------------------------------|--------|-----------|----------|
| native Rust (release, LTO)     | 7.1 s  | 1.0×      | ~14×     |
| wasm, wasmtime (Cranelift AOT) | 10.8 s | 1.5×      | ~9×      |
| wasm, Node 22 (V8)             | 11.2 s | 1.6×      | ~9×      |

Sumo's simulation plus scene building runs at more than 100,000 frames/s
headless; with the GPU, both runners hold 60 fps.

## Layout

```
spec/gasm.h, spec/ABI.md     the ABI (normative doc + C header)
guests/                      Rust workspace (wasm32-unknown-unknown)
  gasm/                        Rust bindings + game! macro + native stub host
  sumo/                        3D 2-player game: sim.rs (deterministic), render.rs, lib.rs (lockstep)
  nes/                         NES emulator on tetanes-core
  triangle/                    smallest GPU example
  parity/                      runs the NES game natively (parity + benchmarks)
  test-pattern/                C guest (wasi-sdk)
runners/native/              Rust: wasmtime + wasmtime-wasi, wgpu, winit, cpal, gilrs, tungstenite
  src/bin/gasm-relay.rs        WebSocket room relay
runners/web/                 gasm-host.js (browser + Node), webgpu-gfx.js, index.html, app.js, headless.mjs
scripts/                     fetch-wasi-sdk, fetch-roms, determinism-test, net-test, web-smoke,
                             build-site, package-cli, package-macos
site/                        website (VitePress): docs, dev guides, demos → gasm.emdzej.pl
.github/workflows/           CI (tests), Pages (site), Release (macOS apps + CLI builds)
```

## Licensing

- gasm (spec, runners, relay, bindings, games): MIT.
- `nes.wasm` statically contains tetanes-core (MIT OR Apache-2.0). No copyleft
  code is involved anymore; the earlier QuickNES (GPL-2.0) build was replaced.
- ROMs are not part of the repo. `make roms` downloads test ROMs and homebrew
  demos from the [nes-test-roms](https://github.com/christopherpow/nes-test-roms)
  collection for testing.

## Next steps

1. Runner-level **rollback netplay**: snapshot and restore guest memory, which
   makes online play feel lag-free and works for NES too.
2. `gasm:storage` for saves; a manifest custom section for capabilities and
   network hosts.
3. `gasm:gfx` v1: textures, samplers, instancing.
4. A **wasm2c** runner, to show the no-runtime/no-JIT path (iOS, consoles).
