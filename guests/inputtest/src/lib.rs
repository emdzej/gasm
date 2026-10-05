//! Input tester: shows every raw input the runner gives (and nothing it doesn't).
//!
//! - top: one cell per key (`gasm::keys`), lit while held, outlined on press/release
//! - middle: the pointer as a crosshair at its frame position, buttons, wheel and
//!   relative-motion meters (a click also saves a storage key; the bar counts them)
//! - bottom: four gamepad slots (buttons and axes), and the virtual pads
//!
//! It reads the keyboard itself (`KEYS_RAW`), so the keymap doesn't turn keys into
//! pads; gamepads still do. Everything shown goes into the video hash.
//!
//! Clipboard (`gasm:clipboard`): pasted text is logged ("pasted ..."), and C copies
//! "copied by inputtest" (scripts/player-test.mjs). Files (`gasm:files`): S saves
//! inputtest.txt for the player and logs the save's state ("save: ...").

use gasm::input::{self, Pointer};
use gasm::keys;

const W: usize = 320;
const H: usize = 240;

struct InputTest {
    fb: Vec<u8>,
    wheel: (f32, f32),
    motion: (f32, f32),
    clicks: u32,
    flash: Vec<u8>, // per key: frames left to outline after an event
    saves: u32,
    /// the last save, until it's done
    save: Option<gasm::files::Save>,
}

fn rect(fb: &mut [u8], x: i32, y: i32, w: i32, h: i32, c: [u8; 3]) {
    for yy in y.max(0)..(y + h).min(H as i32) {
        for xx in x.max(0)..(x + w).min(W as i32) {
            let i = (yy as usize * W + xx as usize) * 4;
            fb[i..i + 4].copy_from_slice(&[c[0], c[1], c[2], 255]);
        }
    }
}

fn frame_rect(fb: &mut [u8], x: i32, y: i32, w: i32, h: i32, c: [u8; 3]) {
    rect(fb, x, y, w, 1, c);
    rect(fb, x, y + h - 1, w, 1, c);
    rect(fb, x, y, 1, h, c);
    rect(fb, x + w - 1, y, 1, h, c);
}

/// A horizontal meter centred at x: value -1..1 over half-width `half`.
fn meter(fb: &mut [u8], x: i32, y: i32, half: i32, v: f32, c: [u8; 3]) {
    rect(fb, x - half, y, half * 2, 4, [40, 40, 50]);
    let w = (v.clamp(-1.0, 1.0) * half as f32) as i32;
    if w >= 0 { rect(fb, x, y, w.max(1), 4, c) } else { rect(fb, x + w, y, -w, 4, c) }
}

impl gasm::Game for InputTest {
    fn init() -> Result<Self, String> {
        gasm::set_frame_rate(60.0);
        input::set_mode(input::KEYS_RAW | input::POINTER_HIDDEN);
        // ?aspect=16:9: show the frame at that display aspect (the pointer's frame position follows)
        if let Some(a) = gasm::param("aspect") {
            let (n, d) = a.split_once(':').ok_or("aspect: expected num:den")?;
            let ratio = (n.parse().map_err(|_| "aspect: bad num")?, d.parse().map_err(|_| "aspect: bad den")?);
            gasm::log!("aspect {}:{} shown by the runner: {}", ratio.0, ratio.1, gasm::video_set_aspect(ratio.0, ratio.1));
        }
        Ok(InputTest { fb: vec![0; W * H * 4], wheel: (0.0, 0.0), motion: (0.0, 0.0), clicks: 0, flash: vec![0; keys::NAMES.len()], saves: 0, save: None })
    }

