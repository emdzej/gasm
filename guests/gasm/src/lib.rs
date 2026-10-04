//! Rust bindings for the gasm game ABI v0 — see `spec/ABI.md`.
//!
//! ```ignore
//! struct MyGame { t: u32 }
//! impl gasm::Game for MyGame {
//!     fn init() -> Result<Self, String> { gasm::set_frame_rate(60.0); Ok(MyGame { t: 0 }) }
//!     fn frame(&mut self) { self.t += 1; /* simulate, draw */ }
//! }
//! gasm::game!(MyGame);
//! ```
//!
//! Build with `--target wasm32-unknown-unknown` and `crate-type = ["cdylib"]`.
//! On native targets the same code links against an in-process stub host
//! ([`native`]) for tests and benchmarks.

pub mod sys;

pub mod gles;
pub mod sync;
pub mod thread;

#[cfg(not(target_arch = "wasm32"))]
pub mod native;

use std::cell::UnsafeCell;

pub const ABI_VERSION: i32 = 0;

// ---- core --------------------------------------------------------------------

pub fn log(msg: &str) {
    unsafe { sys::log(msg.as_ptr(), msg.len() as u32) }
}

/// `format!`-style logging: `gasm::log!("x = {x}")`.
#[macro_export]
macro_rules! log {
    ($($t:tt)*) => { $crate::log(&::std::format!($($t)*)) };
}

/// Monotonic milliseconds (virtual, frame-derived time in headless runs).
pub fn time_ms() -> f64 {
    unsafe { sys::time_ms() }
}

/// The player's time zone now: minutes east of UTC, daylight saving included
/// (120 for CEST). 0 in headless runs, and on older runners without it.
pub fn utc_offset_minutes() -> i32 {
    if has("gasm.utc_offset_minutes") { unsafe { sys::utc_offset_minutes() } } else { 0 }
}

/// Does the runner provide an import? A module (`"gasm:gfx"`) or a function in
/// one (`"gasm.asset_size64"`, `"gasm:gfx.destroy"`). Calling an import the
/// runner lacks traps, so probe optional features first.
pub fn has(name: &str) -> bool {
    unsafe { sys::has(name.as_ptr(), name.len() as u32) == 1 }
}

/// Rate at which the runner calls `frame()`. Default 60.
pub fn set_frame_rate(hz: f64) {
    unsafe { sys::set_frame_rate(hz) }
}

/// Show `video_present` frames at display aspect `num:den` (e.g. 4:3 for a
/// 320×200 game drawn for a CRT) instead of square pixels; `(0, 0)` resets.
/// Returns false on runners without it: the game then corrects the aspect itself
/// (or accepts square pixels). Display only: hashes don't change.
pub fn video_set_aspect(num: u32, den: u32) -> bool {
    static SUPPORTED: std::sync::OnceLock<bool> = std::sync::OnceLock::new();
    let ok = *SUPPORTED.get_or_init(|| has("gasm.video_set_aspect"));
    if ok {
        unsafe { sys::video_set_aspect(num, den) }
    }
    ok
}

/// The game's built-in title (custom section `gasm.title`): runners show it
/// (and launchers list it) before the game runs and while it hasn't called
/// [`set_title`]. Use once, at the top level: `gasm::title!("Sumo");`
#[macro_export]
macro_rules! title {
    ($t:literal) => {
        // a wasm custom section; native builds of a game (tests, parity) just keep the bytes
        #[cfg_attr(target_arch = "wasm32", unsafe(link_section = "gasm.title"))]
        #[used]
        static __GASM_TITLE: [u8; $t.len()] = $crate::__title_bytes($t);
    };
}

#[doc(hidden)]
pub const fn __title_bytes<const N: usize>(s: &str) -> [u8; N] {
    let b = s.as_bytes();
    let mut out = [0u8; N];
    let mut i = 0;
    while i < N {
        out[i] = b[i];
        i += 1;
    }
    out
}

/// Name the game's window or browser tab (runners add their own suffix, e.g.
/// "DOOM — gasm"). Control characters are dropped, at most 256 bytes are kept,
/// and `""` restores the default (the file name). Does nothing on runners
/// without it.
pub fn set_title(title: &str) {
    static SUPPORTED: std::sync::OnceLock<bool> = std::sync::OnceLock::new();
    if *SUPPORTED.get_or_init(|| has("gasm.set_title")) {
        unsafe { sys::set_title(title.as_ptr(), title.len() as u32) }
    }
}

/// Launch parameter (`--param k=v` natively, `?k=v` in the browser).
pub fn param(name: &str) -> Option<String> {
    let len = unsafe { sys::param(name.as_ptr(), name.len() as u32, std::ptr::null_mut(), 0) };
    if len < 0 {
        return None;
    }
    let mut buf = vec![0u8; len as usize];
    unsafe { sys::param(name.as_ptr(), name.len() as u32, buf.as_mut_ptr(), len as u32) };
    Some(String::from_utf8_lossy(&buf).into_owned())
}

/// End the game (WASI `proc_exit`). Runners treat code 0 as a clean exit.
pub fn exit(code: i32) -> ! {
    unsafe { sys::proc_exit(code) }
}

// ---- video / audio / input / assets --------------------------------------------

