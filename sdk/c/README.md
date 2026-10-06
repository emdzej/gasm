# gasm C/C++ SDK

Write games for [gasm](https://gasm.emdzej.pl) in C or C++. The SDK is one
header plus CMake glue:

```
include/gasm.h              the whole ABI (generated from spec/abi.json)
include/gasm_loop.h         optional: games with their own main loop
src/gasm_loop.c             (gasm_main + gasm_wait_frame: Asyncify, or gasm_run for runners that switch stacks)
include/gasm_thread.h       optional: cooperative threads, mutexes, conditions, semaphores, keys
src/gasm_thread.c           (with gasm_loop.c built with -DGASM_LOOP_THREADS)
src/gasm_pthread.c          optional: POSIX threads (pthread_*, sem_*) on the same threads
include/gasm_manifest.h     optional: GASM_MANIFEST({ ... }) embeds the capabilities manifest
include/gasm_vfile.h        optional: stdio FILE* over assets and gasm:storage
src/gasm_vfile.c
include/GLES3/gl3.h         optional: OpenGL ES 3.0 on gasm:gl (also GLES2/gl2.h, gl2ext.h)
src/gasm_gl.c               (generated from the Khronos registry)
src/gasm_gl_proc.c          (generated: gasm_gl_get_proc_address)
cmake/gasm-toolchain.cmake  wasm32 toolchain (wraps wasi-sdk's)
cmake/Gasm.cmake            gasm_add_game(<target> [LOOP [THREADS]] <sources...>)
example/                    a minimal game
example-loop/               the same with its own loop (needs wasm-opt)
```

You also need [wasi-sdk](https://github.com/WebAssembly/wasi-sdk/releases) (clang,
wasi-libc and libc++ for wasm32).

```sh
export WASI_SDK_PATH=/path/to/wasi-sdk
cd example
cmake -B build -DCMAKE_TOOLCHAIN_FILE=../cmake/gasm-toolchain.cmake -DCMAKE_BUILD_TYPE=Release
cmake --build build
gasm-run build/hello.wasm        # from the gasm release / cargo install gasm-host
```

In your own project:

```cmake
cmake_minimum_required(VERSION 3.20)
project(mygame C)
include(Gasm)
gasm_add_game(mygame main.c)
```

Without CMake, use:
`clang --target=wasm32-wasip1 -mexec-model=reactor -O2 -Iinclude game.c -o game.wasm -lm`.
The header also works freestanding (`--target=wasm32 -nostdlib -Wl,--no-entry`).

For a game that keeps its own loop, write `int gasm_main(void)` that calls
`gasm_wait_frame()` once per frame and use `gasm_add_game(mygame LOOP main.c)`;
it needs `wasm-opt` from [Binaryen](https://github.com/WebAssembly/binaryen/releases)
(on `PATH` or `-DGASM_WASM_OPT=...`). See `include/gasm_loop.h`.

There's no filesystem, but `gasm_vfile.h` gives assets and saves a real
`FILE*`, so `fread`, `fgets`, `fprintf` and `fseek` keep working:

```c
FILE *wad = gasm_vfile_open(GASM_VFILE_ASSET, "maps/e1m1.lmp", "rb");   // streamed
FILE *sav = gasm_vfile_open(GASM_VFILE_STORAGE, "save1.dat", "wb");     // stored on fclose
```

Compile `src/gasm_vfile.c` with the game (it needs `_GNU_SOURCE` for
`fopencookie`); with CMake: `target_sources(mygame PRIVATE ${GASM_VFILE_SOURCE})`.

OpenGL ES 3.0 code builds unchanged with `#include <GLES3/gl3.h>` and
`src/gasm_gl.c` (CMake: `target_sources(mygame PRIVATE ${GASM_GL_SOURCE})`).
WebGL 2's rules apply (GLSL ES 3.00, no program binaries); `glMapBufferRange`
works on a copy in guest memory, and client-side vertex and index arrays (GLES 2
style, no buffer bound, the default vertex array only) are copied into buffers
at each draw. Code that loads GL by name (a loader, `SDL_GL_GetProcAddress`)
also links `src/gasm_gl_proc.c`: `gasm_gl_get_proc_address(name)` (CMake:
`${GASM_GL_PROC_SOURCE}`). A game uses `gasm:gl` or `gasm:gfx`, not both.
`gasm:gl` runs on WebGL 2 in browsers and on ANGLE in `gasm-run` (shipped with
the release bundles; `scripts/fetch-angle.sh` in the repository).

Newer imports can be probed before use: `gasm_has_str("gasm:gfx.destroy")`.
`gasm_set_title_str(title)` and `gasm_video_aspect(num, den)` probe for
themselves (the latter returns 0 if the runner doesn't show the aspect).
`gasm_storage_set` returns a `GASM_STORAGE_ERR_*` code on failure, and
`GASM_POINTER_OFF_*` / `GASM_GAMEPAD_OFF_*` are the field offsets of the
pointer and gamepad bytes.

Guide: https://gasm.emdzej.pl/dev/games#c-and-other-languages · ABI: https://gasm.emdzej.pl/docs/abi
