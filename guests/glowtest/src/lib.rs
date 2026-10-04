//! OpenGL ES 3 from Rust through glow: glow's own API (HasContext) on gasm:gl, via
//! the gasm fork of glow (sdk/glow) and gasm's GLES functions. A rotating textured
//! quad with a uniform block, an instanced draw and a mapped buffer; the GL errors of
//! a few deliberate mistakes are uploaded every frame, so the hash covers them.

use glow::HasContext;

const VS: &str = r#"#version 300 es
layout(location = 0) in vec2 pos;
layout(location = 1) in vec2 uv;
layout(std140) uniform Scene { vec4 tint; float angle; };
out vec2 v_uv;
void main() {
  float a = angle + float(gl_InstanceID) * 2.0944;
  mat2 r = mat2(cos(a), sin(a), -sin(a), cos(a));
  v_uv = uv;
  gl_Position = vec4(r * pos * 0.45 + vec2(float(gl_InstanceID - 1) * 0.5, 0.0), 0.0, 1.0);
}"#;
const FS: &str = r#"#version 300 es
precision highp float;
layout(std140) uniform Scene { vec4 tint; float angle; };
uniform sampler2D tex;
in vec2 v_uv;
out vec4 color;
void main() { color = texture(tex, v_uv) * tint; }"#;

struct GlowTest {
    gl: glow::Context,
    vao: glow::VertexArray,
    ubo: glow::Buffer,
    errbuf: glow::Buffer,
    errors: Vec<u32>,
    frame: u32,
}

fn compile(gl: &glow::Context, ty: u32, src: &str) -> Result<glow::Shader, String> {
    unsafe {
        let s = gl.create_shader(ty)?;
        gl.shader_source(s, src);
        gl.compile_shader(s);
        if !gl.get_shader_compile_status(s) {
            return Err(gl.get_shader_info_log(s));
        }
        Ok(s)
    }
}

impl gasm::Game for GlowTest {
    fn init() -> Result<Self, String> {
        let gl = unsafe { glow::Context::from_loader_function_cstr(gasm::gles::get_proc_address) };
        let v = gl.version();
        gasm::log(&format!("glowtest: GL {}.{} (es: {}), {}", v.major, v.minor, v.is_embedded, v.vendor_info));
        unsafe {
            let program = gl.create_program()?;
            let (vs, fs) = (compile(&gl, glow::VERTEX_SHADER, VS)?, compile(&gl, glow::FRAGMENT_SHADER, FS)?);
            gl.attach_shader(program, vs);
            gl.attach_shader(program, fs);
            gl.link_program(program);
            if !gl.get_program_link_status(program) {
                return Err(gl.get_program_info_log(program));
            }
            gl.delete_shader(vs);
            gl.delete_shader(fs);
            gl.use_program(Some(program));
            gl.uniform_1_i32(gl.get_uniform_location(program, "tex").as_ref(), 0);
            if let Some(block) = gl.get_uniform_block_index(program, "Scene") {
                gl.uniform_block_binding(program, block, 0);
            }

            let quad: [f32; 16] = [-1.0, -1.0, 0.0, 1.0, 1.0, -1.0, 1.0, 1.0, 1.0, 1.0, 1.0, 0.0, -1.0, 1.0, 0.0, 0.0];
            let idx: [u16; 6] = [0, 1, 2, 0, 2, 3];
            let vao = gl.create_vertex_array()?;
            gl.bind_vertex_array(Some(vao));
            let vbo = gl.create_buffer()?;
            gl.bind_buffer(glow::ARRAY_BUFFER, Some(vbo));
            gl.buffer_data_u8_slice(glow::ARRAY_BUFFER, bytes(&quad), glow::STATIC_DRAW);
            gl.enable_vertex_attrib_array(0);
            gl.vertex_attrib_pointer_f32(0, 2, glow::FLOAT, false, 16, 0);
            gl.enable_vertex_attrib_array(1);
            gl.vertex_attrib_pointer_f32(1, 2, glow::FLOAT, false, 16, 8);
            let ibo = gl.create_buffer()?;
            gl.bind_buffer(glow::ELEMENT_ARRAY_BUFFER, Some(ibo));
            gl.buffer_data_u8_slice(glow::ELEMENT_ARRAY_BUFFER, bytes(&idx), glow::STATIC_DRAW);
            gl.bind_vertex_array(None);

            let ubo = gl.create_buffer()?;
            gl.bind_buffer_base(glow::UNIFORM_BUFFER, 0, Some(ubo));
            gl.buffer_data_size(glow::UNIFORM_BUFFER, 32, glow::DYNAMIC_DRAW);

            // an 8x8 checker, RGBA
            let mut px = [0u8; 8 * 8 * 4];
            for (i, p) in px.chunks_mut(4).enumerate() {
                let (x, y) = (i % 8, i / 8);
                let on = (x + y) % 2 == 0;
                p.copy_from_slice(&[if on { 250 } else { 40 }, (x * 30) as u8, (y * 30) as u8, 255]);
            }
            let tex = gl.create_texture()?;
            gl.bind_texture(glow::TEXTURE_2D, Some(tex));
            gl.tex_image_2d(glow::TEXTURE_2D, 0, glow::RGBA8 as i32, 8, 8, 0, glow::RGBA, glow::UNSIGNED_BYTE,
                glow::PixelUnpackData::Slice(Some(&px)));
            gl.tex_parameter_i32(glow::TEXTURE_2D, glow::TEXTURE_MIN_FILTER, glow::NEAREST as i32);
            gl.tex_parameter_i32(glow::TEXTURE_2D, glow::TEXTURE_MAG_FILTER, glow::NEAREST as i32);

            // mistakes with known errors (glow passes them through)
            let mut errors = Vec::new();
            let mut note = |gl: &glow::Context| {
                loop {
                    let e = gl.get_error();
                    if e == 0 { break }
                    errors.push(e);
                }
                errors.push(0);
            };
            gl.bind_buffer(0x1234, Some(vbo)); note(&gl);                              // INVALID_ENUM
            gl.draw_arrays(glow::TRIANGLES, 0, -1); note(&gl);                         // INVALID_VALUE
            gl.pixel_store_i32(glow::UNPACK_ALIGNMENT, 3); note(&gl);                  // INVALID_VALUE
            gl.bind_buffer(glow::COPY_READ_BUFFER, None);
            gl.buffer_sub_data_u8_slice(glow::COPY_READ_BUFFER, 0, &[1, 2, 3, 4]); note(&gl); // INVALID_OPERATION
            gasm::log(&format!("glowtest: {} error records", errors.len()));

            let errbuf = gl.create_buffer()?;
            Ok(GlowTest { gl, vao, ubo, errbuf, errors, frame: 0 })
        }
    }

