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
    storage: Vec<(String, Vec<u8>)>,
    pads: [u32; 4],
    text: String,
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
    storage: Vec::new(),
    pads: [0; 4],
    text: String::new(),
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
/// Text returned by `text_input` until the next [`end_frame`].
pub fn set_text(text: &str) {
    with(|s| s.text = text.into());
}
pub fn set_hashing(on: bool) {
    with(|s| s.hashing = on);
}
/// Call after each gasm_frame so time_ms advances.
pub fn end_frame() {
    with(|s| {
        s.frames += 1;
        s.text.clear();
    });
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
    pub unsafe fn text_input(dst: *mut u8, cap: u32) -> i32 {
        with(|s| unsafe { copy_out(s.text.as_bytes(), dst, cap) })
    }
    pub unsafe fn asset_size(name: *const u8, len: u32) -> i32 {
        let n = unsafe { text(name, len) };
        with(|s| s.assets.iter().find(|a| a.0 == n).map_or(-1, |a| a.1.len() as i32))
    }
    pub unsafe fn asset_count() -> u32 {
        with(|s| s.assets.len() as u32)
    }
    pub unsafe fn asset_name(index: u32, dst: *mut u8, cap: u32) -> i32 {
        with(|s| {
            let mut names: Vec<&str> = s.assets.iter().map(|a| a.0.as_str()).collect();
            names.sort();
            match names.get(index as usize) {
                Some(n) => unsafe { copy_out(n.as_bytes(), dst, cap) },
                None => -1,
            }
        })
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
    pub unsafe fn asset_read_at(name: *const u8, len: u32, offset: u32, dst: *mut u8, cap: u32) -> i32 {
        let n = unsafe { text(name, len) };
        with(|s| match s.assets.iter().find(|a| a.0 == n) {
            Some(a) => {
                let start = (offset as usize).min(a.1.len());
                let k = (a.1.len() - start).min(cap as usize);
                unsafe { std::ptr::copy_nonoverlapping(a.1[start..].as_ptr(), dst, k) };
                k as i32
            }
            None => -1,
        })
    }

    // storage: in memory (like headless runners)
    pub unsafe fn storage_get(key: *const u8, len: u32, dst: *mut u8, cap: u32) -> i32 {
        let k = unsafe { text(key, len) };
        with(|s| match s.storage.iter().find(|e| e.0 == k) {
            Some(e) => unsafe { copy_out(&e.1, dst, cap) },
            None => -1,
        })
    }
    pub unsafe fn storage_set(key: *const u8, len: u32, data: *const u8, data_len: u32) -> i32 {
        let k = unsafe { text(key, len) };
        let v = unsafe { bytes(data, data_len) }.to_vec();
        with(|s| {
            s.storage.retain(|e| e.0 != k);
            s.storage.push((k, v));
            0
        })
    }
    pub unsafe fn storage_delete(key: *const u8, len: u32) -> i32 {
        let k = unsafe { text(key, len) };
        with(|s| {
            let before = s.storage.len();
            s.storage.retain(|e| e.0 != k);
            if s.storage.len() < before { 0 } else { -1 }
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
    pub unsafe fn gfx_create_bind_group_layout(_: *const u8, _: u32) -> u32 { handle() }
    pub unsafe fn gfx_create_texture(_: *const u8, _: u32) -> u32 { handle() }
    pub unsafe fn gfx_create_sampler(_: *const u8, _: u32) -> u32 { handle() }
    #[allow(clippy::too_many_arguments)]
    pub unsafe fn gfx_write_texture(t: u32, mip: u32, x: u32, y: u32, w: u32, h: u32, ptr: *const u8, len: u32) {
        with(|s| {
            if s.hashing {
                for v in [t, mip, x, y, w, h] {
                    s.video_hash = fnv(s.video_hash, &v.to_le_bytes());
                }
                s.video_hash = fnv(s.video_hash, unsafe { bytes(ptr, len) });
            }
        })
    }
    pub unsafe fn gfx_set_bind_group_offsets(_: u32, _: u32, _: *const u32, _: u32) {}
    pub unsafe fn gfx_set_viewport(_: f32, _: f32, _: f32, _: f32, _: f32, _: f32) {}
    pub unsafe fn gfx_set_scissor_rect(_: u32, _: u32, _: u32, _: u32) {}
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
