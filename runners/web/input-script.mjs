// Scripted input for headless runs (--input). Mirrors runners/native/src/script.rs
// item for item (the same scripts are accepted and rejected; numbers are plain
// decimals); arithmetic in doubles, rounded to f32 once, so both runners feed
// guests the same bits.
//
// Items are FRAMES:ACTION, comma-separated (commas inside quotes or parentheses don't
// split). FRAMES is N or FROM-TO (inclusive). Actions:
//   A+B+START                 virtual pad 1 buttons
//   "text"                    typed text on frame N (escapes \n enter, \b backspace, \\, \")
//   KEY(ShiftLeft+ArrowLeft)  raw keys held (W3C names); events from changes between frames
//   PTR(x,y) / PTR(x,y,L+R)   pointer position (drawable pixels) and buttons (L R M BACK FWD)
//   MOVE(dx,dy)  WHEEL(x,y)   relative motion / wheel per frame
//   GP0(B0+B9+A1=0.5)         gamepad slot 0-3: buttons by index, axes An=value

import { KEY_CODES, KEY_STATE_BYTES } from './gasm-host.js';

const PAD_NAMES = ['A', 'B', 'X', 'Y', 'L', 'R', 'SELECT', 'START', 'UP', 'DOWN', 'LEFT', 'RIGHT'];
const MOUSE_NAMES = ['L', 'R', 'M', 'BACK', 'FWD'];
const f32 = Math.fround;

function splitItems(spec) {
  const items = [];
  let cur = '', quoted = false, escaped = false, depth = 0;
  for (const ch of spec) {
    if (escaped) escaped = false;
    else if (ch === '\\' && quoted) escaped = true;
    else if (ch === '"') quoted = !quoted;
    else if (!quoted && ch === '(') depth++;
    else if (!quoted && ch === ')') depth--;
    else if (ch === ',' && !quoted && depth === 0) { items.push(cur); cur = ''; continue; }
    cur += ch;
  }
  items.push(cur);
  return items;
}

const DECIMAL = /^[+-]?(\d+\.?\d*|\.\d+)([eE][+-]?\d+)?$/;
const args = (rest, name) => (rest.startsWith(name + '(') && rest.endsWith(')') ? rest.slice(name.length + 1, -1) : null);

export class InputScript {
  constructor(spec = '') {
    Object.assign(this, { pads: [], text: [], keys: [], ptr: [], moves: [], wheel: [], gp: [] });
    if (spec) for (const item of splitItems(spec)) this.parseItem(item);
  }

