#!/usr/bin/env bash
# Build the website: VitePress docs + the playable web runner at /play/.
#   scripts/build-site.sh            (expects build/*.wasm; run `make guests` first)
# Output: site/.vitepress/dist
set -euo pipefail
cd "$(dirname "$0")/.."

PLAY=site/public/play
rm -rf "$PLAY"
mkdir -p "$PLAY/build"
scripts/vendor-web.sh >/dev/null   # csfs for the OPFS import page
cp runners/web/index.html runners/web/app.js runners/web/gasm-host.js runners/web/webgpu-gfx.js \
   runners/web/gasm-worker.js runners/web/opfs.html runners/web/opfs.js runners/web/testdata.js "$PLAY/"
cp -R runners/web/vendor "$PLAY/"
for g in sumo triangle nes test-pattern assetcheck; do
  [ -f "build/$g.wasm" ] || { echo "missing build/$g.wasm; run 'make guests' first" >&2; exit 1; }
  cp "build/$g.wasm" "$PLAY/build/"
done
# On the site, games live next to the page (./build/), not at the repo root.
sed -i.bak 's|<meta name="gasm-root" content="../../">|<meta name="gasm-root" content="./">|' "$PLAY/index.html"
rm -f "$PLAY/index.html.bak"

cd site
pnpm install --frozen-lockfile
pnpm exec vitepress build
echo "site built: site/.vitepress/dist"
