//! macOS window vibrancy via `NSVisualEffectView` inserted behind the window; requires `renderer-femtovg-wgpu` with a transparent swapchain. winit asserts `contentView` IS its `WinitView` — replacing it panics on focus loss — so the effect view is inserted as the backmost subview of the frame view (content view's superview).

use i_slint_backend_winit::WinitWindowAccessor as _;
use objc2::ClassType as _;
use objc2::rc::Retained;
use objc2_app_kit::{
    NSAutoresizingMaskOptions, NSView, NSVisualEffectBlendingMode, NSVisualEffectMaterial,
    NSVisualEffectState, NSVisualEffectView, NSWindowOrderingMode,
};
use objc2_foundation::{MainThreadMarker, NSObjectProtocol as _};
use slint::ComponentHandle as _;
use winit::raw_window_handle::{HasWindowHandle as _, RawWindowHandle};

use crate::generated::EditorWindow;
use crate::macos::defer_with_retry;

/// Creates a `BehindWindow` effect view autoresizing to fill `host`.
fn effect_view(
    mtm: MainThreadMarker,
    host: &NSView,
    material: NSVisualEffectMaterial,
) -> Retained<NSVisualEffectView> {
    let effect = NSVisualEffectView::initWithFrame(mtm.alloc(), host.bounds());
    effect.setMaterial(material);
    effect.setBlendingMode(NSVisualEffectBlendingMode::BehindWindow);
    effect.setState(NSVisualEffectState::Active);
    effect.setAutoresizingMask(
        NSAutoresizingMaskOptions::ViewWidthSizable | NSAutoresizingMaskOptions::ViewHeightSizable,
    );
    effect
}

/// Inserts `effect` as the backmost subview of `host`.
fn add_backmost(host: &NSView, effect: &NSVisualEffectView) {
    host.addSubview_positioned_relativeTo(effect, NSWindowOrderingMode::Below, None);
}

/// Applies vibrancy. Returns false until the native window exists (retried by [`install`]).
fn apply_vibrancy(ui: &EditorWindow) -> bool {
    let Some(mtm) = MainThreadMarker::new() else {
        return false;
    };
    ui.window().with_winit_window(|win| {
        let Ok(handle) = win.window_handle() else {
            return;
        };
        let RawWindowHandle::AppKit(appkit) = handle.as_raw() else {
            return;
        };
        let content_view: Retained<NSView> =
            unsafe { Retained::retain(appkit.ns_view.as_ptr().cast()) }.expect("ns_view");
        let (Some(ns_window), Some(frame_view)) =
            (content_view.window(), unsafe { content_view.superview() })
        else {
            return;
        };

        let installed = frame_view
            .subviews()
            .iter()
            .any(|sv| sv.isKindOfClass(NSVisualEffectView::class()));
        if installed {
            return;
        }

        ns_window.setOpaque(false);
        ns_window.setTitlebarAppearsTransparent(true);

        let glass = effect_view(
            mtm,
            &frame_view,
            NSVisualEffectMaterial::UnderWindowBackground,
        );
        add_backmost(&frame_view, &glass);
    });
    ui.window().has_winit_window()
}

/// Installs window vibrancy once the native window is created.
pub fn install(ui: &EditorWindow) {
    let weak = ui.as_weak();
    defer_with_retry(move || weak.upgrade().is_some_and(|ui| apply_vibrancy(&ui)));
}
