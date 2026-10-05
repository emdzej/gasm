/*
  SDL3 build configuration for gasm (https://gasm.emdzej.pl): wasm32 with
  wasi-libc, one thread, gasm's own drivers (SDL "private platform").

  SPDX-License-Identifier: Zlib
*/
#ifndef SDL_build_config_private_h_
#define SDL_build_config_private_h_
#define SDL_build_config_h_

#include <SDL3/SDL_platform_defines.h>

#define SDL_PLATFORM_PRIVATE_NAME "gasm"

/* Headers and C library (wasi-libc) */
#define HAVE_FLOAT_H 1
#define HAVE_INTTYPES_H 1
#define HAVE_LIMITS_H 1
#define HAVE_MATH_H 1
#define HAVE_STDARG_H 1
#define HAVE_STDDEF_H 1
#define HAVE_STDINT_H 1
#define HAVE_STDIO_H 1
#define HAVE_STDLIB_H 1
#define HAVE_STRING_H 1
#define HAVE_SYS_TYPES_H 1
#define HAVE_WCHAR_H 1

#define HAVE_LIBC 1
#define HAVE_MALLOC 1
#define HAVE_GETENV 1
#define HAVE_ABS 1
#define HAVE_MEMSET 1
#define HAVE_MEMCPY 1
#define HAVE_MEMMOVE 1
#define HAVE_MEMCMP 1
#define HAVE_STRLEN 1
#define HAVE_STRNLEN 1
#define HAVE_STRPBRK 1
#define HAVE_STRCHR 1
#define HAVE_STRRCHR 1
#define HAVE_STRSTR 1
#define HAVE_STRTOK_R 1
#define HAVE_STRTOL 1
#define HAVE_STRTOUL 1
#define HAVE_STRTOLL 1
#define HAVE_STRTOULL 1
#define HAVE_STRTOD 1
#define HAVE_ATOI 1
#define HAVE_ATOF 1
#define HAVE_STRCMP 1
#define HAVE_STRNCMP 1
#define HAVE_VSSCANF 1
#define HAVE_VSNPRINTF 1
#define HAVE_ACOS 1
#define HAVE_ACOSF 1
#define HAVE_ASIN 1
#define HAVE_ASINF 1
#define HAVE_ATAN 1
#define HAVE_ATANF 1
#define HAVE_ATAN2 1
#define HAVE_ATAN2F 1
#define HAVE_CEIL 1
#define HAVE_CEILF 1
#define HAVE_COPYSIGN 1
#define HAVE_COPYSIGNF 1
#define HAVE_COS 1
#define HAVE_COSF 1
#define HAVE_EXP 1
#define HAVE_EXPF 1
#define HAVE_FABS 1
#define HAVE_FABSF 1
#define HAVE_FLOOR 1
#define HAVE_FLOORF 1
#define HAVE_FMOD 1
#define HAVE_FMODF 1
#define HAVE_ISINF 1
#define HAVE_ISINF_FLOAT_MACRO 1
#define HAVE_ISNAN 1
#define HAVE_ISNAN_FLOAT_MACRO 1
#define HAVE_LOG 1
#define HAVE_LOGF 1
#define HAVE_LOG10 1
#define HAVE_LOG10F 1
#define HAVE_LROUND 1
#define HAVE_LROUNDF 1
#define HAVE_MODF 1
#define HAVE_MODFF 1
#define HAVE_POW 1
#define HAVE_POWF 1
#define HAVE_ROUND 1
#define HAVE_ROUNDF 1
#define HAVE_SCALBN 1
#define HAVE_SCALBNF 1
#define HAVE_SIN 1
#define HAVE_SINF 1
#define HAVE_SQRT 1
#define HAVE_SQRTF 1
#define HAVE_TAN 1
#define HAVE_TANF 1
#define HAVE_TRUNC 1
#define HAVE_TRUNCF 1
#define HAVE_GCC_ATOMICS 1

/* No SIMD paths (SDL's are x86/ARM/LoongArch) */
#define SDL_DISABLE_MMX 1
#define SDL_DISABLE_SSE 1
#define SDL_DISABLE_SSE2 1
#define SDL_DISABLE_SSE3 1
#define SDL_DISABLE_SSE4_1 1
#define SDL_DISABLE_SSE4_2 1
#define SDL_DISABLE_AVX 1
#define SDL_DISABLE_AVX2 1
#define SDL_DISABLE_AVX512F 1
#define SDL_DISABLE_NEON 1
#define SDL_DISABLE_LSX 1
#define SDL_DISABLE_LASX 1

/* gasm drivers (sdk/sdl3/src) */
#define SDL_VIDEO_DRIVER_PRIVATE 1     /* video_present, raw keyboard/pointer/text input */
#define SDL_AUDIO_DRIVER_PRIVATE 1     /* audio_push, one frame of samples per gasm frame */
#define SDL_JOYSTICK_PRIVATE 1         /* gamepad(): W3C standard mapping */
#define SDL_JOYSTICK_VIRTUAL 1
#define SDL_PRIVATE_GAMEPAD_DEFINITIONS   /* none: standard-mapped pads describe themselves */
#define SDL_VIDEO_RENDER_SW 1          /* SDL_Renderer draws in software */
/* OpenGL ES on gasm:gl, for games linked with gasm_gl.o (src/SDL_gasmopengles.c);
   SDL_Renderer then also draws with GLES 2 (on the GPU), otherwise in software */
#define SDL_VIDEO_OPENGL_ES2 1
#define SDL_VIDEO_RENDER_OGL_ES2 1
/* SDL is built against its own GL headers (games use the C SDK's GLES3/gl3.h) */
#define SDL_USE_BUILTIN_OPENGL_DEFINITIONS 1

/* Threads: cooperative, on the guest's one wasm thread (src/SDL_gasmthread.c on the
   C SDK's gasm_thread.h); SDL_CreateThread needs the threaded loop helper. The flag
   stays: it is how SDL picks the generic thread handle type, and the generic files
   it would stub out are replaced (SDL_SKIP). Of the rest, only SDL_assert.c reads it
   (no lock around assertion reports). */
#define SDL_THREADS_DISABLED 1

/* Everything else is stubbed */
#define SDL_HAPTIC_DISABLED 1
#define SDL_HIDAPI_DISABLED 1
#define SDL_SENSOR_DISABLED 1
#define SDL_POWER_DISABLED 1
#define SDL_LOADSO_DUMMY 1
#define SDL_PROCESS_DUMMY 1
#define SDL_CAMERA_DRIVER_DUMMY 1
#define SDL_DIALOG_DUMMY 1
#define SDL_TRAY_DUMMY 1
#define SDL_LOCALE_DUMMY 1
#define SDL_MISC_DUMMY 1

#endif /* SDL_build_config_private_h_ */
