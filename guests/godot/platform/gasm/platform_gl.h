/**************************************************************************/
/*  platform_gl.h (gasm): OpenGL ES 3.0 on gasm:gl, the C SDK's headers    */
/**************************************************************************/
#pragma once

#ifndef GLES_API_ENABLED
#define GLES_API_ENABLED // Allow using GLES.
#endif

#include <GLES3/gl3.h>
#include <GLES2/gl2ext.h>

// WebGL 2 has no OVR multiview (gasm:gl has no entry points for it); Godot only uses
// these where the extension is reported, which it never is here.
#define GL_FRAMEBUFFER_ATTACHMENT_TEXTURE_NUM_VIEWS_OVR 0x9630
#define GL_FRAMEBUFFER_ATTACHMENT_TEXTURE_BASE_VIEW_INDEX_OVR 0x9632
#define GL_MAX_VIEWS_OVR 0x9631
#define GL_FRAMEBUFFER_INCOMPLETE_VIEW_TARGETS_OVR 0x9633
static inline void glFramebufferTextureMultiviewOVR(GLenum, GLenum, GLuint, GLint, GLint, GLsizei) {}
static inline void glFramebufferTextureMultisampleMultiviewOVR(GLenum, GLenum, GLuint, GLint, GLsizei, GLint, GLsizei) {}
