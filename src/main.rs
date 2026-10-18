#[cfg(not(target_os = "macos"))]
compile_error!("cctwin supports macOS only for now: https://github.com/arifulislamat/cctwin");

mod hook;
mod mac;
mod state;
mod welcome;

use anyhow::{Result, bail};
use global_hotkey::{GlobalHotKeyEvent, GlobalHotKeyManager, HotKeyState, hotkey::HotKey};
use objc2::MainThreadMarker;
use state::{Account, Config, Paths};
use std::io::IsTerminal;
use std::os::unix::process::CommandExt;
use std::path::Path;
use std::process::Command;
use std::sync::{Mutex, OnceLock};
use std::time::Duration;
use tao::event::{Event, StartCause};
use tao::event_loop::{ControlFlow, EventLoopBuilder, EventLoopProxy};
use tao::platform::macos::{ActivationPolicy, EventLoopExtMacOS};
use tray_icon::menu::{CheckMenuItem, Menu, MenuEvent, MenuItem, PredefinedMenuItem, Submenu};
use tray_icon::{TrayIcon, TrayIconBuilder};

const REPO: &str = "https://github.com/arifulislamat/cctwin";
const VERSION: &str = env!("CARGO_PKG_VERSION");

const USAGE: &str = "usage: cctwin                  start the menu bar app in the background
       cctwin --foreground     run the menu bar app attached to this terminal
       cctwin list             list accounts (* = active)
       cctwin use <name>       switch to an account (nickname, email, id or \"default\")
       cctwin init <shell>     print the shell hook (zsh, bash, fish)
       cctwin --version";

fn main() {
    // Finder on older macOS passes -psn_<id> when launching the .app
    let args: Vec<String> = std::env::args()
        .skip(1)
        .filter(|a| !a.starts_with("-psn_"))
        .collect();
    let args: Vec<&str> = args.iter().map(String::as_str).collect();
    let r = match args.as_slice() {
        // From a terminal, hand off to a background process so the shell is free; the .app and
        // LaunchAgent have no terminal and run the menu bar app directly.
        [] if std::io::stdin().is_terminal() => start_detached(),
        [] | ["--foreground"] => tray(),
        ["list"] => list(),
        ["use", name] => cli_use(name),
        ["init", shell] => hook::script(shell).map(|s| print!("{s}")),
        ["--version" | "-V"] => {
            println!("cctwin {VERSION}");
            Ok(())
        }
        _ => {
            eprintln!("{USAGE}");
            std::process::exit(2);
        }
    };
    if let Err(e) = r {
        eprintln!("cctwin: {e:#}");
        std::process::exit(1);
    }
}

fn list() -> Result<()> {
    let p = Paths::new();
    let cfg = Config::load(&p)?;
    let active = state::active_dir(&p);
    let mark = |on: bool| if on { "*" } else { " " };
    println!(
        "{} Default  {}",
        mark(active.is_none()),
        state::email(&p, None).unwrap_or_default()
    );
    for a in &cfg.accounts {
        let email = state::email(&p, Some(a));
        let name = state::label(a, email.as_deref());
        println!(
            "{} {name}  {}  {}",
            mark(active.as_ref() == Some(&a.dir)),
            email.unwrap_or_default(),
            a.dir.display()
        );
    }
    Ok(())
}

fn cli_use(name: &str) -> Result<()> {
    let p = Paths::new();
    let cfg = Config::load(&p)?;
    if name.eq_ignore_ascii_case("default") {
        return switch(&p, None);
    }
    let Some(a) = cfg.find(&p, name) else {
        bail!("no account named {name:?}, see `cctwin list`")
    };
    switch(&p, Some(a))
}

/// Make an account (None = Default) active for new `claude` sessions and GUI apps.
fn switch(p: &Paths, acc: Option<&Account>) -> Result<()> {
    let dir = acc.map(|a| a.dir.as_path());
    state::set_active(p, dir)?;
    mac::setenv(dir);
    Ok(())
}

fn name_of(p: &Paths, acc: Option<&Account>) -> String {
    acc.map_or("Default".into(), |a| {
        state::label(a, state::email(p, Some(a)).as_deref())
    })
}

enum Ev {
    Menu(MenuEvent),
    Hotkey(GlobalHotKeyEvent),
    Welcome(&'static str),
    Refresh,
}

/// Lets the welcome window's controls reach the event loop.
static PROXY: OnceLock<Mutex<EventLoopProxy<Ev>>> = OnceLock::new();

/// Called by the welcome window's controls; handled in the event loop like a menu click.
pub fn welcome_action(action: &'static str) {
    if let Some(px) = PROXY.get() {
        let _ = px.lock().unwrap().send_event(Ev::Welcome(action));
    }
}

/// Single-instance lock, held for the life of the menu bar app.
fn lock(p: &Paths) -> Result<std::fs::File> {
    state::mkdir_private(&p.root)?;
    let lock = std::fs::File::create(p.root.join("lock"))?;
    if lock.try_lock().is_err() {
        bail!("already running in the menu bar");
    }
    Ok(lock)
}

fn start_detached() -> Result<()> {
    let p = Paths::new();
    drop(lock(&p)?); // fail here, visibly, if it is already running
    let log = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(p.root.join("cctwin.log"))?;
    Command::new(exe()?)
        .arg("--foreground")
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::null())
        .stderr(log)
        .process_group(0) // survives closing the terminal and Ctrl+C
        .spawn()?;
    println!("cctwin is running in the menu bar.");
    Ok(())
}

fn tray() -> Result<()> {
    let p = Paths::new();
    let lock = lock(&p)?;

    let mut event_loop = EventLoopBuilder::<Ev>::with_user_event().build();
    event_loop.set_activation_policy(ActivationPolicy::Accessory);
    let proxy = event_loop.create_proxy();
    let _ = PROXY.set(Mutex::new(proxy.clone()));
    let px = proxy.clone();
    MenuEvent::set_event_handler(Some(move |e| drop(px.send_event(Ev::Menu(e)))));
    let px = proxy.clone();
    GlobalHotKeyEvent::set_event_handler(Some(move |e| drop(px.send_event(Ev::Hotkey(e)))));
    let hotkeys = GlobalHotKeyManager::new()?;
    let mut tray: Option<TrayIcon> = None;

    event_loop.run(move |event, _, flow| {
        *flow = ControlFlow::Wait;
        let _keep = (&lock, &hotkeys);
        match event {
            Event::NewEvents(StartCause::Init) => {
                tray = Some(
                    TrayIconBuilder::new()
                        .with_icon_templated(icon())
                        .build()
                        .expect("tray icon"),
                );
                report(startup(&p, &hotkeys));
            }
            Event::UserEvent(Ev::Welcome(a)) => report(on_welcome(&p, &proxy, a)),
            Event::UserEvent(Ev::Menu(e)) => report(on_menu(&p, &proxy, e.id.0.as_str())),
            Event::UserEvent(Ev::Hotkey(e)) if e.state == HotKeyState::Pressed => {
                report(continue_next(&p))
            }
            Event::UserEvent(Ev::Refresh) => {}
            _ => return,
        }
        if let Some(t) = &tray {
            report(refresh(&p, t));
        }
        welcome::refresh(&p);
    })
}

/// Menu bar template icon: a small two-legged pixel critter, 3px per cell, padded to 36px tall
/// (18pt at 2x) so pixels stay sharp. Template icons are tinted by macOS (white on a dark menu bar).
fn icon() -> tray_icon::Icon {
    const ART: [&str; 10] = [
        "..########..",
        "..########..",
        "..#.####.#..",
        "..#.####.#..",
        "############",
        "############",
        "..########..",
        "..########..",
        "....#..#....",
        "....#..#....",
    ];
    const CELL: usize = 3;
    const PAD: usize = 3;
    let (w, h) = (ART[0].len() * CELL, ART.len() * CELL + 2 * PAD);
    let mut rgba = vec![0u8; w * h * 4];
    for y in 0..h - 2 * PAD {
        for x in 0..w {
            if ART[y / CELL].as_bytes()[x / CELL] == b'#' {
                rgba[((y + PAD) * w + x) * 4 + 3] = 255;
            }
        }
    }
    tray_icon::Icon::from_rgba(rgba, w as u32, h as u32).expect("icon")
}

fn report(r: Result<()>) {
    if let Err(e) = r {
        eprintln!("cctwin: {e:#}");
        mac::tell("cctwin ran into a problem", &format!("{e:#}"));
    }
}

/// Run a dialog-driven action off the main thread so the menu stays responsive, then rebuild the menu.
fn bg(proxy: &EventLoopProxy<Ev>, f: impl FnOnce(&Paths) -> Result<()> + Send + 'static) {
    let proxy = proxy.clone();
    std::thread::spawn(move || {
        report(f(&Paths::new()));
        let _ = proxy.send_event(Ev::Refresh);
    });
}

fn exe() -> Result<std::path::PathBuf> {
    Ok(std::env::current_exe()?)
}

fn main_thread() -> MainThreadMarker {
    MainThreadMarker::new().expect("runs on the main thread")
}

fn startup(p: &Paths, hotkeys: &GlobalHotKeyManager) -> Result<()> {
    mac::set_app_icon(main_thread());
    let mut cfg = Config::load(p)?;
    mac::setenv(state::active_dir(p).as_deref()); // launchctl env does not survive a reboot
    if state::adopt_strays(p, &mut cfg)? {
        cfg.save(p)?;
    }
    if cfg.shared_history {
        for a in &cfg.accounts {
            state::link_shared(p, &a.dir)?; // picks up items created in ~/.claude since last run
        }
    }
    if cfg.launch_at_login {
        mac::set_launch_at_login(true, &exe()?, &p.root.join("cctwin.log"))?;
    }
    // A hotkey taken by another app must not stop the rest of startup.
    let key = &cfg.hotkey_next;
    match key.parse::<HotKey>() {
        Ok(k) => {
            if let Err(e) = hotkeys.register(k) {
                report(Err(anyhow::anyhow!(
                    "Hotkey {key} is unavailable ({e}). Change hotkey_next in ~/.cctwin/config.toml."
                )));
            }
        }
        Err(e) => report(Err(anyhow::anyhow!(
            "Bad hotkey_next {key:?} in ~/.cctwin/config.toml: {e}"
        ))),
    }
    if !cfg.notice_shown {
        welcome::show(main_thread(), p);
    }
    Ok(())
}

/// Controls in the welcome window.
fn on_welcome(p: &Paths, proxy: &EventLoopProxy<Ev>, action: &str) -> Result<()> {
    match action {
        "ack" | "unack" => {
            let mut cfg = Config::load(p)?;
            cfg.notice_shown = action == "ack";
            cfg.save(p)?;
        }
        "hook" => install_hook(true)?,
        "done" => welcome::close(),
        "github" => mac::reveal(REPO),
        other => on_menu(p, proxy, other)?, // "add", "login"
    }
    Ok(())
}

/// `quiet`: the welcome window shows a checkmark instead of a dialog.
fn install_hook(quiet: bool) -> Result<()> {
    let home = std::env::home_dir().unwrap();
    let changed = hook::install(&home, &exe()?)?;
    let names: Vec<_> = changed
        .iter()
        .map(|f| f.strip_prefix(&home).unwrap_or(f).display().to_string())
        .collect();
    if !quiet {
        let body = if names.is_empty() {
            "Your shell config already loads it.".to_string()
        } else {
            format!(
                "Added to ~/{}. Open a new terminal tab; tabs that are already open pick it up after `source ~/.zshrc`.",
                names.join(", ~/")
            )
        };
        mac::tell("Shell hook installed", &body);
    }
    Ok(())
}

fn refresh(p: &Paths, tray: &TrayIcon) -> Result<()> {
    let cfg = Config::load(p)?;
    let active = state::active_dir(p);
    let active_acc = cfg
        .accounts
        .iter()
        .find(|a| Some(&a.dir) == active.as_ref());
    tray.set_tooltip(Some(format!("cctwin: {}", name_of(p, active_acc))))?;

    let menu = Menu::new();
    let sep = PredefinedMenuItem::separator();
    let email = state::email(p, None).unwrap_or_default();
    menu.append(&CheckMenuItem::with_id(
        "use:default",
        format!("Default    {email}"),
        true,
        active.is_none(),
        None,
    ))?;
    let manage = Submenu::new("Manage accounts", !cfg.accounts.is_empty());
    for a in &cfg.accounts {
        let email = state::email(p, Some(a));
        let name = state::label(a, email.as_deref());
        let text = match &email {
            Some(e) if *e != name => format!("{name}    {e}"),
            _ => name.clone(),
        };
        menu.append(&CheckMenuItem::with_id(
            format!("use:{}", a.id),
            &text,
            true,
            active_acc == Some(a),
            None,
        ))?;
        let sub = Submenu::new(&name, true);
        sub.append_items(&[
            &MenuItem::with_id(format!("term:{}", a.id), "Open terminal", true, None),
            &MenuItem::with_id(
                format!("ide:{}", a.id),
                format!("Open {}", cfg.ide_app),
                true,
                None,
            ),
            &MenuItem::with_id(format!("rename:{}", a.id), "Rename…", true, None),
            &MenuItem::with_id(format!("remove:{}", a.id), "Remove…", true, None),
            &MenuItem::with_id(format!("show:{}", a.id), "Show folder", true, None),
        ])?;
        manage.append(&sub)?;
    }
    menu.append_items(&[
        &sep,
        &MenuItem::with_id(
            "next",
            "Continue on next account",
            !cfg.accounts.is_empty(),
            cfg.hotkey_next.parse().ok(),
        ),
        &MenuItem::with_id("term", "Open terminal (active)", true, None),
        &MenuItem::with_id("ide", format!("Open {} (active)", cfg.ide_app), true, None),
        &MenuItem::with_id("add", "Add account…", true, None),
        &manage,
        &sep,
        &CheckMenuItem::with_id(
            "shared",
            "Shared session history",
            true,
            cfg.shared_history,
            None,
        ),
        &CheckMenuItem::with_id("login", "Launch at login", true, cfg.launch_at_login, None),
        &MenuItem::with_id("hook", "Install shell hook", true, None),
        &MenuItem::with_id("welcome", "Welcome & setup…", true, None),
        &MenuItem::with_id("about", format!("About cctwin {VERSION}"), true, None),
        &MenuItem::with_id("update", "Check for updates…", true, None),
        &MenuItem::with_id("quit", "Quit cctwin", true, None),
    ])?;
    tray.set_menu(Some(Box::new(menu)));
    Ok(())
}

fn on_menu(p: &Paths, proxy: &EventLoopProxy<Ev>, id: &str) -> Result<()> {
    let (cmd, arg) = id.split_once(':').unwrap_or((id, ""));
    let mut cfg = Config::load(p)?;
    let acc = cfg.accounts.iter().find(|a| a.id == arg).cloned();
    if !arg.is_empty() && arg != "default" && acc.is_none() {
        bail!("account {arg} no longer exists");
    }
    // Explicit account from a submenu, else the active one.
    let target = acc
        .as_ref()
        .map(|a| a.dir.clone())
        .or_else(|| state::active_dir(p));
    match cmd {
        "use" => {
            switch(p, acc.as_ref())?;
        }
        "term" => {
            let mut body = mac::env_line(target.as_deref());
            if let Some(a) = &acc {
                body = format!("export CCTWIN_PIN={}\n{body}", mac::sh(&a.dir)); // hook honors the pin
            }
            let who = acc.clone().or_else(|| {
                cfg.accounts
                    .iter()
                    .find(|a| Some(&a.dir) == target.as_ref())
                    .cloned()
            });
            let note = match &acc {
                Some(_) => "stays on this account even if you switch in the menu",
                None => "follows the account picked in the menu",
            };
            let msg = format!(
                "cctwin: claude in this window uses {} ({note}).",
                name_of(p, who.as_ref())
            );
            body = format!("clear\necho {}\n{body}", mac::sh(msg));
            mac::open_terminal(&cfg.terminal, &p.root, &state::last_cwd(p), &body)?;
        }
        "ide" => bg(proxy, move |p| {
            let cfg = Config::load(p)?;
            let ide = &cfg.ide_app;
            let title = format!("{ide} is already running");
            let body =
                "It keeps the account it started with. Quit it first for the switch to apply.";
            if mac::app_running(ide) && !mac::confirm(&title, body, "Open anyway", false) {
                return Ok(());
            }
            mac::open_ide(ide, target.as_deref(), &state::last_cwd(p))
        }),
        "rename" => bg(proxy, move |p| rename(p, &acc.unwrap())),
        "remove" => bg(proxy, move |p| remove(p, &acc.unwrap())),
        "show" => mac::reveal(&acc.unwrap().dir),
        "next" => continue_next(p)?,
        "add" => bg(proxy, add),
        "shared" => {
            cfg.shared_history = !cfg.shared_history;
            for a in &cfg.accounts {
                if cfg.shared_history {
                    state::link_shared(p, &a.dir)?
                } else {
                    state::unlink_shared(&a.dir)?
                }
            }
            cfg.save(p)?;
        }
        "login" => {
            cfg.launch_at_login = !cfg.launch_at_login;
            mac::set_launch_at_login(cfg.launch_at_login, &exe()?, &p.root.join("cctwin.log"))?;
            cfg.save(p)?;
        }
        "hook" => install_hook(false)?,
        "welcome" => welcome::show(main_thread(), p),
        "about" => mac::about_panel(VERSION, REPO),
        "update" => bg(proxy, |_| check_for_updates()),
        "quit" => std::process::exit(0),
        _ => {}
    }
    Ok(())
}

/// Only runs when the user asks: cctwin makes no network calls on its own.
fn check_for_updates() -> Result<()> {
    // /releases/latest redirects to /releases/tag/v<version>; no API token or JSON needed.
    let out = Command::new("curl")
        .args([
            "-fsSLI",
            "-m",
            "15",
            "-o",
            "/dev/null",
            "-w",
            "%{url_effective}",
        ])
        .arg(format!("{REPO}/releases/latest"))
        .output()?;
    let url = String::from_utf8_lossy(&out.stdout).into_owned();
    let Some(latest) = url.rsplit_once("/tag/v").map(|(_, v)| v.trim().to_string()) else {
        bail!("Couldn't reach GitHub to check for updates. Check your connection and try again.");
    };
    if !newer(&latest, VERSION) {
        mac::alert(
            "You're up to date",
            &format!("cctwin {VERSION} is the latest version."),
        );
        return Ok(());
    }
    let title = format!("cctwin {latest} is available");
    let notes = format!("{REPO}/releases/tag/v{latest}");
    match update_command(&exe()?) {
        Some(cmd) => {
            let body = format!("You have {VERSION}. To update, run:\n\n{cmd}");
            match mac::dialog(
                &title,
                &body,
                &["Copy command", "What's new", "Later"],
                None,
                false,
            )
            .0
            {
                0 => mac::copy_to_clipboard(cmd),
                1 => mac::reveal(&notes),
                _ => {}
            }
        }
        None => {
            let body = format!(
                "You have {VERSION}. Download the new .dmg and drag cctwin to Applications again."
            );
            if mac::dialog(&title, &body, &["Download", "Later"], None, false).0 == 0 {
                mac::reveal(&notes);
            }
        }
    }
    Ok(())
}

/// Whether version `a` is newer than `b` (numeric x.y.z; pre-release suffixes are ignored).
fn newer(a: &str, b: &str) -> bool {
    let parse = |v: &str| -> Vec<u64> {
        v.split(['-', '+'])
            .next()
            .unwrap_or_default()
            .split('.')
            .map(|n| n.parse().unwrap_or(0))
            .collect()
    };
    parse(a) > parse(b)
}

/// How this copy was installed, judged by where it runs from. None means the .dmg app.
fn update_command(exe: &Path) -> Option<&'static str> {
    let path = std::fs::canonicalize(exe).unwrap_or_else(|_| exe.to_path_buf());
    let path = path.to_string_lossy();
    if path.contains(".app/Contents/MacOS/") {
        None
    } else if path.contains("/Cellar/") {
        Some("brew upgrade cctwin")
    } else if path.contains("/.cargo/bin/") {
        Some("cargo install cctwin")
    } else {
        Some(
            "curl -fsSL https://raw.githubusercontent.com/arifulislamat/cctwin/main/install.sh | sh",
        )
    }
}

