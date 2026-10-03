/*
 * SDL threads on gasm: a classic main() that uses SDL_CreateThread, mutexes,
 * conditions, a semaphore, SDL_AddTimer (SDL's timer thread) and SDL_SetTLS.
 * Threads are cooperative (design/threads.md), so the schedule and the output
 * are the same on every runner. Build: gasm_sdl3_app(<target> LOOP THREADS).
 *
 * Two workers add to a shared total under a mutex and signal main through a
 * condition; a timer posts a semaphore every 100 ms that main counts; each
 * thread keeps its own TLS value. Worker 1 sometimes holds a read/write lock
 * for writing across a delay: main reads the step counters under it, so it
 * never sees them mid-update. The window shows the counters as bars; the
 * app quits with 0 when everything added up (1 otherwise).
 *
 * SPDX-License-Identifier: Zlib
 */
#include <SDL3/SDL.h>
#include <SDL3/SDL_main.h>

#define W 160
#define H 120

static SDL_Mutex *lock;
static SDL_Condition *changed;
static SDL_Semaphore *ticks;
static SDL_TLSID tls;
static SDL_RWLock *rw;
static int total, steps[2], done_workers, torn;

static int SDLCALL worker(void *arg)
{
    const int id = (int)(intptr_t)arg;
    SDL_SetTLS(&tls, (void *)(intptr_t)(100 + id), NULL);
    for (int i = 0; i < 40; i++) {
        SDL_LockMutex(lock);
        total += id + 1;
        SDL_SignalCondition(changed);
        SDL_UnlockMutex(lock);
        SDL_LockRWLockForWriting(rw);
        steps[id]++;
        if (id == 1 && i % 10 == 0) {
            steps[1] += 1000;            /* half-done update, visible only to a torn read */
            SDL_Delay(30);
            steps[1] -= 1000;
        }
        SDL_UnlockRWLock(rw);
        SDL_Delay(id == 0 ? 20 : 35);   /* sleeps this thread; frames go on */
    }
    const int ok = SDL_GetTLS(&tls) == (void *)(intptr_t)(100 + id);
    SDL_LockMutex(lock);
    done_workers++;
    SDL_SignalCondition(changed);
    SDL_UnlockMutex(lock);
    return ok ? 40 : -1;
}

static Uint32 SDLCALL tick(void *data, SDL_TimerID id, Uint32 interval)
{
    (void)data;
    (void)id;
    SDL_SignalSemaphore(ticks);
    return interval;
}

static void bar(SDL_Surface *s, int y, int len, Uint32 color)
{
    SDL_Rect r = { 4, y, len < W - 8 ? len : W - 8, 10 };
    SDL_FillSurfaceRect(s, &r, color);
}

int main(int argc, char **argv)
{
    (void)argc;
    (void)argv;
    if (!SDL_Init(SDL_INIT_VIDEO)) {
        return 1;
    }
    SDL_Window *win = SDL_CreateWindow("SDL threads", W, H, 0);
    SDL_Surface *screen = SDL_GetWindowSurface(win);
    lock = SDL_CreateMutex();
    changed = SDL_CreateCondition();
    ticks = SDL_CreateSemaphore(0);
    rw = SDL_CreateRWLock();
    SDL_SetTLS(&tls, (void *)(intptr_t)1, NULL);

    SDL_Thread *t0 = SDL_CreateThread(worker, "worker 0", (void *)(intptr_t)0);
    SDL_Thread *t1 = SDL_CreateThread(worker, "worker 1", (void *)(intptr_t)1);
    SDL_TimerID timer = SDL_AddTimer(100, tick, NULL);
    if (!t0 || !t1 || !timer) {
        SDL_Log("threads: %s", SDL_GetError());
        return 1;
    }

    int timer_ticks = 0, frames = 0;
    for (;;) {
        /* wait (at most a frame) for the workers to change something */
        SDL_LockMutex(lock);
        SDL_WaitConditionTimeout(changed, lock, 16);
        const int finished = done_workers == 2;
        const int sum = total;
        SDL_UnlockMutex(lock);
        while (SDL_TryWaitSemaphore(ticks)) {
            timer_ticks++;
        }

        SDL_LockRWLockForReading(rw);
        const int s0 = steps[0], s1 = steps[1];
        SDL_UnlockRWLock(rw);
        torn |= s1 >= 1000;

        SDL_FillSurfaceRect(screen, NULL, SDL_MapSurfaceRGB(screen, 16, 18, 24));
        bar(screen, 10, s0 * 3, SDL_MapSurfaceRGB(screen, 240, 180, 60));
        bar(screen, 30, s1 * 3, SDL_MapSurfaceRGB(screen, 90, 200, 120));
        bar(screen, 50, sum, SDL_MapSurfaceRGB(screen, 120, 150, 250));
        bar(screen, 70, timer_ticks * 4, SDL_MapSurfaceRGB(screen, 230, 90, 90));
        SDL_UpdateWindowSurface(win);   /* ends the frame */
        frames++;
        if (finished) {
            break;
        }
    }

    int r0, r1;
    SDL_WaitThread(t0, &r0);
    SDL_WaitThread(t1, &r1);
    SDL_RemoveTimer(timer);
    const int ok = r0 == 40 && r1 == 40 && total == 40 * 1 + 40 * 2 && timer_ticks > 0 && !torn &&
                   SDL_GetTLS(&tls) == (void *)(intptr_t)1;
    SDL_Log("threads: %s: total %d, %d timer ticks, %d frames", ok ? "ok" : "FAILED", total, timer_ticks, frames);
    SDL_Quit();
    return ok ? 0 : 1;
}
