/**************************************************************************/
/*  file_access_gasm.cpp                                                  */
/**************************************************************************/
/* Godot on gasm (https://gasm.emdzej.pl): MIT, like Godot.                */
#include "file_access_gasm.h"

#include "gasm.h"

namespace gasm_fs {

static String normalize(const String &p_path) {
	String p = p_path.replace("\\", "/");
	if (p.begins_with("res://")) {
		p = p.substr(6);
	} else if (p.begins_with("user://")) {
		p = "/user/" + p.substr(7);
	}
	if (!p.begins_with("/")) {
		p = "/" + p;
	}
	return p.simplify_path();
}

bool is_user(const String &p_path) {
	String p = normalize(p_path);
	return p == "/user" || p.begins_with("/user/");
}

String asset_name(const String &p_path) {
	return normalize(p_path).substr(1);
}

// Storage keys are [A-Za-z0-9._-]{1,128}: '/' becomes "_s", '_' "_u", anything else "_xHH".
String storage_key(const String &p_path) {
	String p = normalize(p_path);
	if (!p.begins_with("/user/")) {
		return String();
	}
	CharString rel = p.substr(6).utf8();
	String key;
	for (int i = 0; i < rel.length(); i++) {
		uint8_t c = (uint8_t)rel[i];
		if ((c >= 'a' && c <= 'z') || (c >= 'A' && c <= 'Z') || (c >= '0' && c <= '9') || c == '.' || c == '-') {
			key += String::chr(c);
		} else if (c == '/') {
			key += "_s";
		} else if (c == '_') {
			key += "_u";
		} else {
			key += vformat("_x%02x", c);
		}
	}
	if (key == "." || key == ".." || key.length() > 128) {
		return String();
	}
	return key;
}

String key_to_path(const String &p_key) {
	CharString k = p_key.utf8();
	Vector<uint8_t> out;
	for (int i = 0; i < k.length(); i++) {
		if (k[i] == '_' && i + 1 < k.length()) {
			char t = k[i + 1];
			if (t == 's') {
				out.push_back('/');
				i++;
				continue;
			}
			if (t == 'u') {
				out.push_back('_');
				i++;
				continue;
			}
			if (t == 'x' && i + 3 < k.length()) {
				out.push_back((uint8_t)String(String::utf8(k.ptr() + i + 2, 2)).hex_to_int());
				i += 3;
				continue;
			}
		}
		out.push_back((uint8_t)k[i]);
	}
	return "/user/" + String::utf8((const char *)out.ptr(), out.size());
}

static Vector<String> storage_paths() {
	Vector<String> out;
	uint32_t n = gasm_storage_count();
	for (uint32_t i = 0; i < n; i++) {
		char buf[129];
		int32_t len = gasm_storage_key(i, buf, sizeof buf - 1);
		if (len > 0 && len < (int32_t)sizeof buf) {
			out.push_back(key_to_path(String::utf8(buf, len)));
		}
	}
	return out;
}

static Vector<String> asset_paths() {
	Vector<String> out;
	uint32_t n = gasm_asset_count();
	for (uint32_t i = 0; i < n; i++) {
		int32_t len = gasm_asset_name(i, nullptr, 0);
		if (len <= 0) {
			continue;
		}
		CharString buf;
		buf.resize_uninitialized(len + 1);
		gasm_asset_name(i, buf.ptrw(), len);
		out.push_back("/" + String::utf8(buf.ptr(), len));
	}
	return out;
}

} // namespace gasm_fs

using namespace gasm_fs;

