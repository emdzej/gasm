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
cp build/sumo.wasm build/nes.wasm build/doom.wasm build/triangle.wasm build/test-pattern.wasm build/inputtest.wasm build/scummvm.wasm \
  build/sdl3-snake.wasm build/sdl3-woodeneye.wasm "$PKG/games/"
cat > "$PKG/games/scummvm-LICENSE.txt" <<TXT
scummvm.wasm is ScummVM for gasm, licensed under the GNU General Public License
version 3. Its complete source code is gasm-$VERSION-scummvm-src.tar.gz,
published next to this package at
https://github.com/emdzej/gasm/releases/tag/$VERSION
Games are not included (except where noted); use your own copies.
TXT
cat > "$PKG/games/doom-LICENSE.txt" <<TXT
doom.wasm is DOOM (doomgeneric, chocolate-doom) for gasm, licensed under the
GNU General Public License version 2. Its complete source code is
gasm-$VERSION-doom-src.tar.gz, published next to this package at
https://github.com/emdzej/gasm/releases/tag/$VERSION
The game data (WAD files) is not included and has its own license.
TXT

if [ -z "$EXE" ]; then
  for g in triangle test-pattern inputtest sdl3-snake sdl3-woodeneye; do
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
[ -n "$1" ] || { echo "usage: $0 <rom.nes>"; exit 2; }
rom=$(cd "$(dirname "$1")" && pwd)/$(basename "$1")
cd "$(dirname "$0")"
exec ./gasm-run games/nes.wasm --rom "$rom"
SH
  cat > "$PKG/run-doom.sh" <<'SH'
#!/bin/sh
# ./run-doom.sh path/to/doom1.wad [DOOM options, e.g. -warp 1 1 -skill 4]
# Any IWAD works: shareware doom1.wad, doom.wad, doom2.wad, Freedoom.
[ -n "$1" ] || { echo "usage: $0 <file.wad> [options]"; exit 2; }
wad=$(cd "$(dirname "$1")" && pwd)/$(basename "$1"); shift
cd "$(dirname "$0")"
exec ./gasm-run games/doom.wasm --asset "wad=$wad" --param "args=$*"
SH
  cat > "$PKG/run-scummvm.sh" <<'SH'
#!/bin/sh
# ./run-scummvm.sh path/to/game-folder      (detects the game and starts it)
# ./run-scummvm.sh path/to/folder -p / sky  (any ScummVM command line, game files at /)
[ -n "$1" ] || { echo "usage: $0 <game folder> [scummvm options]"; exit 2; }
dir=$(cd "$1" && pwd); shift
cd "$(dirname "$0")"
args=${*:---auto-detect -p /}
exec ./gasm-run games/scummvm.wasm --asset-dir "$dir" --param "args=$args"
SH
  cat > "$PKG/run-relay.sh" <<'SH'
#!/bin/sh
cd "$(dirname "$0")" && exec ./gasm-relay "${1:-0.0.0.0:9000}"
SH
  chmod +x "$PKG"/*.sh "$PKG/gasm-run" "$PKG/gasm-relay"
else
  for g in triangle test-pattern inputtest sdl3-snake sdl3-woodeneye; do
    printf '@echo off\r\ncd /d "%%~dp0"\r\ngasm-run.exe games\\%s.wasm %%*\r\n' "$g" > "$PKG/run-$g.cmd"
  done
  printf '@echo off\r\nrem Sumo vs. the bot: run-sumo.cmd    Online: run-sumo.cmd ws://HOST:9000 [room]\r\ncd /d "%%~dp0"\r\nif "%%~1"=="" (gasm-run.exe games\\sumo.wasm) else (if "%%~2"=="" (gasm-run.exe games\\sumo.wasm --allow-net --param relay=%%1 --param room=sumo) else (gasm-run.exe games\\sumo.wasm --allow-net --param relay=%%1 --param room=%%2))\r\n' > "$PKG/run-sumo.cmd"
  printf '@echo off\r\nrem run-nes.cmd path\\to\\game.nes\r\ncd /d "%%~dp0"\r\ngasm-run.exe games\\nes.wasm --rom %%1\r\n' > "$PKG/run-nes.cmd"
  printf '@echo off\r\ncd /d "%%~dp0"\r\ngasm-relay.exe 0.0.0.0:9000\r\n' > "$PKG/run-relay.cmd"
  printf '@echo off\r\nrem run-scummvm.cmd path\\to\\game-folder (detects the game and starts it)\r\ncd /d "%%~dp0"\r\ngasm-run.exe games\\scummvm.wasm --asset-dir %%1 --param "args=--auto-detect -p /"\r\n' > "$PKG/run-scummvm.cmd"
  printf '@echo off\r\nrem run-doom.cmd path\\to\\doom1.wad (any IWAD: doom1.wad, doom.wad, doom2.wad, Freedoom)\r\ncd /d "%%~dp0"\r\ngasm-run.exe games\\doom.wasm --asset wad=%%1\r\n' > "$PKG/run-doom.cmd"
fi

cat > "$PKG/README.txt" <<TXT
gasm $VERSION ($PLATFORM): portable games on WebAssembly
https://gasm.emdzej.pl

  gasm-run$EXE      native runner (wasmtime + wgpu)   gasm-run$EXE --help
  gasm-relay$EXE    WebSocket room relay for online play
  games/           sumo.wasm (3D, 2 players), nes.wasm (NES emulator),
                   doom.wasm (DOOM, GPL-2.0), triangle.wasm, test-pattern.wasm,
                   inputtest.wasm (shows keyboard, mouse, gamepads),
                   scummvm.wasm (ScummVM, GPL-3.0; bring your own games),
                   sdl3-snake.wasm, sdl3-woodeneye.wasm (SDL 3's demos, unchanged)

Quick start: run-sumo (vs. bot), run-sumo ws://HOST:9000 (online, start
run-relay somewhere first), run-nes <rom.nes>, run-doom <doom1.wad>,
run-scummvm <game folder>,
run-triangle.

Controls: arrows = D-pad, X = A, Z = B, Enter = Start, hold Esc = quit. Gamepads work.
The same .wasm files run in the browser: https://gasm.emdzej.pl/demos/
TXT
echo "$PKG"
