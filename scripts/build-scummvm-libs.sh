#!/usr/bin/env bash
# Build ScummVM's audio and compression libraries for gasm (wasm32, wasi-sdk) into
# tools/scummvm-libs/{include,lib}: zlib, libmad (MP3), libogg + libvorbis (Ogg
# Vorbis) and libFLAC. Sources are fetched into tools/scummvm-libs/src (git-ignored)
# and checked against pinned SHA-256s; scripts/package-scummvm-src.sh ships them.
#
# Each library is compiled from its plain C sources with a fixed configuration
# (no autotools: their configure scripts don't know wasm32). No SIMD or assembly;
# libmad uses its portable 64-bit fixed point. Every computation happens inside the
# module, so decoding is identical on every runner.
#
#   scripts/build-scummvm-libs.sh
set -euo pipefail
cd "$(dirname "$0")/.."
OUT=tools/scummvm-libs
SRC=$OUT/src
WASI_SDK=${WASI_SDK:-$PWD/tools/wasi-sdk}
CC="$WASI_SDK/bin/clang --target=wasm32-wasip1 --sysroot=$WASI_SDK/share/wasi-sysroot"
AR="$WASI_SDK/bin/llvm-ar"
CFLAGS="-O2 -DNDEBUG -w"
STAMP="$OUT/.gasm-libs"
SIG=$(cksum < "$0" | cut -d' ' -f1)
[ -f "$STAMP" ] && [ "$(cat "$STAMP")" = "$SIG" ] && exit 0

# name|url|sha256 (libmad: SourceForge, Debian's identical orig tarball as a fallback)
LIBS="zlib-1.3.2|https://github.com/madler/zlib/releases/download/v1.3.2/zlib-1.3.2.tar.gz|bb329a0a2cd0274d05519d61c667c062e06990d72e125ee2dfa8de64f0119d16
libmad-0.15.1b|https://downloads.sourceforge.net/project/mad/libmad/0.15.1b/libmad-0.15.1b.tar.gz http://deb.debian.org/debian/pool/main/libm/libmad/libmad_0.15.1b.orig.tar.gz|bbfac3ed6bfbc2823d3775ebb931087371e142bb0e9bb1bee51a76a6e0078690
libogg-1.3.6|https://downloads.xiph.org/releases/ogg/libogg-1.3.6.tar.gz|83e6704730683d004d20e21b8f7f55dcb3383cdf84c0daedf30bde175f774638
libvorbis-1.3.7|https://downloads.xiph.org/releases/vorbis/libvorbis-1.3.7.tar.gz|0e982409a9c3fc82ee06e08205b1355e5c6aa4c36bca58146ef399621b0ce5ab
flac-1.5.0|https://downloads.xiph.org/releases/flac/flac-1.5.0.tar.xz|f2c1c76592a82ffff8413ba3c4a1299b6c7ab06c734dee03fd88630485c2b920"

mkdir -p "$SRC"
TMP=$(mktemp -d)
trap 'rm -rf "$TMP"' EXIT
echo "$LIBS" | while IFS='|' read -r name urls sha; do
  [ -d "$SRC/$name" ] && continue
  echo "fetching $name"
  ok=
  for url in $urls; do
    if curl -fsSL "$url" -o "$TMP/$name.tar" && echo "$sha  $TMP/$name.tar" | shasum -a 256 -c - >/dev/null 2>&1; then ok=1; break; fi
  done
  [ -n "$ok" ] || { echo "$name: download failed or checksum mismatch" >&2; exit 1; }
  tar xf "$TMP/$name.tar" -C "$SRC"
done

rm -rf "$OUT/include" "$OUT/lib" "$OUT/obj"
mkdir -p "$OUT/include" "$OUT/lib" "$OUT/obj"

# lib <name> <cflags> <sources...>
lib() {
  local name=$1 flags=$2; shift 2
  local objs=() src obj
  mkdir -p "$OUT/obj/$name"
  for src in "$@"; do
    obj="$OUT/obj/$name/$(basename "${src%.c}").o"
    $CC $CFLAGS $flags -c "$src" -o "$obj"
    objs+=("$obj")
  done
  "$AR" rcs "$OUT/lib/lib$name.a" "${objs[@]}"
  echo "built lib$name.a (${#objs[@]} files)"
}

# zlib
Z=$SRC/zlib-1.3.2
lib z "-DZ_HAVE_UNISTD_H -I$Z" $Z/adler32.c $Z/compress.c $Z/crc32.c $Z/deflate.c $Z/gzclose.c $Z/gzlib.c \
  $Z/gzread.c $Z/gzwrite.c $Z/infback.c $Z/inffast.c $Z/inflate.c $Z/inftrees.c $Z/trees.c $Z/uncompr.c $Z/zutil.c
