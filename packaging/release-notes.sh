#!/bin/sh
# Print the GitHub release notes for a version: its CHANGELOG section plus install instructions.
#   packaging/release-notes.sh 0.1.2
set -eu

VERSION="$1"
CHANGES=$(awk -v h="## [$VERSION]" 'index($0, h) == 1 { on = 1; next } /^## \[/ { if (on) exit } on' CHANGELOG.md)
[ -n "$CHANGES" ] || { echo "No CHANGELOG.md section for $VERSION" >&2; exit 1; }

cat <<EOF
$CHANGES

## Install

\`\`\`sh
brew install arifulislamat/tap/cctwin
\`\`\`

or the install script (installs to \`~/.local/bin\`, checks the sha256):

\`\`\`sh
curl -fsSL https://raw.githubusercontent.com/arifulislamat/cctwin/main/install.sh | sh
\`\`\`

or from source: \`cargo install cctwin\` ([crates.io](https://crates.io/crates/cctwin))

or download **cctwin-$VERSION.dmg** below and drag cctwin to Applications. It is not notarized yet, so clear the download flag once before opening:

\`\`\`sh
xattr -dr com.apple.quarantine /Applications/cctwin.app && open /Applications/cctwin.app
\`\`\`

All files are one universal build for Apple Silicon and Intel. Verify a download with \`gh attestation verify <file> --repo arifulislamat/cctwin\`.
EOF