fn continue_next(p: &Paths) -> Result<()> {
    let cfg = Config::load(p)?;
    let Some(next) = cfg.next_index(cfg.active_index(p)) else {
        bail!("Add another account to continue on it.");
    };
    let a = next.map(|i| &cfg.accounts[i]);
    switch(p, a)?;
    let body = format!(
        "{}\nclaude --continue",
        mac::env_line(a.map(|a| a.dir.as_path()))
    );
    mac::open_terminal(&cfg.terminal, &p.root, &state::last_cwd(p), &body)
}

fn add(p: &Paths) -> Result<()> {
    let cfg = Config::load(p)?;
    let (id, dir) = state::create_account_dir(p)?;
    if cfg.shared_history {
        state::link_shared(p, &dir)?;
    }
    // `claude auth login` doesn't echo the pasted code, so people think the paste failed.
    let body = format!(
        "echo 'cctwin: log in with the account you want to add.'\n\
         echo 'Signing in on another device? Open the link below there, then paste the code here and press Return. The code stays hidden while you paste.'\n\
         echo\n{}\nclaude auth login",
        mac::env_line(Some(&dir))
    );
    mac::open_terminal(&cfg.terminal, &p.root, &state::last_cwd(p), &body)?;

    let json = dir.join(".claude.json");
    let Some(email) = (0..600).find_map(|_| {
        std::thread::sleep(Duration::from_secs(1));
        state::read_email(&json)
    }) else {
        state::delete_account_dir(&dir)?;
        bail!("No login within 10 minutes, account not added.");
    };

    let suggested = email
        .split('@')
        .next()
        .unwrap_or_default()
        .chars()
        .take(32)
        .collect::<String>();
    let nickname = loop {
        let body = format!("Signed in as {email}. Pick a short name for the menu.");
        let Some(name) = mac::input("Name this account", &body, &suggested) else {
            break String::new();
        };
        match Config::load(p)?.validate_nickname(&name, None) {
            Ok(n) => break n,
            Err(e) => mac::alert("Choose another nickname", &e.to_string()),
        }
    };
    let mut cfg = Config::load(p)?;
    cfg.accounts.push(Account { id, nickname, dir });
    cfg.save(p)?;
    Ok(())
}

