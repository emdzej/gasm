/*
  SDL3 on gasm: video driver ("gasm") and input events.

  One window is shown: its framebuffer (RGBA, the window's size) goes to
  gasm_video_present. SDL_Renderer draws in software into it. Input is read once
  per gasm frame: raw keys (W3C codes -> scancodes), typed text while text input
  is on, the pointer in frame pixels (= window pixels), relative motion in
  relative mouse mode, the wheel.

  SPDX-License-Identifier: Zlib
*/
#include "SDL_internal.h"

#ifdef SDL_VIDEO_DRIVER_PRIVATE

#include "SDL_properties_c.h"
#include "video/SDL_sysvideo.h"
#include "video/SDL_pixels_c.h"
#include "events/SDL_events_c.h"
#include "events/SDL_keyboard_c.h"
#include "events/SDL_mouse_c.h"
#include "events/SDL_windowevents_c.h"

#include "SDL_gasm_c.h"
#include "gasm.h"

#define GASM_SURFACE "SDL.internal.window.surface"
#define DISPLAY_W 1280
#define DISPLAY_H 720

#include "SDL_gasmkeys.h"

static SDL_Window *shown;          /* the window that is presented (the first one) */
static Uint64 input_frame;         /* the frame whose input was sent */
static Uint32 buttons_sent;
static float last_x = -1, last_y = -1;
static bool cursor_hidden, relative_mode;

void SDL_GASM_UpdateInputMode(void)
{
    gasm_input_mode(GASM_INPUT_KEYS_RAW | (cursor_hidden ? GASM_INPUT_POINTER_HIDDEN : 0) |
                    (relative_mode ? GASM_INPUT_POINTER_LOCKED : 0));
}

/* ---- input ------------------------------------------------------------------------ */

static float F32(const Uint8 *p)
{
    float f;
    SDL_memcpy(&f, p, 4);
    return f;
}

static Uint32 U32(const Uint8 *p)
{
    return (Uint32)p[0] | (Uint32)p[1] << 8 | (Uint32)p[2] << 16 | (Uint32)p[3] << 24;
}

static void PumpKeys(Uint64 ts)
{
    Uint8 small[512];
    Uint8 *ev = small;
    int n = gasm_key_events(small, sizeof small);
    if (n > (int)sizeof small) {
        ev = (Uint8 *)SDL_malloc((size_t)n);
        if (!ev) {
            return;
        }
        n = gasm_key_events(ev, (Uint32)n);
    }
    for (int i = 0; i + 4 <= n; i += 4) {
        const unsigned code = ev[i] | ev[i + 1] << 8;
        const SDL_Scancode sc = code < SDL_arraysize(gasm_scancodes) ? gasm_scancodes[code] : SDL_SCANCODE_UNKNOWN;
        SDL_SendKeyboardKey(ts, SDL_DEFAULT_KEYBOARD_ID, (int)code, sc, ev[i + 2] != 0);
    }
    if (ev != small) {
        SDL_free(ev);
    }
}

/* Typed text, without the \b and \n that gasm includes (SDL apps get those as keys). */
static void PumpText(void)
{
    char text[256], out[256];
    if (!shown || !SDL_TextInputActive(shown)) {
        return;
    }
    const int n = gasm_text_input(text, sizeof text - 1);
    if (n <= 0 || n >= (int)sizeof text) {
        return;
    }
    int m = 0;
    for (int i = 0; i < n; i++) {
        if ((Uint8)text[i] >= 0x20 && text[i] != 0x7f) {
            out[m++] = text[i];
        }
    }
    out[m] = 0;
    if (m) {
        SDL_SendKeyboardText(out);
    }
}

