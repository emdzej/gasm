// Browser runner: canvas (2D or WebGPU) + AudioWorklet + keyboard/Gamepad API
// + WebSocket networking around GasmHost.
import { GasmHost, IdbStorage, MemoryStorage, ProcExit, Resampler } from './gasm-host.js';
import { WebGpuGfx } from './webgpu-gfx.js';

// Where build/*.wasm and roms/ live, relative to this page: the repo root in
// development (`make web`), the page's own directory on the website.
const ROOT = new URL(document.querySelector('meta[name=gasm-root]')?.content ?? '../../', import.meta.url);
const GAMES = {
  'sumo.wasm': '3D sumo (2 players)', 'nes.wasm': 'NES (tetanes-core)',
  'triangle.wasm': 'GPU triangle', 'test-pattern.wasm': 'test pattern (C)',
};

const $ = (id) => document.getElementById(id);
// A canvas can only ever have one context type, so each game gets a fresh one.
let canvas = $('screen'), ctx = null;
function freshCanvas() {
  const c = document.createElement('canvas');
  c.id = 'screen';
  canvas.replaceWith(c);
  canvas = c;
  ctx = null;
  return c;
}
const log = (m) => { console.log(m); $('log').textContent = m; };

// ---- input -----------------------------------------------------------------
const KEYS = { KeyX: 0, KeyZ: 1, KeyS: 2, KeyA: 3, KeyQ: 4, KeyW: 5, ShiftRight: 6, ShiftLeft: 6,
               Enter: 7, ArrowUp: 8, ArrowDown: 9, ArrowLeft: 10, ArrowRight: 11 };
// W3C "standard" gamepad mapping -> gasm bit
const PAD = { 1: 0, 0: 1, 3: 2, 2: 3, 4: 4, 5: 5, 8: 6, 9: 7, 12: 8, 13: 9, 14: 10, 15: 11 };
let keyMask = 0;
addEventListener('keydown', (e) => { if (e.code in KEYS) { keyMask |= 1 << KEYS[e.code]; e.preventDefault(); } });
addEventListener('keyup', (e) => { if (e.code in KEYS) keyMask &= ~(1 << KEYS[e.code]); });
addEventListener('blur', () => { keyMask = 0; });

function readPads() {
  const pads = [keyMask, 0, 0, 0];
  let i = 0;
  for (const gp of navigator.getGamepads?.() ?? []) {
    if (!gp || i > 3) continue;
    let m = 0;
    for (const [btn, bit] of Object.entries(PAD)) if (gp.buttons[btn]?.pressed) m |= 1 << bit;
    const [x = 0, y = 0] = gp.axes;
    if (x < -0.5) m |= 1 << 10; if (x > 0.5) m |= 1 << 11;
    if (y < -0.5) m |= 1 << 8;  if (y > 0.5) m |= 1 << 9;
    pads[i++] |= m;
  }
  return pads;
}

// ---- audio -----------------------------------------------------------------
const WORKLET = `
class GasmOut extends AudioWorkletProcessor {
  constructor() {
    super();
    this.q = []; this.off = 0; this.len = 0; this.primed = false;
    const rate = sampleRate;
    this.target = Math.round(rate * 0.06) * 2; this.max = Math.round(rate * 0.2) * 2;
    this.port.onmessage = (e) => {
      this.q.push(e.data); this.len += e.data.length;
      while (this.len > this.max && this.q.length > 1) { this.len -= this.q[0].length - this.off; this.q.shift(); this.off = 0; }
    };
  }
  process(_, [out]) {
    const L = out[0], R = out[1] ?? out[0];
    if (!this.primed && this.len >= this.target) this.primed = true;
    for (let i = 0; i < L.length; i++) {
      if (!this.primed || this.len < 2) { this.primed = false; L[i] = R[i] = 0; continue; }
      const b = this.q[0];
      L[i] = b[this.off]; R[i] = b[this.off + 1];
      this.off += 2; this.len -= 2;
      if (this.off >= b.length) { this.q.shift(); this.off = 0; }
    }
    return true;
  }
}
registerProcessor('gasm-out', GasmOut);`;

let audioCtx, audioNode, resampler;
async function initAudio() {
  if (audioCtx) return;
  try {
    audioCtx = new AudioContext({ latencyHint: 'interactive' });
    await audioCtx.audioWorklet.addModule(URL.createObjectURL(new Blob([WORKLET], { type: 'text/javascript' })));
    audioNode = new AudioWorkletNode(audioCtx, 'gasm-out', { outputChannelCount: [2] });
    audioNode.connect(audioCtx.destination);
    resampler = new Resampler(audioCtx.sampleRate);
  } catch (e) {
    log(`audio disabled: ${e.message}`);
    audioCtx = null;
  }
}

// ---- game loop -------------------------------------------------------------
let host = null, running = false, rafId = 0;

function present(rgba, w, h) {
  ctx ??= canvas.getContext('2d');
  if (canvas.width !== w || canvas.height !== h) {
    canvas.width = w; canvas.height = h;
    canvas.style.aspectRatio = `${w} / ${h}`;
  }
  ctx.putImageData(new ImageData(rgba, w, h), 0, 0);
}

function onAudio(samples, rate, channels) {
  if (!audioNode) return;
  const out = resampler.process(samples, rate, channels);
  audioNode.port.postMessage(out.slice(), []);
}

