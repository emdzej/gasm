/**************************************************************************/
/*  relay_peer.cpp: Godot's MultiplayerPeer through gasm-relay (gasm, MIT) */
/**************************************************************************/

#include "relay_peer.h"

#include "gasm.h"

// gasm-relay's protocol (runners/native/relay): byte 0 is the type
enum : uint8_t {
	RELAY_WELCOME = 1, // [you][peers in room]
	RELAY_JOIN = 2, // [index]
	RELAY_LEAVE = 3, // [index]
	RELAY_FULL = 4,
	RELAY_DATA = 0x10, // from the relay: [from][payload]; to it: [payload] (to every other peer)
};
enum : uint8_t {
	KIND_DATA = 0,
	KIND_HERE = 1,
};
static const int HEADER = 7; // kind, target (4), channel, mode

Error GasmRelayPeer::connect_to_url(const String &p_url) {
	ERR_FAIL_COND_V_MSG(gasm_has_str("gasm:net") != 1, ERR_UNAVAILABLE, "This gasm runner has no gasm:net (WebSocket connections).");
	ERR_FAIL_COND_V(status != CONNECTION_DISCONNECTED, ERR_ALREADY_IN_USE);
	CharString url = p_url.utf8();
	conn = gasm_net_open(url.get_data(), url.length());
	if (conn <= 0) {
		conn = -1;
		return FAILED; // the runner logs why (denied, invalid, too many connections)
	}
	status = CONNECTION_CONNECTING;
	return OK;
}

void GasmRelayPeer::send_raw(uint8_t p_kind, int p_target, const uint8_t *p_data, int p_size) {
	if (conn <= 0 || index < 0) {
		return;
	}
	scratch.resize(1 + HEADER + p_size);
	uint8_t *w = scratch.ptrw();
	w[0] = RELAY_DATA;
	w[1] = p_kind;
	uint32_t t = (uint32_t)p_target;
	for (int i = 0; i < 4; i++) {
		w[2 + i] = (t >> (8 * i)) & 0xff;
	}
	w[6] = (uint8_t)get_transfer_channel();
	w[7] = (uint8_t)get_transfer_mode();
	if (p_size > 0) {
		memcpy(w + 1 + HEADER, p_data, p_size);
	}
	if (gasm_net_send(conn, scratch.ptr(), scratch.size()) != 0) {
		WARN_PRINT_ONCE("gasm-relay: the send queue is full; a message was dropped.");
	}
}

Error GasmRelayPeer::put_packet(const uint8_t *p_buffer, int p_buffer_size) {
	ERR_FAIL_COND_V(status != CONNECTION_CONNECTED, ERR_UNCONFIGURED);
	send_raw(KIND_DATA, target, p_buffer, p_buffer_size);
	return OK;
}

Error GasmRelayPeer::get_packet(const uint8_t **r_buffer, int &r_buffer_size) {
	ERR_FAIL_COND_V(incoming.is_empty(), ERR_UNAVAILABLE);
	current = incoming.front()->get();
	incoming.pop_front();
	*r_buffer = current.data.ptr();
	r_buffer_size = current.data.size();
	return OK;
}

int GasmRelayPeer::get_packet_peer() const {
	ERR_FAIL_COND_V(incoming.is_empty(), 0);
	return incoming.front()->get().from;
}

MultiplayerPeer::TransferMode GasmRelayPeer::get_packet_mode() const {
	ERR_FAIL_COND_V(incoming.is_empty(), TRANSFER_MODE_RELIABLE);
	return incoming.front()->get().mode;
}

int GasmRelayPeer::get_packet_channel() const {
	ERR_FAIL_COND_V(incoming.is_empty(), 0);
	return incoming.front()->get().channel;
}

void GasmRelayPeer::connected(int p_id) {
	if (p_id != get_unique_id() && !peers.has(p_id)) {
		peers.insert(p_id);
		emit_signal(SNAME("peer_connected"), p_id);
	}
}

void GasmRelayPeer::disconnected(int p_id) {
	if (peers.has(p_id)) {
		peers.erase(p_id);
		emit_signal(SNAME("peer_disconnected"), p_id);
	}
}

void GasmRelayPeer::poll() {
	if (conn <= 0) {
		return;
	}
	for (;;) {
		int n = gasm_net_recv(conn, nullptr, 0);
		if (n == -1) {
			close(); // the relay went away
			return;
		}
		if (n <= 0) {
			break;
		}
		scratch.resize(n);
		gasm_net_recv(conn, scratch.ptrw(), n);
		const uint8_t *m = scratch.ptr();
		switch (m[0]) {
			case RELAY_WELCOME:
				if (n < 3) {
					break;
				}
				index = m[1];
				status = CONNECTION_CONNECTED;
				// the relay gives a newcomer the lowest free index, so a client's server
				// (index 0) is already there: connect to it and say we're here
				if (!is_server()) {
					connected(1);
					send_raw(KIND_HERE, 1, nullptr, 0);
				}
				break;
			case RELAY_JOIN:
				if (n >= 2 && is_server()) {
					connected(m[1] + 1);
				} else if (n >= 2 && m[1] == 0) {
					// a server arrived after us (the previous one left): introduce ourselves
					connected(1);
					send_raw(KIND_HERE, 1, nullptr, 0);
				}
				break;
			case RELAY_LEAVE:
				if (n >= 2) {
					disconnected(m[1] + 1);
				}
				break;
			case RELAY_FULL:
				ERR_PRINT("gasm-relay: the room is full.");
				close();
				return;
			case RELAY_DATA: {
				if (n < 2 + HEADER) {
					break;
				}
				int from = m[1] + 1;
				const uint8_t *p = m + 2;
				uint32_t t = p[1] | (p[2] << 8) | (p[3] << 16) | ((uint32_t)p[4] << 24);
				int to = (int32_t)t;
				if (p[0] == KIND_HERE) {
					if (is_server()) {
						connected(from);
					}
					break;
				}
				bool mine = to == 0 || to == get_unique_id() || (to < 0 && -to != get_unique_id());
				if (!mine) {
					break;
				}
				// a server learns a client by its first message, a client knows only the server
				if (is_server()) {
					connected(from);
				} else if (from != 1) {
					break;
				}
				Packet pk;
				pk.from = from;
				pk.channel = p[5];
				pk.mode = (TransferMode)CLAMP((int)p[6], 0, 2);
				pk.data.resize(n - 2 - HEADER);
				if (pk.data.size()) {
					memcpy(pk.data.ptrw(), p + HEADER, pk.data.size());
				}
				incoming.push_back(pk);
			} break;
		}
	}
}

void GasmRelayPeer::disconnect_peer(int p_peer, bool p_force) {
	// the relay can't drop one peer; forget it here
	disconnected(p_peer);
}

void GasmRelayPeer::close() {
	if (conn > 0) {
		gasm_net_close(conn);
	}
	conn = -1;
	for (int id : peers) {
		emit_signal(SNAME("peer_disconnected"), id);
	}
	peers.clear();
	incoming.clear();
	index = -1;
	status = CONNECTION_DISCONNECTED;
}

GasmRelayPeer::~GasmRelayPeer() {
	if (conn > 0) {
		gasm_net_close(conn);
	}
}
