/* Shared between gasm_doom.c and gasm_opl.c. SPDX-License-Identifier: MIT */
#ifndef GASM_DOOM_H
#define GASM_DOOM_H
#include <stdint.h>

/* Generate `frames` stereo samples of OPL music (int16 range) into `stereo`. */
void gasm_opl_render(int32_t *stereo, unsigned int frames);

#endif
