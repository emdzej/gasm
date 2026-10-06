//! The windowed runner: winit window, wgpu surface, cpal audio, gilrs gamepads,
//! keyboard layouts, fixed-timestep frames with catch-up, and the hold-Escape quit.

use std::collections::HashSet;
use std::sync::Arc;
use std::time::{Duration, Instant};

use gilrs::{Button, Gilrs};
use winit::application::ApplicationHandler;
use winit::dpi::LogicalSize;
use winit::event::{DeviceEvent, DeviceId, ElementState, MouseButton, MouseScrollDelta, WindowEvent};
use winit::event_loop::{ActiveEventLoop, ControlFlow, EventLoop};
use winit::keyboard::{Key, KeyCode, NamedKey, PhysicalKey};
use winit::window::{CursorGrabMode, Window, WindowId};

use crate::audio::{AudioOut, AudioSink, AudioStream};
use crate::angle::NativeWindow;
use crate::gfx::Gfx;
use crate::host::{self, Game, Gamepad, KEY_STATE_BYTES, Pointer, RawInput, Stop};
use crate::keymap;
use crate::present::Present;
use crate::session::Session;
use crate::splash;

pub struct Options {
    /// initial window size in logical pixels
    pub size: (u32, u32),
    pub keymap: keymap::Keymap,
    pub mute: bool,
    /// how 2D frames are shown
    pub present: Present,
    /// write frame N as the window shows it and quit (gasm:gl games; for tests)
    pub screenshot: Option<(u64, String)>,
    /// show the gasm splash screen while the game loads (`--no-splash`: off)
    pub splash: bool,
    /// copy the game's frame to the clipboard on this key (`--copy-key`; None: off)
    pub copy_key: Option<KeyCode>,
}

/// The splash screen, shown while the game's module compiles on another thread.
struct Splash {
    frame: u32,
    next: Instant,
    compiling: Option<std::thread::JoinHandle<Result<wasmtime::Module, Stop>>>,
    module: Option<Result<wasmtime::Module, Stop>>,
    /// a key or click: end it as soon as the game is ready
    skip: bool,
    /// what the game starts with once it's compiled
    audio: Option<Box<dyn AudioOut>>,
    gfx: Gfx,
    gl: Option<crate::angle::Angle>,
}

/// The platform window handle ANGLE draws into.
fn native_window(window: &Window) -> Result<NativeWindow, String> {
    use winit::raw_window_handle::{HasDisplayHandle, HasWindowHandle, RawDisplayHandle, RawWindowHandle};
    let w = window.window_handle().map_err(|e| e.to_string())?.as_raw();
    let d = window.display_handle().map_err(|e| e.to_string())?.as_raw();
    Ok(match (w, d) {
        (RawWindowHandle::AppKit(h), _) => NativeWindow::AppKit(h.ns_view.as_ptr()),
        (RawWindowHandle::Win32(h), _) => NativeWindow::Win32(h.hwnd.get() as *mut std::ffi::c_void),
        (RawWindowHandle::Xlib(h), RawDisplayHandle::Xlib(d)) => {
            NativeWindow::Xlib(d.display.map_or(std::ptr::null_mut(), |p| p.as_ptr()), h.window)
        }
        (RawWindowHandle::Wayland(_), RawDisplayHandle::Wayland(d)) => NativeWindow::Wayland(d.display.as_ptr()),
        _ => return Err("gasm:gl: this kind of window isn't supported".into()),
    })
}

/// Open a window and play until the guest exits, traps or the player quits.
/// Returns the guest's exit code (0 when the player quit).
pub fn run(session: Session, opts: Options) -> Result<i32, String> {
    let mut builder = EventLoop::builder();
    // gasm:gl draws with ANGLE, which needs an X11 window here (XWayland on Wayland desktops)
    #[cfg(all(unix, not(target_os = "macos")))]
    if session.uses_gl() {
        use winit::platform::x11::EventLoopBuilderExtX11;
        builder.with_x11();
    }
    let event_loop = builder.build().map_err(|e| e.to_string())?;
    // default title: the module file name without .wasm (set_title replaces it)
    let default_title = host::static_title(&session.wasm).unwrap_or_else(|| {
        std::path::Path::new(&session.name).file_stem().map_or(session.name.clone(), |s| s.to_string_lossy().into_owned())
    });
    let mut app = App {
        title: default_title.clone(),
        default_title,
        fps: 0,
        opts,
        consent: session.consent.clone(),
        asking: None,
        swallowed: HashSet::new(),
        session: Some(session),
        window: None,
        game: None,
        keys: HashSet::new(),
        typed: String::new(),
        key_events: Vec::new(),
        esc_down: None,
        mouse: MouseState::default(),
        applied_mode: 0,
        pointer_flags: 0,
        gilrs: Gilrs::new().ok(),
        next: Instant::now(),
        fps_t: Instant::now(),
        fps_n: 0,
        result: Ok(0),
        frames_run: 0,
        audio_stream: None,
        splash: None,
        copy_pending: false,
        clipboard: None,
        paste: None,
    };
    event_loop.run_app(&mut app).map_err(|e| e.to_string())?;
    app.result
}

