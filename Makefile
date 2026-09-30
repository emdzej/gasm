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

BUILD    := build

# --- Rust guests (guests/: gasm crate, sumo, nes, triangle) -----------------------
# Rust guests need a rustup toolchain with the wasm32-unknown-unknown target
# (Homebrew's rust has no wasm targets). Linking uses wasi-sdk's wasm-ld.
RUSTUP   ?= $(shell command -v rustup 2>/dev/null || echo /opt/homebrew/opt/rustup/bin/rustup)
TOOLCHAIN ?= stable
RUSTC_W  := $(shell $(RUSTUP) which --toolchain $(TOOLCHAIN) rustc 2>/dev/null)
CARGO_W  := $(shell $(RUSTUP) which --toolchain $(TOOLCHAIN) cargo 2>/dev/null)
RUST_OUT := guests/target/wasm32-unknown-unknown/release
RUST_SRC := $(shell find guests/gasm guests/sumo guests/nes guests/triangle guests/assetcheck -name '*.rs' -o -name Cargo.toml) guests/Cargo.toml

# --- C guest (guests/test-pattern) via wasi-sdk -------------------------------------
WASI_SDK ?= $(CURDIR)/tools/wasi-sdk
CC       := $(WASI_SDK)/bin/clang
TARGET   := --target=wasm32-wasip1
REACTOR  := -mexec-model=reactor
OPT      := -O2 -DNDEBUG

GUESTS   := $(BUILD)/test-pattern.wasm $(BUILD)/nes.wasm $(BUILD)/sumo.wasm $(BUILD)/triangle.wasm $(BUILD)/assetcheck.wasm $(BUILD)/doom.wasm

.PHONY: all guests native test web relay roms parity clean rust-toolchain doom
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

$(BUILD)/test-pattern.wasm: guests/test-pattern/main.c spec/gasm.h | $(WASI_SDK)/bin/clang
	@mkdir -p $(@D)
	$(CC) $(TARGET) $(REACTOR) $(OPT) -Ispec $< -o $@ -lm

# One cargo invocation builds all Rust games (stamp file: portable to make 3.81).
$(BUILD)/.rust-guests: $(RUST_SRC) | rust-toolchain $(WASI_SDK)/bin/clang
	@mkdir -p $(@D)
	cd guests && RUSTC=$(RUSTC_W) CARGO_TARGET_WASM32_UNKNOWN_UNKNOWN_LINKER=$(WASI_SDK)/bin/wasm-ld \
	  $(CARGO_W) build --release --target wasm32-unknown-unknown -p sumo -p nes -p triangle -p assetcheck
	@touch $@

$(RUST_OUT)/%.wasm: $(BUILD)/.rust-guests ;

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
