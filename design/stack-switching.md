# Stack switching: `gasm_run` and `yield_frame`

> **Status: implemented** (all four phases; see "As built" at the end). The ABI
> is in `spec/abi.json` and `spec/ABI.md` ("Stack switching").

## Summary

Games with their own main loop (ScummVM, SDL 3 classic `main()`, anything
built on `gasm_loop.h` or `gasm::main_loop!`) are suspended between frames by
Binaryen's Asyncify inside the module today. This adds a second way: the guest
exports `gasm_run()` and calls the import `gasm.yield_frame()` once per frame,
and **the runner** suspends the guest's wasm stack (wasmtime's async fibers
natively, JSPI in browsers and Node). Modules built for it skip the Asyncify
pass: smaller and faster, and none of Asyncify's build rules apply.

`gasm_frame` stays the default model and the fallback: the SDK loop helpers
build both entry points from the same source, and runners without stack
switching keep running the Asyncify build.

## Why

- **Size and speed.** ScummVM is 16.1 MB with Asyncify and 10.6 MB without;
  Asyncify also slows every instrumented function (it checks its state around
  calls that might unwind).
- **Fragility.** The Asyncify rules listed in `gasm_loop.c` and `AGENTS.md`
  (yield through an indirect call, `--asyncify` before `-O2`, the remove list,
  nothing instrumented between unwind and stop, wrapping `exit`) were each a
  bug once.
- **Threads.** Cooperative threads ([threads.md](threads.md)) need the same
  suspension; with runner-side switching they need no Asyncify either.

## The ABI

| | Name | Signature | Semantics |
|---|---|---|---|
| export (optional) | `gasm_run` | `() -> i32` | The game's whole run: the runner calls it on the first frame, after `gasm_init`. Returning ends the game with that exit code (like `proc_exit`). |
| import | `gasm.yield_frame` | `()` | Ends the current frame: the runner suspends the guest and resumes it at the start of the next frame (input sampled, time advanced, as before every `gasm_frame`). Only valid inside `gasm_run`; anywhere else it traps. |

- The import is `yield_frame`, not `wait_frame`: its C binding (`gasm_yield_frame`)
  would otherwise clash with the loop helper's `gasm_wait_frame`.
- A module exporting `gasm_run` is run that way by runners that support it
  (`has("gasm.yield_frame")` tells a guest). Otherwise runners call
  `gasm_frame`, which every module still exports.
- Frames are the same in both models: frame N is everything between the N-th
  resume and the next `yield_frame` (frame 0 starts `gasm_run`). Input, time,
  `begin_frame`'s return value, catch-up and hashing are unchanged, so the
  same game gives **the same hashes** either way.
- `gasm_exit` (the player quits) is called while `gasm_run` is suspended, on
  top of the suspended stack; afterwards the runner abandons the suspended
  call. Saves made there work as before.
- Watchdog (`--call-timeout`): each frame segment gets the time limit, not
  `gasm_run` as a whole.
- `yield_frame` from `gasm_init`, `gasm_exit`, `gasm_frame` or a nested call
  traps.

## SDKs

The loop helpers keep their API (`gasm_main` + `gasm_wait_frame`,
`gasm::main_loop!`). They export both `gasm_frame` (the Asyncify path) and
`gasm_run`; `gasm_wait_frame` calls the import when the game runs under
`gasm_run`, and unwinds otherwise. What differs is only the post-processing:

- **Asyncify build** (`game.wasm`, as today): `wasm-opt --asyncify …`. Runs
  everywhere, with either entry point.
- **Run build** (`game-run.wasm`): no `--asyncify`. Needs a runner with stack
  switching. Its `gasm_frame` can't suspend (the `asyncify.*` imports stay
  unresolved), so runners recognise such a module (it exports `gasm_run` and
  imports from `asyncify`) and refuse it with a clear message when they can't
  switch stacks.

## Runners

### Native (wasmtime)

wasmtime's async support runs a call on a fiber that suspends when an async
host function returns `Pending`. It is a per-engine setting, and a call keeps
`&mut Store` for as long as it runs, so:

- **Two engines**: the existing one, and one with `async_support` for modules
  that export `gasm_run` (found by reading the export section before
  compiling). `.cwasm` files are compiled for the matching engine.
- **The store moves into the running call.** `Game` owns a pinned future of
  `gasm_run` and polls it once per frame with a no-op waker: the guest runs
  until `yield_frame` yields, then the poll returns. Everything happens on the
  runner's thread (the GPU surface stays where it is).
