//! Bridge editor damage to the wgpu canvas by reconciliation: diff the map against the last-synced snapshot, derive the dirty sectors/lines/verts/things, and patch exactly those GPU slots and surface spans in place. [`apply_damage`] is the sole entry point: `Edited` runs the reconciler, `View`/`Repaint` regenerate the grid and repaint the cached mesh. The only full build is map load (`last_synced == None`) and buffer-capacity growth (re-upload from the CPU mirror, no re-emission). Animated sector lights update only the brightness buffer at 35 Hz; map render state is reset on map load.

use std::cell::RefCell;
use std::collections::HashSet;
use std::mem;
use std::rc::Rc;
use std::time::Duration;

use bytemuck::Zeroable as _;
use slint::{ComponentHandle as _, Timer};

use editor_core::{
    Arena, ArenaKey, EditorMap, LineDef, LineKey, Name8, SectorKey, SideDef, Thing, ThingKey,
    VertKey,
};

use crate::generated::{CanvasController, EditorWindow};
use crate::level_editor::bvh::MeshBvh;
use crate::level_editor::view::vert_pair;
use crate::level_editor::{LevelEditorState, thing_leaves};
use crate::render::atlas;
use crate::render::camera3d::Camera;
use crate::render::frame::{self, FrameInput, thing_world_half_extent};
use crate::render::frame3d::{SpanPatch, line_wall_verts, sector_surface_verts};
use crate::render::triangulate::{self, retriangulate_sectors};
use crate::render::wgpu::{LineInst, MarkerInst, Sector3D, SectorAttr, ThingInst};
use crate::state::{Damage, MapRender, SectorFill, SelItem, SharedState};
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
    match damage {
        Damage::None => return,
        Damage::Overlay => {
            set_edit_preview(ui, shared);
            return;
        }
        // A grid-plane move under View/Repaint re-emits the plane-riding instances via reconcile.
        Damage::View | Damage::Repaint if !plane_moved_since_sync(shared) => {
            upload_edit_preview(ui, shared);
            regrid_and_paint(ui, shared);
        }
        Damage::View | Damage::Repaint | Damage::Edited => {
            upload_edit_preview(ui, shared);
            reconcile(ui, shared);
        }
        Damage::Restyle => {
            shared.borrow_mut().map_render.last_synced = None;
            upload_edit_preview(ui, shared);
            reconcile(ui, shared);
        }
    }
    // Pan/zoom never rebuilds the light list (re-spawning flickers); edits rebuild it only when the reconcile touched light inputs.
    let lights_may_change = match damage {
        Damage::Edited => mem::take(&mut shared.borrow_mut().map_render.light_set_dirty),
        Damage::Restyle | Damage::Repaint => true,
        _ => false,
    };
    refresh_light_anim(ui, shared, lights_may_change);
    if shared.borrow().app.camera.needs_ease() {
        start_cam_ease(ui, shared);
    }
}

/// True when the grid plane moved since the last sync — the plane-riding layers are stale.
fn plane_moved_since_sync(shared: &Rc<RefCell<SharedState>>) -> bool {
    let state = shared.borrow();
    state.map_render.last_synced.is_some()
        && (state.app.camera.grid_z() - state.map_render.last_grid_z).abs() > f32::EPSILON
}

/// Diff map + selection against the last-synced snapshot and patch the GPU; first sync = full build.
fn reconcile(ui: &EditorWindow, shared: &Rc<RefCell<SharedState>>) {
    if !shared.borrow().wgpu.is_ready() {
        // The edit still happened: derived caches the reconciler owns must not go stale.
        let state = &mut *shared.borrow_mut();
        state.map_render.light_set_dirty = true;
        state.app.snap_index = None;
        return;
    }
    if shared.borrow().app.map.is_none() {
        shared.borrow_mut().map_render.last_synced = None;
        return;
    }
    if shared.borrow().map_render.last_synced.is_none() {
        push_wgpu_frame(ui, shared);
        return;
    }
    let mut plan = {
        let state = shared.borrow();
        let map = state.app.map.as_ref().expect("checked above");
        let old = state
            .map_render
            .last_synced
            .as_ref()
            .expect("checked above");
        plan_reconcile(old, map)
    };
    // Atlas inputs (flat/texture names, thing kinds, asset edits) unchanged → skip all atlas work; project thing-icon overrides are read by ensure_thing_sprites but have no UI mutation path — one added must also open this gate (or bump atlas_key).
    let atlas_dirty = plan.flats_changed
        || plan.wall_tex_changed
        || plan.thing_kinds_changed
        || asset_gen_moved(&shared.borrow());
    let repacked = atlas_dirty && {
        let state = &mut *shared.borrow_mut();
        if plan.thing_kinds_changed {
            ensure_thing_sprites(state);
        }
        let dirty_sectors: Vec<SectorKey> = plan
            .sectors_changed
            .iter()
            .chain(&plan.sectors_removed)
            .copied()
            .collect();
        refresh_atlases(state, Some(&dirty_sectors))
    };
    if repacked {
        // Every wall span bakes atlas rects; a repack moves them all.
        let state = shared.borrow();
        let map = state.app.map.as_ref().expect("checked above");
        plan.rewall = map.lines.keys().collect();
    }
    let (pw, ph) = physical_size(ui, shared);
    {
        let state = &mut *shared.borrow_mut();
        let pr = pixel_ratio(state, pw);
        apply_reconcile(state, pr, &plan);
        state.map_render.panels_key = None;
    }
    paint(ui, shared, pw, ph);
}

