#!/usr/bin/env bash
# Download Binaryen (wasm-opt) into tools/binaryen. Used for Asyncify in guests whose
# engines block in their own loops (ScummVM): wasm-opt --asyncify.
set -euo pipefail
cd "$(dirname "$0")/.."
VERSION=${BINARYEN_VERSION:-133}
case "$(uname -s)-$(uname -m)" in
  Darwin-arm64)  PLAT=arm64-macos ;;
  Darwin-x86_64) PLAT=x86_64-macos ;;
  Linux-x86_64)  PLAT=x86_64-linux ;;
  Linux-aarch64) PLAT=aarch64-linux ;;
  *) echo "unsupported host $(uname -s)-$(uname -m); install binaryen and set WASM_OPT=..." >&2; exit 1 ;;
esac
URL=https://github.com/WebAssembly/binaryen/releases/download/version_$VERSION/binaryen-version_$VERSION-$PLAT.tar.gz
mkdir -p tools
echo "fetching $URL"
curl -fL "$URL" | tar xz -C tools
rm -rf tools/binaryen && mv "tools/binaryen-version_$VERSION" tools/binaryen
[ "$(uname -s)" = Darwin ] && xattr -dr com.apple.quarantine tools/binaryen 2>/dev/null || true
tools/binaryen/bin/wasm-opt --version
