# `gasm:gl`: OpenGL ES 3.0 for guests

> **Status: phases 1 to 4 implemented** (the ABI, the browser runner on
> WebGL 2, the null GL of both headless runners, the C drop-in headers,
> `guests/gltest`, ANGLE in gasm-run, glow for Rust). Left: Worker mode and the
> Godot port ([roadmap](https://github.com/emdzej/gasm/blob/main/site/docs/roadmap.md)).
> The normative description is the
> [`gasm:gl` section of ABI.md](https://github.com/emdzej/gasm/blob/main/spec/ABI.md).
> Where the implementation differs from the plan below:
>
> - ANGLE isn't built here: the libraries come from an Electron release
>   (43.7.7, the last that ships `libEGL`/`libGLESv2` as separate files, with
>   SwiftShader and the Vulkan loader). Official, checksummed, the same ANGLE as
>   Chrome's; `scripts/fetch-angle.sh` pins and fetches them, `package-angle.sh`
>   puts them in the bundles (macOS: lipo + ad-hoc re-sign). No `angle.yml`.
> - The context is created in ANGLE's WebGL compatibility mode
>   (`EGL_ANGLE_create_context_webgl_compatibility`, extensions through
>   `glRequestExtensionANGLE`), with robust access and resource
>   initialisation: natively a game gets the validation Chrome gives WebGL 2.
> - No `glow` and no `khronos-egl`: `gles.rs` (the GLES 3.0 function table,
>   generated from gl.xml by `scripts/gen-gl-headers.py`) and a small EGL loader
>   (`angle.rs`, `libloading`). The model runs every call first; the backend
>   (`gl_backend.rs`) executes only calls that recorded no GL error, as WebGL
>   does in the browser runner, so hashes don't depend on the backend.
> - glow's `HasContext` is sealed, so it can't be implemented outside glow, and
>   on `wasm32-unknown-unknown` glow only has its web-sys backend. Instead
>   `gasm::gles` implements the GLES 3.0 C API on the imports (the Rust twin of
>   `gasm_gl.c`; `get_proc_address` is the loader), and `sdk/glow` is glow
>   0.17.0 with its native backend on wasm32 (`sdk/glow.patch`, 3 small changes,
>   rebuilt by `scripts/update-glow.sh`). Games use it with `[patch.crates-io]`,
>   so crates built on glow (egui_glow) work unchanged: `guests/glowtest`,
>   `guests/eguidemo`.
> - Linux windows are X11 (XWayland on Wayland); `--gl-software` (SwiftShader)
>   works on Linux and Windows, and is the automatic fallback when the GPU path
>   fails. CI checks gltest on ANGLE on all four platforms
>   (`scripts/gl-native-test.sh`: the hashes equal the null GL's).
>
> - The null GL's limits and strings are tables in the two runners
>   (`runners/web/lib/gl.js`, `runners/native/src/gl.rs`), not in `abi.json`.
> - The native runner links the 224 imports from a generated signature table
>   (`gl_sigs.rs`) and dispatches by name; `gen-abi.mjs --check` checks that both
>   runners handle every function (JS: arity too).
> - Worker mode isn't done: `gasm:gl` games run on the main thread.
> - gltest is smaller than described (a textured, instanced quad, a uniform
>   block, render to texture + blit, a mapped buffer, a query and a fence) and
>   uploads the GL errors of 15 deliberate mistakes each frame, so the hash
>   checks error parity: Chrome's WebGL 2 and both null GLs give the same hash.
> - Chrome (ANGLE on Metal) drops a clear-only framebuffer's contents when it is
>   blitted without a draw in between; gltest draws into its framebuffer.

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
  WebGL 2's restrictions. It uses about 130 distinct GL functions (an estimate
  from grepping `drivers/gles3`; to be counted from a build). With `gasm:gl`,
  Godot 2D and simple 3D games need a platform port (`platform/gasm`) and no new
  renderer.
- **Engines and games with an Emscripten/WebGL 2 build.** Their GL code already
  lives within WebGL 2's limits.
- **SDL/GLES ports** (many open-source games) and Rust code using `glow`.

## Goals

1. Guests written in C/C++ compile against a standard `GLES3/gl3.h` and run
   unchanged on every runner.
2. Same behaviour in the browser and natively, as far as possible: ANGLE on
   both sides where browsers use it (Chrome and Edge everywhere, Safari, Firefox
   on Windows). Firefox on macOS and Linux implements WebGL 2 on the system's
   OpenGL instead, so driver differences show up there as they do for any WebGL
   game.
3. gasm's properties are kept: every guest pointer is checked, nothing can crash
   the runner, headless runs are reproducible and hashed.
4. Worker mode works (WebGL 2 on an `OffscreenCanvas`).

## Non-goals

- Desktop OpenGL, GL 1.x fixed function, GLES 3.1+ (compute, SSBOs, images).
- Client-side vertex arrays and other things WebGL 2 forbids.
- Mixing `gasm:gl` and `gasm:gfx` in one guest. Runners refuse a module that
  imports both, at load time, with an error saying so.
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
  way (`enable_extension(name)`). Headless runs report a fixed set (see
  "Null GL" below), so a guest's choices don't depend on the machine. The set exposed is the intersection the
  runners can provide on every platform, plus optional ones a game must check:
  `EXT_color_buffer_float`, `OES_texture_float_linear`,
  `EXT_texture_filter_anisotropic`, and compressed formats (S3TC/BPTC on desktop,
  ETC2/ASTC where the GPU has them).
- Shaders are GLSL ES 3.00 (`#version 300 es`), as in WebGL 2.
- `generate_mipmap` is part of WebGL 2 and of this module: here runners do
  generate mipmaps, unlike `gasm:gfx`, where guests upload every level (the
  "runners never generate mipmaps" rule is about `gasm:gfx`). The null GL only
  records that the levels exist; hashes cover the uploads, not generated levels.

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
| Screenshots | canvas | ANGLE pbuffer surface, `readPixels` | `gasm-run --screenshot`: ANGLE offscreen. The Node runner has no GL (it stays dependency-free) and captures nothing, as with `gasm:gfx` |

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
local builds (checksum-pinned, like every other download); the release bundles
and the macOS `.app`s include them (size to be measured; BSD-3-Clause, notices
in `THIRD-PARTY.txt`). CI also needs ANGLE's SwiftShader backend for GPU-less
runners (size to be measured too). The ANGLE build runs only
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
  same GL errors as a real context for the misuse it can see (bad names,
  wrong targets, invalid enums, out-of-range levels and sizes, incomplete
  framebuffers by its own completeness rules). Everything a guest may branch on
  is fixed in headless runs, so they stay reproducible on every machine:
  - **Limits** (`GL_MAX_*`) are WebGL 2's guaranteed minimums, from a table in
    `abi.json`; windowed runs report the device's.
  - **Extensions:** none of the optional ones.
  - **Shaders and programs** compile and link successfully. The null GL
    doesn't parse GLSL, so introspection is synthetic and deterministic:
    `get_uniform_location`, `get_attrib_location` and
    `get_uniform_block_index` give every name asked for a new location or
    index, numbered in the order of the queries, never -1;
    `GL_ACTIVE_UNIFORMS`, `GL_ACTIVE_ATTRIBUTES` and `GL_ACTIVE_UNIFORM_BLOCKS`
    are 0. So `uniform*` calls reach the hash; a guest that enumerates active
    uniforms instead of looking them up sees none headless (Godot looks them
    up).
  - **GPU results:** `read_pixels` returns zeros, occlusion queries "passed",
    timer queries 0, `client_wait_sync` "already signaled".
- **Error parity is tested, not assumed.** Three implementations answer
  `get_error` (WebGL, ANGLE, the null GL in Rust and JS). gltest includes a
  list of misuse cases (each with the error it must produce), run on all of
  them in CI: Chrome, native ANGLE (SwiftShader), and both null GLs. The null
  GL only claims parity for the cases on that list; growing the list is part
  of each phase.
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
- **Asynchronous WebGL.** Some results only arrive after the page returns to
  the browser's event loop: query results (`GL_QUERY_RESULT_AVAILABLE` stays
  false within a frame), and `client_wait_sync` can only wait with timeout 0.
  Natively they could arrive at once. To behave the same, runners report
  query results and sync status no earlier than the next frame on every
  runner (natively too), and `client_wait_sync` with a timeout above 0 returns
  `GL_TIMEOUT_EXPIRED` unless already signaled.
- **Context loss.** Browsers can drop a WebGL context (GPU reset, too many
  contexts). The runner stops the guest with a clear message (no restore in
  v1); natively ANGLE contexts are robust and report resets the same way.
- **Compressed textures** differ by platform (S3TC/BPTC on desktop, ETC2/ASTC on
  mobile and some browsers). Games must query, as on WebGL. Godot already does.
- **Two GPU modules** means two paths in each runner. Mitigated by keeping them
  separate (one per guest) and sharing the surface, input and frame logic.
- **Version.** An additive change by the ABI's versioning rules (a new optional
  module, see `spec/ABI.md`), so `GASM_ABI_VERSION` stays 0; guests probe
  with `has("gasm:gl")`.
