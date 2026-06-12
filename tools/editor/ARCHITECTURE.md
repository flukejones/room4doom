# Editor architecture

Map editor. Four crates, bottom-up. Each layer depends only on those below it.

## geom-kernel

Pure CAD-style geometry on `EditorMap` (the map document: vertices, lines,
sidedefs, sectors, things). serde-only dep. NO UI, undo, selection, view, or I/O.

- `model` — `EditorMap`, `LineDef`, `SideDef`, `Vertex`, `Sector`, `Thing`, flags.
  Inherent doc-maintenance methods (`find_or_add_vertex`, `remove_lines`,
  `prune_orphan_vertices`, `prune_unused_sectors`).
- `name8` — `Name8` 8-byte lump name + `NameError`.
- `geom` — point/segment math, intersection, split, weld, dedup, ring winding,
  `sector_at`, `is_front_side`, snap candidates.
- `sector_build` — directed-edge sector tracing (`build_sectors`, `trace_sector`).
- `ops` — higher-level pure ops: `weld_cluster`, `move_vertices`, `add_edge`,
  `flip_lines`, `merge_sectors`, fragment copy/paste. Take world-space tolerances;
  never a View. Return result structs describing what changed.

Rule: every geometric computation lives here. If a primitive is missing, add it
here — the editor never does geometry math inline, never duplicates kernel code.

## doomed-parser

DoomEd ASCII formats. Deps geom-kernel + serde.

- `cursor` — shared `Cursor<'a, E: CursorError>` (whitespace-skipping fscanf-style
  tokenizer; the error type is fixed once per parser).
- `dwd` — `.dwd` map import → `EditorMap`.
- `dsp` — `.dsp`/`.dpr` project defs (ThingDef/SpecialDef/AnimDef/TextureDef) +
  read/write.

## editor-core

Data-layer facade. Deps geom-kernel + doomed-parser + rbsp + wad + ron.
I/O (native RON maps, WAD import/export, BSP nodes via rbsp), project files,
validation. Re-exports the kernel + parser surface so the binary imports one crate.

## editor (binary)

Slint UI + the command layer. Deps editor-core.

- `level_editor/` (`LevelEditorState`) — the command layer: selection, undo,
  view transform, tool modes, clipboard. Methods gather selection → record undo →
  call a kernel op → return a `Damage`. No geometry math.
- `render/` — the wgpu canvas: CPU geometry build (`frame`/`frame3d`), the GPU
  renderer (`wgpu` + `shaders/`), the camera (`camera3d`/`editor_camera`), and the
  push-driven `Damage` dispatch (`sync`). See `DAMAGE.md` for the damage model.
- `views/` — Slint callback wiring; handlers stay thin
  (`borrow_mut → app.method → after_edit`).
- `boundary.rs` — canonical Rust enums ↔ generated Slint mirrors (`From`/`TryFrom`).
  The wire stays typed; no magic-number ints cross the boundary.

## Layering rules

- Kernel ops are pure; tolerances arrive in world units, pixel→world conversion and
  grid snap stay editor-side.
- Editor commands orchestrate undo/selection/Damage around kernel ops.
- Slint handlers do no model logic beyond the borrow → command → refresh trio.
- Transient visuals (rubber band, previews, hover rings) are Slint-drawn overlays
  pushed as screen-space `CanvasController` properties — never rastered into tiles.
