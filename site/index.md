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
  - title: 3D via WebGPU
    details: gasm:gfx is a compact WebGPU subset with WGSL shaders. Browsers forward it almost 1:1; natively it runs on Metal, Vulkan and D3D12 through wgpu.
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
| **Games** | **Sumo**: 3D two-player arena with lockstep netcode. **NES**: a complete emulator (tetanes-core) passing the blargg CPU, timing and APU tests. **Triangle** and **test pattern**: minimal examples in Rust and C. |
| **Tools** | `gasm-relay` (WebSocket rooms), the `gasm` Rust crate, `gasm.h` for C, cross-runner determinism and network test suites. |

## Performance

Measured on an Apple M1 Pro. The NES emulator compiled to wasm runs at
**1.5×** (wasmtime AOT) and **1.6×** (V8) the time of the same Rust code
built natively. That's about 9× faster than the console itself. Sumo holds
60 fps with 4× MSAA on both runners.

## Status

gasm is a **proof of concept**. ABI v0 is experimental and will change. See the
[roadmap](/docs/abi#roadmap-not-in-v0): rollback netplay, storage, textures,
and a wasm2c runner for consoles.
