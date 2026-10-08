extends Node2D
# Godot's high-level multiplayer in a gasm-relay room, no Godot server: the room's first
# player is the server (peer 1). Each player is a square; its position goes to everyone
# as an RPC (`moved.rpc(pos)`), through the server, 15 times a second.
#   gasm-relay 127.0.0.1:9000 &
#   gasm-run build/godot-2d.wasm --asset game.pck=build/godot/relaymp.pck --allow-net --param relay=ws://127.0.0.1:9000/room
# (two or more of them). "quit_after": quit that many frames after seeing another player (tests).

const COLORS := [Color("ff6b6b"), Color("4ecdc4"), Color("ffe66d"), Color("a78bfa"), Color("6bcB77"), Color("f78fb3")]
var pos := Vector2(320, 200)
var others := {}   # peer id -> position
var since_send := 0.0
var quit_after := 0
var seen_at := -1

func _ready() -> void:
	var url := "ws://127.0.0.1:9000/godot-relaymp"
	if not Engine.has_singleton("Gasm"):
		$Status.text = "needs gasm (Gasm singleton)"
		return
	var gasm = Engine.get_singleton("Gasm")
	if gasm.get_param("relay") != "":
		url = gasm.get_param("relay")
	if gasm.get_param("room") != "":
		url = url.trim_suffix("/") + "/" + gasm.get_param("room")
	quit_after = int(gasm.get_param("quit_after"))
	var peer: MultiplayerPeer = gasm.create_relay_peer(url)
	if peer == null:
		print("mp: can't connect to %s" % url)
		$Status.text = "can't connect to %s (natively: run with --allow-net)" % url
		return
	multiplayer.multiplayer_peer = peer
	multiplayer.peer_connected.connect(func(id): print("mp: peer %d connected" % id))
	multiplayer.peer_disconnected.connect(func(id): others.erase(id); print("mp: peer %d left" % id))
	multiplayer.connected_to_server.connect(func(): print("mp: connected as peer %d" % multiplayer.get_unique_id()))
	multiplayer.server_disconnected.connect(func(): $Status.text = "the server left")
	print("mp: joining %s" % url)

func _process(delta: float) -> void:
	if seen_at >= 0 and quit_after > 0 and Engine.get_process_frames() - seen_at >= quit_after:
		get_tree().quit()
	pos += Input.get_vector("ui_left", "ui_right", "ui_up", "ui_down") * 220.0 * delta
	pos = pos.clamp(Vector2(10, 50), Vector2(630, 350))
	var peer := multiplayer.multiplayer_peer
	if peer and peer.get_connection_status() == MultiplayerPeer.CONNECTION_CONNECTED:
		$Status.text = "peer %d%s - arrow keys to move" % [multiplayer.get_unique_id(), " (server)" if multiplayer.is_server() else ""]
		since_send += delta
		if since_send >= 1.0 / 15.0:
			since_send = 0.0
			moved.rpc(pos)
	queue_redraw()

@rpc("any_peer", "unreliable_ordered")
func moved(p: Vector2) -> void:
	var id := multiplayer.get_remote_sender_id()
	if not others.has(id):
		print("mp: first position from peer %d" % id)
		if seen_at < 0:
			seen_at = Engine.get_process_frames()
	others[id] = p

func _draw() -> void:
	draw_rect(Rect2(0, 0, 640, 360), Color(0.1, 0.12, 0.17))
	for id in others:
		draw_rect(Rect2(others[id] - Vector2(10, 10), Vector2(20, 20)), COLORS[id % COLORS.size()])
	var me := multiplayer.get_unique_id() if multiplayer.multiplayer_peer else 0
	draw_rect(Rect2(pos - Vector2(12, 12), Vector2(24, 24)), COLORS[me % COLORS.size()])
	draw_rect(Rect2(pos - Vector2(12, 12), Vector2(24, 24)), Color.WHITE, false, 2.0)