- **Host access between frames** goes through `Game::with_host(|host| …)`: the
  closure is handed to the suspended `yield_frame`, which runs it with the
  store's `Host` and yields again, all within one poll. For `gasm_frame`
  guests it just calls the closure. Runners use `with_host` instead of
  `host_mut()`.

### JS (JSPI)

`yield_frame` is a `WebAssembly.Suspending` import and `gasm_run` is wrapped
with `WebAssembly.promising`. Resuming a suspended guest happens on a
microtask, so frames of such guests are asynchronous: `GasmHost` gets
`frameAsync()` / `runFramesAsync()`, `frame()` throws for them. The player and
the Worker already have an asynchronous loop (one batch in flight);
`headless.mjs` awaits each frame. JSPI is in Chromium and Node 24 (CI moves
from Node 22 to 24); elsewhere the player loads the Asyncify build.

## Testing

- Determinism: every own-loop case (`loopdemo-rust`, `loopdemo-c`,
  `sdl3-classic`, ScummVM's three games) also runs from its run build, natively
  (JIT and AOT) and in Node: **same golden hashes** as the Asyncify build.
- Traps: `yield_frame` outside `gasm_run`; a run build on a runner without
  switching gives the clear refusal.
- Sizes and speed measured and recorded (ScummVM, SDL classic).
- Browser: the player picks the run build when JSPI is there.

## Plan

1. **ABI + SDKs.** `abi.json` (export `gasm_run`, import `yield_frame`),
   `ABI.md`, `gasm_loop.c` and `main_loop!` with both entry points, run builds
   in the Makefile for the own-loop games.
2. **Native runner.** Second engine, the polled future, `with_host`, runners
   switched over, watchdog per segment, `.cwasm`.
3. **JS runner.** JSPI in `GasmHost`, `headless.mjs`, Worker mode, the player
   (picks the build); CI on Node 24.
4. **Tests, measurements, docs.** Determinism cases for the run builds, sizes,
   the site docs, CHANGELOG, roadmap.

## Risks

- **Stack depth.** wasmtime's async fibers have a fixed stack
  (`async_stack_size`); deep guest recursion that worked synchronously could
  overflow. Set it generously (ScummVM) and test.
- **JSPI coverage.** Browsers without JSPI need the Asyncify build, so games
  ship both for now.
- **Embedders.** `Game::host_mut()` can't work while a `gasm_run` guest is
  suspended; it stays for `gasm_frame` guests and panics with a pointer to
  `with_host` otherwise.

## As built

- **One engine, not two.** wasmtime 49 always supports async calls
  (`Config::async_support` is deprecated and does nothing), so sync
  (`gasm_frame`) and async (`gasm_run`) guests share the engine and `.cwasm`
  files need no special handling. `async_stack_size` is 4 MiB.
- **`Host` became `Send`** (async calls require it): `AudioOut: Send`, the asset
  file cache uses `Arc`, and `AudioSink::open()` returns the cpal stream (not
  `Send` on any platform) separately; the window runner keeps it.
- **`with_host`** lends a closure to the suspended `yield_frame` for one poll
  (a pointer with an erased lifetime, used only inside that poll, on the
  runner's thread). `host()`/`host_mut()` panic while a run is suspended.
- **Runners prefer `gasm_run`**, so the Asyncify builds also run through it
  wherever stacks can switch; `--no-stack-switching` (both headless runners)
  keeps the Asyncify path tested.
- **Results.** The determinism suite runs each own-loop case (loopdemo Rust and
  C, SDL 3 classic, ScummVM's five games) in 8 variants: Asyncify build
  natively (JIT, AOT, without switching) and in Node (with and without
  switching), run build natively (JIT, AOT) and in Node 24. All give the golden
  hashes. Sizes: ScummVM 16.1 → 10.6 MB, SDL 3 classic 1.15 → 0.81 MB,
  loopdemo 27.9 → 18.2 KB. ScummVM's run build loads with 506 MB peak RSS
  instead of 904 MB. Frame rates in the scripted headless runs were the same
  within noise (about 9,000 frames/s natively for Beneath a Steel Sky, which
  idles most of the time, so it doesn't show Asyncify's per-call cost).
- **Browser.** Chromium (154 here) runs the run builds on the main thread and
  in Worker mode; `?hashframes` gives the Node hashes.
