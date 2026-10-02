# gasm-host

Native host for **[gasm](https://gasm.emdzej.pl)** games. A game is one `.wasm`
file, and this crate runs it: wasmtime for the module, wgpu (Metal, Vulkan,
D3D12) for `gasm:gfx`, cpal audio, gilrs gamepads, WebSocket networking
(`ws://`/`wss://`, rustls) and per-game storage.

Assets are never preloaded: files (`--asset`) and whole folders
(`--asset-dir`, e.g. a mounted CD) are read on demand into guest memory, with
case-insensitive names. Keyboard layouts are configurable
(`--keymap`, `--print-keymap`); the default gives two players one keyboard.

It ships the `gasm-run` binary:

```sh
cargo install gasm-host
gasm-run game.wasm                       # play in a window
gasm-run game.wasm --headless 600        # CI: run 600 frames, print video/audio hashes
gasm-run game.wasm --compile game.cwasm  # ahead-of-time compile (no JIT at load)
gasm-run game.cwasm --allow-precompiled  # run it (native code: only files you compiled)
```

A guest call (init, a frame) that runs longer than `--call-timeout` (default
30 s, `0` = never) traps. The room relay for online play, `gasm-relay`, is a
separate crate in [`relay/`](https://github.com/emdzej/gasm/tree/main/runners/native/relay)
(release bundles and `ghcr.io/emdzej/gasm-relay`; not on crates.io).

And a library, for embedding games in your own app or building a runner for
another platform. It contains complete runners, `headless::run` (reproducible
runs with hashes) and `window::run` (winit window, cpal audio, gilrs gamepads,
keyboard layouts), and the parts they're made of:

```rust
use std::collections::HashMap;
use gasm_host::{assets::Assets, gfx::Gfx, host::{Game, Host, LoadOptions}, net::Net, storage::Storage};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let host = Host::new(Assets::new(), HashMap::new(), None, Gfx::null(), Net::new(false), Storage::memory());
    let mut game = Game::load(&std::fs::read("game.wasm")?, host, LoadOptions::default())?;
    for _ in 0..600 {
        game.frame()?;                    // call at game.host().frame_rate Hz
    }
    let h = game.host();
    println!("{}x{}, video hash {:08x}", h.width, h.height, h.video_hash.0);
    Ok(())
}
```

`LoadOptions` sets `allow_precompiled` and `call_timeout`; audio output is the
`audio::AudioOut` trait. The windowed runner is the default feature `window`;
`default-features = false` gives a headless-only library without winit, cpal
and gilrs.

Guests are sandboxed: they see only their own memory and the ABI. Every
pointer and handle is checked. There's no filesystem (the runner implements
its own WASI subset: no preopened directories, env or args), network is
opt-in, and the storage namespace is chosen by the host.

- ABI: [spec](https://gasm.emdzej.pl/docs/abi) (machine-readable: `spec/abi.json`)
- Guide: [Writing runners](https://gasm.emdzej.pl/dev/runners)
- Writing games: the [`gasm-sdk`](https://crates.io/crates/gasm-sdk) crate

On Linux, building needs `libasound2-dev libudev-dev pkg-config`. MIT licensed.
