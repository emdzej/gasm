extends Node2D
# Added by the mod: a square circling the bottom right corner.
var t := 0.0
func _process(delta: float) -> void:
	t += delta
	position = Vector2(520, 270) + Vector2(cos(t * 2.0), sin(t * 2.0)) * 40.0
	rotation = t * 3.0
	queue_redraw()
func _draw() -> void:
	draw_rect(Rect2(-14, -14, 28, 28), Color(0.95, 0.55, 0.2))
