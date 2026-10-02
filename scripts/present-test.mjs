#!/usr/bin/env node
// Upscaling filters: native (wgpu, runners/native/src/present.rs) vs browser
// (WebGL 2, runners/web/gasm-present.js) on the same frames (design/presentation.md).
//
// gasm-run renders the last frame of a headless run raw (--screenshot) and through
// each filter at several output sizes (--screenshot-filtered); headless Chrome
// draws the raw frame with GlPresenter at the same sizes and compares the pixels.
// GPUs round differently in the last bit, so a channel may differ by a little;
// a pixel that differs by more counts as wrong, and only a few may.
//
// The test pattern at GOLDEN_SIZE is also compared with tests/golden/present/
// (the same tolerance; GPUs differ slightly), so filter changes show up as
// failures: UPDATE_GOLDEN=1 re-records them after a change meant to alter output.
//
//   node scripts/present-test.mjs     (needs build/*.wasm and the release gasm-run;
//                                      the NES case needs roms/bladebuster.nes)
import { spawn, execFileSync } from 'node:child_process';
import { createServer } from 'node:http';
import { copyFileSync, existsSync, mkdirSync, mkdtempSync, readFileSync, rmSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { extname, join, normalize } from 'node:path';

const ROOT = new URL('..', import.meta.url).pathname;
const OUT = join(ROOT, 'build/present-test');
const NATIVE = join(ROOT, 'runners/native/target/release/gasm-run');
const CHROME = process.env.CHROME ?? '/Applications/Google Chrome.app/Contents/MacOS/Google Chrome';
const FILTERS = ['nearest', 'sharp', 'xbr', 'fsr', 'crt'];
// integer, non-integer, wide (bars at the sides), smaller than the frame
const SIZES = [[768, 720], [1000, 750], [1280, 720], [200, 150]];
const TOLERANCE = 3;          // per channel
const MAX_WRONG = 0.002;      // share of pixels beyond the tolerance
const GOLDEN = join(ROOT, 'tests/golden/present');
const GOLDEN_SIZE = '1000x750';
const GAMES = [
  { name: 'test-pattern', args: ['build/test-pattern.wasm', '--headless', '120'] },
  { name: 'nes', needs: 'roms/bladebuster.nes', args: ['build/nes.wasm', '--rom', 'roms/bladebuster.nes', '--headless', '400'] },
  // a display aspect (video_set_aspect): non-uniform scaling
  { name: 'aspect', args: ['build/inputtest.wasm', '--headless', '30', '--param', 'aspect=16:9'], aspect: [16, 9] },
];

rmSync(OUT, { recursive: true, force: true });
mkdirSync(OUT, { recursive: true });
const sleep = (ms) => new Promise((r) => setTimeout(r, ms));
const cases = [];
for (const g of GAMES) {
  if (g.needs && !existsSync(join(ROOT, g.needs))) { console.log(`SKIP  ${g.name} (no ${g.needs})`); continue; }
  for (const filter of FILTERS) {
    for (const [w, h] of SIZES) {
      for (const integer of filter === 'nearest' ? [false, true] : [false]) {
        const id = `${g.name}-${filter}${integer ? '-int' : ''}-${w}x${h}`;
        execFileSync(NATIVE, [...g.args, '--screenshot', `${OUT}/${g.name}-raw.png`, '--screenshot-filtered', `${OUT}/${id}.png`,
          '--window', `${w}x${h}`, '--filter', filter, ...(integer ? ['--integer-scale'] : [])], { cwd: ROOT, stdio: 'ignore' });
        if (!existsSync(`${OUT}/${id}.png`)) { console.log('SKIP  no GPU for gasm-run --screenshot-filtered'); process.exit(0); }
        const golden = g.name === 'test-pattern' && `${w}x${h}` === GOLDEN_SIZE ? `${id}.png` : null;
        if (golden && process.env.UPDATE_GOLDEN) { mkdirSync(GOLDEN, { recursive: true }); copyFileSync(`${OUT}/${id}.png`, `${GOLDEN}/${golden}`); }
        cases.push({ id, raw: `${g.name}-raw.png`, filter, integer, aspect: g.aspect ?? null, size: [w, h], golden: golden && existsSync(`${GOLDEN}/${golden}`) ? golden : null });
      }
    }
  }
}

// a static server for the repo (the page imports gasm-present.js, fetches the PNGs)
const TYPES = { '.js': 'text/javascript', '.png': 'image/png', '.html': 'text/html' };
const server = createServer((req, res) => {
  const path = normalize(decodeURIComponent(new URL(req.url, 'http://x').pathname)).replace(/^\/+/, '');
  if (!path || path === '.') { res.writeHead(200, { 'content-type': 'text/html' }).end('<!doctype html><title>present-test</title>'); return; }
  if (path.startsWith('..') || !existsSync(join(ROOT, path))) { res.writeHead(404).end(); return; }
  res.writeHead(200, { 'content-type': TYPES[extname(path)] ?? 'application/octet-stream' }).end(readFileSync(join(ROOT, path)));
}).listen(0);
const port = server.address().port;
const profile = mkdtempSync(join(tmpdir(), 'gasm-present-chrome-'));
const chromeArgs = ['--headless=new', '--remote-debugging-port=9335', `--user-data-dir=${profile}`, '--enable-unsafe-swiftshader'];
if (process.env.CI) chromeArgs.push('--no-sandbox');
const chrome = spawn(CHROME, [...chromeArgs, 'about:blank'], { stdio: 'ignore' });

// Runs in the page: decode PNGs exactly (no colour conversion), draw, compare.
async function compare(cases, tolerance) {
  const { GlPresenter } = await import('/runners/web/gasm-present.js');
  const decode = async (url) => {
    const bmp = await createImageBitmap(await (await fetch(url)).blob(), { colorSpaceConversion: 'none', premultiplyAlpha: 'none' });
    const c = new OffscreenCanvas(bmp.width, bmp.height).getContext('2d', { colorSpace: 'srgb' });
    c.drawImage(bmp, 0, 0);
    return { w: bmp.width, h: bmp.height, px: c.getImageData(0, 0, bmp.width, bmp.height).data };
  };
  const diff = (a, b) => {
    let wrong = 0, maxDiff = 0;
    for (let i = 0; i < a.length; i += 4) {
      const d = Math.max(Math.abs(a[i] - b[i]), Math.abs(a[i + 1] - b[i + 1]), Math.abs(a[i + 2] - b[i + 2]));
      maxDiff = Math.max(maxDiff, d);
      if (d > tolerance) wrong++;
    }
    return { wrong: wrong / (a.length / 4), maxDiff };
  };
  const results = [];
  for (const k of cases) {
    const raw = await decode(`/build/present-test/${k.raw}`), want = await decode(`/build/present-test/${k.id}.png`);
    const canvas = document.createElement('canvas');
    const p = GlPresenter.create(canvas);
    p.draw(raw.px, raw.w, raw.h, k.size, { filter: k.filter, integerScale: k.integer, aspect: k.aspect });
    const sizeOk = want.w === k.size[0] && want.h === k.size[1];
    results.push({ id: k.id, sizeOk, ...diff(p.read(), want.px) });
    if (k.golden) {
      const gold = await decode(`/tests/golden/present/${k.golden}`);
      results.push({ id: `${k.id} golden`, sizeOk: sizeOk && gold.w === want.w && gold.h === want.h, ...(sizeOk ? diff(want.px, gold.px) : {}) });
    }
  }
  return results;
}

let failed = 0, checked = 0;
try {
  let target;
  for (let i = 0; i < 300 && !target; i++) {
    await sleep(200);
    target = await fetch('http://127.0.0.1:9335/json').then((r) => r.json()).then((t) => t.find((x) => x.type === 'page')).catch(() => null);
  }
  if (!target) throw new Error('Chrome did not open its DevTools port within 60 s');
  const ws = new WebSocket(target.webSocketDebuggerUrl);
  await new Promise((r) => (ws.onopen = r));
  let id = 0; const pending = new Map();
  ws.onmessage = (e) => { const m = JSON.parse(e.data); if (m.id && pending.has(m.id)) { pending.get(m.id)(m); pending.delete(m.id); } };
  const send = (method, params = {}) => new Promise((r) => { const i = ++id; pending.set(i, r); ws.send(JSON.stringify({ id: i, method, params })); });
  await send('Page.enable');
  await send('Page.navigate', { url: `http://127.0.0.1:${port}/` });
  await sleep(500);
  const r = await send('Runtime.evaluate', {
    expression: `(${compare})(${JSON.stringify(cases)}, ${TOLERANCE})`, awaitPromise: true, returnByValue: true,
  });
  if (r.result.exceptionDetails) throw new Error(r.result.exceptionDetails.exception?.description ?? r.result.exceptionDetails.text);
  checked = r.result.result.value.length;
  for (const x of r.result.result.value) {
    const ok = x.sizeOk && x.wrong <= MAX_WRONG;
    if (!ok) failed++;
    console.log(`${ok ? 'PASS' : 'FAIL'}  ${x.id.padEnd(32)} ${(x.wrong * 100).toFixed(3)}% beyond ±${TOLERANCE}, max diff ${x.maxDiff}`);
  }
  ws.close();
} finally {
  chrome.kill();
  server.close();
  await sleep(300);
  rmSync(profile, { recursive: true, force: true });
}
console.log(`${checked - failed} passed, ${failed} failed`);
process.exit(failed ? 1 : 0);
