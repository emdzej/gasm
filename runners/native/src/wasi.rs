//! The WASI preview1 subset gasm runners provide: enough for wasi-libc's stdio,
//! malloc, clocks and random numbers, and nothing that reaches the host's files,
//! environment or arguments. runners/web/gasm-host.js `wasiImports` implements
//! exactly the same functions with the same results (see spec/ABI.md):
//!
//! - `fd_write` to fd 1 and 2 goes to the runner's log (stderr here), never to
//!   stdout (that carries the hash lines of headless runs)
//! - `clock_time_get` / `clock_res_get`: the virtual time in headless runs
//! - `random_get`: OS randomness, or a fixed sequence in headless runs
//! - `fd_prestat_get` answers EBADF (no preopened directories)
//! - anything else the guest imports from `wasi_snapshot_preview1` returns ENOSYS
//! - `proc_exit` ends the game

use std::io::Write;

use wasmtime::{Caller, Linker, Module, Val};

use crate::host::{Host, guest_slice, guest_slice_mut, memory};

pub const MODULE: &str = "wasi_snapshot_preview1";

const SUCCESS: i32 = 0;
const EBADF: i32 = 8;
const EINVAL: i32 = 28;
const ENOSYS: i32 = 52;
const ESPIPE: i32 = 70;

/// The guest called `proc_exit(code)`.
#[derive(Debug)]
pub struct Exit(pub i32);

impl std::fmt::Display for Exit {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "guest called proc_exit({})", self.0)
    }
}

impl std::error::Error for Exit {}

/// Deterministic `random_get` for headless runs: splitmix64 from a fixed seed,
/// little-endian (same sequence as gasm-host.js).
#[derive(Clone, Copy)]
pub struct Splitmix(pub u64);

impl Splitmix {
    pub fn next_u64(&mut self) -> u64 {
        self.0 = self.0.wrapping_add(0x9e37_79b9_7f4a_7c15);
        let mut z = self.0;
        z = (z ^ (z >> 30)).wrapping_mul(0xbf58_476d_1ce4_e5b9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94d0_49bb_1331_11eb);
        z ^ (z >> 31)
    }

    pub fn fill(&mut self, out: &mut [u8]) {
        for chunk in out.chunks_mut(8) {
            let v = self.next_u64().to_le_bytes();
            chunk.copy_from_slice(&v[..chunk.len()]);
        }
    }
}

fn write_u32(caller: &mut Caller<'_, Host>, ptr: u32, v: u32) -> wasmtime::Result<()> {
    let mem = memory(caller)?;
    guest_slice_mut(mem.data_mut(caller), ptr, 4)?.copy_from_slice(&v.to_le_bytes());
    Ok(())
}

fn write_u64(caller: &mut Caller<'_, Host>, ptr: u32, v: u64) -> wasmtime::Result<()> {
    let mem = memory(caller)?;
    guest_slice_mut(mem.data_mut(caller), ptr, 8)?.copy_from_slice(&v.to_le_bytes());
    Ok(())
}

/// Nanoseconds for a WASI clock: virtual time in headless runs (realtime counts
/// from the epoch, 1970), else the system clock / time since start.
fn clock_ns(host: &Host, id: u32) -> u64 {
    match host.virtual_time_ms {
        Some(ms) => (ms * 1e6).round() as u64,
        None if id == 0 => std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_or(0, |d| d.as_nanos() as u64),
        None => host.started().elapsed().as_nanos() as u64,
    }
}

/// The functions implemented above (the rest answer ENOSYS).
pub const IMPLEMENTED: [&str; 14] = [
    "fd_write", "fd_close", "fd_seek", "fd_prestat_get", "fd_fdstat_get", "clock_res_get", "clock_time_get",
    "random_get", "args_sizes_get", "environ_sizes_get", "args_get", "environ_get", "sched_yield", "proc_exit",
];