/// Gamepads in connection order, plus how many are connected.
fn gamepad_pads(gilrs: &mut Gilrs) -> ([u32; 4], usize) {
    const MAP: &[(Button, u32)] = &[
        (Button::East, 1 << 0),
        (Button::South, 1 << 1),
        (Button::North, 1 << 2),
        (Button::West, 1 << 3),
        (Button::LeftTrigger, 1 << 4),
        (Button::RightTrigger, 1 << 5),
        (Button::Select, 1 << 6),
        (Button::Start, 1 << 7),
        (Button::DPadUp, 1 << 8),
        (Button::DPadDown, 1 << 9),
        (Button::DPadLeft, 1 << 10),
        (Button::DPadRight, 1 << 11),
    ];
    while gilrs.next_event().is_some() {}
    let mut pads = [0u32; 4];
    let connected = gilrs.gamepads().count();
    for (i, (_, pad)) in gilrs.gamepads().take(4).enumerate() {
        pads[i] = MAP.iter().filter(|(b, _)| pad.is_pressed(*b)).fold(0, |m, (_, bit)| m | bit);
        let (x, y) = (pad.value(gilrs::Axis::LeftStickX), pad.value(gilrs::Axis::LeftStickY));
        if x < -0.5 { pads[i] |= 1 << 10 }
        if x > 0.5 { pads[i] |= 1 << 11 }
        if y > 0.5 { pads[i] |= 1 << 8 }
        if y < -0.5 { pads[i] |= 1 << 9 }
    }
    (pads, connected)
}

/// Mouse input collected between frames (window pixels).
#[derive(Default)]
struct MouseState {
    x: f32,
    y: f32,
    inside: bool,
    buttons: u32,
    /// buttons that went down / up since the last frame (a click inside one frame counts)
    pressed: u32,
    released: u32,
    dx: f32,
    dy: f32,
    wheel_x: f32,
    wheel_y: f32,
}

/// Hold Escape this long to quit (a tap goes to the game).
const ESC_HOLD: Duration = Duration::from_millis(1000);

fn mouse_bit(b: MouseButton) -> u32 {
    match b {
        MouseButton::Left => 1,
        MouseButton::Right => 2,
        MouseButton::Middle => 4,
        MouseButton::Back => 8,
        MouseButton::Forward => 16,
        MouseButton::Other(_) => 0,
    }
}

/// Raw gamepads (first 4 connected, the same order as the pads). Known
/// controllers in the W3C standard layout; others raw, ordered by code.
fn gamepads_raw(gilrs: &Gilrs) -> [Gamepad; 4] {
    use gilrs::{Axis, MappingSource};
    const BUTTONS: [Button; 17] = [
        Button::South, Button::East, Button::West, Button::North, Button::LeftTrigger, Button::RightTrigger,
        Button::LeftTrigger2, Button::RightTrigger2, Button::Select, Button::Start, Button::LeftThumb,
        Button::RightThumb, Button::DPadUp, Button::DPadDown, Button::DPadLeft, Button::DPadRight, Button::Mode,
    ];
    let mut out: [Gamepad; 4] = Default::default();
    for (slot, (_, pad)) in gilrs.gamepads().take(4).enumerate() {
        let g = &mut out[slot];
        g.connected = true;
        g.name = pad.name().to_owned();
        if pad.mapping_source() != MappingSource::None {
            g.standard = true;
            g.buttons = BUTTONS.iter().map(|b| pad.button_data(*b).map_or(0.0, |d| d.value())).collect();
            // W3C: y axes point down
            g.axes = vec![pad.value(Axis::LeftStickX), -pad.value(Axis::LeftStickY), pad.value(Axis::RightStickX), -pad.value(Axis::RightStickY)];
        } else {
            let mut b: Vec<_> = pad.state().buttons().map(|(c, d)| (c.into_u32(), d.value())).collect();
            let mut a: Vec<_> = pad.state().axes().map(|(c, d)| (c.into_u32(), d.value())).collect();
            b.sort_by_key(|x| x.0);
            a.sort_by_key(|x| x.0);
            g.buttons = b.into_iter().map(|x| x.1).take(host::GAMEPAD_BUTTONS).collect();
            g.axes = a.into_iter().map(|x| x.1).take(host::GAMEPAD_AXES).collect();
        }
    }
    out
}

