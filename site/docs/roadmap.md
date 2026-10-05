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
cooperative threads in C, POSIX, SDL 3 and Rust
([design/threads.md](https://github.com/emdzej/gasm/blob/main/design/threads.md)),
`gasm:gl` on every runner
([design/gasm-gl.md](https://github.com/emdzej/gasm/blob/main/design/gasm-gl.md)),
Godot 4.7 ([guests/godot](https://github.com/emdzej/gasm/blob/main/guests/godot/README.md)),
HTTP requests (`gasm:fetch`,
[design/fetch.md](https://github.com/emdzej/gasm/blob/main/design/fetch.md)),
the splash screen, OpenGL ES through SDL 3, a guest memory limit, and since
0.10.0: the clipboard (`gasm:clipboard`, F2 copies the frame), saving files for
the player (`gasm:files`), player consent for hosts and saves, Godot multiplayer
(`WebSocketPeer` on `gasm:net`) and a smaller engine for 2D games.

1. **Capabilities manifest** ([Runtime and ABI](#runtime-and-abi)): a game
   declares the hosts and modules it needs, so runners can ask once, up front,
   and check a game before running it; the base for `.gasm` packages.
2. **Opening files** ([Runtime and ABI](#runtime-and-abi)): the other half of
   `gasm:files`, a picker run by the runner.
3. **Godot, the rest** ([Godot](#godot)): Jolt, TLS and `Crypto`, threads when a
   game needs them ([design/threads.md](https://github.com/emdzej/gasm/blob/main/design/threads.md)).

## Runtime and ABI

| Item | What it gives | Status |
|---|---|---|
| Stack switching beyond JSPI | Browsers without JSPI (and Node 22) still need the Asyncify builds; wasm's stack-switching proposal would cover them too. | waiting on engines |
| Real wasm threads | Shared memory and atomics, opt-in, for guests that need parallel CPU (physics, job systems, Godot's worker pool). Not deterministic. | proposal; for Godot, a design pass found the gain small (rendering stays on the main thread; physics and loading would gain), so no prototype yet: [design/threads.md](https://github.com/emdzej/gasm/blob/main/design/threads.md) (part B, "Godot: design pass") |
| Capabilities manifest | Custom section `gasm.manifest` declaring required and optional imports, network hosts and platform extensions (`gasm:ext/*`), so runners can check a game before running it. | not started |
| `.gasm` packages | One file bundling `game.wasm`, its assets and a manifest. | not started |
| Opening files (`gasm:files`) | A file picker run by the runner (importing a map, a replay): the game only sees what the player picks. Saving for the player already works (`gasm:files.save`). | not started |
| Asking before a save | `gasm-run` writes a game's saves to a default folder (or `--save-dir`, or refuses them with `--no-save`); a native save dialog for each would let the player pick the place. | idea |
| WIT / Component Model | Move the ABI to WIT once browsers don't need transpiling for components. | waiting on browsers |

## Graphics

| Item | What it gives | Status |
|---|---|---|
| `gasm:gl` on Wayland | ANGLE draws into X11 windows, so `gasm:gl` games use XWayland on Wayland desktops. | not started |
| `gasm:gl` on SwiftShader on macOS | `--gl-software` needs a Vulkan loader there (Electron doesn't ship one); Metal is always available, so it only matters for tests. | not planned |
| ANGLE built from source | The libraries come from Electron 43, the last release that ships them as separate files; a newer ANGLE means building it (depot_tools) or another distribution. | when needed |
| Copying `gasm:gfx` frames natively | `gasm-run`'s copy key (F2) copies 2D and `gasm:gl` frames; WebGPU games draw straight to the window's surface, which can't be read back yet (the web player copies them). | not started |
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

## Netplay

| Item | What it gives | Status |
|---|---|---|
| Rollback netplay | Run by the runner: snapshot and restore guest memory between frames (the runner already drives frames and owns the guest's state), instead of lockstep only. | not started |

## Games and ports

### SDL 3

What SDL 3 for gasm doesn't do yet, and what would bring it:

| Missing | What it takes |
|---|---|
| Desktop OpenGL | not planned: OpenGL ES on `gasm:gl` works (SDL3::GL) |
| `SDL_GPU`, Vulkan | `SDL_GPU` on `gasm:gfx` (WebGPU) would be a new GPU driver for SDL; not started |
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
| Godot as a WebSocket server | `WebSocketPeer` and `WebSocketMultiplayerPeer` work as clients on `gasm:net` (a relay room, or a Godot server outside gasm); a game can't host (`create_server`), and ENet and UDP need sockets browsers don't have. | not planned |
| Complex text | The advanced text server (ICU, HarfBuzz: right-to-left, ligatures) instead of the fallback one; larger. | not started |
| Touch | Godot's touch events from gasm's pointer on touch screens. | not started |

## Testing and tooling

| Item | What it gives | Status |
|---|---|---|
| Filter parity in CI | `scripts/present-test.mjs` (native vs WebGL 2 filters, golden images) skips without a GPU; CI's Linux runner would need a software Vulkan driver (lavapipe). | not started |
