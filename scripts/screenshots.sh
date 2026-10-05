#!/usr/bin/env bash
# The website's screenshots: site/public/screenshots/*.webp, made by running the games
# (headless gasm-run where it can render them, headless Chrome for the browser player).
#   make && make roms && scripts/screenshots.sh
# Needs roms/doom1.wad and roms/bass (make roms), cwebp, Python with Pillow, and Chrome.
set -euo pipefail
cd "$(dirname "$0")/.."
OUT=site/public/screenshots
TMP=$(mktemp -d)
trap 'rm -rf "$TMP"; [ -n "${SERVER:-}" ] && kill "$SERVER" 2>/dev/null || true' EXIT
R=runners/native/target/release/gasm-run
mkdir -p "$OUT"
shot() { # <name> <guest> <frames> [args...]: the window's view at 960x720 (2D games) or the GPU frame
  local name=$1 guest=$2 frames=$3; shift 3
  "$R" "build/$guest.wasm" --headless "$frames" "$@" >/dev/null 2>&1 </dev/null
}

shot sumo sumo 700 --input "130-900:RIGHT+A" --screenshot "$TMP/sumo.png"
shot textured textured 300 --screenshot "$TMP/textured.png"
shot doom doom 1400 --asset wad=roms/doom1.wad --screenshot-filtered "$TMP/doom.png" --filter sharp --window 960x720
for f in sharp xbr fsr crt; do
  shot doom-$f doom 1400 --asset wad=roms/doom1.wad --screenshot-filtered "$TMP/doom-$f.png" --filter $f --window 960x720
done
shot sky scummvm 2400 --asset-dir roms/bass --param "args=-p / sky" --input '600-603:KEY(Escape)' \
  --screenshot-filtered "$TMP/sky.png" --filter xbr --window 960x720
godot() { # <name> <example> <frames> [args]: a Godot example, rendered with ANGLE
  local name=$1 ex=$2 frames=$3; shift 3
  "$R" build/godot.wasm --asset "game.pck=build/godot/$ex.pck" --headless "$frames" --screenshot "$TMP/$name.png" "$@" >/dev/null 2>&1 </dev/null
}
godot godot3d scene3d 300 --input '100-160:KEY(ArrowLeft),200-205:KEY(Space)'
godot godot2d platformer 420 --input '30-400:KEY(ArrowRight),60-64:KEY(Space),130-134:KEY(Space),200-204:KEY(Space),270-274:KEY(Space)'
shot egui eguidemo 120 --input '40:PTR(1225,628),41-43:PTR(1225,628,L),44:PTR(600,300)' --screenshot "$TMP/egui.png"
shot woodeneye sdl3-woodeneye 400 --input '50-150:KEY(KeyW),100-200:MOVE(8,0)' \
  --screenshot-filtered "$TMP/woodeneye.png" --window 960x720

# the browser player (WebGL 2 for gltest; the page around it)
python3 -m http.server 8765 >/dev/null 2>&1 </dev/null &
SERVER=$!
sleep 1
node scripts/web-smoke.mjs "http://localhost:8765/runners/web/?game=gltest.wasm&autostart&nosplash" "$TMP/gltest.png" 3 >/dev/null
node scripts/web-smoke.mjs "http://localhost:8765/runners/web/?game=scummvm.wasm&autostart&nosplash" "$TMP/player.png" 25 \
  "8:keydown:Escape;8.1:keyup:Escape" >/dev/null

# filters side by side: the same 240x240 crop of each, scaled 4:3 like the window
python3 - "$TMP" <<'EOF'
import sys
from PIL import Image
tmp = sys.argv[1]
tiles = [Image.open(f'{tmp}/doom-{f}.png').convert('RGB').crop((600, 200, 840, 440)) for f in ('sharp', 'xbr', 'fsr', 'crt')]
strip = Image.new('RGB', (240 * 4 + 8 * 3, 240), (13, 17, 23))
for i, t in enumerate(tiles):
    strip.paste(t, (i * 248, 0))
strip.save(f'{tmp}/filters.png')
EOF

for n in sumo textured doom sky woodeneye gltest egui godot3d godot2d player filters; do
  cwebp -quiet -q 85 "$TMP/$n.png" -o "$OUT/$n.webp"
done
ls -la "$OUT"
