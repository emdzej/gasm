#!/usr/bin/env node
// gasm:media in the web player (headless Chrome): build/mediatest.wasm plays the local
// https radio (scripts/media-server.mjs, a throwaway certificate) on an <audio> element,
// on the main thread and in Worker mode. Without CORS: element volume only; with CORS
// (?cors): Web Audio's gain and low-pass. Also a track's end, the player's no (the
// manifest's question) and a private address not listed. Skips without Chrome or openssl.
//   node scripts/media-web-test.mjs     (serves the repo itself; needs build/mediatest.wasm)
import { spawn, spawnSync } from 'node:child_process';
import { existsSync, mkdtempSync, rmSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';

const CHROME = process.env.CHROME ?? '/Applications/Google Chrome.app/Contents/MacOS/Google Chrome';
const ROOT = new URL('..', import.meta.url).pathname;
const port = 9351, httpPort = 8781, mediaPort = 8473;
const BASE = `http://localhost:${httpPort}/runners/web/`;
const RADIO = `https://127.0.0.1:${mediaPort}`;
if (!existsSync(CHROME)) { console.log('SKIP  media-web: Chrome not found'); process.exit(0); }
if (spawnSync('openssl', ['version']).status !== 0) { console.log('SKIP  media-web: no openssl'); process.exit(0); }
const sleep = (ms) => new Promise((r) => setTimeout(r, ms));
const tmp = mkdtempSync(join(tmpdir(), 'gasm-media-web-'));

// a self-signed certificate for 127.0.0.1 (Chrome is told to accept it)
writeFileSync(join(tmp, 'leaf.cnf'), '[req]\ndistinguished_name = dn\nprompt = no\nx509_extensions = v3\n[dn]\nCN = localhost\n[v3]\nsubjectAltName = DNS:localhost, IP:127.0.0.1\n');
spawnSync('openssl', ['req', '-x509', '-newkey', 'rsa:2048', '-nodes', '-days', '1', '-config', join(tmp, 'leaf.cnf'), '-keyout', join(tmp, 'leaf.key'), '-out', join(tmp, 'leaf.pem')], { stdio: 'ignore' });

const radio = spawn('node', [join(ROOT, 'scripts/media-server.mjs'), '--port', String(mediaPort), '--cert', join(tmp, 'leaf.pem'), '--key', join(tmp, 'leaf.key')], { stdio: 'ignore' });
const server = spawn('python3', ['-m', 'http.server', String(httpPort), '--bind', '127.0.0.1'], { cwd: ROOT, stdio: 'ignore' });
const profile = join(tmp, 'profile');
const args = ['--headless=new', `--remote-debugging-port=${port}`, `--user-data-dir=${profile}`, '--ignore-certificate-errors', '--autoplay-policy=no-user-gesture-required', '--window-size=900,760'];
if (process.env.CI) args.push('--no-sandbox');
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
  const ws = new WebSocket(target.webSocketDebuggerUrl);
  await new Promise((r) => (ws.onopen = r));
  let id = 0; const pending = new Map();
  ws.onmessage = (e) => {
    const m = JSON.parse(e.data);
    if (m.id && pending.has(m.id)) { pending.get(m.id)(m); pending.delete(m.id); }
    if (m.method === 'Runtime.consoleAPICalled') logs.push(m.params.args.map((a) => a.value ?? a.description).join(' '));
  };
  const send = (method, params = {}) => new Promise((r) => { const i = ++id; pending.set(i, r); ws.send(JSON.stringify({ id: i, method, params })); });
  const evaluate = async (expression) => (await send('Runtime.evaluate', { expression, awaitPromise: true, returnByValue: true })).result.result?.value;
  await send('Page.enable');
  await send('Runtime.enable');
  await send('Network.setCacheDisabled', { cacheDisabled: true });

  // play a case: the guest's "[mediatest]" lines after `ms`
  const play = async (query, ms, { answer = null } = {}) => {
    logs.length = 0;
    await send('Page.navigate', { url: `${BASE}?game=mediatest.wasm&autostart&nosplash&${query}` });
    if (answer) {
      for (let i = 0; i < 100 && !(await evaluate(`document.getElementById('consentdlg')?.open`)); i++) await sleep(100);
      await evaluate(`(() => { const d = document.getElementById('consentdlg'); d.close(${JSON.stringify(answer)}); return true; })()`);
    }
    await sleep(ms);
    if (process.env.MEDIA_DEBUG) console.log(logs.join('\n'), '\n#log:', await evaluate(`document.getElementById('log').textContent`));
    return logs.filter((l) => l.includes('[mediatest]'));
  };
  const has = (lines, re) => lines.some((l) => re.test(l));
  const url = (path) => `url=${encodeURIComponent(RADIO + path)}`;

  for (const mode of ['', '&worker']) {
    let lines = await play(`allowmedia=127.0.0.1&${url('/live.mp3')}${mode}`, 4000);
    check(`media-web${mode}: a stream without CORS plays (element volume)`, has(lines, /state Playing \(volume true, lowpass false, metadata false\)/), lines.join(' / ') || 'no log');
    lines = await play(`allowmedia=127.0.0.1&${url('/live.mp3?cors')}${mode}`, 6500);
    check(`media-web${mode}: with CORS it plays through Web Audio (low-pass)`, has(lines, /state Playing \(volume true, lowpass true, metadata false\)/), lines.join(' / ') || 'no log');
    check(`media-web${mode}: the position advances`, has(lines, /position [1-9]/), lines.join(' / ') || 'no log');
  }
  let lines = await play(`allowmedia=127.0.0.1&${url('/track.mp3?cors')}`, 5000);
  check('media-web: a track plays to its end', has(lines, /state Ended at [23]\.\d s/), lines.join(' / ') || 'no log');
  lines = await play(`url=${encodeURIComponent('https://radio.example/live.mp3')}`, 1500, { answer: 'never' });
  check("media-web: the player's no (the manifest's question)", has(lines, /state Failed: the player said no to internet audio/), lines.join(' / ') || 'no log');
  lines = await play(`allowmedia&${url('/live.mp3')}`, 1500);
  check('media-web: a private address not listed is refused', has(lines, /state Failed: 127\.0\.0\.1 is a private or local address/), lines.join(' / ') || 'no log');
} catch (e) {
  fail++;
  console.log(`FAIL  media-web: ${e.message}`);
} finally {
  chrome.kill();
  server.kill();
  radio.kill();
  await sleep(300);
  rmSync(tmp, { recursive: true, force: true });
}
console.log(`${pass} passed, ${fail} failed`);
process.exit(fail ? 1 : 0);