/// Present an RGBA8 frame (`stride` bytes per row; 1-4096 pixels each way).
pub fn present(rgba: &[u8], width: u32, height: u32, stride: u32) {
    let need = (stride as u64)
        .checked_mul(height.saturating_sub(1) as u64)
        .and_then(|n| n.checked_add(width as u64 * 4))
        .filter(|_| width > 0 && height > 0 && stride as u64 >= width as u64 * 4);
    assert!(need.is_some_and(|n| rgba.len() as u64 >= n), "present: bad geometry or buffer too small");
    unsafe { sys::video_present(rgba.as_ptr(), width, height, stride) }
}

pub mod audio {
    use std::sync::atomic::{AtomicU32, Ordering};
    static CHANNELS: AtomicU32 = AtomicU32::new(2);

    /// Format for [`push`]: sample rate (8–192 kHz) and 1 or 2 channels. Other
    /// values are ignored (by the runner too): the format stays as it was.
    pub fn config(sample_rate: u32, channels: u32) {
        if (8000..=192_000).contains(&sample_rate) && (channels == 1 || channels == 2) {
            CHANNELS.store(channels, Ordering::Relaxed);
        }
        unsafe { crate::sys::audio_config(sample_rate, channels) }
    }

    /// Queue interleaved f32 samples in [-1, 1] (whole frames: a length that is a
    /// multiple of the channel count).
    pub fn push(samples: &[f32]) {
        let ch = CHANNELS.load(Ordering::Relaxed);
        debug_assert!(samples.len() % ch as usize == 0, "audio::push: not a whole number of frames");
        unsafe { crate::sys::audio_push(samples.as_ptr(), samples.len() as u32 / ch) }
    }
}

/// Virtual gamepad buttons (bit positions per spec/ABI.md).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Buttons(pub u32);

impl Buttons {
    pub const A: u32 = sys::GASM_BTN_A;
    pub const B: u32 = sys::GASM_BTN_B;
    pub const X: u32 = sys::GASM_BTN_X;
    pub const Y: u32 = sys::GASM_BTN_Y;
    pub const L: u32 = sys::GASM_BTN_L;
    pub const R: u32 = sys::GASM_BTN_R;
    pub const SELECT: u32 = sys::GASM_BTN_SELECT;
    pub const START: u32 = sys::GASM_BTN_START;
    pub const UP: u32 = sys::GASM_BTN_UP;
    pub const DOWN: u32 = sys::GASM_BTN_DOWN;
    pub const LEFT: u32 = sys::GASM_BTN_LEFT;
    pub const RIGHT: u32 = sys::GASM_BTN_RIGHT;

    pub fn held(self, mask: u32) -> bool {
        self.0 & mask != 0
    }
}

/// Buttons held on virtual pad `player` (0..=3) this frame.
pub fn pad(player: u32) -> Buttons {
    Buttons(unsafe { sys::input_pad(player) })
}

/// Text typed since the previous frame (`'\u{8}'` = backspace, `'\n'` = enter),
/// or `None` if the runner has no keyboard.
pub fn text_input() -> Option<String> {
    let len = unsafe { sys::text_input(std::ptr::null_mut(), 0) };
    if len < 0 {
        return None;
    }
    let mut buf = vec![0u8; len as usize];
    if len > 0 {
        unsafe { sys::text_input(buf.as_mut_ptr(), len as u32) };
    }
    Some(String::from_utf8_lossy(&buf).into_owned())
}

/// All asset names, sorted (by UTF-8 bytes). Folder entries keep their on-disk case.
pub fn asset_names() -> Vec<String> {
    let n = unsafe { sys::asset_count() };
    (0..n)
        .filter_map(|i| {
            let len = unsafe { sys::asset_name(i, std::ptr::null_mut(), 0) };
            if len < 0 {
                return None;
            }
            let mut buf = vec![0u8; len as usize];
            if len > 0 {
                unsafe { sys::asset_name(i, buf.as_mut_ptr(), len as u32) };
            }
            Some(String::from_utf8_lossy(&buf).into_owned())
        })
        .collect()
}

/// Size of an asset in bytes (any size), or `None` if it doesn't exist.
pub fn asset_size(name: &str) -> Option<u64> {
    let n = unsafe { sys::asset_size64(name.as_ptr(), name.len() as u32) };
    (n >= 0).then_some(n as u64)
}

/// Read a whole asset (it must fit in memory: under 2 GiB).
pub fn asset(name: &str) -> Option<Vec<u8>> {
    let n = unsafe { sys::asset_size(name.as_ptr(), name.len() as u32) };
    if n < 0 {
        return None;
    }
    let mut buf = vec![0u8; n as usize];
    let got = unsafe { sys::asset_read(name.as_ptr(), name.len() as u32, buf.as_mut_ptr(), n as u32) };
    buf.truncate(got.max(0) as usize);
    Some(buf)
}

/// Read up to `buf.len()` bytes of an asset starting at `offset` (for streaming
/// large assets). Returns bytes read (0 at the end), or `None` if missing.
pub fn asset_read_at(name: &str, offset: u64, buf: &mut [u8]) -> Option<usize> {
    let len = buf.len().min(u32::MAX as usize) as u32;
    let n = unsafe { sys::asset_read_at64(name.as_ptr(), name.len() as u32, offset, buf.as_mut_ptr(), len) };
    (n >= 0).then_some(n as usize)
}

// ---- storage ------------------------------------------------------------------------

/// Persistent per-game key/value store (`gasm:storage`): saves, settings, scores.
///
/// Keys: 1–128 bytes of `[A-Za-z0-9._-]`. Values up to 1 MiB, 16 MiB per game.
/// The runner picks the namespace; headless runs start with an empty store.
pub mod storage {
    use crate::sys;

