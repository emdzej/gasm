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

/* OpenGL ES on gasm:gl (SDL_gasmopengles.c): available if the game links gasm_gl.o */
struct SDL_VideoDevice;
struct SDL_Window;
extern bool GASM_GL_Available(void);
extern bool GASM_GL_LoadLibrary(struct SDL_VideoDevice *_this, const char *path);
extern SDL_FunctionPointer GASM_GL_GetProcAddress(struct SDL_VideoDevice *_this, const char *proc);
extern void GASM_GL_UnloadLibrary(struct SDL_VideoDevice *_this);
extern void GASM_GL_DefaultProfileConfig(struct SDL_VideoDevice *_this, int *mask, int *major, int *minor);
extern SDL_GLContext GASM_GL_CreateContext(struct SDL_VideoDevice *_this, struct SDL_Window *window);
extern bool GASM_GL_MakeCurrent(struct SDL_VideoDevice *_this, struct SDL_Window *window, SDL_GLContext context);
extern bool GASM_GL_SetSwapInterval(struct SDL_VideoDevice *_this, int interval);
extern bool GASM_GL_GetSwapInterval(struct SDL_VideoDevice *_this, int *interval);
extern bool GASM_GL_SwapWindow(struct SDL_VideoDevice *_this, struct SDL_Window *window);
extern bool GASM_GL_DestroyContext(struct SDL_VideoDevice *_this, SDL_GLContext context);
extern bool GASM_GL_DrawableSize(int *w, int *h);
extern void GASM_GL_CheckSize(struct SDL_Window *window);

#endif /* SDL_gasm_c_h_ */
