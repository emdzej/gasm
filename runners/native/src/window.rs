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
use crate::gfx::Gfx;
use crate::host::{self, Game, Gamepad, KEY_STATE_BYTES, Pointer, RawInput, Stop};
use crate::keymap;
use crate::present::Present;
use crate::session::Session;

pub struct Options {
    /// initial window size in logical pixels
    pub size: (u32, u32),
    pub keymap: keymap::Keymap,
    pub mute: bool,
    /// how 2D frames are shown
    pub present: Present,
}

/// Open a window and play until the guest exits, traps or the player quits.
/// Returns the guest's exit code (0 when the player quit).
pub fn run(session: Session, opts: Options) -> Result<i32, String> {
    let event_loop = EventLoop::new().map_err(|e| e.to_string())?;
    // default title: the module file name without .wasm (set_title replaces it)
    let default_title = host::static_title(&session.wasm).unwrap_or_else(|| {
        std::path::Path::new(&session.name).file_stem().map_or(session.name.clone(), |s| s.to_string_lossy().into_owned())
    });
    let mut app = App {
        title: default_title.clone(),
        default_title,
        fps: 0,
        opts,
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
        audio_stream: None,
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

    fn tick(&mut self, el: &ActiveEventLoop) {
        let Some(game) = &mut self.game else { return };
        let period = Duration::from_secs_f64(1.0 / game.with_host(|h| h.frame_rate));
        let now = Instant::now();
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
            let drawable = game.with_host(|h| h.gfx.size());
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
                } else {
                    host.input.key_events.clear();
                    host.input.pointer = Some(pointer);
                }
                host.show_frame = k + 1 == steps;
                host.gfx.used = false;
                });
                match game.frame() {
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
        let gfx = match Gfx::for_window(window.clone()) {
            Ok(mut g) => {
                g.present = self.opts.present;
                g
            }
            Err(e) => return self.stop(el, Err(format!("cannot initialise GPU: {e}"))),
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
        let Some(session) = self.session.take() else { return };
        match session.start(audio, gfx, false, false) {
            Ok(g) => self.game = Some(g),
            Err(Stop::Exit(code)) => return self.stop(el, Ok(code)),
            Err(Stop::Trap(e)) => return self.stop(el, Err(e)),
        }
        self.window = Some(window);
        self.next = Instant::now();
        el.set_control_flow(ControlFlow::WaitUntil(self.next));
    }

    fn window_event(&mut self, el: &ActiveEventLoop, _id: WindowId, event: WindowEvent) {
        match event {
            WindowEvent::CloseRequested => self.quit(el),
            WindowEvent::Resized(size) => {
                if let Some(g) = &mut self.game {
                    g.with_host(|h| h.gfx.resize(size.width, size.height));
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
                if let PhysicalKey::Code(code) = event.physical_key {
                    if code == KeyCode::Escape && !event.repeat {
                        self.esc_down = (event.state == ElementState::Pressed).then(Instant::now);
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

