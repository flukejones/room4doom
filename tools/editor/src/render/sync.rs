//! Reconcile editor damage into GPU buffers: diff the last-synced map and patch only dirty
//! sector/line/vertex/thing slots. Only map load and deliberate restyling rebuild the mesh.
//! The map tab supplies physical pixel metrics, publishes painted images and schedules animation.

use std::collections::HashSet;

use bytemuck::Zeroable as _;

use editor_core::{
    Arena, ArenaKey, EditorMap, LineDef, LineKey, Name8, SectorKey, SideDef, Thing, ThingKey,
    VertKey,
};

use crate::app::session::EditorSession;
use crate::gfx::image::UiImage;
use crate::level_editor::bvh::MeshBvh;
use crate::level_editor::state::{Damage, SectorFill, SelItem};
use crate::level_editor::view::vert_pair;
use crate::level_editor::{LevelEditorState, thing_leaves};
use crate::render::cache::{MapLinks, MapRender};
use crate::render::camera3d::Camera;
use crate::render::frame::{self, FrameInput, thing_world_half_extent};
use crate::render::frame3d::{SpanPatch, line_wall_verts, sector_surface_verts};
use crate::render::triangulate::{self, retriangulate_sectors};
use crate::render::wgpu::{LineInst, MarkerInst, Sector3D, SectorAttr, ThingInst};
use crate::render::{atlas, fnv_fold};
use crate::{defaults, gfx, light_anim};

/// Update GPU caches; return whether the light-effect set needs reconciliation.
pub(crate) fn apply_damage(state: &mut EditorSession, damage: Damage, pixel_ratio: f32) -> bool {
    match damage {
        Damage::None => return false,
        Damage::Overlay => {}
        // Grid-plane and DPI changes re-emit instance sizes through reconcile.
        Damage::View | Damage::Repaint if !instance_metrics_changed(state, pixel_ratio) => {
            regrid(state, pixel_ratio);
        }
        Damage::View | Damage::Repaint | Damage::Edited => {
            reconcile(state, pixel_ratio);
        }
        Damage::Restyle => {
            state.map_render.last_synced = None;
            reconcile(state, pixel_ratio);
        }
    }
    // Reconciliation clears transient GPU data; publish selection chrome afterwards.
    upload_edit_preview(state, pixel_ratio);
    // Pan/zoom must preserve flicker phases; only changes to light inputs rebuild the list.
    match damage {
        Damage::Edited => std::mem::take(&mut state.map_render.light_set_dirty),
        Damage::Restyle | Damage::Repaint => true,
        _ => false,
    }
}

/// Cached instances contain both world-space heights and physical-pixel sizes.
fn instance_metrics_changed(state: &EditorSession, pixel_ratio: f32) -> bool {
    state.map_render.last_synced.is_some()
        && ((state.map_editor.camera.grid_z() - state.map_render.last_grid_z).abs() > f32::EPSILON
            || (pixel_ratio - state.map_render.last_pixel_ratio).abs() > f32::EPSILON)
}

