# Canvas damage system

The editor canvas is **push-driven**, not polled. Every input op (click, drag,
edit, pan, theme change) returns a `Damage` describing *what changed*; the glue
hands it to `apply_damage`, which does the least work that covers it. There is no
render loop — the canvas repaints only when a `Damage` says it must.

The canvas is **one wgpu off-screen texture** handed to Slint as an `Image`. There
are no software tiles (an older design the `Damage::Pan`/`Zoom` doc comments still
echo). "Repaint" = re-encode that one texture.

## The pieces

- `Damage` (`state.rs`) — the change kind an op reports.
- `ChangedElems` (`state.rs`) — for `Damage::Patch`, which element slots changed.
- `apply_damage` (`render/sync.rs`) — the sole consumer; matches on `Damage` and
  calls one dispatch helper.
- The glue (`views/view_canvas.rs::after_edit` and peers) — calls the op, then
  `apply_damage(ui, shared, damage)`. Nothing else consumes a `Damage`.

It is a return-value contract, not a pub/sub bus: an op *returns* a `Damage`, the
glue *passes* it to `apply_damage`. To follow "what happens on edit X", read the
op (what it returns) and `apply_damage` (what that triggers) — two functions.

## The variants, cheapest → most expensive

| `Damage`   | Means                                  | `apply_damage` does                                    |
|------------|----------------------------------------|--------------------------------------------------------|
| `None`     | nothing changed                        | returns immediately                                    |
| `Overlay`  | in-progress preview moved              | rebuild the overlay layer, repaint; no grid/mesh/camera|
| `Pan`/`Zoom`| camera moved                          | regrid (2D only) + repaint; mesh reused                |
| `Repaint`  | view-wide redraw (grid spacing, filter)| rebuild grid + repaint; mesh reused                    |
| `Patch(e)` | named elements changed value/position  | rewrite those GPU slots in place; no mesh rebuild      |
| `Geometry` | topology / counts changed              | rebuild caches + re-upload the whole mesh              |

`Geometry` is the only one that rebuilds the mesh; everything above it keeps the
uploaded geometry and touches only the camera, grid, overlay, or single slots.

A **theme change is `Geometry`, not `Repaint`**: line/vertex/sector-fill colours
are baked into the persistent mesh by `build_map_geometry`, so recolouring needs
a rebuild. `Repaint` only regenerates the grid (whose colour it does refresh),
which is why a theme switch on a `Repaint` would update the grid + GPU clear but
leave the map lines their old colour. A chrome-only change (glass vibrancy) stays
`Repaint` — it touches no baked colour.

### Patch vs Geometry — the dividing line

The GPU mesh is **fixed-stride per element**: line `i` lives at slot `i`, vertex
`i` at slot `i`, etc. So an edit that changes a value/position **without changing
the element counts or topology** is a `Patch` — it rewrites those slots in place
(`patch_line`/`patch_vert`/`patch_thing`/`patch_sector_attr`/`patch_sector_3d`).
Anything that adds/removes/splits/welds/merges, or moves a vertex such that the
sector tracer must re-run, is `Geometry` — the slot layout itself changed, so the
whole mesh is rebuilt and re-uploaded.

`apply_sector` is the canonical classifier:
- height change → `Geometry` (reshapes the 3D surface mesh)
- flat change → `Patch(ChangedElems::sector_flat)` (re-pack atlas, then patch)
- light/tint → `Patch(ChangedElems::sector)` (attr patch only)

`ChangedElems` separates `sectors` (patch per-sector GPU attrs in place) from
`sector_flats` (also changed the packed flat → the atlas re-packs before the
patch). A selection/light/tint touches `sectors` only and never re-packs.

## `Damage::combine`

When one handler produces several changes (paste = added geometry + sector
records; wall edit = a line patch + a sector patch), `combine` folds them into the
one that covers both:

- `None` is the identity.
- `Geometry` dominates everything (a whole-map rebuild covers any change).
- `Patch ⊔ Patch` unions their `ChangedElems`.
- `Patch` ⊔ a pure view/overlay change stays `Patch` (its repaint already applies
  the current camera).
- any other non-geometry pair widens to `Repaint`.

## Dispatch helpers (`render/sync.rs`)

- `set_edit_preview` (`Overlay`) — rebuild rubber/line/shape/move instances on the
  shared overlay layer at the camera's `grid_z`, repaint.
- `regrid_and_paint` (`Pan`/`Zoom`/`Repaint`) — 2D regenerates the visible-rect
  grid; 3D reuses the map-anchored grid unless `force_grid` (Repaint); repaint.
- `patch_elements` (`Patch`) — re-pack the atlas iff `sector_flats`; rewrite the
  changed line/vert/thing/sector slots; refresh the grid (a 3D pick may have moved
  the plane); clear the overlay; repaint.
- `push_wgpu_frame` (`Geometry`) — rebuild thing sprites + atlases + brightness,
  rebuild and upload the whole mesh, regenerate the grid, retain the CPU mesh for
  ray-pick, clear the overlay, repaint.

## Cross-cutting effects

- **Build-anim overlay** is cleared on any non-`None` damage (navigation/edit
  desyncs it from the view it started in).
- **Light animation** (`refresh_light_anim`) reconciles its set only on `Geometry`
  or `Repaint` (the light *set* can change then); a `Pan`/`Zoom`/`Patch` must not,
  or every light re-spawns at full bright and flickers.
- **Panel re-sync** (`panels_key`) is forced (set `None`) on `Geometry` and on a
  flat-changing `Patch`, so the inspector re-reads values the key alone wouldn't
  catch.

## Adding an op

1. Mutate the model via a kernel op; record undo first.
2. Return the *cheapest* `Damage` that covers the change — `Patch` if counts are
   unchanged, `Geometry` if topology moved, a view variant for camera-only.
3. The glue passes it to `apply_damage`; do not call paint/upload directly.
4. If a `Patch`, fill the right `ChangedElems` lists (and `sector_flats` when a
   flat changed). Under-reporting leaves stale pixels; over-reporting (`Geometry`
   where a `Patch` fits) just costs a needless rebuild.
