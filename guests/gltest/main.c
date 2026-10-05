/*
 * gltest: OpenGL ES 3.0 through the C SDK's drop-in headers (<GLES3/gl3.h> on
 * gasm:gl). A spinning textured quad with instanced copies, a uniform block, an
 * offscreen framebuffer pass, a mapped buffer, a query and a fence; and a run of
 * deliberate mistakes whose GL errors are uploaded each frame, so the video hash
 * covers the error behaviour as well as every upload (both runners must agree).
 *
 * SPDX-License-Identifier: MIT
 */
#include <GLES3/gl3.h>
#include <math.h>
#include <stdint.h>
#include <stdio.h>
#include <string.h>

#include "gasm.h"

GASM_EXPORT("gasm_abi_version") int32_t abi_version(void) { return GASM_ABI_VERSION; }
GASM_TITLE("GLES 3 test");

static const char *VS =
    "#version 300 es\n"
    "layout(location = 0) in vec2 pos;\n"
    "layout(location = 1) in vec2 uv;\n"
    "layout(std140) uniform Scene { vec4 tint; float spread; };\n"
    "uniform mat2 rot;\n"
    "out vec2 v_uv;\n"
    "void main() {\n"
    "  float i = float(gl_InstanceID);\n"
    "  vec2 p = rot * pos * (0.6 - i * 0.12) + vec2((i - 1.0) * spread, 0.0);\n"
    "  v_uv = uv;\n"
    "  gl_Position = vec4(p, 0.0, 1.0);\n"
    "}\n";
static const char *FS =
    "#version 300 es\n"
    "precision highp float;\n"
    "layout(std140) uniform Scene { vec4 tint; float spread; };\n"
    "uniform sampler2D tex;\n"
    "in vec2 v_uv;\n"
    "out vec4 color;\n"
    "void main() { color = texture(tex, v_uv) * tint; }\n";

static GLuint prog, vao, vbo, ibo, ubo, tex, fbo, fbo_tex, errbuf, query;
static GLint rot_loc;
static GLsync fence;
static unsigned frame;
static uint32_t errors[64];   /* (code) of each mistake, uploaded every frame */
static unsigned nerr;

static void note(void) {
    GLenum e;
    while ((e = glGetError()) != GL_NO_ERROR)
        if (nerr < 64) errors[nerr++] = e;
    if (nerr < 64) errors[nerr++] = 0;   /* separator: one per step */
}

static GLuint shader(GLenum type, const char *src) {
    GLuint s = glCreateShader(type);
    glShaderSource(s, 1, &src, NULL);
    glCompileShader(s);
    GLint ok = 0;
    glGetShaderiv(s, GL_COMPILE_STATUS, &ok);
    if (!ok) {
        char log[512];
        glGetShaderInfoLog(s, sizeof log, NULL, log);
        gasm_log_str(log);
    }
    return s;
}

/* Mistakes with known GLES 3.0 / WebGL 2 errors (each followed by note()). */
static void mistakes(void) {
    glBindBuffer(GL_ARRAY_BUFFER, 12345);            note();   /* INVALID_OPERATION: not a name */
    glBindBuffer(0x1234, vbo);                       note();   /* INVALID_ENUM */
    glBindBuffer(GL_COPY_READ_BUFFER, 0);
    glBufferSubData(GL_COPY_READ_BUFFER, 0, 4, errors); note(); /* INVALID_OPERATION: nothing bound */
    glDrawArrays(GL_TRIANGLES, 0, -1);               note();   /* INVALID_VALUE */
    glPixelStorei(GL_UNPACK_ALIGNMENT, 3);           note();   /* INVALID_VALUE */
    glActiveTexture(GL_TEXTURE0 + 40);               note();   /* INVALID_ENUM */
    glUniform1f(-1, 1.0f);                           note();   /* ignored: no error */
    glUniform1f(rot_loc + 100, 1.0f);                note();   /* INVALID_OPERATION: unknown location */
    GLuint s = glCreateShader(0x1234);               note();   /* INVALID_ENUM, returns 0 */
    glCompileShader(s);                              note();   /* INVALID_VALUE: 0 */
    glFenceSync(GL_SYNC_GPU_COMMANDS_COMPLETE, 1);   note();   /* INVALID_VALUE: flags */
    glEndQuery(GL_ANY_SAMPLES_PASSED);               note();   /* INVALID_OPERATION: none active */
    GLuint b;
    glGenBuffers(1, &b);
    glDeleteBuffers(1, &b);
    glBindBuffer(GL_ARRAY_BUFFER, b);                note();   /* INVALID_OPERATION: deleted */
    glBindBuffer(GL_ARRAY_BUFFER, vbo);
    glBindTexture(GL_TEXTURE_2D, 0);
    glTexParameteri(GL_TEXTURE_2D, GL_TEXTURE_MIN_FILTER, GL_LINEAR); note();   /* INVALID_OPERATION: no texture */
    glBindTexture(GL_TEXTURE_2D, tex);
    glGetString(0x1234);                             note();   /* INVALID_ENUM */
}

