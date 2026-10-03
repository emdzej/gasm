# Writing games

A gasm game is a wasm32 module that exports three functions and imports what it
needs from `gasm`, `gasm:gfx`, `gasm:net` and `gasm:storage`. The recommended path
is **Rust** with the `gasm-sdk` crate (library name `gasm`). C also works (see [C and other languages](#c-and-other-languages)).

- Contract: [ABI v0](/docs/abi)
- Examples: [`guests/triangle`](https://github.com/emdzej/gasm/tree/main/guests/triangle) (smallest GPU game),
  [`guests/textured`](https://github.com/emdzej/gasm/tree/main/guests/textured) (textures, samplers, explicit layouts, dynamic offsets, storage buffer, viewport),
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
gasm-sdk = "0.6"                   # crates.io; the library is named `gasm`

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
`gasm:net`, `gasm:storage` or `wasi_snapshot_preview1`:

```sh
node -e 'const m=new WebAssembly.Module(require("fs").readFileSync("hello.wasm"));
  console.log(WebAssembly.Module.imports(m).map(i=>i.module+"."+i.name))'
```

## 3. Running and iterating

```sh
R=runners/native/target/release/gasm-run
$R hello.wasm --param name=Ada                          # window
$R hello.wasm --headless 60 --screenshot a.png          # quick visual check (GPU games too)
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
let all = gasm::asset_names();       // Vec<String>, sorted: discover a folder's contents
gasm::exit(0);                       // end the game (WASI proc_exit)
if gasm::has("gasm:gfx.destroy") { … }   // does this runner provide an import?
gasm::title!("My Game");             // built-in name (custom section), at the top level
gasm::set_title("Level 2");          // the window / tab title (runners add "— gasm")
gasm::video_set_aspect(4, 3);        // show 320x200 frames at 4:3; false on older runners
```

`set_title` and `video_set_aspect` probe for themselves and do nothing on runners
without them; `video_set_aspect` returns `false` there, so a game that cares
can correct the aspect itself (DOOM and ScummVM scale to 640×480 / 320×240 on
older runners).

`gasm::has` takes a module (`"gasm:net"`) or a function (`"gasm.asset_size64"`).
Calling an import the runner lacks traps, so probe imports that are newer than
the runners you want to support ([CHANGELOG](https://github.com/emdzej/gasm/blob/main/CHANGELOG.md) lists when each
arrived).

### Input

```rust
let pad = gasm::pad(0);              // players 0..=3, stable for the whole frame
if pad.held(Buttons::LEFT) { … }
if pad.held(Buttons::A | Buttons::B) { … }   // any of
```

Buttons are positional: `A` is the east face button, `B` south, `X` north, `Y` west.

**Raw devices,** when pads aren't enough (keyboard games, mouse, analog sticks):

```rust
use gasm::{input, keys};
input::set_mode(input::KEYS_RAW | input::POINTER_HIDDEN);   // no keymap pads, game draws the cursor
let k = input::keys().unwrap_or_default();
if k.held(keys::SHIFT_LEFT) && k.held(keys::ARROW_LEFT) { … }    // combinations just work
for e in input::key_events().unwrap_or_default() { … }         // presses/releases in order
if let Some(p) = input::pointer() {
    if p.pressed & input::MOUSE_LEFT != 0 { click(p.frame_x, p.frame_y) }   // frame pixels (2D)
    look(p.dx, p.dy);                                          // with POINTER_LOCKED: mouselook
}
if let Some(g) = input::gamepad(0) && g.connected && g.standard {
    steer(g.axes[0]); throttle(g.buttons[7]);                  // W3C order, analog values
}
```

- `KEYS_RAW` stops the runner's keymap from turning keys into pads, so a key
  doesn't arrive twice. Gamepads keep feeding the pads.
- Keys are physical (`KeyA` is the same key on QWERTY and AZERTY); use
  `text_input` for characters.
- `POINTER_LOCKED` captures the mouse for relative motion. Browsers only lock
  on a click, so check `p.locked()`. A tap of Escape reaches the game; holding
  it quits.
- Joysticks, wheels and other non-standard devices come in their own button
  and axis order (`standard == false`); `input::gamepad_name(slot)` names them.
- [`guests/inputtest`](https://github.com/emdzej/gasm/blob/main/guests/inputtest/src/lib.rs)
  shows everything a runner reports.

For names and chat, `gasm::text_input()` returns the text typed since the last
frame (`'\u{8}'` = backspace, `'\n'` = enter), or `None` if the runner has no
keyboard. Keys bound to pads produce text too, so read it only while a text
field is focused. Headless runs type it from the input script:
`--input '120:"ANNA\n"'`, and the raw devices too:
`KEY(ControlLeft+KeyS)`, `PTR(400,300,L)`, `MOVE(5,0)`, `WHEEL(0,1)`,
`GP0(B0+A1=0.5)` (see the [ABI spec](/docs/abi#scripted-input-headless)).

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
- Automatic layouts (`"layout"` omitted) belong to one pipeline: **create a
  bind group per pipeline**, even when two pipelines use the same uniform
  struct. Explicit layouts (below) are shared.
- `write_buffer` calls take effect before the frame's draws, so writing one
  uniform buffer twice in a frame means both draws see the second value. Use one
  slot per object instead (sumo uses 256-byte slots in one buffer, one bind group per
  slot, and writes the whole buffer once per frame; with dynamic offsets it's one bind group).
- Offsets and sizes are multiples of 4. Uniform slot offsets must be multiples
  of 256.
- Set and draw calls only work between `begin_frame` and `end_frame`. Vertex
  buffers need `gfx::VERTEX` usage and index buffers `gfx::INDEX`; draws must
  stay within their buffers.
- Invalid calls trap with a message, identically on every runner (also
  headless, with the null GPU): check the runner log. The full list is in the
  [ABI spec](/docs/abi#gasm-gfx-optional-gpu-rendering).
- `gfx::destroy(handle)` frees an object you no longer need (a level's
  textures). The handle is invalid afterwards and never reused; bind groups
  and pipelines made from it keep working.

[`guests/sumo/src/render.rs`](https://github.com/emdzej/gasm/blob/main/guests/sumo/src/render.rs)
is a complete lit 3D renderer on this API: procedural meshes, two pipelines,
per-object uniforms and alpha-blended shadows, in about 300 lines.

#### Textures, explicit layouts and dynamic offsets

```rust
// a texture with a full mip chain: the runner never generates mipmaps
let tex = gfx::create_texture(r#"{"size":[256,256],"format":"rgba8unorm","mipLevelCount":9}"#);
for mip in 0..9 { gfx::write_texture(tex, mip, 0, 0, 256 >> mip, 256 >> mip, &levels[mip]); }
let smp = gfx::create_sampler(r#"{"addressModeU":"repeat","addressModeV":"repeat",
    "magFilter":"linear","minFilter":"linear","mipmapFilter":"linear"}"#);

// explicit layouts: one per-object group (dynamic offset) and one per-material group
let obj = gfx::create_bind_group_layout(r#"{"entries":[{"binding":0,"visibility":1,
    "buffer":{"type":"uniform","hasDynamicOffset":true,"minBindingSize":96}}]}"#);
let mat = gfx::create_bind_group_layout(r#"{"entries":[
    {"binding":0,"visibility":2,"texture":{}},{"binding":1,"visibility":2,"sampler":{}}]}"#);
// pipelines list them: "layout":[obj, mat]; any number of pipelines can share them
let objects = gfx::create_bind_group(&format!(
    r#"{{"layout":{},"entries":[{{"binding":0,"buffer":{},"size":96}}]}}"#, obj.0, uniforms.0));
let material = gfx::create_bind_group(&format!(
    r#"{{"layout":{},"entries":[{{"binding":0,"texture":{}}},{{"binding":1,"sampler":{}}}]}}"#, mat.0, tex.0, smp.0));

// frame: 4:3 image in any window, one bind group for every object
gfx::set_viewport(vx, vy, vw, vh, 0.0, 1.0);
gfx::set_scissor_rect(vx as u32, vy as u32, vw as u32, vh as u32);
gfx::set_bind_group(1, material);
for i in 0..n {
    gfx::set_bind_group_offsets(0, objects, &[i * 256]);
    gfx::draw_indexed(6, 1, 0, 0, 0);
}
```

- `write_texture` takes tightly packed RGBA8 rows (`w * h * 4` bytes) and
  can update any region every frame (a video, a scrolling texture).
- Storage buffers (`gfx::STORAGE` usage, `"type":"read-only-storage"`) carry
  per-instance data for instanced draws.
- Pipelines can set `depthBias`, `depthBiasSlopeScale` and `depthBiasClamp`
  for decals drawn over a surface.
- The surface isn't sRGB: colours are written as the shader returns them.

[`guests/textured`](https://github.com/emdzej/gasm/blob/main/guests/textured/src/lib.rs)
uses all of this in about 300 lines.

### Storage (`gasm:storage`)

```rust
use gasm::storage;

let best = storage::get("best-score").map(|b| u32::from_le_bytes(b[..4].try_into().unwrap()));
storage::set("best-score", &score.to_le_bytes());   // false if it failed
match storage::try_set("slot-1", &save) {           // or: why it failed
    Ok(()) => {}
    Err(storage::Error::Quota) => gasm::log!("no room for saves"),
    Err(e) => gasm::log!("save failed: {e:?}"),      // Key, Size, Io
}
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
instead of the whole file (`offset` is a `u64`), and `gasm::asset_size(name)`
gives the size as `Option<u64>`. Assets of any size work; only
`gasm::asset(name)`, which reads everything into memory, needs them under
2 GiB. Runners don't preload assets (native reads from disk on demand; the
browser does too in Worker mode with OPFS or picked folders), so streaming a 200 MB soundtrack this way costs almost no memory.
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

Relay protocol: [`relay/src/main.rs`](https://github.com/emdzej/gasm/blob/main/runners/native/relay/src/main.rs).
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

The header has the same features as the Rust crate: `gasm_has_str("gasm:gfx.destroy")`
probes for an import, `GASM_TITLE("My Game");` (file scope) gives the module
its built-in name, `gasm_set_title_str("Level 2")` names the window and
`gasm_video_aspect(4, 3)` shows frames at 4:3 (returns 0 on runners without it), `gasm_gfx_destroy(handle)` frees a GPU object,
`gasm_storage_set` returns `0` or a `GASM_STORAGE_ERR_*` code (`_KEY`, `_SIZE`,
`_QUOTA`, `_IO`), and the `GASM_POINTER_OFF_*` and `GASM_GAMEPAD_OFF_*`
constants give the field offsets in the bytes `gasm_pointer` and
`gasm_gamepad` fill in.

### Porting an existing C game

[`guests/doom`](https://github.com/emdzej/gasm/tree/main/guests/doom) runs
DOOM (doomgeneric) with under 800 lines of glue. The same issues come up in
most old C codebases:

- **Own the main loop.** Desktop code loops forever; a guest runs one step per
  `gasm_frame`. Look for other loops that wait for time to pass (DOOM's screen
  melt, "wait for the next tic") and turn them into per-frame state.
- **Engines that can't be turned inside out.** When the loop is spread over a
  whole engine (ScummVM has dozens), keep it: see
  [your own main loop](#your-own-main-loop) below, and call `gasm_wait_frame()`
  from the sleep call when a frame is due.
- **Count frames, not milliseconds.** Derive the game's clock from the frame
  number, and let `sleep` just advance it. The game then runs identically on
  every runner, and headless tests are reproducible.
- **Files without a filesystem.** Force-include a header (`-include prelude.h`)
  that redirects `fopen`, `remove` and `rename`, and return real `FILE *`
  streams from the C SDK's
  [`gasm_vfile.h`](https://github.com/emdzej/gasm/blob/main/sdk/c/include/gasm_vfile.h):
  `gasm_vfile_open(GASM_VFILE_ASSET, name, "rb")` streams an asset
  (`asset_read_at`), `gasm_vfile_open(GASM_VFILE_STORAGE, key, mode)` reads
  a `gasm:storage` value and stores writes on `fclose`. All of stdio (`fread`,
  `fscanf`, `fprintf`, `ftell`) then keeps working. Compile
  `sdk/c/src/gasm_vfile.c` with the game (it needs `_GNU_SOURCE` for
  `fopencookie`); with CMake, `target_sources(mygame PRIVATE ${GASM_VFILE_SOURCE})`.
  DOOM and SDL 3 use it.
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

### Your own main loop

gasm calls the game once per frame ([why](/docs/abi#why-the-runner-drives-the-frames)).
For code that wants to own its loop, both SDKs have a loop helper: write a
normal `main` that loops and call `wait_frame()` where a frame ends. The helper
exports the gasm entry points, starts `main` on the first frame and suspends it
in `wait_frame()` until the next one. Returning from `main` ends the game with
that exit code. Two builds come from one link:

- **`game.wasm`**, post-processed with Binaryen's Asyncify: suspends itself,
  inside the module, so it runs on every runner.
- **`game-run.wasm`**, without Asyncify: the runner suspends it
  ([stack switching](/docs/abi#stack-switching): `gasm-run`, Chromium, Node
  24+). About a third smaller (ScummVM: 10.6 MB instead of 16.1 MB, SDL 3
  classic: 0.81 MB instead of 1.15 MB); runners without stack switching refuse
  it, so ship both for the browser.

```c
#include "gasm.h"
#include "gasm_loop.h"

int gasm_main(void) {
    for (;;) {
        if (gasm_pad(0) & GASM_BTN_START) return 0;
        update(); draw();
        gasm_wait_frame();
    }
}
```

With the C SDK: `gasm_add_game(mygame LOOP main.c)` (needs `wasm-opt` from
[Binaryen](https://github.com/WebAssembly/binaryen/releases); set
`GASM_WASM_OPT` if it's not on `PATH`); it writes both builds. By hand: compile
`src/gasm_loop.c` with the game, link with `-Wl,--wrap=exit`, then run
`wasm-opt game.wasm -O2 -o game-run.wasm` and
`wasm-opt game.wasm --asyncify --pass-arg=asyncify-removelist@gasm_loop_frame -O2 -o game.wasm`.
Optional `gasm_loop_init()` and `gasm_loop_exit()` run at `gasm_init` and
`gasm_exit`; `-DGASM_LOOP_STACK_SIZE=` sets the space for the suspended stack
(default 1 MiB; ScummVM uses 4 MiB).

In Rust:

```rust
fn run() -> i32 {
    loop {
        if gasm::pad(0).held(gasm::Buttons::START) { return 0; }
        draw();
        gasm::main_loop::wait_frame();
    }
}
gasm::main_loop!(run);              // or main_loop!(run, on_exit): also exports gasm_exit
```

then `wasm-opt game.wasm -O2 -o game-run.wasm` and
`wasm-opt game.wasm --asyncify --pass-arg=asyncify-removelist@gasm_frame,gasm_loop_frame -O2 -o game.wasm`.
Only `main_loop!` games carry the loop export (`gasm_loop_frame`) and the
Asyncify imports; `game!` games don't.
Examples: [`sdk/c/example-loop`](https://github.com/emdzej/gasm/tree/main/sdk/c/example-loop),
[`guests/loopdemo`](https://github.com/emdzej/gasm/tree/main/guests/loopdemo).

The Asyncify build costs size: Asyncify instruments every function that can
reach `wait_frame()`. Prefer the per-frame callback for new games; use the loop
helper for ports. The rules it follows, each learned from
a bug, are at the top of
[`gasm_loop.c`](https://github.com/emdzej/gasm/blob/main/sdk/c/src/gasm_loop.c).

### Threads

Games with their own main loop can use **cooperative threads**
([`gasm_thread.h`](https://github.com/emdzej/gasm/blob/main/sdk/c/include/gasm_thread.h)):
threads, mutexes (plain and recursive), condition variables, semaphores and
thread-local keys. Every thread runs on the guest's one wasm thread and runs
until it blocks, yields or waits for the next frame; then the next ready thread
runs, in creation order. A frame ends when no thread is ready. So there are no
data races and the schedule depends only on the input: runs stay reproducible.

```c
#include "gasm_thread.h"

static gasm_mutex lock = GASM_MUTEX_INIT;
static gasm_cond ready = GASM_COND_INIT;

static int loader(void *arg) {
    load_level(arg);                       // may wait for frames, sleep, lock
    gasm_mutex_lock(&lock); loaded = 1; gasm_cond_signal(&ready); gasm_mutex_unlock(&lock);
    return 0;
}

int gasm_main(void) {
    gasm_thread *t = gasm_thread_create(loader, "level1", 0);   // 256 KiB C stack
    while (!loaded) { draw_loading_screen(); gasm_wait_frame(); }
    gasm_thread_join(t);
    ...
}
```

Build with `gasm_add_game(mygame LOOP THREADS main.c)` (by hand: add
`src/gasm_thread.c` and compile `gasm_loop.c` with `-DGASM_LOOP_THREADS`).
Threads switch with Asyncify, so such games only come as the Asyncify build.

- Time is the frame's (`gasm_time_ms()`): `gasm_thread_sleep_ms` and the
  timeouts of `gasm_cond_wait` / `gasm_sem_wait` end at the first frame at or
  after their deadline.
- `errno` is per thread; `_Thread_local` variables are shared (use
  `gasm_thread_key_*`).
- A thread that spins without blocking or yielding keeps the frame from
  ending. If every thread waits for another, the frame traps with the list of
  who waits for what.
- Each thread costs its C stack plus a 256 KiB Asyncify buffer.

### SDL 3

SDL programs build for gasm with their source unchanged:
[SDL 3 for gasm](https://github.com/emdzej/gasm/blob/main/sdk/sdl3/README.md)
is SDL itself with gasm as an SDL "private platform" (release asset
`gasm-sdl3-<version>.zip`, or `make sdl3`). The window framebuffer and the
software `SDL_Renderer` go to `video_present`; keyboard, text input, mouse
(including relative mode), joysticks and `SDL_Gamepad`, audio playback, files
(assets and `gasm:storage`) and storage all map onto the ABI, on virtual time.

```cmake
find_package(SDL3 REQUIRED)        # -DSDL3_DIR=<gasm-sdl3>/lib/cmake/SDL3
add_executable(mygame main.c)
target_link_libraries(mygame PRIVATE SDL3::SDL3)
gasm_sdl3_app(mygame)              # SDL_MAIN_USE_CALLBACKS
gasm_sdl3_app(mygame LOOP)         # or: a classic main() loop (Asyncify)
```

Apps on the main callbacks map one-to-one: `SDL_AppIterate` runs once per gasm
frame. A classic `main()` loop runs on the loop helper above; its frame ends at
`SDL_RenderPresent` (or at an `SDL_Delay` across a frame boundary). SDL's own
demos run unchanged: [snake and woodeneye-008](/demos/#sdl-3). Not available:
threads (and `SDL_AddTimer`), OpenGL/Vulkan/`SDL_GPU`, audio recording, camera.

## Checklist

- [ ] `crate-type = ["cdylib"]`, built for `wasm32-unknown-unknown` in release mode
- [ ] `gasm::game!(YourGame)` (or the three exports in C)
- [ ] Imports only `gasm`, `gasm:gfx`, `gasm:net`, `gasm:storage`, WASI (or unused ones that trap harmlessly)
- [ ] Imports newer than your target runners are probed with `gasm::has` first
- [ ] Frame rate (and audio format) set in `init`
- [ ] GPU: `"surface"` format, a bind group per pipeline, per-object uniform slots
- [ ] Deterministic: same hashes on `gasm-run` and `headless.mjs`
