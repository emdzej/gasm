/*
 * ScummVM on gasm: OSystem_Gasm.
 *
 * - Time is virtual. Engines advance it by sleeping (delayMillis); when it
 *   crosses a 60 Hz frame boundary the engine yields to the runner (Asyncify,
 *   gasm-main.cpp), which resumes it on the next gasm_frame. So a run depends
 *   only on its input, and the runner's frame pacing paces the engine.
 * - Input: raw keys (with modifiers), the pointer in frame pixels, the wheel and
 *   gamepads become Common::Events at the start of each frame.
 * - Audio: the mixer is pulled for exactly one frame of samples per yield.
 * - Config and saves live in gasm:storage; game data is read from the assets.
 *
 * SPDX-License-Identifier: MIT (this file; the ScummVM build as a whole is GPL-3.0)
 */
#define FORBIDDEN_SYMBOL_ALLOW_ALL
#include "common/scummsys.h"

#include "backends/platform/gasm/gasm-system.h"
#include "backends/platform/gasm/gasm-graphics.h"
#include "backends/platform/gasm/gasm-fs.h"
#include "backends/platform/gasm/gasm-saves.h"
#include "backends/platform/gasm/gasm-data.h"

#include "audio/mixer_intern.h"
#include "backends/events/default/default-events.h"
#include "backends/mixer/mixer.h"
#include "backends/mutex/null/null-mutex.h"
#include "backends/timer/default/default-timer.h"
#include "common/config-manager.h"
#include "common/fs.h"
#include "common/memstream.h"

#include "gasm.h"

// ---- audio --------------------------------------------------------------------------

class GasmMixerManager : public MixerManager {
public:
	void init() override {
		_mixer = new Audio::MixerImpl(OSystem_Gasm::kSampleRate, true, kFrameSamples);
		_mixer->setReady(true);
		gasm_audio_config(OSystem_Gasm::kSampleRate, 2);
	}
	void suspendAudio() override { _audioSuspended = true; }
	int resumeAudio() override { _audioSuspended = false; return 0; }

	/** One frame of audio: 44100 / 60 = 735 stereo samples. */
	void mixFrame() {
		if (!_mixer)
			return;
		memset(_pcm, 0, sizeof _pcm);
		if (!_audioSuspended)
			_mixer->mixCallback((byte *)_pcm, sizeof _pcm);
		for (int i = 0; i < kFrameSamples * 2; i++)
			_out[i] = _pcm[i] / 32768.0f;
		gasm_audio_push(_out, kFrameSamples);
	}

private:
	static const int kFrameSamples = OSystem_Gasm::kSampleRate / OSystem_Gasm::kFrameRate;
	int16 _pcm[kFrameSamples * 2];
	float _out[kFrameSamples * 2];
};

// ---- config in storage ------------------------------------------------------------------

static const char kConfigKey[] = "scummvm.ini";

// ---- OSystem ----------------------------------------------------------------------------

OSystem_Gasm::OSystem_Gasm()
	: _millis(0), _nextFrame(1000.0 / kFrameRate), _spins(0), _frames(0), _mouseButtons(0),
	  _gamepadButtons(0), _quitRequested(false) {
	memset(_keys, 0, sizeof _keys);
	memset(_gamepadAxes, 0, sizeof _gamepadAxes);
	_fsFactory = new GasmFilesystemFactory();
}

OSystem_Gasm::~OSystem_Gasm() {
	delete _savefileManager;
	_savefileManager = nullptr;
	delete _timerManager;
	_timerManager = nullptr;
	delete _fsFactory;
	_fsFactory = nullptr;
}

void OSystem_Gasm::initBackend() {
	gasm_set_frame_rate(kFrameRate);
	// the game reads the keyboard itself and draws its own cursor
	gasm_input_mode(GASM_INPUT_KEYS_RAW | GASM_INPUT_POINTER_HIDDEN);
	_savefileManager = new GasmSaveFileManager();
	_timerManager = new DefaultTimerManager();
	_eventManager = new DefaultEventManager(this);
	_mixerManager = new GasmMixerManager();
	_graphicsManager = new GasmGraphicsManager();
	_mixerManager->init();
	BaseBackend::initBackend();
}

