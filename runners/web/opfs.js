// OPFS import page: copies a picked folder into the origin private file system with
// csfs (read side: csfs-fsa or a webkitdirectory FileList; write side: csfs-opfs),
// under gasm-assets/<name>/. Games then read it lazily via gasm's OPFS asset
// provider in Worker mode (index.html?opfs=gasm-assets/<name>).
import { walkFileSystem } from '@emdzej/csfs-core';
import { fsaFileSystem, isFsaSupported, pickDirectory } from '@emdzej/csfs-fsa';
import { isOpfsSupported, opfsFileSystem, persist } from '@emdzej/csfs-opfs';
import { testStream } from './testdata.js';

const ROOT = 'gasm-assets';
const $ = (id) => document.getElementById(id);
const log = (m) => { $('log').textContent = m; console.log(m); };
const mb = (n) => `${(n / 1048576).toFixed(1)} MB`;
const cleanName = (s) => s.trim().replace(/[^A-Za-z0-9._-]+/g, '-').replace(/^[-.]+/, '') || 'data';

async function target(name) {
  await persist().catch(() => {}); // ask the browser not to evict it under storage pressure
  return opfsFileSystem({ namespace: `${ROOT}/${name}` });
}

/** Copy entries [[relative path, () => ReadableStream, size]] into OPFS, with progress. */
async function copyInto(name, entries) {
  const fs = await target(name);
  const total = entries.reduce((n, e) => n + e[2], 0);
  let done = 0;
  const t0 = performance.now();
  for (let i = 0; i < entries.length; i++) {
    const [path, stream, size] = entries[i];
    await fs.write(`/${path}`, stream());
    done += size;
    $('progress').value = total ? done / total : 1;
    log(`${name}: ${i + 1}/${entries.length} files, ${mb(done)} of ${mb(total)}  ${path}`);
  }
  log(`imported ${entries.length} files, ${mb(total)} into ${ROOT}/${name} in ${((performance.now() - t0) / 1000).toFixed(1)} s`);
  await refresh();
  return { files: entries.length, bytes: total };
}

async function importPicked() {
  if (!isFsaSupported()) return $('folderinput').click(); // Firefox, Safari: webkitdirectory
  let handle;
  try { handle = await pickDirectory('read'); } catch (e) { if (e.name !== 'AbortError') log(e.message); return; }
  const src = fsaFileSystem(handle);
  const entries = [];
  for await (const e of walkFileSystem(src)) {
    if (e.kind !== 'file' || e.path.split('/').some((s) => s.startsWith('.'))) continue;
    entries.push([e.path.replace(/^\//, ''), () => src.file(e.path).then((f) => f.stream()), e.size]);
  }
  // csfs file() is async: wrap into a stream that opens lazily
  const lazy = entries.map(([p, open, size]) => [p, () => lazyStream(open), size]);
  await copyInto(cleanName($('name').value || handle.name), lazy);
}

function lazyStream(open) {
  let reader;
  return new ReadableStream({
    async pull(ctrl) {
      reader ??= (await open()).getReader();
      const { value, done } = await reader.read();
      if (done) ctrl.close(); else ctrl.enqueue(value);
    },
  });
}

$('folderinput').onchange = async (e) => {
  const files = [...e.target.files];
  if (!files.length) return;
  const root = files[0].webkitRelativePath.split('/')[0];
  const entries = files
    .map((f) => [f.webkitRelativePath.split('/').slice(1).join('/'), () => f.stream(), f.size])
    .filter(([p]) => p && !p.split('/').some((s) => s.startsWith('.')));
  await copyInto(cleanName($('name').value || root), entries);
};

/** Synthetic data set for tests: big.bin (deterministic) + small files in a CD-like layout. */
async function generate(name, sizeMb) {
  const text = (s) => () => new Blob([s]).stream();
  const size = sizeMb * 1048576;
  return copyInto(name, [
    ['big.bin', () => testStream(size), size],
    ['ART/ART.CAR', text('ART CAR FILE v1\n'), 16],
    ['README.TXT', text('RETURN FIRE TEST DISC\n'), 22],
  ]);
}

async function refresh() {
  const tbody = $('list');
  tbody.textContent = '';
  let root;
  try {
    root = await (await navigator.storage.getDirectory()).getDirectoryHandle(ROOT, { create: true });
  } catch (e) { log(`OPFS unavailable: ${e.message}`); return; }
  for await (const [name, h] of root.entries()) {
    if (h.kind !== 'directory') continue;
    const fs = await opfsFileSystem({ namespace: `${ROOT}/${name}` });
    let files = 0, bytes = 0;
    for await (const e of walkFileSystem(fs)) if (e.kind === 'file') { files++; bytes += e.size; }
    const tr = document.createElement('tr');
    tr.innerHTML = `<td>${name}</td><td>${files}</td><td>${mb(bytes)}</td>
      <td><a href="index.html?game=assetcheck.wasm&opfs=${ROOT}/${name}&read=art/art.car,readme.txt&stream=big.bin&autostart">test</a>
      · <button data-del="${name}">delete</button></td>`;
    tbody.append(tr);
  }
  tbody.querySelectorAll('[data-del]').forEach((b) => (b.onclick = async () => {
    await root.removeEntry(b.dataset.del, { recursive: true });
    refresh();
  }));
  const est = await navigator.storage.estimate?.();
  if (est) $('quota').textContent = `Storage: ${mb(est.usage ?? 0)} used of ${mb(est.quota ?? 0)}.`;
}

$('pick').onclick = importPicked;
$('generate').onclick = () => generate(cleanName($('name').value || 'test-data'), Number($('mb').value) || 200);

if (!isOpfsSupported()) log('This browser has no origin private file system.');
else refresh();

// ?generate=<name>&mb=<n>: create test data, then (with &then=<url>) continue, e.g. to a game.
const q = new URLSearchParams(location.search);
if (q.has('generate')) {
  generate(cleanName(q.get('generate')), Number(q.get('mb') ?? 200)).then((r) => {
    globalThis.__gasmImported = r;
    if (q.get('then')) location.href = q.get('then');
  }, (e) => log(`generate failed: ${e.message}`));
}
