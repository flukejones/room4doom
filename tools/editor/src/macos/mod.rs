//! macOS native integration: app-menu Preferences item ([`menu`]) and vibrant titlebar ([`window`]); deferred to a timer — the native menu/window is not built until app launch completes.

pub mod menu;
pub mod window;

use std::cell::Cell;
use std::rc::Rc;
use std::time::Duration;

use slint::{Timer, TimerMode};

const RETRY_DELAY: Duration = Duration::from_millis(100);
/// Caps polling so a permanent failure cannot loop forever.
const MAX_ATTEMPTS: u32 = 100;

/// Polls `attempt` on a repeating timer until it returns `true` or [`MAX_ATTEMPTS`] is reached.
pub fn defer_with_retry(attempt: impl Fn() -> bool + 'static) {
    let timer = Rc::new(Timer::default());
    let attempts = Cell::new(0u32);
    let held = timer.clone();
    timer.start(TimerMode::Repeated, RETRY_DELAY, move || {
        let n = attempts.get() + 1;
        attempts.set(n);
        if attempt() || n >= MAX_ATTEMPTS {
            held.stop();
        }
    });
}
