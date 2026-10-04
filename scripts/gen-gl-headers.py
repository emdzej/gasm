#!/usr/bin/env python3
"""Generate the C SDK's drop-in GLES 3.0 headers and their implementation on gasm:gl.

    python3 scripts/gen-gl-headers.py [gl.xml]

Reads the Khronos registry (gl.xml; default: the khronos_api crate in the cargo
registry, version 3.1.0) and spec/abi.json, and writes:

  sdk/c/include/GLES3/gl3.h        typedefs, the GLES 2.0 + 3.0 enums and prototypes
  sdk/c/include/GLES3/gl3platform.h, GLES2/gl2.h, GLES2/gl2ext.h, KHR/khrplatform.h
  sdk/c/src/gasm_gl.c              every GLES 3.0 function, on the gasm:gl imports

Most functions map one to one onto an import; the rest are written out below
(OVERRIDES): gen/delete loops, string arrays, glGetString caching, glMapBufferRange
emulated in guest memory, the glGet* variants. Don't edit the outputs by hand.
"""
import glob, json, os, re, sys
import xml.etree.ElementTree as ET

ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
xml_path = sys.argv[1] if len(sys.argv) > 1 else (glob.glob(os.path.expanduser(
    '~/.cargo/registry/src/*/khronos_api-3.1.0/api/xml/gl.xml')) or [None])[0]
if not xml_path:
    sys.exit('gl.xml not found: pass its path (Khronos OpenGL-Registry, or the khronos_api 3.1.0 crate)')
reg = ET.parse(xml_path).getroot()

# ---- the GLES 3.0 command and enum set --------------------------------------------------
want_cmds, want_enums = [], set()
for f in reg.findall('feature'):
    if f.get('api') == 'gles2' and f.get('name') in ('GL_ES_VERSION_2_0', 'GL_ES_VERSION_3_0'):
        for req in f.findall('require'):
            want_cmds += [c.get('name') for c in req.findall('command')]
            want_enums |= {e.get('name') for e in req.findall('enum')}
values = {}
for es in reg.findall('enums'):
    for e in es.findall('enum'):
        if e.get('api') in (None, 'gles2'):
            values.setdefault(e.get('name'), e.get('value'))
# extension enums a WebGL 2 game may use (S3TC, BPTC, anisotropy, float buffers)
EXT_ENUMS = ['GL_TEXTURE_MAX_ANISOTROPY_EXT', 'GL_MAX_TEXTURE_MAX_ANISOTROPY_EXT',
             'GL_COMPRESSED_RGB_S3TC_DXT1_EXT', 'GL_COMPRESSED_RGBA_S3TC_DXT1_EXT',
             'GL_COMPRESSED_RGBA_S3TC_DXT3_EXT', 'GL_COMPRESSED_RGBA_S3TC_DXT5_EXT',
             'GL_COMPRESSED_SRGB_S3TC_DXT1_EXT', 'GL_COMPRESSED_SRGB_ALPHA_S3TC_DXT1_EXT',
             'GL_COMPRESSED_SRGB_ALPHA_S3TC_DXT3_EXT', 'GL_COMPRESSED_SRGB_ALPHA_S3TC_DXT5_EXT',
             'GL_COMPRESSED_RGBA_BPTC_UNORM_EXT', 'GL_COMPRESSED_SRGB_ALPHA_BPTC_UNORM_EXT',
             'GL_COMPRESSED_RGB_BPTC_SIGNED_FLOAT_EXT', 'GL_COMPRESSED_RGB_BPTC_UNSIGNED_FLOAT_EXT',
             'GL_RGBA16F_EXT', 'GL_RGBA32F_EXT', 'GL_R16F_EXT', 'GL_RG16F_EXT', 'GL_R11F_G11F_B10F_EXT']

cmds = {}
for c in reg.find('commands').findall('command'):
    proto = c.find('proto')
    name = proto.find('name').text
    if name not in want_cmds:
        continue
    ret = ''.join(proto.itertext()).rsplit(name, 1)[0].strip()
    params = []
    for p in c.findall('param'):
        pname = p.find('name').text
        ptype = ''.join(p.itertext()).rsplit(pname, 1)[0].strip()
        params.append((ptype, pname))
    cmds[name] = (ret, params)
assert len(cmds) == len(set(want_cmds)), 'missing commands'

abi = json.load(open(os.path.join(ROOT, 'spec/abi.json')))
imports = {f['name']: f for f in next(m for m in abi['modules'] if m['name'] == 'gasm:gl')['functions']}

def snake(gl):
    s = re.sub(r'([a-z0-9])([A-Z])', r'\1_\2', gl[2:]).lower()
    s = re.sub(r'_?(\d)_d$', r'_\1d', s)          # tex_image_2d, not tex_image_2_d
    return s.replace('attrib_i4', 'attribi4')   # vertex_attribi4i

# ---- implementations that aren't a straight call --------------------------------------------
GEN = {'glGenBuffers': 'buffer', 'glGenTextures': 'texture', 'glGenFramebuffers': 'framebuffer',
       'glGenRenderbuffers': 'renderbuffer', 'glGenQueries': 'query', 'glGenVertexArrays': 'vertex_array',
       'glGenSamplers': 'sampler', 'glGenTransformFeedbacks': 'transform_feedback'}
