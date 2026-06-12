use std::collections::HashMap;

use super::*;
use editor_core::geom::sector_at;
use editor_core::{
    Axis, DenseLineDef, DenseMap, DenseSideDef, LineDef, LineKey, Name8, SectorKey, ThingKey,
    Vertex, derive_sectors, import_wad_map, validate,
};

use crate::level_editor::draw::default_sector;
use crate::state::{Damage, SelItem};

fn line_key(map: &EditorMap, i: usize) -> LineKey {
    map.lines.keys().nth(i).expect("line exists")
}

fn vert_key(map: &EditorMap, i: usize) -> VertKey {
    map.vertices.keys().nth(i).expect("vertex exists")
}

fn thing_key(map: &EditorMap, i: usize) -> ThingKey {
    map.things.keys().nth(i).expect("thing exists")
}

fn e1m1() -> EditorMap {
    let wad = wad::WadData::new(&test_utils::doom1_wad_path());
    import_wad_map(&wad, "E1M1").expect("E1M1 imports")
}

fn app_with_map() -> LevelEditorState {
    let mut app = LevelEditorState::new();
    app.load_map(e1m1(), "E1M1");
    app.zoom_fit(); // picking culls off-screen geometry; fit ensures full coverage
    app
}

fn empty_app() -> LevelEditorState {
    let mut app = LevelEditorState::new();
    app.load_map(EditorMap::default(), "NEW");
    app
}

fn app_from_ron(ron: &str, name: &str) -> LevelEditorState {
    let map = editor_core::parse_map_ron(ron).expect("fixture parses");
    let mut app = LevelEditorState::new();
    app.load_map(map, name);
    app.zoom_fit();
    app
}

fn app_with_e1m2() -> LevelEditorState {
    let wad = wad::WadData::new(&test_utils::doom1_wad_path());
    let imported = import_wad_map(&wad, "E1M2").expect("E1M2 imports");
    let mut app = LevelEditorState::new();
    app.load_map(imported, "E1M2");
    app.zoom_fit();
    app
}

#[test]
fn load_map_resets_view_and_tool() {
    let mut app = app_with_map();
    app.orbit(40.0, 30.0);
    app.set_tool(Tool::Draw(DrawShape::Line));
    app.load_map(e1m1(), "E1M1");
    assert_eq!(
        app.tool,
        Tool::Select(SelectMode::All),
        "tool reset to Select"
    );
}

fn click_at(app: &mut LevelEditorState, world: [f32; 2]) -> Damage {
    app.rebuild_pick_mesh();
    let screen = app.camera.world_to_screen(world);
    app.tool_click(screen, false)
}

fn shift_click_at(app: &mut LevelEditorState, world: [f32; 2]) -> Damage {
    app.rebuild_pick_mesh();
    let screen = app.camera.world_to_screen(world);
    app.tool_click(screen, true)
}

fn draw_line(app: &mut LevelEditorState, a: [f32; 2], b: [f32; 2]) {
    app.set_tool(Tool::Draw(DrawShape::Line));
    click_at(app, a);
    shift_click_at(app, b);
}

fn draw_shape(app: &mut LevelEditorState, shape: DrawShape, anchor: [f32; 2], pointer: [f32; 2]) {
    app.set_tool(Tool::Draw(shape));
    click_at(app, anchor);
    click_at(app, pointer);
}

fn drag(app: &mut LevelEditorState, from: [f32; 2], to: [f32; 2]) -> Damage {
    app.rebuild_pick_mesh();
    let from_s = app.camera.world_to_screen(from);
    let to_s = app.camera.world_to_screen(to);
    app.begin_tool_drag(from_s, false);
    app.drag_to(to_s);
    app.end_drag(to_s)
}

#[test]
fn line_tool_draws_with_snap_and_vertex_reuse() {
    let mut app = empty_app();
    draw_line(&mut app, [3.0, 2.0], [61.0, 1.0]);

    let map = app.map.as_ref().expect("map");
    assert_eq!(map.lines.len(), 1);
    let first = map.lines.values().next().expect("one line");
    let p1 = map.vertices[first.v1];
    let p2 = map.vertices[first.v2];
    assert_eq!((p1.x, p1.y), (0.0, 0.0), "snapped to grid 8");
    assert_eq!((p2.x, p2.y), (64.0, 0.0));
    assert!(map.sectors.is_empty(), "a lone line creates no sector");
    assert_eq!(first.front.sector, None, "front faces the void");
    assert!(app.dirty);

    draw_line(&mut app, [63.0, 1.0], [64.0, 66.0]);
    let map = app.map.as_ref().expect("map");
    assert_eq!(map.lines.len(), 2);
    let lines: Vec<&LineDef> = map.lines.values().collect();
    assert_eq!(lines[1].v1, lines[0].v2);
    assert_eq!(map.vertices.len(), 3);
}

#[test]
fn degenerate_line_makes_nothing() {
    let mut app = empty_app();
    draw_line(&mut app, [3.0, 2.0], [1.0, 1.0]); // both snap to the same point
    assert_eq!(app.map.as_ref().expect("map").lines.len(), 0);
    assert!(app.poly.is_none());
}

#[test]
fn poly_chain_closes_at_start() {
    let mut app = empty_app();
    app.set_tool(Tool::Draw(DrawShape::Line));
    app.camera.set_zoom(1.0);
    click_at(&mut app, [0.0, 0.0]);
    click_at(&mut app, [128.0, 0.0]);
    click_at(&mut app, [128.0, 128.0]);
    click_at(&mut app, [1.0, 1.0]); // within close radius of start → closes chain
    let map = app.map.as_ref().expect("map");
    assert_eq!(map.lines.len(), 3);
    let mut degree: HashMap<VertKey, u32> = HashMap::new();
    for l in map.lines.values() {
        *degree.entry(l.v1).or_default() += 1;
        *degree.entry(l.v2).or_default() += 1;
    }
    assert!(
        degree.len() == 3 && degree.values().all(|&d| d == 2),
        "chain closed into a triangle"
    );
    assert!(app.poly.is_none());

    app.undo();
    assert_eq!(app.map.as_ref().expect("map").lines.len(), 0);
}

#[test]
fn chain_defers_all_geometry_until_finish() {
    let mut app = empty_app();
    app.set_tool(Tool::Draw(DrawShape::Line));
    app.camera.set_zoom(1.0);

    click_at(&mut app, [0.0, 0.0]);
    assert!(
        matches!(app.overlay, Overlay::Chain { .. }),
        "overlay shows chain"
    );
    assert!(
        app.map.as_ref().expect("map").lines.is_empty(),
        "no edge yet"
    );
    click_at(&mut app, [128.0, 0.0]);
    click_at(&mut app, [128.0, 128.0]);
    assert!(
        app.map.as_ref().expect("map").lines.is_empty(),
        "geometry deferred through the whole chain"
    );
    assert_eq!(app.poly.as_ref().expect("chain").points.len(), 3);

    shift_click_at(&mut app, [0.0, 128.0]);
    assert!(app.poly.is_none());
    assert_eq!(app.overlay, Overlay::None);
    assert_eq!(
        app.map.as_ref().expect("map").lines.len(),
        3,
        "all edges committed"
    );
    app.undo();
    assert!(
        app.map.as_ref().expect("map").lines.is_empty(),
        "one undo unwinds the whole chain"
    );
}

fn vertical_divider_at(map: &EditorMap, x: f32) -> LineKey {
    map.lines
        .iter()
        .find(|(_, l)| map.vertices[l.v1].x == x && map.vertices[l.v2].x == x)
        .map(|(k, _)| k)
        .expect("divider exists")
}

fn draw_room(app: &mut LevelEditorState, pts: &[[f32; 2]]) {
    app.set_tool(Tool::Draw(DrawShape::Line));
    for &p in pts {
        click_at(app, p);
    }
    click_at(app, [pts[0][0] + 1.0, pts[0][1] + 1.0]);
}

#[test]
fn draw_room_makes_one_sector_walls_single_sided() {
    let mut app = empty_app();
    app.camera.set_zoom(1.0);
    draw_room(
        &mut app,
        &[[0.0, 0.0], [128.0, 0.0], [128.0, 128.0], [0.0, 128.0]],
    );
    let map = app.map.as_ref().expect("map");
    assert_eq!(map.sectors.len(), 1);
    let room = map.sectors.keys().next().expect("one sector");
    for l in map.lines.values() {
        assert!(l.back.is_none(), "room wall single-sided");
        assert_eq!(l.front.sector, Some(room), "front faces the room");
    }
    assert_eq!(sector_at(map, [64.0, 64.0]), Some(room));
}

#[test]
fn drawn_room_takes_the_draw_brush_settings() {
    let mut app = empty_app();
    app.camera.set_zoom(1.0);
    app.draw_brush = DrawBrush {
        floor_h: 32,
        ceil_h: 200,
        floor_flat: Name8::new("FLAT5_5").expect("flat name"),
        ceil_flat: Name8::new("FLOOR7_1").expect("flat name"),
        wall_tex: Name8::new("STARTAN3").expect("texture name"),
    };
    draw_room(
        &mut app,
        &[[0.0, 0.0], [128.0, 0.0], [128.0, 128.0], [0.0, 128.0]],
    );
    let map = app.map.as_ref().expect("map");
    let sector = map.sectors.values().next().expect("one sector");
    assert_eq!(sector.floor_height, 32);
    assert_eq!(sector.ceil_height, 200);
    assert_eq!(sector.floor_flat, Name8::new("FLAT5_5").expect("flat"));
    assert_eq!(sector.ceil_flat, Name8::new("FLOOR7_1").expect("flat"));
    for l in map.lines.values() {
        assert_eq!(
            l.front.middle_tex,
            Name8::new("STARTAN3").expect("texture"),
            "drawn wall uses the brush middle texture"
        );
    }
}

