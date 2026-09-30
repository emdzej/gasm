// Deterministic test bytes, identical in browsers and Node: byte p of a test file is a
// hash of p, so any chunking produces the same content. Used by opfs.js ("generate
// test data") and scripts/opfs-test.mjs to compare OPFS/Worker runs with Node runs.
export function fillTestBytes(dst, offset) {
  for (let i = 0; i < dst.length; i++) {
    let x = Math.imul((offset + i) ^ 0x9e3779b9, 0x85ebca6b) >>> 0;
    x ^= x >>> 13; x = Math.imul(x, 0xc2b2ae35) >>> 0; x ^= x >>> 16;
    dst[i] = x & 0xff;
  }
  return dst;
}

/** ReadableStream of `size` test bytes, in 1 MiB chunks. */
export function testStream(size) {
  let pos = 0;
  return new ReadableStream({
    pull(ctrl) {
      if (pos >= size) return ctrl.close();
      const n = Math.min(1 << 20, size - pos);
      ctrl.enqueue(fillTestBytes(new Uint8Array(n), pos));
      pos += n;
    },
  });
}