Error FileAccessGasm::open_internal(const String &p_path, int p_mode_flags) {
	close();
	path = p_path;
	pos = 0;
	eof = false;
	writable = p_mode_flags & WRITE;
	if (is_user(p_path)) {
		key = storage_key(p_path);
		if (key.is_empty()) {
			return last_error = ERR_FILE_BAD_PATH;
		}
		CharString k = key.utf8();
		int32_t n = gasm_storage_get(k.ptr(), k.length(), nullptr, 0);
		bool truncate = (p_mode_flags & WRITE) && !(p_mode_flags & READ);
		if (n < 0 && !(p_mode_flags & WRITE)) {
			return last_error = ERR_FILE_NOT_FOUND;
		}
		data.clear();
		if (n > 0 && !truncate) {
			data.resize(n);
			gasm_storage_get(k.ptr(), k.length(), data.ptrw(), n);
		}
		length = data.size();
		dirty = truncate || n < 0;
	} else {
		if (p_mode_flags & WRITE) {
			return last_error = ERR_FILE_CANT_WRITE; // assets are read-only
		}
		asset = asset_name(p_path);
		CharString a = asset.utf8();
		int64_t n = gasm_asset_size64(a.ptr(), a.length());
		if (n < 0) {
			asset = String();
			return last_error = ERR_FILE_NOT_FOUND;
		}
		length = (uint64_t)n;
	}
	open = true;
	return last_error = OK;
}

// Assets: their gasm.asset_version (0 as launched, larger each time the runner replaces
// one), so FileAccess.get_modified_time() tells a game when to re-read. user://: 0.
uint64_t FileAccessGasm::_get_modified_time(const String &p_file) {
	static const bool has = gasm_has_str("gasm.asset_version") == 1;
	if (!has || gasm_fs::is_user(p_file)) {
		return 0;
	}
	CharString a = gasm_fs::asset_name(p_file).utf8();
	int32_t v = gasm_asset_version(a.ptr(), a.length());
	return v > 0 ? (uint64_t)v : 0;
}

int64_t FileAccessGasm::_get_size(const String &p_file) {
	if (is_user(p_file)) {
		CharString k = storage_key(p_file).utf8();
		return k.length() ? gasm_storage_get(k.ptr(), k.length(), nullptr, 0) : -1;
	}
	CharString a = asset_name(p_file).utf8();
	return gasm_asset_size64(a.ptr(), a.length());
}

void FileAccessGasm::seek(uint64_t p_position) {
	pos = p_position;
	eof = false;
}

void FileAccessGasm::seek_end(int64_t p_position) {
	seek((uint64_t)((int64_t)length + p_position));
}

uint64_t FileAccessGasm::get_buffer(uint8_t *p_dst, uint64_t p_length) const {
	ERR_FAIL_COND_V(!open, 0);
	uint64_t n = pos < length ? MIN(p_length, length - pos) : 0;
	if (n > 0) {
		if (!asset.is_empty()) {
			CharString a = asset.utf8();
			uint64_t done = 0;
			while (done < n) { // asset_read_at64 reads at most 2 GiB at once
				uint32_t chunk = (uint32_t)MIN(n - done, (uint64_t)1 << 30);
				int32_t got = gasm_asset_read_at64(a.ptr(), a.length(), pos + done, p_dst + done, chunk);
				if (got <= 0) {
					break;
				}
				done += (uint64_t)got;
			}
			n = done;
		} else {
			memcpy(p_dst, data.ptr() + pos, n);
		}
	}
	const_cast<FileAccessGasm *>(this)->pos += n;
	if (n < p_length) {
		eof = true;
	}
	return n;
}

Error FileAccessGasm::resize(int64_t p_length) {
	ERR_FAIL_COND_V(!open || !writable, ERR_FILE_CANT_WRITE);
	data.resize(p_length);
	length = p_length;
	dirty = true;
	return OK;
}

bool FileAccessGasm::store_buffer(const uint8_t *p_src, uint64_t p_length) {
	ERR_FAIL_COND_V(!open || !writable || key.is_empty(), false);
	if (pos + p_length > (uint64_t)data.size()) {
		data.resize(pos + p_length);
	}
	memcpy(data.ptrw() + pos, p_src, p_length);
	pos += p_length;
	length = MAX(length, pos);
	dirty = true;
	return true;
}

void FileAccessGasm::flush() {
	if (open && dirty && !key.is_empty()) {
		CharString k = key.utf8();
		int32_t r = gasm_storage_set(k.ptr(), k.length(), data.ptr(), data.size());
		if (r != 0) {
			last_error = ERR_FILE_CANT_WRITE;
			ERR_PRINT(vformat("gasm: saving %s failed (gasm:storage error %d)", path, r));
		}
		dirty = false;
	}
}

