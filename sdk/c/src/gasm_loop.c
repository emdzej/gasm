/*
 * gasm_loop.c: the gasm exports for games with their own main loop (gasm_loop.h).
 *
 * Two ways to suspend gasm_main() between frames, from the same object:
 * - gasm_run (runners with stack switching, design/stack-switching.md): the
 *   runner calls it once and suspends the guest in the gasm.yield_frame import.
 *   Needs no Asyncify: a "run build" skips the wasm-opt --asyncify pass.
 * - gasm_frame (every runner): Asyncify, below. Needs the --asyncify pass.
 *
 * The first gasm_frame() starts gasm_main(). gasm_wait_frame() unwinds the wasm
 * stack (Binaryen Asyncify) back out of gasm_frame(); the next gasm_frame()
 * rewinds into it, and gasm_main() continues where it waited. The suspended stack
 * is kept in linear memory (async_stack), so the game's whole state stays in memory
 * between frames, as for any other gasm guest.
 *
 * What it takes to work (each was a bug without it):
 * - gasm_wait_frame() reaches the unwind through an indirect call, so Asyncify
 *   instruments its callers while wait_impl itself stays plain, like an import;
 * - wasm-opt runs --asyncify before -O, or the loop gets inlined into the frame;
 * - gasm_loop_frame (the export) is on the remove list and reaches the game only
 *   through the noinline run_main(), so an unwind never looks like a return;
 * - nothing instrumented runs between an unwind starting and asyncify_stop_unwind();
 * - exit() is wrapped (link with --wrap=exit): it would run static destructors,
 *   which may wait, while the module is being torn down.
 *
 * Built with -DGASM_LOOP_THREADS, the frame driver is a scheduler of
 * cooperative threads (gasm_thread.h): the main thread (gasm_main) and the
 * game's threads are each suspended the same way, each with its own Asyncify
 * buffer; threads other than main also have their own C stack, so a switch
 * sets __stack_pointer too. Such builds don't export gasm_run (threads switch
 * with Asyncify), so runners call gasm_frame.
 *
 * C stacks and Asyncify: Asyncify leaves __stack_pointer alone both ways (an
 * unwind doesn't run the functions' epilogues, a rewind doesn't run their
 * prologues). So a suspended thread's C frames are everything above the stack
 * pointer it had when it unwound: the scheduler records it then, sets it back
 * before rewinding the thread, and runs its own calls on a stack of its own so
 * nothing is written below a suspended thread's frames meanwhile (checked by
 * guests/threadtest).
 *
 * SPDX-License-Identifier: MIT
 */
#include <stdio.h>
#include <stdlib.h>

#include "gasm.h"
#include "gasm_loop.h"

/* Also compiled as C++ (ScummVM's backend copies it): keep C names for the exports,
 * the remove list (gasm_loop_frame) and --wrap=exit (__wrap_exit). */
