//! The gasm ABI v0 host: instantiates a guest and implements the "gasm",
//! "gasm:gfx" and "gasm:net" imports.

use std::collections::HashMap;
use std::time::Instant;

use wasmtime::{Caller, Engine, Instance, Linker, Memory, Module, Store, TypedFunc, bail, format_err};
use wasmtime_wasi::WasiCtx;
use wasmtime_wasi::p1::{self, WasiP1Ctx};

use crate::audio::AudioSink;
use crate::gfx::Gfx;
use crate::net::Net;

pub const ABI_VERSION: i32 = 0;

/// FNV-1a 32-bit; mirrored in runners/web/gasm-host.js for cross-runner checks.
#[derive(Clone, Copy)]
pub struct Fnv(pub u32);

impl Fnv {
    pub fn new() -> Self {
        Fnv(0x811c_9dc5)
    }
    pub fn update(&mut self, bytes: &[u8]) {
        let mut h = self.0;
        for &b in bytes {
            h ^= b as u32;
            h = h.wrapping_mul(0x0100_0193);
        }
        self.0 = h;
    }
}

pub struct Host {
    wasi: WasiP1Ctx,
    memory: Option<Memory>,
    start: Instant,
    /// Headless runs use frame-derived time so results are reproducible.
    pub virtual_time_ms: Option<f64>,
    pub frame_rate: f64,
    pub pads: [u32; 4],
    pub assets: HashMap<String, Vec<u8>>,
    pub params: HashMap<String, String>,
    pub audio: Option<AudioSink>,
    pub gfx: Gfx,
    pub net: Net,
    /// false during catch-up frames: gfx begin_frame returns 0
    pub show_frame: bool,
    audio_rate: u32,
    audio_channels: u32,
    /// Last presented frame as RGBA8, tightly packed.
    pub rgba: Vec<u8>,
    pub width: usize,
    pub height: usize,
    pub frames_presented: u64,
    pub hashing: bool,
    pub video_hash: Fnv,
    pub audio_hash: Fnv,
    pub audio_frames: u64,
}

impl Host {
    pub fn new(
        assets: HashMap<String, Vec<u8>>,
        params: HashMap<String, String>,
        audio: Option<AudioSink>,
        gfx: Gfx,
        net: Net,
    ) -> Self {
        let wasi = WasiCtx::builder().inherit_stdout().inherit_stderr().build_p1();
        Host {
            wasi,
            memory: None,
            start: Instant::now(),
            virtual_time_ms: None,
            frame_rate: 60.0,
            pads: [0; 4],
            assets,
            params,
            audio,
            gfx,
            net,
            show_frame: true,
            audio_rate: 44100,
            audio_channels: 2,
            rgba: Vec::new(),
            width: 0,
            height: 0,
            frames_presented: 0,
            hashing: true,
            video_hash: Fnv::new(),
            audio_hash: Fnv::new(),
            audio_frames: 0,
        }
    }
}

/// Borrow `len` bytes at guest offset `ptr`, trapping the guest if out of bounds.
fn guest_slice<'a>(mem: &'a [u8], ptr: u32, len: u64) -> wasmtime::Result<&'a [u8]> {
    let start = ptr as u64;
    let end = start.checked_add(len).ok_or_else(|| format_err!("guest pointer overflow"))?;
    if end > mem.len() as u64 {
        bail!("guest access out of bounds: {start:#x}..{end:#x} (memory is {:#x})", mem.len());
    }
    Ok(&mem[start as usize..end as usize])
}

fn memory(caller: &Caller<'_, Host>) -> wasmtime::Result<Memory> {
    caller.data().memory.ok_or_else(|| format_err!("guest memory not yet available"))
}

fn guest_str(caller: &Caller<'_, Host>, ptr: u32, len: u32) -> wasmtime::Result<String> {
    let mem = memory(caller)?;
    let bytes = guest_slice(mem.data(caller), ptr, len as u64)?;
    Ok(String::from_utf8_lossy(bytes).into_owned())
}

