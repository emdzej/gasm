#!/usr/bin/env bash
# Build and test gasm on Linux in an Apple `container` (Apple silicon, macOS 26+).
#   scripts/linux-container.sh [test|build|shell]      (default: test)
# Linux arm64 binaries (gasm-run, gasm-relay) and the games end up in dist/linux-arm64/.
# Source is mounted read-only and synced into a named volume, so incremental
# builds are fast and your macOS target/ directories are never touched.
set -euo pipefail
cd "$(dirname "$0")/.."
IMAGE=gasm-linux:dev
CPUS=${CPUS:-6}
MEMORY=${MEMORY:-8g}

container system status >/dev/null 2>&1 || container system start
container build -t "$IMAGE" -f container/linux.Containerfile container/
for v in gasm-linux-work gasm-linux-cargo; do
  container volume inspect "$v" >/dev/null 2>&1 || container volume create "$v" >/dev/null
done
mkdir -p dist/linux-arm64
TTY=(); [ "${1:-test}" = shell ] && TTY=(-it)
container run --rm ${TTY[@]+"${TTY[@]}"} --name gasm-linux --cpus "$CPUS" --memory "$MEMORY" \
  -v "$PWD:/src:ro" \
  -v gasm-linux-work:/work \
  -v gasm-linux-cargo:/usr/local/cargo/registry \
  -v "$PWD/dist/linux-arm64:/out" \
  -e NET_REPEAT="${NET_REPEAT:-1}" \
  "$IMAGE" bash /src/scripts/linux-in-container.sh "${1:-test}"
