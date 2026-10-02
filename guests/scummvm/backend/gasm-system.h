/*
 * ScummVM on gasm: the OSystem implementation.
 *
 * Copied into backends/platform/gasm/ by scripts/fetch-scummvm.sh.
 * SPDX-License-Identifier: MIT (this file; the ScummVM build as a whole is GPL-3.0)
 */
#ifndef BACKENDS_PLATFORM_GASM_SYSTEM_H
#define BACKENDS_PLATFORM_GASM_SYSTEM_H

#include "backends/modular-backend.h"
#include "common/events.h"
#include "common/queue.h"

class GasmGraphicsManager;
class GasmMixerManager;

class OSystem_Gasm : public ModularMixerBackend, public ModularGraphicsBackend, public Common::EventSource {
public:
	static const int kFrameRate = 60;
	static const int kSampleRate = 44100;

	OSystem_Gasm();
	~OSystem_Gasm() override;

	void initBackend() override;
	bool hasFeature(Feature f) override;

	bool pollEvent(Common::Event &event) override;

	Common::MutexInternal *createMutex() override;
	uint32 getMillis(bool skipRecord = false) override;
	void delayMillis(uint msecs) override;
	void getTimeAndDate(TimeDate &td, bool skipRecord = false) const override;

	void quit() override;
	void logMessage(LogMessageType::Type type, const char *message) override;
	void addSysArchivesToSearchSet(Common::SearchSet &s, int priority) override;
	Common::SeekableReadStream *createConfigReadStream() override;
	Common::WriteStream *createConfigWriteStream() override;
	Common::Path getDefaultConfigFileName() override;

	/** Called by gasm_frame when the engine resumes: read this frame's input. */
	void beginFrame();
	/** Called before yielding to the runner: mix this frame's audio. */
	void endFrame();
	bool quitRequested() const { return _quitRequested; }

private:
	void collectKeys();
	void collectPointer();
	void collectGamepads();
	void pushKey(uint32 gasmKey, bool down);

	Common::Queue<Common::Event> _events;
	double _millis;          // virtual time: advanced by delayMillis, frame-aligned yields
	double _nextFrame;       // virtual time at which the current frame ends
	uint32 _spins;           // getMillis calls without a delay (busy-wait guard)
	uint32 _frames;
	uint8 _keys[32];         // held raw keys (GASM_KEY_* bitset), for modifiers
	uint32 _mouseButtons;
	float _gamepadAxes[4];
	uint32 _gamepadButtons;
	bool _quitRequested;
};

/** Yield to the runner until the next gasm_frame (Asyncify, see gasm-main.cpp). */
void gasm_scummvm_yield();

#endif
