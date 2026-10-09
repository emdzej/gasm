//! The gasm ABI v0 host: instantiates a guest and implements the "gasm",
//! "gasm:gfx", "gasm:net" and "gasm:storage" imports, plus the WASI subset in
//! [`crate::wasi`].

use std::collections::{HashMap, HashSet};
use std::future::Future;
use std::pin::Pin;
use std::sync::{Arc, OnceLock};
use std::task::Poll;
use std::time::{Duration, Instant};

use wasmtime::{Caller, Config, Engine, Instance, Linker, Memory, Module, Store, TypedFunc, bail, format_err};

use crate::assets::Assets;
use crate::audio::AudioOut;
use crate::gfx::Gfx;
use crate::net::Net;
use crate::present::letterbox;
use crate::storage::Storage;
use crate::switching::{self, Cmd, Exchange, Job};
use crate::wasi::{self, Splitmix};

pub const ABI_VERSION: i32 = 0;

/// FNV-1a 32-bit; mirrored in runners/web/gasm-host.js for cross-runner checks.
#[derive(Clone, Copy)]
pub struct Fnv(pub u32);

impl Default for Fnv {
    fn default() -> Self {
        Fnv(0x811c_9dc5)
    }
}

impl Fnv {
    pub fn new() -> Self {
        Self::default()
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

/// Headless virtual time: derived from the frame number, monotonic when the
/// guest changes its frame rate. Same arithmetic in gasm-host.js
/// (`VirtualClock`) and the `gasm::native` stub.
#[derive(Clone, Copy)]
pub struct VirtualClock {
    base_ms: f64,
    base_frame: u64,
    rate: f64,
}

impl Default for VirtualClock {
    fn default() -> Self {
        VirtualClock { base_ms: 0.0, base_frame: 0, rate: 60.0 }
    }
}

impl VirtualClock {
    /// Time at the start of `frame` (frames are counted from 0) at frame rate `rate`.
    pub fn at(&mut self, frame: u64, rate: f64) -> f64 {
        if rate != self.rate {
            self.base_ms += (frame - self.base_frame) as f64 * 1000.0 / self.rate;
            self.base_frame = frame;
            self.rate = rate;
        }
        self.base_ms + (frame - self.base_frame) as f64 * 1000.0 / self.rate
    }
}

/// Raw input for one frame, set by the runner before each `gasm_frame`.
/// `None` fields mean the runner has no such device (the import returns -1).
#[derive(Default, Clone)]
pub struct RawInput {
    /// held keys, bit k%8 of byte k/8 = GASM_KEY code k
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
    /// the runner shows 2D frames at whole multiples (`--integer-scale`)
    pub integer_scale: bool,
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
/// runners display it, `present::letterbox`). Same arithmetic in runners/web/lib/input.js.
pub fn frame_position(x: f32, y: f32, drawable: (f32, f32), frame: (usize, usize), integer_scale: bool, aspect: Option<(u32, u32)>) -> (f32, f32) {
    if frame.0 == 0 || frame.1 == 0 || drawable.0 <= 0.0 || drawable.1 <= 0.0 {
        return (x, y);
    }
    let (ox, oy, sx, sy) = letterbox((drawable.0 as f64, drawable.1 as f64), (frame.0 as f64, frame.1 as f64), integer_scale, aspect);
    (((x as f64 - ox) / sx) as f32, ((y as f64 - oy) / sy) as f32)
}

/// Longest title `set_title` keeps, in UTF-8 bytes.
pub const TITLE_MAX_BYTES: usize = 256;

/// The system time zone's offset from UTC now, in minutes east (daylight saving included).
#[cfg(unix)]
fn utc_offset_minutes() -> i32 {
    let now = unsafe { libc::time(std::ptr::null_mut()) };
    let mut tm: libc::tm = unsafe { std::mem::zeroed() };
    if unsafe { libc::localtime_r(&now, &mut tm) }.is_null() {
        return 0;
    }
    (tm.tm_gmtoff / 60) as i32
}

#[cfg(windows)]
fn utc_offset_minutes() -> i32 {
    use windows_sys::Win32::System::Time::{GetTimeZoneInformation, TIME_ZONE_INFORMATION};
    let mut tz: TIME_ZONE_INFORMATION = unsafe { std::mem::zeroed() };
    // UTC = local + bias: the standard or daylight bias by the current state
    let bias = match unsafe { GetTimeZoneInformation(&mut tz) } {
        1 => tz.Bias + tz.StandardBias, // TIME_ZONE_ID_STANDARD
        2 => tz.Bias + tz.DaylightBias, // TIME_ZONE_ID_DAYLIGHT
        0 => tz.Bias,                   // TIME_ZONE_ID_UNKNOWN: no daylight saving
        _ => 0,
    };
    -bias
}

#[cfg(not(any(unix, windows)))]
fn utc_offset_minutes() -> i32 {
    0
}

/// A guest's `set_title` text as runners show it: control characters (Unicode
/// Cc) and bidi controls (U+202A–U+202E, U+2066–U+2069) removed, then cut to
/// 256 bytes at a character boundary. None: empty, the runner's default.
/// Same rules in runners/web/lib/host.js `cleanTitle`.
pub fn clean_title(text: &str) -> Option<String> {
    let mut out = String::new();
    for c in text.chars().filter(|&c| !c.is_control() && !matches!(c, '\u{202a}'..='\u{202e}' | '\u{2066}'..='\u{2069}')) {
        if out.len() + c.len_utf8() > TITLE_MAX_BYTES {
            break;
        }
        out.push(c);
    }
    (!out.is_empty()).then_some(out)
}

/// The payload of custom section `name` in a wasm binary (None for a .cwasm or
/// a malformed module: the loader reports those).
pub fn custom_section<'a>(wasm: &'a [u8], name: &str) -> Option<&'a [u8]> {
    fn leb(b: &[u8], at: &mut usize) -> Option<usize> {
        let (mut v, mut shift) = (0usize, 0);
        loop {
            let byte = *b.get(*at)?;
            *at += 1;
            v |= ((byte & 0x7f) as usize).checked_shl(shift)?;
            if byte & 0x80 == 0 {
                return Some(v);
            }
            shift += 7;
            if shift > 28 {
                return None;
            }
        }
    }
    if wasm.get(..4)? != b"\0asm" {
        return None;
    }
    let mut at = 8;
    while at < wasm.len() {
        let id = wasm[at];
        at += 1;
        let size = leb(wasm, &mut at)?;
        let body = wasm.get(at..at.checked_add(size)?)?;
        at += size;
        if id == 0 {
            let mut p = 0;
            let n = leb(body, &mut p)?;
            if body.get(p..p.checked_add(n)?)? == name.as_bytes() {
                return body.get(p + n..);
            }
        }
    }
    None
}

/// The guest's built-in title (custom section `gasm.title`, UTF-8), cleaned like
/// set_title: the default title before (or without) set_title.
pub fn static_title(wasm: &[u8]) -> Option<String> {
    clean_title(std::str::from_utf8(custom_section(wasm, "gasm.title")?).ok()?)
}

/// The guest's memory cap (`--memory-limit`): growing past it traps, with a message
/// that says so (the JS runner checks after each guest call, with the same message).
pub struct MemoryLimit(pub Option<usize>);

impl wasmtime::ResourceLimiter for MemoryLimit {
    fn memory_growing(&mut self, _current: usize, desired: usize, _maximum: Option<usize>) -> wasmtime::Result<bool> {
        match self.0 {
            Some(limit) if desired > limit => bail!("{}", memory_limit_message(limit)),
            _ => Ok(true),
        }
    }
    fn table_growing(&mut self, _current: usize, _desired: usize, _maximum: Option<usize>) -> wasmtime::Result<bool> {
        Ok(true)
    }
}

/// The trap message when a guest needs more memory than allowed (also the JS runner's).
pub fn memory_limit_message(limit: usize) -> String {
    format!("the game needs more memory than its limit ({} MiB; see --memory-limit)", limit >> 20)
}

/// The default memory cap: far above what the games here use (Godot's 3D example: 39 MiB).
pub const DEFAULT_MEMORY_LIMIT: usize = 1 << 30;

pub struct Host {
    memory: Option<GuestMemory>,
    start: Instant,
    /// Headless runs use frame-derived time so results are reproducible (also
    /// the WASI clocks).
    pub virtual_time_ms: Option<f64>,
    /// Headless runs: a fixed `random_get` sequence; None = OS randomness.
    pub random: Option<Splitmix>,
    pub frame_rate: f64,
    pub pads: [u32; 4],
    /// UTF-8 typed since the previous frame (set by the runner before each frame);
    /// None: no keyboard, text_input returns -1
    pub text: Option<String>,
    pub input: RawInput,
    /// GASM_INPUT_* flags requested by the guest (input_mode)
    pub input_mode: u32,
    pub assets: Assets,
    pub params: HashMap<String, String>,
    pub audio: Option<Box<dyn AudioOut>>,
    pub gfx: Gfx,
    /// gasm:gl state (the null GL: natively, gl guests run headless)
    pub gl: crate::gl::Gl,
    /// frames run so far (gl queries and syncs are ready from the next frame on)
    pub frame_index: u64,
    pub net: Net,
    /// gasm:fetch (denied until the runner sets a policy: `Fetch::new`)
    pub fetch: crate::fetch::Fetch,
    /// gasm:clipboard: what the game may read this frame and what it copied
    pub clipboard: Clipboard,
    /// gasm:files: saves queued for the player (written between frames)
    pub files: crate::files::Files,
    /// the manifest's `requires`: the game is refused before it starts if one is missing
    pub requires: Vec<String>,
    /// wasi-threads: spawning and the shared memory (None: a module without threads)
    threads: Option<std::sync::Arc<crate::threads::Threads>>,
    pub storage: Storage,
    /// false during catch-up frames: gfx begin_frame returns 0
    pub show_frame: bool,
    audio_rate: u32,
    audio_channels: u32,
    /// imports this runner provides ("module" and "module.function"), for `has`
    provided: HashSet<String>,
    /// Last presented frame as RGBA8, tightly packed.
    pub rgba: Vec<u8>,
    pub width: usize,
    pub height: usize,
    pub frames_presented: u64,
    /// video_set_aspect (num, den); None: square pixels
    pub aspect: Option<(u32, u32)>,
    /// gasm_run guests: what the runner asks of the suspended guest (yield_frame)
    pub(crate) run: Option<Arc<Exchange>>,
    /// set_title (cleaned); None: the runner's default
    pub title: Option<String>,
    /// set_title changed `title` since the runner last looked (it clears this)
    pub title_changed: bool,
    pub hashing: bool,
    pub video_hash: Fnv,
    pub audio_hash: Fnv,
    pub audio_frames: u64,
    /// the guest's memory cap (`LoadOptions::memory_limit`)
    pub memory_limit: MemoryLimit,
    /// a catch-up frame (the runner shows only the last of a batch): gasm:gl `frame_shown` is 0
    pub catch_up: bool,
}

impl Host {
    /// A worker thread's host (threads.rs): compute only, the main host's clock origin.
    pub(crate) fn worker(threads: std::sync::Arc<crate::threads::Threads>, start: Instant, assets: Assets) -> Host {
        let mut h = Host::new(assets, HashMap::new(), None, Gfx::null(), Net::new(false), Storage::memory());
        h.start = start;
        h.threads = Some(threads);
        h
    }

