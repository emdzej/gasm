# ScummVM for gasm

[ScummVM](https://www.scummvm.org/) compiled to a single gasm guest,
`build/scummvm.wasm`, with a gasm backend. This directory holds gasm's own files:
the backend (`backend/`, copied into ScummVM as `backends/platform/gasm/`) and a
small `configure` patch that adds the `wasm32-gasm` host. `make scummvm` fetches
ScummVM 2026.3.0 into `tools/scummvm-src` (git-ignored), builds it with wasi-sdk
and post-processes it with Binaryen's Asyncify.

```sh
make scummvm && make roms                  # scummvm.wasm; Beneath a Steel Sky (freeware) into roms/bass/,
                                           # SCUMM demos (Monkey Island 1, Day of the Tentacle) into roms/scumm/
R=runners/native/target/release/gasm-run
$R build/scummvm.wasm --asset-dir roms/bass --param "args=-p / sky"
$R build/scummvm.wasm --asset-dir ~/Games/MonkeyIsland --param "args=--auto-detect -p /"
$R build/scummvm.wasm --asset-dir roms/scumm/dott-dos-ni-demo-en --param "args=--auto-detect -p /"
$R build/scummvm.wasm                       # the launcher (no games)
```

In the browser player, pick **ScummVM**: without a folder it starts Beneath a
Steel Sky; **open folder…** plays your own game (auto-detected). Worker mode
reads big games from OPFS on demand.

## Engines

The build includes **scumm** (with its **scumm_7_8** and **he** sub-engines) and
**sky** by default:

| Engine | Games |
|---|---|
| `scumm` | LucasArts v0–v6: Maniac Mansion, Zak McKracken, Indiana Jones and the Last Crusade, Loom, Monkey Island 1 and 2, Indiana Jones and the Fate of Atlantis, Day of the Tentacle, Sam & Max Hit the Road |
| `scumm_7_8` | Full Throttle, The Dig, The Curse of Monkey Island |
| `he` | Humongous Entertainment (Putt-Putt, Freddi Fish, Pajama Sam, Spy Fox, ...) |
| `sky` | Beneath a Steel Sky |

Tested with the LucasArts demos ScummVM hosts (Monkey Island 1 EGA, Day of the
Tentacle, Sam & Max CD) and Beneath a Steel Sky. More engines:
`make scummvm SCUMMVM_ENGINES="sky scumm scumm_7_8 he queen lure" SCUMMVM_DATA="sky.cpt queen.tbl lure.dat"`
(`SCUMMVM_DATA` lists the files from ScummVM's `dists/engine-data` that get built
into the module). External libraries (zlib, FLAC, MP3, Vorbis, FreeType, ...) are
off for now, so engines or game versions that need them don't work yet.

## Parameters

| Param | Meaning |
|---|---|
| `args` | A ScummVM command line, e.g. `-p / sky`, `--auto-detect -p /`, `-x 1 -p / sky` (load slot 1). Game files are at `/`. |

## How it maps onto gasm

- **Frames and time.** ScummVM's engines run their own loops. Time is virtual:
  `delayMillis` advances it, and when it crosses a 60 Hz frame boundary the
  engine **yields** to the runner and resumes on the next `gasm_frame`. Runs
  depend only on their input, so headless hashes match on every runner.
- **Asyncify.** The yield is the C SDK's loop helper
  ([`sdk/c/src/gasm_loop.c`](https://github.com/emdzej/gasm/blob/main/sdk/c/src/gasm_loop.c)):
  `gasm_main()` runs ScummVM, `gasm_wait_frame()` suspends it with Binaryen's
  Asyncify, entirely inside the guest, so runners need nothing. The rules it
  follows are at the top of that file. It costs size: 9.7 MB without
  Asyncify, 15.0 MB with it.
- **Video.** A software graphics manager combines the game screen (8, 16 or 32
  bit), the GUI overlay (640x480) and the cursor into one RGBA frame.
  320x200 and 640x400 screens are stretched to 4:3. The GUI shows the game
  behind it.
- **Audio.** ScummVM's mixer (with its AdLib emulation for music) is pulled for
  exactly one frame of samples per frame, 44.1 kHz stereo.
- **Input.** Raw keys with modifiers (`KEYS_RAW`), the pointer in frame pixels
  (the system cursor is hidden; ScummVM draws its own), the wheel, and the first
  gamepad as ScummVM joystick events.
- **Files.** The assets are a read-only directory tree (`asset_count`,
  `asset_name`), streamed with `asset_read_at`. Engine data files are built in.
- **Saves and settings.** `gasm:storage`: save games are `save.<name>`, the
  config is `scummvm.ini`; save lists come from the storage keys.
- **Reproducible builds.** `SOURCE_DATE_EPOCH` pins the build date ScummVM puts
  in its version string, so `make scummvm` gives the same bytes from the same
  source.

## License

`scummvm.wasm` is licensed as a whole under the **GNU GPL version 3** (ScummVM's
license). The backend in this directory is gasm's own and MIT-licensed; the
`configure` patch is GPL-3.0 like the file it changes. Releases include the
complete source as `gasm-<version>-scummvm-src.tar.gz`: exactly the ScummVM
files the build used, the backend, the patch and the build scripts
(`scripts/package-scummvm-src.sh`); rebuilding from it gives the same module.
The website serves it next to the game.

Beneath a Steel Sky is freeware from Revolution Software; its readme (with the
license) stays next to the game files. Use your own copies of other games.
