/*
 * gasm_vfile.h — stdio FILE* streams over gasm assets and gasm:storage.
 *
 * gasm guests have no file system: a game's data are read-only assets and its
 * saves are storage keys. gasm_vfile_open gives either one a FILE* (via
 * fopencookie), so code written against fopen/fread/fwrite/fseek keeps working:
 *
 *   FILE *f = gasm_vfile_open(GASM_VFILE_ASSET, "maps/e1m1.lmp", "rb");    // streamed
 *   FILE *s = gasm_vfile_open(GASM_VFILE_STORAGE, "save1.dat", "wb");      // stored on fclose
 *
 * Assets are read on demand (asset_read_at64, any size) and can't be written.
 * Storage values are loaded on open and stored when the stream is closed; mode
 * "r" needs an existing key, "w" starts empty, "a" appends, "+" allows both
 * directions. Errors set errno (ENOENT, EROFS, EINVAL, ENOMEM, EIO).
 *
 * Needs _GNU_SOURCE (fopencookie). Compile sdk/c/src/gasm_vfile.c with the game.
 * Used by the DOOM guest and SDL 3 on gasm. MIT license.
 */
#ifndef GASM_VFILE_H
#define GASM_VFILE_H

#include <stdio.h>

#ifdef __cplusplus
extern "C" {
#endif

typedef enum { GASM_VFILE_ASSET, GASM_VFILE_STORAGE } gasm_vfile_kind;

/* Open asset `name` or storage key `name` as a stdio stream; NULL (errno set) on failure. */
FILE *gasm_vfile_open(gasm_vfile_kind kind, const char *name, const char *mode);

/* Is `key` a valid storage key (1-128 bytes of [A-Za-z0-9._-], not "." or "..")? */
int gasm_vfile_valid_key(const char *key);

#ifdef __cplusplus
}
#endif

#endif /* GASM_VFILE_H */
