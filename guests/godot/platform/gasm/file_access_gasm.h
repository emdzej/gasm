/**************************************************************************/
/*  file_access_gasm.h                                                    */
/**************************************************************************/
/* Godot on gasm (https://gasm.emdzej.pl): MIT, like Godot.                */
/* Files on gasm: paths under /user (user://) are gasm:storage keys, read and
 * written whole; every other path is a gasm asset (the game's .pck, ...), read
 * on demand with positioned reads. Directories are implied by the names. */
#pragma once

#include "core/io/dir_access.h"
#include "core/io/file_access.h"

namespace gasm_fs {
// "/user/saves/a_b.sav" -> storage key "saves_sa_ub.sav"; "" if the path isn't under /user
String storage_key(const String &p_path);
String key_to_path(const String &p_key); // the path under /user
String asset_name(const String &p_path); // "/game.pck" -> "game.pck"
bool is_user(const String &p_path);
} // namespace gasm_fs

class FileAccessGasm : public FileAccess {
	GDSOFTCLASS(FileAccessGasm, FileAccess);

	String path;
	String asset; // read from this asset...
	String key; // ...or this storage key (data holds the value)
	Vector<uint8_t> data;
	uint64_t length = 0;
	uint64_t pos = 0;
	bool open = false;
	bool writable = false;
	bool dirty = false;
	mutable bool eof = false;
	mutable Error last_error = OK;

	Error open_internal(const String &p_path, int p_mode_flags) override;
	uint64_t _get_modified_time(const String &p_file) override { return 0; }
	uint64_t _get_access_time(const String &p_file) override { return 0; }
	int64_t _get_size(const String &p_file) override;
	BitField<FileAccess::UnixPermissionFlags> _get_unix_permissions(const String &p_file) override { return 0; }
	Error _set_unix_permissions(const String &p_file, BitField<FileAccess::UnixPermissionFlags> p_permissions) override { return ERR_UNAVAILABLE; }
	bool _get_hidden_attribute(const String &p_file) override { return false; }
	Error _set_hidden_attribute(const String &p_file, bool p_hidden) override { return ERR_UNAVAILABLE; }
	bool _get_read_only_attribute(const String &p_file) override { return !gasm_fs::is_user(p_file); }
	Error _set_read_only_attribute(const String &p_file, bool p_ro) override { return ERR_UNAVAILABLE; }

public:
	bool is_open() const override { return open; }
	String get_path() const override { return path; }
	String get_path_absolute() const override { return path; }
	void seek(uint64_t p_position) override;
	void seek_end(int64_t p_position = 0) override;
	uint64_t get_position() const override { return pos; }
	uint64_t get_length() const override { return length; }
	bool eof_reached() const override { return eof; }
	uint64_t get_buffer(uint8_t *p_dst, uint64_t p_length) const override;
	Error get_error() const override { return last_error; }
	Error resize(int64_t p_length) override;
	void flush() override;
	bool store_buffer(const uint8_t *p_src, uint64_t p_length) override;
	void close() override;
	bool file_exists(const String &p_name) override;

	~FileAccessGasm() override;
};

class DirAccessGasm : public DirAccess {
	GDSOFTCLASS(DirAccessGasm, DirAccess);

	String current = "/";
	Vector<String> listing;
	Vector<bool> listing_dirs;
	int listing_pos = -1;
	bool listing_is_dir = false;

	String absolute(const String &p_path) const;
	// every file path (assets, and storage keys as /user/...) under p_dir
	Vector<String> paths_under(const String &p_dir) const;

public:
	Error list_dir_begin() override;
	String get_next() override;
	bool current_is_dir() const override { return listing_is_dir; }
	bool current_is_hidden() const override { return false; }
	void list_dir_end() override;
	int get_drive_count() override { return 0; }
	String get_drive(int p_drive) override { return String(); }
	Error change_dir(String p_dir) override;
	String get_current_dir(bool p_include_drive = true) const override { return current; }
	Error make_dir(String p_dir) override { return OK; } // directories are implied by names
	bool file_exists(String p_file) override;
	bool dir_exists(String p_dir) override;
	uint64_t get_space_left() override { return 16 * 1024 * 1024; }
	Error rename(String p_from, String p_to) override;
	Error remove(String p_name) override;
	bool is_link(String p_file) override { return false; }
	String read_link(String p_file) override { return p_file; }
	Error create_link(String p_source, String p_target) override { return ERR_UNAVAILABLE; }
	String get_filesystem_type() const override { return "gasm"; }
};
