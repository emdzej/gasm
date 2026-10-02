/*
 * test-pattern — smallest useful gasm guest.
 * Scrolling gradient, a square moved by the d-pad (twice as fast with Shift),
 * dragged with the mouse, and a tone while A is held.
 */
#include <math.h>
#include <stdio.h>
#include <string.h>
#include "gasm.h"

#define W 256
#define H 240
#define RATE 44100

static uint8_t fb[W * H * 4];
static float audio[RATE / 60 + 1];
static uint32_t frame;
static int px = W / 2, py = H / 2;
static double phase;

GASM_EXPORT("gasm_abi_version") int32_t abi_version(void) { return GASM_ABI_VERSION; }

GASM_TITLE("Test pattern");

GASM_EXPORT("gasm_init") int32_t init(void) {
    gasm_set_frame_rate(60.0);
    gasm_audio_config(RATE, 1);
    gasm_input_mode(GASM_INPUT_POINTER_HIDDEN);   /* the square is the cursor */
    printf("test-pattern: hello from WASI stdout\n");
    gasm_log_str("test-pattern: init ok");
    return 0;
}

GASM_EXPORT("gasm_frame") void frame_tick(void) {
    uint32_t pad = gasm_input_pad(0);
    /* raw keyboard: either Shift doubles the speed (the arrows still arrive as the pad) */
    uint8_t keys[GASM_KEY_STATE_BYTES] = {0};
    gasm_key_state(keys, sizeof keys);
    int fast = (keys[GASM_KEY_SHIFT_LEFT / 8] >> (GASM_KEY_SHIFT_LEFT % 8) & 1) |
               (keys[GASM_KEY_SHIFT_RIGHT / 8] >> (GASM_KEY_SHIFT_RIGHT % 8) & 1);
    int step = fast ? 4 : 2;
    if (pad & GASM_BTN_LEFT)  px -= step;
    if (pad & GASM_BTN_RIGHT) px += step;
    if (pad & GASM_BTN_UP)    py -= step;
    if (pad & GASM_BTN_DOWN)  py += step;
    /* mouse: drag the square (position in frame pixels) */
    uint8_t ptr[GASM_POINTER_BYTES];
    if (gasm_pointer(ptr, sizeof ptr) > 0) {
        float fx, fy;
        uint32_t buttons;
        memcpy(&fx, ptr + 8, 4);
        memcpy(&fy, ptr + 12, 4);
        memcpy(&buttons, ptr + 32, 4);
        if (buttons & GASM_MOUSE_LEFT) { px = (int)fx; py = (int)fy; }
    }
    px = (px + W) % W;
    py = (py + H) % H;

    for (int y = 0; y < H; y++) {
        for (int x = 0; x < W; x++) {
            uint8_t *p = &fb[(y * W + x) * 4];
            p[0] = (uint8_t)(x + frame);
            p[1] = (uint8_t)(y + frame / 2);
            p[2] = (uint8_t)(128 + (x ^ y));
            p[3] = 255;
            int dx = x - px, dy = y - py;
            if (dx > -8 && dx < 8 && dy > -8 && dy < 8) {
                p[0] = p[1] = p[2] = (pad & GASM_BTN_A) ? 255 : 0;
            }
        }
    }
    gasm_video_present(fb, W, H, W * 4);

    /* Exactly RATE/60 frames per tick: deterministic, drift handled by runner. */
    uint32_t n = RATE / 60;
    double freq = 220.0 + 440.0 * (double)py / H;
    for (uint32_t i = 0; i < n; i++) {
        audio[i] = (pad & GASM_BTN_A) ? (float)(0.2 * sin(phase)) : 0.0f;
        phase += 2.0 * M_PI * freq / RATE;
        if (phase > 2.0 * M_PI) phase -= 2.0 * M_PI;
    }
    gasm_audio_push(audio, n);
    frame++;
}