DEL = {k.replace('glGen', 'glDelete'): v for k, v in GEN.items()}
OVERRIDES = {}
for g, k in GEN.items():
    n, a = cmds[g][1][0][1], cmds[g][1][1][1]
    OVERRIDES[g] = f'for (GLsizei i = 0; i < {n}; i++) {a}[i] = gasm_gl_create_{k}();'
for d, k in DEL.items():
    n, a = cmds[d][1][0][1], cmds[d][1][1][1]
    OVERRIDES[d] = f'for (GLsizei i = 0; i < {n}; i++) gasm_gl_delete_{k}({a}[i]);'
O = OVERRIDES
O['glGetError'] = 'if (c_error) { GLenum e = c_error; c_error = 0; return e; }\n    return gasm_gl_get_error();'
O['glShaderSource'] = 'shader_source(shader, count, string, length);'
O['glBufferData'] = 'gasm_gl_buffer_data(target, data, (uint32_t)size, usage);'
O['glBufferSubData'] = 'gasm_gl_buffer_sub_data(target, (uint32_t)offset, data, (uint32_t)size);'
O['glBindBuffer'] = 'track_buffer(target, buffer);\n    gasm_gl_bind_buffer(target, buffer);'
O['glPixelStorei'] = 'track_store(pname, param);\n    gasm_gl_pixel_storei(pname, param);'
O['glTexImage2D'] = 'gasm_gl_tex_image_2d(target, level, internalformat, width, height, border, format, type, UNPACK_PTR(pixels), UNPACK_LEN(pixels, width, height, 1, format, type));'
O['glTexImage3D'] = 'gasm_gl_tex_image_3d(target, level, internalformat, width, height, depth, border, format, type, UNPACK_PTR(pixels), UNPACK_LEN(pixels, width, height, depth, format, type));'
O['glTexSubImage2D'] = 'gasm_gl_tex_sub_image_2d(target, level, xoffset, yoffset, width, height, format, type, UNPACK_PTR(pixels), UNPACK_LEN(pixels, width, height, 1, format, type));'
O['glTexSubImage3D'] = 'gasm_gl_tex_sub_image_3d(target, level, xoffset, yoffset, zoffset, width, height, depth, format, type, UNPACK_PTR(pixels), UNPACK_LEN(pixels, width, height, depth, format, type));'
O['glCompressedTexImage2D'] = 'gasm_gl_compressed_tex_image_2d(target, level, internalformat, width, height, border, data, (uint32_t)imageSize);'
O['glCompressedTexImage3D'] = 'gasm_gl_compressed_tex_image_3d(target, level, internalformat, width, height, depth, border, data, (uint32_t)imageSize);'
O['glCompressedTexSubImage2D'] = 'gasm_gl_compressed_tex_sub_image_2d(target, level, xoffset, yoffset, width, height, format, data, (uint32_t)imageSize);'
O['glCompressedTexSubImage3D'] = 'gasm_gl_compressed_tex_sub_image_3d(target, level, xoffset, yoffset, zoffset, width, height, depth, format, data, (uint32_t)imageSize);'
O['glReadPixels'] = ('if (pack_buffer) { gasm_gl_read_pixels(x, y, width, height, format, type, pixels, 0); return; }\n'
                     '    gasm_gl_read_pixels(x, y, width, height, format, type, pixels, image_bytes(&pack, width, height, 1, format, type));')
O['glDrawElements'] = 'gasm_gl_draw_elements(mode, count, type, (uint32_t)(uintptr_t)indices);'
O['glDrawRangeElements'] = 'gasm_gl_draw_range_elements(mode, start, end, count, type, (uint32_t)(uintptr_t)indices);'
O['glDrawElementsInstanced'] = 'gasm_gl_draw_elements_instanced(mode, count, type, (uint32_t)(uintptr_t)indices, instancecount);'
O['glVertexAttribPointer'] = 'gasm_gl_vertex_attrib_pointer(index, size, type, normalized, stride, (uint32_t)(uintptr_t)pointer);'
O['glVertexAttribIPointer'] = 'gasm_gl_vertex_attrib_ipointer(index, size, type, stride, (uint32_t)(uintptr_t)pointer);'
O['glGetIntegerv'] = ('if (pname == GL_NUM_EXTENSIONS) { *data = (GLint)extension_count(); return; }\n'
                      '    gasm_gl_get_integerv(pname, data, 64);')
O['glGetBooleanv'] = ('GLint v[64];\n    int n = gasm_gl_get_integerv(pname, v, 64);\n'
                      '    for (int i = 0; i < n && i < 64; i++) data[i] = v[i] ? GL_TRUE : GL_FALSE;')
O['glGetFloatv'] = 'gasm_gl_get_floatv(pname, data, 64);'
O['glGetInteger64v'] = 'gasm_gl_get_integer64v(pname, data, 64);'
O['glGetIntegeri_v'] = 'gasm_gl_get_integeri_v(target, index, data, 16);'
O['glGetInteger64i_v'] = ('GLint v[16];\n    int n = gasm_gl_get_integeri_v(target, index, v, 16);\n'
                          '    for (int i = 0; i < n && i < 16; i++) data[i] = v[i];')
