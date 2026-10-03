/*
  SDL3 on gasm: frames, time, entry points.

  gasm calls the guest once per frame (60 Hz) and samples input between calls.
  SDL's clock is virtual and moves with those frames, so runs depend only on
  their input and are identical on every runner.

  - SDL_MAIN_USE_CALLBACKS apps: one gasm_frame = pump events, SDL_AppEvent for
    each, SDL_AppIterate once.
  - Apps with their own loop (classic main) run on gasm_loop (Asyncify). A frame
    ends at a present (SDL_RenderPresent, SDL_UpdateWindowSurface), unless the
    app paces itself with SDL_Delay: a delay across a frame boundary ends the
    frame there, and the next present then doesn't end another one.

  SPDX-License-Identifier: Zlib
*/
#include "SDL_internal.h"

#include "SDL_gasm_c.h"
#include "main/SDL_main_callbacks.h"

#include <stdio.h>
#include <stdlib.h>
#include <time.h>

#include "gasm.h"

/* ---- frames and the virtual clock ----------------------------------------------- */

#define FRAME_RATE 60

static Uint64 now_ns;         /* SDL's clock */
static Uint64 frame;          /* gasm frames begun */
static bool delay_ended_frame;
static void (*wait_frame)(void);

/* with the threaded loop helper (gasm_thread.h) any thread can wait for a frame: SDL's
   frames are counted from the scheduler's then (gasm_loop.c; weak: callback apps don't
   link it) */
extern int gasm_loop_threads(void) __attribute__((weak));
extern unsigned gasm_loop_frames(void) __attribute__((weak));

static bool Threaded(void)
{
    return wait_frame && gasm_loop_threads && gasm_loop_threads();
}

static Uint64 FrameStart(Uint64 f)
{
    return f * SDL_NS_PER_SECOND / FRAME_RATE;
}

Uint64 SDL_GASM_Frame(void)
{
    return frame;
}

void SDL_GASM_BeginFrame(void)
{
    frame++;
    if (now_ns < FrameStart(frame - 1)) {
        now_ns = FrameStart(frame - 1);
    }
    SDL_GASM_AudioFrame();
}

/* Classic main(): main runs on gasm_loop and can wait for the next frame. The app
   passes gasm_wait_frame (SDL_main_impl_private.h), so only such apps link gasm_loop. */
void SDL_GASM_UseLoop(void (*wait)(void))
{
    wait_frame = wait;
}

/* Threaded: begin the frames the scheduler has begun, in whichever thread looks first. */
static void SyncFrames(void)
{
    if (Threaded()) {
        const Uint64 f = (Uint64)gasm_loop_frames() + 1;
        while (frame < f) {
            SDL_GASM_BeginFrame();
        }
    }
}

static void EndFrame(void)
{
    if (wait_frame) {
        wait_frame();
        if (Threaded()) {
            SyncFrames();
        } else {
            SDL_GASM_BeginFrame();
        }
    }
}

void SDL_GASM_Presented(void)
{
    if (!delay_ended_frame) {
        EndFrame();
    }
    delay_ended_frame = false;
}

static void DelayNS(Uint64 ns)
{
    SyncFrames();
    const Uint64 target = now_ns + ns;
    if (wait_frame) {
        while (FrameStart(frame) <= target) {
            delay_ended_frame = true;
            EndFrame();
        }
    }
    if (now_ns < target) {
        now_ns = target;
    }
}

/* ---- timer (SDL_timer.c) ---------------------------------------------------------- */

Uint64 SDL_GetPerformanceCounter(void)
{
    SyncFrames();
    return now_ns + 1;   /* never 0: SDL_InitTicks treats 0 as "not started" */
}

Uint64 SDL_GetPerformanceFrequency(void)
{
    return SDL_NS_PER_SECOND;
}

void SDL_SYS_DelayNS(Uint64 ns)
{
    DelayNS(ns);
}