/// Escape commits already-drawn segments (sector derivation still runs).
#[test]
fn cancelled_chain_still_sectors_committed_lines() {
    let mut app = empty_app();
    app.camera.set_zoom(1.0);
    draw_room(
        &mut app,
        &[[0.0, 0.0], [0.0, 128.0], [64.0, 128.0], [64.0, 0.0]],
    );
    draw_room(
        &mut app,
        &[[128.0, 0.0], [128.0, 128.0], [192.0, 128.0], [192.0, 0.0]],
    );
    assert_eq!(app.map.as_ref().expect("map").sectors.len(), 2);

    app.set_tool(Tool::Draw(DrawShape::Line));
    click_at(&mut app, [32.0, 32.0]);
    click_at(&mut app, [160.0, 32.0]);
    app.cancel_gesture();
    click_at(&mut app, [32.0, 96.0]);
    click_at(&mut app, [160.0, 96.0]);
    app.cancel_gesture();

    let map = app.map.as_ref().expect("map");
    assert_eq!(map.sectors.len(), 3, "two boxes + the corridor sector");
    let corridor = sector_at(map, [96.0, 64.0]).expect("corridor gap is a sector");
    let left = sector_at(map, [32.0, 64.0]).expect("left box");
    let right = sector_at(map, [160.0, 64.0]).expect("right box");
    assert!(
        corridor != left && corridor != right,
        "corridor is its own sector"
    );
}

#[test]
fn cancelled_chain_with_no_committed_line_is_noop() {
    let mut app = empty_app();
    app.set_tool(Tool::Draw(DrawShape::Line));
    click_at(&mut app, [10.0, 10.0]);
    app.cancel_gesture();
    let map = app.map.as_ref().expect("map");
    assert!(map.lines.is_empty(), "no line committed");
    assert!(map.sectors.is_empty());
    assert!(app.poly.is_none());
}

#[test]
fn line_across_room_splits_into_two() {
    let mut app = empty_app();
    app.camera.set_zoom(1.0);
    draw_room(
        &mut app,
        &[[0.0, 0.0], [128.0, 0.0], [128.0, 128.0], [0.0, 128.0]],
    );
    assert_eq!(app.map.as_ref().expect("map").sectors.len(), 1);
    draw_line(&mut app, [64.0, 0.0], [64.0, 128.0]);
    let map = app.map.as_ref().expect("map");
    assert_eq!(map.sectors.len(), 2, "room sliced into two");
    let left = sector_at(map, [32.0, 64.0]);
    let right = sector_at(map, [96.0, 64.0]);
    assert!(left.is_some() && right.is_some());
    assert_ne!(left, right, "different sector each side");
}

/// Regression: Enter-commit must report an edit for the reconciler.
#[test]
fn enter_finishing_a_subdividing_chain_reports_edited() {
    let mut app = empty_app();
    app.camera.set_zoom(1.0);
    draw_room(
        &mut app,
        &[[0.0, 0.0], [128.0, 0.0], [128.0, 128.0], [0.0, 128.0]],
    );
    app.set_tool(Tool::Draw(DrawShape::Line));
    click_at(&mut app, [64.0, 0.0]);
    click_at(&mut app, [64.0, 128.0]);
    assert_eq!(
        app.map.as_ref().expect("map").sectors.len(),
        1,
        "chain still open"
    );
    let damage = app.cancel_gesture();
    assert_eq!(damage, Damage::Edited, "commit reaches the reconciler");
    assert_eq!(
        app.map.as_ref().expect("map").sectors.len(),
        2,
        "the divider subdivided the room"
    );
}

/// Split two opposing inner-sector edges, connect; both halves must inherit sector flats/heights.
#[test]
fn split_then_connect_inner_sector_inherits_record_and_meshes() {
    use crate::render::triangulate::build_sector_tris;

    let mut app = empty_app();
    app.camera.set_zoom(1.0);
    draw_room(
        &mut app,
        &[[0.0, 0.0], [256.0, 0.0], [256.0, 256.0], [0.0, 256.0]],
    );
    draw_room(
        &mut app,
        &[[64.0, 64.0], [192.0, 64.0], [192.0, 192.0], [64.0, 192.0]],
    );
    app.zoom_fit();
    let inner = sector_at(app.map.as_ref().expect("map"), [128.0, 128.0]).expect("inner sectored");
    {
        let m = app.map.as_mut().expect("map");
        m.sectors[inner].floor_height = 10;
        m.sectors[inner].floor_flat = Name8::new("NUKAGE1").expect("flat");
        m.sectors[inner].ceil_flat = Name8::new("FLAT20").expect("flat");
    }
    let want_floor = app.map.as_ref().expect("map").sectors[inner].floor_flat;
    let want_ceil = app.map.as_ref().expect("map").sectors[inner].ceil_flat;

    split_inner_edge(&mut app, [128.0, 64.0]);
    split_inner_edge(&mut app, [128.0, 192.0]);
    app.selection.clear();

    draw_line(&mut app, [128.0, 64.0], [128.0, 192.0]);

    let map = app.map.as_ref().expect("map");
    let left = sector_at(map, [96.0, 128.0]).expect("left half sectored");
    let right = sector_at(map, [160.0, 128.0]).expect("right half sectored");
    assert_ne!(left, right, "the two halves are distinct sectors");

    let tris = build_sector_tris(map);
    for (label, s) in [("left", left), ("right", right)] {
        assert!(
            !tris.tris(s).is_empty(),
            "{label} sector {s:?} has no floor/ceil triangles"
        );
        assert_eq!(
            map.sectors[s].floor_flat, want_floor,
            "{label} inherits the split sector's floor flat, not the brush default"
        );
        assert_eq!(map.sectors[s].ceil_flat, want_ceil, "{label} ceil");
        assert_eq!(
            map.sectors[s].floor_height, 10,
            "{label} inherits the raised floor"
        );
    }
}

/// Auto-split-and-divide: both halves inherit the inner sector's heights/flats.
#[test]
fn draw_across_inner_sector_keeps_heights() {
    let mut app = empty_app();
    app.camera.set_zoom(1.0);
    draw_room(
        &mut app,
        &[[0.0, 0.0], [256.0, 0.0], [256.0, 256.0], [0.0, 256.0]],
    );
    draw_room(
        &mut app,
        &[[64.0, 64.0], [192.0, 64.0], [192.0, 192.0], [64.0, 192.0]],
    );
    app.zoom_fit();
    let outer = sector_at(app.map.as_ref().expect("map"), [16.0, 16.0]).expect("outer sectored");
    let inner = sector_at(app.map.as_ref().expect("map"), [128.0, 128.0]).expect("inner sectored");
    {
        let m = app.map.as_mut().expect("map");
        m.sectors[outer].floor_height = 0;
        m.sectors[outer].ceil_height = 128;
        m.sectors[inner].floor_height = 10;
        m.sectors[inner].ceil_height = 96;
        m.sectors[inner].floor_flat = Name8::new("NUKAGE1").expect("flat");
        m.sectors[inner].ceil_flat = Name8::new("FLAT20").expect("flat");
    }
    let want_floor = app.map.as_ref().expect("map").sectors[inner].floor_flat;
    let want_ceil = app.map.as_ref().expect("map").sectors[inner].ceil_flat;

    draw_line(&mut app, [128.0, 64.0], [128.0, 192.0]);

    let map = app.map.as_ref().expect("map");
    let left = sector_at(map, [96.0, 128.0]).expect("left half sectored");
    let right = sector_at(map, [160.0, 128.0]).expect("right half sectored");
    assert_ne!(left, right, "two distinct halves");
    for (label, s) in [("left", left), ("right", right)] {
        assert_eq!(
            map.sectors[s].floor_height, 10,
            "{label} keeps the inner floor height"
        );
        assert_eq!(
            map.sectors[s].ceil_height, 96,
            "{label} keeps the inner ceil height"
        );
        assert_eq!(map.sectors[s].floor_flat, want_floor, "{label} floor flat");
        assert_eq!(map.sectors[s].ceil_flat, want_ceil, "{label} ceil flat");
    }
}

fn split_inner_edge(app: &mut LevelEditorState, mid: [f32; 2]) {
    app.set_tool(Tool::Select(SelectMode::Line));
    app.selection.clear();
    click_at(app, mid);
    app.split_selected_line_at(mid);
}

/// Drag top vertex through bottom edge → triangle sector on room side; lobes stay void.
#[test]
fn pinch_void_pocket_keeps_lobes_void() {
    let mut app = app_from_ron(
        include_str!("../../../editor-core/tests/fixtures/void_pocket_pinch.ron"),
        "PINCH",
    );

    {
        let m = app.map.as_ref().expect("map");
        let single_sided = m.lines.values().filter(|l| l.back.is_none()).count();
        assert_eq!(single_sided, 11, "all walls single-sided before the pinch");
    }

    app.set_tool(Tool::Select(SelectMode::All));
    drag(&mut app, [2248.0, -1288.0], [2208.0, -1912.0]);

    let m = app.map.as_ref().expect("map");
    assert_eq!(
        m.sectors.len(),
        2,
        "only the poke-through triangle is a new sector, not the void lobes"
    );

    let room = sector_at(m, [1200.0, -1100.0]);
    let triangle = sector_at(m, [2208.0, -1750.0]);
    assert!(room.is_some(), "room is a sector");
    assert!(triangle.is_some(), "triangle is a sector");
    assert_ne!(room, triangle, "triangle distinct from the room sector");
    assert_eq!(sector_at(m, [1700.0, -1450.0]), None, "left lobe is void");
    assert_eq!(sector_at(m, [2700.0, -1450.0]), None, "right lobe is void");

    let originals = [
        [1528.0f32, -1288.0],
        [1912.0, -1296.0],
        [2560.0, -1288.0],
        [2864.0, -1280.0],
        [2864.0, -1704.0],
        [1520.0, -1624.0],
    ];
    let is_orig = |x: f32, y: f32| originals.iter().any(|o| o[0] == x && o[1] == y);
    for l in m.lines.values() {
        let a = m.vertices[l.v1];
        let b = m.vertices[l.v2];
        if is_orig(a.x, a.y) && is_orig(b.x, b.y) {
            assert!(
                l.back.is_none(),
                "void-lobe wall ({},{})->({},{}) must stay single-sided",
                a.x,
                a.y,
                b.x,
                b.y
            );
        }
    }
}

/// Drag triangle point inside box: room right of the new interior edge stays filled.
#[test]
fn drag_triangle_point_past_box_keeps_room_filled() {
    let mut app = app_from_ron(
        include_str!("../../../editor-core/tests/fixtures/triangle_in_box.ron"),
        "TRI",
    );

    app.set_tool(Tool::Select(SelectMode::All));
    drag(&mut app, [2704.0, -1568.0], [1000.0, -1568.0]);

    let m = app.map.as_ref().expect("map");
    let room_right = sector_at(m, [2400.0, -1500.0]);
    let triangle = sector_at(m, [1700.0, -1500.0]);
    assert!(
        room_right.is_some(),
        "room right of the triangle stays filled"
    );
    assert!(triangle.is_some(), "triangle region is a sector");
    assert_ne!(
        room_right, triangle,
        "room and triangle are distinct sectors"
    );
}

