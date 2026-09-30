//! Smallest gasm:gfx example: one vertex-colored triangle, redrawn every frame.

use gasm::gfx::{self, Buffer, Pipeline};

const SHADER: &str = r#"
struct VO { @builtin(position) pos: vec4<f32>, @location(0) color: vec3<f32> };
@vertex fn vs(@location(0) p: vec2<f32>, @location(1) c: vec3<f32>) -> VO {
  return VO(vec4<f32>(p, 0.0, 1.0), c);
}
@fragment fn fs(i: VO) -> @location(0) vec4<f32> { return vec4<f32>(i.color, 1.0); }
"#;

struct Triangle { pipeline: Pipeline, vertices: Buffer }

impl gasm::Game for Triangle {
    fn init() -> Result<Self, String> {
        let shader = gfx::create_shader(SHADER);
        let pipeline = gfx::create_pipeline(&format!(
            r#"{{"vertex":{{"module":{s},"entryPoint":"vs","buffers":[{{"arrayStride":20,"attributes":[
                {{"format":"float32x2","offset":0,"shaderLocation":0}},
                {{"format":"float32x3","offset":8,"shaderLocation":1}}]}}]}},
              "fragment":{{"module":{s},"entryPoint":"fs","targets":[{{"format":"surface"}}]}},
              "primitive":{{"topology":"triangle-list"}}}}"#,
            s = shader.0
        ));
        // x, y, r, g, b
        let data: [f32; 15] = [0.0, 0.6, 1.0, 0.2, 0.2, -0.6, -0.5, 0.2, 1.0, 0.2, 0.6, -0.5, 0.2, 0.4, 1.0];
        let vertices = gfx::create_buffer(std::mem::size_of_val(&data) as u32, gfx::VERTEX);
        gfx::write_buffer(vertices, 0, &data);
        Ok(Triangle { pipeline, vertices })
    }

    fn frame(&mut self) {
        if gfx::begin_frame([0.05, 0.05, 0.1, 1.0]) {
            gfx::set_pipeline(self.pipeline);
            gfx::set_vertex_buffer(0, self.vertices, 0);
            gfx::draw(3, 1, 0, 0);
        }
        gfx::end_frame();
    }
}

gasm::game!(Triangle);
