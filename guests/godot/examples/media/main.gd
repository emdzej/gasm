extends Node2D
# Godot on gasm: an audio stream (internet radio) through the Gasm singleton's media_*
# calls (gasm:media). Plays the URL in the launch parameter `url` and logs every change
# of its state and title ("media: state playing", "media: title ...").
#   gasm-run godot.wasm --asset game.pck=media.pck --allow-media --param url=https://...

var gasm: Object
var stream := -1
var state := -1
var version := -1
const NAMES := ["connecting", "playing", "paused", "failed", "ended"]

func _ready() -> void:
	if not Engine.has_singleton("Gasm") or not Engine.get_singleton("Gasm").can_play_media():
		print("media: this runner can't play streams")
		return
	gasm = Engine.get_singleton("Gasm")
	var url: String = gasm.get_param("url")
	stream = gasm.media_open(url)
	if stream < 0:
		print("media: refused: ", gasm.media_refusal())
		return
	print("media: opened ", url)
	gasm.media_set_volume(stream, 0.8)
	gasm.media_play(stream)

func _process(_delta: float) -> void:
	if stream < 0:
		return
	var s: int = gasm.media_state(stream)
	if s != state:
		state = s
		var line: String = "media: state " + NAMES[s]
		if s == gasm.MEDIA_FAILED:
			line += ": " + gasm.media_error(stream)
		if s == gasm.MEDIA_PLAYING:
			line += " " + str(gasm.media_caps(stream))
		print(line)
	var v: int = gasm.media_metadata_version(stream)
	if v != version:
		version = v
		var m: Dictionary = gasm.media_metadata(stream)
		print("media: title ", m.get("title"), " (", m.get("name"), ", ", m.get("codec"), ")")
		$Label.text = "%s\n%s" % [m.get("name", ""), m.get("title", "")]
