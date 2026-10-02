#!/usr/bin/env bash
# Fetch freely distributable test data into roms/ (git-ignored): NES test ROMs and
# homebrew demos, the Freedoom IWADs, the DOOM shareware WAD, Beneath a Steel Sky,
# and (test-only, never redistributed) ScummVM's SCUMM demos and Drascula.
# Every download is checked against a pinned SHA-256.
#
#   scripts/fetch-roms.sh               everything
#   scripts/fetch-roms.sh doom1 bass    only some groups: nes freedoom doom1 bass scumm drascula
set -euo pipefail
cd "$(dirname "$0")/.."
. scripts/lib.sh
mkdir -p roms
GROUPS_WANTED=" ${*:-nes freedoom doom1 bass scumm drascula} "
want() { case "$GROUPS_WANTED" in *" $1 "*) return 0 ;; *) return 1 ;; esac; }

# NES: https://github.com/christopherpow/nes-test-roms (test ROMs and demos circulated
# on the NESdev community for emulator testing), pinned to a commit.
NES_BASE=https://raw.githubusercontent.com/christopherpow/nes-test-roms/95d8f621ae55cee0d09b91519a8989ae0e64753b
nes() { # <remote path> <local name> <sha256>
  [ -f "roms/$2" ] && return
  echo "fetching $2"
  download "$NES_BASE/$1" "roms/$2.part" "$3"
  mv "roms/$2.part" "roms/$2"
}
if want nes; then
  nes instr_test-v5/official_only.nes           cpu_instr_test.nes  589b8835deb5cbc69618dac193a3dbd675540f7f2794e2d2a92e97beb8abc3cb # blargg: CPU instructions
  nes cpu_timing_test6/cpu_timing_test.nes      cpu_timing_test.nes 6ab4fe8af23b12ca0dfccfc030de3d4069bf2498e3ef20ddcf1ca75555065b85 # blargg: CPU timing
  nes apu_test/apu_test.nes                     apu_test.nes        00d4722bae1c82a14528dd3220462d3fb9ce4b14b8cec996619dea23e07fef0a # blargg: APU
  nes other/nestest.nes                         nestest.nes         f67d55fd6b3cf0bad1cc85f1df0d739c65b53e79cecb7fea8f77ec0eadab0004 # kevtris: CPU test menu
  nes spritecans-2011/spritecans.nes            spritecans.nes      87ccbff575df8679b7474688090ba758c50334bcf3a794ec64867fddded2c61d # sprite demo
  nes other/BladeBuster.nes                     bladebuster.nes     0195973f2a9783a3642325b30fdbb3bf9da4e70554fb56c2ceb6cb38ccc9930a # homebrew shmup
  nes other/quantum_disco_brothers_by_wAMMA.nes quantum_disco.nes   53582b185aaef646354b1ba15a4ecca70323e0d0ba9a53a4861f256bbed71e1a # demoscene prod
fi

# Freedoom (BSD-3-Clause): free replacement IWADs for the DOOM guest.
FREEDOOM=0.13.0
if want freedoom && { [ ! -f roms/freedoom1.wad ] || [ ! -f roms/freedoom2.wad ]; }; then
  echo "fetching freedoom $FREEDOOM"
  tmp=$(mktemp -d)
  download "https://github.com/freedoom/freedoom/releases/download/v$FREEDOOM/freedoom-$FREEDOOM.zip" "$tmp/freedoom.zip" \
    3f9b264f3e3ce503b4fb7f6bdcb1f419d93c7b546f4df3e874dd878db9688f59
  unzip -q -j -o "$tmp/freedoom.zip" "freedoom-$FREEDOOM/freedoom1.wad" "freedoom-$FREEDOOM/freedoom2.wad" -d roms
  rm -rf "$tmp"
fi

# DOOM shareware v1.9 (episode 1). id Software: "The DOOM shareware wad is freely
# distributable" (John Carmack, 1999). Taken unmodified from Debian's archive.
if want doom1 && [ ! -f roms/doom1.wad ]; then
  echo "fetching doom1.wad (shareware)"
  tmp=$(mktemp -d)
  download http://deb.debian.org/debian/pool/non-free/d/doom-wad-shareware/doom-wad-shareware_1.9.fixed-5_all.deb "$tmp/doom1.deb" \
    5802f176c0303e228095b5312def53de602781cf4c53e79842257484a0d9e938
  (cd "$tmp" && ar x doom1.deb && tar xf data.tar.xz ./usr/share/games/doom/doom1.wad)
  sha256_ok "$tmp/usr/share/games/doom/doom1.wad" 1d7d43be501e67d927e415e0b8f3e29c3bf33075e859721816f652a526cac771
  mv "$tmp/usr/share/games/doom/doom1.wad" roms/doom1.wad
  rm -rf "$tmp"
fi

