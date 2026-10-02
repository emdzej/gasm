# gasm — build guests (wasm) and runners.
#
#   make            build all guests + native runner
#   make guests     build build/*.wasm
#   make native     build runners/native (gasm-run, gasm-relay)
#   make test       cross-runner determinism + network tests
#   make web        serve the web runner on http://localhost:8080/runners/web/
#   make relay      run gasm-relay on ws://0.0.0.0:9000
#   make parity     NES guest built natively vs as wasm (ROM=, FRAMES=)
#   make roms       fetch freely distributable test ROMs into roms/
#   make doom       build/doom.wasm (fetches the GPL-2.0 engine into tools/doom-src)
#   make scummvm    build/scummvm.wasm (fetches ScummVM, GPL-3.0, into tools/scummvm-src; Asyncify)
#   make sdl3       build/sdl3: SDL3 for gasm (libSDL3.a + headers; SDL fetched into tools/SDL3-src)

BUILD    := build

# --- Rust guests (guests/: gasm crate, sumo, nes, triangle) -----------------------
# Rust guests need a rustup toolchain with the wasm32-unknown-unknown target
# (Homebrew's rust has no wasm targets). Linking uses wasi-sdk's wasm-ld.
RUSTUP   ?= $(shell command -v rustup 2>/dev/null || echo /opt/homebrew/opt/rustup/bin/rustup)
TOOLCHAIN ?= stable
RUSTC_W  := $(shell $(RUSTUP) which --toolchain $(TOOLCHAIN) rustc 2>/dev/null)
CARGO_W  := $(shell $(RUSTUP) which --toolchain $(TOOLCHAIN) cargo 2>/dev/null)
RUST_OUT := guests/target/wasm32-unknown-unknown/release
RUST_SRC := $(shell find guests/gasm guests/sumo guests/nes guests/triangle guests/textured guests/inputtest guests/loopdemo guests/assetcheck -name '*.rs' -o -name Cargo.toml) guests/Cargo.toml

# --- C guest (guests/test-pattern) via wasi-sdk -------------------------------------
WASI_SDK ?= $(CURDIR)/tools/wasi-sdk
CC       := $(WASI_SDK)/bin/clang
TARGET   := --target=wasm32-wasip1
REACTOR  := -mexec-model=reactor
OPT      := -O2 -DNDEBUG

GUESTS   := $(BUILD)/test-pattern.wasm $(BUILD)/nes.wasm $(BUILD)/sumo.wasm $(BUILD)/triangle.wasm $(BUILD)/textured.wasm $(BUILD)/inputtest.wasm $(BUILD)/loopdemo.wasm $(BUILD)/loopdemo-c.wasm $(BUILD)/assetcheck.wasm $(BUILD)/doom.wasm $(BUILD)/scummvm.wasm \
  $(BUILD)/sdl3-snake.wasm $(BUILD)/sdl3-woodeneye.wasm $(BUILD)/sdl3-callbacks.wasm $(BUILD)/sdl3-classic.wasm

.PHONY: all guests native test web relay roms parity clean rust-toolchain doom scummvm sdl3
all: guests native

guests: $(GUESTS)

$(WASI_SDK)/bin/clang:
	scripts/fetch-wasi-sdk.sh

rust-toolchain:
	@test -n "$(CARGO_W)" || { echo "rustup toolchain '$(TOOLCHAIN)' not found; install rustup, then: rustup toolchain install $(TOOLCHAIN) --target wasm32-unknown-unknown"; exit 1; }
	@$(RUSTUP) target list --installed --toolchain $(TOOLCHAIN) | grep -q wasm32-unknown-unknown || $(RUSTUP) target add --toolchain $(TOOLCHAIN) wasm32-unknown-unknown