    pub fn get(key: &str) -> Option<Vec<u8>> {
        let n = unsafe { sys::storage_get(key.as_ptr(), key.len() as u32, std::ptr::null_mut(), 0) };
        if n < 0 {
            return None;
        }
        let mut buf = vec![0u8; n as usize];
        unsafe { sys::storage_get(key.as_ptr(), key.len() as u32, buf.as_mut_ptr(), n as u32) };
        Some(buf)
    }

    /// Why [`try_set`] failed.
    #[derive(Clone, Copy, Debug, PartialEq, Eq)]
    pub enum Error {
        /// not 1–128 bytes of `[A-Za-z0-9._-]`
        Key,
        /// value over 1 MiB
        Size,
        /// the game's 16 MiB are used up
        Quota,
        /// the runner couldn't write it
        Io,
    }

    /// Store a value.
    pub fn try_set(key: &str, value: &[u8]) -> Result<(), Error> {
        match unsafe { sys::storage_set(key.as_ptr(), key.len() as u32, value.as_ptr(), value.len() as u32) } {
            sys::GASM_STORAGE_OK => Ok(()),
            sys::GASM_STORAGE_ERR_KEY => Err(Error::Key),
            sys::GASM_STORAGE_ERR_SIZE => Err(Error::Size),
            sys::GASM_STORAGE_ERR_QUOTA => Err(Error::Quota),
            _ => Err(Error::Io),
        }
    }

    /// Store a value. False on invalid key, size/quota limits or I/O error ([`try_set`] says which).
    pub fn set(key: &str, value: &[u8]) -> bool {
        try_set(key, value).is_ok()
    }

    /// Delete a key. False if it didn't exist.
    pub fn delete(key: &str) -> bool {
        unsafe { sys::storage_delete(key.as_ptr(), key.len() as u32) == 0 }
    }

    /// All keys of this game, sorted.
    pub fn keys() -> Vec<String> {
        let n = unsafe { sys::storage_count() };
        (0..n)
            .filter_map(|i| {
                let len = unsafe { sys::storage_key(i, std::ptr::null_mut(), 0) };
                if len < 0 {
                    return None;
                }
                let mut buf = vec![0u8; len as usize];
                unsafe { sys::storage_key(i, buf.as_mut_ptr(), len as u32) };
                Some(String::from_utf8_lossy(&buf).into_owned())
            })
            .collect()
    }
}

// ---- raw input ---------------------------------------------------------------------

/// Raw keyboard, pointer and gamepads (next to the virtual [`pad`]s).
///
/// ```ignore
/// use gasm::{input, keys};
/// input::set_mode(input::KEYS_RAW);              // read the keyboard directly, no keymap pads
/// let k = input::keys().unwrap_or_default();
/// if k.held(keys::SHIFT_LEFT) && k.held(keys::ARROW_LEFT) { … }
/// if let Some(p) = input::pointer() && p.pressed & input::MOUSE_LEFT != 0 { click(p.frame_x, p.frame_y) }
/// ```
pub mod input {
    use crate::sys;

    /// The runner stops mapping the keyboard to pads (gamepads still map).
    pub const KEYS_RAW: u32 = sys::GASM_INPUT_KEYS_RAW;
    /// Hide the system cursor over the game.
    pub const POINTER_HIDDEN: u32 = sys::GASM_INPUT_POINTER_HIDDEN;
    /// Capture the pointer for relative motion (best effort; the browser needs a click).
    pub const POINTER_LOCKED: u32 = sys::GASM_INPUT_POINTER_LOCKED;

    pub const MOUSE_LEFT: u32 = sys::GASM_MOUSE_LEFT;
    pub const MOUSE_RIGHT: u32 = sys::GASM_MOUSE_RIGHT;
    pub const MOUSE_MIDDLE: u32 = sys::GASM_MOUSE_MIDDLE;
    pub const MOUSE_BACK: u32 = sys::GASM_MOUSE_BACK;
    pub const MOUSE_FORWARD: u32 = sys::GASM_MOUSE_FORWARD;

    const KEY_BYTES: usize = sys::GASM_KEY_STATE_BYTES as usize;
    const POINTER_BYTES: usize = sys::GASM_POINTER_BYTES as usize;
    const GAMEPAD_BYTES: usize = sys::GASM_GAMEPAD_BYTES as usize;

    /// `KEYS_RAW | POINTER_HIDDEN | POINTER_LOCKED`, any combination.
    pub fn set_mode(flags: u32) {
        unsafe { sys::input_mode(flags) }
    }

    /// Keys held this frame; index with [`crate::keys`] constants.
    #[derive(Clone, Copy, Default, PartialEq, Eq)]
    pub struct Keys(pub [u8; KEY_BYTES]);

    impl Keys {
        pub fn held(&self, code: u32) -> bool {
            (code as usize) < KEY_BYTES * 8 && self.0[code as usize / 8] & (1 << (code % 8)) != 0
        }
    }

    /// `None` if the runner has no keyboard.
    pub fn keys() -> Option<Keys> {
        let mut k = Keys::default();
        (unsafe { sys::key_state(k.0.as_mut_ptr(), KEY_BYTES as u32) } >= 0).then_some(k)
    }

    #[derive(Clone, Copy, Debug, PartialEq, Eq)]
    pub struct KeyEvent {
        pub code: u32,
        pub down: bool,
    }