    fn frame(&mut self) {
        let gl = &self.gl;
        let t = self.frame as f32 / 60.0;
        unsafe {
            let (w, h) = (gasm::sys::gl_width() as i32, gasm::sys::gl_height() as i32);
            gl.viewport(0, 0, w, h);
            gl.clear_color(0.08, 0.1, 0.16, 1.0);
            gl.clear(glow::COLOR_BUFFER_BIT);
            // the uniform block through a mapped range (gasm emulates it in guest memory)
            gl.bind_buffer(glow::UNIFORM_BUFFER, Some(self.ubo));
            let p = gl.map_buffer_range(glow::UNIFORM_BUFFER, 0, 32, glow::MAP_WRITE_BIT | glow::MAP_INVALIDATE_BUFFER_BIT);
            if !p.is_null() {
                let s = std::slice::from_raw_parts_mut(p as *mut f32, 8);
                s[..5].copy_from_slice(&[1.0, 0.85, 0.7, 1.0, t]);
                gl.unmap_buffer(glow::UNIFORM_BUFFER);
            }
            gl.bind_vertex_array(Some(self.vao));
            gl.draw_elements_instanced(glow::TRIANGLES, 6, glow::UNSIGNED_SHORT, 0, 3);
            gl.bind_vertex_array(None);
            let e = gl.get_error();
            if e != 0 {
                self.errors.push(0xf000 | e);
            }
            gl.bind_buffer(glow::COPY_WRITE_BUFFER, Some(self.errbuf));
            gl.buffer_data_u8_slice(glow::COPY_WRITE_BUFFER, bytes(&self.errors), glow::DYNAMIC_DRAW);
        }
        self.frame += 1;
    }
}

fn bytes<T: Copy>(v: &[T]) -> &[u8] {
    unsafe { std::slice::from_raw_parts(v.as_ptr() as *const u8, std::mem::size_of_val(v)) }
}

gasm::title!("glow test");
gasm::game!(GlowTest);
