#!/usr/bin/env node
// A static file server for the repository whose pages are cross-origin isolated (COOP +
// COEP headers), which browsers require for SharedArrayBuffer: games built with threads
// get worker threads in the player only on such pages (python3 -m http.server can't send
// the headers).
//   node scripts/serve-isolated.mjs [port]     (default 8766), then
//   http://localhost:8766/runners/web/?wasm=../../build/mttest.wasm&autostart
import { createServer } from 'node:http';
import { createReadStream, statSync } from 'node:fs';
import { extname, join, normalize } from 'node:path';

const PORT = Number(process.argv[2] ?? 8766);
const ROOT = new URL('..', import.meta.url).pathname;
const TYPES = { '.html': 'text/html', '.js': 'text/javascript', '.mjs': 'text/javascript', '.json': 'application/json', '.wasm': 'application/wasm',
  '.css': 'text/css', '.png': 'image/png', '.svg': 'image/svg+xml', '.pck': 'application/octet-stream', '.txt': 'text/plain' };

createServer((req, res) => {
  let path = decodeURIComponent(new URL(req.url, 'http://x').pathname);
  path = normalize(join(ROOT, path));
  if (!path.startsWith(ROOT)) { res.writeHead(403).end(); return; }
  let st = statSync(path, { throwIfNoEntry: false });
  if (st?.isDirectory()) { path = join(path, 'index.html'); st = statSync(path, { throwIfNoEntry: false }); }
  if (!st?.isFile()) { res.writeHead(404).end('not found'); return; }
  res.writeHead(200, {
    'Content-Type': TYPES[extname(path)] ?? 'application/octet-stream', 'Content-Length': st.size,
    'Cross-Origin-Opener-Policy': 'same-origin', 'Cross-Origin-Embedder-Policy': 'require-corp', 'Cache-Control': 'no-store',
  });
  createReadStream(path).pipe(res);
}).listen(PORT, '127.0.0.1', () => console.error(`serving ${ROOT} on http://localhost:${PORT} (cross-origin isolated)`));
