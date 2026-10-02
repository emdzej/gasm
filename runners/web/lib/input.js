// Input: raw key codes, the browser's keyboard / pointer / gamepads, keyboard layouts.

/** gasm raw key codes (GASM_KEY_*): index = code, W3C KeyboardEvent.code names (spec/abi.json). */
export const KEY_CODES = [
  '', 'Escape', 'F1', 'F2', 'F3', 'F4', 'F5', 'F6', 'F7', 'F8', 'F9', 'F10', 'F11', 'F12', 'Backquote', 'Digit0', 'Digit1', 'Digit2', 'Digit3', 'Digit4', 'Digit5', 'Digit6', 'Digit7', 'Digit8', 'Digit9', 'Minus', 'Equal', 'Backspace', 'Tab', 'KeyA', 'KeyB', 'KeyC', 'KeyD', 'KeyE', 'KeyF', 'KeyG', 'KeyH', 'KeyI', 'KeyJ', 'KeyK', 'KeyL', 'KeyM', 'KeyN', 'KeyO', 'KeyP', 'KeyQ', 'KeyR', 'KeyS', 'KeyT', 'KeyU', 'KeyV', 'KeyW', 'KeyX', 'KeyY', 'KeyZ', 'BracketLeft', 'BracketRight', 'Backslash', 'CapsLock', 'Semicolon', 'Quote', 'Enter', 'ShiftLeft', 'IntlBackslash', 'Comma', 'Period', 'Slash', 'ShiftRight', 'ControlLeft', 'MetaLeft', 'AltLeft', 'Space', 'AltRight', 'MetaRight', 'ContextMenu', 'ControlRight', 'PrintScreen', 'ScrollLock', 'Pause', 'Insert', 'Home', 'PageUp', 'Delete', 'End', 'PageDown', 'ArrowUp', 'ArrowLeft', 'ArrowDown', 'ArrowRight', 'NumLock', 'NumpadDivide', 'NumpadMultiply', 'NumpadSubtract', 'NumpadAdd', 'NumpadEnter', 'NumpadDecimal', 'Numpad0', 'Numpad1', 'Numpad2', 'Numpad3', 'Numpad4', 'Numpad5', 'Numpad6', 'Numpad7', 'Numpad8', 'Numpad9', 'NumpadEqual', 'NumpadComma', 'IntlRo', 'IntlYen', 'F13', 'F14', 'F15', 'F16', 'F17', 'F18', 'F19', 'F20', 'F21', 'F22', 'F23', 'F24',
];
const KEY_INDEX = new Map(KEY_CODES.map((n, i) => [n, i]).filter(([n]) => n));
/** GASM_KEY_* code for a KeyboardEvent.code (0 if none). */
export const keyCode = (code) => KEY_INDEX.get(code) ?? KEY_INDEX.get({ OSLeft: 'MetaLeft', OSRight: 'MetaRight' }[code]) ?? 0;
/** A KeyboardEvent.code as keymaps name it (old browsers say OSLeft for MetaLeft). */
export const normalizeCode = (code) => ({ OSLeft: 'MetaLeft', OSRight: 'MetaRight' }[code] ?? code);
export const KEY_STATE_BYTES = 32;
export const POINTER_BYTES = 48;
export const GAMEPAD_BYTES = 204;
export const GAMEPAD_BUTTONS = 32, GAMEPAD_AXES = 16;
export const INPUT_KEYS_RAW = 1, INPUT_POINTER_HIDDEN = 2, INPUT_POINTER_LOCKED = 4;

/**
 * A drawable position mapped into the last video_present frame (letterboxed as the
 * runners display it). Same arithmetic as runners/native/src/host.rs frame_position.
 */
export function framePosition(x, y, [dw, dh] = [0, 0], [fw, fh] = [0, 0]) {
  if (!fw || !fh || !(dw > 0) || !(dh > 0)) return [x, y];
  const scale = Math.min(dw / fw, dh / fh);
  return [(x - (dw - fw * scale) / 2) / scale, (y - (dh - fh * scale) / 2) / scale];
}

const MOUSE_BITS = [1, 4, 2, 8, 16];   // DOM MouseEvent.button 0..4 -> GASM_MOUSE_* (left, middle, right, back, forward)
// W3C standard gamepads: the browser already reports the standard order.

/**
 * Collects keyboard, pointer and gamepad input on a page for GasmHost.input (or
 * GasmWorker.frames). `element`: the canvas (pointer coordinates, pointer lock).
 *   const input = new BrowserInput(canvas).attach();
 *   host.input = input.frame(true);       // before each frame; true = first of a batch
 *   input.setMode(host.inputMode);        // after frames: hide / lock the cursor
 */
