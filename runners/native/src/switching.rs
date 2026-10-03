//! Stack switching for guests that export `gasm_run` (design/stack-switching.md).
//!
//! The guest's whole run is one async wasmtime call on a fiber. `gasm.yield_frame`
//! is an async host function that returns `Pending`, which suspends the fiber: the
//! runner's poll of the call returns, and the frame is over. The next frame polls
//! again and the guest continues after `yield_frame`.
//!
//! While suspended, the running call owns the store, so the runner reaches the
//! [`Host`] through [`Exchange`]: it leaves a command (resume, run a closure on
//! the host, quit) and polls; `yield_frame`, which holds the `Caller`, carries it
//! out and suspends again. Everything happens on the runner's thread, inside its
//! poll.

use std::future::Future;
use std::pin::Pin;
use std::sync::{Arc, Mutex};
use std::task::{Context, Poll, Waker};

use wasmtime::{AsContextMut, Caller, Linker, bail};

use crate::host::Host;

/// A closure on the host, lent to the suspended guest for the length of one poll.
pub(crate) struct Job(pub(crate) *mut (dyn FnMut(&mut Host) + 'static));

// Only ever used on the thread that polls: the pointer is taken and called within
// the poll that `Game::with_host` makes, while the closure is alive on its stack.
unsafe impl Send for Job {}

pub(crate) enum Cmd {
    None,
    /// start the next frame
    Resume,
    /// run a closure on the host, then stay suspended
    Job(Job),
    /// the player quits: call `gasm_exit`, then end the run
    Exit,
}

/// What the runner asks of a suspended guest (shared with `yield_frame` via the host).
pub(crate) struct Exchange {
    pub(crate) cmd: Mutex<Cmd>,
    /// epoch ticks per frame segment (`--call-timeout`)
    pub(crate) deadline: u64,
}

impl Exchange {
    pub(crate) fn new(deadline: u64) -> Arc<Exchange> {
        Arc::new(Exchange { cmd: Mutex::new(Cmd::None), deadline })
    }

    pub(crate) fn set(&self, cmd: Cmd) {
        *self.cmd.lock().unwrap() = cmd;
    }

    fn take(&self) -> Cmd {
        std::mem::replace(&mut *self.cmd.lock().unwrap(), Cmd::None)
    }
}

/// The run ended because the player quit (after `gasm_exit`).
#[derive(Debug)]
pub(crate) struct Quit;

impl std::fmt::Display for Quit {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("the player quit")
    }
}

impl std::error::Error for Quit {}

/// Ready on the second poll: one suspension of the fiber.
struct YieldOnce(bool);

impl Future for YieldOnce {
    type Output = ();
    fn poll(mut self: Pin<&mut Self>, _: &mut Context<'_>) -> Poll<()> {
        if self.0 {
            Poll::Ready(())
        } else {
            self.0 = true;
            Poll::Pending
        }
    }
}

/// `gasm.yield_frame` for the async engine: suspend until the runner resumes the
/// guest, carrying out its commands meanwhile.
pub(crate) fn add_yield_async(linker: &mut Linker<Host>) -> wasmtime::Result<()> {
    linker.func_wrap_async("gasm", "yield_frame", |mut caller: Caller<'_, Host>, (): ()| {
        Box::new(async move {
            let Some(ex) = caller.data().run.clone() else {
                bail!("gasm.yield_frame: only inside gasm_run");
            };
            loop {
                YieldOnce(false).await;
                match ex.take() {
                    Cmd::Resume => break,
                    Cmd::Job(job) => unsafe { (*job.0)(caller.data_mut()) },
                    Cmd::Exit => {
                        // on top of the suspended run, like a gasm_exit between frames
                        if let Some(f) = caller.get_export("gasm_exit").and_then(|e| e.into_func()) {
                            caller.as_context_mut().set_epoch_deadline(ex.deadline);
                            if let Err(e) = f.typed::<(), ()>(&caller)?.call_async(&mut caller, ()).await {
                                if e.downcast_ref::<crate::wasi::Exit>().is_none() {
                                    eprintln!("[gasm] gasm_exit trapped: {e:?}");
                                }
                            }
                        }
                        return Err(wasmtime::Error::new(Quit));
                    }
                    Cmd::None => {}
                }
            }
            caller.as_context_mut().set_epoch_deadline(ex.deadline);
            Ok(())
        })
    })?;
    Ok(())
}

/// `gasm.yield_frame` for the sync engine (guests without `gasm_run`).
pub(crate) fn add_yield_sync(linker: &mut Linker<Host>) -> wasmtime::Result<()> {
    linker.func_wrap("gasm", "yield_frame", || -> wasmtime::Result<()> { bail!("gasm.yield_frame: only inside gasm_run") })?;
    Ok(())
}

/// Poll a future once, without a real waker (the runner polls once per frame).
pub(crate) fn poll_once<F: Future + ?Sized>(f: Pin<&mut F>) -> Poll<F::Output> {
    f.poll(&mut Context::from_waker(Waker::noop()))
}

/// Run a future that must not suspend (init, exit of a guest that never ran).
pub(crate) fn block_on<F: Future>(f: F) -> wasmtime::Result<F::Output> {
    let mut f = std::pin::pin!(f);
    match poll_once(f.as_mut()) {
        Poll::Ready(v) => Ok(v),
        Poll::Pending => bail!("gasm.yield_frame: only inside gasm_run"),
    }
}
