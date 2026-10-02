//! Scripted input for headless runs (`--input`), mirrored by runners/web/headless.mjs.
//!
//! Items are `FRAMES:ACTION`, comma-separated (commas inside quotes or parentheses
//! don't split). FRAMES is `N` or `FROM-TO` (inclusive). Actions:
//!
//! - `A+B+START` virtual pad 1 buttons
//! - `"text"` typed text on frame N (escapes `\n` enter, `\b` backspace, `\\`, `\"`)
//! - `KEY(ShiftLeft+ArrowLeft)` raw keys held (W3C names); press/release events
//!   come from the changes between frames
//! - `PTR(x,y)` / `PTR(x,y,L+R)` pointer position in drawable pixels and buttons
//!   (`L R M BACK FWD`); the position stays where the last item put it
//! - `MOVE(dx,dy)` relative motion, `WHEEL(x,y)` wheel, per frame
//! - `GP0(B0+B9+A1=0.5)` gamepad slot 0-3: buttons by index, axes `An=value`;
//!   slots that appear anywhere in the script are connected (standard mapping)
//!
//! Numbers are plain decimals (no hex, inf or nan). All arithmetic is done in f64
//! and rounded to f32 once, exactly like the JS runner (input-script.mjs), which
//! accepts and rejects the same scripts.

use crate::host::{Gamepad, KEY_STATE_BYTES, Pointer, RawInput};
use crate::keys::KEY_CODES;

const PAD_NAMES: [&str; 12] = ["A", "B", "X", "Y", "L", "R", "SELECT", "START", "UP", "DOWN", "LEFT", "RIGHT"];
const MOUSE_NAMES: [&str; 5] = ["L", "R", "M", "BACK", "FWD"];

/// (from, to, slot, buttons [(index, value)], axes [(index, value)])
type GamepadItem = (u64, u64, usize, Vec<(usize, f64)>, Vec<(usize, f64)>);

#[derive(Default, Clone)]
pub struct Script {
    pads: Vec<(u64, u64, u32)>,
    text: Vec<(u64, String)>,
    keys: Vec<(u64, u64, Vec<u16>)>,
    ptr: Vec<(u64, u64, f64, f64, u32)>,
    moves: Vec<(u64, u64, f64, f64)>,
    wheel: Vec<(u64, u64, f64, f64)>,
    gp: Vec<GamepadItem>,
}

/// Carried between frames: previous keys, pointer position and buttons.
#[derive(Default)]
pub struct ScriptState {
    keys: [u8; KEY_STATE_BYTES],
    x: f64,
    y: f64,
    buttons: u32,
}

fn split_items(spec: &str) -> Vec<String> {
    let (mut items, mut cur, mut quoted, mut escaped, mut depth) = (Vec::new(), String::new(), false, false, 0);
    for ch in spec.chars() {
        if escaped {
            escaped = false;
        } else if ch == '\\' && quoted {
            escaped = true;
        } else if ch == '"' {
            quoted = !quoted;
        } else if !quoted && ch == '(' {
            depth += 1;
        } else if !quoted && ch == ')' {
            depth -= 1;
        } else if ch == ',' && !quoted && depth == 0 {
            items.push(std::mem::take(&mut cur));
            continue;
        }
        cur.push(ch);
    }
    items.push(cur);
    items
}

/// A decimal number: optional sign, digits, optional fraction and exponent.
fn decimal(v: &str) -> Option<f64> {
    let digits = v.strip_prefix(['-', '+']).unwrap_or(v);
    let ok = !digits.is_empty()
        && digits.starts_with(|c: char| c.is_ascii_digit() || c == '.')
        && digits.chars().all(|c| c.is_ascii_digit() || matches!(c, '.' | 'e' | 'E' | '-' | '+'));
    v.parse::<f64>().ok().filter(|x| ok && x.is_finite())
}

fn args<'a>(rest: &'a str, name: &str) -> Option<&'a str> {
    rest.strip_prefix(name)?.strip_prefix('(')?.strip_suffix(')')
}

