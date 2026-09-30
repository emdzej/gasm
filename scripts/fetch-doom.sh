#!/usr/bin/env bash
# Fetch the DOOM engine sources (GPL-2.0) into tools/doom-src (git-ignored).
# guests/doom holds only gasm's own glue (MIT); doom.wasm is GPL-2.0 as a whole.
#   doomgeneric  - portable Chocolate Doom derivative (engine, renderer, game logic)
#   chocolate-doom - OPL music player (i_oplmusic.c, midifile.c) and the DOSBox OPL emulator (dbopl.c)
set -euo pipefail
cd "$(dirname "$0")/.."
DOOMGENERIC=dcb7a8dbc7a16ce3dda29382ac9aae9d77d21284
CHOCOLATE=chocolate-doom-2.2.1
OUT=tools/doom-src
STAMP="$OUT/.version"
PATCH=guests/doom/engine.patch
VERSION="$DOOMGENERIC $CHOCOLATE $(cksum < "$PATCH" | cut -d' ' -f1)"
[ -f "$STAMP" ] && [ "$(cat "$STAMP")" = "$VERSION" ] && exit 0
TMP=$(mktemp -d)
trap 'rm -rf "$TMP"' EXIT
echo "fetching doomgeneric $DOOMGENERIC"
curl -fsSL "https://github.com/ozkl/doomgeneric/archive/$DOOMGENERIC.tar.gz" | tar xz -C "$TMP"
echo "fetching $CHOCOLATE"
curl -fsSL "https://github.com/chocolate-doom/chocolate-doom/archive/refs/tags/$CHOCOLATE.tar.gz" | tar xz -C "$TMP"
DG="$TMP/doomgeneric-$DOOMGENERIC"
CD="$TMP/chocolate-doom-$CHOCOLATE"
rm -rf "$OUT" && mkdir -p "$OUT"
cp "$DG"/doomgeneric/*.c "$DG"/doomgeneric/*.h "$DG/LICENSE" "$OUT/"
# platform backends we replace with guests/doom/gasm_doom.c
rm -f "$OUT"/doomgeneric_*.c "$OUT"/i_sdlsound.c "$OUT"/i_sdlmusic.c "$OUT"/i_allegro*.c
cp "$CD"/src/i_oplmusic.c "$CD"/src/midifile.c "$CD"/src/midifile.h "$OUT/"
cp "$CD"/opl/opl.h "$CD"/opl/opl_queue.c "$CD"/opl/opl_queue.h "$CD"/opl/dbopl.c "$CD"/opl/dbopl.h "$OUT/"
# small engine changes for gasm (GPL-2.0, like the code they modify)
patch -s -p1 -d "$OUT" < "$PATCH"
echo "$VERSION" > "$STAMP"
echo "doom sources in $OUT ($(ls "$OUT"/*.c | wc -l | tr -d ' ') .c files)"
