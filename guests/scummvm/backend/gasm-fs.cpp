/*
 * ScummVM on gasm: asset filesystem (see gasm-fs.h).
 *
 * SPDX-License-Identifier: MIT (this file; the ScummVM build as a whole is GPL-3.0)
 */
#define FORBIDDEN_SYMBOL_ALLOW_ALL
#include "common/scummsys.h"

#include "backends/platform/gasm/gasm-fs.h"

#include "common/array.h"
#include "common/bufferedstream.h"
#include "common/stream.h"

#include "gasm.h"

/** All asset names, sorted (the set is fixed at start-up). */
static const Common::Array<Common::String> &assetNames() {
	static Common::Array<Common::String> names;
	static bool loaded = false;
	if (!loaded) {
		loaded = true;
		uint32 n = gasm_asset_count();
		for (uint32 i = 0; i < n; i++) {
			int32 len = gasm_asset_name(i, nullptr, 0);
			if (len <= 0)
				continue;
			Common::String s;
			char *buf = new char[len + 1];
			gasm_asset_name(i, buf, (uint32)len);
			buf[len] = 0;
			s = buf;
			delete[] buf;
			names.push_back(s);
		}
	}
	return names;
}

static int64 assetSize(const Common::String &name) {
	return gasm_asset_size64(name.c_str(), name.size());
}

/** Bytes read from the runner at once: engines read a few bytes at a time
 *  (readByte, readUint16LE), and each host call is a lookup and a positioned read. */
static const uint32 kReadAhead = 16 * 1024;

/** An asset read on demand (wrapped in a read-ahead buffer by createReadStream). */
class GasmAssetStream : public Common::SeekableReadStream {
public:
	GasmAssetStream(const Common::String &name, int64 size) : _name(name), _size(size), _pos(0), _eos(false), _err(false) {}
	bool err() const override { return _err; }
	void clearErr() override { _err = false; _eos = false; }
	bool eos() const override { return _eos; }
	uint32 read(void *dataPtr, uint32 dataSize) override {
		if (_pos >= _size) {
			_eos = true;
			return 0;
		}
		if ((int64)dataSize > _size - _pos)
			dataSize = (uint32)(_size - _pos);
		int32 n = gasm_asset_read_at64(_name.c_str(), _name.size(), (uint64)_pos, dataPtr, dataSize);
		if (n < 0) {
			_err = true;
			return 0;
		}
		_pos += n;
		if ((uint32)n < dataSize)
			_eos = true;
		return (uint32)n;
	}
	int64 pos() const override { return _pos; }
	int64 size() const override { return _size; }
	bool seek(int64 offset, int whence = SEEK_SET) override {
		int64 p = whence == SEEK_SET ? offset : whence == SEEK_CUR ? _pos + offset : _size + offset;
		if (p < 0 || p > _size)
			return false;
		_pos = p;
		_eos = false;
		return true;
	}

private:
	Common::String _name;
	int64 _size, _pos;
	bool _eos, _err;
};

GasmFSNode::GasmFSNode(const Common::String &path) : _path(path), _isDir(false), _isFile(false) {
	while (_path.hasSuffix("/"))
		_path.deleteLastChar();
	if (_path.empty()) {
		_isDir = true;
		return;
	}
	_isFile = assetSize(_path) >= 0;
	if (!_isFile) {
		Common::String prefix = _path + "/";
		for (const Common::String &n : assetNames()) {
			if (n.hasPrefixIgnoreCase(prefix)) {
				_isDir = true;
				break;
			}
		}
	}
}

bool GasmFSNode::exists() const {
	return _isDir || _isFile;
}

Common::String GasmFSNode::getName() const {
	size_t slash = _path.findLastOf('/');
	return slash == Common::String::npos ? _path : _path.substr(slash + 1);
}

AbstractFSNode *GasmFSNode::getChild(const Common::String &name) const {
	return new GasmFSNode(_path.empty() ? name : _path + "/" + name);
}

AbstractFSNode *GasmFSNode::getParent() const {
	if (_path.empty())
		return nullptr;
	size_t slash = _path.findLastOf('/');
	return new GasmFSNode(slash == Common::String::npos ? "" : _path.substr(0, slash));
}

bool GasmFSNode::getChildren(AbstractFSList &list, ListMode mode, bool) const {
	if (!_isDir)
		return false;
	Common::String prefix = _path.empty() ? "" : _path + "/";
	// names are sorted, so a subfolder's files are adjacent: comparing with the last child suffices
	Common::String last;
	bool any = false;
	for (const Common::String &n : assetNames()) {
		if (!n.hasPrefixIgnoreCase(prefix))
			continue;
		Common::String rest = n.substr(prefix.size());
		size_t slash = rest.findFirstOf('/');
		bool dir = slash != Common::String::npos;
		Common::String child = dir ? rest.substr(0, slash) : rest;
		if (child.empty() || (mode == Common::FSNode::kListFilesOnly && dir) || (mode == Common::FSNode::kListDirectoriesOnly && !dir))
			continue;
		if (any && child == last)
			continue;
		last = child;
		any = true;
		list.push_back(new GasmFSNode(n.substr(0, prefix.size()) + child));
	}
	return true;
}

Common::SeekableReadStream *GasmFSNode::createReadStream() {
	int64 size = _isFile ? assetSize(_path) : -1;
	if (size < 0)
		return nullptr;
	return Common::wrapBufferedSeekableReadStream(new GasmAssetStream(_path, size), kReadAhead, DisposeAfterUse::YES);
}

AbstractFSNode *GasmFilesystemFactory::makeFileNodePath(const Common::String &path) const {
	Common::String p = path;
	while (p.hasPrefix("/") || p.hasPrefix("./"))
		p = p.substr(p.hasPrefix("/") ? 1 : 2);
	if (p == ".")
		p.clear();
	return new GasmFSNode(p);
}
