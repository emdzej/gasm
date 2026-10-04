//! Cooperative threads for games with their own loop (design/threads.md), the Rust
//! side of the C SDK's `gasm_thread.h`: the same scheduler, the same rules.
//!
//! Every thread runs on the guest's one wasm thread, until it blocks, yields or
//! waits for the next frame; then the next ready thread runs, in creation order. A
//! frame ends when no thread is ready. So there are no data races, and the schedule
//! depends only on the input: runs stay reproducible on every runner.
//!
//! Threads need [`threaded_main_loop!`](crate::threaded_main_loop!) (and Binaryen's
//! `wasm-opt --asyncify`, as `main_loop!`); [`spawn`] panics elsewhere. The locks
//! in [`crate::sync`] work everywhere.
//!
//! ```ignore
//! fn run() -> i32 {
//!     let loader = gasm::thread::spawn(|| load_level("e1m1"));   // may wait for frames
//!     while !loader.is_finished() { draw_loading_screen(); gasm::main_loop::wait_frame(); }
//!     let level = loader.join();
//!     ...
//! }
//! gasm::threaded_main_loop!(run);
//! ```
//!
//! How it works: each thread is suspended with Asyncify into its own buffer and has
//! its own shadow stack. Asyncify leaves the wasm stack pointer alone both ways (an
//! unwind skips the functions' epilogues, a rewind their prologues), so a suspended
//! thread's frames are everything above the stack pointer it had when it unwound:
//! the scheduler records it then, sets it back before rewinding the thread, and runs
//! its own calls on a stack of its own (`gasm__get_sp`/`gasm__set_sp`: sp/gasm_sp.s).
//!
//! Differences from `std::thread`: closures needn't be `Send` (it is one wasm
//! thread), `thread_local!` values are shared by all threads, time is the frame's
//! ([`sleep`] ends at the first frame at or after its deadline), and a thread that
//! spins without blocking or yielding keeps the frame from ending.

#![cfg_attr(not(target_arch = "wasm32"), allow(dead_code, unused_imports))]

use std::cell::{Cell, RefCell, UnsafeCell};
use std::rc::Rc;
use std::time::Duration;

/// A thread's default stack (its locals, the shadow stack) and Asyncify buffer.
pub const DEFAULT_STACK: usize = 256 * 1024;
const ASYNC_SIZE: usize = 256 * 1024;
const MAIN_ASYNC_SIZE: usize = 1024 * 1024;
const SCHED_STACK: usize = 64 * 1024;
/// "No deadline" for [`block`].
pub const FOREVER: f64 = -1.0;

#[cfg(target_arch = "wasm32")]
mod asm {
    #[link(wasm_import_module = "asyncify")]
    unsafe extern "C" {
        pub fn start_unwind(data: *mut u8);
        pub fn stop_unwind();
        pub fn start_rewind(data: *mut u8);
        pub fn stop_rewind();
    }
    // sp/gasm_sp.s (libgasm_sp.a, linked by build.rs)
    unsafe extern "C" {
        pub fn gasm__get_sp() -> usize;
        pub fn gasm__set_sp(sp: usize);
    }
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum State {
    Ready,
    Blocked,
    WaitFrame,
    Done,
}

#[repr(C)]
struct AsyncData {
    cur: *mut u8,
    end: *mut u8,
}

struct Thread {
    id: u64,
    state: State,
    body: Option<Box<dyn FnOnce()>>,
    started: bool,
    detached: bool,
    timed_out: bool,
    /// its stack pointer where it unwound (suspended)
    sp: usize,
    /// Blocked: what it waits for (0: only a deadline)
    on: usize,
    /// Blocked: the time_ms() when it times out, or FOREVER
    deadline: f64,
    /// its stack (empty for main: the module's own)
    stack: Vec<u128>,
    adata: AsyncData,
    abuf: Vec<u8>,
}

impl Thread {
    fn new(id: u64, body: Option<Box<dyn FnOnce()>>, stack: usize, abuf: usize) -> Box<Thread> {
        Box::new(Thread {
            id,
            state: State::Ready,
            body,
            started: false,
            detached: false,
            timed_out: false,
            sp: 0,
            on: 0,
            deadline: FOREVER,
            stack: vec![0u128; stack.div_ceil(16)],
            adata: AsyncData { cur: std::ptr::null_mut(), end: std::ptr::null_mut() },
            abuf: vec![0u8; abuf],
        })
    }
    fn top(&self) -> usize {
        self.stack.as_ptr_range().end as usize
    }
}

struct Sched {
    /// creation order; [0] is main. Boxed: Asyncify keeps a pointer to a suspended
    /// thread's `adata` while spawns grow the Vec
    #[allow(clippy::vec_box)]
    threads: Vec<Box<Thread>>,
    current: usize,
    cursor: usize,
    next_id: u64,
    active: bool,
    started: bool,
    finished: bool,
    main: Option<fn() -> i32>,
    main_result: i32,
    main_base: usize,
    frames: u32,
}

// Asyncify's state, read right after an unwind returns and in switch_impl during a
// rewind: plain statics, no calls (nothing instrumented may run in between).
static mut REWINDING: bool = false;
static mut UNWINDING: bool = false;

struct Global(UnsafeCell<Option<Sched>>);
// One wasm thread; only the frame export and the threads it runs touch it.
unsafe impl Sync for Global {}
static SCHED: Global = Global(UnsafeCell::new(None));

#[repr(align(16))]
struct SchedStack(UnsafeCell<[u8; SCHED_STACK]>);
unsafe impl Sync for SchedStack {}
static SCHED_STACK_BUF: SchedStack = SchedStack(UnsafeCell::new([0; SCHED_STACK]));

fn sched() -> &'static mut Sched {
    // single-threaded: no two of these references are used at the same time
    unsafe {
        (*SCHED.0.get()).get_or_insert_with(|| Sched {
            threads: Vec::new(),
            current: 0,
            cursor: 0,
            next_id: 1,
            active: false,
            started: false,
            finished: false,
            main: None,
            main_result: 0,
            main_base: 0,
            frames: 0,
        })
    }
}

