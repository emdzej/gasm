# Linux build/test environment for gasm (arm64 on Apple silicon via Apple `container`,
# also works with Docker/Podman). Only toolchains live in the image; the source is
# mounted at run time (see scripts/linux-container.sh).
FROM docker.io/library/node:22-bookworm

RUN apt-get update && apt-get install -y --no-install-recommends \
      build-essential pkg-config curl ca-certificates git rsync python3 perl \
      libasound2-dev libudev-dev unzip \
    && rm -rf /var/lib/apt/lists/*

ENV RUSTUP_HOME=/usr/local/rustup CARGO_HOME=/usr/local/cargo PATH=/usr/local/cargo/bin:$PATH
RUN curl -fsSL https://sh.rustup.rs | sh -s -- -y --profile minimal --default-toolchain stable \
      --target wasm32-unknown-unknown \
    && rustc --version && cargo --version && node --version

WORKDIR /work
