# cctwin

[![CI](https://github.com/arifulislamat/cctwin/actions/workflows/ci.yml/badge.svg)](https://github.com/arifulislamat/cctwin/actions/workflows/ci.yml)
[![Release](https://img.shields.io/github/v/release/arifulislamat/cctwin)](https://github.com/arifulislamat/cctwin/releases/latest)
[![crates.io](https://img.shields.io/crates/v/cctwin)](https://crates.io/crates/cctwin)
[![License: MIT](https://img.shields.io/badge/license-MIT-blue)](LICENSE)

A small menu bar app that switches [Claude Code](https://docs.claude.com/en/docs/claude-code) between Claude subscriptions you own. You click once and every new `claude` session uses that account, with no `/login` needed.

Run two accounts side by side in different terminals, or pick up the same conversation on another account when one hits its usage limit. macOS only (Apple Silicon and Intel).

## Install

```sh
brew install arifulislamat/tap/cctwin
```

or with the install script (installs to `~/.local/bin`, checks the sha256):

```sh
curl -fsSL https://raw.githubusercontent.com/arifulislamat/cctwin/main/install.sh | sh
```

or from source with `cargo install cctwin`.

### Or download the app

Download `cctwin-<version>.dmg` from the [latest release](https://github.com/arifulislamat/cctwin/releases/latest), open it and drag **cctwin** to **Applications**.

The app is not notarized by Apple yet, so macOS blocks the first launch with "Apple could not verify cctwin is free of malware". Clear the download flag once, then open it:

```sh
xattr -dr com.apple.quarantine /Applications/cctwin.app && open /Applications/cctwin.app
```

(Or try to open it, then go to **System Settings → Privacy & Security** and click **Open Anyway**, if your macOS version shows the button.) Homebrew and the install script skip this step entirely.

To use the command line from the app version:

```sh
ln -s /Applications/cctwin.app/Contents/MacOS/cctwin ~/.local/bin/cctwin
```

Then start it:

```sh
cctwin
```

On first run a welcome window walks you through it: accept the terms, install the shell hook, add your first account, and choose whether cctwin starts at login. Reopen it any time with **Welcome & setup…** in the menu.

## Use

Click the critter icon in the menu bar:

```
  ✓ Default    you@company.com
    Work       you@company.com
    Personal   you@gmail.com
  ─────────────
  Continue on next account   ⌥⇧⌘C
  Open terminal (active)
  Open Visual Studio Code (active)
  Add account…
  Manage accounts            ▸  Personal ▸  Open terminal / Open VS Code / Rename… / Remove… / Show folder
  ─────────────
  Shared session history
  Launch at login
  Install shell hook
  Quit cctwin
```

- **Add account…** opens Terminal running `claude auth login` in a new folder. Log in with the account you want, and cctwin asks for a nickname once the login lands.
- **Click an account** to make it active. New `claude` sessions use it, and sessions that are already running keep theirs. Terminal tabs opened before the shell hook was installed need `source ~/.zshrc` once.
- **Manage accounts › account › Open terminal** opens a window locked to that account. It stays on it even when you switch in the menu, so you can use two accounts at the same time.
- **Continue on next account** (`⌥⇧⌘C`) is for when one plan hits its usage limit. It switches to the next account and opens Terminal in the folder where you last ran `claude`, running `claude --continue`. Turn on **Shared session history** first, so the other account can see the conversation.

The same thing works from the command line:

```sh
cctwin list            # * marks the active account
cctwin use personal    # nickname, email, id, or "default"
```

## How it works

Each account is its own Claude Code config folder in `~/.cctwin/accounts/<id>`. The **Default** account is your normal `~/.claude`. Switching changes which folder new sessions start with:

- **Terminals.** The shell hook wraps `claude` so it reads `~/.cctwin/active` on every run and sets `CLAUDE_CONFIG_DIR` (or clears it for Default).
- **GUI apps.** `launchctl setenv CLAUDE_CONFIG_DIR` applies to apps opened after the switch. An IDE that is already running keeps its old account, and cctwin warns you about that.

The login for each account stays where Claude Code puts it: the macOS Keychain, scoped per folder. cctwin never reads, copies or sends tokens. The only file it reads inside an account folder is `.claude.json`, and only to show the email.

### Shell hook

**Install shell hook** adds one line to `~/.zshrc`, plus `~/.bash_profile`, `~/.bashrc` and fish's `config.fish` if those files exist. To add it by hand:

```sh
eval "$(cctwin init zsh)"      # zsh or bash
cctwin init fish | source      # fish
```

### Shared session history

When this is on, each account folder gets symlinks to the matching items in `~/.claude`, so all accounts share:

`projects/` (conversation history) · `settings.json` · `CLAUDE.md` · `commands/` · `agents/` · `skills/` · `plugins/`

Logins (`.claude.json`, Keychain) are never shared. User-scope MCP servers are stored in `.claude.json`, so they stay per account. Turning sharing on merges each account's existing history into `~/.claude/projects`. If an account has its own `settings.json` (or similar), it is kept as `settings.json.cctwin-bak`. Turning sharing off copies the shared items back into each account.

### Files

```
~/.cctwin/config.toml     settings and account list
~/.cctwin/active          path of the active folder (empty = Default)
~/.cctwin/last_cwd        last folder `claude` ran in
~/.cctwin/accounts/<id>/  one Claude Code config folder per account
~/.cctwin/cctwin.log      errors from the background app
```

`config.toml` options:

```toml
hotkey_next = "CmdOrCtrl+Alt+Shift+C"
terminal = "Terminal"            # or "iTerm"
ide_app = "Visual Studio Code"   # or "Cursor", any app name
```

Restart cctwin after editing.

## Terms

Only add Claude accounts you personally own. Sharing logins with other people is against Anthropic's terms. Switching is always your own click or key press; cctwin never rotates accounts automatically. Check Anthropic's current terms yourself.

cctwin is not affiliated with Anthropic.

## Uninstall

1. Untick **Launch at login** and quit cctwin.
2. Remove each account from the menu, which also logs it out.
3. Run `brew uninstall cctwin` or `rm ~/.local/bin/cctwin`.
4. Delete the `# cctwin` line from your shell rc and run `rm -rf ~/.cctwin`.

## Build

```sh
cargo test
cargo build --release   # target/release/cctwin
```

See [CONTRIBUTING.md](CONTRIBUTING.md) for the test suite, manual checks and the release process.

## Verify a download

Every release archive has a `.sha256` file and a [build provenance attestation](https://docs.github.com/en/actions/security-for-github-actions/using-artifact-attestations) that ties it to the commit and workflow that built it:

```sh
gh attestation verify cctwin-0.1.0.dmg --repo arifulislamat/cctwin
```

The app is ad-hoc signed but not notarized yet. Installs through Homebrew, the install script and `cargo install` are not quarantined by macOS, so they run as is; the `.dmg` needs the one-time step above.
