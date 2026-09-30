#!/usr/bin/env bash
# Bundle the C/C++ SDK: dist/gasm-c-sdk-<version>.zip (include/, cmake/, example/).
#   scripts/package-c-sdk.sh <version>
set -euo pipefail
cd "$(dirname "$0")/.."
VERSION=${1:?version}
PKG=dist/gasm-c-sdk-$VERSION
rm -rf "$PKG"; mkdir -p "$PKG/include"
cp spec/gasm.h "$PKG/include/"
cp -R sdk/c/cmake sdk/c/example sdk/c/README.md "$PKG/"
rm -rf "$PKG/example/build"
(cd dist && rm -f "gasm-c-sdk-$VERSION.zip" && zip -qr "gasm-c-sdk-$VERSION.zip" "gasm-c-sdk-$VERSION")
echo "dist/gasm-c-sdk-$VERSION.zip"