# Beneath a Steel Sky, floppy version (freeware, Revolution Software; free to
# redistribute with its readme, which stays next to the game in roms/bass/).
if want bass && { [ ! -f roms/bass/sky.dnr ] || [ ! -f roms/bass/sky.dsk ]; }; then
  echo "fetching Beneath a Steel Sky (floppy, freeware)"
  tmp=$(mktemp -d)
  curl -fsSL "https://downloads.scummvm.org/frs/extras/Beneath%20a%20Steel%20Sky/BASS-Floppy-1.3.zip" -o "$tmp/bass.zip"
  mkdir -p roms/bass && unzip -q -o "$tmp/bass.zip" -d roms/bass
  chmod 644 roms/bass/*   # the zip stores them owner-only
  rm -rf "$tmp"
  sha256_ok roms/bass/sky.dnr e1ea726858bfa024b9696856c32cd2f525d0e33b5fb0e0284ad2e6ce943115c8
  sha256_ok roms/bass/sky.dsk 355a6782b9741d0e9eb5e202343c944577e7d78ad36b4c8c7b83965526b1d2aa
fi

# LucasArts SCUMM demos hosted by ScummVM, for testing the SCUMM engine (not
# redistributed by gasm: they stay in roms/, which is never committed or published).
scumm_demo() { # <name> <sha256 of the zip>
  [ -d "roms/scumm/$1" ] && return
  echo "fetching SCUMM demo $1"
  tmp=$(mktemp -d)
  download "https://downloads.scummvm.org/frs/demos/scumm/$1.zip" "$tmp/d.zip" "$2"
  mkdir -p "roms/scumm/$1.part" && unzip -q -o "$tmp/d.zip" -d "roms/scumm/$1.part" && chmod -R u+rwX,go+rX "roms/scumm/$1.part"
  mv "roms/scumm/$1.part" "roms/scumm/$1"
  rm -rf "$tmp"
}
if want scumm; then
  scumm_demo monkey1-dos-ega-demo-en 1cb530fc4ab1d1f005e6630de31fe1a4186a7ad5d1a2b73e126e898e5e9039d0
  scumm_demo dott-dos-ni-demo-en b77f03981da3815a352330f39f54d46e187bb05503cf1c832bb0d14539f15964
fi

# Drascula: The Vampire Strikes Back (freeware), for testing compressed CD audio:
# the game, and its music as Ogg Vorbis, MP3 and FLAC (only the tracks the opening
# plays plus track 1, which ScummVM checks for; pulled out of the zips with range
# requests). Test-only, like the SCUMM demos.
DRASCULA="https://downloads.scummvm.org/frs/extras/Drascula_%20The%20Vampire%20Strikes%20Back"
drascula_audio() { # <zip> <ext> <sha256 track1> <sha256 track25> <sha256 track28>
  [ -f "roms/drascula/$2/audio/track28.$2" ] && return
  echo "fetching Drascula music ($2)"
  node scripts/zip-get.mjs "$DRASCULA/$1" "roms/drascula/$2" "audio/track1.$2" "audio/track25.$2" "audio/track28.$2" >/dev/null
  sha256_ok "roms/drascula/$2/audio/track1.$2" "$3"
  sha256_ok "roms/drascula/$2/audio/track25.$2" "$4"
  sha256_ok "roms/drascula/$2/audio/track28.$2" "$5"
}
if want drascula; then
  if [ ! -f roms/drascula/game/Packet.001 ]; then
    echo "fetching Drascula"
    tmp=$(mktemp -d)
    download "$DRASCULA/drascula-1.0.zip" "$tmp/d.zip" b731f6cb5a22ba8b4c3b3362f570b9a10a67b6cb0b395394b19a94b36e4e42de
    mkdir -p roms/drascula/game && unzip -q -o "$tmp/d.zip" -d roms/drascula/game && chmod -R u+rwX,go+rX roms/drascula/game
    rm -rf "$tmp"
  fi
  drascula_audio drascula-audio-2.0.zip ogg c15b9423e07b4110aa8af3f950b2000f5bbbaf3662b97562a14342c2372b4445 \
    c7d1fb605c25e2950fc1b30946d956fe6d73fcbd0ed7900495d7b3e312678d61 3476061a6f9e5d2d3afcc9b5b619481681ad4663ae95f94f6b6fb7dbf73724e6
  drascula_audio drascula-audio-mp3-2.0.zip mp3 6672303a4563c5c4edce6b0d1d5b776e6bc58693051ede5ff66624c0b9e25777 \
    aac0ba1ee355e4dbd4785bd9b18e937212c43fabfdbe14001ddf089bab398c10 5b873ab3985e45f8f77abfa927009bfb9a04ffcd9e924f80459ea76d2a7b3493
  drascula_audio drascula-audio-flac-2.0.zip flac a2013415a2a9498c419ce5dd4a1d6a656579c2aa3097b58747eb94866472e780 \
    40e3c7e981f523c3f53ead0e22b298326ba8d7e7ea82c2b654e4df56fae6b96e 4332599f4c6d098248a7f2c7eb4b8db4b661e1a8b0801d93ffa3e63f01ff4e31
fi
ls roms
