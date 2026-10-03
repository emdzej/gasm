# ScummVM for gasm

[ScummVM](https://www.scummvm.org/) compiled to a single gasm guest,
`build/scummvm.wasm`, with a gasm backend. This directory holds gasm's own files:
the backend (`backend/`, copied into ScummVM as `backends/platform/gasm/`) and a
small `configure` patch that adds the `wasm32-gasm` host. `make scummvm` fetches
ScummVM 2026.3.0 into `tools/scummvm-src` (git-ignored), builds its libraries
(zlib, MP3, Ogg Vorbis, FLAC) and ScummVM with wasi-sdk, and post-processes the
module twice: `scummvm.wasm` with Binaryen's Asyncify (every runner) and
`scummvm-run.wasm` without (runners that switch stacks: `gasm-run`, Chromium,
Node 24+; 10.6 MB instead of 16.1 MB).

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

The build includes **scumm** (with its **scumm_7_8** and **he** sub-engines),
**sky** and **drascula** by default:

| Engine | Games | State |
|---|---|---|
| `scumm` | LucasArts v0–v6: Maniac Mansion, Zak McKracken, Indiana Jones and the Last Crusade, Loom, Monkey Island 1 and 2, Indiana Jones and the Fate of Atlantis, Day of the Tentacle, Sam & Max Hit the Road | played: the Monkey Island 1 (EGA), Day of the Tentacle and Sam & Max demos |
| `scumm_7_8` | Full Throttle, The Dig, The Curse of Monkey Island | built, not tested yet (no freely available data) |
| `he` | Humongous Entertainment (Putt-Putt, Freddi Fish, Pajama Sam, Spy Fox, ...) | built, not tested yet |
| `sky` | Beneath a Steel Sky | played: floppy version, saving and loading (freeware) |
| `drascula` | Drascula: The Vampire Strikes Back | played: the opening, with its CD music as Ogg Vorbis, MP3 and FLAC (freeware) |

The tests in `scripts/determinism-test.sh` cover Beneath a Steel Sky (intro,
walking, save and restore), the Day of the Tentacle demo and Drascula with
each audio format. More engines:
`make scummvm SCUMMVM_ENGINES="sky scumm scumm_7_8 he drascula queen lure" SCUMMVM_DATA="sky.cpt drascula.dat queen.tbl lure.dat"`
(`SCUMMVM_DATA` lists the files from ScummVM's `dists/engine-data` that get built
into the module).

## Libraries

`scripts/build-scummvm-libs.sh` builds the libraries ScummVM links, from their
release tarballs (checksums pinned), into `tools/scummvm-libs`:

| Library | For | License |
|---|---|---|
| zlib 1.3.2 | compressed data, ZIP archives, save games | zlib |
| libmad 0.15.1b | MP3 (portable 64-bit fixed point) | GPL-2.0-or-later |
| libogg 1.3.6, libvorbis 1.3.7 | Ogg Vorbis | BSD-3-Clause |
| libFLAC 1.5.0 (decoder) | FLAC | BSD-3-Clause |

That covers compressed speech and music (`monster.so3` / `.sog` / `.sof`,
`track1.mp3` / `.ogg` / `.flac` and the like, as made by `scummvm-tools` or rips of
CD audio). No SIMD and no assembly; all decoding runs inside the module, so it
gives the same samples on every runner.

## Porting status

Done:

- **Backend**: video (8, 16 and 32 bit game screens, the GUI overlay, the cursor,
  shaking, 4:3 aspect correction), audio (the mixer, one frame of samples per
  frame), input (raw keyboard with modifiers, mouse and wheel, the first gamepad
  as a joystick), files (the asset tree, streamed), saves and settings in
  `gasm:storage`, virtual time, engine data built into the module, the launcher.
- **Engines run their own loops**, suspended every frame by the C SDK's loop
  helper (Asyncify), on every runner: native, browser (main thread and Worker,
  big games from OPFS) and headless, with identical hashes.
- **Libraries**: zlib, MP3, Ogg Vorbis, FLAC.
- **Music**: AdLib (OPL emulation) and PC speaker, built into ScummVM.
- **Licensing**: reproducible builds and a complete source tarball (ScummVM's
  files, the libraries, the backend) with every release and on the website;
  rebuilding from it gives the same module byte for byte.

Not done yet (more engines, MT-32 and General MIDI music, GUI themes, video
codecs, 3D engines, more gamepads, size): see the
[roadmap](https://github.com/emdzej/gasm/blob/main/site/docs/roadmap.md#scummvm).

## Parameters

| Param | Meaning |
|---|---|
| `args` | A ScummVM command line, e.g. `-p / sky`, `--auto-detect -p /`, `-x 1 -p / sky` (load slot 1). Game files are at `/`. |

## How it maps onto gasm

- **Frames and time.** ScummVM's engines run their own loops. Time is virtual:
  `delayMillis` advances it, and when it crosses a 60 Hz frame boundary the
  engine **yields** to the runner and resumes on the next `gasm_frame`. Runs
  depend only on their input, so headless hashes match on every runner.
- **Suspending.** The yield is the C SDK's loop helper
  ([`sdk/c/src/gasm_loop.c`](https://github.com/emdzej/gasm/blob/main/sdk/c/src/gasm_loop.c)):
  `gasm_main()` runs ScummVM and `gasm_wait_frame()` suspends it, either with
  Binaryen's Asyncify inside the guest (`scummvm.wasm`) or by the runner
  through `gasm_run` and `yield_frame` (stack switching, `scummvm-run.wasm`).
  Both give the same hashes. The Asyncify rules are at the top of that file.
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
  `asset_name`), streamed with 64-bit `asset_read_at64` through a 16 KB
  read-ahead buffer. Engine data files are built in.
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
files the build used, the libraries' release sources, the backend, the patch
and the build scripts (`scripts/package-scummvm-src.sh`); rebuilding from it
gives the same module, which the release workflow checks before publishing.
`THIRD-PARTY.txt` next to the games has the libraries' license notices.
The website serves it next to the game.

Beneath a Steel Sky is freeware from Revolution Software; its readme (with the
license) stays next to the game files. Use your own copies of other games.
