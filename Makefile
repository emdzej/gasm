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

GUESTS   := $(BUILD)/test-pattern.wasm $(BUILD)/nes.wasm $(BUILD)/sumo.wasm $(BUILD)/triangle.wasm $(BUILD)/assetcheck.wasm

.PHONY: all guests native test web relay roms parity clean rust-toolchain
all: guests native

guests: $(GUESTS)

$(WASI_SDK)/bin/clang:
	scripts/fetch-wasi-sdk.sh

rust-toolchain:
	@test -n "$(CARGO_W)" || { echo "rustup toolchain '$(TOOLCHAIN)' not found; install rustup, then: rustup toolchain install $(TOOLCHAIN) --target wasm32-unknown-unknown"; exit 1; }
	@$(RUSTUP) target list --installed --toolchain $(TOOLCHAIN) | grep -q wasm32-unknown-unknown || $(RUSTUP) target add --toolchain $(TOOLCHAIN) wasm32-unknown-unknown

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
