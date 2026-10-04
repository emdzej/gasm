// Asset providers. A GasmAssetProvider is synchronous (the ABI is): { size(name) -> bytes or -1,
// readAt(name, offset, dst: Uint8Array) -> bytes copied or -1 }. AssetTable
// implements the same naming rules as the native runner (runners/native/src/assets.rs):
// exact names win (explicit entries over folder entries); folder entries also match
// case-insensitively (ASCII), ties resolved by the first name in sorted order.

// UTF-8 byte order = code point order (JS sort compares UTF-16 units, which differs above U+FFFF)
export function byCodePoint(a, b) {
  const x = Array.from(a, (c) => c.codePointAt(0)), y = Array.from(b, (c) => c.codePointAt(0));
  for (let i = 0; i < Math.min(x.length, y.length); i++) if (x[i] !== y[i]) return x[i] - y[i];
  return x.length - y.length;
}

export const isAssetProvider = (a) => a && typeof a.size === 'function' && typeof a.readAt === 'function';
const asciiLower = (s) => s.replace(/[A-Z]/g, (c) => c.toLowerCase());
const hidden = (segments) => segments.some((seg) => seg.startsWith('.'));

/** An asset source: { size() -> number, readAt(offset, dst) -> bytes copied }. */
export const bytesSource = (u8) => ({
  size: () => u8.length,
  readAt: (offset, dst) => {
    const n = Math.max(0, Math.min(dst.length, u8.length - offset));
    if (n > 0) dst.set(u8.subarray(offset, offset + n));
    return n;
  },
});

export class AssetTable {
  constructor(log = () => {}) { this.exact = new Map(); this.folded = new Map(); this.log = log; this.lastVersion = 0; }
  /** Add a source. Explicit entries replace; folder entries never replace an existing name. */
  add(name, source, { fromDir = false } = {}) {
    if (fromDir && this.exact.has(name)) return false;
    this.exact.set(name, { source, fromDir, version: 0 });
    return true;
  }
  /**
   * Add or replace an asset while the game runs (between frames): `bytes` (Uint8Array)
   * or a source. It gets a new version (gasm.asset_version), which is returned.
   */
  set(name, bytes) {
    const source = bytes instanceof Uint8Array ? bytesSource(bytes) : bytes;
    this.exact.set(name, { source, fromDir: false, version: ++this.lastVersion });
    this.index();
    return this.lastVersion;
  }
  /** Remove an asset while the game runs. False if there was none. */
  remove(name) {
    if (!this.exact.delete(name)) return false;
    this.index();
    return true;
  }
  /** Merge another table's entries (as folder entries if `fromDir`). */
  merge(table, { fromDir = true } = {}) {
    for (const [name, e] of table.exact) this.add(name, e.source, { fromDir: fromDir || e.fromDir });
    return this.finish();
  }
  /** Build the case-insensitive index; warns about names that differ only in case. */
  finish() {
    this.index();
    for (const names of this.folded.values()) {
      if (names.length > 1) this.log(`[gasm] assets: ${names.join(', ')} differ only in case; case-insensitive lookups use "${names[0]}"`);
    }
    return this;
  }
  index() {
    this.folded.clear();
    this.sorted = null;
    for (const [name, e] of this.exact) {
      if (!e.fromDir) continue;
      const k = asciiLower(name);
      (this.folded.get(k) ?? this.folded.set(k, []).get(k)).push(name);
    }
    for (const names of this.folded.values()) names.sort();
  }
  resolve(name) {
    const e = this.exact.get(name);
    if (e) return e;
    const k = this.folded.get(asciiLower(name));
    return k ? this.exact.get(k[0]) : undefined;
  }
  size(name) { const e = this.resolve(name); return e ? e.source.size() : -1; }
  /** 0 for assets given at start, a new number per replacement (set), -1 if missing. */
  version(name) { const e = this.resolve(name); return e ? e.version : -1; }
  readAt(name, offset, dst) { const e = this.resolve(name); return e ? e.source.readAt(offset, dst) : -1; }
  /** Names sorted by UTF-8 bytes (= code point order), like the native runner. */
  names() { return (this.sorted ??= [...this.exact.keys()].sort(byCodePoint)); }
}

/** In-memory provider from { name: Uint8Array } (explicit entries: exact names only). */
export function memoryAssets(record = {}) {
  const t = new AssetTable();
  for (const [name, bytes] of Object.entries(record)) t.add(name, bytesSource(bytes));
  return t.finish();
}

const joinName = (prefix, rel) => (prefix ? `${prefix.replace(/\/+$/, '')}/${rel}` : rel);

/**
 * Folder from showDirectoryPicker() (Chromium). Preloads every file into memory
 * (main-thread mode); in Worker mode prefer fileAssets()/opfsAssets() for lazy reads.
 * onProgress({ done, total, bytes, name }) is called per file.
 */
