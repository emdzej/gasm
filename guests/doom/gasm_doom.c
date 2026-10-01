/*
 * DOOM on gasm: the platform layer for doomgeneric.
 *
 *   frames   gasm_frame runs one doomgeneric tick at DOOM's native 35 Hz.
 *            The engine clock counts gasm frames (plus any time it "sleeps"),
 *            never the wall clock, so a run is reproducible on every runner.
 *   video    the 320x200 palettized screen, stretched to 640x480 (4:3, as on
 *            a CRT) and presented as RGBA8.
 *   input    the raw keyboard with DOOM's own keys (Ctrl fire, Space use, Shift
 *            run, Alt strafe, Esc menu, typed save names), the mouse while
 *            playing (locked: turn, fire, strafe, forward), and gamepads through
 *            virtual pad 1, whose buttons mean what the context needs (game,
 *            menu, yes/no prompt, save name).
 *   files    no filesystem: the WAD is an asset read on demand, the config and
 *            save games live in gasm:storage.
 *   audio    sound effects mixed here, music from the OPL emulator
 *            (gasm_opl.c), 44.1 kHz stereo: exactly 1260 samples per frame.
 *
 * SPDX-License-Identifier: MIT
 */
#include <ctype.h>
#include <errno.h>
#include <stdarg.h>
#include <stdlib.h>
#include <string.h>
#include <sys/types.h>

#include "gasm.h"
#include "gasm_doom.h"

#include "d_event.h"
#include "doomgeneric.h"
#include "doomkeys.h"
#include "doomstat.h"
#include "i_sound.h"
#include "i_video.h"
#include "m_config.h"
#include "p_saveg.h"
#include "w_wad.h"
#include "z_zone.h"

#define TICRATE_HZ 35
#define OUT_W 640
#define OUT_H 480
#define RATE 44100
#define FRAME_SAMPLES (RATE / TICRATE_HZ) /* 1260 */

/* ---- clock ------------------------------------------------------------------ */

static uint32_t frame_count;
static uint32_t slept_ms;

/* ceil(frame * 1000 / 35), so I_GetTime() (ms * 35 / 1000) is exactly the frame number. */
uint32_t DG_GetTicksMs(void) {
    return (uint32_t)(((uint64_t)frame_count * 1000 + TICRATE_HZ - 1) / TICRATE_HZ) + slept_ms;
}

/* The engine sleeps while it waits for the next tic (and during the screen
 * wipe). Nothing else can happen meanwhile, so sleeping just advances time. */
void DG_SleepMs(uint32_t ms) { slept_ms += ms; }

/* ---- video ------------------------------------------------------------------ */

static uint8_t fb[OUT_W * OUT_H * 4];

void DG_Init(void) {}
void DG_SetWindowTitle(const char *title) { (void)title; }

void DG_DrawFrame(void) {
    static uint32_t rgba[256];
    const uint8_t *src = (const uint8_t *)DG_ScreenBuffer;
    for (int i = 0; i < 256; i++) {
        uint32_t r = colors[i].r, g = colors[i].g, b = colors[i].b;
        rgba[i] = r | g << 8 | b << 16 | 0xffu << 24; /* bytes R,G,B,A on little-endian wasm */
    }
    uint32_t *out = (uint32_t *)fb;
    for (int y = 0; y < OUT_H; y++) {
        const uint8_t *row = src + (y * DOOMGENERIC_RESY / OUT_H) * DOOMGENERIC_RESX;
        for (int x = 0; x < DOOMGENERIC_RESX; x++) {
            uint32_t c = rgba[row[x]];
            out[2 * x] = out[2 * x + 1] = c;
        }
        out += OUT_W;
    }
}

/* ---- input ------------------------------------------------------------------ */

#define NUM_BUTTONS 12
enum { BTN_A, BTN_B, BTN_X, BTN_Y, BTN_L, BTN_R, BTN_SELECT, BTN_START, BTN_UP, BTN_DOWN, BTN_LEFT, BTN_RIGHT };
#define QUEUE_SIZE 64

