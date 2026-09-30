# Demos

Every demo below is the **same `.wasm` file** you can also run natively with
`gasm-run`. It runs here in the browser runner, which is gasm's JavaScript
host with WebGPU for 3D.

<div class="demo-grid">
  <a class="demo-card" href="/play/?game=sumo.wasm&autostart" target="_blank"><strong>Sumo (3D)</strong><span>Push the other ball off the platform. Arrows move, X or Z dashes. Plays a bot, or a friend online through a relay.</span></a>
  <a class="demo-card" href="/play/?game=triangle.wasm&autostart" target="_blank"><strong>GPU triangle</strong><span>The smallest gasm:gfx program: one shader, one vertex buffer, about 50 lines of Rust.</span></a>
  <a class="demo-card" href="/play/?game=nes.wasm" target="_blank"><strong>NES emulator</strong><span>tetanes-core compiled to wasm. Open or drop your own <code>.nes</code> file; nothing is uploaded.</span></a>
  <a class="demo-card" href="/play/?game=test-pattern.wasm&autostart" target="_blank"><strong>Test pattern (C)</strong><span>A 70-line C game built with wasi-sdk: gradient, movable square, a tone while A is held.</span></a>
</div>

## Try sumo right here

Click into the frame to give it keyboard focus. Arrows move; **X** or **Z**
dashes. A gamepad works too. Needs a browser with WebGPU (Chrome, Edge, Safari
26+, Firefox 141+ on Windows).

<iframe class="demo-frame" src="/play/?game=sumo.wasm&autostart" title="gasm sumo demo" allow="gamepad; autoplay"></iframe>

## Play sumo online

Online play needs a **relay**, a tiny WebSocket server that pairs players in
rooms. This site is static, so it doesn't include one. Run your own:

```sh
git clone https://github.com/emdzej/gasm && cd gasm && make
runners/native/target/release/gasm-relay 0.0.0.0:9000
```

Both players open [the sumo demo](/play/?game=sumo.wasm), enter the relay URL
and the same room name, and press start. Native players join with
`gasm-run build/sumo.wasm --allow-net --param relay=ws://HOST:9000 --param room=NAME`.

::: warning HTTPS pages need wss://
This site is served over HTTPS, so browsers only allow **`wss://`** relays from
it. Give `gasm-relay` a certificate (`--tls-cert fullchain.pem --tls-key privkey.pem`)
or put it behind a TLS proxy (for example Caddy: `reverse_proxy localhost:9000`).
Alternatively, run the web runner locally with `make web`, which works with plain `ws://`.
:::

## Run the demos locally

```sh
make && make roms && make web
# http://localhost:8080/runners/web/  (includes test ROMs and homebrew for the NES demo)
```
