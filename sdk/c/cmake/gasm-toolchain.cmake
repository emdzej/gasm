# CMake toolchain for gasm games in C/C++ (wasm32 via wasi-sdk).
#
#   cmake -B build -DCMAKE_TOOLCHAIN_FILE=<sdk>/cmake/gasm-toolchain.cmake [-DWASI_SDK_PREFIX=/path/to/wasi-sdk]
#
# wasi-sdk is found via -DWASI_SDK_PREFIX, then $WASI_SDK_PATH, then ../tools/wasi-sdk
# relative to a gasm checkout. Get it from https://github.com/WebAssembly/wasi-sdk/releases.
# Then include(Gasm) in your CMakeLists.txt and call gasm_add_game().

if(NOT WASI_SDK_PREFIX)
  if(DEFINED ENV{WASI_SDK_PATH})
    set(WASI_SDK_PREFIX "$ENV{WASI_SDK_PATH}")
  elseif(EXISTS "${CMAKE_CURRENT_LIST_DIR}/../../../tools/wasi-sdk/share/cmake/wasi-sdk-p1.cmake")
    get_filename_component(WASI_SDK_PREFIX "${CMAKE_CURRENT_LIST_DIR}/../../../tools/wasi-sdk" ABSOLUTE)
  else()
    message(FATAL_ERROR "gasm: set WASI_SDK_PREFIX or WASI_SDK_PATH to your wasi-sdk directory")
  endif()
endif()
# try_compile re-reads toolchain files in fresh scopes: pass the prefix along.
set(CMAKE_TRY_COMPILE_PLATFORM_VARIABLES WASI_SDK_PREFIX)

include("${WASI_SDK_PREFIX}/share/cmake/wasi-sdk-p1.cmake")
list(APPEND CMAKE_MODULE_PATH "${CMAKE_CURRENT_LIST_DIR}")
