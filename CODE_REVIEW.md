# Code Review Guidelines — room4doom

What to look for when reviewing a commit. Each item is a check, not an assertion.

Items already enforced by clippy / rustc or by a kb-hook lint are not repeated here — trust the tools for those; review covers what they cannot. `kb-hook audit` lists what the lints find across the tree, and each refusal carries the rule, so an item removed from this file is not a rule dropped. The rules below are the ones that bit us in practice: a Doom port has fixed-point math, a shared BSP3D fed to two renderers (CPU `software3d` + GPU `wgpu3d`), per-frame hot paths, and WGSL/Rust layout coupling that compiles cleanly and fails (or silently corrupts) only at runtime.

Project conventions live in `CLAUDE.md` and `~/.claude/rules/rust.md`; this file is the review lens, not a restatement.

**Audit documented contracts against call sites.** A rule this file states as enforced is a claim, not a fact — this file once said the draw path's `affected == newly_created` bug was fixed while the code still shipped it. When a review touches an area with a written contract, grep the call sites and verify.

## Module structure

- **Look for single-file folders** — `foo/mod.rs` with no siblings. Should be flat `foo.rs`.
- **Check for mutual sibling imports** — if `a.rs` and `b.rs` import each other, the boundary is wrong.
- **Look for shape fragmentation** — three `impl Foo` blocks in three files should be one file.
- **Watch for the inverse: 1000-LOC files mixing 3+ concerns** — WAD I/O + geometry + render state in one file is the same anti-pattern.
- **Verify `mod.rs`/`lib.rs` stays small** — re-exports and the cross-cutting type, not a dumping ground.
- **Look for two-level dispatch** — a folder split by category over an enum, where each file is a second match on the same enum. One match should suffice.
- **Verify a function lives in the module that uses it** — used by only one other module → it probably belongs there.

## Naming

- **Verify variable names describe role, not type** — renaming on type change is noise.
- **Flag stringly-typed values for finite sets** — render modes, tool kinds, sky modes, thing classes. Enums, with `TryFrom` for WAD-parsed input; never silent-default.
- **Check for module-name collisions** — the same module name reachable via two `use` paths. Rename the more deeply-nested one.
- **Verify map keys cover the full identity** — keying a texture by name when identity is `(name, kind)` silently collides walls with flats.

## Public API

- **Audit every `pub use`** — confirm a caller reaches it. An unused re-export blocks dead-code detection and misleads about the surface.
- **Prefer `pub(crate)` over `pub`** for items used only inside the crate — a `pub` BSP3D field read only inside `build.rs` should be `pub(crate)`.
- **Flag a field made `pub` to let one caller read one value** — an accessor keeps the invariant.

## Type system and ownership

- **Flag `Option<&T>` returns** that could be `&Option<T>`, or the reverse — the wrong shape forces awkward call sites.
- **Flag two `Option` fields only valid together**, where one branch silently ignores the other. Collapse into an enum.
- **Flag a wrapper struct carrying parallel data** — change the base type or define a richer local type.
- **Check trait bounds on generics** — bounds live at the impl/fn that needs them, not duplicated everywhere.
- **Flag `'static` bounds added without justification** — usually an ownership workaround.
- **Check for `Arc<Mutex<T>>` where `&mut T` would suffice** — shared ownership for single-threaded state is overkill.

## One source of truth

- **Flag a value reachable by more than one field or path.** Two homes means a search finds several sites and none is authoritative, with no signal if they disagree. Pick the canonical location and delete the duplicates.
- **A derived struct must not carry its own `Default`** of the values it derives from.

## Error handling

- **Flag `unwrap_or_default()` on fallible ops** — a transient failure becomes a plausible-looking default with no log.
- **Flag bare `.unwrap()`** on runtime-fallible ops (locks, channels, I/O, WAD lookups). `expect("reason")` or handling; `.unwrap()` only on compile-time-provable invariants.
- **Verify error variants carry context** — an error without a detail field is unhelpful at the call site.

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

