#[cfg(not(target_os = "macos"))]
compile_error!("cctwin supports macOS only for now: https://github.com/arifulislamat/cctwin");


mod hook;
mod mac;
mod state;

use anyhow::{Result, bail};
use global_hotkey::{GlobalHotKeyEvent, GlobalHotKeyManager, HotKeyState, hotkey::HotKey};
use state::{Account, Config, Paths};
use std::io::IsTerminal;
use std::os::unix::process::CommandExt;
use std::path::Path;
use std::process::Command;
use std::time::Duration;
use tao::event::{Event, StartCause};
use tao::event_loop::{ControlFlow, EventLoopBuilder, EventLoopProxy};
use tao::platform::macos::{ActivationPolicy, EventLoopExtMacOS};
use tray_icon::menu::{CheckMenuItem, Menu, MenuEvent, MenuItem, PredefinedMenuItem, Submenu};
use tray_icon::{TrayIcon, TrayIconBuilder};

const USAGE: &str = "usage: cctwin                  start the menu bar app in the background
       cctwin --foreground     run the menu bar app attached to this terminal
       cctwin list             list accounts (* = active)
       cctwin use <name>       switch to an account (nickname, email, id or \"default\")
       cctwin init <shell>     print the shell hook (zsh, bash, fish)
       cctwin --version";

const NOTICE: &str = "cctwin points Claude Code at one of several config folders. It never reads or copies your login tokens.\n\nOnly add Claude accounts you personally own, and check Anthropic's current terms. Switching is always your action; cctwin never rotates accounts on its own.";

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
            println!("cctwin {}", env!("CARGO_PKG_VERSION"));
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
    Refresh,
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
                report(startup(&p, &hotkeys, &proxy));
            }
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
        mac::tell(format!("{e:#}"));
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

fn startup(p: &Paths, hotkeys: &GlobalHotKeyManager, proxy: &EventLoopProxy<Ev>) -> Result<()> {
    let cfg = Config::load(p)?;
    mac::setenv(state::active_dir(p).as_deref()); // launchctl env does not survive a reboot
    // An Add cut short by quitting leaves a folder that is not in the config: log it out and delete it.
    let orphans: Vec<_> = std::fs::read_dir(p.root.join("accounts"))
        .into_iter()
        .flatten()
        .flatten()
        .map(|e| e.path())
        .filter(|d| !cfg.accounts.iter().any(|a| &a.dir == d))
        .collect();
    if !orphans.is_empty() {
        bg(proxy, move |_| {
            for dir in orphans {
                if state::read_email(&dir.join(".claude.json")).is_some() {
                    logout(&dir);
                }
                state::delete_account_dir(&dir)?;
            }
            Ok(())
        });
    }
    if cfg.shared_history {
        for a in &cfg.accounts {
            state::link_shared(p, &a.dir)?; // picks up items created in ~/.claude since last run
        }
    }
    if cfg.launch_at_login {
        mac::set_launch_at_login(true, &exe()?, &p.root.join("cctwin.log"))?;
    }
    match cfg.hotkey_next.parse::<HotKey>() {
        Ok(k) => hotkeys.register(k)?,
        Err(e) => eprintln!("cctwin: bad hotkey_next {:?}: {e}", cfg.hotkey_next),
    }
    if !cfg.notice_shown {
        bg(proxy, |p| {
            mac::alert(NOTICE);
            if mac::confirm(
                "Install the shell hook in ~/.zshrc so `claude` in new terminals follows the active account?",
                "Install",
            ) {
                install_hook()?;
            }
            let mut cfg = Config::load(p)?;
            cfg.notice_shown = true;
            cfg.save(p)
        });
    }
    Ok(())
}

fn install_hook() -> Result<()> {
    let home = std::env::home_dir().unwrap();
    let changed = hook::install(&home, &exe()?)?;
    let names: Vec<_> = changed
        .iter()
        .map(|f| f.strip_prefix(&home).unwrap_or(f).display().to_string())
        .collect();
    mac::tell(if names.is_empty() {
        "Shell hook already installed.".into()
    } else {
        format!(
            "Shell hook added to ~/{}. Open a new terminal tab; tabs that are already open keep the old setup until you run `source ~/.zshrc`.",
            names.join(", ~/")
        )
    });
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
            let msg = format!(
                "{ide} is already running and keeps the account it started with. Quit it first for the switch to apply."
            );
            if mac::app_running(ide) && !mac::confirm(&msg, "Open anyway") {
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
        "hook" => install_hook()?,
        "quit" => std::process::exit(0),
        _ => {}
    }
    Ok(())
}

fn continue_next(p: &Paths) -> Result<()> {
    let cfg = Config::load(p)?;
    let Some(i) = cfg.next_index(cfg.active_index(p)) else {
        bail!("Add another account to continue on it.");
    };
    let a = &cfg.accounts[i];
    switch(p, Some(a))?;
    let body = format!("{}\nclaude --continue", mac::env_line(Some(&a.dir)));
    mac::open_terminal(&cfg.terminal, &p.root, &state::last_cwd(p), &body)
}

fn add(p: &Paths) -> Result<()> {
    let cfg = Config::load(p)?;
    let (id, dir) = state::create_account_dir(p)?;
    if cfg.shared_history {
        state::link_shared(p, &dir)?;
    }
    let body = format!(
        "echo 'cctwin: log in with the account you want to add.'\n{}\nclaude auth login",
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
        let Some(name) = mac::input(&format!("Nickname for {email}"), &suggested) else {
            break String::new();
        };
        match Config::load(p)?.validate_nickname(&name, None) {
            Ok(n) => break n,
            Err(e) => mac::alert(&e.to_string()),
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
        let Some(name) = mac::input(&format!("New nickname for {current}"), &acc.nickname) else {
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
            Err(e) => mac::alert(&e.to_string()),
        }
    }
}

fn remove(p: &Paths, acc: &Account) -> Result<()> {
    let name = name_of(p, Some(acc));
    let msg = format!("Remove {name}? It is logged out of Claude Code and its folder is deleted.");
    if !mac::confirm(&msg, "Remove") {
        return Ok(());
    }
    logout(&acc.dir);
    state::delete_account_dir(&acc.dir)?;
    if state::active_dir(p).as_deref() == Some(acc.dir.as_path()) {
        switch(p, None)?;
    }
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
