# Minimal gasm game in C:
#   cmake -B build -DCMAKE_TOOLCHAIN_FILE=../cmake/gasm-toolchain.cmake -DCMAKE_BUILD_TYPE=Release
#   cmake --build build        ->  build/hello.wasm
#   gasm-run build/hello.wasm
cmake_minimum_required(VERSION 3.20)
project(hello C)
include(Gasm)
gasm_add_game(hello main.c)
