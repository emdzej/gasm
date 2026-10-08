#!/usr/bin/env bash
# Network lockstep test: gasm-relay + two headless sumo peers on different
# runners. Both must reach the same simulation state (and never log DESYNC).
# Godot: two peers of the net example (WebSocketPeer on gasm:net), native and Node,
# see each other's positions through a relay room.
set -uo pipefail
cd "$(dirname "$0")/.."
NATIVE=runners/native/target/release/gasm-run
RELAY=runners/native/target/release/gasm-relay
PORT=${PORT:-9123}
QUIT=${QUIT:-1800}
TMP=$(mktemp -d)
"$RELAY" 127.0.0.1:$PORT 2>"$TMP/relay.log" &
RELAY_PID=$!
trap 'kill $RELAY_PID 2>/dev/null; wait 2>/dev/null; rm -rf "$TMP"' EXIT
# wait (up to 10 s) until a relay says it is listening
wait_listening() { # <log>
  local i
  for i in $(seq 100); do grep -q listening "$1" 2>/dev/null && return 0; sleep 0.1; done
  echo "relay did not start: $(cat "$1")" >&2; exit 1
}
wait_listening "$TMP/relay.log"

# Each peer gets a hard time limit so a lockstep stall fails the test instead of hanging it.
LIMIT=${LIMIT:-60}
RELAY_URL=ws://127.0.0.1:$PORT
peer() { # <runner: native|node> <room> <input script> <log>
  local args=(build/sumo.wasm --headless 100000000 --allow-net --param relay=$RELAY_URL
              --param room=$2 --param quit_at=$QUIT --input "$3" --no-hash)
  if [ "$1" = native ]; then perl -e "alarm $LIMIT; exec @ARGV" "$NATIVE" "${args[@]}" </dev/null >/dev/null 2>"$4"
  else perl -e "alarm $LIMIT; exec @ARGV" node runners/web/headless.mjs "${args[@]}" </dev/null >/dev/null 2>"$4"; fi
}

pass=0; fail=0
run_case() { # <name> <runner A> <runner B>
  local room="t$RANDOM"
  peer "$2" "$room" "0-100000000:RIGHT+A" "$TMP/a.log" &
  local a=$!
  sleep 0.2
  peer "$3" "$room" "0-100000000:UP" "$TMP/b.log" &
  local b=$!
  wait $a; wait $b
  local sa sb
  sa=$(grep -o 'frame [0-9]* state=[0-9a-f]* score=[0-9:]*' "$TMP/a.log")
  sb=$(grep -o 'frame [0-9]* state=[0-9a-f]* score=[0-9:]*' "$TMP/b.log")
  if [ -n "$sa" ] && [ "$sa" = "$sb" ] && ! grep -q DESYNC "$TMP/a.log" "$TMP/b.log"; then
    pass=$((pass + 1)); printf 'PASS  %-18s %s\n' "$1" "$sa"
  else
    fail=$((fail + 1)); printf 'FAIL  %s\n  A: %s\n  B: %s\n' "$1" "$sa" "$sb"
    for f in a b; do echo "  --- peer $f"; grep -v "waiting for opponent input" "$TMP/$f.log" | tail -6 | sed 's/^/    /'; done
  fi
}

run_case native-native native native
run_case native-node   native node
run_case node-native   node   native

# Godot's WebSocketPeer: both peers get the other's position
if [ -f build/godot.wasm ] && [ -f build/godot/net.pck ]; then
  room="g$RANDOM"
  gpeer() { # <log> <runner command...>: until it has seen the other player for 2 s (or LIMIT)
    local log=$1; shift
    perl -e "alarm $LIMIT; exec @ARGV" "$@" build/godot.wasm --asset game.pck=build/godot/net.pck --headless 100000000 --realtime \
      --allow-net --param relay=ws://127.0.0.1:$PORT/$room --param quit_after=120 --input '0-100000000:KEY(ArrowRight)' --no-hash </dev/null >/dev/null 2>"$log"
  }
  gpeer "$TMP/ga.log" "$NATIVE" &
  ga=$!
  sleep 0.5
  gpeer "$TMP/gb.log" node runners/web/headless.mjs &
  gb=$!
  wait $ga; wait $gb
  # either may join first (compiling Godot takes a while): each sees the other's position
  if grep -q 'first position from player' "$TMP/ga.log" && grep -q 'first position from player' "$TMP/gb.log" \
     && [ "$(grep -o 'joined as [0-9]*' "$TMP/ga.log")" != "$(grep -o 'joined as [0-9]*' "$TMP/gb.log")" ]; then
    pass=$((pass + 1)); printf 'PASS  %-18s %s\n' "godot-websocket" "native and node see each other"
  else
    fail=$((fail + 1)); printf 'FAIL  godot-websocket\n'
    for f in ga gb; do echo "  --- $f"; grep -E 'net:|ERROR|gasm\]' "$TMP/$f.log" | tail -6 | sed 's/^/    /'; done
  fi
