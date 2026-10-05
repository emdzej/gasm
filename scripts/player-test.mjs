#!/usr/bin/env node
// The web player, the clipboard and saving files in headless Chrome. gasm:files: S in
// inputtest saves inputtest.txt, which must arrive as a download (main thread and Worker). gasm:clipboard: inputtest logs what
// Ctrl+V pastes and copies "copied by inputtest" on C. The copy key (F2): a 2D game (test-pattern), a gasm:gl game
// (gltest) and a gasm:gfx game (triangle) each put a PNG of their frame on the
// clipboard, with pixels that aren't all one colour. Skips without Chrome, and the
// gasm:gfx game without WebGPU (headless Chrome on Linux).
//   node scripts/player-test.mjs     (serves the repo itself; needs build/*.wasm)
import { spawn } from 'node:child_process';
import { existsSync, mkdtempSync, readFileSync, readdirSync, rmSync } from 'node:fs';
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
const logs = [];
const check = (name, ok, got) => { if (ok) { pass++; console.log(`PASS  ${name}`); } else { fail++; console.log(`FAIL  ${name}: ${got}`); } };
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
    ws.onmessage = (e) => {
      const m = JSON.parse(e.data);
      if (m.id && pending.has(m.id)) { pending.get(m.id)(m); pending.delete(m.id); }
      if (m.method === 'Runtime.consoleAPICalled') logs.push(m.params.args.map((a) => a.value ?? a.description).join(' '));
    };
    return { ws, send: (method, params = {}) => new Promise((r) => { const i = ++id; pending.set(i, r); ws.send(JSON.stringify({ id: i, method, params })); }) };
  };
  const b = await connect(browser.webSocketDebuggerUrl);
  await b.send('Browser.grantPermissions', { origin: new URL(BASE).origin, permissions: ['clipboardReadWrite', 'clipboardSanitizedWrite'] });
  const { ws, send } = await connect(target.webSocketDebuggerUrl);
  await send('Page.enable');
  await send('Network.setCacheDisabled', { cacheDisabled: true });
  await send('Emulation.setFocusEmulationEnabled', { enabled: true });   // the clipboard needs a focused page
  const evaluate = async (expression) => (await send('Runtime.evaluate', { expression, awaitPromise: true, returnByValue: true })).result.result?.value;
  await send('Runtime.enable');
  // after a navigation: until the player says the game runs (a cold Chrome takes a while), then a moment more
  const running = async (extra = 500) => {
    for (let i = 0; i < 100 && !/running/.test((await evaluate(`document.getElementById('log').textContent`)) ?? ''); i++) await sleep(100);
    await sleep(extra);
  };

  // @emdzej/gasm-host/splash on a page's own canvas: frames drawn, held on the logo
  // until ready(), then over
  await send('Page.navigate', { url: `${BASE}?game=inputtest.wasm&nosplash` });
  await sleep(1500);
  const splash = await evaluate(`(async () => {
    const { playSplash } = await import('./gasm-splash.js');
    const c = document.createElement('canvas');
    document.body.append(c);
    const s = playSplash(c, { skipOnInput: false });
    await new Promise((r) => setTimeout(r, 2500));   // past the logo frame: it holds
    const d = c.getContext('2d').getImageData(0, 0, c.width, c.height).data, colours = new Set();
    for (let i = 0; i < d.length; i += 4) colours.add(d[i] << 16 | d[i + 1] << 8 | d[i + 2]);
    let over = false;
    s.done.then(() => { over = true; });
    await new Promise((r) => setTimeout(r, 100));
    const held = !over;
    await Promise.race([s.ready(), new Promise((r) => setTimeout(r, 3000))]);
    return c.width + 'x' + c.height + ' ' + colours.size + ' colours, held ' + held + ', over ' + over;
  })()`);
  check(`gasm-splash.js: ${splash}`, /^320x180 \d+ colours, held true, over true$/.test(splash ?? '') && Number(/ (\d+) colours/.exec(splash)[1]) > 2, splash);

  // gasm:clipboard
  for (const mode of ['', '&worker']) {
    logs.length = 0;
    await send('Page.navigate', { url: `${BASE}?game=inputtest.wasm&autostart&nosplash${mode}` });
    await running();
    await evaluate(`navigator.clipboard.writeText('pasted from the page')`);
    const mod = process.platform === 'darwin' ? 4 : 2;   // Meta on macOS, else Control
    await send('Input.dispatchKeyEvent', { type: 'keyDown', code: 'KeyV', key: 'v', windowsVirtualKeyCode: 86, modifiers: mod, commands: ['paste'] });
    await send('Input.dispatchKeyEvent', { type: 'keyUp', code: 'KeyV', key: 'v', windowsVirtualKeyCode: 86, modifiers: mod });
    await sleep(500);
    check(`inputtest${mode}: Ctrl/Cmd+V pastes into the game`, logs.some((l) => l.includes('pasted "pasted from the page"')), logs.filter((l) => /past|KeyV/.test(l)).join(' / ') || 'no paste log');
    await send('Input.dispatchKeyEvent', { type: 'keyDown', code: 'KeyC', key: 'c', windowsVirtualKeyCode: 67, text: 'c' });
    await send('Input.dispatchKeyEvent', { type: 'keyUp', code: 'KeyC', key: 'c', windowsVirtualKeyCode: 67 });
    await sleep(500);
    const copied = await evaluate(`navigator.clipboard.readText()`);
    check(`inputtest${mode}: the game copies text`, copied === 'copied by inputtest', JSON.stringify(copied));
    const frames = logs.filter((l) => l.includes('pasted ')).length;
    check(`inputtest${mode}: pasted text lasts one frame`, frames === 1, `${frames} frames saw it`);
  }
  // gasm:files: a save is a download
  const downloads = join(profile, 'downloads');
  await b.send('Browser.setDownloadBehavior', { behavior: 'allow', downloadPath: downloads });
  for (const mode of ['', '&worker']) {
    logs.length = 0;
    rmSync(downloads, { recursive: true, force: true });
    await send('Page.navigate', { url: `${BASE}?game=inputtest.wasm&autostart&nosplash${mode}` });
    await running();
    await send('Input.dispatchKeyEvent', { type: 'keyDown', code: 'KeyS', key: 's', windowsVirtualKeyCode: 83, text: 's' });
    await send('Input.dispatchKeyEvent', { type: 'keyUp', code: 'KeyS', key: 's', windowsVirtualKeyCode: 83 });
    await sleep(1500);
    const got = existsSync(join(downloads, 'inputtest.txt')) ? readFileSync(join(downloads, 'inputtest.txt'), 'utf8') : `files: ${existsSync(downloads) ? readdirSync(downloads).join(',') : 'none'}`;
    check(`inputtest${mode}: a save is a download`, got === 'inputtest save 1\n' && logs.some((l) => l.includes('save: saved')), `${JSON.stringify(got)} / ${logs.filter((l) => /save/.test(l)).join(' / ')}`);
  }

  // player consent: fetchtest asks for this server's host; "never" is remembered, "forget
  // answers" clears it, "allow this time" lets the requests through
  const fetchUrl = `${BASE}?game=fetchtest.wasm&autostart&nosplash&base=${encodeURIComponent(`http://localhost:${httpPort}/api`)}`;
  const dialogOpen = () => evaluate(`document.getElementById('consentdlg').open`);
  const answer = (v) => evaluate(`(() => { const d = document.getElementById('consentdlg'); d.close(${JSON.stringify(v)}); return true; })()`);
  logs.length = 0;
  await send('Page.navigate', { url: fetchUrl });
  await sleep(3000);
  const asked = await dialogOpen();
  await answer('never');
  await sleep(800);
  check('consent: an unasked host opens the question', asked === true, `dialog open: ${asked}`);
  check('consent: "never" refuses the request', logs.some((l) => /get: not sent|denied/.test(l)) || logs.some((l) => /get: 0|failed/.test(l)), logs.filter((l) => /fetchtest|denied/.test(l)).slice(0, 4).join(' / '));
  await send('Page.navigate', { url: fetchUrl });
  await sleep(3000);
  check('consent: "never" is remembered', (await dialogOpen()) === false, 'asked again');
  await evaluate(`document.getElementById('forget').click()`);
  logs.length = 0;
  await send('Page.navigate', { url: fetchUrl });
  await sleep(3000);
  const again = await dialogOpen();
  await answer('once');
  await sleep(1500);
  check('consent: "forget answers" asks again, "this time" allows', again === true && logs.some((l) => /fetchtest\] get: 404/.test(l)), `asked: ${again} / ${logs.filter((l) => /fetchtest\] get/.test(l)).join(' / ')}`);

  // Godot: paste into the ui example's name field (Godot's shortcuts are Ctrl+ on gasm)
  if (existsSync(join(ROOT, 'build/godot/ui.pck'))) {
    logs.length = 0;
    await send('Page.navigate', { url: `${BASE}?game=godot-ui&autostart&nosplash` });
    await running(2000);
    await evaluate(`navigator.clipboard.writeText('pasted name')`);
    await send('Input.dispatchMouseEvent', { type: 'mousePressed', x: 250, y: 205, button: 'left', buttons: 1, clickCount: 1 });
    await send('Input.dispatchMouseEvent', { type: 'mouseReleased', x: 250, y: 205, button: 'left', clickCount: 1 });
    await sleep(300);
    await send('Input.dispatchKeyEvent', { type: 'keyDown', code: 'ControlLeft', key: 'Control', windowsVirtualKeyCode: 17, modifiers: 2 });
    await send('Input.dispatchKeyEvent', { type: 'keyDown', code: 'KeyV', key: 'v', windowsVirtualKeyCode: 86, modifiers: 2, commands: ['paste'] });
    await send('Input.dispatchKeyEvent', { type: 'keyUp', code: 'KeyV', key: 'v', windowsVirtualKeyCode: 86, modifiers: 2 });
    await send('Input.dispatchKeyEvent', { type: 'keyUp', code: 'ControlLeft', key: 'Control', windowsVirtualKeyCode: 17 });
    await sleep(800);
    check('godot-ui: Ctrl+V pastes into a LineEdit', logs.some((l) => l.includes('ui: name=pasted name')), logs.filter((l) => /ui:/.test(l)).join(' / ') || 'no ui log');
  } else console.log('SKIP  godot-ui (make godot)');
  for (const game of ['test-pattern.wasm', 'gltest.wasm', 'triangle.wasm']) {
    await send('Page.navigate', { url: `${BASE}?game=${game}&autostart&nosplash` });
    await running(1000);
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
    check(`${game}: F2 copied ${got}`, ok && Number(ok[1]) > 1, got);
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
