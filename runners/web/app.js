// Browser runner: canvas (2D or WebGPU) + AudioWorklet + keyboard/Gamepad API
// + WebSocket networking around GasmHost.
import { playSplash } from './gasm-splash.js';
import {
  GasmHost, STACK_SWITCHING, staticTitle, IdbStorage, MemoryStorage, ProcExit, Resampler, AssetTable, bytesSource,
  directoryHandleEntries, fileListEntries, preloadAssets, DEFAULT_KEYMAP, parseKeymap, keyboardPads,
  BrowserInput, INPUT_KEYS_RAW, gamepadPads, normalizeCode, SPLASH_W, SPLASH_H, NetPolicy, moduleManifest, parseManifest,
} from './gasm-host.js';
import { GasmWorker } from './gasm-worker.js';
import { WebGpuGfx } from './webgpu-gfx.js';
import { FILTERS, GlPresenter } from './gasm-present.js';

// Where build/*.wasm and roms/ live, relative to this page: the repo root in
// development (`make web`), the page's own directory on the website.
const ROOT = new URL(document.querySelector('meta[name=gasm-root]')?.content ?? '../../', import.meta.url);
const GAMES = {
  'sumo.wasm': '3D sumo (2 players)', 'nes.wasm': 'NES (tetanes-core)', 'doom.wasm': 'DOOM (doomgeneric)',
  'scummvm.wasm': 'ScummVM',
  'sdl3-snake.wasm': 'SDL3: snake', 'sdl3-woodeneye.wasm': 'SDL3: woodeneye-008', 'sdl3-callbacks.wasm': 'SDL3: callbacks + audio',
  'sdl3-classic.wasm': 'SDL3: classic main loop', 'sdl3-threads.wasm': 'SDL3: threads (cooperative)',
  'sdl3-gl.wasm': 'SDL3: OpenGL ES', 'sdl3-snake-gl.wasm': 'SDL3: snake on GLES 2',
  'triangle.wasm': 'GPU triangle', 'textured.wasm': 'GPU textures (test)', 'inputtest.wasm': 'input tester', 'test-pattern.wasm': 'test pattern (C)', 'gltest.wasm': 'GLES 3 test (C)', 'glowtest.wasm': 'GLES 3 from Rust (glow)', 'eguidemo.wasm': 'egui demo (egui_glow)',
  'assetcheck.wasm': 'asset check (test)', 'fetchtest.wasm': 'HTTP requests (test)',
  'godot.wasm': 'Godot: your game (.pck)',
};
// Godot projects: an engine with the project's pack as asset game.pck. 2D games run on
// the smaller engine without 3D (build/godot-2d.wasm), 3D ones on build/godot.wasm.
const GODOT_GAMES = {
  'godot-hello2d': { pck: 'godot/hello2d.pck', label: 'Godot: hello 2D', engine: 'godot-2d.wasm' },
  'godot-platformer': { pck: 'godot/platformer.pck', label: 'Godot: platformer', engine: 'godot-2d.wasm' },
  'godot-scene3d': { pck: 'godot/scene3d.pck', label: 'Godot: 3D scene', engine: 'godot.wasm' },
  'godot-ui': { pck: 'godot/ui.pck', label: 'Godot: UI and saves', engine: 'godot-2d.wasm' },
  'godot-audio': { pck: 'godot/audio.pck', label: 'Godot: audio', engine: 'godot-2d.wasm' },
  'godot-net': { pck: 'godot/net.pck', label: 'Godot: multiplayer (relay)', engine: 'godot-2d.wasm' },
  'godot-relaymp': { pck: 'godot/relaymp.pck', label: 'Godot: RPC multiplayer (relay)', engine: 'godot-2d.wasm' },
  'godot-mods': { pck: 'godot/mods.pck', label: 'Godot: mods (pick them with "mods...")', engine: 'godot-2d.wasm' },
};
for (const [id, g] of Object.entries(GODOT_GAMES)) GAMES[id] = g.label;
// Games that take a content file from roms/ (or an opened/dropped file) as an asset.
// `known` is offered even without a roms/ directory listing (the website has none).
const CONTENT = {
  'nes.wasm': { ext: '.nes', asset: 'rom', preferred: 'bladebuster', known: [] },
  'doom.wasm': { ext: '.wad', asset: 'wad', preferred: 'doom1.wad', known: ['doom1.wad'] },
  // any Godot 4.7 game exported as a pack (Compatibility renderer): open or drop a .pck
  'godot.wasm': { ext: '.pck', asset: 'game.pck', preferred: '', known: [] },
};
// Games that take a folder of files: without "open folder..." they get a default set
// from roms/ (the freeware Beneath a Steel Sky for ScummVM) and launch args.
const FOLDER_GAMES = {
  'scummvm.wasm': { dir: 'bass/', files: ['sky.dnr', 'sky.dsk', 'readme.txt'], args: '-p / sky', folderArgs: '--auto-detect -p /' },
};
// Games with their own main loop also come as a "run build" (no Asyncify: smaller,
// faster), used where the browser can switch stacks (JSPI): design/stack-switching.md.
const RUN_BUILDS = new Set(['scummvm.wasm', 'sdl3-classic.wasm']);
const contentGame = (name) => Object.keys(CONTENT).find((g) => name.toLowerCase().endsWith(CONTENT[g].ext));

