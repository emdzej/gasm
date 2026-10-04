/**************************************************************************/
/*  os_gasm.cpp                                                           */
/**************************************************************************/
/* Godot on gasm (https://gasm.emdzej.pl): MIT, like Godot.                */
#include "os_gasm.h"

#include "display_server_gasm.h"
#include "file_access_gasm.h"
#include "ip_gasm.h"

#include "core/debugger/engine_debugger.h"
#include "core/io/dir_access.h"
#include "core/io/file_access.h"
#include "core/os/main_loop.h"
#include "main/main.h"

#include "gasm.h"

#include <time.h>
#include <unistd.h>

void OS_Gasm::initialize() {
	// res:// is the game's .pck (a gasm asset), user:// is gasm:storage: one file system
	FileAccess::make_default<FileAccessGasm>(FileAccess::ACCESS_RESOURCES);
	FileAccess::make_default<FileAccessGasm>(FileAccess::ACCESS_USERDATA);
	FileAccess::make_default<FileAccessGasm>(FileAccess::ACCESS_FILESYSTEM);
	DirAccess::make_default<DirAccessGasm>(DirAccess::ACCESS_RESOURCES);
	DirAccess::make_default<DirAccessGasm>(DirAccess::ACCESS_USERDATA);
	DirAccess::make_default<DirAccessGasm>(DirAccess::ACCESS_FILESYSTEM);
	DisplayServerGasm::register_gasm_driver();
}

void OS_Gasm::set_main_loop(MainLoop *p_main_loop) {
	main_loop = p_main_loop;
}

void OS_Gasm::delete_main_loop() {
	if (main_loop) {
		memdelete(main_loop);
	}
	main_loop = nullptr;
}

void OS_Gasm::finalize() {
	delete_main_loop();
}

bool OS_Gasm::_check_internal_feature_support(const String &p_feature) {
	return p_feature == "gasm" || p_feature == "wasm32" || p_feature == "single_threaded";
}

bool OS_Gasm::main_loop_iterate() {
	DisplayServer::get_singleton()->process_events();
	return Main::iteration();
}

Error OS_Gasm::get_entropy(uint8_t *r_buffer, int p_bytes) {
	// WASI random_get: the OS's natively, a fixed sequence in headless runs
	while (p_bytes > 0) {
		int n = MIN(p_bytes, 256);
		if (getentropy(r_buffer, n) != 0) {
			return FAILED;
		}
		r_buffer += n;
		p_bytes -= n;
	}
	return OK;
}

uint64_t OS_Gasm::get_ticks_usec() const {
	// the frame's time (virtual in headless runs, so they are reproducible)
	return (uint64_t)(gasm_time_ms() * 1000.0);
}

double OS_Gasm::get_unix_time() const {
	struct timespec ts;
	clock_gettime(CLOCK_REALTIME, &ts);
	return (double)ts.tv_sec + (double)ts.tv_nsec / 1e9;
}

// The player's time zone (gasm.utc_offset_minutes; UTC in headless runs and on older runners).
static int utc_offset_minutes() {
	static const bool has = gasm_has_str("gasm.utc_offset_minutes") == 1;
	return has ? gasm_utc_offset_minutes() : 0;
}

OS::DateTime OS_Gasm::get_datetime(bool p_utc) const {
	time_t t = time(nullptr);
	if (!p_utc) {
		t += (time_t)utc_offset_minutes() * 60;
	}
	struct tm lt;
	gmtime_r(&t, &lt); // WASI's libc has no time zones: shift the time, then read it as UTC
	DateTime ret;
	ret.year = 1900 + lt.tm_year;
	ret.month = (Month)(lt.tm_mon + 1);
	ret.day = lt.tm_mday;
	ret.weekday = (Weekday)lt.tm_wday;
	ret.hour = lt.tm_hour;
	ret.minute = lt.tm_min;
	ret.second = lt.tm_sec;
	ret.dst = false; // the offset includes daylight saving, but gasm doesn't say whether it applies
	return ret;
}

OS::TimeZoneInfo OS_Gasm::get_time_zone_info() const {
	int bias = utc_offset_minutes();
	TimeZoneInfo ret;
	ret.bias = bias;
	ret.name = bias == 0 ? String("UTC") : vformat("UTC%s%02d:%02d", bias < 0 ? "-" : "+", Math::abs(bias) / 60, Math::abs(bias) % 60);
	return ret;
}

OS_Gasm::OS_Gasm() {
	IPGasm::make_default();
	AudioDriverManager::add_driver(&audio_driver);
	Vector<Logger *> loggers;
	loggers.push_back(memnew(StdLogger));
	_set_logger(memnew(CompositeLogger(loggers)));
}
