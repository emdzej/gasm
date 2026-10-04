# Threads for guests

Status: **part A implemented** for C, POSIX threads, SDL 3 and Rust (see "As
built" and "Plan"); part B is a proposal. Two designs that
complement each other:
**cooperative threads** inside the guest (no ABI change, deterministic), and
**real wasm threads** as an optional capability later (parallel, not
deterministic).

## Summary

gasm guests have one thread today. Code that creates threads (SDL's
`SDL_CreateThread` and `SDL_AddTimer`, `pthread_create`, engines with loader or
audio threads) fails at that call. Most game code uses threads for structure,
not for speed: a loader that waits for requests, an audio decoder that waits
for buffer space, a timer that waits for a deadline. Cooperative threads cover
those cases on every runner without giving up determinism. Real parallelism
(physics or job systems on several cores) needs shared-memory wasm threads,
which cost determinism and complicate hosting, so they come second and opt-in.

## Why

- **SDL 3 for gasm** (`sdk/sdl3`) has no threads: `SDL_CreateThread` and
  `SDL_AddTimer` fail. Async I/O had to be rewritten as synchronous.
- **Ports**: C/C++ code written for pthreads (loaders, decoders, game logic on
  a second thread) can't be built as is.
- **Engines like Godot** can build single-threaded (`threads=no`, like their
  web export), but run better with a worker pool.

## A. Cooperative threads (green threads)

### Model

Every thread runs on the guest's single wasm thread. A thread runs until it
**blocks** (mutex already locked, condition wait, semaphore wait, sleep,
`sched_yield`, joining a thread that hasn't finished) or until the frame ends;
then the scheduler picks the next ready thread. Threads switch only at those
points, so there are no data races in the C sense, and lock-free code that
spins without blocking never yields (see Limits).

Scheduling is deterministic: ready threads run in creation order (round robin),
sleeps wake on virtual time, and the frame boundary is part of the schedule.
The same input gives the same interleaving on every runner, so hashes still
match.

### Mechanism

The same one as the loop helper (`sdk/c/src/gasm_loop.c`): Binaryen's Asyncify,
driven inside the guest.

- Each thread has an **Asyncify buffer** (its suspended wasm locals and call
  stack) and its own **C stack** region in linear memory; a switch saves and
  restores `__stack_pointer` (wasi-libc's shadow stack) along with the Asyncify
  state. Emscripten's fibers (`emscripten_fiber_swap`) are built this way.
- A switch is: unwind the current thread to the scheduler loop, then rewind the
  next one. The scheduler loop runs inside `gasm_frame` (or the loop helper's
  frame), on the remove list like `gasm_loop_frame`.
- **Thread-local storage needs care.** Without the `atomics` feature (the
  default `wasm32-wasip1` target), clang turns `_Thread_local` variables into
  plain globals, and wasi-libc has no TLS at all: `errno` is an ordinary global
  (checked: no `__tls_base` in wasi-sdk 34's `wasm32-wasip1/libc.a`). With
  `-matomics -mbulk-memory` on non-shared memory, wasm-ld lays the TLS
  variables out as one block at `__tls_base`, but `__tls_base` is then an
  immutable constant and there is no `__wasm_init_tls` (checked with wasi-sdk
  34), so it can't be swapped. The scheduler therefore **copies**: games that
  use green threads are built with `-matomics -mbulk-memory` (atomics on
  unshared memory, accepted by every runner), and a switch saves the TLS block
  (`__tls_base` … `+ __builtin_wasm_tls_size()`) of the outgoing thread and
  restores the incoming one's. `errno` and the few other libc globals
  (`strtok`, `rand` state) are saved and restored the same way, from a fixed
  list. Cost per switch: a copy of the TLS size (tens of bytes for typical C).
- Stacks and buffers are fixed per thread (sizes at creation, defaults like
  1 MiB stack + 256 KiB Asyncify buffer), allocated with `malloc`.

### API

- **C** (`sdk/c/include/gasm_thread.h`): `gasm_thread_create(fn, arg, stack)`,
  `join`, `yield`, `sleep_ns`, plus mutex, condition, semaphore. Main-callback
  games (no loop helper) can use it too: the scheduler runs the other threads
  after `gasm_frame` returns to it. In such games **the main thread must not
  block** (it is the frame callback, which can't be suspended: it is on the
  Asyncify remove list); blocking there traps with a message. Games whose main
  thread blocks use the loop helper, where the main thread is a suspendable
  thread like the others.
- **pthreads shim** (`sdk/c/src/gasm_pthread.c`): `pthread_create/join/detach`,
  mutexes (normal, recursive), condition variables (with timeouts),
  `pthread_once`, keys, `sched_yield`, `nanosleep`. Built against wasi-libc's
  `wasm32-wasip1` headers, so unchanged pthread code compiles. **Timeouts and
  sleeps use the clock the guest computes deadlines with**: `pthread_cond_timedwait`
  takes an absolute `clock_gettime` time, so the scheduler compares deadlines
  with `clock_time_get` at each switch and frame boundary. Runners make the
  WASI clocks virtual in headless runs (CHANGELOG, after 0.5.0), so schedules
  are reproducible there; in windowed runs they follow real time, like
  everything else.
- **SDL 3**: no SDL patch. SDL's `SDL_THREAD_PRIVATE` is only a CMake config
  flag (nothing in SDL's sources uses it), so the backend is a set of files in
  `sdk/sdl3/src` (`SDL_systhread.c`, mutex, condition, semaphore, TLS on the
  scheduler) compiled instead of `src/thread/generic`, which the Makefile
  leaves out with `SDL_SKIP`, as it does for the other replaced drivers.
  `SDL_AddTimer` then works (its thread waits on the clock above), and the
  synchronous async I/O can stay or move to a thread.
- **Rust**: `gasm::thread` (`spawn`, `Builder`, `JoinHandle`, `yield_now`,
  `sleep`, `wait_frame`) and `gasm::sync` (`Mutex`, `Condvar`, `Semaphore`),
  the same scheduler ported to Rust, driven by `gasm::threaded_main_loop!`.
  `std::thread` on `wasm32-unknown-unknown` can't be redirected, so games use
  these instead; closures needn't be `Send`.

### Interaction with frames

- Input, audio and video stay per frame. Threads that run during a frame see
  that frame's input.
- A frame ends when every thread is blocked or waiting for the next frame (the
  main thread presenting, or the loop helper's `wait_frame`). A thread that
  never blocks would keep the frame from ending: the scheduler can preempt at
  `gasm_wait_frame` only, so a CPU-bound thread should yield (documented).
- Sleeping threads wake at the first switch point or frame boundary after
  their deadline: the resolution is one frame at the guest's frame rate
  (16.7 ms at 60 Hz, 28.6 ms for DOOM's 35 Hz), like `SDL_Delay` today.
- **Deadlock** (every thread blocked, none waiting for the next frame or a
  deadline) traps with a list of the blocked threads, instead of hanging.

### Limits

- **No parallelism**: everything still runs on one core.
- **Spinning**: busy-wait loops on atomics without a blocking call or yield
  hang (the frame never ends). Spinlocks in the shim and SDL yield; foreign
  lock-free code may need a yield added.
- **Cost**: Asyncify instruments everything that can reach a switch point,
  about +50% code size (ScummVM: 10.6 MB without Asyncify, 16.1 MB with it),
  plus per-thread memory for stacks and buffers.
- **Blocking runner calls**: none exist in the ABI, so nothing to worry about;
  a thread that calls `gasm_*` imports just runs them.

### Testing

- Determinism suite: a C guest with several threads (producer/consumer on a
  condition, a recursive mutex, sleeps across frames, `SDL_AddTimer`) whose
  output is hashed per frame; JIT == AOT == V8.
- SDL's own thread tests (`test/testthread.c`, `testsem.c`, `testlock.c`,
  `testtimer.c`) built unchanged.

### Plan

1. Scheduler + C API + determinism case (the hard part: stack pointer and TLS
   switching, Asyncify rules shared with `gasm_loop.c`). **Done.**
2. SDL thread backend; SDL thread and timer tests in CI. **Done**:
   `sdk/sdl3/src/SDL_gasmthread.c` (threads, mutexes, conditions,
   semaphores, read/write locks, TLS) replaces all of `src/thread/generic`.
   `SDL_THREADS_DISABLED` stays defined, because it is what selects the generic
   thread handle type (SDL isn't patched); its other effects are a shared
   error buffer and an unlocked event queue, both safe with cooperative
   threads. SDL's clock catches up with the scheduler's frames in whichever
   thread next waits or reads the time (the scheduler can't run SDL's per-frame
   audio work itself: it may block). Apps link `gasm_loop_threads.o`
   (`gasm_sdl3_app(<target> LOOP THREADS)`). Test: `sdk/sdl3/examples/threads`
   (workers on a mutex, condition and read/write lock across `SDL_Delay`,
   `SDL_AddTimer`'s timer thread posting a semaphore, TLS), in the determinism
   suite and checked in Chrome. SDL's own test programs need SDL_test, which
   isn't part of this build.
3. pthreads shim; port one pthread-using program as the example. **Done**:
   `sdk/c/src/gasm_pthread.c` replaces wasi-libc's single-thread stubs (weak,
   or alone in their archive members) and keeps musl's object layouts, so
   wasi-libc's `pthread_mutex_init`, attribute functions and
   `pthread_cond_init` stay in use. Spinlocks yield instead of spinning, and
   `nanosleep`/`usleep`/`sleep` sleep the thread on the frame's time (WASI's
   `poll_oneoff` isn't available). Not covered: `pthread_exit`, cancellation,
   C11 `<threads.h>` (musl calls internal pthread names there). Example and
   test: `guests/pthreadtest`, plain POSIX code, in the determinism suite.
4. Docs (site dev guide), Rust API. **Done.** Rust can't read or set the wasm
   stack pointer on stable, so `gasm-sdk` ships the two functions as wasm
   assembly (`guests/gasm/sp/gasm_sp.s`), assembled once into a 360-byte
   `libgasm_sp.a` (`scripts/build-rust-sp.sh`) that its `build.rs` links on
   wasm32: games need no assembler. The scheduler state that is read between an
   unwind and `asyncify_stop_unwind` lives in plain statics, and a thread is
   resumed by id (not index), since threads come and go while it is suspended.
   Test: `guests/rthreadtest` (default, `mode=many`, `mode=deadlock`).

### As built (phase 1)

- **In the loop helper.** The scheduler is `gasm_loop.c` built with
  `-DGASM_LOOP_THREADS`; the API and primitives are `gasm_thread.h` /
  `gasm_thread.c` (`gasm_thread_*`, `gasm_mutex_*`, `gasm_cond_*`, `gasm_sem_*`,
  keys). Without the define, `gasm_loop.c` is unchanged, and `gasm_thread.c`'s
  weak single-thread versions apply (creating a thread fails, locks succeed,
  waits with a deadline time out, waits without one are a deadlock).
- **Main is a thread like the others** (it can block, e.g. in a join); threads
  other than main get their own C stack. Main-callback games (no loop helper)
  don't get threads: their frame callback can't be suspended.
- **No `gasm_run` in threaded builds**: threads switch with Asyncify, so these
  games come as the Asyncify build only, and runners call `gasm_frame`.
- **C stacks.** Asyncify leaves `__stack_pointer` alone in both directions (no
  epilogues on unwind, no prologues on rewind). So the scheduler records each
  thread's stack pointer when it unwinds, restores exactly that before
  rewinding it, and runs its own calls on a separate 64 KiB stack, so nothing
  writes below a suspended thread's frames. (A first version that resumed every
  thread from its base, or ran the scheduler on the main stack, corrupted
  main's frames: caught by `guests/threadtest`.)
- **Thread-local state**: `errno` is saved per thread; `_Thread_local`
  variables are shared (the TLS copy described above is not done:
  `gasm_thread_key_*` instead).
- **Tests**: `guests/threadtest` (producer/consumer on a condition, a
  semaphore timeout, a recursive mutex, joins, keys, errno, sleeps) and its
  `mode=many` (32 threads) give the same hashes natively (JIT, AOT) and in
  Node 22/24; `mode=deadlock` traps with the list of blocked threads on both
  runners. All in `scripts/determinism-test.sh`.

## B. Real wasm threads (later, optional)

### Model

The threads proposal: shared linear memory, atomics, `memory.atomic.wait/notify`.
Each guest thread is a new instance of the module sharing the memory, run by
the runner on an OS thread (wasmtime) or a Web Worker (browser).

### Shape

- An optional capability, declared by the guest (a `gasm:threads` import such
  as `spawn(start_arg) -> tid`, or the wasi-threads `thread-spawn` import that
  wasi-libc's `wasm32-wasip1-threads` target already uses). Runners without it
  refuse the guest with a clear error, or run it with cooperative threads if it
  was built for both.
- Only the main thread may call `gasm.*` imports that touch video, audio and
  input; other threads get compute only (and maybe assets). That keeps runners
  simple and the frame model intact.

### Costs

- **Determinism is gone** for those guests: OS scheduling decides the order.
  Headless hashes and lockstep netplay only work if the game makes its own
  results independent of timing (job systems with ordered joins can). Runners
  would mark such guests as non-reproducible.
- **Browser hosting**: shared memory needs `SharedArrayBuffer`, which needs
  cross-origin isolation (`Cross-Origin-Opener-Policy` and
  `Cross-Origin-Embedder-Policy` headers on the page and everything it embeds).
  Today's rule for Worker mode is "transferables only, no SharedArrayBuffer,
  no COOP/COEP", so the player and embedding pages would need an isolated
  variant.
- **Native**: wasmtime supports shared memory and wasi-threads; the runner
  needs a thread per guest thread and a policy for traps (one thread trapping
  ends the game).
- **The browser's main thread can't block** (`Atomics.wait` is not allowed
  there, and `memory.atomic.wait` traps). A guest whose main thread waits on a
  lock must run in Worker mode. Today `gasm:gfx` guests fall back to the main
  thread when a browser lacks WebGPU in workers; threaded gfx guests would have
  to refuse to run there instead.
- **Two builds**: a module either has shared memory or not, so one `.wasm`
  can't serve real threads and the cooperative scheme. A game that wants both
  ships two modules (the same source: the C API and the pthreads shim are
  shared).
- **Memory**: shared memories must declare a maximum size up front.
- **Invariants that change**: "headless = reproducible" and rollback netplay
  (snapshotting memory while other threads run) don't hold for these guests.
  Headless runners would run them with one worker thread and mark the hashes as
  not comparable, and lockstep games must not use them for simulation.

### When

After cooperative threads, and only when a guest needs parallel CPU (a physics
or job system, a Godot build with its worker pool). The cooperative API (C,
pthreads shim, SDL backend) stays the same, so the same source can target either.

## Later: cheaper switching

Runner-side stack switching exists now for the main loop
([stack-switching.md](stack-switching.md): `gasm_run` and `yield_frame`,
wasmtime async calls natively, JSPI in browsers). Whether cooperative threads
can use it too is open: `gasm_run` suspends one stack, and threads need one per
thread (several suspended calls, and a C shadow stack each). Until then threads
use Asyncify, as `gasm_loop` did.

## Open questions

- Default stack and Asyncify buffer sizes, and growing them (fail loudly when a
  buffer overflows: Asyncify reports it as a trap today).
- Whether the scheduler should be able to end a frame while a thread is
  runnable (a time budget per frame) for CPU-bound threads, at the cost of a
  schedule that depends on instruction counts (still deterministic if counted
  by the guest, not by wall time).
- One scheduler shared by C, SDL and Rust in the same module.
