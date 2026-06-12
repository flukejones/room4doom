# Editor camera

The editor canvas is a **fully 3D scene** viewed through a 3D CAD-style camera
(think Fusion 360 / Shapr3D). There is **no 2D top-down mode underneath** — the
"plan view" is just the camera looking straight down. Everything (pick, grid,
billboards, overlays) goes through this one camera.

This file is the contract. If a change makes the camera behave differently from
what is written here, the change is wrong (or this file must be updated with the
new agreed behaviour first).

## Two types

- **`Camera`** (`src/render/camera3d.rs`) — the pure 3D camera. State is an `eye`
  position plus an **orthonormal orientation basis** `right`/`up`/`fwd`. No Euler
  angles. No "heading/pitch" fields. No ground-plane fold. `fwd` is the look
  direction (eye→scene); `right`/`up` are screen +X / +Y.
- **`EditorCamera`** (`src/render/editor_camera.rs`) — wraps `Camera` with the
  viewport, the eased `goal`, the `view_3d` toggle, the editing-plane `grid_z`,
  and the **sticky orbit `pivot`**. Gestures call methods here; `render_camera()`
  hands the single `Camera` to both paint and picking so they can never disagree.

## Non-negotiable expectations

1. **It is a real 3D camera, not 2D-with-a-tilt.** No Euler `heading_deg` /
   `pitch_deg`, no `rot_x(90 - pitch)` fold, no implicit "Z is down". Orientation
   is the basis vectors; rotations act on them directly.

2. **Orbit pivots on ONE point — the point under the cursor.** Clicking an object
   (or starting an orbit) sets a sticky 3D `pivot` from the **mesh hit** under the
   cursor (`pick_mesh(...).world`), falling back to the grid-plane point. Orbit
   then **rigidly rotates the whole rig (eye + basis) about that pivot**, so the
   pivot keeps its screen position — the world appears to pivot on it. The pivot
   stays until the next click/orbit-start sets a new one.

3. **Up-locked turntable (no roll).** Yaw rotates about **world +Z**; pitch
   rotates about the camera's **own right axis**. The view never rolls. Pitch is
   clamped (`PITCH_LIMIT`) so `fwd` never reaches straight up/down (no gimbal
   flip).

4. **Pan is pure screen-axis, at any orbit.** Drag-left moves content left,
   drag-up moves it up — translating the eye along the camera's `right`/`up`
   basis. Pan never touches a scene plane and never drifts diagonally when tilted.

5. **Zoom is cursor-anchored.** The world point under the cursor stays fixed on
   screen; ortho scales `ortho_height` and shifts the eye to re-anchor.

6. **The grid plane snaps to the selected part's own Z, never the click point.**
   The pick returns `grid_z` = the matched element's real height (a vertex's Z, a
   linedef rim's Z, a sector's floor/ceil). See `pick_3d_select` in
   `src/level_editor/view.rs` and the BVH pick docs.

7. **Top-down is a preset, not the foundation.** `top_down()` / `look_down_at()`
   reset the basis to look straight down (`fwd = (0,0,-1)`, `up = (0,1,0)`,
   `right = (1,0,0)`); at that orientation world +X reads screen-right and world
   +Y screen-up, world Z does not shift a point. This is the only place a
   "2D-looking" view comes from — it is the 3D camera pointed down.

## Gesture → behaviour map

Wired in `ui/views/map/map_canvas.slint` → `src/views/view_canvas.rs` →
`EditorCamera`:

| Gesture | Callback | Camera op |
|---|---|---|
| Shift + left-drag (or shift + two-finger drag) | `orbit-start` (press) + `orbit` (move) | set pivot under cursor, then `Camera::orbit(pivot, yaw, pitch)` |
| Middle-drag, or Space + left-drag, or two-finger drag | `pan` | `EditorCamera::pan` (eye along right/up) |
| Wheel / ctrl-scroll / pinch | `zoom-at` / `pinch-updated` | `EditorCamera::zoom` (cursor-anchored) |
| Click on an object (select) | `tool-click` → `select_resolve` | `set_pivot(hit.world)` (sticky orbit centre) |

Orbiting from the plan view enters 3D (`view_3d = true`). Toggling 3D off eases
back to the plan view.

## Sign conventions (the bit that bites)

These are deliberate and were tuned by hand against the running editor. Do not
"fix" them blindly.

- `EditorCamera::orbit(dx, dy)` passes `(-dx, dy)` as `(yaw, pitch)` to
  `Camera::orbit` — yaw negated so a rightward drag turns the view the expected
  way.
- `Camera::orbit` applies pitch as `-pitch` internally so that **positive pitch
  lifts the far edge and world height reads up on screen** (this is the
  projection-handedness fix; without it a tilt mirrors the world across XY).
- The two negations are at different layers and fix different things — the
  EditorCamera one is **gesture feel**, the Camera one is **projection
  correctness**. Flipping the wrong one reintroduces the mirror bug.

A pan/orbit **direction preference** (invert per-axis) is a planned setting; the
sign sites above are where it will hook in.

## Projection

Ortho and perspective share the same view (eye + basis). `view_proj(aspect)` is
eye-at-origin (`rot * translate(-eye)`) then the projection tail. The `rot()`
matrix rows are `right / up / -fwd`, column-major (`m[col][row]`), matching WGSL.
`ray`/`ground_hit_at`/`world_to_ndc` all invert exactly what the GPU draws, so
picking and rendering agree.

## Tests

`src/render/camera3d.rs` and `editor_camera.rs` test modules assert the
invariants above against the **public API only** (no test-only setters added to
production types): plan-view linearity, pivot-stays-fixed-under-orbit,
height-reads-up-when-tilted, ground-hit round-trip, cursor-anchored zoom, fit
centring. If you change the camera, these must stay green or be updated to the new
agreed contract.