/// Diff map + selection against the last-synced snapshot and patch the GPU; first sync = full build.
fn reconcile(state: &mut EditorSession, pixel_ratio: f32) {
    if !state.wgpu.is_ready() {
        // The edit still happened: derived caches the reconciler owns must not go stale.
        state.map_render.light_set_dirty = true;
        state.map_editor.snap_index = None;
        return;
    }
    if state.map_editor.map.is_none() {
        state.map_render.last_synced = None;
        return;
    }
    if state.map_render.last_synced.is_none() {
        push_wgpu_frame(state, pixel_ratio);
        return;
    }
    let mut plan = plan_reconcile_with_links(
        state
            .map_render
            .last_synced
            .as_ref()
            .expect("checked above"),
        state.map_editor.map.as_ref().expect("checked above"),
        &state.map_render.links,
    );
    // Resource replacement can restart asset counters while retaining every map record.
    let resources_changed =
        state.map_render.last_resource_generation != Some(state.resource_generation);
    // Texture names, kinds and asset generation gate atlas work.
    let atlas_dirty = plan.flats_changed
        || plan.wall_tex_changed
        || plan.thing_kinds_changed
        || assets_changed(state);
    let repacked = atlas_dirty && {
        if plan.thing_kinds_changed || resources_changed || state.thing_sprites.is_none() {
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
        plan.rewall = state
            .map_editor
            .map
            .as_ref()
            .expect("checked above")
            .lines
            .keys()
            .collect();
    }
    apply_reconcile(state, pixel_ratio, &plan, repacked);
}

/// Cheap input comparison used when the native map tab is rendered or activated.
pub(crate) fn assets_changed(state: &EditorSession) -> bool {
    state.map_render.last_resource_generation != Some(state.resource_generation)
        || state.assets.as_ref().map(|a| a.generation()) != state.map_render.last_asset_gen
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
    if let Some(slots) = new.changed_slots_since(old) {
        let mut changed = Vec::new();
        let mut removed = Vec::new();
        for &slot in slots {
            let before = old.key_at_slot(slot);
            let after = new.key_at_slot(slot);
            if before != after {
                removed.extend(before);
                changed.extend(after);
            } else if let Some(key) = after
                && old.get(key) != new.get(key)
            {
                changed.push(key);
            }
        }
        changed.sort_unstable();
        removed.sort_unstable();
        return (changed, removed);
    }
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

#[expect(
    clippy::too_many_lines,
    reason = "keep the complete dispatch or geometry pass together without changing its established ordering"
)]
fn plan_reconcile_with_links(old: &EditorMap, new: &EditorMap, links: &MapLinks) -> ReconcilePlan {
    fn sorted<K: Ord>(set: HashSet<K>) -> Vec<K> {
        let mut v: Vec<K> = set.into_iter().collect();
        v.sort_unstable();
        v
    }

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
        for vertex in verts_changed.iter().chain(&verts_removed) {
            for &key in links.vertices.get(vertex).into_iter().flatten() {
                if let Some(line) = new
                    .lines
                    .get(key)
                    .filter(|line| dirty_verts.contains(&line.v1) || dirty_verts.contains(&line.v2))
                {
                    rewall.insert(key);
                    retri.extend(line_sectors(line));
                }
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
    let mut height_changed = Vec::new();
    for &s in &sectors_changed {
        let n = &new.sectors[s];
        let same_heights = old
            .sectors
            .get(s)
            .is_some_and(|o| o.floor_height == n.floor_height && o.ceil_height == n.ceil_height);
        if !same_heights {
            respan.insert(s);
            height_changed.push(s);
        }
    }
    if !height_changed.is_empty() {
        for sector in &height_changed {
            for &key in links.sectors.get(sector).into_iter().flatten() {
                if new.lines.contains(key) {
                    rewall.insert(key);
                }
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

#[cfg(test)]
pub(crate) fn plan_reconcile(old: &EditorMap, new: &EditorMap) -> ReconcilePlan {
    plan_reconcile_with_links(old, new, &MapLinks::build(old))
}

fn sync_map_snapshot(snapshot: &mut EditorMap, map: &mut EditorMap) {
    snapshot.vertices.sync_snapshot_from(&mut map.vertices);
    snapshot.lines.sync_snapshot_from(&mut map.lines);
    snapshot.sectors.sync_snapshot_from(&mut map.sectors);
    snapshot.things.sync_snapshot_from(&mut map.things);
    snapshot.required_wads.clone_from(&map.required_wads);
}

/// Re-triangulate dirty sectors, rewrite spans + slots, patch storage, wire, BVH, then snapshot.
#[expect(
    clippy::too_many_lines,
    reason = "keep the complete dispatch or geometry pass together without changing its established ordering"
)]
fn apply_reconcile(
    state: &mut EditorSession,
    pixel_ratio: f32,
    plan: &ReconcilePlan,
    atlas_repacked: bool,
) {
    fn live_sorted<K: ArenaKey + Ord>(set: HashSet<K>, live: impl Fn(K) -> bool) -> Vec<K> {
        let mut v: Vec<K> = set.into_iter().filter(|&k| live(k)).collect();
        v.sort_unstable();
        v
    }

    {
        let EditorSession {
            map_editor: app,
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

    let highlighted = state.map_editor.highlighted_sectors();
    let skill = state.map_editor.skill_filter;
    let gradient = state.prefs.sector_gradient.gradient();
    let EditorSession {
        map_editor: app,
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
    let Some(map) = map.as_mut() else { return };
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
        links,
        ..
    } = map_render;
    let visible = move |t: &Thing| skill.allows(t.options);
    let input = FrameInput {
        map,
        tris: sector_tris,
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
        verts_to_patch.extend(wire_z_verts(plan, map, old, links));
    }
    let mut things_to_patch: HashSet<ThingKey> = plan.things_changed.iter().copied().collect();
    let mut sectors_to_patch: HashSet<SectorKey> = plan.sectors_changed.iter().copied().collect();
    if atlas_repacked {
        // Tile origins, sprite UVs and sprite extents may change without editing the map.
        things_to_patch.extend(map.things.keys());
        sectors_to_patch.extend(map.sectors.keys());
    }
    let selection_changed = *last_selection != *selection;
    let mut line_sel_changed = false;
    if selection_changed {
        let old_items: HashSet<SelItem> = last_selection.items().iter().copied().collect();
        let new_items: HashSet<SelItem> = selection.items().iter().copied().collect();
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
    }
    let highlights_changed = *last_highlighted != highlighted;
    if highlights_changed {
        let old_tint: HashSet<SectorKey> = last_highlighted.iter().copied().collect();
        let new_tint: HashSet<SectorKey> = highlighted.iter().copied().collect();
        sectors_to_patch.extend(old_tint.symmetric_difference(&new_tint));
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
    let things_dirty =
        atlas_repacked || !plan.things_changed.is_empty() || !plan.things_removed.is_empty();
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
        for key in plan.lines_removed.iter().chain(&plan.lines_changed) {
            let Some(line) = old.lines.get(*key) else {
                continue;
            };
            let pair = vert_pair(line.v1, line.v2);
            if !orphaned.remove(&pair) {
                continue;
            }
            let twin = links
                .vertices
                .get(&pair.0)
                .into_iter()
                .flatten()
                .copied()
                .filter(|&key| {
                    map.lines
                        .get(key)
                        .is_some_and(|line| vert_pair(line.v1, line.v2) == pair)
                })
                .max();
            if let Some(key) = twin {
                edge_lines.insert(pair, key);
            }
        }
    }

    let snapshot = last_synced
        .as_mut()
        .expect("reconcile diffed this snapshot");
    links.update(snapshot, map, &plan.lines_changed, &plan.lines_removed);
    sync_map_snapshot(snapshot, map);
    if selection_changed {
        *last_selection = selection.clone();
    }
    if highlights_changed {
        last_highlighted.clone_from(&highlighted);
    }
    *last_grid_z = camera.grid_z();
    *last_fill = *sector_fill;
    *last_pixel_ratio = pixel_ratio;
}

/// Vertices whose wireframe floor Z can move under `plan`: endpoints of rewall lines, plus endpoints of removed lines and removed sectors' border lines (resolved via the pre-edit map).
fn wire_z_verts(
    plan: &ReconcilePlan,
    map: &EditorMap,
    old: &EditorMap,
    links: &MapLinks,
) -> HashSet<VertKey> {
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
    for sector in &plan.sectors_removed {
        for &key in links.sectors.get(sector).into_iter().flatten() {
            if let Some(line) = old.lines.get(key) {
                verts.insert(line.v1);
                verts.insert(line.v2);
            }
        }
    }
    verts
}

/// The one full build (map load / deliberate invalidation): every cache + the whole mesh.
pub(crate) fn push_wgpu_frame(state: &mut EditorSession, pixel_ratio: f32) {
    if !state.wgpu.is_ready() {
        return;
    }
    ensure_thing_sprites(state);
    refresh_atlases(state, None);
    full_sync(state, pixel_ratio);
}

/// Build all caches + GPU buffers from scratch and snapshot for future reconciles.
fn full_sync(state: &mut EditorSession, pixel_ratio: f32) {
    let Some(map) = &state.map_editor.map else {
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

    let highlighted = state.map_editor.highlighted_sectors();
    let skill = state.map_editor.skill_filter;
    let visible = move |t: &Thing| skill.allows(t.options);
    let (frame, slots) = {
        let input = frame_input(state, pixel_ratio, &highlighted, &visible);
        let built = frame::build_map_geometry(&input);
        state.wgpu.set_grid_style(frame::grid_style(&input));
        state.wgpu.set_grid_z(input.grid_z);
        built
    };
    let brightness = compute_brightness(
        state.map_editor.map.as_ref().expect("checked above"),
        state.map_editor.sector_fill,
        &state.map_render.light_anim,
    );
    state
        .wgpu
        .set_sector_data(&brightness, &frame.sector_attrs, &frame.sector3d);
    state.wgpu.upload_map(&frame);
    state.wgpu.set_fill_mode(state.map_editor.sector_fill);
    state.wgpu.set_overlay(&[], &[]);
    state.map_editor.surface_mesh = frame.surface3d; // retained CPU mirror for picking + patches
    state.map_render.surface_slots = slots;
    state.map_editor.rebuild_bvh();
    if let Some(map) = &mut state.map_editor.map {
        state.map_render.links = MapLinks::build(map);
        let snapshot = state
            .map_render
            .last_synced
            .get_or_insert_with(EditorMap::default);
        sync_map_snapshot(snapshot, map);
    }
    state.map_render.last_selection = state.map_editor.selection.clone();
    state.map_render.last_highlighted = highlighted;
    state.map_render.last_grid_z = state.map_editor.camera.grid_z();
    state.map_render.last_fill = state.map_editor.sector_fill;
    state.map_render.last_pixel_ratio = pixel_ratio;
    state.map_render.last_resource_generation = Some(state.resource_generation);
    state.map_render.last_asset_gen = state.assets.as_ref().map(|assets| assets.generation());
    state.map_render.light_set_dirty = true;
}

/// Update procedural grid uniforms for the current view without rebuilding the mesh.
pub(crate) fn regrid(state: &EditorSession, pixel_ratio: f32) {
    if state.wgpu.is_ready() {
        set_grid_params(state, pixel_ratio);
    }
}

/// Upload the edit preview + transform handles to the GPU overlay layer (no repaint).
fn upload_edit_preview(state: &mut EditorSession, pixel_ratio: f32) {
    if !state.wgpu.is_ready() {
        return;
    }
    let z = state.map_editor.camera.grid_z();
    let lines = &mut state.map_render.preview_lines;
    let markers = &mut state.map_render.preview_markers;
    frame::build_preview(
        lines,
        markers,
        &state.map_editor.overlay,
        &state.map_editor.style,
        pixel_ratio,
        z,
    );
    if let Some(h) = state.map_editor.transform_handles() {
        frame::push_handles(lines, markers, &h, &state.map_editor.style, pixel_ratio, z);
    }
    state.wgpu.set_overlay(lines, markers);
}

/// Terminal step of every render path: paint the cached mesh + grid with the camera.
pub(crate) fn paint(state: &EditorSession, size: [u32; 2]) -> Option<UiImage> {
    if !state.wgpu.is_ready() {
        return None;
    }
    state
        .wgpu
        .set_grid_on_top(state.map_editor.editing_active());
    state
        .wgpu
        .set_overlays_visible(state.map_editor.overlays_visible);
    state
        .wgpu
        .paint(state.map_editor.render_camera(), size[0], size[1])
}

/// Re-pack the GPU atlases if the content key moved; true = wall spans must re-emit. A cache hit remaps tiles for `dirty_sectors` only (`None` = every sector, the load path).
fn refresh_atlases(state: &mut EditorSession, dirty_sectors: Option<&[SectorKey]>) -> bool {
    if state.map_editor.map.is_none() || !state.ensure_assets() {
        return false;
    }
    state.refresh_missing_resources();
    let map_wad = state.texture_wad();
    let resource_generation = state.resource_generation;
    let EditorSession {
        map_editor: app,
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
    map_render.last_resource_generation = Some(resource_generation);

    let key = fnv_fold(
        atlas::content_key(assets, map, sprites, &names),
        resource_generation,
    );
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
#[expect(
    clippy::cast_precision_loss,
    reason = "map and viewport coordinates deliberately use float precision; GPU and image records retain their packed widths"
)]
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
#[expect(
    clippy::cast_precision_loss,
    reason = "map and viewport coordinates deliberately use float precision; GPU and image records retain their packed widths"
)]
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
#[expect(
    clippy::cast_precision_loss,
    reason = "map and viewport coordinates deliberately use float precision; GPU and image records retain their packed widths"
)]
fn upload_brightness(state: &EditorSession) {
    if state.map_editor.map.is_none() {
        return;
    }
    for light in &state.map_render.light_anim {
        let v = light.current.clamp(0, 255) as f32 / 255.0;
        state.wgpu.patch_brightness(light.sector.slot(), v);
    }
}

/// Push grid style + plane height for the current view. No-op without a loaded map.
fn set_grid_params(state: &EditorSession, pixel_ratio: f32) {
    if state.map_editor.map.is_none() {
        return;
    }
    let visible = |_: &Thing| true;
    let input = frame_input(state, pixel_ratio, &[], &visible);
    state.wgpu.set_grid_style(frame::grid_style(&input));
    state.wgpu.set_grid_z(input.grid_z);
}

fn frame_input<'a>(
    state: &'a EditorSession,
    pixel_ratio: f32,
    highlighted: &'a [SectorKey],
    thing_visible: &'a dyn Fn(&Thing) -> bool,
) -> FrameInput<'a> {
    FrameInput {
        map: state.map_editor.map.as_ref().expect("checked by caller"),
        tris: &state.map_render.sector_tris,
        pixel_ratio,
        style: &state.map_editor.style,
        selection: &state.map_editor.selection,
        grid: state.map_editor.grid,
        fill: state.map_editor.sector_fill,
        selected_sectors: highlighted,
        thing_visible,
        thing_extents: &state.map_editor.thing_extents,
        thing_colors: &state.map_editor.thing_colors,
        atlas: &state.map_render.atlas_maps,
        thing_radius: &defaults::thing_radius,
        sector_gradient: state.prefs.sector_gradient.gradient(),
        highlight_unenclosed: state.map_editor.highlight_unenclosed,
        mode: state.map_editor.camera.mode(),
        grid_z: state.map_editor.camera.grid_z(),
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
fn ensure_thing_sprites(state: &mut EditorSession) {
    if state.map_editor.map.is_none() || !state.ensure_assets() {
        return;
    }
    let EditorSession {
        wad_data,
        project,
        assets,
        thing_sprites,
        map_editor: app,
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

pub(crate) fn light_anim_active(state: &EditorSession) -> bool {
    state.prefs.light_anim
        && state.map_editor.sector_fill == SectorFill::Texture
        && state.bsp_anim.is_none()
        && !state.map_render.light_anim.is_empty()
}

/// Reconcile light effects and report whether animation is active. Camera changes keep phases.
pub(crate) fn refresh_light_anim(state: &mut EditorSession, set_may_change: bool) -> bool {
    if set_may_change {
        let next = match (&state.map_editor.map, state.map_editor.sector_fill) {
            (Some(map), SectorFill::Texture) => light_anim::build(map),
            _ => Vec::new(),
        };
        let same_set = next.len() == state.map_render.light_anim.len()
            && next
                .iter()
                .zip(&state.map_render.light_anim)
                .all(|(a, b)| a.sector == b.sector);
        if !same_set {
            // Revert live slots leaving the set; removed keys may have been reused.
            if let Some(map) = &state.map_editor.map {
                for old in &state.map_render.light_anim {
                    if map.sectors.contains(old.sector)
                        && !next.iter().any(|n| n.sector == old.sector)
                    {
                        let v =
                            sector_brightness(map, old.sector, state.map_editor.sector_fill, &[]);
                        state.wgpu.patch_brightness(old.sector.slot(), v);
                    }
                }
            }
            state.map_render.light_anim = next;
        }
    }
    if light_anim_active(state) {
        true
    } else {
        stop_light_anim(state);
        false
    }
}

/// Revert animated slots to authored brightness and clear the active effects.
fn stop_light_anim(state: &mut EditorSession) {
    if let Some(map) = &state.map_editor.map {
        for light in &state.map_render.light_anim {
            if map.sectors.contains(light.sector) {
                let v = sector_brightness(map, light.sector, state.map_editor.sector_fill, &[]);
                state.wgpu.patch_brightness(light.sector.slot(), v);
            }
        }
    }
    state.map_render.light_anim.clear();
}

pub(crate) fn light_tic(state: &mut EditorSession) -> bool {
    if !light_anim_active(state) {
        stop_light_anim(state);
        return false;
    }
    light_anim::tic(&mut state.map_render.light_anim);
    upload_brightness(state);
    true
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::level_editor::LevelEditorState;
    use crate::render::wgpu;
    use wad::WadData;

    fn e1m1() -> EditorMap {
        let wad = WadData::new(&test_utils::doom1_wad_path());
        editor_core::import_wad_map(&wad, "E1M1").expect("E1M1 imports")
    }

    #[test]
    fn tracked_reconciliation_matches_full_diffs_across_edits_and_undo() {
        use crate::undo::{EditAction, UndoStack};
        use editor_core::geom::split_line_at;

        fn check(snapshot: &mut EditorMap, map: &mut EditorMap, links: &mut MapLinks) {
            let plan = plan_reconcile_with_links(snapshot, map, links);
            // Independent copies discard mutation logs and rebuild incidence.
            let expected = plan_reconcile(&snapshot.clone(), &map.clone());
            assert_eq!(plan, expected);
            links.update(snapshot, map, &plan.lines_changed, &plan.lines_removed);
            sync_map_snapshot(snapshot, map);
            assert_eq!(snapshot, map);
        }

        let mut map = e1m1();
        let mut snapshot = EditorMap::default();
        sync_map_snapshot(&mut snapshot, &mut map);
        let mut links = MapLinks::build(&snapshot);
        let mut undo = UndoStack::new();
        let line = map.lines.keys().next().unwrap();
        let vertex = map.lines[line].v1;
        let sector = map.sectors.keys().next().unwrap();
        let thing = map.things.keys().next().unwrap();

        undo.record(EditAction::MoveSelection, &map);
        map.vertices[vertex].x += 8.;
        map.sectors[sector].floor_height += 8;
        check(&mut snapshot, &mut map, &mut links);

        undo.record(EditAction::SplitLines, &map);
        let edge = map.lines[line];
        let a = map.vertices[edge.v1];
        let b = map.vertices[edge.v2];
        split_line_at(
            &mut map,
            line,
            [f32::midpoint(a.x, b.x), f32::midpoint(a.y, b.y)],
        );
        check(&mut snapshot, &mut map, &mut links);

        undo.record(EditAction::DeleteSelection, &map);
        let mut replacement = map.things.remove(thing).unwrap();
        replacement.kind = 3004;
        map.things.insert(replacement);
        check(&mut snapshot, &mut map, &mut links);

        for _ in 0..3 {
            undo.undo(&mut map).unwrap();
            check(&mut snapshot, &mut map, &mut links);
        }
        for _ in 0..3 {
            undo.redo(&mut map).unwrap();
            check(&mut snapshot, &mut map, &mut links);
        }
        // Mutable iteration may expose unchanged values; these are not edits.
        for _ in map.lines.values_mut() {}
        assert_eq!(
            plan_reconcile_with_links(&snapshot, &map, &links),
            ReconcilePlan::default()
        );
    }

    /// CPU planning and snapshot traffic, with identical incidence caches in both paths.
    #[test]
    #[ignore = "manual optimized profiling workload"]
    fn profile_reconciliation() {
        use std::{hint::black_box, time::Instant};
        let wad = WadData::new(&test_utils::doom1_wad_path());
        for name in ["E1M1", "E1M8"] {
            let fixture = editor_core::import_wad_map(&wad, name).unwrap();
            for tracked in [false, true] {
                let mut map = fixture.clone();
                let mut snapshot = map.clone();
                if tracked {
                    sync_map_snapshot(&mut snapshot, &mut map);
                }
                let mut links = MapLinks::build(&snapshot);
                let vertex = map.vertices.keys().next().unwrap();
                let sector = map.sectors.keys().next().unwrap();
                let line = map.lines.keys().next().unwrap();
                let thing = map.things.keys().next().unwrap();
                let started = Instant::now();
                for step in 0..10000 {
                    let delta: i16 = if step % 8 < 4 { 1 } else { -1 };
                    match step % 4 {
                        0 => map.vertices[vertex].x += f32::from(delta),
                        1 => map.sectors[sector].floor_height += i32::from(delta),
                        2 => map.lines[line].special ^= 1,
                        _ => map.things[thing].angle ^= 1,
                    }
                    let plan = plan_reconcile_with_links(&snapshot, &map, &links);
                    links.update(&snapshot, &map, &plan.lines_changed, &plan.lines_removed);
                    if tracked {
                        sync_map_snapshot(&mut snapshot, &mut map);
                    } else {
                        snapshot = map.clone();
                    }
                    black_box(&plan);
                }
                println!(
                    "EDITOR_RECONCILE: {name}, tracked={tracked}, 10000 edits: {:?}",
                    started.elapsed()
                );
                assert_eq!(snapshot, map);
            }
        }
    }

    #[test]
    fn selected_line_outline_survives_mesh_reconciliation() {
        let Ok(context) = wgpu::initialize() else {
            return;
        };
        let map = e1m1();
        let key = map.lines.keys().next().unwrap();
        let line = map.lines[key];
        let a = map.vertices[line.v1];
        let b = map.vertices[line.v2];
        let mut app = LevelEditorState::new();
        app.load_map(map, "E1M1");
        app.camera.set_viewport(256.0, 256.0);
        app.camera.set_zoom(1.0);
        let [x, y] = app
            .camera
            .world_to_screen([a.x.midpoint(b.x), a.y.midpoint(b.y)]);
        app.camera.pan(128.0 - x, 128.0 - y);
        app.style.point = [255, 0, 255, 255];
        let mut state = EditorSession::new(app, None, Default::default());
        state.wgpu = context;
        push_wgpu_frame(&mut state, 1.0);
        state.map_editor.selection.replace(SelItem::Line(key));
        let corners = state.map_editor.transform_handles().unwrap().corners;
        let middle = [
            corners[0][0].midpoint(corners[1][0]),
            corners[0][1].midpoint(corners[1][1]),
        ];
        let [x, y] = state.map_editor.camera.world_to_screen(middle);
        for damage in [Damage::Edited, Damage::Restyle] {
            apply_damage(&mut state, damage, 1.0);
            let pixels = state
                .wgpu
                .render_canvas_rgba(state.map_editor.render_camera(), 256, 256)
                .unwrap();
            let mut visible = false;
            for py in 0..256_u16 {
                for px in 0..256_u16 {
                    if (f32::from(px) - x).abs() <= 2.0 && (f32::from(py) - y).abs() <= 2.0 {
                        let offset = (usize::from(py) * 256 + usize::from(px)) * 4;
                        visible |= pixels[offset] > 240
                            && pixels[offset + 1] < 10
                            && pixels[offset + 2] > 240;
                    }
                }
            }
            assert!(
                visible,
                "selection outline must remain visible after {damage:?}"
            );
        }
    }

    #[test]
    fn normal_ticks_keep_their_screen_length_when_zoom_changes() {
        let Ok(context) = wgpu::initialize() else {
            return;
        };
        let map = e1m1();
        let key = map.lines.keys().next().unwrap();
        let line = map.lines[key];
        let a = map.vertices[line.v1];
        let b = map.vertices[line.v2];
        let centre = [a.x.midpoint(b.x), a.y.midpoint(b.y)];
        let mut app = LevelEditorState::new();
        app.load_map(map, "E1M1");
        app.camera.set_viewport(256.0, 256.0);
        app.camera.set_zoom(1.0);
        let mut state = EditorSession::new(app, None, Default::default());
        state.wgpu = context;
        state.wgpu.set_overlays_visible(false);
        state.wgpu.set_fill_mode(SectorFill::None);
        for ratio in [1_u16, 2] {
            let size = 256 * u32::from(ratio);
            let input = frame_input(&state, f32::from(ratio), &[], &|_| true);
            let (_, mut normal) = frame::line_instances(&input, key);
            normal.rgba = [1.0, 0.0, 1.0, 1.0];
            state.wgpu.upload_map(&wgpu::MapFrame {
                normals: vec![normal],
                ..Default::default()
            });
            let mut counts = [0; 3];
            for (count, zoom) in counts.iter_mut().zip([1.0, 0.25, 4.0]) {
                state.map_editor.camera.set_zoom(zoom);
                let [x, y] = state.map_editor.camera.world_to_screen(centre);
                state.map_editor.camera.pan(128.0 - x, 128.0 - y);
                apply_damage(&mut state, Damage::View, f32::from(ratio));
                let pixels = state
                    .wgpu
                    .render_canvas_rgba(state.map_editor.render_camera(), size, size)
                    .unwrap();
                *count = pixels
                    .as_chunks::<4>()
                    .0
                    .iter()
                    .filter(|p| p[0] > 240 && p[1] < 10 && p[2] > 240)
                    .count();
            }
            assert!(
                counts[0] > 0 && counts.iter().all(|n| n.abs_diff(counts[0]) <= 2),
                "normal pixels at zoom 1, 0.25, 4 and DPI {ratio}: {counts:?}"
            );
        }
    }

    #[test]
    fn dpi_change_resizes_markers_without_retriangulating_sectors() {
        let Ok(context) = wgpu::initialize() else {
            return;
        };
        let mut map = e1m1();
        map.things.retain(|_, _| false);
        let vertex = map.vertices.values().next().unwrap();
        let centre = [vertex.x, vertex.y];
        let mut app = LevelEditorState::new();
        app.load_map(map, "E1M1");
        app.camera.set_viewport(256.0, 256.0);
        app.camera.set_zoom(1.0);
        let [x, y] = app.camera.world_to_screen(centre);
        app.camera.pan(128.0 - x, 128.0 - y);
        app.sector_fill = SectorFill::None;
        app.style.point = [255, 0, 255, 255];
        let mut state = EditorSession::new(app, None, Default::default());
        state.wgpu = context;
        push_wgpu_frame(&mut state, 1.0);
        let render = |state: &EditorSession, size| {
            state
                .wgpu
                .render_canvas_rgba(state.map_editor.render_camera(), size, size)
                .unwrap()
        };
        let marker_pixels = |bytes: &[u8]| {
            bytes
                .as_chunks::<4>()
                .0
                .iter()
                .filter(|pixel| pixel[0] > 240 && pixel[1] < 10 && pixel[2] > 240)
                .count()
        };
        let normal = marker_pixels(&render(&state, 256));
        assert!(
            normal > 0,
            "the fixture must contain visible vertex markers"
        );
        let allocations: Vec<_> = state
            .map_render
            .sector_tris
            .sector
            .iter()
            .map(|(&sector, tris)| (sector, tris.as_ptr()))
            .collect();

        apply_damage(&mut state, Damage::View, 2.0);
        let retina = marker_pixels(&render(&state, 512));
        assert!(
            retina > normal * 3 && retina < normal * 5,
            "markers should keep their logical size: {normal} pixels at 1x, {retina} at 2x"
        );
        for (sector, allocation) in allocations {
            assert_eq!(
                state.map_render.sector_tris.sector[&sector].as_ptr(),
                allocation
            );
        }
    }

    /// `ensure_thing_sprites` early-returned on maps with no things, blocking `refresh_atlases` (needs the sprite cache); result: atlas empty → textured 3D surfaces sampled nothing, canvas showed only lines.
    #[test]
    fn map_without_things_still_textures_walls_and_floors() {
        let mut map = e1m1();
        map.things.retain(|_, _| false);

        let mut app = LevelEditorState::new();
        app.load_map(map, "E1M1");
        let mut state =
            EditorSession::new(app, Some(test_utils::doom1_wad_path()), Default::default());

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

    #[test]
    #[expect(
        clippy::too_many_lines,
        reason = "exercise resource replacement and incremental texture refresh through the real renderer"
    )]
    fn resource_reload_and_texture_edits_refresh_atlases_without_retriangulating() {
        let Ok(context) = wgpu::initialize() else {
            return;
        };
        let mut app = LevelEditorState::new();
        app.load_map(e1m1(), "E1M1");
        app.set_viewport(256.0, 256.0);
        app.zoom_fit();
        app.sector_fill = SectorFill::Texture;
        let mut state =
            EditorSession::new(app, Some(test_utils::doom1_wad_path()), Default::default());
        state.wgpu = context;
        push_wgpu_frame(&mut state, 1.0);
        let before = state
            .wgpu
            .render_canvas_rgba(state.map_editor.render_camera(), 256, 256)
            .unwrap();
        let allocations: Vec<_> = state
            .map_render
            .sector_tris
            .sector
            .iter()
            .map(|(&key, tris)| (key, tris.as_ptr()))
            .collect();
        let generation = state.assets.as_ref().unwrap().generation();
        let sprite_kind = state
            .map_editor
            .map
            .as_ref()
            .unwrap()
            .things
            .values()
            .map(|thing| thing.kind)
            .find(|&kind| state.thing_sprites.as_ref().unwrap().get(kind).is_some())
            .unwrap();

        let unique = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let path = std::env::temp_dir().join(format!(
            "editor-palette-{}-{unique}.wad",
            std::process::id()
        ));
        let pnames = state
            .wad_data
            .as_ref()
            .unwrap()
            .get_lump("PNAMES")
            .unwrap()
            .data
            .clone();
        wad::write::save_pwad(
            &path,
            &[
                wad::Lump {
                    name: "PLAYPAL".into(),
                    data: [255, 0, 0].repeat(256),
                },
                wad::Lump {
                    name: "PNAMES".into(),
                    data: pnames,
                },
            ],
        )
        .unwrap();
        assert!(state.add_pwad(path.clone()));
        assert!(state.ensure_assets());
        std::fs::remove_file(path).unwrap();
        assert!(
            state.assets.as_ref().unwrap().generation() == generation,
            "reloaded resources start with the same local asset edit counters"
        );
        apply_damage(&mut state, Damage::Edited, 1.0);
        let after = state
            .wgpu
            .render_canvas_rgba(state.map_editor.render_camera(), 256, 256)
            .unwrap();
        let changed = before.iter().zip(&after).filter(|(a, b)| a != b).count();
        assert!(
            changed > before.len() / 100,
            "the replacement palette must reach the GPU; {changed} framebuffer bytes changed"
        );
        assert!(
            state
                .thing_sprites
                .as_ref()
                .unwrap()
                .get(sprite_kind)
                .is_some(),
            "resource reload restores decoded sprites"
        );

        let name = *state
            .map_render
            .atlas_maps
            .wall_rects
            .keys()
            .next()
            .unwrap();
        let assets = state.assets.as_mut().unwrap();
        let index = assets
            .textures()
            .iter()
            .position(|texture| texture.name == name)
            .unwrap();
        let texture = assets.texture_mut(index).unwrap();
        texture.width += 7;
        let width = texture.width;
        assets.refresh_texture_index();
        apply_damage(&mut state, Damage::Edited, 1.0);
        assert_eq!(
            state.map_render.atlas_maps.wall_rects[&name].w,
            u32::try_from(width).unwrap()
        );
        for (key, allocation) in allocations {
            assert_eq!(
                state.map_render.sector_tris.sector[&key].as_ptr(),
                allocation,
                "resource changes keep existing sector triangulations"
            );
        }
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
        let patched = wire_z_verts(&plan, &new, &old, &MapLinks::build(&old));

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
