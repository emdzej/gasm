#!/usr/bin/env bash
# Write the license notices for the games gasm distributes (build/*.wasm) to stdout:
# what each GPL game is and where its complete source is, and the notices the
# libraries linked into them require in binary distributions (BSD-3-Clause: libogg,
# libvorbis, FLAC; zlib). Shipped as THIRD-PARTY.txt next to the games (release
# bundles, the games zip, the website).
#   scripts/third-party-notices.sh <version> > build/THIRD-PARTY.txt
set -euo pipefail
cd "$(dirname "$0")/.."
VERSION=${1:?version}
LIBS=tools/scummvm-libs/src
src() { echo "https://github.com/emdzej/gasm/releases/tag/$VERSION ($1)"; }
cat <<TXT
Third-party software in the gasm $VERSION games
===============================================

The gasm runtime, SDKs and the other games are MIT-licensed
(https://github.com/emdzej/gasm/blob/main/LICENSE). These games contain other
software under its own license:

doom.wasm: DOOM (doomgeneric, chocolate-doom's OPL music player and DOSBox's OPL
  emulator), GNU General Public License version 2. Complete source:
  $(src "gasm-$VERSION-doom-src.tar.gz")

scummvm.wasm: ScummVM, GNU General Public License version 3, with zlib (zlib
  license), libmad (GPL-2.0 or later), libogg, libvorbis and libFLAC (BSD-3-Clause).
  Complete source, including these libraries:
  $(src "gasm-$VERSION-scummvm-src.tar.gz")

sdl3-*.wasm: SDL 3 (zlib license), https://libsdl.org

Notices the BSD-3-Clause and zlib licenses ask to reproduce follow.
TXT
notice() { # <title> <file>
  printf '\n\n---- %s ----\n\n' "$1"
  if [ -f "$2" ]; then cat "$2"; else echo "(license text: $2, missing from this build tree)"; fi
}
notice "libogg 1.3.6 (BSD-3-Clause)" "$LIBS/libogg-1.3.6/COPYING"
notice "libvorbis 1.3.7 (BSD-3-Clause)" "$LIBS/libvorbis-1.3.7/COPYING"
notice "libFLAC 1.5.0 (BSD-3-Clause)" "$LIBS/flac-1.5.0/COPYING.Xiph"
notice "zlib 1.3.2 (zlib license)" "$LIBS/zlib-1.3.2/LICENSE"