struct App {
    /// frames run so far (--window-screenshot)
    frames_run: u64,
    /// what the window shows before " — gasm": set_title, else `default_title`
    title: String,
    default_title: String,
    /// guest frames in the last second
    fps: u32,
    opts: Options,
    /// taken when the window opens
    session: Option<Session>,
    window: Option<Arc<Window>>,
    game: Option<Game>,
    keys: HashSet<KeyCode>,
    /// text typed since the last frame (for text_input)
    typed: String,
    /// raw key presses/releases since the last frame
    key_events: Vec<(u16, bool)>,
    /// Escape: a tap goes to the game, holding it quits
    esc_down: Option<Instant>,
    mouse: MouseState,
    /// cursor mode currently applied to the window (GASM_INPUT_POINTER_*)
    applied_mode: u32,
    /// GASM_POINTER_IS_HIDDEN / IS_LOCKED as actually achieved
    pointer_flags: u32,
    gilrs: Option<Gilrs>,
    next: Instant,
    fps_t: Instant,
    fps_n: u32,
    result: Result<i32, String>,
    /// the audio device stream (kept alive while the game plays)
    audio_stream: Option<AudioStream>,
    splash: Option<Splash>,
    /// the copy key was pressed: copy the next frame shown
    copy_pending: bool,
    /// kept open: on X11 and Wayland the copied image lives as long as its owner
    clipboard: Option<arboard::Clipboard>,
    /// the player pressed the paste key: the clipboard's text, for the next frame (gasm:clipboard)
    paste: Option<String>,
    /// the player's answers about hosts and saves (None: --no-ask)
    consent: Option<crate::consent::Consent>,
    /// the question on screen (the game waits meanwhile)
    asking: Option<crate::consent::Subject>,
    /// keys pressed to answer: their releases aren't the game's
    swallowed: HashSet<KeyCode>,
}

/// The system clipboard, opened on first use.
fn open_clipboard(clipboard: &mut Option<arboard::Clipboard>) -> Option<&mut arboard::Clipboard> {
    if clipboard.is_none() {
        match arboard::Clipboard::new() {
            Ok(c) => *clipboard = Some(c),
            Err(e) => eprintln!("[gasm] clipboard: unavailable ({e})"),
        }
    }
    clipboard.as_mut()
}

/// Put a frame (RGBA8, top to bottom) on the system clipboard.
fn copy_frame(clipboard: &mut Option<arboard::Clipboard>, w: u32, h: u32, mut rgba: Vec<u8>) {
    // opaque, as the window shows it (GL frames can carry any alpha)
    for px in rgba.chunks_exact_mut(4) {
        px[3] = 255;
    }
    if open_clipboard(clipboard).is_none() {
        return;
    }
    let img = arboard::ImageData { width: w as usize, height: h as usize, bytes: rgba.into() };
    match clipboard.as_mut().map(|c| c.set_image(img)) {
        Some(Ok(())) => eprintln!("[gasm] copied the frame ({w}x{h}) to the clipboard"),
        Some(Err(e)) => eprintln!("[gasm] copy: {e}"),
        None => {}
    }
}

impl App {

    fn stop(&mut self, el: &ActiveEventLoop, result: Result<i32, String>) {
        self.result = result;
        // the event loop can tick again before it ends: never call into a guest
        // that has exited or trapped
        self.game = None;
        el.exit();
    }

    /// Hide / lock the cursor as the guest asked (input_mode), when it changes.
    /// The window title: the game's (set_title or the file name), then the runner's own part,
    /// which the guest can't change.
    fn show_title(&self) {
        if let Some(w) = &self.window {
            w.set_title(&format!("{} — gasm — {} fps", self.title, self.fps));
        }
    }

