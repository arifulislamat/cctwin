#!/bin/sh
# Wrap a cctwin binary in cctwin.app and a drag-to-Applications .dmg.
#   packaging/dmg.sh <binary> <version> <output.dmg>
set -eu

BIN="$1"
VERSION="$2"
OUT="$3"
HERE=$(cd "$(dirname "$0")" && pwd)
WORK=$(mktemp -d)
trap 'rm -rf "$WORK"' EXIT

APP="$WORK/root/cctwin.app"
mkdir -p "$APP/Contents/MacOS" "$APP/Contents/Resources"
install -m 755 "$BIN" "$APP/Contents/MacOS/cctwin"
cp "$HERE/cctwin.icns" "$APP/Contents/Resources/"
sed "s/VERSION/$VERSION/g" "$HERE/Info.plist" > "$APP/Contents/Info.plist"
plutil -lint "$APP/Contents/Info.plist" >/dev/null

# ponytail: ad-hoc signature only; Developer ID signing and notarization need an Apple Developer account.
codesign --force --sign - "$APP"

ln -s /Applications "$WORK/root/Applications"
rm -f "$OUT"
hdiutil create -quiet -volname "cctwin $VERSION" -srcfolder "$WORK/root" -format UDZO "$OUT"
echo "Built $OUT"
