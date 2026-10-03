/*
 * gasm_pthread.c: POSIX threads on gasm's cooperative threads (gasm_thread.h,
 * design/threads.md), so pthread code builds unchanged against wasi-libc's
 * headers. Link it with gasm_thread.c and gasm_loop.c built with
 * -DGASM_LOOP_THREADS (CMake: gasm_add_game(<target> LOOP THREADS ...) adds it).
 *
 * wasi-libc (wasm32-wasip1) has single-thread pthread stubs; these definitions
 * replace them (the stubs are weak, or alone in their archive members). The
 * objects keep musl's layouts: pthread_mutex_init, the attribute functions and
 * pthread_cond_init stay wasi-libc's and set the fields read here.
 *
 * Covered: pthread_create/join/detach/self/equal, mutexes (normal, recursive,
 * error-checking; timedlock), conditions (timedwait on the condition's clock),
 * read/write locks, pthread_once, keys, POSIX semaphores, spinlocks (they
 * yield), sched_yield, nanosleep/usleep/sleep (on the frame's time).
 * Not covered: pthread_exit and cancellation, process-shared objects, C11
 * <threads.h> (musl calls its internal pthread functions there).
 *
 * SPDX-License-Identifier: MIT
 */
#include <errno.h>
#include <limits.h>
#include <pthread.h>
#include <sched.h>
#include <semaphore.h>
#include <stdint.h>
#include <stdlib.h>
#include <time.h>
#include <unistd.h>

#include "gasm.h"
#include "gasm_thread.h"

#define FOREVER GASM_FOREVER

/* ---- time ---------------------------------------------------------------------------------- */

/* An absolute time on `clock` as a deadline in gasm_time_ms() (the frame's time). */
static double deadline(clockid_t clock, const struct timespec *abs) {
    if (!abs)
        return FOREVER;
    struct timespec now;
    if (clock_gettime(clock, &now) != 0)
        clock_gettime(CLOCK_REALTIME, &now);
    double left = (double)(abs->tv_sec - now.tv_sec) * 1000.0 + (double)(abs->tv_nsec - now.tv_nsec) / 1e6;
    return gasm_time_ms() + (left > 0 ? left : 0);
}

/* ---- threads ------------------------------------------------------------------------------- */

struct gasm_pthread {
    gasm_thread *t;
    void *(*fn)(void *);
    void *arg;
    void *ret;
};

static struct gasm_pthread main_pthread;
static int self_key = -1;

static struct gasm_pthread *self_of_current(void) {
    struct gasm_pthread *p = self_key >= 0 ? (struct gasm_pthread *)gasm_thread_get(self_key) : NULL;
    return p ? p : &main_pthread;
}

static int trampoline(void *arg) {
    struct gasm_pthread *p = (struct gasm_pthread *)arg;
    gasm_thread_set(self_key, p);
    p->ret = p->fn(p->arg);
    return 0;
}

int pthread_create(pthread_t *restrict thread, const pthread_attr_t *restrict attr, void *(*fn)(void *), void *restrict arg) {
    if (self_key < 0 && (self_key = gasm_thread_key_create(NULL)) < 0)
        return EAGAIN;
    size_t stack = 0;
    if (attr)
        pthread_attr_getstacksize(attr, &stack);
    struct gasm_pthread *p = (struct gasm_pthread *)calloc(1, sizeof *p);
    if (!p)
        return EAGAIN;
    p->fn = fn;
    p->arg = arg;
    p->t = gasm_thread_create(trampoline, p, stack);
    if (!p->t) {
        free(p);
        return EAGAIN;
    }
    int detached = 0;
    if (attr && pthread_attr_getdetachstate(attr, &detached) == 0 && detached == PTHREAD_CREATE_DETACHED) {
        gasm_thread_detach(p->t);
        p->t = NULL;   /* the struct leaks with it: detached threads can't be joined */
    }
    *thread = (pthread_t)p;
    return 0;
}

int pthread_join(pthread_t thread, void **ret) {
    struct gasm_pthread *p = (struct gasm_pthread *)thread;
    if (!p || p == &main_pthread || !p->t)
        return EINVAL;
    if (p == self_of_current())
        return EDEADLK;
    gasm_thread_join(p->t);
    if (ret)
        *ret = p->ret;
    free(p);
    return 0;
}

int pthread_detach(pthread_t thread) {
    struct gasm_pthread *p = (struct gasm_pthread *)thread;
    if (!p || p == &main_pthread || !p->t)
        return EINVAL;
    gasm_thread_detach(p->t);
    p->t = NULL;
    return 0;
}

