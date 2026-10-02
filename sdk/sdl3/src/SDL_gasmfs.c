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
    const size_t n = SDL_strlen(k);
    if (n == 0 || n > KEY_MAX || !SDL_strcmp(k, ".") || !SDL_strcmp(k, "..")) {
        return false;
    }
    for (size_t i = 0; i < n; i++) {
        const char c = k[i];
        if (!((c >= 'a' && c <= 'z') || (c >= 'A' && c <= 'Z') || (c >= '0' && c <= '9') || c == '.' || c == '_' || c == '-')) {
            return false;
        }
    }
    return true;
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

static Sint32 AssetSize(const char *name)
{
    return gasm_asset_size(name, (Uint32)SDL_strlen(name));
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

/* Is `dir` a folder of assets (some asset name starts with "dir/")? */
static bool AssetDir(const char *dir)
{
    char name[1024];
    const size_t n = SDL_strlen(dir);
    const Uint32 count = gasm_asset_count();
    for (Uint32 i = 0; i < count; i++) {
        if (AssetName(i, name, sizeof name) && !SDL_strncmp(name, dir, n) && name[n] == '/') {
            return true;
        }
    }
    return false;
}

/* ---- stdio streams (fopencookie) ------------------------------------------------------- */

typedef struct
{
    char name[1024];
    bool asset, writing, dirty;
    Uint8 *data;
    size_t len, cap;
    off_t pos, size;
} VFile;

static ssize_t VF_Read(void *c, char *buf, size_t size)
{
    VFile *f = (VFile *)c;
    if (f->pos >= f->size) {
        return 0;
    }
    if ((off_t)size > f->size - f->pos) {
        size = (size_t)(f->size - f->pos);
    }
    if (f->asset) {
        const Sint32 n = gasm_asset_read_at(f->name, (Uint32)SDL_strlen(f->name), (Uint32)f->pos, buf, (Uint32)size);
        if (n < 0) {
            return -1;
        }
        size = (size_t)n;
    } else {
        SDL_memcpy(buf, f->data + f->pos, size);
    }
    f->pos += (off_t)size;
    return (ssize_t)size;
}

static bool Reserve(VFile *f, size_t end)
{
    if (end <= f->cap) {
        return true;
    }
    size_t cap = f->cap ? f->cap : 4096;
    while (cap < end) {
        cap *= 2;
    }
    Uint8 *d = (Uint8 *)SDL_realloc(f->data, cap);
    if (!d) {
        return false;
    }
    f->data = d;
    f->cap = cap;
    return true;
}

static ssize_t VF_Write(void *c, const char *buf, size_t size)
{
    VFile *f = (VFile *)c;
    if (!f->writing) {
        errno = EBADF;
        return -1;
    }
    const size_t end = (size_t)f->pos + size;
    if (!Reserve(f, end)) {
        errno = ENOMEM;
        return -1;
    }
    if ((size_t)f->pos > f->len) {
        SDL_memset(f->data + f->len, 0, (size_t)f->pos - f->len);
    }
    SDL_memcpy(f->data + f->pos, buf, size);
    f->pos = (off_t)end;
    if (end > f->len) {
        f->len = end;
    }
    f->size = (off_t)f->len;
    f->dirty = true;
    return (ssize_t)size;
}

static int VF_Seek(void *c, off_t *off, int whence)
{
    VFile *f = (VFile *)c;
    const off_t base = whence == SEEK_SET ? 0 : whence == SEEK_CUR ? f->pos : f->size;
    if (base + *off < 0) {
        errno = EINVAL;
        return -1;
    }
    f->pos = base + *off;
    *off = f->pos;
    return 0;
}

static int VF_Close(void *c)
{
    VFile *f = (VFile *)c;
    int rc = 0;
    if (f->writing && (f->dirty || f->len == 0) &&
        gasm_storage_set(f->name, (Uint32)SDL_strlen(f->name), f->data, (Uint32)f->len) != 0) {
        errno = EIO;
        rc = -1;
    }
    SDL_free(f->data);
    SDL_free(f);
    return rc;
}

static bool LoadStorage(VFile *f)
{
    const Sint32 n = StorageSize(f->name);
    if (n < 0) {
        return false;
    }
    if (!Reserve(f, n ? (size_t)n : 1)) {
        return false;
    }
    gasm_storage_get(f->name, (Uint32)SDL_strlen(f->name), f->data, (Uint32)n);
    f->len = (size_t)n;
    f->size = n;
    return true;
}

/* fopen for SDL (SDL_iostream.c is built with -Dfopen=SDL_GASM_fopen). */
FILE *SDL_GASM_fopen(const char *path, const char *mode)
{
    const Path p = Resolve(path);
    const bool write = SDL_strchr(mode, 'w') || SDL_strchr(mode, 'a') || SDL_strchr(mode, '+');
    VFile *f = (VFile *)SDL_calloc(1, sizeof *f);
    if (!f) {
        errno = ENOMEM;
        return NULL;
    }
    SDL_strlcpy(f->name, p.name, sizeof f->name);
    if (p.kind == P_STORAGE) {
        f->writing = write;
        const bool exists = LoadStorage(f);
        if (SDL_strchr(mode, 'w')) {
            f->len = 0;
            f->size = 0;
            f->dirty = true;
        } else if (!exists && !SDL_strchr(mode, 'a')) {
            errno = ENOENT;
            goto fail;
        }
        if (SDL_strchr(mode, 'a')) {
            f->pos = f->size;
            f->dirty = !exists;
        }
    } else if (p.kind == P_ASSET && !write) {
        const Sint32 size = AssetSize(p.name);
        if (size < 0) {
            errno = ENOENT;
            goto fail;
        }
        f->asset = true;
        f->size = size;
    } else {
        errno = p.kind == P_ASSET ? EROFS : p.kind == P_NONE ? EINVAL : EISDIR;
        goto fail;
    }
    {
        cookie_io_functions_t io = { VF_Read, VF_Write, VF_Seek, VF_Close };
        FILE *file = fopencookie(f, mode, io);
        if (file) {
            return file;
        }
    }
fail:
    SDL_free(f->data);
    SDL_free(f);
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

    /* entries directly inside: each once (folders appear once per file in them) */
    char **seen = NULL;
    int nseen = 0;
    if (p.kind == P_ROOT) {
        rc = cb(userdata, dirname, "storage");
    }
    const Uint32 count = gasm_asset_count();
    for (Uint32 i = 0; i < count && rc == SDL_ENUM_CONTINUE; i++) {
        if (!AssetName(i, name, sizeof name)) {
            continue;
        }
        if (plen && (SDL_strncmp(name, prefix, plen) || name[plen] != '/')) {
            continue;
        }
        char *entry = name + (plen ? plen + 1 : 0);
        char *slash = SDL_strchr(entry, '/');
        if (slash) {
            *slash = 0;
        }
        bool dup = false;
        for (int j = 0; j < nseen && !dup; j++) {
            dup = !SDL_strcmp(seen[j], entry);
        }
        if (dup) {
            continue;
        }
        char **grown = (char **)SDL_realloc(seen, sizeof(char *) * (size_t)(nseen + 1));
        if (!grown) {
            break;
        }
        seen = grown;
        seen[nseen++] = SDL_strdup(entry);
        rc = cb(userdata, dirname, entry);
    }
    for (int j = 0; j < nseen; j++) {
        SDL_free(seen[j]);
    }
    SDL_free(seen);
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
        const Sint32 n = AssetSize(p.name);
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