O['glGetString'] = 'return get_string(name);'
O['glGetStringi'] = 'return name == GL_EXTENSIONS ? extension(index) : (c_error = GL_INVALID_ENUM, (const GLubyte *)0);'
for f, imp in [('glGetShaderiv', 'get_shaderiv(shader, pname)'), ('glGetProgramiv', 'get_programiv(program, pname)'),
               ('glGetBufferParameteriv', 'get_buffer_parameteriv(target, pname)'),
               ('glGetRenderbufferParameteriv', 'get_renderbuffer_parameteriv(target, pname)'),
               ('glGetFramebufferAttachmentParameteriv', 'get_framebuffer_attachment_parameteriv(target, attachment, pname)'),
               ('glGetTexParameteriv', 'get_tex_parameteriv(target, pname)'), ('glGetTexParameterfv', 'get_tex_parameterfv(target, pname)'),
               ('glGetSamplerParameteriv', 'get_sampler_parameteriv(sampler, pname)'),
               ('glGetSamplerParameterfv', 'get_sampler_parameterfv(sampler, pname)'),
               ('glGetQueryiv', 'get_queryiv(target, pname)'), ('glGetQueryObjectuiv', 'get_query_objectuiv(id, pname)'),
               ('glGetVertexAttribiv', 'get_vertex_attribiv(index, pname)'), ('glGetVertexAttribIiv', 'get_vertex_attribiv(index, pname)'),
               ('glGetVertexAttribIuiv', 'get_vertex_attribiv(index, pname)')]:
    out = cmds[f][1][-1][1]
    O[f] = f'*{out} = gasm_gl_{imp};'
O['glGetBufferParameteri64v'] = '*params = gasm_gl_get_buffer_parameteriv(target, pname);'
O['glGetVertexAttribfv'] = 'gasm_gl_get_vertex_attribfv(index, pname, params, 4);'
O['glGetVertexAttribPointerv'] = '*pointer = (void *)(uintptr_t)gasm_gl_get_vertex_attrib_offset(index, pname);'
for f in ['glTexParameteriv', 'glTexParameterfv', 'glSamplerParameteriv', 'glSamplerParameterfv']:
    args = [p[1] for p in cmds[f][1]]
    imp = snake(f)[:-1]   # ..._parameteri / _parameterf
    O[f] = f'gasm_gl_{imp}({args[0]}, {args[1]}, {args[2]}[0]);'
O['glGetShaderInfoLog'] = 'truncated(gasm_gl_get_shader_info_log, shader, bufSize, length, infoLog);'
O['glGetProgramInfoLog'] = 'truncated(gasm_gl_get_program_info_log, program, bufSize, length, infoLog);'
O['glGetShaderSource'] = 'truncated(gasm_gl_get_shader_source, shader, bufSize, length, source);'
O['glGetActiveUniformBlockName'] = 'truncated2(gasm_gl_get_active_uniform_block_name, program, uniformBlockIndex, bufSize, length, uniformBlockName);'
for f, imp in [('glGetActiveAttrib', 'get_active_attrib'), ('glGetActiveUniform', 'get_active_uniform'),
               ('glGetTransformFeedbackVarying', 'get_transform_feedback_varying')]:
    O[f] = f'active_info(gasm_gl_{imp}, program, index, bufSize, length, size, type, name);'
O['glGetAttachedShaders'] = 'int n = gasm_gl_get_attached_shaders(program, shaders, (uint32_t)maxCount);\n    if (count) *count = n < maxCount ? n : maxCount;'
for f in ['glGetUniformLocation', 'glGetAttribLocation', 'glGetFragDataLocation', 'glGetUniformBlockIndex']:
    nm = cmds[f][1][1][1]
    O[f] = f'return gasm_gl_{snake(f)}(program, {nm}, (uint32_t)strlen({nm}));'
O['glBindAttribLocation'] = 'gasm_gl_bind_attrib_location(program, index, name, (uint32_t)strlen(name));'
O['glGetUniformIndices'] = 'for (GLsizei i = 0; i < uniformCount; i++) uniformIndices[i] = gasm_gl_get_uniform_index(program, uniformNames[i], (uint32_t)strlen(uniformNames[i]));'
O['glGetActiveUniformsiv'] = 'gasm_gl_get_active_uniformsiv(program, uniformIndices, (uint32_t)uniformCount, pname, params);'
O['glGetActiveUniformBlockiv'] = 'gasm_gl_get_active_uniform_blockiv(program, uniformBlockIndex, pname, params, 64);'
O['glGetUniformfv'] = 'gasm_gl_get_uniformfv(program, location, params, 16);'
O['glGetUniformiv'] = 'gasm_gl_get_uniformiv(program, location, params, 16);'
O['glGetUniformuiv'] = 'gasm_gl_get_uniformuiv(program, location, params, 16);'
O['glVertexAttrib1f'] = 'gasm_gl_vertex_attrib4f(index, x, 0, 0, 1);'
O['glVertexAttrib2f'] = 'gasm_gl_vertex_attrib4f(index, x, y, 0, 1);'
O['glVertexAttrib3f'] = 'gasm_gl_vertex_attrib4f(index, x, y, z, 1);'
O['glVertexAttrib1fv'] = 'gasm_gl_vertex_attrib4f(index, v[0], 0, 0, 1);'
O['glVertexAttrib2fv'] = 'gasm_gl_vertex_attrib4f(index, v[0], v[1], 0, 1);'
O['glVertexAttrib3fv'] = 'gasm_gl_vertex_attrib4f(index, v[0], v[1], v[2], 1);'
O['glVertexAttrib4fv'] = 'gasm_gl_vertex_attrib4f(index, v[0], v[1], v[2], v[3]);'
O['glVertexAttribI4iv'] = 'gasm_gl_vertex_attribi4i(index, v[0], v[1], v[2], v[3]);'
O['glVertexAttribI4uiv'] = 'gasm_gl_vertex_attribi4ui(index, v[0], v[1], v[2], v[3]);'
O['glGetShaderPrecisionFormat'] = 'GLint v[3];\n    gasm_gl_get_shader_precision_format(shadertype, precisiontype, v);\n    range[0] = v[0];\n    range[1] = v[1];\n    *precision = v[2];'
O['glFenceSync'] = 'return (GLsync)(uintptr_t)gasm_gl_fence_sync(condition, flags);'
O['glIsSync'] = 'return (GLboolean)gasm_gl_is_sync((uint32_t)(uintptr_t)sync);'
O['glDeleteSync'] = 'gasm_gl_delete_sync((uint32_t)(uintptr_t)sync);'
O['glClientWaitSync'] = 'return gasm_gl_client_wait_sync((uint32_t)(uintptr_t)sync, flags, timeout);'
O['glWaitSync'] = 'gasm_gl_wait_sync((uint32_t)(uintptr_t)sync, flags, timeout);'
O['glGetSynciv'] = ('if (bufSize > 0) values[0] = gasm_gl_get_synciv((uint32_t)(uintptr_t)sync, pname);\n'
                    '    if (length) *length = bufSize > 0 ? 1 : 0;')
