// gasm:gl in the native stub host: names count up, everything else is 0 (there is
// no GL natively). GENERATED from spec/abi.json by scripts/gen-abi.mjs; include!d in native.rs.

static GL_NAMES: std::sync::atomic::AtomicU32 = std::sync::atomic::AtomicU32::new(1);
fn gl_name() -> u32 {
    GL_NAMES.fetch_add(1, std::sync::atomic::Ordering::Relaxed)
}

pub unsafe fn gl_width() -> u32 { 1280 }
pub unsafe fn gl_height() -> u32 { 720 }
pub unsafe fn gl_frame_shown() -> u32 { 0 }
pub unsafe fn gl_present() {}
pub unsafe fn gl_get_error() -> u32 { 0 }
pub unsafe fn gl_get_string(_name: u32, _dst: *mut u8, _cap: u32) -> i32 { 0 }
pub unsafe fn gl_enable_extension(_name: *const u8, _name_len: u32) -> u32 { 0 }
pub unsafe fn gl_get_integerv(_pname: u32, _dst: *mut u8, _count: u32) -> i32 { 0 }
pub unsafe fn gl_get_floatv(_pname: u32, _dst: *mut u8, _count: u32) -> i32 { 0 }
pub unsafe fn gl_get_integer64v(_pname: u32, _dst: *mut u8, _count: u32) -> i32 { 0 }
pub unsafe fn gl_get_integeri_v(_target: u32, _index: u32, _dst: *mut u8, _count: u32) -> i32 { 0 }
pub unsafe fn gl_get_internalformativ(_target: u32, _internalformat: u32, _pname: u32, _dst: *mut u8, _count: u32) -> i32 { 0 }
pub unsafe fn gl_get_shader_precision_format(_shadertype: u32, _precisiontype: u32, _dst: *mut u8) {}
pub unsafe fn gl_active_texture(_texture: u32) {}
pub unsafe fn gl_blend_color(_red: f32, _green: f32, _blue: f32, _alpha: f32) {}
pub unsafe fn gl_blend_equation(_mode: u32) {}
pub unsafe fn gl_blend_equation_separate(_mode_rgb: u32, _mode_alpha: u32) {}
pub unsafe fn gl_blend_func(_sfactor: u32, _dfactor: u32) {}
pub unsafe fn gl_blend_func_separate(_src_rgb: u32, _dst_rgb: u32, _src_alpha: u32, _dst_alpha: u32) {}
pub unsafe fn gl_clear(_mask: u32) {}
pub unsafe fn gl_clear_color(_red: f32, _green: f32, _blue: f32, _alpha: f32) {}
pub unsafe fn gl_clear_depthf(_depth: f32) {}
pub unsafe fn gl_clear_stencil(_s: i32) {}
pub unsafe fn gl_color_mask(_red: u32, _green: u32, _blue: u32, _alpha: u32) {}
pub unsafe fn gl_cull_face(_mode: u32) {}
pub unsafe fn gl_depth_func(_func: u32) {}
pub unsafe fn gl_depth_mask(_flag: u32) {}
pub unsafe fn gl_depth_rangef(_near: f32, _far: f32) {}
pub unsafe fn gl_disable(_cap: u32) {}
pub unsafe fn gl_enable(_cap: u32) {}
pub unsafe fn gl_is_enabled(_cap: u32) -> u32 { 0 }
pub unsafe fn gl_front_face(_mode: u32) {}
pub unsafe fn gl_hint(_target: u32, _mode: u32) {}
pub unsafe fn gl_line_width(_width: f32) {}
pub unsafe fn gl_pixel_storei(_pname: u32, _param: i32) {}
pub unsafe fn gl_polygon_offset(_factor: f32, _units: f32) {}
pub unsafe fn gl_sample_coverage(_value: f32, _invert: u32) {}
pub unsafe fn gl_scissor(_x: i32, _y: i32, _width: i32, _height: i32) {}
pub unsafe fn gl_viewport(_x: i32, _y: i32, _width: i32, _height: i32) {}
pub unsafe fn gl_stencil_func(_func: u32, _ref: i32, _mask: u32) {}
pub unsafe fn gl_stencil_func_separate(_face: u32, _func: u32, _ref: i32, _mask: u32) {}
pub unsafe fn gl_stencil_mask(_mask: u32) {}
pub unsafe fn gl_stencil_mask_separate(_face: u32, _mask: u32) {}
pub unsafe fn gl_stencil_op(_fail: u32, _zfail: u32, _zpass: u32) {}
pub unsafe fn gl_stencil_op_separate(_face: u32, _sfail: u32, _dpfail: u32, _dppass: u32) {}
pub unsafe fn gl_finish() {}
pub unsafe fn gl_flush() {}
pub unsafe fn gl_create_buffer() -> u32 { gl_name() }
pub unsafe fn gl_delete_buffer(_buffer: u32) {}
pub unsafe fn gl_is_buffer(_buffer: u32) -> u32 { 0 }
pub unsafe fn gl_bind_buffer(_target: u32, _buffer: u32) {}
pub unsafe fn gl_bind_buffer_base(_target: u32, _index: u32, _buffer: u32) {}
pub unsafe fn gl_bind_buffer_range(_target: u32, _index: u32, _buffer: u32, _offset: u32, _size: u32) {}
pub unsafe fn gl_buffer_data(_target: u32, _data: *const u8, _len: u32, _usage: u32) {}
pub unsafe fn gl_buffer_sub_data(_target: u32, _offset: u32, _data: *const u8, _len: u32) {}
pub unsafe fn gl_copy_buffer_sub_data(_read_target: u32, _write_target: u32, _read_offset: u32, _write_offset: u32, _size: u32) {}
pub unsafe fn gl_get_buffer_sub_data(_target: u32, _offset: u32, _dst: *mut u8, _len: u32) {}
pub unsafe fn gl_get_buffer_parameteriv(_target: u32, _pname: u32) -> i32 { 0 }
pub unsafe fn gl_create_vertex_array() -> u32 { gl_name() }
pub unsafe fn gl_delete_vertex_array(_array: u32) {}
pub unsafe fn gl_is_vertex_array(_array: u32) -> u32 { 0 }
pub unsafe fn gl_bind_vertex_array(_array: u32) {}
pub unsafe fn gl_enable_vertex_attrib_array(_index: u32) {}
pub unsafe fn gl_disable_vertex_attrib_array(_index: u32) {}
pub unsafe fn gl_vertex_attrib_pointer(_index: u32, _size: i32, _type: u32, _normalized: u32, _stride: i32, _offset: u32) {}
pub unsafe fn gl_vertex_attrib_ipointer(_index: u32, _size: i32, _type: u32, _stride: i32, _offset: u32) {}
pub unsafe fn gl_vertex_attrib_divisor(_index: u32, _divisor: u32) {}
pub unsafe fn gl_vertex_attrib4f(_index: u32, _x: f32, _y: f32, _z: f32, _w: f32) {}
pub unsafe fn gl_vertex_attribi4i(_index: u32, _x: i32, _y: i32, _z: i32, _w: i32) {}
pub unsafe fn gl_vertex_attribi4ui(_index: u32, _x: u32, _y: u32, _z: u32, _w: u32) {}
pub unsafe fn gl_get_vertex_attribiv(_index: u32, _pname: u32) -> i32 { 0 }
pub unsafe fn gl_get_vertex_attribfv(_index: u32, _pname: u32, _dst: *mut u8, _count: u32) -> i32 { 0 }
pub unsafe fn gl_get_vertex_attrib_offset(_index: u32, _pname: u32) -> u32 { 0 }
pub unsafe fn gl_draw_arrays(_mode: u32, _first: i32, _count: i32) {}
pub unsafe fn gl_draw_elements(_mode: u32, _count: i32, _type: u32, _offset: u32) {}
pub unsafe fn gl_draw_arrays_instanced(_mode: u32, _first: i32, _count: i32, _instances: i32) {}
pub unsafe fn gl_draw_elements_instanced(_mode: u32, _count: i32, _type: u32, _offset: u32, _instances: i32) {}
pub unsafe fn gl_draw_range_elements(_mode: u32, _start: u32, _end: u32, _count: i32, _type: u32, _offset: u32) {}
pub unsafe fn gl_draw_buffers(_bufs: *const u8, _count: u32) {}
pub unsafe fn gl_clear_bufferiv(_buffer: u32, _drawbuffer: i32, _value: *const u8, _count: u32) {}
pub unsafe fn gl_clear_bufferuiv(_buffer: u32, _drawbuffer: i32, _value: *const u8, _count: u32) {}
pub unsafe fn gl_clear_bufferfv(_buffer: u32, _drawbuffer: i32, _value: *const u8, _count: u32) {}
pub unsafe fn gl_clear_bufferfi(_buffer: u32, _drawbuffer: i32, _depth: f32, _stencil: i32) {}
pub unsafe fn gl_create_texture() -> u32 { gl_name() }
pub unsafe fn gl_delete_texture(_texture: u32) {}
pub unsafe fn gl_is_texture(_texture: u32) -> u32 { 0 }
pub unsafe fn gl_bind_texture(_target: u32, _texture: u32) {}
pub unsafe fn gl_tex_parameteri(_target: u32, _pname: u32, _param: i32) {}
pub unsafe fn gl_tex_parameterf(_target: u32, _pname: u32, _param: f32) {}
pub unsafe fn gl_get_tex_parameteriv(_target: u32, _pname: u32) -> i32 { 0 }
pub unsafe fn gl_get_tex_parameterfv(_target: u32, _pname: u32) -> f32 { 0.0 }
pub unsafe fn gl_tex_image_2d(_target: u32, _level: i32, _internalformat: i32, _width: i32, _height: i32, _border: i32, _format: u32, _type: u32, _pixels: *const u8, _len: u32) {}
pub unsafe fn gl_tex_image_3d(_target: u32, _level: i32, _internalformat: i32, _width: i32, _height: i32, _depth: i32, _border: i32, _format: u32, _type: u32, _pixels: *const u8, _len: u32) {}
pub unsafe fn gl_tex_sub_image_2d(_target: u32, _level: i32, _x: i32, _y: i32, _width: i32, _height: i32, _format: u32, _type: u32, _pixels: *const u8, _len: u32) {}
pub unsafe fn gl_tex_sub_image_3d(_target: u32, _level: i32, _x: i32, _y: i32, _z: i32, _width: i32, _height: i32, _depth: i32, _format: u32, _type: u32, _pixels: *const u8, _len: u32) {}
pub unsafe fn gl_tex_storage_2d(_target: u32, _levels: i32, _internalformat: u32, _width: i32, _height: i32) {}
pub unsafe fn gl_tex_storage_3d(_target: u32, _levels: i32, _internalformat: u32, _width: i32, _height: i32, _depth: i32) {}
pub unsafe fn gl_compressed_tex_image_2d(_target: u32, _level: i32, _internalformat: u32, _width: i32, _height: i32, _border: i32, _data: *const u8, _len: u32) {}
pub unsafe fn gl_compressed_tex_image_3d(_target: u32, _level: i32, _internalformat: u32, _width: i32, _height: i32, _depth: i32, _border: i32, _data: *const u8, _len: u32) {}
pub unsafe fn gl_compressed_tex_sub_image_2d(_target: u32, _level: i32, _x: i32, _y: i32, _width: i32, _height: i32, _format: u32, _data: *const u8, _len: u32) {}
pub unsafe fn gl_compressed_tex_sub_image_3d(_target: u32, _level: i32, _x: i32, _y: i32, _z: i32, _width: i32, _height: i32, _depth: i32, _format: u32, _data: *const u8, _len: u32) {}
pub unsafe fn gl_copy_tex_image_2d(_target: u32, _level: i32, _internalformat: u32, _x: i32, _y: i32, _width: i32, _height: i32, _border: i32) {}
pub unsafe fn gl_copy_tex_sub_image_2d(_target: u32, _level: i32, _xoffset: i32, _yoffset: i32, _x: i32, _y: i32, _width: i32, _height: i32) {}
pub unsafe fn gl_copy_tex_sub_image_3d(_target: u32, _level: i32, _xoffset: i32, _yoffset: i32, _zoffset: i32, _x: i32, _y: i32, _width: i32, _height: i32) {}
pub unsafe fn gl_generate_mipmap(_target: u32) {}
pub unsafe fn gl_create_sampler() -> u32 { gl_name() }
pub unsafe fn gl_delete_sampler(_sampler: u32) {}
pub unsafe fn gl_is_sampler(_sampler: u32) -> u32 { 0 }
pub unsafe fn gl_bind_sampler(_unit: u32, _sampler: u32) {}
pub unsafe fn gl_sampler_parameteri(_sampler: u32, _pname: u32, _param: i32) {}
pub unsafe fn gl_sampler_parameterf(_sampler: u32, _pname: u32, _param: f32) {}
pub unsafe fn gl_get_sampler_parameteriv(_sampler: u32, _pname: u32) -> i32 { 0 }
pub unsafe fn gl_get_sampler_parameterfv(_sampler: u32, _pname: u32) -> f32 { 0.0 }
pub unsafe fn gl_create_framebuffer() -> u32 { gl_name() }
pub unsafe fn gl_delete_framebuffer(_framebuffer: u32) {}
pub unsafe fn gl_is_framebuffer(_framebuffer: u32) -> u32 { 0 }
pub unsafe fn gl_bind_framebuffer(_target: u32, _framebuffer: u32) {}
pub unsafe fn gl_check_framebuffer_status(_target: u32) -> u32 { 0 }
pub unsafe fn gl_framebuffer_texture_2d(_target: u32, _attachment: u32, _textarget: u32, _texture: u32, _level: i32) {}
pub unsafe fn gl_framebuffer_texture_layer(_target: u32, _attachment: u32, _texture: u32, _level: i32, _layer: i32) {}
pub unsafe fn gl_framebuffer_renderbuffer(_target: u32, _attachment: u32, _renderbuffertarget: u32, _renderbuffer: u32) {}
pub unsafe fn gl_get_framebuffer_attachment_parameteriv(_target: u32, _attachment: u32, _pname: u32) -> i32 { 0 }
pub unsafe fn gl_blit_framebuffer(_src_x0: i32, _src_y0: i32, _src_x1: i32, _src_y1: i32, _dst_x0: i32, _dst_y0: i32, _dst_x1: i32, _dst_y1: i32, _mask: u32, _filter: u32) {}
pub unsafe fn gl_invalidate_framebuffer(_target: u32, _attachments: *const u8, _count: u32) {}
pub unsafe fn gl_invalidate_sub_framebuffer(_target: u32, _attachments: *const u8, _count: u32, _x: i32, _y: i32, _width: i32, _height: i32) {}
pub unsafe fn gl_read_buffer(_src: u32) {}
pub unsafe fn gl_read_pixels(_x: i32, _y: i32, _width: i32, _height: i32, _format: u32, _type: u32, _dst: *mut u8, _len: u32) {}
pub unsafe fn gl_create_renderbuffer() -> u32 { gl_name() }
pub unsafe fn gl_delete_renderbuffer(_renderbuffer: u32) {}
pub unsafe fn gl_is_renderbuffer(_renderbuffer: u32) -> u32 { 0 }
pub unsafe fn gl_bind_renderbuffer(_target: u32, _renderbuffer: u32) {}
pub unsafe fn gl_renderbuffer_storage(_target: u32, _internalformat: u32, _width: i32, _height: i32) {}
pub unsafe fn gl_renderbuffer_storage_multisample(_target: u32, _samples: i32, _internalformat: u32, _width: i32, _height: i32) {}
pub unsafe fn gl_get_renderbuffer_parameteriv(_target: u32, _pname: u32) -> i32 { 0 }
pub unsafe fn gl_create_shader(_type: u32) -> u32 { gl_name() }
pub unsafe fn gl_delete_shader(_shader: u32) {}
pub unsafe fn gl_is_shader(_shader: u32) -> u32 { 0 }
pub unsafe fn gl_shader_source(_shader: u32, _source: *const u8, _source_len: u32) {}
pub unsafe fn gl_compile_shader(_shader: u32) {}
pub unsafe fn gl_get_shaderiv(_shader: u32, _pname: u32) -> i32 { 0 }
pub unsafe fn gl_get_shader_info_log(_shader: u32, _dst: *mut u8, _cap: u32) -> i32 { 0 }
pub unsafe fn gl_get_shader_source(_shader: u32, _dst: *mut u8, _cap: u32) -> i32 { 0 }
pub unsafe fn gl_create_program() -> u32 { gl_name() }
pub unsafe fn gl_delete_program(_program: u32) {}
pub unsafe fn gl_is_program(_program: u32) -> u32 { 0 }
pub unsafe fn gl_attach_shader(_program: u32, _shader: u32) {}
pub unsafe fn gl_detach_shader(_program: u32, _shader: u32) {}
pub unsafe fn gl_link_program(_program: u32) {}
pub unsafe fn gl_use_program(_program: u32) {}
pub unsafe fn gl_validate_program(_program: u32) {}
pub unsafe fn gl_get_programiv(_program: u32, _pname: u32) -> i32 { 0 }
pub unsafe fn gl_get_program_info_log(_program: u32, _dst: *mut u8, _cap: u32) -> i32 { 0 }
pub unsafe fn gl_get_attached_shaders(_program: u32, _dst: *mut u8, _count: u32) -> i32 { 0 }
pub unsafe fn gl_bind_attrib_location(_program: u32, _index: u32, _name: *const u8, _name_len: u32) {}
pub unsafe fn gl_get_attrib_location(_program: u32, _name: *const u8, _name_len: u32) -> i32 { 0 }
pub unsafe fn gl_get_frag_data_location(_program: u32, _name: *const u8, _name_len: u32) -> i32 { 0 }
pub unsafe fn gl_get_active_attrib(_program: u32, _index: u32, _name: *mut u8, _cap: u32, _info: *mut u8) -> i32 { 0 }
pub unsafe fn gl_get_active_uniform(_program: u32, _index: u32, _name: *mut u8, _cap: u32, _info: *mut u8) -> i32 { 0 }
pub unsafe fn gl_get_uniform_location(_program: u32, _name: *const u8, _name_len: u32) -> i32 { 0 }
pub unsafe fn gl_get_uniform_index(_program: u32, _name: *const u8, _name_len: u32) -> u32 { 0 }
pub unsafe fn gl_get_active_uniformsiv(_program: u32, _indices: *const u8, _count: u32, _pname: u32, _dst: *mut u8) {}
pub unsafe fn gl_get_uniform_block_index(_program: u32, _name: *const u8, _name_len: u32) -> u32 { 0 }
pub unsafe fn gl_get_active_uniform_block_name(_program: u32, _index: u32, _dst: *mut u8, _cap: u32) -> i32 { 0 }
pub unsafe fn gl_get_active_uniform_blockiv(_program: u32, _index: u32, _pname: u32, _dst: *mut u8, _count: u32) -> i32 { 0 }
pub unsafe fn gl_uniform_block_binding(_program: u32, _index: u32, _binding: u32) {}
pub unsafe fn gl_get_uniformfv(_program: u32, _location: i32, _dst: *mut u8, _count: u32) -> i32 { 0 }
pub unsafe fn gl_get_uniformiv(_program: u32, _location: i32, _dst: *mut u8, _count: u32) -> i32 { 0 }
pub unsafe fn gl_get_uniformuiv(_program: u32, _location: i32, _dst: *mut u8, _count: u32) -> i32 { 0 }
pub unsafe fn gl_transform_feedback_varyings(_program: u32, _names: *const u8, _len: u32, _count: u32, _buffer_mode: u32) {}
pub unsafe fn gl_get_transform_feedback_varying(_program: u32, _index: u32, _name: *mut u8, _cap: u32, _info: *mut u8) -> i32 { 0 }
pub unsafe fn gl_uniform1f(_location: i32, _x: f32) {}
pub unsafe fn gl_uniform2f(_location: i32, _x: f32, _y: f32) {}
pub unsafe fn gl_uniform3f(_location: i32, _x: f32, _y: f32, _z: f32) {}
pub unsafe fn gl_uniform4f(_location: i32, _x: f32, _y: f32, _z: f32, _w: f32) {}
pub unsafe fn gl_uniform1i(_location: i32, _x: i32) {}
pub unsafe fn gl_uniform2i(_location: i32, _x: i32, _y: i32) {}
pub unsafe fn gl_uniform3i(_location: i32, _x: i32, _y: i32, _z: i32) {}
pub unsafe fn gl_uniform4i(_location: i32, _x: i32, _y: i32, _z: i32, _w: i32) {}
pub unsafe fn gl_uniform1ui(_location: i32, _x: u32) {}
pub unsafe fn gl_uniform2ui(_location: i32, _x: u32, _y: u32) {}
pub unsafe fn gl_uniform3ui(_location: i32, _x: u32, _y: u32, _z: u32) {}
pub unsafe fn gl_uniform4ui(_location: i32, _x: u32, _y: u32, _z: u32, _w: u32) {}
pub unsafe fn gl_uniform1fv(_location: i32, _count: i32, _value: *const u8) {}
pub unsafe fn gl_uniform2fv(_location: i32, _count: i32, _value: *const u8) {}
pub unsafe fn gl_uniform3fv(_location: i32, _count: i32, _value: *const u8) {}
pub unsafe fn gl_uniform4fv(_location: i32, _count: i32, _value: *const u8) {}
pub unsafe fn gl_uniform1iv(_location: i32, _count: i32, _value: *const u8) {}
pub unsafe fn gl_uniform2iv(_location: i32, _count: i32, _value: *const u8) {}
pub unsafe fn gl_uniform3iv(_location: i32, _count: i32, _value: *const u8) {}
pub unsafe fn gl_uniform4iv(_location: i32, _count: i32, _value: *const u8) {}
pub unsafe fn gl_uniform1uiv(_location: i32, _count: i32, _value: *const u8) {}
pub unsafe fn gl_uniform2uiv(_location: i32, _count: i32, _value: *const u8) {}
pub unsafe fn gl_uniform3uiv(_location: i32, _count: i32, _value: *const u8) {}
pub unsafe fn gl_uniform4uiv(_location: i32, _count: i32, _value: *const u8) {}
pub unsafe fn gl_uniform_matrix2fv(_location: i32, _count: i32, _transpose: u32, _value: *const u8) {}
pub unsafe fn gl_uniform_matrix3fv(_location: i32, _count: i32, _transpose: u32, _value: *const u8) {}
pub unsafe fn gl_uniform_matrix4fv(_location: i32, _count: i32, _transpose: u32, _value: *const u8) {}
pub unsafe fn gl_uniform_matrix2x3fv(_location: i32, _count: i32, _transpose: u32, _value: *const u8) {}
pub unsafe fn gl_uniform_matrix3x2fv(_location: i32, _count: i32, _transpose: u32, _value: *const u8) {}
pub unsafe fn gl_uniform_matrix2x4fv(_location: i32, _count: i32, _transpose: u32, _value: *const u8) {}
pub unsafe fn gl_uniform_matrix4x2fv(_location: i32, _count: i32, _transpose: u32, _value: *const u8) {}
pub unsafe fn gl_uniform_matrix3x4fv(_location: i32, _count: i32, _transpose: u32, _value: *const u8) {}
pub unsafe fn gl_uniform_matrix4x3fv(_location: i32, _count: i32, _transpose: u32, _value: *const u8) {}
pub unsafe fn gl_create_query() -> u32 { gl_name() }
pub unsafe fn gl_delete_query(_query: u32) {}
pub unsafe fn gl_is_query(_query: u32) -> u32 { 0 }
pub unsafe fn gl_begin_query(_target: u32, _query: u32) {}
pub unsafe fn gl_end_query(_target: u32) {}
pub unsafe fn gl_get_queryiv(_target: u32, _pname: u32) -> u32 { 0 }
pub unsafe fn gl_get_query_objectuiv(_query: u32, _pname: u32) -> u32 { 0 }
pub unsafe fn gl_fence_sync(_condition: u32, _flags: u32) -> u32 { gl_name() }
pub unsafe fn gl_is_sync(_sync: u32) -> u32 { 0 }
pub unsafe fn gl_delete_sync(_sync: u32) {}
pub unsafe fn gl_client_wait_sync(_sync: u32, _flags: u32, _timeout: u64) -> u32 { 0 }
pub unsafe fn gl_wait_sync(_sync: u32, _flags: u32, _timeout: u64) {}
pub unsafe fn gl_get_synciv(_sync: u32, _pname: u32) -> i32 { 0 }
pub unsafe fn gl_create_transform_feedback() -> u32 { gl_name() }
pub unsafe fn gl_delete_transform_feedback(_tf: u32) {}
pub unsafe fn gl_is_transform_feedback(_tf: u32) -> u32 { 0 }
pub unsafe fn gl_bind_transform_feedback(_target: u32, _tf: u32) {}
pub unsafe fn gl_begin_transform_feedback(_primitive_mode: u32) {}
pub unsafe fn gl_end_transform_feedback() {}
pub unsafe fn gl_pause_transform_feedback() {}
pub unsafe fn gl_resume_transform_feedback() {}
