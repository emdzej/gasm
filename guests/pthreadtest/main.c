/*
 * pthreadtest: ordinary POSIX threads code (only <pthread.h>, <semaphore.h>,
 * <sched.h>, <time.h>) on gasm's cooperative threads (sdk/c/src/gasm_pthread.c).
 * The loop helper's gasm_main stands in for main(); everything else is what a
 * pthread program would write. The event log is drawn into the frame, so the
 * video hash covers the schedule.
 *
 * SPDX-License-Identifier: MIT
 */
#include <errno.h>
#include <pthread.h>
#include <sched.h>
#include <semaphore.h>
#include <stdint.h>
#include <stdio.h>
#include <string.h>
#include <time.h>

#include "gasm.h"
#include "gasm_loop.h"

GASM_TITLE("pthread test");

#define W 64
#define H 64
static uint32_t fb[W * H];
static uint8_t events[W * H];
static unsigned nev;
static void ev(uint8_t e) {
    if (nev < sizeof events)
        events[nev++] = e;
}

static void present(void) {
    for (unsigned i = 0; i < W * H; i++) {
        uint32_t e = i < nev ? events[i] : 0;
        fb[i] = ((e * 29) & 255) | ((e * 83) & 255) << 8 | ((e * 47) & 255) << 16 | 0xffu << 24;
    }
    gasm_video_present(fb, W, H, W * 4);
}

/* producer / consumer with a timed wait */
static pthread_mutex_t lock = PTHREAD_MUTEX_INITIALIZER;
static pthread_cond_t cond = PTHREAD_COND_INITIALIZER;
static int items, taken, timeouts;

static void *producer(void *arg) {
    int n = (int)(intptr_t)arg;
    for (int i = 0; i < n; i++) {
        struct timespec d = { 0, 50 * 1000 * 1000 };   /* 50 ms: three frames */
        nanosleep(&d, NULL);
        pthread_mutex_lock(&lock);
        items++;
        ev((uint8_t)(10 + i));
        pthread_cond_signal(&cond);
        pthread_mutex_unlock(&lock);
    }
    return (void *)(intptr_t)n;
}

static void *consumer(void *arg) {
    int n = (int)(intptr_t)arg;
    pthread_mutex_lock(&lock);
    while (taken < n) {
        while (items == 0) {
            struct timespec at;
            clock_gettime(CLOCK_REALTIME, &at);
            at.tv_nsec += 20 * 1000 * 1000;            /* 20 ms */
            if (at.tv_nsec >= 1000000000L) {
                at.tv_sec++;
                at.tv_nsec -= 1000000000L;
            }
            if (pthread_cond_timedwait(&cond, &lock, &at) == ETIMEDOUT)
                timeouts++;
        }
        items--;
        taken++;
        ev((uint8_t)(40 + taken));
    }
    pthread_mutex_unlock(&lock);
    return NULL;
}

/* once, keys (with a destructor), recursive mutex, rwlock, semaphore, spinlock */
static pthread_once_t once = PTHREAD_ONCE_INIT;
static int inits;
static void init_once(void) {
    inits++;
    sched_yield();   /* another thread calling pthread_once meanwhile must wait */
}

static pthread_key_t key;
static int destructed;
static void dtor(void *v) { destructed += (int)(intptr_t)v; }

static pthread_mutex_t rec;
static pthread_rwlock_t rw = PTHREAD_RWLOCK_INITIALIZER;
static sem_t sem;
static pthread_spinlock_t spin;
static int shared, spin_count;

static void *mixer(void *arg) {
    int id = (int)(intptr_t)arg;
    pthread_once(&once, init_once);
    pthread_setspecific(key, (void *)(intptr_t)(id * 10));
    for (int i = 0; i < 3; i++) {
        pthread_mutex_lock(&rec);
        pthread_mutex_lock(&rec);
        int v = shared;
        sched_yield();
        shared = v + 1;
        pthread_mutex_unlock(&rec);
        pthread_mutex_unlock(&rec);
        pthread_rwlock_rdlock(&rw);
        sched_yield();
        pthread_rwlock_unlock(&rw);
        pthread_spin_lock(&spin);
        sched_yield();   /* holding a spinlock across a switch: the other spins by yielding */
        spin_count++;
        pthread_spin_unlock(&spin);
        ev((uint8_t)(60 + id * 4 + i));
    }
    sem_post(&sem);
    return pthread_getspecific(key);
}

int gasm_main(void) {
    pthread_mutexattr_t a;
    pthread_mutexattr_init(&a);
    pthread_mutexattr_settype(&a, PTHREAD_MUTEX_RECURSIVE);
    pthread_mutex_init(&rec, &a);
    pthread_key_create(&key, dtor);
    sem_init(&sem, 0, 0);
    pthread_spin_init(&spin, PTHREAD_PROCESS_PRIVATE);
    ev(1);

    pthread_t p, c, m1, m2;
    pthread_attr_t small;
    pthread_attr_init(&small);
    pthread_attr_setstacksize(&small, 64 * 1024);
    int bad = 0;
    bad |= pthread_create(&p, &small, producer, (void *)(intptr_t)4);
    bad |= pthread_create(&c, NULL, consumer, (void *)(intptr_t)4);
    bad |= pthread_create(&m1, NULL, mixer, (void *)(intptr_t)1);
    bad |= pthread_create(&m2, NULL, mixer, (void *)(intptr_t)2);

    /* main writes under the rwlock while the mixers read */
    for (int f = 0; f < 6; f++) {
        pthread_rwlock_wrlock(&rw);
        present();
        gasm_wait_frame();       /* holding the write lock across a frame */
        pthread_rwlock_unlock(&rw);
    }
    struct timespec at;
    clock_gettime(CLOCK_REALTIME, &at);
    at.tv_sec += 5;
    bad |= sem_timedwait(&sem, &at) != 0;
    bad |= sem_wait(&sem) != 0;
    void *r;
    bad |= pthread_join(m1, &r) || r != (void *)(intptr_t)10;
    bad |= pthread_join(m2, &r) || r != (void *)(intptr_t)20;
    bad |= pthread_join(p, &r) || r != (void *)(intptr_t)4;
    bad |= pthread_join(c, NULL);
    bad |= inits != 1 || shared != 6 || spin_count != 6 || destructed != 30 || timeouts == 0;
    bad |= !pthread_equal(pthread_self(), pthread_self()) || pthread_equal(pthread_self(), p);
    ev(bad ? 200 : 201);
    char msg[96];
    snprintf(msg, sizeof msg, "pthreadtest: %s after %u frames, %u events, %d timeouts", bad ? "FAILED" : "ok",
             gasm_loop_frames(), nev, timeouts);
    gasm_log_str(msg);
    present();
    gasm_wait_frame();
    return bad;
}