fn add_gasm_imports(linker: &mut Linker<Host>) -> wasmtime::Result<()> {
    linker.func_wrap("gasm", "log", |caller: Caller<'_, Host>, ptr: u32, len: u32| {
        let msg = guest_str(&caller, ptr, len)?;
        eprintln!("[guest] {msg}");
        Ok(())
    })?;

    linker.func_wrap("gasm", "time_ms", |caller: Caller<'_, Host>| -> f64 {
        let h = caller.data();
        h.virtual_time_ms.unwrap_or_else(|| h.start.elapsed().as_secs_f64() * 1000.0)
    })?;

    linker.func_wrap("gasm", "set_frame_rate", |mut caller: Caller<'_, Host>, hz: f64| {
        if hz.is_finite() && (1.0..=1000.0).contains(&hz) {
            caller.data_mut().frame_rate = hz;
        }
    })?;

    linker.func_wrap(
        "gasm",
        "video_present",
        |mut caller: Caller<'_, Host>, ptr: u32, w: u32, h: u32, stride: u32| -> wasmtime::Result<()> {
            let (w, h, stride) = (w as usize, h as usize, stride as usize);
            if w == 0 || h == 0 || w > 4096 || h > 4096 || stride < w * 4 {
                bail!("video_present: bad geometry {w}x{h} stride {stride}");
            }
            let mem = memory(&caller)?;
            let (data, host) = mem.data_and_store_mut(&mut caller);
            let src = guest_slice(data, ptr, (stride * (h - 1) + w * 4) as u64)?;
            host.rgba.resize(w * h * 4, 0);
            for y in 0..h {
                let row = &src[y * stride..y * stride + w * 4];
                host.rgba[y * w * 4..(y + 1) * w * 4].copy_from_slice(row);
            }
            if host.hashing {
                host.video_hash.update(&host.rgba);
            }
            host.width = w;
            host.height = h;
            host.frames_presented += 1;
            Ok(())
        },
    )?;

    linker.func_wrap("gasm", "audio_config", |mut caller: Caller<'_, Host>, rate: u32, channels: u32| {
        let h = caller.data_mut();
        if (8000..=192_000).contains(&rate) && (channels == 1 || channels == 2) {
            h.audio_rate = rate;
            h.audio_channels = channels;
            if let Some(a) = &mut h.audio {
                a.configure(rate, channels);
            }
        }
    })?;

    linker.func_wrap(
        "gasm",
        "audio_push",
        |mut caller: Caller<'_, Host>, ptr: u32, frames: u32| -> wasmtime::Result<()> {
            let mem = memory(&caller)?;
            let (data, host) = mem.data_and_store_mut(&mut caller);
            let len = frames as u64 * host.audio_channels as u64 * 4;
            let bytes = guest_slice(data, ptr, len)?;
            if host.hashing {
                host.audio_hash.update(bytes);
            }
            host.audio_frames += frames as u64;
            if let Some(a) = &mut host.audio {
                a.push_le_f32(bytes);
            }
            Ok(())
        },
    )?;

    linker.func_wrap("gasm", "input_pad", |caller: Caller<'_, Host>, player: u32| -> u32 {
        caller.data().pads.get(player as usize).copied().unwrap_or(0)
    })?;

    linker.func_wrap(
        "gasm",
        "param",
        |mut caller: Caller<'_, Host>, ptr: u32, len: u32, dst: u32, cap: u32| -> wasmtime::Result<i32> {
            let name = guest_str(&caller, ptr, len)?;
            let mem = memory(&caller)?;
            let (data, host) = mem.data_and_store_mut(&mut caller);
            let Some(v) = host.params.get(&name) else { return Ok(-1) };
            let v = v.as_bytes();
            if v.len() <= cap as usize {
                guest_slice(data, dst, v.len() as u64)?;
                data[dst as usize..dst as usize + v.len()].copy_from_slice(v);
            }
            Ok(v.len() as i32)
        },
    )?;

    linker.func_wrap(
        "gasm",
        "asset_size",
        |caller: Caller<'_, Host>, ptr: u32, len: u32| -> wasmtime::Result<i32> {
            let name = guest_str(&caller, ptr, len)?;
            Ok(caller.data().assets.get(&name).map_or(-1, |a| a.len() as i32))
        },
    )?;

    linker.func_wrap(
        "gasm",
        "asset_read",
        |mut caller: Caller<'_, Host>, ptr: u32, len: u32, dst: u32, cap: u32| -> wasmtime::Result<i32> {
            let name = guest_str(&caller, ptr, len)?;
            let mem = memory(&caller)?;
            let (data, host) = mem.data_and_store_mut(&mut caller);
            let Some(asset) = host.assets.get(&name) else { return Ok(-1) };
            let n = asset.len().min(cap as usize);
            guest_slice(data, dst, n as u64)?; // bounds check
            data[dst as usize..dst as usize + n].copy_from_slice(&asset[..n]);
            Ok(n as i32)
        },
    )?;
    Ok(())
}

