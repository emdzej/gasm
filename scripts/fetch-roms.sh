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
# Beneath a Steel Sky, floppy version (freeware, Revolution Software; free to
# redistribute with its readme, which stays next to the game in roms/bass/).
if [ ! -f roms/bass/sky.dnr ] || [ ! -f roms/bass/sky.dsk ]; then
  echo "fetching Beneath a Steel Sky (floppy, freeware)"
  tmp=$(mktemp -d)
  curl -fsSL "https://downloads.scummvm.org/frs/extras/Beneath%20a%20Steel%20Sky/BASS-Floppy-1.3.zip" -o "$tmp/bass.zip"
  mkdir -p roms/bass && unzip -q -o "$tmp/bass.zip" -d roms/bass
  chmod 644 roms/bass/*   # the zip stores them owner-only
  rm -rf "$tmp"
  sum() { shasum -a 256 "$1" 2>/dev/null || sha256sum "$1"; }
  for f in sky.dnr:e1ea726858bfa024b9696856c32cd2f525d0e33b5fb0e0284ad2e6ce943115c8 \
           sky.dsk:355a6782b9741d0e9eb5e202343c944577e7d78ad36b4c8c7b83965526b1d2aa; do
    s=$(sum "roms/bass/${f%%:*}"); [ "${s%% *}" = "${f#*:}" ] || { echo "roms/bass/${f%%:*}: checksum mismatch" >&2; exit 1; }
  done
fi
# LucasArts SCUMM demos hosted by ScummVM, for testing the SCUMM engine (not
# redistributed by gasm: they stay in roms/, which is never committed or published).
scumm_demo() { # <name> <sha256 of the zip>
  [ -d "roms/scumm/$1" ] && return
  echo "fetching SCUMM demo $1"
  tmp=$(mktemp -d)
  curl -fsSL "https://downloads.scummvm.org/frs/demos/scumm/$1.zip" -o "$tmp/d.zip"
  s=$(shasum -a 256 "$tmp/d.zip" 2>/dev/null || sha256sum "$tmp/d.zip")
  [ "${s%% *}" = "$2" ] || { echo "$1.zip: checksum mismatch" >&2; exit 1; }
  mkdir -p "roms/scumm/$1" && unzip -q -o "$tmp/d.zip" -d "roms/scumm/$1" && chmod -R u+rwX,go+rX "roms/scumm/$1"
  rm -rf "$tmp"
}
scumm_demo monkey1-dos-ega-demo-en 1cb530fc4ab1d1f005e6630de31fe1a4186a7ad5d1a2b73e126e898e5e9039d0
scumm_demo dott-dos-ni-demo-en b77f03981da3815a352330f39f54d46e187bb05503cf1c832bb0d14539f15964
ls -la roms
