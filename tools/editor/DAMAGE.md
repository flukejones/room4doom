# Canvas damage system

The editor canvas is **push-driven**, not polled. Every input op (click, drag,
edit, pan, theme change) returns a `Damage` describing *what kind of thing
changed*; the glue hands it to `apply_damage`, which does the least work that
covers it. There is no render loop — the canvas repaints only when a `Damage`
says it must.

The canvas is **one wgpu off-screen texture** handed to Slint as an `Image`.
"Repaint" = re-encode that one texture.

## The pieces

- `Damage` (`state.rs`) — the change kind an op reports.
- `apply_damage` (`render/sync.rs`) — the sole consumer; matches on `Damage` and
  calls one dispatch helper.
- The reconciler (`render/sync.rs::reconcile` → `plan_reconcile` +
  `apply_reconcile`) — figures out *which elements* changed by diffing, so ops
  never have to say.
- The glue (`views/view_canvas.rs::after_edit` and peers) — calls the op, then
  `apply_damage(ui, shared, damage)`. Nothing else consumes a `Damage`.

It is a return-value contract, not a pub/sub bus: an op *returns* a `Damage`,
the glue *passes* it to `apply_damage`. Ops do **not** classify what they
touched — that was the old `Patch(ChangedElems)`/`Geometry` split, and it is
gone. An edit is just `Edited`; the reconciler derives the affected set itself.

## The variants, cheapest → most expensive

| `Damage`  | Means                                   | `apply_damage` does                                   |
|-----------|-----------------------------------------|-------------------------------------------------------|
| `None`    | nothing changed                         | returns immediately                                    |
| `Overlay` | in-progress preview moved               | rebuild the overlay layer, repaint; no grid/mesh/camera|
| `View`    | camera moved                            | regrid + repaint; mesh reused                          |
| `Repaint` | view-wide redraw (grid spacing, filter) | regrid + repaint; mesh reused                          |
| `Edited`  | the map or selection changed            | reconcile: diff → patch exactly the affected slots/spans |
| `Restyle` | baked instance colours went stale (theme/gradient) | drop `last_synced`, reconcile → the one full rebuild |

`combine` folds multiple results: `None` is the identity, `Restyle` then
`Edited` dominate, any other pair widens to `Repaint`. `apply_damage` owns one
more escalation: a `View`/`Repaint` arriving while the grid plane differs from
`last_grid_z` routes to reconcile (the plane-riding instance layers are stale) —
ops that move the plane just combine `Damage::View` in; they never decide the
tier themselves.

## Reconciliation (the `Edited` path)

`map_render.last_synced` holds a keyed clone of the map as last pushed to the
GPU (plus `last_selection`, `last_highlighted`, `last_grid_z`, `last_fill`).
On `Edited`, `reconcile`:

1. **Diffs** each arena (`vertices`/`lines`/`sectors`/`things`) against
   `last_synced` — stable generational keys make this a per-key equality scan.
   Undo/redo/paste/delete need no special path: snapshots preserve keys, so an
   undone delete simply diffs as the line reappearing at its old key.
2. **Derives dirty sets** (`plan_reconcile`, pure and unit-tested):
   - re-triangulate (`retri`): sectors whose XY outline changed — structural
     line changes (endpoints, sector assignment, add/remove) and moved vertices.
   - re-emit fill spans (`respan_sectors`): `retri` ∪ height-changed sectors.
     Height changes re-emit at the new Z **without** re-triangulating.
   - re-emit wall spans (`rewall`): changed lines ∪ lines touching moved
     vertices ∪ border lines of height-changed sectors — or every line after an
     atlas repack (rects moved; still no triangulation).
   - instance/storage patches: changed/removed elements, plus selection and
     sector-tint diffs (selection is diffed too, not reported by ops). Line
     segment + normal instances follow `rewall`, not the line-record diff —
     they derive from vertex positions and sector heights.
3. **Gates the atlas work**: `flats_changed`/`wall_tex_changed`/
   `thing_kinds_changed` (plus an asset-generation check for texture-editor
   edits) decide whether sprite/atlas refresh runs at all — a vertex nudge or
   height spin skips every atlas pass. A repack folds "re-emit all wall spans"
   into the plan's `rewall`.
4. **Applies**: per-sector `retriangulate_sector`; span rewrites through
   `SurfaceSlots` into the CPU mirror `app.surface_mesh` (overflowing spans
   relocate to the tail, old region tombstoned); `wgpu.patch_surface` per span;
   instance slots patched by arena slot (removed → tombstone instance); sector
   storage patched by slot; wire layer rebuilt wholesale when fill == None;
   BVH `refit` (thing payloads only refreshed when the plan touched things;
   `covers`/rebuild checked only when things or the mesh length moved);
   `edge_lines` re-keyed only on structural line change.
5. **Snapshots** the map/selection as the new `last_synced`.

A grid-plane or fill-mode change (tracked via `last_grid_z`/`last_fill`)
re-emits the instance layers riding the plane; still no surface re-emission.

**The only full builds** are map load (`last_synced == None` →
`push_wgpu_frame`/`full_sync`, which logs `wgpu full map build`), GPU buffer
capacity exhaustion (re-upload from the CPU mirror — memcpy, no re-emission),
and `Damage::Restyle` (theme/gradient changes recolouring baked instances —
`apply_damage` drops `last_synced` itself; call sites never touch it). Edits
never take a full build.

## Dispatch helpers (`render/sync.rs`)

- `set_edit_preview` (`Overlay`) — rebuild rubber/line/shape/move instances on
  the shared overlay layer at the camera's `grid_z`, repaint.
- `regrid_and_paint` (`View`/`Repaint`) — push grid uniforms for the current
  view, repaint.
- `reconcile` (`Edited`) — as above; falls through to `push_wgpu_frame` when
  nothing is synced yet.

## Cross-cutting effects

- **Build-anim overlay** is cleared on any non-`None` damage (navigation/edit
  desyncs it from the view it started in).
- **Light animation** (`refresh_light_anim`) reconciles its set only on
  `Edited` or `Repaint`; a `View`/`Overlay` must not, or every light re-spawns
  at full bright and flickers.
- **Panel re-sync** (`panels_key`) is forced (set `None`) on every reconcile so
  the inspector re-reads values the selection key alone wouldn't catch.

## Adding an op

1. Mutate the model via a kernel op; record undo first.
2. Return the *cheapest* `Damage` that covers the change — `Edited` for any map
   or selection mutation, a view variant for camera-only, `None` if nothing
   observable moved.
3. The glue passes it to `apply_damage`; do not call paint/upload directly.
4. Do **not** try to describe what changed — the reconciler diffs it out. If a
   new op class dirties something the dirty-set derivation misses, extend
   `plan_reconcile` (and its tests), never add a whole-map rebuild.
