// The WASI preview1 subset gasm runners provide (identical to
// runners/native/src/wasi.rs): wasi-libc's stdio, malloc, clocks and random
// numbers, and nothing that reaches files, environment or arguments.
//   fd_write to fd 1/2 -> the runner's log; clocks and random_get are virtual /
//   fixed in reproducible (headless) runs; fd_prestat_get answers EBADF (no
//   preopens); anything else answers ENOSYS; proc_exit ends the game.

export class ProcExit extends Error {
  constructor(code) { super(`guest called proc_exit(${code})`); this.code = code; }
}

const M64 = (1n << 64n) - 1n;

/** Deterministic random_get for reproducible runs: splitmix64, little-endian (as natively). */
export class Splitmix {
  constructor(seed = 0n) { this.s = seed; }
  next() {
    this.s = (this.s + 0x9e3779b97f4a7c15n) & M64;
    let z = this.s;
    z = ((z ^ (z >> 30n)) * 0xbf58476d1ce4e5b9n) & M64;
    z = ((z ^ (z >> 27n)) * 0x94d049bb133111ebn) & M64;
    return z ^ (z >> 31n);
  }
  fill(u8) {
    for (let i = 0; i < u8.length; i += 8) {
      let v = this.next();
      for (let k = 0; k < 8 && i + k < u8.length; k++) { u8[i + k] = Number(v & 255n); v >>= 8n; }
    }
  }
}

const SUCCESS = 0, EBADF = 8, EINVAL = 28, ENOSYS = 52, ESPIPE = 70;
const LOSSY = new TextDecoder();

/** WASI imports for `host` (a GasmHost: memory views, virtual time, log). */
export function wasiImports(host) {
  const stdio = { 1: '', 2: '' };
  const clockNs = (id) => {
    if (host.virtualTime) return BigInt(Math.round(host.vtime * 1e6));
    return BigInt(Math.round((id === 0 ? Date.now() : performance.now() - host.t0) * 1e6));
  };
  const impl = {
    fd_write: (fd, iovs, iovsLen, nwrittenPtr) => {
      if (fd !== 1 && fd !== 2) return EBADF;
      const dv = host.view();
      let total = 0;
      for (let i = 0; i < iovsLen; i++) {
        const ptr = dv.getUint32(iovs + i * 8, true), len = dv.getUint32(iovs + i * 8 + 4, true);
        stdio[fd] += LOSSY.decode(host.bytes(ptr, len).slice(), { stream: true });   // a copy: decoders refuse shared memory
        total += len;
      }
      let nl;
      while ((nl = stdio[fd].indexOf('\n')) >= 0) {
        host.onLog(stdio[fd].slice(0, nl));
        stdio[fd] = stdio[fd].slice(nl + 1);
      }
      host.view().setUint32(nwrittenPtr, total, true);
      return SUCCESS;
    },
    fd_close: () => SUCCESS,
    fd_seek: () => ESPIPE,
    fd_prestat_get: () => EBADF,
    fd_fdstat_get: (fd, ptr) => {
      if (fd < 0 || fd > 2) return EBADF;
      const b = host.bytes(ptr, 24);
      b.fill(0);
      b[0] = 2; // filetype: character device
      return SUCCESS;
    },
    clock_res_get: (id, out) => {
      if (id > 3) return EINVAL;
      host.view().setBigUint64(out, 1000n, true);
      return SUCCESS;
    },
    clock_time_get: (id, _precision, out) => {
      if (id > 3) return EINVAL;
      host.view().setBigUint64(out, clockNs(id), true);
      return SUCCESS;
    },
    random_get: (ptr, len) => {
      const buf = host.bytes(ptr, len);
      if (host.random) host.random.fill(buf);
      else if (buf.buffer instanceof ArrayBuffer) for (let i = 0; i < buf.length; i += 65536) crypto.getRandomValues(buf.subarray(i, i + 65536));
      else {   // shared memory (a game with threads): getRandomValues refuses it, so through a copy
        const tmp = new Uint8Array(Math.min(buf.length, 65536));
        for (let i = 0; i < buf.length; i += tmp.length) { const n = Math.min(tmp.length, buf.length - i); crypto.getRandomValues(tmp.subarray(0, n)); buf.set(tmp.subarray(0, n), i); }
      }
      return SUCCESS;
    },
    args_sizes_get: (a, b) => { const dv = host.view(); dv.setUint32(a, 0, true); dv.setUint32(b, 0, true); return SUCCESS; },
    args_get: () => SUCCESS,
    environ_sizes_get: (a, b) => { const dv = host.view(); dv.setUint32(a, 0, true); dv.setUint32(b, 0, true); return SUCCESS; },
    environ_get: () => SUCCESS,
    sched_yield: () => SUCCESS,
    proc_exit: (code) => { throw new ProcExit(code); },
  };
  // Anything else a guest imports exists and answers "not supported".
  return new Proxy(impl, { get: (t, name) => t[name] ?? (() => ENOSYS) });
}

/** The functions implemented above (what `has` reports). */
export const WASI_IMPLEMENTED = ['fd_write', 'fd_close', 'fd_seek', 'fd_prestat_get', 'fd_fdstat_get', 'clock_res_get',
  'clock_time_get', 'random_get', 'args_sizes_get', 'args_get', 'environ_sizes_get', 'environ_get', 'sched_yield', 'proc_exit'];
