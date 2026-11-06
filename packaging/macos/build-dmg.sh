#!/bin/sh
# Puts a release build of TypeAlike in TypeAlike.app, and the app in a disk image.
# Usage: build-dmg.sh VERSION PROGRAM OUTPUT_FOLDER
# With SIGNING_KEYCHAIN, the release certificate in that keychain signs the app. Else an ad-hoc signature does.
set -eu

version=$1
program=$2
output=$3
# macOS keeps the permissions of the app only while each release has this certificate. Never change it.
release_certificate=1dbecadbd7d25fa6a7c7780cbdaa05b23ada75fd

packaging=$(cd "$(dirname "$0")" && pwd)
staging=$(mktemp -d)
app="$staging/TypeAlike.app"

mkdir -p "$app/Contents/MacOS" "$app/Contents/Resources"
cp "$program" "$app/Contents/MacOS/TypeAlike"
cp "$packaging/../../LICENSE" "$app/Contents/Resources/LICENSE"
cp "$packaging/TypeAlike.icns" "$app/Contents/Resources/TypeAlike.icns"
sed "s/@VERSION@/$version/g" "$packaging/Info.plist" >"$app/Contents/Info.plist"
plutil -lint "$app/Contents/Info.plist"

if [ -n "${SIGNING_KEYCHAIN:-}" ]; then
    bundle_id=$(/usr/libexec/PlistBuddy -c 'Print :CFBundleIdentifier' "$app/Contents/Info.plist")
    codesign --force --sign "$release_certificate" --keychain "$SIGNING_KEYCHAIN" --timestamp=none "$app"
    codesign --verify --strict \
        --test-requirement="=identifier \"$bundle_id\" and certificate leaf = H\"$release_certificate\"" "$app"
else
    codesign --force --sign - "$app"
    codesign --verify --strict "$app"
fi

ln -s /Applications "$staging/Applications"
mkdir -p "$output"
hdiutil create -volname TypeAlike -srcfolder "$staging" -format UDZO -ov "$output/TypeAlike-$version-macos-arm64.dmg"
