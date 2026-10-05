extends Node2D
# Roam together: every player is a square moved with the arrow keys, and positions go
# through a gasm-relay room 15 times a second. Godot's WebSocketPeer runs on gasm:net
# (natively with --allow-net). The relay URL is the "relay" launch parameter, read
# through the Gasm singleton (with "room" appended if given, as the web player passes it):
#   gasm-relay 127.0.0.1:9000 &
#   gasm-run build/godot.wasm --asset game.pck=build/godot/net.pck --allow-net --param relay=ws://127.0.0.1:9000/roam
#
# gasm-relay's protocol (byte 0 = type): WELCOME [1][you][peers], JOIN [2][index],
# LEAVE [3][index], FULL [4], DATA [0x10][from][payload] (we send [0x10][payload]).

const COLORS := [Color("ff6b6b"), Color("4ecdc4"), Color("ffe66d"), Color("a78bfa"), Color("6bcB77"), Color("f78fb3")]
var ws := WebSocketPeer.new()
var me := -1
var pos := Vector2(320, 200)
var others := {}   # relay index -> position
var since_send := 0.0
var closed_logged := false
# tests: quit this many frames after first seeing another player (param quit_after; 0: never)
var quit_after := 0
var seen_at := -1

func _ready() -> void:
	var url := "ws://127.0.0.1:9000/godot-net"
	if Engine.has_singleton("Gasm"):
		var gasm = Engine.get_singleton("Gasm")
		if gasm.get_param("relay") != "":
			url = gasm.get_param("relay")
		if gasm.get_param("room") != "":
			url = url.trim_suffix("/") + "/" + gasm.get_param("room")
		quit_after = int(gasm.get_param("quit_after"))
	var err := ws.connect_to_url(url)
	print("net: connecting to %s: %s" % [url, error_string(err)])
	if err != OK:
		$Status.text = "can't connect to %s (natively: run with --allow-net)" % url

func _process(delta: float) -> void:
	if seen_at >= 0 and quit_after > 0 and Engine.get_process_frames() - seen_at >= quit_after:
		get_tree().quit()
	pos += Input.get_vector("ui_left", "ui_right", "ui_up", "ui_down") * 220.0 * delta
	pos = pos.clamp(Vector2(10, 50), Vector2(630, 350))
	ws.poll()
	match ws.get_ready_state():
		WebSocketPeer.STATE_OPEN:
			while ws.get_available_packet_count() > 0:
				_message(ws.get_packet())
			since_send += delta
			if me >= 0 and since_send >= 1.0 / 15.0:
				since_send = 0.0
				var b := PackedByteArray()
				b.resize(9)
				b[0] = 0x10
				b.encode_float(1, pos.x)
				b.encode_float(5, pos.y)
				ws.send(b)
		WebSocketPeer.STATE_CLOSED:
			if not closed_logged:
				closed_logged = true
				print("net: closed (code %d)" % ws.get_close_code())
				if me >= 0:
					$Status.text = "disconnected"
	queue_redraw()

func _message(m: PackedByteArray) -> void:
	if m.is_empty():
		return
	match m[0]:
		1:
			me = m[1]
			print("net: joined as %d, %d in the room" % [me, m[2]])
			$Status.text = "you are player %d - arrow keys to move" % (me + 1)
		2:
			print("net: player %d joined" % (m[1] + 1))
		3:
			others.erase(m[1])
			print("net: player %d left" % (m[1] + 1))
		4:
			$Status.text = "the room is full"
		0x10:
			if m.size() >= 10:
				if not others.has(m[1]):
					print("net: first position from player %d" % (m[1] + 1))
					if seen_at < 0:
						seen_at = Engine.get_process_frames()
				others[m[1]] = Vector2(m.decode_float(2), m.decode_float(6))

func _draw() -> void:
	draw_rect(Rect2(0, 0, 640, 360), Color(0.1, 0.12, 0.17))
	for i in others:
		draw_rect(Rect2(others[i] - Vector2(10, 10), Vector2(20, 20)), COLORS[i % COLORS.size()])
	if me >= 0:
		draw_rect(Rect2(pos - Vector2(12, 12), Vector2(24, 24)), COLORS[me % COLORS.size()])
		draw_rect(Rect2(pos - Vector2(12, 12), Vector2(24, 24)), Color.WHITE, false, 2.0)