pub fn add_to_linker(linker: &mut Linker<Host>, module: &Module) -> wasmtime::Result<()> {
    linker.func_wrap(MODULE, "fd_write", |mut caller: Caller<'_, Host>, fd: i32, iovs: u32, iovs_len: u32, nwritten: u32| -> wasmtime::Result<i32> {
        if fd != 1 && fd != 2 {
            return Ok(EBADF);
        }
        let mem = memory(&caller)?;
        let data = mem.data(&caller);
        let list = guest_slice(data, iovs, iovs_len as u64 * 8)?;
        let mut total = 0u32;
        let mut err = std::io::stderr().lock();
        for iov in list.chunks_exact(8) {
            let ptr = u32::from_le_bytes([iov[0], iov[1], iov[2], iov[3]]);
            let len = u32::from_le_bytes([iov[4], iov[5], iov[6], iov[7]]);
            let _ = err.write_all(guest_slice(data, ptr, len as u64)?);
            total = total.wrapping_add(len);
        }
        drop(err);
        write_u32(&mut caller, nwritten, total)?;
        Ok(SUCCESS)
    })?;
    linker.func_wrap(MODULE, "fd_close", |_: Caller<'_, Host>, _fd: i32| -> i32 { SUCCESS })?;
    linker.func_wrap(MODULE, "fd_seek", |_: Caller<'_, Host>, _fd: i32, _off: i64, _whence: i32, _out: u32| -> i32 { ESPIPE })?;
    linker.func_wrap(MODULE, "fd_prestat_get", |_: Caller<'_, Host>, _fd: i32, _out: u32| -> i32 { EBADF })?;
    linker.func_wrap(MODULE, "fd_fdstat_get", |mut caller: Caller<'_, Host>, fd: i32, out: u32| -> wasmtime::Result<i32> {
        if !(0..=2).contains(&fd) {
            return Ok(EBADF);
        }
        let mem = memory(&caller)?;
        let stat = guest_slice_mut(mem.data_mut(&mut caller), out, 24)?;
        stat.fill(0);
        stat[0] = 2; // filetype: character device
        Ok(SUCCESS)
    })?;
    linker.func_wrap(MODULE, "clock_res_get", |mut caller: Caller<'_, Host>, id: u32, out: u32| -> wasmtime::Result<i32> {
        if id > 3 {
            return Ok(EINVAL);
        }
        write_u64(&mut caller, out, 1000)?;
        Ok(SUCCESS)
    })?;
    linker.func_wrap(MODULE, "clock_time_get", |mut caller: Caller<'_, Host>, id: u32, _precision: i64, out: u32| -> wasmtime::Result<i32> {
        if id > 3 {
            return Ok(EINVAL);
        }
        let ns = clock_ns(caller.data(), id);
        write_u64(&mut caller, out, ns)?;
        Ok(SUCCESS)
    })?;
    linker.func_wrap(MODULE, "random_get", |mut caller: Caller<'_, Host>, ptr: u32, len: u32| -> wasmtime::Result<i32> {
        let mem = memory(&caller)?;
        let (data, host) = mem.data_and_store_mut(&mut caller);
        let buf = guest_slice_mut(data, ptr, len as u64)?;
        match &mut host.random {
            Some(rng) => rng.fill(buf),
            None => getrandom::fill(buf).map_err(|e| wasmtime::format_err!("random_get: {e}"))?,
        }
        Ok(SUCCESS)
    })?;
    for name in ["args_sizes_get", "environ_sizes_get"] {
        linker.func_wrap(MODULE, name, |mut caller: Caller<'_, Host>, count: u32, buf_size: u32| -> wasmtime::Result<i32> {
            write_u32(&mut caller, count, 0)?;
            write_u32(&mut caller, buf_size, 0)?;
            Ok(SUCCESS)
        })?;
    }
    linker.func_wrap(MODULE, "args_get", |_: Caller<'_, Host>, _a: u32, _b: u32| -> i32 { SUCCESS })?;
    linker.func_wrap(MODULE, "environ_get", |_: Caller<'_, Host>, _a: u32, _b: u32| -> i32 { SUCCESS })?;
    linker.func_wrap(MODULE, "sched_yield", |_: Caller<'_, Host>| -> i32 { SUCCESS })?;
    linker.func_wrap(MODULE, "proc_exit", |_: Caller<'_, Host>, code: i32| -> wasmtime::Result<()> {
        Err(wasmtime::Error::new(Exit(code)))
    })?;

    // Everything else the guest imports from WASI exists and answers ENOSYS.
    for import in module.imports().filter(|i| i.module() == MODULE && !IMPLEMENTED.contains(&i.name())) {
        let Some(ty) = import.ty().func().cloned() else { continue };
        let returns_errno = ty.results().len() == 1 && ty.results().next().is_some_and(|t| t.is_i32());
        let name = import.name().to_owned();
        let what = format!("unsupported WASI function {MODULE}.{name}");
        linker.func_new(MODULE, &name, ty, move |_, _params, results| {
            if returns_errno {
                results[0] = Val::I32(ENOSYS);
                Ok(())
            } else {
                Err(wasmtime::format_err!("{what}"))
            }
        })?;
    }
    Ok(())
}
