#!/usr/bin/env bash
# Build and test gasm on Linux in an Apple `container` (Apple silicon, macOS 26+).
#   scripts/linux-container.sh [test|build|shell|gl]   (default: test; gl: only gasm:gl on ANGLE)
#   scripts/linux-container.sh clean     remove what this leaves behind (several GB):
#                                        the volumes, the image, the builder container
#                                        and its image, then stop the container system
#                                        (other projects' containers and volumes stay)
# Linux arm64 binaries (gasm-run, gasm-relay) and the games end up in dist/linux-arm64/.
# Source is mounted read-only and synced into a named volume, so incremental
# builds are fast and your macOS target/ directories are never touched.
set -euo pipefail
cd "$(dirname "$0")/.."
IMAGE=gasm-linux:dev
CPUS=${CPUS:-6}
MEMORY=${MEMORY:-8g}

if [ "${1:-}" = clean ]; then
  container rm -f gasm-linux 2>/dev/null || true
  container volume delete gasm-linux-work gasm-linux-cargo 2>/dev/null || true
  container image delete "$IMAGE" 2>/dev/null || true
  container builder stop 2>/dev/null || true
  container builder delete 2>/dev/null || true
  # the image builder's own image (re-downloaded by the next `container build`)
  container image list -q 2>/dev/null | grep '^ghcr.io/apple/container-builder-shim/' | xargs container image delete 2>/dev/null || true
  container system stop 2>/dev/null || true
  echo "removed the gasm Linux volumes, image and builder"
  exit 0
fi

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
