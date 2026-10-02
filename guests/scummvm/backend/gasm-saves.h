/*
 * ScummVM on gasm: save games and the config file in gasm:storage.
 * Each file is one key; save games are "save.<name>" (characters storage keys
 * can't hold become '_'), the config is "scummvm.ini".
 *
 * SPDX-License-Identifier: MIT (this file; the ScummVM build as a whole is GPL-3.0)
 */
#ifndef BACKENDS_PLATFORM_GASM_SAVES_H
#define BACKENDS_PLATFORM_GASM_SAVES_H

#include "common/savefile.h"

/** A storage value as a stream (null if the key doesn't exist). */
Common::SeekableReadStream *gasmStorageRead(const Common::String &key);
/** A stream stored under `key` when finalized or destroyed. */
Common::SeekableWriteStream *gasmStorageWrite(const Common::String &key);

class GasmSaveFileManager : public Common::SaveFileManager {
public:
	Common::OutSaveFile *openForSaving(const Common::String &name, bool compress = true) override;
	Common::InSaveFile *openForLoading(const Common::String &name) override;
	Common::InSaveFile *openRawFile(const Common::String &name) override { return openForLoading(name); }
	bool removeSavefile(const Common::String &name) override;
	Common::StringArray listSavefiles(const Common::String &pattern) override;
	void updateSavefilesList(Common::StringArray &lockedFiles) override {}
	bool exists(const Common::String &name) override;
};

#endif