    pub(crate) fn threads(&self) -> Option<&std::sync::Arc<crate::threads::Threads>> {
        self.threads.as_ref()
    }

    pub(crate) fn set_memory(&mut self, m: GuestMemory) {
        self.memory = Some(m);
    }

    pub fn new(
        assets: impl Into<Assets>,
        params: HashMap<String, String>,
        audio: Option<Box<dyn AudioOut>>,
        gfx: Gfx,
        net: Net,
        storage: Storage,
    ) -> Self {
        Host {
            memory: None,
            start: Instant::now(),
            virtual_time_ms: None,
            random: None,
            frame_rate: 60.0,
            pads: [0; 4],
            text: None,
            input: RawInput::default(),
            input_mode: 0,
            assets: assets.into(),
            params,
            audio,
            gfx,
            gl: Default::default(),
            fetch: Default::default(),
            clipboard: Default::default(),
            files: Default::default(),
            requires: Vec::new(),
            threads: None,
            memory_limit: MemoryLimit(Some(DEFAULT_MEMORY_LIMIT)),
            catch_up: false,
            frame_index: 0,
            net,
            storage,
            show_frame: true,
            audio_rate: 44100,
            audio_channels: 2,
            provided: HashSet::new(),
            title: None,
            title_changed: false,
            aspect: None,
            run: None,
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

    /// Reproducible mode for headless runs: virtual time (starting at 0) and a
    /// fixed random sequence.
    pub fn set_reproducible(&mut self) {
        self.virtual_time_ms = Some(0.0);
        self.random = Some(Splitmix(0));
    }

    pub(crate) fn started(&self) -> Instant {
        self.start
    }
}

/// Whether a module imports anything from `module` (e.g. "gasm:gl"), read before
/// loading it. A .cwasm is checked only with `allow_precompiled` (it is native code).
pub fn imports_module(wasm: &[u8], module: &str, allow_precompiled: bool) -> bool {
    if !wasm.starts_with(b"\0asm") {
        if !allow_precompiled {
            return false;
        }
        return unsafe { Module::deserialize(engine(), wasm) }.is_ok_and(|m| m.imports().any(|i| i.module() == module));
    }
    fn leb(b: &[u8], p: &mut usize) -> Option<u32> {
        let (mut v, mut shift) = (0u32, 0);
        loop {
            let byte = *b.get(*p)?;
            *p += 1;
            v |= ((byte & 0x7f) as u32) << shift;
            if byte & 0x80 == 0 {
                return Some(v);
            }
            shift += 7;
            if shift > 28 {
                return None;
            }
        }
    }
    fn name<'a>(b: &'a [u8], p: &mut usize) -> Option<&'a [u8]> {
        let n = leb(b, p)? as usize;
        let s = b.get(*p..*p + n)?;
        *p += n;
        Some(s)
    }
    let scan = || -> Option<bool> {
        let mut p = 8;
        while p < wasm.len() {
            let id = wasm[p];
            p += 1;
            let size = leb(wasm, &mut p)? as usize;
            let end = p.checked_add(size)?;
            if id == 2 {
                let mut q = p;
                for _ in 0..leb(wasm, &mut q)? {
                    if name(wasm, &mut q)? == module.as_bytes() {
                        return Some(true);
                    }
                    name(wasm, &mut q)?;
                    match *wasm.get(q)? {
                        0 => { q += 1; leb(wasm, &mut q)?; }                       // func: type index
                        1 => { q += 2; let f = wasm[q - 1]; leb(wasm, &mut q)?; if f & 1 != 0 { leb(wasm, &mut q)?; } } // table
                        2 => { q += 1; let f = *wasm.get(q)?; q += 1; leb(wasm, &mut q)?; if f & 1 != 0 { leb(wasm, &mut q)?; } } // memory
                        3 => q += 3,                                                 // global: type, mutability
                        4 => { q += 2; leb(wasm, &mut q)?; }                         // tag
                        _ => return None,
                    }
                }
                return Some(false);
            }
            p = end;
        }
        Some(false)
    };
    scan().unwrap_or(false)
}

/// Borrow `len` bytes at guest offset `ptr`, trapping the guest if out of bounds.
pub(crate) fn guest_slice(mem: &[u8], ptr: u32, len: u64) -> wasmtime::Result<&[u8]> {
    let start = ptr as u64;
    let end = start.checked_add(len).ok_or_else(|| format_err!("guest pointer overflow"))?;
    if end > mem.len() as u64 {
        bail!("guest access out of bounds: {start:#x}..{end:#x} (memory is {:#x})", mem.len());
    }
    Ok(&mem[start as usize..end as usize])
}

/// Mutable [`guest_slice`].
pub(crate) fn guest_slice_mut(mem: &mut [u8], ptr: u32, len: u64) -> wasmtime::Result<&mut [u8]> {
    let start = ptr as u64;
    let end = start.checked_add(len).ok_or_else(|| format_err!("guest pointer overflow"))?;
    if end > mem.len() as u64 {
        bail!("guest access out of bounds: {start:#x}..{end:#x} (memory is {:#x})", mem.len());
    }
    Ok(&mut mem[start as usize..end as usize])
}

/// The "copied only if it fits" convention: copy `src` to `dst` if `src.len() <= cap`;
/// returns the full length either way.
/// gasm:clipboard's state. The runner owns the system clipboard: it sets `pasted` for
/// the frame that carries the player's paste key press (cleared after it) and takes
/// `copied` after each frame. Headless runs never paste.
#[derive(Default)]
pub struct Clipboard {
    pub pasted: Option<String>,
    pub copied: Option<String>,
}

/// The largest text gasm:clipboard passes either way
pub const CLIPBOARD_MAX: usize = 1 << 20;

fn copy_if_fits(mem: &mut [u8], dst: u32, cap: u32, src: &[u8]) -> wasmtime::Result<i32> {
    if src.len() <= cap as usize {
        guest_slice_mut(mem, dst, src.len() as u64)?.copy_from_slice(src);
    }
    Ok(src.len() as i32)
}

/// The guest's linear memory: its own, or a shared one (wasi-threads: every thread's
/// instance imports the same). Host functions use it the same way either way.
#[derive(Clone)]
pub(crate) enum GuestMemory {
    Plain(Memory),
    Shared(wasmtime::SharedMemory),
}

impl GuestMemory {
    pub fn data<'a, T: 'static>(&self, store: impl Into<wasmtime::StoreContext<'a, T>>) -> &'a [u8] {
        match self {
            GuestMemory::Plain(m) => m.data(store),
            // SAFETY: shared memory is racy by nature (as in any native threaded program);
            // the guest synchronizes its threads, and the host only reads/writes the
            // ranges a call names, as it does for plain memory
            GuestMemory::Shared(m) => unsafe { std::slice::from_raw_parts(m.data().as_ptr() as *const u8, m.data().len()) },
        }
    }

    pub fn data_mut<'a, T: 'static>(&self, store: impl Into<wasmtime::StoreContextMut<'a, T>>) -> &'a mut [u8] {
        match self {
            GuestMemory::Plain(m) => m.data_mut(store),
            // SAFETY: as in data()
            GuestMemory::Shared(m) => unsafe { std::slice::from_raw_parts_mut(m.data().as_ptr() as *mut u8, m.data().len()) },
        }
    }

    pub fn data_and_store_mut<'a, S: wasmtime::AsContextMut + 'a>(&self, store: &'a mut S) -> (&'a mut [u8], &'a mut S::Data)
    where
        S::Data: 'static,
    {
        match self {
            GuestMemory::Plain(m) => m.data_and_store_mut(store),
            GuestMemory::Shared(m) => {
                // SAFETY: the shared memory isn't the store's, so its bytes and the store's
                // data don't alias; `store` is borrowed for 'a, which bounds both (as in data())
                let data = unsafe { std::slice::from_raw_parts_mut(m.data().as_ptr() as *mut u8, m.data().len()) };
                let host: *mut S::Data = store.as_context_mut().data_mut();
                (data, unsafe { &mut *host })
            }
        }
    }
}

pub(crate) fn memory(caller: &Caller<'_, Host>) -> wasmtime::Result<GuestMemory> {
    caller.data().memory.clone().ok_or_else(|| format_err!("guest memory not yet available"))
}

fn guest_str(caller: &Caller<'_, Host>, ptr: u32, len: u32) -> wasmtime::Result<String> {
    let mem = memory(caller)?;
    let bytes = guest_slice(mem.data(caller), ptr, len as u64)?;
    std::str::from_utf8(bytes).map(str::to_owned).map_err(|e| format_err!("string argument is not UTF-8: {e}"))
}

/// The pointer() record (GASM_POINTER_OFF_* layout).
fn pointer_bytes(p: &Pointer, frame: (usize, usize), aspect: Option<(u32, u32)>) -> [u8; POINTER_BYTES] {
    let (fx, fy) = frame_position(p.x, p.y, p.drawable, frame, p.integer_scale, aspect);
    let mut b = [0u8; POINTER_BYTES];
    for (i, v) in [p.x, p.y, fx, fy, p.dx, p.dy, p.wheel_x, p.wheel_y].into_iter().enumerate() {
        b[i * 4..i * 4 + 4].copy_from_slice(&v.to_le_bytes());
    }
    for (i, v) in [p.buttons, p.pressed, p.released, p.flags].into_iter().enumerate() {
        b[32 + i * 4..36 + i * 4].copy_from_slice(&v.to_le_bytes());
    }
    b
}

/// The gamepad() record (GASM_GAMEPAD_OFF_* layout).
fn gamepad_bytes(g: &Gamepad) -> [u8; GAMEPAD_BYTES] {
    let mut b = [0u8; GAMEPAD_BYTES];
    let nb = g.buttons.len().min(GAMEPAD_BUTTONS);
    let na = g.axes.len().min(GAMEPAD_AXES);
    let flags = if g.connected { 1 | if g.standard { 2 } else { 0 } } else { 0u32 };
    for (i, v) in [flags, nb as u32, na as u32].into_iter().enumerate() {
        b[i * 4..i * 4 + 4].copy_from_slice(&v.to_le_bytes());
    }
    for (i, v) in g.buttons.iter().take(nb).enumerate() {
        b[12 + i * 4..16 + i * 4].copy_from_slice(&v.to_le_bytes());
    }
    for (i, v) in g.axes.iter().take(na).enumerate() {
        b[140 + i * 4..144 + i * 4].copy_from_slice(&v.to_le_bytes());
    }
    b
}

fn add_gasm_imports(linker: &mut Linker<Host>) -> wasmtime::Result<()> {
    linker.func_wrap("gasm", "log", |caller: Caller<'_, Host>, ptr: u32, len: u32| -> wasmtime::Result<()> {
        let mem = memory(&caller)?;
        let msg = guest_slice(mem.data(&caller), ptr, len as u64)?;
        eprintln!("[guest] {}", String::from_utf8_lossy(msg));
        Ok(())
    })?;

    linker.func_wrap("gasm", "has", |caller: Caller<'_, Host>, ptr: u32, len: u32| -> wasmtime::Result<i32> {
        let name = guest_str(&caller, ptr, len)?;
        Ok(caller.data().provided.contains(&name) as i32)
    })?;

    linker.func_wrap("gasm", "video_set_aspect", |mut caller: Caller<'_, Host>, num: u32, den: u32| -> wasmtime::Result<()> {
        let aspect = match (num, den) {
            (0, 0) => None,
            (1..=65535, 1..=65535) if num as u64 * 8 >= den as u64 && den as u64 * 8 >= num as u64 => Some((num, den)),
            _ => bail!("video_set_aspect: invalid ratio {num}:{den}"),
        };
        caller.data_mut().aspect = aspect;
        Ok(())
    })?;

