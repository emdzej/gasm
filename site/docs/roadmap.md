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
and `gasm:gl` in the browser and headless
([design/gasm-gl.md](https://github.com/emdzej/gasm/blob/main/design/gasm-gl.md)).

1. **`gasm:gl` natively.** ANGLE in `gasm-run`'s window (phase 3 of the plan):
   mostly building and shipping ANGLE for macOS, Windows and Linux. Then the
   Rust `glow` backend, then a Godot port.

## Runtime and ABI

| Item | What it gives | Status |
|---|---|---|
| Cooperative threads in Rust | `gasm::thread::spawn` and locks on the same scheduler (the C API, POSIX threads and SDL threads are done: [design/threads.md](https://github.com/emdzej/gasm/blob/main/design/threads.md), part A). `std::thread` on `wasm32-unknown-unknown` can't be redirected. | not started |
| Stack switching beyond JSPI | Browsers without JSPI (and Node 22) still need the Asyncify builds; wasm's stack-switching proposal would cover them too. | waiting on engines |
| Real wasm threads | Shared memory and atomics, opt-in, for guests that need parallel CPU (physics, job systems, Godot's worker pool). Not deterministic. | proposal, after cooperative threads: [design/threads.md](https://github.com/emdzej/gasm/blob/main/design/threads.md) (part B) |
| Guest memory limit | A cap on linear memory growth. Today a guest can grow to the engine maximum (4 GiB for wasm32); needed before running untrusted content. | not started |
| Capabilities manifest | Custom section `gasm.manifest` declaring required and optional imports, network hosts and platform extensions (`gasm:ext/*`), so runners can check a game before running it. | not started |
| `.gasm` packages | One file bundling `game.wasm`, its assets and a manifest. | not started |
| `gasm:files` | A file picker run by the runner: the game only sees what the player picks. | not started |
| WIT / Component Model | Move the ABI to WIT once browsers don't need transpiling for components. | waiting on browsers |

## Graphics

| Item | What it gives | Status |
|---|---|---|
| `gasm:gl` natively | OpenGL ES 3.0 in `gasm-run`'s window through ANGLE (Metal, D3D11, Vulkan). The ABI, the browser runner (WebGL 2), the null GL of headless runs and the C headers are done; the native window refuses `gasm:gl` games until then. | next: [design/gasm-gl.md](https://github.com/emdzej/gasm/blob/main/design/gasm-gl.md), phase 3 |
| `gasm:gl` in Worker mode | WebGL 2 on a transferred `OffscreenCanvas`; `gasm:gl` games run on the main thread for now. | not started |
| `gasm:gl` for Rust | A `glow::HasContext` implementation over `gasm::sys::gl_*` (egui_glow and other glow code). | not started |
| Godot | A `platform/gasm` port on `gasm:gl`. | after native `gasm:gl` |
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

A `platform/gasm` port on `gasm:gl`: headless first, then the Compatibility
renderer. Phase 5 of the
[`gasm:gl` plan](https://github.com/emdzej/gasm/blob/main/design/gasm-gl.md#plan),
its own project.

## Testing and tooling

| Item | What it gives | Status |
|---|---|---|
| Filter parity in CI | `scripts/present-test.mjs` (native vs WebGL 2 filters, golden images) skips without a GPU; CI's Linux runner would need a software Vulkan driver (lavapipe). | not started |
