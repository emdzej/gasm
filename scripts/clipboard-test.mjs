#!/usr/bin/env node
// The web player's copy key (F2) in headless Chrome: a 2D game (test-pattern), a gasm:gl game
// (gltest) and a gasm:gfx game (triangle) each put a PNG of their frame on the
// clipboard, with pixels that aren't all one colour. Skips without Chrome, and the
// gasm:gfx game without WebGPU (headless Chrome on Linux).
//   node scripts/clipboard-test.mjs     (serves the repo itself; needs build/*.wasm)
import { spawn } from 'node:child_process';
import { existsSync, mkdtempSync, rmSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';

const CHROME = process.env.CHROME ?? '/Applications/Google Chrome.app/Contents/MacOS/Google Chrome';
const ROOT = new URL('..', import.meta.url).pathname;
const port = 9341, httpPort = 8771;
const BASE = `http://localhost:${httpPort}/runners/web/`;
if (!existsSync(CHROME)) { console.log('SKIP  Chrome not found'); process.exit(0); }
const sleep = (ms) => new Promise((r) => setTimeout(r, ms));
const profile = mkdtempSync(join(tmpdir(), 'gasm-clipboard-'));
const args = ['--headless=new', `--remote-debugging-port=${port}`, `--user-data-dir=${profile}`, '--enable-unsafe-webgpu', '--window-size=900,760'];
if (process.env.CI) args.push('--no-sandbox');
const server = spawn('python3', ['-m', 'http.server', String(httpPort), '--bind', '127.0.0.1'], { cwd: ROOT, stdio: 'ignore' });
const chrome = spawn(CHROME, [...args, 'about:blank'], { stdio: 'ignore' });
let pass = 0, fail = 0;
try {
  let target;
  for (let i = 0; i < 300 && !target; i++) {
    await sleep(200);
    target = await fetch(`http://127.0.0.1:${port}/json`).then((r) => r.json()).then((t) => t.find((x) => x.type === 'page')).catch(() => null);
  }
  if (!target) throw new Error('Chrome did not open its DevTools port within 60 s');
  const browser = await fetch(`http://127.0.0.1:${port}/json/version`).then((r) => r.json());
  const connect = async (url) => {
    const ws = new WebSocket(url);
    await new Promise((r) => (ws.onopen = r));
    let id = 0; const pending = new Map();
    ws.onmessage = (e) => { const m = JSON.parse(e.data); if (m.id && pending.has(m.id)) { pending.get(m.id)(m); pending.delete(m.id); } };
    return { ws, send: (method, params = {}) => new Promise((r) => { const i = ++id; pending.set(i, r); ws.send(JSON.stringify({ id: i, method, params })); }) };
  };
  const b = await connect(browser.webSocketDebuggerUrl);
  await b.send('Browser.grantPermissions', { origin: new URL(BASE).origin, permissions: ['clipboardReadWrite', 'clipboardSanitizedWrite'] });
  const { ws, send } = await connect(target.webSocketDebuggerUrl);
  await send('Page.enable');
  await send('Network.setCacheDisabled', { cacheDisabled: true });
  await send('Emulation.setFocusEmulationEnabled', { enabled: true });   // the clipboard needs a focused page
  const evaluate = async (expression) => (await send('Runtime.evaluate', { expression, awaitPromise: true, returnByValue: true })).result.result?.value;
  for (const game of ['test-pattern.wasm', 'gltest.wasm', 'triangle.wasm']) {
    await send('Page.navigate', { url: `${BASE}?game=${game}&autostart&nosplash` });
    await sleep(4000);
    if (game === 'triangle.wasm' && !(await evaluate(`!!navigator.gpu && navigator.gpu.requestAdapter().then((a) => !!a)`))) {
      console.log(`SKIP  ${game}: no WebGPU here`);
      continue;
    }
    await evaluate(`navigator.clipboard.writeText('')`);
    await send('Input.dispatchKeyEvent', { type: 'keyDown', code: 'F2', key: 'F2', windowsVirtualKeyCode: 113 });
    await send('Input.dispatchKeyEvent', { type: 'keyUp', code: 'F2', key: 'F2', windowsVirtualKeyCode: 113 });
    await sleep(1500);
    // the clipboard's image: its size and how many distinct colours it has
    const got = await evaluate(`(async () => {
      const items = await navigator.clipboard.read();
      const item = items.find((i) => i.types.includes('image/png'));
      if (!item) return 'no image: ' + items.map((i) => i.types).join(',') + ' / log: ' + document.getElementById('log').textContent;
      const bmp = await createImageBitmap(await item.getType('image/png'));
      const c = new OffscreenCanvas(bmp.width, bmp.height), x = c.getContext('2d');
      x.drawImage(bmp, 0, 0);
      const d = x.getImageData(0, 0, bmp.width, bmp.height).data, colours = new Set();
      for (let i = 0; i < d.length && colours.size < 50; i += 4) colours.add(d[i] << 16 | d[i + 1] << 8 | d[i + 2]);
      return bmp.width + 'x' + bmp.height + ' ' + colours.size + ' colours';
    })()`);
    const ok = /^\d+x\d+ (\d+) colours$/.exec(got ?? '');
    if (ok && Number(ok[1]) > 1) { pass++; console.log(`PASS  ${game}: copied ${got}`); }
    else { fail++; console.log(`FAIL  ${game}: ${got}`); }
  }
  ws.close(); b.ws.close();
} finally {
  chrome.kill();
  server.kill();
  await sleep(300);
  rmSync(profile, { recursive: true, force: true });
}
console.log(`${pass} passed, ${fail} failed`);
process.exit(fail ? 1 : 0);