O['glTransformFeedbackVaryings'] = 'feedback_varyings(program, count, varyings, bufferMode);'
O['glDrawBuffers'] = 'gasm_gl_draw_buffers(bufs, (uint32_t)n);'
for f in ['glClearBufferiv', 'glClearBufferuiv', 'glClearBufferfv']:
    O[f] = f'gasm_gl_{snake(f)}(buffer, drawbuffer, value, buffer == GL_COLOR ? 4 : 1);'
O['glInvalidateFramebuffer'] = 'gasm_gl_invalidate_framebuffer(target, attachments, (uint32_t)numAttachments);'
O['glInvalidateSubFramebuffer'] = 'gasm_gl_invalidate_sub_framebuffer(target, attachments, (uint32_t)numAttachments, x, y, width, height);'
O['glGetInternalformativ'] = 'gasm_gl_get_internalformativ(target, internalformat, pname, params, (uint32_t)bufSize);'
O['glMapBufferRange'] = 'return map_range(target, offset, length, access);'
O['glUnmapBuffer'] = 'return unmap(target);'
O['glFlushMappedBufferRange'] = 'flush_mapped(target, offset, length);'
O['glGetBufferPointerv'] = '*params = mapping_for(target) ? mapping_for(target)->ptr : 0;'
O['glReleaseShaderCompiler'] = ''
O['glShaderBinary'] = 'c_error = GL_INVALID_ENUM;   /* WebGL has no shader formats */'
O['glGetProgramBinary'] = 'if (length) *length = 0;\n    c_error = GL_INVALID_OPERATION;'
O['glProgramBinary'] = 'c_error = GL_INVALID_ENUM;'
O['glProgramParameteri'] = ''

TYPE_TO_ABI = {'u32': 'uint32_t', 'i32': 'int32_t', 'f32': 'float', 'u64': 'uint64_t', 'i64': 'int64_t'}
def direct(name, ret, params):
    imp = snake(name)
    f = imports.get(imp)
    if not f:
        return None
    ip = f.get('params', [])
    if len(ip) != len(params):
        return None
    args = ', '.join(p[1] for p in params)
    call = f'gasm_gl_{imp}({args})'
    if ret == 'void':
        return call + ';'
    return f'return ({ret}){call};'

impl, missing = [], []
for name in want_cmds:
    if name in [i[0] for i in impl]:
        continue
    ret, params = cmds[name]
    body = O[name] if name in O else direct(name, ret, params)
    if body is None:
        missing.append(name)
        continue
    impl.append((name, ret, params, body))
if missing:
    sys.exit('no implementation for: ' + ' '.join(missing))

def body_of(params, b):
    unused = [q[1] for q in params if not re.search(r'\b' + q[1] + r'\b', b)]
    return ''.join(f'(void){u};\n    ' for u in unused) + b

def proto(name, ret, params):
    ps = ', '.join(f'{t} {n}' for t, n in params) or 'void'
    return f'{ret} GL_APIENTRY {name}({ps})'

HEADER = '/* Generated by scripts/gen-gl-headers.py from the Khronos registry (gl.xml) and spec/abi.json: do not edit. */\n'
inc = os.path.join(ROOT, 'sdk/c/include')
os.makedirs(os.path.join(inc, 'GLES3'), exist_ok=True)
os.makedirs(os.path.join(inc, 'GLES2'), exist_ok=True)
os.makedirs(os.path.join(inc, 'KHR'), exist_ok=True)