fn sched_top() -> usize {
    SCHED_STACK_BUF.0.get() as usize + SCHED_STACK
}

/// Suspend the current thread (its state already set) to the scheduler; returns
/// when the scheduler resumes it. Reached through a pointer (Asyncify instruments
/// the callers of indirect calls, while this stays plain, like an import).
fn switch_impl() {
    #[cfg(target_arch = "wasm32")]
    unsafe {
        if REWINDING {
            asm::stop_rewind();
            REWINDING = false;
            return;
        }
        let s = sched();
        let t = &mut s.threads[s.current];
        t.adata.cur = t.abuf.as_mut_ptr();
        t.adata.end = t.adata.cur.add(t.abuf.len());
        UNWINDING = true;
        asm::start_unwind((&raw mut t.adata).cast::<u8>());
    }
}
static SWITCH: Global2 = Global2(switch_impl);
struct Global2(fn());

fn suspend(state: State) {
    let s = sched();
    let cur = s.current;
    s.threads[cur].state = state;
    let f = unsafe { std::ptr::read_volatile(&SWITCH.0) };
    f()
}

/// Whether cooperative threads run (inside `threaded_main_loop!`).
pub fn available() -> bool {
    sched().active
}

/// Wait until [`wake`] is called on `on` or `deadline_ms` ([`crate::time_ms`] time,
/// [`FOREVER`]: none) passes; true if it timed out. For building locks: `on` is any
/// address that identifies what is waited for. Without the scheduler, a wait with a
/// deadline times out at once and one without is a deadlock (trap).
pub fn block(on: usize, deadline_ms: f64) -> bool {
    let s = sched();
    if !s.active {
        if deadline_ms >= 0.0 {
            return true;
        }
        crate::log("gasm::thread: deadlock (the only thread waits for something no other thread can do)");
        std::process::abort();
    }
    let cur = s.current;
    let t = &mut s.threads[cur];
    t.on = on;
    t.deadline = deadline_ms;
    t.timed_out = false;
    suspend(State::Blocked);
    sched().threads[sched().current].timed_out
}

/// Make threads blocked on `on` ready (the first one in creation order, or all).
pub fn wake(on: usize, one: bool) {
    if on == 0 {
        return;
    }
    for t in sched().threads.iter_mut() {
        if t.state == State::Blocked && t.on == on {
            t.state = State::Ready;
            if one {
                return;
            }
        }
    }
}

/// Let the other ready threads run first.
pub fn yield_now() {
    if sched().active {
        suspend(State::Ready);
    }
}

