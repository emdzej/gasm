#!/usr/bin/env bash
# Cross-runner determinism test: every case must produce identical video/audio
# hashes on the native runner (wasmtime JIT), the native runner with an AOT
# artifact, and the Node runner (V8).
set -uo pipefail
cd "$(dirname "$0")/.."
NATIVE=runners/native/target/release/gasm-run
NODE="node runners/web/headless.mjs"
[ -d roms ] && [ -n "$(ls roms/*.nes 2>/dev/null)" ] && [ -f roms/freedoom2.wad ] && [ -f roms/doom1.wad ] || scripts/fetch-roms.sh
for g in nes test-pattern sumo textured doom; do "$NATIVE" build/$g.wasm --compile build/$g.cwasm 2>/dev/null; done

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
# textured: texture uploads (full mip chain + a region per frame), dynamic offsets,
# storage buffer, and scripted text input (commas and escapes inside quotes)
check textured-text       textured 600 --input '100:"hi",150:"\b!\n",200:"x,y",400-450:A'
check sumo-vs-bot         sumo 3000 --input "130-900:RIGHT+A,900-1800:UP+B,1800-3000:LEFT+DOWN"
check cpu_instr_test     nes 3000 --rom roms/cpu_instr_test.nes
check cpu_timing_test    nes 1200 --rom roms/cpu_timing_test.nes
check apu_test           nes 1200 --rom roms/apu_test.nes
check spritecans         nes 600  --rom roms/spritecans.nes
check quantum_disco      nes 1800 --rom roms/quantum_disco.nes
check bladebuster-play   nes 2400 --rom roms/bladebuster.nes --input "100-104:START,200-204:START,300-2400:RIGHT+A,600-900:UP,1200-1500:DOWN"
# doom: attract-mode demos (IWAD demo lumps), and a scripted game on Freedoom
# that saves, keeps playing, then loads the save (input, storage, OPL music)
check doom1-demos        doom 2100 --asset wad=roms/doom1.wad
check freedoom1-demos    doom 2100 --asset wad=roms/freedoom1.wad
check freedoom2-save-load doom 1100 --asset wad=roms/freedoom2.wad --param "args=-warp 1 -skill 4" \
  --input "20-200:UP+A,210-211:START,220-221:DOWN,230-231:DOWN,240-241:DOWN,250-251:A,260-261:A,270-271:A,300-500:LEFT+UP+A,510-511:START,520-521:UP,530-531:A,540-541:A,600-900:RIGHT+UP+A+Y,905-906:X,910-1100:LEFT+R+A"

echo "$pass passed, $fail failed"
[ "$fail" -eq 0 ]
