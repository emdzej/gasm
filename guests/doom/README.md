# DOOM for gasm

DOOM compiled to a single gasm guest, `build/doom.wasm`. The engine is
[doomgeneric](https://github.com/ozkl/doomgeneric) (a portable Chocolate Doom
derivative). Music is Chocolate Doom's OPL player on the DOSBox OPL emulator,
so the soundtrack is the FM synthesis of an AdLib / Sound Blaster card, as in
1993. This directory holds only gasm's platform layer. `make doom` fetches the
engine into `tools/doom-src` (git-ignored) and applies `engine.patch`.

```sh
make doom && make roms                  # doom.wasm; Freedoom + shareware DOOM1.WAD into roms/
R=runners/native/target/release/gasm-run
$R build/doom.wasm --asset wad=roms/doom1.wad
$R build/doom.wasm --asset wad=roms/freedoom2.wad --param "args=-warp 7 -skill 4"
$R build/doom.wasm --asset-dir ~/Games/doom           # finds doom2.wad, DOOM.WAD, ... by name
```

In the browser player, pick **DOOM**, or drop any `.wad` file on the screen.

## Game data

Any IWAD works: shareware `doom1.wad`, `doom.wad` (registered or Ultimate),
`doom2.wad`, `plutonia.wad`, `tnt.wad`, and the free
[Freedoom](https://freedoom.github.io/) `freedoom1.wad` / `freedoom2.wad`.
Give it as the asset `wad` under any file name (it's identified by its
lumps), or as a folder that contains one of the standard names.

`make roms` fetches Freedoom (BSD-3-Clause) and the v1.9 shareware
`doom1.wad`, which id Software made freely distributable. The commercial WADs
are not free; use your own copies.

## Parameters

| Param | Meaning |
|---|---|
| `args` | A DOOM command line, e.g. `-warp 1 3 -skill 4`, `-playdemo demo2`, `-fast`, `-nomonsters`, `-nomusic` |

## Controls

**Keyboard and mouse** (DOOM reads them directly, with its original keys):

| Key | Action |
|---|---|
| Arrows | move and turn |
| Ctrl | fire |
| Space | use (doors, switches) |
| Shift | run |
| Alt + arrows, `,` / `.` | strafe |
| 1-7 | weapons |
| Tab | automap |
| Esc (tap) | menu; Enter selects, `y`/`n` answer prompts |
| F1-F12, `-` / `=` | help, save, load, …, screen size |
| Mouse (while playing) | turn; left fires, right strafes, middle moves forward |

The mouse is captured while you play and freed in the menus (in the browser,
click the game to capture it). Moving the mouse up and down doesn't walk.
Hold Esc for a second to quit the runner. Save games get the names you type.

**Gamepads** use a context-dependent mapping on virtual pad 1:

| gasm button | Keyboard (default) | In the game | In menus | Yes/no prompts |
|---|---|---|---|---|
| D-pad | arrows | move and turn | navigate | |
| A | X | fire | select | yes |
| B | Z | use (doors, switches) | back (closes the main menu) | no |
| X | S | next weapon | | |
| Y | A | run (hold) | | |
| L / R | Q / W | strafe left / right | | |
| Start | Enter | menu | select | cancel |
| Select | Right Shift | automap | close menu | |

A gamepad can't type, so saving to an empty slot from the pad names it
`SLOT n`.

## How it maps onto gasm

- **Frames and time:** DOOM's native 35 Hz. The engine clock counts frames,
  and when the engine "sleeps" to wait for a tic, time simply moves ahead.
  So a run never depends on the wall clock, and hashes match on every runner.
- **Video:** the 320x200 palettized screen is stretched to 640x480, the 4:3
  shape it had on a CRT.
- **Audio:** 44.1 kHz stereo, exactly 1260 samples per frame. Sound effects
  (8-bit DMX lumps) are mixed with linear interpolation. Music is rendered by
  `gasm_opl.c`, which fires the player's timer callbacks at exact sample
  positions.
- **Files:** there is no filesystem. `prelude.h` redirects the engine's
  `fopen`/`remove`/`rename`. Reads come from assets (the WAD, read on demand)
  or from `gasm:storage`; writes go to storage when the file is closed. The
  keys are `default.cfg`, `doomgenericdoom.cfg` and `doomsav0.dsg` ... `doomsav7.dsg`.
- **Input:** the glue asks for `KEYS_RAW` (no keymap pads) and hides the
  cursor; it locks the pointer while a level is being played. Raw keys become
  DOOM keys, mouse motion and buttons become DOOM mouse events, and pad 1
  (gamepads) keeps the mapping above.
- **Settings:** volume, screen size, gamma and messages persist. Key bindings
  don't (controls come from the mappings above).

## Engine changes (`engine.patch`)

- The screen melt advances one step per frame. On the desktop it's a loop
  that blocks until the melt is done, which in a frame-driven guest would
  hide it.
- `G_CheckDemoStatus` is registered as an exit hook through a function
  pointer of the wrong type. WebAssembly checks call signatures and traps, so
  a correctly typed wrapper is used.
- `I_Quit` really exits (doomgeneric returned to the game loop).
- Config files are loaded and saved again (doomgeneric had disabled that),
  except key bindings. The config directory is empty, so files are flat
  storage keys.

## License

`doom.wasm` is licensed as a whole under the **GNU GPL version 2**, the
license of the DOOM source code. The files in this directory are gasm's own
and MIT-licensed, except `engine.patch`, which modifies GPL code and is
GPL-2.0 too. Releases include the complete source of `doom.wasm` as
`gasm-<version>-doom-src.tar.gz` (built by `scripts/package-doom-src.sh`),
and the website serves it next to the game.