enum_lines = [f'#define {e} {values[e]}' for e in sorted(want_enums, key=lambda e: (int(values[e], 0) if values[e].startswith('0x') or values[e].isdigit() else 0, e))]
gl3 = HEADER + '''/*
 * OpenGL ES 3.0 for gasm (gasm:gl, design/gasm-gl.md): the GLES 3.0 API, with
 * WebGL 2's rules, on every gasm runner. Link sdk/c/src/gasm_gl.c with the game.
 * The enum values and prototypes come from the Khronos OpenGL registry
 * (Copyright The Khronos Group Inc., Apache-2.0 / MIT).
 */
#ifndef __gles3_gl3_h_
#define __gles3_gl3_h_ 1
#define __gles2_gl2_h_ 1

#include <GLES3/gl3platform.h>

#ifdef __cplusplus
extern "C" {
#endif

typedef void GLvoid;
typedef char GLchar;
typedef unsigned int GLenum;
typedef unsigned char GLboolean;
typedef unsigned int GLbitfield;
typedef signed char GLbyte;
typedef unsigned char GLubyte;
typedef short GLshort;
typedef unsigned short GLushort;
typedef int GLint;
typedef unsigned int GLuint;
typedef int GLsizei;
typedef float GLfloat;
typedef float GLclampf;
typedef int GLfixed;
typedef long GLintptr;
typedef long GLsizeiptr;
typedef unsigned short GLhalf;
typedef long long GLint64;
typedef unsigned long long GLuint64;
typedef struct __GLsync *GLsync;   /* a gasm:gl sync name */

#define GL_ES_VERSION_2_0 1
#define GL_ES_VERSION_3_0 1
''' + '\n'.join(enum_lines) + '\n\n' + '\n'.join(f'GL_APICALL {proto(n, r, p)};' for n, r, p, _ in impl) + '''

#ifdef __cplusplus
}
#endif

#endif
'''
open(os.path.join(inc, 'GLES3/gl3.h'), 'w').write(gl3)
open(os.path.join(inc, 'GLES3/gl3platform.h'), 'w').write(HEADER + '''#ifndef __gl3platform_h_
#define __gl3platform_h_
#include <KHR/khrplatform.h>
#ifndef GL_APICALL
#define GL_APICALL
#endif
#ifndef GL_APIENTRY
#define GL_APIENTRY
#endif
#endif
''')
open(os.path.join(inc, 'KHR/khrplatform.h'), 'w').write(HEADER + '''#ifndef __khrplatform_h_
#define __khrplatform_h_
#include <stdint.h>
typedef int32_t khronos_int32_t;
typedef uint32_t khronos_uint32_t;
typedef int64_t khronos_int64_t;
typedef uint64_t khronos_uint64_t;
typedef float khronos_float_t;
typedef long khronos_intptr_t;
typedef long khronos_ssize_t;
#define KHRONOS_APICALL
#define KHRONOS_APIENTRY
#endif
''')
open(os.path.join(inc, 'GLES2/gl2.h'), 'w').write(HEADER + '/* GLES 2 code builds against the GLES 3.0 header (a superset). */\n#include <GLES3/gl3.h>\n')
open(os.path.join(inc, 'GLES2/gl2ext.h'), 'w').write(HEADER + '''/* The extension enums a WebGL 2 game may use; enable the extensions with
 * gasm_gl_enable_extension() after finding them in GL_EXTENSIONS. */
#ifndef __gles2_gl2ext_h_
#define __gles2_gl2ext_h_
''' + '\n'.join(f'#define {e} {values[e]}' for e in EXT_ENUMS) + '\n#endif\n')

