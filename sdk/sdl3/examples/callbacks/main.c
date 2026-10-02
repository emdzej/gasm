/*
  An SDL3 app on the main callbacks (SDL_MAIN_USE_CALLBACKS): SDL_AppIterate runs
  once per gasm frame. A ball bounces; keys 1-8 (or gamepad buttons) play notes.

  SPDX-License-Identifier: MIT
*/
#define SDL_MAIN_USE_CALLBACKS 1
#include <SDL3/SDL.h>
#include <SDL3/SDL_main.h>

#define RATE 48000

static SDL_Window *window;
static SDL_Renderer *renderer;
static SDL_AudioStream *stream;
static float bx = 100, by = 60, vx = 2.5f, vy = 1.5f;
static float freq, phase;
static int note_frames;
static const char *pad_name = "none";

static void play(int note)
{
    static const float scale[] = { 261.63f, 293.66f, 329.63f, 349.23f, 392.00f, 440.00f, 493.88f, 523.25f };
    freq = scale[note % 8];
    note_frames = 12;
}

SDL_AppResult SDL_AppInit(void **appstate, int argc, char *argv[])
{
    (void)appstate; (void)argc; (void)argv;
    if (!SDL_Init(SDL_INIT_VIDEO | SDL_INIT_AUDIO | SDL_INIT_GAMEPAD)) {
        SDL_Log("SDL_Init: %s", SDL_GetError());
        return SDL_APP_FAILURE;
    }
    if (!SDL_CreateWindowAndRenderer("callbacks", 320, 240, 0, &window, &renderer)) {
        return SDL_APP_FAILURE;
    }
    const SDL_AudioSpec spec = { SDL_AUDIO_F32, 1, RATE };
    stream = SDL_OpenAudioDeviceStream(SDL_AUDIO_DEVICE_DEFAULT_PLAYBACK, &spec, NULL, NULL);
    if (stream) {
        SDL_ResumeAudioStreamDevice(stream);
    }
    return SDL_APP_CONTINUE;
}

SDL_AppResult SDL_AppEvent(void *appstate, SDL_Event *event)
{
    (void)appstate;
    switch (event->type) {
    case SDL_EVENT_QUIT:
        return SDL_APP_SUCCESS;
    case SDL_EVENT_KEY_DOWN:
        if (event->key.key == SDLK_ESCAPE) {
            return SDL_APP_SUCCESS;
        }
        if (event->key.scancode >= SDL_SCANCODE_1 && event->key.scancode <= SDL_SCANCODE_8) {
            play(event->key.scancode - SDL_SCANCODE_1);
        }
        break;
    case SDL_EVENT_GAMEPAD_ADDED: {
        SDL_Gamepad *pad = SDL_OpenGamepad(event->gdevice.which);
        if (pad) {
            pad_name = SDL_GetGamepadName(pad);
        }
        break;
    }
    case SDL_EVENT_GAMEPAD_BUTTON_DOWN:
        play(event->gbutton.button);
        break;
    default:
        break;
    }
    return SDL_APP_CONTINUE;
}

SDL_AppResult SDL_AppIterate(void *appstate)
{
    (void)appstate;
    bx += vx;
    by += vy;
    if (bx < 0 || bx > 304) {
        vx = -vx;
    }
    if (by < 16 || by > 224) {
        vy = -vy;
    }
    /* one frame of tone (800 samples at 48 kHz), silence otherwise */
    float samples[RATE / 60];
    for (int i = 0; i < RATE / 60; i++) {
        samples[i] = note_frames > 0 ? 0.2f * SDL_sinf(phase) : 0.0f;
        phase += 2.0f * SDL_PI_F * freq / RATE;
    }
    if (note_frames > 0) {
        note_frames--;
    }
    if (stream) {
        SDL_PutAudioStreamData(stream, samples, sizeof samples);
    }

    SDL_SetRenderDrawColor(renderer, 30, 20, 40, 255);
    SDL_RenderClear(renderer);
    SDL_FRect ball = { bx, by, 16, 16 };
    SDL_SetRenderDrawColor(renderer, 250, 200, note_frames ? 255 : 60, 255);
    SDL_RenderFillRect(renderer, &ball);
    SDL_SetRenderDrawColor(renderer, 255, 255, 255, 255);
    SDL_RenderDebugTextFormat(renderer, 4, 4, "keys 1-8: notes  pad: %s", pad_name);
    SDL_RenderPresent(renderer);
    return SDL_APP_CONTINUE;
}

void SDL_AppQuit(void *appstate, SDL_AppResult result)
{
    (void)appstate; (void)result;
}
