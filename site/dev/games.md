# Writing games

A gasm game is a wasm32 module that exports three functions and imports what it
needs from `gasm`, `gasm:gfx` and `gasm:net`. The recommended path is **Rust**
with the `gasm` crate. C also works (see [C and other languages](#c-and-other-languages)).

- Contract: [ABI v0](/docs/abi)
- Examples: [`guests/triangle`](https://github.com/emdzej/gasm/tree/main/guests/triangle) (smallest GPU game),
  [`guests/sumo`](https://github.com/emdzej/gasm/tree/main/guests/sumo) (3D + networking),
  [`guests/nes`](https://github.com/emdzej/gasm/tree/main/guests/nes) (wrapping an existing Rust crate),
  [`guests/doom`](https://github.com/emdzej/gasm/tree/main/guests/doom) (porting an existing C game, [play it](/play/?game=doom.wasm&autostart))

## 1. A minimal game

```toml
# Cargo.toml
[package]
name = "hello"
version = "0.1.0"
edition = "2024"

[lib]
crate-type = ["cdylib"]

[dependencies]
gasm-sdk = "0.1"                   # crates.io; the library is named `gasm`

[profile.release]
lto = true
panic = "abort"
```

```rust
// src/lib.rs
use gasm::Buttons;

const W: u32 = 160;
const H: u32 = 144;

struct Hello {
    fb: Vec<u8>,
    t: u32,
}

impl gasm::Game for Hello {
    fn init() -> Result<Self, String> {
        gasm::set_frame_rate(60.0);
        gasm::log!("hello from gasm (player name: {:?})", gasm::param("name"));
        Ok(Hello { fb: vec![0; (W * H * 4) as usize], t: 0 })
    }

    fn frame(&mut self) {
        let pressed = gasm::pad(0).held(Buttons::A);
        for (i, px) in self.fb.chunks_exact_mut(4).enumerate() {
            px.copy_from_slice(&[(i as u32 + self.t) as u8, if pressed { 255 } else { 0 }, 64, 255]);
        }
        gasm::present(&self.fb, W, H, W * 4);
        self.t += 1;
    }
}

gasm::game!(Hello);
```

`gasm::game!` exports `gasm_abi_version`, `gasm_init` and `gasm_frame`. It also
installs a panic hook that logs the panic message before the guest traps.
Returning `Err` from `init` logs the error and refuses to start.

## 2. Building

```sh
rustup target add wasm32-unknown-unknown
cargo build --release --target wasm32-unknown-unknown
# -> target/wasm32-unknown-unknown/release/hello.wasm
```

::: tip Linker on macOS
If rust-lld fails with `Library not loaded: @rpath/libLLVM.dylib` (a known
rustup packaging issue on some machines), link with wasi-sdk's `wasm-ld`
instead:
`CARGO_TARGET_WASM32_UNKNOWN_UNKNOWN_LINKER=tools/wasi-sdk/bin/wasm-ld cargo build …`.
The repo's Makefile always does this.
:::

In this repo, add your crate to `guests/Cargo.toml` (`members`) and to the
cargo line in the `Makefile`. `make guests` then copies it to `build/`.

Check what you built. Every import should come from `gasm`, `gasm:gfx`,
`gasm:net` or `wasi_snapshot_preview1`:

```sh
node -e 'const m=new WebAssembly.Module(require("fs").readFileSync("hello.wasm"));
  console.log(WebAssembly.Module.imports(m).map(i=>i.module+"."+i.name))'
```

## 3. Running and iterating

```sh
R=runners/native/target/release/gasm-run
$R hello.wasm --param name=Ada                          # window
$R hello.wasm --headless 60 --screenshot a.png          # quick visual check
node runners/web/headless.mjs hello.wasm --headless 60  # the browser's engine (V8)
```

To try it in the browser runner, copy it into `build/` and add it to the
`GAMES` map in `runners/web/app.js`.

## 4. API tour

### Core

```rust
gasm::set_frame_rate(60.0);          // runner calls frame() at this rate
gasm::log!("score {}", score);       // runner log / browser console
let t = gasm::time_ms();             // monotonic (virtual in headless runs)
let relay = gasm::param("relay");    // Option<String>: --param relay=… / ?relay=…
let rom = gasm::asset("level1");     // Option<Vec<u8>>: --asset level1=path
gasm::exit(0);                       // end the game (WASI proc_exit)
```

### Input

```rust
let pad = gasm::pad(0);              // players 0..=3, stable for the whole frame
if pad.held(Buttons::LEFT) { … }
if pad.held(Buttons::A | Buttons::B) { … }   // any of
```

Buttons are positional: `A` is the east face button, `B` south, `X` north, `Y` west.

### 2D video and audio

```rust
gasm::present(&rgba, w, h, w * 4);   // RGBA8, any size up to 4096², copied by the runner
gasm::audio::config(48_000, 1);      // once: rate and channel count
gasm::audio::push(&samples);         // interleaved f32 in [-1, 1], about rate/fps frames per frame
```

### GPU (`gasm:gfx`)

A WebGPU subset with WGSL shaders. Create resources in `init` (JSON
descriptors mirror WebGPU), then draw each frame:

```rust
use gasm::gfx;

// init
let shader = gfx::create_shader(WGSL);
let pipeline = gfx::create_pipeline(&format!(r#"{{
  "vertex":   {{"module":{s},"entryPoint":"vs","buffers":[{{"arrayStride":20,"attributes":[
                 {{"format":"float32x2","offset":0,"shaderLocation":0}},
                 {{"format":"float32x3","offset":8,"shaderLocation":1}}]}}]}},
  "fragment": {{"module":{s},"entryPoint":"fs","targets":[{{"format":"surface"}}]}},
  "primitive":{{"topology":"triangle-list"}} }}"#, s = shader.0));
let vertices = gfx::create_buffer(60, gfx::VERTEX);
gfx::write_buffer(vertices, 0, &VERTEX_DATA);

// frame
if gfx::begin_frame([0.05, 0.05, 0.1, 1.0]) {   // false = frame won't be shown: skip draws
    gfx::set_pipeline(pipeline);
    gfx::set_vertex_buffer(0, vertices, 0);
    gfx::draw(3, 1, 0, 0);
}
gfx::end_frame();
```

Rules that matter (details in the [ABI spec](/docs/abi#gasm-gfx-optional-gpu-rendering)):

- Color targets say `"format": "surface"`. Depth is `"depth24plus"` or
  omitted. Never set `multisample`: the runner owns the swapchain, depth
  buffer and MSAA.
- Automatic layouts belong to one pipeline: **create a bind group per
  pipeline**, even when two pipelines use the same uniform struct.
- `write_buffer` calls take effect before the frame's draws, so writing one
  uniform buffer twice in a frame means both draws see the second value. Use one
  slot per object instead (sumo uses 256-byte slots in one buffer, one bind group per
  slot, and writes the whole buffer once per frame).
- Offsets and sizes are multiples of 4. Uniform slot offsets must be multiples
  of 256.
- Validation errors trap with the wgpu/WebGPU message: check the runner log.

[`guests/sumo/src/render.rs`](https://github.com/emdzej/gasm/blob/main/guests/sumo/src/render.rs)
is a complete lit 3D renderer on this API: procedural meshes, two pipelines,
per-object uniforms and alpha-blended shadows, in about 300 lines.

### Storage (`gasm:storage`)

```rust
use gasm::storage;

let best = storage::get("best-score").map(|b| u32::from_le_bytes(b[..4].try_into().unwrap()));
storage::set("best-score", &score.to_le_bytes());   // false: invalid key / too big / quota / I/O
storage::delete("best-score");
```

Keys are 1–128 characters of `[A-Za-z0-9._-]`, values up to 1 MiB, 16 MiB
per game. The runner chooses the namespace, and headless runs start empty.
Save when something changes, not every frame. For state that changes
constantly (like NES battery RAM), check a hash every few seconds and write
only on change.

To flush on quit, implement `Game::exit` (exported as `gasm_exit`). It's
best effort: it doesn't run after a crash, so don't rely on it alone.

```rust
impl gasm::Game for MyGame {
    fn init() -> Result<Self, String> { … }
    fn frame(&mut self) { … }
    fn exit(&mut self) { self.save(); }
}
```

For large assets, `gasm::asset_read_at(name, offset, &mut buf)` reads a window
instead of the whole file. Runners don't preload assets (native reads from
disk on demand; the browser does too in Worker mode with OPFS or picked
folders), so streaming a 200 MB soundtrack this way costs almost no memory.
When a game ships data as a folder (a CD image, say), ask for names as you
know them: folder assets match case-insensitively, so `Art/art.car` finds
`ART/ART.CAR` on an upper-case CD.

### Network (`gasm:net`)

```rust
use gasm::net::{Conn, Recv};

let conn = Conn::open("ws://127.0.0.1:9000/myroom").ok_or("network not allowed")?;
// every frame:
loop {
    match conn.recv() {
        Recv::Message(m) => handle(&m),
        Recv::Empty => break,
        Recv::Closed => { /* reconnect or go offline */ break }
    }
}
conn.send(&msg);   // false if not open (yet)
```

Connections open asynchronously: `conn.state()` goes `Connecting` → `Open`.
Natively the player must run with `--allow-net`; handle `open` returning `None`.

## 5. Determinism

Determinism is what makes gasm's tests, replays and lockstep netplay work. A
deterministic game is a pure function of (assets, params, per-frame input).

- Keep **simulation** separate from rendering, and step it exactly once per
  `frame()` (or per confirmed network frame).
- Use only `+ - * / sqrt` on floats in the simulation. They're exact in IEEE
  and wasm, and Rust never fuses them. `sin`/`cos`/`atan2` come from libm
  compiled into your module: identical across wasm runners, but they may
  differ from a native build of the same code. Keep them in rendering.
- No clocks (`time_ms`, `std::time`) or randomness in the simulation. Seed your own
  PRNG from a param or constant.
- Don't iterate `HashMap`s in the simulation; use `Vec` or `BTreeMap`.
- Hash your state (`#[repr(C)]` plus a byte hash, like `Sim::hash`) and compare
  across runners:

```sh
diff <($R g.wasm --headless 600 --input "30-90:RIGHT+A" | grep fnv) \
     <(node runners/web/headless.mjs g.wasm --headless 600 --input "30-90:RIGHT+A" 2>/dev/null | grep fnv)
```

## 6. Multiplayer with lockstep

Sumo's netcode ([`guests/sumo/src/lib.rs`](https://github.com/emdzej/gasm/blob/main/guests/sumo/src/lib.rs))
is a template for any deterministic game:

1. Connect to `gasm-relay` (`ws://host:port/<room>`). The relay assigns your
   player index and announces joins and leaves.
2. When both players are present, reset the sim and prefill inputs for frames
   `0..DELAY`.
3. Each frame: if inputs for frame `f` are known for both players, send your input
   for `f + DELAY`, then step. Otherwise stall and just render.
4. Every N frames, exchange a state hash to detect desyncs.

Relay protocol: [`gasm-relay.rs`](https://github.com/emdzej/gasm/blob/main/runners/native/src/bin/gasm-relay.rs).
The relay never looks inside game messages.

## 7. Wrapping an existing Rust crate

`guests/nes` wraps [tetanes-core](https://crates.io/crates/tetanes-core) in about
80 lines. What it took, and what to look for in other crates:

- **No filesystem.** Disable file features (tetanes: `sram_dir = None`) and feed
  data through assets.
- **No OS randomness.** On `wasm32-unknown-unknown`, `getrandom` needs a backend.
  `guests/.cargo/config.toml` selects `getrandom_backend="unsupported"`, and the
  game makes sure nothing calls it (tetanes: `RamState::AllZeros`, which also
  keeps it deterministic).
- **Browser glue.** Crates that also target browsers may pull in
  wasm-bindgen/web-sys imports. gasm runners link unknown imports as traps, so
  that's fine as long as those code paths never run. Check the import list, and
  if one does trap, the log names it.
- **Time.** Anything based on `Instant::now()` will be wall time; prefer frame
  counting.

## 8. Native builds and debugging

The `gasm` crate compiles for native targets too, against an in-process stub
host (`gasm::native`): no window, null GPU, virtual time, and the same hashing
as the runners. A small harness can drive your game:

```rust
gasm::native::set_asset("rom", std::fs::read(path)?);
assert_eq!(nes::gasm_init(), 0);
for _ in 0..frames { nes::gasm_frame(); gasm::native::end_frame(); }
let s = gasm::native::stats();   // same hashes as gasm-run --headless
```

That's `guests/parity`. Use it for lldb, sanitizers, profilers and
`make parity` (native vs wasm hashes).

Other tools:

- **Logs:** `gasm::log!` output appears on the native runner's stderr and in the
  browser console.
- **Traps** print a wasm backtrace with function names (the release profile
  keeps the name section).

## C and other languages

Anything that emits a wasm32 module with the right imports and exports works.
The C header is [`spec/gasm.h`](https://github.com/emdzej/gasm/blob/main/spec/gasm.h):

```sh
# with libc (printf, malloc) via wasi-sdk
tools/wasi-sdk/bin/clang --target=wasm32-wasip1 -mexec-model=reactor -O2 -Ispec game.c -o game.wasm -lm
# freestanding, no libc
tools/wasi-sdk/bin/clang --target=wasm32 -nostdlib -O2 -Ispec -Wl,--no-entry game.c -o game.wasm
```

The [C/C++ SDK](/dev/packages#for-game-authors) (release asset
`gasm-c-sdk-<version>.zip`) wraps this in CMake: `include(Gasm)`, then
`gasm_add_game(mygame main.c)`.

`-mexec-model=reactor` is required with libc (it produces `_initialize`
instead of `main`) and belongs on the link step only. Export the entry points
with `GASM_EXPORT("gasm_init")` and friends; see
[`guests/test-pattern/main.c`](https://github.com/emdzej/gasm/blob/main/guests/test-pattern/main.c).
With C++, add `-fno-exceptions`. Zig (`wasm32-freestanding`) should work the
same way but hasn't been tried here.

### Porting an existing C game

[`guests/doom`](https://github.com/emdzej/gasm/tree/main/guests/doom) runs
DOOM (doomgeneric) with under 800 lines of glue. The same issues come up in
most old C codebases:

- **Own the main loop.** Desktop code loops forever; a guest runs one step per
  `gasm_frame`. Look for other loops that wait for time to pass (DOOM's screen
  melt, "wait for the next tic") and turn them into per-frame state.
- **Count frames, not milliseconds.** Derive the game's clock from the frame
  number, and let `sleep` just advance it. The game then runs identically on
  every runner, and headless tests are reproducible.
- **Files without a filesystem.** Force-include a header (`-include prelude.h`)
  that redirects `fopen`, `remove` and `rename`, and return real `FILE *`
  streams with `fopencookie`: reads from assets (`asset_read_at`, so large
  files stream) or `gasm:storage`, writes buffered and stored on `fclose`.
  All of stdio (`fread`, `fscanf`, `fprintf`, `ftell`) then keeps working.
- **Function pointer types must match.** Native C tolerates calling a
  function through a pointer of another type; wasm traps with *indirect call
  type mismatch*. The backtrace names the caller. Fix the cast, or add a
  wrapper with the right signature.
- **Line-buffer stdout** (`setvbuf(stdout, NULL, _IOLBF, 0)`), or messages
  sit in the buffer until it fills.
- **Missing libc bits** (`system`, signals, threads): stub them in the
  prelude, since the code paths that need them rarely matter.
- **Keep the license in mind.** DOOM is GPL-2.0, so the repository fetches the
  engine at build time, and releases ship the complete source next to
  `doom.wasm`.

## Checklist

- [ ] `crate-type = ["cdylib"]`, built for `wasm32-unknown-unknown` in release mode
- [ ] `gasm::game!(YourGame)` (or the three exports in C)
- [ ] Imports only `gasm`, `gasm:gfx`, `gasm:net`, WASI (or unused ones that trap harmlessly)
- [ ] Frame rate (and audio format) set in `init`
- [ ] GPU: `"surface"` format, a bind group per pipeline, per-object uniform slots
- [ ] Deterministic: same hashes on `gasm-run` and `headless.mjs`
