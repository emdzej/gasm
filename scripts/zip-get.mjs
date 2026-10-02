#!/usr/bin/env node
// Extract single files from a remote zip with HTTP range requests (no full download).
//   node scripts/zip-get.mjs <url> <out-dir> <member>...
// Reads the central directory from the end of the archive, then each member's
// local header and data. Stored and deflated members; no zip64.
import { mkdirSync, writeFileSync } from 'node:fs';
import { dirname, join } from 'node:path';
import { inflateRawSync } from 'node:zlib';

const [url, outDir, ...members] = process.argv.slice(2);
if (!url || !outDir || !members.length) {
  console.error('usage: zip-get.mjs <url> <out-dir> <member>...');
  process.exit(2);
}

async function range(start, end) {   // inclusive
  const res = await fetch(url, { headers: { Range: `bytes=${start}-${end}` }, redirect: 'follow' });
  if (res.status !== 206) throw new Error(`${url}: range request answered ${res.status}`);
  return Buffer.from(await res.arrayBuffer());
}

const head = await fetch(url, { method: 'HEAD', redirect: 'follow' });
const size = Number(head.headers.get('content-length'));
if (!size) throw new Error(`${url}: no content-length`);

// end of central directory: in the last 64 KiB + 22 bytes
const tailStart = Math.max(0, size - 65558);
const tail = await range(tailStart, size - 1);
const eocd = tail.lastIndexOf(Buffer.from([0x50, 0x4b, 0x05, 0x06]));
if (eocd < 0) throw new Error('not a zip (no end of central directory)');
const cdSize = tail.readUInt32LE(eocd + 12), cdOffset = tail.readUInt32LE(eocd + 16);
const cd = await range(cdOffset, cdOffset + cdSize - 1);

const entries = new Map();
for (let p = 0; p + 46 <= cd.length && cd.readUInt32LE(p) === 0x02014b50;) {
  const method = cd.readUInt16LE(p + 10), csize = cd.readUInt32LE(p + 20), usize = cd.readUInt32LE(p + 24);
  const nlen = cd.readUInt16LE(p + 28), xlen = cd.readUInt16LE(p + 30), clen = cd.readUInt16LE(p + 32);
  const local = cd.readUInt32LE(p + 42);
  entries.set(cd.toString('utf8', p + 46, p + 46 + nlen), { method, csize, usize, local });
  p += 46 + nlen + xlen + clen;
}

for (const name of members) {
  const e = entries.get(name);
  if (!e) throw new Error(`${name}: not in the archive`);
  const lh = await range(e.local, e.local + 29);
  const dataStart = e.local + 30 + lh.readUInt16LE(26) + lh.readUInt16LE(28);
  const raw = e.csize ? await range(dataStart, dataStart + e.csize - 1) : Buffer.alloc(0);
  const data = e.method === 0 ? raw : e.method === 8 ? inflateRawSync(raw) : null;
  if (!data) throw new Error(`${name}: compression method ${e.method} not supported`);
  if (data.length !== e.usize) throw new Error(`${name}: size ${data.length}, expected ${e.usize}`);
  const out = join(outDir, name);
  mkdirSync(dirname(out), { recursive: true });
  writeFileSync(out, data);
  console.log(`${out} (${data.length} bytes)`);
}
