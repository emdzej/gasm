#!/usr/bin/env bash
# Download ANGLE (OpenGL ES on Metal / Direct3D 11 / Vulkan) into tools/angle/<platform>:
# libEGL + libGLESv2, and SwiftShader (software Vulkan, for machines without a GPU) with
# the Vulkan loader on Linux and Windows.
# gasm-run loads them for gasm:gl games (design/gasm-gl.md).
#
#   scripts/fetch-angle.sh [platform...]   default: this machine's
#   platforms: macos-arm64 macos-x86_64 linux-x86_64 linux-arm64 windows-x86_64
#
# The libraries come from an Electron release (Electron ships Chromium's ANGLE build
# as separate libraries up to version 43): official, checksummed in SHASUMS256.txt,
# pinned here. tools/angle/<platform>/.version records what is installed.
# Licenses: ANGLE BSD-3-Clause, SwiftShader Apache-2.0, Electron MIT (THIRD-PARTY.txt).
set -euo pipefail
cd "$(dirname "$0")/.."
. scripts/lib.sh
ELECTRON=43.7.7

host_platform() {
  case "$(uname -s)-$(uname -m)" in
    Darwin-arm64) echo macos-arm64 ;;
    Darwin-x86_64) echo macos-x86_64 ;;
    Linux-x86_64) echo linux-x86_64 ;;
    Linux-aarch64) echo linux-arm64 ;;
    MINGW*-x86_64 | MSYS*-x86_64 | CYGWIN*-x86_64) echo windows-x86_64 ;;
    *) echo "unsupported host $(uname -s)-$(uname -m)" >&2; exit 1 ;;
  esac
}

fetch() {
  local plat=$1 zip sha dir files
  case "$plat" in
    macos-arm64)    zip=darwin-arm64 sha=9327d8ba5bc9e279d1a2f7da90235301c65a2e80eb4ad3bc5610d28d483340f9 ;;
    macos-x86_64)   zip=darwin-x64   sha=cbed66567d55db4a2bffad0bb6ee9795ca0037ad241039f7473fe75680d10905 ;;
    linux-arm64)    zip=linux-arm64  sha=16071038a9677d0f00b11d3d0f0b7b4ea3250987727aa73aa563b87789f2d5ab ;;
    linux-x86_64)   zip=linux-x64    sha=4d0a48398c444258dbcf2f5f83b49ca5bc53583130f354e0c299dad5b22b5271 ;;
    windows-x86_64) zip=win32-x64    sha=97dcb75065444ef031b9b6ea814ccd2109b97934fffb0c503a555d4737ca79cc ;;
    *) echo "unknown platform $plat" >&2; exit 1 ;;
  esac
  case "$plat" in
    macos-*)   dir='Electron.app/Contents/Frameworks/Electron Framework.framework/Versions/A/Libraries/'
               files="libEGL.dylib libGLESv2.dylib libvk_swiftshader.dylib vk_swiftshader_icd.json" ;;
    linux-*)   dir='' files="libEGL.so libGLESv2.so libvulkan.so.1 libvk_swiftshader.so vk_swiftshader_icd.json" ;;
    windows-*) dir='' files="libEGL.dll libGLESv2.dll vulkan-1.dll vk_swiftshader.dll vk_swiftshader_icd.json" ;;
  esac
  local out=tools/angle/$plat stamp="electron-$ELECTRON-$plat $files"
  [ -f "$out/.version" ] && [ "$(cat "$out/.version")" = "$stamp" ] && return 0
  local tmp
  tmp=$(mktemp -d)
  local url=https://github.com/electron/electron/releases/download/v$ELECTRON/electron-v$ELECTRON-$zip.zip
  echo "fetching $url"
  download "$url" "$tmp/electron.zip" "$sha"
  local members=() f
  for f in $files; do members+=("$dir$f"); done
  unzip -q -j -o "$tmp/electron.zip" "${members[@]}" LICENSE LICENSES.chromium.html -d "$tmp/x"
  notices "$tmp/x" > "$tmp/x/ANGLE-NOTICES.txt"
  rm "$tmp/x/LICENSE" "$tmp/x/LICENSES.chromium.html"
  rm -rf "$out" && mkdir -p tools/angle && mv "$tmp/x" "$out"
  rm -rf "$tmp"
  [ "$(uname -s)" = Darwin ] && xattr -dr com.apple.quarantine "$out" 2>/dev/null || true
  echo "$stamp" > "$out/.version"
  echo "$out: $(ls "$out" | tr '\n' ' ')"
}

