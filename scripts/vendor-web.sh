#!/usr/bin/env bash
# Copy the browser-side dependencies of the web player's OPFS import page (csfs)
# into runners/web/vendor/, where opfs.html's import map points. Not part of the
# published @emdzej/gasm-host (which stays dependency-free).
set -euo pipefail
cd "$(dirname "$0")/../runners/web"
[ -d node_modules/@emdzej/csfs-opfs ] || pnpm install --frozen-lockfile
rm -rf vendor && mkdir -p vendor
for p in core fsa opfs; do
  cp -R "node_modules/@emdzej/csfs-$p/dist" "vendor/csfs-$p"
done
echo "vendored csfs $(node -p "require('./node_modules/@emdzej/csfs-core/package.json').version") into runners/web/vendor"