let acc = 0, last = 0, fpsN = 0, fpsT = 0;
function tick(now) {
  rafId = requestAnimationFrame(tick);
  if (!running) return;
  const period = 1000 / host.frameRate;
  acc += Math.min(now - last, 100);   // clamp after tab switches
  last = now;
  let steps = 0;
  // Fixed timestep; when catching up, only the last frame is rendered.
  const due = Math.min(4, Math.floor(acc / period));
  while (steps < due) {
    const pads = readPads();
    host.getPad = (p) => pads[p] ?? 0;
    host.showFrame = steps === due - 1;
    try { host.frame(); } catch (e) {
      running = false;
      if (e instanceof ProcExit) log(`game exited (code ${e.code})`);
      else { log(`guest trapped: ${e.message}`); console.error(e); }
      return;
    }
    acc -= period; steps++; fpsN++;
  }
  if (acc > period * 4) acc = 0; // fell far behind: resync
  if (now - fpsT >= 1000) { $('fps').textContent = `${fpsN} fps`; fpsN = 0; fpsT = now; }
}

async function start({ romBytes } = {}) {
  cancelAnimationFrame(rafId);
  running = false;
  await initAudio();
  audioCtx?.resume();
  const game = $('game').value;
  const assets = {};
  if (game === 'nes.wasm') {
    romBytes ??= await fetchBytes(new URL(`roms/${$('rom').value}`, ROOT));
    assets.rom = romBytes;
  }
  // Launch parameters: URL query (?relay=...&room=...) plus the relay/room fields.
  const params = Object.fromEntries([...new URLSearchParams(location.search)].filter(([k]) => !['game', 'autostart'].includes(k)));
  if ($('relay').value.trim()) { params.relay = $('relay').value.trim(); params.room = $('room').value.trim() || 'sumo'; }
  try {
    const bytes = await fetchBytes(new URL(`build/${game}`, ROOT));
    const usesGfx = WebAssembly.Module.imports(await WebAssembly.compile(bytes)).some((i) => i.module === 'gasm:gfx');
    const c = freshCanvas();
    c.classList.toggle('gpu', usesGfx);
    const gfx = usesGfx ? await WebGpuGfx.create(c, log) : undefined;
    // Saves live in IndexedDB, one namespace per game file (sumo.wasm -> "sumo").
    const storage = await IdbStorage.open(game.replace(/\.wasm$/, '')).catch((e) => {
      log(`storage unavailable (${e.message}); saves won't persist`);
      return new MemoryStorage();
    });
    host?.exit();
    host = new GasmHost({ assets, params, gfx, storage, allowNet: true, onPresent: present, onAudio, onLog: log });
    await host.load(bytes);
  } catch (e) {
    if (e instanceof ProcExit) log(`game exited during init (code ${e.code})`);
    else { log(`load failed: ${e.message}`); console.error(e); }
    return;
  }
  log(`running ${game}${assets.rom ? ` (${assets.rom.length} byte rom)` : ''} @ ${host.frameRate.toFixed(2)} Hz`);
  acc = 0; last = performance.now(); running = true;
  rafId = requestAnimationFrame(tick);
}

async function fetchBytes(url) {
  const r = await fetch(url);
  if (!r.ok) throw new Error(`${url.pathname}: HTTP ${r.status}`);
  return new Uint8Array(await r.arrayBuffer());
}

// ---- UI ----------------------------------------------------------------------
for (const [file, label] of Object.entries(GAMES)) $('game').add(new Option(label, file));
$('game').onchange = () => {
  const g = $('game').value;
  $('rom').hidden = $('romlabel').hidden = g !== 'nes.wasm';
  $('netfields').hidden = g !== 'sumo.wasm';
};
$('relay').value = new URLSearchParams(location.search).get('relay') ?? '';
$('room').value = new URLSearchParams(location.search).get('room') ?? 'sumo';
$('relay').placeholder = `ws://${location.hostname || 'localhost'}:9000  (empty = vs. bot)`;

// roms/ is listed via the dev server's directory index (python -m http.server)
fetch(new URL('roms/', ROOT)).then((r) => r.text()).then((html) => {
  const names = [...html.matchAll(/href="([^"]+\.nes)"/gi)].map((m) => decodeURIComponent(m[1]));
  for (const n of names) $('rom').add(new Option(n, n));
  const preferred = names.find((n) => n.startsWith('bladebuster'));
  if (preferred) $('rom').value = preferred;
  if (!names.length) log('no roms/ found — run `make roms`, or open a .nes file');
}).catch(() => log('no roms/ listing — open a .nes file'));

$('start').onclick = () => start();
$('pause').onclick = () => { running = !running; last = performance.now(); $('pause').textContent = running ? '❚❚ pause' : '▶ resume'; };
$('romfile').onchange = async (e) => {
  const f = e.target.files[0];
  if (f) { $('game').value = 'nes.wasm'; start({ romBytes: new Uint8Array(await f.arrayBuffer()) }); }
};
const stage = $('stage');
stage.ondragover = (e) => { e.preventDefault(); stage.classList.add('drag'); };
stage.ondragleave = () => stage.classList.remove('drag');
stage.ondrop = async (e) => {
  e.preventDefault(); stage.classList.remove('drag');
  const f = e.dataTransfer.files[0];
  if (f) { $('game').value = 'nes.wasm'; start({ romBytes: new Uint8Array(await f.arrayBuffer()) }); }
};

const params = new URLSearchParams(location.search);
if (params.get('game') in GAMES) $('game').value = params.get('game');
$('game').onchange();
if (params.has('autostart')) start();
// Leaving the page: let the game flush its saves (gasm_exit).
addEventListener('pagehide', () => host?.exit());