  parseItem(item) {
    const bad = (why = '') => new Error(`bad --input item ${JSON.stringify(item)}${why}`);
    const i = item.indexOf(':');
    if (i < 0) throw bad();
    const range = item.slice(0, i), rest = item.slice(i + 1);
    const r = /^(\d+)(?:-(\d+))?$/.exec(range);
    if (!r) throw bad();
    const from = Number(r[1]), to = Number(r[2] ?? r[1]);
    const num = (v) => { const t = v.trim(), n = Number(t); if (!DECIMAL.test(t) || !Number.isFinite(n)) throw bad(); return n; };
    const pair = (s) => { const k = s.indexOf(','); if (k < 0) throw bad(); return [num(s.slice(0, k)), num(s.slice(k + 1))]; };
    let x;
    if (rest.startsWith('"')) {
      if (!rest.endsWith('"') || rest.length < 2) throw bad();
      let t = '';
      const body = [...rest.slice(1, -1)];
      for (let k = 0; k < body.length; k++) {
        if (body[k] !== '\\') { t += body[k]; continue; }
        if (++k >= body.length) throw bad();   // a lone trailing backslash
        t += body[k] === 'n' ? '\n' : body[k] === 'b' ? '\b' : body[k];
      }
      this.text.push([from, t]);
    } else if ((x = args(rest, 'KEY')) !== null) {
      const codes = x.split('+').map((k) => {
        const c = KEY_CODES.indexOf(k);
        if (c < 1) throw bad(`: unknown key ${JSON.stringify(k)}`);
        return c;
      });
      this.keys.push([from, to, codes]);
    } else if ((x = args(rest, 'PTR')) !== null) {
      const parts = x.split(',');
      if (parts.length < 2) throw bad();
      let buttons = 0;
      if (parts.length > 2) for (const n of parts.slice(2).join(',').split('+')) {
        const bit = MOUSE_NAMES.indexOf(n.trim().toUpperCase());
        if (bit < 0) throw bad();
        buttons |= 1 << bit;
      }
      this.ptr.push([from, to, num(parts[0]), num(parts[1]), buttons]);
    } else if ((x = args(rest, 'MOVE')) !== null) {
      this.moves.push([from, to, ...pair(x)]);
    } else if ((x = args(rest, 'WHEEL')) !== null) {
      this.wheel.push([from, to, ...pair(x)]);
    } else if (/^GP\d\(/.test(rest)) {
      const slot = Number(rest[2]), body = args(rest.slice(3), '');
      if (body === null || slot > 3) throw bad();
      const buttons = [], axes = [];
      for (const part of body.split('+').filter(Boolean)) {
        if (part[0] === 'B' && /^\d+$/.test(part.slice(1))) buttons.push([Number(part.slice(1)), 1]);
        else if (part[0] === 'A' && part.includes('=')) {
          const eq = part.indexOf('='), k = part.slice(1, eq);
          if (!/^\d+$/.test(k)) throw bad();
          axes.push([Number(k), num(part.slice(eq + 1))]);
        }
        else throw bad();
      }
      this.gp.push([from, to, slot, buttons, axes]);
    } else {
      const mask = rest.split('+').reduce((m, n) => {
        const bit = PAD_NAMES.indexOf(n.toUpperCase());
        if (bit < 0) throw bad();
        return m | (1 << bit);
      }, 0);
      this.pads.push([from, to, mask]);
    }
  }

  pad(frame) { return this.pads.reduce((m, [f, t, b]) => (frame >= f && frame <= t ? m | b : m), 0); }
  textAt(frame) { return this.text.filter(([f]) => f === frame).map(([, t]) => t).join(''); }

  /** GasmHost.input for `frame`; `st` carries keys/pointer between frames ({}). */
  raw(frame, st, drawable, mode) {
    const on = (f, t) => frame >= f && frame <= t;
    st.keys ??= new Uint8Array(KEY_STATE_BYTES); st.x ??= 0; st.y ??= 0; st.buttons ??= 0;
    const keys = new Uint8Array(KEY_STATE_BYTES);
    for (const [f, t, codes] of this.keys) if (on(f, t)) for (const c of codes) keys[c >> 3] |= 1 << (c & 7);
    const keyEvents = [];   // releases first, then presses, each in code order
    for (const down of [false, true]) {
      for (let c = 1; c < KEY_CODES.length; c++) {
        const was = (st.keys[c >> 3] >> (c & 7)) & 1, is = (keys[c >> 3] >> (c & 7)) & 1;
        if (was !== is && !!is === down) keyEvents.push([c, down]);
      }
    }
    st.keys = keys;
    const px = st.x, py = st.y;
    let buttons = 0;
    for (const [f, t, x, y, b] of this.ptr) if (on(f, t)) { st.x = x; st.y = y; buttons = b; }
    let dx = st.x - px, dy = st.y - py, wx = 0, wy = 0;
    for (const [f, t, x, y] of this.moves) if (on(f, t)) { dx += x; dy += y; }
    for (const [f, t, x, y] of this.wheel) if (on(f, t)) { wx += x; wy += y; }
    const inside = st.x >= 0 && st.y >= 0 && st.x < drawable[0] && st.y < drawable[1];
    const pointer = {
      x: f32(st.x), y: f32(st.y), dx: f32(dx), dy: f32(dy), wheelX: f32(wx), wheelY: f32(wy),
      buttons, pressed: buttons & ~st.buttons, released: st.buttons & ~buttons,
      flags: (inside ? 1 : 0) | (mode & 6), drawable,
    };
    st.buttons = buttons;
    const gamepads = Array.from({ length: 4 }, (_, slot) => (this.gp.some((e) => e[2] === slot)
      ? { connected: true, standard: true, buttons: new Array(17).fill(0), axes: new Array(4).fill(0), name: 'scripted' }
      : { connected: false, standard: false, buttons: [], axes: [], name: '' }));
    for (const [f, t, slot, bs, axs] of this.gp) {
      if (!on(f, t)) continue;
      const g = gamepads[slot];
      for (const [i, v] of bs) if (i < 32) { while (g.buttons.length <= i) g.buttons.push(0); g.buttons[i] = f32(v); }
      for (const [i, v] of axs) if (i < 16) { while (g.axes.length <= i) g.axes.push(0); g.axes[i] = f32(v); }
    }
    return { keys, keyEvents, pointer, gamepads };
  }
}
