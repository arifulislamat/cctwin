// macOS glue: osascript dialogs, launchctl env, Terminal and IDE launching, LaunchAgent.
use anyhow::{Result, bail};
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

/// Non-blocking alert. (osascript notifications belong to Script Editor and open it when clicked.)
pub fn tell(msg: String) {
    std::thread::spawn(move || alert(&msg));
}

pub fn alert(msg: &str) {
    osa(&format!(
        "display dialog {} with title \"cctwin\" buttons {{\"OK\"}} default button \"OK\"",
        q(msg)
    ));
}

pub fn confirm(msg: &str, ok: &str) -> bool {
    let script = format!(
        "display dialog {} with title \"cctwin\" buttons {{\"Cancel\", {ok}}} default button {ok} cancel button \"Cancel\"",
        q(msg),
        ok = q(ok)
    );
    osa(&script).is_some()
}

pub fn input(msg: &str, default: &str) -> Option<String> {
    osa(&format!(
        "text returned of (display dialog {} default answer {} with title \"cctwin\")",
        q(msg),
        q(default)
    ))
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
