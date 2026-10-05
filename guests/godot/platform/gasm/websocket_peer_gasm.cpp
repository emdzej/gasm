/**************************************************************************/
/*  websocket_peer_gasm.cpp: WebSocketPeer on gasm:net (gasm platform, MIT) */
/**************************************************************************/

#include "websocket_peer_gasm.h"

#ifdef MODULE_WEBSOCKET_ENABLED

#include "core/io/ip_address.h"
#include "core/object/class_db.h"

#include "gasm.h"

WebSocketPeer *GasmWSPeer::_create(bool p_notify_postinitialize) {
	return static_cast<WebSocketPeer *>(ClassDB::creator<GasmWSPeer>(p_notify_postinitialize));
}

Error GasmWSPeer::connect_to_url(const String &p_url, const Ref<TLSOptions> &p_tls_options) {
	ERR_FAIL_COND_V(p_url.is_empty(), ERR_INVALID_PARAMETER);
	ERR_FAIL_COND_V(p_tls_options.is_valid() && p_tls_options->is_server(), ERR_INVALID_PARAMETER);
	ERR_FAIL_COND_V(ready_state != STATE_CLOSED && ready_state != STATE_CLOSING, ERR_ALREADY_IN_USE);
	ERR_FAIL_COND_V_MSG(gasm_has_str("gasm:net") != 1, ERR_UNAVAILABLE, "This gasm runner has no gasm:net (WebSocket connections).");
	_clear();

	String scheme, host, path, fragment;
	int port = 0;
	Error err = p_url.parse_url(scheme, host, port, path, fragment);
	ERR_FAIL_COND_V_MSG(err != OK, err, "Invalid URL: " + p_url);
	if (scheme.is_empty()) {
		scheme = "ws://";
	}
	ERR_FAIL_COND_V_MSG(scheme != "ws://" && scheme != "wss://", ERR_INVALID_PARAMETER, vformat("Invalid protocol: \"%s\" (must be either \"ws://\" or \"wss://\").", scheme));
	if (handshake_headers.size() || supported_protocols.size()) {
		WARN_PRINT_ONCE("Custom headers and subprotocols are not supported on gasm.");
	}
	if (p_tls_options.is_valid()) {
		WARN_PRINT_ONCE("TLS options are ignored on gasm: the runner checks certificates against the system's.");
	}
	requested_url = scheme + host;
	if (port && ((scheme == "ws://" && port != 80) || (scheme == "wss://" && port != 443))) {
		requested_url += ":" + String::num_int64(port);
	}
	requested_url += path.is_empty() ? String("/") : path;

	CharString url = requested_url.utf8();
	conn = gasm_net_open(url.get_data(), url.length());
	if (conn <= 0) {
		conn = -1;
		return FAILED; // the runner logs why (denied, invalid, too many connections)
	}
	in_buffer.resize(Math::nearest_shift((uint32_t)inbound_buffer_size), max_queued_packets);
	packet_buffer.resize(inbound_buffer_size);
	ready_state = STATE_CONNECTING;
	return OK;
}

Error GasmWSPeer::accept_stream(const Ref<StreamPeer> &p_stream) {
	WARN_PRINT_ONCE("Acting as a WebSocket server is not supported on gasm (use gasm-relay, or Godot outside gasm).");
	return ERR_UNAVAILABLE;
}

Error GasmWSPeer::send(const uint8_t *p_buffer, int p_buffer_size, WriteMode p_mode) {
	ERR_FAIL_COND_V(ready_state != STATE_OPEN, FAILED);
	ERR_FAIL_COND_V_MSG(p_buffer_size <= 0, ERR_INVALID_PARAMETER, "gasm:net messages can't be empty.");
	return gasm_net_send(conn, p_buffer, p_buffer_size) == 0 ? OK : ERR_OUT_OF_MEMORY;
}

Error GasmWSPeer::put_packet(const uint8_t *p_buffer, int p_buffer_size) {
	return send(p_buffer, p_buffer_size, WRITE_MODE_BINARY);
}

Error GasmWSPeer::get_packet(const uint8_t **r_buffer, int &r_buffer_size) {
	if (in_buffer.packets_left() == 0) {
		return ERR_UNAVAILABLE;
	}
	uint8_t info = 0;
	int read = 0;
	Error err = in_buffer.read_packet(packet_buffer.ptrw(), packet_buffer.size(), &info, read);
	ERR_FAIL_COND_V(err != OK, err);
	*r_buffer = packet_buffer.ptr();
	r_buffer_size = read;
	return OK;
}

int GasmWSPeer::get_available_packet_count() const {
	return in_buffer.packets_left();
}

void GasmWSPeer::poll() {
	if (conn <= 0) {
		return;
	}
	if (ready_state == STATE_CONNECTING && gasm_net_state(conn) == GASM_NET_OPEN) {
		ready_state = STATE_OPEN;
	}
	// move arrived messages into Godot's queue while they fit (the rest wait in the runner)
	int n = 0;
	while (in_buffer.packets_space_left() > 0) {
		n = gasm_net_recv(conn, nullptr, 0);
		if (n <= 0 || n > in_buffer.payload_space_left() || n > packet_buffer.size()) {
			if (n > packet_buffer.size()) {
				ERR_PRINT_ONCE(vformat("A message of %d bytes is larger than inbound_buffer_size (%d): raise it.", n, packet_buffer.size()));
			}
			break;
		}
		scratch.resize(n);
		gasm_net_recv(conn, scratch.ptrw(), n);
		uint8_t is_string = 0;
		in_buffer.write_packet(scratch.ptr(), n, &is_string);
	}
	// -1: closed (or failed) and every message received
	if (n == -1) {
		close_code = gasm_net_state(conn) == GASM_NET_CLOSED ? 1000 : -1;
		gasm_net_close(conn);
		conn = -1;
		ready_state = STATE_CLOSED;
	}
}

void GasmWSPeer::close(int p_code, const String &p_reason) {
	if (conn > 0) {
		gasm_net_close(conn); // flushes what was sent, then closes
		conn = -1;
		close_code = p_code < 0 ? -1 : p_code;
	}
	ready_state = STATE_CLOSED;
	in_buffer.clear();
	packet_buffer.clear();
}

void GasmWSPeer::_clear() {
	if (conn > 0) {
		gasm_net_close(conn);
	}
	conn = -1;
	ready_state = STATE_CLOSED;
	close_code = -1;
	requested_url.clear();
	in_buffer.clear();
	packet_buffer.clear();
}

IPAddress GasmWSPeer::get_connected_host() const {
	ERR_FAIL_V_MSG(IPAddress(), "Not available on gasm (the runner holds the connection).");
}

uint16_t GasmWSPeer::get_connected_port() const {
	ERR_FAIL_V_MSG(0, "Not available on gasm (the runner holds the connection).");
}

GasmWSPeer::~GasmWSPeer() {
	_clear();
}

#endif // MODULE_WEBSOCKET_ENABLED