# --- DOOM (guests/doom): doomgeneric + chocolate-doom's OPL music, fetched at build ---
DOOM_SRC  := tools/doom-src
DOOM_ENGINE := am_map d_event d_items d_iwad d_loop d_main d_mode d_net doomdef doomgeneric \
  doomstat dstrings dummy f_finale f_wipe g_game hu_lib hu_stuff i_cdmus i_endoom i_input \
  i_joystick i_scale i_sound i_system i_timer i_video info m_argv m_bbox m_cheat m_config \
  m_controls m_fixed m_menu m_misc m_random memio mus2mid p_ceilng p_doors p_enemy p_floor \
  p_inter p_lights p_map p_maputl p_mobj p_plats p_pspr p_saveg p_setup p_sight p_spec \
  p_switch p_telept p_tick p_user r_bsp r_data r_draw r_main r_plane r_segs r_sky r_things \
  s_sound sha1 sounds st_lib st_stuff statdump tables v_video w_checksum w_file w_file_stdc \
  w_main w_wad wi_stuff z_zone i_oplmusic midifile opl_queue dbopl
DOOM_GLUE := guests/doom/gasm_doom.c guests/doom/gasm_opl.c
DOOM_DEFS := -DCMAP256 -DDOOMGENERIC_RESX=320 -DDOOMGENERIC_RESY=200 -DFEATURE_SOUND \
  -D_GNU_SOURCE -DGASM=1 -Dmusic_opl_module=DG_music_module
DOOM_WARN := -Wno-implicit-function-declaration -Wno-int-conversion -Wno-incompatible-pointer-types \
  -Wno-pointer-sign -Wno-dangling-else -Wno-parentheses -Wno-format -Wno-unused-value \
  -Wno-return-type -Wno-deprecated-non-prototype -Wno-string-concatenation -Wno-absolute-value

doom: $(BUILD)/doom.wasm

$(DOOM_SRC)/.version: scripts/fetch-doom.sh guests/doom/engine.patch
	scripts/fetch-doom.sh

$(BUILD)/doom.wasm: $(DOOM_GLUE) guests/doom/gasm_doom.h guests/doom/prelude.h spec/gasm.h $(DOOM_SRC)/.version | $(WASI_SDK)/bin/clang
	@mkdir -p $(@D)
	$(CC) $(TARGET) $(REACTOR) $(OPT) $(DOOM_DEFS) $(DOOM_WARN) -include guests/doom/prelude.h \
	  -Iguests/doom/shim -Iguests/doom -I$(DOOM_SRC) -Ispec \
	  $(DOOM_GLUE) $(addprefix $(DOOM_SRC)/,$(addsuffix .c,$(DOOM_ENGINE))) \
	  -Wl,-z,stack-size=1048576 -o $@ -lm

# --- ScummVM (guests/scummvm): GPL-3.0 sources fetched at build, gasm backend + Asyncify ---
SCUMMVM_SRC     := tools/scummvm-src
WASM_OPT        ?= $(CURDIR)/tools/binaryen/bin/wasm-opt
SCUMMVM_ENGINES ?= sky scumm scumm_7_8 he drascula
# __DATE__/__TIME__ in ScummVM's version string: the release date, so builds are
# reproducible (and the in-game menu, which shows it, hashes the same everywhere)
SCUMMVM_DATE    ?= 1774224000
# engine data built into scummvm.wasm (from ScummVM's dists/engine-data)
SCUMMVM_DATA    ?= sky.cpt drascula.dat
# zlib, MP3 (libmad), Ogg Vorbis, FLAC: built for wasm32 by scripts/build-scummvm-libs.sh
SCUMMVM_LIBS := $(CURDIR)/tools/scummvm-libs
SCUMMVM_CONFIG  := --host=wasm32-gasm --backend=gasm --disable-all-engines \
  $(addprefix --enable-engine=,$(SCUMMVM_ENGINES)) --enable-release --disable-debug \
  --disable-mt32emu --disable-timidity --disable-seq-midi --disable-fluidsynth --disable-fluidlite \
  --disable-sonivox --disable-tremor --disable-faad --disable-png --disable-jpeg --disable-gif --disable-freetype2 \
  --disable-fribidi --disable-theoradec --disable-vpx --disable-libcurl --disable-sdlnet \
  --disable-enet --disable-cloud --disable-tts --disable-taskbar --disable-discord --disable-readline \
  --disable-lua --disable-tinygl --disable-opengl-game --disable-system-dialogs \
  --disable-eventrecorder --disable-translation \
  $(foreach l,zlib mad vorbis ogg flac,--enable-$(l) --with-$(l)-prefix=$(SCUMMVM_LIBS))
