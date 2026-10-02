#!/usr/bin/env bash
# Cross-runner determinism test: every case must produce identical video/audio
# hashes on the native runner (wasmtime JIT), the native runner with an AOT
# artifact, and the Node runner (V8), and match the golden hashes in
# tests/golden/determinism.txt (the same on every platform: CI runs this on
# Linux, macOS and Windows).
#
#   scripts/determinism-test.sh                  run all cases
#   UPDATE_GOLDEN=1 scripts/determinism-test.sh  record new golden hashes (after
#                                                a change that is meant to alter output)
set -uo pipefail
cd "$(dirname "$0")/.."
GOLDEN=tests/golden/determinism.txt
UPDATE=${UPDATE_GOLDEN:-}
NEW_GOLDEN=$(mktemp)
trap 'rm -f "$NEW_GOLDEN"' EXIT
NATIVE=runners/native/target/release/gasm-run
NODE="node runners/web/headless.mjs"
[ -d roms ] && [ -n "$(ls roms/*.nes 2>/dev/null)" ] && [ -f roms/freedoom2.wad ] && [ -f roms/doom1.wad ] && [ -f roms/bass/sky.dnr ] && [ -d roms/scumm/dott-dos-ni-demo-en ] && [ -f roms/drascula/flac/audio/track28.flac ] || scripts/fetch-roms.sh
for g in nes test-pattern sumo triangle textured inputtest loopdemo loopdemo-c doom scummvm sdl3-snake sdl3-woodeneye sdl3-callbacks sdl3-classic; do "$NATIVE" build/$g.wasm --compile build/$g.cwasm 2>/dev/null; done

pass=0; fail=0
check() { # <name> <guest-basename> <frames> [runner args...]
  local name=$1 guest=$2 frames=$3; shift 3
  local a b c
  a=$("$NATIVE" build/$guest.wasm  --headless "$frames" "$@" 2>/dev/null | grep -E '^(frames|video)' | tr '\n' ' ')
  b=$("$NATIVE" build/$guest.cwasm --allow-precompiled --headless "$frames" "$@" 2>/dev/null | grep -E '^(frames|video)' | tr '\n' ' ')
  c=$($NODE     build/$guest.wasm  --headless "$frames" "$@" 2>/dev/null | grep -E '^(frames|video)' | tr '\n' ' ')
  local want
  want=$([ -f "$GOLDEN" ] && grep "^$name " "$GOLDEN" | cut -d' ' -f2-)
  [ -n "$a" ] && printf '%s %s\n' "$name" "$a" >> "$NEW_GOLDEN"
  if [ -n "$a" ] && [ "$a" = "$b" ] && [ "$a" = "$c" ] && { [ -n "$UPDATE" ] || [ "$a" = "$want" ]; }; then
    pass=$((pass + 1)); printf 'PASS  %-22s %s\n' "$name" "$(echo "$a" | cut -d' ' -f4-5)"
  else
    fail=$((fail + 1)); printf 'FAIL  %s\n  jit:    %s\n  aot:    %s\n  node:   %s\n  golden: %s\n' "$name" "$a" "$b" "$c" "${want:-(none; UPDATE_GOLDEN=1 records it)}"
  fi
}

check test-pattern       test-pattern 300 --input "30-200:RIGHT+A,100-150:DOWN,50-80:KEY(ShiftLeft),220-230:PTR(500,200,L)"
# sumo: gfx null backend hashes every GPU buffer upload (uniforms = full scene state)
# textured: texture uploads (full mip chain + a region per frame), dynamic offsets,
# storage buffer, and scripted text input (commas and escapes inside quotes)
check textured-text       textured 600 --input '100:"hi",150:"\b!\n",200:"x,y",400-450:A,250-260:WHEEL(0,1),300-330:PTR(640,360,L),310-330:MOVE(20,0)'
# raw input: keys with modifiers, pointer (frame mapping, clicks, wheel, motion), gamepads, storage keys
check inputtest           inputtest 200 --input '10-40:KEY(ShiftLeft+ArrowLeft),20-25:KEY(KeyA),50:PTR(640,360),51-53:PTR(700,300,L),54:PTR(700,300),60-70:WHEEL(0,1),71-80:MOVE(5,-3),90-120:GP0(B0+B9+A0=-0.75+A1=0.5),100-110:GP2(B3),130:PTR(10,10,R),131:PTR(10,10),140-150:UP+A,160:"hi"'
# display aspect (video_set_aspect): the pointer's frame position follows the 16:9 letterbox
check inputtest-aspect    inputtest 120 --param aspect=16:9 --input '20:PTR(100,100),30-32:PTR(1000,500,L),33:PTR(1000,500),40-60:PTR(1270,10)'
# own main loop (Asyncify in the guest): Rust gasm::main_loop and C gasm_loop.h, both
# play until START is held for a second and then return from main (exit code 0)
check loopdemo-rust       loopdemo 400 --input '100-110:START,150-200:RIGHT+A,230-240:DOWN,250-330:START'
check loopdemo-c          loopdemo-c 300 --input '50-120:RIGHT,200-270:START'
# SDL3 (sdk/sdl3): SDL's own demos unchanged (snake; woodeneye: WASD, relative mouse, shooting),
# a callbacks app (audio stream, gamepad events) and a classic main() loop (Asyncify:
# keyboard state, text input, SDL_Delay, a save file in the pref path)
check sdl3-snake          sdl3-snake 900 --input '100-110:KEY(ArrowUp),200-210:KEY(ArrowLeft),300-310:KEY(ArrowDown),500-510:KEY(ArrowRight)'
check sdl3-woodeneye      sdl3-woodeneye 400 --input '50-150:KEY(KeyW),100-200:MOVE(8,0),160-180:KEY(KeyA+Space),210:PTR(640,360),211-213:PTR(640,360,L),250-300:KEY(KeyD)'
check sdl3-callbacks      sdl3-callbacks 300 --input '20-21:KEY(Digit1),60-61:KEY(Digit5),100-103:GP0(B0),140-142:GP0(B3),200-201:KEY(Digit8),280-281:KEY(Escape)'
check sdl3-classic        sdl3-classic 300 --input '20-60:KEY(ArrowRight),70-71:KEY(Tab),80:"hello gasm",90-91:KEY(Backspace),100-140:KEY(ArrowDown+ArrowLeft),250-251:KEY(Escape)'
# triangle: the smallest gfx guest (one buffer, one pipeline); --screenshot of it is the GPU smoke test
check triangle            triangle 300
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
# DOOM on the raw keyboard and mouse: Ctrl fire, Shift run, mouse turn and fire, weapon key, Alt strafe, Esc menu
check freedoom2-keyboard  doom 520 --asset wad=roms/freedoom2.wad --param "args=-warp 1" \
  --input '40-200:KEY(ControlLeft+ArrowUp),200-300:KEY(ShiftLeft+ArrowLeft),300-400:MOVE(12,0),320-360:PTR(0,0,L),410-412:KEY(Digit2),420-460:KEY(AltLeft+ArrowRight),470:KEY(Escape),490-491:KEY(ArrowDown),500-501:KEY(Enter)'
