# `gasm:gl`: OpenGL ES 3.0 for guests

> **Status: proposed.** Nothing implemented yet. This is the feature description
> and the plan; the ABI details land in `spec/abi.json` and `spec/ABI.md` when
> the work starts.

## Summary

A new optional import module, `gasm:gl`, that gives guests **OpenGL ES 3.0 with
WebGL 2's rules**. In the browser it forwards almost call for call to a WebGL 2
context. Natively it runs on **ANGLE** (Chrome's GLES implementation) on top of
Metal, Direct3D 11 or Vulkan, so native and browser compile shaders and validate
calls with the same code. Headless runs get a null GL that tracks objects and
hashes uploads, like the null GPU of `gasm:gfx`.

It sits next to `gasm:gfx` (WebGPU), not instead of it. A game imports one of
the two.

## Why

`gasm:gfx` is the right API for code written for gasm. A lot of existing code
is written for GLES 3 / WebGL 2, and porting its renderer to WebGPU is the most
expensive part of bringing it to gasm:

- **Godot.** Its Compatibility renderer (`drivers/gles3`) targets exactly
  GLES 3 / WebGL 2 and already has web-specific code paths (`WEB_ENABLED`) for
  WebGL 2's restrictions. It uses 126 distinct GL functions. With `gasm:gl`,
  Godot 2D and simple 3D games need a platform port (`platform/gasm`) and no new
  renderer.
- **Engines and games with an Emscripten/WebGL 2 build.** Their GL code already
  lives within WebGL 2's limits.
- **SDL/GLES ports** (many open-source games) and Rust code using `glow`.

## Goals

1. Guests written in C/C++ compile against a standard `GLES3/gl3.h` and run
   unchanged on every runner.
2. Same behaviour in the browser and natively: ANGLE on both sides (Chrome,
   Firefox on Windows and Safari use ANGLE for WebGL too).
3. gasm's properties are kept: every guest pointer is checked, nothing can crash
   the runner, headless runs are reproducible and hashed.
4. Worker mode works (WebGL 2 on an `OffscreenCanvas`).

## Non-goals

- Desktop OpenGL, GL 1.x fixed function, GLES 3.1+ (compute, SSBOs, images).
- Client-side vertex arrays and other things WebGL 2 forbids.
- Mixing `gasm:gl` and `gasm:gfx` in one guest.
- Making rendered pixels identical across GPUs (they aren't, as with `gasm:gfx`).

## The API

**Shape: the GLES 3.0 C API, restricted to what WebGL 2 allows.** Guests are
C/C++, so the guest-facing names and object model are GLES's: integer names
from `glGen*`, `GLenum` constants from the Khronos headers. The browser runner
keeps a name → WebGL object table, as Emscripten does.

What the ABI looks like:

- One import per GL entry point WebGL 2 has: about 220 (state, buffers,
  textures, samplers, framebuffers and renderbuffers, shaders and programs,
  uniforms and uniform blocks, vertex arrays, instancing, transform feedback,
  queries, sync, `readPixels`). The list and its signatures go into `abi.json`,
  so `gasm.h`, the Rust bindings and the conformance checks are generated, as for
  every other module.
- Data goes in as `(ptr, len)` into guest memory: buffer and texture uploads,
  uniform arrays, `readPixels` targets. Lengths are explicit and checked, unlike
  in C GL.
- Strings: `shader_source(shader, ptr, len)` takes one string; the C header
  joins GL's string arrays. `get_string` copies into a buffer (the header caches
  it so `glGetString` can return a pointer).
- GL errors are GL's: `get_error` returns them, the call has no effect. GL code
  checks `glGetError` and expects to continue, so these don't trap. Violations of
  the gasm boundary (out-of-bounds pointers, impossible lengths, bad names for
  the handle table) do trap, as everywhere else.
- Extensions are queried with `get_string(GL_EXTENSIONS)` and enabled the WebGL
  way (`enable_extension(name)`). The set exposed is the intersection the
  runners can provide on every platform, plus optional ones a game must check:
  `EXT_color_buffer_float`, `OES_texture_float_linear`,
  `EXT_texture_filter_anisotropic`, and compressed formats (S3TC/BPTC on desktop,
  ETC2/ASTC where the GPU has them).
- Shaders are GLSL ES 3.00 (`#version 300 es`), as in WebGL 2.

**Frames and the default framebuffer.** The runner owns the window surface:

| Import | |
|---|---|
| `width` / `height` | Drawable size; the default framebuffer follows it on resize. |
| `frame_shown() -> u32` | `0` during catch-up frames and headless runs (the guest may skip drawing), as `begin_frame` returns in `gasm:gfx`. |
| `present()` | Show the default framebuffer now (`eglSwapBuffers`; nothing in the browser). If the guest doesn't call it, the runner presents at the end of the frame. |

Context: GLES 3.0, depth 24 + stencil 8, no alpha, no MSAA on the default
framebuffer (games use multisampled renderbuffers), not premultiplied, buffer
not preserved. Fixed, so every runner creates the same thing.

**C SDK.** A drop-in `GLES3/gl3.h` + `GLES2/gl2ext.h` (generated) so existing code
compiles with `#include <GLES3/gl3.h>`. It implements the parts WebGL 2 lacks
in the guest, as Emscripten does: `glMapBufferRange`/`glUnmapBuffer` (guest
memory, uploaded with `buffer_sub_data` on unmap), `glGetString` caching,
`glShaderSource` with string arrays. Godot uses `glMapBufferRange`, so this
matters.

**Rust SDK.** Raw bindings (`gasm::sys::gl_*`), plus a `glow::HasContext`
implementation so `glow`-based code (egui_glow, many small engines) runs as is.

## Runners

| | Browser | Native | Headless |
|---|---|---|---|
| Context | canvas `getContext('webgl2')` (also `OffscreenCanvas` in Worker mode) | ANGLE via EGL: Metal (macOS), D3D11 (Windows), Vulkan (Linux, X11 and Wayland) | null GL |
| Calls | forwarded to WebGL 2 through the name table | forwarded to ANGLE's GLES 3 (`glow` loads the entry points) | validated, objects tracked, nothing drawn |
| Screenshots | canvas | ANGLE pbuffer surface, `readPixels` | with `--screenshot`: ANGLE offscreen |

**Native details.**

- ANGLE is loaded dynamically (`libEGL` + `libGLESv2`) the first time a guest
  imports `gasm:gl`, so `gasm-run` keeps working without it for everything else.
  `--gl-lib DIR` overrides the location.
- The window surface comes from winit's window handle: `CAMetalLayer` on macOS,
  `HWND` on Windows, X11/Wayland surfaces on Linux.
- ANGLE runs with robust buffer access, so out-of-range reads in shaders return
  zero instead of touching other memory.
- New crates: `glow` (GL function loading and calls) and `khronos-egl` (EGL,
  dynamically loaded). Both are widely used.

**Distributing ANGLE.** There are no official binaries; community prebuilts
exist but aren't something to depend on. Plan: a separate workflow
(`.github/workflows/angle.yml`) builds ANGLE at a pinned revision for macOS
(universal), Windows x86_64, Linux x86_64 and arm64, and publishes the libraries
as a release (`angle-<revision>`). `scripts/fetch-angle.sh` downloads them for
local builds; the release bundles and the macOS `.app`s include them (about
10 MB per platform, BSD-3-Clause, notices included). The ANGLE build runs only
when the pinned revision changes.

**macOS signatures.** No Apple certificate is involved, but one packaging step
is needed:

- On Apple silicon every executable and library must carry a valid code
  signature; an *ad-hoc* one (`codesign --sign -`, no identity) is enough. The
  linker adds it, which is why `gasm-run` needs nothing today (`lipo` keeps each
  slice's signature).
- Packaging edits the ANGLE dylibs: `install_name_tool` sets their install names
  to `@loader_path/...` so `gasm-run` and the `.app`s find them, and the two
  architectures are merged. Editing a Mach-O invalidates its signature and macOS
  then refuses to load it ("code signature invalid"). So `package-macos.sh` and
  the CLI packaging re-sign them ad hoc afterwards:
  `codesign --force --sign - libEGL.dylib libGLESv2.dylib`.
- Gatekeeper is unchanged: releases aren't Developer ID signed or notarized,
  and the one-time approval the guide already describes (right-click, Open, or
  `xattr -dr com.apple.quarantine`) covers the dylibs in the same `.app` or
  folder. To verify on a clean download. Notarization (a paid Apple Developer
  ID) would help every gasm release and is a separate decision.
- Windows DLLs and Linux `.so` files need no signatures.

## Validation, security, determinism

- Every `(ptr, len)` is bounds-checked; lengths are checked against what the call
  needs (`tex_image_2d` against width × height × format, `buffer_sub_data`
  against the buffer size). Violations trap.
- Each object kind has its own name table in every runner, so a texture name
  can't be used as a buffer. Unknown names follow GL rules (`GL_INVALID_*`).
- **Null GL:** the same tables and the same checks, so headless runs report the
  same GL errors as a real context for API misuse. Queries that need a GPU
  (`readPixels`, occlusion queries, shader compile and link status)
  return documented defaults: zeros, "passed", success.
- **Hashing:** headless runners fold every upload into the video hash: buffer
  data, texture data (header + payload, as `write_texture`), uniform values.
  Draw calls and state aren't hashed, as in `gasm:gfx`.
- Runners link unknown imports as traps, so a guest calling an extension
  function that isn't available fails visibly instead of silently.

## Testing

- `guests/gltest` (C, GLES 3 through the drop-in header): a lit, textured,
  instanced cube rendered into a framebuffer object with MSAA, resolved and
  drawn to the screen with a post-process shader, a uniform buffer, transform
  feedback particles, `glMapBufferRange` updates, and a deliberate GL error that
  must be reported, not trapped.
- Determinism case: hashes equal on wasmtime JIT, AOT and Node.
- Screenshots: native (ANGLE) and Chrome rendering of the same frame, checked by
  eye and with a tolerant image diff (GPUs differ in the last bits).
- `gen-abi.mjs --check` extended to `gasm:gl` (both runners, the stub, the C
  header).
- CI runs the native ANGLE path where runners have a GPU or a software backend
  (ANGLE's SwiftShader backend works without a GPU; that keeps CI honest).

## Plan

| Phase | Deliverable | Done when |
|---|---|---|
| 1. ABI | `gasm:gl` in `abi.json`: functions, enums, the frame imports; `ABI.md` section | generated `gasm.h`/`sys.rs`, conformance check knows the module |
| 2. Browser + headless | JS runner (WebGL 2 forwarding, name tables, Worker mode), null GL in both runners and the Rust stub, hashing; C drop-in headers; `guests/gltest` | gltest runs in Chrome and headless; hashes equal on JIT, AOT, Node |
| 3. ANGLE | `angle.yml` build + release, `fetch-angle.sh`, native EGL contexts (window + offscreen) on macOS, Windows, Linux | gltest native screenshots match Chrome's; CI green on all three |
| 4. SDKs and docs | Rust `glow` backend, site docs, packaging (bundles, `.app`s), release | a new gasm release with `gasm:gl` |
| 5. Godot | `platform/gasm` port on `gasm:gl` (separate plan): headless first, then the Compatibility renderer | a Godot 2D demo project runs natively and in the browser |

Phases 1–2 are the bulk of the API work and need no native GPU code, so they can
be verified early. Phase 3 is mostly build and packaging work. Phase 5 is its own
project.

## Risks and open questions

- **ANGLE build and packaging.** Building ANGLE needs depot_tools and takes a
  while; it only happens when the pin changes. On macOS the dylibs need an
  ad-hoc re-sign after packaging edits them (see "macOS signatures"); a missed
  step shows up as a library that won't load, so the packaged bundles get a
  smoke test.
- **Call volume.** GL is chatty (thousands of calls per frame). Calls from wasm
  into wasmtime are cheap; in the browser each call crosses into JavaScript, as
  with any Emscripten WebGL build. Expected to be fine; measure with gltest and
  Godot.
- **Compressed textures** differ by platform (S3TC/BPTC on desktop, ETC2/ASTC on
  mobile and some browsers). Games must query, as on WebGL. Godot already does.
- **Two GPU modules** means two paths in each runner. Mitigated by keeping them
  separate (one per guest) and sharing the surface, input and frame logic.
- **Version.** Additive like 0.4/0.5 (new optional module), so
  `GASM_ABI_VERSION` stays 0.
