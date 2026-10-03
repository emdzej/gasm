/*
 * gasm_thread.c: mutexes, conditions, semaphores and thread-local keys for
 * gasm_thread.h, on top of the scheduler's block/wake (gasm_loop.c built with
 * -DGASM_LOOP_THREADS). Without it, the weak single-thread versions below are
 * used: there is only the calling thread, so locks always succeed, waits with
 * a deadline time out at once and waits without one are a deadlock.
 *
 * SPDX-License-Identifier: MIT
 */
#include <stdio.h>
#include <stdlib.h>

#include "gasm.h"
#include "gasm_thread.h"

#ifdef __cplusplus
extern "C" {
#endif

/* ---- single-thread defaults (gasm_loop.c with GASM_LOOP_THREADS overrides these) ----------- */

static struct { void *value[GASM_THREAD_KEYS]; } solo_thread;
static void (*solo_dtor[GASM_THREAD_KEYS])(void *);
static unsigned char solo_used[GASM_THREAD_KEYS];

__attribute__((weak)) gasm_thread *gasm_thread_create(int (*fn)(void *), void *arg, size_t stack) {
    (void)fn; (void)arg; (void)stack;
    gasm_log_str("gasm_thread_create: threads need the loop helper built with GASM_LOOP_THREADS (and Asyncify)");
    return NULL;
}
__attribute__((weak)) int gasm_thread_join(gasm_thread *t) { (void)t; return 0; }
__attribute__((weak)) void gasm_thread_detach(gasm_thread *t) { (void)t; }
__attribute__((weak)) gasm_thread *gasm_thread_self(void) { return (gasm_thread *)&solo_thread; }
__attribute__((weak)) uint32_t gasm_thread_id(gasm_thread *t) { (void)t; return 0; }
__attribute__((weak)) void gasm_thread_yield(void) {}
__attribute__((weak)) void gasm_thread_sleep_ms(double ms) { (void)ms; }

__attribute__((weak)) int gasm__thread_block(const void *on, double deadline_ms) {
    (void)on;
    if (deadline_ms >= 0)
        return 1;   /* nothing else can run: the wait times out */
    gasm_log_str("gasm_thread: deadlock (the only thread waits for something no other thread can do)");
    __builtin_trap();
}
__attribute__((weak)) void gasm__thread_wake(const void *on, int one) { (void)on; (void)one; }

__attribute__((weak)) int gasm_thread_key_create(void (*destructor)(void *)) {
    for (int k = 0; k < GASM_THREAD_KEYS; k++)
        if (!solo_used[k]) {
            solo_used[k] = 1;
            solo_dtor[k] = destructor;
            solo_thread.value[k] = NULL;
            return k;
        }
    return -1;
}
__attribute__((weak)) void gasm_thread_key_delete(int key) {
    if (key >= 0 && key < GASM_THREAD_KEYS)
        solo_used[key] = 0;
}
__attribute__((weak)) void *gasm_thread_get(int key) {
    return key >= 0 && key < GASM_THREAD_KEYS ? solo_thread.value[key] : NULL;
}
__attribute__((weak)) void gasm_thread_set(int key, void *value) {
    if (key >= 0 && key < GASM_THREAD_KEYS)
        solo_thread.value[key] = value;
}

/* ---- primitives (any scheduler) --------------------------------------------------------------- */

void gasm_mutex_lock(gasm_mutex *m) {
    gasm_thread *self = gasm_thread_self();
    if (m->owner == self && m->recursive) {
        m->count++;
        return;
    }
    while (m->owner)   /* held (by another thread, or by us without recursion: a deadlock) */
        gasm__thread_block(m, GASM_FOREVER);
    m->owner = self;
    m->count = 1;
}

int gasm_mutex_trylock(gasm_mutex *m) {
    gasm_thread *self = gasm_thread_self();
    if (m->owner == self && m->recursive) {
        m->count++;
        return 1;
    }
    if (m->owner)
        return 0;
    m->owner = self;
    m->count = 1;
    return 1;
}

void gasm_mutex_unlock(gasm_mutex *m) {
    if (!m->owner || --m->count > 0)
        return;
    m->owner = NULL;
    gasm__thread_wake(m, 1);
}

int gasm_cond_wait(gasm_cond *c, gasm_mutex *m, double deadline_ms) {
    /* keep the recursion depth across the wait (pthreads only allows depth 1 here) */
    uint32_t depth = m->count;
    m->count = 1;
    gasm_mutex_unlock(m);
    int timed_out = gasm__thread_block(c, deadline_ms);
    gasm_mutex_lock(m);
    m->count = depth;
    return timed_out;
}

void gasm_cond_signal(gasm_cond *c) { gasm__thread_wake(c, 1); }
void gasm_cond_broadcast(gasm_cond *c) { gasm__thread_wake(c, 0); }

int gasm_sem_wait(gasm_sem *s, double deadline_ms) {
    while (s->count == 0)
        if (gasm__thread_block(s, deadline_ms))
            return s->count == 0 ? 1 : (s->count--, 0);
    s->count--;
    return 0;
}

int gasm_sem_trywait(gasm_sem *s) {
    if (s->count == 0)
        return 0;
    s->count--;
    return 1;
}

void gasm_sem_post(gasm_sem *s) {
    s->count++;
    gasm__thread_wake(s, 1);
}

#ifdef __cplusplus
}
#endif
