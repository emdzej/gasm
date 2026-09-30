# gasm_add_game(<target> <sources...>) — build a gasm game (<target>.wasm).
#
# Sets the WASI reactor exec model (no main(); _initialize runs constructors),
# adds gasm.h to the include path and links libm. Export the entry points in your
# code with GASM_EXPORT("gasm_abi_version"), GASM_EXPORT("gasm_init") and
# GASM_EXPORT("gasm_frame"). For C++, exceptions are disabled.

if(NOT GASM_INCLUDE_DIR)
  if(EXISTS "${CMAKE_CURRENT_LIST_DIR}/../include/gasm.h")        # release bundle layout
    get_filename_component(GASM_INCLUDE_DIR "${CMAKE_CURRENT_LIST_DIR}/../include" ABSOLUTE)
  elseif(EXISTS "${CMAKE_CURRENT_LIST_DIR}/../../../spec/gasm.h") # gasm repository layout
    get_filename_component(GASM_INCLUDE_DIR "${CMAKE_CURRENT_LIST_DIR}/../../../spec" ABSOLUTE)
  else()
    message(FATAL_ERROR "gasm: gasm.h not found; set GASM_INCLUDE_DIR")
  endif()
endif()

function(gasm_add_game target)
  add_executable(${target} ${ARGN})
  set_target_properties(${target} PROPERTIES SUFFIX ".wasm")
  target_include_directories(${target} PRIVATE "${GASM_INCLUDE_DIR}")
  target_compile_options(${target} PRIVATE $<$<COMPILE_LANGUAGE:CXX>:-fno-exceptions>)
  target_link_options(${target} PRIVATE -mexec-model=reactor)
  target_link_libraries(${target} PRIVATE m)
endfunction()