    /// Presses and releases since the previous frame, in order (`None`: no keyboard).
    pub fn key_events() -> Option<Vec<KeyEvent>> {
        let n = unsafe { sys::key_events(std::ptr::null_mut(), 0) };
        if n < 0 {
            return None;
        }
        let mut b = vec![0u8; n as usize];
        if n > 0 {
            unsafe { sys::key_events(b.as_mut_ptr(), n as u32) };
        }
        Some(b.chunks_exact(sys::GASM_KEY_EVENT_BYTES as usize).map(|e| KeyEvent { code: u16::from_le_bytes([e[0], e[1]]) as u32, down: e[2] != 0 }).collect())
    }

    #[derive(Clone, Copy, Debug, Default, PartialEq)]
    pub struct Pointer {
        /// position in drawable pixels (gfx width/height space)
        pub x: f32,
        pub y: f32,
        /// position in the last presented frame's pixels (2D games)
        pub frame_x: f32,
        pub frame_y: f32,
        /// relative motion since the previous frame (also while locked)
        pub dx: f32,
        pub dy: f32,
        /// about 1 per wheel notch; y > 0 = down
        pub wheel_x: f32,
        pub wheel_y: f32,
        pub buttons: u32,
        /// went down / up since the previous frame (a quick click shows in both)
        pub pressed: u32,
        pub released: u32,
        pub flags: u32,
    }

    impl Pointer {
        pub fn inside(&self) -> bool {
            self.flags & 1 != 0
        }
        pub fn locked(&self) -> bool {
            self.flags & 4 != 0
        }
    }

    /// `None` if the runner has no pointer.
    pub fn pointer() -> Option<Pointer> {
        let mut b = [0u8; POINTER_BYTES];
        if unsafe { sys::pointer(b.as_mut_ptr(), POINTER_BYTES as u32) } < 0 {
            return None;
        }
        let f = |o: u32| { let i = o as usize; f32::from_le_bytes([b[i], b[i + 1], b[i + 2], b[i + 3]]) };
        let u = |o: u32| { let i = o as usize; u32::from_le_bytes([b[i], b[i + 1], b[i + 2], b[i + 3]]) };
        Some(Pointer {
            x: f(sys::GASM_POINTER_OFF_X), y: f(sys::GASM_POINTER_OFF_Y),
            frame_x: f(sys::GASM_POINTER_OFF_FX), frame_y: f(sys::GASM_POINTER_OFF_FY),
            dx: f(sys::GASM_POINTER_OFF_DX), dy: f(sys::GASM_POINTER_OFF_DY),
            wheel_x: f(sys::GASM_POINTER_OFF_WHEEL_X), wheel_y: f(sys::GASM_POINTER_OFF_WHEEL_Y),
            buttons: u(sys::GASM_POINTER_OFF_BUTTONS), pressed: u(sys::GASM_POINTER_OFF_PRESSED),
            released: u(sys::GASM_POINTER_OFF_RELEASED), flags: u(sys::GASM_POINTER_OFF_FLAGS),
        })
    }

    /// Raw gamepad or joystick. With `standard`, W3C order: buttons 0 south, 1 east,
    /// 2 west, 3 north, 4/5 shoulders, 6/7 triggers, 8 select, 9 start, 10/11 sticks,
    /// 12-15 d-pad, 16 home; axes 0/1 left stick, 2/3 right stick (y > 0 = down).
    #[derive(Clone, Debug, Default, PartialEq)]
    pub struct Gamepad {
        pub connected: bool,
        pub standard: bool,
        pub buttons: Vec<f32>,
        pub axes: Vec<f32>,
    }

    /// Slot 0-3 (connection order). `None` if the runner has no gamepad support.
    pub fn gamepad(slot: u32) -> Option<Gamepad> {
        let mut b = [0u8; GAMEPAD_BYTES];
        if unsafe { sys::gamepad(slot, b.as_mut_ptr(), GAMEPAD_BYTES as u32) } < 0 {
            return None;
        }
        let u = |i: usize| u32::from_le_bytes([b[i], b[i + 1], b[i + 2], b[i + 3]]);
        let f = |i: usize| f32::from_le_bytes([b[i], b[i + 1], b[i + 2], b[i + 3]]);
        let (flags, nb, na) = (
            u(sys::GASM_GAMEPAD_OFF_FLAGS as usize),
            (u(sys::GASM_GAMEPAD_OFF_BUTTON_COUNT as usize) as usize).min(sys::GASM_GAMEPAD_BUTTONS as usize),
            (u(sys::GASM_GAMEPAD_OFF_AXIS_COUNT as usize) as usize).min(sys::GASM_GAMEPAD_AXES as usize),
        );
        Some(Gamepad {
            connected: flags & sys::GASM_GAMEPAD_CONNECTED != 0,
            standard: flags & sys::GASM_GAMEPAD_STANDARD != 0,
            buttons: (0..nb).map(|i| f(sys::GASM_GAMEPAD_OFF_BUTTONS as usize + i * 4)).collect(),
            axes: (0..na).map(|i| f(sys::GASM_GAMEPAD_OFF_AXES as usize + i * 4)).collect(),
        })
    }

    /// Device name, if a gamepad is connected in `slot`.
    pub fn gamepad_name(slot: u32) -> Option<String> {
        let n = unsafe { sys::gamepad_name(slot, std::ptr::null_mut(), 0) };
        if n < 0 {
            return None;
        }
        let mut b = vec![0u8; n as usize];
        if n > 0 {
            unsafe { sys::gamepad_name(slot, b.as_mut_ptr(), n as u32) };
        }
        Some(String::from_utf8_lossy(&b).into_owned())
    }
}