    fn apply_input_mode(&mut self) {
        let (Some(game), Some(w)) = (&mut self.game, &self.window) else { return };
        let want = game.with_host(|h| h.input_mode) & 6;
        if want == self.applied_mode {
            return;
        }
        self.applied_mode = want;
        let locked = want & 4 != 0
            && w.set_cursor_grab(CursorGrabMode::Locked).or_else(|_| w.set_cursor_grab(CursorGrabMode::Confined)).is_ok();
        if want & 4 == 0 {
            let _ = w.set_cursor_grab(CursorGrabMode::None);
        }
        // locked implies hidden
        w.set_cursor_visible(want == 0);
        self.pointer_flags = if want != 0 { 2 } else { 0 } | if locked { 4 } else { 0 };
    }

    /// The player closed the window / held Esc: let the game save, then stop.
    fn quit(&mut self, el: &ActiveEventLoop) {
        if let Some(g) = &mut self.game {
            g.exit();
        }
        self.stop(el, Ok(0));
    }

    /// One splash frame when it's due; starts the game when the splash is over and
    /// the module compiled. Holds on the logo while it is still compiling.
    fn tick_splash(&mut self, el: &ActiveEventLoop) {
        let Some(sp) = &mut self.splash else { return };
        if Instant::now() < sp.next {
            return el.set_control_flow(ControlFlow::WaitUntil(sp.next));
        }
        if sp.compiling.as_ref().is_some_and(|t| t.is_finished()) {
            let t = sp.compiling.take().expect("compile thread");
            sp.module = Some(t.join().unwrap_or_else(|_| Err(Stop::Trap("the compiler thread panicked".into()))));
        }
        let ready = sp.module.is_some();
        if sp.skip {
            sp.frame = if ready { splash::FRAMES } else { sp.frame.max(splash::HOLD) };
        }
        if sp.frame >= splash::FRAMES && ready {
            // the manifest's hosts and saves, and mods that want hosts: answered before the game starts
            if let Some(q) = self.consent.as_ref().and_then(|c| c.lock().unwrap().question().cloned()) {
                if self.asking.as_ref() != Some(&q) {
                    let (wants, what) = crate::consent::describe(&q);
                    eprintln!("[gasm] consent: {} {wants} {what}? (1: this time, 2: always, 3: not now, 4: never)", self.default_title);
                }
                let img = crate::prompt::image(&self.default_title, &q);
                let (w, h) = (crate::prompt::W as u32, crate::prompt::H as u32);
                match &sp.gl {
                    Some(angle) => angle.show_image(&img, w, h),
                    None => sp.gfx.present_video(&img, w, h, None),
                }
                self.asking = Some(q);
                return el.set_control_flow(ControlFlow::WaitUntil(Instant::now() + Duration::from_millis(50)));
            }
            self.asking = None;
            let sp = self.splash.take().expect("splash");
            return self.start_game(el, sp.module.expect("compiled"), sp.audio, sp.gfx, sp.gl);
        }
        let rgba = splash::frame(sp.frame);
        match &sp.gl {
            Some(angle) => angle.show_image(&rgba, splash::W as u32, splash::H as u32),
            None => sp.gfx.present_video(&rgba, splash::W as u32, splash::H as u32, None),
        }
        if sp.frame != splash::HOLD || ready {
            sp.frame += 1;
        }
        sp.next += Duration::from_secs_f64(1.0 / 60.0);
        if sp.next < Instant::now() {
            sp.next = Instant::now();
        }
        el.set_control_flow(ControlFlow::WaitUntil(sp.next));
    }

    /// The game begins: instantiate it (its init runs) with the window's GPU and audio.
    fn start_game(&mut self, el: &ActiveEventLoop, module: Result<wasmtime::Module, Stop>, audio: Option<Box<dyn AudioOut>>, gfx: Gfx, gl: Option<crate::angle::Angle>) {
        let Some(session) = self.session.take() else { return };
        let started = match module {
            Ok(m) => session.start_compiled(m, audio, gfx, gl),
            Err(e) => Err(e),
        };
        match started {
            Ok(g) => self.game = Some(g),
            Err(Stop::Exit(code)) => return self.stop(el, Ok(code)),
            Err(Stop::Trap(e)) => return self.stop(el, Err(e)),
        }
        // what the player pressed during the splash isn't the game's
        self.typed.clear();
        self.key_events.clear();
        self.mouse.pressed = 0;
        self.mouse.released = 0;
        self.next = Instant::now();
        el.set_control_flow(ControlFlow::WaitUntil(self.next));
    }

