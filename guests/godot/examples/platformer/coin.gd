extends Area2D
# A coin: spins, and tells the level when the player touches it.

signal collected

var t := randf() * TAU

func _ready() -> void:
	body_entered.connect(_on_body_entered)

func _process(delta: float) -> void:
	t += delta * 4.0
	$Shape.scale.x = abs(cos(t))
	position.y += sin(t) * 0.3

func _on_body_entered(body: Node) -> void:
	if body.name == "Player":
		collected.emit()
		queue_free()
