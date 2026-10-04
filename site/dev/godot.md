# Godot games on gasm

[Godot](https://godotengine.org) 4.7 runs on gasm. The engine is one module,
`godot.wasm`; a game is the `.pck` the Godot editor exports. The same pair runs
in `gasm-run`'s window, in the browser player and headless, with Godot's
**Compatibility renderer** on [`gasm:gl`](/dev/graphics#gasm-gl-opengl-es-3-0)
(WebGL 2 in browsers, ANGLE natively).

```sh
gasm-run godot.wasm --asset game.pck=mygame.pck
```

The five example projects in
[`guests/godot/examples`](https://github.com/emdzej/gasm/tree/main/guests/godot/examples)
each show one part of the engine (2D, physics, 3D, UI and saves, audio); they
are playable on the [demos page](/demos/#godot).

## 1. Set up the project

Use the **Godot 4.7** editor, the engine module's version (packs from newer
versions are refused; older 4.x packs may load, but export with 4.7 to be sure). In Project Settings:

| Setting | Value | Why |
|---|---|---|
| Rendering > Renderer > Rendering Method | `gl_compatibility` (also for Mobile) | the only renderer on gasm (Forward+ and Mobile need Vulkan, Metal or D3D12). A Forward+ project falls back to it by itself, but looks different from what the editor showed |
| Display > Window > Stretch > Mode | `canvas_items` or `viewport` | the drawable is whatever size the player's window is |
| Physics > Common > Physics Ticks per Second | `60` | frames come at 60 Hz |

In `project.godot`:

```ini
[rendering]
renderer/rendering_method="gl_compatibility"
renderer/rendering_method.mobile="gl_compatibility"

[display]
window/size/viewport_width=640
window/size/viewport_height=360
window/stretch/mode="canvas_items"
```

GDScript works; C# and GDExtension don't (see [Limits](#limits)).

## 2. Export a pack

Add any desktop export preset (Project > Export > Add... > Linux, for example)
and press **Export PCK/ZIP...**, saving as `game.pck`. No export templates are
needed for a pack. From the command line:

```sh
godot --headless --path mygame --export-pack "Linux" game.pck
```

The pack is the whole game: scenes, scripts, imported textures, sounds.

## 3. Run it

| Where | How |
|---|---|
| `gasm-run` window | `gasm-run godot.wasm --asset game.pck=game.pck` (release bundles: `run-godot game.pck`) |
| Browser player | choose **Godot: your game (.pck)**, then open or drop the `.pck` |
| Headless | `gasm-run godot.wasm --asset game.pck=game.pck --headless 600 --screenshot out.png` |
| Your own page | `@emdzej/gasm-host` with `assets: { 'game.pck': bytes }` and a WebGL 2 context (`gl` option) |

Launch parameters: `pck=<asset name>` picks another asset than `game.pck`;
`args=...` adds a Godot command line (`--param "args=--verbose"`).

**Start-up time.** Natively, wasmtime compiles the 32 MB engine (8 MB gzipped)
when it loads it: about 5 s on an M1 Pro. Precompile it once and the start is
instant (130 ms measured):

```sh
gasm-run godot.wasm --compile godot.cwasm
gasm-run godot.cwasm --allow-precompiled --asset game.pck=game.pck
```

(A `.cwasm` is native code for one machine and wasmtime version: make it where
it runs, or ship one per platform.) Browsers compile lazily and start in under
a second.

## 4. Input

| Godot | gasm |
|---|---|
| `InputEventKey` | the raw keyboard: physical keys (`keycode` = `physical_keycode`, the US layout), modifiers |
| typed text (`LineEdit`, `TextEdit`) | `gasm.text_input`: key events carrying `unicode` |
| `InputEventMouseButton`, `InputEventMouseMotion`, wheel | the pointer (drawable pixels; Godot maps them to the viewport) |
| `Input.mouse_mode = MOUSE_MODE_CAPTURED` / `HIDDEN` | pointer lock / hidden cursor |
| joypads | gamepads 0 to 3, W3C standard layout (buttons, sticks, triggers as axes) |

Input actions (`ui_left`, your own) work as usual. Godot reads the keyboard
itself, so gasm's built-in keyboard-to-pad layout is off for Godot games. No
touch yet.

Other assets than the pack are files in `res://` too:
`--asset levels/extra.json=...` is `res://levels/extra.json`. An asset the
launcher or the page replaces while the game runs (`--watch-asset`,
`setAsset`, see [assets that change](/docs/abi#assets-that-change)) gets a new
`FileAccess.get_modified_time("res://...")`: poll it and re-read.

## 5. Saves

`user://` is [`gasm:storage`](/docs/abi#gasm-storage-optional-persistent-key-value-store):
`FileAccess`, `ConfigFile` and `ResourceSaver` work there, and saves persist
(natively in the data directory, in the browser in IndexedDB). Each file is one
storage key, so:

- a file is at most 1 MiB, all of a game's saves 16 MiB;
- a path (relative to `user://`, with `/` and other characters encoded) is at
  most 128 bytes;
- files are written when they are closed (`close()`, or when the `FileAccess`
  goes away).

Headless runs start with empty storage (add `--storage-dir DIR` to keep it). The
[ui example](https://github.com/emdzej/gasm/blob/main/guests/godot/examples/ui/main.gd)
saves and loads a settings file. When the player quits, the game gets
`NOTIFICATION_WM_CLOSE_REQUEST` and one more frame: save there.

## 6. Audio, time, windows

- **Audio** is mixed once per frame at the project's mix rate (44.1 kHz by
  default), stereo: players, buses, effects and `AudioStreamGenerator` work.
- **Time** is the frame's: `_process(delta)` gets 1/60 s steps, and
  `Time.get_ticks_usec()` follows the frames (virtual in headless runs).
  `Engine.max_fps` has no effect: the runner paces frames.
- **The date** is real: `Time.get_unix_time_from_system()` and
  `Time.get_datetime_dict_from_system()` give the player's clock and local time,
  `Time.get_time_zone_from_system()` their UTC offset (`{"bias": 120, "name":
  "UTC+02:00"}`; no zone names). Headless runs are reproducible: the clock starts
  at 1970-01-01 00:00 UTC.
- **The window** is the gasm drawable: its size is the player's window or canvas,
  `DisplayServer.window_set_title` sets the title, and `get_tree().quit(code)`
  ends the game with that exit code. There is one window (no popups as separate
  windows) and no fullscreen switching from the game.

## 7. Test it with hashes

Headless runs are reproducible: virtual time, a fixed random sequence, the null
GL. Script the input and compare the hashes between runs (and runners) to catch
nondeterminism and regressions:

```sh
gasm-run godot.wasm --asset game.pck=game.pck --headless 300 \
  --input '30-200:KEY(ArrowRight),60-64:KEY(Space),120:PTR(640,360),121-123:PTR(640,360,L)'
# frames=300 presented=0 size=0x0
# video_fnv32=... audio_fnv32=... audio_frames=...
npx -p @emdzej/gasm-host gasm-headless godot.wasm --asset game.pck=game.pck --headless 300 --input '...'   # the same hashes
```

Use `KEY(...)` for keys (Godot reads the raw keyboard), `PTR(x,y,L)` for clicks
in drawable pixels (1280x720 headless), `"text"` for typing. The video hash
covers every GL upload, so it only compares on the null GL: with
`--screenshot` the run renders on the GPU, and Godot adapts to the GPU's
extensions. A different engine build can change the hashes too (Godot's
internal hash maps follow memory layout): compare runs of the same
`godot.wasm`.

For packs that come out the same on every machine, commit the `.uid` and
`.import` files and set `editor/export/convert_text_resources_to_binary=false`
(binary scenes get random node ids at export).

## 8. Ship it

A Godot game on gasm is `godot.wasm` plus its `.pck`:

- **Natively:** the [release bundles](https://github.com/emdzej/gasm/releases)
  have `gasm-run`, ANGLE and `games/godot.wasm`; add your pack and a launcher
  (`run-godot.sh` shows how), or a precompiled `godot.cwasm` for a fast start.
- **On the web:** host the player (or your own page with `@emdzej/gasm-host`),
  `godot.wasm` and the pack; serve them compressed (the engine gzips to 8 MB).
- **Licenses:** Godot is MIT; ship its `LICENSE.txt` and `COPYRIGHT.txt` (the
  third-party components) with the engine, as the release's `THIRD-PARTY.txt`
  does.

## Limits

| | |
|---|---|
| Renderer | Compatibility only (2D and 3D, WebGL 2's feature set: no compute shaders) |
| Scripting | GDScript; no C# (.NET) and no GDExtension (no dynamic libraries) |
| Threads | the engine is single-threaded (`threads=no`): `Thread` and `WorkerThreadPool` run their work on the main thread |
| Physics | Godot Physics 2D and 3D (Jolt doesn't build for WASI yet) |
| Networking | none: `HTTPRequest`, WebSockets, ENet need sockets; no TLS or `Crypto` |
| Text | the fallback text server: no right-to-left scripts or ligatures |
| Input | no touch, no IME |

What's planned is on the [roadmap](/docs/roadmap#godot).

## Troubleshooting

| Symptom | Fix |
|---|---|
| `godot: setup failed (is game.pck an asset?)` | pass the pack: `--asset game.pck=...` (or `--param pck=<name>`) |
| looks different from the editor (lighting, glow, shadows) | the project uses Forward+ or Mobile, and gasm runs it with Compatibility: switch the project to `gl_compatibility` to see in the editor what players get |
| `Pack created with a newer version of the engine` | export with the Godot 4.7 editor |
| `user://` file missing after a restart | it wasn't closed; or its path is over 128 bytes encoded; or the run was headless without `--storage-dir` |
| hashes differ between two runs | something reads real time or OS randomness outside Godot's APIs, or one run had `--screenshot` |

## The engine module

`make godot` builds `build/godot.wasm` from the Godot 4.7.2 source with gasm's
platform and exports the examples. How the platform is built (and what each file
does): [guests/godot](https://github.com/emdzej/gasm/blob/main/guests/godot/README.md).