static void PumpPointer(Uint64 ts)
{
    static const struct { Uint32 gasm; Uint8 sdl; } map[] = {
        { GASM_MOUSE_LEFT, SDL_BUTTON_LEFT }, { GASM_MOUSE_MIDDLE, SDL_BUTTON_MIDDLE },
        { GASM_MOUSE_RIGHT, SDL_BUTTON_RIGHT }, { GASM_MOUSE_BACK, SDL_BUTTON_X1 },
        { GASM_MOUSE_FORWARD, SDL_BUTTON_X2 },
    };
    Uint8 p[GASM_POINTER_BYTES];
    if (!shown || gasm_pointer(p, sizeof p) != GASM_POINTER_BYTES) {
        return;
    }
    const float fx = F32(p + GASM_POINTER_OFF_FX), fy = F32(p + GASM_POINTER_OFF_FY);
    const float dx = F32(p + GASM_POINTER_OFF_DX), dy = F32(p + GASM_POINTER_OFF_DY);
    const float wx = F32(p + GASM_POINTER_OFF_WHEEL_X), wy = F32(p + GASM_POINTER_OFF_WHEEL_Y);
    const Uint32 held = U32(p + GASM_POINTER_OFF_BUTTONS), pressed = U32(p + GASM_POINTER_OFF_PRESSED);
    const Uint32 released = U32(p + GASM_POINTER_OFF_RELEASED), flags = U32(p + GASM_POINTER_OFF_FLAGS);

    if (relative_mode) {
        if (dx != 0.0f || dy != 0.0f) {
            SDL_SendMouseMotion(ts, shown, SDL_DEFAULT_MOUSE_ID, true, dx, dy);
        }
    } else if ((flags & GASM_POINTER_INSIDE) && (fx != last_x || fy != last_y)) {
        SDL_SendMouseMotion(ts, shown, SDL_DEFAULT_MOUSE_ID, false, fx, fy);
        last_x = fx;
        last_y = fy;
    }
    for (size_t i = 0; i < SDL_arraysize(map); i++) {
        const Uint32 b = map[i].gasm;
        if ((pressed & b) && (released & b)) {   /* a click within one frame */
            if (!(buttons_sent & b)) {
                SDL_SendMouseButton(ts, shown, SDL_DEFAULT_MOUSE_ID, map[i].sdl, true);
            }
            SDL_SendMouseButton(ts, shown, SDL_DEFAULT_MOUSE_ID, map[i].sdl, false);
            buttons_sent &= ~b;
        }
        if ((held & b) != (buttons_sent & b)) {
            SDL_SendMouseButton(ts, shown, SDL_DEFAULT_MOUSE_ID, map[i].sdl, (held & b) != 0);
            buttons_sent ^= b;
        }
    }
    if (wx != 0.0f || wy != 0.0f) {   /* gasm: y > 0 = down; SDL: y > 0 = away from the user */
        SDL_SendMouseWheel(ts, shown, SDL_DEFAULT_MOUSE_ID, wx, -wy, SDL_MOUSEWHEEL_NORMAL);
    }
}

static void GASM_PumpEvents(SDL_VideoDevice *_this)
{
    (void)_this;
    if (input_frame == SDL_GASM_Frame()) {
        return;   /* input is per frame */
    }
    input_frame = SDL_GASM_Frame();
    const Uint64 ts = SDL_GetTicksNS();
    PumpKeys(ts);
    PumpText();
    PumpPointer(ts);
}

/* ---- cursor ------------------------------------------------------------------------- */

static SDL_Cursor *GASM_CreateCursor(SDL_Surface *surface, int hot_x, int hot_y)
{
    (void)surface; (void)hot_x; (void)hot_y;
    return (SDL_Cursor *)SDL_calloc(1, sizeof(SDL_Cursor));   /* the runner shows its own cursor */
}

static SDL_Cursor *GASM_CreateSystemCursor(SDL_SystemCursor id)
{
    (void)id;
    return (SDL_Cursor *)SDL_calloc(1, sizeof(SDL_Cursor));
}

static void GASM_FreeCursor(SDL_Cursor *cursor)
{
    SDL_free(cursor);
}

static bool GASM_ShowCursor(SDL_Cursor *cursor)
{
    cursor_hidden = cursor == NULL;
    SDL_GASM_UpdateInputMode();
    return true;
}

static bool GASM_SetRelativeMouseMode(bool enabled)
{
    relative_mode = enabled;
    SDL_GASM_UpdateInputMode();
    return true;
}

/* ---- windows and the framebuffer --------------------------------------------------------- */

static bool GASM_CreateWindow(SDL_VideoDevice *_this, SDL_Window *window, SDL_PropertiesID props)
{
    (void)_this; (void)props;
    if (!shown) {
        shown = window;
    }
    SDL_SetKeyboardFocus(window);
    SDL_SetMouseFocus(window);
    return true;
}

static void GASM_DestroyWindow(SDL_VideoDevice *_this, SDL_Window *window)
{
    (void)_this;
    if (window == shown) {
        shown = NULL;
    }
}

static void GASM_SetWindowSize(SDL_VideoDevice *_this, SDL_Window *window)
{
    (void)_this;
    SDL_SendWindowEvent(window, SDL_EVENT_WINDOW_RESIZED, window->pending.w, window->pending.h);
}

static bool GASM_SetWindowPosition(SDL_VideoDevice *_this, SDL_Window *window)
{
    (void)_this;
    SDL_SendWindowEvent(window, SDL_EVENT_WINDOW_MOVED, window->pending.x, window->pending.y);
    return true;
}

static SDL_FullscreenResult GASM_SetWindowFullscreen(SDL_VideoDevice *_this, SDL_Window *window, SDL_VideoDisplay *display, SDL_FullscreenOp fullscreen)
{
    (void)_this; (void)window; (void)display; (void)fullscreen;
    return SDL_FULLSCREEN_SUCCEEDED;   /* the runner decides how the frame is shown */
}

static bool GASM_CreateWindowFramebuffer(SDL_VideoDevice *_this, SDL_Window *window, SDL_PixelFormat *format, void **pixels, int *pitch)
{
    (void)_this;
    int w, h;
    SDL_GetWindowSizeInPixels(window, &w, &h);
    SDL_Surface *surface = SDL_CreateSurface(w, h, SDL_PIXELFORMAT_RGBA32);   /* bytes R, G, B, A */
    if (!surface) {
        return false;
    }
    SDL_SetSurfaceProperty(SDL_GetWindowProperties(window), GASM_SURFACE, surface);
    *format = SDL_PIXELFORMAT_RGBA32;
    *pixels = surface->pixels;
    *pitch = surface->pitch;
    return true;
}

