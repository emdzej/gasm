//! In-process stub host for running a guest natively (non-wasm targets).
//!
//! It implements the same ABI as the runners' headless mode: no display or
//! audio, virtual time, and FNV-1a hashes over presented frames, audio and gfx
//! buffer writes. A harness sets assets/params/input, calls the guest's
//! `gasm_init`/`gasm_frame`, then reads [`stats`]. Hashes match
//! `gasm-run --headless` for deterministic guests.

use std::sync::Mutex;

struct State {
    frame_rate: f64,
    frames: u64,
    assets: Vec<(String, Vec<u8>)>,
    params: Vec<(String, String)>,
    pads: [u32; 4],
    channels: u32,
    hashing: bool,
    video_hash: u32,
    audio_hash: u32,
    presented: u64,
    size: (u32, u32),
    audio_frames: u64,
    next_handle: u32,
}

static STATE: Mutex<State> = Mutex::new(State {
    frame_rate: 60.0,
    frames: 0,
    assets: Vec::new(),
    params: Vec::new(),
    pads: [0; 4],
    channels: 2,
    hashing: true,
    video_hash: FNV_INIT,
    audio_hash: FNV_INIT,
    presented: 0,
    size: (0, 0),
    audio_frames: 0,
    next_handle: 1,
});

const FNV_INIT: u32 = 0x811c_9dc5;
fn fnv(mut h: u32, bytes: &[u8]) -> u32 {
    for &b in bytes {
        h = (h ^ b as u32).wrapping_mul(0x0100_0193);
    }
    h
}

fn with<R>(f: impl FnOnce(&mut State) -> R) -> R {
    f(&mut STATE.lock().unwrap())
}

/// Harness API.
pub fn set_asset(name: &str, bytes: Vec<u8>) {
    with(|s| s.assets.push((name.into(), bytes)));
}
pub fn set_param(name: &str, value: &str) {
    with(|s| s.params.push((name.into(), value.into())));
}
pub fn set_pad(player: usize, buttons: u32) {
    with(|s| s.pads[player] = buttons);
}
pub fn set_hashing(on: bool) {
    with(|s| s.hashing = on);
}
/// Call after each gasm_frame so time_ms advances.
pub fn end_frame() {
    with(|s| s.frames += 1);
}

pub struct Stats {
    pub frame_rate: f64,
    pub presented: u64,
    pub width: u32,
    pub height: u32,
    pub video_hash: u32,
    pub audio_hash: u32,
    pub audio_frames: u64,
}

pub fn stats() -> Stats {
    with(|s| Stats {
        frame_rate: s.frame_rate,
        presented: s.presented,
        width: s.size.0,
        height: s.size.1,
        video_hash: s.video_hash,
        audio_hash: s.audio_hash,
        audio_frames: s.audio_frames,
    })
}

/// Same signatures as the wasm imports (see sys.rs).
#[allow(clippy::missing_safety_doc)]
pub mod abi {
    use super::*;

    unsafe fn bytes<'a>(ptr: *const u8, len: u32) -> &'a [u8] {
        if len == 0 { &[] } else { unsafe { std::slice::from_raw_parts(ptr, len as usize) } }
    }
    unsafe fn text(ptr: *const u8, len: u32) -> String {
        String::from_utf8_lossy(unsafe { bytes(ptr, len) }).into_owned()
    }
    unsafe fn copy_out(src: &[u8], dst: *mut u8, cap: u32) -> i32 {
        if src.len() <= cap as usize && !src.is_empty() {
            unsafe { std::ptr::copy_nonoverlapping(src.as_ptr(), dst, src.len()) };
        }
        src.len() as i32
    }

