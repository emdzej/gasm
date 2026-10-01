//! Native host for [gasm](https://gasm.emdzej.pl) games.
//!
//! Load a guest `.wasm` with [`host::Game::load`], drive it with
//! [`host::Game::frame`] at the guest's frame rate, and plug in the pieces you
//! need: [`assets::Assets`] (memory, file-backed, folders), [`gfx::Gfx`] (wgpu: window, offscreen or null), [`audio::AudioSink`]
//! (cpal), [`net::Net`] (WebSocket, `ws://`/`wss://`), [`storage::Storage`]
//! (per-game key/value). The `gasm-run` binary in this crate is a complete
//! runner built from these parts; `gasm-relay` is the room relay.
//!
//! ABI: <https://gasm.emdzej.pl/docs/abi> (machine-readable: `spec/abi.json`).

pub mod assets;
pub mod audio;
pub mod gfx;
pub mod host;
pub mod keymap;
pub mod net;
pub mod script;
pub mod storage;
