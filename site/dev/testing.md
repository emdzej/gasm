# Testing and contributing

## Test suites

| Command | What it proves | Time |
|---|---|---|
| `scripts/determinism-test.sh` | Every case gives identical video, audio and GPU-upload hashes on **wasmtime JIT**, **wasmtime AOT** and **V8 (Node)** | ~75 s |
| `scripts/net-test.sh` | Full online sumo matches through `gasm-relay` for native↔native, native↔Node, Node↔native, and native↔Node over **TLS** (`wss://`, throwaway CA) end in the **identical game state**, with no desync | ~15 s |
| `make test` | Both of the above | |
| `make parity ROM=… FRAMES=…` | The NES game built **natively** (Rust, stub host) matches the wasm build | ~1 min first build |
| `node scripts/web-smoke.mjs <url> <out.png> [secs]` | The browser runner loads and runs in headless Chrome (including WebGPU); prints status, fps and console, saves a screenshot | ~10 s |

### Linux locally (Apple `container`)

On Apple silicon (macOS 26+), the whole suite also runs on Linux arm64 in a
container. It uses your read-only source; builds go to a named volume, so your
macOS `target/` dirs are untouched.

```sh
scripts/linux-container.sh            # build + all tests (NET_REPEAT=5 to stress the network test)
scripts/linux-container.sh build      # just build; binaries land in dist/linux-arm64/
scripts/linux-container.sh shell      # interactive shell in the synced tree
CPUS=8 MEMORY=12g scripts/linux-container.sh
```

The image (`container/linux.Containerfile`: Debian + Node 22 + rustup) holds only
toolchains. It also works with Docker/Podman
(`docker build -f container/linux.Containerfile container/`). Headless tests
need no display or GPU. CI runs the same suites on Linux x86_64, **Windows**
and **macOS** runners (`.github/workflows/ci.yml`, job `platforms`), so the
Windows runner is verified bit-identical too.

### Determinism test

Cases: the C test pattern, sumo vs. the bot with scripted input (GPU uploads
are hashed, covering the whole scene), the blargg CPU instruction, CPU timing
and APU tests, spritecans, Quantum Disco Brothers, and 2,400 frames of Blade
Buster gameplay with scripted input. It fetches ROMs if missing and
AOT-compiles the games first.

```sh
check <name> <game> <frames> [runner args...]
check my_game nes 1800 --rom roms/my_game.nes --input "60-65:START,120-900:RIGHT"
```

A mismatch means one of:

1. the game reads nondeterministic state (clock, randomness, `HashMap` order),
2. a runner bug (wrong copy stride, hashing padding, mis-ordered input), or
3. an engine bug (rare; reduce to a minimal module and report upstream).

### Network test

It starts `gasm-relay` on port 9123 and, for each runner pair, launches two
headless sumo peers in a fresh room with different held inputs, running as
fast as lockstep allows. Each peer logs `frame N state=… score=a:b` at
`quit_at` and exits. The test requires identical lines and no `DESYNC`. Each
peer has a hard time limit (`LIMIT`, default 60 s), so a stall fails the test
instead of hanging it.

```sh
QUIT=3600 LIMIT=40 PORT=9200 scripts/net-test.sh
```

Browser cross-play is checked by hand with `web-smoke.mjs` against a native
peer:

```sh
gasm-relay 127.0.0.1:9125 &
gasm-run build/sumo.wasm --headless 100000000 --allow-net \
  --param relay=ws://127.0.0.1:9125 --param room=x --param quit_at=1200 --input "0-100000000:RIGHT+A" &
node scripts/web-smoke.mjs "http://localhost:8765/runners/web/?game=sumo.wasm&autostart&relay=ws://127.0.0.1:9125&room=x&quit_at=1200" out.png 24
# both print: [sumo] frame 1200 state=<same hash>
```

### Parity test

`guests/parity` runs the NES game as a normal native binary against the `gasm`
crate's stub host and prints the same hash lines as the runners. If wasm and
native builds disagree, suspect undefined behaviour, pointer-size assumptions
(`usize` is 4 bytes in wasm32), or transcendental float functions (native libm
vs the libm compiled into the module).

### Visual checks

Every headless run can take `--screenshot out.png`. GPU games render the last
frame on a real GPU offscreen. Useful ROMs from `make roms`:

| ROM | Expected screen |
|---|---|
| `cpu_instr_test.nes` (≈3000 frames) | `All 16 tests passed` |
| `cpu_timing_test.nes` (≈1200 frames) | `PASSED` |
| `apu_test.nes` (≈1500 frames) | `All 8 tests passed` |
| `bladebuster.nes` | Title screen; with the input script from the test, gameplay |

