#!/usr/bin/env bash
# Asset provider tests (R1 file-backed assets, R2 folders + case-insensitive lookup).
# Every run must print the same assetcheck hash and video hash.
set -uo pipefail
cd "$(dirname "$0")/.."
NATIVE=runners/native/target/release/gasm-run
NODE="node runners/web/headless.mjs"
G=build/assetcheck.wasm
CD=tests/fixtures/cd
READ='art/art.car,Worlds/1Player/Level1/rfmap001.RFM,readme.txt,Sound/Music.raw,MISSING.BIN'
pass=0; fail=0
line() { grep -hE '^(video_fnv32|\[guest\] \[assetcheck\] done)' | sed 's/^\[guest\] //' | sort | tr '\n' ' '; }
run() { "$@" --headless 1000 --param "read=$READ" --param stream=Sound/Music.raw --param frames=30 2>&1 | line; }
check() { # <name> <reference> <candidate>
  if [ -n "$2" ] && [ "$2" = "$3" ]; then pass=$((pass + 1)); printf 'PASS  %s\n' "$1"; else fail=$((fail + 1)); printf 'FAIL  %s\n  want: %s\n  got:  %s\n' "$1" "$2" "$3"; fi
}

ref=$(run "$NATIVE" $G --asset-dir $CD)
echo "      reference: $ref"
check "native --asset list"  "$ref" "$(run "$NATIVE" $G --asset art/art.car=$CD/ART/ART.CAR \
  --asset Worlds/1Player/Level1/rfmap001.RFM=$CD/WORLDS/1PLAYER/LEVEL1/RFMAP001.RFM \
  --asset readme.txt=$CD/README.TXT --asset Sound/Music.raw=$CD/SOUND/MUSIC.RAW)"
check "node --asset-dir"      "$ref" "$(run $NODE $G --asset-dir $CD)"
check "node --asset (memory)" "$ref" "$(run $NODE $G --asset art/art.car=$CD/ART/ART.CAR \
  --asset Worlds/1Player/Level1/rfmap001.RFM=$CD/WORLDS/1PLAYER/LEVEL1/RFMAP001.RFM \
  --asset readme.txt=$CD/README.TXT --asset Sound/Music.raw=$CD/SOUND/MUSIC.RAW)"
# prefix form: names become cd/<path>
pref=$("$NATIVE" $G --asset-dir cd=$CD --headless 1000 --param read=cd/art/ART.car --param frames=1 2>&1 | grep -o 'cd/art/ART.car: [0-9]* bytes fnv=[0-9a-f]*')
check "prefix form" "cd/art/ART.car: 16 bytes fnv=$(printf 'ART CAR FILE v1\n' | python3 -c 'import sys;h=0x811c9dc5
for b in sys.stdin.buffer.read(): h=((h^b)*0x01000193)&0xffffffff
print("%08x"%h)')" "$pref"
# hidden files are not exposed; explicit --asset wins over a folder entry
hid=$("$NATIVE" $G --asset-dir $CD --headless 1000 --param read=.DS_Store --param frames=1 2>&1 | grep -o '\.DS_Store: missing')
check "hidden files skipped" ".DS_Store: missing" "$hid"
ovr=$("$NATIVE" $G --asset-dir $CD --asset README.TXT=$CD/ART/ART.CAR --headless 1000 --param read=README.TXT --param frames=1 2>&1 | grep -o 'README.TXT: [0-9]* bytes')
check "explicit --asset wins" "README.TXT: 16 bytes" "$ovr"
# enumeration (asset_count/asset_name): same sorted list natively and in Node, hidden files absent
lst() { "$@" $G --asset-dir $CD --asset extra=$CD/README.TXT --headless 5 --param list=1 --param frames=1 2>&1 | grep -o '\[assetcheck\] [0-9]* assets: .*'; }
nl=$(lst "$NATIVE"); jl=$(lst $NODE)
check "asset enumeration: native == node" "$nl" "$jl"
case "$nl" in *".DS_Store"*|"") check "asset enumeration: hidden files skipped" "no .DS_Store" "$nl" ;; *) check "asset enumeration: hidden files skipped" ok ok ;; esac

# ---- R1: a large asset streamed at random offsets costs no RAM ----------------------
# (LARGE_MB=0 to skip). Same hash as an in-memory run (Node, --asset); resident memory
# of the file-backed native run compared with the same run on a tiny asset.
LARGE_MB=${LARGE_MB:-200}
if [ "$LARGE_MB" -gt 0 ]; then
  TMP=$(mktemp -d); trap 'rm -rf "$TMP"' EXIT
  python3 - "$TMP/big.bin" "$LARGE_MB" <<'PY'
import random, sys
path, mb = sys.argv[1], int(sys.argv[2])
with open(path, 'wb') as f:
    for i in range(mb):
        f.write(random.Random(i).randbytes(1 << 20))
PY
  printf 'tiny' > "$TMP/tiny.bin"
  big() { "$@" --headless 100000 --param stream=big --param reads=256 --param chunk=65536 --param frames=60 2>&1; }
  n=$(big "$NATIVE" $G --asset big="$TMP/big.bin" | line)
  m=$(big $NODE $G --asset big="$TMP/big.bin" | line)
  check "large asset: file-backed == in-memory (${LARGE_MB} MB, 1 GB of random reads)" "$m" "$n"
  rss() { # max resident set size in MB of a command, or "?" if /usr/bin/time can't tell
    if /usr/bin/time -l true >/dev/null 2>&1; then
      /usr/bin/time -l "$@" 2>&1 >/dev/null | awk '/maximum resident set size/ {printf "%.1f", $1/1048576}'
    elif /usr/bin/time -v true >/dev/null 2>&1; then
      /usr/bin/time -v "$@" 2>&1 >/dev/null | awk -F: '/Maximum resident set size/ {printf "%.1f", $2/1024}'
    else echo "?"; fi
  }
  r_big=$(rss "$NATIVE" $G --asset big="$TMP/big.bin" --headless 100000 --param stream=big --param reads=256 --param chunk=65536 --param frames=60)
  r_tiny=$(rss "$NATIVE" $G --asset big="$TMP/tiny.bin" --headless 100000 --param stream=big --param reads=256 --param chunk=65536 --param frames=60)
  echo "      resident memory (native, max RSS): ${LARGE_MB} MB asset ${r_big} MB, tiny asset ${r_tiny} MB"
fi

echo "$pass passed, $fail failed"
[ "$fail" -eq 0 ]