/// Ahead-of-time compile a guest to native code for this host (no JIT needed at load).
pub fn precompile(wasm: &[u8]) -> wasmtime::Result<Vec<u8>> {
    Engine::default().precompile_module(wasm)
}

fn trap(e: String) -> wasmtime::Error {
    format_err!("{e}")
}

fn add_gfx_imports(linker: &mut Linker<Host>) -> wasmtime::Result<()> {
    const M: &str = "gasm:gfx";
    linker.func_wrap(M, "width", |c: Caller<'_, Host>| c.data().gfx.size().0)?;
    linker.func_wrap(M, "height", |c: Caller<'_, Host>| c.data().gfx.size().1)?;
    linker.func_wrap(M, "create_shader", |mut c: Caller<'_, Host>, ptr: u32, len: u32| -> wasmtime::Result<u32> {
        let src = guest_str(&c, ptr, len)?;
        c.data_mut().gfx.create_shader(&src).map_err(trap)
    })?;
    linker.func_wrap(M, "create_buffer", |mut c: Caller<'_, Host>, size: u32, usage: u32| -> wasmtime::Result<u32> {
        c.data_mut().gfx.create_buffer(size, usage).map_err(trap)
    })?;
    linker.func_wrap(M, "create_pipeline", |mut c: Caller<'_, Host>, ptr: u32, len: u32| -> wasmtime::Result<u32> {
        let json = guest_str(&c, ptr, len)?;
        c.data_mut().gfx.create_pipeline(&json).map_err(trap)
    })?;
    linker.func_wrap(M, "create_bind_group", |mut c: Caller<'_, Host>, ptr: u32, len: u32| -> wasmtime::Result<u32> {
        let json = guest_str(&c, ptr, len)?;
        c.data_mut().gfx.create_bind_group(&json).map_err(trap)
    })?;
    linker.func_wrap(
        M,
        "write_buffer",
        |mut c: Caller<'_, Host>, buf: u32, offset: u32, ptr: u32, len: u32| -> wasmtime::Result<()> {
            let mem = memory(&c)?;
            let (data, host) = mem.data_and_store_mut(&mut c);
            let bytes = guest_slice(data, ptr, len as u64)?;
            if host.hashing {
                host.video_hash.update(bytes);
            }
            host.gfx.write_buffer(buf, offset, bytes).map_err(trap)
        },
    )?;
    linker.func_wrap(M, "begin_frame", |mut c: Caller<'_, Host>, r: f32, g: f32, b: f32, a: f32| -> wasmtime::Result<u32> {
        let h = c.data_mut();
        let show = h.show_frame;
        h.gfx.begin_frame([r, g, b, a], show).map(u32::from).map_err(trap)
    })?;
    linker.func_wrap(M, "set_pipeline", |mut c: Caller<'_, Host>, p: u32| -> wasmtime::Result<()> {
        c.data_mut().gfx.set_pipeline(p).map_err(trap)
    })?;
    linker.func_wrap(M, "set_bind_group", |mut c: Caller<'_, Host>, i: u32, bg: u32| -> wasmtime::Result<()> {
        c.data_mut().gfx.set_bind_group(i, bg).map_err(trap)
    })?;
    linker.func_wrap(M, "set_vertex_buffer", |mut c: Caller<'_, Host>, slot: u32, b: u32, off: u32| -> wasmtime::Result<()> {
        c.data_mut().gfx.set_vertex_buffer(slot, b, off).map_err(trap)
    })?;
    linker.func_wrap(M, "set_index_buffer", |mut c: Caller<'_, Host>, b: u32, fmt: u32, off: u32| -> wasmtime::Result<()> {
        c.data_mut().gfx.set_index_buffer(b, fmt, off).map_err(trap)
    })?;
    linker.func_wrap(M, "draw", |mut c: Caller<'_, Host>, vc: u32, ic: u32, fv: u32, fi: u32| {
        c.data_mut().gfx.draw(vc, ic, fv, fi)
    })?;
    linker.func_wrap(M, "draw_indexed", |mut c: Caller<'_, Host>, ic: u32, n: u32, first: u32, base: i32, fi: u32| {
        c.data_mut().gfx.draw_indexed(ic, n, first, base, fi)
    })?;
    linker.func_wrap(M, "end_frame", |mut c: Caller<'_, Host>| -> wasmtime::Result<()> {
        let h = c.data_mut();
        h.frames_presented += 1;
        h.gfx.end_frame().map_err(trap)
    })?;
    Ok(())
}

