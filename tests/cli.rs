// Smoke tests for the command-line surface, run against a throwaway HOME.
use std::process::{Command, Output};

fn cctwin(home: &std::path::Path, args: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_cctwin"))
        .args(args)
        .env("HOME", home)
        .output()
        .unwrap()
}

#[test]
fn cli() {
    let home = std::env::temp_dir().join(format!("cctwin-cli-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&home);
    std::fs::create_dir_all(&home).unwrap();
    let text = |o: &Output| String::from_utf8_lossy(&o.stdout).into_owned();

    let v = cctwin(&home, &["--version"]);
    assert!(v.status.success());
    assert_eq!(
        text(&v).trim(),
        format!("cctwin {}", env!("CARGO_PKG_VERSION"))
    );

    for shell in ["zsh", "bash", "fish"] {
        let o = cctwin(&home, &["init", shell]);
        assert!(
            o.status.success() && text(&o).contains("CLAUDE_CONFIG_DIR"),
            "init {shell}"
        );
    }
    assert_eq!(cctwin(&home, &["init", "tcsh"]).status.code(), Some(1));
    assert_eq!(cctwin(&home, &["bogus"]).status.code(), Some(2));

    // Fresh home: only Default, marked active.
    let l = cctwin(&home, &["list"]);
    assert!(l.status.success());
    assert!(text(&l).starts_with("* Default"));
    assert_eq!(cctwin(&home, &["use", "nobody"]).status.code(), Some(1));
}