const $ = (id) => document.getElementById(id);
// A canvas can only ever have one context type, so each game gets a fresh one
// (and a 2D game gets a new one when it switches between canvas 2D and WebGL).
let canvas = $('screen'), ctx = null, presenter = null;
function freshCanvas() {
  const c = document.createElement('canvas');
  c.id = 'screen';
  canvas.replaceWith(c);
  canvas = c;
  ctx = presenter = null;
  rawInput.setElement(c);
  return c;
}
const log = (m) => { console.log(m); $('log').textContent = m; };
// The tab title: the game's (gasm.set_title, else its file name), then ours.
let gameName = '';
const showTitle = (t) => { document.title = `${t ?? gameName} — gasm`; };

// ---- input -----------------------------------------------------------------
// Keyboard layout: the shared keymap format (gasm-host.js DEFAULT_KEYMAP), editable
// with "keys..." and kept in localStorage. Gamepads take pads in connection order;
// keyboard bindings for pad N >= 2 apply while fewer than N gamepads are connected.
const KEYMAP_KEY = 'gasm.keymap';
let keymap = loadKeymap(localStorage.getItem(KEYMAP_KEY) ?? DEFAULT_KEYMAP);
function loadKeymap(text) {
  const k = parseKeymap(text);
  if (k.errors.length) { log(`keymap: ${k.errors[0]}; using the default`); return parseKeymap(DEFAULT_KEYMAP).bindings; }
  return k.bindings;
}
const held = new Set();
const typing = (e) => e.target instanceof HTMLInputElement || e.target instanceof HTMLTextAreaElement;
// Raw keyboard, pointer and gamepads for the guest (gasm-host.js BrowserInput).
const rawInput = new BrowserInput(canvas).attach();
// Escape: a tap goes to the game; holding it stops the game.
let escDown = 0;
addEventListener('keydown', (e) => { if (e.code === 'Escape' && !e.repeat) escDown = performance.now(); });
addEventListener('keyup', (e) => { if (e.code === 'Escape') escDown = 0; });
// Text for text_input: characters typed since the last frame (also for keys bound to pads).
let typed = '';
addEventListener('keydown', (e) => {
  if (typing(e)) return;
  // a game reading the raw keyboard gets every key (Space doesn't scroll, F5 doesn't reload)
  // (except the paste shortcut: preventing it would cancel the paste event)
  const paste = e.code === 'KeyV' && (e.ctrlKey || e.metaKey);
  if (running && inputMode() & INPUT_KEYS_RAW && e.code !== 'Escape' && !paste) e.preventDefault();
  if (e.key === 'Enter') typed += '\n';
  else if (e.key === 'Backspace') typed += '\b';
  else if (e.key.length === 1 && !e.ctrlKey && !e.metaKey) typed += e.key;
  const code = normalizeCode(e.code);
  if (!keymap.has(code)) return;
  held.add(code);
  e.preventDefault();
});
const takeTyped = () => { const t = typed; typed = ''; return t; };
// gasm:clipboard: the player pasted (Ctrl/Cmd+V): the text goes with the next frame, the one
// that carries the key press; text the game copies goes on the clipboard
let pasted;
addEventListener('paste', (e) => { if (running && !typing(e)) pasted = e.clipboardData?.getData('text/plain') ?? ''; });
const takePasted = () => { const t = pasted; pasted = undefined; return t; };
// gasm:files: a file the game saves for the player is a download
function saveFile(name, mime, bytes) {
  const a = document.createElement('a');
  a.href = URL.createObjectURL(new Blob([bytes], { type: mime }));
  a.download = name;
  a.click();
  setTimeout(() => URL.revokeObjectURL(a.href), 60000);
  log(`saved ${name}`);
  return true;
}
// Player consent: a game reaching a host the page didn't allow (?allownet[=a.org,b.org]
// and the relay field allow up front) waits while the player answers. "always" and
// "never" are remembered per game in localStorage; "forget answers" clears them.
const CONSENT = 'gasm.consent.';
const remembered = (g) => { try { return JSON.parse(localStorage.getItem(CONSENT + g) ?? '{}'); } catch { return {}; } };
let consentQueue = Promise.resolve();
// Subjects: "net:<host>", "net:a.org,b.org" (a manifest's hosts, asked at once and
// remembered for each) and "mod:<file>" (a mod whose manifest wants hosts).
function askPlayer(subject, detail = '') {
  const g = gameName, r = remembered(g);
  const each = subject.startsWith('net:') ? subject.slice(4).split(',').map((h) => `net:${h}`) : [subject];
  if (each.every((s) => s in r)) return each.every((s) => r[s]);
  const what = subject.startsWith('mod:') ? `wants to load the mod ${subject.slice(4)}${detail ? `, which connects to ${detail}` : ''}`
    : `wants to connect to ${subject.slice(4).replaceAll(',', ', ')}`;
  const asked = consentQueue.then(() => new Promise((resolve) => {
    const dlg = $('consentdlg');
    $('consentq').textContent = `${g} ${what}`;
    dlg.onclose = () => {
      const a = dlg.returnValue;
      if (a === 'always' || a === 'never') localStorage.setItem(CONSENT + g, JSON.stringify({ ...remembered(g), ...Object.fromEntries(each.map((s) => [s, a === 'always'])) }));
      log(`${g}: ${subject.replace(/^(net|mod):/, '')}: ${{ once: 'allowed this time', always: 'always allowed', never: 'never allowed' }[a] ?? 'not now'}`);
      resolve(a === 'once' || a === 'always');
    };
    dlg.returnValue = '';
    dlg.showModal();
  }));
  consentQueue = asked.then(() => {}, () => {});
  return asked;
}
$('forget').onclick = (e) => {
  e.preventDefault();
  const keys = Object.keys(localStorage).filter((k) => k.startsWith(CONSENT));
  keys.forEach((k) => localStorage.removeItem(k));
  log(`forgot the remembered answers of ${keys.length} game${keys.length === 1 ? '' : 's'}`);
};
/** Hosts allowed up front: ?allownet (all) or ?allownet=a.org,b.org, plus the relay's host,
 *  plus what the player allowed for this run before it started (runHosts). */