export class BrowserInput {
  constructor(element, { ignore = (e) => e.target instanceof HTMLInputElement || e.target instanceof HTMLTextAreaElement } = {}) {
    this.el = element; this.ignore = ignore;
    this.keys = new Uint8Array(KEY_STATE_BYTES); this.events = [];
    this.p = { x: 0, y: 0, dx: 0, dy: 0, wheelX: 0, wheelY: 0, buttons: 0, pressed: 0, released: 0, inside: false };
    this.mode = 0;
    this.handlers = []; this.elHandlers = [];
  }
  on(target, type, fn, opts) { target.addEventListener(type, fn, opts); this.handlers.push([target, type, fn, opts]); }
  setKey(code, down) {
    const k = keyCode(code);
    if (!k) return;
    const bit = 1 << (k & 7), was = (this.keys[k >> 3] & bit) !== 0;
    if (was === down) return;   // auto-repeat
    if (down) this.keys[k >> 3] |= bit; else this.keys[k >> 3] &= ~bit;
    this.events.push([k, down]);
  }
  attach() {
    this.on(globalThis, 'keydown', (e) => { if (!this.ignore(e)) this.setKey(e.code, true); });
    this.on(globalThis, 'keyup', (e) => this.setKey(e.code, false));
    this.on(globalThis, 'blur', () => { for (let k = 1; k < KEY_CODES.length; k++) if (this.keys[k >> 3] & (1 << (k & 7))) this.setKey(KEY_CODES[k], false); });
    this.on(globalThis, 'pointerup', (e) => { const bit = MOUSE_BITS[e.button] ?? 0; this.p.buttons &= ~bit; this.p.released |= bit; });
    this.setElement(this.el);
    return this;
  }
  /** Follow a new canvas (pointer coordinates and lock are relative to it). */
  setElement(el) {
    for (const [t, type, fn, opts] of this.elHandlers) t.removeEventListener(type, fn, opts);
    this.elHandlers = [];
    this.el = el;
    const dpr = () => globalThis.devicePixelRatio || 1;
    const on = (type, fn, opts) => { el.addEventListener(type, fn, opts); this.elHandlers.push([el, type, fn, opts]); };
    const pos = (e) => { this.p.x = e.offsetX * dpr(); this.p.y = e.offsetY * dpr(); };
    on('pointermove', (e) => { pos(e); this.p.inside = true; this.p.dx += e.movementX * dpr(); this.p.dy += e.movementY * dpr(); });
    on('pointerenter', () => { this.p.inside = true; });
    on('pointerleave', () => { this.p.inside = false; });
    on('pointerdown', (e) => {
      pos(e);
      const bit = MOUSE_BITS[e.button] ?? 0;
      this.p.buttons |= bit; this.p.pressed |= bit;
      if (this.mode & INPUT_POINTER_LOCKED && document.pointerLockElement !== el) el.requestPointerLock?.();
    });
    on('contextmenu', (e) => e.preventDefault());
    on('wheel', (e) => {
      // about 1 per wheel notch (DOM: pixels ~100/notch, lines ~3/notch)
      const k = e.deltaMode === 1 ? 1 / 3 : e.deltaMode === 2 ? 1 : 1 / 100;
      this.p.wheelX += e.deltaX * k; this.p.wheelY += e.deltaY * k;
      e.preventDefault();
    }, { passive: false });
    this.setMode(this.mode);
  }
  detach() {
    for (const [t, type, fn, opts] of [...this.handlers, ...this.elHandlers]) t.removeEventListener(type, fn, opts);
    this.handlers = []; this.elHandlers = [];
  }
  /** Hide / lock the cursor (GASM_INPUT_POINTER_*). Locking happens on the next click (browser rule). */
  setMode(flags) {
    this.mode = flags & 6;
    this.el.style.cursor = this.mode ? 'none' : '';
    if (!(this.mode & INPUT_POINTER_LOCKED) && globalThis.document?.pointerLockElement === this.el) document.exitPointerLock();
  }
  /** Raw input for one frame. `first`: the first frame of a catch-up batch gets the
   *  deltas and events; later frames of the batch reuse its measurements. */
  frame(first = true) {
    if (first || !this.drawable) {
      const r = this.el.getBoundingClientRect?.() ?? { width: 0, height: 0 }, dpr = globalThis.devicePixelRatio || 1;
      this.drawable = [Math.round(r.width * dpr), Math.round(r.height * dpr)];
      this.pads = browserGamepads();
    }
    const drawable = this.drawable;
    const p = this.p, locked = document.pointerLockElement === this.el;
    const pointer = {
      x: p.x, y: p.y, dx: first ? p.dx : 0, dy: first ? p.dy : 0, wheelX: first ? p.wheelX : 0, wheelY: first ? p.wheelY : 0,
      buttons: p.buttons, pressed: first ? p.pressed : 0, released: first ? p.released : 0,
      flags: (p.inside || locked ? 1 : 0) | (this.mode ? 2 : 0) | (locked ? 4 : 0), drawable,
    };
    if (first) { p.dx = p.dy = p.wheelX = p.wheelY = 0; p.pressed = p.released = 0; }
    const events = first ? this.events : [];
    if (first) this.events = [];
    return { keys: this.keys.slice(), keyEvents: events, pointer, gamepads: this.pads };
  }
}

