---
layout: home

hero:
  name: gasm
  text: Write a game once. Run it everywhere.
  tagline: Games compile to one WebAssembly file. Small native and browser runners give them a stable interface for graphics, sound, input and networking.
  image:
    src: /favicon.svg
    alt: gasm
  actions:
    - theme: brand
      text: Play the demos
      link: /demos/
    - theme: alt
      text: Get started
      link: /guide/
    - theme: alt
      text: How it works
      link: /docs/how-it-works

features:
  - title: One artifact
    details: A game is a single .wasm file. The same bytes run in a native window (wasmtime + wgpu), in any WebGPU browser, and headless in Node for CI.
  - title: A small, stable ABI
    details: A small core (video, audio, input, assets, params), plus optional GPU, network and storage modules. A new runner is weeks of work, not years.
  - title: 3D via WebGPU or OpenGL ES
    details: gasm:gfx is a compact WebGPU subset with WGSL shaders; natively it runs on Metal, Vulkan and D3D12 through wgpu. gasm:gl is OpenGL ES 3.0 with WebGL 2's rules for existing GL code, on WebGL 2 in browsers and ANGLE natively.
  - title: Online multiplayer
    details: gasm:net gives every runner WebSocket-style messaging. The sumo demo plays lockstep matches between native and browser players through a tiny relay.
  - title: Bit-exact determinism
    details: The same inputs produce the same frames, sounds and game state on every engine, and CI checks it with hashes. That's the basis for replays, tests and netplay.
  - title: Sandboxed by design
    details: Games only see their own memory and the ABI. No filesystem, network only with the player's opt-in, and every pointer and handle is checked.
---

## What's inside

| | |
|---|---|
| **Runners** | `gasm-run`: native (Rust: wasmtime, wgpu, winit, cpal, gilrs), with AOT compilation for no-JIT platforms. Browser runner: WebAssembly, WebGPU, AudioWorklet, Gamepad API. Headless Node runner. |
| **Games** | **Sumo**: 3D two-player arena with lockstep netcode. **NES**: a complete emulator (tetanes-core) passing the blargg CPU, timing and APU tests. **DOOM**: doomgeneric in C, with OPL music and saves, playing any IWAD. **ScummVM**: point-and-click adventures, starting with the freeware Beneath a Steel Sky. **SDL 3**: SDL programs build unchanged, threads included; SDL's own snake and woodeneye demos. **GLES 3**: OpenGL ES code through drop-in headers. **Triangle** and **test pattern**: minimal examples in Rust and C. |
| **Tools** | `gasm-relay` (WebSocket rooms), the `gasm-sdk` Rust crate (library `gasm`), `gasm.h` and the C/C++ SDK, SDL 3 for gasm, cross-runner determinism and network test suites with golden hashes. |

## Screenshots

<div class="shots">
  <figure><img src="/screenshots/doom.webp" alt="DOOM in gasm-run" loading="lazy"><figcaption>DOOM (doomgeneric in C) with OPL music, at 4:3 with the sharp filter.</figcaption></figure>
  <figure><img src="/screenshots/sky.webp" alt="Beneath a Steel Sky in ScummVM" loading="lazy"><figcaption>ScummVM playing Beneath a Steel Sky, upscaled with xBR.</figcaption></figure>
  <figure><img src="/screenshots/sumo.webp" alt="Sumo, the 3D demo" loading="lazy"><figcaption>Sumo: gasm:gfx (WebGPU) with lockstep netplay.</figcaption></figure>
  <figure><img src="/screenshots/textured.webp" alt="GPU texture test" loading="lazy"><figcaption>Mipmapped textures, samplers, instancing and dynamic offsets on gasm:gfx.</figcaption></figure>
  <figure><img src="/screenshots/gltest.webp" alt="GLES 3 test in the browser player" loading="lazy"><figcaption>OpenGL ES 3 C code on gasm:gl, in the browser player (WebGL 2).</figcaption></figure>
  <figure><img src="/screenshots/woodeneye.webp" alt="SDL 3 woodeneye-008" loading="lazy"><figcaption>SDL 3's woodeneye-008, source unchanged, on SDL for gasm.</figcaption></figure>
</div>

<figure class="shot-wide"><img src="/screenshots/filters.webp" alt="The same DOOM frame with the sharp, xbr, fsr and crt filters" loading="lazy"><figcaption>One 2D frame, four ways to scale it up: sharp, xBR, FSR and CRT (the player's filter menu, <code>gasm-run --filter</code>).</figcaption></figure>

## Performance

Measured on an Apple M1 Pro. The NES emulator compiled to wasm runs at
**1.5×** (wasmtime AOT) and **1.6×** (V8) the time of the same Rust code
built natively. That's about 9× faster than the console itself. Sumo holds
60 fps with 4× MSAA on both runners.

## Status

gasm is a **proof of concept**. ABI v0 is experimental and will change. See the
[roadmap](/docs/roadmap): `gasm:gl` natively, rollback netplay, `.gasm`
packages.
