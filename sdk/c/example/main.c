/* Smallest useful gasm game in C: a gradient that scrolls, green while A is held. */
#include "gasm.h"

#define W 160
#define H 144

static uint8_t fb[W * H * 4];
static uint32_t t;

GASM_EXPORT("gasm_abi_version") int32_t abi_version(void) { return GASM_ABI_VERSION; }

GASM_EXPORT("gasm_init") int32_t init(void) {
    gasm_set_frame_rate(60);
    gasm_log_str("hello from C");
    return 0;
}

GASM_EXPORT("gasm_frame") void frame(void) {
    uint32_t pad = gasm_input_pad(0);
    for (int i = 0; i < W * H; i++) {
        fb[i * 4 + 0] = (uint8_t)(i + t);
        fb[i * 4 + 1] = (pad & GASM_BTN_A) ? 255 : 0;
        fb[i * 4 + 2] = 64;
        fb[i * 4 + 3] = 255;
    }
    gasm_video_present(fb, W, H, W * 4);
    t++;
}