extern int saveStringEnter;
extern int saveSlot;
extern int messageToPrint;
extern char savegamestrings[10][SAVESTRINGSIZE];
struct menu_s;
extern struct menu_s MainDef, *currentMenu;

static uint16_t queue[QUEUE_SIZE];
static unsigned int queue_head, queue_tail;
static unsigned char held[NUM_BUTTONS];   /* key sent when the button went down */
static unsigned char tap_release;         /* synthetic key to release next frame */
static uint32_t prev_pad;

static void post_key(int pressed, unsigned char key) {
    if (((queue_tail + 1) % QUEUE_SIZE) == queue_head) return; /* full: drop */
    queue[queue_tail] = (uint16_t)(pressed << 8 | key);
    queue_tail = (queue_tail + 1) % QUEUE_SIZE;
}

int DG_GetKey(int *pressed, unsigned char *key) {
    if (queue_head == queue_tail) return 0;
    uint16_t e = queue[queue_head];
    queue_head = (queue_head + 1) % QUEUE_SIZE;
    *pressed = e >> 8;
    *key = e & 0xff;
    return 1;
}

/* Weapon slots in cycling order; the key selects the slot, as in the original. */
static const struct { weapontype_t weapon; unsigned char key; } weapon_order[] = {
    { wp_fist, '1' }, { wp_chainsaw, '1' }, { wp_pistol, '2' }, { wp_shotgun, '3' },
    { wp_supershotgun, '3' }, { wp_chaingun, '4' }, { wp_missile, '5' },
    { wp_plasma, '6' }, { wp_bfg, '7' },
};
#define NUM_ORDER (int)(sizeof weapon_order / sizeof weapon_order[0])

static int weapon_usable(const player_t *p, weapontype_t w) {
    if (!p->weaponowned[w]) return 0;
    if (w == wp_fist && p->weaponowned[wp_chainsaw]) return 0; /* '1' always picks the saw */
    if (w == wp_supershotgun && gamemode != commercial) return 0;
    return 1;
}

static unsigned char next_weapon_key(void) {
    const player_t *p = &players[consoleplayer];
    weapontype_t cur = p->pendingweapon != wp_nochange ? p->pendingweapon : p->readyweapon;
    int at = 0;
    for (int i = 0; i < NUM_ORDER; i++)
        if (weapon_order[i].weapon == cur) at = i;
    for (int step = 1; step <= NUM_ORDER; step++) {
        int i = (at + step) % NUM_ORDER;
        if (weapon_usable(p, weapon_order[i].weapon)) return weapon_order[i].key;
    }
    return 0;
}

/* What a button press means right now (0 = nothing). */
static unsigned char key_for(int button) {
    if (saveStringEnter) {
        if (button == BTN_A || button == BTN_START) {
            /* No keyboard for typing a name: an empty slot gets one. */
            if (savegamestrings[saveSlot][0] == 0)
                snprintf(savegamestrings[saveSlot], SAVESTRINGSIZE, "SLOT %d", saveSlot + 1);
            return KEY_ENTER;
        }
        return button == BTN_B ? KEY_ESCAPE : 0;
    }
    if (messageToPrint) {
        switch (button) {
        case BTN_A: return 'y';
        case BTN_B: return 'n';
        case BTN_START: return KEY_ESCAPE;
        default: return 0;
        }
    }
    if (menuactive) {
        /* Start (Enter) selects like A; B goes back, and closes the menu from the top */
        switch (button) {
        case BTN_A: return KEY_ENTER;
        case BTN_START: return KEY_ENTER;
        case BTN_B: return currentMenu == &MainDef ? KEY_ESCAPE : KEY_BACKSPACE;
        case BTN_SELECT: return KEY_ESCAPE;
        case BTN_UP: return KEY_UPARROW;
        case BTN_DOWN: return KEY_DOWNARROW;
        case BTN_LEFT: return KEY_LEFTARROW;
        case BTN_RIGHT: return KEY_RIGHTARROW;
        default: return 0;
        }
    }
    switch (button) {
    case BTN_A: return KEY_FIRE;
    case BTN_B: return KEY_USE;
    case BTN_Y: return KEY_RSHIFT; /* run */
    case BTN_L: return KEY_STRAFE_L;
    case BTN_R: return KEY_STRAFE_R;
    case BTN_SELECT: return KEY_TAB; /* automap */
    case BTN_START: return KEY_ESCAPE; /* menu */
    case BTN_UP: return KEY_UPARROW;
    case BTN_DOWN: return KEY_DOWNARROW;
    case BTN_LEFT: return KEY_LEFTARROW;
    case BTN_RIGHT: return KEY_RIGHTARROW;
    default: return 0;
    }
}

