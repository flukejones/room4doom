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
- **Conventional prefix** — `feat(scope):`, `fix(scope):`, `refactor(scope):`. Terse body, bullets, no session/forward refs.
- **Fold fixes into the commit that introduced them** (`git commit --amend` / `rebase -i --autosquash`); no stacked `fixup` commits. When fixes scatter across many commits with heavy file overlap, one `refactor: review cleanups` commit on HEAD is the pragmatic alternative.
- **Stage by path, never `git add -A/.`** — `git status` first.
