// A custom shader with a varying: vertex() hands the world position to fragment().
// (Godot sizes its varying budget from the GL limits: this checks runners report them.)
shader_type spatial;

varying vec3 world_pos;

void vertex() {
	world_pos = (MODEL_MATRIX * vec4(VERTEX, 1.0)).xyz;
}

void fragment() {
	float s = sin(world_pos.y * 6.0 - TIME * 2.0) * 0.5 + 0.5;
	ALBEDO = mix(vec3(0.15, 0.45, 0.9), vec3(0.95, 0.35, 0.6), s);
	ROUGHNESS = 0.4;
}
