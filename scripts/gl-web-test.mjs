#!/usr/bin/env node
// gasm:gl in Chrome on real WebGL 2: the player's hash run (?hashframes) of the GL
// determinism cases must give their golden hashes (tests/golden/determinism.txt), and
// WebGL must report no errors in the console. The hashes cover every upload and the
// GL errors the guests record, so an upload WebGL rejects (a float texture given as
// bytes, say) changes them.
//
//   node scripts/gl-web-test.mjs     (needs build/gltest.wasm, build/glowtest.wasm, build/sdl3-gl.wasm)
import { spawn } from 'node:child_process';
import { createServer } from 'node:http';
import { existsSync, mkdtempSync, readFileSync, rmSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { extname, join, normalize } from 'node:path';

const ROOT = new URL('..', import.meta.url).pathname;
const CHROME = process.env.CHROME ?? '/Applications/Google Chrome.app/Contents/MacOS/Google Chrome';
// cases of the determinism suite that run without scripted input: [name, game, frames]
const CASES = [['gltest', 'gltest.wasm', 120], ['glowtest', 'glowtest.wasm', 120], ['sdl3-gl', 'sdl3-gl.wasm', 120]];
const golden = Object.fromEntries(readFileSync(join(ROOT, 'tests/golden/determinism.txt'), 'utf8')
  .split('\n').filter(Boolean).map((l) => [l.split(' ')[0], l.slice(l.indexOf(' ') + 1).trim()]));

const TYPES = { '.js': 'text/javascript', '.mjs': 'text/javascript', '.html': 'text/html', '.css': 'text/css', '.wasm': 'application/wasm', '.json': 'application/json' };
const server = createServer((req, res) => {
  const path = normalize(decodeURIComponent(new URL(req.url, 'http://x').pathname)).replace(/^\/+/, '');
  const file = join(ROOT, path.endsWith('/') ? `${path}index.html` : path);
  if (path.startsWith('..') || !existsSync(file)) { res.writeHead(404).end(); return; }
  res.writeHead(200, { 'content-type': TYPES[extname(file)] ?? 'application/octet-stream' }).end(readFileSync(file));
}).listen(0);
const port = server.address().port;
const profile = mkdtempSync(join(tmpdir(), 'gasm-gl-chrome-'));
const chromeArgs = ['--headless=new', '--remote-debugging-port=9336', `--user-data-dir=${profile}`, '--enable-unsafe-swiftshader'];
if (process.env.CI) chromeArgs.push('--no-sandbox');
const chrome = spawn(CHROME, [...chromeArgs, 'about:blank'], { stdio: 'ignore' });
const sleep = (ms) => new Promise((r) => setTimeout(r, ms));

let failed = 0;
try {
  let target;
  for (let i = 0; i < 300 && !target; i++) {
    await sleep(200);
    target = await fetch('http://127.0.0.1:9336/json').then((r) => r.json()).then((t) => t.find((x) => x.type === 'page')).catch(() => null);
  }
  if (!target) throw new Error('Chrome did not open its DevTools port');
  const ws = new WebSocket(target.webSocketDebuggerUrl);
  await new Promise((r) => (ws.onopen = r));
  let id = 0; const pending = new Map(); let consoleLines = [];
  ws.onmessage = (e) => {
    const m = JSON.parse(e.data);
    if (m.id && pending.has(m.id)) { pending.get(m.id)(m); pending.delete(m.id); }
    if (m.method === 'Runtime.consoleAPICalled') consoleLines.push(m.params.args.map((a) => a.value ?? a.description).join(' '));
    // WebGL's own errors ("WebGL: INVALID_OPERATION: texImage2D: ...") arrive as log entries
    if (m.method === 'Log.entryAdded') consoleLines.push(m.params.entry.text);
  };
  const send = (method, params = {}) => new Promise((r) => { const i = ++id; pending.set(i, r); ws.send(JSON.stringify({ id: i, method, params })); });
  await send('Runtime.enable'); await send('Page.enable'); await send('Log.enable');
  for (const [name, game, frames] of CASES) {
    if (!existsSync(join(ROOT, 'build', game))) { console.log(`SKIP  ${name} (no build/${game})`); continue; }
    consoleLines = [];
    await send('Page.navigate', { url: `http://127.0.0.1:${port}/runners/web/?game=${game}&autostart&hashframes=${frames}` });
    let got = null;
    for (let i = 0; i < 300 && !got; i++) {
      await sleep(100);
      const r = await send('Runtime.evaluate', { expression: 'globalThis.__gasmResult ?? null', returnByValue: true });
      got = r.result?.result?.value ?? null;
    }
    const webgl = consoleLines.filter((l) => /WebGL: [A-Z_]+/.test(l));
    const want = golden[name];
    if (got && got.trim() === want && !webgl.length) console.log(`PASS  ${name} in Chrome: ${got.split(' ')[3]}`);
    else {
      failed++;
      console.log(`FAIL  ${name} in Chrome\n  got:    ${got ?? '(no result)'}\n  golden: ${want}`);
      for (const l of (got ? webgl : consoleLines).slice(0, 8)) console.log(`  ${l}`);
    }
  }
  ws.close();
} finally {
  chrome.kill();
  server.close();
  await sleep(300);
  rmSync(profile, { recursive: true, force: true });
}
process.exit(failed ? 1 : 0);