/* Pixels that aren't bytes: WebGL 2 takes them only as the matching typed array
 * (Float32Array for FLOAT, Uint16Array for HALF_FLOAT and packed shorts, ...), so
 * the browser runner must convert. Each upload must record no error; one comes
 * from an address that isn't aligned for its type. */
static void typed_uploads(void) {
    static float r32f[4 * 4];
    static uint16_t rgba16f[4 * 4 * 4], rgb565[4 * 4];
    static uint8_t unaligned[2 + sizeof r32f];
    for (int i = 0; i < 16; i++) {
        r32f[i] = (float)i * 0.25f - 1.0f;
        rgb565[i] = (uint16_t)(i * 4099);
        for (int c = 0; c < 4; c++) rgba16f[i * 4 + c] = (uint16_t)(0x3C00 + i * 16 + c);   /* halves around 1.0 */
    }
    memcpy(unaligned + 2, r32f, sizeof r32f);
    GLuint t[4];
    glGenTextures(4, t);
    glBindTexture(GL_TEXTURE_2D, t[0]);
    glTexImage2D(GL_TEXTURE_2D, 0, GL_R32F, 4, 4, 0, GL_RED, GL_FLOAT, r32f);            note();
    glTexSubImage2D(GL_TEXTURE_2D, 0, 1, 1, 2, 2, GL_RED, GL_FLOAT, r32f);               note();
    glBindTexture(GL_TEXTURE_2D, t[1]);
    glTexImage2D(GL_TEXTURE_2D, 0, GL_RGBA16F, 4, 4, 0, GL_RGBA, GL_HALF_FLOAT, rgba16f); note();
    glBindTexture(GL_TEXTURE_2D, t[2]);
    glTexImage2D(GL_TEXTURE_2D, 0, GL_RGB565, 4, 4, 0, GL_RGB, GL_UNSIGNED_SHORT_5_6_5, rgb565); note();
    glBindTexture(GL_TEXTURE_2D, t[3]);
    glTexImage2D(GL_TEXTURE_2D, 0, GL_R32F, 4, 4, 0, GL_RED, GL_FLOAT, unaligned + 2);   note();
    glBindTexture(GL_TEXTURE_3D, t[0]);                                                  note();   /* INVALID_OPERATION: a 2D texture */
    glDeleteTextures(4, t);
    glBindTexture(GL_TEXTURE_2D, tex);
}

/* Client-side arrays (GLES; WebGL has none): gasm_gl.c copies them into buffers at the
 * draw, so GLES 2 code that uses them works. An enabled attribute without a buffer (and no
 * client pointer) is INVALID_OPERATION on every runner, as in WebGL. */
static void client_arrays(void) {
    static const float tri[] = { -0.5f, -0.5f, 0.5f, -0.5f, 0.0f, 0.5f };
    static const uint16_t idx[] = { 0, 1, 2 };
    glBindVertexArray(0);
    glBindBuffer(GL_ARRAY_BUFFER, 0);
    glEnableVertexAttribArray(0);
    glVertexAttribPointer(0, 2, GL_FLOAT, GL_FALSE, 0, tri);
    glDrawArrays(GL_TRIANGLES, 0, 3);                            note();
    glDrawElements(GL_TRIANGLES, 3, GL_UNSIGNED_SHORT, idx);     note();
    glDisableVertexAttribArray(0);
    glEnableVertexAttribArray(7);
    glVertexAttribPointer(7, 2, GL_FLOAT, GL_FALSE, 0, NULL);
    glDrawArrays(GL_TRIANGLES, 0, 3);                            note();   /* INVALID_OPERATION: no buffer */
    glDisableVertexAttribArray(7);
    glEnableVertexAttribArray(99);                               note();   /* INVALID_VALUE */
    glBindBuffer(GL_ARRAY_BUFFER, vbo);
}