impl Script {
    pub fn parse(spec: &str) -> Result<Script, String> {
        let mut s = Script::default();
        for item in split_items(spec) {
            let bad = || format!("bad --input item {item:?}");
            let (range, rest) = item.split_once(':').ok_or_else(bad)?;
            let (from, to) = range.split_once('-').unwrap_or((range, range));
            let (from, to): (u64, u64) = (from.parse().map_err(|_| bad())?, to.parse().map_err(|_| bad())?);
            let num = |v: &str| decimal(v.trim()).ok_or_else(bad);
            let pair = |a: &str| -> Result<(f64, f64), String> {
                let (x, y) = a.split_once(',').ok_or_else(bad)?;
                Ok((num(x)?, num(y)?))
            };
            if let Some(q) = rest.strip_prefix('"') {
                let body = q.strip_suffix('"').ok_or_else(bad)?;
                let mut t = String::new();
                let mut chars = body.chars();
                while let Some(c) = chars.next() {
                    t.push(if c != '\\' {
                        c
                    } else {
                        match chars.next().ok_or_else(bad)? {
                            'n' => '\n',
                            'b' => '\u{8}',
                            other => other,
                        }
                    });
                }
                s.text.push((from, t));
            } else if let Some(a) = args(rest, "KEY") {
                let mut codes = Vec::new();
                for k in a.split('+') {
                    let c = KEY_CODES.iter().position(|n| !n.is_empty() && *n == k).ok_or_else(|| format!("{}: unknown key {k:?}", bad()))?;
                    codes.push(c as u16);
                }
                s.keys.push((from, to, codes));
            } else if let Some(a) = args(rest, "PTR") {
                let parts: Vec<&str> = a.splitn(3, ',').collect();
                if parts.len() < 2 {
                    return Err(bad());
                }
                let mut buttons = 0;
                if let Some(b) = parts.get(2) {
                    for n in b.split('+') {
                        buttons |= 1 << MOUSE_NAMES.iter().position(|m| m.eq_ignore_ascii_case(n.trim())).ok_or_else(bad)?;
                    }
                }
                s.ptr.push((from, to, num(parts[0])?, num(parts[1])?, buttons));
            } else if let Some(a) = args(rest, "MOVE") {
                let (x, y) = pair(a)?;
                s.moves.push((from, to, x, y));
            } else if let Some(a) = args(rest, "WHEEL") {
                let (x, y) = pair(a)?;
                s.wheel.push((from, to, x, y));
            } else if rest.starts_with("GP") && rest.len() > 3 {
                let slot: usize = rest.get(2..3).and_then(|d| d.parse().ok()).ok_or_else(bad)?;
                let a = args(&rest[3..], "").ok_or_else(bad)?;
                if slot > 3 {
                    return Err(bad());
                }
                let (mut buttons, mut axes) = (Vec::new(), Vec::new());
                for part in a.split('+').filter(|p| !p.is_empty()) {
                    if let Some(i) = part.strip_prefix('B') {
                        buttons.push((i.parse().map_err(|_| bad())?, 1.0));
                    } else if let Some(ax) = part.strip_prefix('A') {
                        let (i, v) = ax.split_once('=').ok_or_else(bad)?;
                        axes.push((i.parse().map_err(|_| bad())?, num(v)?));
                    } else {
                        return Err(bad());
                    }
                }
                s.gp.push((from, to, slot, buttons, axes));
            } else {
                let mut mask = 0;
                for b in rest.split('+') {
                    mask |= 1 << PAD_NAMES.iter().position(|n| n.eq_ignore_ascii_case(b)).ok_or_else(bad)?;
                }
                s.pads.push((from, to, mask));
            }
        }
        Ok(s)
    }

    pub fn pad(&self, frame: u64) -> u32 {
        self.pads.iter().filter(|(f, t, _)| (*f..=*t).contains(&frame)).fold(0, |m, (_, _, b)| m | b)
    }

    pub fn text(&self, frame: u64) -> String {
        self.text.iter().filter(|(f, _)| *f == frame).map(|(_, t)| t.as_str()).collect()
    }

