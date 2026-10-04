//! Locks for [`crate::thread`]'s cooperative threads, shaped like `std::sync`:
//! [`Mutex`], [`Condvar`], [`Semaphore`]. A wait lets the other threads run; time
//! is the frame's. Without the threaded loop there is only the calling thread: locks
//! always succeed, waits with a timeout time out at once, waits without one trap
//! (nothing else could ever wake them).

use std::cell::{Cell, UnsafeCell};
use std::ops::{Deref, DerefMut};
use std::time::Duration;

use crate::thread::{self, FOREVER};

fn addr<T: ?Sized>(x: &T) -> usize {
    x as *const T as *const u8 as usize
}

/// A mutual exclusion lock (not recursive: locking it again from the same thread
/// is a deadlock).
pub struct Mutex<T: ?Sized> {
    owner: Cell<u64>,
    data: UnsafeCell<T>,
}

// One wasm thread, and only one cooperative thread inside at a time.
unsafe impl<T: ?Sized> Sync for Mutex<T> {}
unsafe impl<T: ?Sized> Send for Mutex<T> {}

impl<T> Mutex<T> {
    pub const fn new(value: T) -> Mutex<T> {
        Mutex { owner: Cell::new(0), data: UnsafeCell::new(value) }
    }
    pub fn into_inner(self) -> T {
        self.data.into_inner()
    }
}

impl<T: ?Sized> Mutex<T> {
    /// Wait until the lock is free, then hold it until the guard drops.
    pub fn lock(&self) -> MutexGuard<'_, T> {
        while self.owner.get() != 0 {
            thread::block(addr(self), FOREVER);
        }
        self.owner.set(thread::current().0);
        MutexGuard { lock: self }
    }
    /// The lock if it is free now.
    pub fn try_lock(&self) -> Option<MutexGuard<'_, T>> {
        if self.owner.get() != 0 {
            return None;
        }
        self.owner.set(thread::current().0);
        Some(MutexGuard { lock: self })
    }
    pub fn get_mut(&mut self) -> &mut T {
        self.data.get_mut()
    }
    fn unlock(&self) {
        self.owner.set(0);
        thread::wake(addr(self), true);
    }
}

impl<T: Default> Default for Mutex<T> {
    fn default() -> Self {
        Mutex::new(T::default())
    }
}

/// Holds a [`Mutex`]; unlocks it when dropped.
pub struct MutexGuard<'a, T: ?Sized> {
    lock: &'a Mutex<T>,
}

impl<T: ?Sized> Deref for MutexGuard<'_, T> {
    type Target = T;
    fn deref(&self) -> &T {
        unsafe { &*self.lock.data.get() }
    }
}
impl<T: ?Sized> DerefMut for MutexGuard<'_, T> {
    fn deref_mut(&mut self) -> &mut T {
        unsafe { &mut *self.lock.data.get() }
    }
}
impl<T: ?Sized> Drop for MutexGuard<'_, T> {
    fn drop(&mut self) {
        self.lock.unlock();
    }
}

/// Whether a timed wait returned because the time ran out.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct WaitTimeoutResult(bool);
impl WaitTimeoutResult {
    pub fn timed_out(&self) -> bool {
        self.0
    }
}

/// A condition variable: wait for a change another thread makes under a [`Mutex`].
#[derive(Default)]
pub struct Condvar {
    // a byte, so every Condvar has its own address (what waiters block on)
    _id: u8,
}

impl Condvar {
    pub const fn new() -> Condvar {
        Condvar { _id: 0 }
    }
    /// Unlock, wait for a notify, lock again.
    pub fn wait<'a, T: ?Sized>(&self, guard: MutexGuard<'a, T>) -> MutexGuard<'a, T> {
        self.wait_deadline(guard, FOREVER).0
    }
    /// [`wait`](Condvar::wait) with a timeout in frame time.
    pub fn wait_timeout<'a, T: ?Sized>(&self, guard: MutexGuard<'a, T>, d: Duration) -> (MutexGuard<'a, T>, WaitTimeoutResult) {
        self.wait_deadline(guard, crate::time_ms() + d.as_secs_f64() * 1000.0)
    }
    /// Wait while `condition` holds.
    pub fn wait_while<'a, T: ?Sized>(&self, mut guard: MutexGuard<'a, T>, mut condition: impl FnMut(&mut T) -> bool) -> MutexGuard<'a, T> {
        while condition(&mut *guard) {
            guard = self.wait(guard);
        }
        guard
    }
    fn wait_deadline<'a, T: ?Sized>(&self, guard: MutexGuard<'a, T>, deadline: f64) -> (MutexGuard<'a, T>, WaitTimeoutResult) {
        let lock = guard.lock;
        drop(guard);
        let timed_out = thread::block(addr(self), deadline);
        (lock.lock(), WaitTimeoutResult(timed_out))
    }
    pub fn notify_one(&self) {
        thread::wake(addr(self), true);
    }
    pub fn notify_all(&self) {
        thread::wake(addr(self), false);
    }
}

/// A counting semaphore.
pub struct Semaphore {
    count: Cell<u32>,
}

unsafe impl Sync for Semaphore {}

impl Semaphore {
    pub const fn new(count: u32) -> Semaphore {
        Semaphore { count: Cell::new(count) }
    }
    /// Wait until the count is above 0, then take one.
    pub fn acquire(&self) {
        self.acquire_deadline(FOREVER);
    }
    /// [`acquire`](Semaphore::acquire) with a timeout in frame time; false if it timed out.
    pub fn acquire_timeout(&self, d: Duration) -> bool {
        self.acquire_deadline(crate::time_ms() + d.as_secs_f64() * 1000.0)
    }
    pub fn try_acquire(&self) -> bool {
        let n = self.count.get();
        if n == 0 {
            return false;
        }
        self.count.set(n - 1);
        true
    }
    pub fn release(&self) {
        self.count.set(self.count.get() + 1);
        thread::wake(addr(self), true);
    }
    pub fn count(&self) -> u32 {
        self.count.get()
    }
    fn acquire_deadline(&self, deadline: f64) -> bool {
        while self.count.get() == 0 {
            if thread::block(addr(self), deadline) {
                return self.try_acquire();
            }
        }
        self.count.set(self.count.get() - 1);
        true
    }
}
