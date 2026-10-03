/*
 * threadtest: cooperative threads (gasm_thread.h) in a deterministic scene.
 * Every event (a thread starting, producing, consuming, timing out, ending) is
 * appended to a log that is drawn into the frame, so the video hash covers the
 * exact schedule: it must be the same on every runner.
 *
 * Threads: a producer that makes an item every 3 frames (sleep), a consumer
 * waiting on a condition, a worker that takes a semaphore with a timeout, two
 * that share a recursive mutex, and a "computer" whose result main joins.
 * Main exits with 0 once everything has happened (or 1 if something is wrong).
 *
 * SPDX-License-Identifier: MIT
 */
#include <errno.h>
#include <stdint.h>
#include <stdio.h>
#include <string.h>

#include "gasm.h"
#include "gasm_loop.h"
#include "gasm_thread.h"

GASM_TITLE("Thread test");

#define W 64
#define H 64
static uint32_t fb[W * H];

/* the event log: one byte per event, drawn as a row of coloured pixels */
static uint8_t log_[W * H];
static unsigned nlog;
static void ev(uint8_t e) {
    if (nlog < sizeof log_)
        log_[nlog++] = e;
}

static gasm_mutex lock = GASM_MUTEX_INIT;
static gasm_cond ready = GASM_COND_INIT;
static int queue[8], qlen, produced, consumed;

static int producer(void *arg) {
    int n = *(int *)arg;
    for (int i = 1; i <= n; i++) {
        gasm_thread_sleep_ms(3 * 1000.0 / 60);   /* three frames */
        gasm_mutex_lock(&lock);
        queue[qlen++] = i;
        produced++;
        ev((uint8_t)(10 + i));
        gasm_cond_signal(&ready);
        gasm_mutex_unlock(&lock);
    }
    return n;
}

static int consumer(void *arg) {
    int want = *(int *)arg, sum = 0;
    gasm_mutex_lock(&lock);
    while (consumed < want) {
        while (qlen == 0)
            gasm_cond_wait(&ready, &lock, GASM_FOREVER);
        sum += queue[--qlen];
        consumed++;
        ev((uint8_t)(40 + consumed));
    }
    gasm_mutex_unlock(&lock);
    return sum;
}

static gasm_sem token;
static int waiter(void *arg) {
    (void)arg;
    /* nobody posts in time: times out after 5 frames, then gets the token main posts */
    int timed_out = gasm_sem_wait(&token, gasm_time_ms() + 5 * 1000.0 / 60);
    ev(timed_out ? 70 : 71);
    int again = gasm_sem_wait(&token, GASM_FOREVER);
    ev(again ? 72 : 73);
    return timed_out && !again ? 0 : 1;
}

static gasm_mutex rec = GASM_MUTEX_RECURSIVE_INIT;
static int shared;
static int sharer(void *arg) {
    int id = *(int *)arg;
    for (int i = 0; i < 3; i++) {
        gasm_mutex_lock(&rec);
        gasm_mutex_lock(&rec);   /* recursive */
        int v = shared;
        gasm_thread_yield();     /* the other sharer can't get in meanwhile */
        shared = v + 1;
        ev((uint8_t)(80 + id * 10 + i));
        gasm_mutex_unlock(&rec);
        gasm_mutex_unlock(&rec);
        gasm_thread_yield();
    }
    return 0;
}

static int key;
static int computer(void *arg) {
    (void)arg;
    gasm_thread_set(key, (void *)(uintptr_t)7);
    errno = 33;                  /* errno and keys are per thread */
    gasm_thread_yield();
    int ok = gasm_thread_get(key) == (void *)(uintptr_t)7 && errno == 33;
    ev(ok ? 100 : 101);
    int f = 1;
    for (int i = 2; i <= 10; i++)
        f *= i;
    return f;                    /* 3628800 */
}

static void draw(void) {
    for (unsigned i = 0; i < W * H; i++) {
        uint32_t e = i < nlog ? log_[i] : 0;
        uint32_t r = (e * 37) & 255, g = (e * 91) & 255, b = (e * 53) & 255;
        fb[i] = r | g << 8 | b << 16 | 0xffu << 24;
    }
    gasm_video_present(fb, W, H, W * 4);
}