    fn tick(&mut self, el: &ActiveEventLoop) {
        if self.splash.is_some() {
            if self.esc_down.is_some_and(|t| t.elapsed() >= ESC_HOLD) {
                return self.quit(el);
            }
            return self.tick_splash(el);
        }
        let Some(game) = &mut self.game else { return };
        let period = Duration::from_secs_f64(1.0 / game.with_host(|h| h.frame_rate));
        let now = Instant::now();
        // a question for the player (a host, saving files): the game waits for the answer
        if let Some(q) = self.consent.as_ref().and_then(|c| c.lock().unwrap().question().cloned()) {
            if self.esc_down.is_some_and(|t| t.elapsed() >= ESC_HOLD) {
                return self.quit(el);
            }
            if self.asking.as_ref() != Some(&q) {
                let (wants, what) = crate::consent::describe(&q);
                eprintln!("[gasm] consent: {} {wants} {what}? (1: this time, 2: always, 3: not now, 4: never)", self.default_title);
            }
            let img = crate::prompt::image(&self.default_title, &q);
            let (w, h) = (crate::prompt::W as u32, crate::prompt::H as u32);
            game.with_host(|host| {
                if !host.gl.show_over_game(&img, w, h) {
                    host.gfx.present_video(&img, w, h, None);
                }
            });
            self.asking = Some(q);
            self.next = now + period; // no catch-up for the time spent asking
            return el.set_control_flow(ControlFlow::WaitUntil(now + Duration::from_millis(50)));
        }
        self.asking = None;
        if now >= self.next {
            // Fixed timestep: catch up at most 4 frames, only the last one is shown.
            let behind = ((now - self.next).as_secs_f64() / period.as_secs_f64()) as u32 + 1;
            let steps = behind.min(4);
            // Escape held long enough: quit (a short tap went to the game as a key)
            if self.esc_down.is_some_and(|t| t.elapsed() >= ESC_HOLD) {
                return self.quit(el);
            }
            let mut keys = [0u8; KEY_STATE_BYTES];
            for &c in &self.keys {
                let k = keymap::gasm_key(c);
                if k != 0 {
                    keys[k as usize / 8] |= 1 << (k % 8);
                }
            }
            // gamepads are sampled once per tick: pads and raw state agree within a batch
            let (mut pads, gamepads) = self.gilrs.as_mut().map(gamepad_pads).unwrap_or_default();
            let raw_pads = self.gilrs.as_ref().map(gamepads_raw).unwrap_or_default();
            // a guest that reads the keyboard itself (KEYS_RAW) gets no keymap pads
            if game.with_host(|h| h.input_mode) & 1 == 0 {
                let kb = keymap::pads(&self.opts.keymap, &self.keys, gamepads);
                for (p, k) in pads.iter_mut().zip(kb) {
                    *p |= k;
                }
            }
            let drawable = game.with_host(|h| h.gl.size().unwrap_or_else(|| h.gfx.size()));
            for k in 0..steps {
                game.with_host(|host| {
                host.pads = pads;
                // typed text, key events and relative motion go to the first frame of a catch-up batch
                let first = k == 0;
                host.text = Some(if first { std::mem::take(&mut self.typed) } else { String::new() });
                let m = &mut self.mouse;
                let pointer = Pointer {
                    x: m.x,
                    y: m.y,
                    dx: if first { m.dx } else { 0.0 },
                    dy: if first { m.dy } else { 0.0 },
                    wheel_x: if first { m.wheel_x } else { 0.0 },
                    wheel_y: if first { m.wheel_y } else { 0.0 },
                    buttons: m.buttons,
                    pressed: if first { m.pressed } else { 0 },
                    released: if first { m.released } else { 0 },
                    flags: m.inside as u32 | self.pointer_flags,
                    drawable: (drawable.0 as f32, drawable.1 as f32),
                    integer_scale: self.opts.present.integer_scale,
                };
                if first {
                    (m.dx, m.dy, m.wheel_x, m.wheel_y, m.pressed, m.released) = (0.0, 0.0, 0.0, 0.0, 0, 0);
                }
                if first {
                    host.input = RawInput { keys: Some(keys), key_events: std::mem::take(&mut self.key_events), pointer: Some(pointer), gamepads: Some(raw_pads.clone()) };
                    // pasted text goes with the frame that carries the paste key press
                    host.clipboard.pasted = self.paste.take();
                } else {
                    host.input.key_events.clear();
                    host.clipboard.pasted = None;
                    host.input.pointer = Some(pointer);
                }
                host.show_frame = k + 1 == steps;
                host.catch_up = k + 1 < steps;
                host.gfx.used = false;
                });
                let r = game.frame();
                self.frames_run += 1;
                let shot = self.opts.screenshot.as_ref().filter(|(n, _)| *n == self.frames_run).map(|(_, p)| p.clone());
                let copy = std::mem::take(&mut self.copy_pending);
                let (img, copied, text) = game.with_host(|h| {
                    let img = shot.as_ref().and_then(|_| h.gl.read_frame());
                    // the copy key: the GL frame, else the last 2D frame
                    let copied = copy.then(|| h.gl.read_frame().or_else(|| (h.width > 0).then(|| (h.width as u32, h.height as u32, h.rgba.clone())))).flatten();
                    let show = h.show_frame;
                    h.gl.end_frame(show);
                    (img, copied, h.clipboard.copied.take())
                });
                if let Some(t) = text {
                    if let Some(Err(e)) = open_clipboard(&mut self.clipboard).map(|c| c.set_text(t)) {
                        eprintln!("[gasm] clipboard: {e}");
                    }
                }
                match copied {
                    Some((w, h, rgba)) => copy_frame(&mut self.clipboard, w, h, rgba),
                    None if copy => eprintln!("[gasm] copy: nothing to copy (gasm:gfx games can't be copied yet)"),
                    None => {}
                }
                if let Some(path) = shot {
                    let r = match img {
                        Some((w, h, rgba)) => crate::headless::write_png(&path, w, h, &rgba).map(|_| eprintln!("[gasm] wrote {path}")),
                        None => Err("--window-screenshot: only gasm:gl games".into()),
                    };
                    return match r {
                        Ok(()) => self.quit(el),
                        Err(e) => self.stop(el, Err(e)),
                    };
                }
                match r {
                    Ok(()) => {}
                    Err(Stop::Exit(code)) => return self.stop(el, Ok(code)),
                    Err(Stop::Trap(e)) => return self.stop(el, Err(e)),
                }
                self.next += period;
                self.fps_n += 1;
            }
            if behind > 4 {
                self.next = Instant::now() + period;
            }
            // 2D guests: show the last video_present frame.
            let title = game.with_host(|host| {
                if !host.gfx.used && host.width > 0 {
                    let (w, h) = (host.width as u32, host.height as u32);
                    let rgba = std::mem::take(&mut host.rgba);
                    host.gfx.present_video(&rgba, w, h, host.aspect);
                    host.rgba = rgba;
                }
                std::mem::take(&mut host.title_changed).then(|| host.title.clone())
            });
            if let Some(t) = title {
                self.title = t.unwrap_or_else(|| self.default_title.clone());
                self.show_title();
            }
        }
        self.apply_input_mode();
        if let Some(s) = &mut self.audio_stream {
            s.check(); // a broken stream or a new output device: reopen
        }
        if self.fps_t.elapsed() >= Duration::from_secs(1) {
            self.fps = self.fps_n;
            self.show_title();
            self.fps_n = 0;
            self.fps_t = Instant::now();
        }
        el.set_control_flow(ControlFlow::WaitUntil(self.next));
    }
}