c = HEADER + r'''/*
 * The GLES 3.0 API on the gasm:gl imports. Most functions are a call; the rest
 * adapt GL's C conventions to the ABI: name arrays, string arrays, the pixel
 * store (data lengths are exact: the runner checks them), glGetString caching and
 * glMapBufferRange emulated in guest memory (WebGL has no mapping).
 *
 * SPDX-License-Identifier: MIT
 */
#include <stdlib.h>
#include <string.h>

#include "gasm.h"
#include <GLES3/gl3.h>

static GLenum c_error;   /* errors found here (no binary formats, bad indices) */

/* ---- the pixel store and the pack/unpack buffers, mirrored to compute exact lengths ---- */

typedef struct { int alignment, row_length, image_height, skip_pixels, skip_rows, skip_images; } store;
static store unpack = { 4, 0, 0, 0, 0, 0 }, pack = { 4, 0, 0, 0, 0, 0 };
static GLuint unpack_buffer, pack_buffer;

static void track_buffer(GLenum target, GLuint buffer) {
    if (target == GL_PIXEL_UNPACK_BUFFER) unpack_buffer = buffer;
    if (target == GL_PIXEL_PACK_BUFFER) pack_buffer = buffer;
}

static void track_store(GLenum pname, GLint v) {
    switch (pname) {
    case GL_UNPACK_ALIGNMENT: unpack.alignment = v; break;
    case GL_UNPACK_ROW_LENGTH: unpack.row_length = v; break;
    case GL_UNPACK_IMAGE_HEIGHT: unpack.image_height = v; break;
    case GL_UNPACK_SKIP_PIXELS: unpack.skip_pixels = v; break;
    case GL_UNPACK_SKIP_ROWS: unpack.skip_rows = v; break;
    case GL_UNPACK_SKIP_IMAGES: unpack.skip_images = v; break;
    case GL_PACK_ALIGNMENT: pack.alignment = v; break;
    case GL_PACK_ROW_LENGTH: pack.row_length = v; break;
    case GL_PACK_SKIP_PIXELS: pack.skip_pixels = v; break;
    case GL_PACK_SKIP_ROWS: pack.skip_rows = v; break;
    }
}

static uint32_t pixel_bytes(GLenum format, GLenum type) {
    switch (type) {
    case GL_UNSIGNED_SHORT_5_6_5: case GL_UNSIGNED_SHORT_4_4_4_4: case GL_UNSIGNED_SHORT_5_5_5_1: return 2;
    case GL_UNSIGNED_INT_2_10_10_10_REV: case GL_UNSIGNED_INT_10F_11F_11F_REV:
    case GL_UNSIGNED_INT_5_9_9_9_REV: case GL_UNSIGNED_INT_24_8: return 4;
    case GL_FLOAT_32_UNSIGNED_INT_24_8_REV: return 8;
    }
    uint32_t size = 0, comps = 0;
    switch (type) {
    case GL_BYTE: case GL_UNSIGNED_BYTE: size = 1; break;
    case GL_SHORT: case GL_UNSIGNED_SHORT: case GL_HALF_FLOAT: size = 2; break;
    case GL_INT: case GL_UNSIGNED_INT: case GL_FLOAT: size = 4; break;
    }
    switch (format) {
    case GL_RED: case GL_RED_INTEGER: case GL_ALPHA: case GL_LUMINANCE: case GL_DEPTH_COMPONENT: comps = 1; break;
    case GL_RG: case GL_RG_INTEGER: case GL_LUMINANCE_ALPHA: case GL_DEPTH_STENCIL: comps = 2; break;
    case GL_RGB: case GL_RGB_INTEGER: comps = 3; break;
    case GL_RGBA: case GL_RGBA_INTEGER: comps = 4; break;
    }
    return size * comps;
}

/* Bytes of a w×h×d image under a pixel store (GLES 3.0 3.7.1; same formula in the runners). */
static uint32_t image_bytes(const store *s, GLsizei w, GLsizei h, GLsizei d, GLenum format, GLenum type) {
    uint32_t bpp = pixel_bytes(format, type);
    if (w <= 0 || h <= 0 || d <= 0 || !bpp || s->alignment <= 0) return 0;
    uint32_t row_len = s->row_length > 0 ? (uint32_t)s->row_length : (uint32_t)w;
    uint32_t a = (uint32_t)s->alignment;
    uint32_t row = (row_len * bpp + a - 1) / a * a;
    uint32_t img_h = s->image_height > 0 ? (uint32_t)s->image_height : (uint32_t)h;
    uint32_t img = row * img_h;
    return (uint32_t)s->skip_images * img + (uint32_t)(d - 1) * img + (uint32_t)s->skip_rows * row +
           (uint32_t)(h - 1) * row + (uint32_t)s->skip_pixels * bpp + (uint32_t)w * bpp;
}

/* with an unpack buffer bound, "pixels" is an offset into it: no pointer, the offset as len */
#define UNPACK_PTR(p) (unpack_buffer ? (const void *)0 : (p))
#define UNPACK_LEN(p, w, h, d, f, t) (unpack_buffer ? (uint32_t)(uintptr_t)(p) : (p) ? image_bytes(&unpack, w, h, d, f, t) : 0)

/* ---- strings ---------------------------------------------------------------------------- */

static void shader_source(GLuint shader, GLsizei count, const GLchar *const *strings, const GLint *lengths) {
    size_t total = 0;
    for (GLsizei i = 0; i < count; i++)
        total += lengths && lengths[i] >= 0 ? (size_t)lengths[i] : strlen(strings[i]);
    char *all = (char *)malloc(total + 1), *p = all;
    if (!all) { c_error = GL_OUT_OF_MEMORY; return; }
    for (GLsizei i = 0; i < count; i++) {
        size_t n = lengths && lengths[i] >= 0 ? (size_t)lengths[i] : strlen(strings[i]);
        memcpy(p, strings[i], n);
        p += n;
    }
    gasm_gl_shader_source(shader, all, (uint32_t)total);
    free(all);
}

static void feedback_varyings(GLuint program, GLsizei count, const GLchar *const *names, GLenum mode) {
    size_t total = 0;
    for (GLsizei i = 0; i < count; i++) total += strlen(names[i]) + 1;
    char *all = (char *)malloc(total ? total : 1), *p = all;
    if (!all) { c_error = GL_OUT_OF_MEMORY; return; }
    for (GLsizei i = 0; i < count; i++) {
        size_t n = strlen(names[i]) + 1;
        memcpy(p, names[i], n);
        p += n;
    }
    gasm_gl_transform_feedback_varyings(program, all, (uint32_t)total, (uint32_t)count, mode);
    free(all);
}

/* The "returns the full length, copies if it fits" imports, as GL's truncating copies. */
typedef int32_t (*text_fn)(uint32_t, void *, uint32_t);
static void truncated(text_fn f, GLuint object, GLsizei size, GLsizei *length, GLchar *out) {
    int32_t n = f(object, 0, 0);
    if (n < 0 || size <= 0) { if (length) *length = 0; return; }
    char *tmp = (char *)malloc((size_t)n + 1);
    if (!tmp) { if (length) *length = 0; return; }
    f(object, tmp, (uint32_t)n);
    int32_t k = n < size - 1 ? n : size - 1;
    memcpy(out, tmp, (size_t)k);
    out[k] = 0;
    if (length) *length = k;
    free(tmp);
}
typedef int32_t (*text2_fn)(uint32_t, uint32_t, void *, uint32_t);
static void truncated2(text2_fn f, GLuint object, GLuint index, GLsizei size, GLsizei *length, GLchar *out) {
    int32_t n = f(object, index, 0, 0);
    if (n < 0 || size <= 0) { if (length) *length = 0; return; }
    char *tmp = (char *)malloc((size_t)n + 1);
    if (!tmp) { if (length) *length = 0; return; }
    f(object, index, tmp, (uint32_t)n);
    int32_t k = n < size - 1 ? n : size - 1;
    memcpy(out, tmp, (size_t)k);
    out[k] = 0;
    if (length) *length = k;
    free(tmp);
}
typedef int32_t (*info_fn)(uint32_t, uint32_t, void *, uint32_t, void *);
static void active_info(info_fn f, GLuint program, GLuint index, GLsizei size, GLsizei *length, GLint *out_size, GLenum *out_type, GLchar *name) {
    int32_t info[2] = { 0, 0 };
    int32_t n = f(program, index, 0, 0, info);
    if (n < 0) { if (length) *length = 0; return; }
    char *tmp = (char *)malloc((size_t)n + 1);
    if (!tmp) return;
    f(program, index, tmp, (uint32_t)n, info);
    int32_t k = size > 0 ? (n < size - 1 ? n : size - 1) : 0;
    if (size > 0) { memcpy(name, tmp, (size_t)k); name[k] = 0; }
    if (length) *length = k;
    if (out_size) *out_size = info[0];
    if (out_type) *out_type = (GLenum)info[1];
    free(tmp);
}

/* glGetString returns pointers that stay valid: one cached copy per name */
static const GLubyte *get_string(GLenum name) {
    static const GLenum names[] = { GL_VENDOR, GL_RENDERER, GL_VERSION, GL_SHADING_LANGUAGE_VERSION, GL_EXTENSIONS };
    static char *cache[5];
    for (int i = 0; i < 5; i++) {
        if (names[i] != name) continue;
        if (!cache[i]) {
            int32_t n = gasm_gl_get_string(name, 0, 0);
            if (n < 0) return 0;
            cache[i] = (char *)calloc((size_t)n + 1, 1);
            if (cache[i]) gasm_gl_get_string(name, cache[i], (uint32_t)n);
        }
        return (const GLubyte *)cache[i];
    }
    c_error = GL_INVALID_ENUM;
    return 0;
}

static char **ext_list;
static unsigned ext_count;
static unsigned extension_count(void) {
    if (!ext_list) {
        const char *all = (const char *)get_string(GL_EXTENSIONS);
        size_t len = all ? strlen(all) : 0;
        char *copy = (char *)malloc(len + 1);
        if (!copy) return 0;
        memcpy(copy, all ? all : "", len + 1);
        ext_list = (char **)calloc(len / 2 + 2, sizeof *ext_list);
        for (char *t = strtok(copy, " "); t && ext_list; t = strtok(0, " ")) ext_list[ext_count++] = t;
    }
    return ext_count;
}
static const GLubyte *extension(GLuint index) {
    if (index >= extension_count()) { c_error = GL_INVALID_VALUE; return 0; }
    return (const GLubyte *)ext_list[index];
}

/* ---- glMapBufferRange: a guest-memory copy, uploaded on unmap or flush ------------------- */

typedef struct { GLenum target; void *ptr; GLintptr offset; GLsizeiptr length; GLbitfield access; } mapping;
static mapping maps[8];

static mapping *mapping_for(GLenum target) {
    for (int i = 0; i < 8; i++)
        if (maps[i].ptr && maps[i].target == target) return &maps[i];
    return 0;
}

static void *map_range(GLenum target, GLintptr offset, GLsizeiptr length, GLbitfield access) {
    if (mapping_for(target) || length <= 0 || offset < 0) { c_error = GL_INVALID_OPERATION; return 0; }
    for (int i = 0; i < 8; i++) {
        if (maps[i].ptr) continue;
        void *p = malloc((size_t)length);
        if (!p) { c_error = GL_OUT_OF_MEMORY; return 0; }
        if ((access & GL_MAP_READ_BIT) || !(access & (GL_MAP_INVALIDATE_RANGE_BIT | GL_MAP_INVALIDATE_BUFFER_BIT)))
            gasm_gl_get_buffer_sub_data(target, (uint32_t)offset, p, (uint32_t)length);   /* the current contents */
        maps[i] = (mapping){ target, p, offset, length, access };
        return p;
    }
    c_error = GL_OUT_OF_MEMORY;
    return 0;
}

static void flush_mapped(GLenum target, GLintptr offset, GLsizeiptr length) {
    mapping *m = mapping_for(target);
    if (!m || offset < 0 || offset + length > m->length) { c_error = GL_INVALID_VALUE; return; }
    gasm_gl_buffer_sub_data(target, (uint32_t)(m->offset + offset), (char *)m->ptr + offset, (uint32_t)length);
}

static GLboolean unmap(GLenum target) {
    mapping *m = mapping_for(target);
    if (!m) { c_error = GL_INVALID_OPERATION; return GL_FALSE; }
    if ((m->access & GL_MAP_WRITE_BIT) && !(m->access & GL_MAP_FLUSH_EXPLICIT_BIT))
        gasm_gl_buffer_sub_data(target, (uint32_t)m->offset, m->ptr, (uint32_t)m->length);
    free(m->ptr);
    m->ptr = 0;
    return GL_TRUE;
}

/* ---- the API ------------------------------------------------------------------------------ */

''' + '\n'.join(f'{proto(n, r, p)} {{\n    {body_of(p, b).rstrip()}\n}}\n' for n, r, p, b in impl)
open(os.path.join(ROOT, 'sdk/c/src/gasm_gl.c'), 'w').write(c)
# ---- the Rust SDK's GLES 3.0 C API (guests/gasm/src/gles_gen.rs) --------------------------
# The same functions as gasm_gl.c, for Rust guests (no libc to link the C file) and
# GL loaders such as glow: functions that forward one to one are generated here; the
# rest (the C OVERRIDES above) are written by hand in guests/gasm/src/gles.rs. Every
# function gets a type assertion, so the hand-written signatures are checked against gl.xml.
RUSTG = {'GLenum': 'u32', 'GLuint': 'u32', 'GLbitfield': 'u32', 'GLint': 'i32', 'GLsizei': 'i32',
         'GLfloat': 'f32', 'GLboolean': 'u8', 'GLintptr': 'isize', 'GLsizeiptr': 'isize', 'GLint64': 'i64',
         'GLuint64': 'u64', 'GLsync': '*const c_void', 'GLubyte': 'u8', 'GLchar': 'u8'}
