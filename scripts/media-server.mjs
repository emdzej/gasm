#!/usr/bin/env node
// A local https stream server for gasm:media's tests (scripts/media-test.sh): radio and
// tracks from tests/fixtures/media/tone.mp3 (3 s, 64 kbps).
//   /live.mp3     a live stream (the tone looped at real time), ICY metadata when asked
//                 (icy-metaint 8000: a block a second; the title changes every 2 s:
//                 "gasm test - Song 1", "Song 2", ...), icy-name and icy-br
//   /track.mp3    the tone once, with a length (a track: it ends)
//   /redirect     302 to /live.mp3
//   /drop.mp3     a live stream that closes after 1.5 s (each connection is counted)
//   /notaudio     an HTML page
// Prints "listening on <port>" when ready, and "connection <path> <n>" for each request.
//   node scripts/media-server.mjs --port 8443 --cert leaf.pem --key leaf.key
import { createServer } from 'node:https';
import { readFileSync } from 'node:fs';
import { dirname, join } from 'node:path';
import { fileURLToPath } from 'node:url';

const arg = (name, dflt) => {
  const i = process.argv.indexOf(name);
  return i > 0 ? process.argv[i + 1] : dflt;
};
const ROOT = join(dirname(fileURLToPath(import.meta.url)), '..');
const tone = readFileSync(join(ROOT, 'tests/fixtures/media/tone.mp3'));
const BYTES_PER_S = 8000;   // 64 kbps
const METAINT = 8000;
const counts = new Map();

// the tone looped forever, paced at real time; ICY blocks interleaved when asked
function live(req, res, { dropAfterMs = 0 } = {}) {
  const icy = req.headers['icy-metadata'] === '1';
  res.writeHead(200, {
    'content-type': 'audio/mpeg',
    'icy-name': 'gasm test radio',
    'icy-br': '64',
    ...(icy ? { 'icy-metaint': String(METAINT) } : {}),
    'cache-control': 'no-cache',
  });
  const started = Date.now();
  let at = 0;          // position in the tone
  let sinceMeta = 0;   // audio bytes since the last ICY block
  const timer = setInterval(() => {
    if (dropAfterMs && Date.now() - started >= dropAfterMs) {
      clearInterval(timer);
      res.destroy();
      return;
    }
    let left = BYTES_PER_S / 10;
    const parts = [];
    while (left > 0) {
      let n = Math.min(left, tone.length - at);
      if (icy) n = Math.min(n, METAINT - sinceMeta);
      parts.push(tone.subarray(at, at + n));
      at = (at + n) % tone.length;
      left -= n;
      if (icy) {
        sinceMeta += n;
        if (sinceMeta === METAINT) {
          sinceMeta = 0;
          const song = 1 + Math.floor((Date.now() - started) / 2000);
          const text = Buffer.from(`StreamTitle='gasm test - Song ${song}';`);
          const blocks = Math.ceil(text.length / 16);
          const meta = Buffer.alloc(1 + blocks * 16);
          meta[0] = blocks;
          text.copy(meta, 1);
          parts.push(meta);
        }
      }
    }
    res.write(Buffer.concat(parts));
  }, 100);
  res.on('close', () => clearInterval(timer));
}

const options = { cert: readFileSync(arg('--cert')), key: readFileSync(arg('--key')) };
const server = createServer(options, (req, res) => {
  const path = new URL(req.url, 'https://x').pathname;
  counts.set(path, (counts.get(path) ?? 0) + 1);
  console.log(`connection ${path} ${counts.get(path)}`);
  if (path === '/live.mp3') return live(req, res);
  if (path === '/drop.mp3') return live(req, res, { dropAfterMs: 1500 });
  if (path === '/track.mp3') {
    res.writeHead(200, { 'content-type': 'audio/mpeg', 'content-length': String(tone.length) });
    return res.end(tone);
  }
  if (path === '/redirect') {
    res.writeHead(302, { location: '/live.mp3' });
    return res.end();
  }
  if (path === '/notaudio') {
    res.writeHead(200, { 'content-type': 'text/html' });
    return res.end('<!doctype html><title>not audio</title><p>hello</p>');
  }
  res.writeHead(404);
  res.end();
});
server.listen(Number(arg('--port', '8443')), '127.0.0.1', () => console.log(`listening on ${server.address().port}`));