fn add_net_imports(linker: &mut Linker<Host>) -> wasmtime::Result<()> {
    const M: &str = "gasm:net";
    linker.func_wrap(M, "open", |mut c: Caller<'_, Host>, ptr: u32, len: u32| -> wasmtime::Result<i32> {
        let url = guest_str(&c, ptr, len)?;
        Ok(c.data_mut().net.open(&url))
    })?;
    linker.func_wrap(M, "state", |c: Caller<'_, Host>, h: i32| c.data().net.state(h))?;
    linker.func_wrap(M, "send", |mut c: Caller<'_, Host>, h: i32, ptr: u32, len: u32| -> wasmtime::Result<i32> {
        if len == 0 {
            return Ok(-1);
        }
        let mem = memory(&c)?;
        let msg = guest_slice(mem.data(&c), ptr, len as u64)?.to_vec();
        Ok(c.data_mut().net.send(h, msg))
    })?;
    linker.func_wrap(M, "recv", |mut c: Caller<'_, Host>, h: i32, dst: u32, cap: u32| -> wasmtime::Result<i32> {
        let mem = memory(&c)?;
        let (data, host) = mem.data_and_store_mut(&mut c);
        let msg = match host.net.peek(h) {
            Ok(m) => m,
            Err(code) => return Ok(code),
        };
        let n = msg.len();
        if n <= cap as usize {
            guest_slice(data, dst, n as u64)?;
            data[dst as usize..dst as usize + n].copy_from_slice(msg);
            host.net.pop(h);
        }
        Ok(n as i32)
    })?;
    linker.func_wrap(M, "close", |mut c: Caller<'_, Host>, h: i32| c.data_mut().net.close(h))?;
    Ok(())
}

/// Why a guest call ended.
pub enum Stop {
    /// The guest called proc_exit(code).
    Exit(i32),
    Trap(String),
}

fn classify(e: wasmtime::Error) -> Stop {
    match e.downcast_ref::<wasmtime_wasi::I32Exit>() {
        Some(exit) => Stop::Exit(exit.0),
        None => Stop::Trap(format!("{e:?}")),
    }
}

pub struct Game {
    pub store: Store<Host>,
    frame: TypedFunc<(), ()>,
}

impl Game {
    pub fn load(wasm: &[u8], host: Host) -> Result<Game, Stop> {
        Self::load_inner(wasm, host).map_err(classify)
    }

    fn load_inner(wasm: &[u8], host: Host) -> wasmtime::Result<Game> {
        let engine = Engine::default();
        let module = if wasm.starts_with(b"\0asm") {
            Module::new(&engine, wasm)?
        } else {
            // Precompiled artifact from `gasm-run --compile`. Only load files you
            // produced yourself: native code is trusted, unlike .wasm.
            unsafe { Module::deserialize(&engine, wasm)? }
        };
        let mut linker: Linker<Host> = Linker::new(&engine);
        p1::add_to_linker_sync(&mut linker, |h: &mut Host| &mut h.wasi)?;
        add_gasm_imports(&mut linker)?;
        add_gfx_imports(&mut linker)?;
        add_net_imports(&mut linker)?;
        // Imports this runner doesn't know (e.g. JS glue some Rust crates pull in)
        // link as traps: harmless unless the guest actually calls them.
        linker.define_unknown_imports_as_traps(&module)?;

        let mut store = Store::new(&engine, host);
        let instance: Instance = linker.instantiate(&mut store, &module)?;
        let memory = instance
            .get_memory(&mut store, "memory")
            .ok_or_else(|| format_err!("guest does not export `memory`"))?;
        store.data_mut().memory = Some(memory);

        if let Ok(init) = instance.get_typed_func::<(), ()>(&mut store, "_initialize") {
            init.call(&mut store, ())?;
        }
        let version = instance
            .get_typed_func::<(), i32>(&mut store, "gasm_abi_version")?
            .call(&mut store, ())?;
        if version != ABI_VERSION {
            bail!("guest targets gasm ABI v{version}, runner implements v{ABI_VERSION}");
        }
        let rc = instance.get_typed_func::<(), i32>(&mut store, "gasm_init")?.call(&mut store, ())?;
        if rc != 0 {
            bail!("gasm_init failed with code {rc}");
        }
        let frame = instance.get_typed_func::<(), ()>(&mut store, "gasm_frame")?;
        Ok(Game { store, frame })
    }

    pub fn frame(&mut self) -> Result<(), Stop> {
        self.frame.call(&mut self.store, ()).map_err(classify)
    }

    pub fn host(&self) -> &Host {
        self.store.data()
    }

    pub fn host_mut(&mut self) -> &mut Host {
        self.store.data_mut()
    }
}
