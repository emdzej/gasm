# User guide

How to run gasm games natively, in the browser, and online, and how to build
everything from source.

## Quickest start: no build needed

- **In the browser:** [play the demos](/demos/) (sumo needs WebGPU).
- **Prebuilt:** download from [GitHub Releases](https://github.com/emdzej/gasm/releases):
  - `gasm-<version>-macos-apps.zip`: **Sumo.app**, **NES.app**, **Triangle.app**,
    **Test Pattern.app**. They're unsigned: right-click → **Open** the first
    time, or run `xattr -dr com.apple.quarantine Sumo.app`. Logs go to
    `~/Library/Logs/gasm/`.
  - `gasm-<version>-<platform>` archives (macOS universal, Linux x86_64 and
    arm64, Windows): `gasm-run`, `gasm-relay`, the games, and `run-sumo` /
    `run-nes` / `run-doom` / `run-scummvm` / `run-relay` / `run-triangle` scripts.
  - `gasm-<version>-games-wasm.zip`: just the games, with their license notices
    (`THIRD-PARTY.txt`). The DOOM and ScummVM source archives are next to them.

The rest of this guide builds from source.

## 1. Install prerequisites

| Tool | Why | Check |
|---|---|---|
| rustup (+ `stable` toolchain) | Rust games (wasm32) and the native runner | `rustup --version` |
| Node.js ≥ 22 | headless runner, tests (needs the built-in `WebSocket`) | `node --version` |
| Python 3 | local web server for the browser runner | `python3 --version` |
| git, curl, make | fetching sources/toolchain | — |

- **Linux runtime:** `gasm-run` needs ALSA (`libasound2`), `libudev1`, a
  Vulkan (or GL) driver such as Mesa, and X11 or Wayland. Building needs
  `libasound2-dev libudev-dev pkg-config`.
- **Windows:** `gasm-run.exe` uses Direct3D 12 (or Vulkan) and WASAPI; no
  extra installs. Release builds are unsigned, so SmartScreen may ask once.
- **Rust from Homebrew isn't enough** for the games: it has no wasm targets.
  Install rustup (`brew install rustup` then `rustup-init`, or see rustup.rs).
  The Makefile adds the `wasm32-unknown-unknown` target to the `stable`
  toolchain automatically. A different toolchain works too: `make TOOLCHAIN=nightly`.
- **wasi-sdk** is downloaded into `tools/wasi-sdk` by the first `make`. It
  compiles the C example and provides the `wasm-ld` linker used for the Rust
  games. To use your own copy, pass `make WASI_SDK=/path/to/wasi-sdk`.
- Browser runner: any browser with **WebGPU** for sumo (Chrome/Edge 113+,
  Safari 26+, Firefox 141+ on Windows). NES and the test pattern only need a
  2D canvas.

## 2. Build

```sh
make          # games (build/*.wasm) + native runner + relay
make roms     # optional: test ROMs, homebrew demos, Freedoom and shareware DOOM into roms/
```

| File | What |
|---|---|
| `build/sumo.wasm` | 3D two-player sumo (GPU + network) |
| `build/nes.wasm` | NES emulator (tetanes-core); needs a ROM |
| `build/doom.wasm` | DOOM (doomgeneric); needs a WAD |
| `build/test-pattern.wasm` | Tiny C demo: gradient, movable square, tone on A |
| `runners/native/target/release/gasm-run` | Native runner |
| `runners/native/target/release/gasm-relay` | Room relay for online play |

## 3. Play natively

```sh
R=runners/native/target/release/gasm-run
$R build/sumo.wasm                                  # vs. bot
$R build/sumo.wasm --param mode=local2              # two players: one keyboard, or two gamepads
$R build/nes.wasm --rom roms/bladebuster.nes
$R build/nes.wasm --rom ~/path/to/your-game.nes --param filter=ntsc
$R build/doom.wasm --asset wad=roms/doom1.wad
$R build/doom.wasm --asset wad=~/Games/DOOM2.WAD --param "args=-warp 7 -skill 4"
```

ScummVM takes a game folder: `$R build/scummvm.wasm --asset-dir ~/Games/MI1 --param "args=--auto-detect -p /"`
(`make roms` fetches the freeware Beneath a Steel Sky to `roms/bass/`: `--param "args=-p / sky"`).
Hold **Esc** to quit; **Ctrl+F5** is ScummVM's menu. See
[guests/scummvm](https://github.com/emdzej/gasm/blob/main/guests/scummvm/README.md).

DOOM takes any IWAD as the asset `wad` (shareware `doom1.wad`, `doom.wad`,
`doom2.wad`, Freedoom, ...). `args` is a DOOM command line. See
[guests/doom](https://github.com/emdzej/gasm/blob/main/guests/doom/README.md)
for its controls.

The window is resizable. Its title shows the game's frame rate. **Hold Esc**
for a second, or close the window, to quit (a short tap of Esc goes to the
game, for example DOOM's menu).

### Games with lots of data

Point the runner at a folder instead of listing files. Every file under it
becomes an asset named by its relative path, and nothing is loaded up front:
files are read on demand, so a 300 MB CD costs no memory and no start-up
time.

```sh
$R game.wasm --asset-dir /Volumes/GAMECD              # a mounted CD or ISO
$R game.wasm --asset-dir cd=~/Games/data              # names become cd/<path>
```

Names match case-insensitively (`Art/art.car` finds `ART/ART.CAR`). Hidden
files and symlinks are skipped, and an explicit `--asset name=path` overrides a
folder entry with the same name.

Data that changes while the game runs (fetched by a script, rebuilt by a tool):
`--watch-asset name=path` re-reads the file whenever it changes and gives the
game a new [asset version](/docs/abi#assets-that-change). Write it by renaming
a finished file over it, so the game never reads a half-written one:

```sh
$R game.wasm --watch-asset weather.json=build/weather.json &
curl -s "$URL" > build/weather.tmp && mv build/weather.tmp build/weather.json
```

## 4. Play in the browser

```sh
make web
# open http://localhost:8080/runners/web/
```

1. Pick a game in the first drop-down.
2. **Sumo:** leave the relay field empty to play the bot, or enter a relay URL
   to play online (next section).
   **NES:** pick a ROM from the list (from `roms/`), click **open .nes…**, or
   drag and drop a `.nes` file onto the screen.
   **DOOM:** pick a WAD from the list (`doom1.wad` by default), click
   **open .wad…**, or drop a `.wad` file onto the screen.
3. Click **▶ start**. Browsers only allow audio after a click, so the first
   start must be a click.

<figure class="shot-wide"><img src="/screenshots/player.webp" alt="The browser player running ScummVM" loading="lazy"><figcaption>The browser player: game, files, Worker mode, keyboard layout and the 2D filter at the top.</figcaption></figure>

URL parameters are passed to the game: `?game=sumo.wasm&relay=ws://host:9000&room=abc&autostart`.
`game`, `autostart`, `wasm`, `worker`, `opfs`, `prefix`, `rom`, `filter`,
`integer`, `asyncify` and `hashframes` are used by the page itself; everything else becomes
a game parameter.

**Scaling 2D games:** the filter drop-down (or `?filter=`) picks how frames
are scaled up, as `gasm-run --filter` does: `sharp` (the default: even pixels
at any size, exactly like `nearest` at whole multiples), `nearest` (plain
pixel doubling), `xbr` (smooth edges for pixel art), `fsr` (AMD FSR 1:
upscaling with sharpening, for rendered or dithered games such as DOOM) or
`crt` (scanlines and an aperture grille). **integer**
(or `?integer`) scales by whole multiples only. The choice is remembered.
Filters other than `nearest` need WebGL 2 (without it the page falls back to
`nearest`); 3D games aren't affected.

<figure class="shot-wide"><img src="/screenshots/filters.webp" alt="sharp, xbr, fsr and crt" loading="lazy"><figcaption>The same DOOM frame with sharp, xbr, fsr and crt.</figcaption></figure>

**Folders and big data sets in the browser:**

- **open folder…** gives the game a folder as assets, named like
  `--asset-dir`. It uses the folder picker in Chromium and falls back to
  `webkitdirectory` elsewhere.
- **worker** runs the game in a Worker. 3D games move there too where the
  browser has WebGPU in workers (Chromium); elsewhere they fall back to the
  main thread. Assets are then
  read on demand instead of loaded into memory first.
- For data you use repeatedly (a game CD), import it once on the **OPFS**
  page, `…/opfs.html`. It copies the folder into the site's private
  storage; after that, `index.html?game=<game>.wasm&opfs=gasm-assets/<name>`
  starts instantly, with no prompt and no preloading, even after a
  reload.

ROMs you open are read locally by the page and never uploaded.

## 5. Play sumo online

Players meet in a **room** on a **gasm-relay**. Any mix works: native↔native,
browser↔browser, native↔browser.

```sh
# on any machine both players can reach (LAN or a server)
runners/native/target/release/gasm-relay 0.0.0.0:9000       # or: make relay

# player 1 (native)
$R build/sumo.wasm --allow-net --param relay=ws://RELAY_HOST:9000 --param room=friday

# player 2 (browser): relay field = ws://RELAY_HOST:9000, room = friday, then start
```

- The first player to join is **red**, the second **blue**. Each player's camera
  looks from their own side, and arrows are relative to the screen.
- Until the opponent arrives, your ball pulses and the log says *waiting for an
  opponent*. If a player leaves, the other returns to waiting.
- The native runner needs `--allow-net`; without it, network access is denied
  and the game falls back to the bot.
- Inputs are delayed by 4 frames (~66 ms) to hide network latency. That
  feels fine on a LAN; over long distances the game briefly pauses when an
  input is late (lockstep: nobody ever sees a different game state). The log
  prints *in sync at frame N* every 20 s. *DESYNC* would indicate a bug.
- Both runners support `wss://` (TLS). Native uses the operating system's
  trusted certificates (set `SSL_CERT_FILE` to use a specific CA bundle).
  Browsers *require* `wss://` when the page is served over HTTPS, as on gasm.emdzej.pl.

## 6. Controls

| gasm button | Keyboard | Gamepad (position) | NES | Sumo | DOOM |
|---|---|---|---|---|---|
| D-pad | Arrow keys | D-pad / left stick | D-pad | move | move, turn |
| A | X | East (right face) | A | dash | fire; select in menus |
| B | Z | South (bottom face) | B | dash | use (doors); back in menus |
| X | S | North (top face) | — | — | next weapon |
| Y | A | West (left face) | — | — | run |
| L / R | Q / W | LB / RB | — | — | strafe |
| Start | Enter | Start | Start | — | menu; select in menus |
| Select | Right Shift (web: either Shift) | Select/Back | Select | — | automap |

The DOOM column is for gamepads. With a keyboard, DOOM reads the keys directly
and uses its original layout: arrows move, **Ctrl** fires, **Space** opens
doors, **Shift** runs, **Alt** + arrows or `,` `.` strafe, **1**–**7** pick
weapons, **Tab** shows the map, a tap of **Esc** opens the menu. While you
play, the mouse turns, the left button fires, the right one strafes and the
middle one moves forward (the cursor is captured; open the menu to free it).

Keyboard and the first gamepad both drive player 1. **Player 2 has its own
keys by default:** I/J/K/L move, `.` = A, `,` = B, M = X, N = Y, U/O = L/R,
Right Ctrl or keypad Enter = Start, Backspace = Select. So two people can
share one keyboard (sumo: `--param mode=local2`, or `?mode=local2` in the
browser). Gamepads take players in connection order. The player-2 keys apply
while fewer than two gamepads are connected.

**Change the layout:** the web player's **keys…** button opens an editor
(saved in your browser). Natively, `gasm-run --print-keymap > keymap.txt`,
edit it, then `--keymap keymap.txt`, or save it as
`<data dir>/gasm/keymap.txt` (see [Saves](#saves)) to make it the default.
The format is one line per binding, `<pad 1-4> <button> <key code>...`,
with [KeyboardEvent.code](https://developer.mozilla.org/en-US/docs/Web/API/UI_Events/Keyboard_event_code_values)
names; both runners use the same files. **Sumo tips:** dashing has a cooldown (the small white orb
above your ball shows it's ready). A dash into the opponent pushes much
harder than rolling into them. Keep away from the edge.

## 7. `gasm-run` reference

```
gasm-run <game.wasm|game.cwasm> [options]

--rom <path>             shorthand for --asset rom=<path>
--asset <name>=<path>    expose a file to the game as asset <name> (repeatable; read on demand)
--asset-dir [prefix=]dir expose every file under dir (repeatable; case-insensitive names)
--keymap <file>          keyboard layout (default <data dir>/gasm/keymap.txt, else built-in)
--print-keymap           print the active keyboard layout and exit
--param <name>=<value>   launch parameter for the game (repeatable)
--allow-net              let the game open network connections
--storage-dir <dir>      where saves live (default: see "Saves" below)
--storage-id <id>        save namespace (default: the game file's name)
--window <W>x<H>         initial window size (default 960x720)
--filter <name>          how 2D frames are scaled up: sharp (default), nearest, xbr, fsr, crt
--integer-scale          scale 2D frames by whole multiples only (black border around)
--mute                   no audio output
--compile <out.cwasm>    ahead-of-time compile to native code and exit
--allow-precompiled      accept a .cwasm (native code: only files you compiled yourself)
--call-timeout <secs>    trap a game call (init, a frame) that runs longer (default 30, 0 = never)
--gl-lib <dir>           where ANGLE is, for OpenGL ES (gasm:gl) games (default: next to gasm-run)
--gl-software            gasm:gl on SwiftShader (software) instead of the GPU
--window-screenshot <frames>:<out.png>
                         write that frame as the window shows it, then quit (gasm:gl games)
--headless <N>           run N frames with no window or audio; print hashes
--screenshot <out.png>   (headless) save the last frame (GPU games render offscreen)
--screenshot-filtered <out.png>
                         (headless) save the last frame as the window shows it
                         (--filter, --integer-scale at --window size)
--input <script>         (headless) scripted input, see below
--realtime               (headless) run at the game's frame rate instead of flat out
--no-hash                (headless) skip hashing (for benchmarks)
```

Game parameters:

| Game | Parameter | Meaning |
|---|---|---|
| sumo | `relay=ws://host:port` | play online through a relay |
| sumo | `room=name` | relay room (default `sumo`) |
| sumo | `mode=local2` | offline two-player on one machine (default: vs. bot) |
| sumo, nes | `quit_at=N` | exit after N frames (tests) |
| nes | `filter=ntsc` | NTSC composite filter (default: sharp pixels) |
| doom | `args=...` | a DOOM command line (`-warp 1 3 -skill 4`, `-playdemo demo2`, ...) |
| scummvm | `args=...` | a ScummVM command line; game files are at `/` (`-p / sky`, `--auto-detect -p /`) |

### Saves

Games can keep saves, settings and scores (`gasm:storage`). Sumo keeps your
win/loss record; the NES emulator keeps battery-backed cartridge saves
(`sram-<rom hash>`, the raw save RAM, compatible with `.srm` files).

| Runner | Location |
|---|---|
| macOS | `~/Library/Application Support/gasm/<game>/` |
| Linux | `$XDG_DATA_HOME/gasm/<game>/` (usually `~/.local/share/gasm/<game>/`) |
| Windows | `%APPDATA%\gasm\<game>\` |
| Browser | IndexedDB (database `gasm`) for the site you play on |
| Headless | in memory only (unless `--storage-dir` is given) |

`<game>` is the file name without extension (`sumo`, `nes`). Delete the
folder to reset. Games save on change and when you quit normally (close the
window or hold Esc).

### Precompiling (AOT)

```sh
$R build/nes.wasm --compile build/nes.cwasm
$R build/nes.cwasm --allow-precompiled --rom roms/bladebuster.nes
```

A `.cwasm` starts faster and needs no JIT. It only works with the same
`gasm-run` build on the same CPU architecture. It contains native code that
isn't sandboxed, so `gasm-run` loads one only with `--allow-precompiled`.
**Only run `.cwasm` files you compiled yourself.**

### Headless runs and input scripts

```sh
$R build/nes.wasm --rom roms/bladebuster.nes --headless 2400 \
   --input "100-104:START,200-204:START,300-2400:RIGHT+A" --screenshot out.png
```

Output:

```
frames=2400 presented=2400 size=256x240
video_fnv32=fd15bbc8 audio_fnv32=56acada3 audio_frames=1916721
```

Input script syntax: comma-separated `FROM-TO:ACTION`, frame numbers
starting at 0, ranges inclusive (`N:...` alone means a single frame). Actions:
`A+B+START` (player 1's pad: `A B X Y L R SELECT START UP DOWN LEFT RIGHT`),
`"text"` (typed text), `KEY(ShiftLeft+ArrowLeft)` (raw keys),
`PTR(x,y)` / `PTR(x,y,L+R)` (mouse position and buttons), `MOVE(dx,dy)`,
`WHEEL(x,y)` and `GP0(B0+A1=0.5)` (raw gamepad). Details in the
[ABI spec](/docs/abi#scripted-input-headless).

```sh
$R build/doom.wasm --asset wad=roms/freedoom2.wad --param "args=-warp 1" --headless 520 \
   --input '40-200:KEY(ControlLeft+ArrowUp),300-400:MOVE(12,0),470:KEY(Escape)'
```

The Node runner takes the same headless options: `--headless`, `--rom`,
`--asset`, `--asset-dir`, `--param`, `--allow-net`, `--storage-dir`,
`--storage-id`, `--input`, `--screenshot`, `--realtime` and `--no-hash`
(unknown options are an error). Node has no GPU, so its `--screenshot` only
captures `video_present` frames; use `gasm-run` for GPU games.

```sh
node runners/web/headless.mjs build/sumo.wasm --headless 100000 --param quit_at=3000
```

### `gasm-relay`

```
gasm-relay [addr:port] [--max-peers N] [--max-clients N]   (default 0.0.0.0:9000, 2 peers per room, 256 clients)
gasm-relay 0.0.0.0:9443 --tls-cert fullchain.pem --tls-key privkey.pem   # serve wss:// directly
```

`gasm-relay` comes with the release bundles and as the container image
`ghcr.io/emdzej/gasm-relay` (`docker run -p 9000:9000 ghcr.io/emdzej/gasm-relay`).

For a public relay, use a real certificate, e.g. from Let's Encrypt
(`certbot certonly --standalone -d relay.example.com`, then point `--tls-cert`
at `fullchain.pem` and `--tls-key` at `privkey.pem`). Players then use
`wss://relay.example.com:9443`. Putting gasm-relay behind a TLS reverse proxy
(Caddy, nginx) works too.

Clients connect to `ws://host:port/<room>`. The relay forwards messages
between the peers in a room and announces joins and leaves. It knows nothing
about the games. Handshakes time out after 10 s, and a peer that stops reading
is disconnected instead of buffering everyone's traffic.

### Godot games

Godot 4.7 games come as a `.pck` and run on the Godot engine module
(`games/godot.wasm` in the release bundles):

```sh
./run-godot.sh mygame.pck                          # release bundle (run-godot.cmd on Windows)
gasm-run godot.wasm --asset game.pck=mygame.pck    # anywhere
```

The examples are in `games/godot/` (`hello2d`, `platformer`, `scene3d`, `ui`,
`audio`). In the browser player, choose **Godot: your game (.pck)** and open or
drop the file. The first native start compiles the engine (a few seconds);
`gasm-run godot.wasm --compile godot.cwasm` once, then run `godot.cwasm` with
`--allow-precompiled`, makes it instant. Making games for it:
[Godot games on gasm](/dev/godot).

## 8. Which ROMs work

The NES game uses tetanes-core, an accurate, cycle-based emulator that supports
the common mappers (NROM, MMC1, UxROM, CNROM, MMC3, MMC5, AxROM, and many
more; see the [tetanes project](https://github.com/lukexor/tetanes)). Use
iNES `.nes` files. It passes the blargg CPU, timing and APU test
ROMs. Battery-backed saves are kept (see [Saves](#saves)). For commercial games, use dumps
of cartridges you own.

### DOOM WADs

Any DOOM IWAD works: the shareware `doom1.wad` (`make roms` fetches it; id
Software made it freely distributable), `doom.wad` (registered or Ultimate),
`doom2.wad`, `plutonia.wad`, `tnt.wad`, and the free
[Freedoom](https://freedoom.github.io/) IWADs. For the commercial ones, use
your own copies.

## 9. Troubleshooting

| Symptom | Fix |
|---|---|
| `rustup toolchain 'stable' not found` | Install rustup and run `rustup toolchain install stable`. |
| `no asset named "rom"` | Pass `--rom file.nes` (native) or choose a ROM (web). |
| `cannot load rom` | Not a valid iNES file, or an unsupported mapper. |
| sumo: *network not allowed?* | Add `--allow-net` (native). |
| sumo: *waiting for an opponent* forever | Both players need the same relay URL **and** room; check the relay's log for joins. |
| sumo: *room is full* | Two players are already in that room; pick another room name. |
| Web: `WebGPU is not available` | Use a WebGPU-capable browser, or enable it (Firefox: `dom.webgpu.enabled`; Linux Chrome: `--enable-unsafe-webgpu`). |
| Web: `HTTP 404` for `build/*.wasm` | Serve the **repo root** (`make web`), not `runners/web/`. Build the games first. |
| `ANGLE (libEGL…) not found` | An OpenGL ES (`gasm:gl`) game needs ANGLE next to `gasm-run`: it is in the release bundles; in a repository build run `scripts/fetch-angle.sh`, or pass `--gl-lib DIR`. |
| OpenGL ES game: `no display` / `eglInitialize failed` | No usable GPU driver: gasm-run tries SwiftShader next (`--gl-software` forces it). |
| OpenGL ES game draws wrong or crashes | Try another ANGLE backend: `GASM_ANGLE_BACKEND=opengl` (macOS), `vulkan`, `d3d11` (Windows), `metal`. In macOS virtual machines gasm-run picks `opengl` itself. |
| No sound (native) | Check `[gasm] audio:` on stderr; `audio disabled: …` explains why. |
| No sound (web) | Click the page / **start** button; autoplay policies block audio before interaction. |
| `.cwasm` is refused | Add `--allow-precompiled` (only for files you compiled yourself). |
| `.cwasm` fails to load | It was built by a different `gasm-run` version; recompile it. |
| A game stops with a timeout | One call ran longer than `--call-timeout` (30 s); raise it, or `0` to turn it off. |
| macOS: "developer cannot be verified" for wasi-sdk | `xattr -dr com.apple.quarantine tools/wasi-sdk` (the fetch script does this). |
