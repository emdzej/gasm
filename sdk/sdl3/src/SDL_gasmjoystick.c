/*
  SDL3 on gasm: joystick driver for gasm's four gamepad slots.

  Each connected slot is a joystick with its buttons and axes in gasm's order.
  Pads with the W3C standard mapping are also SDL gamepads: the analog triggers
  (buttons 6 and 7) become two extra axes, and the driver supplies the mapping.

  SPDX-License-Identifier: Zlib
*/
#include "SDL_internal.h"

#ifdef SDL_JOYSTICK_PRIVATE

#include "joystick/SDL_sysjoystick.h"

#include "SDL_gasm_c.h"
#include "gasm.h"

#define SLOTS 4

typedef struct
{
    SDL_JoystickID id;     /* 0 = slot empty */
    bool standard;
    int nbuttons, naxes;
    char name[64];
    SDL_GUID guid;
    SDL_Joystick *joystick;
    float buttons[GASM_GAMEPAD_BUTTONS], axes[GASM_GAMEPAD_AXES + 2];
} Slot;

static Slot slots[SLOTS];

static float F32(const Uint8 *p)
{
    float f;
    SDL_memcpy(&f, p, 4);
    return f;
}

static Uint32 U32(const Uint8 *p)
{
    return (Uint32)p[0] | (Uint32)p[1] << 8 | (Uint32)p[2] << 16 | (Uint32)p[3] << 24;
}

static bool ReadSlot(int i, Uint8 *raw)
{
    return gasm_gamepad((Uint32)i, raw, GASM_GAMEPAD_BYTES) == GASM_GAMEPAD_BYTES &&
           (U32(raw) & GASM_GAMEPAD_CONNECTED);
}

/* Connected devices, in slot order (device_index counts only those). */
static Slot *ByIndex(int device_index)
{
    for (int i = 0; i < SLOTS; i++) {
        if (slots[i].id && device_index-- == 0) {
            return &slots[i];
        }
    }
    return NULL;
}

static void GASM_JoystickDetect(void)
{
    Uint8 raw[GASM_GAMEPAD_BYTES];
    for (int i = 0; i < SLOTS; i++) {
        Slot *s = &slots[i];
        const bool connected = ReadSlot(i, raw);
        if (s->id && !connected) {
            const SDL_JoystickID id = s->id;
            s->id = 0;
            SDL_PrivateJoystickRemoved(id);
        } else if (!s->id && connected) {
            SDL_zerop(s);
            s->standard = (U32(raw) & GASM_GAMEPAD_STANDARD) != 0;
            s->nbuttons = (int)SDL_min(U32(raw + 4), GASM_GAMEPAD_BUTTONS);
            s->naxes = (int)SDL_min(U32(raw + 8), GASM_GAMEPAD_AXES);
            if (s->standard) {
                s->naxes = 6;   /* sticks + analog triggers */
            }
            if (gasm_gamepad_name((Uint32)i, s->name, sizeof s->name - 1) <= 0) {
                SDL_snprintf(s->name, sizeof s->name, "gasm gamepad %d", i + 1);
            }
            s->guid = SDL_CreateJoystickGUID(SDL_HARDWARE_BUS_VIRTUAL, 0, 0, 0, NULL, s->name, 'g', s->standard ? 1 : 0);
            s->id = SDL_GetNextObjectID();
            SDL_PrivateJoystickAdded(s->id);
        }
    }
}

static bool GASM_JoystickInit(void)
{
    GASM_JoystickDetect();
    return true;
}

static int GASM_JoystickGetCount(void)
{
    int n = 0;
    for (int i = 0; i < SLOTS; i++) {
        n += slots[i].id != 0;
    }
    return n;
}

static bool GASM_JoystickIsDevicePresent(Uint16 vendor_id, Uint16 product_id, Uint16 version, const char *name)
{
    (void)vendor_id; (void)product_id; (void)version; (void)name;
    return false;
}

static const char *GASM_JoystickGetDeviceName(int device_index)
{
    Slot *s = ByIndex(device_index);
    return s ? s->name : NULL;
}

static const char *GASM_JoystickGetDevicePath(int device_index)
{
    (void)device_index;
    return NULL;
}

static int GASM_JoystickGetDeviceSteamVirtualGamepadSlot(int device_index)
{
    (void)device_index;
    return -1;
}

static int GASM_JoystickGetDevicePlayerIndex(int device_index)
{
    Slot *s = ByIndex(device_index);
    return s ? (int)(s - slots) : -1;
}

static void GASM_JoystickSetDevicePlayerIndex(int device_index, int player_index)
{
    (void)device_index; (void)player_index;
}

static SDL_GUID GASM_JoystickGetDeviceGUID(int device_index)
{
    Slot *s = ByIndex(device_index);
    SDL_GUID none;
    SDL_zero(none);
    return s ? s->guid : none;
}

static SDL_JoystickID GASM_JoystickGetDeviceInstanceID(int device_index)
{
    Slot *s = ByIndex(device_index);
    return s ? s->id : 0;
}

static bool GASM_JoystickOpen(SDL_Joystick *joystick, int device_index)
{
    Slot *s = ByIndex(device_index);
    if (!s) {
        return SDL_SetError("No such device");
    }
    joystick->hwdata = (struct joystick_hwdata *)s;
    joystick->nbuttons = s->nbuttons;
    joystick->naxes = s->naxes;
    joystick->nhats = 0;
    s->joystick = joystick;
    for (int i = 0; i < SDL_arraysize(s->axes); i++) {
        s->axes[i] = 0.0f;
    }
    if (s->standard) {   /* released triggers rest at -1 */
        s->axes[4] = s->axes[5] = -1.0f;
        SDL_SendJoystickAxis(0, joystick, 4, SDL_JOYSTICK_AXIS_MIN);
        SDL_SendJoystickAxis(0, joystick, 5, SDL_JOYSTICK_AXIS_MIN);
    }
    return true;
}

