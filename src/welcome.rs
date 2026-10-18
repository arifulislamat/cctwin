// First-run window: terms, shell hook, first account, launch at login. Its controls send the same
// actions as the menu (see `crate::welcome_action`), and `refresh` redraws it from what is on disk.
use crate::state::{self, Config, Paths};
use objc2::rc::Retained;
use objc2::runtime::AnyObject;
use objc2::{MainThreadMarker, MainThreadOnly, define_class, msg_send, sel};
use objc2_app_kit::{
    NSBackingStoreType, NSButton, NSColor, NSControlStateValueOff, NSControlStateValueOn, NSFont,
    NSImageScaling, NSImageView, NSTextAlignment, NSTextField, NSView, NSWindow, NSWindowStyleMask,
};
use objc2_foundation::{NSObject, NSObjectProtocol, NSPoint, NSRect, NSSize, NSString};
use std::cell::RefCell;

const W: f64 = 480.;
const H: f64 = 620.;

define_class!(
    // SAFETY: NSObject has no subclassing requirements and Target has no Drop.
    #[unsafe(super = NSObject)]
    #[thread_kind = MainThreadOnly]
    struct Target;

    unsafe impl NSObjectProtocol for Target {}

    impl Target {
        #[unsafe(method(ack:))]
        fn ack(&self, sender: &NSButton) {
            crate::welcome_action(if sender.state() == NSControlStateValueOn { "ack" } else { "unack" });
        }
        #[unsafe(method(hook:))]
        fn hook(&self, _sender: &AnyObject) {
            crate::welcome_action("hook");
        }
        #[unsafe(method(add:))]
        fn add(&self, _sender: &AnyObject) {
            crate::welcome_action("add");
        }
        #[unsafe(method(login:))]
        fn login(&self, _sender: &AnyObject) {
            crate::welcome_action("login");
        }
        #[unsafe(method(github:))]
        fn github(&self, _sender: &AnyObject) {
            crate::welcome_action("github");
        }
        #[unsafe(method(done:))]
        fn done(&self, _sender: &AnyObject) {
            crate::welcome_action("done");
        }
    }
);

struct Welcome {
    window: Retained<NSWindow>,
    _target: Retained<Target>, // controls hold their target weakly
    ack: Retained<NSButton>,
    hook: Retained<NSButton>,
    hook_status: Retained<NSTextField>,
    add: Retained<NSButton>,
    add_status: Retained<NSTextField>,
    login: Retained<NSButton>,
    done: Retained<NSButton>,
}

thread_local! {
    static WINDOW: RefCell<Option<Welcome>> = const { RefCell::new(None) };
}

/// Frame from the top-left corner (AppKit counts y from the bottom).
fn place(parent: &NSView, view: &NSView, x: f64, top: f64, w: f64, h: f64) {
    view.setFrame(NSRect::new(NSPoint::new(x, H - top - h), NSSize::new(w, h)));
    parent.addSubview(view);
}

fn label(
    mtm: MainThreadMarker,
    text: &str,
    font: &NSFont,
    secondary: bool,
) -> Retained<NSTextField> {
    let l = NSTextField::wrappingLabelWithString(&NSString::from_str(text), mtm);
    l.setFont(Some(font));
    if secondary {
        l.setTextColor(Some(&NSColor::secondaryLabelColor()));
    }
    l
}

fn button(
    mtm: MainThreadMarker,
    title: &str,
    target: &Target,
    action: objc2::runtime::Sel,
) -> Retained<NSButton> {
    unsafe {
        NSButton::buttonWithTitle_target_action(
            &NSString::from_str(title),
            Some(target),
            Some(action),
            mtm,
        )
    }
}

fn checkbox(
    mtm: MainThreadMarker,
    title: &str,
    target: &Target,
    action: objc2::runtime::Sel,
) -> Retained<NSButton> {
    unsafe {
        NSButton::checkboxWithTitle_target_action(
            &NSString::from_str(title),
            Some(target),
            Some(action),
            mtm,
        )
    }
}

