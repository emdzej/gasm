#!/usr/bin/env bash
# Download Binaryen (wasm-opt) into tools/binaryen. Used for Asyncify in guests whose
# engines block in their own loops (ScummVM): wasm-opt --asyncify.
# The archive is checked against a pinned SHA-256 (another version needs BINARYEN_SHA256).
# tools/binaryen/.version records what is installed; the Makefile depends on it.
set -euo pipefail
cd "$(dirname "$0")/.."
. scripts/lib.sh
PINNED=133
VERSION=${BINARYEN_VERSION:-$PINNED}
case "$(uname -s)-$(uname -m)" in
  Darwin-arm64)  PLAT=arm64-macos   SHA=ad66da82ac13f163e424b1643f16c6dfcccc98b5966296b43e52d3cab04f84a8 ;;
  Darwin-x86_64) PLAT=x86_64-macos  SHA=13a9b90be775c6389ce3d1f879cb8627bea56708ba8c122983941d53a8199b95 ;;
  Linux-x86_64)  PLAT=x86_64-linux  SHA=2dc9c7813f5375db93d96ead4b78222fcc3e2677bbb832297af4797782a37489 ;;
  Linux-aarch64) PLAT=aarch64-linux SHA=89c07ea56faf38d0fbecf36ca8ec0721756716185f265b568e133d427f299bf8 ;;
  *) echo "unsupported host $(uname -s)-$(uname -m); install binaryen and set WASM_OPT=..." >&2; exit 1 ;;
esac
if [ "$VERSION" != "$PINNED" ]; then
  SHA=${BINARYEN_SHA256:?binaryen $VERSION is not pinned: set BINARYEN_SHA256 to its archive checksum}
fi
STAMP="$VERSION-$PLAT"
[ -f tools/binaryen/.version ] && [ "$(cat tools/binaryen/.version)" = "$STAMP" ] && exit 0
URL=https://github.com/WebAssembly/binaryen/releases/download/version_$VERSION/binaryen-version_$VERSION-$PLAT.tar.gz
mkdir -p tools
TMP=$(mktemp -d)
trap 'rm -rf "$TMP"' EXIT
echo "fetching $URL"
download "$URL" "$TMP/binaryen.tar.gz" "$SHA"
tar xzf "$TMP/binaryen.tar.gz" -C "$TMP"
rm -rf tools/binaryen && mv "$TMP/binaryen-version_$VERSION" tools/binaryen
[ "$(uname -s)" = Darwin ] && xattr -dr com.apple.quarantine tools/binaryen 2>/dev/null || true
echo "$STAMP" > tools/binaryen/.version
tools/binaryen/bin/wasm-opt --version
