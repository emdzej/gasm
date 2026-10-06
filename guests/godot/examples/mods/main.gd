extends Node2D
# Mods as resource packs: gasm-run --mods <dir> mounts the folder's *.pck / *.zip and the
# Gasm singleton lists them in load order. Each loaded pack overrides the game's files
# by path (content/settings.cfg here) and can add new ones (content/extra.tscn).
#   gasm-run build/godot-2d.wasm --asset game.pck=build/godot/mods.pck --mods <folder with modpack.pck>

var greeting := ""
var color := Color.WHITE
var lines: PackedStringArray = []
var t := 0.0

func _ready() -> void:
	if Engine.has_singleton("Gasm"):
		var gasm = Engine.get_singleton("Gasm")
		for path in gasm.get_mods():
			var ok := ProjectSettings.load_resource_pack(path)
			lines.append("%s %s" % ["loaded" if ok else "failed", path.get_file()])
			print("mods: %s %s" % ["loaded" if ok else "failed", path.get_file()])
		var refused: Dictionary = gasm.get_refused_mods()
		for name in refused:
			lines.append("refused %s (%s)" % [name, refused[name]])
			print("mods: refused %s (%s)" % [name, refused[name]])
	if lines.is_empty():
		lines.append("no mods (run with --mods <folder>)")
	var cfg := ConfigFile.new()
	cfg.load("res://content/settings.cfg")
	greeting = cfg.get_value("look", "greeting", "?")
	color = cfg.get_value("look", "color", Color.WHITE)
	print("mods: %s" % greeting)
	if ResourceLoader.exists("res://content/extra.tscn"):
		add_child(load("res://content/extra.tscn").instantiate())
		print("mods: the mod added content/extra.tscn")

func _process(delta: float) -> void:
	t += delta
	queue_redraw()

func _draw() -> void:
	draw_rect(Rect2(0, 0, 640, 360), Color(0.1, 0.12, 0.17))
	draw_rect(Rect2(40, 40, 560, 80), color)
	var font := ThemeDB.fallback_font
	draw_string(font, Vector2(56, 90), greeting, HORIZONTAL_ALIGNMENT_LEFT, -1, 24, Color(0.08, 0.09, 0.12))
	for i in lines.size():
		draw_string(font, Vector2(40, 170 + i * 24), lines[i], HORIZONTAL_ALIGNMENT_LEFT, -1, 18, Color(0.85, 0.87, 0.9))