else
  echo "SKIP  godot-websocket (make godot)"
fi

# Godot's high-level multiplayer (RPCs) through the relay (Gasm.create_relay_peer): the
# first to join is the server, both get the other's RPCs
if [ -f build/godot-2d.wasm ] && [ -f build/godot/relaymp.pck ]; then
  room="m$RANDOM"
  mpeer() { # <log> <runner command...>
    local log=$1; shift
    perl -e "alarm $LIMIT; exec @ARGV" "$@" build/godot-2d.wasm --asset game.pck=build/godot/relaymp.pck --headless 100000000 --realtime \
      --allow-net --param relay=ws://127.0.0.1:$PORT/$room --param quit_after=120 --input '0-100000000:KEY(ArrowLeft)' --no-hash </dev/null >/dev/null 2>"$log"
  }
  mpeer "$TMP/ma.log" "$NATIVE" &
  ma=$!
  sleep 0.5
  mpeer "$TMP/mb.log" node runners/web/headless.mjs &
  mb=$!
  wait $ma; wait $mb
  if grep -q 'first position from peer' "$TMP/ma.log" && grep -q 'first position from peer' "$TMP/mb.log"; then
    pass=$((pass + 1)); printf 'PASS  %-18s %s\n' "godot-rpc" "native and node get each other's RPCs"
  else
    fail=$((fail + 1)); printf 'FAIL  godot-rpc\n'
    for f in ma mb; do echo "  --- $f"; grep -E 'mp:|ERROR|gasm\]' "$TMP/$f.log" | tail -6 | sed 's/^/    /'; done
  fi
else
  echo "SKIP  godot-rpc (make godot)"
fi

# Same over TLS (wss://): throwaway CA + relay certificate for localhost. Config
# files instead of -subj/-addext: portable across OpenSSL, old LibreSSL (macOS)
# and Git Bash (which rewrites "/CN=..." arguments into Windows paths).
if command -v openssl >/dev/null; then
  TLS_PORT=$((PORT + 1))
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
  openssl req -new -newkey rsa:2048 -nodes -config "$TMP/leaf.cnf" -keyout "$TMP/relay.key" -out "$TMP/relay.csr" 2>>"$TMP/openssl.log"
  openssl x509 -req -in "$TMP/relay.csr" -CA "$TMP/ca.pem" -CAkey "$TMP/ca.key" -CAcreateserial -days 1 \
    -extfile "$TMP/leaf.cnf" -extensions v3_leaf -out "$TMP/relay.pem" 2>>"$TMP/openssl.log"
  if [ -s "$TMP/relay.pem" ]; then
    "$RELAY" 127.0.0.1:$TLS_PORT --tls-cert "$TMP/relay.pem" --tls-key "$TMP/relay.key" 2>"$TMP/relay-tls.log" &
    TLS_PID=$!
    trap 'kill $RELAY_PID $TLS_PID 2>/dev/null; wait 2>/dev/null; rm -rf "$TMP"' EXIT
    wait_listening "$TMP/relay-tls.log"
    CA="$TMP/ca.pem"
    command -v cygpath >/dev/null && CA=$(cygpath -m "$CA") # native Windows programs need C:/... paths
    export SSL_CERT_FILE="$CA" NODE_EXTRA_CA_CERTS="$CA"
    RELAY_URL=wss://localhost:$TLS_PORT
    run_case tls-native-node native node
  else
    fail=$((fail + 1)); echo "FAIL  tls: could not create test certificates"; cat "$TMP/openssl.log"
  fi
else
  echo "SKIP  tls (no openssl)"
fi
echo "$pass passed, $fail failed"
[ "$fail" -eq 0 ]
