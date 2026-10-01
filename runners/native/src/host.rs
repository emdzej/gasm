//! The gasm ABI v0 host: instantiates a guest and implements the "gasm",
//! "gasm:gfx" and "gasm:net" imports.

use std::collections::HashMap;
use std::time::Instant;

use wasmtime::{Caller, Engine, Instance, Linker, Memory, Module, Store, TypedFunc, bail, format_err};
use wasmtime_wasi::WasiCtx;
use wasmtime_wasi::p1::{self, WasiP1Ctx};

use crate::assets::Assets;
use crate::audio::AudioSink;
use crate::gfx::Gfx;
use crate::net::Net;
use crate::storage::Storage;

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

/// Raw input for one frame, set by the runner before each `gasm_frame`.
/// `None` fields mean the runner has no such device (the import returns -1).
#[derive(Default)]
pub struct RawInput {
    /// held keys, bit k = GASM_KEY code k
    pub keys: Option<[u8; KEY_STATE_BYTES]>,
    /// (code, down) since the previous frame, in order
    pub key_events: Vec<(u16, bool)>,
    pub pointer: Option<Pointer>,
    pub gamepads: Option<[Gamepad; 4]>,
}

pub const KEY_STATE_BYTES: usize = 32;
pub const POINTER_BYTES: usize = 48;
pub const GAMEPAD_BYTES: usize = 204;
pub const GAMEPAD_BUTTONS: usize = 32;
pub const GAMEPAD_AXES: usize = 16;

#[derive(Default, Clone, Copy)]
pub struct Pointer {
    /// position in drawable pixels; the frame position is derived by the host
    pub x: f32,
    pub y: f32,
    pub dx: f32,
    pub dy: f32,
    pub wheel_x: f32,
    pub wheel_y: f32,
    pub buttons: u32,
    pub pressed: u32,
    pub released: u32,
    /// GASM_POINTER_INSIDE | IS_HIDDEN | IS_LOCKED
    pub flags: u32,
    /// drawable size the position refers to
    pub drawable: (f32, f32),
}

#[derive(Default, Clone)]
pub struct Gamepad {
    pub connected: bool,
    pub standard: bool,
    pub buttons: Vec<f32>,
    pub axes: Vec<f32>,
    pub name: String,
}

/// Map a drawable position into the last video_present frame (letterboxed as the
/// runners display it). Same arithmetic in runners/web/gasm-host.js.
pub fn frame_position(x: f32, y: f32, drawable: (f32, f32), frame: (usize, usize)) -> (f32, f32) {
    if frame.0 == 0 || frame.1 == 0 || drawable.0 <= 0.0 || drawable.1 <= 0.0 {
        return (x, y);
    }
    let (dw, dh, fw, fh) = (drawable.0 as f64, drawable.1 as f64, frame.0 as f64, frame.1 as f64);
    let scale = (dw / fw).min(dh / fh);
    let (ox, oy) = ((dw - fw * scale) / 2.0, (dh - fh * scale) / 2.0);
    (((x as f64 - ox) / scale) as f32, ((y as f64 - oy) / scale) as f32)
}

