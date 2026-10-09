#!/usr/bin/env node
// Headless Node runner: same host code as the browser runner, no canvas/audio.
// Output format matches `gasm-run --headless` so results can be diffed.
//
//   node runners/web/headless.mjs <game.wasm> --headless N [--rom p] [--asset n=p]
//        [--asset-dir [prefix=]dir] [--param k=v] [--allow-net] [--storage-dir dir]
//        [--storage-id id] [--input script] [--screenshot out.png] [--realtime] [--no-hash]
//        [--no-stack-switching] [--watch-asset n=p] [--allow-net=hosts]
//        [--fetch-record dir] [--fetch-replay dir] [--memory-limit MiB] [--app-id text]
//        [--save-dir dir] [--no-save] [--mods dir] [--no-mods] [--manifest file] [--threads n]
//
// The options mean what they mean for gasm-run. --screenshot writes the last
// video_present frame: there is no GPU here, so gasm:gfx games can't be captured
// (use gasm-run --screenshot). A guest calling proc_exit ends the run early, cleanly.

import { closeSync, fstatSync, lstatSync, statSync, mkdirSync, openSync, readFileSync, readSync, readdirSync, renameSync, rmSync, writeFileSync } from 'node:fs';
import { join } from 'node:path';
import { deflateSync } from 'node:zlib';
import { AssetTable, GasmHost, MemoryStorage, NetPolicy, ProcExit, bytesSource, parseManifest, staticTitle, validKey } from './gasm-host.js';
import { InputScript } from './input-script.mjs';

const VERSION = JSON.parse(readFileSync(new URL('./package.json', import.meta.url), 'utf8')).version;

const USAGE = 'usage: headless.mjs <game.wasm> --headless N [--rom path] [--asset name=path] [--watch-asset name=path] [--asset-dir [prefix=]dir] ' +
  '[--param k=v] [--allow-net[=hosts]] [--fetch-record dir] [--fetch-replay dir] [--app-id text] [--save-dir dir] [--no-save] [--mods dir] [--no-mods] [--manifest file] [--storage-dir dir] [--storage-id id] [--input script] [--screenshot out.png] [--realtime] [--no-hash] [--threads n]';
