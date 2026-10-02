//! A game ready to start: the module, its assets, parameters, storage and load
//! options. [`crate::headless::run`] and `window::run` (feature `window`) drive it.

use std::collections::HashMap;
use std::time::Instant;

use crate::assets::Assets;
use crate::audio::AudioOut;
use crate::gfx::Gfx;
use crate::host::{Game, Host, LoadOptions, Stop};
use crate::net::Net;
use crate::storage::Storage;

pub struct Session {
    /// shown in logs and the window title
    pub name: String,
    /// a .wasm module, or a .cwasm if `load.allow_precompiled`
    pub wasm: Vec<u8>,
    pub assets: Assets,
    pub params: HashMap<String, String>,
    pub allow_net: bool,
    pub storage: Storage,
    pub load: LoadOptions,
}

impl Session {
    /// Instantiate the guest and run its init. `reproducible`: headless rules
    /// (virtual time, fixed random sequence, hashing).
    pub fn start(self, audio: Option<Box<dyn AudioOut>>, gfx: Gfx, reproducible: bool, hashing: bool) -> Result<Game, Stop> {
        let mut host = Host::new(self.assets, self.params, audio, gfx, Net::new(self.allow_net), self.storage);
        if reproducible {
            host.set_reproducible();
        }
        host.hashing = hashing;
        let t0 = Instant::now();
        let game = Game::load(&self.wasm, host, self.load)?;
        eprintln!("[gasm] loaded {} in {:.0} ms", self.name, t0.elapsed().as_secs_f64() * 1000.0);
        Ok(game)
    }
}
