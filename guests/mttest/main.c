/* mttest: real threads (wasi-threads, plain pthreads) on gasm.
 * Built for wasm32-wasip1-threads with an imported shared memory: gasm-run runs each
 * pthread on an OS thread (--threads N, default the CPU count in a window, 0 headless).
 * gasm_init does the same work four times, first on one thread, then on four, checks
 * the sums agree and logs both times; with no threads allowed (headless default) it
 * says so and does all four on the main thread. Then the module exits (code 0, or 1 if
 * the sums differ). The video frame shows the result, so hashes cover it.
 * --param trap=1: a worker thread traps, which must end the whole game. */
#include <pthread.h>
#include <stdint.h>
#include <stdio.h>
#include <stdlib.h>
#include "gasm.h"

#define JOBS 4
static uint64_t sums[JOBS];
static int trap_in_worker;

static uint64_t job(int i) {
	uint64_t s = 0x9e3779b97f4a7c15ull * (uint64_t)(i + 1);
	for (uint32_t k = 0; k < 400000000u; k++) s = (s ^ (s >> 29)) * 6364136223846793005ull + k;
	return s;
}
static void *worker(void *arg) {
	int i = (int)(intptr_t)arg;
	if (trap_in_worker && i == JOBS - 1) __builtin_trap();
	sums[i] = job(i);
	return 0;
}

GASM_EXPORT("gasm_abi_version") int32_t abi_version(void) { return GASM_ABI_VERSION; }

static uint8_t frame[64 * 64 * 4];
static int ok = -1, spawned = 0;

GASM_EXPORT("gasm_init") int gasm_init(void) {
	char v[4] = {0};
	trap_in_worker = gasm_param("trap", 4, v, sizeof v) > 0 && v[0] == '1';
	uint64_t serial[JOBS];
	double t0 = gasm_time_ms();
	for (int i = 0; i < JOBS; i++) serial[i] = job(i);
	double t1 = gasm_time_ms();
	pthread_t t[JOBS];
	for (int i = 0; i < JOBS; i++) {
		if (pthread_create(&t[i], 0, worker, (void *)(intptr_t)i) == 0) spawned++;
		else { sums[i] = job(i); t[i] = 0; } /* no thread allowed: do it here */
	}
	for (int i = 0; i < JOBS; i++) if (t[i]) pthread_join(t[i], 0);
	double t2 = gasm_time_ms();
	ok = 1;
	for (int i = 0; i < JOBS; i++) ok &= sums[i] == serial[i];
	printf("[mttest] %d threads; sums %s; one thread %.0f ms, %d threads %.0f ms\n", spawned, ok ? "agree" : "DIFFER", t1 - t0, spawned, t2 - t1);
	return 0;
}

GASM_EXPORT("gasm_frame") void gasm_frame(void) {
	for (int p = 0; p < 64 * 64; p++) {
		frame[p * 4 + 0] = ok ? 40 : 220;
		frame[p * 4 + 1] = ok ? 200 : 40;
		frame[p * 4 + 2] = (uint8_t)(spawned * 60);
		frame[p * 4 + 3] = 255;
	}
	gasm_video_present(frame, 64, 64, 64 * 4);
	static int n = 0;
	if (++n == 3) exit(ok ? 0 : 1);
}
