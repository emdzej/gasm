# Presentation: upscaling filters, display aspect, window title

> **Status: implemented** (all phases; see "Plan" for what each became and what
> was left out): the filters (`sharp` by default, `nearest`, `xbr`, `fsr`,
> `crt`) and integer scaling natively and in the browser,
> `--screenshot-filtered`, `scripts/present-test.mjs` with golden images,
> `gasm.set_title`, the `gasm.title` section and `gasm.video_set_aspect`. The ABI details land in `spec/abi.json` and `spec/ABI.md` when that
> work starts.

## Summary

Three changes to how runners show a game, none of which touch the simulation:

1. **Upscaling filters** for 2D guests (`video_present`). Runners stop showing
   frames only as nearest-neighbour pixels and offer edge-directed and spatial
   upscalers that take any small frame to any output size. Runner-only, no ABI
   change.
2. **Display aspect ratio** (`gasm.video_set_aspect`). Lets a guest say that its
   frame isn't meant to be shown with square pixels (320×200 DOS games are 4:3).
   ABI addition.
3. **Window title** (`gasm.set_title`). Lets a guest name its window or tab,
   including at runtime (ScummVM shows the launcher first, then the game's
   name). ABI addition.

Both ABI changes are additions under the existing versioning rule:
`GASM_ABI_VERSION` stays 0, older runners link the new imports as traps, and
guests check for them before calling (see "Feature detection").

## Why

- Many guests render small frames: NES 256×240, DOOM 320×200, ScummVM
  320×200/640×480, Game Boy-class 160×144. On a 1440p or 4K screen,
  nearest-neighbour at a non-integer factor gives uneven pixels and shimmering
  when things scroll. Even at integer factors, many people prefer a smoothed or
  CRT-style look.
- 320×200 games drawn with square pixels look about 17% too flat. Today both
  letterboxes (`frame_position` in `runners/native/src/host.rs`,
  `framePosition` in `runners/web/lib/input.js`) assume square pixels, and a
  guest can't say otherwise. So the guests correct it themselves, at a cost:
  DOOM scales its 320×200 screen to 640×480 before `video_present` (1.2 MB per
  frame to copy and hash instead of 256 KB), and the ScummVM backend stretches
  320×200 games to 320×240 (`aspectHeight` in `gasm-graphics.cpp`).
- The native window is always titled `gasm — <file> — <n> fps`, and the browser
  tab is always `gasm — web runner`. SDL (`SDL_SetWindowTitle`), ScummVM
  (`OSystem::setWindowCaption`) and most ported games already set a title and
  currently have nowhere to send it.

## Goals

1. Any input size to any output size: integer and non-integer factors,
   resolution changes mid-game, downscaling when the frame is bigger than the
   output.
2. The same filters with the same results natively (wgpu) and in the browser
   (WebGL 2), selectable by the user.
3. Determinism, hashes and headless runs are untouched: filters work only on
   what is displayed.
4. The new ABI functions are safe: validated pointers and strings, and no way
   for a guest to pass itself off as the runner's own UI.

## Non-goals

- Filters for `gasm:gfx` guests. They already render at drawable size.
- Neural upscalers. Model weights don't fit a dependency-free host, and the
  shaders below cover the content in question.
- GPL shaders (xBRZ, crt-royale). Runners and `@emdzej/gasm-host` are MIT.
- Guests choosing the filter. It's a user preference. A content hint ("pixel
  art" vs "rendered") might come later; see "Open questions".

## 1. Upscaling filters

### Filters