static void frame_end(void) {
    draw();
    gasm_wait_frame();
}


/* ?mode=deadlock: two threads take two mutexes in opposite order (must trap). */
static gasm_mutex m1 = GASM_MUTEX_INIT, m2 = GASM_MUTEX_INIT;
static int lock_pair(void *arg) {
    int first = *(int *)arg;
    gasm_mutex_lock(first ? &m1 : &m2);
    gasm_thread_yield();
    gasm_mutex_lock(first ? &m2 : &m1);
    return 0;
}

/* ?mode=many: 32 threads, each yields 200 times and adds to a shared counter. */
static unsigned counter;
static int spinner(void *arg) {
    unsigned id = (unsigned)(uintptr_t)arg;
    for (int i = 0; i < 200; i++) {
        counter += id;
        if (i % 50 == 49)
            gasm_wait_frame();   /* any thread can wait for the next frame */
        else
            gasm_thread_yield();
    }
    return (int)id;
}

int gasm_main(void) {
    static int items = 4, a = 1, b = 2;
    char mode[16] = "";
    gasm_param_str("mode", mode, sizeof mode);
    if (!strcmp(mode, "deadlock")) {
        static int x = 1, y = 0;
        gasm_thread *t1 = gasm_thread_create(lock_pair, &x, 0), *t2 = gasm_thread_create(lock_pair, &y, 0);
        gasm_thread_join(t1);   /* never returns: the frame ends in a deadlock trap */
        gasm_thread_join(t2);
        return 3;
    }
    if (!strcmp(mode, "many")) {
        gasm_thread *t[32];
        for (unsigned i = 0; i < 32; i++)
            t[i] = gasm_thread_create(spinner, (void *)(uintptr_t)(i + 1), 16 * 1024);
        int sum = 0;
        for (unsigned i = 0; i < 32; i++)
            sum += gasm_thread_join(t[i]);
        ev((uint8_t)(counter & 255));
        char msg[64];
        snprintf(msg, sizeof msg, "threadtest many: counter %u after %u frames", counter, gasm_loop_frames());
        gasm_log_str(msg);
        frame_end();
        return sum == 32 * 33 / 2 && counter == 200u * 528 ? 0 : 1;
    }
    key = gasm_thread_key_create(NULL);
    gasm_thread_set(key, (void *)(uintptr_t)1);
    errno = 5;
    ev(1);
    gasm_thread *p = gasm_thread_create(producer, &items, 0);
    gasm_thread *c = gasm_thread_create(consumer, &items, 0);
    gasm_thread *w = gasm_thread_create(waiter, NULL, 0);
    gasm_thread *s1 = gasm_thread_create(sharer, &a, 0);
    gasm_thread *s2 = gasm_thread_create(sharer, &b, 0);
    gasm_thread *k = gasm_thread_create(computer, NULL, 0);
    if (!p || !c || !w || !s1 || !s2 || !k)
        return 2;
    int bad = 0;
    for (int f = 0; f < 8; f++)
        frame_end();
    gasm_sem_post(&token);                       /* the waiter's second take */
    bad |= gasm_thread_join(k) != 3628800;
    bad |= gasm_thread_join(s1) | gasm_thread_join(s2);
    bad |= shared != 6;
    bad |= gasm_thread_join(w);
    bad |= gasm_thread_join(p) != items;
    bad |= gasm_thread_join(c) != 1 + 2 + 3 + 4;
    bad |= gasm_thread_get(key) != (void *)(uintptr_t)1 || errno != 5;
    ev(bad ? 200 : 201);
    char msg[64];
    snprintf(msg, sizeof msg, "threadtest: %s after %u frames, %u events", bad ? "FAILED" : "ok", gasm_loop_frames(), nlog);
    gasm_log_str(msg);
    for (int f = 0; f < 2; f++)
        frame_end();
    return bad;
}