/// Drag triangle point outside box: room winds around it, stays one closed region.
#[test]
fn drag_triangle_point_outside_box_keeps_room_filled() {
    let mut app = app_from_ron(
        include_str!("../../../editor-core/tests/fixtures/triangle_in_box.ron"),
        "TRI",
    );

    app.set_tool(Tool::Select(SelectMode::All));
    drag(&mut app, [2704.0, -1568.0], [3200.0, -1568.0]);

    let m = app.map.as_ref().expect("map");
    let room = m.sectors.keys().next().expect("room sector");
    for p in [[1400.0, -1500.0], [2700.0, -900.0], [2700.0, -2200.0]] {
        assert_eq!(
            sector_at(m, p),
            Some(room),
            "room interior {p:?} stays filled"
        );
    }
    let sliver = sector_at(m, [3050.0, -1568.0]);
    assert!(sliver.is_some(), "poked-out sliver is a sector");
    assert_ne!(sliver, Some(room), "sliver distinct from the room");
}

/// Add Sector inside void pillar: enclosing walls must become two-sided.
#[test]
fn add_sector_in_triangle_pillar_makes_it_two_sided() {
    let mut app = app_from_ron(
        include_str!("../../../editor-core/tests/fixtures/triangle_in_box.ron"),
        "TRI",
    );

    let center = [2128.0, -1565.0];
    let room = [1500.0, -1500.0];
    let sectors0 = app.map.as_ref().expect("map").sectors.len();
    let room_sector = sector_at(app.map.as_ref().expect("map"), room);
    assert!(room_sector.is_some(), "room is sectored");
    assert!(
        sector_at(app.map.as_ref().expect("map"), center).is_none(),
        "pillar interior is void"
    );

    app.set_tool(Tool::Select(SelectMode::Sector));
    assert_eq!(app.add_sector_at(center), Damage::Edited);

    let m = app.map.as_ref().expect("map");
    assert_eq!(m.sectors.len(), sectors0 + 1, "a sector was added");
    let new = sector_at(m, center).expect("pillar now filled");
    assert_ne!(Some(new), room_sector, "pillar is a distinct sector");
    for l in m.lines.values() {
        let bounds_new =
            l.front.sector == Some(new) || l.back.as_ref().is_some_and(|b| b.sector == Some(new));
        if bounds_new && l.back.is_some() {
            assert!(
                l.flags.contains(LineFlags::TWO_SIDED),
                "pillar wall two-sided set"
            );
        }
    }
    assert_eq!(sector_at(m, room), room_sector, "room sector preserved");
}

/// Regression: non-crossing nudge must not re-sector. Old bug traced phantom loop across unrelated sectors and stapled sectors onto ~30 distant walls.
#[test]
fn nudge_without_crossing_changes_no_sides() {
    let mut app = app_with_e1m2();

    let vid = app
        .map
        .as_ref()
        .expect("map")
        .vertices
        .iter()
        .find(|(_, v)| v.x == 256.0 && v.y == 224.0)
        .map(|(k, _)| k)
        .expect("vertex at (256,224)");
    let pos = {
        let v = app.map.as_ref().expect("map").vertices[vid];
        [v.x, v.y]
    };

    let sectors_before = app.map.as_ref().expect("map").sectors.len();
    let two_sided_before = app
        .map
        .as_ref()
        .expect("map")
        .lines
        .values()
        .filter(|l| l.back.is_some())
        .count();

    app.set_tool(Tool::Select(SelectMode::All));
    click_at(&mut app, pos);
    drag(&mut app, pos, [pos[0] + 1.0, pos[1]]);

    let map = app.map.as_ref().expect("map");
    assert_eq!(
        map.sectors.len(),
        sectors_before,
        "a non-crossing nudge invented or dropped a sector"
    );
    assert_eq!(
        map.lines.values().filter(|l| l.back.is_some()).count(),
        two_sided_before,
        "a non-crossing nudge changed how many walls are two-sided"
    );
}

/// Delete two-sided divider: adjacent sectors merge; lower-index record survives.
#[test]
fn delete_divider_merges_to_lowest_index() {
    let mut app = empty_app();
    app.camera.set_zoom(1.0);
    draw_room(
        &mut app,
        &[[0.0, 0.0], [128.0, 0.0], [128.0, 128.0], [0.0, 128.0]],
    );
    draw_line(&mut app, [64.0, 0.0], [64.0, 128.0]);
    let first_sector = {
        let map = app.map.as_mut().expect("map");
        let keys: Vec<SectorKey> = map.sectors.keys().collect();
        map.sectors[keys[0]].light_level = 111;
        map.sectors[keys[1]].light_level = 222;
        keys[0]
    };
    assert_eq!(app.map.as_ref().expect("map").sectors.len(), 2);

    let divider = vertical_divider_at(app.map.as_ref().expect("map"), 64.0);
    app.selection.replace(SelItem::Line(divider));
    app.delete_selection();

    let map = app.map.as_ref().expect("map");
    assert_eq!(map.sectors.len(), 1, "merged to one sector");
    assert_eq!(
        map.sectors[first_sector].light_level, 111,
        "lowest-key sector's record survives"
    );
    for l in map.lines.values() {
        assert_eq!(
            l.front.sector,
            Some(first_sector),
            "every wall faces the survivor"
        );
    }
}

#[test]
fn chained_divider_delete_merges_to_one() {
    let mut app = empty_app();
    app.camera.set_zoom(1.0);
    draw_room(
        &mut app,
        &[[0.0, 0.0], [192.0, 0.0], [192.0, 128.0], [0.0, 128.0]],
    );
    draw_line(&mut app, [64.0, 0.0], [64.0, 128.0]);
    draw_line(&mut app, [128.0, 0.0], [128.0, 128.0]);
    assert_eq!(app.map.as_ref().expect("map").sectors.len(), 3);

    let dividers: Vec<LineKey> = {
        let map = app.map.as_ref().expect("map");
        [64.0, 128.0].map(|x| vertical_divider_at(map, x)).to_vec()
    };
    for d in dividers {
        app.selection.push(SelItem::Line(d));
    }
    app.delete_selection();

    let map = app.map.as_ref().expect("map");
    assert_eq!(map.sectors.len(), 1, "all three columns merged");
    let survivor = map.sectors.keys().next().expect("one sector");
    for l in map.lines.values() {
        assert_eq!(
            l.front.sector,
            Some(survivor),
            "every wall faces the survivor"
        );
    }
}

/// Regression: delete must not re-derive sectors outside deleted region. Old whole-region re-trace voided distant walls (UnenclosedSide).
#[test]
fn delete_lines_on_e1m2_no_distant_corruption() {
    use editor_core::Issue;
    let mut app = app_with_e1m2();

    let unenclosed_before = validate(app.map.as_ref().expect("map"))
        .iter()
        .filter(|i| matches!(i, Issue::UnenclosedSide { .. }))
        .count();

    let (mut minx, mut miny, mut maxx, mut maxy) = (f32::MAX, f32::MAX, f32::MIN, f32::MIN);
    for v in app.map.as_ref().expect("map").vertices.values() {
        minx = minx.min(v.x);
        miny = miny.min(v.y);
        maxx = maxx.max(v.x);
        maxy = maxy.max(v.y);
    }
    let (bx0, bx1) = (minx + (maxx - minx) * 0.4, minx + (maxx - minx) * 0.6);
    let (by0, by1) = (miny + (maxy - miny) * 0.4, miny + (maxy - miny) * 0.6);
    app.set_tool(Tool::Select(SelectMode::All));
    let a = app.camera.world_to_screen([bx0, by0]);
    let b = app.camera.world_to_screen([bx1, by1]);
    app.begin_tool_drag(a, false);
    app.drag_to(b);
    app.end_drag(b);
    assert!(!app.selected_lines().is_empty(), "box caught some lines");
    app.delete_selection();

    let unenclosed_after = validate(app.map.as_ref().expect("map"))
        .iter()
        .filter(|i| matches!(i, Issue::UnenclosedSide { .. }))
        .count();
    assert_eq!(
        unenclosed_after,
        unenclosed_before,
        "delete introduced {} voided (UnenclosedSide) walls — sectors flooded",
        unenclosed_after as i64 - unenclosed_before as i64
    );
}

#[test]
fn drag_line_across_another_splits_both() {
    let mut app = empty_app();
    app.camera.set_zoom(1.0);
    draw_line(&mut app, [0.0, 64.0], [128.0, 64.0]);
    draw_line(&mut app, [200.0, 0.0], [200.0, 128.0]);
    assert_eq!(app.map.as_ref().expect("map").lines.len(), 2);

    app.set_tool(Tool::Select(SelectMode::All));
    click_at(&mut app, [200.0, 64.0]);
    drag(&mut app, [200.0, 64.0], [64.0, 64.0]);
    app.commit_pending_move();

    let map = app.map.as_ref().expect("map");
    assert_eq!(map.lines.len(), 4, "both lines split at the crossing");
    let cross = map
        .vertices
        .iter()
        .find(|(_, v)| (v.x, v.y) == (64.0, 64.0))
        .map(|(k, _)| k)
        .expect("crossing vertex exists");
    let degree = map
        .lines
        .values()
        .flat_map(|l| [l.v1, l.v2])
        .filter(|&v| v == cross)
        .count();
    assert_eq!(degree, 4, "four line-ends meet at the crossing");
}

/// Collinear overlap must dedup to one linedef, not leave coincident duplicates.
#[test]
fn drag_line_onto_collinear_dedups_overlap() {
    let mut app = empty_app();
    app.camera.set_zoom(1.0);
    draw_line(&mut app, [0.0, 0.0], [128.0, 0.0]);
    draw_line(&mut app, [0.0, 64.0], [128.0, 64.0]);
    assert_eq!(app.map.as_ref().expect("map").lines.len(), 2);

    app.zoom_fit();
    app.set_tool(Tool::Select(SelectMode::All));
    click_at(&mut app, [64.0, 64.0]);
    drag(&mut app, [64.0, 64.0], [64.0, 0.0]);
    app.commit_pending_move();

    let map = app.map.as_ref().expect("map");
    let on_y0 = map
        .lines
        .values()
        .filter(|l| {
            let p1 = map.vertices[l.v1];
            let p2 = map.vertices[l.v2];
            p1.y == 0.0 && p2.y == 0.0
        })
        .count();
    assert_eq!(on_y0, 1, "overlapping segment is a single line");
    assert_eq!(map.lines.len(), 1, "no duplicate coincident line remains");
}

