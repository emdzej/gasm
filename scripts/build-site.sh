#!/usr/bin/env bash
# Build the website: VitePress docs + the playable web runner at /play/.
#   scripts/build-site.sh            (expects build/*.wasm; run `make guests` first)
# Output: site/.vitepress/dist
set -euo pipefail
cd "$(dirname "$0")/.."

PLAY=site/public/play
rm -rf "$PLAY"
mkdir -p "$PLAY/build"
cp runners/web/index.html runners/web/app.js runners/web/gasm-host.js runners/web/webgpu-gfx.js "$PLAY/"
for g in sumo triangle nes test-pattern; do
  [ -f "build/$g.wasm" ] || { echo "missing build/$g.wasm; run 'make guests' first" >&2; exit 1; }
  cp "build/$g.wasm" "$PLAY/build/"
done
# On the site, games live next to the page (./build/), not at the repo root.
sed -i.bak 's|<meta name="gasm-root" content="../../">|<meta name="gasm-root" content="./">|' "$PLAY/index.html"
rm -f "$PLAY/index.html.bak"

cd site
if [ -f package-lock.json ]; then npm ci --no-audit --no-fund; else npm install --no-audit --no-fund; fi
npx vitepress build
echo "site built: site/.vitepress/dist"