bool OSystem_Gasm::hasFeature(Feature f) {
	return ModularGraphicsBackend::hasFeature(f);
}

Common::MutexInternal *OSystem_Gasm::createMutex() {
	return new NullMutexInternal();   // one thread
}

uint32 OSystem_Gasm::getMillis(bool) {
	// An engine that waits by polling the clock would never return: let a long
	// run of reads without a sleep count as a short sleep.
	if (++_spins >= 64)
		delayMillis(1);
	return (uint32)_millis;
}

void OSystem_Gasm::delayMillis(uint msecs) {
	_spins = 0;
	_millis += msecs;
	((DefaultTimerManager *)_timerManager)->checkTimers();
	while (_millis >= _nextFrame && !_quitRequested) {
		endFrame();
		gasm_scummvm_yield();     // back in the next gasm_frame
		beginFrame();
		_nextFrame += 1000.0 / kFrameRate;
	}
}

void OSystem_Gasm::getTimeAndDate(TimeDate &td, bool) const {
	// virtual too (reproducible saves and random seeds): 2026-01-01 + run time
	uint32 s = (uint32)(_millis / 1000);
	td.tm_sec = s % 60;
	td.tm_min = s / 60 % 60;
	td.tm_hour = s / 3600 % 24;
	td.tm_mday = 1 + s / 86400 % 28;
	td.tm_mon = 0;
	td.tm_year = 126;
	td.tm_wday = 4;
}

void OSystem_Gasm::quit() {
	_quitRequested = true;
}

void OSystem_Gasm::logMessage(LogMessageType::Type, const char *message) {
	size_t n = strlen(message);
	while (n && message[n - 1] == '\n')
		n--;
	gasm_log(message, (uint32)n);
}

void OSystem_Gasm::addSysArchivesToSearchSet(Common::SearchSet &s, int priority) {
	// engine data built into the module (sky.cpt, ...)
	s.add("gasm-embedded", new GasmEmbeddedArchive(), priority);
	// more engine data and themes, if the player provides them as assets under scummvm/
	Common::FSNode data("scummvm");
	if (data.exists() && data.isDirectory())
		s.add("scummvm-data", new Common::FSDirectory(data, 4), priority);
}

Common::Path OSystem_Gasm::getDefaultConfigFileName() {
	return Common::Path(kConfigKey);
}

Common::SeekableReadStream *OSystem_Gasm::createConfigReadStream() {
	return gasmStorageRead(kConfigKey);
}

Common::WriteStream *OSystem_Gasm::createConfigWriteStream() {
	return gasmStorageWrite(kConfigKey);
}

void OSystem_Gasm::beginFrame() {
	_frames++;
	collectKeys();
	collectPointer();
	collectGamepads();
}

void OSystem_Gasm::endFrame() {
	((GasmMixerManager *)_mixerManager)->mixFrame();
}

bool OSystem_Gasm::pollEvent(Common::Event &event) {
	((DefaultTimerManager *)_timerManager)->checkTimers();
	if (_quitRequested && _events.empty()) {
		event = Common::Event();
		event.type = Common::EVENT_QUIT;
		_quitRequested = false;
		return true;
	}
	if (_events.empty())
		return false;
	event = _events.pop();
	return true;
}

// ---- keyboard ---------------------------------------------------------------------------

static bool held(const uint8 *keys, uint32 k) { return keys[k / 8] >> (k % 8) & 1; }

