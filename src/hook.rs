// Shell wrapper for `claude`: reads the active account on every run and records $PWD.
use anyhow::{Result, bail};
use std::fs;
use std::path::{Path, PathBuf};

const POSIX: &str = r#"claude() {
  local d="${CCTWIN_PIN:-$(cat "$HOME/.cctwin/active" 2>/dev/null)}"
  [ -d "$HOME/.cctwin" ] && printf '%s\n' "$PWD" > "$HOME/.cctwin/last_cwd"
  if [ -n "$d" ]; then CLAUDE_CONFIG_DIR="$d" command claude "$@"; else env -u CLAUDE_CONFIG_DIR claude "$@"; fi
}
"#;

const FISH: &str = r#"function claude
  set -l d $CCTWIN_PIN
  test -z "$d"; and set d (cat $HOME/.cctwin/active 2>/dev/null)
  test -d $HOME/.cctwin; and echo $PWD > $HOME/.cctwin/last_cwd
  if test -n "$d"
    CLAUDE_CONFIG_DIR=$d command claude $argv
  else
    env -u CLAUDE_CONFIG_DIR claude $argv
  end
end
"#;

pub fn script(shell: &str) -> Result<&'static str> {
    match shell {
        "zsh" | "bash" => Ok(POSIX),
        "fish" => Ok(FISH),
        _ => bail!("unsupported shell {shell:?}, use zsh, bash or fish"),
    }
}

/// Add the eval line to ~/.zshrc (always) and to the bash/fish rc files that exist. Returns files changed.
pub fn install(home: &Path, exe: &Path) -> Result<Vec<PathBuf>> {
    let exe = exe.to_string_lossy().replace('\'', r"'\''");
    let posix = |sh: &str| format!("[ -x '{exe}' ] && eval \"$('{exe}' init {sh})\"  # cctwin\n");
    let targets = [
        (home.join(".zshrc"), posix("zsh"), true),
        (home.join(".bash_profile"), posix("bash"), false),
        (home.join(".bashrc"), posix("bash"), false),
        (
            home.join(".config/fish/config.fish"),
            format!("test -x '{exe}'; and '{exe}' init fish | source  # cctwin\n"),
            false,
        ),
    ];
    let mut changed = vec![];
    for (rc, line, always) in targets {
        let old = fs::read_to_string(&rc).unwrap_or_default();
        if (!always && !rc.exists()) || old.contains(&line) {
            continue;
        }
        // Replace a line from an earlier install (e.g. a different binary path).
        let old: String = old
            .split_inclusive('\n')
            .filter(|l| !l.trim_end().ends_with("# cctwin"))
            .collect();
        let sep = if old.is_empty() || old.ends_with('\n') {
            ""
        } else {
            "\n"
        };
        fs::write(&rc, format!("{old}{sep}{line}"))?;
        changed.push(rc);
    }
    Ok(changed)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::process::Command;

    #[test]
    fn hook_selects_account() {
        // Run the zsh hook against a fake `claude` that prints its CLAUDE_CONFIG_DIR.
        let home = std::env::temp_dir().join(format!("cctwin-hook-{}", std::process::id()));
        let _ = fs::remove_dir_all(&home);
        fs::create_dir_all(home.join(".cctwin")).unwrap();
        fs::create_dir_all(home.join("bin")).unwrap();
        fs::write(
            home.join("bin/claude"),
            "#!/bin/sh\necho \"dir=${CLAUDE_CONFIG_DIR-unset}\"\n",
        )
        .unwrap();
        Command::new("chmod")
            .arg("+x")
            .arg(home.join("bin/claude"))
            .status()
            .unwrap();
        // zsh and bash both load the POSIX hook; bash is run with --norc so no user config leaks in.
        for shell in [&["zsh", "-fc"][..], &["bash", "--norc", "-c"]] {
            let run = |active: &str, env: &[(&str, &str)]| {
                fs::write(home.join(".cctwin/active"), active).unwrap();
                let script = format!("{POSIX}\ncd /tmp && claude");
                let out = Command::new(shell[0])
                    .args(&shell[1..])
                    .arg(script)
                    .env("HOME", &home)
                    .env(
                        "PATH",
                        format!("{}:/usr/bin:/bin", home.join("bin").display()),
                    )
                    .envs(env.iter().copied())
                    .output()
                    .unwrap();
                String::from_utf8(out.stdout).unwrap().trim().to_string()
            };
            assert_eq!(run("/acc/a", &[]), "dir=/acc/a", "{}", shell[0]);
            // Default strips a stale var inherited from launchctl
            assert_eq!(
                run("", &[("CLAUDE_CONFIG_DIR", "/stale")]),
                "dir=unset",
                "{}",
                shell[0]
            );
            assert_eq!(
                run("/acc/a", &[("CCTWIN_PIN", "/acc/b")]),
                "dir=/acc/b",
                "{}",
                shell[0]
            );
            let cwd = fs::read_to_string(home.join(".cctwin/last_cwd")).unwrap();
            assert_eq!(cwd.trim(), "/tmp", "{}", shell[0]);
        }

        let changed = install(&home, Path::new("/opt/x/cctwin")).unwrap();
        assert_eq!(changed, vec![home.join(".zshrc")]);
        assert!(
            install(&home, Path::new("/opt/x/cctwin"))
                .unwrap()
                .is_empty()
        ); // once only
        install(&home, Path::new("/opt/y/cctwin")).unwrap(); // new path replaces the old line
        let rc = fs::read_to_string(home.join(".zshrc")).unwrap();
        assert_eq!(rc.matches("# cctwin").count(), 1);
        assert!(rc.contains("/opt/y/cctwin"));
    }
}
