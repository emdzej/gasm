//! In-process stub host for running a guest natively (non-wasm targets).
//!
//! It implements the same ABI as the runners' headless mode: no display or
//! audio, virtual time, in-memory storage, and FNV-1a hashes over presented
//! frames, audio and gfx uploads. It checks arguments like the runners do
//! (frame rate, audio format, frame geometry, storage keys and limits, gfx
//! handles and buffer ranges) and panics where a runner would trap. A harness
//! sets assets/params/input, calls the guest's `gasm_init`/`gasm_frame`, then
//! reads [`stats`]. Hashes match `gasm-run --headless` for deterministic guests.
//!
//! The state is process-wide: run one guest per process (or per test binary
//! with `--test-threads=1`).

use std::collections::BTreeMap;
use std::sync::Mutex;

struct State {
    frame_rate: f64,
    frames: u64,
    clock: (f64, u64, f64),
    now_ms: f64,
    /// sorted by name (UTF-8 bytes)
    assets: BTreeMap<String, Vec<u8>>,
    params: BTreeMap<String, String>,
    storage: BTreeMap<String, Vec<u8>>,
    storage_used: usize,
    pads: [u32; 4],
    text: String,
    input_mode: u32,
    channels: u32,
    hashing: bool,
    video_hash: u32,
    audio_hash: u32,
    presented: u64,
    size: (u32, u32),
    audio_frames: u64,
    /// gfx objects: Some(buffer size) for buffers, None for other kinds, in creation order
    gfx: Vec<GfxObj>,
}

#[derive(Clone, Copy, PartialEq)]
enum GfxObj {
    Buffer(u64),
    Other,
    Destroyed,
}

static STATE: Mutex<State> = Mutex::new(State {
    frame_rate: 60.0,
    frames: 0,
    clock: (0.0, 0, 60.0),
    now_ms: 0.0,
    assets: BTreeMap::new(),
    params: BTreeMap::new(),
    storage: BTreeMap::new(),
    storage_used: 0,
    pads: [0; 4],
    text: String::new(),
    input_mode: 0,
    channels: 2,
    hashing: true,
    video_hash: FNV_INIT,
    audio_hash: FNV_INIT,
    presented: 0,
    size: (0, 0),
    audio_frames: 0,
    gfx: Vec::new(),
});

const FNV_INIT: u32 = 0x811c_9dc5;
const STORAGE_MAX_VALUE: usize = 1 << 20;
const STORAGE_QUOTA: usize = 16 << 20;

fn fnv(mut h: u32, bytes: &[u8]) -> u32 {
    for &b in bytes {
        h = (h ^ b as u32).wrapping_mul(0x0100_0193);
    }
    h
}

fn with<R>(f: impl FnOnce(&mut State) -> R) -> R {
    f(&mut STATE.lock().unwrap_or_else(|e| e.into_inner()))
}

fn valid_key(k: &str) -> bool {
    !k.is_empty()
        && k.len() <= 128
        && k.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'.' || b == b'_' || b == b'-')
        && k != "."
        && k != ".."
}