/// Regression: vertex move must only re-sector incident geometry. Old whole-map re-derive flooded sectors across the level.
#[test]
fn move_vertex_on_e1m2_does_not_flood_distant_sectors() {
    let mut app = app_with_e1m2();

    let moved_pos = {
        let map = app.map.as_ref().expect("map");
        let v = map.vertices[vert_key(map, 474)];
        [v.x, v.y]
    };

    // Key by endpoint positions (indices shift after a move).
    let key = |a: [f32; 2], b: [f32; 2]| {
        let lo = (a[0].min(b[0]), a[1].min(b[1]));
        let hi = (a[0].max(b[0]), a[1].max(b[1]));
        (
            (lo.0.to_bits(), lo.1.to_bits()),
            (hi.0.to_bits(), hi.1.to_bits()),
        )
    };
    let far = |a: [f32; 2], b: [f32; 2]| {
        let near = |p: [f32; 2]| {
            (p[0] - moved_pos[0]).abs() < 256.0 && (p[1] - moved_pos[1]).abs() < 256.0
        };
        !near(a) && !near(b)
    };
    let snapshot = |app: &LevelEditorState| {
        let map = app.map.as_ref().expect("map");
        map.lines
            .values()
            .filter_map(|l| {
                let a = map.vertices.get(l.v1)?;
                let b = map.vertices.get(l.v2)?;
                let (a, b) = ([a.x, a.y], [b.x, b.y]);
                far(a, b).then(|| {
                    (
                        key(a, b),
                        (l.front.sector, l.back.as_ref().and_then(|s| s.sector)),
                    )
                })
            })
            .collect::<HashMap<_, _>>()
    };

    let before = snapshot(&app);

    app.set_tool(Tool::Select(SelectMode::All));
    click_at(&mut app, moved_pos);
    drag(&mut app, moved_pos, [moved_pos[0] + 8.0, moved_pos[1]]);

    let after = snapshot(&app);
    let changed = before
        .iter()
        .filter(|(k, sectors_before)| after.get(*k) != Some(*sectors_before))
        .count();
    assert_eq!(
        changed, 0,
        "{changed} distant lines were re-sectored by a single vertex move (flood)"
    );
}

#[test]
fn shape_tools_make_n_sided_sectors() {
    for (shape, centre, edges) in [
        (DrawShape::Rect, [64.0, 48.0], 4),
        (DrawShape::Triangle, [100.0, 100.0], 3),
        (DrawShape::Ngon, [100.0, 100.0], 6),
    ] {
        let mut app = empty_app();
        app.camera.set_zoom(1.0);
        app.set_snap(false);
        app.set_ngon_sides(6);
        let (a, b) = match shape {
            DrawShape::Rect => ([0.0, 0.0], [128.0, 96.0]),
            _ => ([100.0, 100.0], [160.0, 100.0]),
        };
        draw_shape(&mut app, shape, a, b);
        let map = app.map.as_ref().expect("map");
        assert_eq!(map.lines.len(), edges, "{shape:?} edge count");
        assert_eq!(map.sectors.len(), 1, "{shape:?} one sector");
        let sector = map.sectors.keys().next().expect("one sector");
        assert_eq!(sector_at(map, centre), Some(sector));
        assert!(matches!(app.shape_draw, ShapeDraw::None), "draw cleared");
    }
}

#[test]
fn rect_into_existing_room_splits_walls() {
    let mut app = empty_app();
    app.camera.set_zoom(1.0);
    draw_room(
        &mut app,
        &[[0.0, 0.0], [128.0, 0.0], [128.0, 128.0], [0.0, 128.0]],
    );
    let before = app.map.as_ref().expect("map").lines.len();
    draw_shape(&mut app, DrawShape::Rect, [64.0, 32.0], [192.0, 96.0]);
    let map = app.map.as_ref().expect("map");
    assert!(
        map.lines.len() > before + 4,
        "the room wall split on overlap"
    );
}

#[test]
fn escape_cancels_anchored_shape() {
    let mut app = empty_app();
    app.camera.set_zoom(1.0);
    app.set_tool(Tool::Draw(DrawShape::Rect));
    click_at(&mut app, [0.0, 0.0]);
    app.cancel_gesture();
    let map = app.map.as_ref().expect("map");
    assert!(map.lines.is_empty(), "no shape committed");
    assert!(map.sectors.is_empty());
    assert!(matches!(app.shape_draw, ShapeDraw::None));
}

#[test]
fn shape_vertices_snap_only_when_grid_snap_on() {
    let mut app = empty_app();
    app.camera.set_zoom(1.0);
    app.set_snap(false);
    let pts = app.shape_points(DrawShape::Triangle, [100.0, 100.0], [163.0, 100.0]);
    assert_eq!(pts[0], [163.0, 100.0], "snap off → exact vertex");

    app.set_snap(true);
    let pts = app.shape_points(DrawShape::Rect, [1.0, 2.0], [61.0, 67.0]);
    for p in pts {
        assert_eq!(p[0] % 8.0, 0.0, "x on grid");
        assert_eq!(p[1] % 8.0, 0.0, "y on grid");
    }
}

#[test]
fn stray_line_in_room_keeps_one_sector() {
    let mut app = empty_app();
    app.camera.set_zoom(1.0);
    draw_room(
        &mut app,
        &[[0.0, 0.0], [128.0, 0.0], [128.0, 128.0], [0.0, 128.0]],
    );
    draw_line(&mut app, [40.0, 40.0], [40.0, 88.0]);
    let map = app.map.as_ref().expect("map");
    assert_eq!(map.sectors.len(), 1, "stray line splits nothing");
    let room = sector_at(map, [80.0, 64.0]);
    assert_eq!(sector_at(map, [20.0, 64.0]), room);
}

/// Drawing a second room sharing a wall with the first must not merge them.
#[test]
fn adjacent_room_draw_keeps_sectors_distinct() {
    let mut app = empty_app();
    app.camera.set_zoom(1.0);
    draw_room(
        &mut app,
        &[[0.0, 0.0], [128.0, 0.0], [128.0, 128.0], [0.0, 128.0]],
    );
    draw_room(
        &mut app,
        &[[128.0, 0.0], [256.0, 0.0], [256.0, 128.0], [128.0, 128.0]],
    );
    let map = app.map.as_ref().expect("map");
    assert_eq!(map.sectors.len(), 2, "two distinct rooms");
    let a = sector_at(map, [64.0, 64.0]).expect("room A sectored");
    let b = sector_at(map, [192.0, 64.0]).expect("room B sectored");
    assert_ne!(a, b, "rooms did not merge");
    let shared = map
        .lines
        .values()
        .find(|l| map.vertices[l.v1].x == 128.0 && map.vertices[l.v2].x == 128.0)
        .expect("shared wall");
    let back = shared.back.as_ref().expect("shared wall is two-sided");
    assert_ne!(shared.front.sector, back.sector, "one room each side");
}

/// Extending a room with three walls closing against an existing wall makes a sector, and the shared wall becomes two-sided.
#[test]
fn three_side_extension_sectors_and_shares_the_wall() {
    let mut app = empty_app();
    app.camera.set_zoom(1.0);
    draw_room(
        &mut app,
        &[[0.0, 0.0], [128.0, 0.0], [128.0, 128.0], [0.0, 128.0]],
    );
    app.set_tool(Tool::Draw(DrawShape::Line));
    click_at(&mut app, [128.0, 0.0]);
    click_at(&mut app, [256.0, 0.0]);
    click_at(&mut app, [256.0, 128.0]);
    shift_click_at(&mut app, [128.0, 128.0]);

    let map = app.map.as_ref().expect("map");
    assert_eq!(map.sectors.len(), 2, "the extension is a sector");
    let a = sector_at(map, [64.0, 64.0]).expect("room A sectored");
    let b = sector_at(map, [192.0, 64.0]).expect("extension sectored");
    assert_ne!(a, b, "extension distinct from the room");
    let shared = map
        .lines
        .values()
        .find(|l| map.vertices[l.v1].x == 128.0 && map.vertices[l.v2].x == 128.0)
        .expect("shared wall");
    let back = shared.back.as_ref().expect("shared wall went two-sided");
    assert!(shared.flags.contains(LineFlags::TWO_SIDED));
    assert_ne!(shared.front.sector, back.sector, "one room each side");
}

#[test]
fn box_in_box_two_draws_inner_sector_and_ring() {
    let mut app = empty_app();
    app.camera.set_zoom(1.0);
    draw_room(
        &mut app,
        &[[0.0, 0.0], [256.0, 0.0], [256.0, 256.0], [0.0, 256.0]],
    );
    assert_eq!(app.map.as_ref().expect("map").sectors.len(), 1);
    draw_room(
        &mut app,
        &[[64.0, 64.0], [192.0, 64.0], [192.0, 192.0], [64.0, 192.0]],
    );
    let map = app.map.as_ref().expect("map");
    let inner = sector_at(map, [128.0, 128.0]);
    let ring = sector_at(map, [32.0, 128.0]);
    assert!(inner.is_some() && ring.is_some());
    assert_ne!(inner, ring, "inner box distinct from the ring");
}

