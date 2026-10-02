/*
  SDL3 on gasm: files.

  gasm guests have no filesystem. SDL sees two trees:
  - "/" (SDL_GetBasePath, also the current directory): the game's assets,
    read-only, streamed with asset_read_at. Relative paths are assets too.
  - "/storage/" (SDL_GetPrefPath, the user storage): gasm:storage. Each file is a
    key (path separators become '.'), stored when the file is closed.
  SDL_IOFromFile (via fopen, redirected here), SDL_OpenTitleStorage,
  SDL_OpenUserStorage, SDL_GetPathInfo and SDL_EnumerateDirectory all work on them.

  SPDX-License-Identifier: Zlib
*/
#define _GNU_SOURCE 1
#include "SDL_internal.h"

#include "filesystem/SDL_sysfilesystem.h"

#include <errno.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>

#include "gasm.h"
#include "gasm_vfile.h"

#define STORAGE_DIR "/storage/"
#define KEY_MAX 128

/* ---- paths -------------------------------------------------------------------------- */

typedef enum { P_NONE, P_ROOT, P_ASSET, P_STORAGE_DIR, P_STORAGE } PathKind;

typedef struct
{
    PathKind kind;
    char name[1024];   /* asset name, or storage key */
} Path;

static bool ValidKey(const char *k)
{
    return gasm_vfile_valid_key(k) != 0;
}

static Path Resolve(const char *path)
{
    Path p;
    SDL_zero(p);
    while (path[0] == '.' && path[1] == '/') {
        path += 2;
    }
    while (*path == '/') {
        path++;
    }
    if (!*path || !SDL_strcmp(path, ".")) {
        p.kind = P_ROOT;
        return p;
    }
    if (!SDL_strncmp(path, "storage", 7) && (path[7] == 0 || path[7] == '/')) {
        const char *rest = path[7] ? path + 8 : "";
        if (!*rest) {
            p.kind = P_STORAGE_DIR;
            return p;
        }
        SDL_strlcpy(p.name, rest, sizeof p.name);
        size_t n = SDL_strlen(p.name);
        while (n && p.name[n - 1] == '/') {
            p.name[--n] = 0;
        }
        for (char *c = p.name; *c; c++) {
            if (*c == '/') {
                *c = '.';
            }
        }
        p.kind = ValidKey(p.name) ? P_STORAGE : P_NONE;
        return p;
    }
    SDL_strlcpy(p.name, path, sizeof p.name);
    size_t n = SDL_strlen(p.name);
    while (n && p.name[n - 1] == '/') {
        p.name[--n] = 0;
    }
    p.kind = P_ASSET;
    return p;
}

static Sint32 StorageSize(const char *key)
{
    return gasm_storage_get(key, (Uint32)SDL_strlen(key), NULL, 0);
}

static Sint64 AssetSize(const char *name)
{
    return gasm_asset_size64(name, (Uint32)SDL_strlen(name));
}

/* Asset `index`'s name, or false past the end. */
static bool AssetName(Uint32 index, char *buf, size_t cap)
{
    const Sint32 n = gasm_asset_name(index, buf, (Uint32)cap - 1);
    if (n < 0 || (size_t)n >= cap) {
        return false;
    }
    buf[n] = 0;
    return true;
}

/* Does asset `name` lie in folder `dir`? Case-insensitive (ASCII), like the
   runners' lookup of folder entries. */
static bool InDir(const char *name, const char *dir, size_t n)
{
    return !SDL_strncasecmp(name, dir, n) && name[n] == '/';
}

/* Is `dir` a folder of assets (some asset name starts with "dir/")? */
static bool AssetDir(const char *dir)
{
    char name[1024];
    const size_t n = SDL_strlen(dir);
    const Uint32 count = gasm_asset_count();
    for (Uint32 i = 0; i < count; i++) {
        if (AssetName(i, name, sizeof name) && InDir(name, dir, n)) {
            return true;
        }
    }
    return false;
}

/* ---- stdio streams: sdk/c gasm_vfile ------------------------------------------------- */

/* fopen for SDL (SDL_iostream.c is built with -Dfopen=SDL_GASM_fopen). */
FILE *SDL_GASM_fopen(const char *path, const char *mode)
{
    const Path p = Resolve(path);
    const bool write = SDL_strchr(mode, 'w') || SDL_strchr(mode, 'a') || SDL_strchr(mode, '+');
    if (p.kind == P_STORAGE) {
        return gasm_vfile_open(GASM_VFILE_STORAGE, p.name, mode);
    }
    if (p.kind == P_ASSET && !write) {
        return gasm_vfile_open(GASM_VFILE_ASSET, p.name, mode);
    }
    errno = p.kind == P_ASSET ? EROFS : p.kind == P_NONE ? EINVAL : EISDIR;
    return NULL;
}

/* ---- SDL_filesystem ------------------------------------------------------------------------ */

char *SDL_SYS_GetBasePath(void)
{
    return SDL_strdup("/");
}

char *SDL_SYS_GetExeName(void)
{
    return NULL;
}

/* The runner already keeps each game's storage apart: org and app don't matter. */
char *SDL_SYS_GetPrefPath(const char *org, const char *app)
{
    (void)org; (void)app;
    return SDL_strdup(STORAGE_DIR);
}

