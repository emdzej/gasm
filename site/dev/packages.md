# Packages

The ABI is published as libraries for game authors and for people who run or
embed games. Every artifact is built and published by
[`release.yml`](https://github.com/emdzej/gasm/blob/main/.github/workflows/release.yml)
when a version tag (like `0.2.0`, no `v`) is pushed. Registries use
**trusted publishing** (GitHub OIDC), so no long-lived tokens are stored.

## For game authors

| Language | Package | Install |
|---|---|---|
| Rust | [`gasm-sdk`](https://crates.io/crates/gasm-sdk) (library name `gasm`) | `gasm-sdk = "0.1"`, `crate-type = ["cdylib"]` |
| C / C++ | `gasm-c-sdk-<version>.zip` on [Releases](https://github.com/emdzej/gasm/releases) | `gasm.h` + CMake toolchain (wasi-sdk) + example |
| anything else | `gasm.h` / `abi.json` on Releases | bind the imports yourself; see below |

**Rust:**

```toml
[lib]
crate-type = ["cdylib"]

[dependencies]
gasm-sdk = "0.1"
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
| [`@emdzej/gasm-host`](https://www.npmjs.com/package/@emdzej/gasm-host) (npm) | Browser + Node host: `GasmHost`, WebGPU backend (`@emdzej/gasm-host/webgpu`), IndexedDB storage, TypeScript types, and `gasm-headless` (`npx -p @emdzej/gasm-host gasm-headless game.wasm --headless 600`) |
| [`gasm-host`](https://crates.io/crates/gasm-host) (crates.io) | Native host library (wasmtime, wgpu, cpal, gilrs, WebSocket/TLS, storage). `cargo install gasm-host` installs `gasm-run` and `gasm-relay` |
| `ghcr.io/emdzej/gasm-relay` | Relay container image (amd64, arm64): `docker run -p 9000:9000 ghcr.io/emdzej/gasm-relay` |
| Releases | Prebuilt `gasm-run`/`gasm-relay` for macOS (universal), Linux (x86_64, arm64), Windows, plus macOS `.app` bundles |

Embedding in a web page:

```js
import { GasmHost } from '@emdzej/gasm-host';
import { WebGpuGfx } from '@emdzej/gasm-host/webgpu';

const host = new GasmHost({ gfx: await WebGpuGfx.create(canvas), getPad: () => keys });
await host.load(await (await fetch('game.wasm')).arrayBuffer());
// call host.frame() at host.frameRate Hz
```

Embedding natively:

```rust
use std::collections::HashMap;
use gasm_host::{gfx::Gfx, host::{Game, Host}, net::Net, storage::Storage};

let host = Host::new(HashMap::new(), HashMap::new(), None, Gfx::null(), Net::new(false), Storage::memory());
let mut game = Game::load(&std::fs::read("game.wasm")?, host)?;
game.frame()?;
```

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