#[test]
fn bridge_between_two_drawn_boxes_makes_three_sectors() {
    let mut app = empty_app();
    app.camera.set_zoom(1.0);
    draw_room(
        &mut app,
        &[[0.0, 0.0], [128.0, 0.0], [128.0, 128.0], [0.0, 128.0]],
    );
    draw_room(
        &mut app,
        &[[144.0, 0.0], [272.0, 0.0], [272.0, 128.0], [144.0, 128.0]],
    );
    assert_eq!(app.map.as_ref().expect("map").sectors.len(), 2);
    draw_shape(&mut app, DrawShape::Rect, [128.0, 32.0], [144.0, 96.0]);
    let map = app.map.as_ref().expect("map");
    assert_eq!(map.sectors.len(), 3, "box A + corridor + box B");
    let a = sector_at(map, [64.0, 64.0]);
    let corridor = sector_at(map, [136.0, 64.0]);
    let b = sector_at(map, [208.0, 64.0]);
    assert!(a.is_some() && corridor.is_some() && b.is_some());
    assert_ne!(a, corridor, "corridor distinct from box A");
    assert_ne!(b, corridor, "corridor distinct from box B");
    assert_ne!(a, b, "boxes stay separate");
    let coincident = map
        .lines
        .values()
        .filter(|l| {
            let k = (l.v1.min(l.v2), l.v1.max(l.v2));
            map.lines
                .values()
                .filter(|o| (o.v1.min(o.v2), o.v1.max(o.v2)) == k)
                .count()
                > 1
        })
        .count();
    assert_eq!(coincident, 0, "no coincident duplicate linedefs");
}

#[test]
fn drawn_room_validates_clean() {
    let mut app = empty_app();
    app.camera.set_zoom(1.0);
    draw_room(
        &mut app,
        &[[0.0, 0.0], [128.0, 0.0], [128.0, 128.0], [0.0, 128.0]],
    );
    let issues = validate(app.map.as_ref().expect("map"));
    assert!(issues.is_empty(), "clean room, got {issues:?}");
}

#[test]
fn flip_swaps_endpoints_and_sides() {
    let mut app = app_with_map();
    let key = line_key(app.map.as_ref().expect("map"), 0);
    let before = {
        let l = &app.map.as_ref().expect("map").lines[key];
        (l.v1, l.v2)
    };
    app.flip_selected_lines(&[key]);
    let after = {
        let l = &app.map.as_ref().expect("map").lines[key];
        (l.v1, l.v2)
    };
    assert_eq!(after, (before.1, before.0), "endpoints swapped");
    app.undo();
    let restored = {
        let l = &app.map.as_ref().expect("map").lines[key];
        (l.v1, l.v2)
    };
    assert_eq!(restored, before, "undo restores winding");
}

#[test]
fn thing_tool_places_template() {
    let mut app = empty_app();
    app.set_tool(Tool::Thing);
    app.thing_template.kind = 3001;
    click_at(&mut app, [33.0, -31.0]);
    let map = app.map.as_ref().expect("map");
    assert_eq!(map.things.len(), 1);
    let t = map.things.values().next().expect("one thing");
    assert_eq!((t.x, t.y, t.kind), (32, -32, 3001));
}

#[test]
fn select_drag_moves_with_snap_and_undo_restores() {
    let mut app = app_with_map();
    let vk = vert_key(app.map.as_ref().expect("map"), 0);
    let v0 = app.map.as_ref().expect("map").vertices[vk];
    let from = app.camera.world_to_screen([v0.x, v0.y]);
    app.rebuild_pick_mesh();
    app.begin_tool_drag(from, false);
    assert!(app.selection.contains(&SelItem::Vertex(vk)));
    assert!(matches!(app.drag, DragState::MoveSel { .. }));

    let to = app.camera.world_to_screen([v0.x + 19.0, v0.y + 1.0]);
    app.drag_to(to);
    let mid = app.map.as_ref().expect("map").vertices[vk];
    assert_eq!((mid.x, mid.y), (v0.x, v0.y), "map unchanged until release");
    assert!(matches!(app.overlay, Overlay::Move { .. }));

    app.end_drag(to);
    let moved = app.map.as_ref().expect("map").vertices[vk];
    assert_eq!((moved.x, moved.y), (v0.x + 16.0, v0.y), "snapped delta");

    app.undo();
    let restored = app.map.as_ref().expect("map").vertices[vk];
    assert_eq!((restored.x, restored.y), (v0.x, v0.y));
}

#[test]
fn click_selects_shift_click_toggles() {
    let mut app = app_with_map();
    let (k0, k1) = {
        let map = app.map.as_ref().expect("map");
        (vert_key(map, 0), vert_key(map, 1))
    };
    let v0 = app.map.as_ref().expect("map").vertices[k0];
    let at = app.camera.world_to_screen([v0.x, v0.y]);
    let v1 = app.map.as_ref().expect("map").vertices[k1];
    let at1 = app.camera.world_to_screen([v1.x, v1.y]);
    app.rebuild_pick_mesh();
    app.tool_click(at, false);
    assert!(app.selection.contains(&SelItem::Vertex(k0)));
    app.tool_click(at, false);
    assert!(app.selection.contains(&SelItem::Vertex(k0)));
    app.tool_click(at1, true);
    assert!(app.selection.contains(&SelItem::Vertex(k0)));
    assert!(app.selection.contains(&SelItem::Vertex(k1)));
    app.tool_click(at, true);
    assert!(!app.selection.contains(&SelItem::Vertex(k0)));
    assert!(app.selection.contains(&SelItem::Vertex(k1)));
}

#[test]
fn rubber_band_selects_contained_items() {
    let mut app = empty_app();
    draw_line(&mut app, [0.0, 0.0], [64.0, 0.0]);
    app.set_tool(Tool::Thing);
    click_at(&mut app, [32.0, 32.0]);

    app.set_tool(Tool::Select(SelectMode::All));
    drag(&mut app, [-100.0, 200.0], [200.0, -100.0]);
    let (lk, tk) = {
        let map = app.map.as_ref().expect("map");
        (line_key(map, 0), thing_key(map, 0))
    };
    let items = app.selection.items();
    assert!(items.contains(&SelItem::Line(lk)));
    assert!(items.contains(&SelItem::Thing(tk)));
    assert_eq!(items.len(), 4, "two vertices, one line, one thing");
}

#[test]
fn delete_selection_removes_and_undo_restores() {
    let mut app = app_with_map();
    let lines_before = app.map.as_ref().expect("map").lines.len();
    let things_before = app.map.as_ref().expect("map").things.len();

    let (lk, tk) = {
        let map = app.map.as_ref().expect("map");
        (line_key(map, 0), thing_key(map, 0))
    };
    app.selection.replace(SelItem::Thing(tk));
    app.selection.push(SelItem::Line(lk));
    let damage = app.delete_selection();
    assert_eq!(damage, Damage::Edited);
    assert_eq!(app.map.as_ref().expect("map").lines.len(), lines_before - 1);
    assert_eq!(
        app.map.as_ref().expect("map").things.len(),
        things_before - 1
    );

    app.undo();
    assert_eq!(app.map.as_ref().expect("map").lines.len(), lines_before);
    assert_eq!(app.map.as_ref().expect("map").things.len(), things_before);
}

#[test]
fn get_tool_samples_facing_sector() {
    let mut app = app_with_map();
    app.set_tool(Tool::Sector);
    click_at(&mut app, [1056.0, -3616.0]);
    assert!(app.sampled_sector.is_some());

    app.sampled_sector = None;
    click_at(&mut app, [1056.0, -3800.0]);
    assert_eq!(app.sampled_sector, None);
}

#[test]
fn sector_sample_damages_so_highlight_repaints() {
    let mut app = app_with_map();
    app.set_tool(Tool::Sector);
    let first = click_at(&mut app, [1056.0, -3616.0]);
    assert_eq!(first, Damage::Edited);
    let again = click_at(&mut app, [1056.0, -3616.0]);
    assert_eq!(again, Damage::None);
}

#[test]
fn sector_click_void_clears_current_sector() {
    let mut app = app_with_map();
    app.set_tool(Tool::Sector);
    click_at(&mut app, [1056.0, -3616.0]);
    assert!(app.current_sector.is_some());
    click_at(&mut app, [1056.0, -3800.0]);
    assert_eq!(app.current_sector, None);
}

#[test]
fn rubber_band_filters_by_select_mode() {
    let mut app = app_with_map();
    app.set_tool(Tool::Select(SelectMode::Thing));
    app.begin_rubber(SelectMode::Thing, [0.0, 0.0], false);
    let bounds = map_bounds(app.map.as_ref().expect("map")).expect("non-empty");
    app.rubber_select(
        SelectMode::Thing,
        [bounds.min_x, bounds.min_y],
        [bounds.max_x, bounds.max_y],
    );
    assert!(
        app.selection
            .items()
            .iter()
            .all(|i| matches!(i, SelItem::Thing(_))),
        "select-thing mode selects only things"
    );
}

/// Select-Line mode picks lines even when a vertex is at the same position.
#[test]
fn select_line_mode_clicks_line_not_vertex() {
    let mut app = app_with_map();
    app.set_tool(Tool::Select(SelectMode::Line));
    let vk = vert_key(app.map.as_ref().expect("map"), 0);
    let v = app.map.as_ref().expect("map").vertices[vk];
    let lines_at_v: Vec<LineKey> = app
        .map
        .as_ref()
        .expect("map")
        .lines
        .iter()
        .filter(|(_, l)| l.v1 == vk || l.v2 == vk)
        .map(|(k, _)| k)
        .collect();
    assert!(!lines_at_v.is_empty(), "the first vertex has a line");
    click_at(&mut app, [v.x, v.y]);
    assert!(
        app.selection
            .items()
            .iter()
            .any(|i| matches!(i, SelItem::Line(l) if lines_at_v.contains(l))),
        "select-line mode picks a line at the vertex, not the vertex"
    );
    assert!(
        !app.selection
            .items()
            .iter()
            .any(|i| matches!(i, SelItem::Vertex(_))),
        "no vertex selected in line mode"
    );
}

#[test]
fn skill_filtered_thing_is_not_pickable() {
    use editor_core::ThingFlags;
    let mut app = empty_app();
    app.camera.set_zoom(1.0);
    draw_room(
        &mut app,
        &[[0.0, 0.0], [256.0, 0.0], [256.0, 256.0], [0.0, 256.0]],
    );
    app.zoom_fit();
    let tk = {
        let m = app.map.as_mut().expect("map");
        m.things.insert(editor_core::Thing {
            x: 128,
            y: 128,
            z: 0,
            angle: 0,
            kind: 1,
            options: ThingFlags::HARD,
        })
    };
    app.thing_extents.insert(1, [16.0, 16.0]);
    app.set_tool(Tool::Select(SelectMode::Thing));

    app.set_skill_filter(SkillFilter::Easy);
    click_at(&mut app, [128.0, 128.0]);
    assert!(
        !app.selection.contains(&SelItem::Thing(tk)),
        "a skill-filtered (hidden) thing must not be pickable"
    );

    app.set_skill_filter(SkillFilter::All);
    click_at(&mut app, [128.0, 128.0]);
    assert!(
        app.selection.contains(&SelItem::Thing(tk)),
        "the visible thing is pickable"
    );
}