/// Raw key codes (`GASM_KEY_*`, W3C `KeyboardEvent.code` names in [`keys::NAMES`]).
pub use sys::keys;

// ---- gfx ---------------------------------------------------------------------------

/// GPU rendering through `gasm:gfx` (a WebGPU subset; descriptors are JSON).
pub mod gfx {
    use crate::sys;

    #[derive(Clone, Copy, Debug, PartialEq, Eq)]
    pub struct Shader(pub u32);
    #[derive(Clone, Copy, Debug, PartialEq, Eq)]
    pub struct Buffer(pub u32);
    #[derive(Clone, Copy, Debug, PartialEq, Eq)]
    pub struct Pipeline(pub u32);
    #[derive(Clone, Copy, Debug, PartialEq, Eq)]
    pub struct BindGroup(pub u32);
    #[derive(Clone, Copy, Debug, PartialEq, Eq)]
    pub struct BindGroupLayout(pub u32);
    #[derive(Clone, Copy, Debug, PartialEq, Eq)]
    pub struct Texture(pub u32);
    #[derive(Clone, Copy, Debug, PartialEq, Eq)]
    pub struct Sampler(pub u32);

    pub const COPY_DST: u32 = sys::GASM_BUF_COPY_DST;
    pub const INDEX: u32 = sys::GASM_BUF_INDEX;
    pub const VERTEX: u32 = sys::GASM_BUF_VERTEX;
    pub const UNIFORM: u32 = sys::GASM_BUF_UNIFORM;
    pub const STORAGE: u32 = sys::GASM_BUF_STORAGE;

    /// Bind group layout entry visibility.
    pub const STAGE_VERTEX: u32 = sys::GASM_STAGE_VERTEX;
    pub const STAGE_FRAGMENT: u32 = sys::GASM_STAGE_FRAGMENT;
    /// Dynamic offsets must be multiples of this.
    pub const OFFSET_ALIGNMENT: u32 = 256;

    #[derive(Clone, Copy)]
    pub enum IndexFormat {
        U16 = sys::GASM_INDEX_U16 as isize,
        U32 = sys::GASM_INDEX_U32 as isize,
    }

    /// Any object handle (for [`destroy`]).
    pub trait Handle {
        fn raw(&self) -> u32;
    }
    macro_rules! handles {
        ($($t:ident),*) => { $(impl Handle for $t { fn raw(&self) -> u32 { self.0 } })* };
    }
    handles!(Shader, Buffer, Pipeline, BindGroup, BindGroupLayout, Texture, Sampler);

    /// Plain-old-data that can be uploaded byte for byte.
    ///
    /// # Safety
    /// Implementors must have no padding and no pointers.
    pub unsafe trait Pod: Copy {}
    unsafe impl Pod for u8 {}
    unsafe impl Pod for u16 {}
    unsafe impl Pod for u32 {}
    unsafe impl Pod for f32 {}
    unsafe impl<T: Pod, const N: usize> Pod for [T; N] {}

    pub fn as_bytes<T: Pod>(data: &[T]) -> &[u8] {
        unsafe { std::slice::from_raw_parts(data.as_ptr() as *const u8, std::mem::size_of_val(data)) }
    }