/* DOOM key for a raw gasm key (0: unused). Vanilla's keyboard layout. */
static unsigned char doom_key(uint32_t code) {
    if (code >= GASM_KEY_KEY_A && code <= GASM_KEY_KEY_Z) return (unsigned char)('a' + code - GASM_KEY_KEY_A);
    if (code >= GASM_KEY_DIGIT0 && code <= GASM_KEY_DIGIT9) return (unsigned char)('0' + code - GASM_KEY_DIGIT0);
    if (code >= GASM_KEY_F1 && code <= GASM_KEY_F12) return (unsigned char)(KEY_F1 + code - GASM_KEY_F1);
    switch (code) {
    case GASM_KEY_ARROW_UP: return KEY_UPARROW;
    case GASM_KEY_ARROW_DOWN: return KEY_DOWNARROW;
    case GASM_KEY_ARROW_LEFT: return KEY_LEFTARROW;
    case GASM_KEY_ARROW_RIGHT: return KEY_RIGHTARROW;
    case GASM_KEY_CONTROL_LEFT: case GASM_KEY_CONTROL_RIGHT: return KEY_FIRE;
    case GASM_KEY_SPACE: return KEY_USE;
    case GASM_KEY_SHIFT_LEFT: case GASM_KEY_SHIFT_RIGHT: return KEY_RSHIFT;
    case GASM_KEY_ALT_LEFT: case GASM_KEY_ALT_RIGHT: return KEY_RALT;
    case GASM_KEY_COMMA: return KEY_STRAFE_L;
    case GASM_KEY_PERIOD: return KEY_STRAFE_R;
    case GASM_KEY_ENTER: case GASM_KEY_NUMPAD_ENTER: return KEY_ENTER;
    case GASM_KEY_ESCAPE: return KEY_ESCAPE;
    case GASM_KEY_TAB: return KEY_TAB;
    case GASM_KEY_BACKSPACE: return KEY_BACKSPACE;
    case GASM_KEY_MINUS: case GASM_KEY_NUMPAD_SUBTRACT: return KEY_MINUS;
    case GASM_KEY_EQUAL: case GASM_KEY_NUMPAD_ADD: return KEY_EQUALS;
    case GASM_KEY_PAUSE: return KEY_PAUSE;
    default: return 0;
    }
}

static uint8_t key_buf[256 * 4];
static uint32_t mode_set = ~0u;
static uint32_t mouse_buttons;

/* Keyboard and mouse, read raw. The mouse is locked while playing a level. */
static void poll_raw(void) {
    int n = gasm_key_events(key_buf, sizeof key_buf);
    for (int i = 0; i + 3 < n; i += 4) {
        unsigned char k = doom_key(key_buf[i] | key_buf[i + 1] << 8);
        if (k) post_key(key_buf[i + 2], k);
    }
    int playing = gamestate == GS_LEVEL && !menuactive && !demoplayback && !paused;
    uint32_t mode = GASM_INPUT_KEYS_RAW | GASM_INPUT_POINTER_HIDDEN | (playing ? GASM_INPUT_POINTER_LOCKED : 0);
    if (mode != mode_set) gasm_input_mode(mode_set = mode);

    uint8_t p[GASM_POINTER_BYTES];
    if (gasm_pointer(p, sizeof p) < 0) return;
    float dx;
    uint32_t buttons, flags;
    memcpy(&dx, p + 16, 4);
    memcpy(&buttons, p + 32, 4);
    memcpy(&flags, p + 44, 4);
    if (!(flags & GASM_POINTER_IS_LOCKED)) buttons = 0;   /* the click that locks it isn't a shot */
    int motion = (flags & GASM_POINTER_IS_LOCKED) ? (int)dx : 0;
    /* DOOM's mouse buttons: left fire, right strafe, middle forward. No vertical motion. */
    uint32_t b = (buttons & GASM_MOUSE_LEFT ? 1 : 0) | (buttons & GASM_MOUSE_RIGHT ? 2 : 0) | (buttons & GASM_MOUSE_MIDDLE ? 4 : 0);
    if (motion != 0 || b != mouse_buttons) {
        event_t ev = { .type = ev_mouse, .data1 = (int)b, .data2 = motion, .data3 = 0 };
        D_PostEvent(&ev);
        mouse_buttons = b;
    }
}

