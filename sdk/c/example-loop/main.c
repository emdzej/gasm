/*
 * example-loop: a game with its own main loop (gasm_loop.h).
 * A bouncing square; the d-pad pushes it, A changes its colour. The loop never
 * returns until START is held for a second; gasm_wait_frame() yields each frame.
 */
#include <string.h>
#include "gasm.h"
#include "gasm_loop.h"

#define W 160
#define H 120

static uint8_t fb[W * H * 4];

int gasm_main(void) {
    gasm_set_frame_rate(60);
    int x = 20, y = 20, dx = 1, dy = 1, hue = 0, held = 0;
    gasm_log_str("example-loop: running its own loop");
    while (held < 60) {
        uint32_t pad = gasm_input_pad(0);
        held = pad & GASM_BTN_START ? held + 1 : 0;
        if (pad & GASM_BTN_A) hue = (hue + 7) & 255;
        if (pad & GASM_BTN_LEFT) dx = -2;
        if (pad & GASM_BTN_RIGHT) dx = 2;
        x += dx; y += dy;
        if (x < 0 || x > W - 16) { dx = -dx; x += dx; }
        if (y < 0 || y > H - 16) { dy = -dy; y += dy; }
        memset(fb, 20, sizeof fb);
        for (int j = 0; j < 16; j++)
            for (int i = 0; i < 16; i++) {
                uint8_t *p = &fb[((y + j) * W + x + i) * 4];
                p[0] = (uint8_t)hue; p[1] = (uint8_t)(255 - hue); p[2] = 200; p[3] = 255;
            }
        gasm_video_present(fb, W, H, W * 4);
        gasm_wait_frame();
    }
    gasm_log_str("example-loop: done");
    return 0;
}
