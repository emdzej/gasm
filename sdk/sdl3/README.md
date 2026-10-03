# SDL 3 for gasm

[SDL 3](https://libsdl.org) built for gasm: SDL programs compile to a gasm guest
with their source unchanged. Both kinds of SDL program work:

- **Main callbacks** (`SDL_MAIN_USE_CALLBACKS`): `SDL_AppInit` runs at start,
  then every gasm frame delivers that frame's events to `SDL_AppEvent` and calls
  `SDL_AppIterate` once. Nothing else is needed.
- **A classic `main()` with its own loop**: `main` runs on the C SDK's loop helper
  (`gasm_loop`: Binaryen's Asyncify, or the runner's stack switching for the
  `-run.wasm` build) and is suspended where a frame ends:
  `SDL_RenderPresent` / `SDL_UpdateWindowSurface`, or an `SDL_Delay` that crosses
  a frame boundary (programs that pace themselves with `SDL_Delay` keep their
  rate). Needs one `wasm-opt` step after linking.

It's SDL's own code (SDL 3.4.16, zlib license) with gasm as an SDL "private
platform": gasm's drivers live here, and SDL is not patched. `make sdl3` fetches
SDL into `tools/SDL3-src` and builds `build/sdl3/` (`lib/libSDL3.a`, `include/SDL3/`,
`lib/cmake/SDL3/`). Releases ship the same as `gasm-sdl3-<version>.zip`.

## Building a program

With CMake (the gasm C SDK's toolchain file, and this package's `find_package` config):

```cmake
cmake_minimum_required(VERSION 3.20)
project(mygame C)
find_package(SDL3 REQUIRED)
add_executable(mygame main.c)
target_link_libraries(mygame PRIVATE SDL3::SDL3)
gasm_sdl3_app(mygame)          # callbacks; use gasm_sdl3_app(mygame LOOP) for a main() loop
```

```sh
cmake -B build -DCMAKE_TOOLCHAIN_FILE=<gasm-c-sdk>/cmake/gasm-toolchain.cmake \
      -DSDL3_DIR=<gasm-sdl3>/lib/cmake/SDL3 -DCMAKE_BUILD_TYPE=Release
cmake --build build && gasm-run build/mygame.wasm
```

By hand:

```sh
clang --target=wasm32-wasip1 -mexec-model=reactor -O2 -I<gasm-sdl3>/include main.c \
      <gasm-sdl3>/lib/libSDL3.a -lm -o game.wasm
# a classic main() loop also needs:
wasm-opt game.wasm --asyncify --pass-arg=asyncify-removelist@gasm_loop_frame -O2 -o game.wasm
```

`examples/` has one program of each kind. The repository also builds SDL's own
demos unchanged (`build/sdl3-snake.wasm`, `build/sdl3-woodeneye.wasm`); of SDL's 34
examples, all but the camera one run.

Command line: the `args` param (`--param "args=-x 1"`) becomes `argv[1...]`.

## What maps to what

| SDL | gasm |
|---|---|
| Window framebuffer, `SDL_Renderer` (software) | `video_present`, at the window's size; the runner scales it. One window is shown (the first) |
| Keyboard: scancodes, key events, `SDL_GetKeyboardState` | raw keys (`key_events`): physical keys, modifiers are keys |
| Text input (`SDL_StartTextInput`) | `text_input` (Backspace and Enter come as keys) |
| Mouse: motion, buttons, wheel, `SDL_HideCursor`, relative mode | `pointer` in frame pixels; relative mode = `GASM_INPUT_POINTER_LOCKED` |
| Joysticks, `SDL_Gamepad` | the four `gamepad` slots; standard-mapped pads are SDL gamepads (triggers as axes) |
| Audio playback (streams, callbacks) | `audio_push`: float, mono or stereo, one frame of samples per gasm frame |
| `SDL_GetTicks`, performance counter, `SDL_Delay` | virtual time: 1/60 s per frame, so runs are reproducible |
| `SDL_GetCurrentTime`, date/time | virtual too, from 2026-01-01 00:00 UTC; local time is UTC |
| Files: `SDL_IOFromFile`, `SDL_GetBasePath` (`/`), title storage | assets, read-only and streamed (`asset_read_at64`, any size); folders work (`SDL_EnumerateDirectory`, `SDL_GetPathInfo`) |
| `SDL_GetPrefPath` (`/storage/`), user storage | `gasm:storage`: a file is a key, written when closed |
| Async I/O | done at once (gasm guests have one thread); results are ready on the next poll |
| `SDL_ShowMessageBox` | the log; returns the default button |

Not available: threads (`SDL_CreateThread` fails, and with it `SDL_AddTimer`),
OpenGL, Vulkan and `SDL_GPU` (gasm has no GL yet; see `gasm:gl` on the
[roadmap](https://github.com/emdzej/gasm/blob/main/site/docs/roadmap.md#sdl-3)),
audio recording, camera, haptics and rumble, sensors, dialogs, tray, processes,
loading shared objects. Each fails the way SDL fails on a platform without it.

## Notes for ports

- Everything is per frame: input is sampled before the frame, so polling events
  twice in one frame gives nothing new, and a loop that waits for a key without
  presenting or delaying never sees one. Present or `SDL_Delay` in such loops.
- A frame ends at the first present. A program that presents more than once per
  frame (or never, for long stretches) should add `SDL_Delay` pacing.
- Sizes: SDL adds about 0.8 MB to a module (`sdl3-snake.wasm` is 810 KB);
  Asyncify adds a little more for a `main()` loop (`sdl3-classic.wasm`: 1.15 MB;
  the run build without it, `sdl3-classic-run.wasm`: 0.81 MB).

## Files

- `include/SDL_build_config_private.h`: the build configuration (wasi-libc, no threads, gasm drivers)
- `include/SDL_main_private.h`, `SDL_main_impl_private.h`: the entry points (installed next to SDL's headers)
- `src/SDL_gasm.c`: frames, virtual time, main callbacks
- `src/SDL_gasmvideo.c`, `SDL_gasmkeys.h`: video, keyboard, text, pointer
- `src/SDL_gasmaudio.c`, `src/SDL_gasmjoystick.c`, `src/SDL_gasmfs.c`, `src/SDL_gasmasyncio.c`
- `cmake/SDL3Config.cmake`: `find_package(SDL3)` and `gasm_sdl3_app()`

gasm's files here are zlib-licensed like SDL (the examples are MIT). `libSDL3.a`
also contains the C SDK's `gasm_loop.c` and `gasm_vfile.c` (MIT; `fopen` in SDL's
`SDL_iostream.c` goes through `gasm_vfile` to assets and storage).
