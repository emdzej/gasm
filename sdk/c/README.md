# gasm C/C++ SDK

Write games for [gasm](https://gasm.emdzej.pl) in C or C++. The SDK is one
header plus CMake glue:

```
include/gasm.h              the whole ABI (generated from spec/abi.json)
include/gasm_loop.h         optional: games with their own main loop
src/gasm_loop.c             (gasm_main + gasm_wait_frame, Binaryen Asyncify)
include/gasm_vfile.h        optional: stdio FILE* over assets and gasm:storage
src/gasm_vfile.c
cmake/gasm-toolchain.cmake  wasm32 toolchain (wraps wasi-sdk's)
cmake/Gasm.cmake            gasm_add_game(<target> [LOOP] <sources...>)
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

Newer imports can be probed before use: `gasm_has_str("gasm:gfx.destroy")`.
`gasm_set_title_str(title)` and `gasm_video_aspect(num, den)` probe for
themselves (the latter returns 0 if the runner doesn't show the aspect).
`gasm_storage_set` returns a `GASM_STORAGE_ERR_*` code on failure, and
`GASM_POINTER_OFF_*` / `GASM_GAMEPAD_OFF_*` are the field offsets of the
pointer and gamepad bytes.

Guide: https://gasm.emdzej.pl/dev/games#c-and-other-languages · ABI: https://gasm.emdzej.pl/docs/abi
