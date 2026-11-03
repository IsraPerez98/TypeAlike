#!/bin/sh
# Puts a release build of TypeAlike in TypeAlike.app, and the app in a disk image.
# Usage: build-dmg.sh VERSION PROGRAM OUTPUT_FOLDER
set -eu

version=$1
program=$2
output=$3

packaging=$(cd "$(dirname "$0")" && pwd)
staging=$(mktemp -d)
app="$staging/TypeAlike.app"

mkdir -p "$app/Contents/MacOS" "$app/Contents/Resources"
cp "$program" "$app/Contents/MacOS/TypeAlike"
cp "$packaging/../../LICENSE" "$app/Contents/Resources/LICENSE"
sed "s/@VERSION@/$version/g" "$packaging/Info.plist" >"$app/Contents/Info.plist"
plutil -lint "$app/Contents/Info.plist"

# Without a Developer ID, an ad-hoc signature seals the bundle, so macOS can open it after the user allows it.
codesign --force --sign - "$app"
codesign --verify --strict "$app"

ln -s /Applications "$staging/Applications"
mkdir -p "$output"
hdiutil create -volname TypeAlike -srcfolder "$staging" -format UDZO -ov "$output/TypeAlike-$version-macos-arm64.dmg"
