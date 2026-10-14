use crate::app::preferences::{EditorPreferences, WindowGeom};
use crate::app::session::EditorSession;
use crate::components::types::PopupId;
use crate::dialogs::{DialogRequest, DialogsView};
use crate::level_editor::LevelEditorState;
use gpui::{
    AppContext as _, Context, Entity, Render, ScrollDelta, ScrollWheelEvent, TestAppContext,
    VisualTestContext, Window, div, point, prelude::*, px, size,
};
use std::{cell::RefCell, rc::Rc};

struct DialogHost {
    dialogs: Entity<DialogsView>,
}

impl Render for DialogHost {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        div().size_full()
    }
}

#[gpui::test]
fn native_map_chooser_resizes_and_scrolls_a_long_list(cx: &mut TestAppContext) {
    cx.update(|cx| ui_common::init("Map chooser test", cx));
    let mut prefs = EditorPreferences::default();
    prefs.popup_windows.map_list = WindowGeom {
        off_x: 125.,
        off_y: -60.,
        w: 320.,
        h: 290.,
    };
    let mut state = EditorSession::new(LevelEditorState::new(), None, prefs);
    state.wad_maps = (1..=9).map(|index| format!("E1M{index}")).collect();
    let shared = Rc::new(RefCell::new(state));
    let window = cx.open_window(size(px(1000.), px(800.)), |window, cx| {
        let dialogs = cx.new(|cx| DialogsView::new(shared, cx));
        dialogs.update(cx, |dialogs, cx| {
            dialogs.open(DialogRequest::ChooseMap, window, cx);
        });
        DialogHost {
            dialogs,
        }
    });
    let native = window
        .update(cx, |host, _, cx| {
            host.dialogs.read(cx).native_window().unwrap()
        })
        .unwrap();
    assert_ne!(
        native,
        window.into(),
        "the chooser must have its own native window"
    );
    native
        .update(cx, |_, window, _| assert!(window.is_resizable()))
        .unwrap();
    let mut visual = VisualTestContext::from_window(native, cx);
    visual.run_until_parked();
    visual.update(|window, cx| window.draw(cx).clear(cx));

    let dialog = visual.debug_bounds("editor-dialog").unwrap();
    let scroll = window
        .update(cx, |host, _, cx| {
            host.dialogs.read(cx).popup_scroll[&(PopupId::MapList as usize)].clone()
        })
        .unwrap();
    let list = scroll.bounds();
    assert!(
        list.top() >= dialog.top()
            && list.bottom() <= dialog.bottom()
            && list.left() >= dialog.left()
            && list.right() <= dialog.right(),
        "the scrolling viewport must remain inside the dialog: {list:?} in {dialog:?}"
    );
    let choices_before = visual.debug_bounds("map-choices").unwrap();
    visual.simulate_event(ScrollWheelEvent {
        position: list.center(),
        delta: ScrollDelta::Pixels(point(px(0.), px(-300.))),
        ..Default::default()
    });
    assert!(
        scroll.offset().y < px(0.),
        "scrolling inside the chooser must reveal later map rows"
    );
    visual.update(|window, cx| window.draw(cx).clear(cx));
    let choices_after = visual.debug_bounds("map-choices").unwrap();
    assert!(
        choices_after.top() < choices_before.top() && choices_after.bottom() <= list.bottom(),
        "scrolling must reveal the bottom map rows: {choices_before:?} -> {choices_after:?} in {list:?}"
    );
    cx.simulate_window_resize(native, size(px(600.), px(220.)));
    visual.update(|window, cx| window.draw(cx).clear(cx));
    let resized = visual.debug_bounds("editor-dialog").unwrap();
    let choices_resized = visual.debug_bounds("map-choices").unwrap();
    assert!(
        resized.size.width > dialog.size.width
            && choices_resized.size.width > choices_after.size.width
    );
    assert!(
        scroll.bounds().bottom() <= resized.bottom(),
        "native resizing must keep the scrolling viewport inside the dialog"
    );
}
