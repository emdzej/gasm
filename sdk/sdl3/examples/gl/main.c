/*
  OpenGL ES through SDL on gasm: SDL_GL_CreateContext on gasm:gl, a spinning
  triangle on a colour that cycles, drawn with GLSL ES 3.00. glClearColor and
  glClear come from SDL_GL_GetProcAddress (loader-style code), the rest are called
  directly (<GLES3/gl3.h>, gasm_gl.o). Link with gasm_gl.o (SDL3::GL).
  Resizing the window resizes the drawable: SDL_EVENT_WINDOW_RESIZED.

  SPDX-License-Identifier: MIT
*/
#define SDL_MAIN_USE_CALLBACKS 1
#include <SDL3/SDL.h>
#include <SDL3/SDL_main.h>
#include <GLES3/gl3.h>

static SDL_Window *window;
static SDL_GLContext context;
static GLuint program, vao, vbo;
static GLint angle_loc;
static Uint64 frame;
static void (*clear_color)(GLfloat, GLfloat, GLfloat, GLfloat);
static void (*clear)(GLbitfield);

static const char *VS =
    "#version 300 es\n"
    "layout(location = 0) in vec2 pos;\n"
    "layout(location = 1) in vec3 col;\n"
    "uniform float angle;\n"
    "out vec3 v_col;\n"
    "void main() {\n"
    "  float c = cos(angle), s = sin(angle);\n"
    "  gl_Position = vec4(mat2(c, s, -s, c) * pos, 0.0, 1.0);\n"
    "  v_col = col;\n"
    "}\n";
static const char *FS =
    "#version 300 es\n"
    "precision mediump float;\n"
    "in vec3 v_col;\n"
    "out vec4 color;\n"
    "void main() { color = vec4(v_col, 1.0); }\n";

static GLuint shader(GLenum type, const char *src)
{
    GLuint s = glCreateShader(type);
    glShaderSource(s, 1, &src, NULL);
    glCompileShader(s);
    GLint ok = 0;
    glGetShaderiv(s, GL_COMPILE_STATUS, &ok);
    if (!ok) {
        char log[512];
        glGetShaderInfoLog(s, sizeof log, NULL, log);
        SDL_Log("shader: %s", log);
    }
    return s;
}

SDL_AppResult SDL_AppInit(void **appstate, int argc, char *argv[])
{
    (void)appstate; (void)argc; (void)argv;
    if (!SDL_Init(SDL_INIT_VIDEO)) {
        SDL_Log("SDL_Init: %s", SDL_GetError());
        return SDL_APP_FAILURE;
    }
    SDL_GL_SetAttribute(SDL_GL_CONTEXT_PROFILE_MASK, SDL_GL_CONTEXT_PROFILE_ES);
    SDL_GL_SetAttribute(SDL_GL_CONTEXT_MAJOR_VERSION, 3);
    SDL_GL_SetAttribute(SDL_GL_CONTEXT_MINOR_VERSION, 0);
    window = SDL_CreateWindow("SDL3 + OpenGL ES", 640, 480, SDL_WINDOW_OPENGL | SDL_WINDOW_RESIZABLE);
    if (!window || !(context = SDL_GL_CreateContext(window))) {
        SDL_Log("GL: %s", SDL_GetError());
        return SDL_APP_FAILURE;
    }
    SDL_GL_SetSwapInterval(1);
    clear_color = (void (*)(GLfloat, GLfloat, GLfloat, GLfloat))SDL_GL_GetProcAddress("glClearColor");
    clear = (void (*)(GLbitfield))SDL_GL_GetProcAddress("glClear");
    if (!clear_color || !clear || SDL_GL_GetProcAddress("glNotAFunction")) {
        SDL_Log("SDL_GL_GetProcAddress gave the wrong answers");
        return SDL_APP_FAILURE;
    }
    int w, h;
    SDL_GetWindowSizeInPixels(window, &w, &h);
    SDL_Log("gl: %s, %s; drawable %dx%d", (const char *)glGetString(GL_VERSION), (const char *)glGetString(GL_RENDERER), w, h);

    program = glCreateProgram();
    glAttachShader(program, shader(GL_VERTEX_SHADER, VS));
    glAttachShader(program, shader(GL_FRAGMENT_SHADER, FS));
    glLinkProgram(program);
    angle_loc = glGetUniformLocation(program, "angle");
    static const float tri[] = { 0.0f, 0.7f, 1, 0.35f, 0.3f, -0.6f, -0.4f, 0.2f, 0.5f, 1, 0.6f, -0.4f, 0.3f, 0.6f, 1 };
    glGenVertexArrays(1, &vao);
    glBindVertexArray(vao);
    glGenBuffers(1, &vbo);
    glBindBuffer(GL_ARRAY_BUFFER, vbo);
    glBufferData(GL_ARRAY_BUFFER, sizeof tri, tri, GL_STATIC_DRAW);
    glEnableVertexAttribArray(0);
    glVertexAttribPointer(0, 2, GL_FLOAT, GL_FALSE, 20, (void *)0);
    glEnableVertexAttribArray(1);
    glVertexAttribPointer(1, 3, GL_FLOAT, GL_FALSE, 20, (void *)8);
    return SDL_APP_CONTINUE;
}

SDL_AppResult SDL_AppEvent(void *appstate, SDL_Event *event)
{
    (void)appstate;
    if (event->type == SDL_EVENT_QUIT || (event->type == SDL_EVENT_KEY_DOWN && event->key.key == SDLK_ESCAPE)) {
        return SDL_APP_SUCCESS;
    }
    if (event->type == SDL_EVENT_WINDOW_RESIZED) {
        SDL_Log("resized to %dx%d", event->window.data1, event->window.data2);
    }
    return SDL_APP_CONTINUE;
}

SDL_AppResult SDL_AppIterate(void *appstate)
{
    (void)appstate;
    int w, h;
    SDL_GetWindowSizeInPixels(window, &w, &h);
    glViewport(0, 0, w, h);
    const float t = (float)frame / 60.0f;
    clear_color(0.10f + 0.05f * (float)(frame % 60) / 60.0f, 0.12f, 0.18f, 1.0f);
    clear(GL_COLOR_BUFFER_BIT);
    glUseProgram(program);
    glUniform1f(angle_loc, t);
    glBindVertexArray(vao);
    glDrawArrays(GL_TRIANGLES, 0, 3);
    SDL_GL_SwapWindow(window);
    frame++;
    return SDL_APP_CONTINUE;
}

void SDL_AppQuit(void *appstate, SDL_AppResult result)
{
    (void)appstate; (void)result;
    SDL_GL_DestroyContext(context);
}