pub struct Host {
    wasi: WasiP1Ctx,
    memory: Option<Memory>,
    start: Instant,
    /// Headless runs use frame-derived time so results are reproducible.
    pub virtual_time_ms: Option<f64>,
    pub frame_rate: f64,
    pub pads: [u32; 4],
    /// UTF-8 typed since the previous frame (set by the runner before each frame);
    /// None: no keyboard, text_input returns -1
    pub text: Option<String>,
    pub input: RawInput,
    /// GASM_INPUT_* flags requested by the guest (input_mode)
    pub input_mode: u32,
    pub assets: Assets,
    /// sorted asset names, computed on first asset_count/asset_name
    asset_names: Option<Vec<String>>,
    pub params: HashMap<String, String>,
    pub audio: Option<AudioSink>,
    pub gfx: Gfx,
    pub net: Net,
    pub storage: Storage,
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
        assets: impl Into<Assets>,
        params: HashMap<String, String>,
        audio: Option<AudioSink>,
        gfx: Gfx,
        net: Net,
        storage: Storage,
    ) -> Self {
        let wasi = WasiCtx::builder().inherit_stdout().inherit_stderr().build_p1();
        Host {
            wasi,
            memory: None,
            start: Instant::now(),
            virtual_time_ms: None,
            frame_rate: 60.0,
            pads: [0; 4],
            text: None,
            input: RawInput::default(),
            input_mode: 0,
            assets: assets.into(),
            asset_names: None,
            params,
            audio,
            gfx,
            net,
            storage,
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
        "text_input",
        |mut caller: Caller<'_, Host>, dst: u32, cap: u32| -> wasmtime::Result<i32> {
            let mem = memory(&caller)?;
            let (data, host) = mem.data_and_store_mut(&mut caller);
            let Some(t) = &host.text else { return Ok(-1) };
            let t = t.as_bytes();
            if t.len() <= cap as usize {
                guest_slice(data, dst, t.len() as u64)?;
                data[dst as usize..dst as usize + t.len()].copy_from_slice(t);
            }
            Ok(t.len() as i32)
        },
    )?;

    linker.func_wrap("gasm", "input_mode", |mut caller: Caller<'_, Host>, flags: u32| {
        caller.data_mut().input_mode = flags & 7;
    })?;

    linker.func_wrap(
        "gasm",
        "key_state",
        |mut caller: Caller<'_, Host>, dst: u32, len: u32| -> wasmtime::Result<i32> {
            let mem = memory(&caller)?;
            let (data, host) = mem.data_and_store_mut(&mut caller);
            let Some(keys) = host.input.keys else { return Ok(-1) };
            let n = (len as usize).min(KEY_STATE_BYTES);
            guest_slice(data, dst, n as u64)?;
            data[dst as usize..dst as usize + n].copy_from_slice(&keys[..n]);
            Ok(KEY_STATE_BYTES as i32)
        },
    )?;

    linker.func_wrap(
        "gasm",
        "key_events",
        |mut caller: Caller<'_, Host>, dst: u32, cap: u32| -> wasmtime::Result<i32> {
            let mem = memory(&caller)?;
            let (data, host) = mem.data_and_store_mut(&mut caller);
            if host.input.keys.is_none() {
                return Ok(-1);
            }
            let bytes: Vec<u8> = host.input.key_events.iter().flat_map(|&(c, d)| { let c = c.to_le_bytes(); [c[0], c[1], d as u8, 0] }).collect();
            if bytes.len() <= cap as usize {
                guest_slice(data, dst, bytes.len() as u64)?;
                data[dst as usize..dst as usize + bytes.len()].copy_from_slice(&bytes);
            }
            Ok(bytes.len() as i32)
        },
    )?;

    linker.func_wrap(
        "gasm",
        "pointer",
        |mut caller: Caller<'_, Host>, dst: u32, cap: u32| -> wasmtime::Result<i32> {
            let mem = memory(&caller)?;
            let (data, host) = mem.data_and_store_mut(&mut caller);
            let Some(p) = host.input.pointer else { return Ok(-1) };
            if cap as usize >= POINTER_BYTES {
                let (fx, fy) = frame_position(p.x, p.y, p.drawable, (host.width, host.height));
                let mut b = Vec::with_capacity(POINTER_BYTES);
                for v in [p.x, p.y, fx, fy, p.dx, p.dy, p.wheel_x, p.wheel_y] {
                    b.extend_from_slice(&v.to_le_bytes());
                }
                for v in [p.buttons, p.pressed, p.released, p.flags] {
                    b.extend_from_slice(&v.to_le_bytes());
                }
                guest_slice(data, dst, POINTER_BYTES as u64)?;
                data[dst as usize..dst as usize + POINTER_BYTES].copy_from_slice(&b);
            }
            Ok(POINTER_BYTES as i32)
        },
    )?;

    linker.func_wrap(
        "gasm",
        "gamepad",
        |mut caller: Caller<'_, Host>, slot: u32, dst: u32, cap: u32| -> wasmtime::Result<i32> {
            let mem = memory(&caller)?;
            let (data, host) = mem.data_and_store_mut(&mut caller);
            let (Some(pads), true) = (&host.input.gamepads, slot < 4) else { return Ok(-1) };
            if cap as usize >= GAMEPAD_BYTES {
                let g = &pads[slot as usize];
                let nb = g.buttons.len().min(GAMEPAD_BUTTONS);
                let na = g.axes.len().min(GAMEPAD_AXES);
                let flags = if g.connected { 1 | if g.standard { 2 } else { 0 } } else { 0u32 };
                let mut b = Vec::with_capacity(GAMEPAD_BYTES);
                for v in [flags, nb as u32, na as u32] {
                    b.extend_from_slice(&v.to_le_bytes());
                }
                for i in 0..GAMEPAD_BUTTONS {
                    b.extend_from_slice(&g.buttons.get(i).copied().unwrap_or(0.0).to_le_bytes());
                }
                for i in 0..GAMEPAD_AXES {
                    b.extend_from_slice(&g.axes.get(i).copied().unwrap_or(0.0).to_le_bytes());
                }
                guest_slice(data, dst, GAMEPAD_BYTES as u64)?;
                data[dst as usize..dst as usize + GAMEPAD_BYTES].copy_from_slice(&b);
            }
            Ok(GAMEPAD_BYTES as i32)
        },
    )?;

    linker.func_wrap(
        "gasm",
        "gamepad_name",
        |mut caller: Caller<'_, Host>, slot: u32, dst: u32, cap: u32| -> wasmtime::Result<i32> {
            let mem = memory(&caller)?;
            let (data, host) = mem.data_and_store_mut(&mut caller);
            let Some(g) = host.input.gamepads.as_ref().and_then(|p| p.get(slot as usize)).filter(|g| g.connected) else { return Ok(-1) };
            let n = g.name.as_bytes();
            if n.len() <= cap as usize {
                guest_slice(data, dst, n.len() as u64)?;
                data[dst as usize..dst as usize + n.len()].copy_from_slice(n);
            }
            Ok(n.len() as i32)
        },
    )?;

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
        "asset_read_at",
        |mut caller: Caller<'_, Host>, ptr: u32, len: u32, offset: u32, dst: u32, cap: u32| -> wasmtime::Result<i32> {
            let name = guest_str(&caller, ptr, len)?;
            let mem = memory(&caller)?;
            let (data, host) = mem.data_and_store_mut(&mut caller);
            let Some(size) = host.assets.size(&name) else { return Ok(-1) };
            let n = size.saturating_sub(offset as u64).min(cap as u64);
            guest_slice(data, dst, n)?; // bounds check before touching guest memory
            let dst = &mut data[dst as usize..dst as usize + n as usize];
            Ok(host.assets.read_at(&name, offset as u64, dst).map_or(-1, |k| k as i32))
        },
    )?;

    linker.func_wrap("gasm", "asset_count", |mut caller: Caller<'_, Host>| -> u32 {
        let h = caller.data_mut();
        h.asset_names.get_or_insert_with(|| h.assets.names()).len() as u32
    })?;

    linker.func_wrap(
        "gasm",
        "asset_name",
        |mut caller: Caller<'_, Host>, index: u32, dst: u32, cap: u32| -> wasmtime::Result<i32> {
            let mem = memory(&caller)?;
            let (data, host) = mem.data_and_store_mut(&mut caller);
            let names = host.asset_names.get_or_insert_with(|| host.assets.names());
            let Some(n) = names.get(index as usize) else { return Ok(-1) };
            let n = n.as_bytes();
            if n.len() <= cap as usize {
                guest_slice(data, dst, n.len() as u64)?;
                data[dst as usize..dst as usize + n.len()].copy_from_slice(n);
            }
            Ok(n.len() as i32)
        },
    )?;

    linker.func_wrap(
        "gasm",
        "asset_size",
        |caller: Caller<'_, Host>, ptr: u32, len: u32| -> wasmtime::Result<i32> {
            let name = guest_str(&caller, ptr, len)?;
            Ok(caller.data().assets.size(&name).map_or(-1, |n| n as i32))
        },
    )?;

    linker.func_wrap(
        "gasm",
        "asset_read",
        |mut caller: Caller<'_, Host>, ptr: u32, len: u32, dst: u32, cap: u32| -> wasmtime::Result<i32> {
            let name = guest_str(&caller, ptr, len)?;
            let mem = memory(&caller)?;
            let (data, host) = mem.data_and_store_mut(&mut caller);
            let Some(size) = host.assets.size(&name) else { return Ok(-1) };
            let n = size.min(cap as u64);
            guest_slice(data, dst, n)?;
            let dst = &mut data[dst as usize..dst as usize + n as usize];
            Ok(host.assets.read_at(&name, 0, dst).map_or(-1, |k| k as i32))
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
    linker.func_wrap(M, "create_bind_group_layout", |mut c: Caller<'_, Host>, ptr: u32, len: u32| -> wasmtime::Result<u32> {
        let json = guest_str(&c, ptr, len)?;
        c.data_mut().gfx.create_bind_group_layout(&json).map_err(trap)
    })?;
    linker.func_wrap(M, "create_texture", |mut c: Caller<'_, Host>, ptr: u32, len: u32| -> wasmtime::Result<u32> {
        let json = guest_str(&c, ptr, len)?;
        c.data_mut().gfx.create_texture(&json).map_err(trap)
    })?;
    linker.func_wrap(M, "create_sampler", |mut c: Caller<'_, Host>, ptr: u32, len: u32| -> wasmtime::Result<u32> {
        let json = guest_str(&c, ptr, len)?;
        c.data_mut().gfx.create_sampler(&json).map_err(trap)
    })?;
    linker.func_wrap(
        M,
        "write_texture",
        |mut c: Caller<'_, Host>, tex: u32, mip: u32, x: u32, y: u32, w: u32, h: u32, ptr: u32, len: u32| -> wasmtime::Result<()> {
            let mem = memory(&c)?;
            let (data, host) = mem.data_and_store_mut(&mut c);
            let bytes = guest_slice(data, ptr, len as u64)?;
            host.gfx.write_texture(tex, mip, x, y, w, h, bytes).map_err(trap)?;
            if host.hashing {
                // header (little-endian u32s) + payload, so the target region counts too
                for v in [tex, mip, x, y, w, h] {
                    host.video_hash.update(&v.to_le_bytes());
                }
                host.video_hash.update(bytes);
            }
            Ok(())
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
    linker.func_wrap(
        M,
        "set_bind_group_offsets",
        |mut c: Caller<'_, Host>, i: u32, bg: u32, ptr: u32, count: u32| -> wasmtime::Result<()> {
            let mem = memory(&c)?;
            let (data, host) = mem.data_and_store_mut(&mut c);
            let bytes = guest_slice(data, ptr, count as u64 * 4)?;
            let offsets: Vec<u32> = bytes.chunks_exact(4).map(|b| u32::from_le_bytes([b[0], b[1], b[2], b[3]])).collect();
            host.gfx.set_bind_group_offsets(i, bg, &offsets).map_err(trap)
        },
    )?;
    linker.func_wrap(
        M,
        "set_viewport",
        |mut c: Caller<'_, Host>, x: f32, y: f32, w: f32, h: f32, min: f32, max: f32| -> wasmtime::Result<()> {
            c.data_mut().gfx.set_viewport(x, y, w, h, min, max).map_err(trap)
        },
    )?;
    linker.func_wrap(M, "set_scissor_rect", |mut c: Caller<'_, Host>, x: u32, y: u32, w: u32, h: u32| -> wasmtime::Result<()> {
        c.data_mut().gfx.set_scissor_rect(x, y, w, h).map_err(trap)
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

fn add_storage_imports(linker: &mut Linker<Host>) -> wasmtime::Result<()> {
    const M: &str = "gasm:storage";
    linker.func_wrap(M, "get", |mut c: Caller<'_, Host>, kp: u32, kl: u32, dst: u32, cap: u32| -> wasmtime::Result<i32> {
        let key = guest_str(&c, kp, kl)?;
        let mem = memory(&c)?;
        let (data, host) = mem.data_and_store_mut(&mut c);
        let Some(v) = host.storage.get(&key) else { return Ok(-1) };
        if v.len() <= cap as usize {
            guest_slice(data, dst, v.len() as u64)?;
            data[dst as usize..dst as usize + v.len()].copy_from_slice(v);
        }
        Ok(v.len() as i32)
    })?;
    linker.func_wrap(M, "set", |mut c: Caller<'_, Host>, kp: u32, kl: u32, vp: u32, vl: u32| -> wasmtime::Result<i32> {
        let key = guest_str(&c, kp, kl)?;
        let mem = memory(&c)?;
        let (data, host) = mem.data_and_store_mut(&mut c);
        let value = guest_slice(data, vp, vl as u64)?;
        Ok(match host.storage.set(&key, value) {
            Ok(()) => 0,
            Err(e) => {
                eprintln!("[gasm] storage: {e}");
                -1
            }
        })
    })?;
    linker.func_wrap(M, "count", |c: Caller<'_, Host>| c.data().storage.keys().len() as u32)?;
    linker.func_wrap(M, "key", |mut c: Caller<'_, Host>, index: u32, dst: u32, cap: u32| -> wasmtime::Result<i32> {
        let mem = memory(&c)?;
        let (data, host) = mem.data_and_store_mut(&mut c);
        let keys = host.storage.keys();
        let Some(k) = keys.get(index as usize) else { return Ok(-1) };
        let k = k.as_bytes();
        if k.len() <= cap as usize {
            guest_slice(data, dst, k.len() as u64)?;
            data[dst as usize..dst as usize + k.len()].copy_from_slice(k);
        }
        Ok(k.len() as i32)
    })?;
    linker.func_wrap(M, "delete", |mut c: Caller<'_, Host>, kp: u32, kl: u32| -> wasmtime::Result<i32> {
        let key = guest_str(&c, kp, kl)?;
        Ok(if c.data_mut().storage.delete(&key) { 0 } else { -1 })
    })?;
    Ok(())
}

