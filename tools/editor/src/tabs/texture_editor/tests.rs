use super::*;
use crate::level_editor::LevelEditorState;
use gpui::{
    AppContext as _, Focusable as _, Render, TestAppContext, div, img, prelude::*, px, size,
};

// Paint the tab-owned image without relying on the tab's render pass for cleanup.
struct PreviewHost {
    tab: Entity<TextureEditor>,
    visible: bool,
}

impl Render for PreviewHost {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        div().size_full().children(
            self.visible
                .then(|| self.tab.read(cx).preview.clone())
                .flatten()
                .map(|image| img(image).w(px(128.)).h(px(128.))),
        )
    }
}

fn wad_session() -> Rc<RefCell<EditorSession>> {
    Rc::new(RefCell::new(EditorSession::new(
        LevelEditorState::new(),
        Some(test_utils::doom1_wad_path()),
        Default::default(),
    )))
}

#[gpui::test]
fn closing_resources_evicts_current_and_pending_previews_while_hidden(cx: &mut TestAppContext) {
    let shared = wad_session();
    let window = cx.open_window(size(px(256.), px(256.)), |window, cx| {
        let tab = cx.new(|cx| TextureEditor::new(shared.clone(), cx));
        tab.update(cx, |tab, cx| tab.refresh(window, cx));
        PreviewHost {
            tab,
            visible: true,
        }
    });
    let previous = window
        .update(cx, |host, window, cx| {
            let previous = host.tab.read(cx).preview.clone().unwrap();
            assert!(window.has_image_atlas_entry(&previous));
            host.tab.update(cx, |tab, cx| tab.refresh_preview(cx));
            cx.notify();
            previous
        })
        .unwrap();
    cx.update_window(window.into(), |_, window, cx| window.draw(cx).clear(cx))
        .unwrap();
    window
        .update(cx, |host, window, cx| {
            let current = host.tab.read(cx).preview.clone().unwrap();
            assert!(window.has_image_atlas_entry(&current));
            host.visible = false;
            {
                let mut state = shared.borrow_mut();
                state.iwad = None;
                state.invalidate_wad_caches();
            }
            host.tab.update(cx, |tab, cx| tab.reset(window, cx));
            assert!(
                !window.has_image_atlas_entry(&previous) && !window.has_image_atlas_entry(&current),
                "closing resources must evict preview tiles even when the tab cannot render"
            );
        })
        .unwrap();
}

#[gpui::test]
fn entering_a_patch_name_adds_it_once_before_the_field_blurs(cx: &mut TestAppContext) {
    cx.update(|cx| {
        ui_common::init("Texture editor test", cx);
        bind_keys(cx);
    });
    let shared = wad_session();
    let window = cx.open_window(size(px(900.), px(900.)), |window, cx| {
        let mut tab = TextureEditor::new(shared.clone(), cx);
        tab.refresh(window, cx);
        tab
    });
    let (index, name, before) = window
        .update(cx, |tab, window, cx| {
            let state = shared.borrow();
            let index = tab.current.unwrap();
            let def = &state.assets.as_ref().unwrap().textures()[index];
            let input = &tab.inputs[&Input::NewPatch];
            window.focus(&input.read(cx).focus_handle(cx), cx);
            (index, def.patches[0].patch, def.patches.len())
        })
        .unwrap();
    cx.simulate_input(window.into(), name.as_str());
    cx.simulate_keystrokes(window.into(), "enter");
    window
        .update(cx, |tab, window, cx| tab.focus(window, cx))
        .unwrap();
    let (x, y) = {
        let state = shared.borrow();
        let def = &state.assets.as_ref().unwrap().textures()[index];
        assert_eq!(
            def.patches.len(),
            before + 1,
            "Enter must add one patch; the ensuing blur must not add another"
        );
        assert_eq!(def.patches.last().unwrap().patch, name);
        (def.patches[0].origin_x, def.patches[0].origin_y)
    };
    window
        .update(cx, |tab, window, cx| {
            tab.selected_patch = Some(0);
            tab.focus(window, cx);
        })
        .unwrap();
    cx.simulate_keystrokes(window.into(), "left shift-up");
    {
        let state = shared.borrow();
        let patch = &state.assets.as_ref().unwrap().textures()[index].patches[0];
        assert_eq!((patch.origin_x, patch.origin_y), (x - 1, y - 10));
    }
    window
        .update(cx, |tab, window, cx| {
            window.focus(&tab.inputs[&Input::Width].focus_handle(cx), cx);
        })
        .unwrap();
    cx.simulate_keystrokes(window.into(), "up");
    let state = shared.borrow();
    let patch = &state.assets.as_ref().unwrap().textures()[index].patches[0];
    assert_eq!(
        patch.origin_y,
        y - 10,
        "arrows in a field must not move its patch"
    );
}
