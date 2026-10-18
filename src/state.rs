// Config, accounts and the active-account file. Everything lives in ~/.cctwin.
use anyhow::{Context, Result, bail};
use serde::{Deserialize, Serialize};
use std::fs;
use std::hash::BuildHasher;
use std::os::unix::fs::{DirBuilderExt, symlink};
use std::path::{Path, PathBuf};
use std::process::Command;

/// Items symlinked from every account folder to ~/.claude when shared history is on.
/// Never credentials or .claude.json: those hold the account identity.
pub const SHARED: [&str; 7] = [
    "projects",
    "settings.json",
    "CLAUDE.md",
    "commands",
    "agents",
    "skills",
    "plugins",
];

pub struct Paths {
    pub root: PathBuf,         // ~/.cctwin
    pub claude_home: PathBuf,  // ~/.claude, the Default account and the shared store
    pub default_json: PathBuf, // ~/.claude.json, the Default account identity
}

impl Paths {
    pub fn new() -> Paths {
        Paths::from_home(&std::env::home_dir().expect("no home dir"))
    }

    pub fn from_home(home: &Path) -> Paths {
        Paths {
            root: home.join(".cctwin"),
            claude_home: home.join(".claude"),
            default_json: home.join(".claude.json"),
        }
    }

    pub fn config(&self) -> PathBuf {
        self.root.join("config.toml")
    }
    pub fn active_file(&self) -> PathBuf {
        self.root.join("active")
    }
    pub fn last_cwd_file(&self) -> PathBuf {
        self.root.join("last_cwd")
    }
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct Account {
    pub id: String,
    pub nickname: String,
    pub dir: PathBuf,
}

#[derive(Serialize, Deserialize, Debug, PartialEq)]
#[serde(default)]
pub struct Config {
    pub hotkey_next: String,
    pub terminal: String,
    pub ide_app: String,
    pub shared_history: bool,
    pub launch_at_login: bool,
    pub notice_shown: bool,
    pub accounts: Vec<Account>,
}

impl Default for Config {
    fn default() -> Config {
        Config {
            hotkey_next: "CmdOrCtrl+Alt+Shift+C".into(),
            terminal: "Terminal".into(),
            ide_app: "Visual Studio Code".into(),
            shared_history: false,
            launch_at_login: false,
            notice_shown: false,
            accounts: vec![],
        }
    }
}

impl Config {
    pub fn load(p: &Paths) -> Result<Config> {
        match fs::read_to_string(p.config()) {
            Ok(s) => toml::from_str(&s).context("bad ~/.cctwin/config.toml"),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(Config::default()),
            Err(e) => Err(e.into()),
        }
    }

    // ponytail: whole-file read/modify/write with no lock; fine for one user clicking a tray menu.
    pub fn save(&self, p: &Paths) -> Result<()> {
        mkdir_private(&p.root)?;
        let tmp = p.root.join("config.toml.tmp");
        fs::write(&tmp, toml::to_string(self)?)?;
        fs::rename(tmp, p.config())?;
        Ok(())
    }

    /// Account matching a nickname, email or id, case-insensitive.
    pub fn find(&self, p: &Paths, name: &str) -> Option<&Account> {
        let n = name.to_lowercase();
        self.accounts.iter().find(|a| {
            a.nickname.to_lowercase() == n
                || a.id == n
                || email(p, Some(a)).is_some_and(|e| e.to_lowercase() == n)
        })
    }

    pub fn active_index(&self, p: &Paths) -> Option<usize> {
        let dir = active_dir(p)?;
        self.accounts.iter().position(|a| a.dir == dir)
    }

    /// Next account after `current` in menu order, skipping Default. None when there is no other account.
    pub fn next_index(&self, current: Option<usize>) -> Option<usize> {
        if self.accounts.is_empty() {
            return None;
        }
        let next = current.map_or(0, |i| (i + 1) % self.accounts.len());
        (Some(next) != current).then_some(next)
    }

