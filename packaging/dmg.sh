#!/bin/sh
# Wrap a cctwin binary in cctwin.app and a drag-to-Applications .dmg.
#   packaging/dmg.sh <binary> <version> <output.dmg>
# Needs dmgbuild: pipx install dmgbuild
set -eu

BIN="$1"
VERSION="$2"
OUT="$3"
HERE=$(cd "$(dirname "$0")" && pwd)
WORK=$(mktemp -d)
trap 'rm -rf "$WORK"' EXIT

APP="$WORK/cctwin.app"
mkdir -p "$APP/Contents/MacOS" "$APP/Contents/Resources"
install -m 755 "$BIN" "$APP/Contents/MacOS/cctwin"
cp "$HERE/cctwin.icns" "$APP/Contents/Resources/"
sed "s/VERSION/$VERSION/g" "$HERE/Info.plist" > "$APP/Contents/Info.plist"
plutil -lint "$APP/Contents/Info.plist" >/dev/null

# ponytail: ad-hoc signature only; Developer ID signing and notarization need an Apple Developer account.
codesign --force --sign - "$APP"

rm -f "$OUT"
# dmgbuild (pip/pipx package) lays out the window: background, icon positions, Applications link.
dmgbuild -s "$HERE/dmg-settings.py" -D app="$APP" -D res="$HERE" "cctwin $VERSION" "$OUT"
echo "Built $OUT"
