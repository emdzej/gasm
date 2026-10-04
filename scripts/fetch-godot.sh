#!/usr/bin/env bash
# Godot for gasm (guests/godot): the engine source (MIT) in tools/godot-src with
# guests/godot/godot.patch applied and the gasm platform (guests/godot/platform/gasm)
# copied in as platform/gasm; SCons in tools/scons; and the Godot editor for this
# machine in tools/godot-editor (it imports and exports the example projects).
# Every download is pinned. tools/godot-src/.gasm-fetch records what is installed.
#   scripts/fetch-godot.sh            (make godot runs it)
set -euo pipefail
cd "$(dirname "$0")/.."
. scripts/lib.sh
VERSION=4.7.2-stable
SRC_SHA=a18ce0ccec3ecc40b0dd6c4f5132ca934e9fb7c2979717940ff32aee1eb35481
SCONS=4.11.1
SCONS_URL=https://files.pythonhosted.org/packages/8e/43/d6285848e893c19682c06e92679dc1a07d37ff7ea148747b1df681ec496c/scons-$SCONS-py3-none-any.whl
SCONS_SHA=454cef364348053422696e3d2ecb4fa593c96a624f955842eaaea64f95c8d11d
REL=https://github.com/godotengine/godot/releases/download/$VERSION
TMP=$(mktemp -d)
trap 'rm -rf "$TMP"' EXIT

# ---- engine source (re-fetched when the patch changes) ----
STAMP="$VERSION $(sha256_of guests/godot/godot.patch)"
if [ ! -f tools/godot-src/.gasm-fetch ] || [ "$(cat tools/godot-src/.gasm-fetch)" != "$STAMP" ]; then
  echo "fetching $REL/godot-$VERSION.tar.xz"
  download "$REL/godot-$VERSION.tar.xz" "$TMP/godot.tar.xz" "$SRC_SHA"
  rm -rf tools/godot-src && mkdir -p tools/godot-src
  tar xJf "$TMP/godot.tar.xz" -C tools/godot-src --strip-components=1
  (cd tools/godot-src && patch -p1 -s --no-backup-if-mismatch) < guests/godot/godot.patch
  echo "$STAMP" > tools/godot-src/.gasm-fetch
fi

# ---- the gasm platform: copy only what changed (keeps scons' incremental builds) ----
mkdir -p tools/godot-src/platform/gasm
(cd guests/godot/platform/gasm && find . -type f) | while read -r f; do
  dst=tools/godot-src/platform/gasm/$f
  mkdir -p "$(dirname "$dst")"
  cmp -s "guests/godot/platform/gasm/$f" "$dst" || cp "guests/godot/platform/gasm/$f" "$dst"
done

# ---- SCons (a wheel is a zip: run it with PYTHONPATH) ----
if [ ! -f tools/scons/.version ] || [ "$(cat tools/scons/.version)" != "$SCONS" ]; then
  download "$SCONS_URL" "$TMP/scons.whl" "$SCONS_SHA"
  rm -rf tools/scons && mkdir -p tools/scons
  (cd tools/scons && unzip -q "$TMP/scons.whl")
  echo "$SCONS" > tools/scons/.version
fi

# ---- the editor, for importing and exporting projects ----
case "$(uname -s)-$(uname -m)" in
  Darwin-*)      ED=macos.universal ED_SHA=c58a24e31d720be9d62f60cb5627c4e695fb72f21b0cfe1bc9ccaa9a3b3ba63e ;;
  Linux-x86_64)  ED=linux.x86_64    ED_SHA=cadd3204e728a35d3f13adb7fd0d7902636b79f6b95c40c265eb73b6c35329e4 ;;
  Linux-aarch64) ED=linux.arm64     ED_SHA=5dd0d86405cf7e8adf79fb6377b38ba682a2846cb378ffe5364f38c01ad29b9d ;;
  *) echo "fetch-godot.sh: no Godot editor for $(uname -s)-$(uname -m)" >&2; exit 1 ;;
esac
if [ ! -f tools/godot-editor/.version ] || [ "$(cat tools/godot-editor/.version)" != "$VERSION-$ED" ]; then
  echo "fetching the Godot $VERSION editor ($ED)"
  download "$REL/Godot_v${VERSION}_$ED.zip" "$TMP/editor.zip" "$ED_SHA"
  rm -rf tools/godot-editor && mkdir -p tools/godot-editor
  (cd tools/godot-editor && unzip -q "$TMP/editor.zip")
  [ "$(uname -s)" = Darwin ] && xattr -dr com.apple.quarantine tools/godot-editor 2>/dev/null || true
  echo "$VERSION-$ED" > tools/godot-editor/.version
fi
