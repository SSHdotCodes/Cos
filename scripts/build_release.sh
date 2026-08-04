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
NODE_VERSION="24.16.0"
BETTERWRIGHT_VERSION="1.6.3"
COS_SIGNING_IDENTITY="${COS_CODESIGN_IDENTITY:--}"
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

case "$ARCH" in
  arm64)
    NODE_ARCH="arm64"
    NODE_SHA256="39189dab4eeb15706c424af0ac08a3044c9e48f7db12a7d77f6b7aafc7dd5df6"
    ;;
  x86_64)
    NODE_ARCH="x64"
    NODE_SHA256="298b4c7b3cb80765c8703e42b90324a4ece3b6634947b89e769c3c980ab55185"
    ;;
  *)
    echo "Unsupported BetterWright runtime architecture: $ARCH" >&2
    exit 1
    ;;
esac

BETTERWRIGHT_ROOT="$APP_DIR/Contents/Resources/BetterWright"
NODE_ARCHIVE="$BUILD_DIR/node-v$NODE_VERSION-darwin-$NODE_ARCH.tar.gz"
NODE_DIST="$BUILD_DIR/node-v$NODE_VERSION-darwin-$NODE_ARCH"
BETTERWRIGHT_ARCHIVE="$BUILD_DIR/betterwright-$BETTERWRIGHT_VERSION.tgz"
mkdir -p "$BETTERWRIGHT_ROOT/runtime" "$BETTERWRIGHT_ROOT/package" "$APP_DIR/Contents/Resources/ThirdPartyLicenses"
curl -fsSL "https://nodejs.org/dist/v$NODE_VERSION/node-v$NODE_VERSION-darwin-$NODE_ARCH.tar.gz" -o "$NODE_ARCHIVE"
echo "$NODE_SHA256  $NODE_ARCHIVE" | shasum -a 256 -c -
tar -xzf "$NODE_ARCHIVE" -C "$BUILD_DIR"
cp "$NODE_DIST/bin/node" "$BETTERWRIGHT_ROOT/runtime/node"
cp "$NODE_DIST/LICENSE" "$APP_DIR/Contents/Resources/ThirdPartyLicenses/Node.js.txt"

curl -fsSL "https://registry.npmjs.org/betterwright/-/betterwright-$BETTERWRIGHT_VERSION.tgz" -o "$BETTERWRIGHT_ARCHIVE"
echo "c91525716c852431dd56410056f0e0a39f5b731bb09d962b3b067e87ed3b20f0  $BETTERWRIGHT_ARCHIVE" | shasum -a 256 -c -
"$NODE_DIST/bin/node" "$NODE_DIST/lib/node_modules/npm/bin/npm-cli.js" install \
  --prefix "$BETTERWRIGHT_ROOT/package" \
  --cache "$BUILD_DIR/npm-cache" \
  --omit=dev \
  --ignore-scripts \
  --no-audit \
  --no-fund \
  --package-lock=false \
  "$BETTERWRIGHT_ARCHIVE"
cp "$ROOT_DIR/THIRD_PARTY_LICENSES/BetterWright.txt" "$APP_DIR/Contents/Resources/ThirdPartyLicenses/BetterWright.txt"
cp "$ROOT_DIR/THIRD_PARTY_LICENSES/LobeIcons.txt" "$APP_DIR/Contents/Resources/ThirdPartyLicenses/LobeIcons.txt"
chmod 755 "$BETTERWRIGHT_ROOT/runtime/node"
test "$("$BETTERWRIGHT_ROOT/runtime/node" "$BETTERWRIGHT_ROOT/package/node_modules/betterwright/dist/bin/betterwright.js" --version)" = "$BETTERWRIGHT_VERSION"

swift "$ROOT_DIR/scripts/generate_icon.swift" "$ICONSET_DIR"
iconutil -c icns "$ICONSET_DIR" -o "$APP_DIR/Contents/Resources/Cos.icns"
xattr -cr "$APP_DIR"
codesign --force --deep --sign "$COS_SIGNING_IDENTITY" "$APP_DIR"
codesign --verify --deep --strict --verbose=2 "$APP_DIR"

rm -f "$VERSIONED_ZIP" "$GENERIC_ZIP" "$DMG"
ditto -c -k --sequesterRsrc --keepParent "$APP_DIR" "$VERSIONED_ZIP"
cp "$VERSIONED_ZIP" "$GENERIC_ZIP"
hdiutil create -quiet -volname "Cos" -srcfolder "$APP_DIR" -ov -format UDZO "$DMG"

echo "$APP_DIR"