/* ---- wall clock (SDL_time.c): virtual too, from 2026-01-01 00:00 UTC ----------------- */

#define EPOCH_2026 1767225600LL

void SDL_GetSystemTimeLocalePreferences(SDL_DateFormat *df, SDL_TimeFormat *tf)
{
    if (df) {
        *df = SDL_DATE_FORMAT_YYYYMMDD;
    }
    if (tf) {
        *tf = SDL_TIME_FORMAT_24HR;
    }
}

bool SDL_GetCurrentTime(SDL_Time *ticks)
{
    CHECK_PARAM(!ticks) {
        return SDL_InvalidParamError("ticks");
    }
    *ticks = SDL_SECONDS_TO_NS(EPOCH_2026) + (SDL_Time)now_ns;
    return true;
}

bool SDL_TimeToDateTime(SDL_Time ticks, SDL_DateTime *dt, bool localTime)
{
    CHECK_PARAM(!dt) {
        return SDL_InvalidParamError("dt");
    }
    (void)localTime;   /* gasm has no time zone: local time is UTC */
    const time_t tval = (time_t)SDL_NS_TO_SECONDS(ticks);
    struct tm tm;
    if (!gmtime_r(&tval, &tm)) {
        return SDL_SetError("SDL_DateTime conversion failed");
    }
    dt->year = tm.tm_year + 1900;
    dt->month = tm.tm_mon + 1;
    dt->day = tm.tm_mday;
    dt->hour = tm.tm_hour;
    dt->minute = tm.tm_min;
    dt->second = tm.tm_sec;
    dt->nanosecond = ticks % SDL_NS_PER_SECOND;
    dt->day_of_week = tm.tm_wday;
    dt->utc_offset = 0;
    return true;
}

/* ---- command line ------------------------------------------------------------------- */

int SDL_GASM_Args(char ***argv)
{
    static char buf[2048];
    static char *args[64];
    int argc = 0;
    args[argc++] = "app";
    if (gasm_param_str("args", buf, sizeof buf)) {
        for (char *save = NULL, *t = SDL_strtok_r(buf, " \t", &save); t && argc < 63; t = SDL_strtok_r(NULL, " \t", &save)) {
            args[argc++] = t;
        }
    }
    args[argc] = NULL;
    *argv = args;
    return argc;
}

/* ---- main callbacks (SDL_MAIN_USE_CALLBACKS) ---------------------------------------- */

static bool callbacks_running;

static void Finish(SDL_AppResult rc)
{
    callbacks_running = false;
    SDL_QuitMainCallbacks(rc);
    fflush(NULL);
    _Exit(rc == SDL_APP_FAILURE ? 1 : 0);   /* proc_exit: the runner ends the game */
}

/* Called by SDL_main (SDL_main_impl.h) from gasm_init; returns to the runner. */
int SDL_EnterAppMainCallbacks(int argc, char *argv[], SDL_AppInit_func appinit, SDL_AppIterate_func appiter, SDL_AppEvent_func appevent, SDL_AppQuit_func appquit)
{
    SDL_AppResult rc = SDL_InitMainCallbacks(argc, argv, appinit, appiter, appevent, appquit);
    if (rc != SDL_APP_CONTINUE) {
        Finish(rc);
    }
    callbacks_running = true;
    return 0;
}

int SDL_GASM_CallbacksFrame(void)
{
    if (!callbacks_running) {
        return 0;
    }
    SDL_GASM_BeginFrame();
    SDL_AppResult rc = SDL_IterateMainCallbacks(true);
    if (rc != SDL_APP_CONTINUE) {
        Finish(rc);
    }
    return 0;
}

/* The player quits: let the app save in SDL_AppQuit. */
void SDL_GASM_CallbacksExit(void)
{
    if (callbacks_running) {
        callbacks_running = false;
        SDL_QuitMainCallbacks(SDL_APP_SUCCESS);
    }
}