char *SDL_SYS_GetUserFolder(SDL_Folder folder)
{
    if (folder == SDL_FOLDER_SAVEDGAMES || folder == SDL_FOLDER_DOCUMENTS) {
        return SDL_strdup(STORAGE_DIR);
    }
    SDL_Unsupported();
    return NULL;
}

char *SDL_SYS_GetCurrentDirectory(void)
{
    return SDL_strdup("/");
}

/* ---- SDL_fsops ------------------------------------------------------------------------------- */

bool SDL_SYS_EnumerateDirectory(const char *path, SDL_EnumerateDirectoryCallback cb, void *userdata)
{
    const Path p = Resolve(path);
    char dirname[1100], name[1024];
    SDL_EnumerationResult rc = SDL_ENUM_CONTINUE;

    if (p.kind == P_STORAGE_DIR) {
        const Uint32 count = gasm_storage_count();
        for (Uint32 i = 0; i < count && rc == SDL_ENUM_CONTINUE; i++) {
            const Sint32 n = gasm_storage_key(i, name, sizeof name - 1);
            if (n > 0 && n < (Sint32)sizeof name) {
                name[n] = 0;
                rc = cb(userdata, STORAGE_DIR, name);
            }
        }
        return rc != SDL_ENUM_FAILURE;
    }
    if (p.kind != P_ROOT && (p.kind != P_ASSET || !AssetDir(p.name))) {
        return SDL_SetError("Can't open directory '%s'", path);
    }
    const char *prefix = p.kind == P_ROOT ? "" : p.name;
    const size_t plen = SDL_strlen(prefix);
    SDL_snprintf(dirname, sizeof dirname, "/%s%s", prefix, plen ? "/" : "");

    /* entries directly inside, each once: names are sorted, so the files of a
       subfolder are next to each other and comparing with the last entry suffices */
    char last[1024] = "";
    if (p.kind == P_ROOT) {
        rc = cb(userdata, dirname, "storage");
    }
    const Uint32 count = gasm_asset_count();
    for (Uint32 i = 0; i < count && rc == SDL_ENUM_CONTINUE; i++) {
        if (!AssetName(i, name, sizeof name)) {
            continue;
        }
        if (plen && !InDir(name, prefix, plen)) {
            continue;
        }
        char *entry = name + (plen ? plen + 1 : 0);
        char *slash = SDL_strchr(entry, '/');
        if (slash) {
            *slash = 0;
        }
        if (!SDL_strcmp(last, entry)) {
            continue;
        }
        SDL_strlcpy(last, entry, sizeof last);
        rc = cb(userdata, dirname, entry);
    }
    return rc != SDL_ENUM_FAILURE;
}

bool SDL_SYS_RemovePath(const char *path)
{
    const Path p = Resolve(path);
    if (p.kind != P_STORAGE) {
        return SDL_SetError("Can't remove '%s': read-only", path);
    }
    if (StorageSize(p.name) < 0) {
        return true;   /* SDL_RemovePath: a missing path is not an error */
    }
    return gasm_storage_delete(p.name, (Uint32)SDL_strlen(p.name)) == 0 ? true : SDL_SetError("Can't remove '%s'", path);
}

bool SDL_SYS_CopyFile(const char *oldpath, const char *newpath)
{
    const Path to = Resolve(newpath);
    if (to.kind != P_STORAGE) {
        return SDL_SetError("Can't write '%s': read-only", newpath);
    }
    size_t len = 0;
    void *data = SDL_LoadFile(oldpath, &len);
    if (!data) {
        return false;
    }
    const bool ok = gasm_storage_set(to.name, (Uint32)SDL_strlen(to.name), data, (Uint32)len) == 0;
    SDL_free(data);
    return ok ? true : SDL_SetError("Can't write '%s'", newpath);
}

bool SDL_SYS_RenamePath(const char *oldpath, const char *newpath)
{
    const Path from = Resolve(oldpath);
    if (from.kind != P_STORAGE) {
        return SDL_SetError("Can't rename '%s': read-only", oldpath);
    }
    if (!SDL_SYS_CopyFile(oldpath, newpath)) {
        return false;
    }
    gasm_storage_delete(from.name, (Uint32)SDL_strlen(from.name));
    return true;
}

/* Storage has no folders; creating one inside it succeeds and changes nothing. */
bool SDL_SYS_CreateDirectory(const char *path)
{
    const Path p = Resolve(path);
    if (p.kind == P_STORAGE || p.kind == P_STORAGE_DIR) {
        return true;
    }
    return SDL_SetError("Can't create '%s': read-only", path);
}

bool SDL_SYS_GetPathInfo(const char *path, SDL_PathInfo *info)
{
    const Path p = Resolve(path);
    SDL_zerop(info);
    switch (p.kind) {
    case P_ROOT:
    case P_STORAGE_DIR:
        info->type = SDL_PATHTYPE_DIRECTORY;
        return true;
    case P_STORAGE: {
        const Sint32 n = StorageSize(p.name);
        if (n < 0) {
            break;
        }
        info->type = SDL_PATHTYPE_FILE;
        info->size = (Uint64)n;
        return true;
    }
    case P_ASSET: {
        const Sint64 n = AssetSize(p.name);
        if (n >= 0) {
            info->type = SDL_PATHTYPE_FILE;
            info->size = (Uint64)n;
            return true;
        }
        if (AssetDir(p.name)) {
            info->type = SDL_PATHTYPE_DIRECTORY;
            return true;
        }
        break;
    }
    default:
        break;
    }
    return SDL_SetError("Can't stat '%s'", path);
}
