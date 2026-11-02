# Porting DirectX 7 games

How to bring a Windows game from the DirectX 7 era (DirectDraw, Direct3D 7,
DirectSound, DirectInput, Win32) to gasm. It is written from two ports in
progress: OpenCMR2 (Colin McRae Rally 2.0) and OpenNFSPU (Need for Speed:
Porsche Unleashed). Both are clean C11 rewrites of the original executable,
function by function, on `gasm:gl`. The same structure works when you have the
original source instead: you keep its code and replace the same layers.

An agent-oriented checklist of this page is the
[porting-directx7 skill](./skills/porting-directx7/SKILL.md).

## The shape of a port

Neither port emulates COM or wraps Win32. The game's code is ported as it is,
and everything it asked of Windows and DirectX goes through one narrow
platform layer:

- `src/platform.h`: the contract the game uses (frame, input, audio, files,
  storage, time, logging).
- `src/platform_gasm.c`: the only file that includes `gasm.h`. It exports
  `gasm_abi_version`, `gasm_init`, `gasm_frame` and `gasm_exit` and turns the
  contract into gasm calls. About 400 lines.
- `src/platform_host.c`: a native implementation of the same contract, so the
  game builds as a normal executable for tools, `ctest` and an
  AddressSanitizer build (wasm doesn't trap on a use-after-free; ASan does).
- Renderer back end on GLES 3.0 (below), the only code that calls GL.
- A virtual file system (`src/vfs.c`) over gasm assets.

Keep everything else free of gasm: the ported code then reads like the
original, and the native build tests it.

## The main loop

A DirectX 7 game owns its loop:

```c
while (!done) {
    if (PeekMessage(&msg, ...)) { TranslateMessage(&msg); DispatchMessage(&msg); }
    update(); render(); surface->Flip(...);
}
```

and has more of them: loading screens, "press a key", movie players, the
front end, the race. gasm calls the game once per frame (`gasm_frame`), so
there are two ways to go:

- **State machines** (both ports). Each blocking loop becomes a step function
  that does one iteration and returns, and the top level (`src/app.c`) calls
  the current step from `gasm_frame`. No Asyncify, no stack switching, the
  smallest and fastest module. It is the biggest structural change of the
  port: every place that waits (for a key, a load, a fade, a movie frame) must
  be re-entrant, with its locals moved into a state struct.
- **Keep the loops** with the C SDK's loop helper: write `gasm_main()` and
  call `gasm_wait_frame()` where the original called `Flip`. The runner
  switches stacks (`gasm_run`) or Asyncify unwinds the stack (`gasm_frame`);
  see [own main loops](./games.md). Less rewriting when you have the source
  and many nested loops (ScummVM and SDL 3's `main()` apps work this way), at
  the cost of a larger module and Asyncify's rules.

Call `gasm_set_frame_rate(60)` in `gasm_init`, and keep the simulation clock
separate from the frame clock. OpenNFSPU's simulation runs at 64 Hz: each 60 Hz
frame accumulates 64/60 ticks and runs the whole ones. Quitting is
`__wasi_proc_exit(0)` (or `exit(0)`).

## Graphics: Direct3D 7 on `gasm:gl`

`gasm:gl` is OpenGL ES 3.0 with WebGL 2's rules (see
[choosing a graphics API](./graphics.md)). It runs on ANGLE natively and WebGL 2
in browsers, and its null GL makes headless runs reproducible.

**Find the game's own render layer first.** Many games of the era didn't call
Direct3D directly. Porsche Unleashed draws through EA's driver interface,
THRASH: 42 `_THRASH_*` entry points loaded from `Drivers/dx7z.dll`. OpenNFSPU
ports that one layer to GLES (`src/render/thrash.c`, about 1,500 lines, laid
out like the DLL's functions), and the engine, renderer and HUD above it run
unchanged. If the game has such a seam, port that: it is smaller than
Direct3D and already says what the game needs.

If the game calls the device itself (CMR2 does), write a Direct3D 7 *device*
as plain C functions that take D3D7's own enums and FVF layouts:
`D3d_SetRenderState`, `D3d_SetTexture`, `D3d_DrawPrimitive`, `D3d_Clear`.
The ported code then reads like the original's vtable calls. Reduce
DirectDraw to the surfaces the game really uses: the back buffer, textures,
a few 2D blits.

| Direct3D 7 | On GLES 3.0 / WebGL 2 |
|---|---|
| Back buffer, `Flip` | Render into a framebuffer object and present it once per frame. Drawing in D3D's row order (y down) keeps viewports, scissors and clear rectangles in D3D coordinates; flip once when copying to the default framebuffer. |
| Display mode (640×480 …) | Render at the game's resolution and scale it to `gasm_gl_width()`×`gasm_gl_height()`: an integer multiple, letterboxed. |
| Fixed-function pipeline | One "uber" shader driven by uniforms: the texture stage combiner (`COLOROP`/`ALPHAOP`: `MODULATE`, `SELECTARG1/2`, `ADD`, …, two stages is usually enough), alpha test (`discard`), vertex fog, specular. |
| Pre-transformed vertices (`D3DFVF_XYZRHW`) | Map screen x/y linearly to clip space and use `w = 1/rhw`, so perspective-correct texturing and fog still work. |
| Pixel centres | D3D7 samples pixel centres at integer coordinates: add 0.5 when mapping screen-space vertices. |
| FVF vertex formats | One vertex array layout per FVF; stream the game's vertices into a buffer (`glBufferData`/`glBufferSubData`). WebGL 2 has no client-side arrays. |
| `D3DPT_*` primitives | Triangle lists, strips and fans map directly. There are no quads: expand them to triangles. |
| `D3DRENDERSTATE_ZBIAS` | `glPolygonOffset`. |
| `D3DTADDRESS_BORDER` | No clamp-to-border in WebGL 2: clamp to edge, or do it in the shader. |
| Textures (16-bit 565/1555/4444, palettised) | Convert to RGBA8 at upload. Mind WebGL 2's rules for non-power-of-two textures. |
| `Clear` with `D3DRECT`s | Scissored clears. A `D3DRECT` is `x1,y1,x2,y2`, not `x,y,w,h`. |
| Device caps (`D3DDEVICEDESC7`) | Report a plain hardware card. Check every bit the game tests against `d3dtypes.h`, not by guessing from the name (below). |

GLSL ES 3.00 needs explicit `precision` qualifiers. Front-end menus and HUDs
are textured quads on the same path, not `video_present`: a module that
imports `gasm:gl` draws everything with GL.

## Input: DirectInput

- **Keyboard.** `gasm_input_mode(GASM_INPUT_KEYS_RAW)` (add
  `GASM_INPUT_POINTER_HIDDEN` if the game draws its own cursor), then
  `gasm_key_state` for the state array the game polls and `gasm_key_events`
  for text entry. Map gasm's key codes (W3C `KeyboardEvent.code`) to `DIK_*`
  scan codes once, in a table; extended keys get `| 0x80`.
- **Check which key table the game reads.** A game whose key handler reads
  `WM_KEYDOWN`'s `lParam` scan code `& 0x7f` (Porsche Unleashed does) sees the
  arrow keys as the keypad's codes: up is `0x48`, not `DIK_UP` (`0xC8`).
- **Joysticks.** `gasm_gamepad(slot, …)` gives the raw device (buttons and
  axes, `GASM_GAMEPAD_BYTES`). Fill a `DIJOYSTATE`-shaped record from it: axes
  0–3 to `lX`, `lY`, `lZ`, `lRz` scaled to 0..0xFFFF, the triggers (buttons 6
  and 7) to `lRx`/`lRy`, the d-pad to POV 0 (hundredths of a degree, -1 when
  centred), buttons as `0x80`. `gasm_input_pad(player)` gives the mapped
  virtual pad instead, which also follows the keyboard.
- **Re-enumerate pads.** A browser shows a gamepad only after its first button
  press, so look for devices when the game shows its controller list or
  starts a race, not once at start-up as DirectInput did.
- Force feedback has no gasm API.

## Audio: DirectSound

Mix in software and push one frame's samples each frame:
`gasm_audio_config(44100, 2)` once, then `gasm_audio_push` with
`rate / frame_rate` frames per frame.

- If the game has its own sound library (EA's SND in Porsche Unleashed), port
  its mixer and have it write floats.
- Otherwise emulate the secondary buffers the game uses (OpenCMR2's
  `src/audio/dsound.c`): play, loop, stop, the position queries the game
  polls, frequency, pan, and volume in hundredths of a decibel
  (`gain = 10^(volume / 2000)`). DirectSound3D needs more work, but games of
  the era often did their own 3D attenuation.

## Win32 and the rest

| Original | On gasm |
|---|---|
| `GetTickCount`, `timeGetTime`, `QueryPerformanceCounter` | Time derived from frames or simulation ticks. In OpenNFSPU, the sound library's 100 Hz tick × 10. `gasm_time_ms` is virtual in headless runs, but frame-derived time is the safer contract. |
| `rand()` | MSVC's generator reimplemented exactly (`x = x * 214013 + 2531011`, return `(x >> 16) & 0x7FFF`) when AI, replays or tracks depend on it. Port the game's own generators exactly too. |
| Registry, INI files, saves | `gasm_storage_get/set/delete/count/key`. Map path separators in keys (OpenCMR2: `\` to `_`). |
| `CreateFile`/`ReadFile`, CRT `fopen` | The game's VFS over assets (below). |
| Worker threads (streaming, loading) | Synchronous reads inside the frame. Real threads exist (wasi-threads) but headless runs allow none, so a threaded game needs a main-thread fallback anyway. |
| Window title | `GASM_TITLE("…")` or `gasm_set_title`. |
| `MessageBox`, message pump | Gone. Log with `gasm_log`. |
| DirectPlay | Not ported by either game yet. `gasm:net` carries WebSocket-style messages to a relay room (`gasm-relay`), so a LAN game would talk through that. |
| Bink / Smacker movies | Skipped so far: a decoder is a port of its own. |
| C++ classes, `__thiscall`, `__stdcall` | Structs and plain functions; virtual calls through an explicit vtable struct. Calling conventions disappear. |
| Static initializers that run before `WinMain` | Find the CRT's initializer table (in Porsche.exe at `0x5cb004…`): the functions in it have no direct callers. Port them into one explicit init function, called in table order. |

## Files, assets and licensing

Neither port ships a byte of the original game: no data, no code, no tables.
The player supplies their copy.

- **Read the original's data at run time.** Tables the game needs from the
  executable are read from the player's own `.exe` (`src/exe.c`) after
  checking its size and CRC-32, so the source and the `.wasm` contain nothing
  copied and stay redistributable. The same goes for float constants: read
  them, don't retype them.
- **One VFS for every file the game opens** (`src/vfs.c`): a folder
  (`--asset-dir`) or a CD image (`--asset cd=game.iso`), with ASCII
  case-insensitive names and `\` or `/` as separators.
  - ISO 9660 primary trees have 8.3 names only: read the Joliet tree for long
    names.
  - Read the image lazily with `gasm_asset_size64` and `gasm_asset_read_at64`;
    list files with `gasm_asset_count`/`gasm_asset_name`.
  - When the executable exists only inside an InstallShield cabinet on the
    CD (CMR2), layer a cabinet reader over the ISO (`src/vfs_cab.c`): the port
    then needs nothing but the disc.
- **CD checks** fall away: the image is present, there is no drive letter.
- **Browser.** A page imports the player's installed folder or image into
  OPFS and checks it (the executable's size, the expected folders). Never host
  game data.
- **Binary structs.** wasm32 is ILP32 like Win32, which helps. Still state
  packing (`#pragma pack` or explicit offsets) wherever the original reads raw
  bytes, read with `memcpy` (unaligned data), never overlay host structs on
  file data, and give pointers that the original stored inside 32-bit records
  fields of their own. Check `char` signedness and `long` sizes where the
  original relied on them.

## Build

The C SDK's CMake toolchain wraps wasi-sdk:

```sh
cmake -B build-wasm \
  -DCMAKE_TOOLCHAIN_FILE=.deps/gasm-c-sdk/cmake/gasm-toolchain.cmake \
  -DWASI_SDK_PREFIX=.deps/wasi-sdk
cmake --build build-wasm
```

with `gasm_add_game(mygame src/…)` from `Gasm.cmake` (a fetch script that
downloads a pinned SDK and wasi-sdk keeps the port's builds reproducible).

- **Raise the stack**: `-Wl,-z,stack-size=1048576`. wasm-ld's default is
  64 KiB. Games of the era keep large arrays on the stack, and an overflow
  silently corrupts the data below it, with no trap.
- **x87 precision.** The original computed in 80-bit registers and rounded
  when storing to memory. Keep `float` and `double` exactly where the
  original stored them. Where results must be bit-exact (physics integration,
  timers), reproduce the x87 step with a helper (OpenCMR2's
  `X87_MulRound`); plain C changes trajectories. Even so, exact parity with
  the original's replays drifts after a few seconds in Porsche Unleashed.
- Plain C11 with no MSVC extensions is easiest. Inline assembly has to be
  rewritten in C.

## Determinism and tests

gasm's headless runs are reproducible, so they are the test suite:

```sh
gasm-run game.wasm --asset cd=game.iso --headless 600            # hashes
gasm-run game.wasm --asset cd=game.iso --headless 600 --screenshot out.png
gasm-run game.wasm --asset-dir game --headless 900 --storage-dir /tmp/saves \
  --input '100-104:KEY(Enter),200:PTR(320,240,L),300-600:GP0(B0+A1=1)'
```

- `video_fnv32` and `audio_fnv32` must stay the same for the same input: a
  folder and the same files in an ISO give identical hashes, and a commit
  that changes a hash says why.
- The video hash covers GL uploads (buffers and textures), not only pixels:
  uploading one more texture changes it while the frames look identical.
  Compare `--screenshot`s before suspecting logic.
- In headless runs the null GL returns zeros from `glReadPixels` unless
  `--screenshot` is given.
- `--input` scripts keys (`KEY(…)`), the pointer (`PTR(x,y,L)`), the virtual
  pad and raw gamepads in slots 0–3 (`GP0(…)`…`GP3(…)`, so two-player setups
  can be scripted). Game-side test switches (`gasm_param`: a start state,
  autodrive, scripted front-end clicks) make long runs short.
- Headless storage is in memory: pass `--storage-dir` to test saves.
- What breaks determinism: wall clocks, `rand()` call order, x87 precision,
  uninitialised memory. OpenCMR2 keeps the game's globals at the original's
  addresses in one zeroed block, which rules out the last kind.
- Never open a window in unattended runs: `--headless` only.

## Traps

- **Caps bits.** CMR2 tests `dwRasterCaps & 0x8000`, which is
  `D3DPRASTERCAPS_ZBUFFERLESSHSR`. Reporting it made the game skip its depth
  clears and cut geometry with a stale depth buffer. Look each bit up in
  `d3dtypes.h`.
- **`D3DRECT`** is `x1,y1,x2,y2`.
- **Arrow keys** share the keypad's scan codes in `WM_KEYDOWN` tables.
- **Pads appear late** in browsers.
- **Default stack** is 64 KiB, and overflowing it doesn't trap.
- **Upload hashes** move without visible changes.
- **Ghidra and DLLs**: an imported driver DLL showed 0 functions until
  auto-analysis was run explicitly.
- **Static initializers** run before `WinMain` with no callers in sight.
- **Shipping natively**: `gasm:gl` games need ANGLE (and SwiftShader for
  machines without a GPU) next to `gasm-run`. In a macOS `.app`, put ANGLE in
  `Contents/Resources/angle/` and pass `--gl-lib`: `codesign --strict` allows
  code only in `Frameworks/`.

What gasm still lacks for these games (force feedback, for one) is on the
[roadmap](../docs/roadmap.md).
