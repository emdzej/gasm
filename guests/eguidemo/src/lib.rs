//! egui's demo windows on gasm: egui_glow (unchanged) paints through glow, which
//! runs on gasm:gl via the gasm fork of glow (sdk/glow) and gasm::gles. gasm's
//! pointer, wheel, keys and text become egui input; time is the frame's.

use std::sync::Arc;

use gasm::input;
use gasm::sys::keys;

struct Demo {
    gl: Arc<glow::Context>,
    ctx: egui::Context,
    painter: egui_glow::Painter,
    demo: egui_demo_lib::DemoWindows,
    buttons: u32,
}

/// gasm key codes (W3C names) to egui keys.
fn egui_key(code: u32) -> Option<egui::Key> {
    let name = keys::NAMES.get(code as usize)?;
    use egui::Key::*;
    Some(match *name {
        "Backspace" => Backspace,
        "Delete" => Delete,
        "Enter" | "NumpadEnter" => Enter,
        "Tab" => Tab,
        "Escape" => Escape,
        "Space" => Space,
        "ArrowLeft" => ArrowLeft,
        "ArrowRight" => ArrowRight,
        "ArrowUp" => ArrowUp,
        "ArrowDown" => ArrowDown,
        "Home" => Home,
        "End" => End,
        "PageUp" => PageUp,
        "PageDown" => PageDown,
        n if n.starts_with("Key") && n.len() == 4 => egui::Key::from_name(&n[3..])?,
        n if n.starts_with("Digit") && n.len() == 6 => egui::Key::from_name(&n[5..])?,
        _ => return None,
    })
}

impl gasm::Game for Demo {
    fn init() -> Result<Self, String> {
        let gl = Arc::new(unsafe { glow::Context::from_loader_function_cstr(gasm::gles::get_proc_address) });
        let painter = egui_glow::Painter::new(gl.clone(), "", None, false).map_err(|e| e.to_string())?;
        // the game reads the keyboard itself: no keymap pads
        input::set_mode(input::KEYS_RAW);
        Ok(Demo { gl, ctx: egui::Context::default(), painter, demo: egui_demo_lib::DemoWindows::default(), buttons: 0 })
    }

    fn frame(&mut self) {
        let (w, h) = unsafe { (gasm::sys::gl_width(), gasm::sys::gl_height()) };
        // drawable pixels per egui point: gasm has no display scale, so about 720 points
        // tall (2 on a Retina window of the default size), in quarter steps
        let ppp = ((h as f32 / 720.0) * 4.0).round().max(4.0) / 4.0;
        if self.ctx.pixels_per_point() != ppp {
            self.ctx.set_pixels_per_point(ppp);
        }
        let mut events = Vec::new();
        let mut modifiers = egui::Modifiers::default();
        if let Some(k) = input::keys() {
            modifiers.shift = k.held(keys::SHIFT_LEFT) || k.held(keys::SHIFT_RIGHT);
            modifiers.ctrl = k.held(keys::CONTROL_LEFT) || k.held(keys::CONTROL_RIGHT);
            modifiers.alt = k.held(keys::ALT_LEFT) || k.held(keys::ALT_RIGHT);
            modifiers.mac_cmd = k.held(keys::META_LEFT) || k.held(keys::META_RIGHT);
            modifiers.command = modifiers.ctrl || modifiers.mac_cmd;
        }
        for e in input::key_events().unwrap_or_default() {
            if let Some(key) = egui_key(e.code) {
                events.push(egui::Event::Key { key, physical_key: None, pressed: e.down, repeat: false, modifiers });
            }
        }
        if let Some(t) = gasm::text_input().filter(|t| !t.is_empty()) {
            let t: String = t.chars().filter(|c| !c.is_control()).collect();
            if !t.is_empty() {
                events.push(egui::Event::Text(t));
            }
        }
        if let Some(p) = input::pointer() {
            let pos = egui::pos2(p.x / ppp, p.y / ppp);
            if p.inside() {
                events.push(egui::Event::PointerMoved(pos));
            } else {
                events.push(egui::Event::PointerGone);
            }
            for (bit, button) in [(input::MOUSE_LEFT, egui::PointerButton::Primary), (input::MOUSE_RIGHT, egui::PointerButton::Secondary), (input::MOUSE_MIDDLE, egui::PointerButton::Middle)] {
                if (p.buttons ^ self.buttons) & bit != 0 || p.pressed & bit != 0 || p.released & bit != 0 {
                    let pressed = p.buttons & bit != 0;
                    events.push(egui::Event::PointerButton { pos, button, pressed, modifiers });
                }
            }
            self.buttons = p.buttons;
            if p.wheel_x != 0.0 || p.wheel_y != 0.0 {
                events.push(egui::Event::MouseWheel {
                    unit: egui::MouseWheelUnit::Line,
                    delta: egui::vec2(-p.wheel_x, -p.wheel_y),
                    modifiers,
                    phase: egui::TouchPhase::Move,
                });
            }
        }
        let raw = egui::RawInput {
            screen_rect: Some(egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(w as f32 / ppp, h as f32 / ppp))),
            time: Some(gasm::time_ms() / 1000.0),
            events,
            ..Default::default()
        };
        let mut out = self.ctx.run_ui(raw, |ui| self.demo.ui(ui));
        let prims = self.ctx.tessellate(out.shapes, out.pixels_per_point);
        unsafe {
            use glow::HasContext;
            self.gl.clear_color(0.1, 0.1, 0.12, 1.0);
            self.gl.clear(glow::COLOR_BUFFER_BIT);
        }
        self.painter.paint_and_update_textures([w, h], out.pixels_per_point, &prims, &mut out.textures_delta);
    }
}

gasm::title!("egui demo");
gasm::game!(Demo);
