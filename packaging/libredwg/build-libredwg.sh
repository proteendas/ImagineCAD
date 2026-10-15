#!/usr/bin/env bash
# Build GNU LibreDWG's dwg2dxf / dxf2dwg tools (static) for bundling with
# ImagineCAD, applying ImagineCAD's patches.
#
#   packaging/libredwg/build-libredwg.sh OUT_DIR [ARCH]
#
# ARCH (macOS only): arm64 | x86_64 | universal (default: native).
# Works on macOS, Linux and Windows (MSYS2 MINGW64 shell).
set -euo pipefail
OUT=${1:?usage: build-libredwg.sh OUT_DIR [ARCH]}
ARCH=${2:-native}
VERSION=0.13.3
SHA256=83f1f6e78a744777a481ff4520e4cef3f8ac4b2c1c25671077ca12fe81e8816e
HERE=$(cd "$(dirname "$0")" && pwd)
WORK=$(mktemp -d)
mkdir -p "$OUT"
OUT=$(cd "$OUT" && pwd)

curl -fsSL -o "$WORK/libredwg.tar.xz" "https://ftp.gnu.org/gnu/libredwg/libredwg-$VERSION.tar.xz"
ACTUAL=$( (sha256sum "$WORK/libredwg.tar.xz" 2>/dev/null || shasum -a 256 "$WORK/libredwg.tar.xz") | cut -d' ' -f1)
if [ "$ACTUAL" != "$SHA256" ]; then
  echo "checksum mismatch for libredwg-$VERSION.tar.xz: $ACTUAL" >&2
  exit 1
fi
tar -xf "$WORK/libredwg.tar.xz" -C "$WORK"

build_one() { # $1 = arch flag ('' or -arch X), $2 = install prefix
  local src="$WORK/build-$(basename "$2")"
  rm -rf "$src" && cp -pR "$WORK/libredwg-$VERSION" "$src" # -p: keep timestamps so make does not rerun autotools
  (cd "$src" && for p in "$HERE"/*.patch; do patch -p1 < "$p"; done)
  local ldflags=""
  case "$(uname -s)" in MINGW*|MSYS*) ldflags="-static" ;; esac
  (cd "$src" && CFLAGS="-O2 $1" LDFLAGS="$ldflags $1" ./configure --disable-shared --enable-static \
      --disable-bindings --disable-python --disable-docs --prefix="$2" ${3:-} >/dev/null \
   && make -j"$(getconf _NPROCESSORS_ONLN 2>/dev/null || echo 4)" >/dev/null && make install >/dev/null)
}

EXE=""
case "$(uname -s)" in MINGW*|MSYS*) EXE=".exe" ;; esac
if [ "$(uname -s)" = "Darwin" ] && [ "$ARCH" = "universal" ]; then
  build_one "-arch arm64" "$WORK/arm64" "--host=aarch64-apple-darwin"
  build_one "-arch x86_64" "$WORK/x86_64" "--host=x86_64-apple-darwin"
  for t in dwg2dxf dxf2dwg; do
    lipo -create -output "$OUT/$t" "$WORK/arm64/bin/$t" "$WORK/x86_64/bin/$t"
  done
elif [ "$(uname -s)" = "Darwin" ] && [ "$ARCH" != "native" ]; then
  build_one "-arch $ARCH" "$WORK/$ARCH"
  cp "$WORK/$ARCH/bin/dwg2dxf" "$WORK/$ARCH/bin/dxf2dwg" "$OUT/"
else
  build_one "" "$WORK/native"
  cp "$WORK/native/bin/dwg2dxf$EXE" "$WORK/native/bin/dxf2dwg$EXE" "$OUT/"
fi
strip "$OUT/dwg2dxf$EXE" "$OUT/dxf2dwg$EXE" 2>/dev/null || true
cp "$WORK/libredwg-$VERSION/COPYING" "$OUT/LIBREDWG-COPYING.txt"
cat > "$OUT/LIBREDWG-SOURCE.txt" <<TXT
These programs are GNU LibreDWG $VERSION (GPL-3.0-or-later).
Source: https://ftp.gnu.org/gnu/libredwg/libredwg-$VERSION.tar.xz
ImagineCAD patches: packaging/libredwg/*.patch in the ImagineCAD source repository.
TXT
rm -rf "$WORK"
echo "LibreDWG tools in $OUT:"
ls -l "$OUT"