    linker.func_wrap("gasm", "set_title", |mut caller: Caller<'_, Host>, ptr: u32, len: u32| -> wasmtime::Result<()> {
        let title = clean_title(&guest_str(&caller, ptr, len)?);
        let h = caller.data_mut();
        if h.title != title {
            h.title = title;
            h.title_changed = true;
        }
        Ok(())
    })?;

    linker.func_wrap("gasm", "time_ms", |caller: Caller<'_, Host>| -> f64 {
        let h = caller.data();
        h.virtual_time_ms.unwrap_or_else(|| h.start.elapsed().as_secs_f64() * 1000.0)
    })?;

    linker.func_wrap("gasm", "max_threads", |caller: Caller<'_, Host>| -> u32 {
        caller.data().threads().map_or(0, |t| t.limit() as u32)
    })?;

    linker.func_wrap("gasm", "utc_offset_minutes", |caller: Caller<'_, Host>| -> i32 {
        // headless runs are reproducible: UTC, like their clocks
        if caller.data().virtual_time_ms.is_some() { 0 } else { utc_offset_minutes() }
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
            let row = w * 4;
            host.rgba.resize(row * h, 0);
            if stride == row {
                host.rgba.copy_from_slice(src);
            } else {
                for (y, dst) in host.rgba.chunks_exact_mut(row).enumerate() {
                    dst.copy_from_slice(&src[y * stride..y * stride + row]);
                }
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
            copy_if_fits(data, dst, cap, t.as_bytes())
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
            guest_slice_mut(data, dst, n as u64)?.copy_from_slice(&keys[..n]);
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
            let events = &host.input.key_events;
            let len = events.len() * 4;
            if len <= cap as usize {
                let out = guest_slice_mut(data, dst, len as u64)?;
                for (b, &(c, d)) in out.chunks_exact_mut(4).zip(events) {
                    let c = c.to_le_bytes();
                    b.copy_from_slice(&[c[0], c[1], d as u8, 0]);
                }
            }
            Ok(len as i32)
        },
    )?;

    linker.func_wrap(
        "gasm",
        "pointer",
        |mut caller: Caller<'_, Host>, dst: u32, cap: u32| -> wasmtime::Result<i32> {
            let mem = memory(&caller)?;
            let (data, host) = mem.data_and_store_mut(&mut caller);
            let Some(p) = &host.input.pointer else { return Ok(-1) };
            if cap as usize >= POINTER_BYTES {
                let b = pointer_bytes(p, (host.width, host.height), host.aspect);
                guest_slice_mut(data, dst, POINTER_BYTES as u64)?.copy_from_slice(&b);
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
                let b = gamepad_bytes(&pads[slot as usize]);
                guest_slice_mut(data, dst, GAMEPAD_BYTES as u64)?.copy_from_slice(&b);
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
            copy_if_fits(data, dst, cap, g.name.as_bytes())
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
            copy_if_fits(data, dst, cap, v.as_bytes())
        },
    )?;

    // Assets: n = bytes available from offset (clamped to the request) is bounds-checked
    // against guest memory before anything is read.
    fn read_asset(caller: &mut Caller<'_, Host>, ptr: u32, len: u32, offset: u64, dst: u32, cap: u32) -> wasmtime::Result<i32> {
        let name = guest_str(caller, ptr, len)?;
        let mem = memory(caller)?;
        let (data, host) = mem.data_and_store_mut(caller);
        let Some(asset) = host.assets.get(&name) else { return Ok(-1) };
        let n = asset.size().saturating_sub(offset).min(cap as u64);
        let out = guest_slice_mut(data, dst, n)?;
        Ok(asset.read_at(offset, out) as i32)
    }

    linker.func_wrap(
        "gasm",
        "asset_read_at",
        |mut caller: Caller<'_, Host>, ptr: u32, len: u32, offset: u32, dst: u32, cap: u32| -> wasmtime::Result<i32> {
            read_asset(&mut caller, ptr, len, offset as u64, dst, cap)
        },
    )?;

    linker.func_wrap(
        "gasm",
        "asset_read_at64",
        |mut caller: Caller<'_, Host>, ptr: u32, len: u32, offset: u64, dst: u32, cap: u32| -> wasmtime::Result<i32> {
            read_asset(&mut caller, ptr, len, offset, dst, cap)
        },
    )?;

    linker.func_wrap(
        "gasm",
        "asset_read",
        |mut caller: Caller<'_, Host>, ptr: u32, len: u32, dst: u32, cap: u32| -> wasmtime::Result<i32> {
            read_asset(&mut caller, ptr, len, 0, dst, cap)
        },
    )?;

    linker.func_wrap("gasm", "asset_count", |caller: Caller<'_, Host>| -> u32 { caller.data().assets.names().len() as u32 })?;

    linker.func_wrap(
        "gasm",
        "asset_name",
        |mut caller: Caller<'_, Host>, index: u32, dst: u32, cap: u32| -> wasmtime::Result<i32> {
            let mem = memory(&caller)?;
            let (data, host) = mem.data_and_store_mut(&mut caller);
            let Some(n) = host.assets.names().get(index as usize) else { return Ok(-1) };
            copy_if_fits(data, dst, cap, n.as_bytes())
        },
    )?;

    linker.func_wrap(
        "gasm",
        "asset_size",
        |caller: Caller<'_, Host>, ptr: u32, len: u32| -> wasmtime::Result<i32> {
            let name = guest_str(&caller, ptr, len)?;
            Ok(match caller.data().assets.size(&name) {
                None => -1,
                Some(n) if n > i32::MAX as u64 => -2,
                Some(n) => n as i32,
            })
        },
    )?;

    linker.func_wrap(
        "gasm",
        "asset_size64",
        |caller: Caller<'_, Host>, ptr: u32, len: u32| -> wasmtime::Result<i64> {
            let name = guest_str(&caller, ptr, len)?;
            Ok(caller.data().assets.size(&name).map_or(-1, |n| n as i64))
        },
    )?;

    linker.func_wrap(
        "gasm",
        "asset_version",
        |caller: Caller<'_, Host>, ptr: u32, len: u32| -> wasmtime::Result<i32> {
            let name = guest_str(&caller, ptr, len)?;
            Ok(caller.data().assets.version(&name).map_or(-1, |v| v.min(i32::MAX as u32) as i32))
        },
    )?;
    Ok(())
}