static Sint16 AxisValue(float v)
{
    v = SDL_clamp(v, -1.0f, 1.0f);
    return (Sint16)(v < 0 ? v * 32768.0f : v * 32767.0f);
}

static void GASM_JoystickUpdate(SDL_Joystick *joystick)
{
    Slot *s = (Slot *)joystick->hwdata;
    Uint8 raw[GASM_GAMEPAD_BYTES];
    if (!s || !s->id || !ReadSlot((int)(s - slots), raw)) {
        return;
    }
    const Uint64 ts = SDL_GetTicksNS();
    for (int i = 0; i < s->nbuttons; i++) {
        const float v = F32(raw + 12 + 4 * i);
        if ((v > 0.5f) != (s->buttons[i] > 0.5f)) {
            SDL_SendJoystickButton(ts, joystick, (Uint8)i, v > 0.5f);
        }
        s->buttons[i] = v;
    }
    float axes[GASM_GAMEPAD_AXES + 2];
    for (int i = 0; i < s->naxes; i++) {
        axes[i] = F32(raw + 140 + 4 * i);
    }
    if (s->standard) {   /* triggers 0..1 -> -1..1 */
        axes[4] = F32(raw + 12 + 4 * 6) * 2.0f - 1.0f;
        axes[5] = F32(raw + 12 + 4 * 7) * 2.0f - 1.0f;
    }
    for (int i = 0; i < s->naxes; i++) {
        if (axes[i] != s->axes[i]) {
            SDL_SendJoystickAxis(ts, joystick, (Uint8)i, AxisValue(axes[i]));
            s->axes[i] = axes[i];
        }
    }
}

static void GASM_JoystickClose(SDL_Joystick *joystick)
{
    Slot *s = (Slot *)joystick->hwdata;
    if (s) {
        s->joystick = NULL;
    }
    joystick->hwdata = NULL;
}

static void GASM_JoystickQuit(void)
{
    SDL_zeroa(slots);
}

static bool GASM_JoystickGetGamepadMapping(int device_index, SDL_GamepadMapping *out)
{
    Slot *s = ByIndex(device_index);
    if (!s || !s->standard) {
        return false;
    }
#define B(field, n) out->field.kind = EMappingKind_Button, out->field.target = n
#define A(field, n) out->field.kind = EMappingKind_Axis, out->field.target = n
    B(a, 0); B(b, 1); B(x, 2); B(y, 3);   /* by position: south, east, west, north */
    B(leftshoulder, 4); B(rightshoulder, 5);
    B(back, 8); B(start, 9); B(leftstick, 10); B(rightstick, 11);
    B(dpup, 12); B(dpdown, 13); B(dpleft, 14); B(dpright, 15);
    if (s->nbuttons > 16) {
        B(guide, 16);
    }
    A(leftx, 0); A(lefty, 1); A(rightx, 2); A(righty, 3);
    A(lefttrigger, 4); A(righttrigger, 5);
#undef B
#undef A
    return true;
}

static bool GASM_JoystickRumble(SDL_Joystick *joystick, Uint16 low, Uint16 high)
{
    (void)joystick; (void)low; (void)high;
    return SDL_Unsupported();
}

static bool GASM_JoystickRumbleTriggers(SDL_Joystick *joystick, Uint16 left, Uint16 right)
{
    (void)joystick; (void)left; (void)right;
    return SDL_Unsupported();
}

static bool GASM_JoystickSetLED(SDL_Joystick *joystick, Uint8 red, Uint8 green, Uint8 blue)
{
    (void)joystick; (void)red; (void)green; (void)blue;
    return SDL_Unsupported();
}

static bool GASM_JoystickSendEffect(SDL_Joystick *joystick, const void *data, int size)
{
    (void)joystick; (void)data; (void)size;
    return SDL_Unsupported();
}

static bool GASM_JoystickSetSensorsEnabled(SDL_Joystick *joystick, bool enabled)
{
    (void)joystick; (void)enabled;
    return SDL_Unsupported();
}

SDL_JoystickDriver SDL_PRIVATE_JoystickDriver = {
    GASM_JoystickInit,
    GASM_JoystickGetCount,
    GASM_JoystickDetect,
    GASM_JoystickIsDevicePresent,
    GASM_JoystickGetDeviceName,
    GASM_JoystickGetDevicePath,
    GASM_JoystickGetDeviceSteamVirtualGamepadSlot,
    GASM_JoystickGetDevicePlayerIndex,
    GASM_JoystickSetDevicePlayerIndex,
    GASM_JoystickGetDeviceGUID,
    GASM_JoystickGetDeviceInstanceID,
    GASM_JoystickOpen,
    GASM_JoystickRumble,
    GASM_JoystickRumbleTriggers,
    GASM_JoystickSetLED,
    GASM_JoystickSendEffect,
    GASM_JoystickSetSensorsEnabled,
    GASM_JoystickUpdate,
    GASM_JoystickClose,
    GASM_JoystickQuit,
    GASM_JoystickGetGamepadMapping
};

#endif /* SDL_JOYSTICK_PRIVATE */
