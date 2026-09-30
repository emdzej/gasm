#!/usr/bin/env bash
# Cross-runner determinism test: every case must produce identical video/audio
# hashes on the native runner (wasmtime JIT), the native runner with an AOT
# artifact, and the Node runner (V8).
set -uo pipefail
cd "$(dirname "$0")/.."
NATIVE=runners/native/target/release/gasm-run
NODE="node runners/web/headless.mjs"
[ -d roms ] && [ -n "$(ls roms/*.nes 2>/dev/null)" ] || scripts/fetch-roms.sh
for g in nes test-pattern sumo; do "$NATIVE" build/$g.wasm --compile build/$g.cwasm 2>/dev/null; done

pass=0; fail=0
check() { # <name> <guest-basename> <frames> [runner args...]
  local name=$1 guest=$2 frames=$3; shift 3
  local a b c
  a=$("$NATIVE" build/$guest.wasm  --headless "$frames" "$@" 2>/dev/null | grep -E '^(frames|video)')
  b=$("$NATIVE" build/$guest.cwasm --headless "$frames" "$@" 2>/dev/null | grep -E '^(frames|video)')
  c=$($NODE     build/$guest.wasm  --headless "$frames" "$@" 2>/dev/null | grep -E '^(frames|video)')
  if [ -n "$a" ] && [ "$a" = "$b" ] && [ "$a" = "$c" ]; then
    pass=$((pass + 1)); printf 'PASS  %-22s %s\n' "$name" "$(echo "$a" | tail -1 | cut -d' ' -f1-2)"
  else
    fail=$((fail + 1)); printf 'FAIL  %s\n  jit:  %s\n  aot:  %s\n  node: %s\n' "$name" "$a" "$b" "$c"
  fi
}

check test-pattern       test-pattern 300 --input "30-200:RIGHT+A,100-150:DOWN"
# sumo: gfx null backend hashes every GPU buffer upload (uniforms = full scene state)
check sumo-vs-bot         sumo 3000 --input "130-900:RIGHT+A,900-1800:UP+B,1800-3000:LEFT+DOWN"
check cpu_instr_test     nes 3000 --rom roms/cpu_instr_test.nes
check cpu_timing_test    nes 1200 --rom roms/cpu_timing_test.nes
check apu_test           nes 1200 --rom roms/apu_test.nes
check spritecans         nes 600  --rom roms/spritecans.nes
check quantum_disco      nes 1800 --rom roms/quantum_disco.nes
check bladebuster-play   nes 2400 --rom roms/bladebuster.nes --input "100-104:START,200-204:START,300-2400:RIGHT+A,600-900:UP,1200-1500:DOWN"

echo "$pass passed, $fail failed"
[ "$fail" -eq 0 ]