| Name | Algorithm | Best for | License |
|---|---|---|---|
| `nearest` | the behaviour before this work | exact pixel doubling | — |
| `sharp` | integer prescale (nearest) + bilinear to the exact size | everything; **the default** | own code |
| `xbr` | xBR-lv2 (Hyllian's algorithm, own implementation) | NES, ScummVM, sprite art | MIT |
| `fsr` | AMD FSR 1: EASU + RCAS (own implementation) | DOOM, dithered or 3D-rendered content | MIT |
| `crt` | own design: scanline beams + aperture grille | the "authentic" look | own code |
| `scalefx` | ScaleFX (multi-pass xBR refinement) | pixel art, highest quality | not done (see Plan) |
| `mmpx` | MMPX (McGuire & Gagiu, 2021), 2× per pass | pixel art | not done (see Plan) |

### Pass chain

The runner builds a chain from the source size, the letterbox size (which
includes the display aspect, section 2) and the filter:

```
source w×h
  → k × 2× passes of the edge-directed filter    while 2^(k+1)·src still fits the target
  → one final resample to the exact letterbox    (sharp bilinear, Lanczos-2 or FSR EASU)
  → optional sharpening (RCAS) / CRT pass
```

- Single-pass filters (`xbr` any-scale, `fsr` up to ~4×) skip the 2× stages
  when the factor allows.
- Factor < 1.5: edge-directed stages are skipped and `sharp`/`fsr` take over.
- Factor < 1 (frame bigger than the output): linear or area filtering for every
  filter, because nearest-neighbour aliases when shrinking.
- `--integer-scale` (works with any filter): the target becomes the largest
  integer multiple that fits; the rest is black border. The pointer's frame
  position follows it (`frame_position` takes the flag; the browser's
  `framePosition` needs the same when it gets `?integer`). Headless runs
  never use it, so scripted pointer input keeps its hashes.

The chain and its intermediate textures are rebuilt when the frame size, window
size or display aspect changes, not just at start-up. They are a handful of
small textures, so rebuilding costs microseconds.

### Native

`runners/native/src/present.rs` has the letterbox (`letterbox`, shared with
the pointer's `frame_position`), the filters and the `Presenter`:
`Gfx::present_video` and `Gfx::render_video` (filtered screenshots) upload the
frame and draw one fullscreen triangle into the letterboxed viewport with the
filter's pipeline. The shaders are WGSL in the same file. `sharp` and `xbr` are
single passes, so there are no intermediate textures yet; they arrive with
`fsr` (EASU then RCAS). The source texture is recreated when the frame size
changes, and the sizes go to a uniform every frame, so window and frame
resizes need no other handling.

### Browser

The player draws 2D frames with canvas 2D `putImageData` on a canvas sized to
the frame, scaled up by CSS with `image-rendering: pixelated` (`present` in
`app.js`). With a filter other than `nearest`, the canvas is sized to the
display in device pixels (`canvasSize()` already computes this for Worker mode)
and a WebGL 2 presenter draws into it:

- WebGL 2 rather than WebGPU: more browsers support it and the presenter is a
  few fragment shaders. In Worker mode 2D frames already come back to the page,
  which presents them, so nothing changes there.
- Except on a canvas that already has a WebGPU context (a `gasm:gfx` guest that
  also calls `video_present`): a canvas has one context type, so there the
  WebGPU blit (`WebGpuGfx.presentVideo`) stays, nearest only, unless the filters
  get a WGSL path in the browser too.
- The presenter is a separate optional module in the package
  (`gasm-present.js`), so `gasm-host.js` stays dependency-free and pages that
  only want raw frames don't load it.
- The GLSL shaders are ports of the same WGSL shaders. A test compares the two
  (below).
- Fallback: without WebGL 2, the canvas 2D path with `nearest`.
- As built: `nearest` without integer scaling keeps the canvas 2D path;
  anything else uses `GlPresenter` on a canvas sized by `canvasSize()`.
  Switching between the two at runtime replaces the canvas (one context type
  per canvas) and redraws the frame on screen. The page sets
  `BrowserInput.integerScale`, which travels with the pointer to the host (and
  to the worker), so `framePosition` matches what is shown.
- Exact parity: both runners compute the source position from the fragment's
  framebuffer position (an interpolated coordinate rounds differently per GPU,
  and at factors like 3.125 pixel centres land exactly on texel edges), add a
  1/1024 texel bias so such ties go the same way, read `nearest`/`xbr` texels
  with `textureLoad`/`texelFetch`, and use whole-pixel viewports (WebGL has no
  fractional ones).

### Configuration

- Native: `--filter <name>`, `--integer-scale`.
- Browser: `?filter=` and `?integer`, plus a player menu entry stored in
  `localStorage`.
- Default: `sharp` (phase 6; `nearest` until then). At whole factors it is
  pixel-identical to `nearest`, so only non-integer scaling looks different.

### Determinism and headless

Filters are applied after hashing. `video_fnv32` keeps hashing the guest's
`video_present` bytes. Headless `--screenshot` keeps saving the raw frame;
`--screenshot-filtered` renders through the presenter at `--window` size so
filters can be checked visually and by tests.

## 2. Display aspect: `gasm.video_set_aspect`

```
gasm.video_set_aspect(num: u32, den: u32)
```

- The display aspect ratio (width : height) at which the runner shows the
  `video_present` frame, e.g. `(4, 3)` for 320×200 DOS games. It holds until the
  next call. `(0, 0)` resets to square pixels (the default).
- Validation: both zero, or both in `1..=65535` with `1/8 ≤ num/den ≤ 8`;
  anything else traps.
- It affects only display and pointer mapping. `video_present` sizes, hashes and
  `gasm:gfx` are unchanged.
- Pointer mapping: `frame_position` / `framePosition` take the aspect into
  account (the letterbox is `num:den` instead of `w:h`). The formula stays
  identical in `runners/native/src/host.rs` and `runners/web/lib/input.js`
  (scripted input only produces drawable positions; the hosts derive the frame
  position), computed in f64 and rounded to f32 once, as now.
- Guests: DOOM presents its 320×200 screen as is and sets 4:3, instead of
  scaling to 640×480 itself (a quarter of the bytes per frame to copy and
  hash). The ScummVM backend presents 320×200 and sets 4:3 when aspect-ratio
  correction is on (ScummVM's own `aspect_ratio` setting), instead of
  stretching to 320×240. Both change their hashes once (golden update). SDL
  maps it from the private video driver when an app requests a logical
  presentation with a different aspect.

## 3. Window title: `gasm.set_title`

```
gasm.set_title(title: str)
```

- UTF-8. Invalid UTF-8 traps, as for every string argument (ABI.md,
  Conventions). Longer than 256 bytes is cut at the last character boundary
  within the limit (not a trap: titles are cosmetic). Control characters and
  Unicode bidi overrides (U+202A–U+202E, U+2066–U+2069) are removed. An empty
  string resets to the default.
- The runner shows it in a fixed frame the guest can't remove:
  - native window: `<title> — gasm — <n> fps` (today: `gasm — <file> — <n> fps`;
    the fps counter could move behind a flag later);
  - browser: the host fires an `onTitle(title)` callback; the player sets
    `document.title` to `<title> — gasm`. Pages embedding the host do whatever
    they like with it. Worker mode forwards it as a message.
  - headless: logged once per change (`title: …`), not hashed.
- Applied at most once per frame, and only when it changed (a guest setting
  it every frame costs nothing).
- Default title: the module file name without `.wasm`.
- Guests: SDL's `SetWindowTitle` in the gasm video driver, ScummVM's
  `setWindowCaption`, the DOOM platform layer (`DOOM`, `Freedoom: Phase 1`, …),
  and a `gasm::set_title` wrapper in the Rust SDK.

### Static title

A launcher or the site's game list wants the name before the game runs: the
`gasm.title` custom section (UTF-8, cleaned like `set_title`) is read at load
time and is the default title until the guest calls `set_title`. Done in
phase 6.

## Feature detection

Older runners trap on the new imports, so guests that should keep running there
need to detect support. `gasm.has(name)` already reports modules and functions
(`has("gasm.set_title")` is 1 on runners that implement it, 0 on runners that
have `has` but not `set_title`). Runners from before `has` itself (0.5.0 and
earlier) trap on `has`, so the check is safe only from the release that added
it on: the SDK wrappers do the check once and cache it, and document that
release as the oldest runner they support. `gasm::set_title` and
`gasm_set_title` are then no-ops on runners without the import.

## Testing

- **Determinism suite:** unchanged hashes for every case, with every filter
  selected (filters must not leak into hashes).
- **Filter parity** (`scripts/present-test.mjs`, done): `--screenshot-filtered`
  for each filter (and `nearest` with integer scaling) on the test pattern and
  NES, at 768×720, 1000×750, 1280×720 and 200×150, against `GlPresenter` in
  Chrome on the same raw frame. Tolerance ±3 per channel on 0.2% of pixels;
  measured on an M1 Pro (Metal on both sides): all 32 cases identical. Skips
  without a GPU, so it isn't in CI yet (needs a software Vulkan such as
  lavapipe). Golden PNGs per runner are still to do.
- **Resize:** a test-pattern mode that changes frame size every N frames; no
  validation errors, and the chain is rebuilt (counted in a log line).
- **Aspect:** scripted pointer input against a 4:3 320×200 frame gives the same
  `fx, fy` on all runners; invalid ratios trap.
- **Title:** headless log lines; UTF-8 truncation, control and bidi stripping;
  `has("gasm.set_title")` on old and new runners.
- **Browser smoke:** `web-smoke.mjs` with `?filter=xbr` and `?filter=fsr` on
  NES and DOOM; look at the PNG.

## Plan

1. **Native presenter** (done). `sharp` and `xbr`, `--filter`,
   `--integer-scale`, `--screenshot-filtered`. Multi-pass chains wait for `fsr`.
2. **Browser presenter** (done). `gasm-present.js` (WebGL 2), the same
   filters, player controls, Worker mode (frames come back to the page), the
   parity test.
3. **`fsr`** on both runners; goldens for all filters (done). EASU renders
   into a viewport-sized intermediate texture, RCAS (sharpness 0.2 stops, FSR's
   default) from it into the output: the only two-pass filter. EASU runs at any
   factor above 1 in one pass rather than in 2× steps; it doesn't need them.
   Goldens are the test pattern at 1000×750 for every filter
   (`tests/golden/present/`, our own content, so no ROM imagery in the repo).
4. **ABI: `set_title`** (done). `abi.json` → regenerate, `ABI.md`,
   `CHANGELOG.md`, the `gasm` crate, both runners, SDL driver, ScummVM backend,
   DOOM platform layer, site docs. The C helper is `gasm_set_title_str`
   (cached `has` probe). DOOM shows "DOOM Shareware", ScummVM the game's name.
5. **ABI: `video_set_aspect`** (done). Same list, plus the pointer-mapping
   formula; DOOM and ScummVM set 4:3 (and keep scaling themselves on older
   runners). The formula was first written as display width = fh·n/d, which
   made 320×200 at 4:3 scale by 3.0000000000000004 and moved scripted clicks by
   a pixel; it now multiplies whole numbers before dividing. Integer scaling
   floors the vertical scale. The browser player's canvas box takes the frame's
   display aspect (`--ar`), which also fixed stretching in tall windows. SDL
   needs nothing: SDL apps render at window size and letterbox in SDL's own
   renderer (`SDL_SetRenderLogicalPresentation`). Tests: `inputtest-aspect`
   and `aspect-trap` in the determinism suite, 16:9 cases in the parity test.
6. Optional (done, two items declined):
   - `crt`: an own design (no license questions): the two nearest lines as
     beams whose height grows with brightness, sharp bilinear horizontally,
     an RGB aperture grille one output pixel per stripe, in linear light.
     Needs 2 output rows per line, otherwise `sharp`.
   - `sharp` is the default. Checked first: at whole factors it now gives
     exactly `nearest` (the flat region absorbs the 1/1024 tie bias; before
     that fix pixels differed by up to 2/255), so integer-scaled games look
     the same, and other factors lose the uneven pixels.
   - Static title: custom section `gasm.title` (Rust `gasm::title!`, C
     `GASM_TITLE`, which emits the section with top-level assembly: clang's
     `section` attribute doesn't make custom sections). The default title in
     both runners, logged headless. `wasm-opt` keeps it.
   - Not done: `mmpx` and `scalefx`. Both cover what `xbr` already does
     (pixel art edges); MMPX's rule tables and ScaleFX's five passes would have
     to be ported from their reference sources, which this work didn't have
     at hand, and ScaleFX's license still needs checking. Worth revisiting if
     `xbr` falls short on some content.

Phases 1–3 need no ABI change and can ship on their own. Phases 4 and 5 are
independent of each other.

## Risks and open questions

- **WGSL vs GLSL drift.** Two copies of each shader. Mitigation: the golden
  comparison above. Alternative: write WGSL only and translate with naga at
  build time (naga's GLSL backend is good enough for fragment shaders).
- **ScaleFX and CRT licenses.** The libretro shader repository mixes licenses;
  check each shader before porting.
- **Content hint.** Should a guest be able to suggest a filter class
  (`pixel art` / `rendered`) so the player picks a sensible default? It would
  be one more small ABI addition; left out until there's a real need.
- **Title spoofing.** The fixed `— gasm` suffix and bidi stripping keep a guest
  from impersonating the runner or the browser's UI; the 256-byte limit keeps
  titles readable. Browsers already allow pages to set arbitrary titles, so this
  is about clarity more than security.