let runHosts = [];
function allowedHosts() {
  const p = new URLSearchParams(location.search);
  if (p.has('allownet') && !p.get('allownet')) return true;
  const hosts = (p.get('allownet') ?? '').split(',').map((h) => h.trim()).filter(Boolean).concat(runHosts);
  try { if ($('relay').value.trim()) hosts.push(new URL($('relay').value.trim()).hostname); } catch {}
  return hosts.length ? hosts : false;
}
const copyText = (t) => navigator.clipboard.writeText(t).catch((e) => log(`clipboard: ${e.message}`));
// F2 (?copykey=<KeyboardEvent.code> or none): copy the game's frame to the clipboard, as
// gasm-run does. The key also reaches the game. A WebGL or WebGPU canvas can only be read
// in the task that drew it, so the copy happens right after the next frame.
const COPY_KEY = new URLSearchParams(location.search).get('copykey') ?? 'F2';
let copyPending = false;
addEventListener('keydown', (e) => { if (e.code === COPY_KEY && !e.repeat && running) copyPending = true; });
function copyFrame() {
  copyPending = false;
  let png;
  if (shown) {   // a 2D frame: its own pixels, opaque, at the game's size
    const [rgba, w, h] = shown;
    const img = new ImageData(new Uint8ClampedArray(rgba), w, h);
    for (let i = 3; i < img.data.length; i += 4) img.data[i] = 255;
    const c = new OffscreenCanvas(w, h);
    c.getContext('2d').putImageData(img, 0, 0);
    png = c.convertToBlob({ type: 'image/png' });
  } else if (gpu) {
    const c = gpu.gl?.canvas ?? canvas;
    png = new Promise((resolve, reject) => c.toBlob((b) => (b ? resolve(b) : reject(new Error('empty canvas'))), 'image/png'));
  } else return log('copy: nothing to copy (yet)');
  // still within the key press's user activation, which the clipboard requires
  navigator.clipboard.write([new ClipboardItem({ 'image/png': png })])
    .then(() => log('copied the frame to the clipboard'), (e) => log(`copy: ${e.message}`));
}
addEventListener('keyup', (e) => { held.delete(normalizeCode(e.code)); });
addEventListener('blur', () => held.clear());