# The license notices of what is in these libraries (shipped next to them): ANGLE's
# license, then the sections of Chromium's license file for ANGLE's and SwiftShader's
# dependencies, and Electron's (the distribution they come from).
notices() {
  python3 - "$1" "$ELECTRON" <<'PY'
import html, re, sys
d, electron = sys.argv[1], sys.argv[2]
angle = """Copyright 2018 The ANGLE Project Authors.
All rights reserved.

Redistribution and use in source and binary forms, with or without
modification, are permitted provided that the following conditions
are met:

    Redistributions of source code must retain the above copyright
    notice, this list of conditions and the following disclaimer.

    Redistributions in binary form must reproduce the above
    copyright notice, this list of conditions and the following
    disclaimer in the documentation and/or other materials provided
    with the distribution.

    Neither the name of TransGaming Inc., Google Inc., 3DLabs Inc.
    Ltd., nor the names of their contributors may be used to endorse
    or promote products derived from this software without specific
    prior written permission.

THIS SOFTWARE IS PROVIDED BY THE COPYRIGHT HOLDERS AND CONTRIBUTORS
"AS IS" AND ANY EXPRESS OR IMPLIED WARRANTIES, INCLUDING, BUT NOT
LIMITED TO, THE IMPLIED WARRANTIES OF MERCHANTABILITY AND FITNESS
FOR A PARTICULAR PURPOSE ARE DISCLAIMED. IN NO EVENT SHALL THE
COPYRIGHT OWNER OR CONTRIBUTORS BE LIABLE FOR ANY DIRECT, INDIRECT,
INCIDENTAL, SPECIAL, EXEMPLARY, OR CONSEQUENTIAL DAMAGES (INCLUDING,
BUT NOT LIMITED TO, PROCUREMENT OF SUBSTITUTE GOODS OR SERVICES;
LOSS OF USE, DATA, OR PROFITS; OR BUSINESS INTERRUPTION) HOWEVER
CAUSED AND ON ANY THEORY OF LIABILITY, WHETHER IN CONTRACT, STRICT
LIABILITY, OR TORT (INCLUDING NEGLIGENCE OR OTHERWISE) ARISING IN
ANY WAY OUT OF THE USE OF THIS SOFTWARE, EVEN IF ADVISED OF THE
POSSIBILITY OF SUCH DAMAGE.
"""
wanted = ['SwiftShader', 'Vulkan Loader Components', 'Volk Meta loader for Vulkan API', 'Vulkan API headers',
          'VulkanMemoryAllocator', 'SPIRV-Tools', 'SPIRV-Headers', 'Abseil', 'abseil', 'xxHash', 'zlib',
          'cpu_features', 'EGL-Registry', 'OpenGL-Registry', 'Khronos header files']
page = open(f'{d}/LICENSES.chromium.html', encoding='utf-8').read()
out = [f"ANGLE, SwiftShader and the Vulkan loader, from Electron {electron}\n"
       "(https://github.com/electron/electron/releases/tag/v" + electron + "), used by gasm-run for gasm:gl.\n",
       "\n---- ANGLE (BSD-3-Clause) ----\n\n" + angle]
seen = set()
for m in re.finditer(r'<span class="title">([^<]*)</span>.*?<pre>(.*?)</pre>', page, re.S):
    title, text = html.unescape(m.group(1)), html.unescape(m.group(2)).strip()
    if title in wanted and (title, text) not in seen:
        seen.add((title, text))
        out.append(f"\n---- {title} ----\n\n{text}\n")
out.append("\n---- Electron (MIT) ----\n\n" + open(f'{d}/LICENSE', encoding='utf-8').read())
missing = [w for w in wanted if w not in {t for t, _ in seen}]
if missing:
    sys.exit(f'fetch-angle.sh: license sections missing: {missing}')
print(''.join(out))
PY
}

if [ $# -eq 0 ]; then set -- "$(host_platform)"; fi
for p in "$@"; do fetch "$p"; done
