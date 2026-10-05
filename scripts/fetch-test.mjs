#!/usr/bin/env node
// gasm:fetch end to end (guests/fetchtest against scripts/fetch-server.mjs):
// - live requests natively and in Node give the same results (the guest's hash covers
//   statuses, headers and bodies; requests finish in any order, the hash doesn't care);
// - denied without --allow-net, and for a host that isn't on the --allow-net list;
// - --app-id: every request's User-Agent is "<app id> <runner>/<version>";
// - --fetch-record writes the same files on both runners, and replaying them gives the
//   recorded results (the determinism suite replays tests/fixtures/fetch);
// - in Chrome, through the player (skipped without Chrome).
//
//   node scripts/fetch-test.mjs     (needs build/fetchtest.wasm and the release gasm-run)
import { spawn, spawnSync } from 'node:child_process';
import { existsSync, mkdtempSync, readFileSync, readdirSync, rmSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';

const ROOT = new URL('..', import.meta.url).pathname;
const NATIVE = join(ROOT, 'runners/native/target/release/gasm-run');
const CHROME = process.env.CHROME ?? '/Applications/Google Chrome.app/Contents/MacOS/Google Chrome';
const sleep = (ms) => new Promise((r) => setTimeout(r, ms));
let pass = 0, fail = 0;
const check = (name, want, got) => {
  if (want !== undefined && want === got) { pass++; console.log(`PASS  ${name}`); }
  else { fail++; console.log(`FAIL  ${name}\n  want: ${want}\n  got:  ${got}`); }
};

const server = spawn('node', [join(ROOT, 'scripts/fetch-server.mjs'), '0'], { stdio: ['ignore', 'pipe', 'inherit'] });
const port = await new Promise((resolve) => server.stdout.on('data', (d) => { const m = /listening (\d+)/.exec(String(d)); if (m) resolve(Number(m[1])); }));
const base = `http://127.0.0.1:${port}/api`;
let agents = [];
server.stdout.on('data', (d) => { for (const m of String(d).matchAll(/^user-agent: (.*)$/gm)) agents.push(m[1]); });
const tmp = mkdtempSync(join(tmpdir(), 'gasm-fetch-test-'));
try {
  const runners = { native: [NATIVE], node: ['node', join(ROOT, 'runners/web/headless.mjs')] };
  // async: the server runs in this process's event loop
  const run = (runner, ...args) => new Promise((resolve) => {
    const [cmd, ...pre] = runners[runner];
    const p = spawn(cmd, [...pre, join(ROOT, 'build/fetchtest.wasm'), '--param', `base=${base}`, '--headless', '900', '--realtime', ...args], { stdio: ['ignore', 'pipe', 'pipe'] });
    let out = '';
    p.stdout.on('data', (d) => { out += d; });
    p.stderr.on('data', (d) => { out += d; });
    p.on('close', () => resolve(out));
  });
  const done = (out) => /\[fetchtest\] done: \d+ frames, hash=([0-9a-f]+)/.exec(out)?.[1] ?? '(no result)';
  const results = (out) => out.split('\n').filter((l) => /\[fetchtest\] (get|post|missing|redirect|big|head):/.test(l)).map((l) => l.replace(/^.*\[fetchtest\] /, '')).sort().join(' | ');

  const live = {};
  for (const r of Object.keys(runners)) live[r] = await run(r, '--allow-net=127.0.0.1');
  console.log(`      live results: ${results(live.native)}`);
  check('live: native == node (results)', results(live.native), results(live.node));
  check('live: native == node (hash)', done(live.native), done(live.node));
  check('live: all six requests answered', 6, results(live.native).split(' | ').length);
  const version = JSON.parse(readFileSync(join(ROOT, 'runners/web/package.json'), 'utf8')).version;
  for (const [r, name] of [['native', 'gasm-run'], ['node', 'gasm-headless']]) {
    agents = []; // six requests, the redirect twice
    await run(r, '--allow-net=127.0.0.1', '--app-id', 'fetchtest/1.0 (+https://gasm.emdzej.pl)');
    check(`${r}: --app-id in every User-Agent`, `7 x fetchtest/1.0 (+https://gasm.emdzej.pl) ${name}/${version}`, `${agents.length} x ${[...new Set(agents)].join(', ')}`);
  }
  for (const r of Object.keys(runners)) {
    const denied = await run(r);
    check(`${r}: denied without --allow-net`, true, /fetch: denied .* \(/.test(denied) && /get: not sent/.test(denied));
    const other = await run(r, '--allow-net=example.org,*.example.org');
    check(`${r}: denied for a host not on the list`, true, /fetch: denied .*127\.0\.0\.1 is not/.test(other) && /get: not sent/.test(other));
  }
  // record on both runners: the same files; replay them: the live results
  for (const r of Object.keys(runners)) await run(r, '--allow-net=127.0.0.1', '--fetch-record', join(tmp, r));
  const files = (d) => readdirSync(d).sort().map((f) => `${f}:${readFileSync(join(d, f)).toString('base64')}`).join(' ');
  check('record: native == node files', files(join(tmp, 'native')), files(join(tmp, 'node')));
  for (const r of Object.keys(runners)) {
    const replay = await run(r, '--fetch-replay', join(tmp, 'native'));
    check(`${r}: replay == live`, done(live.native), done(replay));
  }

  // the browser player (main thread), same origin as the test server
  if (!existsSync(CHROME) && !process.env.CHROME) console.log('SKIP  Chrome (not found)');
  else {
    const profile = join(tmp, 'chrome');
    const args = ['--headless=new', '--remote-debugging-port=9337', `--user-data-dir=${profile}`];
    if (process.env.CI) args.push('--no-sandbox');
    const chrome = spawn(CHROME, [...args, 'about:blank'], { stdio: 'ignore' });
    try {
      let target;
      for (let i = 0; i < 300 && !target; i++) {
        await sleep(200);
        target = await fetch('http://127.0.0.1:9337/json').then((r) => r.json()).then((t) => t.find((x) => x.type === 'page')).catch(() => null);
      }
      const ws = new WebSocket(target.webSocketDebuggerUrl);
      await new Promise((r) => (ws.onopen = r));
      let id = 0; const pending = new Map(); const logs = [];
      ws.onmessage = (e) => {
        const m = JSON.parse(e.data);
        if (m.id && pending.has(m.id)) { pending.get(m.id)(m); pending.delete(m.id); }
        if (m.method === 'Runtime.consoleAPICalled') logs.push(m.params.args.map((a) => a.value ?? a.description).join(' '));
      };
      const send = (method, params = {}) => new Promise((r) => { const i = ++id; pending.set(i, r); ws.send(JSON.stringify({ id: i, method, params })); });
      await send('Runtime.enable'); await send('Page.enable');
      await send('Page.navigate', { url: `http://127.0.0.1:${port}/runners/web/?game=fetchtest.wasm&autostart&base=${encodeURIComponent(base)}` });
      let got = null;
      for (let i = 0; i < 150 && !got; i++) {
        await sleep(100);
        // the guest's log reaches the console (the page shows only the last line)
        got = logs.map((l) => /\[fetchtest\] done: \d+ frames, hash=([0-9a-f]+)/.exec(l)?.[1]).find(Boolean) ?? null;
      }
      check('Chrome (player): == live', done(live.native), got ?? `(no result) ${logs.slice(-3).join(' / ')}`);
      ws.close();
    } finally { chrome.kill(); }
  }
} finally {
  server.kill();
  await sleep(300);
  rmSync(tmp, { recursive: true, force: true });
}
console.log(`${pass} passed, ${fail} failed`);
process.exit(fail ? 1 : 0);
