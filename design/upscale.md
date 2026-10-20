# gasm:upscale: temporal upscaling for GPU games

Status: **proposal** (not started; waits for `gasm:gfx` render targets and a game
that renders motion vectors).

## Summary

3D games render fewer pixels than the screen has and upscale them: a temporal
upscaler combines the jittered low-resolution frames of the last few frames into a
sharper image at the output size. DLSS (NVIDIA), FSR 2/3 (AMD), XeSS (Intel) and
MetalFX (Apple) all do this from the same inputs: colour, depth, motion vectors
and the camera's jitter.

`gasm:upscale` would be an optional module for `gasm:gfx` games: the game hands
the runner those textures each frame and gets the upscaled one back. The runner
picks the best upscaler it has: a portable one that every runner ships (an FSR
2-class upscaler in WGSL), and natively, where it finds them, the platform's own
(MetalFX on macOS, DLSS on NVIDIA GPUs, maybe XeSS). DLSS is never shipped: the
runner uses it only when the player has its runtime and the GPU can run it.

Asked about for DLSS (2026-10-18). Nothing needs it yet: see
[Prerequisites](#prerequisites).

## What it is not

- **Not for 2D frames.** `video_present` games have no depth or motion vectors;
  they have the runner's spatial filters (`sharp`, `xbr`, `fsr` = FSR 1,
  [presentation.md](presentation.md)), which need nothing from the game.
