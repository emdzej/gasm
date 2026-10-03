#!/usr/bin/env bash
# Copy ANGLE (gasm:gl natively) for a release platform into <dir>, next to gasm-run:
#   scripts/package-angle.sh <platform> <dir>
# platforms as in package-cli.sh: macos-universal, linux-x86_64, linux-arm64, windows-x86_64.
# macOS: the arm64 and x86_64 libraries are merged with lipo, which invalidates their
# signatures, so they are signed again ad hoc (Apple silicon refuses unsigned code).
set -euo pipefail
cd "$(dirname "$0")/.."
PLATFORM=${1:?platform}; DIR=${2:?dir}
mkdir -p "$DIR"
case "$PLATFORM" in
  macos-universal)
    scripts/fetch-angle.sh macos-arm64 macos-x86_64
    for f in libEGL.dylib libGLESv2.dylib libvk_swiftshader.dylib; do
      lipo -create -output "$DIR/$f" "tools/angle/macos-arm64/$f" "tools/angle/macos-x86_64/$f"
      codesign --force --sign - "$DIR/$f"
    done
    cp tools/angle/macos-arm64/vk_swiftshader_icd.json tools/angle/macos-arm64/ANGLE-NOTICES.txt "$DIR/"
    ;;
  linux-x86_64 | linux-arm64 | windows-x86_64)
    scripts/fetch-angle.sh "$PLATFORM"
    (cd "tools/angle/$PLATFORM" && cp $(ls | grep -v '^\.version$') "../../../$DIR/")
    ;;
  *) echo "package-angle.sh: unknown platform $PLATFORM" >&2; exit 1 ;;
esac
