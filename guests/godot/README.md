# Godot on gasm

[Godot](https://godotengine.org) 4.7 with a gasm platform: Godot games exported
as a `.pck` run on every gasm runner (native window, browser, headless), with
the **Compatibility renderer** on `gasm:gl` (OpenGL ES 3, WebGL 2 rules:
WebGL 2 in browsers, ANGLE natively). One engine module, `godot.wasm`, runs any
project; the game is its pack, given as the asset `game.pck`:

```sh
gasm-run godot.wasm --asset game.pck=mygame.pck
```

In the player: `?game=godot-scene3d`, or open your own `.pck` with the engine.

**Making a game for it** (project settings, exporting, input, saves, testing,
shipping): [Godot games on gasm](https://gasm.emdzej.pl/dev/godot). This README
is about the port itself.

## Examples

Each example project shows one part of Godot on gasm. They are determinism
cases (the same hashes on wasmtime JIT, AOT and V8) and demos on the website.

| Project | Shows |
|---|---|
| [`examples/hello2d`](examples/hello2d) | GDScript, drawing, an imported SVG texture, text, input actions |
| [`examples/platformer`](examples/platformer) | 2D physics (`CharacterBody2D`, static bodies), `Area2D` signals, a following camera, gamepad |
| [`examples/scene3d`](examples/scene3d) | 3D in the Compatibility renderer: procedural sky, fog, a shadowed sun, PBR materials, a custom shader (`stripes.gdshader`, with a `varying`), 3D physics |
| [`examples/ui`](examples/ui) | Controls (`LineEdit`, `CheckBox`, `HSlider`, `OptionButton`, `ItemList`), text input, saves with `ConfigFile` in `user://` |
| [`examples/audio`](examples/audio) | `AudioStreamGenerator` synthesis, a WAV built in code, a reverb bus, the spectrum analyzer |
| [`examples/http`](examples/http) | `HTTPRequest` GETs and a POST on gasm:fetch (against `scripts/fetch-server.mjs`; replayed in the determinism suite) |
| [`examples/mods`](examples/mods), [`examples/modpack`](examples/modpack) | Mods: the game loads every pack from `--mods <dir>` (`Gasm.get_mods()`, `load_resource_pack`); the mod replaces a settings file and adds a scene |
| [`examples/relaymp`](examples/relaymp) | High-level multiplayer: RPCs through a `gasm-relay` room with `Gasm.create_relay_peer` (the first player serves) |
| [`examples/net`](examples/net) | Multiplayer: `WebSocketPeer` on gasm:net in a `gasm-relay` room, a square per player (`--allow-net --param relay=ws://host:9000/room`) |

`make godot` builds the engine (`build/godot.wasm`), the smaller engine without
3D for 2D games (`build/godot-2d.wasm`: 26.4 MB, 6.7 MB gzipped, against 32.1 /
8.1 MB) and exports the examples (`build/godot/*.pck`). `make godot-custom
GODOT_PROFILE=game.gdbuild` builds a game's own engine from a Godot build
profile.

## Exporting your game

1. In Godot 4.7, set the project's renderer to **Compatibility**
   (`rendering/renderer/rendering_method="gl_compatibility"`).
2. Export a **pack** (Project > Export > *any desktop preset* > Export PCK/ZIP,
   or `godot --headless --export-pack <preset> game.pck`). No export templates
   are needed for a pack.
3. Run it: `gasm-run godot.wasm --asset game.pck=game.pck`, or in the browser
   player.

For reproducible packs (what the examples do): commit the `.uid` and `.import`
files, and set `editor/export/convert_text_resources_to_binary=false` (binary
scenes get random node ids at export).

## What works, what doesn't

- **Rendering:** the Compatibility renderer (2D and 3D). Forward+ and Mobile
  need Vulkan/Metal/D3D12, which gasm doesn't have.
- **Scripting:** GDScript. No C# (needs .NET) and no GDExtension (no dynamic
  libraries on gasm).
- **Physics:** Godot Physics 2D and 3D. Jolt doesn't build for WASI yet.
- **Input:** keyboard (physical keys), typed text, mouse (including captured
  mode), gamepads (W3C standard mapping). No touch.
- **Audio:** 44.1 kHz stereo (the project's mix rate), mixed once per frame.
- **Files:** `res://` is the pack; `user://` is `gasm:storage` (saves persist
  natively and in the browser). Storage keys are paths with `/` encoded, at
  most 128 bytes; a file is at most 1 MiB, 16 MiB per game.
- **HTTP:** `HTTPRequest`/`HTTPClient` on gasm:fetch (the runner makes the
  requests, `https://` included; natively with `--allow-net`, or the player
  agrees when asked).
- **WebSockets:** `WebSocketPeer` (and `WebSocketMultiplayerPeer`) as a client
  on gasm:net (`websocket_peer_gasm.cpp`): binary messages, no servers.
- **Clipboard:** `DisplayServer.clipboard_set`/`clipboard_get` on
  gasm:clipboard; pasting works while handling Ctrl+V (Ctrl on every system).
- **Files for the player, launch parameters, mods:** the `Gasm` singleton
  (`platform/gasm/api`): `save_file(bytes, name, mime)` on gasm:files (a picture
  in Pictures/<game>/, a download in browsers), `get_param(name)`,
  `get_mods()` / `get_refused_mods()` (resource packs from `--mods`).
- **The game's manifest:** `godot.wasm` is shared by every game, so a game's
  capabilities manifest (hosts, saves) comes as asset `gasm.manifest` or
  `gasm-run --manifest`.
- **Threads:** `godot.wasm` is built with `threads=no` (work runs on the main
  thread); `godot-mt.wasm` (`threads=yes`, `make godot`) runs `WorkerThreadPool`
  and threaded loading on OS threads in `gasm-run` (`--threads`), and on the
  calling thread where the runner allows none (headless, browsers).
- **Not available:** raw sockets and servers (`ENet`,
  `StreamPeerTCP`, `TCPServer`), TLS in the engine (`Crypto`), complex text
  shaping (the fallback text server: no right-to-left or ligatures), several
  windows.
- **Time:** frames are paced by the runner (60 Hz); `OS.get_ticks_usec()` is
  the frame's time, virtual in headless runs, so runs are reproducible. Local
  time and the time zone's offset come from the runner (`gasm.utc_offset_minutes`;
  UTC in headless runs).

## How it's built

`scripts/fetch-godot.sh` puts the Godot 4.7.2 source in `tools/godot-src`
(pinned), applies [`godot.patch`](godot.patch) (Godot's WebGL code paths in
`drivers/gles3` also for gasm; no `HTTPClientTCP`) and copies [`platform/gasm`](platform/gasm) in.
The platform (MIT, like Godot):

| File | What |
|---|---|
| `detect.py` | wasi-sdk, `wasm32-wasip1` reactor (`wasm32-wasip1-threads` with an imported shared memory for `threads=yes`), LTO, setjmp on wasm exceptions, modules off that need what gasm doesn't have |
| `gasm_main.cpp` | the exports: `gasm_init` runs `Main::setup`/`start` with `--main-pack`, `gasm_frame` one `Main::iteration()` |
| `os_gasm.*` | `OS`: time, entropy, paths, no processes |
| `display_server_gasm.*` | one window on `gasm:gl`; keys, text, pointer and gamepads as Godot input events |
| `audio_driver_gasm.*` | mixes a frame of audio and pushes it (`gasm_audio_push`) |
| `file_access_gasm.*` | `FileAccess`/`DirAccess` on gasm assets and `gasm:storage` |
| `ip_gasm.h` | no host names or interfaces |
| `http_client_gasm.*` | `HTTPClient` on `gasm:fetch` (so `HTTPRequest` works); `godot.patch` leaves `HTTPClientTCP` out |
| `platform_gl.h`, `emscripten/html5_webgl.h` | GLES 3 from the C SDK; what Godot's WebGL paths ask of Emscripten |

The module is linked with `wasm-ld`, then `wasm-opt -Oz` (keeping its exact
wasm features: setjmp/longjmp use wasm exceptions, which every gasm runner
has).