const fileStamp = (p) => { try { const s = statSync(p); return `${s.mtimeMs}:${s.size}`; } catch { return null; } };
const fail = (msg) => { console.error(`error: ${msg}\n\n${USAGE}`); process.exit(2); };
const argv = process.argv.slice(2);
let wasm, frames = 600, screenshot, noHash = false, allowNet = false, realtime = false, storageDir = null, storageId = null;
let stackSwitching = true, fetchRecord = null, fetchReplay = null, memoryLimit, appId = null, saveDir = null, noSave = false, modsDir = null, noMods = false;
let threads = 0;   // worker threads a wasi-threads game may have (0, as gasm-run --headless)
// --input: see input-script.mjs (same syntax as gasm-run --input)
let script = new InputScript();
const assets = {}, params = {}, assetDirs = [], watches = [];
const pair = (v, what) => { const k = v.indexOf('='); if (k <= 0) fail(`${what} expects name=value`); return [v.slice(0, k), v.slice(k + 1)]; };
for (let i = 0; i < argv.length; i++) {
  const a = argv[i];
  const val = () => { if (i + 1 >= argv.length) fail(`${a} needs a value`); return argv[++i]; };
  if (a === '--rom') assets.rom = new Uint8Array(readFileSync(val()));
  else if (a === '--asset') { const [k, p] = pair(val(), '--asset'); assets[k] = new Uint8Array(readFileSync(p)); }
  else if (a === '--watch-asset') {
    // like gasm-run: re-read whenever the file changes (or appears), checked before each frame
    const [k, p] = pair(val(), '--watch-asset');
    watches.push({ name: k, path: p, stamp: fileStamp(p) });
    if (watches.at(-1).stamp) assets[k] = new Uint8Array(readFileSync(p));
  }
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
  else if (a.startsWith('--allow-net=')) {
    allowNet = a.slice('--allow-net='.length).split(',').map((h) => h.trim()).filter(Boolean);
    if (!allowNet.length) fail('--allow-net= expects host names');
  }
  else if (a === '--fetch-record') fetchRecord = val();
  else if (a === '--threads') { threads = Number(val()); if (!Number.isInteger(threads) || threads < 0 || threads > 256) fail('--threads expects 0 to 256'); }
  else if (a === '--memory-limit') { const mib = Number(val()); if (!Number.isInteger(mib) || mib < 0) fail('--memory-limit expects MiB'); memoryLimit = mib * 1048576; }
  else if (a === '--fetch-replay') fetchReplay = val();
  else if (a === '--realtime') realtime = true;
  else if (a === '--no-stack-switching') stackSwitching = false;
  else if (a === '--storage-dir') storageDir = val();
  else if (a === '--save-dir') saveDir = val();
  else if (a === '--no-save') noSave = true;
  else if (a === '--mods') modsDir = val();
  else if (a === '--no-mods') noMods = true;
  else if (a === '--manifest') assets['gasm.manifest'] = new Uint8Array(readFileSync(val()));   // as gasm-run --manifest
  else if (a === '--storage-id') storageId = val();
  else if (a === '--app-id') { appId = val(); if (!/^[\x20-\x7e]{1,256}$/.test(appId)) fail('--app-id expects 1 to 256 printable ASCII characters'); }
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
          share: () => ({ kind: 'file', path, size }),
        }, { fromDir: true });
        if (added) n++;
      }
    }
  };
  walk(dir, []);
  console.error(`[gasm-node] assets: ${n} files from ${dir}${prefix ? ` as ${prefix}/` : ''}`);
}
// --mods: the folder's resource packs (*.pck, *.zip, top level) as mods/<name>, read on
// demand, in name order; unreadable ones are refused (asset mods.refused), as gasm-run does
if (modsDir && noMods) console.error('[gasm] mods: off (--no-mods)');
else if (modsDir && !(statSync(modsDir, { throwIfNoEntry: false })?.isDirectory())) console.error(`[gasm] mods: none (${modsDir} isn't a folder)`);
else if (modsDir) {
  const mounted = [], refused = [];
  const names = readdirSync(modsDir).filter((n) => !n.startsWith('.') && /\.(pck|zip)$/i.test(n)).sort((a, b) => (a < b ? -1 : a > b ? 1 : 0));
  for (const name of names) {
    const path = join(modsDir, name);
    try {
      const st = lstatSync(path);
      if (!st.isFile()) throw new Error('not a regular file');
      closeSync(openSync(path, 'r'));
      const size = st.size;
      table.add(`mods/${name}`, {
        size: () => size,
        readAt: (offset, dst) => {
          const fd = fdFor(path);
          let done = 0;
          while (done < dst.length) { const k = readSync(fd, dst, done, dst.length - done, offset + done); if (!k) break; done += k; }
          return done;
        },
        share: () => ({ kind: 'file', path, size }),
      }, { fromDir: true });
      mounted.push(name);
    } catch (e) { refused.push([name, e.code === 'EACCES' ? 'Permission denied (os error 13)' : e.message]); }
  }
  // as gasm-run: what was mounted, the unreadable ones, then what the manifests decide
  console.error(`[gasm] mods: ${mounted.length} from ${modsDir}${mounted.length ? `: ${mounted.join(', ')}` : ''}`);
  for (const [n, w] of refused) console.error(`[gasm] mods: refused ${n} (${w})`);
  // each mod's manifest (<stem>.json): a mod that wants hosts needs --allow-net to cover
  // them (headless runs never ask); a broken manifest refuses the mod
  const policy = new NetPolicy(allowNet);
  const late = [];
  for (const name of mounted) {
    const stem = name.replace(/\.[^.]*$/, '');
    let text;
    try { text = readFileSync(join(modsDir, `${stem}.json`), 'utf8'); } catch { continue; }
    let why = null;
    try {
      const m = parseManifest(text);
      if (!m.hosts.length) continue;
      if (policy.allowed && m.hosts.every((h) => policy.permits(h))) { late.push(`[gasm] mods: ${name} may connect to ${m.hosts.join(', ')}`); continue; }
      why = `it connects to ${m.hosts.join(', ')}, which --allow-net doesn't cover`;
    } catch (e) { why = `its manifest ${stem}.json: ${e.message}`; }
    late.push(`[gasm] mods: refused ${name} (${why})`);
    table.remove?.(`mods/${name}`);
    refused.push([name, why]);
  }
  if (refused.length) table.add('mods.refused', bytesSource(new TextEncoder().encode(refused.map(([n, w]) => `${n}\t${w}\n`).join(''))));
  for (const l of late) console.error(l);
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

// gasm:fetch records, as gasm-run writes and reads them: <key>.json (status, headers) + <key>.body
const replay = fetchReplay && ((key) => {
  try {
    const meta = JSON.parse(readFileSync(join(fetchReplay, `${key}.json`), 'utf8'));
    return { status: meta.status, headers: meta.headers, body: new Uint8Array(readFileSync(join(fetchReplay, `${key}.body`))) };
  } catch { return null; }
});
const record = fetchRecord && ((key, r) => {
  mkdirSync(fetchRecord, { recursive: true });
  writeFileSync(join(fetchRecord, `${key}.body`), r.body);
  writeFileSync(join(fetchRecord, `${key}.json`), `${JSON.stringify({ headers: r.headers, method: r.method, status: r.status, url: r.url }, null, 2)}\n`);
});
// gasm:files: written to --save-dir (never overwriting: "name (2).ext", as gasm-run), else dropped
const saveFile = noSave ? null : !saveDir ? () => true : (name, _mime, bytes) => {
  mkdirSync(saveDir, { recursive: true });
  const dot = name.lastIndexOf('.'), [stem, ext] = dot > 0 ? [name.slice(0, dot), name.slice(dot)] : [name, ''];
  for (let n = 1; n < 1000; n++) {
    const path = join(saveDir, n === 1 ? name : `${stem} (${n})${ext}`);
    try { writeFileSync(path, bytes, { flag: 'wx' }); console.error(`[gasm] files: saved ${path}`); return true; }
    catch (e) { if (e.code !== 'EEXIST') throw e; }
  }
  throw new Error(`too many files named ${name}`);
};
const host = new GasmHost({
  assets: table, params, allowNet, storage, virtualTime: true, onLog: (m) => console.error(m), stackSwitching,
  fetchReplay: replay, fetchRecord: record, onSaveFile: saveFile, userAgent: [appId, `gasm-headless/${VERSION}`].filter(Boolean).join(' '), ...(memoryLimit !== undefined ? { memoryLimit } : {}),
  onTitle: (t) => console.error(`[gasm] title: ${t ?? '(default)'}`),
  getPad: (p) => (p !== 0 ? 0 : script.pad(host.frameIndex)), threads,
});
host.hashing = !noHash;
const t0 = performance.now();
try {
  const bytes = readFileSync(wasm);
  const module = await WebAssembly.compile(bytes);
  const builtIn = staticTitle(module);
  if (builtIn) console.error(`[gasm] title: ${builtIn} (gasm.title)`);
  await host.load(module, { bytes });
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
    for (const w of watches) {
      const now = fileStamp(w.path);
      if (now && (now !== w.stamp || table.version(w.name) < 0)) {
        w.stamp = now;
        try { host.setAsset(w.name, new Uint8Array(readFileSync(w.path))); } catch (e) { console.error(`[gasm-node] --watch-asset ${w.path}: ${e.message}`); }
      }
    }
    host.text = script.textAt(ran);
    host.input = script.raw(ran, scriptState, [host.gfx.width(), host.gfx.height()], host.inputMode);
    if (host.switching) await host.frameAsync(); else host.frame();
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
