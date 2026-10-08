/**************************************************************************/
/*  relay_peer.h: Godot's MultiplayerPeer through gasm-relay (gasm, MIT)   */
/**************************************************************************/

#pragma once

#include "core/templates/hash_set.h"
#include "core/templates/list.h"
#include "scene/main/multiplayer_peer.h"

// Godot's high-level multiplayer (RPCs, MultiplayerSynchronizer, MultiplayerSpawner) in a
// gasm-relay room over gasm:net, with no Godot server: the room's first player (relay
// index 0) is the server, peer id 1; the others are index + 1. The relay only
// broadcasts, so each message carries its target and peers drop what isn't theirs.
// Clients talk to the server, which relays between clients (server relay).
//
//   multiplayer.multiplayer_peer = Engine.get_singleton("Gasm").create_relay_peer("ws://host:9000/room")
//
// Message, inside the relay's DATA payload: [kind u8][target i32 LE][channel u8][mode u8][data]
// kind 0: data; kind 1: "here" (a client announcing itself to the server, for a server
// that started after it). All transfer modes arrive reliably and in order (WebSocket).
class GasmRelayPeer : public MultiplayerPeer {
	GDCLASS(GasmRelayPeer, MultiplayerPeer);

	struct Packet {
		Vector<uint8_t> data;
		int from = 0;
		int channel = 0;
		TransferMode mode = TRANSFER_MODE_RELIABLE;
	};

	int conn = -1;
	ConnectionStatus status = CONNECTION_DISCONNECTED;
	int index = -1;    // our relay index; peer id = index + 1
	int target = 0;    // set_target_peer
	HashSet<int> peers; // connected peer ids (the server knows every client; a client, the server)
	List<Packet> incoming;
	Packet current;
	Vector<uint8_t> scratch;

	void send_raw(uint8_t p_kind, int p_target, const uint8_t *p_data, int p_size);
	void connected(int p_id);
	void disconnected(int p_id);

protected:
	static void _bind_methods() {}

public:
	Error connect_to_url(const String &p_url);

	// PacketPeer
	int get_available_packet_count() const override { return incoming.size(); }
	Error get_packet(const uint8_t **r_buffer, int &r_buffer_size) override;
	Error put_packet(const uint8_t *p_buffer, int p_buffer_size) override;
	int get_max_packet_size() const override { return 1 << 20; }

	// MultiplayerPeer
	void set_target_peer(int p_peer_id) override { target = p_peer_id; }
	int get_packet_peer() const override;
	TransferMode get_packet_mode() const override;
	int get_packet_channel() const override;
	void disconnect_peer(int p_peer, bool p_force = false) override;
	bool is_server() const override { return index == 0; }
	bool is_server_relay_supported() const override { return true; }
	void poll() override;
	void close() override;
	int get_unique_id() const override { return index + 1; }
	ConnectionStatus get_connection_status() const override { return status; }

	~GasmRelayPeer();
};
