#!/usr/bin/env node
// Real threads in Chrome (runners/web/lib/threads.js): the player runs mttest (wasi-threads)
// in Worker mode with a pool of workers on a cross-origin isolated page
// (scripts/serve-isolated.mjs); a worker's trap ends the game even while the game's thread
// waits for it (the page's watchdog); without isolation the game runs on one thread.
//   node scripts/threads-web-test.mjs        (ports 8796 isolated, 8797 plain)
import { spawn, spawnSync } from 'node:child_process';

const ROOT = new URL('..', import.meta.url).pathname;
const sleep = (ms) => new Promise((r) => setTimeout(r, ms));
const isolated = spawn('node', ['scripts/serve-isolated.mjs', '8796'], { cwd: ROOT, stdio: 'ignore' });
const plain = spawn('python3', ['-m', 'http.server', '8797'], { cwd: ROOT, stdio: 'ignore' });
let pass = 0, fail = 0;
const smoke = (url, secs) => spawnSync('node', ['scripts/web-smoke.mjs', url, '/tmp/gasm-threads-web.png', String(secs)], { cwd: ROOT, encoding: 'utf8' }).stdout;
const check = (name, ok, out) => {
  if (ok) { pass++; console.log(`PASS  ${name}`); } else { fail++; console.log(`FAIL  ${name}\n${out}`); }
};
try {
  await sleep(1000);
  const game = 'runners/web/?wasm=../../build/mttest.wasm&autostart&nosplash';
  let out = smoke(`http://localhost:8796/${game}`, 12);
  check('threads-web          4 workers, same sums', /\[mttest\] 4 threads; sums agree/.test(out) && /\(worker\)/.test(out) && /exited \(code 0\)/.test(out), out);
  out = smoke(`http://localhost:8796/${game}&trap=1`, 10);
  check('threads-web-trap     a worker\'s trap ends the game', /thread \d+: unreachable/.test(out), out);
  out = smoke(`http://localhost:8797/${game}`, 12);
  check('threads-web-plain    no isolation: one thread', /\[mttest\] 0 threads; sums agree/.test(out) && /isn't cross-origin isolated/.test(out), out);
} finally {
  isolated.kill();
  plain.kill();
}
console.log(`${pass} passed, ${fail} failed`);
process.exit(fail ? 1 : 0);
