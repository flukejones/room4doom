//! Bridge editor damage to the wgpu canvas: rebuild geometry caches + atlases on
//! content change, upload per-sector brightness, and push a GPU frame.
//!
//! [`apply_damage`] is the sole entry point. `Geometry` rebuilds and uploads the
//! whole-map mesh; `Patch` rewrites only changed element slots; `View`/`Repaint`
//! regenerate the grid and repaint the cached mesh. Animated sector lights update
//! only the brightness buffer at 35 Hz; map render state is reset on map load.

use std::cell::RefCell;
use std::rc::Rc;
use std::time::Duration;

use slint::{ComponentHandle as _, Timer};

use editor_core::Thing;

use crate::generated::{CanvasController, EditorWindow};
use crate::render::atlas;
use crate::render::camera3d::Camera;
use crate::render::frame::{self, FrameInput, thing_world_half_extent};
use crate::render::triangulate;
use crate::render::wgpu::MapFrame;
use crate::state::{ChangedElems, Damage, SectorFill, SharedState};
use crate::views::view_canvas::start_cam_ease;
use crate::{bsp_anim, defaults, gfx, light_anim};

/// Light-effect animation runs at the Doom tic rate (35 Hz).
const LIGHT_TIC_MS: u64 = 1000 / 35;

thread_local! {
    /// `thread_local` not `SharedState` field: avoids borrow-panic if the tic closure fires during `start`.
    static LIGHT_TIMER: Timer = Timer::default();
}

/// Stop the light-effect timer (map reset / fill-mode leaves Texture).
pub(crate) fn stop_light_timer() {
    LIGHT_TIMER.with(Timer::stop);
}

pub(crate) fn apply_damage(ui: &EditorWindow, shared: &Rc<RefCell<SharedState>>, damage: Damage) {
    // BSP build overlay is pinned to the view it started in; any change desyncs it.
    if !matches!(damage, Damage::None) {
        bsp_anim::clear(ui, shared);
    }
    match &damage {
        Damage::None => return,
        Damage::Overlay => {
            set_edit_preview(ui, shared);
            return;
        }
        Damage::View | Damage::Repaint => {
            regrid_and_paint(ui, shared);
        }
        Damage::Patch(changed) => {
            patch_elements(ui, shared, changed);
        }
        Damage::Geometry => {
            {
                let state = &mut *shared.borrow_mut();
                rebuild_map_caches(state);
                state.map_render.panels_key = None;
            }
            push_wgpu_frame(ui, shared);
        }
    }
    // Pan/zoom must not rebuild the light list — that re-spawns lights at full bright and flickers.
    let lights_may_change = matches!(damage, Damage::Geometry | Damage::Repaint);
    refresh_light_anim(ui, shared, lights_may_change);
    if shared.borrow().app.camera.needs_ease() {
        start_cam_ease(ui, shared);
    }
}

/// Patch changed GPU slots in place (non-topological edit). Sector-flat changes also refresh atlases.
fn patch_elements(ui: &EditorWindow, shared: &Rc<RefCell<SharedState>>, changed: &ChangedElems) {
    if !shared.borrow().wgpu.is_ready() {
        return;
    }
    if !changed.sector_flats.is_empty() {
        let state = &mut *shared.borrow_mut();
        state.map_render.panels_key = None;
        refresh_atlases(state);
    }
    let (pw, ph) = physical_size(ui, shared);
    {
        let state = shared.borrow();
        if state.app.map.is_none() {
            return;
        }
        let highlighted = state.app.highlighted_sectors();
        let visible = |t: &Thing| state.app.skill_filter.allows(t.options);
        let input = frame_input(&state, pixel_ratio(&state, pw), &highlighted, &visible);
        for &i in &changed.lines {
            let (inst, normal) = frame::line_instances(&input, i);
            state.wgpu.patch_line(i, inst, normal);
        }
        for &i in &changed.verts {
            state.wgpu.patch_vert(i, frame::vert_instance(&input, i));
        }
        for &i in &changed.things {
            state.wgpu.patch_thing(i, frame::thing_instance(&input, i));
        }
        for &s in &changed.sectors {
            state
                .wgpu
                .patch_sector_attr(s, frame::sector_attr(&input, s));
            state.wgpu.patch_sector_3d(s, frame::sector_3d(&input, s));
        }
        if !changed.sectors.is_empty() {
            upload_brightness(&state);
        }
        state.wgpu.set_grid_style(frame::grid_style(&input));
        state.wgpu.set_grid_z(input.grid_z);
        state.wgpu.set_overlay(&[], &[]);
    }
    paint(ui, shared, pw, ph);
}

fn physical_size(ui: &EditorWindow, shared: &Rc<RefCell<SharedState>>) -> (u32, u32) {
    let scale = ui.window().scale_factor().max(1.0);
    let [w, h] = shared.borrow().app.camera.viewport();
    (
        (w * scale).round().max(1.0) as u32,
        (h * scale).round().max(1.0) as u32,
    )
}

