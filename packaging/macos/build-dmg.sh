#!/bin/sh
# Puts a release build of GhostType in GhostType.app, and the app in a disk image.
# Usage: build-dmg.sh VERSION PROGRAM OUTPUT_FOLDER
set -eu

version=$1
program=$2
output=$3

packaging=$(cd "$(dirname "$0")" && pwd)
staging=$(mktemp -d)
app="$staging/GhostType.app"

mkdir -p "$app/Contents/MacOS"
cp "$program" "$app/Contents/MacOS/GhostType"
sed "s/@VERSION@/$version/g" "$packaging/Info.plist" >"$app/Contents/Info.plist"
plutil -lint "$app/Contents/Info.plist"

# Without a Developer ID, an ad-hoc signature seals the bundle, so macOS can open it after the user allows it.
codesign --force --sign - "$app"
codesign --verify --strict "$app"

ln -s /Applications "$staging/Applications"
mkdir -p "$output"
hdiutil create -volname GhostType -srcfolder "$staging" -format UDZO -ov "$output/GhostType-$version-macos-arm64.dmg"