static void poll_input(void) {
    poll_raw();
    /* A tap must stay down for one tic, or the game never sees it. */
    if (tap_release) {
        post_key(0, tap_release);
        tap_release = 0;
    }
    uint32_t pad = gasm_input_pad(0);
    uint32_t changed = pad ^ prev_pad;
    prev_pad = pad;
    for (int b = 0; b < NUM_BUTTONS; b++) {
        if (!(changed & (1u << b))) continue;
        if (pad & (1u << b)) {
            if (b == BTN_X && !menuactive && !messageToPrint && !saveStringEnter) {
                unsigned char k = gamestate == GS_LEVEL && !demoplayback ? next_weapon_key() : 0;
                if (k && !tap_release) {
                    post_key(1, k);
                    tap_release = k;
                }
                continue;
            }
            unsigned char k = key_for(b);
            if (k) {
                post_key(1, k);
                held[b] = k;
            }
        } else if (held[b]) {
            post_key(0, held[b]);
            held[b] = 0;
        }
    }
}

/* ---- files: assets (read-only) and gasm:storage ------------------------------ */

static char iwad_alias[16];   /* canonical IWAD name that maps to the "wad" asset */

typedef struct {
    char key[129];
    int writing;
    char asset[256];          /* reading an asset: its name */
    uint8_t *data;            /* reading storage, or the write buffer */
    size_t len, cap;
    off_t pos, size;
} vfile;

static const char *strip_path(const char *path) {
    while (path[0] == '.' && path[1] == '/') path += 2;
    return path;
}

/* Storage key for a path: its file name, if that is a valid key. */
static int storage_key(const char *path, char *key) {
    const char *base = strrchr(path, '/');
    base = base ? base + 1 : strip_path(path);
    size_t n = strlen(base);
    if (n == 0 || n > 128 || !strcmp(base, ".") || !strcmp(base, "..")) return 0;
    for (size_t i = 0; i < n; i++)
        if (!isalnum((unsigned char)base[i]) && base[i] != '.' && base[i] != '_' && base[i] != '-') return 0;
    memcpy(key, base, n + 1);
    return 1;
}

static const char *asset_name(const char *path) {
    path = strip_path(path);
    if (iwad_alias[0] && !strcasecmp(path, iwad_alias)) return "wad";
    return path;
}

static ssize_t vf_read(void *c, char *buf, size_t size) {
    vfile *f = c;
    if (f->pos >= f->size) return 0;
    if ((off_t)size > f->size - f->pos) size = (size_t)(f->size - f->pos);
    if (f->asset[0]) {
        int32_t n = gasm_asset_read_at(f->asset, (uint32_t)strlen(f->asset), (uint32_t)f->pos, buf, (uint32_t)size);
        if (n < 0) return -1;
        size = (size_t)n;
    } else {
        memcpy(buf, f->data + f->pos, size);
    }
    f->pos += (off_t)size;
    return (ssize_t)size;
}

static ssize_t vf_write(void *c, const char *buf, size_t size) {
    vfile *f = c;
    size_t end = (size_t)f->pos + size;
    if (end > f->cap) {
        size_t cap = f->cap ? f->cap : 4096;
        while (cap < end) cap *= 2;
        uint8_t *d = realloc(f->data, cap);
        if (!d) return -1;
        f->data = d;
        f->cap = cap;
    }
    if ((size_t)f->pos > f->len) memset(f->data + f->len, 0, (size_t)f->pos - f->len);
    memcpy(f->data + f->pos, buf, size);
    f->pos = (off_t)end;
    if (end > f->len) f->len = end;
    f->size = (off_t)f->len;
    return (ssize_t)size;
}

