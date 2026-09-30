// Browser runner: canvas (2D or WebGPU) + AudioWorklet + keyboard/Gamepad API
// + WebSocket networking around GasmHost.
import {
  GasmHost, IdbStorage, MemoryStorage, ProcExit, Resampler, AssetTable, bytesSource,
  directoryHandleEntries, fileListEntries, preloadAssets, DEFAULT_KEYMAP, parseKeymap, keyboardPads,
} from './gasm-host.js';
import { GasmWorker } from './gasm-worker.js';
import { WebGpuGfx } from './webgpu-gfx.js';

// Where build/*.wasm and roms/ live, relative to this page: the repo root in
// development (`make web`), the page's own directory on the website.
const ROOT = new URL(document.querySelector('meta[name=gasm-root]')?.content ?? '../../', import.meta.url);
const GAMES = {
  'sumo.wasm': '3D sumo (2 players)', 'nes.wasm': 'NES (tetanes-core)',
  'triangle.wasm': 'GPU triangle', 'test-pattern.wasm': 'test pattern (C)',
  'assetcheck.wasm': 'asset check (test)',
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
// Keyboard layout: the shared keymap format (gasm-host.js DEFAULT_KEYMAP), editable
// with "keys..." and kept in localStorage. Gamepads take pads in connection order;
// keyboard bindings for pad N >= 2 apply while fewer than N gamepads are connected.
// W3C "standard" gamepad mapping -> gasm bit
const PAD = { 1: 0, 0: 1, 3: 2, 2: 3, 4: 4, 5: 5, 8: 6, 9: 7, 12: 8, 13: 9, 14: 10, 15: 11 };
const KEYMAP_KEY = 'gasm.keymap';
let keymap = loadKeymap(localStorage.getItem(KEYMAP_KEY) ?? DEFAULT_KEYMAP);
function loadKeymap(text) {
  const k = parseKeymap(text);
  if (k.errors.length) { log(`keymap: ${k.errors[0]}; using the default`); return parseKeymap(DEFAULT_KEYMAP).bindings; }
  return k.bindings;
}
const held = new Set();
const typing = (e) => e.target instanceof HTMLInputElement || e.target instanceof HTMLTextAreaElement;
addEventListener('keydown', (e) => {
  if (typing(e) || !keymap.has(e.code)) return;
  held.add(e.code);
  e.preventDefault();
});
addEventListener('keyup', (e) => { held.delete(e.code); });
addEventListener('blur', () => held.clear());

function readPads() {
  const pads = [0, 0, 0, 0];
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
  const kb = keyboardPads(keymap, held, i);
  return pads.map((p, k) => p | kb[k]);
}

// "keys..." editor
$('keys').onclick = () => {
  $('keymaptext').value = localStorage.getItem(KEYMAP_KEY) ?? DEFAULT_KEYMAP;
  $('keymaperr').textContent = '';
  $('keysdlg').showModal();
};
$('keymapsave').onclick = (e) => {
  const text = $('keymaptext').value, k = parseKeymap(text);
  if (k.errors.length) { e.preventDefault(); $('keymaperr').textContent = k.errors.join('\n'); return; }
  localStorage.setItem(KEYMAP_KEY, text);
  keymap = k.bindings;
  log(`keyboard layout saved (${k.bindings.size} keys)`);
};
$('keymapreset').onclick = (e) => { e.preventDefault(); $('keymaptext').value = DEFAULT_KEYMAP; $('keymaperr').textContent = ''; };

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
// Two runtimes behind one loop: main thread (default; required for gasm:gfx) and
// Worker mode (gasm-worker.js; lazy OPFS / File assets, guest off the main thread).
let host = null, worker = null, inflight = false, running = false, rafId = 0;
let folder = null; // { name, entries: [[relative name, File]] } from "open folder..."

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

function stopped(e) {
  running = false;
  if (e instanceof ProcExit) log(`game exited (code ${e.code})`);
  else { log(`guest trapped: ${e.message}`); console.error(e); }
}

let acc = 0, last = 0, fpsN = 0, fpsT = 0;
function tick(now) {
  rafId = requestAnimationFrame(tick);
  if (!running) return;
  const rate = (worker ?? host).frameRate;
  const period = 1000 / rate;
  acc += Math.min(now - last, 100);   // clamp after tab switches
  last = now;
  // Fixed timestep; when catching up (max 4), only the last frame is rendered.
  const due = Math.min(4, Math.floor(acc / period));
  if (worker) {
    if (!inflight && due > 0) {       // one batch in flight: the worker sets the pace
      const pads = readPads();
      inflight = true;
      acc -= due * period;
      worker.frames(Array.from({ length: due }, () => pads), true).then((r) => {
        inflight = false;
        fpsN += due;
        if (r.frame) present(r.frame.rgba, r.frame.width, r.frame.height);
      }, stopped);
    }
  } else {
    for (let steps = 0; steps < due; steps++) {
      const pads = readPads();
      host.getPad = (p) => pads[p] ?? 0;
      host.showFrame = steps === due - 1;
      try { host.frame(); } catch (e) { return stopped(e); }
      acc -= period; fpsN++;
    }
  }
  if (acc > period * 4) acc = 0; // fell far behind: resync
  if (now - fpsT >= 1000) { $('fps').textContent = `${fpsN} fps${worker ? ' (worker)' : ''}`; fpsN = 0; fpsT = now; }
}

const hex = (h) => (h >>> 0).toString(16).padStart(8, '0');

/** ?hashframes=N: run N frames flat out with no input (virtual time) and report hashes
 *  in the same format as the headless runners, for cross-runner checks. */
async function hashRun(n) {
  let s;
  if (worker) {
    let left = n;
    try {
      while (left > 0) { const k = Math.min(500, left); await worker.frames(Array.from({ length: k }, () => [0, 0, 0, 0]), false); left -= k; }
    } catch (e) { if (!(e instanceof ProcExit)) throw e; }
    s = worker.stats;
  } else {
    try { for (let i = 0; i < n; i++) host.frame(); } catch (e) { if (!(e instanceof ProcExit)) throw e; }
    s = { frames: host.frameIndex, presented: host.framesPresented, width: host.width, height: host.height,
          videoHash: host.videoHash, audioHash: host.audioHash, audioFrames: host.audioFrames };
  }
  const out = `frames=${s.frames} presented=${s.presented} size=${s.width}x${s.height} ` +
              `video_fnv32=${hex(s.videoHash)} audio_fnv32=${hex(s.audioHash)} audio_frames=${s.audioFrames}`;
  globalThis.__gasmResult = out;
  log(`hash ${worker ? '(worker)' : '(main)'}: ${out}`);
}

async function start({ romBytes } = {}) {
  cancelAnimationFrame(rafId);
  running = false;
  host?.exit(); host = null;
  await worker?.exit(); worker = null; inflight = false;
  await initAudio();
  audioCtx?.resume();
  const url = new URLSearchParams(location.search);
  const game = url.get('wasm') ?? $('game').value;   // ?wasm=<url> runs any module
  const hashFrames = Number(url.get('hashframes') || 0);
  const record = {};
  if (game === 'nes.wasm' && !folder && !url.has('opfs')) {
    romBytes ??= await fetchBytes(new URL(`roms/${url.get('rom') ?? $('rom').value}`, ROOT));
    record.rom = romBytes;
  }
  // Launch parameters: URL query plus the relay/room fields.
  const skip = ['game', 'autostart', 'wasm', 'worker', 'opfs', 'prefix', 'hashframes', 'rom'];
  const params = Object.fromEntries([...url].filter(([k]) => !skip.includes(k)));
  if ($('relay').value.trim()) { params.relay = $('relay').value.trim(); params.room = $('room').value.trim() || 'sumo'; }
  try {
    const bytes = await fetchBytes(game.includes('/') ? new URL(game, location.href) : new URL(`build/${game}`, ROOT));
    const usesGfx = WebAssembly.Module.imports(await WebAssembly.compile(bytes)).some((i) => i.module === 'gasm:gfx');
    const useWorker = !usesGfx && ($('worker').checked || url.has('opfs'));
    const c = freshCanvas();
    c.classList.toggle('gpu', usesGfx);
    const namespace = game.split('/').pop().replace(/\.wasm$/, ''); // saves: one namespace per game file
    const prefix = url.get('prefix') ?? '';
    if (useWorker) {
      // Lazy sources: the worker reads OPFS / picked files synchronously on demand.
      const specs = [{ kind: 'memory', record }];
      if (url.has('opfs')) specs.push({ kind: 'opfs', dir: url.get('opfs'), prefix });
      if (folder) specs.push({ kind: 'files', entries: folder.entries, prefix });
      worker = await GasmWorker.start({
        wasm: bytes, assets: specs, params, storage: namespace, allowNet: true,
        hashing: hashFrames > 0, virtualTime: hashFrames > 0, onLog: log, onAudio,
      });
    } else {
      if (usesGfx && $('worker').checked) log('gasm:gfx games run on the main thread (worker mode needs OffscreenCanvas)');
      let assets = record;
      if (folder) {  // main thread: preload the folder into memory, with progress
        const t = new AssetTable(log);
        for (const [n, b] of Object.entries(record)) t.add(n, bytesSource(b));
        t.merge(await preloadAssets(folder.entries, { prefix, log,
          onProgress: (p) => log(`loading ${folder.name}: ${p.done}/${p.total} files, ${(p.bytes / 1048576).toFixed(1)} MB`) }));
        assets = t;
      }
      const gfx = usesGfx ? await WebGpuGfx.create(c, log) : undefined;
      const storage = await IdbStorage.open(namespace).catch((e) => {
        log(`storage unavailable (${e.message}); saves won't persist`);
        return new MemoryStorage();
      });
      host = new GasmHost({ assets, params, gfx, storage, allowNet: true, onPresent: present, onAudio, onLog: log,
                            virtualTime: hashFrames > 0 });
      await host.load(bytes);
    }
  } catch (e) {
    if (e instanceof ProcExit) log(`game exited during init (code ${e.code})`);
    else { log(`load failed: ${e.message}`); console.error(e); }
    return;
  }
  if (hashFrames > 0) return hashRun(hashFrames).catch((e) => log(`hash run failed: ${e.message}`));
  log(`running ${game}${worker ? ' in a worker' : ''} @ ${(worker ?? host).frameRate.toFixed(2)} Hz`);
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
// "open folder...": assets from a folder, named like gasm-run --asset-dir (case-insensitive).
async function pickFolder() {
  if (window.showDirectoryPicker) {
    try {
      const handle = await showDirectoryPicker();
      folder = { name: handle.name, entries: await directoryHandleEntries(handle) };
    } catch (e) { if (e.name !== 'AbortError') log(`folder: ${e.message}`); return; }
  } else {
    $('folderinput').click(); // webkitdirectory fallback (Firefox, Safari)
    return;
  }
  folderChosen();
}
function folderChosen() {
  const bytes = folder.entries.reduce((n, [, f]) => n + f.size, 0);
  log(`folder ${folder.name}: ${folder.entries.length} files, ${(bytes / 1048576).toFixed(1)} MB (${$('worker').checked ? 'read on demand in the worker' : 'preloaded on start; enable worker for on-demand reads'})`);
}
$('folder').onclick = pickFolder;
$('folderinput').onchange = (e) => {
  const entries = fileListEntries(e.target.files);
  const root = e.target.files[0]?.webkitRelativePath.split('/')[0] ?? 'folder';
  folder = { name: root, entries };
  folderChosen();
};
$('worker').checked = new URLSearchParams(location.search).has('worker');

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
addEventListener('pagehide', () => { host?.exit(); worker?.exit(); });
