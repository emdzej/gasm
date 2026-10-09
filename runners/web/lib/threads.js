// Real threads (wasi-threads) for the JS host: design/threads.md, part B, "Browsers".
// The same rules as natively (runners/native/src/threads.rs): each thread is a worker
// with its own instance of the module, all sharing the imported memory; workers compute
// and read assets (the other gasm imports do nothing there); a trap or proc_exit on any
// thread ends the game, seen after the frame. Headless runs allow none unless asked.
//
// Workers start before the game (startup is asynchronous and needs the event loop,
// which a game blocked in pthread_create + join doesn't give back) and park in
// Atomics.wait on their slot of a shared control block. thread-spawn, on any thread,
// claims an idle slot, writes the thread id and start argument and wakes it: no
// messages, no cold start. Browsers can block (Atomics.wait) only off the main
// thread, so a page runs such games in Worker mode, on a cross-origin isolated page.

// Control block (Int32Array over a SharedArrayBuffer)
const NEXT_TID = 0;
const END_KIND = 1;      // 0: running, END_EXIT, END_TRAP (the first end wins)
const END_CODE = 2;      // proc_exit's code
const END_ACK = 3;       // the game's thread saw the end
const END_LEN = 4;       // bytes of the trap message
const END_MSG = 32;      // byte offset of the message (END_MSG_MAX bytes)
const END_MSG_MAX = 1024;
const SLOTS = (END_MSG + END_MSG_MAX) / 4;   // int index of slot 0: [state, tid, arg, -]
export const END_EXIT = 1, END_TRAP = 2;

// slot states
export const STARTING = 0, IDLE = 1, CLAIMED = 2, RUN = 3, STOP = 4;

export const slotIndex = (slot) => SLOTS + slot * 4;
const controlBytes = (size) => (SLOTS + size * 4) * 4;

/** thread-spawn on any thread: a thread id > 0, or -1 when every worker is busy. */
export function spawnThread(ctrl, size, startArg) {
  for (let i = 0; i < size; i++) {
    const s = slotIndex(i);
    if (Atomics.compareExchange(ctrl, s, IDLE, CLAIMED) !== IDLE) continue;
    const tid = Atomics.add(ctrl, NEXT_TID, 1);
    if (tid >= 0x1fffffff) { Atomics.store(ctrl, s, IDLE); return -1; }   // wasi-threads ids are 29 bits
    ctrl[s + 1] = tid;
    ctrl[s + 2] = startArg;
    Atomics.store(ctrl, s, RUN);
    Atomics.notify(ctrl, s);
    return tid;
  }
  return -1;
}

/** A thread ended the game (the first end wins): its kind, code and message. */
export function reportEnd(ctrl, kind, code, message) {
  if (Atomics.compareExchange(ctrl, END_KIND, 0, -1) !== 0) return false;   // -1: being written
  const msg = new TextEncoder().encode(message).subarray(0, END_MSG_MAX);
  new Uint8Array(ctrl.buffer, END_MSG, END_MSG_MAX).set(msg);
  ctrl[END_LEN] = msg.length;
  ctrl[END_CODE] = code;
  Atomics.store(ctrl, END_KIND, kind);
  return true;
}

/** How the game ended on a worker thread, or null; marks it seen (no watchdog). */
export function threadEnd(ctrl) {
  const kind = Atomics.load(ctrl, END_KIND);
  if (kind <= 0) return null;
  Atomics.store(ctrl, END_ACK, 1);
  Atomics.notify(ctrl, END_ACK);
  const msg = new TextDecoder().decode(new Uint8Array(ctrl.buffer, END_MSG, ctrl[END_LEN]).slice());
  return { kind, code: ctrl[END_CODE], message: msg };
}

/** Wait up to `ms` for the game's thread to see the end (the watchdog's wait). */
export function waitForAck(ctrl, ms) {
  return Atomics.wait(ctrl, END_ACK, 0, ms) !== 'timed-out' || Atomics.load(ctrl, END_ACK) === 1;
}

const IS_NODE = typeof process !== 'undefined' && !!process.versions?.node && typeof window === 'undefined';

/** A worker running thread-worker.js: { post, onMessage, terminate } in Node and in browsers. */
async function startWorker(url) {
  if (IS_NODE) {
    const { Worker } = await import('node:worker_threads');
    const w = new Worker(url);
    w.unref();   // parked workers don't keep the process alive
    return { post: (m) => w.postMessage(m), onMessage: (f) => w.on('message', f), onError: (f) => w.on('error', f), terminate: () => w.terminate() };
  }
  const w = new Worker(url, { type: 'module' });
  return { post: (m) => w.postMessage(m), onMessage: (f) => { w.onmessage = (e) => f(e.data); }, onError: (f) => { w.onerror = (e) => f(e.error ?? new Error(e.message)); }, terminate: () => w.terminate() };
}

/** Whether this environment can run threads at all: shared memory, and (browsers) isolation. */
export function threadsAvailable() {
  if (typeof SharedArrayBuffer === 'undefined' || typeof Atomics?.wait !== 'function') return false;
  return IS_NODE || globalThis.crossOriginIsolated === true;
}

export class ThreadPool {
  /**
   * Start `size` workers for a module and its shared memory, each instantiated and
   * parked; resolves once all are. `assets`: AssetTable.share() (what workers read).
   */
  static async start({ module, memory, size, assets, init = {}, onLog = () => {}, workerUrl = new URL('./thread-worker.js', import.meta.url) }) {
    const pool = new ThreadPool();
    pool.size = size;
    pool.ctrl = new Int32Array(new SharedArrayBuffer(controlBytes(size)));
    pool.ctrl[NEXT_TID] = 1;
    pool.workers = await Promise.all(Array.from({ length: size }, () => startWorker(workerUrl)));
    await Promise.all(pool.workers.map((w, slot) => new Promise((resolve, reject) => {
      w.onError(reject);
      w.onMessage((m) => {
        if (m.ready) resolve();
        else if (m.failed) reject(new Error(`threads: worker ${slot}: ${m.failed}`));
        else if (m.log !== undefined) onLog(m.log);
      });
      w.post({ ...init, module, memory, ctrl: pool.ctrl, slot, size, assets });
    })));
    return pool;
  }

  spawn(startArg) { return spawnThread(this.ctrl, this.size, startArg); }
  ended() { return threadEnd(this.ctrl); }

  /** Stop every worker (the game is over). */
  terminate() {
    if (!this.workers.length) return;
    for (let i = 0; i < this.size; i++) { Atomics.store(this.ctrl, slotIndex(i), STOP); Atomics.notify(this.ctrl, slotIndex(i)); }
    for (const w of this.workers) w.terminate();
    this.workers = [];
  }
}
