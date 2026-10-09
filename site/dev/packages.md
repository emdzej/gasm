# Packages

The ABI is published as libraries for game authors and for people who run or
embed games. Every artifact is built and published by
[`release.yml`](https://github.com/emdzej/gasm/blob/main/.github/workflows/release.yml)
when a version tag (like `0.2.0`, no `v`) is pushed. Registries use
**trusted publishing** (GitHub OIDC), so no long-lived tokens are stored.

## For game authors

| Language | Package | Install |
|---|---|---|
| Rust | [`gasm-sdk`](https://crates.io/crates/gasm-sdk) (library name `gasm`) | `gasm-sdk = "0.14"`, `crate-type = ["cdylib"]` |
| C / C++ | `gasm-c-sdk-<version>.zip` on [Releases](https://github.com/emdzej/gasm/releases) | `gasm.h` + CMake toolchain (wasi-sdk) + examples, `gasm_loop.h` for an own main loop, `gasm_vfile.h` for `FILE*` over assets and storage |
| SDL 3 | `gasm-sdl3-<version>.zip` on [Releases](https://github.com/emdzej/gasm/releases) | `libSDL3.a`, headers, `find_package(SDL3)` config, examples ([details](https://github.com/emdzej/gasm/blob/main/sdk/sdl3/README.md)) |
| anything else | `gasm.h` / `abi.json` on Releases | bind the imports yourself; see below |

**Rust:**

```toml
[lib]
crate-type = ["cdylib"]

[dependencies]
gasm-sdk = "0.14"
```

```rust
struct MyGame;
impl gasm::Game for MyGame {
    fn init() -> Result<Self, String> { Ok(MyGame) }
    fn frame(&mut self) { /* … */ }
}
gasm::game!(MyGame);
```

**C / C++ with CMake:**

```sh
export WASI_SDK_PATH=/path/to/wasi-sdk
cmake -B build -DCMAKE_TOOLCHAIN_FILE=<c-sdk>/cmake/gasm-toolchain.cmake -DCMAKE_BUILD_TYPE=Release
cmake --build build
```

```cmake
include(Gasm)
gasm_add_game(mygame main.c)   # -> mygame.wasm (reactor model, gasm.h on the include path)
```

## For runners and embedders

| Package | What |
|---|---|
| [`@emdzej/gasm-host`](https://www.npmjs.com/package/@emdzej/gasm-host) (npm) | Browser + Node host, dependency-free: `GasmHost`; asset providers (`AssetTable`, folder, OPFS and `File` sources); Worker mode (`@emdzej/gasm-host/worker`); WebGPU backend (`@emdzej/gasm-host/webgpu`); the splash screen (`@emdzej/gasm-host/splash`, `playSplash`); IndexedDB storage; keyboard layouts (`parseKeymap`); TypeScript types; and `gasm-headless` (`npx -p @emdzej/gasm-host gasm-headless game.wasm --headless 600`) |
| [`gasm-host`](https://crates.io/crates/gasm-host) (crates.io) | Native host library (wasmtime, wgpu, cpal, gilrs, WebSocket/TLS, storage) with complete headless and windowed runners. `cargo install gasm-host` installs `gasm-run`. Without the default `window` feature it builds headless-only (no winit/cpal/gilrs) |
| `ghcr.io/emdzej/gasm-relay` | Relay container image (amd64, arm64): `docker run -p 9000:9000 ghcr.io/emdzej/gasm-relay`. The relay is the crate `gasm-relay` in [`runners/native/relay`](https://github.com/emdzej/gasm/tree/main/runners/native/relay), not published on crates.io |
| Releases | Prebuilt `gasm-run`/`gasm-relay` for macOS (universal), Linux (x86_64, arm64), Windows, plus macOS `.app` bundles (Sumo, NES, Triangle, Test Pattern) and the games with their license notices |

Embedding in a web page:

```js
import { GasmHost } from '@emdzej/gasm-host';
import { WebGpuGfx } from '@emdzej/gasm-host/webgpu';

const host = new GasmHost({ gfx: await WebGpuGfx.create(canvas), getPad: () => keys });
await host.load(await (await fetch('game.wasm')).arrayBuffer());
// call host.frame() at host.frameRate Hz
```

Data-heavy games in a Worker, with data the page imported into OPFS once
(for example with [csfs](https://github.com/emdzej/csfs), as the gasm player's
`opfs.html` does):

```js
import { GasmWorker } from '@emdzej/gasm-host/worker';

const w = await GasmWorker.start({
  wasm, storage: 'mygame',
  assets: [{ kind: 'opfs', dir: 'gasm-assets/mygame-cd' }],   // read on demand, synchronously
  onAudio: (samples, rate, channels) => feedAudioWorklet(samples, rate, channels),
});
const r = await w.frames([[pad0, pad1, 0, 0]]);               // one entry per frame
if (r.frame) ctx.putImageData(new ImageData(r.frame.rgba, r.frame.width, r.frame.height), 0, 0);
```

3D (`gasm:gfx`) games can run in a Worker too, where the browser has WebGPU in
workers: pass `canvas: canvasElement.transferControlToOffscreen()` and
`size: [w, h]` (device pixels) to `start`, and `{ size }` to `frames`. The
worker renders straight into the canvas. If `start` rejects with a WebGPU
error, run the game on the main thread. `keyboard: true` plus
`frames(steps, true, { texts })` forwards typed text for `text_input`.

Raw keyboard, mouse and gamepads: `const input = new BrowserInput(canvas).attach()`,
then `host.input = input.frame(true)` before each frame (or
`frames(steps, true, { inputs })` in a Worker) and `input.setMode(host.inputMode)`
after it, which hides or captures the cursor as the game asked.

Embedding natively:

```rust
use std::collections::HashMap;
use gasm_host::{assets::Assets, gfx::Gfx, host::{Game, Host, LoadOptions}, net::Net, storage::Storage};

let host = Host::new(Assets::new(), HashMap::new(), None, Gfx::null(), Net::new(false), Storage::memory());
let mut game = Game::load(&std::fs::read("game.wasm")?, host, LoadOptions::default())?;
game.frame()?;
```

`LoadOptions` has `allow_precompiled` (accept a `.cwasm`; off by default) and
`call_timeout` (30 s by default). The complete runners are
`gasm_host::headless::run` and `gasm_host::window::run`; see
[Writing runners](/dev/runners#embedding-the-reference-runners).

## One ABI, generated bindings

[`spec/abi.json`](https://github.com/emdzej/gasm/blob/main/spec/abi.json) is the
machine-readable ABI: every module, function, parameter, result and constant.
`scripts/gen-abi.mjs` generates `spec/gasm.h` and the Rust raw imports
(`gasm::sys`) from it. `--check` (run in CI) fails if a generated file is stale,
or if the native runner, the JS runner or the native stub host implements a
function the spec doesn't list, or misses one it does. Bindings for another
language (Zig, AssemblyScript, TinyGo, C#) are one more generator over the same
file.

Versions: packages follow the release tag. The **ABI version** (`GASM_ABI_VERSION`,
currently `0`) is separate and checked at load time. A package works with any
runner that implements the same ABI version.
