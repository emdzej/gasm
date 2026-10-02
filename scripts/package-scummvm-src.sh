#!/usr/bin/env bash
# Package the complete corresponding source of build/scummvm.wasm (GPL-3.0) as
# dist/gasm-<version>-scummvm-src.tar.gz: exactly the ScummVM files the build used
# (every source and header listed in the compiler's dependency files, plus the
# build system and the embedded engine data), gasm's backend and patch, and the
# scripts to rebuild it. `make scummvm` in the unpacked tree rebuilds the module.
#   scripts/package-scummvm-src.sh <version>     (needs a finished `make scummvm`)
set -euo pipefail
cd "$(dirname "$0")/.."
VERSION=${1:?version}
SRC=tools/scummvm-src
[ -f build/scummvm.wasm ] && [ -n "$(find "$SRC" -path '*/.deps/*.d' -print -quit)" ] || { echo "run make scummvm first" >&2; exit 1; }
NAME="gasm-$VERSION-scummvm-src"
STAGE="dist/$NAME"
rm -rf "$STAGE" && mkdir -p "$STAGE"
list=$(mktemp)
# 1. everything the compiler read inside the tree
find "$SRC" -path '*/.deps/*.d' -print0 | xargs -0 cat | tr ' \\' '\n\n' | sed 's/:$//' \
  | { grep -v '^$' || true; } | { grep -v '^/' || true; } | sort -u > "$list.rel"
abs=$(cd "$SRC" && pwd)
find "$SRC" -path '*/.deps/*.d' -print0 | xargs -0 cat | tr ' \\' '\n\n' | sed 's/:$//' \
  | { grep "^$abs/" || true; } | sed "s|^$abs/||" | sort -u >> "$list.rel"
# 2. the build system and what configure scans
(cd "$SRC" && { ls configure config.guess config.sub Makefile Makefile.common rules.mk ports.mk engines.awk \
   COPYING COPYRIGHT AUTHORS README.md .configure.orig .pristine-* .gasm-version 2>/dev/null
   find . -name '*.mk' -o -name 'configure.engine' -o -name 'module.mk' | sed 's|^\./||'
   find LICENSES -type f
   find backends/platform/gasm -type f ! -name '*.o' ! -path '*/.deps/*'; }) >> "$list.rel"
# sources only: the dependency files also name the objects they produced
sort -u "$list.rel" | grep -vE '\.(o|a)$|/\.deps/' | while read -r f; do [ -f "$SRC/$f" ] && echo "$f"; done > "$list"
mkdir -p "$STAGE/tools/scummvm-src"
(cd "$SRC" && tar cf - -T "$list") | (cd "$STAGE/tools/scummvm-src" && tar xf -)
# the embedded engine data comes from dists/engine-data
for f in $(sed -n 's/^SCUMMVM_DATA *?= *//p' Makefile); do
  mkdir -p "$STAGE/tools/scummvm-src/dists/engine-data" && cp "$SRC/dists/engine-data/$f" "$STAGE/tools/scummvm-src/dists/engine-data/"
done
# 3. gasm's side
mkdir -p "$STAGE/guests" "$STAGE/scripts" "$STAGE/spec"
cp -R guests/scummvm "$STAGE/guests/"
mkdir -p "$STAGE/sdk/c/src" "$STAGE/sdk/c/include"
cp sdk/c/src/gasm_loop.c "$STAGE/sdk/c/src/" && cp sdk/c/include/gasm_loop.h "$STAGE/sdk/c/include/"
cp scripts/fetch-scummvm.sh scripts/fetch-binaryen.sh scripts/fetch-wasi-sdk.sh scripts/embed-files.mjs "$STAGE/scripts/"
cp spec/gasm.h "$STAGE/spec/" && cp Makefile "$STAGE/"
cat > "$STAGE/README.txt" <<TXT
Complete corresponding source for scummvm.wasm in gasm $VERSION.

scummvm.wasm is ScummVM $(sed -n 's/^VERSION=//p' scripts/fetch-scummvm.sh) built for the gasm ABI with gasm's backend.
As a whole it is licensed under the GNU GPL version 3 (tools/scummvm-src/COPYING;
some parts are under compatible licenses, see tools/scummvm-src/LICENSES).

  tools/scummvm-src/                 the ScummVM files used by the build (every
                                     compiled source and header, the build system,
                                     the embedded engine data), configure patched
  guests/scummvm/backend/            gasm's backend (MIT, GPL-compatible)
  guests/scummvm/configure.patch     the configure change (GPL-3.0)

Build: scripts/fetch-wasi-sdk.sh && scripts/fetch-binaryen.sh && make scummvm
Output: build/scummvm.wasm. Project: https://github.com/emdzej/gasm
The full ScummVM source: https://github.com/scummvm/scummvm/archive/refs/tags/v$(sed -n 's/^VERSION=//p' scripts/fetch-scummvm.sh).tar.gz
TXT
tar czf "dist/$NAME.tar.gz" -C dist "$NAME"
rm -rf "$STAGE" "$list" "$list.rel"
echo "dist/$NAME.tar.gz"
