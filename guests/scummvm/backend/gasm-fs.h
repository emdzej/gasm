/*
 * ScummVM on gasm: the game's files are the gasm assets, as a read-only tree.
 * Asset names ("BASS/sky.dnr") are paths; directories are implied by them.
 * Files stream on demand with asset_read_at. Paths start at "/".
 *
 * SPDX-License-Identifier: MIT (this file; the ScummVM build as a whole is GPL-3.0)
 */
#ifndef BACKENDS_PLATFORM_GASM_FS_H
#define BACKENDS_PLATFORM_GASM_FS_H

#include "backends/fs/abstract-fs.h"
#include "backends/fs/fs-factory.h"

class GasmFSNode : public AbstractFSNode {
public:
	/** `path` without the leading "/" ("" is the root). */
	explicit GasmFSNode(const Common::String &path);

	bool exists() const override;
	Common::U32String getDisplayName() const override { return Common::U32String(getName()); }
	Common::String getName() const override;
	Common::String getPath() const override { return "/" + _path; }
	bool isDirectory() const override { return _isDir; }
	bool isReadable() const override { return exists(); }
	bool isWritable() const override { return false; }

	AbstractFSNode *getChild(const Common::String &name) const override;
	bool getChildren(AbstractFSList &list, ListMode mode, bool hidden) const override;
	AbstractFSNode *getParent() const override;

	Common::SeekableReadStream *createReadStream() override;
	Common::SeekableWriteStream *createWriteStream(bool atomic) override { return nullptr; }
	bool createDirectory() override { return false; }

private:
	Common::String _path;
	bool _isDir, _isFile;
};

class GasmFilesystemFactory : public FilesystemFactory {
public:
	AbstractFSNode *makeRootFileNode() const override { return new GasmFSNode(""); }
	AbstractFSNode *makeCurrentDirectoryFileNode() const override { return new GasmFSNode(""); }
	AbstractFSNode *makeFileNodePath(const Common::String &path) const override;
};

#endif
