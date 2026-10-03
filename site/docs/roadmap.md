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

1. **Runner-side stack switching (`gasm_run`).** Contained work, and every game
   with its own main loop gets smaller and faster without changing: ScummVM is
   16.1 MB with Asyncify, 10.6 MB without.
2. **Cooperative threads.** Removes the most common porting blocker
   (`SDL_CreateThread`, `SDL_AddTimer`, pthreads) and keeps runs deterministic.
3. **`gasm:gl`.** The largest payoff (GLES/WebGL engines, Godot, ScummVM's 3D
   engines) and the most work, mostly building and shipping ANGLE natively.

## Runtime and ABI

| Item | What it gives | Status |
|---|---|---|
| `gasm_run` export with a blocking `wait_frame` import | Games with their own loop without Asyncify (code size, speed): runners suspend the guest's stack (wasmtime async, JSPI in browsers, later wasm stack switching). `gasm_frame` stays the default; the SDK loop helpers (`gasm_loop.h`, `gasm::main_loop!`) switch over without changing games. Needs an async path in the runners' frame loops (`GasmHost.frame()` and Worker batches are synchronous today). | not started; notes in [threads, "Later: cheaper switching"](https://github.com/emdzej/gasm/blob/main/design/threads.md#later-cheaper-switching) |
| Cooperative threads | Threads inside the guest (no ABI change, deterministic): a scheduler and C API, an SDL thread backend (`SDL_CreateThread`, `SDL_AddTimer`), a pthreads shim, later a Rust API. | proposal: [design/threads.md](https://github.com/emdzej/gasm/blob/main/design/threads.md) (part A) |
| Real wasm threads | Shared memory and atomics, opt-in, for guests that need parallel CPU (physics, job systems, Godot's worker pool). Not deterministic. | proposal, after cooperative threads: [design/threads.md](https://github.com/emdzej/gasm/blob/main/design/threads.md) (part B) |
| Guest memory limit | A cap on linear memory growth. Today a guest can grow to the engine maximum (4 GiB for wasm32); needed before running untrusted content. | not started |
| Capabilities manifest | Custom section `gasm.manifest` declaring required and optional imports, network hosts and platform extensions (`gasm:ext/*`), so runners can check a game before running it. | not started |
| `.gasm` packages | One file bundling `game.wasm`, its assets and a manifest. | not started |
| `gasm:files` | A file picker run by the runner: the game only sees what the player picks. | not started |
| WIT / Component Model | Move the ABI to WIT once browsers don't need transpiling for components. | waiting on browsers |

## Graphics

| Item | What it gives | Status |
|---|---|---|
| `gasm:gl` | OpenGL ES 3.0 with WebGL 2's rules: WebGL 2 in browsers, ANGLE natively, a null GL headless. Phases: ABI, browser and headless, ANGLE, SDKs and docs, then a Godot port. | proposal: [design/gasm-gl.md](https://github.com/emdzej/gasm/blob/main/design/gasm-gl.md) |
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
| Threads (`SDL_CreateThread`) and timers (`SDL_AddTimer`) | cooperative threads (above) |
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
| Size and speed | 16.1 MB module (10.6 MB without Asyncify) | runner-side stack switching (`gasm_run`, above) |

### Godot

A `platform/gasm` port on `gasm:gl`: headless first, then the Compatibility
renderer. Phase 5 of the
[`gasm:gl` plan](https://github.com/emdzej/gasm/blob/main/design/gasm-gl.md#plan),
its own project.

## Testing and tooling

| Item | What it gives | Status |
|---|---|---|
| Filter parity in CI | `scripts/present-test.mjs` (native vs WebGL 2 filters, golden images) skips without a GPU; CI's Linux runner would need a software Vulkan driver (lavapipe). | not started |