#ifdef __cplusplus
extern "C" {
#endif

#ifndef GASM_LOOP_STACK_SIZE
#define GASM_LOOP_STACK_SIZE (1024 * 1024)   /* saved locals of the suspended call stack */
#endif

/* Asyncify's own functions, called from inside the module (wasm-opt wires them up). */
__attribute__((import_module("asyncify"), import_name("start_unwind"))) void asyncify_start_unwind(void *data);
__attribute__((import_module("asyncify"), import_name("stop_unwind"))) void asyncify_stop_unwind(void);
__attribute__((import_module("asyncify"), import_name("start_rewind"))) void asyncify_start_rewind(void *data);
__attribute__((import_module("asyncify"), import_name("stop_rewind"))) void asyncify_stop_rewind(void);

static struct { void *cur, *end; } async_data;
static char async_stack[GASM_LOOP_STACK_SIZE];
static int rewinding, unwinding, started, finished, exiting, exit_code;

__attribute__((weak)) int gasm_loop_init(void) { return 0; }
__attribute__((weak)) void gasm_loop_exit(void) {}
static unsigned frames;

#ifndef GASM_LOOP_THREADS
static int run_mode;   /* entered through gasm_run: the runner switches stacks */

static void wait_impl(void) {
    if (exiting)
        return;
    if (run_mode) {   /* the runner suspends us until the next frame */
        gasm_yield_frame();
        frames++;
        return;
    }
    if (rewinding) {   /* back from the runner: continue where the game waited */
        asyncify_stop_rewind();
        rewinding = 0;
        frames++;
        return;
    }
    async_data.cur = async_stack;
    async_data.end = async_stack + sizeof async_stack;
    unwinding = 1;
    asyncify_start_unwind(&async_data);
}

static void (*volatile wait_ptr)(void) = wait_impl;

void gasm_wait_frame(void) {
    wait_ptr();
}

unsigned gasm_loop_frames(void) {
    return frames;
}

/* Not inlined: it must be instrumented, unlike the export, so an unwind returns
 * from here at once instead of looking like gasm_main() ended. */
__attribute__((noinline)) static void run_main(void) {
    int rc = gasm_main();
    if (!unwinding) {
        exit_code = rc;
        finished = 1;
    }
}

GASM_EXPORT("gasm_frame") void gasm_loop_frame(void) {
    if (finished)
        return;
    if (!started) {
        started = 1;
        run_main();
    } else {
        rewinding = 1;
        asyncify_start_rewind(&async_data);
        run_main();
    }
    if (unwinding) {
        asyncify_stop_unwind();
        unwinding = 0;
        return;
    }
    if (finished) {
        fflush(NULL);
        _Exit(exit_code);   /* proc_exit: the runner ends the game */
    }
}

/* The whole run, for runners that switch stacks: frames end in gasm_yield_frame(). */
GASM_EXPORT("gasm_run") int32_t gasm_loop_run(void) {
    run_mode = started = 1;
    int rc = gasm_main();
    fflush(NULL);
    return rc;   /* the runner ends the game with it */
}

#else /* GASM_LOOP_THREADS: cooperative threads */

#include <errno.h>
#include <string.h>

#include "gasm_thread.h"

#ifndef GASM_THREAD_ASYNC_SIZE
#define GASM_THREAD_ASYNC_SIZE (256 * 1024)   /* a thread's Asyncify buffer */
#endif
#define GASM_THREAD_STACK (256 * 1024)        /* a thread's default C stack */

/* wasi-libc's C stack pointer (a wasm global), for switching stacks */
__asm__(
    ".globaltype __stack_pointer, i32\n"
    ".section .text.gasm__get_sp,\"\",@\n"
    ".hidden gasm__get_sp\n"
    ".globl gasm__get_sp\n"
    ".type gasm__get_sp,@function\n"
    "gasm__get_sp:\n"
    "  .functype gasm__get_sp () -> (i32)\n"
    "  global.get __stack_pointer\n"
    "  end_function\n"
    ".section .text.gasm__set_sp,\"\",@\n"
    ".hidden gasm__set_sp\n"
    ".globl gasm__set_sp\n"
    ".type gasm__set_sp,@function\n"
    "gasm__set_sp:\n"
    "  .functype gasm__set_sp (i32) -> ()\n"
    "  local.get 0\n"
    "  global.set __stack_pointer\n"
    "  end_function\n");
uintptr_t gasm__get_sp(void);
void gasm__set_sp(uintptr_t sp);

enum { READY, BLOCKED, WAIT_FRAME, DONE };

struct gasm_thread {
    uint32_t id;
    int state;
    int (*fn)(void *);
    void *arg;
    int result;
    int started, detached, timed_out;
    uintptr_t sp;         /* its __stack_pointer where it unwound (suspended) */
    const void *on;       /* BLOCKED: what it waits for (NULL: only a deadline) */
    double deadline;      /* BLOCKED: gasm_time_ms() when it times out, or GASM_FOREVER */
    char *stack;          /* its C stack (NULL for main: the module's own stack) */
    size_t stack_size;
    struct { void *cur, *end; } adata;
    char *abuf;
    size_t abuf_size;
    int err;              /* its errno while suspended */
    void *key[GASM_THREAD_KEYS];
    struct gasm_thread *next;   /* creation order */
};

static struct gasm_thread main_thread;
static struct gasm_thread *threads = &main_thread, *last_thread = &main_thread;
static struct gasm_thread *current = &main_thread, *cursor = &main_thread;
static uint32_t next_id = 1;
static unsigned char key_used[GASM_THREAD_KEYS];
static void (*key_dtor[GASM_THREAD_KEYS])(void *);
/* the C stack pointer main is started at (the first frame's) */
static uintptr_t main_base;
/* where the scheduler's own calls run (never below a suspended thread's base) */
#ifndef GASM_THREAD_SCHED_STACK
#define GASM_THREAD_SCHED_STACK (64 * 1024)
#endif
static char sched_stack[GASM_THREAD_SCHED_STACK] __attribute__((aligned(16)));
#define SCHED_TOP ((uintptr_t)(sched_stack + sizeof sched_stack))

/* Suspend the current thread (its state already set) to the scheduler; returns
 * when the scheduler resumes it. Reached through a pointer, like wait_impl. */
static void switch_impl(void) {
    struct gasm_thread *t = current;
    if (exiting)
        return;
    if (rewinding) {   /* resumed */
        asyncify_stop_rewind();
        rewinding = 0;
        errno = t->err;
        return;
    }
    t->err = errno;
    t->adata.cur = t->abuf;
    t->adata.end = t->abuf + t->abuf_size;
    unwinding = 1;
    asyncify_start_unwind(&t->adata);
}

static void (*volatile switch_ptr)(void) = switch_impl;

static void suspend(int state) {
    current->state = state;
    switch_ptr();
}

void gasm_wait_frame(void) { suspend(WAIT_FRAME); }
unsigned gasm_loop_frames(void) { return frames; }

gasm_thread *gasm_thread_self(void) { return current; }
uint32_t gasm_thread_id(gasm_thread *t) { return t ? t->id : 0; }
void gasm_thread_yield(void) { suspend(READY); }

int gasm__thread_block(const void *on, double deadline_ms) {
    current->on = on;
    current->deadline = deadline_ms;
    current->timed_out = 0;
    suspend(BLOCKED);
    return current->timed_out;
}

void gasm__thread_wake(const void *on, int one) {
    for (struct gasm_thread *t = threads; t; t = t->next)
        if (t->state == BLOCKED && t->on == on && on) {
            t->state = READY;
            if (one)
                return;
        }
}

void gasm_thread_sleep_ms(double ms) {
    if (ms <= 0)
        gasm_thread_yield();
    else
        gasm__thread_block(NULL, gasm_time_ms() + ms);
}

gasm_thread *gasm_thread_create(int (*fn)(void *), void *arg, size_t stack) {
    struct gasm_thread *t = (struct gasm_thread *)calloc(1, sizeof *t);
    if (!t)
        return NULL;
    t->stack_size = stack ? (stack + 15) & ~(size_t)15 : GASM_THREAD_STACK;
    t->stack = (char *)malloc(t->stack_size);
    t->abuf_size = GASM_THREAD_ASYNC_SIZE;
    t->abuf = (char *)malloc(t->abuf_size);
    if (!t->stack || !t->abuf) {
        free(t->stack);
        free(t->abuf);
        free(t);
        return NULL;
    }
    t->id = next_id++;
    t->fn = fn;
    t->arg = arg;
    t->state = READY;
    last_thread->next = t;
    last_thread = t;
    return t;
}

static void free_thread(struct gasm_thread *t) {
    struct gasm_thread *p = threads;
    while (p->next != t)
        p = p->next;
    p->next = t->next;
    if (last_thread == t)
        last_thread = p;
    if (cursor == t)
        cursor = p;
    free(t->stack);
    free(t->abuf);
    free(t);
}

int gasm_thread_join(gasm_thread *t) {
    if (!t || t == &main_thread || t == current)
        return 0;
    while (t->state != DONE)
        gasm__thread_block(t, GASM_FOREVER);
    int rc = t->result;
    free_thread(t);
    return rc;
}

void gasm_thread_detach(gasm_thread *t) {
    if (!t || t == &main_thread)
        return;
    if (t->state == DONE)
        free_thread(t);
    else
        t->detached = 1;
}

int gasm_thread_key_create(void (*destructor)(void *)) {
    for (int k = 0; k < GASM_THREAD_KEYS; k++)
        if (!key_used[k]) {
            key_used[k] = 1;
            key_dtor[k] = destructor;
            for (struct gasm_thread *t = threads; t; t = t->next)
                t->key[k] = NULL;
            return k;
        }
    return -1;
}
void gasm_thread_key_delete(int key) {
    if (key >= 0 && key < GASM_THREAD_KEYS)
        key_used[key] = 0;
}
void *gasm_thread_get(int key) { return key >= 0 && key < GASM_THREAD_KEYS ? current->key[key] : NULL; }
void gasm_thread_set(int key, void *value) {
    if (key >= 0 && key < GASM_THREAD_KEYS)
        current->key[key] = value;
}

/* A thread's body; instrumented (an unwind returns from here) and not inlined. */
__attribute__((noinline)) static void run_thread(struct gasm_thread *t) {
    int rc = t == &main_thread ? gasm_main() : t->fn(t->arg);
    if (unwinding)
        return;
    for (int k = 0; k < GASM_THREAD_KEYS; k++)   /* thread-local destructors, in the thread */
        if (key_used[k] && key_dtor[k] && t->key[k]) {
            void *v = t->key[k];
            t->key[k] = NULL;
            key_dtor[k](v);
        }
    if (unwinding)
        return;
    t->result = rc;
    t->state = DONE;
}

/* The next ready thread after the cursor, in creation order. */
static struct gasm_thread *pick(void) {
    struct gasm_thread *t = cursor;
    do {
        t = t->next ? t->next : threads;
        if (t->state == READY)
            return t;
    } while (t != cursor);
    return NULL;
}

/* One frame: run ready threads until all of them wait (for the next frame, a
 * deadline or each other). On the remove list: never unwinds. */
GASM_EXPORT("gasm_frame") void gasm_loop_frame(void) {
    if (finished)
        return;
    if (!started) {
        started = 1;
        main_base = gasm__get_sp();
        main_thread.abuf = async_stack;
        main_thread.abuf_size = sizeof async_stack;
        main_thread.state = READY;
    } else {
        frames++;
    }
    gasm__set_sp(SCHED_TOP);
    double now = gasm_time_ms();
    for (struct gasm_thread *t = threads; t; t = t->next) {
        if (t->state == WAIT_FRAME)
            t->state = READY;
        else if (t->state == BLOCKED && t->deadline >= 0 && now >= t->deadline) {
            t->state = READY;
            t->timed_out = 1;
        }
    }
    for (;;) {
        struct gasm_thread *t = pick();
        if (!t)
            break;
        cursor = current = t;
        /* a new thread starts at its base; a suspended one continues where it unwound */
        gasm__set_sp(t->started ? t->sp : t == &main_thread ? main_base : (uintptr_t)(t->stack + t->stack_size));
        if (t->started) {
            rewinding = 1;
            asyncify_start_rewind(&t->adata);
        }
        t->started = 1;
        run_thread(t);
        if (unwinding) {
            asyncify_stop_unwind();
            unwinding = 0;
            t->sp = gasm__get_sp();
        }
        gasm__set_sp(SCHED_TOP);
        if (t->state == DONE) {
            if (t == &main_thread) {
                exit_code = t->result;
                finished = 1;
                fflush(NULL);
                _Exit(exit_code);   /* proc_exit: the runner ends the game */
            }
            gasm__thread_wake(t, 0);   /* joiners */
            if (t->detached)
                free_thread(t);
        }
    }
    current = &main_thread;
    /* the frame ends; if nothing can ever run again, say who waits for what */
    int alive = 0;
    for (struct gasm_thread *t = threads; t; t = t->next)
        if (t->state == WAIT_FRAME || (t->state == BLOCKED && t->deadline >= 0))
            alive = 1;
    if (!alive) {
        char line[96];
        gasm_log_str("gasm_thread: deadlock: every thread is blocked");
        for (struct gasm_thread *t = threads; t; t = t->next)
            if (t->state == BLOCKED) {
                snprintf(line, sizeof line, "  thread %u waits on %p", (unsigned)t->id, t->on);
                gasm_log_str(line);
            }
        __builtin_trap();
    }
    /* the epilogue restores the runner's stack pointer */
}

#endif /* GASM_LOOP_THREADS */

/* exit() without static destructors (see above); output is flushed first. */
void __wrap_exit(int code) {
    exiting = 1;
    fflush(NULL);
    _Exit(code);
}

GASM_EXPORT("gasm_abi_version") int32_t gasm_loop_abi_version(void) { return GASM_ABI_VERSION; }

GASM_EXPORT("gasm_init") int32_t gasm_loop_init_export(void) { return gasm_loop_init(); }

GASM_EXPORT("gasm_exit") void gasm_loop_exit_export(void) { gasm_loop_exit(); }

#ifdef __cplusplus
}
#endif
