/**************************************************************************/
/*  api.cpp: the Gasm singleton (gasm platform, MIT)                      */
/**************************************************************************/

// What gasm offers beyond Godot's own APIs, for GDScript:
//
//   if Engine.has_singleton("Gasm"):
//       var gasm = Engine.get_singleton("Gasm")
//       var id: int = gasm.save_file(image.save_png_to_buffer(), "photo-001.png", "image/png")
//       # later: gasm.save_state(id) == 1 (saved), 2 (failed or cancelled), 0 (pending)
//
// save_file hands the runner a copy for the player (gasm:files): natively in
// Pictures/<game>/ (images) or Downloads/<game>/, in browsers a download.
// create_relay_peer(url) is Godot's high-level multiplayer through a gasm-relay room:
//   multiplayer.multiplayer_peer = gasm.create_relay_peer("ws://host:9000/room")
// (the room's first player is the server, id 1).
// get_param(name) is a launch parameter (--param name=value, the page's URL
// parameters), "" if missing.
//
// Mods (gasm-run --mods <dir>): get_mods() lists the packs in load order, ready for
//   for path in gasm.get_mods():
//       ProjectSettings.load_resource_pack(path)   # later packs override earlier ones
// and get_refused_mods() the ones the runner couldn't read ({ name: reason }).
//
// Audio streams (gasm:media: internet radio, tracks), played by the runner after the
// game's own audio; the game's manifest needs "media": true (gasm-run --allow-media):
//   var radio := gasm.media_open("https://radio.example/live.mp3")   # -1: refused, see media_refusal()
//   gasm.media_play(radio); gasm.media_set_volume(radio, 0.8)
//   # each frame: gasm.media_state(radio) (Gasm.MEDIA_PLAYING, ...), and when
//   # gasm.media_metadata_version(radio) changes, gasm.media_metadata(radio).title
//   gasm.media_close(radio)
// A handle media_open didn't return (or a closed one) is an error, not a crash.

#include "api.h"

#include "core/config/engine.h"
#include "core/object/class_db.h"
#include "core/object/object.h"
#include "core/io/dir_access.h"
#include "core/io/file_access.h"
#include "core/io/json.h"
#include "core/templates/hash_set.h"

#include "gasm.h"
#include "relay_peer.h"

class Gasm : public Object {
	GDCLASS(Gasm, Object);

protected:
	static void _bind_methods() {
		ClassDB::bind_method(D_METHOD("save_file", "data", "name", "mime"), &Gasm::save_file);
		ClassDB::bind_method(D_METHOD("save_state", "id"), &Gasm::save_state);
		ClassDB::bind_method(D_METHOD("can_save_files"), &Gasm::can_save_files);
		ClassDB::bind_method(D_METHOD("get_param", "name"), &Gasm::get_param);
		ClassDB::bind_method(D_METHOD("get_mods"), &Gasm::get_mods);
		ClassDB::bind_method(D_METHOD("get_refused_mods"), &Gasm::get_refused_mods);
		ClassDB::bind_method(D_METHOD("create_relay_peer", "url"), &Gasm::create_relay_peer);

		ClassDB::bind_method(D_METHOD("can_play_media"), &Gasm::can_play_media);
		ClassDB::bind_method(D_METHOD("media_open", "url"), &Gasm::media_open);
		ClassDB::bind_method(D_METHOD("media_refusal"), &Gasm::media_refusal);
		ClassDB::bind_method(D_METHOD("media_play", "stream"), &Gasm::media_play);
		ClassDB::bind_method(D_METHOD("media_pause", "stream"), &Gasm::media_pause);
		ClassDB::bind_method(D_METHOD("media_close", "stream"), &Gasm::media_close);
		ClassDB::bind_method(D_METHOD("media_set_volume", "stream", "gain"), &Gasm::media_set_volume);
		ClassDB::bind_method(D_METHOD("media_set_lowpass", "stream", "hz"), &Gasm::media_set_lowpass);
		ClassDB::bind_method(D_METHOD("media_caps", "stream"), &Gasm::media_caps);
		ClassDB::bind_method(D_METHOD("media_state", "stream"), &Gasm::media_state);
		ClassDB::bind_method(D_METHOD("media_error", "stream"), &Gasm::media_error);
		ClassDB::bind_method(D_METHOD("media_position", "stream"), &Gasm::media_position);
		ClassDB::bind_method(D_METHOD("media_duration", "stream"), &Gasm::media_duration);
		ClassDB::bind_method(D_METHOD("media_metadata", "stream"), &Gasm::media_metadata);
		ClassDB::bind_method(D_METHOD("media_metadata_version", "stream"), &Gasm::media_metadata_version);
		BIND_CONSTANT(MEDIA_CONNECTING);
		BIND_CONSTANT(MEDIA_PLAYING);
		BIND_CONSTANT(MEDIA_PAUSED);
		BIND_CONSTANT(MEDIA_FAILED);
		BIND_CONSTANT(MEDIA_ENDED);
	}