/// Wait at least `d` of frame time (other threads run meanwhile).
pub fn sleep(d: Duration) {
    let ms = d.as_secs_f64() * 1000.0;
    if ms <= 0.0 {
        yield_now();
    } else {
        block(0, crate::time_ms() + ms);
    }
}

/// Wait for the next frame (the same as `main_loop::wait_frame` in a threaded loop).
pub fn wait_frame() {
    suspend(State::WaitFrame);
}

/// A thread's number: main is 1, then in creation order.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug, PartialOrd, Ord)]
pub struct ThreadId(pub u64);

/// The running thread's id.
pub fn current() -> ThreadId {
    let s = sched();
    ThreadId(s.threads.get(s.current).map_or(1, |t| t.id))
}

/// The handle of a spawned thread: [`join`](JoinHandle::join) waits for its result.
/// Dropping it detaches the thread (it runs to the end on its own).
pub struct JoinHandle<T> {
    id: u64,
    result: Rc<RefCell<Option<T>>>,
    joined: Cell<bool>,
}

impl<T> JoinHandle<T> {
    pub fn thread_id(&self) -> ThreadId {
        ThreadId(self.id)
    }
    /// The thread has returned (join won't wait).
    pub fn is_finished(&self) -> bool {
        self.result.borrow().is_some()
    }
    /// Wait for the thread to finish and take its result.
    pub fn join(self) -> T {
        while self.result.borrow().is_none() {
            block(thread_addr(self.id), FOREVER);
        }
        self.joined.set(true);
        let r = self.result.borrow_mut().take().expect("result");
        // the scheduler frees a finished thread once nobody can join it
        if let Some(t) = sched().threads.iter_mut().find(|t| t.id == self.id) {
            t.detached = true;
        }
        free_done();
        r
    }
}

impl<T> Drop for JoinHandle<T> {
    fn drop(&mut self) {
        if !self.joined.get() {
            if let Some(t) = sched().threads.iter_mut().find(|t| t.id == self.id) {
                t.detached = true;
            }
            free_done();
        }
    }
}

/// What joiners block on: a thread's id (never 0).
fn thread_addr(id: u64) -> usize {
    usize::MAX - id as usize
}

/// Remove finished, detached threads (never the running one or main).
fn free_done() {
    let s = sched();
    let cur_id = s.threads.get(s.current).map(|t| t.id);
    let cursor_id = s.threads.get(s.cursor).map(|t| t.id);
    s.threads.retain(|t| t.id == 1 || !(t.state == State::Done && t.detached) || Some(t.id) == cur_id);
    s.current = cur_id.and_then(|id| s.threads.iter().position(|t| t.id == id)).unwrap_or(0);
    s.cursor = cursor_id.and_then(|id| s.threads.iter().position(|t| t.id == id)).unwrap_or(0);
}

/// Thread options (the stack size).
#[derive(Default)]
pub struct Builder {
    stack: Option<usize>,
}

impl Builder {
    pub fn new() -> Builder {
        Builder::default()
    }
    /// The thread's stack (its locals; default [`DEFAULT_STACK`]).
    pub fn stack_size(mut self, bytes: usize) -> Builder {
        self.stack = Some(bytes);
        self
    }
    /// Start a thread running `f`; it first runs when the current thread blocks,
    /// yields or waits for a frame.
    pub fn spawn<F, T>(self, f: F) -> JoinHandle<T>
    where
        F: FnOnce() -> T + 'static,
        T: 'static,
    {
        let s = sched();
        if !s.active {
            panic!("gasm::thread::spawn: threads need gasm::threaded_main_loop! (and wasm-opt --asyncify)");
        }
        let result = Rc::new(RefCell::new(None));
        let out = result.clone();
        let id = s.next_id;
        s.next_id += 1;
        let body: Box<dyn FnOnce()> = Box::new(move || {
            let v = f();
            *out.borrow_mut() = Some(v);
        });
        s.threads.push(Thread::new(id, Some(body), self.stack.unwrap_or(DEFAULT_STACK), ASYNC_SIZE));
        JoinHandle { id, result, joined: Cell::new(false) }
    }
}

/// Start a thread running `f` (see [`Builder::spawn`]).
pub fn spawn<F, T>(f: F) -> JoinHandle<T>
where
    F: FnOnce() -> T + 'static,
    T: 'static,
{
    Builder::new().spawn(f)
}

