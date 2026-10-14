use super::*;
use crate::level_editor::LevelEditorState;
use editor_core::ArenaKey as _;
use gpui::{
    Modifiers, Render, ScrollDelta, ScrollWheelEvent, TestAppContext, VisualTestContext, point,
    size,
};

struct Host(Entity<DialogsView>);

impl Render for Host {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        div().size_full()
    }
}

fn open(
    parent: gpui::WindowHandle<Host>,
    request: DialogRequest,
    cx: &mut TestAppContext,
) -> (Entity<DialogsView>, VisualTestContext) {
    let dialogs = parent.update(cx, |host, _, _| host.0.clone()).unwrap();
    parent
        .update(cx, |_, window, cx| {
            dialogs.update(cx, |this, cx| this.open(request, window, cx));
        })
        .unwrap();
    let native = dialogs.read_with(cx, |this, _| this.native_window.unwrap());
    let mut visual = VisualTestContext::from_window(native.into(), cx);
    visual.run_until_parked();
    visual.update(|window, cx| window.draw(cx).clear(cx));
    (dialogs, visual)
}

fn check_browser_fills_height(parent: gpui::WindowHandle<Host>, cx: &mut TestAppContext) {
    let (dialogs, mut visual) = open(
        parent,
        DialogRequest::AssetPicker {
            target: actions::TexBrowseTarget::Brush,
            current: None,
        },
        cx,
    );
    let scroll = dialogs.read_with(cx, |this, _| {
        this.asset_grids["browser"]
            .scroll
            .0
            .borrow()
            .base_handle
            .clone()
    });
    let before = scroll.bounds();
    let dialog = visual.debug_bounds("editor-dialog").unwrap();
    assert!(
        dialog.bottom() - before.bottom() < px(32.),
        "grid must reach the dialog bottom: {before:?} in {dialog:?}"
    );
    visual.simulate_resize(size(px(1000.), px(800.)));
    visual.update(|window, cx| window.draw(cx).clear(cx));
    assert!(
        scroll.bounds().size.height > before.size.height + px(100.),
        "grid must grow when the native dialog grows"
    );
}

#[gpui::test]
fn native_asset_dialogs_fill_height_and_keep_wall_selection_visible(cx: &mut TestAppContext) {
    cx.update(|cx| ui_common::init("Asset dialog test", cx));
    let iwad = test_utils::doom1_wad_path();
    let wad = wad::WadData::new(&iwad);
    let map = editor_core::import_wad_map(&wad, "E1M1").unwrap();
    let line = map
        .lines
        .iter()
        .find(|(_, line)| line.back.is_some())
        .unwrap()
        .0;
    let mut editor = LevelEditorState::new();
    editor.load_map(map.clone(), "E1M1");
    let shared = Rc::new(RefCell::new(EditorSession::new(
        editor,
        Some(iwad),
        Default::default(),
    )));
    let parent = cx.open_window(size(px(1200.), px(900.)), |_, cx| {
        Host(cx.new(|cx| DialogsView::new(shared.clone(), cx)))
    });
    check_browser_fills_height(parent, cx);

    let (dialogs, mut visual) = open(
        parent,
        DialogRequest::Wall(i32::try_from(line.slot()).unwrap()),
        cx,
    );
    let preview = visual.debug_bounds("wall-preview").unwrap();
    let (band, scale, image) = dialogs.read_with(cx, |this, _| {
        let wall = &this.drafts.wall;
        let band = wall
            .bands
            .iter()
            .find(|band| band.slot != wall.selected_slot)
            .unwrap();
        (
            (band.slot, band.y, band.h),
            f32::from(preview.size.height) / wall.wall_h,
            wall.wall_img.clone(),
        )
    });
    visual.simulate_click(
        point(
            preview.center().x,
            preview.top() + px((band.1 + band.2 / 2.) * scale),
        ),
        Modifiers::none(),
    );
    visual.update(|window, cx| window.draw(cx).clear(cx));
    dialogs.read_with(cx, |this, _| {
        assert_eq!(
            this.drafts.wall.selected_slot, band.0,
            "click must select the visible wall band"
        );
        assert_eq!(
            this.drafts.wall.wall_img, image,
            "band selection must retain the wall image"
        );
    });
    let scroll = dialogs.read_with(cx, |this, _| {
        this.asset_grids["wall"]
            .scroll
            .0
            .borrow()
            .base_handle
            .clone()
    });
    visual.simulate_event(ScrollWheelEvent {
        position: scroll.bounds().center(),
        delta: ScrollDelta::Pixels(point(px(0.), px(-500.))),
        ..Default::default()
    });
    visual.update(|window, cx| window.draw(cx).clear(cx));
    assert!(scroll.offset().y < px(0.));
    assert_eq!(
        visual.debug_bounds("wall-preview").unwrap(),
        preview,
        "scrolling textures must keep the wall preview in place"
    );
    assert_eq!(
        shared.borrow().map_editor.map.as_ref().unwrap(),
        &map,
        "selection and scrolling must not edit the map"
    );
}
