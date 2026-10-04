extends Control
# HTTPRequest works unchanged on gasm: the runner makes the requests (gasm:fetch).
# Natively they need --allow-net (or --allow-net=<host>); headless runs can replay
# recorded responses (--fetch-replay). The server: `node scripts/fetch-server.mjs 8787`
# from the gasm repository, or another base URL: --param "args=-- --base=<url>".

var base := "http://127.0.0.1:8787/api"
var lines := {}
var order := ["hello", "echo", "missing", "big"]

func _ready() -> void:
	for a in OS.get_cmdline_user_args():
		if a.begins_with("--base="):
			base = a.substr(7)
	fetch("hello", base + "/hello")
	fetch("echo", base + "/echo", ["Content-Type: text/plain", "X-Test: 7"], HTTPClient.METHOD_POST, "ping from Godot")
	fetch("missing", base + "/missing")
	fetch("big", base + "/big")

func fetch(name: String, url: String, headers := PackedStringArray(), method := HTTPClient.METHOD_GET, body := "") -> void:
	var req := HTTPRequest.new()
	add_child(req)
	req.request_completed.connect(done.bind(name, req))
	var err := req.request(url, headers, method, body)
	if err != OK:
		lines[name] = "%s: not sent (%s)" % [name, error_string(err)]
		show_results()

func done(result: int, code: int, headers: PackedStringArray, body: PackedByteArray, name: String, req: HTTPRequest) -> void:
	req.queue_free()
	var text := body.get_string_from_utf8() if body.size() <= 64 else "%d bytes, fnv %08x" % [body.size(), fnv(body)]
	var ctype := ""
	for h in headers:
		if h.to_lower().begins_with("content-type:"):
			ctype = h.substr(13).strip_edges()
	lines[name] = "%s: result %d, HTTP %d, %s, %s" % [name, result, code, ctype, text]
	print("http: ", lines[name])
	show_results()
	if lines.size() == order.size():
		print("http: all done")

func show_results() -> void:
	var out := PackedStringArray()
	for n in order:
		out.append(lines.get(n, n + ": ..."))
	$Results.text = "\n".join(out)

func fnv(bytes: PackedByteArray) -> int:
	var h := 0x811c9dc5
	for b in bytes:
		h = ((h ^ b) * 0x01000193) & 0xffffffff
	return h
