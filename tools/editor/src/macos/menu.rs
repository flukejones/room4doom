//! Appends "Preferences…" (⌘,) to the native macOS app submenu; action sets a thread-local flag, a Slint timer drains it — Slint must only be touched on the main thread.

use std::cell::{Cell, RefCell};
use std::time::Duration;

use objc2::rc::Retained;
use objc2::runtime::{AnyObject, NSObject};
use objc2::{MainThreadOnly, define_class, msg_send, sel};
use objc2_app_kit::{NSApplication, NSMenuItem};
use objc2_foundation::{MainThreadMarker, NSObjectProtocol, NSString};
use slint::{ComponentHandle as _, Timer, TimerMode};

use crate::generated::{EditorWindow, PrefsController};

const POLL_INTERVAL: Duration = Duration::from_millis(100);
const WATCH_INTERVAL: Duration = Duration::from_millis(500);

thread_local! {
    // Set by the menu action; drained by the poll timer. Both on main thread — `Cell` is enough.
    static PREFERENCES_REQUESTED: Cell<bool> = const { Cell::new(false) };
    // Target must outlive its menu item; timer must keep ticking — both app-lifetime.
    static MENU_STATE: RefCell<Option<(Retained<PreferencesTarget>, Timer)>> =
        const { RefCell::new(None) };
    // Watcher re-adds the Preferences item if Slint rebuilds its MenuBar and drops it.
    static WATCH_TIMER: RefCell<Option<Timer>> = const { RefCell::new(None) };
}

define_class!(
    #[unsafe(super(NSObject))]
    #[name = "Room4DoomPreferencesTarget"]
    #[thread_kind = MainThreadOnly]
    struct PreferencesTarget;

    impl PreferencesTarget {
        #[unsafe(method(openPreferences:))]
        fn open_preferences(&self, _sender: Option<&AnyObject>) {
            PREFERENCES_REQUESTED.with(|f| f.set(true));
        }
    }

    unsafe impl NSObjectProtocol for PreferencesTarget {}
);

impl PreferencesTarget {
    fn new(mtm: MainThreadMarker) -> Retained<Self> {
        unsafe { msg_send![mtm.alloc::<Self>(), init] }
    }
}

/// Appends the Preferences item; no-ops if already present. Returns false if menu not yet built.
pub(crate) fn append_preferences_item(ui: &EditorWindow) -> bool {
    let Some(mtm) = MainThreadMarker::new() else {
        return false;
    };
    let app = NSApplication::sharedApplication(mtm);
    let Some(main_menu) = app.mainMenu() else {
        return false;
    };
    let Some(app_item) = main_menu.itemAtIndex(0) else {
        return false;
    };
    let Some(app_submenu) = app_item.submenu() else {
        return false;
    };

    let title = NSString::from_str("Preferences…");
    if app_submenu.indexOfItemWithTitle(&title) >= 0 {
        return true;
    }

    let target = PreferencesTarget::new(mtm);
    let key = NSString::from_str(",");
    let item = unsafe {
        NSMenuItem::initWithTitle_action_keyEquivalent(
            mtm.alloc(),
            &title,
            Some(sel!(openPreferences:)),
            &key,
        )
    };
    unsafe {
        item.setTarget(Some(&target));
        app_submenu.insertItem_atIndex(&item, 1); // index 0 = app-name item
    }

    let weak = ui.as_weak();
    let timer = Timer::default();
    timer.start(TimerMode::Repeated, POLL_INTERVAL, move || {
        if PREFERENCES_REQUESTED.with(|f| f.replace(false))
            && let Some(ui) = weak.upgrade()
        {
            ui.global::<PrefsController>().set_prefs_visible(true);
        }
    });
    MENU_STATE.with(|s| *s.borrow_mut() = Some((target, timer)));
    true
}

/// Installs a watcher that re-adds the Preferences item whenever Slint rebuilds its MenuBar.
pub fn install(ui: &EditorWindow) {
    let weak = ui.as_weak();
    let timer = Timer::default();
    timer.start(TimerMode::Repeated, WATCH_INTERVAL, move || {
        if let Some(ui) = weak.upgrade() {
            append_preferences_item(&ui);
        }
    });
    WATCH_TIMER.with(|t| *t.borrow_mut() = Some(timer));
}
