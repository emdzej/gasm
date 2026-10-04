//! Cooperative threads in Rust (gasm::thread, gasm::sync): producer/consumer with a
//! condition variable and timed waits, a contended mutex, a semaphore, joins with
//! results, sleeps across frames, threads spawning threads. Every event goes into a
//! log that is drawn into the frame, so the video hash covers the schedule.
//! `--param mode=many`: 32 threads yielding; `mode=deadlock`: must trap.

use std::cell::RefCell;
use std::rc::Rc;
use std::time::Duration;

use gasm::sync::{Condvar, Mutex, Semaphore};
use gasm::thread;

const W: usize = 64;
const H: usize = 64;

thread_local! {
    static LOG: RefCell<Vec<u8>> = const { RefCell::new(Vec::new()) };
}
fn ev(e: u8) {
    LOG.with(|l| l.borrow_mut().push(e));
}

fn present() {
    let log = LOG.with(|l| l.borrow().clone());
    let mut fb = vec![0u8; W * H * 4];
    for (i, px) in fb.chunks_mut(4).enumerate() {
        let e = log.get(i).copied().unwrap_or(0) as u32;
        px.copy_from_slice(&[(e * 29) as u8, (e * 83) as u8, (e * 47) as u8, 255]);
    }
    gasm::present(&fb, W as u32, H as u32, (W * 4) as u32);
}

struct Queue {
    items: Mutex<u32>,
    ready: Condvar,
}

fn default_mode() -> i32 {
    let q = Rc::new(Queue { items: Mutex::new(0), ready: Condvar::new() });
    ev(1);

    // producer: 4 items, 50 ms apart (three frames each)
    let qp = q.clone();
    let producer = thread::Builder::new().stack_size(64 * 1024).spawn(move || {
        for i in 0..4 {
            thread::sleep(Duration::from_millis(50));
            *qp.items.lock() += 1;
            ev(10 + i);
            qp.ready.notify_one();
        }
        4u32
    });
    // consumer: waits with a 20 ms timeout, counts the timeouts
    let qc = q.clone();
    let consumer = thread::spawn(move || {
        let mut taken = 0;
        let mut timeouts = 0;
        let mut items = qc.items.lock();
        while taken < 4 {
            while *items == 0 {
                let (g, r) = qc.ready.wait_timeout(items, Duration::from_millis(20));
                items = g;
                if r.timed_out() {
                    timeouts += 1;
                }
            }
            *items -= 1;
            taken += 1;
            ev(40 + taken);
        }
        timeouts
    });

    // contention: three threads add under a lock with a yield inside
    let counter = Rc::new(Mutex::new(0u32));
    let sem = Rc::new(Semaphore::new(0));
    let mixers: Vec<_> = (0..3u8)
        .map(|id| {
            let (c, s) = (counter.clone(), sem.clone());
            thread::spawn(move || {
                for i in 0..3 {
                    let mut g = c.lock();
                    let v = *g;
                    thread::yield_now(); // the others block on the lock meanwhile
                    *g = v + 1;
                    drop(g);
                    ev(60 + id * 4 + i);
                }
                // a thread spawning a thread
                let inner = thread::spawn(move || id as u32 * 100);
                let r = inner.join();
                s.release();
                r
            })
        })
        .collect();

    // main keeps presenting while the others work
    for _ in 0..6 {
        present();
        thread::wait_frame();
    }
    let mut ok = sem.acquire_timeout(Duration::from_secs(5));
    sem.acquire();
    sem.acquire();
    for (id, m) in mixers.into_iter().enumerate() {
        ok &= m.join() == id as u32 * 100;
    }
    ok &= producer.join() == 4;
    let timeouts = consumer.join();
    ok &= timeouts > 0 && *counter.lock() == 9;
    ev(if ok { 201 } else { 200 });
    gasm::log(&format!(
        "rthreadtest: {} after {} frames, {} events, {timeouts} timeouts",
        if ok { "ok" } else { "FAILED" },
        thread::frames(),
        LOG.with(|l| l.borrow().len())
    ));
    present();
    thread::wait_frame();
    if ok { 0 } else { 1 }
}

fn many_mode() -> i32 {
    let handles: Vec<_> = (0..32u32)
        .map(|i| {
            thread::spawn(move || {
                for k in 0..(i % 5 + 1) {
                    ev((i * 7 + k) as u8);
                    if k % 2 == 0 { thread::yield_now() } else { thread::wait_frame() }
                }
                i
            })
        })
        .collect();
    let sum: u32 = handles.into_iter().map(|h| h.join()).sum();
    gasm::log(&format!("rthreadtest many: sum {sum}, {} frames", thread::frames()));
    present();
    thread::wait_frame();
    if sum == (0..32).sum() { 0 } else { 1 }
}

fn deadlock_mode() -> i32 {
    let a = Rc::new(Mutex::new(()));
    let b = Rc::new(Mutex::new(()));
    let (a2, b2) = (a.clone(), b.clone());
    let t = thread::spawn(move || {
        let _x = b2.lock();
        thread::yield_now();
        let _y = a2.lock();
    });
    let _x = a.lock();
    thread::yield_now();
    let _y = b.lock(); // each waits for the other: the frame traps
    t.join();
    0
}

fn run() -> i32 {
    match gasm::param("mode").as_deref() {
        Some("many") => many_mode(),
        Some("deadlock") => deadlock_mode(),
        _ => default_mode(),
    }
}

gasm::title!("Rust thread test");
gasm::threaded_main_loop!(run);