/// Physical/logical pixel ratio for sizing screen-space primitives. 1.0 when no viewport yet.
fn pixel_ratio(state: &SharedState, pw: u32) -> f32 {
    if state.app.camera.viewport()[0] > 0.0 {
        pw as f32 / state.app.camera.viewport()[0]
    } else {
        1.0
    }
}

/// Full rebuild on geometry change: decode icons, build atlases + brightness,
/// upload the whole-map mesh, regenerate grid, paint. No-op until device ready.
pub(crate) fn push_wgpu_frame(ui: &EditorWindow, shared: &Rc<RefCell<SharedState>>) {
    if !shared.borrow().wgpu.is_ready() {
        return;
    }
    {
        let state = &mut *shared.borrow_mut();
        ensure_thing_sprites(state);
        refresh_atlases(state);
    }
    let (pw, ph) = physical_size(ui, shared);
    {
        let state = &mut *shared.borrow_mut();
        let pr = pixel_ratio(state, pw);
        let mesh = build_map_mesh(state, pr);
        state.wgpu.set_sector_data(
            &compute_brightness(state),
            &mesh.sector_attrs,
            &mesh.sector3d,
        );
        state.wgpu.upload_map(&mesh);
        set_grid_params(state, pr);
        state.wgpu.set_fill_mode(state.app.sector_fill);
        state.wgpu.set_overlay(&[], &[]);
        state.app.surface_mesh = mesh.surface3d; // retained for ray-vs-mesh picking
        state.app.rebuild_bvh();
    }
    paint(ui, shared, pw, ph);
}

/// Update grid uniforms for the current view and repaint. Grid is procedural GPU; no mesh rebuild.
pub(crate) fn regrid_and_paint(ui: &EditorWindow, shared: &Rc<RefCell<SharedState>>) {
    if !shared.borrow().wgpu.is_ready() {
        return;
    }
    let (pw, ph) = physical_size(ui, shared);
    {
        let state = shared.borrow();
        set_grid_params(&state, pixel_ratio(&state, pw));
    }
    paint(ui, shared, pw, ph);
}

/// Upload the current `app.overlay` to the GPU overlay layer and repaint.
fn set_edit_preview(ui: &EditorWindow, shared: &Rc<RefCell<SharedState>>) {
    if !shared.borrow().wgpu.is_ready() {
        return;
    }
    let (pw, _) = physical_size(ui, shared);
    {
        let state = shared.borrow();
        let pr = pixel_ratio(&state, pw);
        let z = state.app.camera.grid_z();
        let (lines, markers) = frame::build_preview(&state.app.overlay, &state.app.style, pr, z);
        state.wgpu.set_overlay(&lines, &markers);
    }
    repaint_canvas(ui, shared);
}

/// Repaint the cached mesh + grid without rebuilding anything.
pub(crate) fn repaint_canvas(ui: &EditorWindow, shared: &Rc<RefCell<SharedState>>) {
    if !shared.borrow().wgpu.is_ready() {
        return;
    }
    let (pw, ph) = physical_size(ui, shared);
    paint(ui, shared, pw, ph);
}

/// Terminal step of every render path: paint the cached mesh + grid with the camera.
fn paint(ui: &EditorWindow, shared: &Rc<RefCell<SharedState>>, pw: u32, ph: u32) {
    let camera = shared.borrow().app.render_camera();
    {
        let state = shared.borrow();
        state.wgpu.set_grid_on_top(state.app.editing_active());
        state.wgpu.set_overlays_visible(state.app.overlays_visible);
    }
    if let Some(img) = shared.borrow().wgpu.paint(camera, pw, ph) {
        let canvas = ui.global::<CanvasController>();
        canvas.set_wgpu_img(img);
        canvas.set_wgpu_mode(true);
    }
}

/// Re-upload the GPU atlases if the generation moved; no-op when unchanged.
fn refresh_atlases(state: &mut SharedState) {
    if state.app.map.is_none() || !state.ensure_assets() {
        return;
    }
    let map_wad = state.texture_wad();
    let SharedState {
        app,
        assets,
        thing_sprites,
        wgpu,
        map_render,
        wad_data,
        ..
    } = state;
    let (Some(map), Some(assets), Some(sprites), Some(wad)) = (
        app.map.as_ref(),
        assets.as_mut(),
        thing_sprites.as_ref(),
        wad_data.as_ref(),
    ) else {
        return;
    };
    assets.set_map_wad(&map_wad);
    let names = atlas::collect_wall_names(assets, map);
    assets.ensure_composed(&names, wad);
    let assets = &*assets;

    let key = atlas::content_key(assets, map, sprites);
    if map_render.atlas_key == Some(key) {
        atlas::remap_sector_tiles(map, &mut map_render.atlas_maps);
        return;
    }
    let (data, maps) = atlas::build(assets, map, sprites, key);
    wgpu.set_atlases(&data);
    map_render.atlas_maps = maps;
    map_render.atlas_key = Some(key);
}

