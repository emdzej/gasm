/*
  The gasm entry points for SDL apps (included once, by SDL_main.h, in the file
  with main() or the SDL_App* callbacks).

  - SDL_MAIN_USE_CALLBACKS: SDL_AppInit runs in gasm_init, then every gasm_frame
    delivers the frame's events to SDL_AppEvent and calls SDL_AppIterate once.
    Nothing else is needed.
  - A classic main() with its own loop: main runs on the first frame and is
    suspended at the end of each frame (SDL_RenderPresent, SDL_UpdateWindowSurface,
    SDL_Delay across a frame boundary) by the C SDK's loop helper (gasm_loop.c,
    built into libSDL3.a). Link with -Wl,--wrap=exit and post-process with
    wasm-opt --asyncify --pass-arg=asyncify-removelist@gasm_loop_frame -O2
    (SDL3Config.cmake / gasm_add_game(... LOOP ...) do both).

  Command line: the "args" param, split at spaces (argv[0] is "app").

  SPDX-License-Identifier: Zlib
*/
#ifndef SDL_main_impl_private_h_
#define SDL_main_impl_private_h_

#ifdef __cplusplus
extern "C" {
#endif

extern SDL_DECLSPEC int SDLCALL SDL_GASM_Args(char ***argv);
extern SDL_DECLSPEC void SDLCALL SDL_GASM_BeginFrame(void);

#ifdef SDL_MAIN_USE_CALLBACKS

extern SDL_DECLSPEC int SDLCALL SDL_GASM_CallbacksFrame(void);
extern SDL_DECLSPEC void SDLCALL SDL_GASM_CallbacksExit(void);

__attribute__((export_name("gasm_abi_version"))) int SDL_GASM_abi_version(void) { return 0; }

__attribute__((export_name("gasm_init"))) int SDL_GASM_init(void)
{
    char **argv;
    int argc = SDL_GASM_Args(&argv);
    /* SDL_main (from SDL_main_impl.h) enters the callbacks and returns */
    return SDL_RunApp(argc, argv, SDL_main, NULL);
}

__attribute__((export_name("gasm_frame"))) void SDL_GASM_frame(void)
{
    SDL_GASM_CallbacksFrame();
}

__attribute__((export_name("gasm_exit"))) void SDL_GASM_exit(void)
{
    SDL_GASM_CallbacksExit();
}

#else /* classic main(): gasm_loop */

extern SDL_DECLSPEC void SDLCALL SDL_GASM_UseLoop(void (*wait)(void));
extern void gasm_wait_frame(void);

int gasm_loop_init(void)
{
    SDL_GASM_UseLoop(gasm_wait_frame);
    return 0;
}

int gasm_main(void)
{
    char **argv;
    int argc = SDL_GASM_Args(&argv);
    SDL_GASM_BeginFrame();
    return SDL_RunApp(argc, argv, SDL_main, NULL);
}

#endif /* SDL_MAIN_USE_CALLBACKS */

#ifdef __cplusplus
}
#endif

#endif /* SDL_main_impl_private_h_ */
