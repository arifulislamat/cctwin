// macOS glue: native dialogs, launchctl env, Terminal and IDE launching, LaunchAgent.
use anyhow::{Result, bail};
use dispatch2::run_on_main;
use objc2::{AllocAnyThread, MainThreadMarker};
use objc2_app_kit::{NSAlert, NSAlertFirstButtonReturn, NSApplication, NSImage, NSTextField};
use objc2_foundation::{NSData, NSPoint, NSRect, NSSize, NSString};
use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::{SystemTime, UNIX_EPOCH};

const LABEL: &str = "dev.cctwin";

/// AppleScript string literal.
fn q(s: &str) -> String {
    format!("\"{}\"", s.replace('\\', "\\\\").replace('"', "\\\""))
}

/// POSIX shell single-quoted literal.
pub fn sh(s: impl AsRef<std::ffi::OsStr>) -> String {
    format!("'{}'", s.as_ref().to_string_lossy().replace('\'', r"'\''"))
}

/// Run AppleScript; stdout on success, None on error or when the user pressed Cancel.
fn osa(script: &str) -> Option<String> {
    let out = Command::new("osascript")
        .arg("-e")
        .arg(script)
        .output()
        .ok()?;
    out.status
        .success()
        .then(|| String::from_utf8_lossy(&out.stdout).trim().to_string())
}

/// Make the cctwin icon the app icon, so dialogs and the welcome window show it even for the
/// Homebrew binary, which has no app bundle.
pub fn set_app_icon(mtm: MainThreadMarker) {
    let data = NSData::with_bytes(include_bytes!("../packaging/cctwin.icns"));
    if let Some(icon) = NSImage::initWithData(NSImage::alloc(), &data) {
        unsafe { NSApplication::sharedApplication(mtm).setApplicationIconImage(Some(&icon)) };
    }
}

/// Bring cctwin to the front; a menu bar app is never active on its own.
pub fn activate(mtm: MainThreadMarker) {
    #[allow(deprecated)]
    NSApplication::sharedApplication(mtm).activateIgnoringOtherApps(true);
}

/// Native dialog with the cctwin icon, run on the main thread from any thread. The first button is
/// the default. Returns whether it was pressed and the text field's contents (when `field` is set).
fn dialog(
    title: &str,
    body: &str,
    buttons: &[&str],
    field: Option<&str>,
    destructive: bool,
) -> (bool, String) {
    let (title, body, field) = (title.to_string(), body.to_string(), field.map(String::from));
    let buttons: Vec<String> = buttons.iter().map(|b| b.to_string()).collect();
    run_on_main(move |mtm| {
        let alert = NSAlert::new(mtm);
        alert.setMessageText(&NSString::from_str(&title));
        alert.setInformativeText(&NSString::from_str(&body));
        for (i, b) in buttons.iter().enumerate() {
            let button = alert.addButtonWithTitle(&NSString::from_str(b));
            button.setHasDestructiveAction(i == 0 && destructive);
        }
        let text = field.map(|v| {
            let text = NSTextField::textFieldWithString(&NSString::from_str(&v), mtm);
            text.setFrame(NSRect::new(NSPoint::new(0., 0.), NSSize::new(260., 24.)));
            alert.setAccessoryView(Some(&text));
            alert.window().setInitialFirstResponder(Some(&text));
            text
        });
        activate(mtm);
        let ok = alert.runModal() == NSAlertFirstButtonReturn;
        (
            ok,
            text.map(|t| t.stringValue().to_string())
                .unwrap_or_default(),
        )
    })
}

pub fn alert(title: &str, body: &str) {
    dialog(title, body, &["OK"], None, false);
}

/// Alert that doesn't block the caller, for errors and confirmations.
pub fn tell(title: &str, body: &str) {
    let (title, body) = (title.to_string(), body.to_string());
    std::thread::spawn(move || alert(&title, &body));
}

pub fn confirm(title: &str, body: &str, ok: &str, destructive: bool) -> bool {
    dialog(title, body, &[ok, "Cancel"], None, destructive).0
}

pub fn input(title: &str, body: &str, default: &str) -> Option<String> {
    let (ok, text) = dialog(title, body, &["Save", "Cancel"], Some(default), false);
    ok.then_some(text)
}

/// Set CLAUDE_CONFIG_DIR for GUI apps launched from now on; None unsets it (Default account).
pub fn setenv(dir: Option<&Path>) {
    let mut cmd = Command::new("launchctl");
    match dir {
        Some(d) => cmd.arg("setenv").arg("CLAUDE_CONFIG_DIR").arg(d),
        None => cmd.arg("unsetenv").arg("CLAUDE_CONFIG_DIR"),
    };
    if let Err(e) = cmd.status() {
        eprintln!("launchctl: {e}");
    }
}

/// `export`/`unset` line binding a shell to an account folder.
pub fn env_line(dir: Option<&Path>) -> String {
    match dir {
        Some(d) => format!("export CLAUDE_CONFIG_DIR={}", sh(d)),
        None => "unset CLAUDE_CONFIG_DIR".into(),
    }
}

