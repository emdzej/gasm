#!/usr/bin/env bash
# gasm:media natively (design/media.md): build/mediatest.wasm against scripts/media-server.mjs
# (https with a throwaway CA, 127.0.0.1), headless in real time. Checks a live stream's states
# and ICY titles, a track's end, a redirect, a dropped stream's reconnect, pause and play,
# what isn't audio, and the refusals (no permission, a private address, a host not listed).
#   scripts/media-test.sh            (needs build/mediatest.wasm, the native runner, node, openssl)
set -euo pipefail
cd "$(dirname "$0")/.."
NATIVE=${NATIVE:-runners/native/target/release/gasm-run}
GUEST=build/mediatest.wasm
PORT=${MEDIA_PORT:-8463}
TMP=$(mktemp -d)
pass=0
fail=0

for need in "$NATIVE" "$GUEST"; do
  [ -f "$need" ] || { echo "media-test: $need is missing (make)"; exit 1; }
done
command -v openssl >/dev/null || { echo "SKIP  media (no openssl)"; exit 0; }

# a throwaway CA and a certificate for 127.0.0.1 (config files: portable, see net-test.sh)
cat > "$TMP/ca.cnf" <<'CNF'
[req]
distinguished_name = dn
prompt = no
x509_extensions = v3_ca
[dn]
CN = gasm test CA
[v3_ca]
basicConstraints = critical, CA:TRUE
keyUsage = critical, keyCertSign, cRLSign
subjectKeyIdentifier = hash
CNF
cat > "$TMP/leaf.cnf" <<'CNF'
[req]
distinguished_name = dn
prompt = no
[dn]
CN = localhost
[v3_leaf]
basicConstraints = CA:FALSE
keyUsage = critical, digitalSignature, keyEncipherment
extendedKeyUsage = serverAuth
subjectAltName = DNS:localhost, IP:127.0.0.1
CNF
openssl req -x509 -newkey rsa:2048 -nodes -days 1 -config "$TMP/ca.cnf" -keyout "$TMP/ca.key" -out "$TMP/ca.pem" 2>"$TMP/openssl.log"
openssl req -new -newkey rsa:2048 -nodes -config "$TMP/leaf.cnf" -keyout "$TMP/leaf.key" -out "$TMP/leaf.csr" 2>>"$TMP/openssl.log"
openssl x509 -req -in "$TMP/leaf.csr" -CA "$TMP/ca.pem" -CAkey "$TMP/ca.key" -CAcreateserial -days 1 \
  -extfile "$TMP/leaf.cnf" -extensions v3_leaf -out "$TMP/leaf.pem" 2>>"$TMP/openssl.log"
CA="$TMP/ca.pem"
command -v cygpath >/dev/null && CA=$(cygpath -m "$CA")
export SSL_CERT_FILE="$CA"

node scripts/media-server.mjs --port "$PORT" --cert "$TMP/leaf.pem" --key "$TMP/leaf.key" >"$TMP/server.log" 2>&1 </dev/null &
SERVER=$!
trap 'kill $SERVER 2>/dev/null; wait 2>/dev/null; rm -rf "$TMP"' EXIT
for _ in $(seq 50); do grep -q listening "$TMP/server.log" && break; sleep 0.1; done
grep -q listening "$TMP/server.log" || { echo "FAIL  media: the server didn't start"; cat "$TMP/server.log"; exit 1; }
BASE=https://127.0.0.1:$PORT

# run NAME FRAMES ARGS...: the guest's log in $TMP/NAME.log (each run limited to 60 s)
run() {
  local name=$1 frames=$2
  shift 2
  perl -e 'alarm 60; exec @ARGV' "$NATIVE" ${GAME:-$GUEST} --headless "$frames" --realtime "$@" >"$TMP/$name.out" 2>"$TMP/$name.log" || true
}
# expect NAME "what" PATTERN...: every pattern is in the log, in this order (the
# guest's lines: "[mediatest] ...", or Godot's "media: ...")
expect() {
  local name=$1 what=$2 log="$TMP/$1.log" rest
  shift 2
  rest=$(grep -E '\[mediatest\]|^media: ' "$log" || true)
  for p in "$@"; do
    if ! printf '%s\n' "$rest" | grep -q -- "$p"; then
      fail=$((fail + 1)); echo "FAIL  media-$name: $what (no \"$p\")"; grep -E 'mediatest|media' "$log" | sed 's/^/        /' | head -12
      return
    fi
    rest=$(printf '%s\n' "$rest" | sed -n "/$(printf '%s' "$p" | sed 's/[][\/.*^$]/\\&/g')/,\$p" | sed 1d)
  done
  pass=$((pass + 1)); echo "PASS  media-$name  $what"
}

