#!/usr/bin/env bash
# Bundle SDL3 for gasm: dist/gasm-sdl3-<version>.zip (include/, lib/libSDL3.a,
# lib/cmake/SDL3/, examples/). Builds it first if needed (make sdl3).
#   scripts/package-sdl3.sh <version>
set -euo pipefail
cd "$(dirname "$0")/.."
VERSION=${1:?version}
make -s sdl3
PKG=dist/gasm-sdl3-$VERSION
rm -rf "$PKG"; mkdir -p "$PKG/lib" "$PKG/examples"
cp -R build/sdl3/include "$PKG/"
cp build/sdl3/lib/libSDL3.a "$PKG/lib/"
cp -R build/sdl3/lib/cmake "$PKG/lib/"
cp -R sdk/sdl3/examples/callbacks sdk/sdl3/examples/classic sdk/sdl3/examples/CMakeLists.txt "$PKG/examples/"
cp sdk/sdl3/README.md "$PKG/"
cp tools/SDL3-src/LICENSE.txt "$PKG/LICENSE-SDL.txt"
(cd dist && rm -f "gasm-sdl3-$VERSION.zip" && zip -qr "gasm-sdl3-$VERSION.zip" "gasm-sdl3-$VERSION")
echo "dist/gasm-sdl3-$VERSION.zip"
