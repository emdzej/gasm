/*
 * gasm_loop.h: keep an engine's own main loop on gasm.
 *
 * gasm runners call gasm_frame() once per frame and the guest returns (see the
 * ABI's "Lifecycle"). Engines built around a blocking loop (`while (running) {
 * update(); render(); sleep(); }`) can keep it: define gasm_main() instead of the
 * gasm exports, and call gasm_wait_frame() once per frame. gasm_loop.c provides
 * the exports and suspends gasm_main() between frames with Binaryen's Asyncify,
 * entirely inside the module (runners need nothing).
 *
 * Build: link gasm_loop.c, link with -Wl,--wrap=exit, then post-process:
 *   wasm-opt game.wasm --asyncify --pass-arg=asyncify-removelist@gasm_loop_frame -O2 -o game.wasm
 * CMake does all of it: gasm_add_game(<target> LOOP <sources...>).
 *
 * Rules for the game: call gasm_wait_frame() only from inside gasm_main()'s call
 * tree; don't call it from gasm_loop_init() or gasm_loop_exit().
 *
 * SPDX-License-Identifier: MIT
 */
#ifndef GASM_LOOP_H
#define GASM_LOOP_H

#ifdef __cplusplus
extern "C" {
#endif

/** The game's main loop, started on the first frame. Its return value is the exit code. */
int gasm_main(void);

/** Return to the runner; continues in the next frame. Input and time are sampled per frame. */
void gasm_wait_frame(void);

/** Number of frames waited so far (0 during the first frame). */
unsigned gasm_loop_frames(void);

/*
 * Optional hooks (weak defaults): gasm_loop_init() runs in gasm_init (return non-zero
 * to fail loading); gasm_loop_exit() runs when the player quits (gasm_exit): save
 * settings there. Neither may call gasm_wait_frame().
 */
int gasm_loop_init(void);
void gasm_loop_exit(void);

#ifdef __cplusplus
}
#endif

#endif
