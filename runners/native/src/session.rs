//! A game ready to start: the module, its assets, parameters, storage and load
//! options. [`crate::headless::run`] and `window::run` (feature `window`) drive it.

use std::collections::HashMap;
use std::time::Instant;

use crate::assets::Assets;
use crate::audio::AudioOut;
use crate::angle::Angle;
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
    /// with `allow_net`: only these hosts (`--allow-net=a.org,*.b.org`; empty: all)
    pub allow_hosts: Vec<String>,
    /// gasm:fetch: live, recording or replaying responses (headless)
    pub fetch: crate::fetch::FetchMode,
    pub storage: Storage,
    pub load: LoadOptions,
    /// where ANGLE is (gasm:gl games): None searches the usual places (`angle::find`)
    pub gl_lib: Option<std::path::PathBuf>,
    /// gasm:gl on SwiftShader (software Vulkan) instead of the GPU
    pub gl_software: bool,
    /// `--watch-asset name=path`: files re-read as assets whenever they change
    /// (create each watch before opening its file, so no change is missed)
    pub watch_assets: Vec<crate::assets::AssetWatch>,
}

impl Session {
    /// Whether the game imports gasm:gl (it then needs ANGLE to draw).
    pub fn uses_gl(&self) -> bool {
        crate::host::imports_module(&self.wasm, "gasm:gl", self.load.allow_precompiled)
    }

    /// ANGLE for this session's gasm:gl game: offscreen at `w`×`h`, or in a window.
    pub fn open_gl(&self, window: Option<crate::angle::NativeWindow>, size: (u32, u32)) -> Result<Angle, String> {
        let dir = crate::angle::find(self.gl_lib.as_deref())?;
        let open = |software| match window {
            Some(w) => Angle::window(&dir, w, software),
            None => Angle::offscreen(&dir, size.0, size.1, software),
        };
        match open(self.gl_software) {
            // no usable GPU (or driver): SwiftShader
            Err(e) if !self.gl_software => {
                eprintln!("[gasm] gl: {e}; trying SwiftShader (software)");
                open(true)
            }
            r => r,
        }
    }

    /// Instantiate the guest and run its init. `reproducible`: headless rules
    /// (virtual time, fixed random sequence, hashing). `gl`: the GL that executes a
    /// gasm:gl guest's calls (None: the null GL).
    /// Compile the module on a background thread (the window runner shows the splash
    /// meanwhile); hand the result to [`Session::start_compiled`].
    pub fn compile_in_background(&self) -> std::thread::JoinHandle<Result<wasmtime::Module, Stop>> {
        let (wasm, allow) = (self.wasm.clone(), self.load.allow_precompiled);
        std::thread::spawn(move || Game::compile(&wasm, allow))
    }

    pub fn start(self, audio: Option<Box<dyn AudioOut>>, gfx: Gfx, gl: Option<Angle>, reproducible: bool, hashing: bool) -> Result<Game, Stop> {
        self.start_with(None, audio, gfx, gl, reproducible, hashing)
    }

    /// [`Session::start`] with a module compiled earlier (`compile_in_background`).
    pub fn start_compiled(self, module: wasmtime::Module, audio: Option<Box<dyn AudioOut>>, gfx: Gfx, gl: Option<Angle>) -> Result<Game, Stop> {
        self.start_with(Some(module), audio, gfx, gl, false, false)
    }

    fn start_with(self, module: Option<wasmtime::Module>, audio: Option<Box<dyn AudioOut>>, gfx: Gfx, gl: Option<Angle>, reproducible: bool, hashing: bool) -> Result<Game, Stop> {
        let policy = crate::net::NetPolicy::new(self.allow_net, self.allow_hosts);
        let mut host = Host::new(self.assets, self.params, audio, gfx, Net::with_policy(policy.clone()), self.storage);
        host.fetch = crate::fetch::Fetch::new(policy, self.fetch);
        if let Some(a) = gl {
            eprintln!("[gasm] gl: {}", a.renderer);
            host.gl.attach(a);
        }
        if reproducible {
            host.set_reproducible();
        }
        host.hashing = hashing;
        let t0 = Instant::now();
        let module = match module {
            Some(m) => m,
            None => Game::compile(&self.wasm, self.load.allow_precompiled)?,
        };
        let mut game = Game::load_module(module, host, self.load)?;
        for w in self.watch_assets {
            game.add_watch(w);
        }
        eprintln!("[gasm] loaded {} in {:.0} ms", self.name, t0.elapsed().as_secs_f64() * 1000.0);
        Ok(game)
    }
}