    /// Trimmed nickname if it is 1..=32 chars and not used by another account.
    pub fn validate_nickname(&self, name: &str, except_id: Option<&str>) -> Result<String> {
        let name = name.trim();
        if name.is_empty() || name.chars().count() > 32 {
            bail!("Nickname must be 1 to 32 characters.");
        }
        if name.eq_ignore_ascii_case("default") {
            bail!("\"Default\" is reserved.");
        }
        let taken = self
            .accounts
            .iter()
            .any(|a| Some(a.id.as_str()) != except_id && a.nickname.eq_ignore_ascii_case(name));
        if taken {
            bail!("Nickname \"{name}\" is already used.");
        }
        Ok(name.to_string())
    }
}

/// Email from a Claude Code .claude.json (oauthAccount.emailAddress). The only file read in an account folder.
pub fn read_email(json: &Path) -> Option<String> {
    let v: serde_json::Value = serde_json::from_str(&fs::read_to_string(json).ok()?).ok()?;
    v.pointer("/oauthAccount/emailAddress")?
        .as_str()
        .filter(|s| !s.is_empty())
        .map(String::from)
}

/// Email for an account, or for Default when `acc` is None.
pub fn email(p: &Paths, acc: Option<&Account>) -> Option<String> {
    match acc {
        Some(a) => read_email(&a.dir.join(".claude.json")),
        None => read_email(&p.default_json),
    }
}

/// Nickname, then email, then folder id.
pub fn label(acc: &Account, email: Option<&str>) -> String {
    if !acc.nickname.is_empty() {
        acc.nickname.clone()
    } else {
        email.map_or_else(|| acc.id.clone(), String::from)
    }
}

/// Active account folder; None means Default (CLAUDE_CONFIG_DIR unset).
pub fn active_dir(p: &Paths) -> Option<PathBuf> {
    let s = fs::read_to_string(p.active_file()).ok()?;
    let s = s.trim();
    (!s.is_empty()).then(|| PathBuf::from(s))
}

pub fn set_active(p: &Paths, dir: Option<&Path>) -> Result<()> {
    mkdir_private(&p.root)?;
    let s = dir
        .map(|d| d.to_string_lossy().into_owned())
        .unwrap_or_default();
    fs::write(p.active_file(), s)?;
    Ok(())
}

pub fn last_cwd(p: &Paths) -> PathBuf {
    fs::read_to_string(p.last_cwd_file())
        .ok()
        .map(|s| PathBuf::from(s.trim()))
        .filter(|d| d.is_dir())
        .unwrap_or_else(|| p.root.parent().unwrap().to_path_buf())
}

pub fn mkdir_private(dir: &Path) -> Result<()> {
    fs::DirBuilder::new()
        .recursive(true)
        .mode(0o700)
        .create(dir)
        .with_context(|| format!("create {}", dir.display()))
}

/// Create ~/.cctwin/accounts/<random id> with 0700.
pub fn create_account_dir(p: &Paths) -> Result<(String, PathBuf)> {
    loop {
        let id = format!(
            "{:06x}",
            std::hash::RandomState::new().hash_one(0u8) & 0xff_ffff
        );
        let dir = p.root.join("accounts").join(&id);
        if !dir.exists() {
            mkdir_private(&dir)?;
            return Ok((id, dir));
        }
    }
}

/// Delete an account folder. remove_dir_all removes symlinks, never their targets, so ~/.claude is safe.
pub fn delete_account_dir(dir: &Path) -> Result<()> {
    match fs::remove_dir_all(dir) {
        Err(e) if e.kind() != std::io::ErrorKind::NotFound => Err(e.into()),
        _ => Ok(()),
    }
}

/// Point an account's shared items at ~/.claude. Existing projects are merged in first; other real
/// items are moved aside to <item>.cctwin-bak. Items missing from ~/.claude are skipped (except projects).
pub fn link_shared(p: &Paths, dir: &Path) -> Result<()> {
    mkdir_private(&p.claude_home.join("projects"))?;
    for item in SHARED {
        let target = p.claude_home.join(item);
        let link = dir.join(item);
        if link.is_symlink() || !target.exists() {
            continue;
        }
        if link.exists() {
            if item == "projects" && merge_into(&link, &target)? == 0 {
                fs::remove_dir_all(&link)?;
            } else {
                let bak = dir.join(format!("{item}.cctwin-bak"));
                if bak.exists() {
                    continue; // never overwrite an earlier backup
                }
                fs::rename(&link, bak)?;
            }
        }
        symlink(&target, &link)?;
    }
    Ok(())
}

/// Replace each shared symlink with a real copy of what it points at.
pub fn unlink_shared(dir: &Path) -> Result<()> {
    for item in SHARED {
        let link = dir.join(item);
        if !link.is_symlink() {
            continue;
        }
        let target = fs::read_link(&link)?;
        fs::remove_file(&link)?;
        if target.exists() {
            let ok = Command::new("cp")
                .arg("-Rp")
                .arg(&target)
                .arg(&link)
                .status()?
                .success();
            if !ok {
                bail!("cp {} failed", target.display());
            }
        }
    }
    Ok(())
}

/// Move files from src into dst, keeping dst's file on a name clash. Returns how many were left behind.
fn merge_into(src: &Path, dst: &Path) -> Result<usize> {
    mkdir_private(dst)?;
    let mut left = 0;
    for entry in fs::read_dir(src)? {
        let entry = entry?;
        let to = dst.join(entry.file_name());
        if entry.file_type()?.is_dir() {
            left += merge_into(&entry.path(), &to)?;
        } else if to.exists() {
            left += 1;
        } else {
            fs::rename(entry.path(), to)?;
        }
    }
    Ok(left)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tmp_home(name: &str) -> PathBuf {
        let d = std::env::temp_dir().join(format!("cctwin-test-{name}-{}", std::process::id()));
        let _ = fs::remove_dir_all(&d);
        fs::create_dir_all(&d).unwrap();
        d
    }

    fn acc(id: &str, nick: &str, dir: &Path) -> Account {
        Account {
            id: id.into(),
            nickname: nick.into(),
            dir: dir.into(),
        }
    }

    #[test]
    fn config_round_trip_and_defaults() {
        let p = Paths::from_home(&tmp_home("cfg"));
        assert_eq!(Config::load(&p).unwrap(), Config::default());
        let mut c = Config::default();
        c.accounts.push(acc("a1", "Work", Path::new("/x")));
        c.shared_history = true;
        c.save(&p).unwrap();
        assert_eq!(Config::load(&p).unwrap(), c);
        // partial file falls back to defaults for missing keys
        fs::write(p.config(), "terminal = \"iTerm\"\n").unwrap();
        let c = Config::load(&p).unwrap();
        assert_eq!(c.terminal, "iTerm");
        assert_eq!(c.hotkey_next, "CmdOrCtrl+Alt+Shift+C");
    }

    #[test]
    fn email_and_label() {
        let home = tmp_home("email");
        let j = home.join("a.json");
        assert_eq!(read_email(&j), None);
        fs::write(&j, "not json").unwrap();
        assert_eq!(read_email(&j), None);
        fs::write(&j, r#"{"oauthAccount":{}}"#).unwrap();
        assert_eq!(read_email(&j), None);
        fs::write(&j, r#"{"oauthAccount":{"emailAddress":"a@b.c"}}"#).unwrap();
        assert_eq!(read_email(&j).as_deref(), Some("a@b.c"));

        assert_eq!(label(&acc("id1", "Work", &home), Some("a@b.c")), "Work");
        assert_eq!(label(&acc("id1", "", &home), Some("a@b.c")), "a@b.c");
        assert_eq!(label(&acc("id1", "", &home), None), "id1");
    }

    #[test]
    fn active_and_next() {
        let p = Paths::from_home(&tmp_home("active"));
        assert_eq!(active_dir(&p), None);
        let mut c = Config::default();
        assert_eq!(c.next_index(None), None);
        c.accounts = vec![
            acc("a", "A", Path::new("/a")),
            acc("b", "B", Path::new("/b")),
        ];
        set_active(&p, Some(Path::new("/b"))).unwrap();
        assert_eq!(c.active_index(&p), Some(1));
        assert_eq!(c.next_index(Some(1)), Some(0));
        assert_eq!(c.next_index(None), Some(0)); // from Default
        set_active(&p, None).unwrap();
        assert_eq!(active_dir(&p), None);
        c.accounts.truncate(1);
        assert_eq!(c.next_index(Some(0)), None); // nothing else to move to
    }

    #[test]
    fn nickname_rules() {
        let mut c = Config::default();
        c.accounts.push(acc("a", "Work", Path::new("/a")));
        assert_eq!(c.validate_nickname("  Home ", None).unwrap(), "Home");
        assert!(c.validate_nickname("work", None).is_err());
        assert_eq!(c.validate_nickname("Work", Some("a")).unwrap(), "Work"); // renaming itself
        assert!(c.validate_nickname("", None).is_err());
        assert!(c.validate_nickname(&"x".repeat(33), None).is_err());
        assert!(c.validate_nickname("Default", None).is_err());
    }

    #[test]
    fn shared_link_merge_unlink_and_delete() {
        let home = tmp_home("shared");
        let p = Paths::from_home(&home);
        fs::create_dir_all(p.claude_home.join("projects/proj")).unwrap();
        fs::write(p.claude_home.join("projects/proj/old.jsonl"), "default").unwrap();
        fs::write(p.claude_home.join("settings.json"), "{}").unwrap();

        let (_, dir) = create_account_dir(&p).unwrap();
        fs::create_dir_all(dir.join("projects/proj")).unwrap();
        fs::write(dir.join("projects/proj/new.jsonl"), "acct").unwrap();
        fs::write(dir.join("settings.json"), "mine").unwrap();
        fs::write(dir.join(".claude.json"), "id").unwrap();

        link_shared(&p, &dir).unwrap();
        assert!(dir.join("projects").is_symlink());
        assert_eq!(
            fs::read_to_string(p.claude_home.join("projects/proj/new.jsonl")).unwrap(),
            "acct"
        );
        assert_eq!(
            fs::read_to_string(dir.join("settings.json.cctwin-bak")).unwrap(),
            "mine"
        );
        assert_eq!(fs::read_to_string(dir.join("settings.json")).unwrap(), "{}");
        assert!(!dir.join("skills").exists()); // missing in ~/.claude: not linked
        assert!(!dir.join(".claude.json").is_symlink());
        link_shared(&p, &dir).unwrap(); // idempotent

        unlink_shared(&dir).unwrap();
        assert!(!dir.join("projects").is_symlink());
        assert!(dir.join("projects/proj/old.jsonl").is_file());

        link_shared(&p, &dir).unwrap();
        delete_account_dir(&dir).unwrap();
        assert!(!dir.exists());
        assert!(p.claude_home.join("projects/proj/old.jsonl").is_file()); // target untouched
        assert!(p.claude_home.join("settings.json").is_file());
    }
}