fn rename(p: &Paths, acc: &Account) -> Result<()> {
    let current = name_of(p, Some(acc));
    loop {
        let title = format!("Rename {current}");
        let Some(name) = mac::input(&title, "Up to 32 characters.", &acc.nickname) else {
            return Ok(());
        };
        let mut cfg = Config::load(p)?;
        match cfg.validate_nickname(&name, Some(&acc.id)) {
            Ok(n) => {
                cfg.accounts
                    .iter_mut()
                    .filter(|a| a.id == acc.id)
                    .for_each(|a| a.nickname = n.clone());
                return cfg.save(p);
            }
            Err(e) => mac::alert("Choose another nickname", &e.to_string()),
        }
    }
}

fn remove(p: &Paths, acc: &Account) -> Result<()> {
    let name = name_of(p, Some(acc));
    let title = format!("Remove {name}?");
    let body = "It is logged out of Claude Code and its folder is deleted. History shared through Shared session history stays in ~/.claude.";
    if !mac::confirm(&title, body, "Remove", true) {
        return Ok(());
    }
    // Switch away first, so a `claude` started meanwhile can't recreate the folder.
    if state::active_dir(p).as_deref() == Some(acc.dir.as_path()) {
        switch(p, None)?;
    }
    logout(&acc.dir);
    state::delete_account_dir(&acc.dir)?;
    let mut cfg = Config::load(p)?;
    cfg.accounts.retain(|a| a.id != acc.id);
    cfg.save(p)?;
    Ok(())
}

