# Security Policy

cctwin sits next to your Claude Code logins. It never reads them, and security reports are handled privately.

## Reporting a vulnerability

**Do not open a public issue.** Use GitHub's private vulnerability reporting:

1. Go to the repository's **Security** tab.
2. Click **Report a vulnerability**.
3. Describe the issue, the cctwin version, and how to reproduce it.

You should receive an initial response within 72 hours.

## Scope

Of particular interest:

- Any path where cctwin reads, copies, logs or sends `.credentials.json`, Keychain items or OAuth tokens. It must only ever read `oauthAccount.emailAddress` from `.claude.json`.
- Command or AppleScript injection through account paths, nicknames or config values.
- Removing an account deleting anything outside its own folder (for example following a shared-history symlink into `~/.claude`).
- Tampered release downloads. Each release archive has a sha256 file and a build provenance attestation (`gh attestation verify <file> --repo arifulislamat/cctwin`).

## Supported versions

Only the latest release receives security fixes.
