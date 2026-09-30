# Testing and contributing

## Test suites

| Command | What it proves | Time |
|---|---|---|
| `scripts/determinism-test.sh` | Every case gives identical video, audio and GPU-upload hashes on **wasmtime JIT**, **wasmtime AOT** and **V8 (Node)** | ~75 s |
| `scripts/net-test.sh` | Full online sumo matches through `gasm-relay` for native↔native, native↔Node and Node↔native end in the **identical game state**, with no desync | ~10 s |
| `make test` | Both of the above | |
| `make parity ROM=… FRAMES=…` | The NES game built **natively** (Rust, stub host) matches the wasm build | ~1 min first build |
| `node scripts/web-smoke.mjs <url> <out.png> [secs]` | The browser runner loads and runs in headless Chrome (including WebGPU); prints status, fps and console, saves a screenshot | ~10 s |

### Determinism test

Cases: the C test pattern, sumo vs. the bot with scripted input (GPU uploads
are hashed, covering the whole scene), the blargg CPU instruction, CPU timing
and APU tests, spritecans, Quantum Disco Brothers, and 2,400 frames of Blade
Buster gameplay with scripted input. It fetches ROMs if missing and
AOT-compiles the games first.

```sh
check <name> <game> <frames> [runner args...]
check my_game nes 1800 --rom roms/my_game.nes --input "60-65:START,120-900:RIGHT"
```

A mismatch means one of:

1. the game reads nondeterministic state (clock, randomness, `HashMap` order),
2. a runner bug (wrong copy stride, hashing padding, mis-ordered input), or
3. an engine bug (rare; reduce to a minimal module and report upstream).

### Network test

It starts `gasm-relay` on port 9123 and, for each runner pair, launches two
headless sumo peers in a fresh room with different held inputs, running as
fast as lockstep allows. Each peer logs `frame N state=… score=a:b` at
`quit_at` and exits. The test requires identical lines and no `DESYNC`. Each
peer has a hard time limit (`LIMIT`, default 60 s), so a stall fails the test
instead of hanging it.

```sh
QUIT=3600 LIMIT=40 PORT=9200 scripts/net-test.sh
```

Browser cross-play is checked by hand with `web-smoke.mjs` against a native
peer:

```sh
gasm-relay 127.0.0.1:9125 &
gasm-run build/sumo.wasm --headless 100000000 --allow-net \
  --param relay=ws://127.0.0.1:9125 --param room=x --param quit_at=1200 --input "0-100000000:RIGHT+A" &
node scripts/web-smoke.mjs "http://localhost:8765/runners/web/?game=sumo.wasm&autostart&relay=ws://127.0.0.1:9125&room=x&quit_at=1200" out.png 24
# both print: [sumo] frame 1200 state=<same hash>
```

### Parity test

`guests/parity` runs the NES game as a normal native binary against the `gasm`
crate's stub host and prints the same hash lines as the runners. If wasm and
native builds disagree, suspect undefined behaviour, pointer-size assumptions
(`usize` is 4 bytes in wasm32), or transcendental float functions (native libm
vs the libm compiled into the module).

### Visual checks

Every headless run can take `--screenshot out.png`. GPU games render the last
frame on a real GPU offscreen. Useful ROMs from `make roms`:

| ROM | Expected screen |
|---|---|
| `cpu_instr_test.nes` (≈3000 frames) | `All 16 tests passed` |
| `cpu_timing_test.nes` (≈1200 frames) | `PASSED` |
| `apu_test.nes` (≈1500 frames) | `All 8 tests passed` |
| `bladebuster.nes` | Title screen; with the input script from the test, gameplay |

### Benchmarks

```sh
R=runners/native/target/release/gasm-run
$R build/nes.wasm --compile build/nes.cwasm
(cd guests && cargo build --release -p parity)
time guests/target/release/parity roms/bladebuster.nes 6000 --no-hash
time $R build/nes.cwasm --rom roms/bladebuster.nes --headless 6000 --no-hash
time node runners/web/headless.mjs build/nes.wasm --rom roms/bladebuster.nes --headless 6000 --no-hash
```

Always use `--no-hash` for speed measurements: hashing 245 KB per frame costs a
lot of time.

## Repository conventions

- **ABI changes** go into `spec/ABI.md`, `spec/gasm.h`, the `gasm` crate *and*
  both runners in the same change. Breaking changes bump `GASM_ABI_VERSION`.
- **Hash format** (`frames=… / video_fnv32=… audio_fnv32=…`) is an interface
  between runners and scripts; keep it stable.
- **Game logic** that must be deterministic lives apart from rendering
  (`sumo/src/sim.rs`).
- **Dependencies:** wasi-sdk version in `scripts/fetch-wasi-sdk.sh`, runner crates
  in `runners/native/Cargo.toml`, game crates in `guests/*/Cargo.toml`.
- Generated or downloaded content (`build/`, `tools/`, `roms/`, `target/`,
  `site/node_modules`, `site/.vitepress/dist`, `site/public/play`) is git-ignored.
  Never commit ROMs.
- **Docs** live in `site/` (VitePress) and are published to
  [gasm.emdzej.pl](https://gasm.emdzej.pl) by GitHub Actions. `spec/ABI.md` is
  included into the site, not copied.

## Updating dependencies

- **wasi-sdk:** `WASI_SDK_VERSION=NN scripts/fetch-wasi-sdk.sh`, then `make clean test`.
- **tetanes-core:** bump in `guests/nes/Cargo.toml`. Hashes may change if the core
  changed behaviour; the suite only requires runners to agree. Re-check the
  test ROM screens.
- **wasmtime, wgpu, winit:** bump in `runners/native/Cargo.toml`; wasmtime and
  wasmtime-wasi together. Old `.cwasm` files become invalid; the test script
  regenerates them.
