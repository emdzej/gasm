/*
  SDL3 on gasm: audio driver ("gasm").

  The default playback device is gasm's audio_push: float samples, mono or
  stereo. Each gasm frame mixes exactly one frame of samples (800 at 48 kHz), so
  audio is part of the frame like video and as deterministic. No recording.

  SPDX-License-Identifier: Zlib
*/
#include "SDL_internal.h"

#ifdef SDL_AUDIO_DRIVER_PRIVATE

#include "audio/SDL_sysaudio.h"

#include "SDL_gasm_c.h"
#include "gasm.h"

#define FRAME_RATE 60

struct SDL_PrivateAudioData
{
    Uint8 *mixbuf;
};

static SDL_AudioDevice *playback;

void SDL_GASM_AudioFrame(void)
{
    if (playback) {
        SDL_PlaybackAudioThreadIterate(playback);
    }
}

static bool GASMAUDIO_OpenDevice(SDL_AudioDevice *device)
{
    if (device->recording) {
        return SDL_SetError("gasm has no audio recording");
    }
    device->hidden = (struct SDL_PrivateAudioData *)SDL_calloc(1, sizeof(*device->hidden));
    if (!device->hidden) {
        return false;
    }
    device->spec.format = SDL_AUDIO_F32;
    if (device->spec.channels > 2) {
        device->spec.channels = 2;   /* SDL downmixes */
    }
    /* whole samples per frame, within gasm's 8-192 kHz; otherwise SDL resamples */
    if (device->spec.freq < 8000 || device->spec.freq > 192000 || device->spec.freq % FRAME_RATE) {
        device->spec.freq = 48000;
    }
    device->sample_frames = device->spec.freq / FRAME_RATE;
    SDL_UpdatedAudioDeviceFormat(device);

    device->hidden->mixbuf = (Uint8 *)SDL_malloc(device->buffer_size);
    if (!device->hidden->mixbuf) {
        return false;
    }
    SDL_memset(device->hidden->mixbuf, device->silence_value, device->buffer_size);
    gasm_audio_config((Uint32)device->spec.freq, (Uint32)device->spec.channels);
    playback = device;
    return true;
}

static bool GASMAUDIO_PlayDevice(SDL_AudioDevice *device, const Uint8 *buffer, int buflen)
{
    gasm_audio_push((const float *)buffer, (Uint32)(buflen / SDL_AUDIO_FRAMESIZE(device->spec)));
    return true;
}

static Uint8 *GASMAUDIO_GetDeviceBuf(SDL_AudioDevice *device, int *buffer_size)
{
    (void)buffer_size;
    return device->hidden->mixbuf;
}

static void GASMAUDIO_CloseDevice(SDL_AudioDevice *device)
{
    if (device == playback) {
        playback = NULL;
    }
    if (device->hidden) {
        SDL_free(device->hidden->mixbuf);
        SDL_free(device->hidden);
        device->hidden = NULL;
    }
}

static bool GASMAUDIO_Init(SDL_AudioDriverImpl *impl)
{
    impl->OpenDevice = GASMAUDIO_OpenDevice;
    impl->PlayDevice = GASMAUDIO_PlayDevice;
    impl->GetDeviceBuf = GASMAUDIO_GetDeviceBuf;
    impl->CloseDevice = GASMAUDIO_CloseDevice;
    impl->OnlyHasDefaultPlaybackDevice = true;
    impl->ProvidesOwnCallbackThread = true;   /* SDL_GASM_AudioFrame drives it */
    return true;
}

AudioBootStrap PRIVATEAUDIO_bootstrap = {
    "gasm", "gasm audio driver", GASMAUDIO_Init, false, false
};

#endif /* SDL_AUDIO_DRIVER_PRIVATE */