/// Harness API.
pub fn set_asset(name: &str, bytes: Vec<u8>) {
    with(|s| s.assets.insert(name.into(), bytes));
}
pub fn set_param(name: &str, value: &str) {
    with(|s| s.params.insert(name.into(), value.into()));
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

/// Advance the virtual clock to the start of frame `frames` (as the runners'
/// `VirtualClock`: monotonic across frame rate changes).
fn tick(s: &mut State) {
    let (base_ms, base_frame, rate) = &mut s.clock;
    if s.frame_rate != *rate {
        *base_ms += (s.frames - *base_frame) as f64 * 1000.0 / *rate;
        *base_frame = s.frames;
        *rate = s.frame_rate;
    }
    s.now_ms = *base_ms + (s.frames - *base_frame) as f64 * 1000.0 / *rate;
}

/// Call before each gasm_frame: time_ms is the start of that frame.
pub fn begin_frame() {
    with(tick);
}

/// Call after each gasm_frame so time advances.
pub fn end_frame() {
    with(|s| {
        s.frames += 1;
        s.text.clear();
        tick(s);
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
    /// A string argument; runners trap on invalid UTF-8.
    unsafe fn text(ptr: *const u8, len: u32) -> String {
        String::from_utf8(unsafe { bytes(ptr, len) }.to_vec()).expect("string argument is not UTF-8")
    }
    unsafe fn copy_out(src: &[u8], dst: *mut u8, cap: u32) -> i32 {
        if src.len() <= cap as usize && !src.is_empty() {
            unsafe { std::ptr::copy_nonoverlapping(src.as_ptr(), dst, src.len()) };
        }
        src.len() as i32
    }

    pub unsafe fn log(ptr: *const u8, len: u32) {
        eprintln!("[guest] {}", String::from_utf8_lossy(unsafe { bytes(ptr, len) }));
    }
    pub unsafe fn has(ptr: *const u8, len: u32) -> i32 {
        let name = unsafe { text(ptr, len) };
        crate::sys::IMPORTS.contains(&name.as_str()) as i32   // this stub implements all of them
    }
    pub unsafe fn time_ms() -> f64 {
        with(|s| s.now_ms)
    }
    pub unsafe fn set_frame_rate(hz: f64) {
        if hz.is_finite() && (1.0..=1000.0).contains(&hz) {
            with(|s| s.frame_rate = hz);
        }
    }
    pub unsafe fn video_present(ptr: *const u8, w: u32, h: u32, stride: u32) {
        assert!(w > 0 && h > 0 && w <= 4096 && h <= 4096 && stride >= w * 4, "video_present: bad geometry {w}x{h} stride {stride}");
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
    pub unsafe fn audio_config(rate: u32, channels: u32) {
        if (8000..=192_000).contains(&rate) && (channels == 1 || channels == 2) {
            with(|s| s.channels = channels);
        }
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
    // raw input: like a headless run with an empty script (keyboard and pointer
    // present, nothing pressed, pointer at 0,0 in a 1280x720 drawable, no gamepads)
    pub unsafe fn input_mode(flags: u32) {
        with(|s| s.input_mode = flags & 7);
    }
    pub unsafe fn key_state(dst: *mut u8, len: u32) -> i32 {
        let n = (len as usize).min(32);
        unsafe { std::ptr::write_bytes(dst, 0, n) };
        32
    }
    pub unsafe fn key_events(_: *mut u8, _: u32) -> i32 {
        0
    }
    pub unsafe fn pointer(dst: *mut u8, cap: u32) -> i32 {
        if cap >= 48 {
            let (fw, fh, mode) = with(|s| (s.size.0 as f64, s.size.1 as f64, s.input_mode));
            let (fx, fy) = if fw > 0.0 && fh > 0.0 {
                let scale = (1280.0 / fw).min(720.0 / fh);
                ((-(1280.0 - fw * scale) / 2.0 / scale) as f32, (-(720.0 - fh * scale) / 2.0 / scale) as f32)
            } else {
                (0.0, 0.0)
            };
            let mut b = [0u8; 48];
            b[8..12].copy_from_slice(&fx.to_le_bytes());
            b[12..16].copy_from_slice(&fy.to_le_bytes());
            b[44..48].copy_from_slice(&(1 | (mode & 6)).to_le_bytes());
            unsafe { std::ptr::copy_nonoverlapping(b.as_ptr(), dst, 48) };
        }
        48
    }
    pub unsafe fn gamepad(slot: u32, dst: *mut u8, cap: u32) -> i32 {
        if slot > 3 {
            return -1;
        }
        if cap >= 204 {
            unsafe { std::ptr::write_bytes(dst, 0, 204) };
        }
        204
    }
    pub unsafe fn gamepad_name(_: u32, _: *mut u8, _: u32) -> i32 {
        -1
    }
    pub unsafe fn text_input(dst: *mut u8, cap: u32) -> i32 {
        with(|s| unsafe { copy_out(s.text.as_bytes(), dst, cap) })
    }
    pub unsafe fn asset_size(name: *const u8, len: u32) -> i32 {
        let n = unsafe { text(name, len) };
        with(|s| match s.assets.get(&n) {
            None => -1,
            Some(a) if a.len() > i32::MAX as usize => -2,
            Some(a) => a.len() as i32,
        })
    }
    pub unsafe fn asset_size64(name: *const u8, len: u32) -> i64 {
        let n = unsafe { text(name, len) };
        with(|s| s.assets.get(&n).map_or(-1, |a| a.len() as i64))
    }
    pub unsafe fn asset_count() -> u32 {
        with(|s| s.assets.len() as u32)
    }
    pub unsafe fn asset_name(index: u32, dst: *mut u8, cap: u32) -> i32 {
        with(|s| match s.assets.keys().nth(index as usize) {
            Some(n) => unsafe { copy_out(n.as_bytes(), dst, cap) },
            None => -1,
        })
    }
    pub unsafe fn asset_read(name: *const u8, len: u32, dst: *mut u8, cap: u32) -> i32 {
        unsafe { asset_read_at64(name, len, 0, dst, cap) }
    }
    pub unsafe fn asset_read_at(name: *const u8, len: u32, offset: u32, dst: *mut u8, cap: u32) -> i32 {
        unsafe { asset_read_at64(name, len, offset as u64, dst, cap) }
    }
    pub unsafe fn asset_read_at64(name: *const u8, len: u32, offset: u64, dst: *mut u8, cap: u32) -> i32 {
        let n = unsafe { text(name, len) };
        with(|s| match s.assets.get(&n) {
            Some(a) => {
                let start = offset.min(a.len() as u64) as usize;
                let k = (a.len() - start).min(cap as usize);
                unsafe { std::ptr::copy_nonoverlapping(a[start..].as_ptr(), dst, k) };
                k as i32
            }
            None => -1,
        })
    }

    // storage: in memory (like headless runners), same rules and error codes
    pub unsafe fn storage_get(key: *const u8, len: u32, dst: *mut u8, cap: u32) -> i32 {
        let k = unsafe { text(key, len) };
        with(|s| match s.storage.get(&k) {
            Some(v) => unsafe { copy_out(v, dst, cap) },
            None => -1,
        })
    }
    pub unsafe fn storage_set(key: *const u8, len: u32, data: *const u8, data_len: u32) -> i32 {
        let k = unsafe { text(key, len) };
        let v = unsafe { bytes(data, data_len) }.to_vec();
        if !valid_key(&k) {
            return crate::sys::GASM_STORAGE_ERR_KEY;
        }
        if v.len() > STORAGE_MAX_VALUE {
            return crate::sys::GASM_STORAGE_ERR_SIZE;
        }
        with(|s| {
            let old = s.storage.get(&k).map_or(0, |o| k.len() + o.len());
            let used = s.storage_used - old + k.len() + v.len();
            if used > STORAGE_QUOTA {
                return crate::sys::GASM_STORAGE_ERR_QUOTA;
            }
            s.storage_used = used;
            s.storage.insert(k, v);
            0
        })
    }
    pub unsafe fn storage_count() -> u32 {
        with(|s| s.storage.len() as u32)
    }
    pub unsafe fn storage_key(index: u32, dst: *mut u8, cap: u32) -> i32 {
        with(|s| match s.storage.keys().nth(index as usize) {
            Some(k) => unsafe { copy_out(k.as_bytes(), dst, cap) },
            None => -1,
        })
    }
    pub unsafe fn storage_delete(key: *const u8, len: u32) -> i32 {
        let k = unsafe { text(key, len) };
        with(|s| match s.storage.remove(&k) {
            Some(v) => {
                s.storage_used -= k.len() + v.len();
                0
            }
            None => -1,
        })
    }

    pub unsafe fn param(name: *const u8, len: u32, dst: *mut u8, cap: u32) -> i32 {
        let n = unsafe { text(name, len) };
        with(|s| match s.params.get(&n) {
            Some(v) => unsafe { copy_out(v.as_bytes(), dst, cap) },
            None => -1,
        })
    }

    // gfx: null backend. Hashes uploads like the headless runners; checks handles
    // and buffer ranges (the runners' full validation lives in gasm-host).
    fn add(obj: GfxObj) -> u32 {
        with(|s| {
            s.gfx.push(obj);
            s.gfx.len() as u32
        })
    }
    fn live(s: &State, h: u32) -> GfxObj {
        match s.gfx.get((h as usize).wrapping_sub(1)) {
            Some(GfxObj::Destroyed) => panic!("gfx: handle {h} was destroyed"),
            Some(o) => *o,
            None => panic!("gfx: invalid handle {h}"),
        }
    }
    fn buffer_size(s: &State, h: u32) -> u64 {
        match live(s, h) {
            GfxObj::Buffer(n) => n,
            _ => panic!("gfx: handle {h} is not a buffer"),
        }
    }
    pub unsafe fn gfx_width() -> u32 { 1280 }
    pub unsafe fn gfx_height() -> u32 { 720 }
    pub unsafe fn gfx_create_shader(_: *const u8, _: u32) -> u32 { add(GfxObj::Other) }
    pub unsafe fn gfx_create_buffer(size: u32, _: u32) -> u32 {
        assert!(size > 0 && size % 4 == 0, "gfx.create_buffer: size {size} must be a non-zero multiple of 4");
        add(GfxObj::Buffer(size as u64))
    }
    pub unsafe fn gfx_create_pipeline(_: *const u8, _: u32) -> u32 { add(GfxObj::Other) }
    pub unsafe fn gfx_create_bind_group(_: *const u8, _: u32) -> u32 { add(GfxObj::Other) }
    pub unsafe fn gfx_create_bind_group_layout(_: *const u8, _: u32) -> u32 { add(GfxObj::Other) }
    pub unsafe fn gfx_create_texture(_: *const u8, _: u32) -> u32 { add(GfxObj::Other) }
    pub unsafe fn gfx_create_sampler(_: *const u8, _: u32) -> u32 { add(GfxObj::Other) }
    #[allow(clippy::too_many_arguments)]
    pub unsafe fn gfx_write_texture(t: u32, mip: u32, x: u32, y: u32, w: u32, h: u32, ptr: *const u8, len: u32) {
        with(|s| {
            live(s, t);
            if s.hashing {
                for v in [t, mip, x, y, w, h] {
                    s.video_hash = fnv(s.video_hash, &v.to_le_bytes());
                }
                s.video_hash = fnv(s.video_hash, unsafe { bytes(ptr, len) });
            }
        })
    }
    pub unsafe fn gfx_set_bind_group_offsets(_: u32, bg: u32, _: *const u32, _: u32) {
        with(|s| live(s, bg));
    }
    pub unsafe fn gfx_set_viewport(_: f32, _: f32, _: f32, _: f32, _: f32, _: f32) {}
    pub unsafe fn gfx_set_scissor_rect(_: u32, _: u32, _: u32, _: u32) {}
    pub unsafe fn gfx_write_buffer(buf: u32, offset: u32, ptr: *const u8, len: u32) {
        assert!(offset % 4 == 0 && len % 4 == 0, "gfx.write_buffer: offset {offset} and length {len} must be multiples of 4");
        with(|s| {
            let size = buffer_size(s, buf);
            assert!(offset as u64 + len as u64 <= size, "gfx.write_buffer: {offset}+{len} exceeds buffer size {size}");
            if s.hashing {
                s.video_hash = fnv(s.video_hash, unsafe { bytes(ptr, len) });
            }
        })
    }
    pub unsafe fn gfx_begin_frame(_: f32, _: f32, _: f32, _: f32) -> u32 { 0 }
    pub unsafe fn gfx_set_pipeline(p: u32) {
        with(|s| live(s, p));
    }
    pub unsafe fn gfx_set_bind_group(_: u32, bg: u32) {
        with(|s| live(s, bg));
    }
    pub unsafe fn gfx_set_vertex_buffer(_: u32, buf: u32, offset: u32) {
        with(|s| assert!(offset as u64 <= buffer_size(s, buf), "gfx.set_vertex_buffer: offset {offset} outside the buffer"));
    }
    pub unsafe fn gfx_set_index_buffer(buf: u32, format: u32, offset: u32) {
        assert!(format <= 1, "gfx.set_index_buffer: format {format}");
        with(|s| assert!(offset as u64 <= buffer_size(s, buf), "gfx.set_index_buffer: offset {offset} outside the buffer"));
    }
    pub unsafe fn gfx_draw(_: u32, _: u32, _: u32, _: u32) {}
    pub unsafe fn gfx_draw_indexed(_: u32, _: u32, _: u32, _: i32, _: u32) {}
    pub unsafe fn gfx_end_frame() {
        with(|s| s.presented += 1);
    }
    pub unsafe fn gfx_destroy(h: u32) {
        with(|s| {
            live(s, h);
            s.gfx[h as usize - 1] = GfxObj::Destroyed;
        })
    }

    // net: always denied natively (no handle is ever valid)
    pub unsafe fn net_open(_: *const u8, _: u32) -> i32 { -1 }
    pub unsafe fn net_state(h: i32) -> u32 { panic!("gasm:net: invalid connection handle {h}") }
    pub unsafe fn net_send(h: i32, _: *const u8, _: u32) -> i32 { panic!("gasm:net: invalid connection handle {h}") }
    pub unsafe fn net_recv(h: i32, _: *mut u8, _: u32) -> i32 { panic!("gasm:net: invalid connection handle {h}") }
    pub unsafe fn net_close(h: i32) { panic!("gasm:net: invalid connection handle {h}") }

    pub unsafe fn proc_exit(code: i32) -> ! {
        std::process::exit(code)
    }
}
