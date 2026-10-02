/*
 * gasm_vfile.c — stdio FILE* streams over gasm assets and gasm:storage (see
 * gasm_vfile.h). MIT license.
 */
#ifndef _GNU_SOURCE
#define _GNU_SOURCE 1
#endif
#include "gasm_vfile.h"

#include <errno.h>
#include <stdint.h>
#include <stdlib.h>
#include <string.h>
#include <sys/types.h>

#include "gasm.h"

typedef struct {
    char name[1024];    /* asset name or storage key */
    int asset, writing, dirty;
    uint8_t *data;      /* storage: the value (read and written in memory) */
    size_t len, cap;
    off_t pos, size;
} vfile;

int gasm_vfile_valid_key(const char *k) {
    size_t n = strlen(k);
    if (n == 0 || n > 128 || !strcmp(k, ".") || !strcmp(k, "..")) return 0;
    for (size_t i = 0; i < n; i++) {
        char c = k[i];
        if (!((c >= 'a' && c <= 'z') || (c >= 'A' && c <= 'Z') || (c >= '0' && c <= '9') || c == '.' || c == '_' || c == '-')) return 0;
    }
    return 1;
}

static int reserve(vfile *f, size_t end) {
    if (end <= f->cap) return 1;
    size_t cap = f->cap ? f->cap : 4096;
    while (cap < end) cap *= 2;
    uint8_t *d = realloc(f->data, cap);
    if (!d) return 0;
    f->data = d;
    f->cap = cap;
    return 1;
}

static ssize_t vf_read(void *c, char *buf, size_t size) {
    vfile *f = c;
    if (f->pos >= f->size) return 0;
    if ((off_t)size > f->size - f->pos) size = (size_t)(f->size - f->pos);
    if (size > 0x7fffffff) size = 0x7fffffff;
    if (f->asset) {
        int32_t n = gasm_asset_read_at64(f->name, (uint32_t)strlen(f->name), (uint64_t)f->pos, buf, (uint32_t)size);
        if (n < 0) { errno = EIO; return -1; }
        size = (size_t)n;
    } else {
        memcpy(buf, f->data + f->pos, size);
    }
    f->pos += (off_t)size;
    return (ssize_t)size;
}

static ssize_t vf_write(void *c, const char *buf, size_t size) {
    vfile *f = c;
    if (!f->writing) { errno = EBADF; return -1; }
    size_t end = (size_t)f->pos + size;
    if (!reserve(f, end)) { errno = ENOMEM; return -1; }
    if ((size_t)f->pos > f->len) memset(f->data + f->len, 0, (size_t)f->pos - f->len);
    memcpy(f->data + f->pos, buf, size);
    f->pos = (off_t)end;
    if (end > f->len) f->len = end;
    f->size = (off_t)f->len;
    f->dirty = 1;
    return (ssize_t)size;
}

/* Seeking while writing works too (save formats pad to alignment with ftell). */
static int vf_seek(void *c, off_t *off, int whence) {
    vfile *f = c;
    off_t base = whence == SEEK_SET ? 0 : whence == SEEK_CUR ? f->pos : f->size;
    if (base + *off < 0) { errno = EINVAL; return -1; }
    f->pos = base + *off;
    *off = f->pos;
    return 0;
}

static int vf_close(void *c) {
    vfile *f = c;
    int rc = 0;
    if (f->writing && f->dirty) {
        int32_t err = gasm_storage_set(f->name, (uint32_t)strlen(f->name), f->data, (uint32_t)f->len);
        if (err != 0) {
            char msg[200];
            int n = snprintf(msg, sizeof msg, "gasm_vfile: could not store %s (error %d)", f->name, (int)err);
            gasm_log(msg, (uint32_t)(n < (int)sizeof msg ? n : (int)sizeof msg - 1));
            errno = EIO;
            rc = -1;
        }
    }
    free(f->data);
    free(f);
    return rc;
}

/* Load storage value f->name; 0 if it doesn't exist. */
static int load_storage(vfile *f) {
    int32_t n = gasm_storage_get(f->name, (uint32_t)strlen(f->name), NULL, 0);
    if (n < 0) return 0;
    if (!reserve(f, n ? (size_t)n : 1)) return -1;
    gasm_storage_get(f->name, (uint32_t)strlen(f->name), f->data, (uint32_t)n);
    f->len = (size_t)n;
    f->size = n;
    return 1;
}

FILE *gasm_vfile_open(gasm_vfile_kind kind, const char *name, const char *mode) {
    const int write = strchr(mode, 'w') || strchr(mode, 'a') || strchr(mode, '+');
    if (strlen(name) >= sizeof ((vfile *)0)->name) { errno = ENAMETOOLONG; return NULL; }
    vfile *f = calloc(1, sizeof *f);
    if (!f) { errno = ENOMEM; return NULL; }
    strcpy(f->name, name);
    if (kind == GASM_VFILE_STORAGE) {
        if (!gasm_vfile_valid_key(name)) { errno = EINVAL; goto fail; }
        f->writing = write;
        int exists = load_storage(f);
        if (exists < 0) { errno = ENOMEM; goto fail; }
        if (strchr(mode, 'w')) {
            f->len = 0;
            f->size = 0;
            f->dirty = 1;           /* "w" stores even an empty file */
        } else if (!exists && !strchr(mode, 'a')) {
            errno = ENOENT;
            goto fail;
        }
        if (strchr(mode, 'a')) {
            f->pos = f->size;
            f->dirty = !exists;     /* "a" creates a missing key */
        }
    } else {
        if (write) { errno = EROFS; goto fail; }
        int64_t size = gasm_asset_size64(name, (uint32_t)strlen(name));
        if (size < 0) { errno = ENOENT; goto fail; }
        f->asset = 1;
        f->size = (off_t)size;
    }
    cookie_io_functions_t io = { vf_read, vf_write, vf_seek, vf_close };
    FILE *file = fopencookie(f, mode, io);
    if (file) return file;
    errno = ENOMEM;
fail:
    free(f->data);
    free(f);
    return NULL;
}
