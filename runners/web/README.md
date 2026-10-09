# @emdzej/gasm-host

Run **[gasm](https://gasm.emdzej.pl)** games in the browser or in Node. A gasm
game is one `.wasm` file; this package is the host that gives it pixels, sound,
input, a GPU (WebGPU), a network (WebSocket) and saves (IndexedDB).

```sh
npm install @emdzej/gasm-host
```

## In a web page

```js
import { GasmHost, IdbStorage } from '@emdzej/gasm-host';
import { WebGpuGfx } from '@emdzej/gasm-host/webgpu';

const canvas = document.querySelector('canvas');
const bytes = await (await fetch('sumo.wasm')).arrayBuffer();
const host = new GasmHost({
  gfx: await WebGpuGfx.create(canvas),           // 3D games (gasm:gfx)
  storage: await IdbStorage.open('sumo'),        // saves (gasm:storage)
  params: { room: 'friday' },                    // gasm.param
  allowNet: ['relay.example.org'],               // gasm:net and gasm:fetch: these hosts (true: all)
  ask: (subject) => confirm(`Allow ${subject}?`), // other hosts: the player decides ("net:<host>")
  getPad: (player) => (player === 0 ? keyboardMask : 0),
  onPresent: (rgba, w, h) => ctx.putImageData(new ImageData(rgba, w, h), 0, 0), // 2D games
});
await host.load(bytes);

let last = performance.now(), acc = 0;
requestAnimationFrame(function tick(now) {
  acc += Math.min(now - last, 100); last = now;
  while (acc >= 1000 / host.frameRate) { host.frame(); acc -= 1000 / host.frameRate; }
  requestAnimationFrame(tick);
});
addEventListener('pagehide', () => host.exit());
```

2D frames can also be scaled up with a filter (WebGL 2), the same ones as
`gasm-run --filter`:

```js
import { GlPresenter } from '@emdzej/gasm-host/present';
const presenter = GlPresenter.create(canvas);   // null without WebGL 2
// size: the canvas' display size in device pixels; filter: 'sharp' (default) | 'nearest' | 'xbr' | 'fsr' | 'crt'
onPresent: (rgba, w, h) => presenter.draw(rgba, w, h, size, { filter: 'xbr', integerScale: false }),
```

With `integerScale`, set `BrowserInput.integerScale` too, so the pointer's frame
position follows the smaller letterbox.

The gasm splash screen (the same frames as `gasm-run` and the player) while the
game loads, on a canvas of its own or through your own draw function:

```js
import { playSplash } from '@emdzej/gasm-host/splash';
const splash = playSplash(splashCanvas);   // or playSplash((rgba, w, h) => presenter.draw(rgba, w, h, size))
await host.load(bytes);                    // loading meanwhile; it holds on the logo if that takes longer
await splash.ready();                      // the last frames, then it resolves (a key or click shortens it)
```

A complete player (canvas, AudioWorklet audio, keyboard and Gamepad API,
catch-up handling) is `app.js` in the [repository](https://github.com/emdzej/gasm/tree/main/runners/web).

## Big data sets: asset providers and Worker mode

Assets can be a `{ name: bytes }` record or any synchronous
`GasmAssetProvider` (`size(name)`, `readAt(name, offset, dst)`). `AssetTable`
applies gasm's naming rules (folder entries match case-insensitively, hidden
files skipped). Helpers: `directoryHandleAssets` / `fileListAssets` (preload a
picked folder, with progress), and in Workers `opfsAssets` / `fileAssets`
(lazy, synchronous reads). Run such games off the main thread:

```js
import { GasmWorker } from '@emdzej/gasm-host/worker';
const w = await GasmWorker.start({ wasm, assets: [{ kind: 'opfs', dir: 'gasm-assets/my-cd' }], onAudio });
const r = await w.frames([[pads0, pads1, 0, 0]]);   // r.frame: { rgba, width, height }
```

`wasm` is the module's bytes or a compiled `WebAssembly.Module`.

No `SharedArrayBuffer` and no COOP/COEP headers are needed (works on GitHub
Pages), except for threads (below). `gasm:gl` games go to the worker with a
canvas from `transferControlToOffscreen()` (`glCanvas`, plus the display `size`
with each batch). Keyboard layouts: `DEFAULT_KEYMAP`, `parseKeymap(text)`,
`keyboardPads(bindings, heldCodes, gamepadCount)`.

## Threads

Games built with threads (wasi-threads: `wasm32-wasip1-threads`, an imported
shared memory) get worker threads with `threads: n` (`GasmHost`, `GasmWorker`,
`gasm-headless --threads n`): a pool of Workers, started with the game, sharing
its memory, as many as `gasm.max_threads()` reports. Each is parked until the
game spawns a thread, so spawning is immediate. They need `SharedArrayBuffer`:
Node, or a cross-origin isolated page (`Cross-Origin-Opener-Policy:
same-origin`, `Cross-Origin-Embedder-Policy: require-corp`) with the game in
Worker mode (the page's own thread can't wait). Pass the module's `bytes` with a
compiled Module (`load(module, { bytes })`, `GasmWorker.start({ wasm, bytes })`):
the shared memory's size is only in the binary. Worker threads read assets whose
source can describe itself (`share()`: in-memory bytes, picked files, OPFS,
files in Node); a trap on any thread ends the game. Elsewhere `thread-spawn`
fails and the game does its work on one thread.

For hosts that can't send the headers (GitHub Pages), `isolate-sw.js` is a
service worker that adds them; the player registers it through `isolate.js`.

## Host API notes

- `host.runFrames(steps, show)` runs a catch-up batch, one frame per step
  (`{ pads, text, input }`), showing only the last.
- `host.shutdown()` calls the guest's `gasm_exit`, then flushes and closes
  network connections and storage. After a trap or `proc_exit`, `host.dead`
  holds the reason and the guest is never called again.
- Storage backends implement `get`, `keys`, `set` and `delete`; `set` throws
  `StorageError`, whose `code` (`STORAGE_ERR_*`) is returned to the guest.
- `GfxModel` validates every `gasm:gfx` call and numbers the handles, the same
  way as the native runner, for any backend. A `GfxBackend` only executes:
  its methods get the handle as the first argument
  (`createBuffer(handle, size, usage)`). See `gasm-host.d.ts`.
- Network: `allowNet` (`true` or host names) and `ask(subject)` for the rest
  (`"net:<host>"`; return a boolean or a promise: the request waits for the
  player). After `load`, `host.manifest` is the game's capabilities manifest
  (`parseManifest`, `moduleManifest`); `load` throws if it `requires` something
  this host lacks.
- `onCopyText(text)` (gasm:clipboard) and a step's `paste`; `onSaveFile(name,
  mime, bytes)` (gasm:files: a download, a file, or `false`).
- The WASI subset (`lib/wasi.js`) matches the native runner's: guest stdout
  goes to `onLog`, and with `virtualTime` the clocks are virtual and
  `random_get` is a fixed sequence.

## Headless (Node ≥ 22)

```sh
npx -p @emdzej/gasm-host gasm-headless game.wasm --headless 600 --input "30-90:RIGHT+A"
# frames=600 presented=600 size=256x240
# video_fnv32=... audio_fnv32=... audio_frames=...
```

The hashes match the native runner (`gasm-run --headless`) bit for bit. That
is how gasm checks determinism across engines. Options: `--headless N`, `--rom`,
`--asset n=p`, `--asset-dir [prefix=]dir`, `--param k=v`, `--allow-net`,
`--storage-dir dir` (saves on disk; default: in memory), `--storage-id id`,
`--input script`, `--screenshot out.png`, `--realtime`, `--no-hash`. Unknown
options are an error. Node has no GPU: `--screenshot` captures `video_present`
frames only (use `gasm-run --screenshot` for `gasm:gfx` games).

- ABI: https://gasm.emdzej.pl/docs/abi
- Writing games: https://gasm.emdzej.pl/dev/games (Rust: the `gasm-sdk` crate; C: `gasm.h`)
- Writing runners: https://gasm.emdzej.pl/dev/runners

MIT licensed.