	// gasm:media handles this game opened (a wrong one would trap in the runner)
	HashSet<int> media_streams;

	bool media_known(int p_stream) const {
		ERR_FAIL_COND_V_MSG(!media_streams.has(p_stream), false, vformat("Gasm: %d is not an open media stream (media_open's result).", p_stream));
		return true;
	}

	static String media_text(int p_stream, int32_t (*p_get)(int32_t, char *, uint32_t)) {
		int32_t n = p_get(p_stream, nullptr, 0);
		if (n <= 0) {
			return String();
		}
		Vector<uint8_t> v;
		v.resize(n);
		p_get(p_stream, (char *)v.ptrw(), n);
		return String::utf8((const char *)v.ptr(), n);
	}

public:
	enum {
		MEDIA_CONNECTING = GASM_MEDIA_CONNECTING,
		MEDIA_PLAYING = GASM_MEDIA_PLAYING,
		MEDIA_PAUSED = GASM_MEDIA_PAUSED,
		MEDIA_FAILED = GASM_MEDIA_FAILED,
		MEDIA_ENDED = GASM_MEDIA_ENDED,
	};

	bool can_play_media() const {
		static const bool has = gasm_has_str("gasm:media") == 1;
		return has;
	}

	// A stream from an https URL, not playing yet: a handle, or -1 if refused (media_refusal() says why).
	int media_open(const String &p_url) {
		if (!can_play_media()) {
			return -1;
		}
		Dictionary d;
		d["url"] = p_url;
		CharString desc = JSON::stringify(d).utf8();
		int h = gasm_media_open(desc.get_data(), desc.length());
		if (h > 0) {
			media_streams.insert(h);
		}
		return h > 0 ? h : -1;
	}

	String media_refusal() const {
		return can_play_media() ? media_text(-1, gasm_media_error) : String("This gasm runner can't play audio streams (gasm:media).");
	}

	void media_play(int p_stream) {
		if (media_known(p_stream)) {
			gasm_media_play(p_stream);
		}
	}

	void media_pause(int p_stream) {
		if (media_known(p_stream)) {
			gasm_media_pause(p_stream);
		}
	}

	void media_close(int p_stream) {
		if (media_streams.erase(p_stream)) {
			gasm_media_close(p_stream);
		}
	}

	void media_set_volume(int p_stream, float p_gain) {
		if (media_known(p_stream)) {
			gasm_media_set_volume(p_stream, p_gain);
		}
	}

	void media_set_lowpass(int p_stream, float p_hz) {
		if (media_known(p_stream)) {
			gasm_media_set_lowpass(p_stream, p_hz);
		}
	}

	// { volume, lowpass, metadata }: what the controls do for this stream (final once it plays)
	Dictionary media_caps(int p_stream) const {
		Dictionary d;
		uint32_t c = media_known(p_stream) ? gasm_media_caps(p_stream) : 0;
		d["volume"] = (c & GASM_MEDIA_CAN_VOLUME) != 0;
		d["lowpass"] = (c & GASM_MEDIA_CAN_LOWPASS) != 0;
		d["metadata"] = (c & GASM_MEDIA_CAN_METADATA) != 0;
		return d;
	}

	// MEDIA_CONNECTING, MEDIA_PLAYING, MEDIA_PAUSED, MEDIA_FAILED (media_error says why), MEDIA_ENDED
	int media_state(int p_stream) const {
		return media_known(p_stream) ? (int)gasm_media_state(p_stream) : (int)MEDIA_FAILED;
	}

	String media_error(int p_stream) const {
		return media_known(p_stream) ? media_text(p_stream, gasm_media_error) : String("not an open stream");
	}

