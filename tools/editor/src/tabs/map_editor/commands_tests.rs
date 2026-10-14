use super::{MapCommand, MapPresentation, render_sync};
use crate::app::preferences::{BspAnimPref, EditorPreferences};
use crate::app::session::EditorSession;
use crate::boundary::{DrawShape, SelectMode, Tool};
use crate::level_editor::LevelEditorState;
use crate::level_editor::state::{Damage, Overlay, SectorFill, SelItem};
use crate::render::camera3d::Projection;

#[test]
fn angled_line_choice_commits_the_preview_after_pointer_moves() {
    for projection in [Projection::Ortho, Projection::Perspective] {
        for choice in [MapCommand::LineDragMove, MapCommand::LineDragExtrude] {
            let mut editor = LevelEditorState::new();
            editor.load_map(editor_core::EditorMap::default(), "NEW");
            editor.camera.set_zoom(2.0);
            editor.set_tool(Tool::Draw(DrawShape::Rect));
            for point in [[0.0, 0.0], [128.0, 128.0]] {
                editor.tool_click(editor.camera.world_to_screen(point), false);
            }
            assert_eq!(editor.map.as_ref().unwrap().sectors.len(), 1);
            editor.set_tool(Tool::Select(SelectMode::Line));
            editor.camera.set_pivot([64.0, 64.0, 0.0]);
            editor.orbit(45.0, 60.0);
            editor.camera.set_projection(projection);
            editor.rebuild_pick_mesh();
            let from = editor.camera.world_to_screen([64.0, 0.0]);
            let before_release = editor.camera.world_to_screen([64.0, -40.0]);
            let to = editor.camera.world_to_screen([64.0, -48.0]);
            let mut state = EditorSession::new(editor, None, Default::default());
            let mut ui = MapPresentation::default();
            ui.command(
                &mut state,
                MapCommand::ToolDragStart(from[0], from[1], false),
            );
            ui.command(
                &mut state,
                MapCommand::ToolDrag(before_release[0], before_release[1]),
            );
            ui.command(&mut state, MapCommand::ToolDragEnd(to[0], to[1]));
            assert!(ui.canvas.line_drag_pending);
            let Overlay::Move {
                points,
                ..
            } = &state.map_editor.overlay
            else {
                panic!("move preview")
            };
            let positions: &[[f32; 3]; 2] = points.as_slice().try_into().unwrap();
            let mut expected = positions.map(|p| [p[0], p[1]]);
            expected.sort_by(|a, b| a[0].total_cmp(&b[0]).then(a[1].total_cmp(&b[1])));
            assert_eq!(
                expected,
                [[0.0, -48.0], [128.0, -48.0]],
                "preview includes the release position"
            );
            let release_world = state.map_editor.cursor_world;
            ui.command(&mut state, MapCommand::Hover(to[0] + 16.0, to[1] + 40.0));
            assert_ne!(state.map_editor.cursor_world, release_world);
            assert_eq!(ui.command(&mut state, choice).damage, Damage::Edited);
            let [SelItem::Line(key)] = state.map_editor.selection.items() else {
                panic!("selected result")
            };
            let map = state.map_editor.map.as_ref().unwrap();
            let line = map.lines[*key];
            let mut actual = [line.v1, line.v2].map(|key| {
                let p = map.vertices[key];
                [p.x, p.y]
            });
            actual.sort_by(|a, b| a[0].total_cmp(&b[0]).then(a[1].total_cmp(&b[1])));
            assert_eq!(
                actual, expected,
                "choice must use the displayed preview, not the menu pointer"
            );
        }
    }
}

fn animated_map(mode: BspAnimPref) -> (EditorSession, MapPresentation) {
    let wad = wad::WadData::new(&test_utils::doom1_wad_path());
    let mut map = editor_core::import_wad_map(&wad, "E1M1").expect("E1M1 imports");
    for sector in map.sectors.values_mut() {
        sector.special = 0;
    }
    let sector = map.sectors.values_mut().next().expect("fixture sector");
    sector.special = 13; // Synchronized fast strobe changes on its first tic.
    sector.light_level = 255;
    let mut editor = LevelEditorState::new();
    editor.load_map(map, "E1M1");
    editor.sector_fill = SectorFill::Texture;
    let prefs = EditorPreferences {
        bsp_anim: mode,
        light_anim: true,
        ..Default::default()
    };
    let mut session = EditorSession::new(editor, None, prefs);
    let mut presentation = MapPresentation::default();
    render_sync::apply_damage(&mut presentation, &mut session, Damage::Edited);
    (session, presentation)
}