/* Save games use ftell() while writing (for alignment padding), so writes seek too. */
static int vf_seek(void *c, off_t *off, int whence) {
    vfile *f = c;
    off_t base = whence == SEEK_SET ? 0 : whence == SEEK_CUR ? f->pos : f->size;
    if (base + *off < 0) return -1;
    f->pos = base + *off;
    *off = f->pos;
    return 0;
}

static int vf_close(void *c) {
    vfile *f = c;
    int rc = 0;
    if (f->writing && gasm_storage_set(f->key, (uint32_t)strlen(f->key), f->data, (uint32_t)f->len) != 0) {
        fprintf(stderr, "doom: could not save %s\n", f->key);
        rc = -1;
    }
    free(f->data);
    free(f);
    return rc;
}

FILE *gasm_doom_fopen(const char *path, const char *mode) {
    vfile *f = calloc(1, sizeof *f);
    if (!f) return NULL;
    int have_key = storage_key(path, f->key);
    if (strchr(mode, 'w') || strchr(mode, 'a')) {
        if (!have_key) goto fail;
        f->writing = 1;
    } else {
        int32_t n = have_key ? gasm_storage_get(f->key, (uint32_t)strlen(f->key), NULL, 0) : -1;
        if (n >= 0) {
            f->data = malloc(n ? (size_t)n : 1);
            if (!f->data) goto fail;
            gasm_storage_get(f->key, (uint32_t)strlen(f->key), f->data, (uint32_t)n);
            f->size = n;
        } else {
            const char *name = asset_name(path);
            int32_t size = gasm_asset_size(name, (uint32_t)strlen(name));
            if (size < 0) goto fail;
            if (strlen(name) >= sizeof f->asset) goto fail;
            strcpy(f->asset, name);
            f->size = size;
        }
    }
    cookie_io_functions_t io = { vf_read, vf_write, vf_seek, vf_close };
    FILE *file = fopencookie(f, mode, io);
    if (file) return file;
fail:
    free(f->data);
    free(f);
    errno = ENOENT;
    return NULL;
}

int gasm_doom_remove(const char *path) {
    char key[129];
    if (!storage_key(path, key)) return -1;
    return gasm_storage_delete(key, (uint32_t)strlen(key)) == 0 ? 0 : -1;
}

int gasm_doom_rename(const char *from, const char *to) {
    char a[129], b[129];
    if (!storage_key(from, a) || !storage_key(to, b)) return -1;
    int32_t n = gasm_storage_get(a, (uint32_t)strlen(a), NULL, 0);
    if (n < 0) return -1;
    uint8_t *data = malloc(n ? (size_t)n : 1);
    if (!data) return -1;
    gasm_storage_get(a, (uint32_t)strlen(a), data, (uint32_t)n);
    int rc = gasm_storage_set(b, (uint32_t)strlen(b), data, (uint32_t)n);
    free(data);
    if (rc != 0) return -1;
    gasm_storage_delete(a, (uint32_t)strlen(a));
    return 0;
}

/* Directories don't exist; storage keys are flat. */
int gasm_doom_mkdir(const char *path, ...) {
    (void)path;
    return 0;
}

/* ---- IWAD detection ------------------------------------------------------------ */

static int32_t read_le32(const uint8_t *p) { return (int32_t)(p[0] | p[1] << 8 | p[2] << 16 | (uint32_t)p[3] << 24); }