pthread_t pthread_self(void) { return (pthread_t)self_of_current(); }
#undef pthread_equal   /* a macro in musl's header; the function is for callers that take its address */
int pthread_equal(pthread_t a, pthread_t b) { return a == b; }

int sched_yield(void) {
    gasm_thread_yield();
    return 0;
}

int nanosleep(const struct timespec *req, struct timespec *rem) {
    if (!req || req->tv_nsec < 0 || req->tv_nsec >= 1000000000L) {
        errno = EINVAL;
        return -1;
    }
    gasm_thread_sleep_ms((double)req->tv_sec * 1000.0 + (double)req->tv_nsec / 1e6);
    if (rem)
        rem->tv_sec = rem->tv_nsec = 0;
    return 0;
}

int usleep(useconds_t us) {
    gasm_thread_sleep_ms(us / 1000.0);
    return 0;
}

unsigned sleep(unsigned seconds) {
    gasm_thread_sleep_ms(seconds * 1000.0);
    return 0;
}

/* ---- mutexes: musl's fields (_m_type = __i[0]); owner and count kept in __p[1], __i[5] ----- */

#define M_TYPE(m) ((m)->__u.__i[0] & 3)
#define M_OWNER(m) ((m)->__u.__p[1])
#define M_COUNT(m) ((m)->__u.__i[5])

static int mutex_lock(pthread_mutex_t *m, double until) {
    gasm_thread *self = gasm_thread_self();
    if (M_OWNER(m) == self) {
        if (M_TYPE(m) == PTHREAD_MUTEX_RECURSIVE) {
            M_COUNT(m)++;
            return 0;
        }
        if (M_TYPE(m) == PTHREAD_MUTEX_ERRORCHECK)
            return EDEADLK;
    }
    while (M_OWNER(m))
        if (gasm__thread_block(m, until))
            return ETIMEDOUT;
    M_OWNER(m) = self;
    M_COUNT(m) = 1;
    return 0;
}

int pthread_mutex_lock(pthread_mutex_t *m) { return mutex_lock(m, FOREVER); }

int pthread_mutex_timedlock(pthread_mutex_t *restrict m, const struct timespec *restrict at) {
    return mutex_lock(m, deadline(CLOCK_REALTIME, at));
}

int pthread_mutex_trylock(pthread_mutex_t *m) {
    gasm_thread *self = gasm_thread_self();
    if (M_OWNER(m) == self && M_TYPE(m) == PTHREAD_MUTEX_RECURSIVE) {
        M_COUNT(m)++;
        return 0;
    }
    if (M_OWNER(m))
        return EBUSY;
    M_OWNER(m) = self;
    M_COUNT(m) = 1;
    return 0;
}

int pthread_mutex_unlock(pthread_mutex_t *m) {
    if (M_OWNER(m) != gasm_thread_self())
        return EPERM;
    if (--M_COUNT(m) > 0)
        return 0;
    M_OWNER(m) = NULL;
    gasm__thread_wake(m, 1);
    return 0;
}

/* ---- conditions: musl's clock field (_c_clock = __i[4]) ---------------------------------- */

static int cond_wait(pthread_cond_t *c, pthread_mutex_t *m, double until) {
    if (M_OWNER(m) != gasm_thread_self())
        return EPERM;
    int depth = M_COUNT(m);
    M_COUNT(m) = 1;
    pthread_mutex_unlock(m);
    int timed_out = gasm__thread_block(c, until);
    mutex_lock(m, FOREVER);
    M_COUNT(m) = depth;
    return timed_out ? ETIMEDOUT : 0;
}

int pthread_cond_wait(pthread_cond_t *restrict c, pthread_mutex_t *restrict m) { return cond_wait(c, m, FOREVER); }

int pthread_cond_timedwait(pthread_cond_t *restrict c, pthread_mutex_t *restrict m, const struct timespec *restrict at) {
    return cond_wait(c, m, deadline((clockid_t)c->__u.__i[4], at));
}

int pthread_cond_signal(pthread_cond_t *c) {
    gasm__thread_wake(c, 1);
    return 0;
}

int pthread_cond_broadcast(pthread_cond_t *c) {
    gasm__thread_wake(c, 0);
    return 0;
}

/* ---- read/write locks: readers in __i[0], the writer in __p[1], its recursion in __i[2] ---- */

#define RW_READERS(l) ((l)->__u.__i[0])
#define RW_WRITER(l) ((l)->__u.__p[1])

static int rdlock(pthread_rwlock_t *l, double until) {
    while (RW_WRITER(l))
        if (gasm__thread_block(l, until))
            return ETIMEDOUT;
    RW_READERS(l)++;
    return 0;
}