/// True when the asset generation moved since the last atlas build (texture-editor edits).
fn asset_gen_moved(state: &SharedState) -> bool {
    state.assets.as_ref().map(|a| a.generation()) != state.map_render.last_asset_gen
}

/// The work a reconcile must apply, derived purely from old-vs-new map state.
#[derive(Debug, Default, PartialEq)]
pub(crate) struct ReconcilePlan {
    /// Sectors whose XY outline changed — re-triangulate, then re-emit.
    pub retri: Vec<SectorKey>,
    /// Sectors whose fill span re-emits (outline or floor/ceil height changed).
    pub respan_sectors: Vec<SectorKey>,
    /// Lines whose wall span re-emits.
    pub rewall: Vec<LineKey>,
    pub sectors_removed: Vec<SectorKey>,
    pub lines_removed: Vec<LineKey>,
    pub verts_removed: Vec<VertKey>,
    pub things_removed: Vec<ThingKey>,
    /// Changed/added elements needing instance or storage patches.
    pub lines_changed: Vec<LineKey>,
    pub verts_changed: Vec<VertKey>,
    pub things_changed: Vec<ThingKey>,
    pub sectors_changed: Vec<SectorKey>,
    /// The line set or its endpoints/sector assignments changed (edge map rebuild).
    pub lines_structural: bool,
    /// A sector's floor/ceil flat name changed (atlas gate).
    pub flats_changed: bool,
    /// A side's texture name changed (atlas gate).
    pub wall_tex_changed: bool,
    /// A thing's kind changed (sprite cache + atlas gate).
    pub thing_kinds_changed: bool,
}

/// Changed-or-added keys and removed keys between two arenas.
fn diff_arena<K: ArenaKey + Ord, T: PartialEq>(
    old: &Arena<K, T>,
    new: &Arena<K, T>,
) -> (Vec<K>, Vec<K>) {
    let changed = new
        .iter()
        .filter(|&(k, v)| old.get(k) != Some(v))
        .map(|(k, _)| k)
        .collect();
    let removed = old.keys().filter(|&k| !new.contains(k)).collect();
    (changed, removed)
}

/// A line change that can alter sector outlines (not just wall appearance).
fn line_structural(a: &LineDef, b: &LineDef) -> bool {
    a.v1 != b.v1
        || a.v2 != b.v2
        || a.front.sector != b.front.sector
        || a.back.as_ref().and_then(|s| s.sector) != b.back.as_ref().and_then(|s| s.sector)
        || a.back.is_some() != b.back.is_some()
}

fn line_sectors(l: &LineDef) -> impl Iterator<Item = SectorKey> {
    l.sides().filter_map(|s| s.sector)
}

/// Derive the dirty sets from a keyed diff; `atlas_repacked` re-emits every wall span (rects moved).
fn side_tex(s: &SideDef) -> (Name8, Name8, Name8) {
    (s.top_tex, s.middle_tex, s.bottom_tex)
}

pub(crate) fn plan_reconcile(old: &EditorMap, new: &EditorMap) -> ReconcilePlan {
    let (verts_changed, verts_removed) = diff_arena(&old.vertices, &new.vertices);
    let (lines_changed, lines_removed) = diff_arena(&old.lines, &new.lines);
    let (sectors_changed, sectors_removed) = diff_arena(&old.sectors, &new.sectors);
    let (things_changed, things_removed) = diff_arena(&old.things, &new.things);

    let dirty_verts: HashSet<VertKey> = verts_changed
        .iter()
        .chain(&verts_removed)
        .copied()
        .collect();

    let mut rewall: HashSet<LineKey> = lines_changed.iter().copied().collect();
    let mut retri: HashSet<SectorKey> = HashSet::new();
    let mut lines_structural = !lines_removed.is_empty();

    for &k in &lines_changed {
        let n = &new.lines[k];
        match old.lines.get(k) {
            None => {
                lines_structural = true;
                retri.extend(line_sectors(n));
            }
            Some(o) => {
                if line_structural(o, n) {
                    lines_structural = true;
                    retri.extend(line_sectors(o));
                    retri.extend(line_sectors(n));
                }
            }
        }
    }
    for &k in &lines_removed {
        retri.extend(line_sectors(&old.lines[k]));
    }

    // A moved vertex shifts every touching line's walls and both bordering fills.
    if !dirty_verts.is_empty() {
        for (k, l) in new.lines.iter() {
            if dirty_verts.contains(&l.v1) || dirty_verts.contains(&l.v2) {
                rewall.insert(k);
                retri.extend(line_sectors(l));
            }
        }
    }

    // Added sectors need a fresh tris-cache entry even with no line churn.
    for &s in &sectors_changed {
        if !old.sectors.contains(s) {
            retri.insert(s);
        }
    }
    retri.retain(|&s| new.sectors.contains(s));

    // Height changes re-emit the fill at the new Z and every border line's walls.
    let mut respan: HashSet<SectorKey> = retri.clone();
    let mut height_changed: HashSet<SectorKey> = HashSet::new();
    for &s in &sectors_changed {
        let n = &new.sectors[s];
        let same_heights = old
            .sectors
            .get(s)
            .is_some_and(|o| o.floor_height == n.floor_height && o.ceil_height == n.ceil_height);
        if !same_heights {
            respan.insert(s);
            height_changed.insert(s);
        }
    }
    if !height_changed.is_empty() {
        for (k, l) in new.lines.iter() {
            if line_sectors(l).any(|s| height_changed.contains(&s)) {
                rewall.insert(k);
            }
        }
    }

    let flats_changed = !sectors_removed.is_empty()
        || sectors_changed.iter().any(|&s| {
            let n = &new.sectors[s];
            old.sectors
                .get(s)
                .is_none_or(|o| o.floor_flat != n.floor_flat || o.ceil_flat != n.ceil_flat)
        });
    let wall_tex_changed = !lines_removed.is_empty()
        || lines_changed.iter().any(|&k| {
            let n = &new.lines[k];
            old.lines.get(k).is_none_or(|o| {
                side_tex(&o.front) != side_tex(&n.front)
                    || o.back.as_ref().map(side_tex) != n.back.as_ref().map(side_tex)
            })
        });
    let thing_kinds_changed = !things_removed.is_empty()
        || things_changed.iter().any(|&k| {
            let n = &new.things[k];
            old.things.get(k).is_none_or(|o| o.kind != n.kind)
        });

    fn sorted<K: Ord>(set: HashSet<K>) -> Vec<K> {
        let mut v: Vec<K> = set.into_iter().collect();
        v.sort_unstable();
        v
    }
    ReconcilePlan {
        retri: sorted(retri),
        respan_sectors: sorted(respan),
        rewall: sorted(rewall),
        sectors_removed,
        lines_removed,
        verts_removed,
        things_removed,
        lines_changed,
        verts_changed,
        things_changed,
        sectors_changed,
        lines_structural,
        flats_changed,
        wall_tex_changed,
        thing_kinds_changed,
    }
}