/** ScummVM key and the character it types on a US layout (0 if none). */
static Common::KeyCode keyFor(uint32 k, bool shift, uint16 &ascii) {
	using namespace Common;
	ascii = 0;
	if (k >= GASM_KEY_KEY_A && k <= GASM_KEY_KEY_Z) {
		ascii = (uint16)((shift ? 'A' : 'a') + (k - GASM_KEY_KEY_A));
		return (KeyCode)(KEYCODE_a + (k - GASM_KEY_KEY_A));
	}
	if (k >= GASM_KEY_DIGIT0 && k <= GASM_KEY_DIGIT9) {
		static const char shifted[] = ")!@#$%^&*(";
		int d = k - GASM_KEY_DIGIT0;
		ascii = (uint16)(shift ? shifted[d] : '0' + d);
		return (KeyCode)(KEYCODE_0 + d);
	}
	if (k >= GASM_KEY_F1 && k <= GASM_KEY_F12) {
		ascii = (uint16)(ASCII_F1 + (k - GASM_KEY_F1));
		return (KeyCode)(KEYCODE_F1 + (k - GASM_KEY_F1));
	}
	if (k >= GASM_KEY_NUMPAD0 && k <= GASM_KEY_NUMPAD9) {
		ascii = (uint16)('0' + (k - GASM_KEY_NUMPAD0));
		return (KeyCode)(KEYCODE_KP0 + (k - GASM_KEY_NUMPAD0));
	}
	struct Map { uint32 gasm; KeyCode key; char plain, shifted; };
	static const Map table[] = {
		{ GASM_KEY_ESCAPE, KEYCODE_ESCAPE, 27, 27 }, { GASM_KEY_ENTER, KEYCODE_RETURN, 13, 13 },
		{ GASM_KEY_NUMPAD_ENTER, KEYCODE_KP_ENTER, 13, 13 }, { GASM_KEY_BACKSPACE, KEYCODE_BACKSPACE, 8, 8 },
		{ GASM_KEY_TAB, KEYCODE_TAB, 9, 9 }, { GASM_KEY_SPACE, KEYCODE_SPACE, ' ', ' ' },
		{ GASM_KEY_MINUS, KEYCODE_MINUS, '-', '_' }, { GASM_KEY_EQUAL, KEYCODE_EQUALS, '=', '+' },
		{ GASM_KEY_BRACKET_LEFT, KEYCODE_LEFTBRACKET, '[', '{' }, { GASM_KEY_BRACKET_RIGHT, KEYCODE_RIGHTBRACKET, ']', '}' },
		{ GASM_KEY_BACKSLASH, KEYCODE_BACKSLASH, '\\', '|' }, { GASM_KEY_SEMICOLON, KEYCODE_SEMICOLON, ';', ':' },
		{ GASM_KEY_QUOTE, KEYCODE_QUOTE, '\'', '"' }, { GASM_KEY_BACKQUOTE, KEYCODE_BACKQUOTE, '`', '~' },
		{ GASM_KEY_COMMA, KEYCODE_COMMA, ',', '<' }, { GASM_KEY_PERIOD, KEYCODE_PERIOD, '.', '>' },
		{ GASM_KEY_SLASH, KEYCODE_SLASH, '/', '?' },
		{ GASM_KEY_ARROW_UP, KEYCODE_UP, 0, 0 }, { GASM_KEY_ARROW_DOWN, KEYCODE_DOWN, 0, 0 },
		{ GASM_KEY_ARROW_LEFT, KEYCODE_LEFT, 0, 0 }, { GASM_KEY_ARROW_RIGHT, KEYCODE_RIGHT, 0, 0 },
		{ GASM_KEY_INSERT, KEYCODE_INSERT, 0, 0 }, { GASM_KEY_DELETE, KEYCODE_DELETE, 127, 127 },
		{ GASM_KEY_HOME, KEYCODE_HOME, 0, 0 }, { GASM_KEY_END, KEYCODE_END, 0, 0 },
		{ GASM_KEY_PAGE_UP, KEYCODE_PAGEUP, 0, 0 }, { GASM_KEY_PAGE_DOWN, KEYCODE_PAGEDOWN, 0, 0 },
		{ GASM_KEY_SHIFT_LEFT, KEYCODE_LSHIFT, 0, 0 }, { GASM_KEY_SHIFT_RIGHT, KEYCODE_RSHIFT, 0, 0 },
		{ GASM_KEY_CONTROL_LEFT, KEYCODE_LCTRL, 0, 0 }, { GASM_KEY_CONTROL_RIGHT, KEYCODE_RCTRL, 0, 0 },
		{ GASM_KEY_ALT_LEFT, KEYCODE_LALT, 0, 0 }, { GASM_KEY_ALT_RIGHT, KEYCODE_RALT, 0, 0 },
		{ GASM_KEY_META_LEFT, KEYCODE_LMETA, 0, 0 }, { GASM_KEY_META_RIGHT, KEYCODE_RMETA, 0, 0 },
		{ GASM_KEY_CAPS_LOCK, KEYCODE_CAPSLOCK, 0, 0 }, { GASM_KEY_PAUSE, KEYCODE_PAUSE, 0, 0 },
		{ GASM_KEY_NUMPAD_ADD, KEYCODE_KP_PLUS, '+', '+' }, { GASM_KEY_NUMPAD_SUBTRACT, KEYCODE_KP_MINUS, '-', '-' },
		{ GASM_KEY_NUMPAD_MULTIPLY, KEYCODE_KP_MULTIPLY, '*', '*' }, { GASM_KEY_NUMPAD_DIVIDE, KEYCODE_KP_DIVIDE, '/', '/' },
		{ GASM_KEY_NUMPAD_DECIMAL, KEYCODE_KP_PERIOD, '.', '.' },
	};
	for (const Map &m : table) {
		if (m.gasm == k) {
			ascii = (uint16)(uint8)(shift ? m.shifted : m.plain);
			return m.key;
		}
	}
	return KEYCODE_INVALID;
}

