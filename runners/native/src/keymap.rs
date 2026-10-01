//! Keyboard layouts: the same text format as the web player (`gasm-host.js`
//! `parseKeymap`). A line per binding: `<pad 1-4> <button> <key code>...`, where key
//! codes are W3C `KeyboardEvent.code` names (winit's `KeyCode` uses the same names).
//! Keyboard bindings for pad N >= 2 apply while fewer than N gamepads are connected.

use winit::keyboard::KeyCode;

pub const DEFAULT_KEYMAP: &str = include_str!("default-keymap.txt");
pub const BUTTONS: [&str; 12] = ["a", "b", "x", "y", "l", "r", "select", "start", "up", "down", "left", "right"];

/// (key, zero-based pad, button bit)
pub type Keymap = Vec<(KeyCode, usize, u32)>;

pub fn parse(text: &str) -> Result<Keymap, String> {
    let mut map = Vec::new();
    let mut errors = Vec::new();
    for (i, raw) in text.lines().enumerate() {
        let line = raw.split('#').next().unwrap_or("").trim();
        if line.is_empty() {
            continue;
        }
        let mut parts = line.split_whitespace();
        let (pad, button) = (parts.next(), parts.next());
        let keys: Vec<&str> = parts.collect();
        let pad = pad.and_then(|p| p.parse::<usize>().ok()).filter(|p| (1..=4).contains(p));
        let bit = button.and_then(|b| BUTTONS.iter().position(|n| n.eq_ignore_ascii_case(b)));
        let (Some(pad), Some(bit), false) = (pad, bit, keys.is_empty()) else {
            errors.push(format!("line {}: expected \"<pad 1-4> <button> <key>...\", got {line:?}", i + 1));
            continue;
        };
        for k in keys {
            match key_from_code(normalize(k)) {
                Some(KeyCode::Escape) => errors.push(format!("line {}: Escape is reserved (quit)", i + 1)),
                Some(code) => map.push((code, pad - 1, 1u32 << bit)),
                None => errors.push(format!("line {}: unknown key code {k:?}", i + 1)),
            }
        }
    }
    if errors.is_empty() { Ok(map) } else { Err(errors.join("\n")) }
}

/// Pads from held keys; keyboard pad N (zero-based n >= 1) only while `gamepads <= n`.
pub fn pads(map: &Keymap, held: &std::collections::HashSet<KeyCode>, gamepads: usize) -> [u32; 4] {
    let mut pads = [0u32; 4];
    for (key, pad, bit) in map {
        if held.contains(key) && (*pad == 0 || gamepads <= *pad) {
            pads[*pad] |= bit;
        }
    }
    pads
}

macro_rules! codes {
    ($($name:ident),* $(,)?) => {
        /// W3C KeyboardEvent.code name -> winit KeyCode.
        pub fn key_from_code(s: &str) -> Option<KeyCode> {
            match s { $(stringify!($name) => Some(KeyCode::$name),)* _ => None }
        }
    };
}

codes!(
    KeyA, KeyB, KeyC, KeyD, KeyE, KeyF, KeyG, KeyH, KeyI, KeyJ, KeyK, KeyL, KeyM,
    KeyN, KeyO, KeyP, KeyQ, KeyR, KeyS, KeyT, KeyU, KeyV, KeyW, KeyX, KeyY, KeyZ,
    Digit0, Digit1, Digit2, Digit3, Digit4, Digit5, Digit6, Digit7, Digit8, Digit9,
    ArrowUp, ArrowDown, ArrowLeft, ArrowRight,
    Numpad0, Numpad1, Numpad2, Numpad3, Numpad4, Numpad5, Numpad6, Numpad7, Numpad8, Numpad9,
    NumpadAdd, NumpadSubtract, NumpadMultiply, NumpadDivide, NumpadDecimal, NumpadEnter, NumpadEqual, NumpadComma,
    F1, F2, F3, F4, F5, F6, F7, F8, F9, F10, F11, F12,
    Space, Enter, Tab, Backspace, Escape, CapsLock,
    ShiftLeft, ShiftRight, ControlLeft, ControlRight, AltLeft, AltRight, SuperLeft, SuperRight,
    Comma, Period, Slash, Semicolon, Quote, BracketLeft, BracketRight, Backslash, Minus, Equal, Backquote, IntlBackslash,
    Insert, Delete, Home, End, PageUp, PageDown,
    F13, F14, F15, F16, F17, F18, F19, F20, F21, F22, F23, F24,
    PrintScreen, ScrollLock, Pause, NumLock, ContextMenu, IntlRo, IntlYen,
);

