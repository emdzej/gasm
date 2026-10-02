/*
 * ScummVM on gasm: the entry points, on gasm_loop (sdk/c/include/gasm_loop.h).
 *
 * ScummVM's engines run their own loops. gasm_loop.c (copied next to this file by
 * scripts/fetch-scummvm.sh) provides the gasm exports, runs gasm_main() and
 * suspends it whenever OSystem_Gasm::delayMillis reaches a frame boundary and
 * calls gasm_wait_frame(). The module is post-processed with wasm-opt --asyncify
 * (see the Makefile).
 *
 * Params: "args" is a ScummVM command line, e.g. "-p / sky" or "--auto-detect -p /".
 *
 * SPDX-License-Identifier: MIT (this file; the ScummVM build as a whole is GPL-3.0)
 */
#define FORBIDDEN_SYMBOL_ALLOW_ALL
#include "common/scummsys.h"

#include "backends/platform/gasm/gasm-system.h"
#include "backends/platform/gasm/gasm_loop.h"
#include "base/main.h"
#include "common/config-manager.h"

#include <string.h>

#include "gasm.h"

static const int kMaxArgs = 64;
static char argBuf[2048];
static const char *argv[kMaxArgs];
static int argc;

void gasm_scummvm_yield() {
	gasm_wait_frame();
}

extern "C" int gasm_loop_init(void) {
	argc = 0;
	argv[argc++] = "scummvm";
	if (gasm_param_str("args", argBuf, sizeof argBuf)) {
		for (char *t = strtok(argBuf, " \t"); t && argc < kMaxArgs - 1; t = strtok(nullptr, " \t"))
			argv[argc++] = t;
	}
	argv[argc] = nullptr;
	g_system = new OSystem_Gasm();
	return 0;
}

GASM_TITLE("ScummVM");   // until a game starts (setWindowCaption)

extern "C" int gasm_main(void) {
	return scummvm_main(argc, argv);
}

/** The player is quitting: keep the settings (save games are stored when written). */
extern "C" void gasm_loop_exit(void) {
	if (Common::ConfigManager::hasInstance())
		ConfMan.flushToDisk();
}
