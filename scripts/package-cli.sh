#!/usr/bin/env bash
# Package runner binaries + games + launcher scripts into dist/gasm-<version>-<platform>/.
#   scripts/package-cli.sh <version> <platform> <bin dir>
# <bin dir> contains gasm-run[.exe] and gasm-relay[.exe]. Prints the package dir.
set -euo pipefail
cd "$(dirname "$0")/.."
VERSION=${1:?version}; PLATFORM=${2:?platform}; BIN=${3:?bin dir}
EXE=""; [[ "$PLATFORM" == windows* ]] && EXE=".exe"
PKG="dist/gasm-$VERSION-$PLATFORM"
rm -rf "$PKG"; mkdir -p "$PKG/games"
cp "$BIN/gasm-run$EXE" "$BIN/gasm-relay$EXE" "$PKG/"
cp build/sumo.wasm build/nes.wasm build/triangle.wasm build/test-pattern.wasm "$PKG/games/"

if [ -z "$EXE" ]; then
  for g in triangle test-pattern; do
    printf '#!/bin/sh\ncd "$(dirname "$0")"\nexec ./gasm-run games/%s.wasm "$@"\n' "$g" > "$PKG/run-$g.sh"
  done
  cat > "$PKG/run-sumo.sh" <<'SH'
#!/bin/sh
# Sumo vs. the bot:          ./run-sumo.sh
# Sumo online (needs relay): ./run-sumo.sh ws://HOST:9000 [room]
cd "$(dirname "$0")"
if [ -n "$1" ]; then exec ./gasm-run games/sumo.wasm --allow-net --param "relay=$1" --param "room=${2:-sumo}"; fi
exec ./gasm-run games/sumo.wasm
SH
  cat > "$PKG/run-nes.sh" <<'SH'
#!/bin/sh
# ./run-nes.sh path/to/game.nes
cd "$(dirname "$0")"
[ -n "$1" ] || { echo "usage: $0 <rom.nes>"; exit 2; }
exec ./gasm-run games/nes.wasm --rom "$1"
SH
  cat > "$PKG/run-relay.sh" <<'SH'
#!/bin/sh
cd "$(dirname "$0")" && exec ./gasm-relay "${1:-0.0.0.0:9000}"
SH
  chmod +x "$PKG"/*.sh "$PKG/gasm-run" "$PKG/gasm-relay"
else
  for g in triangle test-pattern; do
    printf '@echo off\r\ncd /d "%%~dp0"\r\ngasm-run.exe games\\%s.wasm %%*\r\n' "$g" > "$PKG/run-$g.cmd"
  done
  printf '@echo off\r\nrem Sumo vs. the bot: run-sumo.cmd    Online: run-sumo.cmd ws://HOST:9000 [room]\r\ncd /d "%%~dp0"\r\nif "%%~1"=="" (gasm-run.exe games\\sumo.wasm) else (if "%%~2"=="" (gasm-run.exe games\\sumo.wasm --allow-net --param relay=%%1 --param room=sumo) else (gasm-run.exe games\\sumo.wasm --allow-net --param relay=%%1 --param room=%%2))\r\n' > "$PKG/run-sumo.cmd"
  printf '@echo off\r\nrem run-nes.cmd path\\to\\game.nes\r\ncd /d "%%~dp0"\r\ngasm-run.exe games\\nes.wasm --rom %%1\r\n' > "$PKG/run-nes.cmd"
  printf '@echo off\r\ncd /d "%%~dp0"\r\ngasm-relay.exe 0.0.0.0:9000\r\n' > "$PKG/run-relay.cmd"
fi

cat > "$PKG/README.txt" <<TXT
gasm $VERSION ($PLATFORM): portable games on WebAssembly
https://gasm.emdzej.pl

  gasm-run$EXE      native runner (wasmtime + wgpu)   gasm-run$EXE --help
  gasm-relay$EXE    WebSocket room relay for online play
  games/           sumo.wasm (3D, 2 players), nes.wasm (NES emulator),
                   triangle.wasm, test-pattern.wasm

Quick start: run-sumo (vs. bot), run-sumo ws://HOST:9000 (online, start
run-relay somewhere first), run-nes <rom.nes>, run-triangle.

Controls: arrows = D-pad, X = A, Z = B, Enter = Start, Esc = quit. Gamepads work.
The same .wasm files run in the browser: https://gasm.emdzej.pl/demos/
TXT
echo "$PKG"
