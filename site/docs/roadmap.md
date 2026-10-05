# Roadmap

Everything that is planned or known to be missing, in one place. What already
shipped is in the
[CHANGELOG](https://github.com/emdzej/gasm/blob/main/CHANGELOG.md); larger
items have a design document in
[`design/`](https://github.com/emdzej/gasm/tree/main/design) with the details
and a phased plan.

Nothing here is promised or scheduled. ABI additions keep `GASM_ABI_VERSION`
at 0 (older runners trap only if a guest calls something they lack, see
[Versioning](/docs/abi#gasm-abi-v0)); breaking changes would bump it.

## Next up

The suggested order, from the most benefit for the least risk:

Done recently: stack switching (`gasm_run`,
[design/stack-switching.md](https://github.com/emdzej/gasm/blob/main/design/stack-switching.md)),
cooperative threads in C, POSIX and SDL 3
([design/threads.md](https://github.com/emdzej/gasm/blob/main/design/threads.md))
`gasm:gl` on every runner: WebGL 2 in browsers, ANGLE natively, and from
Rust through glow
([design/gasm-gl.md](https://github.com/emdzej/gasm/blob/main/design/gasm-gl.md)),
threads in Rust, Godot 4.7
([guests/godot](https://github.com/emdzej/gasm/blob/main/guests/godot/README.md)),
and in 0.9.0 HTTP requests (`gasm:fetch`,
[design/fetch.md](https://github.com/emdzej/gasm/blob/main/design/fetch.md)),
assets that change while a game runs and the player's time zone.

1. **Godot, the rest** (below): threads, Jolt, TLS, multiplayer.

## Runtime and ABI

| Item | What it gives | Status |
|---|---|---|
| Stack switching beyond JSPI | Browsers without JSPI (and Node 22) still need the Asyncify builds; wasm's stack-switching proposal would cover them too. | waiting on engines |
| Real wasm threads | Shared memory and atomics, opt-in, for guests that need parallel CPU (physics, job systems, Godot's worker pool). Not deterministic. | proposal, after cooperative threads: [design/threads.md](https://github.com/emdzej/gasm/blob/main/design/threads.md) (part B) |
| Guest memory limit | A cap on linear memory growth. Today a guest can grow to the engine maximum (4 GiB for wasm32); needed before running untrusted content. | not started |
| Player consent | The runner asks the player before a game reaches anything outside itself: network connections (`gasm:net`, `gasm:fetch`; per host), files beyond its own assets, other external resources. Natively a prompt in the window (`--allow-net` and friends answer it in advance), in the browser one in the page; the answer remembered per game. Open: which resources count (assets outside the asset dir? storage?), how fine-grained (per host, per session), what headless runs do, and how it fits the capabilities manifest below. | to be discussed |
| Capabilities manifest | Custom section `gasm.manifest` declaring required and optional imports, network hosts and platform extensions (`gasm:ext/*`), so runners can check a game before running it. | not started |
| `.gasm` packages | One file bundling `game.wasm`, its assets and a manifest. | not started |
| `gasm:files` | A file picker run by the runner: the game only sees what the player picks. | not started |
| WIT / Component Model | Move the ABI to WIT once browsers don't need transpiling for components. | waiting on browsers |

## Graphics

| Item | What it gives | Status |
|---|---|---|
| `gasm:gl` on Wayland | ANGLE draws into X11 windows, so `gasm:gl` games use XWayland on Wayland desktops. | not started |
| `gasm:gl` on SwiftShader on macOS | `--gl-software` needs a Vulkan loader there (Electron doesn't ship one); Metal is always available, so it only matters for tests. | not planned |
| ANGLE built from source | The libraries come from Electron 43, the last release that ships them as separate files; a newer ANGLE means building it (depot_tools) or another distribution. | when needed |
| `gasm:gl` in Worker mode | WebGL 2 on a transferred `OffscreenCanvas`; `gasm:gl` games run on the main thread for now. | not started |
| glow upstream | Rust GL code runs on a fork of glow (`sdk/glow`: its native backend on wasm32), used through `[patch.crates-io]`. Upstream support (a loader-based backend on `wasm32-unknown-unknown`) would make the patch unnecessary. | idea |
| A display scale | Guests only see drawable pixels, so UI code guesses a scale (the egui demo uses the drawable height). A `gasm.display_scale()` import would give the real one. | idea |
| `gasm:gfx` render targets | Render-to-texture. | not started |
| `gasm:gfx` cube maps, render bundles | Skyboxes and environment maps; cheaper repeated draws. | not started |
| Batched gfx commands | One call per frame for draw-heavy guests instead of one per command. | not started |
| More of WebGPU | Storage textures, compute, stencil: not in the `gasm:gfx` subset and not planned yet. | not planned |

### Presentation (2D frames)

The filters, integer scaling, display aspect and window titles shipped in 0.6.0
([design/presentation.md](https://github.com/emdzej/gasm/blob/main/design/presentation.md)).
Left over:

| Item | What it gives | Status |
|---|---|---|
| `mmpx`, `scalefx` filters | More pixel-art upscalers. `xbr` covers the same content; both need porting from their reference sources, and ScaleFX's license needs checking. | declined for now |
| Content hint | A guest suggests a filter class ("pixel art" or "rendered") so players can pick a better default. One more small ABI addition. | idea |
| fps counter behind a flag | The native window title shows `— <n> fps` always. | idea |

## Runners

| Item | What it gives | Status |
|---|---|---|
| gasm splash screen | A short intro before the game starts, in the style of gasm's icon: a few frames of Pong (the two paddles, the dashed net, the square ball on the dark tile), then the paddles move and turn into the gasm logo, with the name "gasm" under it. The same on every runner (native window, browser player); skippable with a key or click, off for headless runs and with a flag. | idea |

## Netplay

| Item | What it gives | Status |
|---|---|---|
| Rollback netplay | Run by the runner: snapshot and restore guest memory between frames (the runner already drives frames and owns the guest's state), instead of lockstep only. | not started |

## Games and ports

### SDL 3

What SDL 3 for gasm doesn't do yet, and what would bring it:

| Missing | What it takes |
|---|---|
| OpenGL, Vulkan, `SDL_GPU` | `gasm:gl` (above) for OpenGL ES |
| Audio recording, camera, haptics and rumble, sensors, dialogs, tray, processes, shared objects | not planned; each fails the way SDL fails on a platform without it |

### ScummVM

What works is in the [ScummVM README](https://github.com/emdzej/gasm/blob/main/guests/scummvm/README.md#porting-status). Not done yet:

| Missing | Effect | What it takes |
|---|---|---|
| Other engines | only the five built in: `scumm`, `scumm_7_8`, `he`, `sky`, `drascula` | add them to `SCUMMVM_ENGINES` (+ engine data); the freeware Flight of the Amazon Queen (`queen`) and Lure of the Temptress (`lure`) are next, then Sierra (`agi`, `sci`), `kyra`, `gob`, `saga`, `sword1`/`sword2`, `tinsel` |
| MT-32 emulation | Roland MT-32 music (many Sierra and LucasArts games sound best with it) | `--enable-mt32emu` (built into ScummVM) and the user's MT-32 ROMs as assets |
| General MIDI synth (FluidSynth/fluidlite) | GM music; AdLib is used instead | build fluidlite, ship or load a SoundFont |
| GUI themes | the built-in classic look ("scummremastered" and `gui-icons.dat` aren't embedded) | embed the theme zip and icons (zlib is in now) |
| Translations, TTS, FreeType, fribidi | English GUI, no speech synthesis, no TrueType fonts (some engines need them) | build the libraries, embed `translations.dat` |
| Video codecs (MPEG-2, Theora, VPX, AAC, JPEG, PNG) | cutscenes and images in engines that use them | build the libraries |
| 3D engines (Grim Fandango, Myst III, The Longest Journey) | not built | TinyGL (software 3D) now, or `gasm:gl` later |
| Cloud, LAN, networking | off | `gasm:net` could carry some of it |
| Gamepads 2–4, touch controls, virtual keyboard | one gamepad; mouse and keyboard otherwise | backend work |
| Size in browsers without JSPI | they need the 16.1 MB Asyncify build (the run build is 10.6 MB) | JSPI in more browsers |

### Godot

Godot 4.7 runs (Compatibility renderer, GDScript, 2D and 3D physics, audio,
input, saves: [guests/godot](https://github.com/emdzej/gasm/blob/main/guests/godot/README.md)).
Left over:

| Item | What it gives | Status |
|---|---|---|
| Threads | Godot is built with `threads=no`: its worker pool, threaded loading and the audio thread run on the main thread. Godot's threads on gasm's cooperative scheduler (they would need Asyncify, which Godot's size makes slow), or real wasm threads (part B of design/threads.md). | not started |
| Jolt physics | Jolt doesn't recognize WASI targets (its platform and SIMD detection); a patch like its Emscripten support. Godot Physics 3D is used meanwhile. | not started |
| TLS, `Crypto` | mbedtls needs a time source (`mbedtls_ms_time`, `timing.c`) for WASI. | not started |
| Multiplayer | `HTTPRequest` works (gasm:fetch); WebSockets and ENet need sockets: a `gasm:net` backend for Godot's `WebSocketPeer` would cover multiplayer. | idea |
| Complex text | The advanced text server (ICU, HarfBuzz: right-to-left, ligatures) instead of the fallback one; larger. | not started |
| A smaller engine | 32 MB (8 MB gzipped): a build profile without unused modules (e.g. 3D for 2D games) per game. | idea |
| Touch | Godot's touch events from gasm's pointer on touch screens. | not started |

## Testing and tooling

| Item | What it gives | Status |
|---|---|---|
| Filter parity in CI | `scripts/present-test.mjs` (native vs WebGL 2 filters, golden images) skips without a GPU; CI's Linux runner would need a software Vulkan driver (lavapipe). | not started |