def rtype(t):
    t = t.replace('struct __GLsync *', 'GLsync').strip()
    if '*' in t:
        return '*const c_void' if 'const' in t else '*mut c_void'
    return RUSTG[t.replace('const', '').strip()]
fn_type = lambda r, ps: f'unsafe extern "C" fn({", ".join(rtype(t) for t, _ in ps)})' + ('' if r == 'void' else f' -> {rtype(r)}')
g = ['// The GLES 3.0 functions that forward one to one to gasm:gl, the type of every', '// function, and the name table.',
     '// GENERATED by scripts/gen-gl-headers.py from gl.xml and spec/abi.json: do not edit.', '']
names = list(dict.fromkeys(want_cmds))
for n in names:
    r, ps = cmds[n]
    if n in O:
        continue
    args = ', '.join(f'p{i}: {rtype(t)}' for i, (t, _) in enumerate(ps))
    call = f'crate::sys::gl_{snake(n)}({", ".join(f"p{i} as _" for i in range(len(ps)))})'
    ret = '' if r == 'void' else f' -> {rtype(r)}'
    g.append(f'pub unsafe extern "C" fn {n}({args}){ret} {{\n    unsafe {{ {call}{" as _" if r != "void" else ""} }}\n}}')
g += ['', '// every function has the C signature of gl.xml (glow calls them through pointers)', 'const _: () = {']
for n in names:
    r, ps = cmds[n]
    g.append(f'    let _: {fn_type(r, ps)} = {n};')