fn trap(e: String) -> wasmtime::Error {
    format_err!("{e}")
}

fn add_gfx_imports(linker: &mut Linker<Host>) -> wasmtime::Result<()> {
    const M: &str = "gasm:gfx";
    linker.func_wrap(M, "width", |c: Caller<'_, Host>| -> u32 { c.data().gfx.size().0 })?;
    linker.func_wrap(M, "height", |c: Caller<'_, Host>| -> u32 { c.data().gfx.size().1 })?;
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
            host.gfx.write_buffer(buf, offset, bytes).map_err(trap)?;
            if host.hashing {
                host.video_hash.update(bytes);
            }
            Ok(())
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
            let mut offsets = [0u32; 32];
            if count as usize > offsets.len() {
                bail!("gfx.set_bind_group_offsets: {count} offsets (a bind group has at most {})", offsets.len());
            }
            for (o, b) in offsets.iter_mut().zip(bytes.chunks_exact(4)) {
                *o = u32::from_le_bytes([b[0], b[1], b[2], b[3]]);
            }
            host.gfx.set_bind_group_offsets(i, bg, &offsets[..count as usize]).map_err(trap)
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
    linker.func_wrap(M, "draw", |mut c: Caller<'_, Host>, vc: u32, ic: u32, fv: u32, fi: u32| -> wasmtime::Result<()> {
        c.data_mut().gfx.draw(vc, ic, fv, fi).map_err(trap)
    })?;
    linker.func_wrap(M, "draw_indexed", |mut c: Caller<'_, Host>, ic: u32, n: u32, first: u32, base: i32, fi: u32| -> wasmtime::Result<()> {
        c.data_mut().gfx.draw_indexed(ic, n, first, base, fi).map_err(trap)
    })?;
    linker.func_wrap(M, "end_frame", |mut c: Caller<'_, Host>| -> wasmtime::Result<()> {
        let h = c.data_mut();
        h.frames_presented += 1;
        h.gfx.end_frame().map_err(trap)
    })?;
    linker.func_wrap(M, "destroy", |mut c: Caller<'_, Host>, handle: u32| -> wasmtime::Result<()> {
        c.data_mut().gfx.destroy(handle).map_err(trap)
    })?;
    Ok(())
}

