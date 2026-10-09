//! Real threads for guests (wasi-threads): a module built for `wasm32-wasip1-threads`
//! imports a shared memory (`env.memory`) and `wasi.thread-spawn(start_arg) -> tid`, and
//! exports `wasi_thread_start(tid, start_arg)`. Each spawned thread is an OS thread with
//! its own store and its own instance of the same module, sharing the memory; its
//! `_initialize` doesn't run (the memory is already set up).
//!
//! Worker threads compute and read: the WASI subset, `gasm.time_ms`, `gasm.log`,
//! `gasm.has` and the assets (a copy taken when the thread starts) work there; the other
//! gasm imports (video, audio, input, GPU, network, storage) belong to the main thread
//! and do nothing on a worker. A trap or `proc_exit`
//! on any thread ends the whole game (wasi-threads), seen by the runner after the frame.
//! Threads aren't deterministic: headless runs allow none unless `--threads` says so
//! (`thread-spawn` then fails, as when the limit is reached), so hashes stay comparable.
//! See design/threads.md, part B.

use std::sync::atomic::{AtomicI32, AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};

use wasmtime::{Caller, Engine, Linker, Module, SharedMemory};

use crate::host::{GuestMemory, Host};

/// How a thread ended the game, for the main thread to report.
#[derive(Clone, Debug)]
pub enum ThreadEnd {
    Exit(i32),
    Trap(String),
}

/// What spawning needs, shared by every thread of a game.
pub struct Threads {
    engine: Engine,
    module: Module,
    /// the game's linker (set once it's complete, before the main instance runs)
    linker: Mutex<Option<Linker<Host>>>,
    memory: SharedMemory,
    /// most worker threads alive at once (0: spawning fails)
    limit: usize,
    live: AtomicUsize,
    next_tid: AtomicI32,
    end: Mutex<Option<ThreadEnd>>,
    /// the main thread saw `end` (else the watchdog ends the process: it may be blocked
    /// waiting for the thread that died, and no timeout interrupts an atomic wait)
    acknowledged: std::sync::atomic::AtomicBool,
    /// copied into each thread's host: the clock's origin
    start: std::time::Instant,
}

impl Threads {
    pub fn new(engine: &Engine, module: &Module, memory: SharedMemory, limit: usize, start: std::time::Instant) -> Arc<Threads> {
        Arc::new(Threads {
            engine: engine.clone(),
            module: module.clone(),
            linker: Mutex::new(None),
            memory,
            limit,
            live: AtomicUsize::new(0),
            next_tid: AtomicI32::new(1),
            end: Mutex::new(None),
            acknowledged: std::sync::atomic::AtomicBool::new(false),
            start,
        })
    }

    /// The finished linker, for the threads' instances.
    pub fn set_linker(&self, linker: Linker<Host>) {
        *self.linker.lock().unwrap() = Some(linker);
    }

    pub fn memory(&self) -> &SharedMemory {
        &self.memory
    }

    /// A thread ended the game (the first end wins); the caller reports it.
    pub fn ended(&self) -> Option<ThreadEnd> {
        let e = self.end.lock().unwrap().clone();
        if e.is_some() {
            self.acknowledged.store(true, Ordering::Release);
        }
        e
    }

    /// Most worker threads at once (gasm.max_threads).
    pub fn limit(&self) -> usize {
        self.limit
    }

    pub fn live(&self) -> usize {
        self.live.load(Ordering::Acquire)
    }

