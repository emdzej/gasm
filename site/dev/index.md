# Developer guide

gasm has two sides, and you can work on either one alone:

<div class="demo-grid">
  <a class="demo-card" href="/dev/games"><strong>Writing games →</strong><span>Rust with the <code>gasm</code> crate (or C): 2D, 3D with WebGPU, audio, input, networking, determinism.</span></a>
  <a class="demo-card" href="/dev/runners"><strong>Writing runners →</strong><span>Bring every gasm game to a new platform or engine: the ABI, frame loop, GPU, audio, network, conformance.</span></a>
  <a class="demo-card" href="/dev/packages"><strong>Packages →</strong><span><code>gasm-sdk</code> (Rust), the C/C++ SDK, <code>@emdzej/gasm-host</code> (npm), <code>gasm-host</code> (crates.io), the relay image.</span></a>
  <a class="demo-card" href="/dev/testing"><strong>Testing & contributing →</strong><span>Determinism, network and parity suites, benchmarks, repository conventions.</span></a>
</div>

## Repository layout

```
spec/                abi.json (source of truth), ABI.md (normative), gasm.h (generated C header)
guests/              Rust workspace, target wasm32-unknown-unknown
  gasm/                bindings, game! macro, native stub host
  sumo/                3D two-player game (sim.rs, render.rs, lib.rs)
  nes/                 NES emulator on tetanes-core
  doom/                DOOM: platform layer for doomgeneric (C; engine fetched at build)
  scummvm/             ScummVM: gasm backend (C++; engine fetched at build, Asyncify)
  triangle/            smallest GPU example
  textured/            textures, samplers, explicit layouts, dynamic offsets
  inputtest/           raw keyboard, pointer, gamepads
  parity/              runs the NES game natively (parity + benchmarks)
  test-pattern/        C example (wasi-sdk)
  loopdemo/            a game with its own main loop (Rust)
  assetcheck/          test guest for asset providers
sdk/c/               C/C++ SDK: CMake toolchain, gasm_loop, gasm_vfile, examples
sdk/sdl3/            SDL 3 for gasm (config + drivers; SDL fetched at build)
runners/native/      crate gasm-host: library + gasm-run (Rust); relay/: gasm-relay
runners/web/         @emdzej/gasm-host (gasm-host.js + lib/), browser player, headless Node runner (JS)
tests/golden/        golden hashes of the determinism suite
scripts/             toolchain/ROM fetchers, test suites, packaging, site build
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

exports (optional): gasm_exit()   player is quitting: flush saves

gasm        log · has · time_ms · set_frame_rate · param · video_present
            audio_config · audio_push · input_pad · text_input · input_mode
            key_state · key_events · pointer · gamepad · gamepad_name
            asset_size · asset_size64 · asset_read · asset_read_at
            asset_read_at64 · asset_count · asset_name
gasm:gfx    width · height · create_shader · create_buffer · write_buffer
            create_texture · write_texture · create_sampler
            create_bind_group_layout · create_pipeline · create_bind_group
            begin_frame · set_pipeline · set_bind_group · set_bind_group_offsets
            set_viewport · set_scissor_rect · set_vertex_buffer
            set_index_buffer · draw · draw_indexed · end_frame · destroy
gasm:net    open · state · send · recv · close
gasm:storage get · set · delete · count · key
```

Full details: [ABI v0 specification](/docs/abi). What each release added:
[CHANGELOG](https://github.com/emdzej/gasm/blob/main/CHANGELOG.md).

## Design proposals

Not implemented yet; each describes a feature and a plan:

- [`gasm:gl`](https://github.com/emdzej/gasm/blob/main/design/gasm-gl.md): OpenGL ES 3.0 with WebGL 2 rules
- [Threads](https://github.com/emdzej/gasm/blob/main/design/threads.md): cooperative threads inside the guest, real wasm threads later
- [Presentation](https://github.com/emdzej/gasm/blob/main/design/presentation.md): upscaling filters, display aspect, window title

The rest of the plan is the [ABI roadmap](/docs/abi#roadmap-not-in-v0).