#[test]
fn thing_edits_report_edited_for_the_reconciler() {
    use editor_core::Thing;
    let mut app = app_with_map();
    let key = {
        let map = app.map.as_ref().expect("map");
        map.things.keys().last().expect("has things")
    };

    let kind_damage = app.apply_things(&[key], |t| Thing {
        kind: t.kind + 1000,
        ..*t
    });
    assert_eq!(
        kind_damage,
        Damage::Edited,
        "kind change reaches the reconciler"
    );

    let angle_damage = app.apply_things(&[key], |t| Thing {
        angle: (t.angle + 90) % 360,
        ..*t
    });
    assert_eq!(
        angle_damage,
        Damage::Edited,
        "angle change reaches the reconciler"
    );
}

/// Every E1M1 thing's z must equal its sector floor (derived on import).
#[test]
fn imported_things_carry_their_sector_floor_z() {
    use editor_core::geom::sector_at;
    let app = app_with_map();
    let map = app.map.as_ref().expect("map");
    let mut checked = 0;
    for t in map.things.values() {
        let Some(s) = sector_at(map, [t.x as f32, t.y as f32]) else {
            continue; // void thing keeps z = 0
        };
        assert_eq!(
            t.z, map.sectors[s].floor_height,
            "thing at ({},{}) sits on its sector floor",
            t.x, t.y
        );
        checked += 1;
    }
    assert!(checked > 0, "E1M1 has things inside sectors");
}

#[test]
fn merge_selected_lines_collapses_collinear_chain() {
    let mut app = empty_app();
    app.camera.set_zoom(1.0);
    draw_line(&mut app, [0.0, 0.0], [64.0, 0.0]);
    draw_line(&mut app, [64.0, 0.0], [128.0, 0.0]);
    let (l0, l1) = {
        let map = app.map.as_ref().expect("map");
        assert_eq!(map.lines.len(), 2);
        (line_key(map, 0), line_key(map, 1))
    };
    app.selection.push(SelItem::Line(l0));
    app.selection.push(SelItem::Line(l1));
    assert!(app.lines_mergeable(), "straight chain is mergeable");
    assert_eq!(app.merge_selected_lines(), Damage::Edited);
    assert_eq!(
        app.map.as_ref().expect("map").lines.len(),
        1,
        "merged to one"
    );
}

#[test]
fn dissolve_selected_collapses_chain_and_skips_junction() {
    let mut app = empty_app();
    app.camera.set_zoom(1.0);
    draw_line(&mut app, [0.0, 0.0], [64.0, 0.0]);
    draw_line(&mut app, [64.0, 0.0], [128.0, 0.0]);
    draw_line(&mut app, [128.0, 0.0], [192.0, 0.0]);
    draw_line(&mut app, [128.0, 0.0], [128.0, 64.0]);
    let keys: Vec<LineKey> = app.map.as_ref().expect("map").lines.keys().collect();
    assert_eq!(keys.len(), 4);
    for k in keys {
        app.selection.push(SelItem::Line(k));
    }
    assert!(app.can_dissolve());
    assert_eq!(app.dissolve_selected(), Damage::Edited);
    let map = app.map.as_ref().expect("map");
    assert_eq!(map.lines.len(), 3, "chain merged at (64,0); junction kept");
    app.undo();
    assert_eq!(
        app.map.as_ref().expect("map").lines.len(),
        4,
        "dissolve is one undo step"
    );
}

#[test]
fn single_line_drag_holds_then_extrudes() {
    let mut app = empty_app();
    app.camera.set_zoom(1.0);
    draw_line(&mut app, [0.0, 0.0], [64.0, 0.0]);
    app.set_tool(Tool::Select(SelectMode::All));
    assert_eq!(drag(&mut app, [32.0, 0.0], [32.0, 32.0]), Damage::Overlay);
    let line = app.line_drag_pending().expect("held for Move/Extrude");
    assert_eq!(app.commit_pending_extrude(), Damage::Edited);
    let map = app.map.as_ref().expect("map");
    assert_eq!(map.lines.len(), 4, "quad closed");
    assert_eq!(map.sectors.len(), 1, "extruded space sectored");
    assert!(map.lines.contains(line), "source line survives");
    assert!(app.line_drag_pending().is_none());
}

#[test]
fn single_line_drag_move_choice_commits_move() {
    let mut app = empty_app();
    app.camera.set_zoom(1.0);
    draw_line(&mut app, [0.0, 0.0], [64.0, 0.0]);
    app.set_tool(Tool::Select(SelectMode::All));
    assert_eq!(drag(&mut app, [32.0, 0.0], [32.0, 32.0]), Damage::Overlay);
    assert_eq!(app.commit_pending_move(), Damage::Edited);
    let map = app.map.as_ref().expect("map");
    assert_eq!(map.lines.len(), 1);
    assert!(
        map.vertices.values().all(|v| v.y == 32.0),
        "line moved, not extruded"
    );
}

#[test]
fn multi_selection_drag_commits_immediately() {
    let mut app = empty_app();
    app.camera.set_zoom(1.0);
    draw_line(&mut app, [0.0, 0.0], [64.0, 0.0]);
    draw_line(&mut app, [0.0, 128.0], [64.0, 128.0]);
    app.set_tool(Tool::Select(SelectMode::All));
    let keys: Vec<LineKey> = app.map.as_ref().expect("map").lines.keys().collect();
    for k in keys {
        app.selection.push(SelItem::Line(k));
    }
    app.rebuild_pick_mesh();
    let from = app.camera.world_to_screen([32.0, 0.0]);
    let to = app.camera.world_to_screen([32.0, 16.0]);
    app.begin_tool_drag(from, true);
    app.drag_to(to);
    assert_eq!(app.end_drag(to), Damage::Edited, "no hold for multi-drag");
    assert!(app.line_drag_pending().is_none());
}

#[test]
fn straighten_selected_chain_welds_through_move_pipeline() {
    let mut app = empty_app();
    app.camera.set_zoom(1.0);
    draw_line(&mut app, [0.0, 0.0], [32.0, 8.0]);
    draw_line(&mut app, [32.0, 8.0], [64.0, -8.0]);
    draw_line(&mut app, [64.0, -8.0], [96.0, 0.0]);
    let keys: Vec<LineKey> = app.map.as_ref().expect("map").lines.keys().collect();
    for k in keys {
        app.selection.push(SelItem::Line(k));
    }
    assert!(app.can_align());
    assert_eq!(app.straighten_selected(), Damage::Edited);
    let map = app.map.as_ref().expect("map");
    assert!(
        map.vertices.values().all(|v| v.y == 0.0),
        "interior vertices projected onto the end-to-end segment"
    );
}

#[test]
fn align_selected_vertices_sets_shared_coordinate() {
    let mut app = empty_app();
    app.camera.set_zoom(1.0);
    draw_line(&mut app, [0.0, 0.0], [64.0, 16.0]);
    draw_line(&mut app, [64.0, 16.0], [128.0, 8.0]);
    let keys: Vec<LineKey> = app.map.as_ref().expect("map").lines.keys().collect();
    for k in keys {
        app.selection.push(SelItem::Line(k));
    }
    assert_eq!(app.align_selected(Axis::Y), Damage::Edited);
    let map = app.map.as_ref().expect("map");
    assert_eq!(map.vertices.len(), 3);
    assert!(map.vertices.values().all(|v| v.y == 8.0), "mean y = 8");
}

/// Closed square drawn through the real draw path (derives its sector).
fn draw_square(app: &mut LevelEditorState) {
    draw_line(app, [0.0, 0.0], [64.0, 0.0]);
    draw_line(app, [64.0, 0.0], [64.0, 64.0]);
    draw_line(app, [64.0, 64.0], [0.0, 64.0]);
    draw_line(app, [0.0, 64.0], [0.0, 0.0]);
}

fn select_all_lines(app: &mut LevelEditorState) {
    let keys: Vec<LineKey> = app.map.as_ref().expect("map").lines.keys().collect();
    for k in keys {
        app.selection.push(SelItem::Line(k));
    }
}

#[test]
fn plain_sector_click_replaces_mixed_selection() {
    let mut app = empty_app();
    app.camera.set_zoom(1.0);
    draw_square(&mut app);
    app.set_tool(Tool::Select(SelectMode::All));
    app.rebuild_pick_mesh();
    let line = {
        let map = app.map.as_ref().expect("map");
        map.lines.keys().next().expect("line")
    };
    app.selection.replace(SelItem::Line(line));
    // Plain click on the sector interior: the line selection must not survive.
    click_at(&mut app, [32.0, 32.0]);
    let items = app.selection.items();
    assert!(
        matches!(items, [SelItem::Sector(_)]),
        "sector replaced the line selection: {items:?}"
    );
    // Shift-click a line: multiselect keeps the sector.
    shift_click_at(&mut app, [32.0, 0.0]);
    assert_eq!(
        app.selection.items().len(),
        2,
        "sector + line: {:?}",
        app.selection.items()
    );
}

#[test]
fn dissolve_and_straighten_gates_track_selection_shape() {
    let mut app = empty_app();
    app.camera.set_zoom(1.0);
    // Right-angle corner: nothing dissolvable, but the L is a straightenable chain.
    draw_line(&mut app, [0.0, 0.0], [64.0, 0.0]);
    draw_line(&mut app, [64.0, 0.0], [64.0, 64.0]);
    select_all_lines(&mut app);
    assert!(!app.can_dissolve(), "corner never dissolves at 45 degrees");
    assert!(app.can_straighten(), "an L-chain projects onto its span");

    // Straight chain: dissolvable, but already straight.
    let mut app = empty_app();
    app.camera.set_zoom(1.0);
    draw_line(&mut app, [0.0, 0.0], [64.0, 0.0]);
    draw_line(&mut app, [64.0, 0.0], [128.0, 0.0]);
    select_all_lines(&mut app);
    assert!(app.can_dissolve(), "interior chain vertex dissolves");
    assert!(!app.can_straighten(), "already straight: hidden");
}

