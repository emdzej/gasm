#!/usr/bin/env bash
# Runs inside the Linux container: sync the (read-only) source into the work
# volume, build everything, run the test suites, export the Linux binaries.
#   linux-in-container.sh [test|build|shell|gl]
set -euo pipefail
MODE=${1:-test}
mkdir -p /work/gasm
rsync -a --delete \
  --exclude .git --exclude build --exclude dist --exclude tools --exclude target \
  --exclude node_modules --exclude site/.vitepress/dist --exclude site/public/play \
  --filter 'protect guests/target/' --filter 'protect runners/native/target/' \
  --filter 'protect tools/' --filter 'protect build/' \
  /src/ /work/gasm/
cd /work/gasm
echo "== $(uname -srm), $(rustc --version), node $(node --version)"
[ "$MODE" = shell ] && exec bash
if [ "$MODE" = gl ]; then   # only gasm:gl on ANGLE (SwiftShader): the runner and gltest
  make build/gltest.wasm && (cd runners/native && cargo build --release)
  scripts/gl-native-test.sh --gl-software
  exit 0
fi
make
mkdir -p /out
cp runners/native/target/release/gasm-run runners/native/target/release/gasm-relay /out/
cp build/*.wasm /out/
echo "== Linux binaries exported: $(ls /out | tr '\n' ' ')"
[ "$MODE" = build ] && exit 0
[ -n "$(ls roms/*.nes 2>/dev/null)" ] || make roms
scripts/determinism-test.sh
for _ in $(seq "${NET_REPEAT:-1}"); do scripts/net-test.sh; done
make parity ROM=roms/cpu_instr_test.nes FRAMES=600
scripts/gl-native-test.sh --gl-software
echo "== all Linux tests passed"
