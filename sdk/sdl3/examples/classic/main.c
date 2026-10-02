/*
  An SDL3 program with its own main loop, unchanged for gasm: poll events, draw,
  present. Arrow keys move the square, Tab changes its colour, typed text shows
  at the bottom (Backspace deletes), Escape quits.
  The position is saved to the pref path (gasm:storage) on exit and loaded at start.

  SPDX-License-Identifier: MIT
*/
#include <SDL3/SDL.h>
#include <SDL3/SDL_main.h>

typedef struct { float x, y; Uint8 hue; } State;

static void load(const char *path, State *s)
{
    SDL_IOStream *io = SDL_IOFromFile(path, "rb");
    if (io) {
        if (SDL_ReadIO(io, s, sizeof *s) == sizeof *s) {
            SDL_Log("loaded %s: %.0f,%.0f", path, s->x, s->y);
        }
        SDL_CloseIO(io);
    }
}

static void save(const char *path, const State *s)
{
    SDL_IOStream *io = SDL_IOFromFile(path, "wb");
    if (io) {
        SDL_WriteIO(io, s, sizeof *s);
        SDL_CloseIO(io);
        SDL_Log("saved %s", path);
    }
}

int main(int argc, char *argv[])
{
    (void)argc; (void)argv;
    if (!SDL_Init(SDL_INIT_VIDEO)) {
        SDL_Log("SDL_Init: %s", SDL_GetError());
        return 1;
    }
    SDL_Window *window;
    SDL_Renderer *renderer;
    if (!SDL_CreateWindowAndRenderer("classic", 320, 240, 0, &window, &renderer)) {
        SDL_Log("window: %s", SDL_GetError());
        return 1;
    }
    char *pref = SDL_GetPrefPath("gasm", "classic");
    char path[256];
    SDL_snprintf(path, sizeof path, "%sstate.bin", pref ? pref : "");
    SDL_free(pref);

    SDL_StartTextInput(window);
    char text[40] = "";
    State s = { 150, 110, 0 };
    load(path, &s);
    Uint64 frames = 0;
    bool running = true;
    while (running) {
        SDL_Event e;
        while (SDL_PollEvent(&e)) {
            if (e.type == SDL_EVENT_QUIT || (e.type == SDL_EVENT_KEY_DOWN && e.key.key == SDLK_ESCAPE)) {
                running = false;
            } else if (e.type == SDL_EVENT_KEY_DOWN && e.key.key == SDLK_TAB) {
                s.hue += 40;
            } else if (e.type == SDL_EVENT_KEY_DOWN && e.key.key == SDLK_BACKSPACE && text[0]) {
                size_t n = SDL_strlen(text);
                while (n && (text[n - 1] & 0xc0) == 0x80) {   /* UTF-8 continuation bytes */
                    n--;
                }
                text[n ? n - 1 : 0] = 0;
            } else if (e.type == SDL_EVENT_TEXT_INPUT) {
                SDL_strlcat(text, e.text.text, sizeof text);
            }
        }
        const bool *keys = SDL_GetKeyboardState(NULL);
        s.x += (keys[SDL_SCANCODE_RIGHT] - keys[SDL_SCANCODE_LEFT]) * 2.0f;
        s.y += (keys[SDL_SCANCODE_DOWN] - keys[SDL_SCANCODE_UP]) * 2.0f;

        SDL_SetRenderDrawColor(renderer, 16, 24, 48, 255);
        SDL_RenderClear(renderer);
        SDL_FRect r = { s.x, s.y, 20, 20 };
        SDL_SetRenderDrawColor(renderer, s.hue, (Uint8)(255 - s.hue), 160, 255);
        SDL_RenderFillRect(renderer, &r);
        SDL_SetRenderDrawColor(renderer, 255, 255, 255, 255);
        SDL_RenderDebugTextFormat(renderer, 4, 4, "frame %llu  t=%llu ms", (unsigned long long)frames++, (unsigned long long)SDL_GetTicks());
        SDL_RenderDebugTextFormat(renderer, 4, 228, "> %s", text);
        SDL_RenderPresent(renderer);
        SDL_Delay(1);
    }
    save(path, &s);
    SDL_DestroyRenderer(renderer);
    SDL_DestroyWindow(window);
    SDL_Quit();
    return 0;
}