/// Re-triangulate dirty sectors, rewrite spans + slots, patch storage, wire, BVH, then snapshot.
fn apply_reconcile(state: &mut SharedState, pixel_ratio: f32, plan: &ReconcilePlan) {
    {
        let SharedState {
            app,
            map_render,
            ..
        } = state;
        let Some(map) = app.map.as_ref() else { return };
        retriangulate_sectors(map, &plan.retri, &mut map_render.sector_tris);
        for &s in &plan.sectors_removed {
            map_render.sector_tris.sector.remove(&s);
        }
        // Wireframe marker Z only: rebuild when its inputs move or when entering wireframe fill.
        let wire_z_dirty = !plan.verts_changed.is_empty()
            || !plan.verts_removed.is_empty()
            || !plan.rewall.is_empty()
            || plan.lines_structural
            || !plan.respan_sectors.is_empty();
        // Snap buckets hold positions; any vertex/line change invalidates them.
        if !plan.verts_changed.is_empty()
            || !plan.verts_removed.is_empty()
            || !plan.lines_changed.is_empty()
            || !plan.lines_removed.is_empty()
        {
            app.snap_index = None;
        }
        if app.sector_fill == SectorFill::None
            && (wire_z_dirty || map_render.last_fill != SectorFill::None)
        {
            map_render.vertex_floor_z = frame::build_vertex_floor_z(map);
        }
        // Fill transitions count: the light list is built only in Texture fill.
        map_render.light_set_dirty |= !plan.sectors_changed.is_empty()
            || !plan.sectors_removed.is_empty()
            || !plan.lines_changed.is_empty()
            || !plan.lines_removed.is_empty()
            || app.sector_fill != map_render.last_fill;
    }

    let highlighted = state.app.highlighted_sectors();
    let skill = state.app.skill_filter;
    let gradient = state.prefs.sector_gradient.gradient();
    let SharedState {
        app,
        map_render,
        wgpu,
        ..
    } = state;
    let LevelEditorState {
        map,
        surface_mesh,
        thing_extents,
        thing_colors,
        style,
        selection,
        camera,
        grid,
        sector_fill,
        highlight_unenclosed,
        bvh,
        edge_lines,
        ..
    } = app;
    let Some(map) = map.as_ref() else { return };
    let MapRender {
        sector_tris,
        atlas_maps,
        vertex_floor_z,
        surface_slots,
        last_synced,
        last_selection,
        last_highlighted,
        last_grid_z,
        last_fill,
        last_pixel_ratio,
        light_anim,
        ..
    } = map_render;
    let visible = move |t: &Thing| skill.allows(t.options);
    let input = FrameInput {
        map,
        tris: sector_tris,
        zoom: camera.zoom_level(),
        pixel_ratio,
        style,
        selection,
        grid: *grid,
        fill: *sector_fill,
        selected_sectors: &highlighted,
        thing_visible: &visible,
        thing_extents,
        thing_colors,
        atlas: atlas_maps,
        thing_radius: &defaults::thing_radius,
        sector_gradient: gradient,
        highlight_unenclosed: *highlight_unenclosed,
        mode: camera.mode(),
        grid_z: camera.grid_z(),
        vert_z: vertex_floor_z,
    };

    // Surface spans: free removed, rewrite dirty; overflow relocates within the mirror.
    let used_before = surface_slots.used;
    let mut span_patches: Vec<SpanPatch> = Vec::new();
    for &s in &plan.sectors_removed {
        surface_slots.free_sector(surface_mesh, s, &mut span_patches);
    }
    for &l in &plan.lines_removed {
        surface_slots.free_line(surface_mesh, l, &mut span_patches);
    }
    for &s in &plan.respan_sectors {
        let verts = sector_surface_verts(map, sector_tris, s);
        surface_slots.update_sector(surface_mesh, s, verts, &mut span_patches);
    }
    for &l in &plan.rewall {
        let verts = line_wall_verts(map, l, &atlas_maps.wall_rects);
        surface_slots.update_line(surface_mesh, l, verts, &mut span_patches);
    }
    let mut surface_grew = false;
    for p in &span_patches {
        let range = p.offset as usize..(p.offset + p.count) as usize;
        if !wgpu.patch_surface(p.offset, &surface_mesh[range]) {
            surface_grew = true;
            break;
        }
    }
    if surface_grew {
        wgpu.upload_surface(surface_mesh);
    }

    // Line instances derive from vertex positions + heights → follow `rewall`, not the record diff.
    let mut lines_to_patch: HashSet<LineKey> = plan.rewall.iter().copied().collect();
    let mut verts_to_patch: HashSet<VertKey> = plan.verts_changed.iter().copied().collect();
    // Marker Z rides bordering floor heights in wireframe → follow rewall + removals too.
    if *sector_fill == SectorFill::None {
        let old = last_synced
            .as_ref()
            .expect("reconcile diffed this snapshot");
        verts_to_patch.extend(wire_z_verts(plan, map, old));
    }
    let mut things_to_patch: HashSet<ThingKey> = plan.things_changed.iter().copied().collect();
    let mut sectors_to_patch: HashSet<SectorKey> = plan.sectors_changed.iter().copied().collect();
    let old_items: HashSet<SelItem> = last_selection.items().iter().copied().collect();
    let new_items: HashSet<SelItem> = selection.items().iter().copied().collect();
    let mut line_sel_changed = false;
    for item in old_items.symmetric_difference(&new_items) {
        match *item {
            SelItem::Line(k) => {
                lines_to_patch.insert(k);
                line_sel_changed = true;
            }
            SelItem::Vertex(k) => {
                verts_to_patch.insert(k);
            }
            SelItem::Thing(k) => {
                things_to_patch.insert(k);
            }
            SelItem::Sector(k) => {
                sectors_to_patch.insert(k);
            }
        }
    }
    let old_tint: HashSet<SectorKey> = last_highlighted.iter().copied().collect();
    let new_tint: HashSet<SectorKey> = highlighted.iter().copied().collect();
    sectors_to_patch.extend(old_tint.symmetric_difference(&new_tint));
    fn live_sorted<K: ArenaKey + Ord>(set: HashSet<K>, live: impl Fn(K) -> bool) -> Vec<K> {
        let mut v: Vec<K> = set.into_iter().filter(|&k| live(k)).collect();
        v.sort_unstable();
        v
    }
    let lines_to_patch = live_sorted(lines_to_patch, |k| map.lines.contains(k));
    let verts_to_patch = live_sorted(verts_to_patch, |k| map.vertices.contains(k));
    let things_to_patch = live_sorted(things_to_patch, |k| map.things.contains(k));
    let sectors_to_patch = live_sorted(sectors_to_patch, |k| map.sectors.contains(k));

    // Instance slots: tombstone removed, rewrite changed; overflow rebuilds the layer.
    let mut inst_grew = false;
    for &l in &plan.lines_removed {
        inst_grew |= !wgpu.patch_line(l.slot(), LineInst::default(), LineInst::default());
    }
    for &v in &plan.verts_removed {
        inst_grew |= !wgpu.patch_vert(v.slot(), MarkerInst::default());
    }
    for &t in &plan.things_removed {
        inst_grew |= !wgpu.patch_thing(t.slot(), ThingInst::default());
    }
    for &l in &lines_to_patch {
        let (seg, normal) = frame::line_instances(&input, l);
        inst_grew |= !wgpu.patch_line(l.slot(), seg, normal);
    }
    for &v in &verts_to_patch {
        inst_grew |= !wgpu.patch_vert(v.slot(), frame::vert_instance(&input, v));
    }
    for &t in &things_to_patch {
        inst_grew |= !wgpu.patch_thing(t.slot(), frame::thing_instance(&input, t));
    }
    // Grid-plane, fill-mode, and DPI changes reshape every instance: re-emit the layers.
    let view_changed = *sector_fill != *last_fill
        || (camera.grid_z() - *last_grid_z).abs() > f32::EPSILON
        || (pixel_ratio - *last_pixel_ratio).abs() > f32::EPSILON;
    if inst_grew || view_changed {
        let (lines, normals) = frame::build_line_instances(&input);
        wgpu.upload_lines(&lines, &normals);
        wgpu.upload_verts(&frame::build_vert_instances(&input));
        wgpu.upload_things(&frame::build_thing_instances(&input));
    }

    // Sector storage: patch by slot; a slot beyond the arrays rebuilds them.
    let mut sector_grew = false;
    for &s in &plan.sectors_removed {
        sector_grew |= !wgpu.patch_sector_attr(s.slot(), SectorAttr::zeroed());
        sector_grew |= !wgpu.patch_sector_3d(s.slot(), Sector3D::zeroed());
    }
    for &s in &sectors_to_patch {
        sector_grew |= !wgpu.patch_sector_attr(s.slot(), frame::sector_attr(&input, s));
        sector_grew |= !wgpu.patch_sector_3d(s.slot(), frame::sector_3d(&input, s));
    }
    if sector_grew {
        wgpu.set_sector_data(
            &compute_brightness(map, *sector_fill, light_anim),
            &frame::build_sector_attrs(&input),
            &frame::build_sector_3d(&input),
        );
    } else {
        // Mass edits (remap, undo of a global change) upload once instead of a write per slot.
        let dirty = plan.sectors_removed.len() + sectors_to_patch.len();
        if dirty * 2 > map.sectors.slot_count() {
            wgpu.update_brightness(&compute_brightness(map, *sector_fill, light_anim));
        } else {
            let mut patched = true;
            for &s in plan.sectors_removed.iter().chain(&sectors_to_patch) {
                let v = sector_brightness(map, s, *sector_fill, light_anim);
                patched &= wgpu.patch_brightness(s.slot(), v);
            }
            if !patched {
                wgpu.update_brightness(&compute_brightness(map, *sector_fill, light_anim));
            }
        }
    }

    // The wire derives from line geometry/heights/textures and line-selection colour.
    let wire_dirty = !plan.rewall.is_empty()
        || !plan.lines_removed.is_empty()
        || plan.lines_structural
        || line_sel_changed
        || view_changed;
    if *sector_fill == SectorFill::None {
        if wire_dirty {
            wgpu.upload_wire(&frame::build_wire(&input));
        }
    } else if *sector_fill != *last_fill {
        wgpu.upload_wire(&[]);
    }
    if *sector_fill != *last_fill {
        wgpu.set_fill_mode(*sector_fill);
    }
    wgpu.set_grid_style(frame::grid_style(&input));
    wgpu.set_grid_z(input.grid_z);
    wgpu.set_overlay(&[], &[]);

    // BVH: refit for value edits; leaf-set checks only when things or the mesh length moved.
    let things_dirty = !plan.things_changed.is_empty() || !plan.things_removed.is_empty();
    if things_dirty || surface_slots.used != used_before {
        let things = thing_leaves(map, thing_extents);
        if bvh.covers(surface_mesh, &things) {
            bvh.refit(surface_mesh, &things);
        } else {
            *bvh = MeshBvh::build(surface_mesh, &things);
        }
    } else if !span_patches.is_empty() {
        // Thing payloads are still valid; refresh triangle bounds only.
        bvh.refit(surface_mesh, &[]);
    }
    if plan.lines_structural {
        let old = last_synced
            .as_ref()
            .expect("reconcile diffed this snapshot");
        // Drop stale pairs (keyed by OLD endpoints) before inserting the new ones.
        let mut orphaned: HashSet<(VertKey, VertKey)> = HashSet::new();
        for &k in plan.lines_removed.iter().chain(&plan.lines_changed) {
            if let Some(l) = old.lines.get(k) {
                let pair = vert_pair(l.v1, l.v2);
                if edge_lines.get(&pair) == Some(&k) {
                    edge_lines.remove(&pair);
                    orphaned.insert(pair);
                }
            }
        }
        for &k in &plan.lines_changed {
            if let Some(l) = map.lines.get(k) {
                let pair = vert_pair(l.v1, l.v2);
                edge_lines.insert(pair, k);
                orphaned.remove(&pair);
            }
        }
        // A surviving coincident twin (imported maps allow duplicates) re-registers its pair.
        if !orphaned.is_empty() {
            for (k, l) in map.lines.iter() {
                let pair = vert_pair(l.v1, l.v2);
                if orphaned.contains(&pair) {
                    edge_lines.insert(pair, k);
                }
            }
        }
    }

    // A selection-only reconcile proves the arenas content-equal: keep the old snapshot.
    if *plan != ReconcilePlan::default() {
        *last_synced = Some(map.clone());
    }
    *last_selection = selection.clone();
    *last_highlighted = highlighted.clone();
    *last_grid_z = camera.grid_z();
    *last_fill = *sector_fill;
    *last_pixel_ratio = pixel_ratio;
}

