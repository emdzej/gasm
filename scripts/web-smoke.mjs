#!/usr/bin/env node
// Smoke-test the browser runner in headless Chrome via the DevTools protocol:
// load the page with ?autostart, let it run, report status + fps, save a screenshot.
//   node scripts/web-smoke.mjs <url> <out.png> [seconds] [actions]
// actions (optional, ';'-separated, times in seconds after load):
//   T:keydown:CODE  T:keyup:CODE  T:move:X,Y  T:down:X,Y  T:up:X,Y  T:wheel:DX,DY
// e.g. "2:keydown:ShiftLeft;2:keydown:ArrowLeft;2.5:move:450,380;2.6:down:450,380"
// (X,Y are page CSS pixels; keys are KeyboardEvent.code names)
import { spawn } from 'node:child_process';
import { writeFileSync } from 'node:fs';

const [url, out, secs = '5', actions = ''] = process.argv.slice(2);
const CHROME = process.env.CHROME ?? '/Applications/Google Chrome.app/Contents/MacOS/Google Chrome';
const port = 9333;
const chrome = spawn(CHROME, ['--headless=new', `--remote-debugging-port=${port}`, '--user-data-dir=/tmp/gasm-chrome',
  '--autoplay-policy=no-user-gesture-required', '--window-size=900,760', 'about:blank'], { stdio: 'ignore' });
const sleep = (ms) => new Promise((r) => setTimeout(r, ms));
try {
  let target;
  // a cold Chrome on a CI machine can take a while to open its debugging port
  for (let i = 0; i < 300 && !target; i++) {
    await sleep(200);
    target = await fetch(`http://127.0.0.1:${port}/json`).then((r) => r.json()).then((t) => t.find((x) => x.type === 'page')).catch(() => null);
  }
  if (!target) throw new Error('Chrome did not open its DevTools port within 60 s');
  const ws = new WebSocket(target.webSocketDebuggerUrl);
  await new Promise((r) => (ws.onopen = r));
  let id = 0; const pending = new Map(); const logs = [];
  ws.onmessage = (e) => {
    const m = JSON.parse(e.data);
    if (m.id && pending.has(m.id)) { pending.get(m.id)(m); pending.delete(m.id); }
    if (m.method === 'Runtime.consoleAPICalled') logs.push(m.params.args.map((a) => a.value ?? a.description).join(' '));
    if (m.method === 'Runtime.exceptionThrown') logs.push('EXCEPTION ' + JSON.stringify(m.params.exceptionDetails.exception?.description ?? m.params.exceptionDetails.text));
  };
  const send = (method, params = {}) => new Promise((r) => { const i = ++id; pending.set(i, r); ws.send(JSON.stringify({ id: i, method, params })); });
  await send('Runtime.enable');
  await send('Page.enable');
  // the profile persists (/tmp/gasm-chrome): never test a cached app.js or .wasm
  await send('Network.enable');
  await send('Network.setCacheDisabled', { cacheDisabled: true });
  await send('Page.navigate', { url });
  const t0 = Date.now();
  const VK = { ShiftLeft: [16, 'Shift'], ShiftRight: [16, 'Shift'], ControlLeft: [17, 'Control'], AltLeft: [18, 'Alt'],
    ArrowLeft: [37, 'ArrowLeft'], ArrowUp: [38, 'ArrowUp'], ArrowRight: [39, 'ArrowRight'], ArrowDown: [40, 'ArrowDown'],
    Space: [32, ' '], Enter: [13, 'Enter'], Escape: [27, 'Escape'], Tab: [9, 'Tab'] };
  const key = (type, code) => {
    const [vk, k] = VK[code] ?? (code.startsWith('Key') ? [code.charCodeAt(3), code[3].toLowerCase()] : code.startsWith('Digit') ? [code.charCodeAt(5), code[5]] : [0, code]);
    return send('Input.dispatchKeyEvent', { type, code, key: k, windowsVirtualKeyCode: vk, ...(type === 'keyDown' && k.length === 1 ? { text: k } : {}) });
  };
  for (const a of actions.split(';').filter(Boolean).map((x) => x.split(':')).sort((x, y) => x[0] - y[0])) {
    const [at, what, arg] = a;
    const wait = Number(at) * 1000 - (Date.now() - t0);
    if (wait > 0) await sleep(wait);
    const [x, y] = (arg ?? '').split(',').map(Number);
    if (what === 'keydown') await key('keyDown', arg);
    else if (what === 'keyup') await key('keyUp', arg);
    else if (what === 'move') await send('Input.dispatchMouseEvent', { type: 'mouseMoved', x, y });
    else if (what === 'down') await send('Input.dispatchMouseEvent', { type: 'mousePressed', x, y, button: 'left', buttons: 1, clickCount: 1 });
    else if (what === 'up') await send('Input.dispatchMouseEvent', { type: 'mouseReleased', x, y, button: 'left', clickCount: 1 });
    else if (what === 'wheel') await send('Input.dispatchMouseEvent', { type: 'mouseWheel', x: 400, y: 400, deltaX: x, deltaY: y });
  }
  const rest = Number(secs) * 1000 - (Date.now() - t0);
  if (rest > 0) await sleep(rest);
  const ev = await send('Runtime.evaluate', { expression: `document.getElementById('log').textContent + ' | ' + document.getElementById('fps').textContent`, returnByValue: true });
  console.log('status:', ev.result.result.value);
  const shot = await send('Page.captureScreenshot', { format: 'png' });
  writeFileSync(out, Buffer.from(shot.result.data, 'base64'));
  console.log('console:\n  ' + logs.join('\n  '));
  ws.close();
} finally { chrome.kill(); }
