# gasm-sdk

Rust SDK for writing games for **[gasm](https://gasm.emdzej.pl)**, a portable
game runtime on WebAssembly. A game compiles to one `.wasm` file that runs in
the native runner (wasmtime + wgpu), in the browser (WebGPU), and headless in
CI, bit-identically.

```toml
[lib]
crate-type = ["cdylib"]

[dependencies]
gasm-sdk = "0.14"
```

```rust
use gasm::Buttons;

struct Hello { t: u32 }

impl gasm::Game for Hello {
    fn init() -> Result<Self, String> {
        gasm::set_frame_rate(60.0);
        Ok(Hello { t: 0 })
    }
    fn frame(&mut self) {
        if gasm::pad(0).held(Buttons::A) { gasm::log!("A at frame {}", self.t); }
        self.t += 1;
    }
}

gasm::game!(Hello);
```

```sh
cargo build --release --target wasm32-unknown-unknown
gasm-run target/wasm32-unknown-unknown/release/hello.wasm
```

What's in it:

- **Core:** logging, time, frame rate, launch params, RGBA video, audio, four
  virtual gamepads, assets of any size (`asset_size`, `asset_read_at` with
  `u64` offsets), and `has(name)` to ask whether the runner provides an import.
- **`input`, `keys`:** raw keyboard, pointer, gamepads and joysticks.
- **`gfx`:** a WebGPU subset with WGSL shaders and JSON descriptors;
  `gfx::destroy` frees objects.
- **`gles`:** the OpenGL ES 3.0 C API on gasm:gl (`gles::get_proc_address`),
  for [glow](https://github.com/grovesNL/glow) with the gasm fork patched in
  (`[patch.crates-io] glow = { git = "https://github.com/emdzej/gasm" }`), so
  glow-based crates such as egui_glow run unchanged.
- **`net`:** WebSocket-style messages; **`fetch`:** HTTP requests made by the
  runner.
- **`clipboard`:** copy text, read what the player pastes; **`files`:** save
  files for the player (pictures, exports).
- **`manifest!`:** embed the capabilities manifest (requires, hosts, files),
  which runners check and ask about before the game starts.
- **`storage`:** per-game saves; `storage::try_set` says why a write failed
  (`storage::Error`: `Key`, `Size`, `Quota`, `Io`).
- **`main_loop!`:** for games with their own loop (`main_loop!(run)`, or
  `main_loop!(run, on_exit)` to export `gasm_exit` too), with Binaryen's
  Asyncify. Games using `game!` don't carry its export or imports.
- **`thread`, `sync`:** cooperative threads for `threaded_main_loop!` games
  (`thread::spawn`, `Mutex`, `Condvar`, `Semaphore`), deterministic: one wasm
  thread, switched with Asyncify.
- **Native builds:** on non-wasm targets the crate links an in-process stub
  host (`gasm::native`), so the same game builds natively for debugging and
  parity tests.

The crate is `gasm-sdk`; the library is named `gasm`, so code says `use gasm::…`.
Guide: [Writing games](https://gasm.emdzej.pl/dev/games) · ABI: [spec](https://gasm.emdzej.pl/docs/abi).
The raw bindings (`gasm::sys`) are generated from `spec/abi.json`.

MIT licensed.
