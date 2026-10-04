extends Node2D
# The level: platforms from a table, coins on them, a score.

const PLATFORMS := [
	Rect2(-200, 320, 2400, 400), Rect2(160, 250, 140, 16), Rect2(360, 200, 120, 16),
	Rect2(540, 150, 140, 16), Rect2(760, 230, 160, 16), Rect2(980, 180, 120, 16),
	Rect2(1160, 120, 160, 16), Rect2(1400, 220, 200, 16), Rect2(1680, 160, 140, 16),
]
var score := 0

func _ready() -> void:
	seed(1) # the same level on every runner
	var coin_scene: PackedScene = load("res://coin.tscn")
	for r in PLATFORMS:
		var body := StaticBody2D.new()
		var shape := CollisionShape2D.new()
		var rect := RectangleShape2D.new()
		rect.size = r.size
		shape.shape = rect
		body.position = r.get_center()
		body.add_child(shape)
		var look := ColorRect.new()
		look.color = Color(0.35, 0.55, 0.3) if r.size.y > 20 else Color(0.55, 0.4, 0.25)
		look.size = r.size
		look.position = -r.size / 2
		body.add_child(look)
		$Level.add_child(body)
		if r.size.y < 20 or r.position.x < 0:
			var coin: Area2D = coin_scene.instantiate()
			# small platforms carry one coin; the ground a row of them
			var xs := [r.get_center().x] if r.size.y < 20 else [260.0, 640.0, 900.0, 1300.0]
			for x in xs.slice(1):
				var c: Area2D = coin_scene.instantiate()
				c.position = Vector2(x, r.position.y - 24)
				c.collected.connect(_on_coin)
				$Level.add_child(c)
			coin.position = Vector2(xs[0], r.position.y - 24)
			coin.collected.connect(_on_coin)
			$Level.add_child(coin)
	print("platformer: level with %d platforms" % PLATFORMS.size())

func _on_coin() -> void:
	score += 1
	$UI/Score.text = "Coins: %d" % score
	print("platformer: coin %d at frame %d" % [score, Engine.get_process_frames()])

func _draw() -> void:
	# hills behind the level
	for i in 24:
		var x := i * 120.0 - 200
		draw_circle(Vector2(x, 340), 90 + (i * 37) % 50, Color(0.2, 0.3, 0.45))
