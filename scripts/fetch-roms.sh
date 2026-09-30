#!/usr/bin/env bash
# Fetch freely distributable NES test ROMs / homebrew demos into roms/ (git-ignored).
# Source: https://github.com/christopherpow/nes-test-roms (collection of test ROMs
# and demos circulated on the NESdev community for emulator testing).
set -euo pipefail
cd "$(dirname "$0")/.."
BASE=https://raw.githubusercontent.com/christopherpow/nes-test-roms/master
mkdir -p roms
fetch() { # <remote path> <local name>
  [ -f "roms/$2" ] && return
  echo "fetching $2"
  curl -fsSL "$BASE/$(printf %s "$1" | sed 's/ /%20/g')" -o "roms/$2"
}
fetch instr_test-v5/official_only.nes        cpu_instr_test.nes   # blargg: CPU instructions (prints Passed)
fetch cpu_timing_test6/cpu_timing_test.nes   cpu_timing_test.nes  # blargg: CPU timing
fetch apu_test/apu_test.nes                  apu_test.nes         # blargg: APU
fetch other/nestest.nes                      nestest.nes          # kevtris: CPU test menu
fetch spritecans-2011/spritecans.nes         spritecans.nes       # sprite demo
fetch other/BladeBuster.nes                  bladebuster.nes      # homebrew shmup (playable, music)
fetch other/quantum_disco_brothers_by_wAMMA.nes quantum_disco.nes # demoscene prod (music)
ls -la roms
