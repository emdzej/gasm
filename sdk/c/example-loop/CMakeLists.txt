# A game with its own main loop (gasm_loop.h, Binaryen's wasm-opt needed):
#   cmake -B build -DCMAKE_TOOLCHAIN_FILE=../cmake/gasm-toolchain.cmake -DCMAKE_BUILD_TYPE=Release \
#         [-DGASM_WASM_OPT=/path/to/wasm-opt]
#   cmake --build build        ->  build/example-loop.wasm
cmake_minimum_required(VERSION 3.20)
project(example_loop C)
include(Gasm)
gasm_add_game(example-loop LOOP main.c)
