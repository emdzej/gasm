#!/usr/bin/env bash
# Download wasi-sdk (clang + wasi-libc + libc++ for wasm32) into tools/wasi-sdk.
# The archive is checked against a pinned SHA-256 (another version needs WASI_SDK_SHA256).
# tools/wasi-sdk/.version records what is installed; the Makefile depends on it.
# The sysroot (wasi-libc, libc++) and the compiler runtime come from wasi-sdk's
# host-independent archives, not the host SDK's own copies (those are built per
# host and embed different source paths): the same libraries on every machine, so
# a guest built on macOS and one built on Linux are the same bytes.
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
SYSROOT_SHA=9d813544eeebe38b7b8f2244ed591de46b6db812c6dd1a257ff9f0d2a905a2be
RT_SHA=eee3e634dcf71aa22b1333391623cf5c9965a637dc428a27b1a858c026c587f1
if [ "$VERSION" != "$PINNED" ]; then
  SYSROOT_SHA=${WASI_SYSROOT_SHA256:?set WASI_SYSROOT_SHA256 for wasi-sysroot $VERSION}
  RT_SHA=${WASI_RT_SHA256:?set WASI_RT_SHA256 for libclang_rt $VERSION}
fi
STAMP="$VERSION-$PLAT+sysroot"
[ -f tools/wasi-sdk/.version ] && [ "$(cat tools/wasi-sdk/.version)" = "$STAMP" ] && exit 0
URL=https://github.com/WebAssembly/wasi-sdk/releases/download/wasi-sdk-$VERSION/wasi-sdk-$VERSION.0-$PLAT.tar.gz
mkdir -p tools
TMP=$(mktemp -d)
trap 'rm -rf "$TMP"' EXIT
echo "fetching $URL"
download "$URL" "$TMP/wasi-sdk.tar.gz" "$SHA"
tar xzf "$TMP/wasi-sdk.tar.gz" -C "$TMP"
REL=https://github.com/WebAssembly/wasi-sdk/releases/download/wasi-sdk-$VERSION
download "$REL/wasi-sysroot-$VERSION.0.tar.gz" "$TMP/sysroot.tar.gz" "$SYSROOT_SHA"
download "$REL/libclang_rt-$VERSION.0.tar.gz" "$TMP/rt.tar.gz" "$RT_SHA"
SDK=$TMP/wasi-sdk-$VERSION.0-$PLAT
rm -rf "$SDK/share/wasi-sysroot"
tar xzf "$TMP/sysroot.tar.gz" -C "$SDK/share" && mv "$SDK/share/wasi-sysroot-$VERSION.0" "$SDK/share/wasi-sysroot"
tar xzf "$TMP/rt.tar.gz" -C "$TMP"
for d in "$TMP/libclang_rt-$VERSION.0"/*/; do
  t=$(basename "$d")
  for lib in "$SDK"/lib/clang/*/lib; do
    [ -d "$lib/$t" ] && cp "$d"/* "$lib/$t/"
  done
done
rm -rf tools/wasi-sdk && mv "$SDK" tools/wasi-sdk
[ "$(uname -s)" = Darwin ] && xattr -dr com.apple.quarantine tools/wasi-sdk 2>/dev/null || true
echo "$STAMP" > tools/wasi-sdk/.version
tools/wasi-sdk/bin/clang --version | head -1
