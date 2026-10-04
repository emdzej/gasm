# Choosing a graphics API

A gasm game draws in one of three ways. All three run on every runner (native,
browser, headless), so the choice is about the code you have and the features
you need, not about where the game runs.

| | 2D frames (`video_present`) | `gasm:gfx` (WebGPU) | `gasm:gl` (OpenGL ES 3.0) |
|---|---|---|---|
| What you write | RGBA pixels, one call per frame | a WebGPU subset: WGSL shaders, JSON descriptors, handles | the GLES 3.0 API with WebGL 2's rules, GLSL ES 3.00 |
| Best for | emulators, software renderers, SDL 3's renderer, DOOM, ScummVM | new 3D or GPU 2D code written for gasm | existing GLES 3 / WebGL 2 renderers, engines with a GL backend |
| Browser | canvas, any browser | WebGPU | WebGL 2 (every current browser) |
| Native | wgpu | wgpu: Metal, Vulkan, Direct3D 12 | ANGLE: Metal, Direct3D 11, Vulkan; SwiftShader without a GPU |
| Headless | frames hashed; `--screenshot` | null GPU, uploads hashed; `--screenshot` renders | null GL, uploads hashed; `--screenshot` renders with ANGLE |
| Mistakes | — | trap (the game stops with a message) | GL errors (`glGetError`), the game carries on; bad pointers trap |
| C / C++ | `gasm.h` | `gasm.h` (raw imports) | drop-in `<GLES3/gl3.h>`: existing code compiles |
| Rust | `gasm::present` | `gasm::gfx` | glow (the gasm fork, `sdk/glow`), so glow-based crates such as egui_glow work |
| Upscaling filters | sharp, xbr, fsr, crt, integer scaling | no (you render at drawable size) | no |
| Worker mode (browser) | yes | yes (OffscreenCanvas + WebGPU in the worker) | no, main thread |
| Extra download natively | none | none | ANGLE, shipped next to `gasm-run` (on macOS about 6 MB, plus 17 MB of SwiftShader) |

A module imports `gasm:gfx` or `gasm:gl`, not both. A `gasm:gfx` game can
still call `video_present` (frames that drew with the GPU don't show it); a
`gasm:gl` game draws everything with GL.

## 2D frames

Pick this if the game already produces pixels: an emulator, a software
renderer, a CPU-drawn 2D game, anything on SDL 3's software renderer. It is
the simplest path, it costs nothing to port, and the runner scales the frame
for you (filters, display aspect, integer scaling; see
[presentation](https://github.com/emdzej/gasm/blob/main/design/presentation.md)).
At large resolutions the CPU does all the work, so 3D at 1080p is better on
the GPU.

## `gasm:gfx` (WebGPU)

Pick this for new GPU code written for gasm, in Rust or C. It is small and
strict:

- One render pass per frame, with depth and 4× MSAA managed by the runner;
  you never deal with the swapchain, resizes or resolves.
- Pipelines and bind groups are created once from JSON; per-frame calls take
  only numbers, so it is cheap to call and easy to bind from any language.
- Every mistake traps the same way on every runner (the null GPU validates
  like the real one), so a bug shows up in headless CI, not on one player's
  machine.
- It is a subset: no render targets, cube maps, compute or storage textures yet
  ([roadmap](/docs/roadmap)).
- In browsers it needs WebGPU.

Examples: [sumo](https://github.com/emdzej/gasm/tree/main/guests/sumo),
[triangle](https://github.com/emdzej/gasm/tree/main/guests/triangle),
[textured](https://github.com/emdzej/gasm/tree/main/guests/textured).
API tour: [Writing games, GPU](/dev/games#gpu-gasm-gfx).

## `gasm:gl` (OpenGL ES 3.0)

Pick this when the renderer already exists for GLES 3 or WebGL 2: an engine's
GL backend, a port of an Emscripten game, `glow`-style code. Porting a
renderer to WebGPU is usually the most expensive part of moving a game, and
with `gasm:gl` it isn't needed.

- The whole GLES 3.0 API, with WebGL 2's rules: render to texture,
  multisampled renderbuffers, instancing, uniform buffers, transform feedback,
  queries, `glMapBufferRange` (emulated in guest memory). No program binaries,
  no client-side vertex arrays, extensions only after enabling them.
- GL errors are GL's: `glGetError` reports them and the game continues, as GL
  code expects.
- Natively it runs on ANGLE in the same WebGL compatibility mode Chrome uses,
  so validation is the same in the browser and natively; `gasm-run` loads
  ANGLE only for these games.
- Browser games run on the main thread (no Worker mode yet).
- Queries and fences report results from the next frame on, everywhere.

Examples: [gltest](https://github.com/emdzej/gasm/blob/main/guests/gltest/main.c),
and Godot 4.7, whose Compatibility renderer runs on it
([guests/godot](https://github.com/emdzej/gasm/blob/main/guests/godot/README.md)).
Details: [Writing games, OpenGL ES 3](/dev/games#opengl-es-3) and the
[ABI](/docs/abi#gasm-gl-optional-opengl-es-3-0).

## In short

- You have pixels: **2D frames**.
- You are writing new GPU code for gasm: **`gasm:gfx`**.
- You have GL code: **`gasm:gl`**.
