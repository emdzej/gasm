// gasm:storage stores (same rules as runners/native/src/storage.rs).

export const STORAGE_MAX_VALUE = 1 << 20, STORAGE_QUOTA = 16 << 20;
// GASM_STORAGE_ERR_* (spec/abi.json)
export const STORAGE_ERR_KEY = -1, STORAGE_ERR_SIZE = -2, STORAGE_ERR_QUOTA = -3, STORAGE_ERR_IO = -4;
export const validKey = (k) => /^[A-Za-z0-9._-]{1,128}$/.test(k) && k !== '.' && k !== '..';

/** A failed set: `code` is the GASM_STORAGE_ERR_* value returned to the guest. */
export class StorageError extends Error {
  constructor(code, message) { super(message); this.code = code; }
}

// In-memory store: headless runs (reproducible) and the base for IdbStorage.
export class MemoryStorage {
  constructor(entries = []) {
    this.map = new Map(entries);
    this.used = 0;
    for (const [k, v] of this.map) this.used += k.length + v.length;
    this.sorted = null;
  }
  get(k) { return this.map.get(k); }
  /** All keys, sorted (ASCII, so the same order as the native runner). */
  keys() { return (this.sorted ??= [...this.map.keys()].sort()); }
  /** Store a value; throws StorageError. */
  set(k, v) {
    if (!validKey(k)) throw new StorageError(STORAGE_ERR_KEY, `invalid key ${JSON.stringify(k)}`);
    if (v.length > STORAGE_MAX_VALUE) throw new StorageError(STORAGE_ERR_SIZE, `value is ${v.length} bytes (max ${STORAGE_MAX_VALUE})`);
    const old = this.map.get(k), used = this.used - (old ? k.length + old.length : 0) + k.length + v.length;
    if (used > STORAGE_QUOTA) throw new StorageError(STORAGE_ERR_QUOTA, `storage quota of ${STORAGE_QUOTA} bytes exceeded`);
    if (!old) this.sorted = null;
    this.map.set(k, v);
    this.used = used;
    this.persist?.('put', k, v);
  }
  delete(k) {
    const old = this.map.get(k);
    if (!old) return false;
    this.map.delete(k);
    this.used -= k.length + old.length;
    this.sorted = null;
    this.persist?.('delete', k);
    return true;
  }
  close() {}
}

// Browser store: IndexedDB database "gasm", one record per (namespace, key). Loaded
// fully before the game starts so reads are synchronous; writes are persisted in the
// background (errors are logged; `flush()` waits for them, e.g. on pagehide).
export class IdbStorage extends MemoryStorage {
  static async open(namespace, log = console.log) {
    const db = await new Promise((resolve, reject) => {
      const req = indexedDB.open('gasm', 1);
      req.onupgradeneeded = () => req.result.createObjectStore('kv');
      req.onsuccess = () => resolve(req.result);
      req.onerror = () => reject(req.error);
    });
    const prefix = `${namespace}/`;
    const entries = await new Promise((resolve, reject) => {
      const out = [];
      const range = IDBKeyRange.bound(prefix, `${prefix}￿`);
      const req = db.transaction('kv').objectStore('kv').openCursor(range);
      req.onsuccess = () => {
        const c = req.result;
        if (!c) return resolve(out);
        out.push([String(c.key).slice(prefix.length), new Uint8Array(c.value)]);
        c.continue();
      };
      req.onerror = () => reject(req.error);
    });
    const s = new IdbStorage(entries);
    s.db = db;
    s.writes = new Set();
    s.persist = (op, k, v) => {
      const tx = db.transaction('kv', 'readwrite');
      const store = tx.objectStore('kv');
      if (op === 'put') store.put(v, prefix + k); else store.delete(prefix + k);
      const done = new Promise((resolve) => {
        tx.oncomplete = resolve;
        tx.onerror = tx.onabort = () => { log(`[gasm] storage: saving ${k} failed: ${tx.error?.message ?? 'aborted'}`); resolve(); };
      });
      s.writes.add(done);
      done.then(() => s.writes.delete(done));
    };
    return s;
  }
  /** Wait for the writes in flight. */
  flush() { return Promise.all([...this.writes]); }
  close() { this.flush().then(() => this.db.close()); }
}
