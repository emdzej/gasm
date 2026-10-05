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
// get_param(name) is a launch parameter (--param name=value, the page's URL
// parameters), "" if missing.

#include "api.h"

#include "core/config/engine.h"
#include "core/object/class_db.h"
#include "core/object/object.h"

#include "gasm.h"

class Gasm : public Object {
	GDCLASS(Gasm, Object);

protected:
	static void _bind_methods() {
		ClassDB::bind_method(D_METHOD("save_file", "data", "name", "mime"), &Gasm::save_file);
		ClassDB::bind_method(D_METHOD("save_state", "id"), &Gasm::save_state);
		ClassDB::bind_method(D_METHOD("can_save_files"), &Gasm::can_save_files);
		ClassDB::bind_method(D_METHOD("get_param", "name"), &Gasm::get_param);
	}

public:
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
	gasm_singleton = memnew(Gasm);
	Engine::get_singleton()->add_singleton(Engine::Singleton("Gasm", gasm_singleton));
}

void unregister_gasm_api() {
	memdelete(gasm_singleton);
}
