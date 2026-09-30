#!/usr/bin/env bash
# Fetch freely distributable NES test ROMs / homebrew demos and the Freedoom IWADs
# into roms/ (git-ignored).
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

# Freedoom (BSD-3-Clause): free replacement IWADs for the DOOM guest.
FREEDOOM=0.13.0
if [ ! -f roms/freedoom1.wad ] || [ ! -f roms/freedoom2.wad ]; then
  echo "fetching freedoom $FREEDOOM"
  tmp=$(mktemp -d)
  curl -fsSL "https://github.com/freedoom/freedoom/releases/download/v$FREEDOOM/freedoom-$FREEDOOM.zip" -o "$tmp/freedoom.zip"
  unzip -q -j -o "$tmp/freedoom.zip" "freedoom-$FREEDOOM/freedoom1.wad" "freedoom-$FREEDOOM/freedoom2.wad" -d roms
  rm -rf "$tmp"
fi

# DOOM shareware v1.9 (episode 1). id Software: "The DOOM shareware wad is freely
# distributable" (John Carmack, 1999). Taken unmodified from Debian's archive.
DOOM1_SHA1=5b2e249b9c5133ec987b3ea77596381dc0d6bc1d
if [ ! -f roms/doom1.wad ]; then
  echo "fetching doom1.wad (shareware)"
  tmp=$(mktemp -d)
  curl -fsSL http://deb.debian.org/debian/pool/non-free/d/doom-wad-shareware/doom-wad-shareware_1.9.fixed-5_all.deb -o "$tmp/doom1.deb"
  (cd "$tmp" && ar x doom1.deb && tar xf data.tar.xz ./usr/share/games/doom/doom1.wad)
  sum=$(shasum "$tmp/usr/share/games/doom/doom1.wad" 2>/dev/null || sha1sum "$tmp/usr/share/games/doom/doom1.wad")
  [ "${sum%% *}" = "$DOOM1_SHA1" ] || { echo "doom1.wad checksum mismatch" >&2; exit 1; }
  mv "$tmp/usr/share/games/doom/doom1.wad" roms/doom1.wad
  rm -rf "$tmp"
fi
ls -la roms