/* Name the "wad" asset after the IWAD it is, so the engine picks the right game. */
static const char *identify_wad(void) {
    uint8_t hdr[12];
    if (gasm_asset_read_at("wad", 3, 0, hdr, 12) != 12 || memcmp(hdr + 1, "WAD", 3)) return NULL;
    int32_t count = read_le32(hdr + 4), table = read_le32(hdr + 8);
    if (count <= 0 || count > 65536 || table < 0) return NULL;
    uint8_t *dir = malloc((size_t)count * 16);
    if (!dir) return NULL;
    int doom2 = 0, freedoom = 0;
    if (gasm_asset_read_at("wad", 3, (uint32_t)table, dir, (uint32_t)count * 16) == count * 16) {
        for (int32_t i = 0; i < count; i++) {
            const char *name = (const char *)dir + i * 16 + 8;
            if (!strncmp(name, "MAP01", 8)) doom2 = 1;
            if (!strncmp(name, "FREEDOOM", 8)) freedoom = 1;
        }
    }
    free(dir);
    if (freedoom) return doom2 ? "freedoom2.wad" : "freedoom1.wad";
    return doom2 ? "doom2.wad" : "doom.wad"; /* doom.wad covers shareware, registered and Ultimate */
}

/* ---- sound effects ------------------------------------------------------------- */

#define NUM_CHANNELS 16

typedef struct {
    const uint8_t *samples;   /* unsigned 8-bit */
    uint32_t length;
    uint64_t pos;             /* 32.32 fixed point, in source samples */
    uint64_t step;
    int left, right;          /* 0..255 */
    int playing;
} channel;

static channel channels[NUM_CHANNELS];
static boolean sfx_prefix;

static boolean snd_init(boolean use_sfx_prefix) {
    sfx_prefix = use_sfx_prefix;
    memset(channels, 0, sizeof channels);
    return true;
}

static void snd_shutdown(void) { memset(channels, 0, sizeof channels); }

static int snd_lump(sfxinfo_t *sfx) {
    char name[16];
    if (sfx->link) sfx = sfx->link;
    snprintf(name, sizeof name, sfx_prefix ? "ds%s" : "%s", sfx->name);
    return W_CheckNumForName(name);
}

static void snd_update(void) {}

static void set_params(channel *c, int vol, int sep) {
    if (vol < 0) vol = 0;
    if (vol > 127) vol = 127;
    if (sep < 0) sep = 0;
    if (sep > 254) sep = 254;
    c->left = (254 - sep) * vol / 127;
    c->right = sep * vol / 127;
    if (c->left > 255) c->left = 255;
    if (c->right > 255) c->right = 255;
}

static void snd_update_params(int ch, int vol, int sep) {
    if (ch >= 0 && ch < NUM_CHANNELS) set_params(&channels[ch], vol, sep);
}

/* DMX sound lump: u16 format (3), u16 rate, u32 length, then samples with 16
 * padding bytes at each end (DMX skipped them, so do we). */
static int snd_start(sfxinfo_t *sfx, int ch, int vol, int sep) {
    if (ch < 0 || ch >= NUM_CHANNELS || sfx->lumpnum < 0) return -1;
    const uint8_t *d = W_CacheLumpNum(sfx->lumpnum, PU_STATIC);
    int lumplen = W_LumpLength(sfx->lumpnum);
    if (lumplen < 8 || d[0] != 3 || d[1] != 0) return -1;
    uint32_t rate = d[2] | d[3] << 8;
    uint32_t length = (uint32_t)read_le32(d + 4);
    if (length > (uint32_t)lumplen - 8 || length <= 48 || rate == 0) return -1;
    channel *c = &channels[ch];
    c->samples = d + 8 + 16;
    c->length = length - 32;
    c->pos = 0;
    c->step = ((uint64_t)rate << 32) / RATE;
    set_params(c, vol, sep);
    c->playing = 1;
    return ch;
}

static void snd_stop(int ch) {
    if (ch >= 0 && ch < NUM_CHANNELS) channels[ch].playing = 0;
}

static boolean snd_is_playing(int ch) {
    return ch >= 0 && ch < NUM_CHANNELS && channels[ch].playing;
}

static void snd_cache(sfxinfo_t *sounds, int num) {
    for (int i = 0; i < num; i++) sounds[i].lumpnum = snd_lump(&sounds[i]);
}