g += ['};', '', '/// The function `name` (a GLES 3.0 entry point), or null.',
      'pub fn lookup(name: &[u8]) -> *const c_void {', '    match name {']
for n in names:
    g.append(f'        b"{n}" => {n} as *const c_void,')
g += ['        _ => std::ptr::null(),', '    }', '}', '']
open(os.path.join(ROOT, 'guests/gasm/src/gles_gen.rs'), 'w').write('\n'.join(g))
# ---- the native runner's GLES function table (runners/native/src/gles.rs) -----------------
RUST = {'GLenum': 'u32', 'GLuint': 'u32', 'GLbitfield': 'u32', 'GLint': 'i32', 'GLsizei': 'i32',
        'GLfloat': 'f32', 'GLboolean': 'u8', 'GLintptr': 'isize', 'GLsizeiptr': 'isize', 'GLint64': 'i64',
        'GLuint64': 'u64', 'GLsync': '*const c_void', 'void': '()', 'GLubyte': 'u8', 'GLchar': 'u8'}
def rust_type(t):
    t = t.replace('struct __GLsync *', 'GLsync').strip()
    if '*' in t:
        return '*const c_void' if 'const' in t else '*mut c_void'
    return RUST[t.replace('const', '').strip()]
EXTRA = [('glRequestExtensionANGLE', 'void', [('const GLchar *', 'name')])]
rs = ['//! The GLES 3.0 functions (and the ANGLE extensions gasm uses), loaded at run time.',
      '//! GENERATED by scripts/gen-gl-headers.py from the Khronos registry: do not edit.', '',
      '#![allow(non_snake_case, clippy::type_complexity, clippy::too_many_arguments, clippy::missing_safety_doc, clippy::missing_transmute_annotations, dead_code)]', '',
      'use std::ffi::{c_void, CStr};', '', 'pub struct Gles {']
fns = [(n, *cmds[n]) for n in dict.fromkeys(want_cmds)] + EXTRA
for n, r, ps in fns:
    ret = '' if r == 'void' else f' -> {rust_type(r)}'
    rs.append(f'    pub {n}: unsafe extern "system" fn({", ".join(rust_type(t) for t, _ in ps)}){ret},')
rs += ['}', '', 'impl Gles {',
       '    /// Every function through `get` (eglGetProcAddress); the name of the first that is missing otherwise.',
       '    pub unsafe fn load(get: impl Fn(&CStr) -> *const c_void) -> Result<Gles, String> {',
       '        let f = |name: &CStr| -> Result<*const c_void, String> {',
       '            let p = get(name);',
       '            if p.is_null() { Err(format!("{} is missing", name.to_string_lossy())) } else { Ok(p) }',
       '        };',
       '        unsafe {',
       '            Ok(Gles {']
for n, r, ps in fns:
    rs.append(f'                {n}: std::mem::transmute::<*const c_void, _>(f(c"{n}")?),')
rs += ['            })', '        }', '    }', '}', '']
open(os.path.join(ROOT, 'runners/native/src/gles.rs'), 'w').write('\n'.join(rs))
print(f'{len(impl)} functions, {len(want_enums)} enums; wrote sdk/c/include/GLES3/gl3.h, sdk/c/src/gasm_gl.c, runners/native/src/gles.rs')
