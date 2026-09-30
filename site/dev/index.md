# Developer guide

gasm has two sides, and you can work on either one alone:

<div class="demo-grid">
  <a class="demo-card" href="/dev/games"><strong>Writing games →</strong><span>Rust with the <code>gasm</code> crate (or C): 2D, 3D with WebGPU, audio, input, networking, determinism.</span></a>
  <a class="demo-card" href="/dev/runners"><strong>Writing runners →</strong><span>Bring every gasm game to a new platform or engine: the ABI, frame loop, GPU, audio, network, conformance.</span></a>
  <a class="demo-card" href="/dev/testing"><strong>Testing & contributing →</strong><span>Determinism, network and parity suites, benchmarks, repository conventions.</span></a>
</div>

## Repository layout

```
spec/                ABI.md (normative) + gasm.h (C header)
guests/              Rust workspace, target wasm32-unknown-unknown
  gasm/                bindings, game! macro, native stub host
  sumo/                3D two-player game (sim.rs, render.rs, lib.rs)
  nes/                 NES emulator on tetanes-core
  triangle/            smallest GPU example
  parity/              runs the NES game natively (parity + benchmarks)
  test-pattern/        C example (wasi-sdk)
runners/native/      gasm-run + gasm-relay (Rust)
runners/web/         browser runner + headless Node runner (JS)
scripts/             toolchain/ROM fetchers, test suites, site build
site/                this website (VitePress)
```

## Build from source

```sh
git clone https://github.com/emdzej/gasm && cd gasm
make            # games + native runner + relay (fetches wasi-sdk on first run)
make test       # determinism + network suites
make web        # browser runner at http://localhost:8080/runners/web/
```

Requirements: rustup (stable toolchain; the wasm32 target is added
automatically), Node ≥ 22, Python 3. On Linux, the native runner also needs
`libasound2-dev` and `libudev-dev`.

## The ABI at a glance

```
exports: memory, gasm_abi_version() -> 0, gasm_init() -> 0, gasm_frame()

gasm        log · time_ms · set_frame_rate · param · video_present
            audio_config · audio_push · input_pad · asset_size · asset_read
gasm:gfx    width · height · create_shader · create_buffer · write_buffer
            create_pipeline · create_bind_group · begin_frame · set_pipeline
            set_bind_group · set_vertex_buffer · set_index_buffer · draw
            draw_indexed · end_frame
gasm:net    open · state · send · recv · close
```

Full details: [ABI v0 specification](/docs/abi).