    /// Drawable size in pixels.
    pub fn size() -> (u32, u32) {
        unsafe { (sys::gfx_width(), sys::gfx_height()) }
    }
    pub fn create_shader(wgsl: &str) -> Shader {
        Shader(unsafe { sys::gfx_create_shader(wgsl.as_ptr(), wgsl.len() as u32) })
    }
    pub fn create_buffer(size: u32, usage: u32) -> Buffer {
        Buffer(unsafe { sys::gfx_create_buffer(size, usage) })
    }
    /// WebGPU `GPURenderPipelineDescriptor` as JSON (handles as numbers,
    /// color format `"surface"`, depth format `"depth24plus"`).
    pub fn create_pipeline(json: &str) -> Pipeline {
        Pipeline(unsafe { sys::gfx_create_pipeline(json.as_ptr(), json.len() as u32) })
    }
    /// `{"layout":L,"entries":[...]}` or `{"pipeline":P,"group":0,"entries":[...]}`; entries
    /// `{"binding":0,"buffer":B,"offset":0,"size":N}`, `{"binding":1,"texture":T}`, `{"binding":2,"sampler":S}`.
    pub fn create_bind_group(json: &str) -> BindGroup {
        BindGroup(unsafe { sys::gfx_create_bind_group(json.as_ptr(), json.len() as u32) })
    }
    /// `{"entries":[{"binding":0,"visibility":STAGE_VERTEX,"buffer":{"type":"uniform","hasDynamicOffset":true}}, ...]}`.
    /// Bind groups made from it work with every pipeline whose `"layout":[...]` lists it.
    pub fn create_bind_group_layout(json: &str) -> BindGroupLayout {
        BindGroupLayout(unsafe { sys::gfx_create_bind_group_layout(json.as_ptr(), json.len() as u32) })
    }
    /// `{"size":[w,h],"format":"rgba8unorm","mipLevelCount":n}`
    pub fn create_texture(json: &str) -> Texture {
        Texture(unsafe { sys::gfx_create_texture(json.as_ptr(), json.len() as u32) })
    }
    /// Upload a tightly packed RGBA8 region (`rgba.len() == w * h * 4`) of mip level `mip`.
    pub fn write_texture(t: Texture, mip: u32, x: u32, y: u32, w: u32, h: u32, rgba: &[u8]) {
        unsafe { sys::gfx_write_texture(t.0, mip, x, y, w, h, rgba.as_ptr(), rgba.len() as u32) }
    }
    /// `{"addressModeU":"repeat","magFilter":"linear","minFilter":"linear","mipmapFilter":"linear"}`
    pub fn create_sampler(json: &str) -> Sampler {
        Sampler(unsafe { sys::gfx_create_sampler(json.as_ptr(), json.len() as u32) })
    }
    /// Offset and byte length must be multiples of 4.
    pub fn write_buffer<T: Pod>(buf: Buffer, offset: u32, data: &[T]) {
        let b = as_bytes(data);
        unsafe { sys::gfx_write_buffer(buf.0, offset, b.as_ptr(), b.len() as u32) }
    }
    /// Returns false if the runner will discard this frame (you may skip drawing).
    pub fn begin_frame(clear: [f32; 4]) -> bool {
        unsafe { sys::gfx_begin_frame(clear[0], clear[1], clear[2], clear[3]) != 0 }
    }
    pub fn set_pipeline(p: Pipeline) {
        unsafe { sys::gfx_set_pipeline(p.0) }
    }
    pub fn set_bind_group(index: u32, bg: BindGroup) {
        unsafe { sys::gfx_set_bind_group(index, bg.0) }
    }
    /// One offset (multiple of [`OFFSET_ALIGNMENT`]) per dynamic-offset entry, in binding order.
    pub fn set_bind_group_offsets(index: u32, bg: BindGroup, offsets: &[u32]) {
        unsafe { sys::gfx_set_bind_group_offsets(index, bg.0, offsets.as_ptr(), offsets.len() as u32) }
    }
    /// Viewport in drawable pixels (clamped to the drawable), depth range 0..=1.
    pub fn set_viewport(x: f32, y: f32, w: f32, h: f32, min_depth: f32, max_depth: f32) {
        unsafe { sys::gfx_set_viewport(x, y, w, h, min_depth, max_depth) }
    }
    /// Scissor rectangle in drawable pixels (clamped to the drawable).
    pub fn set_scissor_rect(x: u32, y: u32, w: u32, h: u32) {
        unsafe { sys::gfx_set_scissor_rect(x, y, w, h) }
    }
    pub fn set_vertex_buffer(slot: u32, buf: Buffer, offset: u32) {
        unsafe { sys::gfx_set_vertex_buffer(slot, buf.0, offset) }
    }
    pub fn set_index_buffer(buf: Buffer, format: IndexFormat, offset: u32) {
        unsafe { sys::gfx_set_index_buffer(buf.0, format as u32, offset) }
    }
    pub fn draw(vertices: u32, instances: u32, first_vertex: u32, first_instance: u32) {
        unsafe { sys::gfx_draw(vertices, instances, first_vertex, first_instance) }
    }
    pub fn draw_indexed(indices: u32, instances: u32, first_index: u32, base_vertex: i32, first_instance: u32) {
        unsafe { sys::gfx_draw_indexed(indices, instances, first_index, base_vertex, first_instance) }
    }
    pub fn end_frame() {
        unsafe { sys::gfx_end_frame() }
    }
    /// Release an object (its handle traps from now on); objects made from it stay
    /// valid. Check `gasm::has("gasm:gfx.destroy")` first on older runners.
    pub fn destroy(h: impl Handle) {
        unsafe { sys::gfx_destroy(h.raw()) }
    }
}

// ---- net ---------------------------------------------------------------------------

/// Message connections through `gasm:net` (WebSocket semantics, non-blocking).
pub mod net {
    use crate::sys;

    #[derive(Clone, Copy, Debug, PartialEq, Eq)]
    pub enum State {
        Connecting,
        Open,
        Closed,
        Error,
    }

    pub enum Recv {
        Message(Vec<u8>),
        Empty,
        Closed,
    }

    #[derive(Debug)]
    pub struct Conn(i32);

    impl Conn {
        /// Open `ws://` / `wss://`. `None` if the runner denies it.
        pub fn open(url: &str) -> Option<Conn> {
            let h = unsafe { sys::net_open(url.as_ptr(), url.len() as u32) };
            (h > 0).then_some(Conn(h))
        }
        pub fn state(&self) -> State {
            match unsafe { sys::net_state(self.0) } {
                0 => State::Connecting,
                1 => State::Open,
                2 => State::Closed,
                _ => State::Error,
            }
        }
        /// Send one message. False if the connection is not open.
        pub fn send(&self, msg: &[u8]) -> bool {
            !msg.is_empty() && unsafe { sys::net_send(self.0, msg.as_ptr(), msg.len() as u32) } == 0
        }
        /// Next queued message, if any.
        pub fn recv(&self) -> Recv {
            let mut buf = [0u8; 1024];
            let n = unsafe { sys::net_recv(self.0, buf.as_mut_ptr(), buf.len() as u32) };
            match n {
                0 => Recv::Empty,
                n if n < 0 => Recv::Closed,
                n if n as usize <= buf.len() => Recv::Message(buf[..n as usize].to_vec()),
                n => {
                    let mut big = vec![0u8; n as usize];
                    unsafe { sys::net_recv(self.0, big.as_mut_ptr(), n as u32) };
                    Recv::Message(big)
                }
            }
        }
    }

    impl Drop for Conn {
        fn drop(&mut self) {
            unsafe { sys::net_close(self.0) }
        }
    }
}

// ---- game entry points ----------------------------------------------------------------