cp $Z/zlib.h $Z/zconf.h "$OUT/include/"

# libmad: portable 64-bit fixed point (the shipped mad.h is configured for x86 asm)
M=$SRC/libmad-0.15.1b
cat > "$OUT/obj/mad-config.h" <<'H'
#define SIZEOF_INT 4
#define SIZEOF_LONG 4
#define SIZEOF_LONG_LONG 8
#define HAVE_ASSERT_H 1
#define HAVE_LIMITS_H 1
#define HAVE_STDINT_H 1
#define HAVE_INTTYPES_H 1
#define HAVE_SYS_TYPES_H 1
#define HAVE_UNISTD_H 1
#define HAVE_FCNTL_H 1
#define HAVE_ERRNO_H 1
#define OPT_ACCURACY 1
H
mkdir -p "$OUT/obj/mad-inc" && cp "$OUT/obj/mad-config.h" "$OUT/obj/mad-inc/config.h"
lib mad "-DHAVE_CONFIG_H -DFPM_64BIT -I$OUT/obj/mad-inc -I$M" $M/bit.c $M/decoder.c $M/fixed.c $M/frame.c \
  $M/huffman.c $M/layer12.c $M/layer3.c $M/stream.c $M/synth.c $M/timer.c $M/version.c
sed -e 's/^# define FPM_INTEL$/# define FPM_64BIT/' $M/mad.h > "$OUT/include/mad.h"
grep -q '^# define FPM_64BIT$' "$OUT/include/mad.h"

# libogg
O=$SRC/libogg-1.3.6
mkdir -p "$OUT/include/ogg"
sed -e 's/@INCLUDE_INTTYPES_H@/1/; s/@INCLUDE_STDINT_H@/1/; s/@INCLUDE_SYS_TYPES_H@/1/' \
    -e 's/@SIZE16@/int16_t/; s/@USIZE16@/uint16_t/; s/@SIZE32@/int32_t/; s/@USIZE32@/uint32_t/' \
    -e 's/@SIZE64@/int64_t/; s/@USIZE64@/uint64_t/' $O/include/ogg/config_types.h.in > "$OUT/include/ogg/config_types.h"
cp $O/include/ogg/ogg.h $O/include/ogg/os_types.h "$OUT/include/ogg/"
lib ogg "-I$OUT/include" $O/src/bitwise.c $O/src/framing.c

# libvorbis + libvorbisfile
V=$SRC/libvorbis-1.3.7
VSRC="analysis bitrate block codebook envelope floor0 floor1 info lookup lpc lsp mapping0 mdct psy registry res0 sharedbook smallft synthesis window"
lib vorbis "-I$OUT/include -I$V/include -I$V/lib" $(for f in $VSRC; do echo $V/lib/$f.c; done)
lib vorbisfile "-I$OUT/include -I$V/include -I$V/lib" $V/lib/vorbisfile.c
mkdir -p "$OUT/include/vorbis" && cp $V/include/vorbis/codec.h $V/include/vorbis/vorbisfile.h "$OUT/include/vorbis/"

# libFLAC: the decoder (native FLAC; no Ogg FLAC, no SIMD)
F=$SRC/flac-1.5.0
cat > "$OUT/obj/flac-config.h" <<'H'
#define CPU_IS_BIG_ENDIAN 0
#define ENABLE_64_BIT_WORDS 0
#define OGG_FOUND 0
#define FLAC__HAS_OGG 0
#define FLAC__HAS_X86INTRIN 0
#define FLAC__HAS_NEONINTRIN 0
#define FLAC__HAS_A64NEONINTRIN 0
#define FLAC__NO_ASM 1
#define HAVE_LROUND 1
#define HAVE_INTTYPES_H 1
#define HAVE_STDINT_H 1
#define HAVE_STDLIB_H 1
#define HAVE_STRING_H 1
#define HAVE_SYS_STAT_H 1
#define HAVE_SYS_TYPES_H 1
#define HAVE_UNISTD_H 1
#define HAVE_FSEEKO 1
#define PACKAGE_VERSION "1.5.0"
H
mkdir -p "$OUT/obj/flac-inc" && cp "$OUT/obj/flac-config.h" "$OUT/obj/flac-inc/config.h"
FSRC="bitmath bitreader cpu crc fixed float format lpc md5 memory metadata_object stream_decoder window"   # decoding only
lib FLAC "-DHAVE_CONFIG_H -DFLAC__NO_DLL -I$OUT/obj/flac-inc -I$F/include -I$F/src/libFLAC/include" \
  $(for f in $FSRC; do echo $F/src/libFLAC/$f.c; done)
mkdir -p "$OUT/include/FLAC" && cp $F/include/FLAC/*.h "$OUT/include/FLAC/"

echo "$SIG" > "$STAMP"
echo "libraries in $OUT"
