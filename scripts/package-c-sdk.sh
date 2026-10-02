#!/usr/bin/env bash
# Bundle the C/C++ SDK: dist/gasm-c-sdk-<version>.zip (include/, src/, cmake/, example/, example-loop/).
#   scripts/package-c-sdk.sh <version>
set -euo pipefail
cd "$(dirname "$0")/.."
VERSION=${1:?version}
PKG=dist/gasm-c-sdk-$VERSION
rm -rf "$PKG"; mkdir -p "$PKG/include"
cp spec/gasm.h sdk/c/include/*.h "$PKG/include/"
cp -R sdk/c/cmake sdk/c/src sdk/c/example sdk/c/example-loop sdk/c/README.md "$PKG/"
rm -rf "$PKG/example/build" "$PKG/example-loop/build"
(cd dist && rm -f "gasm-c-sdk-$VERSION.zip" && zip -qr "gasm-c-sdk-$VERSION.zip" "gasm-c-sdk-$VERSION")
echo "dist/gasm-c-sdk-$VERSION.zip"