static snddevice_t sound_devices[] = {
    SNDDEVICE_SB, SNDDEVICE_PAS, SNDDEVICE_GUS, SNDDEVICE_WAVEBLASTER,
    SNDDEVICE_SOUNDCANVAS, SNDDEVICE_AWE32,
};

sound_module_t DG_sound_module = {
    sound_devices, sizeof sound_devices / sizeof sound_devices[0],
    snd_init, snd_shutdown, snd_lump, snd_update, snd_update_params,
    snd_start, snd_stop, snd_is_playing, snd_cache,
};

/* i_sound.c binds these config variables (they belong to SDL's resampler). */
int use_libsamplerate = 0;
float libsamplerate_scale = 0.65f;

static int32_t mix[FRAME_SAMPLES * 2];
static float audio_out[FRAME_SAMPLES * 2];

static void mix_audio(void) {
    gasm_opl_render(mix, FRAME_SAMPLES);
    for (int ch = 0; ch < NUM_CHANNELS; ch++) {
        channel *c = &channels[ch];
        if (!c->playing) continue;
        for (int i = 0; i < FRAME_SAMPLES; i++) {
            uint32_t at = (uint32_t)(c->pos >> 32);
            if (at >= c->length) {
                c->playing = 0;
                break;
            }
            /* linear interpolation between neighbouring source samples */
            int s0 = c->samples[at] - 128;
            int s1 = at + 1 < c->length ? c->samples[at + 1] - 128 : s0;
            int frac = (int)((c->pos >> 16) & 0xffff);
            int s = (s0 << 8) + (((s1 - s0) * frac) >> 8);
            mix[2 * i] += s * c->left >> 8;
            mix[2 * i + 1] += s * c->right >> 8;
            c->pos += c->step;
        }
    }
    for (int i = 0; i < FRAME_SAMPLES * 2; i++) {
        int32_t v = mix[i];
        if (v > 32767) v = 32767;
        if (v < -32768) v = -32768;
        audio_out[i] = (float)v / 32768.0f;
    }
    gasm_audio_push(audio_out, FRAME_SAMPLES);
}

/* ---- gasm exports -------------------------------------------------------------- */

#define MAX_ARGS 64

static char *argv_buf[MAX_ARGS];
static char args_param[1024];

GASM_EXPORT("gasm_abi_version") int32_t abi_version(void) { return GASM_ABI_VERSION; }

/*
 * Params: "args" is a DOOM command line, e.g. "-warp 1 3 -skill 4" or
 * "-playdemo demo1". Assets: "wad" is the IWAD (any name); otherwise the
 * standard names (doom1.wad, doom2.wad, freedoom1.wad, ...) are searched.
 */
GASM_EXPORT("gasm_init") int32_t init(void) {
    setvbuf(stdout, NULL, _IOLBF, 0); /* engine messages reach the log line by line */
    gasm_set_frame_rate(TICRATE_HZ);
    gasm_audio_config(RATE, 2);
    snd_samplerate = RATE;

    int argc = 0;
    argv_buf[argc++] = "doom";
    if (gasm_asset_size("wad", 3) >= 0) {
        const char *name = identify_wad();
        if (!name) {
            gasm_log_str("doom: asset 'wad' is not a WAD file");
            return 1;
        }
        snprintf(iwad_alias, sizeof iwad_alias, "%s", name);
        argv_buf[argc++] = "-iwad";
        argv_buf[argc++] = iwad_alias;
    }
    if (gasm_param_str("args", args_param, sizeof args_param)) {
        for (char *tok = strtok(args_param, " \t"); tok && argc < MAX_ARGS - 1; tok = strtok(NULL, " \t"))
            argv_buf[argc++] = tok;
    }
    argv_buf[argc] = NULL;

    doomgeneric_Create(argc, argv_buf);
    return 0;
}

GASM_EXPORT("gasm_frame") void frame(void) {
    frame_count++;
    poll_input();
    doomgeneric_Tick();
    gasm_video_present(fb, OUT_W, OUT_H, OUT_W * 4);
    mix_audio();
}

/* The player is quitting: keep the settings (save games are written at once). */
GASM_EXPORT("gasm_exit") void quit(void) { M_SaveDefaults(); }
