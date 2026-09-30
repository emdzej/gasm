#!/usr/bin/env bash
# Package the complete source of doom.wasm (GPL-2.0) as dist/gasm-<version>-doom-src.tar.gz:
# the fetched and patched engine (tools/doom-src), gasm's glue (guests/doom), the ABI
# header and the build files. `make doom` rebuilds it from the unpacked tree.
#   scripts/package-doom-src.sh <version>     (prints the archive path)
set -euo pipefail
cd "$(dirname "$0")/.."
VERSION=${1:?version}
scripts/fetch-doom.sh >/dev/null
NAME="gasm-$VERSION-doom-src"
STAGE="dist/$NAME"
rm -rf "$STAGE" && mkdir -p "$STAGE/tools" "$STAGE/scripts" "$STAGE/spec" "$STAGE/guests"
cp -R tools/doom-src "$STAGE/tools/"
cp -R guests/doom "$STAGE/guests/"
cp spec/gasm.h "$STAGE/spec/"
cp Makefile "$STAGE/"
cp scripts/fetch-doom.sh scripts/fetch-wasi-sdk.sh "$STAGE/scripts/"
cp tools/doom-src/LICENSE "$STAGE/COPYING"
cat > "$STAGE/README.txt" <<TXT
Complete corresponding source for doom.wasm in gasm $VERSION.

doom.wasm is DOOM (doomgeneric + chocolate-doom's OPL music player) compiled
for the gasm ABI. As a whole it is licensed under the GNU GPL version 2 (COPYING).

  tools/doom-src/   engine sources, as fetched by scripts/fetch-doom.sh and
                    patched with guests/doom/engine.patch
  guests/doom/      gasm platform layer (MIT, GPL-compatible)
  spec/gasm.h       gasm ABI header (MIT)

Build: scripts/fetch-wasi-sdk.sh && make doom   (output: build/doom.wasm)
Project: https://github.com/emdzej/gasm
TXT
tar czf "dist/$NAME.tar.gz" -C dist "$NAME"
rm -rf "$STAGE"
echo "dist/$NAME.tar.gz"
