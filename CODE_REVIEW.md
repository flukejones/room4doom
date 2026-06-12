# Code Review Guidelines — room4doom

What to look for when reviewing a commit. Each item is a check, not an assertion.

Items already enforced by clippy / rustc are not repeated here — trust CI for those; review covers what tools cannot. The rules below are the ones that bit us in practice: a Doom port has fixed-point math, a shared BSP3D fed to two renderers (CPU `software3d` + GPU `wgpu3d`), per-frame hot paths, and WGSL/Rust layout coupling that compiles cleanly and fails (or silently corrupts) only at runtime.

Project conventions live in `CLAUDE.md` and `~/.claude/rules/rust.md`; this file is the review lens, not a restatement.

## Allocations, clones, hot paths

The render loop runs every frame; `level/src/bsp3d` mover/UV recompute runs per moving sector per frame. Allocation there is paid continuously.

- **Flag `.clone()` that only satisfies the borrow checker.** First fix ownership: disjoint-field destructure (`let Self { a, b, .. } = self;` to borrow separate fields at once), index-loop to release the read borrow before a `&mut self` write, consume by move, or restructure the call site. A `vertices.clone()` held across a `self.foo()` call that reads disjoint fields is pure waste. (In-tree reference: `recompute_wall_uv` / `triangulate` read the vertex list by index instead of cloning.)
- **Distinguish hot from cold.** A clone/alloc in `draw_view_gpu`, the rasterizer scanline, or `apply_interpolated_heights → recompute_*_uv` is per-frame — flag hard. The same at level load (`BSP3D::new`, atlas bake, `Mesh::build`) is one-time — note it, but it is low priority.
- **Reuse per-frame scratch, don't re-allocate.** wgpu3d holds owned `Vec`s (`positions`, `sector_light`, `wall_xlat`, `corner_attr`, `corner_scroll`) and refills them with `.clear()` + `.extend(...)` each frame, never `let v: Vec<_> = iter.collect()` in the draw call. New per-frame uploads follow that pattern.
- **`Vec<T>` where `[T; N]` fits.** N a compile-time constant (a wall quad is 4 verts, a triangle 3, palette 256) → stack array, not heap `Vec`. Doom format invariants (flats 64×64, 256-entry palettes/colourmaps) are fixed-N.
- **`push`/`pop`/`remove` that reallocs.** Pre-size with `with_capacity` when the count is known (it usually is — poly count, corner count, sector count). Index-fill a pre-sized buffer over growing it.
- **Iterator chains materialising an intermediate `Vec`** the consumer takes as `&[T]`. Pass the slice or the iterator.
- **`to_vec()` / `collect()` to bridge an empty-case guard.** Substitute a static one-element fallback slice; don't copy the whole table in the common path.

## Fixed-point and numeric