SCUMMVM_SRCS := $(wildcard guests/scummvm/backend/*) guests/scummvm/configure.patch sdk/c/src/gasm_loop.c sdk/c/include/gasm_loop.h

scummvm: $(BUILD)/scummvm.wasm

$(WASM_OPT):
	scripts/fetch-binaryen.sh

$(SCUMMVM_SRC)/.gasm-version: scripts/fetch-scummvm.sh $(SCUMMVM_SRCS)
	scripts/fetch-scummvm.sh

# configure again only when the patch, the engines or the flags change
$(SCUMMVM_LIBS)/.gasm-libs: scripts/build-scummvm-libs.sh | $(WASI_SDK)/bin/clang
	scripts/build-scummvm-libs.sh

$(SCUMMVM_SRC)/config.mk: $(SCUMMVM_SRC)/.gasm-version $(SCUMMVM_LIBS)/.gasm-libs Makefile | $(WASI_SDK)/bin/clang
	cd $(SCUMMVM_SRC) && CXX="$(WASI_SDK)/bin/clang++ --target=wasm32-wasip1 --sysroot=$(WASI_SDK)/share/wasi-sysroot" \
	  CC="$(WASI_SDK)/bin/clang --target=wasm32-wasip1 --sysroot=$(WASI_SDK)/share/wasi-sysroot" \
	  AR="$(WASI_SDK)/bin/llvm-ar" RANLIB="$(WASI_SDK)/bin/llvm-ranlib" STRIP="$(WASI_SDK)/bin/llvm-strip" \
	  CXXFLAGS="-I$(CURDIR)/spec -fno-exceptions -DGASM_LOOP_STACK_SIZE=4194304" LDFLAGS="-mexec-model=reactor -Wl,-z,stack-size=8388608 -Wl,--wrap=exit" \
	  ./configure $(SCUMMVM_CONFIG)

# Asyncify after linking: gasm_loop_frame is the frame that must not unwind (sdk/c/src/gasm_loop.c)
$(SCUMMVM_SRC)/backends/platform/gasm/gasm-data.cpp: $(SCUMMVM_SRC)/.gasm-version scripts/embed-files.mjs Makefile
	node scripts/embed-files.mjs $@ $(foreach f,$(SCUMMVM_DATA),$(f)=$(SCUMMVM_SRC)/dists/engine-data/$(f))

$(BUILD)/scummvm.wasm: $(SCUMMVM_SRC)/config.mk $(SCUMMVM_SRCS) $(SCUMMVM_SRC)/backends/platform/gasm/gasm-data.cpp | $(WASM_OPT)
	SOURCE_DATE_EPOCH=$(SCUMMVM_DATE) $(MAKE) -C $(SCUMMVM_SRC) -j$$(getconf _NPROCESSORS_ONLN) scummvm
	@mkdir -p $(@D)
	$(WASM_OPT) $(SCUMMVM_SRC)/scummvm --asyncify --pass-arg=asyncify-removelist@gasm_loop_frame -O2 -o $@

# --- SDL3 (sdk/sdl3): SDL as a "private platform" with gasm drivers ------------------
# SDL (zlib) is fetched into tools/SDL3-src; sdk/sdl3 has the config and the drivers.
# Output: build/sdl3/lib/libSDL3.a (with the C SDK's gasm_loop for classic main())
# and build/sdl3/include/SDL3/.
SDL_SRC   := tools/SDL3-src
SDL_OUT   := $(BUILD)/sdl3
SDL_OBJ   := $(SDL_OUT)/obj
SDL_DIRS  := src src/atomic src/audio src/camera src/camera/dummy src/core src/cpuinfo \
  src/dialog src/dialog/dummy src/dynapi src/events src/filesystem src/gpu src/haptic \
  src/haptic/dummy src/hidapi src/io src/io/generic src/joystick src/joystick/virtual src/libm \
  src/loadso/dummy src/locale src/locale/dummy src/main src/misc src/misc/dummy src/power \
  src/process src/process/dummy src/render src/render/software src/sensor src/sensor/dummy \
  src/stdlib src/storage src/storage/generic src/thread src/thread/generic src/time src/timer \
  src/tray src/tray/dummy src/video src/video/yuv2rgb
SDL_SKIP  := src/audio/SDL_audiodev.c src/io/generic/SDL_asyncio_generic.c
SDL_GLUE  := $(wildcard sdk/sdl3/src/*.c)
SDL_OBJS   = $(patsubst $(SDL_SRC)/%.c,$(SDL_OBJ)/%.o,$(SDL_CSRC)) \
  $(patsubst sdk/sdl3/src/%.c,$(SDL_OBJ)/gasm/%.o,$(SDL_GLUE)) $(SDL_OBJ)/gasm/gasm_loop.o
SDL_CFLAGS := $(TARGET) $(OPT) -DSDL_PLATFORM_PRIVATE -D_GNU_SOURCE -Isdk/sdl3/include \
  -I$(SDL_SRC)/include -I$(SDL_SRC)/include/build_config -I$(SDL_SRC)/src -Ispec -Isdk/c/include \
  -Wno-deprecated-declarations
SDL_HDRS  := $(wildcard sdk/sdl3/include/*.h sdk/sdl3/src/*.h) spec/gasm.h
SDL_LIB   := $(SDL_OUT)/lib/libSDL3.a

sdl3: $(SDL_LIB)

$(SDL_SRC)/.gasm-3.4.16: scripts/fetch-sdl3.sh
	scripts/fetch-sdl3.sh

# The SDL files that are compiled (make reads this list, generated after the fetch)
$(SDL_OUT)/sources.mk: $(SDL_SRC)/.gasm-3.4.16 Makefile
	@mkdir -p $(@D)
	@echo "SDL_CSRC := $$(ls $(foreach d,$(SDL_DIRS),$(SDL_SRC)/$(d)/*.c) | grep -v -F $(foreach f,$(SDL_SKIP),-e $(f)) | tr '\n' ' ')" > $@
# (only for goals that build guests: `make clean`, `make native` etc. don't fetch SDL)
ifneq ($(if $(MAKECMDGOALS),$(filter-out clean native relay roms rust-toolchain doom scummvm,$(MAKECMDGOALS)),all),)
-include $(SDL_OUT)/sources.mk
endif

# SDL's files can't open anything by path on gasm: fopen goes to assets and storage
$(SDL_OBJ)/src/io/SDL_iostream.o: SDL_EXTRA := -Dfopen=SDL_GASM_fopen

$(SDL_OBJ)/%.o: $(SDL_SRC)/%.c $(SDL_HDRS) | $(WASI_SDK)/bin/clang
	@mkdir -p $(@D)
	@$(CC) $(SDL_CFLAGS) $(SDL_EXTRA) -c $< -o $@

$(SDL_OBJ)/gasm/%.o: sdk/sdl3/src/%.c $(SDL_HDRS) | $(WASI_SDK)/bin/clang
	@mkdir -p $(@D)
	$(CC) $(SDL_CFLAGS) -c $< -o $@

$(SDL_OBJ)/gasm/gasm_loop.o: sdk/c/src/gasm_loop.c sdk/c/include/gasm_loop.h | $(WASI_SDK)/bin/clang
	@mkdir -p $(@D)
	$(CC) $(TARGET) $(OPT) -Ispec -Isdk/c/include -c $< -o $@

$(SDL_LIB): $(SDL_OBJS) $(wildcard sdk/sdl3/cmake/*.cmake)
	@mkdir -p $(@D) $(SDL_OUT)/include/SDL3
	@rm -f $@ && $(WASI_SDK)/bin/llvm-ar rcs $@ $(SDL_OBJS) && echo "AR $@"
	cp $(SDL_SRC)/include/SDL3/*.h sdk/sdl3/include/SDL_main_private.h sdk/sdl3/include/SDL_main_impl_private.h \
	  sdk/c/include/gasm_loop.h spec/gasm.h $(SDL_OUT)/include/SDL3/
	@mkdir -p $(SDL_OUT)/lib/cmake/SDL3 && cp sdk/sdl3/cmake/*.cmake $(SDL_OUT)/lib/cmake/SDL3/

# SDL3 games: SDL's own demos (public domain) and sdk/sdl3/examples, unchanged
SDL_LINK = $(CC) $(TARGET) $(REACTOR) $(OPT) -Wl,--strip-debug -I$(SDL_OUT)/include $(1) $(SDL_LIB) -lm -o $@

$(BUILD)/sdl3-snake.wasm: $(SDL_SRC)/examples/demo/01-snake/snake.c $(SDL_LIB)
	$(call SDL_LINK,$<)

$(BUILD)/sdl3-woodeneye.wasm: $(SDL_SRC)/examples/demo/02-woodeneye-008/woodeneye-008.c $(SDL_LIB)
	$(call SDL_LINK,$<)

$(BUILD)/sdl3-callbacks.wasm: sdk/sdl3/examples/callbacks/main.c $(SDL_LIB)
	$(call SDL_LINK,$<)

# a classic main() loop: Asyncify, like any gasm_loop game
$(BUILD)/sdl3-classic.wasm: sdk/sdl3/examples/classic/main.c $(SDL_LIB) | $(WASM_OPT)
	$(call SDL_LINK,$<).raw
	$(WASM_OPT) $@.raw --asyncify --pass-arg=asyncify-removelist@gasm_loop_frame -O2 -o $@ && rm -f $@.raw

$(BUILD)/test-pattern.wasm: guests/test-pattern/main.c spec/gasm.h | $(WASI_SDK)/bin/clang
	@mkdir -p $(@D)
	$(CC) $(TARGET) $(REACTOR) $(OPT) -Ispec $< -o $@ -lm

# One cargo invocation builds all Rust games (stamp file: portable to make 3.81).
$(BUILD)/.rust-guests: $(RUST_SRC) | rust-toolchain $(WASI_SDK)/bin/clang
	@mkdir -p $(@D)
	cd guests && RUSTC=$(RUSTC_W) CARGO_TARGET_WASM32_UNKNOWN_UNKNOWN_LINKER=$(WASI_SDK)/bin/wasm-ld \
	  $(CARGO_W) build --release --target wasm32-unknown-unknown -p sumo -p nes -p triangle -p textured -p inputtest -p loopdemo -p assetcheck
	@touch $@

$(RUST_OUT)/%.wasm: $(BUILD)/.rust-guests ;

# own main loop (gasm::main_loop / gasm_loop.h): Asyncify after linking
$(BUILD)/loopdemo.wasm: $(RUST_OUT)/loopdemo.wasm | $(WASM_OPT)
	@mkdir -p $(@D)
	$(WASM_OPT) $< --asyncify --pass-arg=asyncify-removelist@gasm_frame,gasm_loop_frame -O2 -o $@

$(BUILD)/loopdemo-c.wasm: sdk/c/example-loop/main.c sdk/c/src/gasm_loop.c sdk/c/include/gasm_loop.h spec/gasm.h | $(WASI_SDK)/bin/clang $(WASM_OPT)
	@mkdir -p $(@D)
	$(CC) $(TARGET) $(REACTOR) $(OPT) -Ispec -Isdk/c/include sdk/c/example-loop/main.c sdk/c/src/gasm_loop.c -Wl,--wrap=exit -o $@.raw -lm
	$(WASM_OPT) $@.raw --asyncify --pass-arg=asyncify-removelist@gasm_loop_frame -O2 -o $@ && rm -f $@.raw

$(BUILD)/%.wasm: $(RUST_OUT)/%.wasm
	@mkdir -p $(@D)
	cp $< $@

native:
	cd runners/native && cargo build --release

roms:
	scripts/fetch-roms.sh

test: guests native
	scripts/determinism-test.sh
	scripts/net-test.sh

web: guests
	scripts/vendor-web.sh
	@echo "open http://localhost:8080/runners/web/  (OPFS import: /runners/web/opfs.html)"
	python3 -m http.server 8080

relay: native
	runners/native/target/release/gasm-relay 0.0.0.0:9000

# NES guest compiled natively (against the gasm crate's stub host) vs as wasm.
ROM ?= roms/cpu_instr_test.nes
FRAMES ?= 1200
parity: native guests
	cd guests && cargo build --release -p parity -q
	@echo "native Rust build:"; guests/target/release/parity $(ROM) $(FRAMES) 2>/dev/null
	@echo "wasm in gasm-run:";  runners/native/target/release/gasm-run $(BUILD)/nes.wasm --rom $(ROM) --headless $(FRAMES) 2>/dev/null

clean:
	rm -rf $(BUILD) guests/target
