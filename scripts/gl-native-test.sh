#!/usr/bin/env bash
# gasm:gl on ANGLE in gasm-run (needs build/gltest.wasm and the native runner):
#   scripts/gl-native-test.sh [--gl-software]
# Fetches ANGLE (scripts/fetch-angle.sh), renders gltest headless through it, and
# checks that the hashes equal the null GL's golden ones (the model decides what is
# hashed, the backend only draws) and that the frame isn't blank. --gl-software:
# SwiftShader instead of the GPU (machines without one, CI).
set -euo pipefail
cd "$(dirname "$0")/.."
scripts/fetch-angle.sh
NATIVE=runners/native/target/release/gasm-run
[ -x "$NATIVE.exe" ] && NATIVE=$NATIVE.exe
OUT=$(mktemp -d)
trap 'rm -rf "$OUT"' EXIT
want=$(grep '^gltest ' tests/golden/determinism.txt | cut -d' ' -f2- | sed 's/ *$//')
log=$("$NATIVE" build/gltest.wasm --headless 120 --screenshot "$OUT/gl.png" "$@" 2>&1 </dev/null) || { echo "$log"; exit 1; }
got=$(echo "$log" | grep -E '^(frames|video)' | tr '\n' ' ' | sed 's/ *$//')
echo "$log" | grep '^\[gasm\] gl:' || { echo "$log"; echo "FAIL  gasm:gl did not use ANGLE"; exit 1; }
if [ "$got" != "$want" ]; then
  echo "FAIL  gltest on ANGLE: $got"; echo "      golden:         $want"; exit 1
fi
size=$(wc -c < "$OUT/gl.png" | tr -d ' ')
if [ "$size" -lt 20000 ]; then
  echo "FAIL  gltest on ANGLE: the frame looks blank ($size bytes of PNG)"; exit 1
fi
echo "PASS  gltest on ANGLE: hashes == null GL, frame rendered ($size bytes of PNG)"
