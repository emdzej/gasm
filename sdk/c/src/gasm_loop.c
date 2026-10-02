/*
 * gasm_loop.c: the gasm exports for games with their own main loop (gasm_loop.h).
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
static unsigned frames;

static void wait_impl(void) {
    if (exiting)
        return;
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

__attribute__((weak)) int gasm_loop_init(void) { return 0; }
__attribute__((weak)) void gasm_loop_exit(void) {}

/* Not inlined: it must be instrumented, unlike the export, so an unwind returns
 * from here at once instead of looking like gasm_main() ended. */
__attribute__((noinline)) static void run_main(void) {
    int rc = gasm_main();
    if (!unwinding) {
        exit_code = rc;
        finished = 1;
    }
}

/* exit() without static destructors (see above); output is flushed first. */
void __wrap_exit(int code) {
    exiting = 1;
    fflush(NULL);
    _Exit(code);
}

GASM_EXPORT("gasm_abi_version") int32_t gasm_loop_abi_version(void) { return GASM_ABI_VERSION; }

GASM_EXPORT("gasm_init") int32_t gasm_loop_init_export(void) { return gasm_loop_init(); }

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

GASM_EXPORT("gasm_exit") void gasm_loop_exit_export(void) { gasm_loop_exit(); }

#ifdef __cplusplus
}
#endif
