/*
  SDL3 on gasm: OpenGL ES on gasm:gl (GLES 3.0 with WebGL 2's rules: WebGL 2 in
  browsers, ANGLE natively, the null GL headless).

  SDL itself never imports gasm:gl, so games that don't use GL stay 2D games (a
  module importing gasm:gl is drawn by the runner's GL). GL is reached through
  gasm_gl_get_proc_address, a weak symbol: games that use GL link lib/gasm_gl.o
  (the C SDK's gasm_gl.c and gasm_gl_proc.c; SDL3::GL in CMake), and then
  SDL_GL_* and SDL's GLES 2 renderer work.

  One context, on the window the runner shows; its drawable is the runner's window
  or canvas, so GL windows take its size (and follow it: GASM_GL_CheckSize).

  SPDX-License-Identifier: Zlib
*/
#include "SDL_internal.h"

#include "video/SDL_sysvideo.h"
#include "events/SDL_windowevents_c.h"
#include "SDL_gasm_c.h"

typedef void (*GASMglproc)(void);
extern GASMglproc gasm_gl_get_proc_address(const char *name) __attribute__((weak));

static void (*gl_present)(void);
static Uint32 (*gl_width)(void);
static Uint32 (*gl_height)(void);
static int swap_interval = 1;
static int context_dummy;
static bool has_context;

bool GASM_GL_Available(void)
{
    return gasm_gl_get_proc_address != NULL;
}

bool GASM_GL_LoadLibrary(SDL_VideoDevice *_this, const char *path)
{
    (void)_this; (void)path;
    if (!gasm_gl_get_proc_address) {
        return SDL_SetError("OpenGL ES on gasm needs the game linked with gasm_gl.o (SDL3::GL in CMake)");
    }
    gl_present = (void (*)(void))gasm_gl_get_proc_address("gasm_gl_present");
    gl_width = (Uint32 (*)(void))gasm_gl_get_proc_address("gasm_gl_width");
    gl_height = (Uint32 (*)(void))gasm_gl_get_proc_address("gasm_gl_height");
    return true;
}

SDL_FunctionPointer GASM_GL_GetProcAddress(SDL_VideoDevice *_this, const char *proc)
{
    (void)_this;
    return gasm_gl_get_proc_address ? (SDL_FunctionPointer)gasm_gl_get_proc_address(proc) : NULL;
}

void GASM_GL_UnloadLibrary(SDL_VideoDevice *_this)
{
    (void)_this;
}

void GASM_GL_DefaultProfileConfig(SDL_VideoDevice *_this, int *mask, int *major, int *minor)
{
    (void)_this;
    *mask = SDL_GL_CONTEXT_PROFILE_ES;   /* what gasm:gl is */
    *major = 3;
    *minor = 0;
}

SDL_GLContext GASM_GL_CreateContext(SDL_VideoDevice *_this, SDL_Window *window)
{
    (void)window;
    const SDL_GLContext ctx = (SDL_GLContext)&context_dummy;
    if (!(_this->gl_config.profile_mask & SDL_GL_CONTEXT_PROFILE_ES)) {
        SDL_SetError("gasm has OpenGL ES 3.0 (gasm:gl), not desktop OpenGL: ask for SDL_GL_CONTEXT_PROFILE_ES");
        return NULL;
    }
    if (_this->gl_config.major_version > 3 || (_this->gl_config.major_version == 3 && _this->gl_config.minor_version > 0)) {
        SDL_SetError("gasm has OpenGL ES 3.0 (gasm:gl): ES %d.%d isn't available", _this->gl_config.major_version, _this->gl_config.minor_version);
        return NULL;
    }
    if (has_context) {
        SDL_SetError("gasm has one OpenGL ES context");
        return NULL;
    }
    has_context = true;
    return ctx;
}

bool GASM_GL_MakeCurrent(SDL_VideoDevice *_this, SDL_Window *window, SDL_GLContext context)
{
    (void)_this; (void)window; (void)context;
    return true;   /* the one context is always current */
}

bool GASM_GL_SetSwapInterval(SDL_VideoDevice *_this, int interval)
{
    (void)_this;
    swap_interval = interval;   /* frames come at the runner's rate either way */
    return true;
}

bool GASM_GL_GetSwapInterval(SDL_VideoDevice *_this, int *interval)
{
    (void)_this;
    *interval = swap_interval;
    return true;
}

bool GASM_GL_SwapWindow(SDL_VideoDevice *_this, SDL_Window *window)
{
    (void)_this; (void)window;
    if (gl_present) {
        gl_present();
    }
    SDL_GASM_Presented();   /* a classic main() loop: the frame ends here */
    return true;
}

bool GASM_GL_DestroyContext(SDL_VideoDevice *_this, SDL_GLContext context)
{
    (void)_this; (void)context;
    has_context = false;
    return true;
}

/* The drawable's size in pixels, or false before GL is loaded. */
bool GASM_GL_DrawableSize(int *w, int *h)
{
    if (!gl_width || !gl_height) {
        return false;
    }
    *w = (int)gl_width();
    *h = (int)gl_height();
    return *w > 0 && *h > 0;
}

/* A GL window is as large as the drawable: tell the app when that changes (a
   resized window or canvas), once per frame. */
void GASM_GL_CheckSize(SDL_Window *window)
{
    int w, h;
    if (window && (window->flags & SDL_WINDOW_OPENGL) && GASM_GL_DrawableSize(&w, &h) && (w != window->w || h != window->h)) {
        SDL_SendWindowEvent(window, SDL_EVENT_WINDOW_RESIZED, w, h);
    }
}
