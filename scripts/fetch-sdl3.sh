#!/usr/bin/env bash
# Fetch SDL3 (zlib license) into tools/SDL3-src (git-ignored). gasm's platform code
# lives in sdk/sdl3 and is compiled next to it: SDL's "private platform" hooks
# (SDL_PLATFORM_PRIVATE) take it in without patching SDL.
# tools/SDL3-src/.version records the fetched version (the Makefile depends on it).
set -euo pipefail
cd "$(dirname "$0")/.."
. scripts/lib.sh
VERSION=3.4.16
SHA256=7322236cd12090c3eb40b9728be4d49c76f66ad17d04369584d4ecad5cf77c68
OUT=tools/SDL3-src
[ -f "$OUT/.version" ] && [ "$(cat "$OUT/.version")" = "$VERSION" ] && exit 0
echo "fetching SDL $VERSION"
rm -rf "$OUT" && mkdir -p tools
TMP=$(mktemp -d)
trap 'rm -rf "$TMP"' EXIT
download "https://github.com/libsdl-org/SDL/releases/download/release-$VERSION/SDL3-$VERSION.tar.gz" "$TMP/sdl.tar.gz" "$SHA256"
tar xzf "$TMP/sdl.tar.gz" -C "$TMP"
mv "$TMP/SDL3-$VERSION" "$OUT"
echo "$VERSION" > "$OUT/.version"
echo "SDL sources in $OUT"