#[test]
fn angle_snap_quantises_chain_segment_to_45_degrees() {
    let mut app = empty_app();
    app.camera.set_zoom(1.0);
    app.angle_snap = true;
    app.snap = false;
    // Second click at ~32°: the segment must land on the 45° ray.
    draw_line(&mut app, [0.0, 0.0], [80.0, 50.0]);
    let map = app.map.as_ref().expect("map");
    assert_eq!(map.lines.len(), 1);
    let far = map
        .vertices
        .values()
        .find(|v| (v.x, v.y) != (0.0, 0.0))
        .expect("far vertex");
    assert!(
        (far.x - far.y).abs() < 1e-3,
        "x == y on the diagonal, got ({}, {})",
        far.x,
        far.y
    );
}

#[test]
fn rotate_selected_90_is_exact() {
    let mut app = empty_app();
    app.camera.set_zoom(1.0);
    draw_line(&mut app, [0.0, 0.0], [64.0, 0.0]);
    select_all_lines(&mut app);
    assert!(app.can_transform());
    assert_eq!(app.rotate_selected_90(true), Damage::Edited);
    let map = app.map.as_ref().expect("map");
    let mut ys: Vec<f32> = map.vertices.values().map(|v| v.y).collect();
    ys.sort_by(|a, b| a.total_cmp(b));
    assert_eq!(ys, [-32.0, 32.0], "exact quarter turn about the centre");
    assert!(map.vertices.values().all(|v| v.x == 32.0));
}

#[test]
fn mirror_selected_keeps_sector_resolvable() {
    let mut app = empty_app();
    app.camera.set_zoom(1.0);
    draw_square(&mut app);
    assert_eq!(app.map.as_ref().expect("map").sectors.len(), 1);
    select_all_lines(&mut app);
    assert_eq!(app.mirror_selected(Axis::X), Damage::Edited);
    let map = app.map.as_ref().expect("map");
    assert_eq!(map.sectors.len(), 1, "sector survives the mirror");
    assert_eq!(
        sector_at(map, [32.0, 32.0]),
        map.sectors.keys().next(),
        "mirrored interior still resolves (winding fixed)"
    );
}

#[test]
fn scale_handle_drag_scales_about_opposite_corner() {
    let mut app = empty_app();
    app.camera.set_zoom(1.0);
    draw_square(&mut app);
    app.set_tool(Tool::Select(SelectMode::All));
    select_all_lines(&mut app);
    let h = app.transform_handles().expect("handles for selection");
    assert_eq!((h.min, h.max), ([0.0, 0.0], [64.0, 64.0]));
    // Drag the max corner from (64,64) to (128,128): scale 2x about (0,0).
    let from = app.camera.world_to_screen([64.0, 64.0]);
    let to = app.camera.world_to_screen([128.0, 128.0]);
    app.begin_tool_drag(from, false);
    assert!(
        matches!(app.drag, DragState::Transform { .. }),
        "corner handle claimed the drag"
    );
    app.drag_to(to);
    assert_eq!(app.end_drag(to), Damage::Edited);
    let map = app.map.as_ref().expect("map");
    let mut xs: Vec<f32> = map.vertices.values().map(|v| v.x).collect();
    xs.sort_by(|a, b| a.total_cmp(b));
    assert_eq!(xs[0], 0.0);
    assert_eq!(*xs.last().expect("verts"), 128.0);
}

#[test]
fn heal_geometry_fixes_seeded_t_junction() {
    let mut app = empty_app();
    app.camera.set_zoom(1.0);
    draw_line(&mut app, [0.0, 0.0], [64.0, 0.0]);
    // Seed an unsplit stem foot resting on the wall interior.
    {
        let map = app.map.as_mut().expect("map");
        let a = map.vertices.insert(Vertex {
            x: 32.0,
            y: 0.0,
        });
        let b = map.vertices.insert(Vertex {
            x: 32.0,
            y: 32.0,
        });
        let template = map.lines.values().next().copied().expect("line");
        map.lines.insert(LineDef {
            v1: a,
            v2: b,
            ..template
        });
    }
    assert_eq!(app.heal_geometry(), Damage::Edited);
    assert_eq!(
        app.map.as_ref().expect("map").lines.len(),
        3,
        "wall split at the stem foot"
    );
    app.undo();
    assert_eq!(
        app.map.as_ref().expect("map").lines.len(),
        2,
        "heal is one undo step"
    );
}

#[test]
fn chamfer_and_fillet_selected_corner() {
    let mut app = empty_app();
    app.camera.set_zoom(1.0);
    draw_line(&mut app, [0.0, 0.0], [64.0, 0.0]);
    draw_line(&mut app, [64.0, 0.0], [64.0, 64.0]);
    let corner = {
        let map = app.map.as_ref().expect("map");
        map.vertices
            .iter()
            .find(|(_, p)| (p.x, p.y) == (64.0, 0.0))
            .map(|(k, _)| k)
            .expect("corner vertex")
    };
    app.selection.replace(SelItem::Vertex(corner));
    assert!(app.can_fillet(), "corner vertex trims");
    assert_eq!(app.chamfer_selected(16.0), Damage::Edited);
    assert_eq!(app.map.as_ref().expect("map").lines.len(), 3);
    app.undo();

    let map = app.map.as_ref().expect("map");
    let corner = map
        .vertices
        .iter()
        .find(|(_, p)| (p.x, p.y) == (64.0, 0.0))
        .map(|(k, _)| k)
        .expect("corner restored");
    app.selection.replace(SelItem::Vertex(corner));
    assert_eq!(app.fillet_selected(16.0, 4), Damage::Edited);
    assert_eq!(app.map.as_ref().expect("map").lines.len(), 6);
    assert_eq!(app.selected_lines().len(), 4, "new arc lines selected");
}

#[test]
fn set_line_two_sided_flag_syncs_back_side() {
    let mut app = app_with_map();
    let map = app.map.as_ref().expect("map");
    let one_sided = map
        .lines
        .iter()
        .find(|(_, l)| l.back.is_none())
        .map(|(k, _)| k)
        .expect("fixture has one-sided lines");
    let existing = &map.lines[one_sided];
    let mut edited = LineDef {
        v1: existing.v1,
        v2: existing.v2,
        flags: existing.flags | LineFlags::TWO_SIDED,
        special: 0,
        tag: 0,
        front: existing.front,
        back: None,
    };
    app.set_line(one_sided, edited);
    let line = &app.map.as_ref().expect("map").lines[one_sided];
    assert!(line.back.is_some(), "back side cloned from front");

    edited = LineDef {
        v1: line.v1,
        v2: line.v2,
        flags: line.flags.difference(LineFlags::TWO_SIDED),
        special: 0,
        tag: 0,
        front: line.front,
        back: line.back,
    };
    app.set_line(one_sided, edited);
    let line = &app.map.as_ref().expect("map").lines[one_sided];
    assert!(line.back.is_none(), "back side dropped with the flag");
}

#[test]
fn apply_lines_batch_one_undo_step() {
    let mut app = app_with_map();
    let keys: Vec<LineKey> = {
        let map = app.map.as_ref().expect("map loaded");
        map.lines.keys().take(3).collect()
    };
    let orig_tags: Vec<i32> = {
        let map = app.map.as_ref().expect("map loaded");
        keys.iter().map(|&k| map.lines[k].tag).collect()
    };
    app.apply_lines(&keys, |old| LineDef {
        v1: old.v1,
        v2: old.v2,
        flags: old.flags,
        special: old.special,
        tag: 999,
        front: old.front,
        back: old.back,
    });
    let map = app.map.as_ref().expect("map");
    for &k in &keys {
        assert_eq!(map.lines[k].tag, 999);
    }
    app.undo();
    let map = app.map.as_ref().expect("map");
    for (&k, &tag) in keys.iter().zip(&orig_tags) {
        assert_eq!(map.lines[k].tag, tag, "undo reverts line {k:?}");
    }
    assert_eq!(app.undo(), Damage::None, "batch was exactly one step");
}

#[test]
fn copy_paste_appends_offset_geometry() {
    let mut app = app_with_map();
    let (lines0, things0, sectors0) = {
        let m = app.map.as_ref().expect("map");
        (m.lines.len(), m.things.len(), m.sectors.len())
    };
    let (lk, tk) = {
        let m = app.map.as_ref().expect("map");
        (line_key(m, 0), thing_key(m, 0))
    };
    app.selection.push(SelItem::Line(lk));
    app.selection.push(SelItem::Thing(tk));
    assert_eq!(app.copy_selection(), Damage::None);
    assert_eq!(app.clipboard.fragment.lines.len(), 1);
    assert_eq!(app.clipboard.fragment.things.len(), 1);

    app.cursor_world = [
        app.clipboard.anchor[0] + 256.0,
        app.clipboard.anchor[1] + 256.0,
    ];
    assert_eq!(app.paste(), Damage::Edited);
    let m = app.map.as_ref().expect("map");
    assert_eq!(m.lines.len(), lines0 + 1);
    assert_eq!(m.things.len(), things0 + 1);
    assert!(m.sectors.len() > sectors0);
    let orig = *m.things.values().next().expect("first thing");
    let pasted = *m.things.values().last().expect("pasted thing");
    assert!(pasted.x != orig.x || pasted.y != orig.y);
    assert_eq!(pasted.kind, orig.kind);

    app.undo();
    let m = app.map.as_ref().expect("map");
    assert_eq!(m.lines.len(), lines0);
    assert_eq!(m.things.len(), things0);
}

#[test]
fn cut_copies_then_deletes() {
    let mut app = app_with_map();
    let things0 = app.map.as_ref().expect("map").things.len();
    let tk = thing_key(app.map.as_ref().expect("map"), 0);
    app.selection.push(SelItem::Thing(tk));
    assert_eq!(app.cut_selection(), Damage::Edited);
    assert_eq!(app.clipboard.fragment.things.len(), 1);
    assert_eq!(app.map.as_ref().expect("map").things.len(), things0 - 1);
}

