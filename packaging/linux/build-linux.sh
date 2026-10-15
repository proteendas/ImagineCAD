#!/usr/bin/env bash
# Build Linux release artifacts: .deb (via cargo-deb) and a portable .tar.gz,
# both bundling the LibreDWG tools for DWG support.
#   LIBREDWG_DIR=dist/libredwg packaging/linux/build-linux.sh
set -euo pipefail
cd "$(dirname "$0")/../.."
VERSION=$(grep -m1 '^version' Cargo.toml | sed -E 's/.*"(.*)".*/\1/')
ARCH=$(uname -m)
mkdir -p dist
LIBREDWG_DIR=${LIBREDWG_DIR:-dist/libredwg}
if [[ ! -x "$LIBREDWG_DIR/dwg2dxf" ]]; then
  packaging/libredwg/build-libredwg.sh "$LIBREDWG_DIR"
fi
# cargo-deb asset paths are fixed: stage LibreDWG where Cargo.toml expects it.
if [[ "$(cd "$LIBREDWG_DIR" && pwd)" != "$(pwd)/dist/libredwg" ]]; then
  mkdir -p dist/libredwg && cp "$LIBREDWG_DIR"/* dist/libredwg/
fi
[[ -f dist/THIRD_PARTY_LICENSES.html ]] || echo "<p>See https://crates.io for dependency licenses.</p>" > dist/THIRD_PARTY_LICENSES.html
cargo build --release -p imaginecad
command -v cargo-deb >/dev/null || cargo install cargo-deb --locked
cargo deb -p imaginecad --no-build --output dist/
STAGE=$(mktemp -d)/imaginecad-$VERSION
mkdir -p "$STAGE/libredwg"
cp target/release/imaginecad README.md LICENSE NOTICE dist/THIRD_PARTY_LICENSES.html "$STAGE/"
cp dist/libredwg/* "$STAGE/libredwg/"
cp packaging/linux/imaginecad.desktop assets/icon-256.png "$STAGE/"
tar -C "$(dirname "$STAGE")" -czf "dist/ImagineCAD-$VERSION-linux-$ARCH.tar.gz" "imaginecad-$VERSION"
echo "Built dist/*.deb and dist/ImagineCAD-$VERSION-linux-$ARCH.tar.gz"