impl ApplicationHandler for App {
    fn resumed(&mut self, el: &ActiveEventLoop) {
        if self.window.is_some() {
            return;
        }
        let attrs = Window::default_attributes()
            .with_title(format!("{} — gasm", self.title))
            .with_inner_size(LogicalSize::new(self.opts.size.0, self.opts.size.1));
        let window = match el.create_window(attrs) {
            Ok(w) => Arc::new(w),
            Err(e) => return self.stop(el, Err(format!("cannot create window: {e}"))),
        };
        // gasm:gl games draw with ANGLE into the window; everything else with wgpu
        let uses_gl = self.session.as_ref().is_some_and(|s| s.uses_gl());
        let mut gl = None;
        let gfx = if uses_gl {
            let native = match native_window(&window) {
                Ok(n) => n,
                Err(e) => return self.stop(el, Err(e)),
            };
            match self.session.as_ref().map(|s| s.open_gl(Some(native), (0, 0))) {
                Some(Ok(a)) => gl = Some(a),
                Some(Err(e)) => return self.stop(el, Err(format!("cannot initialise OpenGL ES (gasm:gl): {e}"))),
                None => return,
            }
            Gfx::null()
        } else {
            match Gfx::for_window(window.clone()) {
                Ok(mut g) => {
                    g.present = self.opts.present;
                    g
                }
                Err(e) => return self.stop(el, Err(format!("cannot initialise GPU: {e}"))),
            }
        };
        let audio: Option<Box<dyn AudioOut>> = if self.opts.mute {
            None
        } else {
            match AudioSink::open() {
                Ok((a, stream)) => {
                    eprintln!("[gasm] audio: {} Hz", a.device_rate());
                    self.audio_stream = Some(stream);
                    Some(Box::new(a))
                }
                Err(e) => {
                    eprintln!("[gasm] audio disabled: {e}");
                    None
                }
            }
        };
        self.window = Some(window);
        let Some(session) = self.session.as_ref() else { return };
        // questions before the start (manifest, mods) go through the splash's path too, at its end
        let asking = self.consent.as_ref().is_some_and(|c| c.lock().unwrap().asking());
        if self.opts.splash || asking {
            // the module compiles while the splash plays; the game starts after both
            let compiling = Some(session.compile_in_background());
            let next = Instant::now();
            let frame = if self.opts.splash { 0 } else { splash::FRAMES };
            self.splash = Some(Splash { frame, next, compiling, module: None, skip: false, audio, gfx, gl });
            el.set_control_flow(ControlFlow::WaitUntil(next));
        } else {
            let module = crate::host::Game::compile(&session.wasm, session.load.allow_precompiled);
            self.start_game(el, module, audio, gfx, gl);
        }
    }