static int wrlock(pthread_rwlock_t *l, double until) {
    if (RW_WRITER(l) == gasm_thread_self())
        return EDEADLK;
    while (RW_WRITER(l) || RW_READERS(l) > 0)
        if (gasm__thread_block(l, until))
            return ETIMEDOUT;
    RW_WRITER(l) = gasm_thread_self();
    return 0;
}

int pthread_rwlock_rdlock(pthread_rwlock_t *l) { return rdlock(l, FOREVER); }
int pthread_rwlock_wrlock(pthread_rwlock_t *l) { return wrlock(l, FOREVER); }
int pthread_rwlock_timedrdlock(pthread_rwlock_t *restrict l, const struct timespec *restrict at) {
    return rdlock(l, deadline(CLOCK_REALTIME, at));
}
int pthread_rwlock_timedwrlock(pthread_rwlock_t *restrict l, const struct timespec *restrict at) {
    return wrlock(l, deadline(CLOCK_REALTIME, at));
}
int pthread_rwlock_tryrdlock(pthread_rwlock_t *l) {
    if (RW_WRITER(l))
        return EBUSY;
    RW_READERS(l)++;
    return 0;
}
int pthread_rwlock_trywrlock(pthread_rwlock_t *l) {
    if (RW_WRITER(l) || RW_READERS(l) > 0)
        return EBUSY;
    RW_WRITER(l) = gasm_thread_self();
    return 0;
}
int pthread_rwlock_unlock(pthread_rwlock_t *l) {
    if (RW_WRITER(l) == gasm_thread_self())
        RW_WRITER(l) = NULL;
    else if (RW_READERS(l) > 0)
        RW_READERS(l)--;
    else
        return EPERM;
    gasm__thread_wake(l, 0);
    return 0;
}

/* ---- once, keys, spinlocks ------------------------------------------------------------------ */

int pthread_once(pthread_once_t *once, void (*init)(void)) {
    /* 0: not run, 1: running, 2: done */
    while (*once == 1)
        gasm__thread_block(once, FOREVER);
    if (*once == 0) {
        *once = 1;
        init();
        *once = 2;
        gasm__thread_wake(once, 0);
    }
    return 0;
}

int pthread_key_create(pthread_key_t *key, void (*destructor)(void *)) {
    int k = gasm_thread_key_create(destructor);
    if (k < 0)
        return EAGAIN;
    *key = (pthread_key_t)k;
    return 0;
}
int pthread_key_delete(pthread_key_t key) {
    gasm_thread_key_delete((int)key);
    return 0;
}
void *pthread_getspecific(pthread_key_t key) { return gasm_thread_get((int)key); }
int pthread_setspecific(pthread_key_t key, const void *value) {
    gasm_thread_set((int)key, (void *)value);
    return 0;
}

int pthread_spin_lock(pthread_spinlock_t *s) {
    while (*s)   /* held by a suspended thread: let it run */
        gasm_thread_yield();
    *s = 1;
    return 0;
}
int pthread_spin_trylock(pthread_spinlock_t *s) {
    if (*s)
        return EBUSY;
    *s = 1;
    return 0;
}
int pthread_spin_unlock(pthread_spinlock_t *s) {
    *s = 0;
    return 0;
}

/* ---- POSIX semaphores: the count in __val[0] --------------------------------------------- */

int sem_init(sem_t *s, int pshared, unsigned value) {
    if (pshared || value > SEM_VALUE_MAX) {
        errno = pshared ? ENOSYS : EINVAL;
        return -1;
    }
    s->__val[0] = (int)value;
    return 0;
}
int sem_destroy(sem_t *s) {
    (void)s;
    return 0;
}
static int sem_take(sem_t *s, double until) {
    while (s->__val[0] == 0)
        if (gasm__thread_block(s, until) && s->__val[0] == 0) {
            errno = ETIMEDOUT;
            return -1;
        }
    s->__val[0]--;
    return 0;
}
int sem_wait(sem_t *s) { return sem_take(s, FOREVER); }
int sem_timedwait(sem_t *restrict s, const struct timespec *restrict at) { return sem_take(s, deadline(CLOCK_REALTIME, at)); }
int sem_trywait(sem_t *s) {
    if (s->__val[0] == 0) {
        errno = EAGAIN;
        return -1;
    }
    s->__val[0]--;
    return 0;
}
int sem_post(sem_t *s) {
    s->__val[0]++;
    gasm__thread_wake(s, 1);
    return 0;
}
int sem_getvalue(sem_t *restrict s, int *restrict value) {
    *value = s->__val[0];
    return 0;
}
