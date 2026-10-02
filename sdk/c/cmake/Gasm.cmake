# gasm_add_game(<target> [LOOP] <sources...>) — build a gasm game (<target>.wasm).
#
# Sets the WASI reactor exec model (no main(); _initialize runs constructors),
# adds gasm.h to the include path and links libm. Export the entry points in your
# code with GASM_EXPORT("gasm_abi_version"), GASM_EXPORT("gasm_init") and
# GASM_EXPORT("gasm_frame"). For C++, exceptions are disabled.
#
# LOOP: the game keeps its own main loop instead (gasm_loop.h): define gasm_main()
# and call gasm_wait_frame() once per frame. gasm_loop.c provides the exports, and
# the module is post-processed with Binaryen's wasm-opt --asyncify (set GASM_WASM_OPT,
# or have wasm-opt on PATH; GASM_LOOP_STACK_SIZE sets the suspended-stack buffer, per
# target). wasm-opt optimizes (-O2) except in Debug builds, which keep names (-g).
#
# stdio over assets and storage (gasm_vfile.h): target_sources(<target> PRIVATE ${GASM_VFILE_SOURCE}).

if(NOT GASM_INCLUDE_DIR)
  if(EXISTS "${CMAKE_CURRENT_LIST_DIR}/../include/gasm.h")        # release bundle layout
    get_filename_component(GASM_INCLUDE_DIR "${CMAKE_CURRENT_LIST_DIR}/../include" ABSOLUTE)
  elseif(EXISTS "${CMAKE_CURRENT_LIST_DIR}/../../../spec/gasm.h") # gasm repository layout
    get_filename_component(GASM_INCLUDE_DIR "${CMAKE_CURRENT_LIST_DIR}/../../../spec" ABSOLUTE)
  else()
    message(FATAL_ERROR "gasm: gasm.h not found; set GASM_INCLUDE_DIR")
  endif()
endif()

# gasm_loop.h (and other SDK headers): sdk/c/include in the repository, include/ in the bundle
get_filename_component(GASM_SDK_INCLUDE_DIR "${CMAKE_CURRENT_LIST_DIR}/../include" ABSOLUTE)

if(NOT GASM_LOOP_SOURCE)
  if(EXISTS "${CMAKE_CURRENT_LIST_DIR}/../src/gasm_loop.c")
    get_filename_component(GASM_LOOP_SOURCE "${CMAKE_CURRENT_LIST_DIR}/../src/gasm_loop.c" ABSOLUTE)
  endif()
endif()
if(NOT GASM_VFILE_SOURCE AND EXISTS "${CMAKE_CURRENT_LIST_DIR}/../src/gasm_vfile.c")
  get_filename_component(GASM_VFILE_SOURCE "${CMAKE_CURRENT_LIST_DIR}/../src/gasm_vfile.c" ABSOLUTE)
endif()

function(gasm_add_game target)
  set(sources ${ARGN})
  set(loop OFF)
  if(sources AND "${ARGV1}" STREQUAL "LOOP")
    set(loop ON)
    list(REMOVE_AT sources 0)
  endif()
  if(loop)
    if(NOT GASM_LOOP_SOURCE)
      message(FATAL_ERROR "gasm: gasm_loop.c not found; set GASM_LOOP_SOURCE")
    endif()
    list(APPEND sources "${GASM_LOOP_SOURCE}")
  endif()
  add_executable(${target} ${sources})
  set_target_properties(${target} PROPERTIES SUFFIX ".wasm")
  target_include_directories(${target} PRIVATE "${GASM_INCLUDE_DIR}" "${GASM_SDK_INCLUDE_DIR}")
  target_compile_options(${target} PRIVATE $<$<COMPILE_LANGUAGE:CXX>:-fno-exceptions>)
  target_link_options(${target} PRIVATE -mexec-model=reactor)
  target_link_libraries(${target} PRIVATE m)
  if(loop)
    if(GASM_LOOP_STACK_SIZE)
      # per target (the source is shared between targets)
      target_compile_definitions(${target} PRIVATE "GASM_LOOP_STACK_SIZE=${GASM_LOOP_STACK_SIZE}")
    endif()
    target_link_options(${target} PRIVATE -Wl,--wrap=exit)
    if(NOT GASM_WASM_OPT)
      find_program(GASM_WASM_OPT wasm-opt)
    endif()
    if(NOT GASM_WASM_OPT)
      message(FATAL_ERROR "gasm: LOOP needs Binaryen's wasm-opt; set GASM_WASM_OPT")
    endif()
    # Asyncify before optimizing; the frame export must not be instrumented (gasm_loop.c)
    add_custom_command(TARGET ${target} POST_BUILD
      COMMAND "${GASM_WASM_OPT}" "$<TARGET_FILE:${target}>" --asyncify
              --pass-arg=asyncify-removelist@gasm_loop_frame "$<IF:$<CONFIG:Debug>,-g,-O2>" -o "$<TARGET_FILE:${target}>"
      VERBATIM)
  endif()
endfunction()