## Editor reconcile & dirty sets (`tools/editor/src/render/sync.rs`)

The reconciler diffs the map against `last_synced` and patches only derived dirty sets (`DAMAGE.md`). Its failure modes are omissions, not crashes — stale pixels that heal on the next unrelated edit and so evade tests.

- **Derived GPU data re-emits with the dirty set of its INPUTS, not its own record diff.** A line instance is built from vertex positions and sector heights, so it follows `rewall` (vert-touching ∪ height-border ∪ record-changed), never `lines_changed` alone. When adding a derived layer, write down what it reads, then check each input's change reaches it. The "lines with normals don't move until I click a sector" bug was exactly this.
- **Unconditional per-edit work is a finding.** Anything running in `reconcile`/`apply_reconcile` without a gate costs every height-spin tick on a Sunder-scale map. Gate on plan booleans (`flats_changed`/`wall_tex_changed`/`thing_kinds_changed`), a tracked last-* value (`last_grid_z`, `last_fill`, `last_asset_gen`), or a cheap length check — the atlas key hash, `thing_leaves`, and `covers` were all once unconditional full-map passes.
- **A gate's inputs must cover everything the gated work reads.** `content_key` hashes flats + wall names + thing kinds + asset generations, so the gate tracks exactly those; `ensure_thing_sprites` also reads project thing-icon overrides, which nothing tracks — a documented blind spot until a UI mutates them. When gated work grows a new input, grow the gate.
- **Rewriting a conditional data path: enumerate the old branches and prove each survives.** The old code passed `vert_z` only in wireframe fill; the rewrite passed it always and silently moved vertex markers off the grid plane in Colour/Texture. Diff branch conditions, not just data shapes.
- **Ops never classify what they touched.** The reconciler diffs map, selection, highlight, grid plane, and fill itself. Flag an op that reports its own damage sets or a call site that pre-computes "what changed" — that knowledge has one owner.

## WGSL shaders

Strictly typed and memory-safe, but logical and layout traps remain. Shaders are built by `concat!`-ing `sky_common.wgsl` ahead of `scene.wgsl`/`sky.wgsl` — a binding/const added to one must not collide in the combined module.

### Safety & correctness
- **Out-of-bounds storage indexing.** Indexing `wall_rects[tex]` / `sector_light[sector]` / `*_translation[tex]` with an id outside that array's namespace (a flat id into the wall table, `NO_TEX` = `u32::MAX`) is OOB. Default bounds-checking clamps the read (silent wrong value) and a write is UB. Index only inside the branch that owns that namespace; don't read a wall table for a flat fragment.
- **Per-kind translation tables.** Walls and flats are separate atlases/rect/translation arrays — never cross-index.
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

- **One short line max** for a doc comment on a non-obvious item. No multi-paragraph rambles, no decorative dividers.
- **Self-contained.** Readable in 6 months with no chat context. "TODO ask user", "see the workflow" belongs in chat, not the file.

## Imports, constants, types

