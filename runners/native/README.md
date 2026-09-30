# gasm-host

Native host for **[gasm](https://gasm.emdzej.pl)** games. A game is one `.wasm`
file, and this crate runs it: wasmtime for the module, wgpu (Metal, Vulkan,
D3D12) for `gasm:gfx`, cpal audio, gilrs gamepads, WebSocket networking
(`ws://`/`wss://`, rustls) and per-game storage.

It ships two binaries:

```sh
cargo install gasm-host
gasm-run game.wasm                       # play in a window
gasm-run game.wasm --headless 600        # CI: run 600 frames, print video/audio hashes
gasm-run game.wasm --compile game.cwasm  # ahead-of-time compile (no JIT at load)
gasm-relay 0.0.0.0:9000                  # WebSocket room relay for online play
```

And a library, for embedding games in your own app or building a runner for
another platform:

```rust
use std::collections::HashMap;
use gasm_host::{gfx::Gfx, host::{Game, Host}, net::Net, storage::Storage};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let host = Host::new(HashMap::new(), HashMap::new(), None, Gfx::null(), Net::new(false), Storage::memory());
    let mut game = Game::load(&std::fs::read("game.wasm")?, host)?;
    for _ in 0..600 {
        game.frame()?;                    // call at game.host().frame_rate Hz
    }
    let h = game.host();
    println!("{}x{}, video hash {:08x}", h.width, h.height, h.video_hash.0);
    Ok(())
}
```

Guests are sandboxed: they see only their own memory and the ABI. Every
pointer and handle is checked. There's no filesystem, network is opt-in, and
the storage namespace is chosen by the host.

- ABI: [spec](https://gasm.emdzej.pl/docs/abi) (machine-readable: `spec/abi.json`)
- Guide: [Writing runners](https://gasm.emdzej.pl/dev/runners)
- Writing games: the [`gasm-sdk`](https://crates.io/crates/gasm-sdk) crate

On Linux, building needs `libasound2-dev libudev-dev pkg-config`. MIT licensed.