#[test]
fn copy_paste_sector_record_via_unified_clipboard() {
    let mut app = empty_app();
    app.camera.set_zoom(1.0);
    draw_room(
        &mut app,
        &[[0.0, 0.0], [128.0, 0.0], [128.0, 128.0], [0.0, 128.0]],
    );
    draw_line(&mut app, [64.0, 0.0], [64.0, 128.0]);
    let src = app.sector_under([32.0, 64.0]).expect("left sector");
    let dst = app.sector_under([96.0, 64.0]).expect("right sector");
    assert_ne!(src, dst);
    {
        let map = app.map.as_mut().expect("map");
        assert_eq!(map.sectors.len(), 2);
        map.sectors[src].light_level = 200;
        map.sectors[dst].light_level = 80;
    }
    app.selection.replace(SelItem::Sector(src));
    app.current_sector = Some(src);
    app.copy_selection();
    assert_eq!(app.clipboard.sectors.len(), 1);
    assert_eq!(app.clipboard.sectors[0].light_level, 200);
    app.cursor_world = [96.0, 64.0];
    app.paste();
    let map = app.map.as_ref().expect("map");
    assert_eq!(
        map.sectors[dst].light_level, 200,
        "record applied to dst sector"
    );
    assert!(app.clipboard.is_empty(), "clipboard cleared after paste");
}

fn app_with_verts(verts: &[[f32; 2]], lines: &[(u32, u32)]) -> LevelEditorState {
    let side = DenseSideDef {
        x_offset: 0,
        y_offset: 0,
        top_tex: Name8::EMPTY,
        bottom_tex: Name8::EMPTY,
        middle_tex: Name8::EMPTY,
        sector: None,
    };
    let mut map = EditorMap::from_dense(DenseMap {
        vertices: verts
            .iter()
            .map(|p| Vertex {
                x: p[0],
                y: p[1],
            })
            .collect(),
        lines: lines
            .iter()
            .map(|&(v1, v2)| DenseLineDef {
                v1,
                v2,
                flags: LineFlags::empty(),
                special: 0,
                tag: 0,
                front: side,
                back: None,
            })
            .collect(),
        sectors: Vec::new(),
        things: Vec::new(),
        required_wads: Vec::new(),
    })
    .expect("fixture refs valid");
    // Sector sides through the real builder (hand-assigned winding breaks sector_at).
    let drawn: Vec<LineKey> = map.lines.keys().collect();
    derive_sectors(&mut map, &drawn, default_sector());
    let mut app = LevelEditorState::new();
    app.load_map(map, "WELD");
    app.set_tool(Tool::Select(SelectMode::All));
    app
}

#[test]
fn weld_two_close_vertices_collapses_line() {
    let mut app = app_with_verts(
        &[[0.0, 0.0], [4.0, 0.0], [2.0, 40.0]],
        &[(0, 1), (1, 2), (2, 0)],
    );
    let (k0, k1) = {
        let map = app.map.as_ref().expect("map");
        (vert_key(map, 0), vert_key(map, 1))
    };
    app.selection.push(SelItem::Vertex(k0));
    app.selection.push(SelItem::Vertex(k1));
    assert_eq!(app.weld_selected(), Damage::Edited);
    let map = app.map.as_ref().expect("map");
    assert_eq!(map.vertices.len(), 2, "two base corners welded to one");
    assert_eq!(map.lines.len(), 1, "collapsed base removed, sides deduped");
    let welded = map
        .vertices
        .values()
        .find(|v| v.y == 0.0)
        .expect("base vertex");
    assert_eq!((welded.x, welded.y), (2.0, 0.0), "centroid of the corners");
}

#[test]
fn weld_far_apart_vertices_is_noop() {
    let mut app = app_with_verts(&[[0.0, 0.0], [100.0, 0.0]], &[(0, 1)]);
    let (k0, k1) = {
        let map = app.map.as_ref().expect("map");
        (vert_key(map, 0), vert_key(map, 1))
    };
    app.selection.push(SelItem::Vertex(k0));
    app.selection.push(SelItem::Vertex(k1));
    assert_eq!(
        app.weld_selected(),
        Damage::None,
        "both outside weld radius"
    );
    assert_eq!(app.map.as_ref().expect("map").vertices.len(), 2);
}

#[test]
fn add_sector_fills_drawn_void_room() {
    let mut app = empty_app();
    app.camera.set_zoom(1.0);
    draw_room(
        &mut app,
        &[[0.0, 0.0], [128.0, 0.0], [128.0, 128.0], [0.0, 128.0]],
    );
    app.set_tool(Tool::Select(SelectMode::Sector));
    click_at(&mut app, [64.0, 64.0]);
    app.delete_selection();
    assert!(
        app.sector_under([64.0, 64.0]).is_none(),
        "interior now void"
    );
    assert!(app.can_add_sector([64.0, 64.0]));
    assert_eq!(app.add_sector_at([64.0, 64.0]), Damage::Edited);
    assert!(app.sector_under([64.0, 64.0]).is_some(), "sector re-added");
}

#[test]
fn merge_sectors_requires_adjacency() {
    let mut app = empty_app();
    app.camera.set_zoom(1.0);
    draw_room(
        &mut app,
        &[[0.0, 0.0], [128.0, 0.0], [128.0, 128.0], [0.0, 128.0]],
    );
    draw_line(&mut app, [64.0, 0.0], [64.0, 128.0]);
    assert_eq!(app.map.as_ref().expect("map").sectors.len(), 2);
    let left = app.sector_under([32.0, 64.0]).expect("left sector");
    let right = app.sector_under([96.0, 64.0]).expect("right sector");
    app.selection.push(SelItem::Sector(left));
    app.selection.push(SelItem::Sector(right));
    assert!(app.can_merge_sectors(), "adjacent across the divider");
    assert_eq!(app.merge_selected_sectors(), Damage::Edited);
    assert_eq!(app.map.as_ref().expect("map").sectors.len(), 1, "merged");
}

#[test]
fn select_sector_mode_shift_accumulates() {
    let mut app = empty_app();
    app.camera.set_zoom(1.0);
    draw_room(
        &mut app,
        &[[0.0, 0.0], [128.0, 0.0], [128.0, 128.0], [0.0, 128.0]],
    );
    draw_line(&mut app, [64.0, 0.0], [64.0, 128.0]);
    app.set_tool(Tool::Select(SelectMode::Sector));
    click_at(&mut app, [32.0, 64.0]);
    assert_eq!(app.selected_sectors().len(), 1);
    shift_click_at(&mut app, [96.0, 64.0]);
    assert_eq!(app.selected_sectors().len(), 2, "both sectors selected");
    assert_eq!(app.highlighted_sectors().len(), 2, "both highlighted");
}

#[test]
fn selitem_sector_drives_selected_sectors() {
    use editor_core::ArenaKey as _;
    let mut app = empty_app();
    let (s2, s5) = (SectorKey::new(2, 0), SectorKey::new(5, 0));
    app.selection.push(SelItem::Sector(s2));
    app.selection.push(SelItem::Line(LineKey::new(0, 0)));
    app.selection.push(SelItem::Sector(s5));
    assert_eq!(
        app.selected_sectors(),
        vec![s2, s5],
        "sector items, in order"
    );
    app.selection.toggle(SelItem::Sector(s2));
    assert_eq!(app.selected_sectors(), vec![s5], "toggle removes a sector");
}

#[test]
fn all_mode_shift_accumulates_sectors() {
    let mut app = empty_app();
    app.camera.set_zoom(1.0);
    draw_room(
        &mut app,
        &[[0.0, 0.0], [128.0, 0.0], [128.0, 128.0], [0.0, 128.0]],
    );
    draw_line(&mut app, [64.0, 0.0], [64.0, 128.0]);
    app.set_tool(Tool::Select(SelectMode::All));
    click_at(&mut app, [32.0, 64.0]);
    assert_eq!(app.selected_sectors().len(), 1);
    shift_click_at(&mut app, [96.0, 64.0]);
    assert_eq!(
        app.selected_sectors().len(),
        2,
        "All mode accumulates sectors"
    );
}

#[test]
fn delete_removes_selected_sector() {
    let mut app = empty_app();
    app.camera.set_zoom(1.0);
    draw_room(
        &mut app,
        &[[0.0, 0.0], [128.0, 0.0], [128.0, 128.0], [0.0, 128.0]],
    );
    draw_line(&mut app, [64.0, 0.0], [64.0, 128.0]);
    app.set_tool(Tool::Select(SelectMode::All));
    click_at(&mut app, [32.0, 64.0]);
    assert_eq!(app.selected_sectors().len(), 1);
    let before = app.map.as_ref().unwrap().sectors.len();
    app.delete_selection();
    assert_eq!(
        app.map.as_ref().unwrap().sectors.len(),
        before - 1,
        "Delete removed the sector"
    );
}

#[test]
fn can_paste_sector_into_empty_enclosure() {
    let mut app = empty_app();
    app.camera.set_zoom(1.0);
    draw_room(
        &mut app,
        &[[0.0, 0.0], [128.0, 0.0], [128.0, 128.0], [0.0, 128.0]],
    );
    let room = app
        .map
        .as_ref()
        .expect("map")
        .sectors
        .keys()
        .next()
        .expect("room");
    app.selection.replace(SelItem::Sector(room));
    app.current_sector = Some(room);
    app.copy_selection();
    app.cursor_world = [64.0, 64.0];
    assert!(
        app.can_paste(),
        "sector clipboard pastes when cursor over sector"
    );
    app.delete_selection(); // interior is now void but still enclosed
    assert!(
        app.can_paste(),
        "sector clipboard pastes into empty enclosure"
    );
}

#[test]
fn paste_sector_into_enclosure_is_one_undo_step() {
    let mut app = empty_app();
    app.camera.set_zoom(1.0);
    draw_room(
        &mut app,
        &[[0.0, 0.0], [128.0, 0.0], [128.0, 128.0], [0.0, 128.0]],
    );
    let room = app
        .map
        .as_ref()
        .expect("map")
        .sectors
        .keys()
        .next()
        .expect("room");
    app.map.as_mut().expect("map").sectors[room].light_level = 200;
    app.selection.replace(SelItem::Sector(room));
    app.current_sector = Some(room);
    app.copy_selection();
    app.cursor_world = [64.0, 64.0];
    app.delete_selection(); // interior is now void but still enclosed

    assert_eq!(app.paste(), Damage::Edited);
    let sector = app.sector_under([64.0, 64.0]).expect("sector pasted");
    assert_eq!(
        app.map.as_ref().expect("map").sectors[sector].light_level,
        200,
        "clipboard record applied to the added sector"
    );

    assert_eq!(app.undo(), Damage::Edited, "single undo reverts the paste");
    assert!(
        app.sector_under([64.0, 64.0]).is_none(),
        "one undo removes the pasted sector entirely"
    );
}