/// Claude Code's own logout for one folder, so its Keychain item goes too.
fn logout(dir: &Path) {
    let Some(bin) = mac::claude_bin() else {
        eprintln!(
            "cctwin: claude not found, skipped logout for {}",
            dir.display()
        );
        return;
    };
    match Command::new(bin)
        .env("CLAUDE_CONFIG_DIR", dir)
        .args(["auth", "logout"])
        .status()
    {
        Ok(s) if s.success() => {}
        r => eprintln!("cctwin: logout for {} failed: {r:?}", dir.display()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn version_compare() {
        assert!(newer("0.1.10", "0.1.9")); // numeric, not text
        assert!(newer("1.0.0", "0.9.9"));
        assert!(!newer("0.1.2", "0.1.2"));
        assert!(!newer("0.1.1", "0.1.2"));
        assert!(!newer("0.2.0-rc.1", "0.2.0"));
    }

    #[test]
    fn install_method() {
        let p = Path::new;
        assert_eq!(
            update_command(p("/Applications/cctwin.app/Contents/MacOS/cctwin")),
            None
        );
        assert_eq!(
            update_command(p("/opt/homebrew/Cellar/cctwin/0.1.2/bin/cctwin")),
            Some("brew upgrade cctwin")
        );
        assert_eq!(
            update_command(p("/Users/x/.cargo/bin/cctwin")),
            Some("cargo install cctwin")
        );
        assert!(
            update_command(p("/Users/x/.local/bin/cctwin"))
                .unwrap()
                .contains("install.sh")
        );
    }
}