/// A gasm game. `init` runs once; `frame` runs at the configured frame rate.
pub trait Game: Sized + 'static {
    fn init() -> Result<Self, String>;
    fn frame(&mut self);
    /// Called (best effort) when the player closes the game: flush saves here.
    /// Not called after `gasm::exit`, a trap, or a crash, so also save periodically.
    fn exit(&mut self) {}
}

#[doc(hidden)]
pub struct GameCell<T>(pub UnsafeCell<Option<T>>);
// Wasm guests are single-threaded and the runner never calls exports concurrently,
// so a game needn't be Send there (it may hold Rc, egui state, ...); natively (tests,
// the stub host) the exports must not be called from several threads at once.
#[cfg(target_arch = "wasm32")]
unsafe impl<T> Sync for GameCell<T> {}
#[cfg(not(target_arch = "wasm32"))]
unsafe impl<T: Send> Sync for GameCell<T> {}

/// # Safety
/// Not reentrant: call only from the exports [`game!`] defines, never concurrently.
#[doc(hidden)]
pub unsafe fn __init<G: Game>(cell: &GameCell<G>) -> i32 {
    std::panic::set_hook(Box::new(|info| log(&format!("panic: {info}"))));
    match G::init() {
        Ok(g) => {
            unsafe { *cell.0.get() = Some(g) };
            0
        }
        Err(e) => {
            log(&format!("init failed: {e}"));
            1
        }
    }
}

/// # Safety
/// As [`__init`].
#[doc(hidden)]
pub unsafe fn __frame<G: Game>(cell: &GameCell<G>) {
    if let Some(g) = unsafe { (*cell.0.get()).as_mut() } {
        g.frame();
    }
}

/// # Safety
/// As [`__init`].
#[doc(hidden)]
pub unsafe fn __exit<G: Game>(cell: &GameCell<G>) {
    if let Some(g) = unsafe { (*cell.0.get()).as_mut() } {
        g.exit();
    }
}

/// Export the gasm entry points for a [`Game`] type.
#[macro_export]
macro_rules! game {
    ($t:ty) => {
        static __GASM_GAME: $crate::GameCell<$t> = $crate::GameCell(::std::cell::UnsafeCell::new(None));

        #[unsafe(no_mangle)]
        pub extern "C" fn gasm_abi_version() -> i32 {
            $crate::ABI_VERSION
        }
        #[unsafe(no_mangle)]
        pub extern "C" fn gasm_init() -> i32 {
            unsafe { $crate::__init::<$t>(&__GASM_GAME) }
        }
        #[unsafe(no_mangle)]
        pub extern "C" fn gasm_frame() {
            unsafe { $crate::__frame::<$t>(&__GASM_GAME) }
        }
        #[unsafe(no_mangle)]
        pub extern "C" fn gasm_exit() {
            unsafe { $crate::__exit::<$t>(&__GASM_GAME) }
        }
    };
}

// ---- own main loop -------------------------------------------------------------------

/// Games that keep their own main loop (`loop { update(); draw(); wait_frame(); }`)
/// instead of implementing [`Game`]. Same mechanism as the C SDK's `gasm_loop.h`:
/// [`main_loop!`] exports the entry points and runs your function on the first frame;
/// [`main_loop::wait_frame`] suspends it until the next frame: with Binaryen's
/// Asyncify inside the module (`gasm_frame`, any runner), or by the runner itself
/// when it switches stacks (`gasm_run`). Build, then post-process the `.wasm`:
///
/// ```text
/// wasm-opt game.wasm --asyncify --pass-arg=asyncify-removelist@gasm_frame,gasm_loop_frame -O2 -o game.wasm
/// wasm-opt game.wasm -O2 -o game-run.wasm    # runners with stack switching only: smaller, faster
/// ```
///
/// ```ignore
/// fn run() -> i32 {
///     loop {
///         if gasm::pad(0).held(gasm::Buttons::START) { return 0; }
///         draw();
///         gasm::main_loop::wait_frame();
///     }
/// }
/// gasm::main_loop!(run);
/// ```
///
/// `main_loop!(run, on_exit)` also exports `gasm_exit` (the player is quitting: flush
/// saves). wasm32 only: native builds of such a game run `run` straight through.
pub mod main_loop {
    // the suspend/resume state only exists on wasm32
    #![cfg_attr(not(target_arch = "wasm32"), allow(dead_code))]

    #[cfg(target_arch = "wasm32")]
    mod asyncify {
        #[link(wasm_import_module = "asyncify")]
        unsafe extern "C" {
            pub fn start_unwind(data: *mut u8);
            pub fn stop_unwind();
            pub fn start_rewind(data: *mut u8);
            pub fn stop_rewind();
        }
    }

    const STACK: usize = 1024 * 1024;

    #[repr(C)]
    struct Data {
        cur: *mut u8,
        end: *mut u8,
    }

    // One thread; these are only touched from the frame loop below.
    static mut DATA: Data = Data { cur: std::ptr::null_mut(), end: std::ptr::null_mut() };
    static mut STACK_BUF: [u8; STACK] = [0; STACK];
    static mut REWINDING: bool = false;
    static mut UNWINDING: bool = false;
    static mut STARTED: bool = false;
    static mut FINISHED: bool = false;
    static mut EXIT_CODE: i32 = 0;
    static mut MAIN: Option<fn() -> i32> = None;
    static mut FRAMES: u32 = 0;
    /// entered through `gasm_run`: the runner switches stacks (no Asyncify)
    static mut RUN_MODE: bool = false;