void OSystem_Gasm::pushKey(uint32 k, bool down) {
	bool shift = held(_keys, GASM_KEY_SHIFT_LEFT) || held(_keys, GASM_KEY_SHIFT_RIGHT);
	byte flags = (shift ? Common::KBD_SHIFT : 0) |
	             (held(_keys, GASM_KEY_CONTROL_LEFT) || held(_keys, GASM_KEY_CONTROL_RIGHT) ? Common::KBD_CTRL : 0) |
	             (held(_keys, GASM_KEY_ALT_LEFT) || held(_keys, GASM_KEY_ALT_RIGHT) ? Common::KBD_ALT : 0) |
	             (held(_keys, GASM_KEY_META_LEFT) || held(_keys, GASM_KEY_META_RIGHT) ? Common::KBD_META : 0);
	uint16 ascii;
	Common::KeyCode code = keyFor(k, shift, ascii);
	if (code == Common::KEYCODE_INVALID)
		return;
	Common::Event e;
	e.type = down ? Common::EVENT_KEYDOWN : Common::EVENT_KEYUP;
	e.kbd = Common::KeyState(code, ascii, flags);
	_events.push(e);
}

void OSystem_Gasm::collectKeys() {
	uint8 ev[256 * 4];
	int n = gasm_key_events(ev, sizeof ev);
	gasm_key_state(_keys, sizeof _keys);
	for (int i = 0; i + 3 < n; i += 4)
		pushKey(ev[i] | ev[i + 1] << 8, ev[i + 2] != 0);
}

// ---- pointer ------------------------------------------------------------------------------

void OSystem_Gasm::collectPointer() {
	uint8 p[GASM_POINTER_BYTES];
	if (gasm_pointer(p, sizeof p) < 0)
		return;
	float fx, fy, wheel;
	uint32 pressed, released;
	memcpy(&fx, p + GASM_POINTER_OFF_FX, 4);
	memcpy(&fy, p + GASM_POINTER_OFF_FY, 4);
	memcpy(&wheel, p + GASM_POINTER_OFF_WHEEL_Y, 4);
	memcpy(&pressed, p + GASM_POINTER_OFF_PRESSED, 4);
	memcpy(&released, p + GASM_POINTER_OFF_RELEASED, 4);
	GasmGraphicsManager *gfx = (GasmGraphicsManager *)_graphicsManager;
	Common::Point pos = gfx->frameToScreen(fx, fy);
	Common::Event e;
	if (pos != gfx->mousePosition()) {
		gfx->setMousePosition(pos);
		e.type = Common::EVENT_MOUSEMOVE;
		e.mouse = pos;
		_events.push(e);
	}
	static const struct { uint32 bit; Common::EventType down, up; } buttons[] = {
		{ GASM_MOUSE_LEFT, Common::EVENT_LBUTTONDOWN, Common::EVENT_LBUTTONUP },
		{ GASM_MOUSE_RIGHT, Common::EVENT_RBUTTONDOWN, Common::EVENT_RBUTTONUP },
		{ GASM_MOUSE_MIDDLE, Common::EVENT_MBUTTONDOWN, Common::EVENT_MBUTTONUP },
		{ GASM_MOUSE_BACK, Common::EVENT_X1BUTTONDOWN, Common::EVENT_X1BUTTONUP },
		{ GASM_MOUSE_FORWARD, Common::EVENT_X2BUTTONDOWN, Common::EVENT_X2BUTTONUP },
	};
	for (const auto &b : buttons) {
		// a click inside one frame is both pressed and released
		if (pressed & b.bit) {
			e = Common::Event();
			e.type = b.down;
			e.mouse = pos;
			_events.push(e);
		}
		if (released & b.bit) {
			e = Common::Event();
			e.type = b.up;
			e.mouse = pos;
			_events.push(e);
		}
	}
	for (int i = 0; i < (int)(wheel > 0 ? wheel + 0.5f : -wheel + 0.5f); i++) {
		e = Common::Event();
		e.type = wheel > 0 ? Common::EVENT_WHEELDOWN : Common::EVENT_WHEELUP;
		e.mouse = pos;
		_events.push(e);
	}
}