    pub unsafe fn log(ptr: *const u8, len: u32) {
        eprintln!("[guest] {}", unsafe { text(ptr, len) });
    }
    pub unsafe fn time_ms() -> f64 {
        with(|s| s.frames as f64 * 1000.0 / s.frame_rate)
    }
    pub unsafe fn set_frame_rate(hz: f64) {
        with(|s| s.frame_rate = hz);
    }
    pub unsafe fn video_present(ptr: *const u8, w: u32, h: u32, stride: u32) {
        with(|s| {
            if s.hashing {
                for y in 0..h {
                    let row = unsafe { bytes(ptr.add((y * stride) as usize), w * 4) };
                    s.video_hash = fnv(s.video_hash, row);
                }
            }
            s.size = (w, h);
            s.presented += 1;
        })
    }
    pub unsafe fn audio_config(_rate: u32, channels: u32) {
        with(|s| s.channels = channels);
    }
    pub unsafe fn audio_push(ptr: *const f32, frames: u32) {
        with(|s| {
            if s.hashing {
                let b = unsafe { bytes(ptr as *const u8, frames * s.channels * 4) };
                s.audio_hash = fnv(s.audio_hash, b);
            }
            s.audio_frames += frames as u64;
        })
    }
    pub unsafe fn input_pad(player: u32) -> u32 {
        with(|s| s.pads.get(player as usize).copied().unwrap_or(0))
    }
    pub unsafe fn asset_size(name: *const u8, len: u32) -> i32 {
        let n = unsafe { text(name, len) };
        with(|s| s.assets.iter().find(|a| a.0 == n).map_or(-1, |a| a.1.len() as i32))
    }
    pub unsafe fn asset_read(name: *const u8, len: u32, dst: *mut u8, cap: u32) -> i32 {
        let n = unsafe { text(name, len) };
        with(|s| match s.assets.iter().find(|a| a.0 == n) {
            Some(a) => {
                let k = a.1.len().min(cap as usize);
                unsafe { std::ptr::copy_nonoverlapping(a.1.as_ptr(), dst, k) };
                k as i32
            }
            None => -1,
        })
    }
    pub unsafe fn param(name: *const u8, len: u32, dst: *mut u8, cap: u32) -> i32 {
        let n = unsafe { text(name, len) };
        with(|s| match s.params.iter().find(|p| p.0 == n) {
            Some(p) => unsafe { copy_out(p.1.as_bytes(), dst, cap) },
            None => -1,
        })
    }

    // gfx: null backend (hashes buffer writes, like headless runners)
    fn handle() -> u32 {
        with(|s| {
            s.next_handle += 1;
            s.next_handle - 1
        })
    }
    pub unsafe fn gfx_width() -> u32 { 1280 }
    pub unsafe fn gfx_height() -> u32 { 720 }
    pub unsafe fn gfx_create_shader(_: *const u8, _: u32) -> u32 { handle() }
    pub unsafe fn gfx_create_buffer(_: u32, _: u32) -> u32 { handle() }
    pub unsafe fn gfx_create_pipeline(_: *const u8, _: u32) -> u32 { handle() }
    pub unsafe fn gfx_create_bind_group(_: *const u8, _: u32) -> u32 { handle() }
    pub unsafe fn gfx_write_buffer(_: u32, _: u32, ptr: *const u8, len: u32) {
        with(|s| {
            if s.hashing {
                s.video_hash = fnv(s.video_hash, unsafe { bytes(ptr, len) });
            }
        })
    }
    pub unsafe fn gfx_begin_frame(_: f32, _: f32, _: f32, _: f32) -> u32 { 0 }
    pub unsafe fn gfx_set_pipeline(_: u32) {}
    pub unsafe fn gfx_set_bind_group(_: u32, _: u32) {}
    pub unsafe fn gfx_set_vertex_buffer(_: u32, _: u32, _: u32) {}
    pub unsafe fn gfx_set_index_buffer(_: u32, _: u32, _: u32) {}
    pub unsafe fn gfx_draw(_: u32, _: u32, _: u32, _: u32) {}
    pub unsafe fn gfx_draw_indexed(_: u32, _: u32, _: u32, _: i32, _: u32) {}
    pub unsafe fn gfx_end_frame() {
        with(|s| s.presented += 1);
    }

    // net: always denied natively
    pub unsafe fn net_open(_: *const u8, _: u32) -> i32 { -1 }
    pub unsafe fn net_state(_: i32) -> u32 { 3 }
    pub unsafe fn net_send(_: i32, _: *const u8, _: u32) -> i32 { -1 }
    pub unsafe fn net_recv(_: i32, _: *mut u8, _: u32) -> i32 { -1 }
    pub unsafe fn net_close(_: i32) {}

    pub unsafe fn proc_exit(code: i32) -> ! {
        std::process::exit(code)
    }
}