    fn wait_impl() {
        #[cfg(target_arch = "wasm32")]
        unsafe {
            if RUN_MODE {
                crate::sys::yield_frame();
                FRAMES += 1;
                return;
            }
            if REWINDING {
                asyncify::stop_rewind();
                REWINDING = false;
                FRAMES += 1;
                return;
            }
            DATA.cur = (&raw mut STACK_BUF).cast::<u8>();
            DATA.end = DATA.cur.add(STACK);
            UNWINDING = true;
            asyncify::start_unwind((&raw mut DATA).cast::<u8>());
        }
    }

    // Reached through a pointer: Asyncify instruments the callers of indirect calls,
    // while wait_impl itself stays plain (like an import).
    static mut WAIT: fn() = wait_impl;

    /// Return to the runner; continues in the next frame (input and time are per frame).
    pub fn wait_frame() {
        if crate::thread::available() {
            return crate::thread::wait_frame(); // threaded_main_loop!
        }
        let f = unsafe { std::ptr::read_volatile(&raw const WAIT) };
        f()
    }

    /// Frames waited so far.
    pub fn frames() -> u32 {
        if crate::thread::available() {
            return crate::thread::frames();
        }
        unsafe { FRAMES }
    }

    #[doc(hidden)]
    pub fn set_main(f: fn() -> i32) {
        unsafe { MAIN = Some(f) }
    }

    // Must be instrumented (and not inlined into the frame export): an unwind returns
    // from here at once instead of looking like the game's function ended.
    #[inline(never)]
    fn run_main() {
        unsafe {
            let rc = MAIN.map_or(0, |f| f());
            if !UNWINDING {
                EXIT_CODE = rc;
                FINISHED = true;
            }
        }
    }

    /// The whole run for runners that switch stacks (`gasm_run`): frames end in
    /// the `yield_frame` import. A build without the Asyncify pass only works this way.
    #[doc(hidden)]
    pub fn __run() -> i32 {
        unsafe {
            RUN_MODE = true;
            STARTED = true;
            MAIN.map_or(0, |f| f())
        }
    }

    /// The frame driver, inlined into the `gasm_loop_frame` export that
    /// [`main_loop!`](crate::main_loop!) defines: that export is on Asyncify's
    /// remove list (it is the one frame that must not unwind). Games that don't
    /// use the macro don't get it (nor the asyncify imports).
    #[doc(hidden)]
    #[inline(always)]
    pub fn __loop_frame() {
        unsafe {
            if FINISHED {
                return;
            }
            #[cfg(target_arch = "wasm32")]
            {
                if !STARTED {
                    STARTED = true;
                } else {
                    REWINDING = true;
                    asyncify::start_rewind((&raw mut DATA).cast::<u8>());
                }
                run_main();
                if UNWINDING {
                    asyncify::stop_unwind();
                    UNWINDING = false;
                    return;
                }
            }
            #[cfg(not(target_arch = "wasm32"))]
            {
                STARTED = true;
                run_main();
            }
            if FINISHED {
                crate::exit(EXIT_CODE);
            }
        }
    }
}

/// Like [`main_loop!`], with cooperative threads ([`thread`], [`sync`]): `main`
/// runs as the first thread, [`thread::spawn`] starts more. The frame driver is the
/// thread scheduler. Asyncify only (no `gasm_run` export): run `wasm-opt --asyncify
/// --pass-arg=asyncify-removelist@gasm_frame,gasm_loop_frame` on the module.
#[macro_export]
macro_rules! threaded_main_loop {
    ($main:path) => {
        #[unsafe(no_mangle)]
        pub extern "C" fn gasm_abi_version() -> i32 {
            $crate::ABI_VERSION
        }
        #[unsafe(no_mangle)]
        pub extern "C" fn gasm_init() -> i32 {
            $crate::thread::__set_main($main);
            0
        }
        #[unsafe(no_mangle)]
        pub extern "C" fn gasm_frame() {
            gasm_loop_frame()
        }
        #[unsafe(no_mangle)]
        #[inline(never)]
        pub extern "C" fn gasm_loop_frame() {
            $crate::thread::__threaded_loop_frame()
        }
    };
    ($main:path, $exit:path) => {
        $crate::threaded_main_loop!($main);
        #[unsafe(no_mangle)]
        pub extern "C" fn gasm_exit() {
            $exit()
        }
    };
}

/// Export the gasm entry points for a game with its own main loop (see [`main_loop`]).
#[macro_export]
macro_rules! main_loop {
    ($main:path) => {
        #[unsafe(no_mangle)]
        pub extern "C" fn gasm_abi_version() -> i32 {
            $crate::ABI_VERSION
        }
        #[unsafe(no_mangle)]
        pub extern "C" fn gasm_init() -> i32 {
            $crate::main_loop::set_main($main);
            0
        }
        #[unsafe(no_mangle)]
        pub extern "C" fn gasm_frame() {
            gasm_loop_frame()
        }
        #[unsafe(no_mangle)]
        #[inline(never)]
        pub extern "C" fn gasm_loop_frame() {
            $crate::main_loop::__loop_frame()
        }
        #[unsafe(no_mangle)]
        pub extern "C" fn gasm_run() -> i32 {
            $crate::main_loop::__run()
        }
    };
    ($main:path, $exit:path) => {
        $crate::main_loop!($main);
        #[unsafe(no_mangle)]
        pub extern "C" fn gasm_exit() {
            $exit()
        }
    };
}
