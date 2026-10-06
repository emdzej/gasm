//! Native host for [gasm](https://gasm.emdzej.pl) games.
//!
//! Load a guest `.wasm` with [`host::Game::load`], drive it with
//! [`host::Game::frame`] at the guest's frame rate, and plug in the pieces you
//! need: [`assets::Assets`] (memory, file-backed, folders), [`gfx::Gfx`] (wgpu:
//! window, offscreen or null), [`audio::AudioOut`], [`net::Net`] (WebSocket,
//! `ws://`/`wss://`), [`storage::Storage`] (per-game key/value).
//!
//! Complete runners built from these parts: [`headless::run`] (reproducible
//! runs with hashes) and, with the `window` feature (default), `window::run`
//! (winit window, cpal audio, gilrs gamepads, keyboard layouts). The `gasm-run`
//! binary is their command line.
//!
//! ABI: <https://gasm.emdzej.pl/docs/abi> (machine-readable: `spec/abi.json`).

pub mod angle;
pub mod assets;
pub mod audio;
pub mod consent;
pub mod fetch;
pub mod files;
pub mod gfx;
pub mod gl;
pub mod gles;
pub mod headless;
pub mod host;
pub mod keys;
pub mod manifest;
pub mod net;
pub mod present;
pub mod script;
pub mod session;
pub mod splash;
pub mod storage;
pub mod switching;
pub mod wasi;

#[cfg(feature = "window")]
pub mod keymap;
#[cfg(feature = "window")]
pub mod window;
#[cfg(feature = "window")]
pub mod prompt;
