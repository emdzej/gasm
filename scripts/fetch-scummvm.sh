#!/usr/bin/env bash
# Fetch ScummVM (GPL-3.0) into tools/scummvm-src (git-ignored), add gasm's backend
# (guests/scummvm/backend -> backends/platform/gasm) and patch configure for the
# wasm32-gasm host. guests/scummvm holds only gasm's own files.
set -euo pipefail
cd "$(dirname "$0")/.."
VERSION=2026.3.0
OUT=tools/scummvm-src
STAMP="$OUT/.gasm-version"
SIG="$VERSION $(cat guests/scummvm/configure.patch guests/scummvm/backend/* sdk/c/src/gasm_loop.c sdk/c/include/gasm_loop.h | cksum | cut -d' ' -f1)"
[ -f "$STAMP" ] && [ "$(cat "$STAMP")" = "$SIG" ] && exit 0
if [ ! -f "$OUT/.pristine-$VERSION" ]; then
  echo "fetching scummvm $VERSION"
  rm -rf "$OUT" && mkdir -p tools
  TMP=$(mktemp -d)
  trap 'rm -rf "$TMP"' EXIT
  curl -fsSL "https://github.com/scummvm/scummvm/archive/refs/tags/v$VERSION.tar.gz" | tar xz -C "$TMP"
  mv "$TMP/scummvm-$VERSION" "$OUT"
  cp "$OUT/configure" "$OUT/.configure.orig"
  touch "$OUT/.pristine-$VERSION"
fi
# backend + configure patch (re-applied from the pristine configure each time)
rm -rf "$OUT/backends/platform/gasm" && mkdir -p "$OUT/backends/platform/gasm"
cp guests/scummvm/backend/* "$OUT/backends/platform/gasm/"
# the frame loop helper from the C SDK (compiled as C++ there)
cp sdk/c/src/gasm_loop.c "$OUT/backends/platform/gasm/gasm-loop.cpp"
cp sdk/c/include/gasm_loop.h "$OUT/backends/platform/gasm/"
cp "$OUT/.configure.orig" "$OUT/configure"
patch -s -p1 -d "$OUT" < guests/scummvm/configure.patch
echo "$SIG" > "$STAMP"
echo "scummvm sources in $OUT"