fn add_net_imports(linker: &mut Linker<Host>) -> wasmtime::Result<()> {
    const M: &str = "gasm:net";
    linker.func_wrap(M, "open", |mut c: Caller<'_, Host>, ptr: u32, len: u32| -> wasmtime::Result<i32> {
        let url = guest_str(&c, ptr, len)?;
        Ok(c.data_mut().net.open(&url))
    })?;
    linker.func_wrap(M, "state", |c: Caller<'_, Host>, h: i32| -> wasmtime::Result<u32> { c.data().net.state(h).map_err(trap) })?;
    linker.func_wrap(M, "send", |mut c: Caller<'_, Host>, h: i32, ptr: u32, len: u32| -> wasmtime::Result<i32> {
        let mem = memory(&c)?;
        let msg = guest_slice(mem.data(&c), ptr, len as u64)?.to_vec();
        if msg.is_empty() {
            c.data().net.state(h).map_err(trap)?; // still validates the handle
            return Ok(-1);
        }
        c.data_mut().net.send(h, msg).map_err(trap)
    })?;
    linker.func_wrap(M, "recv", |mut c: Caller<'_, Host>, h: i32, dst: u32, cap: u32| -> wasmtime::Result<i32> {
        let mem = memory(&c)?;
        let (data, host) = mem.data_and_store_mut(&mut c);
        let msg = match host.net.peek(h).map_err(trap)? {
            Ok(m) => m,
            Err(code) => return Ok(code),
        };
        let n = copy_if_fits(data, dst, cap, msg)?;
        if n as usize <= cap as usize {
            host.net.pop(h);
        }
        Ok(n)
    })?;
    linker.func_wrap(M, "close", |mut c: Caller<'_, Host>, h: i32| -> wasmtime::Result<()> { c.data_mut().net.close(h).map_err(trap) })?;
    Ok(())
}

fn add_fetch_imports(linker: &mut Linker<Host>) -> wasmtime::Result<()> {
    const M: &str = "gasm:fetch";
    linker.func_wrap(M, "request", |mut c: Caller<'_, Host>, dp: u32, dl: u32, bp: u32, bl: u32| -> wasmtime::Result<i32> {
        let desc = guest_str(&c, dp, dl)?;
        let mem = memory(&c)?;
        let body = guest_slice(mem.data(&c), bp, bl as u64)?.to_vec();
        let h = c.data_mut();
        let frame = h.frame_index;
        Ok(h.fetch.request(&desc, body, frame))
    })?;
    linker.func_wrap(M, "state", |mut c: Caller<'_, Host>, r: i32| -> wasmtime::Result<u32> {
        let h = c.data_mut();
        let frame = h.frame_index;
        h.fetch.state(r, frame).map_err(trap)
    })?;
    linker.func_wrap(M, "status", |mut c: Caller<'_, Host>, r: i32| -> wasmtime::Result<i32> {
        let h = c.data_mut();
        let frame = h.frame_index;
        h.fetch.status(r, frame).map_err(trap)
    })?;
    linker.func_wrap(M, "headers", |mut c: Caller<'_, Host>, r: i32, dst: u32, cap: u32| -> wasmtime::Result<i32> {
        let mem = memory(&c)?;
        let (data, host) = mem.data_and_store_mut(&mut c);
        let frame = host.frame_index;
        match host.fetch.headers(r, frame).map_err(trap)? {
            Some(text) => copy_if_fits(data, dst, cap, text.as_bytes()),
            None => Ok(-1),
        }
    })?;
    linker.func_wrap(M, "read", |mut c: Caller<'_, Host>, r: i32, dst: u32, cap: u32| -> wasmtime::Result<i32> {
        let mem = memory(&c)?;
        guest_slice(mem.data(&c), dst, cap as u64)?; // the whole destination must be guest memory
        let (data, host) = mem.data_and_store_mut(&mut c);
        let frame = host.frame_index;
        match host.fetch.read(r, (cap as usize).min(i32::MAX as usize), frame).map_err(trap)? {
            Ok(bytes) => {
                guest_slice_mut(data, dst, bytes.len() as u64)?.copy_from_slice(&bytes);
                Ok(bytes.len() as i32)
            }
            Err(()) => Ok(-1),
        }
    })?;
    linker.func_wrap(M, "close", |mut c: Caller<'_, Host>, r: i32| -> wasmtime::Result<()> { c.data_mut().fetch.close(r).map_err(trap) })?;
    Ok(())
}

/// Every gasm import this runner has.
fn add_all_imports(linker: &mut Linker<Host>, switching: bool) -> wasmtime::Result<()> {
    add_gasm_imports(linker)?;
    if switching {
        switching::add_yield_async(linker)?;
    } else {
        switching::add_yield_sync(linker)?;
    }
    add_gfx_imports(linker)?;
    crate::gl::add_gl_imports(linker)?;
    add_net_imports(linker)?;
    add_fetch_imports(linker)?;
    add_storage_imports(linker)?;
    add_clipboard_imports(linker)?;
    add_files_imports(linker)?;
    Ok(())
}

/// What `has` reports: the gasm modules and functions linked (not WASI stubs or traps).
fn provided_by(linker: &Linker<Host>, store: &mut Store<Host>) -> HashSet<String> {
    let mut provided = HashSet::new();
    for (m, f, _) in linker.iter(&mut *store) {
        provided.insert(m.to_owned());
        provided.insert(format!("{m}.{f}"));
    }
    for f in wasi::IMPLEMENTED {
        provided.insert(format!("{}.{f}", wasi::MODULE));
    }
    provided.insert(wasi::MODULE.to_owned());
    provided
}

/// What this runner provides (`gasm.has`), without a game (`gasm-run --info`).
pub fn provided() -> HashSet<String> {
    let mut linker: Linker<Host> = Linker::new(engine());
    if add_all_imports(&mut linker, false).is_err() {
        return HashSet::new();
    }
    let host = Host::new(crate::assets::Assets::new(), HashMap::new(), None, crate::gfx::Gfx::null(), Net::new(false), crate::storage::Storage::memory());
    let mut store = Store::new(engine(), host);
    provided_by(&linker, &mut store)
}

fn add_clipboard_imports(linker: &mut Linker<Host>) -> wasmtime::Result<()> {
    const M: &str = "gasm:clipboard";
    linker.func_wrap(M, "set_text", |mut c: Caller<'_, Host>, ptr: u32, len: u32| -> wasmtime::Result<i32> {
        if len as usize > CLIPBOARD_MAX {
            return Ok(-1);
        }
        let text = guest_str(&c, ptr, len)?;
        c.data_mut().clipboard.copied = Some(text);
        Ok(0)
    })?;
    linker.func_wrap(M, "get_text", |mut c: Caller<'_, Host>, dst: u32, cap: u32| -> wasmtime::Result<i32> {
        let mem = memory(&c)?;
        let (data, host) = mem.data_and_store_mut(&mut c);
        match &host.clipboard.pasted {
            Some(t) => copy_if_fits(data, dst, cap, t.as_bytes()),
            None => Ok(-1),
        }
    })?;
    Ok(())
}

