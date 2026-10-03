/*
 * gasm_thread.h: cooperative threads for games with their own main loop
 * (design/threads.md).
 *
 * Every thread runs on the guest's one wasm thread. A thread runs until it
 * blocks (a locked mutex, a condition or semaphore wait, a sleep, a join, a
 * yield) or waits for the next frame (gasm_wait_frame); then the next ready
 * thread runs, in creation order. A frame ends when no thread is ready. So
 * there are no data races, and the schedule depends only on the game's input:
 * hashes stay the same on every runner.
 *
 * Threads need the loop helper built with threads (gasm_loop.c with
 * -DGASM_LOOP_THREADS; CMake: gasm_add_game(<target> LOOP THREADS ...)) and the
 * Asyncify build (threads switch with Binaryen's Asyncify, inside the module).
 * Elsewhere gasm_thread_create() returns NULL, and blocking with nothing to
 * wake the caller traps (a deadlock).
 *
 * Time is the frame's: gasm_time_ms(), virtual in headless runs. Sleeps and
 * timeouts end at the first frame at or after their deadline.
 *
 * Limits: `_Thread_local` variables are shared by all threads (use
 * gasm_thread_key_*); errno is per thread. Code that spins on a flag without
 * blocking or yielding never lets the frame end.
 *
 * SPDX-License-Identifier: MIT
 */
#ifndef GASM_THREAD_H
#define GASM_THREAD_H

#include <stddef.h>
#include <stdint.h>

#ifdef __cplusplus
extern "C" {
#endif

typedef struct gasm_thread gasm_thread;

/** No deadline (wait forever). */
#define GASM_FOREVER (-1.0)

/* ---- threads -------------------------------------------------------------------------------- */

/** Start fn(arg) as a new thread; it first runs when the current thread blocks
 *  or yields. stack: C stack size in bytes (0: 256 KiB). NULL if threads aren't
 *  available or out of memory. */
gasm_thread *gasm_thread_create(int (*fn)(void *arg), void *arg, size_t stack);
/** Wait for the thread to end and return its result; frees it. */
int gasm_thread_join(gasm_thread *t);
/** Free the thread when it ends; it can't be joined afterwards. */
void gasm_thread_detach(gasm_thread *t);
/** The calling thread (the main thread is never NULL). */
gasm_thread *gasm_thread_self(void);
/** A small number for the thread: 0 for the main thread, then 1, 2, … in creation order. */
uint32_t gasm_thread_id(gasm_thread *t);
/** Let the other ready threads run; continues in the same frame. */
void gasm_thread_yield(void);
/** Sleep until gasm_time_ms() reaches now + ms (at least until the next frame if ms > 0). */
void gasm_thread_sleep_ms(double ms);

/* ---- mutexes, conditions, semaphores ---------------------------------------------------------- */

typedef struct { gasm_thread *owner; uint32_t count; int recursive; } gasm_mutex;
#define GASM_MUTEX_INIT { 0, 0, 0 }
#define GASM_MUTEX_RECURSIVE_INIT { 0, 0, 1 }

void gasm_mutex_lock(gasm_mutex *m);
/** 1 if locked, 0 if another thread holds it. */
int gasm_mutex_trylock(gasm_mutex *m);
void gasm_mutex_unlock(gasm_mutex *m);

typedef struct { int unused; } gasm_cond;
#define GASM_COND_INIT { 0 }

/** Unlock m, wait to be signalled (or until gasm_time_ms() >= deadline_ms), lock m
 *  again. Returns 0 when signalled, 1 on timeout. Spurious wakeups are possible. */
int gasm_cond_wait(gasm_cond *c, gasm_mutex *m, double deadline_ms);
void gasm_cond_signal(gasm_cond *c);
void gasm_cond_broadcast(gasm_cond *c);

typedef struct { uint32_t count; } gasm_sem;

/** Take one (waiting until gasm_time_ms() >= deadline_ms at most): 0 if taken, 1 on timeout. */
int gasm_sem_wait(gasm_sem *s, double deadline_ms);
/** 1 if taken, 0 if the count is 0. */
int gasm_sem_trywait(gasm_sem *s);
void gasm_sem_post(gasm_sem *s);

/* ---- thread-local values ---------------------------------------------------------------------- */

/** Up to GASM_THREAD_KEYS keys; each thread has its own value per key (initially NULL). */
#define GASM_THREAD_KEYS 64
/** A new key, or -1 if all are taken. destructor (may be NULL) runs on the value when a thread ends. */
int gasm_thread_key_create(void (*destructor)(void *));
void gasm_thread_key_delete(int key);
void *gasm_thread_get(int key);
void gasm_thread_set(int key, void *value);

/* ---- for the loop helper (gasm_loop.c); not for games ------------------------------------------ */

/** Block the calling thread on `on` until gasm__thread_wake(on) or the deadline:
 *  0 when woken, 1 on timeout. */
int gasm__thread_block(const void *on, double deadline_ms);
/** Make threads blocked on `on` ready again: all of them, or the first (creation order) if one. */
void gasm__thread_wake(const void *on, int one);

#ifdef __cplusplus
}
#endif

#endif