fn partition_events() -> Vec<rbsp::BuildEvent> {
    vec![rbsp::BuildEvent::PartitionChosen {
        p1: [0., 0.],
        p2: [128., 0.],
    }]
}

#[test]
fn skipped_bsp_playback_keeps_animated_lights_ticking() {
    for (mode, events) in [
        (BspAnimPref::Off, partition_events()),
        (BspAnimPref::Timed, Vec::new()),
    ] {
        let (mut session, mut presentation) = animated_map(mode);
        let next_tick = presentation
            .light_deadline
            .expect("animated lights scheduled");
        let initial = session.map_render.light_anim[0].current;
        render_sync::play_build(&mut presentation, &mut session, events, 100, true);
        render_sync::tick(&mut presentation, &mut session, next_tick);
        assert_ne!(
            session.map_render.light_anim[0].current, initial,
            "skipped BSP playback must leave the next light tic active"
        );
    }
}

#[test]
fn dismissing_bsp_overlay_repaints_and_resumes_animated_lights() {
    for command in [MapCommand::CancelOrClear, MapCommand::CommitOrClear] {
        let (mut session, mut presentation) = animated_map(BspAnimPref::Instant);
        render_sync::play_build(
            &mut presentation,
            &mut session,
            partition_events(),
            100,
            true,
        );
        assert!(session.bsp_anim.is_some(), "BSP overlay is displayed");
        let effects = presentation.command(&mut session, command);
        assert_eq!(
            effects.damage,
            Damage::Repaint,
            "dismissal must repaint the cached canvas image"
        );
        render_sync::apply_damage(&mut presentation, &mut session, effects.damage);
        assert!(session.bsp_anim.is_none(), "dismissal releases BSP replay");
        let next_tick = presentation
            .light_deadline
            .expect("dismissal resumes animated lights");
        let initial = session.map_render.light_anim[0].current;
        render_sync::tick(&mut presentation, &mut session, next_tick);
        assert_ne!(
            session.map_render.light_anim[0].current, initial,
            "the resumed deadline advances lighting"
        );
    }
}

#[test]
fn batch_line_edit_preserves_other_fields_and_independent_drafts() {
    let wad = wad::WadData::new(&test_utils::doom1_wad_path());
    let mut map = editor_core::import_wad_map(&wad, "E1M1").expect("E1M1 imports");
    let (first, second) = {
        let mut keys = map.lines.keys();
        (
            keys.next().expect("first line"),
            keys.next().expect("second line"),
        )
    };
    map.lines[first].tag = 11;
    map.lines[first].front.x_offset = -8;
    map.lines[second].tag = 29;
    map.lines[second].front.x_offset = 13;
    let before = [map.lines[first], map.lines[second]];

    let mut editor = LevelEditorState::new();
    editor.load_map(map, "E1M1");
    editor.selection.push(SelItem::Line(first));
    editor.selection.push(SelItem::Line(second));
    let mut session = EditorSession::new(editor, None, EditorPreferences::default());
    let mut active = MapPresentation::default();
    let mut other = MapPresentation::default();
    active.sync(&mut session);
    other.sync(&mut session);
    other.line.front.xoff = "91".into();
    other.line.touched.front_xoff = true;

    active.line.front.xoff = "37".into();
    active.line.touched.front_xoff = true;
    active.command(&mut session, MapCommand::LineApply);

    let map = session.map_editor.map.as_ref().expect("open map");
    for (key, original) in [first, second].into_iter().zip(before) {
        let mut expected = original;
        expected.front.x_offset = 37;
        assert_eq!(map.lines[key], expected, "only the touched offset changes");
    }
    assert_eq!(
        other.line.front.xoff, "91",
        "another view retains its draft"
    );
    assert!(other.line.touched.front_xoff);

    active.command(&mut session, MapCommand::Undo);
    let map = session.map_editor.map.as_ref().expect("open map");
    assert_eq!(
        [map.lines[first], map.lines[second]],
        before,
        "one undo restores the entire batch"
    );
}
