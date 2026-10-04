/**************************************************************************/
/*  gasm_main.cpp                                                         */
/**************************************************************************/
/* Godot on gasm (https://gasm.emdzej.pl): MIT, like Godot.                */
/* The gasm exports. gasm_init runs Godot's setup with the game's pack (asset
 * "game.pck", or the "pck" param; "args" adds a Godot command line), each
 * gasm_frame is one Main::iteration() plus that frame's audio, and when the game
 * quits the module exits with its exit code. */
#include "display_server_gasm.h"
#include "os_gasm.h"

#include "core/config/engine.h"
#include "core/os/main_loop.h"
#include "main/main.h"

#include "gasm.h"

#include <stdlib.h>

static OS_Gasm *os = nullptr;
static bool started = false;
static Vector<CharString> arg_storage;

static String param(const char *p_name, const String &p_default) {
	int32_t n = gasm_param(p_name, strlen(p_name), nullptr, 0);
	if (n < 0) {
		return p_default;
	}
	CharString v;
	v.resize_uninitialized(n + 1);
	gasm_param(p_name, strlen(p_name), v.ptrw(), n);
	v.ptrw()[n] = 0;
	return String::utf8(v.ptr(), n);
}

static void finish() {
	if (started) {
		Main::cleanup();
		started = false;
	}
	int code = os ? os->get_exit_code() : 0;
	exit(code); // proc_exit: the runner ends the game
}

extern "C" {

__attribute__((export_name("gasm_abi_version"))) int32_t gasm_godot_abi_version(void) {
	return GASM_ABI_VERSION;
}

__attribute__((export_name("gasm_init"))) int32_t gasm_godot_init(void) {
	os = new OS_Gasm();
	Vector<String> args;
	args.push_back("--main-pack");
	args.push_back(param("pck", "game.pck"));
	for (const String &a : param("args", "").split(" ", false)) {
		args.push_back(a);
	}
	Vector<char *> argv;
	for (const String &a : args) {
		arg_storage.push_back(a.utf8());
	}
	for (CharString &c : arg_storage) {
		argv.push_back(c.ptrw());
	}
	Error err = Main::setup("godot", argv.size(), argv.ptrw());
	if (err != OK) {
		gasm_log_str(err == ERR_HELP ? "godot: done" : "godot: setup failed (is game.pck an asset?)");
		return err == ERR_HELP ? 0 : 1;
	}
	started = true;
	int rc = Main::start();
	os->set_exit_code(rc);
	if (rc != EXIT_SUCCESS || !os->get_main_loop()) {
		gasm_log_str("godot: start failed");
		return 1;
	}
	os->get_main_loop()->initialize();
	return 0;
}

__attribute__((export_name("gasm_frame"))) void gasm_godot_frame(void) {
	if (!started) {
		return;
	}
	if (os->main_loop_iterate()) {
		finish();
	}
	os->mix_audio(60.0);
}

__attribute__((export_name("gasm_exit"))) void gasm_godot_exit(void) {
	// the player is quitting: the game gets its close request (NOTIFICATION_WM_CLOSE_REQUEST,
	// where games save) and one more frame to handle it
	if (started) {
		DisplayServerGasm::get_singleton_gasm()->send_close_request();
		os->main_loop_iterate();
	}
}

} // extern "C"
