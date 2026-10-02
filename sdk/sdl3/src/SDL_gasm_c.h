/*
  SDL3 on gasm: shared by the gasm drivers.

  SPDX-License-Identifier: Zlib
*/
#ifndef SDL_gasm_c_h_
#define SDL_gasm_c_h_

/* gasm frames begun so far (input and audio are per frame) */
extern Uint64 SDL_GASM_Frame(void);
extern void SDL_GASM_BeginFrame(void);
/* A window was presented: may end the frame (classic main) */
extern void SDL_GASM_Presented(void);
/* The audio driver's per-frame step (one frame of samples) */
extern void SDL_GASM_AudioFrame(void);
/* Re-send gasm_input_mode (cursor hidden, relative mouse) */
extern void SDL_GASM_UpdateInputMode(void);

#endif /* SDL_gasm_c_h_ */