    fn frame(&mut self) {
        let fb = &mut self.fb;
        rect(fb, 0, 0, W as i32, H as i32, [16, 18, 24]);

        // keyboard: 121 keys in rows of 16
        let held = input::keys();
        for e in input::key_events().unwrap_or_default() {
            gasm::log!("key {} {}", keys::NAMES[e.code as usize], if e.down { "down" } else { "up" });
            self.flash[e.code as usize] = 8;
            if e.down && keys::NAMES[e.code as usize] == "KeyC" && gasm::clipboard::available() {
                gasm::clipboard::set_text("copied by inputtest");
            }
            if e.down && keys::NAMES[e.code as usize] == "KeyS" && gasm::files::available() {
                self.saves += 1;
                self.save = gasm::files::save("inputtest.txt", "text/plain", format!("inputtest save {}\n", self.saves).as_bytes());
                gasm::log!("save: {}", if self.save.is_some() { "queued" } else { "refused" });
            }
        }
        if let Some(s) = self.save {
            match s.state() {
                gasm::files::State::Pending => {}
                done => {
                    gasm::log!("save: {}", if done == gasm::files::State::Saved { "saved" } else { "failed" });
                    self.save = None;
                }
            }
        }
        if let Some(t) = gasm::clipboard::available().then(gasm::clipboard::pasted).flatten() {
            gasm::log!("pasted {t:?}");
        }
        for code in 1..keys::NAMES.len() as u32 {
            let (cx, cy) = ((code - 1) % 16, (code - 1) / 16);
            let (x, y) = (8 + cx as i32 * 19, 8 + cy as i32 * 11);
            let on = held.is_some_and(|k| k.held(code));
            rect(fb, x, y, 17, 9, if on { [250, 200, 60] } else if held.is_some() { [50, 54, 66] } else { [30, 20, 20] });
            let f = &mut self.flash[code as usize];
            if *f > 0 {
                frame_rect(fb, x - 1, y - 1, 19, 11, [255, 255, 255]);
                *f -= 1;
            }
        }

        // pointer
        let p: Option<Pointer> = input::pointer();
        if let Some(p) = p {
            self.wheel.0 += p.wheel_x;
            self.wheel.1 += p.wheel_y;
            self.motion.0 += p.dx;
            self.motion.1 += p.dy;
            if p.pressed != 0 {
                self.clicks += 1;
                gasm::log!("click {:#x} at {:.1},{:.1} (frame px)", p.pressed, p.frame_x, p.frame_y);
                gasm::storage::set(&format!("click-{:04}", self.clicks), &p.frame_x.to_le_bytes());
            }
            for b in 0..5 {
                let c = if p.buttons & (1 << b) != 0 { [90, 220, 120] } else { [50, 54, 66] };
                rect(fb, 8 + b * 14, 104, 12, 8, c);
            }
            meter(fb, 130, 104, 40, (self.wheel.1 / 10.0).sin(), [120, 160, 250]);
            meter(fb, 130, 110, 40, (self.motion.0 / 200.0).sin(), [250, 120, 160]);
            meter(fb, 230, 104, 40, (self.motion.1 / 200.0).sin(), [250, 120, 160]);
            let flags = [p.inside(), p.flags & 2 != 0, p.locked()];
            for (i, on) in flags.into_iter().enumerate() {
                rect(fb, 280 + i as i32 * 10, 104, 8, 8, if on { [200, 200, 255] } else { [50, 54, 66] });
            }
            let (x, y) = (p.frame_x as i32, p.frame_y as i32);
            rect(fb, x - 6, y, 13, 1, [255, 255, 255]);
            rect(fb, x, y - 6, 1, 13, [255, 255, 255]);
        }
        // saved clicks (storage enumeration)
        let saved = gasm::storage::keys().iter().filter(|k| k.starts_with("click-")).count() as i32;
        rect(fb, 8, 118, (saved * 4).min(300), 4, [180, 140, 255]);

        // gamepads: one row per slot
        for slot in 0..4u32 {
            let y = 130 + slot as i32 * 24;
            match input::gamepad(slot) {
                Some(g) if g.connected => {
                    rect(fb, 8, y, 8, 8, if g.standard { [90, 220, 120] } else { [250, 200, 60] });
                    for (i, v) in g.buttons.iter().take(20).enumerate() {
                        let l = (60.0 + v * 190.0) as u8;
                        rect(fb, 22 + i as i32 * 9, y, 7, 8, [l, l, 60]);
                    }
                    for (i, v) in g.axes.iter().take(6).enumerate() {
                        meter(fb, 50 + i as i32 * 50, y + 12, 22, *v, [120, 200, 250]);
                    }
                }
                Some(_) => rect(fb, 8, y, 8, 8, [50, 54, 66]),
                None => rect(fb, 8, y, 8, 8, [30, 20, 20]),
            }
        }
        // virtual pads (gamepads only: the keyboard is raw)
        for player in 0..4 {
            let pad = gasm::pad(player);
            for b in 0..12 {
                let c = if pad.held(1 << b) { [90, 220, 120] } else { [40, 44, 54] };
                rect(fb, 200 + b * 9, 130 + player as i32 * 24, 7, 7, c);
            }
        }
        gasm::present(&self.fb, W as u32, H as u32, (W * 4) as u32);
    }
}

gasm::game!(InputTest);