/// Why a guest call ended.
#[derive(Debug)]
pub enum Stop {
    /// The guest called proc_exit(code).
    Exit(i32),
    Trap(String),
}

impl std::fmt::Display for Stop {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Stop::Exit(code) => write!(f, "guest exited with code {code}"),
            Stop::Trap(msg) => write!(f, "guest trapped: {msg}"),
        }
    }
}

impl std::error::Error for Stop {}

fn classify(e: wasmtime::Error) -> Stop {
    match e.downcast_ref::<wasmtime_wasi::I32Exit>() {
        Some(exit) => Stop::Exit(exit.0),
        None => Stop::Trap(format!("{e:?}")),
    }
}

pub struct Game {
    pub store: Store<Host>,
    frame: TypedFunc<(), ()>,
    exit: Option<TypedFunc<(), ()>>,
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
        add_storage_imports(&mut linker)?;
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
        let exit = instance.get_typed_func::<(), ()>(&mut store, "gasm_exit").ok();
        Ok(Game { store, frame, exit })
    }

    pub fn frame(&mut self) -> Result<(), Stop> {
        self.frame.call(&mut self.store, ()).map_err(classify)
    }

    /// Best-effort "the player is quitting" notification (optional `gasm_exit` export).
    pub fn exit(&mut self) {
        if let Some(f) = self.exit.take() {
            if let Err(e) = f.call(&mut self.store, ()) {
                if let Stop::Trap(t) = classify(e) {
                    eprintln!("[gasm] gasm_exit trapped: {t}");
                }
            }
        }
    }

    pub fn host(&self) -> &Host {
        self.store.data()
    }

    pub fn host_mut(&mut self) -> &mut Host {
        self.store.data_mut()
    }
}
