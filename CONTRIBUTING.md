# Contributing to cctwin

Thanks for your interest in contributing.

## Ground rules

- **Never touch tokens.** cctwin only tells the official `claude` binary which config folder to use. The only file it reads inside an account folder is `.claude.json`, for the email. A PR that reads, copies or logs credentials will not be merged.
- **Every switch is the user's action.** No automatic rotation between accounts; it is a ban risk for users.
- **Keep it small.** Few dependencies, few files, no settings window. macOS glue lives in `src/mac.rs`.
- Small, reviewable PRs: one focused change each. For larger changes, open an issue first.

## Build and test

```bash
cargo fmt --check
cargo clippy --all-targets -- -D warnings
cargo test
cargo run              # starts the menu bar app from source
```

The tests run the shell hook under zsh and bash with a fake `claude`, check shell and AppleScript quoting against hostile strings, and exercise shared-history linking in a temporary folder. They never touch your real `~/.claude` or `~/.cctwin`.

## Manual checks

The menu bar UI can't be tested in CI. Before a release, or after a menu change, check by hand:

- [ ] Add account: Terminal opens `claude auth login`, a nickname prompt appears after login
- [ ] Switch: `/status` in a new terminal shows the chosen account; a running session keeps its own
- [ ] Manage accounts › account › Open terminal stays on that account after switching in the menu
- [ ] Rename and Remove (Remove logs the account out and deletes its folder)
- [ ] Shared session history on, then the hotkey (⌥⇧⌘C) continues the last conversation on the next account
- [ ] Open VS Code warns when VS Code is already running
- [ ] Launch at login survives a reboot

## Project structure

```
src/main.rs    CLI commands, menu bar app, menu actions
src/state.rs   config, accounts, active account, shared-history links
src/mac.rs     osascript dialogs, launchctl, Terminal and IDE launching, LaunchAgent
src/hook.rs    the `claude` shell wrapper for zsh, bash and fish
tests/cli.rs   command-line smoke tests
install.sh     curl installer
packaging/     cctwin.app Info.plist, app icon (and the script that draws it), dmg.sh
```

## Releasing

1. Update `version` in `Cargo.toml` and move "Unreleased" in `CHANGELOG.md` under the new version.
2. Commit, then tag and push: `git tag v0.2.0 && git push origin v0.2.0`.
3. The release workflow builds a universal binary and `.dmg`, publishes the GitHub release, crates.io and the Homebrew formula, then installs the release on a clean runner to verify it.

## Reporting security issues

Never open a public issue for a security problem. See [SECURITY.md](SECURITY.md).
