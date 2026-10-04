extends Node3D
# Orbit with the arrow keys; Space drops a burst of bodies. Every second a box or a
# ball falls on the stage anyway (Godot Physics 3D).

var yaw := 0.6
var pitch := -0.35
var dist := 9.0
var t := 0.0
var spawned := 0

func _ready() -> void:
	seed(7)
	for i in 6:
		spawn()
	print("scene3d: ready")

func spawn() -> void:
	var body := RigidBody3D.new()
	var mesh := MeshInstance3D.new()
	var shape := CollisionShape3D.new()
	var mat := StandardMaterial3D.new()
	mat.albedo_color = Color.from_hsv(randf(), 0.6, 0.95)
	mat.roughness = randf_range(0.2, 0.9)
	mat.metallic = 0.3 if spawned % 3 == 0 else 0.0
	if spawned % 2 == 0:
		var b := BoxMesh.new()
		b.size = Vector3.ONE * 0.8
		mesh.mesh = b
		var s := BoxShape3D.new()
		s.size = b.size
		shape.shape = s
	else:
		var sp := SphereMesh.new()
		sp.radius = 0.45
		sp.height = 0.9
		mesh.mesh = sp
		var s := SphereShape3D.new()
		s.radius = 0.45
		shape.shape = s
	mesh.material_override = mat
	body.add_child(mesh)
	body.add_child(shape)
	body.position = Vector3(randf_range(-2.5, 2.5), 6.0 + randf() * 3.0, randf_range(-2.5, 2.5))
	body.rotation = Vector3(randf(), randf(), randf())
	$Bodies.add_child(body)
	spawned += 1
	if $Bodies.get_child_count() > 40:
		$Bodies.get_child(0).queue_free()

func _process(delta: float) -> void:
	t += delta
	yaw += Input.get_axis("ui_left", "ui_right") * delta * 1.5
	pitch = clamp(pitch + Input.get_axis("ui_down", "ui_up") * delta, -1.2, -0.1)
	var target := Vector3(0, 1, 0)
	$Camera.position = target + Vector3(cos(pitch) * sin(yaw), -sin(pitch), cos(pitch) * cos(yaw)) * dist
	$Camera.look_at(target)
	$Torus.rotation = Vector3(t * 0.7, t, 0)
	if Input.is_action_just_pressed("ui_accept"):
		for i in 8:
			spawn()
	if int(t) != int(t - delta):
		spawn()
	if Engine.get_process_frames() % 120 == 0:
		print("scene3d: frame %d, %d bodies" % [Engine.get_process_frames(), $Bodies.get_child_count()])
