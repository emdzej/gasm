#!/usr/bin/env node
// Headless Node runner: same host code as the browser runner, no canvas/audio.
// Output format matches `gasm-run --headless` so results can be diffed.
//
//   node runners/web/headless.mjs <game.wasm> [--rom p] [--asset n=p] [--asset-dir [prefix=]dir]
//        [--param k=v] [--allow-net]
//        [--realtime] --headless N [--input script] [--screenshot out.png] [--no-hash]
//
// A guest calling proc_exit (e.g. sumo's quit_at) ends the run early, cleanly.

import { closeSync, fstatSync, openSync, readFileSync, readSync, readdirSync, writeFileSync } from 'node:fs';
import { join } from 'node:path';
import { deflateSync } from 'node:zlib';
import { AssetTable, GasmHost, ProcExit, bytesSource } from './gasm-host.js';

const argv = process.argv.slice(2);
let wasm, frames = 600, screenshot, input = [], noHash = false, allowNet = false, realtime = false;
const BUTTONS = ['A', 'B', 'X', 'Y', 'L', 'R', 'SELECT', 'START', 'UP', 'DOWN', 'LEFT', 'RIGHT'];
// FROM-TO:BTN+BTN,... and FRAME:"text" (same syntax as gasm-run --input; commas
// inside quotes are literal; escapes \n enter, \b backspace, \\ and \")
const texts = [];
const parseInput = (spec) => {
  const items = [];
  let cur = '', quoted = false, escaped = false;
  for (const ch of spec) {
    if (escaped) { cur += ch; escaped = false; }
    else if (ch === '\\' && quoted) { cur += ch; escaped = true; }
    else if (ch === '"') { quoted = !quoted; cur += ch; }
    else if (ch === ',' && !quoted) { items.push(cur); cur = ''; }
    else cur += ch;
  }
  items.push(cur);
  const out = [];
  for (const item of items) {
    const i = item.indexOf(':');
    if (i < 0) throw new Error(`bad --input item ${item}`);
    const range = item.slice(0, i), rest = item.slice(i + 1);
    if (rest.startsWith('"')) {
      if (!rest.endsWith('"') || rest.length < 2) throw new Error(`bad --input item ${item}`);
      const text = rest.slice(1, -1).replace(/\\(.)/g, (_, c) => (c === 'n' ? '\n' : c === 'b' ? '\b' : c));
      texts.push({ frame: Number(range), text });
      continue;
    }
    const [from, to = from] = range.split('-').map(Number);
    const mask = rest.split('+').reduce((m, b) => {
      const bit = BUTTONS.indexOf(b.toUpperCase());
      if (bit < 0) throw new Error(`bad --input item ${item}`);
      return m | (1 << bit);
    }, 0);
    out.push({ from, to, mask });
  }
  return out;
};
const assets = {}, params = {}, assetDirs = [];
for (let i = 0; i < argv.length; i++) {
  const a = argv[i];
  if (a === '--rom') assets.rom = new Uint8Array(readFileSync(argv[++i]));
  else if (a === '--asset') { const [k, p] = argv[++i].split('='); assets[k] = new Uint8Array(readFileSync(p)); }
  else if (a === '--headless') frames = Number(argv[++i]);
  else if (a === '--screenshot') screenshot = argv[++i];
  else if (a === '--input') input = parseInput(argv[++i]);
  else if (a === '--no-hash') noHash = true;
  else if (a === '--asset-dir') {
    const v = argv[++i], k = v.indexOf('=');
    const p = k > 0 ? v.slice(0, k) : '';
    if (p && !/[\\/]/.test(p)) assetDirs.push([p, v.slice(k + 1)]); else assetDirs.push(['', v]);
  }
  else if (a === '--param') { const v = argv[++i], k = v.indexOf('='); params[v.slice(0, k)] = v.slice(k + 1); }
  else if (a === '--allow-net') allowNet = true;
  else if (a === '--realtime') realtime = true;
  else wasm = a;
}
if (!wasm) { console.error('usage: headless.mjs <game.wasm> [--rom path] --headless N [--screenshot out.png]'); process.exit(2); }

// Explicit assets stay in memory; --asset-dir folders are read lazily from disk
// (same naming/precedence/case rules as gasm-run, via AssetTable).
const table = new AssetTable((m) => console.error(m));
for (const [name, bytes] of Object.entries(assets)) table.add(name, bytesSource(bytes));
const fds = [];
for (const [prefix, dir] of assetDirs) {
  let n = 0;
  const walk = (abs, segs) => {
    for (const d of readdirSync(abs, { withFileTypes: true }).sort((a, b) => (a.name < b.name ? -1 : a.name > b.name ? 1 : 0))) {
      if (d.name.startsWith('.') || d.isSymbolicLink()) continue;
      const s = [...segs, d.name];
      if (d.isDirectory()) walk(join(abs, d.name), s);
      else if (d.isFile()) {
        const fd = openSync(join(abs, d.name), 'r');
        fds.push(fd);
        const name = prefix ? `${prefix.replace(/\/+$/, '')}/${s.join('/')}` : s.join('/');
        const added = table.add(name, {
          size: () => { try { return Math.min(fstatSync(fd).size, 0x7fffffff); } catch { return 0; } },
          readAt: (offset, dst) => { let done = 0; try { while (done < dst.length) { const k = readSync(fd, dst, done, dst.length - done, offset + done); if (!k) break; done += k; } } catch {} return done; },
        }, { fromDir: true });
        if (added) n++;
      }
    }
  };
  walk(dir, []);
  console.error(`[gasm-node] assets: ${n} files from ${dir}${prefix ? ` as ${prefix}/` : ''}`);
}
table.finish();

const host = new GasmHost({
  assets: table, params, allowNet, virtualTime: true, onLog: (m) => console.error(m),
  getPad: (p) => p !== 0 ? 0 : input.reduce((m, r) => host.frameIndex >= r.from && host.frameIndex <= r.to ? m | r.mask : m, 0),
});
host.hashing = !noHash;
const t0 = performance.now();
await host.load(readFileSync(wasm));
console.error(`[gasm-node] loaded ${wasm} in ${(performance.now() - t0).toFixed(0)} ms`);

// Yield to the event loop between frames when networking (WebSocket events are
// delivered there); --realtime also paces frames at the guest's rate.
const tick = () => new Promise((r) => setImmediate(r));
const t1 = performance.now();
let ran = 0, exitCode = null;
try {
  for (; ran < frames; ran++) {
    host.text = texts.filter((t) => t.frame === ran).map((t) => t.text).join('');
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

if (screenshot && host.width) {
  writeFileSync(screenshot, encodePng(host.rgba, host.width, host.height));
  console.error(`[gasm-node] wrote ${screenshot}`);
}
await host.net.closeAll(); // deliver queued messages before exiting
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