fn add_files_imports(linker: &mut Linker<Host>) -> wasmtime::Result<()> {
    const M: &str = "gasm:files";
    linker.func_wrap(M, "save", |mut c: Caller<'_, Host>, np: u32, nl: u32, mp: u32, ml: u32, ptr: u32, len: u32| -> wasmtime::Result<i32> {
        let (name, mime) = (guest_str(&c, np, nl)?, guest_str(&c, mp, ml)?);
        let mem = memory(&c)?;
        let (data, host) = mem.data_and_store_mut(&mut c);
        let bytes = guest_slice(data, ptr, len as u64)?;
        Ok(host.files.save(&name, &mime, bytes))
    })?;
    linker.func_wrap(M, "state", |c: Caller<'_, Host>, handle: i32| -> wasmtime::Result<i32> {
        c.data().files.state(handle).ok_or_else(|| format_err!("gasm:files: invalid handle {handle}"))
    })?;
    Ok(())
}

fn add_storage_imports(linker: &mut Linker<Host>) -> wasmtime::Result<()> {
    const M: &str = "gasm:storage";
    linker.func_wrap(M, "get", |mut c: Caller<'_, Host>, kp: u32, kl: u32, dst: u32, cap: u32| -> wasmtime::Result<i32> {
        let key = guest_str(&c, kp, kl)?;
        let mem = memory(&c)?;
        let (data, host) = mem.data_and_store_mut(&mut c);
        let Some(v) = host.storage.get(&key) else { return Ok(-1) };
        copy_if_fits(data, dst, cap, v)
    })?;
    linker.func_wrap(M, "set", |mut c: Caller<'_, Host>, kp: u32, kl: u32, vp: u32, vl: u32| -> wasmtime::Result<i32> {
        let key = guest_str(&c, kp, kl)?;
        let mem = memory(&c)?;
        let (data, host) = mem.data_and_store_mut(&mut c);
        let value = guest_slice(data, vp, vl as u64)?;
        Ok(match host.storage.set(&key, value) {
            Ok(()) => 0,
            Err(e) => {
                eprintln!("[gasm] storage: {key}: {e}");
                e.code()
            }
        })
    })?;
    linker.func_wrap(M, "count", |mut c: Caller<'_, Host>| -> u32 { c.data_mut().storage.keys().len() as u32 })?;
    linker.func_wrap(M, "key", |mut c: Caller<'_, Host>, index: u32, dst: u32, cap: u32| -> wasmtime::Result<i32> {
        let mem = memory(&c)?;
        let (data, host) = mem.data_and_store_mut(&mut c);
        let Some(k) = host.storage.keys().get(index as usize) else { return Ok(-1) };
        copy_if_fits(data, dst, cap, k.as_bytes())
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
    if let Some(exit) = e.downcast_ref::<wasi::Exit>() {
        return Stop::Exit(exit.0);
    }
    if e.downcast_ref::<switching::Quit>().is_some() {
        return Stop::Exit(0);
    }
    if e.downcast_ref::<wasmtime::Trap>() == Some(&wasmtime::Trap::Interrupt) {
        return Stop::Trap(format!("a guest call ran longer than the time limit (stuck in a loop?); see --call-timeout\n{e:?}"));
    }
    Stop::Trap(format!("{e:?}"))
}

/// Epoch ticks per second (the watchdog thread's rate).
const TICKS_PER_SEC: u64 = 10;

/// The process-wide engine: the same configuration for compiling, precompiling
/// and loading (`.cwasm` files must match it). Epoch interruption lets a guest
/// call that never returns be stopped (`LoadOptions::call_timeout`). Calls can be
/// sync (`gasm_frame` guests) or async (`gasm_run` guests: stack switching).
pub fn engine() -> &'static Engine {
    static ENGINE: OnceLock<Engine> = OnceLock::new();
    ENGINE.get_or_init(new_engine)
}

fn new_engine() -> Engine {
    let mut config = Config::new();
    config.epoch_interruption(true);
    // wasi-threads guests (shared memory); modules without one are unaffected
    config.shared_memory(true);
    // a gasm_run guest's whole run is on this stack: the default wasm stack limit plus room
    config.async_stack_size(4 << 20);
    let engine = Engine::new(&config).expect("wasmtime engine");
    let ticker = engine.clone();
    std::thread::Builder::new()
        .name("gasm-epoch".into())
        .spawn(move || {
            loop {
                std::thread::sleep(Duration::from_millis(1000 / TICKS_PER_SEC));
                ticker.increment_epoch();
            }
        })
        .expect("epoch thread");
    engine
}

/// Ahead-of-time compile a guest to native code for this host (no JIT needed at load).
pub fn precompile(wasm: &[u8]) -> wasmtime::Result<Vec<u8>> {
    engine().precompile_module(wasm)
}

/// How to load a guest.
#[derive(Clone, Copy)]
pub struct LoadOptions {
    /// Accept a precompiled `.cwasm` (native code: only files you made yourself
    /// with `gasm-run --compile`). Otherwise only `.wasm` modules load.
    pub allow_precompiled: bool,
    /// Trap a single guest call (init, a frame, exit) that runs longer than this.
    pub call_timeout: Option<Duration>,
    /// Run guests that export `gasm_run` that way (the runner switches stacks).
    /// Off: always `gasm_frame` (tests of the Asyncify path).
    pub stack_switching: bool,
    /// Largest guest memory in bytes (None: the engine's maximum, 4 GiB for wasm32).
    pub memory_limit: Option<usize>,
    /// Most worker threads a wasi-threads guest may have at once (0: `thread-spawn` fails,
    /// as in reproducible runs).
    pub threads: usize,
}

impl Default for LoadOptions {
    fn default() -> Self {
        LoadOptions { allow_precompiled: false, call_timeout: Some(Duration::from_secs(30)), stack_switching: true, memory_limit: Some(DEFAULT_MEMORY_LIMIT), threads: 0 }
    }
}

/// The running `gasm_run` call: owns the store until the run ends.
type Running = Pin<Box<dyn Future<Output = (Store<Host>, wasmtime::Result<i32>)>>>;

/// A running guest. `gasm_frame` guests are called once per frame; `gasm_run`
/// guests run in one suspended call (stack switching) that each [`Game::frame`]
/// resumes until its next `yield_frame`.
pub struct Game {
    /// None while a `gasm_run` call is running (the call owns it): use [`Game::with_host`]
    store: Option<Store<Host>>,
    frame: TypedFunc<(), ()>,
    exit: Option<TypedFunc<(), ()>>,
    deadline: u64,
    /// `gasm_run` (async engine)
    run: Option<TypedFunc<(), i32>>,
    running: Option<Running>,
    exchange: Option<Arc<Exchange>>,
    /// the guest exited or trapped: it is never called again
    ended: bool,
    /// the guest imports gasm:gl (the native window has no GL backend yet)
    gl: bool,
    /// files re-read as assets when they change (`--watch-asset`), checked before each frame
    watches: Vec<crate::assets::AssetWatch>,
}

impl Game {
    pub fn load(wasm: &[u8], host: Host, opts: LoadOptions) -> Result<Game, Stop> {
        let module = Self::compile(wasm, opts.allow_precompiled)?;
        Self::load_module(module, host, opts)
    }

    /// Compile a module (or load a `.cwasm`) for [`Game::load_module`]: the slow
    /// part of loading, and possible on another thread.
    pub fn compile(wasm: &[u8], allow_precompiled: bool) -> Result<Module, Stop> {
        let module = if wasm.starts_with(b"\0asm") {
            Module::new(engine(), wasm)
        } else if allow_precompiled {
            // Precompiled artifact from `gasm-run --compile`: native code, trusted
            // like the runner itself (unlike .wasm, which is sandboxed).
            unsafe { Module::deserialize(engine(), wasm) }
        } else {
            Err(format_err!("not a wasm module (a precompiled .cwasm runs as native code: pass --allow-precompiled if you made it yourself)"))
        };
        module.map_err(classify)
    }

    /// Instantiate a compiled module and run its init.
    pub fn load_module(module: Module, host: Host, opts: LoadOptions) -> Result<Game, Stop> {
        Self::load_inner(module, host, opts).map_err(classify)
    }

