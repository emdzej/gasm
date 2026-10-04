/**************************************************************************/
/*  emscripten/html5_webgl.h (gasm): what Godot's WebGL paths use of it    */
/**************************************************************************/
#pragma once

#include <stdlib.h>
#include <string.h>

#include "gasm.h"

// WebGL's extension names (gasm:gl reports them as browsers do); free() the result.
static inline char *emscripten_webgl_get_supported_extensions(void) {
	int32_t n = gasm_gl_get_string(0x1F03 /* GL_EXTENSIONS */, NULL, 0);
	char *s = (char *)calloc(n > 0 ? n + 1 : 1, 1);
	if (s && n > 0) {
		gasm_gl_get_string(0x1F03, s, (uint32_t)n);
	}
	return s;
}
