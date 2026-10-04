#!/usr/bin/env bash
# Rebuild sdk/glow: glow (crates.io, pinned) with sdk/glow.patch applied, which makes
# its native backend run on wasm32-unknown-unknown on gasm's GLES functions
# (gasm::gles::get_proc_address). The result is committed so games can use it with
#   [patch.crates-io] glow = { git = "https://github.com/emdzej/gasm" }
#   scripts/update-glow.sh
set -euo pipefail
cd "$(dirname "$0")/.."
. scripts/lib.sh
VERSION=0.17.0
SHA=29038e1c483364cc6bb3cf78feee1816002e127c331a1eec55a4d202b9e1adb5   # Cargo.lock checksum
TMP=$(mktemp -d)
trap 'rm -rf "$TMP"' EXIT
download "https://static.crates.io/crates/glow/glow-$VERSION.crate" "$TMP/glow.crate" "$SHA"
tar xzf "$TMP/glow.crate" -C "$TMP"
SRC=$TMP/glow-$VERSION
# what the fork doesn't use: the web backend, the generator, crates.io metadata
rm -rf "$SRC/.github"
rm -f "$SRC/src/web_sys.rs" "$SRC/Cargo.toml.orig" "$SRC/Cargo.lock" "$SRC/.cargo_vcs_info.json" "$SRC/bors.toml" "$SRC/generate-native.sh"
(cd "$SRC" && patch -p1 --no-backup-if-mismatch -s) < sdk/glow.patch
rm -rf sdk/glow && mv "$SRC" sdk/glow
echo "sdk/glow: glow $VERSION + sdk/glow.patch"