    fn load_inner(module: Module, host: Host, opts: LoadOptions) -> wasmtime::Result<Game> {
        let gl = module.imports().any(|i| i.module() == "gasm:gl");
        if gl && module.imports().any(|i| i.module() == "gasm:gfx") {
            bail!("a module imports gasm:gfx or gasm:gl, not both");
        }
        // guests that export gasm_run run in one async call (stack switching)
        let has_run = module.get_export("gasm_run").is_some();
        let switching = has_run && opts.stack_switching;
        if has_run && !switching && module.imports().any(|i| i.module() == "asyncify") {
            // made without wasm-opt --asyncify: its gasm_frame can't suspend
            bail!("this module is a run build (no Asyncify): it needs stack switching (gasm_run); use the game's Asyncify build");
        }
        let engine = engine();
        let mut linker: Linker<Host> = Linker::new(engine);
        add_all_imports(&mut linker, switching)?;
        let mut store = Store::new(engine, host);
        store.data_mut().memory_limit = MemoryLimit(opts.memory_limit);
        store.limiter(|h| &mut h.memory_limit);
        let provided = provided_by(&linker, &mut store);
        // the manifest's requirements, before the guest runs at all
        let missing: Vec<&String> = store.data().requires.iter().filter(|r| !provided.contains(r.as_str())).collect();
        if !missing.is_empty() {
            bail!("this game needs {}, which this runner (gasm-run {}) doesn't have", missing.iter().map(|s| s.as_str()).collect::<Vec<_>>().join(", "), env!("CARGO_PKG_VERSION"));
        }
        store.data_mut().provided = provided;
        wasi::add_to_linker(&mut linker, &module)?;
        // wasi-threads: a shared memory the module imports (every thread's instance gets
        // the same one) and thread-spawn
        let threads = match crate::threads::shared_memory_import(&module) {
            Some(ty) => {
                if switching {
                    bail!("a module with threads (shared memory) can't use gasm_run (stack switching)");
                }
                let mem = wasmtime::SharedMemory::new(engine, ty)?;
                let import = module.imports().find(|i| matches!(i.ty(), wasmtime::ExternType::Memory(_))).expect("memory import");
                linker.define(&mut store, import.module(), import.name(), mem.clone())?;
                let t = crate::threads::Threads::new(engine, &module, mem, opts.threads, store.data().start);
                store.data_mut().threads = Some(t.clone());
                Some(t)
            }
            None => None,
        };
        crate::threads::add_to_linker(&mut linker)?;
        // Imports this runner doesn't know (e.g. JS glue some Rust crates pull in)
        // link as traps: harmless unless the guest actually calls them.
        linker.define_unknown_imports_as_traps(&module)?;
        if let Some(t) = &threads {
            t.set_linker(linker.clone());
        }

        let deadline = opts.call_timeout.map_or(u64::MAX / 2, |t| (t.as_millis() as u64 * TICKS_PER_SEC).div_ceil(1000).max(1));
        store.epoch_deadline_trap();
        store.set_epoch_deadline(deadline);
        let exchange = switching.then(|| Exchange::new(deadline));
        store.data_mut().run = exchange.clone();
        let instance: Instance = if switching {
            switching::block_on(linker.instantiate_async(&mut store, &module))??
        } else {
            linker.instantiate(&mut store, &module)?
        };
        let memory = match instance.get_memory(&mut store, "memory") {
            Some(m) => GuestMemory::Plain(m),
            None => threads.as_ref().map(|t| GuestMemory::Shared(t.memory().clone())).ok_or_else(|| format_err!("guest does not export `memory`"))?,
        };
        store.data_mut().memory = Some(memory);

        // a call that must not suspend, on either engine
        fn call<R: wasmtime::WasmResults + Send + Sync>(store: &mut Store<Host>, f: TypedFunc<(), R>, switching: bool) -> wasmtime::Result<R> {
            if switching { switching::block_on(f.call_async(store, ()))? } else { f.call(store, ()) }
        }
        if let Ok(init) = instance.get_typed_func::<(), ()>(&mut store, "_initialize") {
            store.set_epoch_deadline(deadline);
            call(&mut store, init, switching)?;
        }
        let version = instance.get_typed_func::<(), i32>(&mut store, "gasm_abi_version")?;
        let version = call(&mut store, version, switching)?;
        if version != ABI_VERSION {
            bail!("guest targets gasm ABI v{version}, runner implements v{ABI_VERSION}");
        }
        store.set_epoch_deadline(deadline);
        let init = instance.get_typed_func::<(), i32>(&mut store, "gasm_init")?;
        let rc = call(&mut store, init, switching)?;
        if rc != 0 {
            bail!("gasm_init failed with code {rc}");
        }
        let frame = instance.get_typed_func::<(), ()>(&mut store, "gasm_frame")?;
        let exit = instance.get_typed_func::<(), ()>(&mut store, "gasm_exit").ok();
        let run = if switching { Some(instance.get_typed_func::<(), i32>(&mut store, "gasm_run")?) } else { None };
        Ok(Game { store: Some(store), frame, exit, deadline, run, running: None, exchange, ended: false, gl, watches: Vec::new() })
    }

    /// Add or replace an asset between frames: the guest sees it from the next
    /// frame on, with a new `gasm.asset_version`. Returns that version.
    pub fn set_asset(&mut self, name: &str, bytes: Vec<u8>) -> u32 {
        self.with_host(|h| h.assets.set(name, bytes))
    }

    /// Remove an asset between frames. False if there was none.
    pub fn remove_asset(&mut self, name: &str) -> bool {
        self.with_host(|h| h.assets.remove(name))
    }

    /// Re-read `path` as asset `name` whenever the file changes (in place or by a
    /// rename), checked before each frame.
    pub fn watch_asset(&mut self, name: &str, path: &std::path::Path) {
        self.add_watch(crate::assets::AssetWatch::new(name, path));
    }

    /// [`Game::watch_asset`] with a watch made earlier (when the asset was first opened).
    pub fn add_watch(&mut self, watch: crate::assets::AssetWatch) {
        self.watches.push(watch);
    }

    /// Whether the guest imports gasm:gl.
    pub fn uses_gl(&self) -> bool {
        self.gl
    }

    /// Whether the guest runs through `gasm_run` (the runner switches stacks).
    pub fn switching(&self) -> bool {
        self.run.is_some()
    }

    /// One frame: call `gasm_frame`, or resume `gasm_run` until its next `yield_frame`.
    pub fn frame(&mut self) -> Result<(), Stop> {
        if self.ended {
            return Err(Stop::Trap("the guest is not running".into()));
        }
        // the last frame's saves (gasm:files)
        self.with_host(|h| h.files.flush());
        if !self.watches.is_empty() {
            let mut watches = std::mem::take(&mut self.watches);
            self.with_host(|h| watches.iter_mut().for_each(|w| _ = w.poll(&mut h.assets)));
            self.watches = watches;
        }
        let Some(run) = self.run.clone() else {
            let store = self.store.as_mut().expect("store");
            store.set_epoch_deadline(self.deadline);
            let mut r = self.frame.call(&mut *store, ()).map_err(classify);
            store.data_mut().frame_index += 1;
            // a worker thread that trapped or exited ends the game too (wasi-threads)
            if let (Ok(()), Some(end)) = (&r, store.data().threads.as_ref().and_then(|t| t.ended())) {
                r = Err(match end {
                    crate::threads::ThreadEnd::Exit(c) => Stop::Exit(c),
                    crate::threads::ThreadEnd::Trap(t) => Stop::Trap(t),
                });
            }
            self.ended = r.is_err();
            return r;
        };
        match &self.exchange {
            Some(ex) if self.running.is_some() => ex.set(Cmd::Resume),
            _ => {
                // frame 0 starts the run; the call owns the store from now on
                let mut store = self.store.take().expect("store");
                store.set_epoch_deadline(self.deadline);
                self.running = Some(Box::pin(async move {
                    let r = run.call_async(&mut store, ()).await;
                    (store, r)
                }));
            }
        }
        self.poll()
    }