### Benchmarks

```sh
R=runners/native/target/release/gasm-run
$R build/nes.wasm --compile build/nes.cwasm
(cd guests && cargo build --release -p parity)
time guests/target/release/parity roms/bladebuster.nes 6000 --no-hash
time $R build/nes.cwasm --rom roms/bladebuster.nes --headless 6000 --no-hash
time node runners/web/headless.mjs build/nes.wasm --rom roms/bladebuster.nes --headless 6000 --no-hash
```

Always use `--no-hash` for speed measurements: hashing 245 KB per frame costs a
lot of time.

## Repository conventions

- **ABI changes** go into `spec/ABI.md`, `spec/gasm.h`, the `gasm` crate *and*
  both runners in the same change. Breaking changes bump `GASM_ABI_VERSION`.
- **Hash format** (`frames=… / video_fnv32=… audio_fnv32=…`) is an interface
  between runners and scripts; keep it stable.
- **Game logic** that must be deterministic lives apart from rendering
  (`sumo/src/sim.rs`).
- **Dependencies:** wasi-sdk version in `scripts/fetch-wasi-sdk.sh`, runner crates
  in `runners/native/Cargo.toml`, game crates in `guests/*/Cargo.toml`.
- Generated or downloaded content (`build/`, `tools/`, `roms/`, `target/`,
  `site/node_modules`, `site/.vitepress/dist`, `site/public/play`) is git-ignored.
  Never commit ROMs.
- **Releases:** push a version tag without a `v` prefix (`git tag 0.2.0 && git push origin 0.2.0`).
  See [Releases and trusted publishing](#releases-and-trusted-publishing).
- **ABI:** edit `spec/abi.json`, run `node scripts/gen-abi.mjs` (regenerates `spec/gasm.h`
  and `guests/gasm/src/sys.rs`), then implement it in both runners and the native stub.
  CI runs `--check`.
- **Agents:** [`AGENTS.md`](https://github.com/emdzej/gasm/blob/main/AGENTS.md) lists build traps, invariants and conventions for coding agents.
- **Docs** live in `site/` (VitePress) and are published to
  [gasm.emdzej.pl](https://gasm.emdzej.pl) by GitHub Actions. `spec/ABI.md` is
  included into the site, not copied.

## Releases and trusted publishing

`release.yml` runs on a version tag (`0.2.0`). It builds everything, creates
the GitHub Release, and publishes `gasm-sdk` + `gasm-host` to crates.io,
`@emdzej/gasm-host` to npm, and `ghcr.io/emdzej/gasm-relay`. Versions come
from the tag, and versions that are already published are skipped, so re-running
a release is safe.

crates.io and npm use **trusted publishing**: the workflow gets a short-lived
token from the registry via GitHub OIDC (`id-token: write`, environment
`release`), and nothing secret is stored in the repository. The relay image is
pushed with the workflow's own `GITHUB_TOKEN`.

One-time setup, because both registries attach trusted publishers to a
*package*, which must exist first:

1. **First publish by hand** (once per package):
   ```sh
   cargo login                                   # crates.io API token
   (cd guests && cargo publish -p gasm-sdk)
   (cd runners/native && cargo publish)
   npm login                                     # pnpm publishes with npm's credentials
   (cd runners/web && pnpm publish --access public --no-git-checks --otp <code>)
   ```
2. **crates.io**, for `gasm-sdk` and for `gasm-host`: Settings → Trusted
   Publishing → Add → GitHub: owner `emdzej`, repository `gasm`, workflow
   `release.yml`, environment `release`.
3. **npm**, for `@emdzej/gasm-host`:
   `npm trust github @emdzej/gasm-host --repo emdzej/gasm --file release.yml --env release --allow-publish`
   (or Settings → Trusted Publisher on npmjs.com). Then, under Publishing
   access, disallow tokens. In CI, pnpm packs and `npm publish` uploads the
   tarball, because npm performs the OIDC exchange and adds provenance.
4. **GitHub** (optional): Settings → Environments → `release`, and add
   required reviewers or restrict it to tags.

## Updating dependencies

- **wasi-sdk:** `WASI_SDK_VERSION=NN scripts/fetch-wasi-sdk.sh`, then `make clean test`.
- **tetanes-core:** bump in `guests/nes/Cargo.toml`. Hashes may change if the core
  changed behaviour; the suite only requires runners to agree. Re-check the
  test ROM screens.
- **wasmtime, wgpu, winit:** bump in `runners/native/Cargo.toml`; wasmtime and
  wasmtime-wasi together. Old `.cwasm` files become invalid; the test script
  regenerates them.
