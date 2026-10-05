/**************************************************************************/
/*  websocket_peer_gasm.h: WebSocketPeer on gasm:net (gasm platform, MIT)   */
/**************************************************************************/

#pragma once

#include "modules/modules_enabled.gen.h" // MODULE_WEBSOCKET_ENABLED

#ifdef MODULE_WEBSOCKET_ENABLED

#include "modules/websocket/packet_buffer.h"
#include "modules/websocket/websocket_peer.h"

// Godot's WebSocketPeer (and so WebSocketMultiplayerPeer as a client) on gasm:net:
// the runner holds the connection (natively with --allow-net, in browsers under the
// page's rules). gasm:net messages are binary, so text frames arrive as bytes
// (was_string_packet() is false) and send_text() sends the text's bytes. Client only:
// no accept_stream (a server is the relay, or Godot running outside gasm).
class GasmWSPeer : public WebSocketPeer {
	int conn = -1;
	State ready_state = STATE_CLOSED;
	Vector<uint8_t> packet_buffer;
	PacketBuffer<uint8_t> in_buffer;
	Vector<uint8_t> scratch;
	int close_code = -1;
	String requested_url;

	static WebSocketPeer *_create(bool p_notify_postinitialize);
	void _clear();

public:
	static void initialize() { WebSocketPeer::_create = GasmWSPeer::_create; }

	// PacketPeer
	int get_available_packet_count() const override;
	Error get_packet(const uint8_t **r_buffer, int &r_buffer_size) override;
	Error put_packet(const uint8_t *p_buffer, int p_buffer_size) override;
	int get_max_packet_size() const override { return packet_buffer.size(); }

	// WebSocketPeer
	Error send(const uint8_t *p_buffer, int p_buffer_size, WriteMode p_mode) override;
	Error connect_to_url(const String &p_url, const Ref<TLSOptions> &p_tls_client_options) override;
	Error accept_stream(const Ref<StreamPeer> &p_stream) override;
	void close(int p_code = 1000, const String &p_reason = "") override;
	void poll() override;

	State get_ready_state() const override { return ready_state; }
	int get_close_code() const override { return close_code; }
	String get_close_reason() const override { return String(); }
	int get_current_outbound_buffered_amount() const override { return 0; }

	IPAddress get_connected_host() const override;
	uint16_t get_connected_port() const override;
	String get_selected_protocol() const override { return String(); }
	String get_requested_url() const override { return requested_url; }

	bool was_string_packet() const override { return false; }
	void set_no_delay(bool p_enabled) override {}

	~GasmWSPeer();
};

#endif // MODULE_WEBSOCKET_ENABLED