/// Open a new terminal window in `cwd`, run `body`, then leave an interactive login shell.
/// Uses a self-deleting .command file so any app that opens those (Terminal, iTerm2) works.
pub fn open_terminal(app: &str, root: &Path, cwd: &Path, body: &str) -> Result<()> {
    let nanos = SystemTime::now().duration_since(UNIX_EPOCH)?.as_nanos();
    let file = root.join(format!("run-{nanos}.command"));
    let script = format!(
        "#!/bin/sh\nrm -f \"$0\"\ncd {} || cd\n{body}\nexec \"${{SHELL:-/bin/zsh}}\" -l\n",
        sh(cwd)
    );
    fs::write(&file, script)?;
    fs::set_permissions(&file, fs::Permissions::from_mode(0o700))?;
    if !Command::new("open")
        .arg("-a")
        .arg(app)
        .arg(&file)
        .status()?
        .success()
    {
        let _ = fs::remove_file(&file);
        bail!("could not open {app}");
    }
    Ok(())
}

pub fn app_running(app: &str) -> bool {
    osa(&format!("application {} is running", q(app))).as_deref() == Some("true")
}

/// Open an IDE in `cwd` with the account's env. Default gets no var (launchctl already unset it).
pub fn open_ide(app: &str, dir: Option<&Path>, cwd: &Path) -> Result<()> {
    let mut cmd = Command::new("open");
    cmd.arg("-a").arg(app);
    if let Some(d) = dir {
        cmd.arg("--env")
            .arg(format!("CLAUDE_CONFIG_DIR={}", d.display()));
    }
    if !cmd.arg(cwd).status()?.success() {
        bail!("could not open {app}");
    }
    Ok(())
}

pub fn reveal(dir: &Path) {
    let _ = Command::new("open").arg(dir).status();
}

/// Absolute path of the `claude` CLI. GUI apps get launchd's bare PATH, so check the usual install
/// locations first and only then ask the user's login shell (slower, and runs their shell config).
pub fn claude_bin() -> Option<PathBuf> {
    let home = std::env::home_dir()?;
    let known = [
        home.join(".local/bin/claude"),
        "/opt/homebrew/bin/claude".into(),
        "/usr/local/bin/claude".into(),
    ];
    if let Some(p) = known.into_iter().find(|p| p.is_file()) {
        return Some(p);
    }
    let shell = std::env::var("SHELL").unwrap_or_else(|_| "/bin/zsh".into());
    let out = Command::new(shell)
        .args(["-ilc", "/usr/bin/which claude"])
        .stdin(std::process::Stdio::null())
        .output()
        .ok()?;
    String::from_utf8_lossy(&out.stdout)
        .lines()
        .rev()
        .find(|l| l.starts_with('/'))
        .map(PathBuf::from)
        .filter(|p| p.is_file())
}

fn agent_plist() -> PathBuf {
    std::env::home_dir()
        .unwrap()
        .join(format!("Library/LaunchAgents/{LABEL}.plist"))
}

/// Write or remove the LaunchAgent that starts cctwin at login.
pub fn set_launch_at_login(on: bool, exe: &Path, log: &Path) -> Result<()> {
    let plist = agent_plist();
    if !on {
        return match fs::remove_file(&plist) {
            Err(e) if e.kind() != std::io::ErrorKind::NotFound => Err(e.into()),
            _ => Ok(()),
        };
    }
    let esc = |p: &Path| {
        p.display()
            .to_string()
            .replace('&', "&amp;")
            .replace('<', "&lt;")
    };
    fs::create_dir_all(plist.parent().unwrap())?;
    fs::write(
        &plist,
        format!(
            r#"<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
  <key>Label</key><string>{LABEL}</string>
  <key>ProgramArguments</key><array><string>{}</string></array>
  <key>RunAtLoad</key><true/>
  <key>ProcessType</key><string>Interactive</string>
  <key>StandardErrorPath</key><string>{}</string>
</dict>
</plist>
"#,
            esc(exe),
            esc(log)
        ),
    )?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    // Account paths and nicknames end up inside shell scripts and AppleScript, so quoting must
    // round-trip anything without letting it run.
    const NASTY: [&str; 6] = [
        "plain",
        "with space",
        "it's",
        "$(touch /tmp/pwned)",
        "a\"b\\c",
        "`id`;\nrm -rf x",
    ];

    #[test]
    fn shell_quoting_round_trips() {
        for s in NASTY {
            let out = Command::new("sh")
                .arg("-c")
                .arg(format!("printf %s {}", sh(s)))
                .output()
                .unwrap();
            assert_eq!(String::from_utf8(out.stdout).unwrap(), s);
        }
    }

    #[test]
    fn applescript_quoting_round_trips() {
        for s in NASTY {
            let out = osa(&format!("return {}", q(s))).unwrap();
            assert_eq!(out, s.trim());
        }
    }

    #[test]
    fn env_line_quotes_paths() {
        assert_eq!(env_line(None), "unset CLAUDE_CONFIG_DIR");
        assert_eq!(
            env_line(Some(Path::new("/a b/it's"))),
            r"export CLAUDE_CONFIG_DIR='/a b/it'\''s'"
        );
    }
}
