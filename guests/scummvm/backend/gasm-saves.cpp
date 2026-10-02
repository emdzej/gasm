/*
 * ScummVM on gasm: storage-backed saves and config (see gasm-saves.h).
 *
 * SPDX-License-Identifier: MIT (this file; the ScummVM build as a whole is GPL-3.0)
 */
#define FORBIDDEN_SYMBOL_ALLOW_ALL
#include "common/scummsys.h"

#include "backends/platform/gasm/gasm-saves.h"

#include "common/array.h"
#include "common/memstream.h"

#include "gasm.h"

static const char kSavePrefix[] = "save.";

/** Storage keys are 1-128 bytes of [A-Za-z0-9._-]. */
static Common::String keyFor(const Common::String &name) {
	Common::String k = kSavePrefix;
	for (uint i = 0; i < name.size() && k.size() < 128; i++) {
		char c = name[i];
		k += Common::isAlnum(c) || c == '.' || c == '_' || c == '-' ? c : '_';
	}
	return k;
}

Common::SeekableReadStream *gasmStorageRead(const Common::String &key) {
	int32 n = gasm_storage_get(key.c_str(), key.size(), nullptr, 0);
	if (n < 0)
		return nullptr;
	byte *data = (byte *)malloc(n ? n : 1);
	if (n)
		gasm_storage_get(key.c_str(), key.size(), data, (uint32)n);
	return new Common::MemoryReadStream(data, (uint32)n, DisposeAfterUse::YES);
}

/** Buffers everything, stores it under the key on finalize() (or when destroyed). */
class GasmStorageWriteStream : public Common::SeekableWriteStream {
public:
	explicit GasmStorageWriteStream(const Common::String &key) : _key(key), _buf(DisposeAfterUse::YES), _stored(false), _err(false) {}
	~GasmStorageWriteStream() override { finalize(); }
	uint32 write(const void *dataPtr, uint32 dataSize) override { return _buf.write(dataPtr, dataSize); }
	int64 pos() const override { return _buf.pos(); }
	int64 size() const override { return _buf.size(); }
	bool seek(int64 offset, int whence = SEEK_SET) override { return _buf.seek(offset, whence); }
	bool err() const override { return _err; }
	void clearErr() override { _err = false; }
	void finalize() override {
		if (_stored)
			return;
		_stored = true;
		if (gasm_storage_set(_key.c_str(), _key.size(), _buf.getData(), (uint32)_buf.size()) != 0)
			_err = true;
	}

private:
	Common::String _key;
	Common::MemoryWriteStreamDynamic _buf;
	bool _stored, _err;
};

Common::SeekableWriteStream *gasmStorageWrite(const Common::String &key) {
	return new GasmStorageWriteStream(key);
}

Common::OutSaveFile *GasmSaveFileManager::openForSaving(const Common::String &name, bool) {
	// stored uncompressed: values are small, and the browser can inspect them
	return new Common::OutSaveFile(gasmStorageWrite(keyFor(name)));
}

Common::InSaveFile *GasmSaveFileManager::openForLoading(const Common::String &name) {
	return gasmStorageRead(keyFor(name));
}

bool GasmSaveFileManager::removeSavefile(const Common::String &name) {
	Common::String k = keyFor(name);
	return gasm_storage_delete(k.c_str(), k.size()) == 0;
}

bool GasmSaveFileManager::exists(const Common::String &name) {
	Common::String k = keyFor(name);
	return gasm_storage_get(k.c_str(), k.size(), nullptr, 0) >= 0;
}

Common::StringArray GasmSaveFileManager::listSavefiles(const Common::String &pattern) {
	Common::StringArray out;
	uint32 n = gasm_storage_count();
	char buf[129];
	for (uint32 i = 0; i < n; i++) {
		int32 len = gasm_storage_key(i, buf, sizeof buf - 1);
		if (len <= 0 || len >= (int32)sizeof buf)
			continue;
		buf[len] = 0;
		Common::String k(buf);
		if (!k.hasPrefix(kSavePrefix))
			continue;
		Common::String name = k.substr(sizeof kSavePrefix - 1);
		if (name.matchString(pattern, true))
			out.push_back(name);
	}
	return out;
}
