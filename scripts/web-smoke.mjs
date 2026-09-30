#!/usr/bin/env node
// Smoke-test the browser runner in headless Chrome via the DevTools protocol:
// load the page with ?autostart, let it run, report status + fps, save a screenshot.
//   node scripts/web-smoke.mjs <url> <out.png> [seconds] [keys]
import { spawn } from 'node:child_process';
import { writeFileSync } from 'node:fs';

const [url, out, secs = '5'] = process.argv.slice(2);
const CHROME = process.env.CHROME ?? '/Applications/Google Chrome.app/Contents/MacOS/Google Chrome';
const port = 9333;
const chrome = spawn(CHROME, ['--headless=new', `--remote-debugging-port=${port}`, '--user-data-dir=/tmp/gasm-chrome',
  '--autoplay-policy=no-user-gesture-required', '--window-size=900,760', 'about:blank'], { stdio: 'ignore' });
const sleep = (ms) => new Promise((r) => setTimeout(r, ms));
try {
  let target;
  for (let i = 0; i < 50 && !target; i++) {
    await sleep(200);
    target = await fetch(`http://127.0.0.1:${port}/json`).then((r) => r.json()).then((t) => t.find((x) => x.type === 'page')).catch(() => null);
  }
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
  await send('Page.navigate', { url });
  await sleep(Number(secs) * 1000);
  const ev = await send('Runtime.evaluate', { expression: `document.getElementById('log').textContent + ' | ' + document.getElementById('fps').textContent`, returnByValue: true });
  console.log('status:', ev.result.result.value);
  const shot = await send('Page.captureScreenshot', { format: 'png' });
  writeFileSync(out, Buffer.from(shot.result.data, 'base64'));
  console.log('console:\n  ' + logs.join('\n  '));
  ws.close();
} finally { chrome.kill(); }
