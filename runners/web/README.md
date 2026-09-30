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
  allowNet: true,                                // gasm:net (WebSocket)
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

A complete player (canvas, AudioWorklet audio, keyboard and Gamepad API,
catch-up handling) is `app.js` in the [repository](https://github.com/emdzej/gasm/tree/main/runners/web).

## Headless (Node ≥ 22)

```sh
npx -p @emdzej/gasm-host gasm-headless game.wasm --headless 600 --input "30-90:RIGHT+A"
# frames=600 presented=600 size=256x240
# video_fnv32=... audio_fnv32=... audio_frames=...
```

The hashes match the native runner (`gasm-run --headless`) bit for bit. That
is how gasm checks determinism across engines. Options: `--rom`, `--asset n=p`,
`--param k=v`, `--allow-net`, `--realtime`, `--screenshot out.png`, `--no-hash`.

- ABI: https://gasm.emdzej.pl/docs/abi
- Writing games: https://gasm.emdzej.pl/dev/games (Rust: the `gasm-sdk` crate; C: `gasm.h`)
- Writing runners: https://gasm.emdzej.pl/dev/runners

MIT licensed.