// W3C "standard" gamepad button -> GASM_BTN_* bit (face buttons by position)
const STANDARD_PAD = { 1: 0, 0: 1, 3: 2, 2: 3, 4: 4, 5: 5, 8: 6, 9: 7, 12: 8, 13: 9, 14: 10, 15: 11 };

/** Virtual pads (GASM_BTN_* bitmasks) from raw gamepads (browserGamepads() or a
 *  frame's input.gamepads): standard buttons plus the left stick as a d-pad. */
export function gamepadPads(gamepads) {
  return gamepads.map((g) => {
    if (!g.connected) return 0;
    let m = 0;
    for (const [btn, bit] of Object.entries(STANDARD_PAD)) if (g.buttons[btn] > 0.5) m |= 1 << bit;
    const [x = 0, y = 0] = g.axes;
    if (x < -0.5) m |= 1 << 10; if (x > 0.5) m |= 1 << 11;
    if (y < -0.5) m |= 1 << 8;  if (y > 0.5) m |= 1 << 9;
    return m;
  });
}

/** navigator.getGamepads() as GasmHost.input.gamepads: connected pads in order, 4 slots. */
export function browserGamepads() {
  const out = Array.from({ length: 4 }, () => ({ connected: false, standard: false, buttons: [], axes: [], name: '' }));
  let i = 0;
  for (const gp of globalThis.navigator?.getGamepads?.() ?? []) {
    if (!gp || i > 3) continue;
    out[i++] = { connected: true, standard: gp.mapping === 'standard', name: gp.id,
      buttons: gp.buttons.map((b) => b.value), axes: [...gp.axes] };
  }
  return out;
}


// ---- keyboard layouts ---------------------------------------------------------------
// One text format for both runners (gasm-run --keymap FILE, the web player's "keys"
// editor). A line per binding: <pad 1-4> <button> <key code> [<key code> ...].
// Buttons: a b x y l r select start up down left right. Key codes are
// KeyboardEvent.code names (KeyX, ArrowUp, Period, ControlRight, NumpadEnter...).
// Keyboard bindings for pad N >= 2 apply while fewer than N gamepads are connected.

export const BUTTONS = ['a', 'b', 'x', 'y', 'l', 'r', 'select', 'start', 'up', 'down', 'left', 'right'];

export const DEFAULT_KEYMAP = `# gasm keyboard layout: <pad> <button> <key code>...  (KeyboardEvent.code names)
# player 1
1 up ArrowUp
1 down ArrowDown
1 left ArrowLeft
1 right ArrowRight
1 a KeyX
1 b KeyZ
1 x KeyS
1 y KeyA
1 l KeyQ
1 r KeyW
1 select ShiftRight
1 start Enter
# player 2 (used while fewer than two gamepads are connected)
2 up KeyI
2 down KeyK
2 left KeyJ
2 right KeyL
2 a Period
2 b Comma
2 x KeyM
2 y KeyN
2 l KeyU
2 r KeyO
2 select Backspace
2 start ControlRight NumpadEnter
`;

// Key names a keymap may use besides KEY_CODES (as the native runner accepts them).
const KEY_ALIASES = { OSLeft: 'MetaLeft', OSRight: 'MetaRight', SuperLeft: 'MetaLeft', SuperRight: 'MetaRight' };

/** Parse a keymap. Returns { bindings: Map<code, [{pad, bit}]>, errors: string[] }.
 *  Accepts and rejects exactly what runners/native/src/keymap.rs does. */
export function parseKeymap(text) {
  const bindings = new Map(), errors = [];
  text.split('\n').forEach((raw, i) => {
    const line = raw.replace(/#.*/, '').trim();
    if (!line) return;
    const [pad, button, ...keys] = line.split(/\s+/);
    const p = /^\+?\d+$/.test(pad) ? Number(pad) : NaN, bit = BUTTONS.indexOf((button ?? '').toLowerCase());
    if (!(p >= 1 && p <= 4) || bit < 0 || !keys.length) {
      errors.push(`line ${i + 1}: expected "<pad 1-4> <button> <key>...", got "${line}"`);
      return;
    }
    for (const name of keys) {
      const code = KEY_ALIASES[name] ?? name;
      if (code === 'Escape') { errors.push(`line ${i + 1}: Escape is reserved (quit)`); continue; }
      if (!KEY_INDEX.has(code)) { errors.push(`line ${i + 1}: unknown key code ${JSON.stringify(name)}`); continue; }
      (bindings.get(code) ?? bindings.set(code, []).get(code)).push({ pad: p - 1, bit });
    }
  });
  return { bindings, errors };
}

/** Pads from held keys: pad N (N >= 1, zero-based) only while fewer than N+1 gamepads exist. */
export function keyboardPads(bindings, held, gamepads = 0) {
  const pads = [0, 0, 0, 0];
  for (const code of held) {
    for (const { pad, bit } of bindings.get(code) ?? []) {
      if (pad === 0 || gamepads <= pad) pads[pad] |= 1 << bit;
    }
  }
  return pads;
}