    /// Raw input for `frame`. `drawable`: the headless drawable size; `mode`: input_mode flags.
    pub fn raw(&self, frame: u64, st: &mut ScriptState, drawable: (f32, f32), mode: u32) -> RawInput {
        let on = |f: u64, t: u64| (f..=t).contains(&frame);
        let mut keys = [0u8; KEY_STATE_BYTES];
        for (f, t, codes) in &self.keys {
            if on(*f, *t) {
                for &c in codes {
                    keys[c as usize / 8] |= 1 << (c % 8);
                }
            }
        }
        // releases first, then presses, each in code order
        let mut key_events = Vec::new();
        for down in [false, true] {
            for c in 1..KEY_CODES.len() as u16 {
                let (was, is) = (st.keys[c as usize / 8] >> (c % 8) & 1 == 1, keys[c as usize / 8] >> (c % 8) & 1 == 1);
                if was != is && is == down {
                    key_events.push((c, down));
                }
            }
        }
        st.keys = keys;

        let (px, py) = (st.x, st.y);
        let mut buttons = 0;
        for (f, t, x, y, b) in &self.ptr {
            if on(*f, *t) {
                st.x = *x;
                st.y = *y;
                buttons = *b;
            }
        }
        let (mut dx, mut dy) = (st.x - px, st.y - py);
        for (f, t, x, y) in &self.moves {
            if on(*f, *t) {
                dx += x;
                dy += y;
            }
        }
        let (mut wx, mut wy) = (0.0, 0.0);
        for (f, t, x, y) in &self.wheel {
            if on(*f, *t) {
                wx += x;
                wy += y;
            }
        }
        let inside = st.x >= 0.0 && st.y >= 0.0 && st.x < drawable.0 as f64 && st.y < drawable.1 as f64;
        let pointer = Pointer {
            x: st.x as f32,
            y: st.y as f32,
            dx: dx as f32,
            dy: dy as f32,
            wheel_x: wx as f32,
            wheel_y: wy as f32,
            buttons,
            pressed: buttons & !st.buttons,
            released: st.buttons & !buttons,
            flags: (inside as u32) | (mode & 6),
            drawable,
            integer_scale: false,
        };
        st.buttons = buttons;

        let mut gamepads: [Gamepad; 4] = Default::default();
        for (slot, g) in gamepads.iter_mut().enumerate() {
            if self.gp.iter().any(|e| e.2 == slot) {
                *g = Gamepad { connected: true, standard: true, buttons: vec![0.0; 17], axes: vec![0.0; 4], name: "scripted".into() };
            }
        }
        for (f, t, slot, bs, axs) in &self.gp {
            if on(*f, *t) {
                let g = &mut gamepads[*slot];
                for &(i, v) in bs {
                    if i < 32 {
                        g.buttons.resize(g.buttons.len().max(i + 1), 0.0);
                        g.buttons[i] = v as f32;
                    }
                }
                for &(i, v) in axs {
                    if i < 16 {
                        g.axes.resize(g.axes.len().max(i + 1), 0.0);
                        g.axes[i] = v as f32;
                    }
                }
            }
        }
        RawInput { keys: Some(keys), key_events, pointer: Some(pointer), gamepads: Some(gamepads) }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_all_actions_and_derives_events() {
        let s = Script::parse(r#"0-1:A+UP,2:"x,y",3-4:KEY(ShiftLeft+ArrowLeft),5:PTR(10,20,L),6:PTR(15,20),6:WHEEL(0,1),7-8:GP1(B0+A1=0.5),9:MOVE(3,-4)"#).unwrap();
        assert_eq!(s.pad(1), 1 | 1 << 8);
        assert_eq!(s.text(2), "x,y");
        let mut st = ScriptState::default();
        let d = (1280.0, 720.0);
        let mut frames: Vec<RawInput> = (0..10).map(|i| s.raw(i, &mut st, d, 0)).collect();
        let shift = KEY_CODES.iter().position(|n| *n == "ShiftLeft").unwrap() as u16;
        assert!(frames[3].key_events.contains(&(shift, true)));
        assert!(frames[5].key_events.contains(&(shift, false)));
        let p5 = frames[5].pointer.unwrap();
        assert_eq!((p5.x, p5.y, p5.buttons, p5.pressed), (10.0, 20.0, 1, 1));
        let p6 = frames[6].pointer.unwrap();
        assert_eq!((p6.dx, p6.released, p6.wheel_y), (5.0, 1, 1.0));
        let g = frames.remove(7).gamepads.unwrap();
        assert!(g[1].connected && !g[0].connected);
        assert_eq!((g[1].buttons[0], g[1].axes[1]), (1.0, 0.5));
        assert_eq!(frames[8].pointer.unwrap().dx, 3.0); // frames shifted by remove(7): index 8 = frame 9
        assert!(Script::parse("1:KEY(NoSuchKey)").is_err());
        // rejected by both runners (input-script.mjs has the same cases)
        for bad in ["1-2-3:A", "1:PTR(0x10,1)", "1:MOVE(inf,0)", "1:MOVE(nan,0)", "1:\"a\\\"", "1:GP0(A1=0.5=3)", "1:GPé(B0)", "x:A"] {
            assert!(Script::parse(bad).is_err(), "{bad}");
        }
        assert!(Script::parse("1:MOVE(-1.5e1,+2)").is_ok());
    }
}
