# Changelog

All notable changes to this project are documented here. The format follows
[Keep a Changelog](https://keepachangelog.com/en/1.1.0/), and versions follow
[Semantic Versioning](https://semver.org/).

## [Unreleased]

### Added

- Welcome window on first run: terms, shell hook, first account and launch at login as a checklist. Reopen it from the menu with **Welcome & setup…**.
- Native macOS dialogs with the cctwin icon for adding, renaming and removing accounts (replacing AppleScript popups).
- Styled `.dmg`: drag-to-Applications window with a background, arrow and volume icon.
- **About cctwin** (version, license, GitHub, report a problem) and **Check for updates…** in the menu. The update check only runs when clicked and tells you the update command for how you installed cctwin.

## [0.1.2] - 2026-10-18

### Fixed

- Account folders missing from the config (an interrupted Add, or a deleted `config.toml`) are added back instead of being logged out and deleted.
- Turning Shared session history off and on more than once no longer stops linking an account; identical copies are dropped and backups are never overwritten (`.cctwin-bak`, `-2`, …).
- A hotkey already taken by another app no longer stops the rest of startup; cctwin says which hotkey failed.
- Removing the active account switches to Default before deleting the folder.
- The active-account file is written atomically, so the shell hook never reads a half-written path.
- Finding `claude` for logout checks the usual install paths before running your login shell.

### Changed

- Release pages list the changes and every install method.
- GitHub Actions are pinned to exact commits.

## [0.1.1] - 2026-10-18

### Fixed

- Building on Linux or Windows now stops with a clear "macOS only" message instead of a missing-API error.

### Changed

- Releases publish to crates.io with trusted publishing and push the Homebrew formula from a tag-only environment.
- README: the `.dmg` instructions use `xattr` to clear the download flag.

## [0.1.0] - 2026-10-18

First release, macOS only.

### Added

- Menu bar app: switch Claude Code between Default (`~/.claude`) and any number of added accounts.
- Add account with a guided `claude auth login`, nicknames, rename and remove (remove logs the account out).
- Shell hook for zsh, bash and fish so `claude` follows the active account in every terminal.
- Terminal windows locked to one account, so two accounts can run side by side.
- Continue on next account (⌥⇧⌘C): reopens the last conversation in the last folder on the next account.
- Shared session history, settings, commands, agents, skills and plugins across accounts.
- Open VS Code (or any configured app) with the active account.
- Launch at login.
- CLI: `cctwin list`, `cctwin use <name>`, `cctwin init <shell>`.
- Install with Homebrew, the install script, `cargo install`, or a `.dmg` with `cctwin.app` (universal, Apple Silicon and Intel).

[Unreleased]: https://github.com/arifulislamat/cctwin/compare/v0.1.2...HEAD
[0.1.2]: https://github.com/arifulislamat/cctwin/compare/v0.1.1...v0.1.2
[0.1.1]: https://github.com/arifulislamat/cctwin/compare/v0.1.0...v0.1.1
[0.1.0]: https://github.com/arifulislamat/cctwin/releases/tag/v0.1.0
