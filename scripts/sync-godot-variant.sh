#!/usr/bin/env bash
# A second Godot tree for another engine build (godot-2d, godot-custom): a copy of
# tools/godot-src (patched, with the gasm platform), kept in step by copying only
# files whose content changed, so SCons rebuilds just those. Each variant needs its
# own tree: SCons writes generated headers (enabled modules, disabled classes) into
# the source tree, so two configurations sharing one would rebuild each other.
#   scripts/sync-godot-variant.sh tools/godot-src-2d
set -euo pipefail
cd "$(dirname "$0")/.."
dst=${1:?usage: sync-godot-variant.sh <dir>}
[ -f tools/godot-src/.gasm-fetch ] || { echo "run scripts/fetch-godot.sh first" >&2; exit 1; }
mkdir -p "$dst"
# --checksum: compare contents, not times (the variant's own build outputs stay)
rsync -a --checksum --delete \
  --exclude=/bin/ --exclude='*.gen.*' --exclude='*.o' --exclude='*.a' --exclude='.sconsign*.dblite' \
  --exclude='__pycache__/' --exclude='/.scons_cache/' \
  tools/godot-src/ "$dst/"