    fn window_event(&mut self, el: &ActiveEventLoop, _id: WindowId, event: WindowEvent) {
        match event {
            WindowEvent::CloseRequested => self.quit(el),
            WindowEvent::Resized(size) => {
                if let Some(g) = &mut self.game {
                    g.with_host(|h| h.gfx.resize(size.width, size.height));
                }
                if let Some(sp) = &mut self.splash {
                    sp.gfx.resize(size.width, size.height);
                }
            }
            WindowEvent::Focused(false) => {
                // nothing stays held while another app has the input
                for c in self.keys.drain() {
                    let k = keymap::gasm_key(c);
                    if k != 0 {
                        self.key_events.push((k, false));
                    }
                }
                self.esc_down = None;
                self.mouse.inside = false;
                self.mouse.released |= self.mouse.buttons;
                self.mouse.buttons = 0;
            }
            WindowEvent::CursorMoved { position, .. } => {
                self.mouse.x = position.x as f32;
                self.mouse.y = position.y as f32;
                self.mouse.inside = true;
            }
            WindowEvent::CursorEntered { .. } => self.mouse.inside = true,
            WindowEvent::CursorLeft { .. } => self.mouse.inside = false,
            WindowEvent::MouseInput { state, button, .. } => {
                if let Some(sp) = &mut self.splash {
                    sp.skip |= state == ElementState::Pressed;
                }
                let bit = mouse_bit(button);
                match state {
                    ElementState::Pressed => {
                        self.mouse.buttons |= bit;
                        self.mouse.pressed |= bit;
                    }
                    ElementState::Released => {
                        self.mouse.buttons &= !bit;
                        self.mouse.released |= bit;
                    }
                }
            }
            WindowEvent::MouseWheel { delta, .. } => {
                // gasm: about 1 per wheel notch, y > 0 = down (W3C sign); winit's is the opposite
                let (x, y) = match delta {
                    MouseScrollDelta::LineDelta(x, y) => (x, y),
                    MouseScrollDelta::PixelDelta(p) => (p.x as f32 / 100.0, p.y as f32 / 100.0),
                };
                self.mouse.wheel_x -= x;
                self.mouse.wheel_y -= y;
            }
            WindowEvent::KeyboardInput { event, .. } => {
                if let Some(sp) = &mut self.splash {
                    sp.skip |= event.state == ElementState::Pressed;
                }
                // the consent question takes the keys: 1-4, Esc (holding Esc still quits)
                if let PhysicalKey::Code(code) = event.physical_key {
                    if event.state == ElementState::Released && self.swallowed.remove(&code) {
                        if code == KeyCode::Escape {
                            self.esc_down = None;
                        }
                        return;
                    }
                    if let (Some(q), ElementState::Pressed) = (self.asking.clone(), event.state) {
                        use crate::consent::Answer;
                        let answer = match code {
                            KeyCode::Digit1 | KeyCode::Numpad1 => Some(Answer::ThisTime),
                            KeyCode::Digit2 | KeyCode::Numpad2 => Some(Answer::Always),
                            KeyCode::Digit3 | KeyCode::Numpad3 | KeyCode::Escape => Some(Answer::No),
                            KeyCode::Digit4 | KeyCode::Numpad4 => Some(Answer::Never),
                            _ => None,
                        };
                        if code == KeyCode::Escape && !event.repeat {
                            self.esc_down = Some(Instant::now());
                        }
                        self.swallowed.insert(code);
                        if let (Some(a), Some(c)) = (answer, &self.consent) {
                            if !event.repeat {
                                c.lock().unwrap().answer(&q, a);
                            }
                        }
                        return;
                    }
                }
                if let PhysicalKey::Code(code) = event.physical_key {
                    if code == KeyCode::Escape && !event.repeat {
                        self.esc_down = (event.state == ElementState::Pressed).then(Instant::now);
                    }
                    // the copy key also goes to the game
                    if Some(code) == self.opts.copy_key && event.state == ElementState::Pressed && !event.repeat {
                        self.copy_pending = true;
                    }
                    // the paste key (Ctrl+V, Cmd+V): the game may read the clipboard's text in the next frame
                    let modifier = [KeyCode::ControlLeft, KeyCode::ControlRight, KeyCode::SuperLeft, KeyCode::SuperRight].iter().any(|k| self.keys.contains(k));
                    if code == KeyCode::KeyV && modifier && event.state == ElementState::Pressed {
                        self.paste = open_clipboard(&mut self.clipboard)
                            .and_then(|c| c.get_text().ok())
                            .filter(|t| t.len() <= host::CLIPBOARD_MAX);
                    }
                    let k = keymap::gasm_key(code);
                    if !event.repeat && k != 0 {
                        self.key_events.push((k, event.state == ElementState::Pressed));
                    }
                    match event.state {
                        ElementState::Pressed => self.keys.insert(code),
                        ElementState::Released => self.keys.remove(&code),
                    };
                }
                if event.state == ElementState::Pressed {
                    match &event.logical_key {
                        Key::Named(NamedKey::Enter) => self.typed.push('\n'),
                        Key::Named(NamedKey::Backspace) => self.typed.push('\u{8}'),
                        _ => {
                            if let Some(t) = &event.text {
                                self.typed.extend(t.chars().filter(|c| !c.is_control()));
                            }
                        }
                    }
                }
            }
            WindowEvent::RedrawRequested => self.tick(el),
            _ => {}
        }
    }

