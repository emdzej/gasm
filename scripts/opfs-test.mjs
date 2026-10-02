#!/usr/bin/env node
// OPFS + Worker-mode acceptance test (headless Chrome, via the DevTools protocol).
//
// 1. opfs.html generates a data set in OPFS (big.bin + small files) through csfs;
// 2. index.html runs assetcheck.wasm in Worker mode from it (OPFS asset provider),
//    streaming random reads of big.bin, while Chrome's resident memory is sampled;
// 3. the same data set written to disk runs in the Node headless runner (--asset-dir):
//    the hashes must match. Repeated with a tiny data set for the memory baseline.
//
//   node scripts/opfs-test.mjs [MB]     (default 200; serves the repo on :8791)
import { spawn, execSync } from 'node:child_process';
import { mkdirSync, mkdtempSync, rmSync, writeFileSync, openSync, writeSync, closeSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { fillTestBytes } from '../runners/web/testdata.js';

const MB = Number(process.argv[2] ?? 200);
const PORT = 8791;
const CHROME = process.env.CHROME ?? '/Applications/Google Chrome.app/Contents/MacOS/Google Chrome';
const PROFILE = mkdtempSync(join(tmpdir(), 'gasm-opfs-chrome-'));
const sleep = (ms) => new Promise((r) => setTimeout(r, ms));
const GUEST = '&read=art/art.car,readme.txt&stream=big.bin&reads=256&chunk=65536&frames=600';

const server = spawn('python3', ['-m', 'http.server', String(PORT)], { cwd: new URL('..', import.meta.url).pathname, stdio: 'ignore' });
const chromeArgs = ['--headless=new', '--remote-debugging-port=9334', `--user-data-dir=${PROFILE}`];
if (process.env.CI) chromeArgs.push('--no-sandbox'); // GitHub runners
const chrome = spawn(CHROME, [...chromeArgs, 'about:blank'], { stdio: 'ignore' });

function chromeProcs() { // [{ pid, rssMb, type }] for this Chrome instance
  return execSync('ps -axo pid=,rss=,command=').toString().split('\n').filter((l) => l.includes(PROFILE)).map((l) => {
    const [pid, rss] = l.trim().split(/\s+/);
    const type = (l.match(/--type=([a-z-]+)/)?.[1] ?? 'browser') + (l.match(/--utility-sub-type=([\w.]+)/)?.[1] ? `:${l.match(/--utility-sub-type=([\w.]+)/)[1].split('.').pop()}` : '');
    return { pid: Number(pid), rssMb: Number(rss) / 1024, type };
  });
}
// Renderer processes host the page and its dedicated Worker (where the guest runs).
function chromeRssMb() { return chromeProcs().filter((p) => p.type === 'renderer').reduce((s, p) => s + p.rssMb, 0); }
// macOS: phys_footprint (dirty + compressed, i.e. memory that can't simply be dropped),
// which unlike RSS excludes clean file-backed pages such as the OS file cache.
function footprintMb(pid) {
  try {
    const out = execSync(`footprint ${pid} 2>/dev/null`).toString();
    const m = out.match(/Footprint:\s*([\d.]+)\s*(KB|MB|GB)/) || out.match(/phys_footprint:\s*([\d.]+)\s*(KB|MB|GB)/);
    return m ? Number(m[1]) * { KB: 1 / 1024, MB: 1, GB: 1024 }[m[2]] : NaN;
  } catch { return NaN; }
}
const hasFootprint = (() => { try { execSync('command -v footprint'); return true; } catch { return false; } })();

async function cdp() {
  let target;
  // a cold Chrome on a CI machine can take a while to open its debugging port
  for (let i = 0; i < 300 && !target; i++) {
    await sleep(200);
    target = await fetch('http://127.0.0.1:9334/json').then((r) => r.json()).then((t) => t.find((x) => x.type === 'page')).catch(() => null);
  }
  if (!target) throw new Error('Chrome did not open its DevTools port within 60 s');
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
  const evaluate = async (expr) => (await send('Runtime.evaluate', { expression: expr, returnByValue: true })).result.result.value;
  const until = async (expr, ms) => { const t0 = Date.now(); while (Date.now() - t0 < ms) { const v = await evaluate(expr); if (v) return v; await sleep(250); } throw new Error(`timeout: ${expr}`); };
  return { send, evaluate, until, logs };
}

function nodeRun(dir) {
  const params = GUEST.split('&').filter(Boolean).flatMap((kv) => ['--param', kv]);
  const out = execSync(`node runners/web/headless.mjs build/assetcheck.wasm --asset-dir ${dir} --headless 100000 ${params.join(' ')} 2>&1`, { cwd: new URL('..', import.meta.url).pathname }).toString();
  return pick(out.split('\n'));
}
const pick = (lines) => {
  const done = lines.find((l) => l.includes('[assetcheck] done'))?.replace(/^.*\[assetcheck\] /, '');
  const video = lines.join(' ').match(/video_fnv32=[0-9a-f]+/)?.[0];
  return `${done} ${video}`;
};

async function scenario(page, name, mb) {
  await page.send('Page.navigate', { url: `http://localhost:${PORT}/runners/web/opfs.html?generate=${name}&mb=${mb}` });
  const imported = await page.until('globalThis.__gasmImported', 600000);
  await page.send('Page.navigate', { url: `http://localhost:${PORT}/runners/web/index.html?game=assetcheck.wasm&opfs=gasm-assets/${name}${GUEST}&hashframes=600` });
  await page.until('document.readyState === "complete"', 10000);
  const before = chromeRssMb();
  const procsBefore = new Map(chromeProcs().map((p) => [p.pid, p]));
  const fpBefore = hasFootprint ? new Map([...procsBefore.keys()].map((pid) => [pid, footprintMb(pid)])) : null;
  let peak = before, peakProcs = [...procsBefore.values()];
  page.logs.length = 0;
  await page.evaluate('document.getElementById("start").click()');
  const t0 = Date.now();
  while (!(await page.evaluate('globalThis.__gasmResult'))) {
    const procs = chromeProcs().filter((p) => p.type === 'renderer'), total = procs.reduce((s, p) => s + p.rssMb, 0);
    if (total > peak) { peak = total; peakProcs = procs; }
    await sleep(100);
    if (Date.now() - t0 > 600000) throw new Error('run timed out');
  }
  // which processes grew, and by how much of it is non-reclaimable (footprint)
  const growth = peakProcs.map((p) => ({ ...p, grew: p.rssMb - (procsBefore.get(p.pid)?.rssMb ?? 0) }))
    .filter((p) => p.grew > 5).sort((a, b) => b.grew - a.grew)
    .map((p) => `${p.type} +${p.grew.toFixed(0)} MB RSS` + (fpBefore ? `, footprint ${(footprintMb(p.pid) - (fpBefore.get(p.pid) ?? 0)).toFixed(0)} MB change` : ''));
  const secs = (Date.now() - t0) / 1000;
  const hash = await page.evaluate('globalThis.__gasmResult');
  const browser = `${page.logs.find((l) => l.includes('[assetcheck] done'))?.replace(/^.*\[assetcheck\] /, '')} ${hash.match(/video_fnv32=[0-9a-f]+/)?.[0]}`;
  // same data on disk for the Node runner
  const dir = mkdtempSync(join(tmpdir(), 'gasm-opfs-data-'));
  const fd = openSync(join(dir, 'big.bin'), 'w');
  for (let off = 0; off < mb * 1048576; off += 1048576) writeSync(fd, fillTestBytes(new Uint8Array(1048576), off));
  closeSync(fd);
  mkdirSync(join(dir, 'ART'));
  writeFileSync(join(dir, 'ART/ART.CAR'), 'ART CAR FILE v1\n');
  writeFileSync(join(dir, 'README.TXT'), 'GASM TEST DISC\n');
  const node = nodeRun(dir);
  rmSync(dir, { recursive: true, force: true });
  return { imported, browser, node, before, peak, secs, growth };
}

let failed = false;
try {
  const page = await cdp();
  // Warm up: a fresh profile runs background jobs (component updates) for a while.
  await page.send('Page.navigate', { url: `http://localhost:${PORT}/runners/web/opfs.html` });
  await sleep(8000);
  const tiny = await scenario(page, 'test-tiny', 1);
  const big = await scenario(page, 'test-big', MB);
  for (const [label, r] of [[`${MB} MB`, big], ['1 MB', tiny]]) {
    const ok = r.browser === r.node && r.browser.includes('hash=');
    failed ||= !ok;
    console.log(`${ok ? 'PASS' : 'FAIL'}  OPFS worker == node --asset-dir (${label}): ${r.browser}${ok ? '' : `\n  node: ${r.node}`}`);
    console.log(`      renderer resident memory: ${r.before.toFixed(0)} MB before, ${r.peak.toFixed(0)} MB peak while streaming (${r.secs.toFixed(1)} s)`);
    if (r.growth.length) console.log(`      grew: ${r.growth.join('; ')}`);
  }
} catch (e) {
  failed = true;
  console.error(`FAIL  ${e.message}`);
} finally {
  // Wait for Chrome to exit before deleting its profile (it writes while shutting down);
  // cleanup problems never fail the test.
  const exited = new Promise((r) => chrome.once('exit', r));
  chrome.kill(); server.kill();
  await Promise.race([exited, sleep(5000)]);
  try { rmSync(PROFILE, { recursive: true, force: true, maxRetries: 5, retryDelay: 200 }); } catch (e) { console.error(`(cleanup: ${e.message})`); }
}
process.exit(failed ? 1 : 0);
