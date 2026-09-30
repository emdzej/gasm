# gasm-sdk

Rust SDK for writing games for **[gasm](https://gasm.emdzej.pl)**, a portable
game runtime on WebAssembly. A game compiles to one `.wasm` file that runs in
the native runner (wasmtime + wgpu), in the browser (WebGPU), and headless in
CI, bit-identically.

```toml
[lib]
crate-type = ["cdylib"]

[dependencies]
gasm-sdk = "0.1"
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
  virtual gamepads, assets.
- **`gfx`:** a WebGPU subset with WGSL shaders and JSON descriptors.
- **`net`:** WebSocket-style messages.
- **`storage`:** per-game saves.
- **Native builds:** on non-wasm targets the crate links an in-process stub
  host (`gasm::native`), so the same game builds natively for debugging and
  parity tests.

The crate is `gasm-sdk`; the library is named `gasm`, so code says `use gasm::…`.
Guide: [Writing games](https://gasm.emdzej.pl/dev/games) · ABI: [spec](https://gasm.emdzej.pl/docs/abi).
The raw bindings (`gasm::sys`) are generated from `spec/abi.json`.

MIT licensed.