/// Per-sector brightness scalar (0..1), applying active light effects.
fn compute_brightness(state: &SharedState) -> Vec<f32> {
    let Some(map) = &state.app.map else {
        return Vec::new();
    };
    let mut brightness: Vec<f32> = map
        .sectors
        .iter()
        .map(|s| s.light_level.clamp(0, 255) as f32 / 255.0)
        .collect();
    if state.app.sector_fill == SectorFill::Texture {
        for light in &state.map_render.light_anim {
            if let Some(b) = brightness.get_mut(light.sector) {
                *b = light.current.clamp(0, 255) as f32 / 255.0;
            }
        }
    }
    brightness
}

fn upload_brightness(state: &SharedState) {
    state.wgpu.update_brightness(&compute_brightness(state));
}

/// Build the whole-map mesh (camera-invariant; `paint` sets the camera at draw time).
fn build_map_mesh(state: &SharedState, pixel_ratio: f32) -> MapFrame {
    if state.app.map.is_none() {
        return MapFrame::default();
    }
    let skill = state.app.skill_filter;
    let highlighted = state.app.highlighted_sectors();
    let visible = |t: &Thing| skill.allows(t.options);
    let input = frame_input(state, pixel_ratio, &highlighted, &visible);
    frame::build_map_geometry(&input)
}

/// Push grid style + plane height for the current view. No-op without a loaded map.
fn set_grid_params(state: &SharedState, pixel_ratio: f32) {
    if state.app.map.is_none() {
        return;
    }
    let visible = |_: &Thing| true;
    let input = frame_input(state, pixel_ratio, &[], &visible);
    state.wgpu.set_grid_style(frame::grid_style(&input));
    state.wgpu.set_grid_z(input.grid_z);
}

fn frame_input<'a>(
    state: &'a SharedState,
    pixel_ratio: f32,
    highlighted: &'a [u32],
    thing_visible: &'a dyn Fn(&Thing) -> bool,
) -> FrameInput<'a> {
    FrameInput {
        map: state.app.map.as_ref().expect("checked by caller"),
        tris: &state.map_render.sector_tris,
        zoom: state.app.camera.zoom_level(),
        pixel_ratio,
        style: &state.app.style,
        selection: &state.app.selection,
        grid: state.app.grid,
        fill: state.app.sector_fill,
        selected_sectors: highlighted,
        thing_visible,
        thing_extents: &state.app.thing_extents,
        thing_colors: &state.app.thing_colors,
        atlas: &state.map_render.atlas_maps,
        thing_radius: &defaults::thing_radius,
        sector_gradient: state.prefs.sector_gradient.gradient(),
        highlight_unenclosed: state.app.highlight_unenclosed,
        mode: state.app.camera.mode(),
        grid_z: state.app.camera.grid_z(),
        // Wireframe verts attach to bordering floor heights; other modes ride the grid plane.
        vert_z: if state.app.sector_fill == SectorFill::None {
            &state.map_render.vertex_floor_z
        } else {
            &[]
        },
    }
}

/// Top-down ortho camera for PNG export / headless render.
pub(crate) fn export_camera(centre: [f32; 2], scale: f32, _w: f32, h: f32) -> Camera {
    let mut cam = Camera::default();
    cam.look_down_at([centre[0], centre[1], 0.0]);
    cam.set_ortho_height(h / scale.max(1e-6));
    cam
}

fn rebuild_map_caches(state: &mut SharedState) {
    let Some(map) = &state.app.map else { return };
    state.map_render.sector_tris = triangulate::build_sector_tris(map);
    state.map_render.vertex_floor_z = frame::build_vertex_floor_z(map);
}

