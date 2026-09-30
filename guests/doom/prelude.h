/*
 * Force-included (-include) into every DOOM engine source file.
 *
 * gasm guests have no filesystem, so the engine's few file operations are
 * redirected to gasm_doom.c: reads come from assets (the WAD) or storage,
 * writes (config, save games) go to gasm:storage. stdio.h and sys/stat.h are
 * included first so their own declarations keep the real names.
 */
#ifndef GASM_DOOM_PRELUDE_H
#define GASM_DOOM_PRELUDE_H

#include <stdio.h>
#include <stdlib.h>
#include <sys/stat.h>

FILE *gasm_doom_fopen(const char *path, const char *mode);
int gasm_doom_remove(const char *path);
int gasm_doom_rename(const char *from, const char *to);
int gasm_doom_mkdir(const char *path, ...);

#define fopen gasm_doom_fopen
#define remove gasm_doom_remove
#define rename gasm_doom_rename
#define mkdir gasm_doom_mkdir
/* i_system.c shells out for error dialogs (zenity); there is no shell. */
#define system(cmd) ((void)(cmd), -1)

/* chocolate-doom's midifile.c uses SDL's byte-swapping macros. */
#define SDL_SwapBE16(x) __builtin_bswap16(x)
#define SDL_SwapBE32(x) __builtin_bswap32(x)

/* chocolate-doom's i_oplmusic.c expects this from its (newer) i_sound.h. */
typedef enum { opl_v_old, opl_v_new } opl_driver_ver_t;

#endif