- **Not for `gasm:gl` games** (Godot's Compatibility renderer, SDL, C GL code).
  The Compatibility renderer makes no motion vectors (Godot's temporal features
  need Forward+ or Mobile: NiP #14), and natively the textures live inside ANGLE:
  handing them to DLSS would mean Vulkan/D3D11 interop underneath ANGLE.
  A GL game can still run an upscaler in its own shaders.
- **Not a frame generator.** DLSS/FSR frame generation inserts frames the game
  never ran; gasm's frames are the game's frames (and its hashes count them).

## Prerequisites

1. **`gasm:gfx` render targets** (on the roadmap, not started): today a texture
   is `rgba8unorm` with `TEXTURE_BINDING | COPY_DST` only, and the render pass
   draws into the runner's own colour, depth24plus and 4x MSAA attachments. The
   upscaler's inputs must be textures the game rendered into: colour (`rgba8unorm`,
   `rgba16float` for HDR), depth the upscaler can read (`depth32float`, sampled),
   motion vectors (`rg16float`), all without MSAA, and an output texture the
   upscaler writes and the game then samples or presents.
2. **A guest that renders motion vectors.** A test guest (`upscaletest`: a jittered
   scene with moving objects and camera cuts) comes with phase 1; a real game is
   what would make it worth doing.

## The API (sketch)

Module `gasm:upscale` (optional; probe `has("gasm:upscale")`). Handles and
descriptions as in `gasm:gfx` (JSON descriptions, handles numbered in creation
order, validated identically by every runner, violations trap).

| function | signature | what |
|---|---|---|
| `create` | `(ptr, len) -> u32` | `{"output":[W,H],"quality":"balanced","hdr":false}`: an upscaler for that output size. `quality` is `quality`, `balanced`, `performance` or `ultra-performance`. |
| `render_size` | `(h, ptr) -> i32` | writes `u32 w, h`: the size to render the inputs at. Fixed ratios in the ABI (1.5, 1.7, 2, 3 per side), the same on every runner and backend. |
| `jitter` | `(h, index, ptr) -> i32` | writes `f32 x, y`: the sub-pixel camera offset for frame `index` (Halton 2,3 over a phase count set by the ratio), in pixels, the same on every runner. |
| `dispatch` | `(h, ptr, len)` | `{"color":t,"depth":t,"motion":t,"output":t,"jitter":[x,y],"motionScale":[sx,sy],"reset":false,"sharpness":0.5}`; between render passes, never inside one. The runner records the upscale into the frame's commands. |
| `destroy` | `(h)` | |

Notes:

- **The game never learns which backend runs.** No call names it: a game that
  branched on it would hash differently on different machines. Ratios and jitter
  are part of the ABI for the same reason (backends' own ratios differ slightly).
- **Validation** is the same everywhere and in the same order (handle kinds,
  sizes: inputs at `render_size`, output at `output`; formats; usages), added to
  `GfxModel` and `gfx.rs` with cases in `tests/gfx-cases.json`, so the null GPU
  traps exactly like a real one.
- `reset` marks a camera cut (history dropped). `motionScale` converts the
  game's motion vectors to pixels at render size; `sharpness` is a hint (backends
  without one ignore it).

## Backends and detection

`gasm-run --upscaler auto|portable|metalfx|dlss|off` (default `auto`); the
player has `portable` and `off`. `auto` picks the first available of: DLSS,
MetalFX, portable. `off` is a plain bilinear resample (the output is still
written, so games behave the same).

| backend | where | shipped | how it's found |
|---|---|---|---|
| portable | every runner (natively, the browser's WebGPU, Worker mode) | yes: our own WGSL port of FSR 2 (MIT) | always |
| MetalFX | macOS 13+ native | part of the OS | wgpu on Metal, `MTLFXTemporalScaler` supported by the device |
| DLSS | Windows, Linux native; NVIDIA RTX | **no** | see below |
| XeSS | Windows native | no (same model as DLSS) | maybe, later |

**DLSS without shipping it.** The runtime is a separate library
(`nvngx_dlss.dll`, `libnvidia-ngx-dlss.so`) that games normally ship themselves;
the driver brings the NGX core. The runner would:

1. need an NVIDIA adapter (`AdapterInfo.vendor == 0x10de`) and wgpu on Vulkan or
   D3D12;
2. look for the runtime only in places the **player** chose: `--dlss <path>`, the
   runner's config, or a known system location if drivers or the NVIDIA app keep
   one there (to verify per platform). Never next to a game or in its assets:
   that would let a game bring native code into the runner;
3. on Windows, load it only if it carries NVIDIA's signature; load it at run time
   (never linked, as ANGLE is), initialise NGX and ask whether super sampling is
   available on this GPU and driver; any failure falls back to the next backend
   with one log line;
4. reach wgpu's Vulkan/D3D12 objects through its HAL (`as_hal`) to hand DLSS the
   textures and command buffer. Bevy's `dlss_wgpu` crate does this on Vulkan
   and is the first thing to read (its current state and requirements need
   checking).

Loading a native library the player names is a new trust boundary for
`gasm-run` (today it loads only ANGLE, which ships with it): it stays opt-in
through the player's own settings, never through a game or its manifest.

## Determinism and hashing

- Hashes cover the `dispatch` call (its description and handles), never the
  upscaled pixels, as `gasm:gfx` hashes writes, not rendered frames. Different
  backends give different pixels on screen and the same hashes.
- Headless runs use the null GPU (validation only). `--screenshot` uses
  `portable`, so screenshots compare between machines (`--upscaler` overrides it).
- Nothing the guest can read depends on the backend: `render_size` and `jitter`
  are fixed by the ABI, and `gasm:gfx` has no readback of textures.

## Licensing (to verify before any code)

- FSR 2 / FidelityFX: MIT, so the port can ship in both runners and the npm
  package. Keep its notice in `THIRD-PARTY.txt`.
- DLSS SDK (or Streamline): building against NVIDIA's headers and NGX library
  binds the runner to NVIDIA's SDK terms even if the runtime isn't shipped;
  check that the terms allow an open source runner and runtime-optional use,
  and what attribution they ask for.
- XeSS: Intel's SDK license, same questions.
- MetalFX: a system framework, nothing to ship.

## Phases

0. **Render targets in `gasm:gfx`** (their own design, already on the roadmap).
1. **The ABI and the portable backend**, both runners, the Rust SDK, the null GPU
   rules and gfx cases, the test guest, docs. Usable on every runner by itself.
2. **MetalFX** natively on macOS: nothing to fetch, a check that wgpu's Metal HAL
   gives what MetalFX needs.
3. **DLSS**, bring your own runtime, Windows and Linux on Vulkan (D3D12 later),
   after the license check.
4. Maybe XeSS, if someone asks.

## Open questions

- Is the portable backend worth having without a game that uses it? Phase 1 only
  makes sense with a real `gasm:gfx` 3D game in sight.
- FSR 2 or FSR 3.1's upscaler for the port (3.1 is newer, larger; both are
  compute-shader heavy: check the WebGPU limits in browsers).
- Exposure: the game passes it, or the backends' auto exposure?
- HDR inputs, and reactive/transparency masks (particles, UI drawn into colour):
  later additions to `dispatch` or in from the start?
