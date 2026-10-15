#!/usr/bin/env bash
# Build ImagineCAD.app and a drag-to-Applications .dmg.
#   packaging/macos/build-dmg.sh              # native architecture
#   UNIVERSAL=1 packaging/macos/build-dmg.sh  # Apple silicon + Intel universal binary
#
# Optional environment:
#   LIBREDWG_DIR        directory with dwg2dxf/dxf2dwg to bundle (DWG support)
#   MACOS_SIGN_IDENTITY "Developer ID Application: …" (default: ad-hoc signature)
#   APPLE_ID, APPLE_TEAM_ID, APPLE_APP_PASSWORD  → notarise and staple the .dmg
set -euo pipefail
cd "$(dirname "$0")/../.."
VERSION=$(grep -m1 '^version' Cargo.toml | sed -E 's/.*"(.*)".*/\1/')
OUT=dist
APP="$OUT/ImagineCAD.app"
IDENTITY="${MACOS_SIGN_IDENTITY:--}"
rm -rf "$APP" && mkdir -p "$APP/Contents/MacOS" "$APP/Contents/Resources"

if [[ "${UNIVERSAL:-0}" == "1" ]]; then
  rustup target add aarch64-apple-darwin x86_64-apple-darwin
  cargo build --release -p imaginecad --target aarch64-apple-darwin
  cargo build --release -p imaginecad --target x86_64-apple-darwin
  lipo -create -output "$APP/Contents/MacOS/imaginecad" \
    target/aarch64-apple-darwin/release/imaginecad target/x86_64-apple-darwin/release/imaginecad
  ARCH=universal
else
  cargo build --release -p imaginecad
  cp target/release/imaginecad "$APP/Contents/MacOS/imaginecad"
  ARCH=$(uname -m)
fi

sed "s/__VERSION__/$VERSION/g" packaging/macos/Info.plist > "$APP/Contents/Info.plist"
cp assets/icon.icns "$APP/Contents/Resources/icon.icns"
cp LICENSE NOTICE "$APP/Contents/Resources/"
[[ -f "$OUT/THIRD_PARTY_LICENSES.html" ]] && cp "$OUT/THIRD_PARTY_LICENSES.html" "$APP/Contents/Resources/"
if [[ -n "${LIBREDWG_DIR:-}" ]]; then
  mkdir -p "$APP/Contents/Resources/libredwg"
  cp "$LIBREDWG_DIR"/dwg2dxf "$LIBREDWG_DIR"/dxf2dwg "$LIBREDWG_DIR"/LIBREDWG-* "$APP/Contents/Resources/libredwg/"
fi

# Sign nested executables first, then the bundle (hardened runtime for notarisation).
SIGN=(codesign --force --timestamp --options runtime --sign "$IDENTITY")
[[ "$IDENTITY" == "-" ]] && SIGN=(codesign --force --sign -)
for t in "$APP/Contents/Resources/libredwg/dwg2dxf" "$APP/Contents/Resources/libredwg/dxf2dwg"; do
  [[ -f "$t" ]] && "${SIGN[@]}" "$t"
done
"${SIGN[@]}" "$APP"

DMG="$OUT/ImagineCAD-$VERSION-macos-$ARCH.dmg"
STAGE=$(mktemp -d)
cp -R "$APP" "$STAGE/"
ln -s /Applications "$STAGE/Applications"
rm -f "$DMG"
hdiutil create -volname "ImagineCAD $VERSION" -srcfolder "$STAGE" -ov -format UDZO "$DMG" >/dev/null
rm -rf "$STAGE"
[[ "$IDENTITY" != "-" ]] && codesign --force --timestamp --sign "$IDENTITY" "$DMG"

if [[ -n "${APPLE_ID:-}" && -n "${APPLE_TEAM_ID:-}" && -n "${APPLE_APP_PASSWORD:-}" ]]; then
  xcrun notarytool submit "$DMG" --apple-id "$APPLE_ID" --team-id "$APPLE_TEAM_ID" --password "$APPLE_APP_PASSWORD" --wait
  xcrun stapler staple "$DMG"
fi
echo "Built $DMG"