- **`FixedT` is the map/physics number; `f32` is the render number.** Don't silently mix. Conversions cross a precision boundary — `f32::from(fixed)` / `FixedT::from_f32` are explicit on purpose. Flag a raw `as` cast across that boundary.
- **WAD fields are unsigned where Doom treats them as offsets.** Blockmap offsets, lump indices: read as `u16`/`u32`, not `i16` then `as usize` (a large map's offset goes negative and overflows on `* word`). The blockmap `read_blockmap` fix is the cautionary tale.
- **`wrapping_*` is deliberate in OPL2 and BAM angle math** — don't "fix" it to checked. Elsewhere, an unchecked `*`/`+` on WAD-derived sizes is an overflow waiting for a big PWAD.
- **Verify against `doom-engines/doom-og-src/`, not memory**, when porting renderer/gameplay behaviour. "Looks right" has been wrong repeatedly (sky projection, masked midtex, blockmap).

## Renderer parity (software3d / software25d / wgpu3d)

Three renderers consume the same BSP3D + pic-data. A fix usually belongs in the shared layer, not one renderer.

- **Shared per-surface math lives in `level::bsp3d`** (`light_band`, `contrast_adjust`, `is_masked_middle`, `corner_uv_texels`), not duplicated per renderer. A `matches!(surface_kind, …)` repeated in two renderers is a missing helper.
- **Texture/UV resolution differences are mechanism, not policy.** software3d reads BSP3D live each frame; wgpu3d mirrors it into GPU buffers gated by a dirty flag. Both must show the same result — when one renders a feature and the other doesn't (scrolling walls, masked-midtex no-tile, animation), the data path diverged.
- **Flag a fix applied to only one renderer** when the cause is in shared data. Ask whether the other renderer has the same latent bug.
- **The index/scene plane is width-strided; the display surface may be padded** (softbuffer IOSurface rows). `DrawBuffer::pitch()` / `get_buf_index()` return the *index* stride (width); surface pitch is used only at `resolve`/`set_pixel`/melt. Mixing them shears the image diagonally and reads OOB.

## Movers, dirty flags, per-frame upload

- **`geometry_dirty` = positions/vertex-z moved; `texture_dirty` = poly_tex/scroll changed.** Keep them separate: scroll dirties every tic, geometry only on movement — folding them forces heavy re-uploads and defeats the idle-frame skip. Both cleared in `d_main` after the GPU upload.
- **A new per-frame GPU upload needs a dirty gate** unless the data genuinely changes every frame (sector light, animation translation, sky uniform do; positions/attrs don't).
- **Movers mutate `vertices[].z` only; topology is stable.** A change that resizes/reorders BSP3D vertex/triangle arrays at runtime breaks the static index buffers — flag it.

## WGSL shaders

Strictly typed and memory-safe, but logical and layout traps remain. Shaders are built by `concat!`-ing `sky_common.wgsl` ahead of `scene.wgsl`/`sky.wgsl` — a binding/const added to one must not collide in the combined module.

### Safety & correctness
- **Out-of-bounds storage indexing.** Indexing `wall_rects[tex]` / `sector_light[sector]` / `*_translation[tex]` with an id outside that array's namespace (a flat id into the wall table, `NO_TEX` = `u32::MAX`) is OOB. Default bounds-checking clamps the read (silent wrong value) and a write is UB. Index only inside the branch that owns that namespace; don't read a wall table for a flat fragment.
- **Per-kind translation tables.** Walls and flats are separate atlases/rect/translation arrays — never cross-index.
- **Uninitialised vars.** WGSL gives no safe default; initialise on declaration.
- **`discard` divergence.** Masked-midtex / transparent-texel discards are fine, but keep the branch shape simple; avoid deep per-fragment divergence in lockstep invocations.

### Pipeline & layout (the silent corruptor)
- **Every uniform/storage struct must byte-match its Rust `#[repr(C)]` twin.** Compute offsets on both sides. std140/std430: `vec2` aligns 8, `vec3`/`vec4`/`mat4` align 16, a struct rounds up to its largest member's alignment. Pad Rust structs (`_pad: [f32; N]`) to hit the WGSL size. `SkyUniform`, `CameraUniform`, `CornerAttr`, `AtlasRect` are the live examples — a field added to one side without the other reads garbage from the tail.
- **`@group`/`@binding` indices must match the Rust bind-group-layout entry order exactly.** Adding a binding means: WGSL decl + layout entry + bind-group entry + (if dynamic) a retained buffer + `update_*` method. Miss one → validation error or wrong resource.
- **Texture view dimension matches the binding.** `texture_2d_array` needs `D2Array` in the layout and a `D2Array` view; a filterable sample needs a `Filtering` sampler.
- **`min_binding_size`** left `None` skips bind-time size validation — acceptable for runtime-sized storage arrays, but fixed-size uniforms can set it to catch future Rust/WGSL size drift.

### Readability
- **Extract math into named functions** (`sky_colour_dir`, `sample_atlas`, `light_band` mirror) rather than inlining into `fs_main`. Name vectors/scalars by role, not single letters.
- **Comment the non-obvious *why*** (a `dir.z *= 3` dome flatten, a `tan(pitch)` vertical map, a layer cap) — the GPU tweaks that read as arbitrary.

## Comments & docs

- **Flag comments narrating the technique or the diff** — `// disjoint-field borrow (no clone)`, `// removed unused field`, `// based on previous discussion`. The code shows *how*; comments justify the non-obvious *why* only. Borrow-checker mechanics are self-evident from the code.
- **One short line max** for a doc comment on a non-obvious item. No multi-paragraph rambles, no decorative dividers.
- **Self-contained.** Readable in 6 months with no chat context. "TODO ask user", "see the workflow" belongs in chat, not the file.
- **Never the word "seam".** Use the actual geometry/UV term.

## Imports, constants, types

- **File layout order, top to bottom:** `mod` → `use` (incl. re-exports) → `const`/`static` → newtypes → simple enums/structs (unit/no-impl or basic-impl) → richer structs/enums → everything else in *use order*, each item declared before it is used. Flag a newtype or `const` stranded between functions, or an item referenced above its declaration.
- **No inline multi-segment path qualifiers** outside `use` (`std::f32::consts::PI` inline → `use std::f32::consts::PI;`). Single-segment `crate::`/`super::` is fine. `#[cfg(...)]`-gated imports get the same `#[cfg]` on the `use`.
- **No `use` inside fn bodies** (except `use super::*;` in `#[cfg(test)] mod tests`).
- **No `const` inside fn bodies/closures** — module top.
- **Magic numbers** (atlas size, tic rate, scroll/scale tuning, light falloff) are named `const` at module top, not bare literals — especially when the same value is duplicated in a WGSL shader (cross-language drift risk; cross-reference it).
- **Enum ↔ int via explicit `match`, never `as`** for GPU/config mappings — `SkyMode as u32` silently breaks if a variant is reordered; `match mode { Static => 0, Dynamic => 1 }` is stable and documents the wire contract.
- **Stringly-typed finite sets** → enums (`TryFrom` for unknown-input parsing, never silent-default).
- **`pub(crate)` over `pub`** for items used only within the crate; audit `pub use` for an actual external consumer. A `pub` BSP3D field only read inside `build.rs` should be `pub(crate)`.
- **`_`-prefixed unused params: remove them.** A `_pad` *struct field* for GPU alignment is legitimate (bytemuck needs it); an unused *parameter* is not.

## Tests & git hygiene

- **Tests in the same file** (`#[cfg(test)] mod tests`); validate mock WAD/parser data through the real loader, not hand-built structs.
- **WAD-dependent tests are `#[ignore]`/feature-gated** with the WAD named in the header (`wad-doom`, `wad-sunder`); E1M1/shareware paths run in CI.
- **Every commit leaves the repo green** — `cargo build` (dev only, never `--release`) + `cargo fmt` + `cargo clippy` clean. The pre-commit hook runs the suite + demo regression.
- **`just lint` is the source of truth, not `cargo clippy -p <crate>`.** It runs clippy with `-D warnings` over the workspace *including test modules* (so `iter_over_hash_type` on a `for … in &hashmap`, etc., fail the build even in `#[cfg(test)]`) and the custom `lint-editor-patterns` ripgrep checks. A crate-scoped `cargo clippy` can pass while `just lint` fails; run `just lint` before declaring done. The `lint-editor-patterns` inline-`crate::` check excludes `use`/`pub use` lines and doc-comment intra-doc links (`///`/`//!`) — a multi-segment `crate::a::B` in a doc link or a `pub use` is idiomatic, not a code-site qualifier; only flag inline qualifiers in executable code.
- **Conventional prefix** — `feat(scope):`, `fix(scope):`, `refactor(scope):`. Terse body, bullets, no session/forward refs.
- **Fold fixes into the commit that introduced them** (`git commit --amend` / `rebase -i --autosquash`); no stacked `fixup` commits. When fixes scatter across many commits with heavy file overlap, one `refactor: review cleanups` commit on HEAD is the pragmatic alternative.
- **Stage by path, never `git add -A/.`** — `git status` first.

## Concurrency

The codebase is sync-threaded — no async runtime, no `.await`, no `async_lock`. The sound backend owns a dedicated thread fed by a channel (`SoundAction`); the editor offloads BSP export/launch to a one-shot `std::thread` and bridges results back over `std::sync::mpsc`. New concurrency should follow that channel-bridge shape, not introduce an executor.

- **Verify channel send/recv errors are handled** — a silent `.ok()`/`let _ =` on a `Sender::send` drops the message; at minimum `log::warn!` (the sound protocol and editor `JobOutcome` sends are the live channels).
- **Flag a `!Send` value (`Rc`, `RefCell`, Slint `Weak`'s borrowed state) captured into a spawned thread.** The worker must take an owned snapshot; cross back to the UI with `upgrade_in_event_loop`, never by sharing the `Rc`.
- **Flag blocking/long work on a thread that must stay responsive** — heavy CPU (BSP build) or sync I/O on the Slint event loop freezes the UI. Move it to the worker thread (see `jobs.rs`).
- **Flag held-across-statement lock guards on sync mutexes.** `let lock = m.lock(); let n = lock.foo().bar;` holds the lock for the rest of the scope. Prefer a single chained expression: `let n = m.lock()…foo().bar;`. Multi-line bodies that need the guard longer should `drop(lock);` at the earliest safe point. Same judgement for a long-lived `RefCell` `borrow_mut()` held across a re-entrant call.

## Slint architecture

The editor's `.slint` lives in `tools/editor/ui/`. Globals are split: typed `*Controller` globals (`foundation/globals.slint`) that Rust populates with `in` properties and listens to via callbacks, and the `Theme` global (`foundation/theme.slint`) that re-exports the active-theme colours from `ThemeController`.

- **Flag business logic in `.slint` files.** Conditionals must be presentational only (`visible: x > 0`); decisions about *what to show* or *what to do* belong in Rust.
- **Flag shadow state** — Rust structs mirroring Slint model contents back into Rust. Slint retains properties; set once, let Slint hold them. (`SharedState` holds the editor model; controller `in` properties are the view projection of it, not a second copy.)
- **Flag direct Slint-to-Slint state sharing.** Components read from the `*Controller` globals Rust populates, not from each other.
- **Flag callbacks treated as commands.** Slint fires events; Rust decides. A controller callback (`apply`, `picked`) reports an intent; the Slint side must not block waiting for the resulting `in`-property update.
- **Recognise gestures and key shortcuts in Slint; forward semantic actions, not raw events.** `canvas.slint` classifies click / drag / pan / zoom (from `TouchArea` state + the active tool) and maps key shortcuts in the `FocusScope`, forwarding named callbacks (`tool-click`, `tool-drag-start/drag/end`, `pan`, `zoom-at`, `undo`/`redo`/`delete-selection`/…). Flag a raw `pointer(kind,button,…)` forwarder or a stringly-typed `key(text)` that re-derives the gesture in Rust — that ternary/`match text` belongs in Slint. The split: Slint decides anything needing only input state (which button, is-it-a-drag, which modifier); Rust keeps anything needing the map (hit-test, snap, mutate) or the authoritative `ViewTransform` (it drives tile rasterization, so it cannot fork to Slint — Slint forwards pan/zoom *deltas*, Rust owns the transform). `TouchArea.moved` fires on any sub-pixel movement, so a drag needs an explicit threshold (`abs(mouse - pressed-x) > drag-threshold`) or every click registers as a micro-drag.
- **`clicked` fires on BOTH presses of a double-click.** Verified in Slint core (`input_items.rs`): every left release fires `clicked`; `double-clicked` fires *additionally* on the second. So a click handler that toggles selection will fire twice during a double-click. Flag any single-click action that conflicts with the double-click action on the same element (e.g. click-deselects while double-click-opens) — see the selection-model rule below.
- **Plain click replaces selection; toggle-deselect lives on a modifier.** Follow the desktop convention (Finder / Explorer / GTK): a plain click on an already-selected item keeps it selected (replace/re-anchor), and Shift/Ctrl/Cmd+click toggles it into/out of the multi-select group. This is not just convention — it dissolves the click/double-click conflict at the source (a double-click's first click can only re-select its target, never deselect it), so no single-vs-double timer deferral is needed. Flag a plain-click handler that deselects an already-selected item.
- **UI drives UI; round-trips to Rust are data requests only.** A button or menu that opens or closes a popup toggles the `*-visible` property *in Slint* (`PrefsController.prefs-visible = true`); it must not call a Rust callback whose job is to set visibility. Rust callbacks are for data — populate a model, apply an edit, handle a pick — never to drive the UI back to itself. Where Rust must *react* to a show/hide (fill on open, revert a live-theme preview on close), it observes the visibility via a `changed *-visible` watcher mirrored into the window root (the `on_property_changed` pattern, like the OS-scheme bridge at `editor.slint` `changed os-dark`); a global can't host the `changed` handler, so mirror it: `property <bool> prefs-open: PrefsController.prefs-visible; changed prefs-open => { ... }`. Rust never writes `*-visible`. **Exception:** a popup whose *show decision* is Rust-computed (the map-list picker appears only when a WAD scan finds >1 map) may have Rust set `*-visible` true on open; its close still belongs to Slint. **Name callbacks for what they do** — `populate-texture-browser`, not `browse`; a generic verb that needs the Rust body read to understand is a smell.
- **Flag hardcoded colors, font sizes, radii, spacing.** Use the local `Theme` global (`Theme.text`, `Theme.pad`, `Theme.toolbutton-size`). New chrome colours go through `ThemeController` so both light/dark resolve; canvas raster colours stay in Rust (`render/style.rs`), not Slint.
- **Flag Rust handling `slint::Image` for display decisions.** Rust builds the pixel data (tiles, texture previews, browser thumbs) and pushes `image`/`TileData`/`GfxEntry`; Slint only places it. Don't push display *logic* (which image to show) into Slint via raw `Image` swaps it shouldn't decide.
- **Flag x/y positioning** where layouts would do. Overlays and modals are the exception — absolute positioning is inherent there (hover preview cards, `PopupScaffold` centring).
- **Flag `visible: false` without zeroing width/height.** The element still occupies layout space.
- **Flag reading a repeated item's laid-out `y`/`height` from a `changed`/`init` handler that writes a layout-affecting property** (e.g. `viewport-y`). It panics at runtime with "Recursion detected": the geometry read forces a layout pass that re-fires the repeater/handler. Scroll-into-view must be a **pure binding** of inputs (`viewport-y: <expr of active-index, columns, row-height>`) with a calculated offset (uniform row height), never `bring-into-view(self.y, …)` from a handler. Also derive grid `columns` from the element's own `width`, not `visible-width` (the scrollbar toggling `visible-width` rebuilds the rows and can feed the loop).

## Slint-Rust boundary

The editor is single-threaded on the Slint event loop: state is `Rc<RefCell<SharedState>>` (defined in `state.rs`), with callbacks set up in `main.rs` and the per-area `set_callbacks_*` / `init` fns. Top-level editors live under `level_editor/` and `texture_editor/`; shared glue (`menu.rs`, `project.rs`, `render/sync.rs`) sits at `src/`. The only off-loop work is the export/launch worker (`jobs.rs`).

- **Verify the call site is on the Slint event-loop thread before `Weak::upgrade()`.** Inside `on_*` callbacks you are on the loop — `weak.upgrade()` is correct. From a spawned worker thread it returns `None` silently; post back with `weak.upgrade_in_event_loop(...)` instead (whose closure must be `Send`, so it cannot carry the `Rc` — bridge data over the `mpsc` channel, as `jobs.rs` does).
- **Flag worker threads that touch `SharedState` or a `Weak` directly.** `Rc`/`RefCell` are `!Send`; the worker gets a plain snapshot (`bincode` bytes) + the `Sender` + the `Weak`, runs the build, sends a `JobOutcome`, then pings `ExportController.job-done`. `start_export` / `start_launch` in `jobs.rs` are the reference pattern.
- **Flag `.await` / async `lock()` inside `on_*` callbacks.** The Slint thread has no async runtime, and this project has none repo-wide. Two correct patterns:
  - **Instant work:** do it inline in the callback on the borrowed `SharedState` — the common case here.
  - **Non-instant work** (the BSP build): spawn a `std::thread`, snapshot the data it needs first, post results back over the channel and `upgrade_in_event_loop` — never block the loop.
- **A `slint::Timer` must not be armed while a `SharedState` borrow is live.** `shared.borrow().timer.start(closure)` holds the borrow across `start()`, and if a previously-armed shot fires during registration its closure's `borrow_mut()` panics. Every editor timer's tic closure borrows `shared`, so they all live in module `thread_local`s reached without a borrow: `HOVER_TIMER` (`level_editor/preview.rs`), `LIGHT_TIMER` (`render/sync.rs`), `CAM_TIMER` (`views/view_canvas.rs`), `ANIM_TIMER` (`bsp_anim.rs`). **Also exempt:** the macOS app-menu poll/watch timers (`macos/menu.rs` `MENU_STATE`/`WATCH_TIMER`) — app-lifetime native-menu plumbing built *before* `SharedState` exists, bridging an ObjC C-fn callback with no `SharedState` access. A new `static`/`thread_local` Slint `Timer` is acceptable for the same reason (its closure needs `shared`); flag one only if it could instead be an `Rc`-owned field armed without a live borrow.

## Slint focus and popups

Dialogs follow a single-window rule: every dialog is a boolean-visibility overlay layer, never a second `slint::Window`. `widgets/popup.slint` (`PopupScaffold`) is the shared scaffold — backdrop dismiss, centred panel that swallows clicks/scroll, hidden `FocusScope` for shortcut routing.

- **Flag element `id`s declared inside `if` blocks.** Slint can't reference them from outside. Use a `visible:` toggle on an always-present element instead.
- **Flag `FocusScope` placed inside a `VerticalLayout` / `HorizontalLayout`** instead of as a direct child of the owning `Rectangle`. Focus silently breaks.
- **Verify new overlays reuse `PopupScaffold`** rather than re-rolling a backdrop + centred panel, and that the dismiss path is wired: the `cancelled =>` handler (and any Close button) set `*-visible = false` *in Slint* per the UI-drives-UI rule, backdrop `TouchArea` dismiss, panel `TouchArea` swallowing clicks/scroll so gestures don't fall through to the map.
- **Flag `parent.width` / `parent.height` inside an overlay's centred panel** when the intent is the panel size — `parent` there is the full-screen backdrop. Bind to the panel's own width/height (e.g. `PopupScaffold`'s `max-popup-width` / margin clamp).

## Slint style

- **Flag fluff comments in `.slint`** (`// Spacer`, section headers describing what the next block already shows). Use named widgets: `spacer := Rectangle {}`.
- **Wholly-UI callback bodies in Slint are fine.** Only flag callbacks doing logic that needs Rust input or affects Rust-owned state.
- **Verify shared controls live in `widgets/`** (`buttons.slint`, `combo.slint`, `inputs.slint`, `popup.slint`) — a fourth hand-rolled button/field/combo means it belongs there, not re-inlined per panel.
- **Verify `*Controller` globals are scoped to one concern** (canvas, status, one inspector panel, one dialog) — not catch-all globals accreting unrelated state.

## Threading (Slint-relevant)

- **Flag work spawned on a thread that then needs the UI.** A worker is for the heavy compute only; UI mutation goes back through `upgrade_in_event_loop`. A worker that holds a `Weak` and calls `upgrade()` (not `upgrade_in_event_loop`) silently no-ops.
- **Flag periodic/animation work driven by anything but `slint::Timer`.** The BSP build animation uses an `Rc`-owned `slint::Timer` ticked on the UI loop — a `std::thread` sleep-loop poking the UI is the anti-pattern.
- **Verify the worker→UI channel is drained once per ping, not polled in a busy loop.** `ExportController.job-done` fires once per posted `JobOutcome`; the handler `try_recv`s and stops.
- **Flag a re-entrant job start while one is in flight.** `job_busy` guards re-entry; a new long task must check it before spawning.

## Duplication

- **Search for similar fns before approving a new one** — grep the obvious names. Re-implementing an existing parser, callback wrapper, or enum is a frequent failure mode.
- **Three near-identical lines is fine. A premature helper is not** — but a fourth duplicate means it's time to extract.

## Sector-build / geom-kernel (`level/src/bsp3d` is unrelated — this is `tools/geom-kernel`)

The editor's sector tracer (`sector_build.rs`) re-derives which `(line, side)` faces which sector after an edit. It is pure geometry; "drawing" is just the editor command that calls `derive_sectors`/`build_sectors` — never put draw/UI concepts (cursor, snap, tool) in the kernel, and never duplicate kernel geometry editor-side (`ARCHITECTURE.md` rule).

- **`affected` vs `newly_created` are not the same set.** `build_sectors(map, affected, newly_created, …)` only *writes* `newly_created` lines; every other `affected` line is **frozen** — read while tracing, never rewritten. Passing `affected == newly_created` (as the draw path once did) lets a split fragment of an existing wall get re-sectored and unify adjacent rooms. The move path's `newly = affected.filter(line_at_crossing)` is the reference: only genuinely-changed lines are writable.
- **`split_line_at` copies `front`/`back` onto BOTH halves.** So a fragment of a sectored wall carries that sector; a freshly `add_edge`'d line carries `front.sector = None`. "Has a sector on some side" is the reliable "pre-existing wall vs brand-new edge" discriminator — not line index (split tails are appended past `base` too).
- **Subdivide vs bridge is decided by the count of *distinct* sectors a traced loop borders.** A loop bordering **one** existing sector subdivides it (reuse/copy that sector, rewrite its walls — room-divide). A loop bordering **two+** bridges separate rooms (fresh sector, and do NOT overwrite either room's walls — corridor between two boxes). Collapsing this to a boolean or to "first frozen sector found" merges distinct rooms. `bordering_sectors` returns the distinct set; `len()==1` → reuse, `len()>1` → bridge.
- **The tracer crosses two-sided walls freely.** `trace_outline` takes smallest-angle turns and will weave room A → corridor → room B as ONE outline (pinned by `trace_shared_wall_is_one_outline`). So "the loop I traced" can span multiple rooms; the bordering-sector count, not the raw trace, tells you whether to merge.
- **The draw path leaves coincident duplicate linedefs; `build_sectors` must dedup.** Drawing an edge onto an existing wall (`add_edge` does not dedup, unlike the move path) yields two lines with identical endpoints. `build_sectors` calls `dedup_coincident_lines` at the end so the genuine two-sided divider survives and the redundant twin is dropped. A "3 sectors but two stacked linedefs, delete-one-leaves-the-other" symptom is this.
- **Build two-loop / multi-sector test fixtures through the real builder, not by hand.** Hand-assigning `front.sector` with the wrong winding (CCW box with front facing outward) makes `sector_at` return `None` and silently breaks the test. Run `build_sectors` then remap sector indices to model a merge.
