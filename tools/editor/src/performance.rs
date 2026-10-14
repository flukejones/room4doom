//! Manual optimized UI workload for allocation and CPU profiling.

use super::*;
use crate::render::input::SelItem;
use crate::tabs::map_editor::MapCommand;
use gpui::{TestAppContext, VisualTestContext, px, size};
use std::time::Instant;

/// Run with `cargo test --release -p editor profile_repaints -- --ignored --nocapture`.
#[gpui::test]
#[ignore = "manual optimized profiling workload"]
fn profile_repaints(cx: &mut TestAppContext) {
    cx.update(|cx| ui_common::init("Editor profile", cx));
    let iwad = test_utils::doom1_wad_path();
    let wad = wad::WadData::new(&iwad);
    let map = editor_core::import_wad_map(&wad, "E1M1").unwrap();
    let line = map.lines.keys().next().unwrap();
    let thing = map.things.keys().next().unwrap();
    let mut editor = LevelEditorState::new();
    editor.load_map(map, "E1M1");
    editor.selection.push(SelItem::Line(line));
    editor.selection.push(SelItem::Thing(thing));
    let prefs = prefs::EditorPreferences {
        recent_projects: (0..10)
            .map(|i| format!("/projects/profile-{i}.dpr"))
            .collect(),
        ..Default::default()
    };
    let shared = Rc::new(RefCell::new(EditorSession::new(editor, Some(iwad), prefs)));
    let appearance = EditorAppearance::new(&shared);
    let root = cx.open_window(size(px(1280.), px(900.)), |window, cx| {
        EditorShell::new(appearance, shared, window, cx)
    });
    let mut visual = VisualTestContext::from_window(root.into(), cx);
    visual.run_until_parked();
    let started = Instant::now();
    for frame in 0_u16..3000 {
        root.update(&mut visual, |shell, _, cx| {
            shell.map.update(cx, |map, cx| {
                map.command(MapCommand::Hover(150. + f32::from(frame % 400), 200.), cx);
            });
        })
        .unwrap();
        visual.update(|window, cx| window.draw(cx).clear(cx));
    }
    println!(
        "EDITOR_PROFILE: 3000 E1M1 hover/redraw iterations: {:?}",
        started.elapsed()
    );
}