// ---- gamepads -----------------------------------------------------------------------------

void OSystem_Gasm::collectGamepads() {
	uint8 g[GASM_GAMEPAD_BYTES];
	if (gasm_gamepad(0, g, sizeof g) < 0)
		return;
	uint32 flags, nb, na;
	memcpy(&flags, g, 4);
	memcpy(&nb, g + GASM_GAMEPAD_OFF_BUTTON_COUNT, 4);
	memcpy(&na, g + GASM_GAMEPAD_OFF_AXIS_COUNT, 4);
	if (!(flags & GASM_GAMEPAD_CONNECTED) || !(flags & GASM_GAMEPAD_STANDARD))
		return;
	// W3C standard mapping -> ScummVM joystick buttons
	static const int8 map[17] = {
		Common::JOYSTICK_BUTTON_A, Common::JOYSTICK_BUTTON_B, Common::JOYSTICK_BUTTON_X, Common::JOYSTICK_BUTTON_Y,
		Common::JOYSTICK_BUTTON_LEFT_SHOULDER, Common::JOYSTICK_BUTTON_RIGHT_SHOULDER, -1, -1,
		Common::JOYSTICK_BUTTON_BACK, Common::JOYSTICK_BUTTON_START, Common::JOYSTICK_BUTTON_LEFT_STICK,
		Common::JOYSTICK_BUTTON_RIGHT_STICK, Common::JOYSTICK_BUTTON_DPAD_UP, Common::JOYSTICK_BUTTON_DPAD_DOWN,
		Common::JOYSTICK_BUTTON_DPAD_LEFT, Common::JOYSTICK_BUTTON_DPAD_RIGHT, Common::JOYSTICK_BUTTON_GUIDE,
	};
	uint32 buttons = 0;
	for (uint32 i = 0; i < nb && i < 17; i++) {
		float v;
		memcpy(&v, g + GASM_GAMEPAD_OFF_BUTTONS + i * 4, 4);
		if (v > 0.5f)
			buttons |= 1u << i;
	}
	for (int i = 0; i < 17; i++) {
		if (map[i] < 0 || !((buttons ^ _gamepadButtons) >> i & 1))
			continue;
		Common::Event e;
		e.type = buttons >> i & 1 ? Common::EVENT_JOYBUTTON_DOWN : Common::EVENT_JOYBUTTON_UP;
		e.joystick.button = (uint8)map[i];
		_events.push(e);
	}
	_gamepadButtons = buttons;
	static const uint8 axes[4] = { Common::JOYSTICK_AXIS_LEFT_STICK_X, Common::JOYSTICK_AXIS_LEFT_STICK_Y,
	                               Common::JOYSTICK_AXIS_RIGHT_STICK_X, Common::JOYSTICK_AXIS_RIGHT_STICK_Y };
	for (uint32 i = 0; i < 4 && i < na; i++) {
		float v;
		memcpy(&v, g + GASM_GAMEPAD_OFF_AXES + i * 4, 4);
		if (v == _gamepadAxes[i])
			continue;
		_gamepadAxes[i] = v;
		Common::Event e;
		e.type = Common::EVENT_JOYAXIS_MOTION;
		e.joystick.axis = axes[i];
		e.joystick.position = (int16)(v * 32767.0f);
		_events.push(e);
	}
}