# ScummVM (Asyncify): Beneath a Steel Sky, skip the intro, walk, save to slot 1 with a typed
# name through the in-game menu (storage), then restore it
check scummvm-sky         scummvm 2400 --asset-dir roms/bass --param "args=-p / sky" \
  --input '600-603:KEY(Escape),1490:PTR(760,330),1500-1502:PTR(760,330,L),1600-1603:KEY(ControlLeft+F5),1700:PTR(640,285),1710-1712:PTR(640,285,L),1800:PTR(250,99),1810-1812:PTR(250,99,L),1830-1831:KEY(KeyG),1835-1836:KEY(KeyA),1840-1841:KEY(KeyS),1845-1846:KEY(KeyM),1870:PTR(1015,669),1880-1882:PTR(1015,669,L),1950:PTR(1060,330),1960-1962:PTR(1060,330,L),2000-2003:KEY(ControlLeft+F5),2100:PTR(640,237),2110-2112:PTR(640,237,L),2200:PTR(250,81),2210-2212:PTR(250,81,L),2230:PTR(1015,669),2240-2242:PTR(1015,669,L)'
# ScummVM SCUMM engine (v6): the Day of the Tentacle demo, auto-detected, a click and Esc
check scumm-dott-demo     scummvm 1800 --asset-dir roms/scumm/dott-dos-ni-demo-en --param "args=--auto-detect -p /" \
  --input '900:PTR(640,400),910-912:PTR(640,400,L),1200-1203:KEY(Escape)'
# ScummVM's compressed audio (zlib, libmad, libvorbis, libFLAC built for wasm32): Drascula's
# opening with its CD music as Ogg Vorbis, MP3 and FLAC
check drascula-ogg        scummvm 900 --asset-dir roms/drascula/game --asset-dir roms/drascula/ogg --param "args=--auto-detect -p /"
check drascula-mp3        scummvm 900 --asset-dir roms/drascula/game --asset-dir roms/drascula/mp3 --param "args=--auto-detect -p /"
check drascula-flac       scummvm 900 --asset-dir roms/drascula/game --asset-dir roms/drascula/flac --param "args=--auto-detect -p /"
check freedoom2-save-load doom 1100 --asset wad=roms/freedoom2.wad --param "args=-warp 1 -skill 4" \
  --input "20-200:UP+A,210-211:START,220-221:DOWN,230-231:DOWN,240-241:DOWN,250-251:A,260-261:A,270-271:A,300-500:LEFT+UP+A,510-511:START,520-521:UP,530-531:A,540-541:A,600-900:RIGHT+UP+A+Y,905-906:X,910-1100:LEFT+R+A"

# video_set_aspect outside 1/8..8 traps on every runner
for runner in "$NATIVE" "$NODE"; do
  out=$($runner build/inputtest.wasm --headless 2 --param aspect=1:9 2>&1)
  if grep -q 'video_set_aspect: invalid ratio 1:9' <<<"$out"; then
    pass=$((pass + 1)); printf 'PASS  %-22s %s\n' "aspect-trap" "${runner##*/}"
  else
    fail=$((fail + 1)); printf 'FAIL  aspect-trap (%s): no trap for 1:9\n' "$runner"
  fi
done

echo "$pass passed, $fail failed"
if [ -n "$UPDATE" ] && [ "$fail" -eq 0 ]; then
  mkdir -p "$(dirname "$GOLDEN")"
  { echo "# golden hashes: <case> frames=… presented=… size=… video_fnv32=… audio_fnv32=… audio_frames=…"
    echo "# from scripts/determinism-test.sh; regenerate with UPDATE_GOLDEN=1 after an intended output change"
    cat "$NEW_GOLDEN"; } > "$GOLDEN"
  echo "golden hashes written to $GOLDEN"
fi
[ "$fail" -eq 0 ]