GASM_EXPORT("gasm_init") int32_t init(void) {
    char msg[160];
    snprintf(msg, sizeof msg, "gltest: %s / %s", (const char *)glGetString(GL_VERSION), (const char *)glGetString(GL_SHADING_LANGUAGE_VERSION));
    gasm_log_str(msg);

    prog = glCreateProgram();
    GLuint vs = shader(GL_VERTEX_SHADER, VS), fs = shader(GL_FRAGMENT_SHADER, FS);
    glAttachShader(prog, vs);
    glAttachShader(prog, fs);
    glLinkProgram(prog);
    GLint ok = 0;
    glGetProgramiv(prog, GL_LINK_STATUS, &ok);
    if (!ok) {
        char log[512];
        glGetProgramInfoLog(prog, sizeof log, NULL, log);
        gasm_log_str(log);
        return 1;
    }
    glDeleteShader(vs);
    glDeleteShader(fs);
    glUseProgram(prog);
    rot_loc = glGetUniformLocation(prog, "rot");
    glUniform1i(glGetUniformLocation(prog, "tex"), 0);
    glUniformBlockBinding(prog, glGetUniformBlockIndex(prog, "Scene"), 0);

    /* quad: pos.xy uv.xy, indexed */
    static const float quad[] = { -1, -1, 0, 1, 1, -1, 1, 1, 1, 1, 1, 0, -1, 1, 0, 0 };
    static const uint16_t idx[] = { 0, 1, 2, 0, 2, 3 };
    glGenVertexArrays(1, &vao);
    glBindVertexArray(vao);
    glGenBuffers(1, &vbo);
    glBindBuffer(GL_ARRAY_BUFFER, vbo);
    glBufferData(GL_ARRAY_BUFFER, sizeof quad, quad, GL_STATIC_DRAW);
    glEnableVertexAttribArray(0);
    glVertexAttribPointer(0, 2, GL_FLOAT, GL_FALSE, 16, (void *)0);
    glEnableVertexAttribArray(1);
    glVertexAttribPointer(1, 2, GL_FLOAT, GL_FALSE, 16, (void *)8);
    glGenBuffers(1, &ibo);
    glBindBuffer(GL_ELEMENT_ARRAY_BUFFER, ibo);
    glBufferData(GL_ELEMENT_ARRAY_BUFFER, sizeof idx, idx, GL_STATIC_DRAW);
    glBindVertexArray(0);

    glGenBuffers(1, &ubo);
    glBindBufferBase(GL_UNIFORM_BUFFER, 0, ubo);
    glBufferData(GL_UNIFORM_BUFFER, 32, NULL, GL_DYNAMIC_DRAW);

    /* a 16x8 RGB texture with rows of 3 bytes per pixel (unpack alignment 1), and mips */
    static uint8_t px[16 * 8 * 3];
    for (int y = 0; y < 8; y++)
        for (int x = 0; x < 16; x++) {
            uint8_t *p = px + (y * 16 + x) * 3;
            p[0] = (uint8_t)(x * 16);
            p[1] = (uint8_t)(y * 32);
            p[2] = ((x ^ y) & 1) ? 255 : 40;
        }
    glGenTextures(1, &tex);
    glBindTexture(GL_TEXTURE_2D, tex);
    glPixelStorei(GL_UNPACK_ALIGNMENT, 1);
    glTexImage2D(GL_TEXTURE_2D, 0, GL_RGB8, 16, 8, 0, GL_RGB, GL_UNSIGNED_BYTE, px);
    glPixelStorei(GL_UNPACK_ALIGNMENT, 4);
    glTexParameteri(GL_TEXTURE_2D, GL_TEXTURE_MIN_FILTER, GL_NEAREST_MIPMAP_LINEAR);
    glTexParameteri(GL_TEXTURE_2D, GL_TEXTURE_MAG_FILTER, GL_NEAREST);
    glGenerateMipmap(GL_TEXTURE_2D);

    /* an offscreen target, cleared and blitted to the default framebuffer each frame */
    glGenTextures(1, &fbo_tex);
    glBindTexture(GL_TEXTURE_2D, fbo_tex);
    glTexStorage2D(GL_TEXTURE_2D, 1, GL_RGBA8, 64, 64);
    glGenFramebuffers(1, &fbo);
    glBindFramebuffer(GL_FRAMEBUFFER, fbo);
    glFramebufferTexture2D(GL_FRAMEBUFFER, GL_COLOR_ATTACHMENT0, GL_TEXTURE_2D, fbo_tex, 0);
    if (glCheckFramebufferStatus(GL_FRAMEBUFFER) != GL_FRAMEBUFFER_COMPLETE) gasm_log_str("gltest: framebuffer incomplete");
    glBindFramebuffer(GL_FRAMEBUFFER, 0);
    glBindTexture(GL_TEXTURE_2D, tex);

    /* the error log goes to a buffer, so the hash covers it */
    glGenBuffers(1, &errbuf);
    glGenQueries(1, &query);
    mistakes();
    typed_uploads();
    client_arrays();
    GLenum e = glGetError();
    snprintf(msg, sizeof msg, "gltest: %u error records, pending %u", nerr, e);
    gasm_log_str(msg);
    return 0;
}