    /// Poll the running `gasm_run` call: Ok when it suspended again, the reason when it ended.
    fn poll(&mut self) -> Result<(), Stop> {
        let Some(running) = self.running.as_mut() else { return Ok(()) };
        match switching::poll_once(running.as_mut()) {
            Poll::Pending => Ok(()),
            Poll::Ready((store, r)) => {
                self.store = Some(store);
                self.running = None;
                self.ended = true;
                Err(match r {
                    Ok(code) => Stop::Exit(code),
                    Err(e) => classify(e),
                })
            }
        }
    }

    /// Best-effort "the player is quitting" notification (optional `gasm_exit` export).
    /// A suspended `gasm_run` guest gets it on top of its run, which then ends.
    pub fn exit(&mut self) {
        if self.store.is_some() || self.running.is_some() {
            self.with_host(|h| h.files.flush());
        }
        if self.running.is_some() {
            if let Some(ex) = &self.exchange {
                ex.set(Cmd::Exit);
            }
            if let Err(Stop::Trap(t)) = self.poll() {
                eprintln!("[gasm] gasm_exit trapped: {t}");
            }
            if self.running.is_some() {
                // gasm_exit yielded: give up on the run (the store stays with it)
                self.running = None;
            }
            self.exit = None;
            return;
        }
        if let (Some(f), Some(store)) = (self.exit.take(), self.store.as_mut()) {
            store.set_epoch_deadline(self.deadline);
            let r = if self.run.is_some() { switching::block_on(f.call_async(&mut *store, ())).and_then(|r| r) } else { f.call(&mut *store, ()) };
            if let Err(e) = r {
                if let Stop::Trap(t) = classify(e) {
                    eprintln!("[gasm] gasm_exit trapped: {t}");
                }
            }
            store.data_mut().files.flush(); // saved on the way out
        }
    }

    /// Run `f` on the host: directly, or (a suspended `gasm_run` guest) through
    /// the guest's `yield_frame`, which owns the store meanwhile. Runners use this
    /// between frames for everything they read or set.
    pub fn with_host<R>(&mut self, f: impl FnOnce(&mut Host) -> R) -> R {
        if let Some(store) = &mut self.store {
            return f(store.data_mut());
        }
        let (Some(running), Some(ex)) = (self.running.as_mut(), &self.exchange) else {
            panic!("with_host: the guest's store is gone (gasm_exit yielded)");
        };
        let mut f = Some(f);
        let mut out = None;
        let mut job = |h: &mut Host| out = Some((f.take().expect("job runs once"))(h));
        let ptr: *mut (dyn FnMut(&mut Host) + '_) = &mut job;
        // SAFETY: the pointer is only used by yield_frame during the poll below,
        // while `job` is alive on this stack; it is dropped before we return.
        let ptr: *mut (dyn FnMut(&mut Host) + 'static) = unsafe { std::mem::transmute(ptr) };
        ex.set(Cmd::Job(Job(ptr)));
        let polled = switching::poll_once(running.as_mut());
        ex.set(Cmd::None);
        match polled {
            Poll::Pending => {}
            Poll::Ready(_) => unreachable!("yield_frame stays suspended after a job"),
        }
        out.expect("yield_frame ran the job")
    }

    /// The host of a guest that isn't suspended in `gasm_run` (every `gasm_frame`
    /// guest; a `gasm_run` guest before its first frame and after it ended).
    /// Otherwise use [`Game::with_host`].
    pub fn host(&self) -> &Host {
        self.store.as_ref().expect("Game::host: a gasm_run guest is running; use Game::with_host").data()
    }

    pub fn host_mut(&mut self) -> &mut Host {
        self.store.as_mut().expect("Game::host_mut: a gasm_run guest is running; use Game::with_host").data_mut()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn titles_are_cleaned_and_cut() {
        assert_eq!(clean_title("A\u{7}B\u{202e}C\u{2069}").as_deref(), Some("ABC"));
        assert_eq!(clean_title("\n\t"), None);
        // 200 two-byte characters: cut at 256 bytes, on a character boundary
        assert_eq!(clean_title(&"é".repeat(200)).map(|t| t.len()), Some(256));
    }

    const RUN_GUEST: &str = r#"(module
      (import "gasm" "yield_frame" (func $yield))
      (memory (export "memory") 1)
      (global $n (mut i32) (i32.const 0))
      (func (export "gasm_abi_version") (result i32) i32.const 0)
      (func (export "gasm_init") (result i32) i32.const 0)
      (func (export "gasm_frame") call $yield)
      (func (export "gasm_exit") (global.set $n (i32.const 100)))
      (func (export "frames") (result i32) global.get $n)
      (func (export "gasm_run") (result i32)
        (loop $l
          (global.set $n (i32.add (global.get $n) (i32.const 1)))
          call $yield
          (br_if $l (i32.lt_u (global.get $n) (i32.const 3))))
        i32.const 7))"#;

    fn host() -> Host {
        Host::new(Assets::new(), HashMap::new(), None, Gfx::null(), Net::new(false), Storage::memory())
    }

    #[test]
    fn gasm_run_guests_suspend_between_frames() {
        let mut g = Game::load(&wat::parse_str(RUN_GUEST).unwrap(), host(), LoadOptions::default()).unwrap();
        assert!(g.switching());
        g.with_host(|h| h.frame_rate = 30.0); // before the first frame: the store is ours
        assert!(g.frame().is_ok()); // n = 1, suspended in yield_frame
        // suspended: the host is reached through the guest's yield_frame
        assert_eq!(g.with_host(|h| h.frame_rate), 30.0);
        assert!(g.frame().is_ok()); // n = 2
        assert!(g.frame().is_ok()); // n = 3
        // the loop ends: gasm_run returns 7, the exit code
        assert!(matches!(g.frame(), Err(Stop::Exit(7))));
        assert!(g.frame().is_err(), "never called again");
    }

    #[test]
    fn exit_runs_on_top_of_a_suspended_run() {
        let mut g = Game::load(&wat::parse_str(RUN_GUEST).unwrap(), host(), LoadOptions::default()).unwrap();
        g.frame().unwrap();
        g.exit();
        // the store is back after the run ended; gasm_exit ran (n = 100)
        assert!(g.store.is_some());
    }

    #[test]
    fn yield_frame_outside_gasm_run_traps() {
        let opts = LoadOptions { stack_switching: false, ..LoadOptions::default() };
        let mut g = Game::load(&wat::parse_str(RUN_GUEST).unwrap(), host(), opts).unwrap();
        assert!(!g.switching());
        match g.frame() {
            Err(Stop::Trap(t)) => assert!(t.contains("only inside gasm_run"), "{t}"),
            other => panic!("expected a trap, got {:?}", other.err()),
        }
    }

    #[test]
    fn custom_sections_are_found() {
        // magic + version, a type section (id 1, empty), then custom "gasm.title" = "Hi\0"
        let mut wasm = b"\0asm\x01\0\0\0\x01\x01\0".to_vec();
        wasm.extend_from_slice(&[0, 14, 10]);
        wasm.extend_from_slice(b"gasm.titleHi\0");
        assert_eq!(custom_section(&wasm, "gasm.title"), Some(&b"Hi\0"[..]));
        assert_eq!(static_title(&wasm).as_deref(), Some("Hi"));
        assert_eq!(custom_section(&wasm, "other"), None);
        assert_eq!(custom_section(&wasm[..wasm.len() - 2], "gasm.title"), None); // truncated
        assert_eq!(custom_section(b"not wasm", "gasm.title"), None);
    }
}
