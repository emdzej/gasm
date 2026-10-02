#!/usr/bin/env node
// Headless Node runner: same host code as the browser runner, no canvas/audio.
// Output format matches `gasm-run --headless` so results can be diffed.
//
//   node runners/web/headless.mjs <game.wasm> --headless N [--rom p] [--asset n=p]
//        [--asset-dir [prefix=]dir] [--param k=v] [--allow-net] [--storage-dir dir]
//        [--storage-id id] [--input script] [--screenshot out.png] [--realtime] [--no-hash]
//
// The options mean what they mean for gasm-run. --screenshot writes the last
// video_present frame: there is no GPU here, so gasm:gfx games can't be captured
// (use gasm-run --screenshot). A guest calling proc_exit ends the run early, cleanly.

import { closeSync, fstatSync, lstatSync, mkdirSync, openSync, readFileSync, readSync, readdirSync, renameSync, rmSync, writeFileSync } from 'node:fs';
import { join } from 'node:path';
import { deflateSync } from 'node:zlib';
import { AssetTable, GasmHost, MemoryStorage, ProcExit, bytesSource, validKey } from './gasm-host.js';
import { InputScript } from './input-script.mjs';

const USAGE = 'usage: headless.mjs <game.wasm> --headless N [--rom path] [--asset name=path] [--asset-dir [prefix=]dir] ' +
  '[--param k=v] [--allow-net] [--storage-dir dir] [--storage-id id] [--input script] [--screenshot out.png] [--realtime] [--no-hash]';
const fail = (msg) => { console.error(`error: ${msg}\n\n${USAGE}`); process.exit(2); };
const argv = process.argv.slice(2);
let wasm, frames = 600, screenshot, noHash = false, allowNet = false, realtime = false, storageDir = null, storageId = null;
// --input: see input-script.mjs (same syntax as gasm-run --input)
let script = new InputScript();
const assets = {}, params = {}, assetDirs = [];
const pair = (v, what) => { const k = v.indexOf('='); if (k <= 0) fail(`${what} expects name=value`); return [v.slice(0, k), v.slice(k + 1)]; };
for (let i = 0; i < argv.length; i++) {
  const a = argv[i];
  const val = () => { if (i + 1 >= argv.length) fail(`${a} needs a value`); return argv[++i]; };
  if (a === '--rom') assets.rom = new Uint8Array(readFileSync(val()));
  else if (a === '--asset') { const [k, p] = pair(val(), '--asset'); assets[k] = new Uint8Array(readFileSync(p)); }
  else if (a === '--headless') { frames = Number(val()); if (!Number.isInteger(frames) || frames < 0) fail('--headless expects a number'); }
  else if (a === '--screenshot') screenshot = val();
  else if (a === '--input') { try { script = new InputScript(val()); } catch (e) { fail(e.message); } }
  else if (a === '--no-hash') noHash = true;
  else if (a === '--asset-dir') {
    const v = val(), k = v.indexOf('=');
    const p = k > 0 ? v.slice(0, k) : '';
    if (p && !/[\\/]/.test(p)) assetDirs.push([p, v.slice(k + 1)]); else assetDirs.push(['', v]);
  }
  else if (a === '--param') { const [k, v] = pair(val(), '--param'); params[k] = v; }
  else if (a === '--allow-net') allowNet = true;
  else if (a === '--realtime') realtime = true;
  else if (a === '--storage-dir') storageDir = val();
  else if (a === '--storage-id') storageId = val();
  else if (a === '-h' || a === '--help') { console.error(USAGE); process.exit(0); }
  else if (a.startsWith('--')) fail(`unknown option ${a}`);
  else wasm = a;
}
if (!wasm) fail('missing <game.wasm>');
if (storageId !== null && !validKey(storageId)) fail(`invalid storage id ${JSON.stringify(storageId)} (use [A-Za-z0-9._-])`);

// Explicit assets stay in memory; --asset-dir folders are read lazily from disk
// (same naming/precedence/case rules as gasm-run, via AssetTable). Files are opened
// on first read and kept in a small cache, like gasm-run, so big trees don't run
// out of file descriptors.
const table = new AssetTable((m) => console.error(m));
for (const [name, bytes] of Object.entries(assets)) table.add(name, bytesSource(bytes));
const open = new Map();   // path -> fd, oldest first
const OPEN_FILES = 64;
const fdFor = (path) => {
  let fd = open.get(path);
  if (fd !== undefined) { open.delete(path); open.set(path, fd); return fd; }
  if (!lstatSync(path).isFile()) throw new Error(`${path}: no longer a regular file`);
  fd = openSync(path, 'r');
  if (open.size >= OPEN_FILES) { const [p, old] = open.entries().next().value; open.delete(p); closeSync(old); }
  open.set(path, fd);
  return fd;
};
for (const [prefix, dir] of assetDirs) {
  let n = 0;
  const walk = (abs, segs) => {
    for (const d of readdirSync(abs, { withFileTypes: true }).sort((a, b) => (a.name < b.name ? -1 : a.name > b.name ? 1 : 0))) {
      if (d.name.startsWith('.') || d.isSymbolicLink()) continue;
      const s = [...segs, d.name], path = join(abs, d.name);
      if (d.isDirectory()) walk(path, s);
      else if (d.isFile()) {
        const name = prefix ? `${prefix.replace(/\/+$/, '')}/${s.join('/')}` : s.join('/');
        const size = lstatSync(path).size;   // as found at start-up (like gasm-run)
        const added = table.add(name, {
          size: () => size,
          readAt: (offset, dst) => {
            let done = 0;
            try {
              const fd = fdFor(path);
              while (done < dst.length) { const k = readSync(fd, dst, done, dst.length - done, offset + done); if (!k) break; done += k; }
            } catch (e) { console.error(`[gasm-node] assets: ${e.message}`); }
            return done;
          },
        }, { fromDir: true });
        if (added) n++;
      }
    }
  };
  walk(dir, []);
  console.error(`[gasm-node] assets: ${n} files from ${dir}${prefix ? ` as ${prefix}/` : ''}`);
}
table.finish();

