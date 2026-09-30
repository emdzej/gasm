#!/usr/bin/env bash
# Wrap the demo games as double-clickable macOS apps.
#   scripts/package-macos.sh <gasm-run binary> <out dir> [version]
# Produces <out>/Sumo.app, NES.app, Triangle.app, Test Pattern.app. Each bundle
# contains gasm-run + one .wasm; a small launcher script starts it. The apps
# are not signed: on first launch use right-click > Open, or
#   xattr -dr com.apple.quarantine <App>.app
set -euo pipefail
cd "$(dirname "$0")/.."
RUN=${1:?usage: package-macos.sh <gasm-run> <out dir> [version]}
OUT=${2:?usage: package-macos.sh <gasm-run> <out dir> [version]}
VERSION=${3:-0.0.0}
mkdir -p "$OUT"

# App icon from assets/icon-1024.png (rendered from site/public/favicon.svg).
ICNS=""
if command -v iconutil >/dev/null && [ -f assets/icon-1024.png ]; then
  SET=$(mktemp -d)/gasm.iconset; mkdir -p "$SET"
  for s in 16 32 128 256 512; do
    sips -z $s $s assets/icon-1024.png --out "$SET/icon_${s}x${s}.png" >/dev/null
    sips -z $((s*2)) $((s*2)) assets/icon-1024.png --out "$SET/icon_${s}x${s}@2x.png" >/dev/null
  done
  ICNS=$(dirname "$SET")/gasm.icns
  iconutil -c icns "$SET" -o "$ICNS"
fi

app() { # <App name> <bundle id suffix> <game> <launcher body>
  local name=$1 id=$2 game=$3 body=$4
  local dir="$OUT/$name.app/Contents"
  rm -rf "$OUT/$name.app"
  mkdir -p "$dir/MacOS" "$dir/Resources"
  cp "$RUN" "$dir/Resources/gasm-run"
  cp "build/$game.wasm" "$dir/Resources/"
  [ -n "$ICNS" ] && cp "$ICNS" "$dir/Resources/gasm.icns"
  cat > "$dir/Info.plist" <<PLIST
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0"><dict>
  <key>CFBundleName</key><string>$name</string>
  <key>CFBundleDisplayName</key><string>$name</string>
  <key>CFBundleIdentifier</key><string>pl.emdzej.gasm.$id</string>
  <key>CFBundleVersion</key><string>$VERSION</string>
  <key>CFBundleShortVersionString</key><string>$VERSION</string>
  <key>CFBundlePackageType</key><string>APPL</string>
  <key>CFBundleExecutable</key><string>launch</string>
  <key>CFBundleIconFile</key><string>gasm</string>
  <key>LSMinimumSystemVersion</key><string>11.0</string>
  <key>NSHighResolutionCapable</key><true/>
</dict></plist>
PLIST
  cat > "$dir/MacOS/launch" <<LAUNCH
#!/bin/bash
# gasm launcher: runs the bundled game with the bundled gasm-run.
R="\$(cd "\$(dirname "\$0")/../Resources" && pwd)"
LOG="\$HOME/Library/Logs/gasm"; mkdir -p "\$LOG"
$body
LAUNCH
  chmod +x "$dir/MacOS/launch" "$dir/Resources/gasm-run"
  echo "packaged $OUT/$name.app"
}

app "Sumo" sumo sumo '
choice=$(osascript -e '"'"'button returned of (display dialog "Sumo: push the other ball off the platform.\nArrows move, X or Z dash." with title "gasm sumo" buttons {"Quit", "Online…", "Vs. bot"} default button "Vs. bot")'"'"') || exit 0
args=()
if [ "$choice" = "Online…" ]; then
  relay=$(osascript -e '"'"'text returned of (display dialog "Relay URL (gasm-relay):" with title "gasm sumo" default answer "ws://127.0.0.1:9000")'"'"') || exit 0
  room=$(osascript -e '"'"'text returned of (display dialog "Room name (same for both players):" with title "gasm sumo" default answer "sumo")'"'"') || exit 0
  args=(--allow-net --param "relay=$relay" --param "room=$room")
elif [ "$choice" = "Quit" ]; then exit 0; fi
exec "$R/gasm-run" "$R/sumo.wasm" --window 1280x720 "${args[@]}" 2>>"$LOG/sumo.log"'

app "NES" nes nes '
rom=$(osascript -e '"'"'POSIX path of (choose file with prompt "Choose a NES ROM (.nes)")'"'"') || exit 0
exec "$R/gasm-run" "$R/nes.wasm" --rom "$rom" --window 768x720 2>>"$LOG/nes.log"'

app "Triangle" triangle triangle '
exec "$R/gasm-run" "$R/triangle.wasm" --window 960x540 2>>"$LOG/triangle.log"'

app "Test Pattern" test-pattern test-pattern '
exec "$R/gasm-run" "$R/test-pattern.wasm" --window 768x720 2>>"$LOG/test-pattern.log"'