GASM_EXPORT("gasm_frame") void frame_tick(void) {
    float t = (float)frame / 60.0f;
    float c = cosf(t), s = sinf(t);
    GLint w = (GLint)gasm_gl_width(), h = (GLint)gasm_gl_height();

    float rot[4] = { c, s, -s, c };
    glUniformMatrix2fv(rot_loc, 1, GL_FALSE, rot);
    glBindVertexArray(vao);

    /* offscreen: a cycling color and one quad, blitted (scaled) to the background */
    glBindFramebuffer(GL_FRAMEBUFFER, fbo);
    glViewport(0, 0, 64, 64);
    glClearColor(0.1f + 0.4f * (s * s), 0.2f, 0.3f, 1);
    glClear(GL_COLOR_BUFFER_BIT);
    glDrawElements(GL_TRIANGLES, 6, GL_UNSIGNED_SHORT, (void *)0);
    glBindFramebuffer(GL_READ_FRAMEBUFFER, fbo);
    glBindFramebuffer(GL_DRAW_FRAMEBUFFER, 0);
    glViewport(0, 0, w, h);
    glBlitFramebuffer(0, 0, 64, 64, 0, 0, w, h, GL_COLOR_BUFFER_BIT, GL_LINEAR);
    glBindFramebuffer(GL_FRAMEBUFFER, 0);

    /* scene uniforms through a mapped range (emulated in guest memory) */
    glBindBuffer(GL_UNIFORM_BUFFER, ubo);
    float *scene = (float *)glMapBufferRange(GL_UNIFORM_BUFFER, 0, 32, GL_MAP_WRITE_BIT | GL_MAP_INVALIDATE_BUFFER_BIT);
    if (scene) {
        scene[0] = 1.0f;
        scene[1] = 0.8f + 0.2f * c;
        scene[2] = 0.8f;
        scene[3] = 1.0f;
        scene[4] = 0.5f + 0.1f * s;
        glUnmapBuffer(GL_UNIFORM_BUFFER);
    }
    glEnable(GL_BLEND);
    glBlendFunc(GL_SRC_ALPHA, GL_ONE_MINUS_SRC_ALPHA);
    if (frame == 10) glBeginQuery(GL_ANY_SAMPLES_PASSED, query);
    glDrawElementsInstanced(GL_TRIANGLES, 6, GL_UNSIGNED_SHORT, (void *)0, 3);
    if (frame == 10) {
        glEndQuery(GL_ANY_SAMPLES_PASSED);
        fence = glFenceSync(GL_SYNC_GPU_COMMANDS_COMPLETE, 0);
    }
    glBindVertexArray(0);

    /* results arrive from the next frame on, on every runner */
    if (frame == 10 || frame == 11) {
        /* availability is the same everywhere; the result is the GPU's (not recorded) */
        GLuint avail = 0, passed = 0;
        glGetQueryObjectuiv(query, GL_QUERY_RESULT_AVAILABLE, &avail);
        if (avail) glGetQueryObjectuiv(query, GL_QUERY_RESULT, &passed);
        GLenum w8 = glClientWaitSync(fence, 0, 0);
        if (nerr + 2 <= 64) {
            errors[nerr++] = avail;
            errors[nerr++] = w8;
        }
        if (frame == 11) glDeleteSync(fence);
    }
    GLenum e = glGetError();   /* a frame without mistakes has none */
    if (e != GL_NO_ERROR && nerr < 64) errors[nerr++] = 0xf000 | e;
    if (frame == 12) {   /* the record, for comparing runners */
        char msg[64 * 6 + 16] = "gltest:", *p = msg + 7;
        for (unsigned i = 0; i < nerr; i++) p += snprintf(p, 8, " %x", errors[i]);
        gasm_log_str(msg);
    }
    glBindBuffer(GL_COPY_WRITE_BUFFER, errbuf);
    glBufferData(GL_COPY_WRITE_BUFFER, sizeof errors, errors, GL_DYNAMIC_DRAW);
    frame++;
}
