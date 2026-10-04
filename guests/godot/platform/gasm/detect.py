# Godot's gasm platform (guests/godot, MIT): a wasm32 reactor built with wasi-sdk.
# The game's .pck is a gasm asset, user:// is gasm:storage, rendering is the
# Compatibility renderer on gasm:gl (OpenGL ES 3.0, WebGL 2 rules).
#   GASM_ROOT=<gasm repository> scons platform=gasm target=template_release
import os
import sys

from methods import print_error


def get_name():
    return "gasm"


def can_build():
    return "GASM_ROOT" in os.environ


def get_opts():
    return []


def get_doc_classes():
    return []


def get_doc_path():
    return "doc_classes"


def get_flags():
    return {
        "arch": "wasm32",
        "target": "template_release",
        "threads": False,
        "vulkan": False,
        "d3d12": False,
        "metal": False,
        "opengl3": True,
        "openxr": False,
        "builtin_pcre2_with_jit": False,
        "optimize": "size",
        # nothing to host these on gasm (sockets, WebXR, cameras, ...) or editor-only
        "module_enet_enabled": False,
        "module_upnp_enabled": False,
        "module_webrtc_enabled": False,
        "module_webxr_enabled": False,
        "module_websocket_enabled": False,
        "module_mobile_vr_enabled": False,
        "module_openxr_enabled": False,
        "module_camera_enabled": False,
        "module_mono_enabled": False,
        "module_raycast_enabled": False,
        "module_lightmapper_rd_enabled": False,
        "module_betsy_enabled": False,
        "module_glslang_enabled": False,
        "module_objectdb_profiler_enabled": False,
        # Jolt and mbedtls don't know WASI targets (Godot Physics 3D is used instead)
        "module_jolt_physics_enabled": False,
        "module_mbedtls_enabled": False,
        # the fallback text server (FreeType, no ICU/HarfBuzz): smaller, enough for games
        "module_text_server_adv_enabled": False,
        "module_text_server_fb_enabled": True,
    }


def configure(env):
    root = os.environ["GASM_ROOT"]
    sdk = os.environ.get("WASI_SDK", os.path.join(root, "tools", "wasi-sdk"))
    if not os.path.exists(os.path.join(sdk, "bin", "clang")):
        print_error("gasm: wasi-sdk not found at %s (scripts/fetch-wasi-sdk.sh)" % sdk)
        sys.exit(255)
    if env["arch"] != "wasm32":
        print_error("gasm: only wasm32")
        sys.exit(255)

    env["CC"] = os.path.join(sdk, "bin", "clang")
    env["CXX"] = os.path.join(sdk, "bin", "clang++")
    env["AR"] = os.path.join(sdk, "bin", "llvm-ar")
    env["RANLIB"] = os.path.join(sdk, "bin", "llvm-ranlib")
    env["LINK"] = env["CXX"]
    env["OBJPREFIX"] = ""
    env["OBJSUFFIX"] = ".o"
    env["PROGPREFIX"] = ""
    env["PROGSUFFIX"] = ".wasm"
    env["LIBPREFIX"] = "lib"
    env["LIBSUFFIX"] = ".a"
    env["LIBPREFIXES"] = ["$LIBPREFIX"]
    env["LIBSUFFIXES"] = ["$LIBSUFFIX"]
    env["ENV"] = os.environ

    target = ["--target=wasm32-wasip1"]
    # setjmp/longjmp (libpng, FreeType) on wasm exceptions, which every gasm runner has
    sjlj = ["-mllvm", "-wasm-enable-sjlj", "-mllvm", "-wasm-use-legacy-eh=false"]
    env.Append(CCFLAGS=target + sjlj + ["-fno-exceptions", "-D_WASI_EMULATED_SIGNAL", "-D_WASI_EMULATED_PROCESS_CLOCKS", "-D_WASI_EMULATED_MMAN"])
    env.Append(LINKFLAGS=target + ["-mexec-model=reactor", "-Wl,-z,stack-size=8388608", "-Wl,--export-if-defined=gasm_title"])
    env.Append(LIBS=["setjmp", "wasi-emulated-signal", "wasi-emulated-process-clocks", "wasi-emulated-mman"])

    if env["lto"] == "auto":
        env["lto"] = "full"
    if env["lto"] != "none":
        flag = "-flto=thin" if env["lto"] == "thin" else "-flto"
        env.Append(CCFLAGS=[flag])
        env.Append(LINKFLAGS=[flag])
        # LTO compiles at link time: the setjmp lowering must be told there too
        env.Append(LINKFLAGS=["-Wl,-mllvm,-wasm-enable-sjlj", "-Wl,-mllvm,-wasm-use-legacy-eh=false"])

    env.Prepend(CPPPATH=["#platform/gasm", os.path.join(root, "spec"), os.path.join(root, "sdk", "c", "include")])
    env.Append(CPPDEFINES=["GASM_ENABLED", "OVERRIDE_PATH_ENABLED"])
    if env["opengl3"]:
        env.Append(CPPDEFINES=["GLES3_ENABLED", "GLES_API_ENABLED"])
    env["GASM_ROOT"] = root