const inputMode = () => (worker ?? host)?.inputMode ?? 0;
/** Virtual pads from a frame's raw gamepads plus the keyboard layout. */
function readPads(gamepads) {
  const pads = gamepadPads(gamepads);
  if (inputMode() & INPUT_KEYS_RAW) return pads;   // the guest reads the keyboard itself
  const kb = keyboardPads(keymap, held, gamepads.filter((g) => g.connected).length);
  return pads.map((p, k) => p | kb[k]);
}
/** The input for a batch of `n` frames (catch-up): text, events and deltas go to the first. */
function batch(n) {
  return Array.from({ length: n }, (_, k) => {
    const input = rawInput.frame(k === 0);
    return { pads: readPads(input.gamepads), text: k === 0 ? takeTyped() : '', input, ...(k === 0 && pasted !== undefined ? { paste: takePasted() } : {}) };
  });
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
// Browsers start audio suspended until the page is clicked or a key is pressed
// (e.g. with ?autostart): resume it on the first such gesture.
for (const type of ['pointerdown', 'keydown']) addEventListener(type, () => { if (audioCtx?.state === 'suspended') audioCtx.resume(); });
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
let host = null, worker = null, gpu = null, inflight = false, running = false, rafId = 0;
let startGen = 0;     // start() calls; an older one still loading gives up
let frame2d = null;   // the latest 2D frame, drawn once per tick
// The canvas' display size in device pixels (Worker mode: the OffscreenCanvas can't measure itself).
const canvasSize = () => [Math.round(canvas.clientWidth * (devicePixelRatio || 1)) || canvas.width,
                          Math.round(canvas.clientHeight * (devicePixelRatio || 1)) || canvas.height];
let folder = null; // { name, entries: [[relative name, File]] } from "open folder..."

// How 2D frames are shown (gasm-present.js): ?filter= and ?integer, else the
// header controls, kept in localStorage. nearest without integer scaling is a
// canvas 2D scaled up by CSS; everything else draws with WebGL 2 at display size.
const view = {
  filter: FILTERS.includes(new URLSearchParams(location.search).get('filter'))
    ? new URLSearchParams(location.search).get('filter') : localStorage.getItem('gasm.filter') ?? 'sharp',
  integerScale: new URLSearchParams(location.search).has('integer') || localStorage.getItem('gasm.integer') === '1',
};
const useGl = () => view.filter !== 'nearest' || view.integerScale;

// 2D frames: remember the latest (catch-up frames would be drawn and overwritten at once)
let shown = null;   // the frame on screen, redrawn when the view settings change
function present(rgba, w, h, aspect = null) { if (!gpu) frame2d = [rgba, w, h, aspect]; }
function draw2d() {
  if (!frame2d) return;
  const [rgba, w, h, aspect] = shown = frame2d;
  frame2d = null;
  // the canvas box has the frame's display aspect (video_set_aspect), so the CSS
  // scaling of the canvas 2D path stretches non-square pixels too
  canvas.style.setProperty('--ar', aspect ? aspect[0] / aspect[1] : w / h);
  if (useGl() && !ctx) {
    presenter ??= GlPresenter.create(canvas);
    if (presenter) {
      canvas.classList.add('gl');
      rawInput.integerScale = view.integerScale;
      return presenter.draw(rgba, w, h, canvasSize(), { ...view, aspect });
    }
    log('WebGL 2 unavailable: showing square pixels');
    view.filter = $('filter').value = 'nearest'; view.integerScale = $('integer').checked = false;
  }
  if (presenter) { freshCanvas(); }   // was WebGL: canvas 2D needs a new canvas
  rawInput.integerScale = false;
  ctx ??= canvas.getContext('2d');
  if (canvas.width !== w || canvas.height !== h) { canvas.width = w; canvas.height = h; }
  ctx.putImageData(new ImageData(rgba, w, h), 0, 0);
}
/** The view settings changed: switch canvases if needed and redraw the frame on screen. */
function viewChanged() {
  localStorage.setItem('gasm.filter', view.filter);
  localStorage.setItem('gasm.integer', view.integerScale ? '1' : '0');
  if (gpu) return;                       // gfx games: no filters
  if (useGl() ? ctx : presenter) freshCanvas();
  if (shown && !frame2d) { frame2d = shown; draw2d(); }
}

function onAudio(samples, rate, channels) {
  if (!audioNode) return;
  const out = resampler.process(samples, rate, channels);   // a new buffer: transfer it
  audioNode.port.postMessage(out, [out.buffer]);
}

// The guest exited or trapped: it is never called again (resume is disabled).
function stopped(e) {
  running = false;
  $('pause').disabled = true;
  if (e instanceof ProcExit) log(`game exited (code ${e.code})`);
  else { log(`guest trapped: ${e.message}`); console.error(e); }
  host?.shutdown(); worker?.exit();
}

/** gasm:gl: the default framebuffer is the canvas at display size. */
function fitGl(c) {
  const [w, h] = canvasSize();
  if (c.width !== w || c.height !== h) { c.width = w; c.height = h; }
}

/** Stop the current game: gasm_exit (saves), close its sockets and storage, free the GPU. */
async function stopGame() {
  cancelAnimationFrame(rafId);
  running = false;
  const h = host, w = worker, g = gpu;
  host = worker = gpu = null; inflight = false; frame2d = shown = null;
  await h?.shutdown();
  await w?.exit();
  g?.device.destroy();
}

let acc = 0, last = 0, fpsN = 0, fpsT = 0;
function tick(now) {
  if (!running) return;                // paused or stopped: resume schedules the next tick
  rafId = requestAnimationFrame(tick);
  if (escDown && now - escDown >= 1000) {   // Escape held: stop (a tap went to the game)
    escDown = 0;
    $('pause').disabled = true;
    stopGame();
    return log('stopped (Escape held)');
  }
  const rate = (worker ?? host).frameRate;
  const period = 1000 / rate;
  acc += Math.min(now - last, 100);   // clamp after tab switches
  last = now;
  // Fixed timestep; when catching up (max 4), only the last frame is rendered.
  const due = Math.min(4, Math.floor(acc / period));
  if (worker) {
    if (!inflight && due > 0) {       // one batch in flight: the worker sets the pace
      inflight = true;
      acc -= due * period;
      const w = worker;
      w.frames(batch(due), true, { size: canvasSize() }).then((r) => {
        if (w !== worker) return;      // a newer game started meanwhile
        inflight = false;
        fpsN += due;
        if (r.frame) { present(r.frame.rgba, r.frame.width, r.frame.height, r.frame.aspect); draw2d(); }
        if (copyPending) copyFrame();
        rawInput.setMode(w.inputMode);
      }, stopped);
    }
  } else if (due > 0 && host.switching) {
    // a gasm_run guest resumes asynchronously: one batch in flight, as for the worker
    if (!inflight) {
      inflight = true;
      acc -= due * period;
      const h = host;
      h.runFramesAsync(batch(due), true).then(() => {
        if (h !== host) return;
        inflight = false;
        fpsN += due;
        draw2d();
        if (copyPending) copyFrame();
        rawInput.setMode(h.inputMode);
      }, stopped);
    }
  } else if (due > 0) {
    // one batch: only the last frame is shown; a gfx canvas gets 2D frames as a WebGPU blit
    if (gpu?.gl) fitGl(gpu.gl.canvas);
    try { host.runFrames(batch(due), true); } catch (e) { return stopped(e); }
    acc -= due * period; fpsN += due;
    draw2d();
    if (copyPending) copyFrame();
    rawInput.setMode(host.inputMode);
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
    // like the headless runners and the worker: frames aren't shown (begin_frame returns 0)
    host.showFrame = false;
    try { for (let i = 0; i < n; i++) await host.frameAsync(); } catch (e) { if (!(e instanceof ProcExit)) throw e; }
    s = { frames: host.frameIndex, presented: host.framesPresented, width: host.width, height: host.height,
          videoHash: host.videoHash, audioHash: host.audioHash, audioFrames: host.audioFrames };
  }
  const out = `frames=${s.frames} presented=${s.presented} size=${s.width}x${s.height} ` +
              `video_fnv32=${hex(s.videoHash)} audio_fnv32=${hex(s.audioHash)} audio_frames=${s.audioFrames}`;
  globalThis.__gasmResult = out;
  log(`hash ${worker ? '(worker)' : '(main)'}: ${out}`);
}

// Saves of games loaded from a URL (?wasm=) get a namespace of their own, so a
// module from elsewhere can't read or overwrite the saves of a game here.
async function namespaceFor(game) {
  const name = game.split('/').pop().replace(/\.wasm$/, '').replace(/[^A-Za-z0-9._-]/g, '_') || 'game';
  if (!game.includes('/')) return name;
  const url = new URL(game, location.href);
  const digest = new Uint8Array(await crypto.subtle.digest('SHA-256', new TextEncoder().encode(url.origin + url.pathname)));
  return `url-${[...digest.slice(0, 8)].map((b) => b.toString(16).padStart(2, '0')).join('')}-${name}`;
}

// The gasm splash screen (lib/splash.js, the same frames as gasm-run's) while the game
// loads: it holds on the logo until finish() is called, and a key or click shortens it.
// ?nosplash (and hash runs) skip it.
function showSplash(stale) {
  freshCanvas();
  canvas.style.setProperty('--ar', SPLASH_W / SPLASH_H);
  const draw = (rgba) => {
    if (useGl()) {
      presenter ??= GlPresenter.create(canvas);
      if (presenter) return presenter.draw(rgba, SPLASH_W, SPLASH_H, canvasSize(), { ...view, aspect: null });
    }
    ctx ??= canvas.getContext('2d');
    if (canvas.width !== SPLASH_W) { canvas.width = SPLASH_W; canvas.height = SPLASH_H; }
    ctx.putImageData(new ImageData(rgba, SPLASH_W, SPLASH_H), 0, 0);
  };
  // the package's player (gasm-splash.js), drawn with the view's filter; ends early if
  // another game is picked meanwhile
  const s = playSplash((rgba) => (stale() ? s.cancel() : draw(rgba)));
  return s;
}

async function start({ romBytes, romName = null } = {}) {
  const gen = ++startGen;
  const stale = () => gen !== startGen;   // a newer start() took over
  await stopGame();
  $('pause').disabled = false; $('pause').textContent = '❚❚ pause';
  await initAudio();
  audioCtx?.resume();
  const url = new URLSearchParams(location.search);
  const game = url.get('wasm') ?? $('game').value;   // ?wasm=<url> runs any module
  const hashFrames = Number(url.get('hashframes') || 0);
  const splash = hashFrames > 0 || url.has('nosplash') ? null : showSplash(stale);
  const record = {};
  if (CONTENT[game] && !folder && !url.has('opfs')) {
    try {
      romBytes ??= await fetchBytes(new URL(`roms/${url.get('rom') ?? $('rom').value}`, ROOT));
    } catch (e) { splash?.ready(); return log(`${e.message}: open a ${CONTENT[game].ext} file instead`); }
    record[CONTENT[game].asset] = romBytes;
  }
  const godot = GODOT_GAMES[game];
  if (godot) {
    try {
      record['game.pck'] = await fetchBytes(new URL(`build/${godot.pck}`, ROOT));
    } catch (e) { splash?.ready(); return log(`${e.message}: build the Godot examples (make godot)`); }
  }
  const fg = FOLDER_GAMES[game];
  if (fg && !folder && !url.has('opfs')) {
    try {
      for (const f of fg.files) record[f] = await fetchBytes(new URL(`roms/${fg.dir}${f}`, ROOT));
    } catch (e) { splash?.ready(); return log(`${e.message}: open a folder with a game instead`); }
  }
  // Launch parameters: URL query plus the relay/room fields.
  const skip = ['game', 'autostart', 'wasm', 'worker', 'opfs', 'prefix', 'hashframes', 'rom', 'filter', 'integer', 'asyncify', 'nosplash', 'copykey', 'allownet'];
  const params = Object.fromEntries([...url].filter(([k]) => !skip.includes(k)));
  if (fg && params.args === undefined) params.args = folder || url.has('opfs') ? fg.folderArgs : fg.args;
  if ($('relay').value.trim()) { params.relay = $('relay').value.trim(); params.room = $('room').value.trim() || 'sumo'; }
  try {
    const bytes = godot ? await fetchBytes(new URL(`build/${godot.engine}`, ROOT))
      : game.includes('/') ? await fetchBytes(new URL(game, location.href)) : await fetchGame(game);
    // compiled once: the imports tell where it can run, the same Module is instantiated
    const module = await WebAssembly.compile(bytes);
    if (stale()) return;
    const usesGfx = WebAssembly.Module.imports(module).some((i) => i.module === 'gasm:gfx');
    const usesGl = WebAssembly.Module.imports(module).some((i) => i.module === 'gasm:gl');
    gameName = staticTitle(module) ?? game.split('/').pop().replace(/\.wasm$/, '');
    showTitle(null);
    // before the game starts: the manifest's hosts (one question) and mods that want hosts
    runHosts = [];
    let manifest = null;
    try {
      const text = record['gasm.manifest'] ? new TextDecoder().decode(record['gasm.manifest']) : moduleManifest(module);
      if (text !== null) manifest = parseManifest(text);
    } catch (e) { splash?.ready(); return log(`the game's manifest: ${e.message}`); }
    const allowed = allowedHosts();
    const policy = new NetPolicy(allowed);
    const unasked = (manifest?.hosts ?? []).filter((h) => !policy.permits(h));
    if (unasked.length && await askPlayer(`net:${unasked.join(',')}`)) runHosts.push(...unasked);
    if (stale()) return;
    if (mods && GODOT_GAMES[game]) await mountMods(record, policy);
    if (stale()) return;
    // gfx guests go to the worker only if the canvas can be transferred (OffscreenCanvas);
    // the worker then needs WebGPU too, otherwise we fall back to the main thread below.
    const offscreenOk = typeof HTMLCanvasElement !== 'undefined' && 'transferControlToOffscreen' in HTMLCanvasElement.prototype;
    let useWorker = ($('worker').checked || url.has('opfs')) && (!usesGfx || offscreenOk) && !usesGl;
    await splash?.ready();   // the game gets a canvas of its own once the splash is over
    if (stale()) return;
    let c = freshCanvas();
    c.classList.toggle('gpu', usesGfx || usesGl);
    gpu = null;
    // saves: one namespace per game, chosen here, never by the game (a Godot pack you open
    // is its own game: its file name, as gasm-run does with --asset game.pck=mygame.pck)
    const packName = game === 'godot.wasm' && romName ? romName.replace(/\.[^.]*$/, '').replace(/[^A-Za-z0-9._-]/g, '_') : '';
    const namespace = packName || await namespaceFor(game);
    const prefix = url.get('prefix') ?? '';
    if (useWorker) {
      // Lazy sources: the worker reads OPFS / picked files synchronously on demand.
      const specs = [{ kind: 'memory', record }];
      if (url.has('opfs')) specs.push({ kind: 'opfs', dir: url.get('opfs'), prefix });
      if (folder) specs.push({ kind: 'files', entries: folder.entries, prefix });
      const start = (canvasOpt) => GasmWorker.start({
        wasm: module, assets: specs, params, storage: namespace, allowNet: allowedHosts(), ask: askPlayer, keyboard: true,
        hashing: hashFrames > 0, virtualTime: hashFrames > 0, onLog: log, onAudio, onTitle: showTitle, onCopyText: copyText, onSaveFile: saveFile, ...canvasOpt,
      });
      if (usesGfx) {
        const size = canvasSize();
        try {
          worker = await start({ canvas: c.transferControlToOffscreen(), size });
        } catch (e) {
          if (!/WebGPU/.test(e.message)) throw e;
          log(`${e.message}: running on the main thread`);
          useWorker = false;
          c = freshCanvas(); c.classList.add('gpu');
        }
      } else {
        worker = await start({});
      }
    }
    if (!useWorker) {
      if (usesGfx && $('worker').checked && !offscreenOk) log('gasm:gfx games run on the main thread here (no OffscreenCanvas)');
      if (usesGl && $('worker').checked) log('gasm:gl games run on the main thread');
      let assets = record;
      if (folder) {  // main thread: preload the folder into memory, with progress
        const t = new AssetTable(log);
        for (const [n, b] of Object.entries(record)) t.add(n, bytesSource(b));
        t.merge(await preloadAssets(folder.entries, { prefix, log,
          onProgress: (p) => log(`loading ${folder.name}: ${p.done}/${p.total} files, ${(p.bytes / 1048576).toFixed(1)} MB`) }));
        assets = t;
      }
      const gfx = usesGfx ? await WebGpuGfx.create(c, log) : undefined;
      // gasm:gl: a WebGL 2 context on the canvas; its drawing buffer follows the display size
      const gl = usesGl ? c.getContext('webgl2', { alpha: false, antialias: false, depth: true, stencil: true }) : null;
      if (usesGl && !gl) throw new Error('this game needs WebGL 2 (gasm:gl)');
      if (gl) fitGl(c);
      gpu = gfx ?? (gl && { gl, device: { destroy: () => gl.getExtension('WEBGL_lose_context')?.loseContext() } }) ?? null;
      const storage = await IdbStorage.open(namespace).catch((e) => {
        log(`storage unavailable (${e.message}); saves won't persist`);
        return new MemoryStorage();
      });
      const h = new GasmHost({ assets, params, gfx, gl, storage, allowNet: allowedHosts(), ask: askPlayer, onPresent: present, onAudio, onLog: log, onTitle: showTitle, onCopyText: copyText, onSaveFile: saveFile,
                               virtualTime: hashFrames > 0 });
      h.text = '';     // the page has a keyboard
      if (stale()) { h.shutdown(); gfx?.device.destroy(); return; }
      host = h;
      await host.load(module, { bytes });
    }
    if (stale()) return stopGame();
  } catch (e) {
    splash?.ready();   // a load that failed: the splash ends (and the log says why)
    if (e instanceof ProcExit) log(`game exited during init (code ${e.code})`);
    else { log(`load failed: ${e.message}`); console.error(e); }
    return;
  }
  if (hashFrames > 0) return hashRun(hashFrames).catch((e) => log(`hash run failed: ${e.message}`));
  const how = [worker && 'in a worker', (worker ?? host).switching && 'stack switching'].filter(Boolean).join(', ');
  log(`running ${game}${how ? ` (${how})` : ''} @ ${(worker ?? host).frameRate.toFixed(2)} Hz`);
  acc = 0; last = performance.now(); running = true;
  rafId = requestAnimationFrame(tick);
}

/** build/<game>, or its run build where the browser switches stacks (falls back if missing). */
async function fetchGame(game) {
  if (STACK_SWITCHING && RUN_BUILDS.has(game) && !new URLSearchParams(location.search).has('asyncify')) {
    try { return await fetchBytes(new URL(`build/${game.replace(/\.wasm$/, '-run.wasm')}`, ROOT)); } catch {}
  }
  return fetchBytes(new URL(`build/${game}`, ROOT));
}

async function fetchBytes(url) {
  const r = await fetch(url);
  if (!r.ok) throw new Error(`${url.pathname}: HTTP ${r.status}`);
  return new Uint8Array(await r.arrayBuffer());
}

// ---- UI ----------------------------------------------------------------------
for (const [file, label] of Object.entries(GAMES)) $('game').add(new Option(label, file));
let romNames = [];   // roms/ directory listing (development server only)
function fillRoms() {
  const c = CONTENT[$('game').value];
  $('rom').replaceChildren();
  if (!c) return;
  const names = [...new Set([...romNames.filter((n) => n.toLowerCase().endsWith(c.ext)), ...c.known])];
  for (const n of names) $('rom').add(new Option(n, n));
  const preferred = names.find((n) => n.startsWith(c.preferred));
  if (preferred) $('rom').value = preferred;
  $('romlabel').firstChild.textContent = `open ${c.ext}…`;
}
$('game').onchange = () => {
  const g = $('game').value;
  $('rom').hidden = $('romlabel').hidden = !CONTENT[g];
  $('netfields').hidden = g !== 'sumo.wasm';
  fillRoms();
};
$('relay').value = new URLSearchParams(location.search).get('relay') ?? '';
$('room').value = new URLSearchParams(location.search).get('room') ?? 'sumo';
$('relay').placeholder = `ws://${location.hostname || 'localhost'}:9000  (empty = vs. bot)`;

// roms/ is listed via the dev server's directory index (python -m http.server)
fetch(new URL('roms/', ROOT)).then((r) => (r.ok ? r.text() : '')).then((html) => {
  romNames = [...html.matchAll(/href="([^"]+\.(?:nes|wad))"/gi)].map((m) => decodeURIComponent(m[1]));
  fillRoms();
}).catch(() => {});

$('start').onclick = () => start();
$('pause').onclick = () => {
  if (!host && !worker) return;   // nothing (alive) to pause or resume
  running = !running;
  last = performance.now();
  $('pause').textContent = running ? '❚❚ pause' : '▶ resume';
  if (running) rafId = requestAnimationFrame(tick);
};
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
// Mods for Godot games (as gasm-run --mods): a picked folder's *.pck / *.zip, mounted as
// mods/<name> in name order; one with a manifest (<stem>.json) that wants hosts is the
// player's choice; refused ones go to asset mods.refused.
let mods = null;   // [{ name, file, manifest: File | null }]
$('mods').onclick = () => $('modsinput').click();
$('modsinput').onchange = (e) => {
  // the folder's own files (picked files, without a folder, count too)
  const top = [...e.target.files].filter((f) => !f.webkitRelativePath || f.webkitRelativePath.split('/').length === 2);
  const byName = new Map(top.map((f) => [f.name, f]));
  mods = top.filter((f) => !f.name.startsWith('.') && /\.(pck|zip)$/i.test(f.name)).sort((a, b) => (a.name < b.name ? -1 : a.name > b.name ? 1 : 0))
    .map((f) => ({ name: f.name, file: f, manifest: byName.get(`${f.name.replace(/\.[^.]*$/, '')}.json`) ?? null }));
  $('mods').textContent = mods.length ? `mods (${mods.length})` : 'mods…';
  log(`${mods.length} mod${mods.length === 1 ? '' : 's'}: ${mods.map((m) => m.name).join(', ') || 'none'} (used by Godot games from the next start)`);
};
async function mountMods(record, policy) {
  const refused = [];
  for (const m of mods) {
    let why = null;
    if (m.manifest) {
      try {
        const man = parseManifest(await m.manifest.text());
        const open = man.hosts.filter((h) => !policy.permits(h));
        if (open.length) {
          if (await askPlayer(`mod:${m.name}`, man.hosts.join(', '))) runHosts.push(...open);
          else why = `it connects to ${man.hosts.join(', ')} and the player said no`;
        }
      } catch (e) { why = `its manifest ${m.manifest.name}: ${e.message}`; }
    }
    if (why) { refused.push(`${m.name}\t${why}\n`); log(`mods: refused ${m.name} (${why})`); continue; }
    record[`mods/${m.name}`] = new Uint8Array(await m.file.arrayBuffer());
  }
  if (refused.length) record['mods.refused'] = new TextEncoder().encode(refused.join(''));
  log(`mods: ${mods.length - refused.length} of ${mods.length} mounted`);
}
$('folderinput').onchange = (e) => {
  const entries = fileListEntries(e.target.files);
  const root = e.target.files[0]?.webkitRelativePath.split('/')[0] ?? 'folder';
  folder = { name: root, entries };
  folderChosen();
};
$('worker').checked = new URLSearchParams(location.search).has('worker');
for (const f of FILTERS) $('filter').add(new Option(f, f));
$('filter').value = view.filter;
$('integer').checked = view.integerScale;
$('filter').onchange = () => { view.filter = $('filter').value; viewChanged(); };
$('integer').onchange = () => { view.integerScale = $('integer').checked; viewChanged(); };

// An opened or dropped content file picks its game by extension (.nes, .wad).
async function openContent(f) {
  const g = contentGame(f.name);
  if (!g) return log(`${f.name}: expected a ${Object.values(CONTENT).map((c) => c.ext).join(' or ')} file`);
  if ($('game').value !== g) { $('game').value = g; $('game').onchange(); }
  start({ romBytes: new Uint8Array(await f.arrayBuffer()), romName: f.name });
}
$('romfile').onchange = (e) => { if (e.target.files[0]) openContent(e.target.files[0]); };
const stage = $('stage');
stage.ondragover = (e) => { e.preventDefault(); stage.classList.add('drag'); };
stage.ondragleave = () => stage.classList.remove('drag');
stage.ondrop = async (e) => {
  e.preventDefault(); stage.classList.remove('drag');
  if (e.dataTransfer.files[0]) openContent(e.dataTransfer.files[0]);
};

const params = new URLSearchParams(location.search);
if (params.get('game') in GAMES) $('game').value = params.get('game');
$('game').onchange();
if (params.has('autostart')) start();
// Leaving the page: let the game flush its saves (gasm_exit), close its connections.
addEventListener('pagehide', () => { stopGame(); });