export async function directoryHandleAssets(handle, { prefix = '', onProgress, log } = {}) {
  return preload(await directoryHandleEntries(handle), prefix, onProgress, log);
}

/**
 * Folder from <input type="file" webkitdirectory> (all browsers). The leading
 * root-folder segment of webkitRelativePath is stripped, so names match the
 * native runner's --asset-dir. Preloads into memory (see directoryHandleAssets).
 */
export async function fileListAssets(fileList, { prefix = '', onProgress, log } = {}) {
  return preload(fileListEntries(fileList), prefix, onProgress, log);
}

/** [relative name, File] pairs from a webkitdirectory FileList (root segment stripped, hidden skipped). */
export function fileListEntries(fileList) {
  const out = [];
  for (const f of fileList) {
    const segs = (f.webkitRelativePath || f.name).split('/');
    const rel = segs.length > 1 ? segs.slice(1) : segs;
    if (!hidden(rel)) out.push([rel.join('/'), f]);
  }
  return out.sort((a, b) => (a[0] < b[0] ? -1 : a[0] > b[0] ? 1 : 0));
}

/** Preload [name, File|Blob] entries into memory as folder assets (main-thread mode). */
export async function preloadAssets(entries, { prefix = '', onProgress, log } = {}) {
  return preload(entries, prefix, onProgress, log);
}

/** [relative name, File] pairs from a showDirectoryPicker() handle (sorted, hidden skipped). */
export async function directoryHandleEntries(handle) {
  const files = [];
  await walkHandles(handle, async (path, h) => files.push([path, await h.getFile()]));
  return files;
}

/** Sorted, recursive walk of a directory handle (hidden entries skipped): onFile(path, fileHandle). */
async function walkHandles(dir, onFile, segs = []) {
  const entries = [];
  for await (const [name, h] of dir.entries()) entries.push([name, h]);
  entries.sort((a, b) => (a[0] < b[0] ? -1 : a[0] > b[0] ? 1 : 0));
  for (const [name, h] of entries) {
    const s = [...segs, name];
    if (hidden(s)) continue;
    if (h.kind === 'directory') await walkHandles(h, onFile, s);
    else await onFile(s.join('/'), h);
  }
}

async function preload(files, prefix, onProgress, log) {
  const t = new AssetTable(log);
  let bytes = 0;
  for (let i = 0; i < files.length; i++) {
    const [rel, file] = files[i];
    const data = new Uint8Array(await file.arrayBuffer());
    bytes += data.length;
    t.add(joinName(prefix, rel), bytesSource(data), { fromDir: true });
    onProgress?.({ done: i + 1, total: files.length, bytes, name: rel });
  }
  return t.finish();
}

/**
 * Worker mode only: lazy, synchronous reads from File/Blob objects via FileReaderSync
 * (e.g. a picked folder posted to the worker). entries: [[name, File], ...].
 */
export function fileAssets(entries, { prefix = '', log } = {}) {
  if (typeof FileReaderSync === 'undefined') throw new Error('fileAssets needs a Worker (FileReaderSync)');
  const reader = new FileReaderSync();
  const t = new AssetTable(log);
  for (const [rel, file] of entries) {
    t.add(joinName(prefix, rel), {
      size: () => file.size,
      readAt: (offset, dst) => {
        const n = Math.max(0, Math.min(dst.length, file.size - offset));
        if (n > 0) dst.set(new Uint8Array(reader.readAsArrayBuffer(file.slice(offset, offset + n))));
        return n;
      },
    }, { fromDir: true });
  }
  return t.finish();
}

/**
 * Worker mode only: a directory in the origin private file system, read lazily and
 * synchronously through FileSystemSyncAccessHandle. Nothing is preloaded: reads go
 * straight from OPFS into guest memory. Handles are opened once, while loading:
 * createSyncAccessHandle() is async and a synchronous read can't await it without
 * SharedArrayBuffer (which would need COOP/COEP headers). Opening is cheap (~450 files
 * for a CD), and each handle is then reused for every read.
 * `dir` is a path like "gasm-assets/mygame" (relative to the OPFS root) or a directory handle.
 */
export async function opfsAssets(dir, { prefix = '', log } = {}) {
  let handle = dir;
  if (typeof dir === 'string') {
    handle = await navigator.storage.getDirectory();
    for (const seg of dir.split('/').filter(Boolean)) handle = await handle.getDirectoryHandle(seg);
  }
  const t = new AssetTable(log);
  await walkHandles(handle, async (path, h) => {
    const access = await h.createSyncAccessHandle();
    const size0 = access.getSize();
    t.add(joinName(prefix, path), {
      size: () => { try { return access.getSize(); } catch { return size0; } },
      readAt: (offset, dst) => (dst.length ? access.read(dst, { at: offset }) : 0),
    }, { fromDir: true });
  });
  return t.finish();
}

