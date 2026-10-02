#!/usr/bin/env bash
# Download wasi-sdk (clang + wasi-libc + libc++ for wasm32) into tools/wasi-sdk.
# The archive is checked against a pinned SHA-256 (another version needs WASI_SDK_SHA256).
# tools/wasi-sdk/.version records what is installed; the Makefile depends on it.
set -euo pipefail
cd "$(dirname "$0")/.."
. scripts/lib.sh
PINNED=34
VERSION=${WASI_SDK_VERSION:-$PINNED}
case "$(uname -s)-$(uname -m)" in
  Darwin-arm64)  PLAT=arm64-macos  SHA=9c59398106b417f8f14913380fdf0097a8cc0ff4af9eb3ce0065a859e88d49e9 ;;
  Darwin-x86_64) PLAT=x86_64-macos SHA=87d27fa8adc68dee59bfbf2e22a6d34ef717c34d6bf1d8af2a56fc929d9ce0eb ;;
  Linux-x86_64)  PLAT=x86_64-linux SHA=b761e3a0721dbae9c09a0059e5fdb2bf917d1b4a8a7b430fb3b5aafb0984b2c4 ;;
  Linux-aarch64) PLAT=arm64-linux  SHA=f7e243dff54d60bcc576e94d6166b69f410f2500ae4a9ceef34315be10e77971 ;;
  *) echo "unsupported host $(uname -s)-$(uname -m); install wasi-sdk and set WASI_SDK=..." >&2; exit 1 ;;
esac
if [ "$VERSION" != "$PINNED" ]; then
  SHA=${WASI_SDK_SHA256:?wasi-sdk $VERSION is not pinned: set WASI_SDK_SHA256 to its archive checksum}
fi
STAMP="$VERSION-$PLAT"
[ -f tools/wasi-sdk/.version ] && [ "$(cat tools/wasi-sdk/.version)" = "$STAMP" ] && exit 0
URL=https://github.com/WebAssembly/wasi-sdk/releases/download/wasi-sdk-$VERSION/wasi-sdk-$VERSION.0-$PLAT.tar.gz
mkdir -p tools
TMP=$(mktemp -d)
trap 'rm -rf "$TMP"' EXIT
echo "fetching $URL"
download "$URL" "$TMP/wasi-sdk.tar.gz" "$SHA"
tar xzf "$TMP/wasi-sdk.tar.gz" -C "$TMP"
rm -rf tools/wasi-sdk && mv "$TMP/wasi-sdk-$VERSION.0-$PLAT" tools/wasi-sdk
[ "$(uname -s)" = Darwin ] && xattr -dr com.apple.quarantine tools/wasi-sdk 2>/dev/null || true
echo "$STAMP" > tools/wasi-sdk/.version
tools/wasi-sdk/bin/clang --version | head -1
