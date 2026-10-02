/*
 * ScummVM on gasm: the embedded engine data files as an archive (see gasm-data.h).
 *
 * SPDX-License-Identifier: MIT (this file; the ScummVM build as a whole is GPL-3.0)
 */
#define FORBIDDEN_SYMBOL_ALLOW_ALL
#include "common/scummsys.h"

#include "backends/platform/gasm/gasm-data.h"

#include "common/memstream.h"

const GasmEmbeddedFile *GasmEmbeddedArchive::find(const Common::Path &path) const {
	Common::String name = path.toString();
	for (unsigned i = 0; i < gasmEmbeddedCount; i++)
		if (name.equalsIgnoreCase(gasmEmbeddedFiles[i].name))
			return &gasmEmbeddedFiles[i];
	return nullptr;
}

bool GasmEmbeddedArchive::hasFile(const Common::Path &path) const {
	return find(path) != nullptr;
}

int GasmEmbeddedArchive::listMembers(Common::ArchiveMemberList &list) const {
	for (unsigned i = 0; i < gasmEmbeddedCount; i++)
		list.push_back(Common::ArchiveMemberPtr(new Common::GenericArchiveMember(Common::Path(gasmEmbeddedFiles[i].name), *this)));
	return (int)gasmEmbeddedCount;
}

const Common::ArchiveMemberPtr GasmEmbeddedArchive::getMember(const Common::Path &path) const {
	const GasmEmbeddedFile *f = find(path);
	return f ? Common::ArchiveMemberPtr(new Common::GenericArchiveMember(Common::Path(f->name), *this)) : Common::ArchiveMemberPtr();
}

Common::SeekableReadStream *GasmEmbeddedArchive::createReadStreamForMember(const Common::Path &path) const {
	const GasmEmbeddedFile *f = find(path);
	return f ? new Common::MemoryReadStream(f->data, f->size, DisposeAfterUse::NO) : nullptr;
}