    fn device_event(&mut self, _el: &ActiveEventLoop, _id: DeviceId, event: DeviceEvent) {
        // raw (unaccelerated) motion, also while the cursor is locked
        if let DeviceEvent::MouseMotion { delta } = event
            && self.window.as_ref().is_some_and(|w| w.has_focus())
        {
            self.mouse.dx += delta.0 as f32;
            self.mouse.dy += delta.1 as f32;
        }
    }

    fn about_to_wait(&mut self, el: &ActiveEventLoop) {
        self.tick(el);
    }
}


#[cfg(test)]
mod tests {
    /// Needs a desktop session (CI has no clipboard): `cargo test -- --ignored copy_frame`
    #[test]
    #[ignore]
    fn copy_frame_puts_the_image_on_the_clipboard() {
        let rgba: Vec<u8> = (0..4 * 3 * 4).map(|i| (i * 7) as u8 | 3).collect();
        let mut clipboard = None;
        super::copy_frame(&mut clipboard, 4, 3, rgba.clone());
        let img = clipboard.as_mut().unwrap().get_image().unwrap();
        assert_eq!((img.width, img.height), (4, 3));
        let opaque: Vec<u8> = rgba.chunks(4).flat_map(|p| [p[0], p[1], p[2], 255]).collect();
        assert_eq!(img.bytes.into_owned(), opaque);
    }
}