// --storage-dir: one file per key, written through a temp file (as gasm-run does).
function dirStorage(dir) {
  mkdirSync(dir, { recursive: true });
  const entries = [];
  for (const d of readdirSync(dir, { withFileTypes: true })) {
    if (d.name.endsWith('~tmp')) rmSync(join(dir, d.name), { force: true });
    else if (validKey(d.name) && d.isFile()) entries.push([d.name, new Uint8Array(readFileSync(join(dir, d.name)))]);
  }
  const s = new MemoryStorage(entries);
  s.persist = (op, k, v) => {
    if (op === 'put') { const tmp = join(dir, `.${k}~tmp`); writeFileSync(tmp, v); renameSync(tmp, join(dir, k)); }
    else rmSync(join(dir, k), { force: true });
  };
  return s;
}
const storage = storageDir ? dirStorage(storageDir) : new MemoryStorage();
console.error(`[gasm-node] storage: ${storageDir ?? 'memory'}`);

const host = new GasmHost({
  assets: table, params, allowNet, storage, virtualTime: true, onLog: (m) => console.error(m),
  getPad: (p) => (p !== 0 ? 0 : script.pad(host.frameIndex)),
});
host.hashing = !noHash;
const t0 = performance.now();
try {
  await host.load(readFileSync(wasm));
} catch (e) {
  if (e instanceof ProcExit) process.exit(e.code);   // exited during init
  throw e;
}
console.error(`[gasm-node] loaded ${wasm} in ${(performance.now() - t0).toFixed(0)} ms`);

// Yield to the event loop between frames when networking (WebSocket events are
// delivered there); --realtime also paces frames at the guest's rate.
const tick = () => new Promise((r) => setImmediate(r));
const t1 = performance.now();
let ran = 0, exitCode = null;
const scriptState = {};
try {
  for (; ran < frames; ran++) {
    host.text = script.textAt(ran);
    host.input = script.raw(ran, scriptState, [host.gfx.width(), host.gfx.height()], host.inputMode);
    host.frame();
    if (realtime) {
      const due = t1 + (ran + 1) * 1000 / host.frameRate;
      while (performance.now() < due) await new Promise((r) => setTimeout(r, Math.max(0, due - performance.now())));
    } else if (allowNet || (ran & 255) === 255) await tick();
  }
} catch (e) {
  if (!(e instanceof ProcExit)) throw e;
  exitCode = e.code;
  ran++;
}
if (exitCode === null) host.exit();
frames = ran;
const secs = (performance.now() - t1) / 1000;

const hex = (h) => h.toString(16).padStart(8, '0');
console.log(`frames=${frames} presented=${host.framesPresented} size=${host.width}x${host.height}`);
console.log(`video_fnv32=${hex(host.videoHash)} audio_fnv32=${hex(host.audioHash)} audio_frames=${host.audioFrames}`);
console.error(`[gasm-node] ${(frames / secs).toFixed(1)} guest frames/s (${(frames / secs / host.frameRate).toFixed(1)}x realtime at ${host.frameRate.toFixed(2)} Hz)`);

if (screenshot) {
  if (host.gfx.used) console.error('[gasm-node] no GPU in Node: gasm:gfx output is not captured (use gasm-run --screenshot)');
  if (host.width) {
    writeFileSync(screenshot, encodePng(host.rgba, host.width, host.height));
    console.error(`[gasm-node] wrote ${screenshot}`);
  }
}
await host.net.closeAll(); // deliver queued messages before exiting
for (const fd of open.values()) closeSync(fd);
if (exitCode !== null) {
  console.error(`[gasm-node] guest exited with code ${exitCode}`);
  process.exit(exitCode);
}
process.exit(0);

function encodePng(rgba, w, h) {
  const crcTable = Array.from({ length: 256 }, (_, n) => {
    let c = n;
    for (let k = 0; k < 8; k++) c = c & 1 ? 0xedb88320 ^ (c >>> 1) : c >>> 1;
    return c >>> 0;
  });
  const crc = (buf) => { let c = ~0; for (const b of buf) c = crcTable[(c ^ b) & 0xff] ^ (c >>> 8); return ~c >>> 0; };
  const chunk = (type, data) => {
    const out = Buffer.alloc(12 + data.length);
    out.writeUInt32BE(data.length, 0);
    out.write(type, 4, 'ascii');
    data.copy(out, 8);
    out.writeUInt32BE(crc(out.subarray(4, 8 + data.length)), 8 + data.length);
    return out;
  };
  const ihdr = Buffer.alloc(13);
  ihdr.writeUInt32BE(w, 0); ihdr.writeUInt32BE(h, 4);
  ihdr[8] = 8; ihdr[9] = 6; // 8-bit RGBA
  const raw = Buffer.alloc((w * 4 + 1) * h);
  for (let y = 0; y < h; y++) Buffer.from(rgba.buffer, rgba.byteOffset + y * w * 4, w * 4).copy(raw, y * (w * 4 + 1) + 1);
  return Buffer.concat([Buffer.from([137, 80, 78, 71, 13, 10, 26, 10]), chunk('IHDR', ihdr), chunk('IDAT', deflateSync(raw)), chunk('IEND', Buffer.alloc(0))]);
}
