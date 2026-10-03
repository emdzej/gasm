# SDL3 for gasm: find_package(SDL3) gives SDL3::SDL3 (static, wasm32).
#
#   cmake -B build -DCMAKE_TOOLCHAIN_FILE=<gasm-c-sdk>/cmake/gasm-toolchain.cmake \
#         -DSDL3_DIR=<gasm-sdl3>/lib/cmake/SDL3
#
#   find_package(SDL3 REQUIRED)
#   add_executable(mygame main.c)
#   target_link_libraries(mygame PRIVATE SDL3::SDL3)
#   gasm_sdl3_app(mygame)          # SDL_MAIN_USE_CALLBACKS apps
#   gasm_sdl3_app(mygame LOOP)     # apps with their own main loop (needs wasm-opt)
#   gasm_sdl3_app(mygame LOOP THREADS)   # ... that also create threads (SDL_CreateThread)
#
# gasm_sdl3_app names the output <target>.wasm and links it as a reactor. LOOP runs
# Binaryen's wasm-opt --asyncify on it (set GASM_WASM_OPT, or have wasm-opt on PATH),
# so main() can stay a loop: frames end at SDL_RenderPresent / SDL_UpdateWindowSurface.
# It also writes <target>-run.wasm without Asyncify, for runners that switch stacks.
# THREADS links the loop helper with cooperative threads (gasm_loop_threads.o):
# SDL_CreateThread, SDL_AddTimer, mutexes, conditions and semaphores then work, one
# thread at a time (design/threads.md); such apps come as the Asyncify build only.

get_filename_component(_gasm_sdl3_root "${CMAKE_CURRENT_LIST_DIR}/../../.." ABSOLUTE)

if(NOT TARGET SDL3::SDL3-static)
  add_library(SDL3::SDL3-static STATIC IMPORTED)
  set_target_properties(SDL3::SDL3-static PROPERTIES
    IMPORTED_LOCATION "${_gasm_sdl3_root}/lib/libSDL3.a"
    INTERFACE_INCLUDE_DIRECTORIES "${_gasm_sdl3_root}/include"
    INTERFACE_LINK_LIBRARIES m)
endif()
if(NOT TARGET SDL3::SDL3)
  add_library(SDL3::SDL3 INTERFACE IMPORTED)
  set_target_properties(SDL3::SDL3 PROPERTIES INTERFACE_LINK_LIBRARIES SDL3::SDL3-static)
endif()
# SDL3_test and SDL3_image etc. are not part of this build
set(SDL3_FOUND TRUE)
set(SDL3_SDL3_FOUND TRUE)
set(SDL3_SDL3-static_FOUND TRUE)

function(gasm_sdl3_app target)
  set_target_properties(${target} PROPERTIES SUFFIX ".wasm")
  target_link_options(${target} PRIVATE -mexec-model=reactor)
  if("${ARGV1}" STREQUAL "LOOP")
    if(NOT GASM_WASM_OPT)
      find_program(GASM_WASM_OPT wasm-opt)
    endif()
    if(NOT GASM_WASM_OPT)
      message(FATAL_ERROR "gasm: LOOP needs Binaryen's wasm-opt; set GASM_WASM_OPT")
    endif()
    if("${ARGV2}" STREQUAL "THREADS")
      # an object before the libraries: it replaces libSDL3.a's plain loop helper
      target_sources(${target} PRIVATE "${_gasm_sdl3_root}/lib/gasm_loop_threads.o")
    else()
      add_custom_command(TARGET ${target} POST_BUILD
        COMMAND "${GASM_WASM_OPT}" "$<TARGET_FILE:${target}>" -O2 -o "$<TARGET_FILE_DIR:${target}>/${target}-run.wasm"
        VERBATIM)
    endif()
    # Asyncify before optimizing; gasm_loop_frame (the frame export) must not be instrumented
    add_custom_command(TARGET ${target} POST_BUILD
      COMMAND "${GASM_WASM_OPT}" "$<TARGET_FILE:${target}>" --asyncify
              --pass-arg=asyncify-removelist@gasm_loop_frame -O2 -o "$<TARGET_FILE:${target}>"
      VERBATIM)
  endif()
endfunction()
