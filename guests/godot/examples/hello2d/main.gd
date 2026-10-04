extends Node2D
# Godot on gasm: GDScript, drawing, input and the frame clock. The player square
# moves with the arrow keys or a gamepad; the sprite spins; the label counts frames.

var frames := 0
var player := Vector2(320, 260)

func _ready() -> void:
	print("hello2d: ready, viewport ", get_viewport_rect().size)

func _process(delta: float) -> void:
	frames += 1
	$Sprite.rotation += delta * 2.0
	var dir := Input.get_vector("ui_left", "ui_right", "ui_up", "ui_down")
	player += dir * 160.0 * delta
	player = player.clamp(Vector2(16, 16), Vector2(624, 344))
	$Label.text = "Godot %s on gasm - frame %d" % [Engine.get_version_info().string, frames]
	queue_redraw()
	if frames == 120:
		print("hello2d: 120 frames, player at ", player.round())

func _draw() -> void:
	draw_rect(Rect2(player - Vector2(16, 16), Vector2(32, 32)), Color(1.0, 0.6, 0.2))
	for i in 8:
		var a := frames * 0.02 + i * TAU / 8.0
		draw_circle(Vector2(320, 180) + Vector2(cos(a), sin(a)) * 120.0, 6.0, Color.from_hsv(i / 8.0, 0.7, 1.0))