/// Decode not-yet-cached thing icons. Project `things.dsp` icon overrides the
/// built-in sprite prefix; kinds with neither fall back to a colour square.
fn ensure_thing_sprites(state: &mut SharedState) {
    if state.app.map.is_none() || !state.ensure_assets() {
        return;
    }
    let SharedState {
        wad_data,
        project,
        assets,
        thing_sprites,
        app,
        ..
    } = state;
    let Some(map) = &app.map else { return };
    let wad = wad_data.as_ref().expect("ensured by ensure_assets");
    let palette = *assets.as_ref().expect("ensured above").palette();
    // Insert cache even for maps with no things so `refresh_atlases` can build wall/flat atlas.
    let cache = thing_sprites.get_or_insert_with(Default::default);

    let mut kinds: Vec<i32> = map.things.iter().map(|t| t.kind).collect();
    kinds.sort_unstable();
    kinds.dedup();
    for &kind in &kinds {
        let project_icon = project.as_ref().and_then(|p| {
            p.things
                .iter()
                .find(|t| t.value == kind)
                .map(|t| t.icon)
                .filter(|icon| !icon.is_empty())
        });
        let prefix = defaults::DEFAULT_THINGS
            .iter()
            .find(|t| t.kind == kind)
            .map(|t| t.sprite)
            .unwrap_or("");
        let source = if let Some(icon) = &project_icon {
            gfx::SpriteSource::Patch(icon.as_str())
        } else if !prefix.is_empty() {
            gfx::SpriteSource::Prefix(prefix)
        } else {
            gfx::SpriteSource::None
        };
        gfx::ensure_thing_sprite(cache, wad, &palette, kind, source);
    }

    app.thing_extents.clear();
    for &kind in &kinds {
        let fallback = defaults::thing_radius(kind);
        let extent = thing_world_half_extent(Some(cache), kind, fallback);
        app.thing_extents.insert(kind, extent);
    }
}

fn light_anim_active(state: &SharedState) -> bool {
    state.prefs.light_anim
        && state.app.sector_fill == SectorFill::Texture
        && state.bsp_anim.is_none()
        && !state.map_render.light_anim.is_empty()
}

/// Reconcile light-effect list and start/stop the 35 Hz timer.
/// When `set_may_change` is false (pan/zoom), keep existing list to preserve phases.
fn refresh_light_anim(ui: &EditorWindow, shared: &Rc<RefCell<SharedState>>, set_may_change: bool) {
    if set_may_change {
        let state = &mut *shared.borrow_mut();
        let next = match (&state.app.map, state.app.sector_fill) {
            (Some(map), SectorFill::Texture) => light_anim::build(map),
            _ => Vec::new(),
        };
        let same_set = next.len() == state.map_render.light_anim.len()
            && next
                .iter()
                .zip(&state.map_render.light_anim)
                .all(|(a, b)| a.sector == b.sector);
        if !same_set {
            state.map_render.light_anim = next;
        }
    }

    if !light_anim_active(&shared.borrow()) {
        stop_light_anim(ui, shared);
        return;
    }
    if LIGHT_TIMER.with(Timer::running) {
        return;
    }
    let weak = ui.as_weak();
    let s = shared.clone();
    LIGHT_TIMER.with(|t| {
        t.start(
            slint::TimerMode::Repeated,
            Duration::from_millis(LIGHT_TIC_MS),
            move || {
                let Some(ui) = weak.upgrade() else { return };
                light_tic(&ui, &s);
            },
        );
    });
}

/// Stop the animation timer and revert the canvas to authored light levels.
fn stop_light_anim(ui: &EditorWindow, shared: &Rc<RefCell<SharedState>>) {
    stop_light_timer();
    let had_lights = !shared.borrow().map_render.light_anim.is_empty();
    if had_lights {
        shared.borrow_mut().map_render.light_anim.clear();
        let state = shared.borrow();
        upload_brightness(&state);
        drop(state);
        repaint_canvas(ui, shared);
    }
}

fn light_tic(ui: &EditorWindow, shared: &Rc<RefCell<SharedState>>) {
    if !light_anim_active(&shared.borrow()) {
        stop_light_anim(ui, shared);
        return;
    }
    {
        let state = &mut *shared.borrow_mut();
        light_anim::tic(&mut state.map_render.light_anim);
    }
    {
        let state = shared.borrow();
        upload_brightness(&state);
    }
    repaint_canvas(ui, shared);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::level_editor::LevelEditorState;
    use wad::WadData;

    /// `ensure_thing_sprites` early-returned on maps with no things, blocking `refresh_atlases` (needs the sprite cache).
    /// Result: atlas empty → textured 3D surfaces sampled nothing, canvas showed only lines.
    #[test]
    fn map_without_things_still_textures_walls_and_floors() {
        let wad = WadData::new(&test_utils::doom1_wad_path());
        let mut map = editor_core::import_wad_map(&wad, "E1M1").expect("E1M1 imports");
        map.things.clear();

        let mut app = LevelEditorState::new();
        app.load_map(map, "E1M1");
        let mut state =
            SharedState::new(app, Some(test_utils::doom1_wad_path()), Default::default());

        ensure_thing_sprites(&mut state);
        assert!(state.thing_sprites.is_some(), "sprite cache inserted");

        refresh_atlases(&mut state);
        assert!(
            !state.map_render.atlas_maps.wall_rects.is_empty(),
            "wall atlas built (walls texture in 3D)"
        );
        assert!(
            state
                .map_render
                .atlas_maps
                .sector_tile
                .iter()
                .any(Option::is_some),
            "flat tiles built (floors fill in 3D)"
        );
    }
}