	double media_position(int p_stream) const {
		return media_known(p_stream) ? gasm_media_position(p_stream) : 0.0;
	}

	// a track's length in seconds; -1 for live streams and while unknown
	double media_duration(int p_stream) const {
		return media_known(p_stream) ? gasm_media_duration(p_stream) : -1.0;
	}

	// { name, title, codec, bitrate } (null where unknown): the station's name and the current title
	Dictionary media_metadata(int p_stream) const {
		if (!media_known(p_stream)) {
			return Dictionary();
		}
		Variant v = JSON::parse_string(media_text(p_stream, gasm_media_metadata));
		return v.get_type() == Variant::DICTIONARY ? Dictionary(v) : Dictionary();
	}

	// changes whenever media_metadata does: poll it each frame
	int media_metadata_version(int p_stream) const {
		return media_known(p_stream) ? (int)gasm_media_metadata_version(p_stream) : 0;
	}

	String get_param(const String &p_name) const {
		CharString name = p_name.utf8();
		int32_t n = gasm_param(name.get_data(), name.length(), nullptr, 0);
		if (n <= 0) {
			return String();
		}
		Vector<uint8_t> v;
		v.resize(n);
		gasm_param(name.get_data(), name.length(), (char *)v.ptrw(), n);
		return String::utf8((const char *)v.ptr(), n);
	}

	// Godot's high-level multiplayer through a gasm-relay room (relay_peer.h): set it as
	// multiplayer.multiplayer_peer. null if it can't connect (the runner logs why).
	Ref<MultiplayerPeer> create_relay_peer(const String &p_url) {
		Ref<GasmRelayPeer> p;
		p.instantiate();
		if (p->connect_to_url(p_url) != OK) {
			return Ref<MultiplayerPeer>();
		}
		return p;
	}

	// The runner mounts mods as assets mods/<name> (the gasm file layer's /mods/).
	PackedStringArray get_mods() const {
		PackedStringArray out;
		Ref<DirAccess> d = DirAccess::open("/mods");
		if (d.is_null()) {
			return out;
		}
		PackedStringArray files = d->get_files();
		files.sort();
		for (const String &f : files) {
			String l = f.to_lower();
			if (l.ends_with(".pck") || l.ends_with(".zip")) {
				out.push_back("/mods/" + f);
			}
		}
		return out;
	}

	Dictionary get_refused_mods() const {
		Dictionary out;
		if (!FileAccess::exists("/mods.refused")) {
			return out;
		}
		for (const String &line : FileAccess::get_file_as_string("/mods.refused").split("\n", false)) {
			int tab = line.find("\t");
			out[tab < 0 ? line : line.substr(0, tab)] = tab < 0 ? String() : line.substr(tab + 1);
		}
		return out;
	}

	bool can_save_files() const {
		static const bool has = gasm_has_str("gasm:files") == 1;
		return has;
	}

	// An id > 0, or -1 if the runner refused (saving off, a bad name or type, too big).
	int save_file(const PackedByteArray &p_data, const String &p_name, const String &p_mime) {
		ERR_FAIL_COND_V_MSG(!can_save_files(), -1, "This gasm runner can't save files for the player (gasm:files).");
		CharString name = p_name.utf8(), mime = p_mime.utf8();
		int id = gasm_files_save(name.get_data(), name.length(), mime.get_data(), mime.length(), p_data.ptr(), p_data.size());
		last_id = MAX(last_id, id);
		return id;
	}

	// 0 pending, 1 saved, 2 failed or cancelled; -1 for an id save_file didn't return.
	int save_state(int p_id) const {
		if (!can_save_files() || p_id <= 0 || p_id > last_id) {
			return -1;
		}
		return gasm_files_state(p_id);
	}

private:
	// ids are handed out in order, so the last one bounds the valid range (the runner
	// traps on an id it never returned)
	int last_id = 0;
};

static Gasm *gasm_singleton = nullptr;

void register_gasm_api() {
	GDREGISTER_ABSTRACT_CLASS(Gasm);
	GDREGISTER_ABSTRACT_CLASS(GasmRelayPeer);
	gasm_singleton = memnew(Gasm);
	Engine::get_singleton()->add_singleton(Engine::Singleton("Gasm", gasm_singleton));
}

void unregister_gasm_api() {
	memdelete(gasm_singleton);
}