/// Frames the threaded loop has run.
pub fn frames() -> u32 {
    sched().frames
}

#[doc(hidden)]
pub fn __set_main(f: fn() -> i32) {
    sched().main = Some(f);
}

// A thread's body; instrumented (an unwind returns from here) and not inlined.
// By id: other threads may come and go while it is suspended.
#[inline(never)]
fn run_thread(id: u64) {
    if id == 1 {
        let main = sched().main;
        let rc = main.map_or(0, |f| f());
        if unsafe { UNWINDING } {
            return;
        }
        sched().main_result = rc;
    } else {
        let body = sched().threads.iter_mut().find(|t| t.id == id).and_then(|t| t.body.take());
        if let Some(b) = body {
            b();
        }
        if unsafe { UNWINDING } {
            return;
        }
    }
    if let Some(t) = sched().threads.iter_mut().find(|t| t.id == id) {
        t.state = State::Done;
    }
}

/// The next ready thread after the cursor, in creation order.
fn pick(s: &Sched) -> Option<usize> {
    let n = s.threads.len();
    (1..=n).map(|k| (s.cursor + k) % n).find(|&i| s.threads[i].state == State::Ready)
}

/// One frame: run ready threads until all of them wait (for the next frame, a
/// deadline or each other). Inlined into the `gasm_loop_frame` export, which is on
/// Asyncify's remove list: it never unwinds.
#[doc(hidden)]
#[inline(always)]
pub fn __threaded_loop_frame() {
    #[cfg(target_arch = "wasm32")]
    unsafe {
        let s = sched();
        if s.finished {
            return;
        }
        let entry_sp = asm::gasm__get_sp();
        if !s.started {
            s.started = true;
            s.active = true;
            s.main_base = entry_sp;
            s.threads.push(Thread::new(s.next_id, None, 0, MAIN_ASYNC_SIZE));
            s.next_id += 1;
        } else {
            s.frames += 1;
        }
        asm::gasm__set_sp(sched_top());
        let now = crate::time_ms();
        for t in sched().threads.iter_mut() {
            if t.state == State::WaitFrame {
                t.state = State::Ready;
            } else if t.state == State::Blocked && t.deadline >= 0.0 && now >= t.deadline {
                t.state = State::Ready;
                t.timed_out = true;
            }
        }
        while let Some(i) = pick(sched()) {
            let s = sched();
            s.cursor = i;
            s.current = i;
            let t = &mut s.threads[i];
            // a new thread starts at its base; a suspended one continues where it unwound
            let sp = if t.started { t.sp } else if i == 0 { s.main_base } else { t.top() };
            let resume = t.started;
            t.started = true;
            let id = t.id;
            asm::gasm__set_sp(sp);
            if resume {
                REWINDING = true;
                asm::start_rewind((&raw mut s.threads[i].adata).cast::<u8>());
            }
            run_thread(id);
            if UNWINDING {
                asm::stop_unwind();
                UNWINDING = false;
                let unwound_sp = asm::gasm__get_sp();
                let s = sched();
                s.threads[s.current].sp = unwound_sp;
            }
            asm::gasm__set_sp(sched_top());
            let s = sched();
            let t = &s.threads[s.current];
            if t.state == State::Done {
                if t.id == 1 {
                    s.finished = true;
                    let code = s.main_result;
                    asm::gasm__set_sp(entry_sp);
                    crate::exit(code); // proc_exit: the runner ends the game
                }
                let id = t.id;
                wake(thread_addr(id), false); // joiners
                free_done();
            }
        }
        let s = sched();
        s.current = 0;
        // the frame ends; if nothing can ever run again, say who waits for what
        let alive = s.threads.iter().any(|t| t.state == State::WaitFrame || (t.state == State::Blocked && t.deadline >= 0.0));
        if !alive {
            crate::log("gasm::thread: deadlock: every thread is blocked");
            for t in &s.threads {
                if t.state == State::Blocked {
                    crate::log(&format!("  thread {} waits on {:#x}", t.id, t.on));
                }
            }
            std::process::abort();
        }
        asm::gasm__set_sp(entry_sp);
    }
    #[cfg(not(target_arch = "wasm32"))]
    {
        // natively (the stub host) there is only main, run to the end
        let s = sched();
        if s.finished {
            return;
        }
        s.finished = true;
        let rc = s.main.map_or(0, |f| f());
        crate::exit(rc);
    }
}
