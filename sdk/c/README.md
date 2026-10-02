# gasm C/C++ SDK

Write games for [gasm](https://gasm.emdzej.pl) in C or C++. The SDK is one
header plus CMake glue:

```
include/gasm.h              the whole ABI (generated from spec/abi.json)
include/gasm_loop.h         optional: games with their own main loop
src/gasm_loop.c             (gasm_main + gasm_wait_frame, Binaryen Asyncify)
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

Guide: https://gasm.emdzej.pl/dev/games#c-and-other-languages · ABI: https://gasm.emdzej.pl/docs/abi