    /// `wasi.thread-spawn`: a thread id > 0, or a negative number if it can't start.
    /// `assets`: the spawning thread's, copied (threaded resource loading reads files).
    fn spawn(self: &Arc<Self>, start_arg: i32, assets: crate::assets::Assets) -> i32 {
        if self.live.fetch_add(1, Ordering::AcqRel) >= self.limit {
            self.live.fetch_sub(1, Ordering::AcqRel);
            return -1;
        }
        let tid = self.next_tid.fetch_add(1, Ordering::AcqRel);
        if tid >= 0x1fff_ffff {
            // wasi-threads ids are 29 bits
            self.live.fetch_sub(1, Ordering::AcqRel);
            return -1;
        }
        let me = self.clone();
        let started = std::thread::Builder::new().name(format!("gasm-thread-{tid}")).spawn(move || {
            let r = me.run(tid, start_arg, assets);
            if let Err(end) = r {
                let first = {
                    let mut e = me.end.lock().unwrap();
                    let first = e.is_none();
                    if first {
                        *e = Some(end.clone());
                    }
                    first
                };
                // wasi-threads: a trap or exit on any thread ends the program. The main
                // thread reports it after its frame; if it doesn't within 2 s (blocked
                // joining this thread, say), end the process here.
                if first {
                    let me = me.clone();
                    std::thread::spawn(move || {
                        std::thread::sleep(std::time::Duration::from_secs(2));
                        if !me.acknowledged.load(Ordering::Acquire) {
                            match end {
                                ThreadEnd::Exit(c) => {
                                    eprintln!("[gasm] guest exited with code {c} (on a worker thread)");
                                    std::process::exit(c);
                                }
                                ThreadEnd::Trap(t) => {
                                    eprintln!("error: {t}");
                                    std::process::exit(1);
                                }
                            }
                        }
                    });
                }
            }
            me.live.fetch_sub(1, Ordering::AcqRel);
        });
        match started {
            Ok(_) => tid,
            Err(e) => {
                eprintln!("[gasm] threads: can't start a thread: {e}");
                self.live.fetch_sub(1, Ordering::AcqRel);
                -1
            }
        }
    }

    fn run(self: &Arc<Self>, tid: i32, start_arg: i32, assets: crate::assets::Assets) -> Result<(), ThreadEnd> {
        let linker = self.linker.lock().unwrap().clone().ok_or_else(|| ThreadEnd::Trap("threads: spawned before the game was linked".into()))?;
        let mut host = Host::worker(self.clone(), self.start, assets);
        host.set_memory(GuestMemory::Shared(self.memory.clone()));
        let mut store = wasmtime::Store::new(&self.engine, host);
        // a worker may run as long as it likes (the main thread's calls have the limit)
        store.epoch_deadline_trap();
        store.set_epoch_deadline(u64::MAX / 2);
        let instance = linker.instantiate(&mut store, &self.module).map_err(|e| ThreadEnd::Trap(format!("thread {tid}: {e:#}")))?;
        let start = instance
            .get_typed_func::<(i32, i32), ()>(&mut store, "wasi_thread_start")
            .map_err(|_| ThreadEnd::Trap("the module imports wasi.thread-spawn but doesn't export wasi_thread_start".into()))?;
        match start.call(&mut store, (tid, start_arg)) {
            Ok(()) => Ok(()),
            Err(e) => match e.downcast_ref::<crate::wasi::Exit>() {
                Some(x) => Err(ThreadEnd::Exit(x.0)),
                None => Err(ThreadEnd::Trap(format!("thread {tid}: {e:#}"))),
            },
        }
    }
}

/// `wasi.thread-spawn`, for modules that import it.
pub fn add_to_linker(linker: &mut Linker<Host>) -> wasmtime::Result<()> {
    linker.func_wrap("wasi", "thread-spawn", |caller: Caller<'_, Host>, start_arg: i32| -> i32 {
        match caller.data().threads() {
            Some(t) => t.clone().spawn(start_arg, caller.data().assets.for_thread()),
            None => -1,
        }
    })?;
    Ok(())
}

/// The module's shared memory import (`env.memory`), if it has one: its type.
pub fn shared_memory_import(module: &Module) -> Option<wasmtime::MemoryType> {
    module.imports().find_map(|i| match i.ty() {
        wasmtime::ExternType::Memory(m) if m.is_shared() => Some(m),
        _ => None,
    })
}
