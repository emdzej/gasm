#!/usr/bin/env bash
# Download wasi-sdk (clang + wasi-libc + libc++ for wasm32) into tools/wasi-sdk.
set -euo pipefail
cd "$(dirname "$0")/.."
VERSION=${WASI_SDK_VERSION:-34}
case "$(uname -s)-$(uname -m)" in
  Darwin-arm64)  PLAT=arm64-macos ;;
  Darwin-x86_64) PLAT=x86_64-macos ;;
  Linux-x86_64)  PLAT=x86_64-linux ;;
  Linux-aarch64) PLAT=arm64-linux ;;
  *) echo "unsupported host $(uname -s)-$(uname -m); install wasi-sdk and set WASI_SDK=..." >&2; exit 1 ;;
esac
URL=https://github.com/WebAssembly/wasi-sdk/releases/download/wasi-sdk-$VERSION/wasi-sdk-$VERSION.0-$PLAT.tar.gz
mkdir -p tools
echo "fetching $URL"
curl -fL "$URL" | tar xz -C tools
rm -rf tools/wasi-sdk && mv "tools/wasi-sdk-$VERSION.0-$PLAT" tools/wasi-sdk
[ "$(uname -s)" = Darwin ] && xattr -dr com.apple.quarantine tools/wasi-sdk 2>/dev/null || true
tools/wasi-sdk/bin/clang --version | head -1
