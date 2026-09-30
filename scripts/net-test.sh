#!/usr/bin/env bash
# Network lockstep test: gasm-relay + two headless sumo peers on different
# runners. Both must reach the same simulation state (and never log DESYNC).
set -uo pipefail
cd "$(dirname "$0")/.."
NATIVE=runners/native/target/release/gasm-run
RELAY=runners/native/target/release/gasm-relay
PORT=${PORT:-9123}
QUIT=${QUIT:-1800}
TMP=$(mktemp -d)
"$RELAY" 127.0.0.1:$PORT 2>"$TMP/relay.log" &
RELAY_PID=$!
trap 'kill $RELAY_PID 2>/dev/null; rm -rf "$TMP"' EXIT
sleep 0.3

# Each peer gets a hard time limit so a lockstep stall fails the test instead of hanging it.
LIMIT=${LIMIT:-60}
peer() { # <runner: native|node> <room> <input script> <log>
  local args=(build/sumo.wasm --headless 100000000 --allow-net --param relay=ws://127.0.0.1:$PORT
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
echo "$pass passed, $fail failed"
[ "$fail" -eq 0 ]
