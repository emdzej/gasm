extends CharacterBody2D
# Run with the arrows (or a stick), jump with Space / Up / pad A.

const SPEED := 180.0
const JUMP := -380.0
var gravity := 900.0

func _physics_process(delta: float) -> void:
	if not is_on_floor():
		velocity.y += gravity * delta
	if Input.is_action_just_pressed("jump") and is_on_floor():
		velocity.y = JUMP
	velocity.x = Input.get_axis("ui_left", "ui_right") * SPEED
	move_and_slide()
	# squash a little when landing, lean when running
	$Body.scale = $Body.scale.lerp(Vector2.ONE, 0.2)
	$Body.rotation = lerp($Body.rotation, velocity.x / SPEED * 0.15, 0.2)
	if position.y > 600:
		position = Vector2(80, 200)
		velocity = Vector2.ZERO