pub fn show(mtm: MainThreadMarker, p: &Paths) {
    crate::mac::activate(mtm);
    let open = WINDOW.with_borrow(|w| w.as_ref().map(|w| w.window.clone()));
    if let Some(window) = open {
        window.makeKeyAndOrderFront(None);
        refresh(p);
        return;
    }

    let target: Retained<Target> = unsafe { msg_send![Target::alloc(mtm), init] };
    let window = unsafe {
        NSWindow::initWithContentRect_styleMask_backing_defer(
            NSWindow::alloc(mtm),
            NSRect::new(NSPoint::new(0., 0.), NSSize::new(W, H)),
            NSWindowStyleMask::Titled | NSWindowStyleMask::Closable,
            NSBackingStoreType::Buffered,
            false,
        )
    };
    unsafe { window.setReleasedWhenClosed(false) }; // kept in WINDOW and reshown
    window.setTitle(&NSString::from_str("Welcome to cctwin"));
    let view = window.contentView().expect("content view");

    let app = objc2_app_kit::NSApplication::sharedApplication(mtm);
    if let Some(icon) = app.applicationIconImage() {
        let image = NSImageView::imageViewWithImage(&icon, mtm);
        image.setImageScaling(NSImageScaling::ScaleProportionallyUpOrDown);
        place(&view, &image, (W - 88.) / 2., 24., 88., 88.);
    }
    let title = label(
        mtm,
        "Welcome to cctwin",
        &NSFont::boldSystemFontOfSize(22.),
        false,
    );
    title.setAlignment(NSTextAlignment::Center);
    place(&view, &title, 20., 120., W - 40., 30.);
    let tagline = label(
        mtm,
        "Switch Claude Code between Claude accounts you own, in one click.",
        &NSFont::systemFontOfSize(13.),
        true,
    );
    tagline.setAlignment(NSTextAlignment::Center);
    place(&view, &tagline, 20., 152., W - 40., 20.);

    let (bold, small) = (
        NSFont::boldSystemFontOfSize(14.),
        NSFont::systemFontOfSize(12.),
    );
    let step = |n: usize, heading: &str, body: &str| {
        let top = 196. + n as f64 * 96.;
        place(
            &view,
            &label(mtm, heading, &bold, false),
            36.,
            top,
            290.,
            20.,
        );
        place(
            &view,
            &label(mtm, body, &small, true),
            36.,
            top + 24.,
            290.,
            48.,
        );
        top
    };
    let status = |top: f64| {
        let s = label(mtm, "", &small, false);
        s.setTextColor(Some(&NSColor::systemGreenColor()));
        s.setAlignment(NSTextAlignment::Center);
        place(&view, &s, 330., top + 34., 124., 36.);
        s
    };

    let top = step(
        0,
        "1. Use only accounts you own",
        "cctwin never reads your login tokens. Sharing a login with other people breaks Anthropic's terms.",
    );
    let ack = checkbox(mtm, "I understand", &target, sel!(ack:));
    place(&view, &ack, 340., top, 120., 24.);

    let top = step(
        1,
        "2. Install the shell hook",
        "Adds one line to your shell config so `claude` in every terminal follows the account you pick.",
    );
    let hook = button(mtm, "Install", &target, sel!(hook:));
    place(&view, &hook, 336., top - 4., 112., 32.);
    let hook_status = status(top);

    let top = step(
        2,
        "3. Add an account",
        "Opens Terminal with `claude auth login`. Your current login is already there as Default.",
    );
    let add = button(mtm, "Add account…", &target, sel!(add:));
    place(&view, &add, 336., top - 4., 112., 32.);
    let add_status = status(top);

    let login = checkbox(mtm, "Start cctwin when I log in", &target, sel!(login:));
    place(&view, &login, 34., 488., 300., 24.);
    let hint = label(
        mtm,
        "cctwin lives in the menu bar: click the critter to switch accounts. Reopen this window from its menu.",
        &small,
        true,
    );
    place(&view, &hint, 36., 528., 290., 48.);
    let done = button(mtm, "Done", &target, sel!(done:));
    done.setKeyEquivalent(&NSString::from_str("\r"));
    place(&view, &done, 336., 536., 112., 32.);
    let link = button(mtm, "Open source on GitHub ↗", &target, sel!(github:));
    link.setBordered(false);
    link.setFont(Some(&small));
    place(&view, &link, 30., 584., 200., 20.);

    window.center();
    window.makeKeyAndOrderFront(None);
    window.orderFrontRegardless();
    WINDOW.with_borrow_mut(|w| {
        *w = Some(Welcome {
            window,
            _target: target,
            ack,
            hook,
            hook_status,
            add,
            add_status,
            login,
            done,
        })
    });
    refresh(p);
}

/// Redraw the checklist from the config and shell files; a no-op when the window isn't open.
pub fn refresh(p: &Paths) {
    WINDOW.with_borrow(|w| {
        let Some(w) = w else { return };
        let cfg = Config::load(p).unwrap_or_default();
        let on = |b: bool| {
            if b {
                NSControlStateValueOn
            } else {
                NSControlStateValueOff
            }
        };
        w.ack.setState(on(cfg.notice_shown));
        w.done.setEnabled(cfg.notice_shown);
        w.login.setState(on(cfg.launch_at_login));

        let hooked = crate::hook::installed(p.root.parent().unwrap());
        w.hook.setTitle(&NSString::from_str(if hooked {
            "Reinstall"
        } else {
            "Install"
        }));
        w.hook_status.setStringValue(&NSString::from_str(if hooked {
            "✓ Installed"
        } else {
            ""
        }));

        let names: Vec<String> = cfg
            .accounts
            .iter()
            .map(|a| state::label(a, state::email(p, Some(a)).as_deref()))
            .collect();
        w.add.setTitle(&NSString::from_str(if names.is_empty() {
            "Add account…"
        } else {
            "Add another…"
        }));
        let added = if names.is_empty() {
            String::new()
        } else {
            format!("✓ {}", names.join(", "))
        };
        w.add_status.setStringValue(&NSString::from_str(&added));
    });
}

pub fn close() {
    WINDOW.with_borrow(|w| {
        if let Some(w) = w {
            w.window.close();
        }
    });
}
