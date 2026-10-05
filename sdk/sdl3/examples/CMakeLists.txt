# SDL3 on gasm, built with CMake:
#   cmake -B build -DCMAKE_TOOLCHAIN_FILE=<gasm-c-sdk>/cmake/gasm-toolchain.cmake \
#         -DSDL3_DIR=<gasm-sdl3>/lib/cmake/SDL3 -DCMAKE_BUILD_TYPE=Release [-DGASM_WASM_OPT=...]
#   cmake --build build      ->  build/callbacks.wasm, build/classic.wasm (+ classic-run.wasm),
#                               build/threads.wasm, build/gl.wasm
cmake_minimum_required(VERSION 3.20)
project(gasm_sdl3_examples C)
find_package(SDL3 3.2 REQUIRED)

add_executable(callbacks callbacks/main.c)
target_link_libraries(callbacks PRIVATE SDL3::SDL3)
gasm_sdl3_app(callbacks)

add_executable(classic classic/main.c)
target_link_libraries(classic PRIVATE SDL3::SDL3)
gasm_sdl3_app(classic LOOP)

add_executable(threads threads/main.c)
target_link_libraries(threads PRIVATE SDL3::SDL3)
gasm_sdl3_app(threads LOOP THREADS)

add_executable(gl gl/main.c)
target_link_libraries(gl PRIVATE SDL3::SDL3 SDL3::GL)
gasm_sdl3_app(gl)
