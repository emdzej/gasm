---
name: porting-directx7
description: Port a Windows game from the DirectX 7 era (DirectDraw, Direct3D 7, DirectSound, DirectInput, Win32) to gasm on gasm:gl, whether as a rewrite of the executable or from source. Use when starting such a port, replacing a DirectX or Win32 subsystem, turning blocking loops into gasm frames, reading the original's data, or chasing a determinism or rendering bug in one.
---

# Porting a DirectX 7 game to gasm

The full guide, with the reasons, is [porting-directx7.md](../../porting-directx7.md).
The ABI is `spec/ABI.md` / `spec/gasm.h` in the gasm repo. Check names there,
not from memory.

## Workflow

1. **Map the original.** List what it imports (DirectDraw, Direct3D, DSOUND,
   DINPUT, DPLAY, WINMM, registry, threads) and look for the game's own seams:
   a render driver DLL (EA's THRASH), a sound library, a file layer. Port at
   the highest seam the game has, not at the DirectX API.
2. **Platform contract.** Write `platform.h`, with `platform_gasm.c` as the only
   file including `gasm.h` (it exports `gasm_abi_version`, `gasm_init`,
   `gasm_frame`, `gasm_exit`) and a native `platform_host.c` for tools, ctest
   and an ASan build.
3. **Main loop.** Turn every blocking loop (`PeekMessage`/render/`Flip`,
   loading screens, "press a key", movies) into a step function called from
   `gasm_frame`, or keep the loops with `gasm_main` + `gasm_wait_frame`
   (`sdk/c/include/gasm_loop.h`). Call `gasm_set_frame_rate(60)` and keep the
   simulation tick separate from the frame.
4. **VFS before anything reads files.** Use a folder (`--asset-dir`) or an ISO
   (`--asset cd=x.iso`, Joliet names, `gasm_asset_read_at64`), with
   case-insensitive names and `\` or `/`. Read tables and constants from the
   player's `.exe` after a size + CRC-32 check. Never commit game data.
5. **Renderer on gasm:gl** (GLES 3.0 with WebGL 2 rules). Use an FBO at the
   game's resolution in D3D row order, flipped and integer-scaled at present.
   One uber-shader covers the stage combiner, alpha test and fog. For XYZRHW,
   use `w = 1/rhw` and a +0.5 pixel centre. Stream vertices into VBOs, turn
   quads into triangles, convert 16-bit and palettised textures to RGBA8, and
   use explicit precision.
6. **Input.** Use `GASM_INPUT_KEYS_RAW`, then `gasm_key_state` to fill a
   DIK-indexed table and `gasm_key_events` for text. Fill `DIJOYSTATE` from
   `gasm_gamepad`. Re-enumerate pads at the controller screen or race start.
7. **Audio.** Use a software mixer, `gasm_audio_config(44100, 2)`, then
   `gasm_audio_push` of rate/fps frames every frame. DirectSound volume is in
   1/100 dB.
8. **Win32 leftovers.** Time comes from ticks (no wall clock). MSVC `rand()`
   is exact (214013/2531011, 15 bits). The registry and saves go to
   `gasm_storage_*`. Threads become synchronous reads. The title comes from
   `GASM_TITLE`.
9. **Test headless** after every change (below), and look at a screenshot
   whenever a hash moves.

## Build checklist

- Use the C SDK toolchain: `-DCMAKE_TOOLCHAIN_FILE=…/gasm-toolchain.cmake
  -DWASI_SDK_PREFIX=…`, then `gasm_add_game`.
- Add `-Wl,-z,stack-size=1048576`. The 64 KiB default overflows silently.
- Use `memcpy` for file structs, with explicit packing. Pointers that the
  original kept inside 32-bit records get their own fields.
- Keep `float` and `double` exactly as the original's x87 code stored them.
  Use an x87 rounding helper where physics or timers must be bit-exact.
- C++ classes become structs with explicit vtables. Pre-`WinMain` static
  initializers become one init function, called in table order.

## Test commands

```sh
gasm-run game.wasm --asset cd=game.iso --headless 600
gasm-run game.wasm --asset cd=game.iso --headless 600 --screenshot out.png
gasm-run game.wasm --asset-dir game --headless 900 --storage-dir /tmp/s \
  --input '100-104:KEY(Enter),200:PTR(320,240,L),300-600:GP0(B0+A1=1),300:GP1(B0)'
```

- The hashes must not change for the same input, and the folder and the ISO
  must give the same hashes. Explain every hash change in the commit.
- The video hash includes uploads: compare screenshots before blaming logic.
- `glReadPixels` returns zeros headless unless `--screenshot` is given.
- Without `--storage-dir`, saves stay in memory.
- Use `--headless` only. Never open a window in unattended runs.

## Traps

- Look up every caps bit in `d3dtypes.h`. `dwRasterCaps & 0x8000` is
  `ZBUFFERLESSHSR`, and setting it skips the game's depth clears.
- A `D3DRECT` is `x1,y1,x2,y2`.
- `WM_KEYDOWN` scan-code tables (`& 0x7f`) give the arrows the keypad's codes
  (up = `0x48`).
- Browsers expose a pad only after its first button press.
- Use a native ASan build for memory bugs: wasm won't trap on them.
- In Ghidra, a driver DLL may show 0 functions until auto-analysis runs.
- In a macOS `.app`, ANGLE goes in `Resources/angle/` and the launcher passes
  `--gl-lib`.

## Not available on gasm (yet)

Force feedback has no API. There is no DirectPlay: use `gasm:net` through a
`gasm-relay` room. Bink and Smacker need a decoder of their own. Check
`site/docs/roadmap.md` before building around a gap, and add new gaps there.

## Keep this current

When a port finds a new trap, fix or gasm gap, add it to the guide and to this
file in the same change.
