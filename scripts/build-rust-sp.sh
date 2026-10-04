#!/usr/bin/env bash
# Assemble guests/gasm/sp/gasm_sp.s into guests/gasm/sp/libgasm_sp.a (committed: the
# Rust SDK's build.rs links it on wasm32, so games need no assembler).
#   scripts/build-rust-sp.sh        (needs wasi-sdk: scripts/fetch-wasi-sdk.sh)
set -euo pipefail
cd "$(dirname "$0")/.."
SDK=${WASI_SDK:-tools/wasi-sdk}
D=guests/gasm/sp
"$SDK/bin/clang" --target=wasm32 -mcpu=mvp -c "$D/gasm_sp.s" -o "$D/gasm_sp.o"
rm -f "$D/libgasm_sp.a"
"$SDK/bin/llvm-ar" crsD "$D/libgasm_sp.a" "$D/gasm_sp.o"
rm "$D/gasm_sp.o"
ls -la "$D/libgasm_sp.a"