/// gasm raw key codes (GASM_KEY_*): index = code, W3C KeyboardEvent.code names.
/// Must equal spec/abi.json (checked by scripts/gen-abi.mjs --check).
pub const KEY_CODES: [&str; 122] = [
    "", "Escape", "F1", "F2", "F3", "F4", "F5", "F6", "F7", "F8", "F9", "F10", "F11", "F12", "Backquote", "Digit0", "Digit1", "Digit2", "Digit3", "Digit4", "Digit5", "Digit6", "Digit7", "Digit8", "Digit9", "Minus", "Equal", "Backspace", "Tab", "KeyA", "KeyB", "KeyC", "KeyD", "KeyE", "KeyF", "KeyG", "KeyH", "KeyI", "KeyJ", "KeyK", "KeyL", "KeyM", "KeyN", "KeyO", "KeyP", "KeyQ", "KeyR", "KeyS", "KeyT", "KeyU", "KeyV", "KeyW", "KeyX", "KeyY", "KeyZ", "BracketLeft", "BracketRight", "Backslash", "CapsLock", "Semicolon", "Quote", "Enter", "ShiftLeft", "IntlBackslash", "Comma", "Period", "Slash", "ShiftRight", "ControlLeft", "MetaLeft", "AltLeft", "Space", "AltRight", "MetaRight", "ContextMenu", "ControlRight", "PrintScreen", "ScrollLock", "Pause", "Insert", "Home", "PageUp", "Delete", "End", "PageDown", "ArrowUp", "ArrowLeft", "ArrowDown", "ArrowRight", "NumLock", "NumpadDivide", "NumpadMultiply", "NumpadSubtract", "NumpadAdd", "NumpadEnter", "NumpadDecimal", "Numpad0", "Numpad1", "Numpad2", "Numpad3", "Numpad4", "Numpad5", "Numpad6", "Numpad7", "Numpad8", "Numpad9", "NumpadEqual", "NumpadComma", "IntlRo", "IntlYen", "F13", "F14", "F15", "F16", "F17", "F18", "F19", "F20", "F21", "F22", "F23", "F24",
];

/// GASM_KEY_* code of a winit key (0 if it has none).
pub fn gasm_key(code: KeyCode) -> u16 {
    use std::sync::OnceLock;
    static MAP: OnceLock<Vec<(KeyCode, u16)>> = OnceLock::new();
    MAP.get_or_init(|| {
        KEY_CODES.iter().enumerate().skip(1).filter_map(|(i, n)| key_from_code(normalize(n)).map(|k| (k, i as u16))).collect()
    })
    .iter()
    .find(|(k, _)| *k == code)
    .map_or(0, |(_, i)| *i)
}

/// Browser names for keys whose winit name differs.
pub fn normalize(s: &str) -> &str {
    match s { "MetaLeft" | "OSLeft" => "SuperLeft", "MetaRight" | "OSRight" => "SuperRight", other => other }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashSet;

    #[test]
    fn default_keymap_parses_and_two_players_share_a_keyboard() {
        let map = parse(DEFAULT_KEYMAP).unwrap();
        let held: HashSet<_> = [KeyCode::KeyX, KeyCode::KeyL, KeyCode::ControlRight].into();
        let p = pads(&map, &held, 0);
        assert_eq!(p[0], 1 << 0); // X = A on pad 1
        assert_eq!(p[1], (1 << 11) | (1 << 7)); // L = right, RCtrl = start on pad 2
        assert_eq!(pads(&map, &held, 2)[1], 0); // two gamepads: keyboard player 2 off
        assert!(parse("1 jump KeyX").is_err());
        assert!(parse("5 a KeyX").is_err());
        assert!(parse("1 a Escape").is_err());
    }
}