void FileAccessGasm::close() {
	flush();
	open = false;
	asset = String();
	key = String();
	data.clear();
	length = 0;
}

bool FileAccessGasm::file_exists(const String &p_name) {
	return _get_size(p_name) >= 0;
}

FileAccessGasm::~FileAccessGasm() {
	close();
}

// ---- directories ----------------------------------------------------------------------------

String DirAccessGasm::absolute(const String &p_path) const {
	String p = fix_path(p_path);
	if (p.begins_with("res://") || p.begins_with("user://")) {
		return gasm_fs::normalize(p);
	}
	if (!p.begins_with("/")) {
		p = current.path_join(p);
	}
	return gasm_fs::normalize(p);
}

Vector<String> DirAccessGasm::paths_under(const String &p_dir) const {
	String dir = p_dir.ends_with("/") ? p_dir : p_dir + "/";
	Vector<String> all = is_user(p_dir) || p_dir == "/" ? storage_paths() : Vector<String>();
	if (!is_user(p_dir)) {
		all.append_array(asset_paths());
	}
	Vector<String> out;
	for (const String &p : all) {
		if (p.begins_with(dir)) {
			out.push_back(p);
		}
	}
	return out;
}

Error DirAccessGasm::list_dir_begin() {
	list_dir_end();
	String dir = current.ends_with("/") ? current : current + "/";
	for (const String &p : paths_under(current)) {
		String rest = p.substr(dir.length());
		int slash = rest.find("/");
		String name = slash < 0 ? rest : rest.substr(0, slash);
		if (!name.is_empty() && !listing.has(name)) {
			listing.push_back(name);
			listing_dirs.push_back(slash >= 0);
		}
	}
	listing_pos = 0;
	return OK;
}

String DirAccessGasm::get_next() {
	if (listing_pos < 0 || listing_pos >= listing.size()) {
		return String();
	}
	listing_is_dir = listing_dirs[listing_pos];
	return listing[listing_pos++];
}

void DirAccessGasm::list_dir_end() {
	listing.clear();
	listing_dirs.clear();
	listing_pos = -1;
}

Error DirAccessGasm::change_dir(String p_dir) {
	String p = absolute(p_dir);
	if (!is_user(p) && p != "/" && !dir_exists(p)) { // under /user every directory exists (they're implied)
		return ERR_INVALID_PARAMETER;
	}
	current = p;
	return OK;
}

bool DirAccessGasm::file_exists(String p_file) {
	String p = absolute(p_file);
	if (is_user(p)) {
		CharString k = storage_key(p).utf8();
		return k.length() && gasm_storage_get(k.ptr(), k.length(), nullptr, 0) >= 0;
	}
	CharString a = asset_name(p).utf8();
	return gasm_asset_size64(a.ptr(), a.length()) >= 0;
}

bool DirAccessGasm::dir_exists(String p_dir) {
	String p = absolute(p_dir);
	return p == "/" || is_user(p) || !paths_under(p).is_empty();
}

Error DirAccessGasm::rename(String p_from, String p_to) {
	String from = absolute(p_from), to = absolute(p_to);
	CharString kf = storage_key(from).utf8(), kt = storage_key(to).utf8();
	ERR_FAIL_COND_V(!kf.length() || !kt.length(), ERR_UNAVAILABLE);
	int32_t n = gasm_storage_get(kf.ptr(), kf.length(), nullptr, 0);
	ERR_FAIL_COND_V(n < 0, ERR_FILE_NOT_FOUND);
	Vector<uint8_t> v;
	v.resize(n);
	gasm_storage_get(kf.ptr(), kf.length(), v.ptrw(), n);
	ERR_FAIL_COND_V(gasm_storage_set(kt.ptr(), kt.length(), v.ptr(), n) != 0, ERR_FILE_CANT_WRITE);
	gasm_storage_delete(kf.ptr(), kf.length());
	return OK;
}

Error DirAccessGasm::remove(String p_name) {
	CharString k = storage_key(absolute(p_name)).utf8();
	if (!k.length()) {
		return ERR_UNAVAILABLE;
	}
	gasm_storage_delete(k.ptr(), k.length());
	return OK;
}
