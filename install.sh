#!/bin/sh
# Install cctwin from GitHub releases:
#   curl -fsSL https://raw.githubusercontent.com/arifulislamat/cctwin/main/install.sh | sh
# Options (environment): CCTWIN_VERSION=0.1.0 to pin a version, CCTWIN_INSTALL_DIR=~/bin to change where it goes.
set -eu

REPO="arifulislamat/cctwin"
DEST="${CCTWIN_INSTALL_DIR:-$HOME/.local/bin}"

[ "$(uname -s)" = "Darwin" ] || { echo "cctwin supports macOS only for now." >&2; exit 1; }

VERSION="${CCTWIN_VERSION:-}"
if [ -z "$VERSION" ]; then
  # /releases/latest redirects to /releases/tag/v<version>
  VERSION=$(curl -fsSLI -o /dev/null -w '%{url_effective}' "https://github.com/$REPO/releases/latest")
  VERSION="${VERSION##*/tag/v}"
fi
case "$VERSION" in
  *[!0-9A-Za-z.-]* | "") echo "Could not determine the version to install (got '$VERSION')." >&2; exit 1 ;;
esac

ASSET="cctwin-$VERSION-universal-apple-darwin.tar.gz"
URL="https://github.com/$REPO/releases/download/v$VERSION/$ASSET"
TMP=$(mktemp -d)
trap 'rm -rf "$TMP"' EXIT

echo "Downloading cctwin $VERSION"
curl -fsSL -o "$TMP/$ASSET" "$URL"
curl -fsSL -o "$TMP/$ASSET.sha256" "$URL.sha256"
(cd "$TMP" && shasum -a 256 -c "$ASSET.sha256" >/dev/null) || { echo "Checksum mismatch, not installing." >&2; exit 1; }

tar -xzf "$TMP/$ASSET" -C "$TMP" cctwin
mkdir -p "$DEST"
install -m 755 "$TMP/cctwin" "$DEST/cctwin"
echo "Installed $DEST/cctwin"

case ":$PATH:" in
  *":$DEST:"*) ;;
  *) echo "Note: $DEST is not on your PATH. Add it with:"
     echo "  echo 'export PATH=\"$DEST:\$PATH\"' >> ~/.zshrc" ;;
esac
echo "Start the menu bar app with: cctwin"