static bool GASM_UpdateWindowFramebuffer(SDL_VideoDevice *_this, SDL_Window *window, const SDL_Rect *rects, int numrects)
{
    (void)_this; (void)rects; (void)numrects;
    SDL_Surface *surface = (SDL_Surface *)SDL_GetPointerProperty(SDL_GetWindowProperties(window), GASM_SURFACE, NULL);
    if (!surface) {
        return SDL_SetError("Couldn't find the gasm surface for window");
    }
    if (window != shown) {
        return true;
    }
    /* the frame is opaque, whatever was drawn into the alpha channel */
    for (int y = 0; y < surface->h; y++) {
        Uint32 *row = (Uint32 *)((Uint8 *)surface->pixels + y * surface->pitch);   /* RGBA bytes: A is the top byte */
        for (int x = 0; x < surface->w; x++) {
            row[x] |= 0xff000000u;
        }
    }
    gasm_video_present(surface->pixels, (Uint32)surface->w, (Uint32)surface->h, (Uint32)surface->pitch);
    SDL_GASM_Presented();
    return true;
}

static void GASM_DestroyWindowFramebuffer(SDL_VideoDevice *_this, SDL_Window *window)
{
    (void)_this;
    SDL_ClearProperty(SDL_GetWindowProperties(window), GASM_SURFACE);
}

/* ---- device ---------------------------------------------------------------------------- */

static bool GASM_VideoInit(SDL_VideoDevice *_this)
{
    (void)_this;
    SDL_DisplayMode mode;
    SDL_zero(mode);
    mode.format = SDL_PIXELFORMAT_XBGR8888;
    mode.w = DISPLAY_W;
    mode.h = DISPLAY_H;
    mode.refresh_rate = 60.0f;
    if (SDL_AddBasicVideoDisplay(&mode) == 0) {
        return false;
    }
    SDL_AddKeyboard(SDL_DEFAULT_KEYBOARD_ID, "gasm keyboard");
    SDL_AddMouse(SDL_DEFAULT_MOUSE_ID, "gasm pointer");

    SDL_Mouse *mouse = SDL_GetMouse();
    mouse->CreateCursor = GASM_CreateCursor;
    mouse->CreateSystemCursor = GASM_CreateSystemCursor;
    mouse->ShowCursor = GASM_ShowCursor;
    mouse->FreeCursor = GASM_FreeCursor;
    mouse->SetRelativeMouseMode = GASM_SetRelativeMouseMode;
    SDL_SetDefaultCursor(GASM_CreateCursor(NULL, 0, 0));

    SDL_GASM_UpdateInputMode();
    return true;
}

static void GASM_VideoQuit(SDL_VideoDevice *_this)
{
    (void)_this;
}

static void GASM_DeleteDevice(SDL_VideoDevice *device)
{
    SDL_free(device);
}

static SDL_VideoDevice *GASM_CreateDevice(void)
{
    SDL_VideoDevice *device = (SDL_VideoDevice *)SDL_calloc(1, sizeof(SDL_VideoDevice));
    if (!device) {
        return NULL;
    }
    device->VideoInit = GASM_VideoInit;
    device->VideoQuit = GASM_VideoQuit;
    device->PumpEvents = GASM_PumpEvents;
    device->CreateSDLWindow = GASM_CreateWindow;
    device->DestroyWindow = GASM_DestroyWindow;
    device->SetWindowSize = GASM_SetWindowSize;
    device->SetWindowPosition = GASM_SetWindowPosition;
    device->SetWindowFullscreen = GASM_SetWindowFullscreen;
    device->CreateWindowFramebuffer = GASM_CreateWindowFramebuffer;
    device->UpdateWindowFramebuffer = GASM_UpdateWindowFramebuffer;
    device->DestroyWindowFramebuffer = GASM_DestroyWindowFramebuffer;
    device->free = GASM_DeleteDevice;
    return device;
}

/* No dialogs: message boxes go to the log and take the default button. */
static bool GASM_ShowMessageBox(const SDL_MessageBoxData *data, int *buttonID)
{
    SDL_Log("%s: %s", data->title ? data->title : "", data->message ? data->message : "");
    if (buttonID) {
        *buttonID = data->numbuttons > 0 ? data->buttons[0].buttonID : -1;
        for (int i = 0; i < data->numbuttons; i++) {
            if (data->buttons[i].flags & SDL_MESSAGEBOX_BUTTON_RETURNKEY_DEFAULT) {
                *buttonID = data->buttons[i].buttonID;
            }
        }
    }
    return true;
}

VideoBootStrap PRIVATE_bootstrap = {
    "gasm", "gasm video driver",
    GASM_CreateDevice,
    GASM_ShowMessageBox,
    false
};

#endif /* SDL_VIDEO_DRIVER_PRIVATE */