/// Vertices whose wireframe floor Z can move under `plan`: endpoints of rewall lines, plus endpoints of removed lines and removed sectors' border lines (resolved via the pre-edit map).
fn wire_z_verts(plan: &ReconcilePlan, map: &EditorMap, old: &EditorMap) -> HashSet<VertKey> {
    let mut verts: HashSet<VertKey> = HashSet::new();
    for &l in &plan.rewall {
        if let Some(line) = map.lines.get(l) {
            verts.insert(line.v1);
            verts.insert(line.v2);
        }
    }
    for &l in &plan.lines_removed {
        if let Some(line) = old.lines.get(l) {
            verts.insert(line.v1);
            verts.insert(line.v2);
        }
    }
    if !plan.sectors_removed.is_empty() {
        let removed: HashSet<SectorKey> = plan.sectors_removed.iter().copied().collect();
        for line in old.lines.values() {
            if line_sectors(line).any(|s| removed.contains(&s)) {
                verts.insert(line.v1);
                verts.insert(line.v2);
            }
        }
    }
    verts
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

/// The one full build (map load / deliberate invalidation): every cache + the whole mesh.
pub(crate) fn push_wgpu_frame(ui: &EditorWindow, shared: &Rc<RefCell<SharedState>>) {
    if !shared.borrow().wgpu.is_ready() {
        return;
    }
    {
        let state = &mut *shared.borrow_mut();
        ensure_thing_sprites(state);
        refresh_atlases(state, None);
    }
    let (pw, ph) = physical_size(ui, shared);
    {
        let state = &mut *shared.borrow_mut();
        let pr = pixel_ratio(state, pw);
        full_sync(state, pr);
    }
    paint(ui, shared, pw, ph);
}

/// Build all caches + GPU buffers from scratch and snapshot for future reconciles.
fn full_sync(state: &mut SharedState, pixel_ratio: f32) {
    let Some(map) = &state.app.map else {
        state.map_render.last_synced = None;
        return;
    };
    log::info!(
        "wgpu full map build: {} lines, {} sectors",
        map.lines.len(),
        map.sectors.len()
    );
    state.map_render.sector_tris = triangulate::build_sector_tris(map);
    state.map_render.vertex_floor_z = frame::build_vertex_floor_z(map);

    let highlighted = state.app.highlighted_sectors();
    let skill = state.app.skill_filter;
    let visible = move |t: &Thing| skill.allows(t.options);
    let (frame, slots) = {
        let input = frame_input(state, pixel_ratio, &highlighted, &visible);
        let built = frame::build_map_geometry(&input);
        state.wgpu.set_grid_style(frame::grid_style(&input));
        state.wgpu.set_grid_z(input.grid_z);
        built
    };
    let brightness = compute_brightness(
        state.app.map.as_ref().expect("checked above"),
        state.app.sector_fill,
        &state.map_render.light_anim,
    );
    state
        .wgpu
        .set_sector_data(&brightness, &frame.sector_attrs, &frame.sector3d);
    state.wgpu.upload_map(&frame);
    state.wgpu.set_fill_mode(state.app.sector_fill);
    state.wgpu.set_overlay(&[], &[]);
    state.app.surface_mesh = frame.surface3d; // retained CPU mirror for picking + patches
    state.map_render.surface_slots = slots;
    state.app.rebuild_bvh();
    state.map_render.last_synced = state.app.map.clone();
    state.map_render.last_selection = state.app.selection.clone();
    state.map_render.last_highlighted = highlighted;
    state.map_render.last_grid_z = state.app.camera.grid_z();
    state.map_render.last_fill = state.app.sector_fill;
    state.map_render.last_pixel_ratio = pixel_ratio;
    state.map_render.light_set_dirty = true;
    state.map_render.panels_key = None;
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
    upload_edit_preview(ui, shared);
    repaint_canvas(ui, shared);
}

/// Upload the edit preview + transform handles to the GPU overlay layer (no repaint).
fn upload_edit_preview(ui: &EditorWindow, shared: &Rc<RefCell<SharedState>>) {
    if !shared.borrow().wgpu.is_ready() {
        return;
    }
    let (pw, _) = physical_size(ui, shared);
    let state = shared.borrow();
    let pr = pixel_ratio(&state, pw);
    let z = state.app.camera.grid_z();
    let (mut lines, mut markers) =
        frame::build_preview(&state.app.overlay, &state.app.style, pr, z);
    if let Some(h) = state.app.transform_handles() {
        frame::push_handles(&mut lines, &mut markers, &h, &state.app.style, pr, z);
    }
    state.wgpu.set_overlay(&lines, &markers);
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

/// Re-pack the GPU atlases if the content key moved; true = wall spans must re-emit. A cache hit remaps tiles for `dirty_sectors` only (`None` = every sector, the load path).
fn refresh_atlases(state: &mut SharedState, dirty_sectors: Option<&[SectorKey]>) -> bool {
    if state.app.map.is_none() || !state.ensure_assets() {
        return false;
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
        return false;
    };
    assets.set_map_wad(&map_wad);
    let names = atlas::collect_wall_names(assets, map);
    assets.ensure_composed(&names, wad);
    let assets = &*assets;
    map_render.last_asset_gen = Some(assets.generation());

    let key = atlas::content_key(assets, map, sprites, &names);
    if map_render.atlas_key == Some(key) {
        match dirty_sectors {
            Some(keys) => atlas::remap_sector_tiles_for(map, &mut map_render.atlas_maps, keys),
            None => atlas::remap_sector_tiles(map, &mut map_render.atlas_maps),
        }
        return false;
    }
    let (data, maps) = atlas::build(assets, map, sprites, key);
    wgpu.set_atlases(&data);
    map_render.atlas_maps = maps;
    map_render.atlas_key = Some(key);
    true
}

/// Per-sector-slot brightness scalar (0..1), applying active light effects.
fn compute_brightness(
    map: &EditorMap,
    fill: SectorFill,
    light_anim: &[light_anim::SectorLight],
) -> Vec<f32> {
    let mut brightness = vec![0.0f32; map.sectors.slot_count()];
    for (key, s) in map.sectors.iter() {
        brightness[key.slot() as usize] = s.light_level.clamp(0, 255) as f32 / 255.0;
    }
    if fill == SectorFill::Texture {
        for light in light_anim {
            if let Some(b) = brightness.get_mut(light.sector.slot() as usize) {
                *b = light.current.clamp(0, 255) as f32 / 255.0;
            }
        }
    }
    brightness
}

/// One sector slot's brightness scalar; a removed sector reads 0. Active effects win in Texture fill.
fn sector_brightness(
    map: &EditorMap,
    key: SectorKey,
    fill: SectorFill,
    light_anim: &[light_anim::SectorLight],
) -> f32 {
    if fill == SectorFill::Texture
        && let Some(light) = light_anim.iter().find(|l| l.sector == key)
    {
        return light.current.clamp(0, 255) as f32 / 255.0;
    }
    map.sectors
        .get(key)
        .map_or(0.0, |s| s.light_level.clamp(0, 255) as f32 / 255.0)
}

/// Light tic: patch only the animated slots; the rest of the buffer is already current.
fn upload_brightness(state: &SharedState) {
    if state.app.map.is_none() {
        return;
    }
    for light in &state.map_render.light_anim {
        let v = light.current.clamp(0, 255) as f32 / 255.0;
        state.wgpu.patch_brightness(light.sector.slot(), v);
    }
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
    highlighted: &'a [SectorKey],
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
        vert_z: &state.map_render.vertex_floor_z,
    }
}

/// Top-down ortho camera for PNG export / headless render (width follows aspect).
pub(crate) fn export_camera(centre: [f32; 2], scale: f32, h: f32) -> Camera {
    let mut cam = Camera::default();
    cam.look_down_at([centre[0], centre[1], 0.0]);
    cam.set_ortho_height(h / scale.max(1e-6));
    cam
}

/// Decode not-yet-cached thing icons; project `things.dsp` icon overrides the built-in sprite prefix, kinds with neither fall back to a colour square.
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

    let mut kinds: Vec<i32> = map.things.values().map(|t| t.kind).collect();
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

/// Reconcile light-effect list and start/stop the 35 Hz timer; when `set_may_change` is false (pan/zoom), keep existing list to preserve phases.
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
            // Live slots leaving the set revert to authored; a dead key's slot may already belong to a reused sector the reconcile just patched — leave it alone.
            if let Some(map) = &state.app.map {
                for old in &state.map_render.light_anim {
                    if map.sectors.contains(old.sector)
                        && !next.iter().any(|n| n.sector == old.sector)
                    {
                        let v = sector_brightness(map, old.sector, state.app.sector_fill, &[]);
                        state.wgpu.patch_brightness(old.sector.slot(), v);
                    }
                }
            }
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
        {
            let state = &mut *shared.borrow_mut();
            // Live animated slots revert to authored before the list drops; dead keys' slots may already belong to reused sectors.
            if let Some(map) = &state.app.map {
                for light in &state.map_render.light_anim {
                    if map.sectors.contains(light.sector) {
                        let v = sector_brightness(map, light.sector, state.app.sector_fill, &[]);
                        state.wgpu.patch_brightness(light.sector.slot(), v);
                    }
                }
            }
            state.map_render.light_anim.clear();
        }
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

    fn e1m1() -> EditorMap {
        let wad = WadData::new(&test_utils::doom1_wad_path());
        editor_core::import_wad_map(&wad, "E1M1").expect("E1M1 imports")
    }

    /// `ensure_thing_sprites` early-returned on maps with no things, blocking `refresh_atlases` (needs the sprite cache); result: atlas empty → textured 3D surfaces sampled nothing, canvas showed only lines.
    #[test]
    fn map_without_things_still_textures_walls_and_floors() {
        let mut map = e1m1();
        map.things.retain(|_, _| false);

        let mut app = LevelEditorState::new();
        app.load_map(map, "E1M1");
        let mut state =
            SharedState::new(app, Some(test_utils::doom1_wad_path()), Default::default());

        ensure_thing_sprites(&mut state);
        assert!(state.thing_sprites.is_some(), "sprite cache inserted");

        refresh_atlases(&mut state, None);
        assert!(
            !state.map_render.atlas_maps.wall_rects.is_empty(),
            "wall atlas built (walls texture in 3D)"
        );
        assert!(
            !state.map_render.atlas_maps.sector_tile.is_empty(),
            "flat tiles built (floors fill in 3D)"
        );
    }

    /// Nudging one vertex dirties exactly its incident sectors and touching lines.
    #[test]
    fn vertex_nudge_plans_only_incident_work() {
        let old = e1m1();
        let mut new = old.clone();
        let vk = new.vertices.keys().next().expect("has vertices");
        new.vertices[vk].x += 8.0;

        let plan = plan_reconcile(&old, &new);

        let touching: Vec<LineKey> = new
            .lines
            .iter()
            .filter(|(_, l)| l.v1 == vk || l.v2 == vk)
            .map(|(k, _)| k)
            .collect();
        let incident: HashSet<SectorKey> = touching
            .iter()
            .flat_map(|&k| line_sectors(&new.lines[k]))
            .collect();

        assert_eq!(plan.verts_changed, vec![vk]);
        assert_eq!(
            plan.rewall.len(),
            touching.len(),
            "only touching lines re-emit"
        );
        assert_eq!(
            plan.retri.iter().copied().collect::<HashSet<_>>(),
            incident,
            "only incident sectors re-triangulate"
        );
        assert_eq!(plan.respan_sectors.len(), plan.retri.len());
        assert!(plan.lines_removed.is_empty() && plan.sectors_removed.is_empty());
        assert!(!plan.lines_structural, "a move is not structural");
        assert!(plan.lines_changed.is_empty(), "no line record changed");
        assert!(
            !plan.flats_changed && !plan.wall_tex_changed && !plan.thing_kinds_changed,
            "a move never dirties the atlas"
        );
    }

    /// A height spin re-emits that sector's span and its border lines' walls only.
    #[test]
    fn height_spin_plans_sector_and_border_walls() {
        let old = e1m1();
        let mut new = old.clone();
        let sk = new.sectors.keys().next().expect("has sectors");
        new.sectors[sk].floor_height += 8;

        let plan = plan_reconcile(&old, &new);

        let borders: HashSet<LineKey> = new
            .lines
            .iter()
            .filter(|(_, l)| line_sectors(l).any(|s| s == sk))
            .map(|(k, _)| k)
            .collect();

        assert_eq!(plan.sectors_changed, vec![sk]);
        assert_eq!(plan.respan_sectors, vec![sk]);
        assert!(plan.retri.is_empty(), "heights never re-triangulate");
        assert_eq!(plan.rewall.iter().copied().collect::<HashSet<_>>(), borders);
        assert!(!plan.lines_structural);
    }

    /// Wireframe vertex markers ride floor Z: a height spin must repatch every border vertex.
    #[test]
    fn height_spin_repatches_wire_vertex_markers() {
        let old = e1m1();
        let mut new = old.clone();
        let sk = new.sectors.keys().next().expect("has sectors");
        new.sectors[sk].floor_height += 8;

        let plan = plan_reconcile(&old, &new);
        let patched = wire_z_verts(&plan, &new, &old);

        let border_verts: HashSet<VertKey> = new
            .lines
            .values()
            .filter(|l| line_sectors(l).any(|s| s == sk))
            .flat_map(|l| [l.v1, l.v2])
            .collect();
        assert!(!border_verts.is_empty());
        assert!(
            border_verts.is_subset(&patched),
            "every vertex bordering the edited sector repatches"
        );
    }

    /// A texture-only wall edit re-emits that wall span; no triangulation, no fills.
    #[test]
    fn texture_swap_plans_wall_only() {
        let old = e1m1();
        let mut new = old.clone();
        let lk = new
            .lines
            .iter()
            .find(|(_, l)| l.back.is_none())
            .map(|(k, _)| k)
            .expect("a one-sided line");
        new.lines[lk].front.middle_tex = Name8::new("STARTAN3").expect("name");

        let plan = plan_reconcile(&old, &new);
        assert_eq!(plan.rewall, vec![lk]);
        assert!(plan.retri.is_empty(), "texture edits never re-triangulate");
        assert!(plan.respan_sectors.is_empty());
        assert!(!plan.lines_structural, "texture edits are not structural");
        assert!(plan.wall_tex_changed, "texture edits open the atlas gate");
    }

    /// Deleting a line frees its span and re-triangulates its old sectors.
    #[test]
    fn line_delete_plans_removal_and_sector_retri() {
        let old = e1m1();
        let mut new = old.clone();
        let lk = new.lines.keys().next().expect("has lines");
        let old_sectors: HashSet<SectorKey> = line_sectors(&new.lines[lk]).collect();
        new.lines.remove(lk);

        let plan = plan_reconcile(&old, &new);
        assert_eq!(plan.lines_removed, vec![lk]);
        assert!(plan.lines_structural);
        let mut old_sectors: Vec<SectorKey> = old_sectors.into_iter().collect();
        old_sectors.sort_unstable();
        for s in old_sectors {
            assert!(
                plan.retri.contains(&s),
                "old bordering sector re-triangulates"
            );
        }
    }

    /// Undo restores the pre-delete map exactly, so the line reappears at its old key — no special path.
    #[test]
    fn undo_of_delete_reconciles_as_reinsertion() {
        let restored = e1m1();
        let mut deleted = restored.clone();
        let lk = deleted.lines.keys().next().expect("has lines");
        deleted.lines.remove(lk);

        let plan = plan_reconcile(&deleted, &restored);
        assert!(
            plan.lines_changed.contains(&lk),
            "line reappears at its original key"
        );
        assert!(plan.rewall.contains(&lk), "its wall span re-emits");
        assert!(plan.lines_structural, "edge map rebuilds");
        assert!(plan.lines_removed.is_empty());
    }

    /// Flat and thing-kind edits open the atlas gate; an identical map leaves it shut.
    #[test]
    fn atlas_gate_tracks_flat_and_kind_changes() {
        let old = e1m1();
        assert_eq!(plan_reconcile(&old, &old), ReconcilePlan::default());

        let mut new = old.clone();
        let sk = new.sectors.keys().next().expect("has sectors");
        new.sectors[sk].floor_flat = Name8::new("ZZNEWFLT").expect("name");
        let plan = plan_reconcile(&old, &new);
        assert!(plan.flats_changed);
        assert!(plan.retri.is_empty(), "a flat swap never re-triangulates");

        let mut new = old.clone();
        let tk = new.things.keys().next().expect("has things");
        new.things[tk].kind += 1000;
        assert!(plan_reconcile(&old, &new).thing_kinds_changed);
    }
}
