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

/// Rate at which the runner calls `frame()`. Default 60.
pub fn set_frame_rate(hz: f64) {
    unsafe { sys::set_frame_rate(hz) }
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

/// Present an RGBA8 frame (`stride` bytes per row).
pub fn present(rgba: &[u8], width: u32, height: u32, stride: u32) {
    assert!(rgba.len() >= (stride * (height - 1) + width * 4) as usize, "present: buffer too small");
    unsafe { sys::video_present(rgba.as_ptr(), width, height, stride) }
}

pub mod audio {
    use std::sync::atomic::{AtomicU32, Ordering};
    static CHANNELS: AtomicU32 = AtomicU32::new(2);

    /// Format for [`push`]: sample rate (8–192 kHz) and 1 or 2 channels.
    pub fn config(sample_rate: u32, channels: u32) {
        CHANNELS.store(channels, Ordering::Relaxed);
        unsafe { crate::sys::audio_config(sample_rate, channels) }
    }

    /// Queue interleaved f32 samples in [-1, 1].
    pub fn push(samples: &[f32]) {
        let ch = CHANNELS.load(Ordering::Relaxed).max(1);
        unsafe { crate::sys::audio_push(samples.as_ptr(), samples.len() as u32 / ch) }
    }
}

/// Virtual gamepad buttons (bit positions per spec/ABI.md).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Buttons(pub u32);

impl Buttons {
    pub const A: u32 = 1 << 0;
    pub const B: u32 = 1 << 1;
    pub const X: u32 = 1 << 2;
    pub const Y: u32 = 1 << 3;
    pub const L: u32 = 1 << 4;
    pub const R: u32 = 1 << 5;
    pub const SELECT: u32 = 1 << 6;
    pub const START: u32 = 1 << 7;
    pub const UP: u32 = 1 << 8;
    pub const DOWN: u32 = 1 << 9;
    pub const LEFT: u32 = 1 << 10;
    pub const RIGHT: u32 = 1 << 11;

    pub fn held(self, mask: u32) -> bool {
        self.0 & mask != 0
    }
}

/// Buttons held on virtual pad `player` (0..=3) this frame.
pub fn pad(player: u32) -> Buttons {
    Buttons(unsafe { sys::input_pad(player) })
}

/// Read a whole asset.
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

    pub const COPY_DST: u32 = 0x08;
    pub const INDEX: u32 = 0x10;
    pub const VERTEX: u32 = 0x20;
    pub const UNIFORM: u32 = 0x40;

    #[derive(Clone, Copy)]
    pub enum IndexFormat {
        U16 = 0,
        U32 = 1,
    }

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
    /// `{"pipeline":P,"group":0,"entries":[{"binding":0,"buffer":B,"offset":0,"size":N}]}`
    pub fn create_bind_group(json: &str) -> BindGroup {
        BindGroup(unsafe { sys::gfx_create_bind_group(json.as_ptr(), json.len() as u32) })
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
}

#[doc(hidden)]
pub struct GameCell<T>(pub UnsafeCell<Option<T>>);
// Wasm guests are single-threaded; the runner never calls exports concurrently.
unsafe impl<T> Sync for GameCell<T> {}

#[doc(hidden)]
pub fn __init<G: Game>(cell: &GameCell<G>) -> i32 {
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

#[doc(hidden)]
pub fn __frame<G: Game>(cell: &GameCell<G>) {
    if let Some(g) = unsafe { (*cell.0.get()).as_mut() } {
        g.frame();
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
            $crate::__init::<$t>(&__GASM_GAME)
        }
        #[unsafe(no_mangle)]
        pub extern "C" fn gasm_frame() {
            $crate::__frame::<$t>(&__GASM_GAME)
        }
    };
}
