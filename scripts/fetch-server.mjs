#!/usr/bin/env node
// Test server for gasm:fetch (guests/fetchtest): fixed answers under /api, and the
// repository's files everywhere else (so the browser player can fetch same-origin).
// No Date header: recorded responses (tests/fixtures/fetch) are the same every time.
//
//   node scripts/fetch-server.mjs [port]     prints "listening <port>" (0: any free port)
import { createServer } from 'node:http';
import { existsSync, readFileSync, statSync } from 'node:fs';
import { extname, join, normalize } from 'node:path';

const ROOT = new URL('..', import.meta.url).pathname;
const TYPES = { '.js': 'text/javascript', '.mjs': 'text/javascript', '.html': 'text/html', '.css': 'text/css', '.wasm': 'application/wasm', '.json': 'application/json', '.svg': 'image/svg+xml' };
const big = Uint8Array.from({ length: 65536 }, (_, i) => (i * 31 + 7) & 255);

const server = createServer((req, res) => {
  res.sendDate = false;
  res.setHeader('access-control-allow-origin', '*');
  res.setHeader('access-control-allow-headers', 'x-test, content-type');
  const path = new URL(req.url, 'http://x').pathname;
  const chunks = [];
  req.on('data', (c) => chunks.push(c));
  req.on('end', () => {
    const body = Buffer.concat(chunks).toString();
    const send = (status, type, data, extra = {}) => { res.writeHead(status, { 'content-type': type, ...extra }); res.end(data); };
    if (req.method === 'OPTIONS') return send(204, 'text/plain', '');
    switch (path) {
      case '/api/hello': return send(200, 'text/plain', 'hello from the test server');
      case '/api/echo': return send(200, 'text/plain', `${req.method} x-test=${req.headers['x-test'] ?? ''} ${body}`);
      case '/api/missing': return send(404, 'text/plain', 'not found');
      case '/api/redirect': return send(302, 'text/plain', '', { location: '/api/hello' });
      case '/api/big': return send(200, 'application/octet-stream', big);
    }
    const rel = normalize(decodeURIComponent(path)).replace(/^\/+/, '');
    let file = join(ROOT, rel);
    if (rel.startsWith('..') || !existsSync(file)) return send(404, 'text/plain', 'not found');
    if (statSync(file).isDirectory()) file = join(file, 'index.html');
    if (!existsSync(file)) return send(404, 'text/plain', 'not found');
    send(200, TYPES[extname(file)] ?? 'application/octet-stream', readFileSync(file));
  });
});
server.listen(Number(process.argv[2] ?? 0), '127.0.0.1', () => console.log(`listening ${server.address().port}`));