run live 330 --allow-media=127.0.0.1 --param url=$BASE/live.mp3
# the first title can come before PLAYING (ICY metadata arrives with the connection, PLAYING
# after 0.25 s decoded), so it is checked on its own; Song 2 comes 2 s later
expect live "plays, with changing titles" \
  'state Connecting' 'state Playing (volume true, lowpass true, metadata true)' 'title=Some("gasm test - Song 2")' 'position'
cp "$TMP/live.log" "$TMP/live-name.log"; expect live-name "the station's name and its first title" 'name=Some("gasm test radio") title=Some("gasm test - Song 1")'

run track 300 --allow-media=127.0.0.1 --param url=$BASE/track.mp3
expect track "a track plays to its end" 'state Playing' 'state Ended at 3.0 s'

run redirect 180 --allow-media=127.0.0.1 --param url=$BASE/redirect
expect redirect "a redirect is followed (and checked)" 'state Playing' 'title=Some("gasm test - Song 1")'

run drop 330 --allow-media=127.0.0.1 --param url=$BASE/drop.mp3
expect drop "a dropped live stream reconnects" 'state Playing' 'state Connecting' 'state Playing'

run pause 300 --allow-media=127.0.0.1 --param url=$BASE/live.mp3 --param pause=120 --param resume=180
expect pause "pause and play again" 'state Playing' 'state Paused' 'state Playing'

run notaudio 120 --allow-media=127.0.0.1 --param url=$BASE/notaudio
expect notaudio "a page that isn't audio fails at once" 'state Failed: .*not a stream gasm can play'

run missing 120 --allow-media=127.0.0.1 --param url=$BASE/nothing
expect missing "a 404 fails at once" 'state Failed: .*HTTP 404'

run unasked 10 --param url=$BASE/live.mp3
expect unasked "no --allow-media headless: refused" 'refused: media streams need --allow-media here'

run private 60 --allow-media --param url=$BASE/live.mp3
expect private "a private address not listed is refused" 'state Failed: 127.0.0.1 is a private or local address'

run unlisted 60 --allow-media=radio.example --param url=$BASE/live.mp3
expect unlisted "a host not in the list is refused" "state Failed: 127.0.0.1 isn't in --allow-media's list"

run plainhttp 10 --allow-media=127.0.0.1 --param url=http://127.0.0.1:$PORT/live.mp3
expect plainhttp "http (not https) is refused" 'refused: only https:// streams'

# Godot: the Gasm singleton's media_* calls (guests/godot/examples/media)
if [ -f build/godot.wasm ] && [ -f build/godot/media.pck ]; then
  GAME="build/godot.wasm --asset game.pck=build/godot/media.pck" run godot 330 --allow-media=127.0.0.1 --param url=$BASE/live.mp3
  expect godot "Godot plays it through the Gasm singleton" 'media: opened' 'media: state playing' 'title gasm test - Song 2'
  cp "$TMP/godot.log" "$TMP/godot-title.log"; expect godot-title "Godot gets the first title" 'media: opened' 'title gasm test - Song 1'
  GAME="build/godot.wasm --asset game.pck=build/godot/media.pck" run godot-refused 30 --param url=$BASE/live.mp3
  expect godot-refused "Godot hears why it was refused" "media: refused: this game's manifest doesn't ask"
else
  echo "SKIP  media-godot (make godot)"
fi

echo "$pass passed, $fail failed"
[ "$fail" -eq 0 ]
