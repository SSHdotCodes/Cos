#!/bin/zsh
set -euo pipefail

ROOT_DIR="${0:A:h:h}"
OUTPUT_DIR="$ROOT_DIR/outputs"
BUILD_DIR="$(mktemp -d /tmp/cos-release.XXXXXX)"
STAGE_DIR="$(mktemp -d /tmp/cos-stage.XXXXXX)"
APP_DIR="$STAGE_DIR/Cos.app"
ICONSET_DIR="$BUILD_DIR/Cos.iconset"
VERSION="$(/usr/libexec/PlistBuddy -c 'Print :CFBundleShortVersionString' "$ROOT_DIR/scripts/Cos-Info.plist")"
ARCH="$(uname -m)"
VERSIONED_ZIP="$OUTPUT_DIR/Cos-$VERSION-macOS-$ARCH.zip"
GENERIC_ZIP="$OUTPUT_DIR/Cos-macOS-$ARCH.zip"
DMG="$OUTPUT_DIR/Cos-$VERSION.dmg"

cleanup() {
  rm -rf "$BUILD_DIR" "$STAGE_DIR"
}
trap cleanup EXIT

mkdir -p "$OUTPUT_DIR" "$APP_DIR/Contents/MacOS" "$APP_DIR/Contents/Resources"
swift build --package-path "$ROOT_DIR" --scratch-path "$BUILD_DIR" -c release
BIN_DIR="$(swift build --package-path "$ROOT_DIR" --scratch-path "$BUILD_DIR" -c release --show-bin-path)"

cp "$BIN_DIR/Cos" "$APP_DIR/Contents/MacOS/Cos"
cp "$ROOT_DIR/scripts/Cos-Info.plist" "$APP_DIR/Contents/Info.plist"

RESOURCE_BUNDLE="$(find "$BIN_DIR" -maxdepth 1 -name 'Cos_Cos.bundle' -print -quit)"
if [[ -n "$RESOURCE_BUNDLE" ]]; then
  cp -R "$RESOURCE_BUNDLE" "$APP_DIR/Contents/Resources/"
fi

swift "$ROOT_DIR/scripts/generate_icon.swift" "$ICONSET_DIR"
iconutil -c icns "$ICONSET_DIR" -o "$APP_DIR/Contents/Resources/Cos.icns"
xattr -cr "$APP_DIR"
codesign --force --deep --sign - "$APP_DIR"
codesign --verify --deep --strict --verbose=2 "$APP_DIR"

rm -f "$VERSIONED_ZIP" "$GENERIC_ZIP" "$DMG"
ditto -c -k --sequesterRsrc --keepParent "$APP_DIR" "$VERSIONED_ZIP"
cp "$VERSIONED_ZIP" "$GENERIC_ZIP"
hdiutil create -quiet -volname "Cos" -srcfolder "$APP_DIR" -ov -format UDZO "$DMG"

echo "$APP_DIR"