- **File layout order, top to bottom:** `mod` → `use` (incl. re-exports) → `const`/`static` → newtypes → simple enums/structs (unit/no-impl or basic-impl) → richer structs/enums → everything else in *use order*, each item declared before it is used. Flag a newtype or `const` stranded between functions, or an item referenced above its declaration.
- **Magic numbers** (atlas size, tic rate, scroll/scale tuning, light falloff) are named `const` at module top, not bare literals — especially when the same value is duplicated in a WGSL shader (cross-language drift risk; cross-reference it).
- **Scalar constants shared across a language boundary have ONE source.** A comment saying "must match X in Y" is not a contract — it drifts silently. WGSL: substitute the Rust values over defaults declared in the shader file (the editor's `surface_wgsl` rewrites `SURFACE_*`, fill thresholds, sentinels in place — prepending instead leaves the file broken for wgsl-analyzer). Native UI: reuse feature types and `EditorTab`; do not repeat integer mappings in menus and views.
- **Enum ↔ int via explicit `match`, never `as`** for GPU/config mappings — `SkyMode as u32` silently breaks if a variant is reordered; `match mode { Static => 0, Dynamic => 1 }` is stable and documents the wire contract.
- **Stringly-typed finite sets** → enums (`TryFrom` for unknown-input parsing, never silent-default).
- **`pub(crate)` over `pub`** for items used only within the crate; audit `pub use` for an actual external consumer. A `pub` BSP3D field only read inside `build.rs` should be `pub(crate)`.
- **`_`-prefixed unused params: remove them.** A `_pad` *struct field* for GPU alignment is legitimate (bytemuck needs it); an unused *parameter* is not.

## Tests & git hygiene

- **Tests in the same file** (`#[cfg(test)] mod tests`); validate mock WAD/parser data through the real loader, not hand-built structs.
- **A WAD-dependent test names the WAD it needs** — `wad-doom`, `wad-sunder`. `clippy::ignore_without_reason` enforces that `#[ignore]` carries a reason; which WAD, and whether it is feature-gated rather than ignored, it cannot know.
- **Every commit leaves the repo green** — `cargo build` (dev only, never `--release`) + `cargo fmt` + `cargo clippy` clean. The pre-commit hook runs the suite + demo regression.
- **`just lint` is the source of truth, not `cargo clippy -p <crate>`.** It runs clippy with `-D warnings` over the workspace *including test modules* (so `iter_over_hash_type` on a `for … in &hashmap`, etc., fail the build even in `#[cfg(test)]`) and the custom `lint-editor-patterns` ripgrep checks. A crate-scoped `cargo clippy` can pass while `just lint` fails; run `just lint` before declaring done. The `lint-editor-patterns` inline-`crate::` check excludes `use` lines of any visibility (`pub`, `pub(crate)`, `pub(super)`) and doc-comment intra-doc links (`///`/`//!`) — a multi-segment `crate::a::B` in a doc link or a re-export is idiomatic, not a code-site qualifier; only flag inline qualifiers in executable code.
- **No hash-order-dependent outcomes.** The `iter_over_hash_type` lint catches `for` loops, but `.reduce`/min/max ties over a `HashSet`, hash-ordered seed selection, and `HashSet`-driven write order escape it — tie-break by lowest key (the `inner_edge` trace start was a run-to-run flake). Sort sets into Vecs before order matters.
- **Audit the test delta by name, not count.** Diff test-fn names against the baseline: renames are conversions, the remainder is new. Judge each new test by the pruning rules (no tests for one-line gates; fold a micro-test whose invariant a sibling already proves; a duplicated test body wants a shared helper; the 4th copy of a fixture builder wants a `#[cfg(test)]` fixtures module).
- **Conventional prefix** — `feat(scope):`, `fix(scope):`, `refactor(scope):`. Terse body, bullets, no session/forward refs.
- **Fold fixes into the commit that introduced them** (`git commit --amend` / `rebase -i --autosquash`); no stacked `fixup` commits. When fixes scatter across many commits with heavy file overlap, one `refactor: review cleanups` commit on HEAD is the pragmatic alternative.
- **Stage by path, never `git add -A/.`** — `git status` first.

## Concurrency

The engine uses synchronous worker threads and channels. The sound backend owns a dedicated thread fed by `SoundAction`; the editor offloads BSP/export/launch to `std::thread` workers and awaits their `async_channel` results on GPUI's foreground executor. Reuse these paths rather than adding another runtime.

- **Flag a `!Send` value (`Rc`, `RefCell`, native UI handles) captured into a spawned thread.** The worker must take an owned snapshot; return owned outcomes through the channel, never by sharing the `Rc`.
- **Flag blocking/long work on a thread that must stay responsive** — heavy CPU (BSP build) or sync I/O on the GPUI foreground thread freezes the UI. Move it to the worker thread (see `jobs.rs`).
- **Flag held-across-statement lock guards on sync mutexes.** `let lock = m.lock(); let n = lock.foo().bar;` holds the lock for the rest of the scope. Prefer a single chained expression: `let n = m.lock()…foo().bar;`. Multi-line bodies that need the guard longer should `drop(lock);` at the earliest safe point. Same judgement for a long-lived `RefCell` `borrow_mut()` held across a re-entrant call.

## GPUI architecture

The editor's presentation is grouped by feature under `tools/editor/src/tabs/` and `dialogs/`. Each tab and the dialog host is a persistent GPUI entity. `MapEditor` owns plain `MapPresentation`; `DialogsView` owns plain `DialogDrafts`. Their commands share the same domain path with headless callers. `app/window.rs` composes entities and routes typed requests; `app/session.rs` owns project/resources and the editing session. `EditorAppearance` contains shell chrome only. Pure editing values live in `level_editor/state.rs`; GPU caches live in `render/cache.rs`.

- **Keep business logic in the domain.** GPUI views classify input gestures and call typed feature actions; hit-testing, snap, geometry and undo stay in the editing model. Rendering reconciliation stays below the native presentation adapter. Cross-feature requests use GPUI events handled by the shell.
- **Flag shadow document state.** Feature rows and fields are a presentation projection, not a second editable map. Widget-local state is for focus, text selection, layout and transient gestures. Do not reintroduce a global property/callback framework.
- **Recognise gestures once.** Native canvas input owns click/drag/pan/orbit/zoom classification and an explicit drag threshold. Forward semantic actions and deltas to domain callbacks; do not duplicate camera transforms or map decisions in a view.
- **Plain click replaces selection; modifier click toggles.** A double-click's first click keeps its target selected, so opening an editor never fights a plain-click deselection.
- **One gated call site means every sibling must gate.** If browser population or a show decision can fail, check every sibling opener for the same success guard.
- **Dialog lifecycle is explicit.** Opening populates the feature's draft once; closing reverts unapplied live edits. Use direct feature methods and the shared modal host, not callback registration or duplicated population at the button.
- **Reuse standard Zed widgets and icons.** Shared text input and theme integration live in `ui-common`. Use the active theme for chrome colours and standard widget sizing; reserve absolute positions for overlays, hover previews and movable popup frames.
- **Flag layout feedback loops.** A canvas size probe should notify only when its size or scale changes. Scroll-to-selection uses the existing scroll handle and measured list geometry, not a render-time mutation loop.
- **Retire replaced images.** GPUI image caches must evict superseded and hidden previews, including the renderer's cached image resources, rather than accumulating a new image on each edit or zoom.
- **Use one native menu action path.** macOS gets native app menus and a themed transparent titlebar; avoid a duplicated in-window menu bar.

## GPUI focus and popups

Dialogs remain overlays in one editor window. The shared popup composition provides drag/resize, Escape-to-cancel, input swallowing and persisted `PopupId` geometry.

- **Flush property fields before commands change their owner.** Save, quit, map/selection changes and document actions must commit the focused field before operating. Blur must not later apply that old field to a new selection. Action fields create only on explicit submission.
- **Text input owns text actions.** Clipboard, select-all, undo/redo, IME and character editing go to the focused input first; map and texture shortcuts must not also mutate the document.
- **Guard every quit path.** Cmd+Q, the app-global fallback and the window close button all honour the unsaved-change guard, including when no element has focus.
- **Unhandled keys propagate.** Popup and canvas handlers consume only the shortcuts they implement; platform/menu accelerators must remain reachable.
- **Cancel pointer gestures on deactivation.** A mouse release outside the window or a focus change must not leave map, texture or popup drags latched.
- **Reuse the shared popup frame and geometry slots.** A new popup needs its own `PopupId` and preference slot; keep its bounds clamped to the window.

## GPUI threading and scheduling

The editor's `Rc<RefCell<EditorSession>>` and native UI handles remain on GPUI's foreground thread.

- **Workers receive snapshots and a sender.** BSP/export/launch jobs must not capture `Rc`, `RefCell`, a native window or feature handles. Await `JobOutcome` delivery through `async_channel` on the foreground executor and apply it there.
- **Release borrows before handlers, modal dialogs and awaits.** Never hold `EditorSession` or a GPUI entity borrow across a native modal loop or suspended future. The shell dispatches application commands on its foreground task; native path prompts use GPUI futures. Extract input registries before flushing them: a blur listener can update its owning entity. Flush focused fields before dispatching. Reject asynchronous results whose resource or document generation no longer matches.
- **Schedule deadlines, never busy-poll.** Retain or detach GPUI tasks intentionally. Timers wake the earliest due animation/preview task and stop when idle; channel delivery awaits the receiver.
- **Keep deadlines local to their owner.** `MapEditor` owns hover, camera, light and BSP deadlines; the shell schedules the earliest wake outside the borrowed editing session. Do not add global timer or callback registries.
- **Guard job re-entry.** Check `job_busy` before starting another long job; report channel send failures.

## Editor state lifecycle

The editor's document state (undo, caches, missing resources and render buffers)
lives in `EditorSession`/`LevelEditorState`; maps are switched in place. Native
views own their presentation and edit drafts.

- **Generational keys have NO cross-map discriminant.** Fresh maps re-issue slot 0/generation 0, so a key or baked slot index from map A can resolve against map B's arenas. Any cache holding keys or slot-indexed mirrors (`surface_mesh`, `bvh`, `edge_lines`, clipboard, drafts) must be cleared on map switch even when a synchronous rebuild "self-heals" — the gap is one skipped rebuild wide.
- **`reset_map` clears EVERY map-scoped field.** A field added to `EditorSession`
  that indexes into the map (an edit draft, a selection, a cache keyed by
  line/sector id) must be cleared on map switch, and any popup holding such a
  draft closed by the shell on document replacement — a stale draft applied after a switch
  writes the old map's data onto the new map's indices. When reviewing a new
  `EditorSession` field, ask: what happens to it on map switch? Resource and document generations must also invalidate native field histories and cached feature state when identities repeat.
- **Composite edits record exactly ONE undo step.** Public ops that record
  their own snapshot (`apply_sector`, `add_sector_at`) must not be composed
  directly — a composite (paste) snapshots once up front and calls the
  non-recording internals (`set_sector`, `add_sector`), discarding the record
  on a no-op. Two records for one user action = a half-revert on first Ctrl-Z.
- **Exports validate, never coerce.** `export_inner` rejects what the WAD
  format cannot express (`UnenclosedSide` for a `sector: None` side) instead of
  silently substituting a default (sector 0 = corrupt map). A new export field
  gets a range/validity check, not an `unwrap_or`.

## User data & I/O

- **User-supplied paths load fallibly.** A registered WAD/project path can be
  moved or deleted between sessions; opening it must log + skip/fail, never
  panic (`WadData::try_new`/`try_add_file`, not `new`/`add_file`). A panicking
  constructor is only acceptable on embedded/fixture data.
- **User-data writes are temp + rename.** Exported WADs/PNGs, map RON, project
  manifest/data files, prefs: write `<path>.tmp` then `fs::rename` — a crash
  mid-write must not destroy the previous file. In-place `fs::write` is only
  for files that are regenerated wholesale with no prior value worth keeping.
- **Blocking work off the event loop.** Anything that can take seconds on a
  large input (PNG encode of a 16k map image, BSP build) runs on the worker
  via the `app/jobs.rs` pattern behind `job_busy`. GPU submission/readback stays on
  the loop (the context lives there) — split the CPU half out. And never hold a
  `EditorSession` borrow across a modal `rfd` dialog.

## Duplication

- **Search for similar fns before approving a new one** — grep the obvious names. Re-implementing an existing parser, callback wrapper, or enum is a frequent failure mode.
- **Three near-identical lines is fine. A premature helper is not** — but a fourth duplicate means it's time to extract.

## Sector-build / geom-kernel (`level/src/bsp3d` is unrelated — this is `tools/geom-kernel`)

The editor's sector tracer (`sector_build.rs`) re-derives which `(line, side)` faces which sector after an edit. It is pure geometry; "drawing" is just the editor command that calls `derive_sectors`/`build_sectors` — never put draw/UI concepts (cursor, snap, tool) in the kernel, and never duplicate kernel geometry editor-side (`ARCHITECTURE.md` rule).

- **A fix here is a hypothesis until a trace dump confirms the mechanism.** Instrument seeds/loops/`bordering` and dump the actual traced edges before accepting any explanation — the adjacent-room merge survived two plausible static diagnoses; only the dump showed a coincident-duplicate figure-eight trace and a probe that never fired. (General debugging rule in `CLAUDE.md`; it earns restating here because this module invites confident wrong reasoning.)
- **`newly_created` is the ONLY writable set.** `build_sectors(map, newly_created, …)` writes sectors onto `newly_created` lines alone; every other line in the map is **frozen** — read while tracing, never rewritten. (The old two-set API's `affected` parameter was only ever read intersected with `newly` and was dropped; the historical `affected == newly_created` bug — a split fragment of an existing wall re-sectored, unifying adjacent rooms — is now unexpressible.) A caller's job is deriving `newly` correctly: the move path takes `line_at_crossing` hits plus the sector-less flood, the weld path floods sector-less lines from the weld point, the draw path filters the shared-vertex expansion to lines with no sector on either side.
- **`VoidRule` picks the void policy per op.** Draws pass `SectorDrawnLoops` (a drawn closed loop always sectors; a bare frozen wall it closes against goes two-sided); moves/welds pass `KeepPockets` (authored void pockets survive — the pinch fixture pins this). A new `build_sectors` caller must choose deliberately.
- **`build_sectors` dedups coincident twins BEFORE tracing.** Two lines between the same vertices make the angular trace weave both twins into one figure-eight loop (the adjacent-room merge). Never reorder the dedup after `correct_sectors`.
- **`split_line_at` copies `front`/`back` onto BOTH halves.** So a fragment of a sectored wall carries that sector; a freshly `add_edge`'d line carries `front.sector = None`. "Has a sector on some side" is the reliable "pre-existing wall vs brand-new edge" discriminator — not line index (split tails are appended past `base` too).
- **Subdivide vs bridge is decided by the count of *distinct* sectors a traced loop borders.** A loop bordering **one** existing sector subdivides it (reuse/copy that sector, rewrite its walls — room-divide). A loop bordering **two+** bridges separate rooms (fresh sector, and do NOT overwrite either room's walls — corridor between two boxes). Collapsing this to a boolean or to "first frozen sector found" merges distinct rooms. `bordering_sectors` returns the distinct set; `len()==1` → reuse, `len()>1` → bridge.
- **The tracer crosses two-sided walls freely.** `trace_outline` takes smallest-angle turns and will weave room A → corridor → room B as ONE outline (pinned by `trace_shared_wall_is_one_outline`). So "the loop I traced" can span multiple rooms; the bordering-sector count, not the raw trace, tells you whether to merge.
- **The draw path leaves coincident duplicate linedefs; `build_sectors` must dedup.** Drawing an edge onto an existing wall (`add_edge` does not dedup, unlike the move path) yields two lines with identical endpoints. `build_sectors` calls `dedup_coincident_lines` at the end so the genuine two-sided divider survives and the redundant twin is dropped. A "3 sectors but two stacked linedefs, delete-one-leaves-the-other" symptom is this.
- **Build two-loop / multi-sector test fixtures through the real builder, not by hand.** Hand-assigning `front.sector` with the wrong winding (CCW box with front facing outward) makes `sector_at` return `None` and silently breaks the test. Run `build_sectors` then remap sector indices to model a merge.
