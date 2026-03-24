# Voxel Rendering — Texel Projection

## Overview

KVX voxel models replace matching sprites with 3D voxel representations.
Each model is pre-sliced into axis-aligned planes at load time. At render
time, occupied voxel faces are projected individually as perspective-correct
quads through the software rasterizer.

## Data Pipeline

### Load time

1. **KVX parser** (`voxel/kvx.rs`): Reads Build engine KVX binary format
   into a flat 3D grid (palette indices, 255 = empty). The trailing 768
   bytes of a KVX file are an embedded palette (256 entries × 3 bytes,
   6-bit per channel 0-63).

2. **Palette remap** (`VoxelModel::remap_to_doom_palette`): KVX palette
   indices reference the model's own 6-bit palette, not Doom's. At load
   time each KVX index is remapped to the closest Doom PLAYPAL entry:
   - Convert 6-bit channel values to 8-bit: `(v << 2) | (v >> 4)`
   - Find the Doom palette entry with minimum RGB Euclidean distance
   - Build a 256-entry remap table, apply to all grid cells
   After remapping, grid values are standard Doom palette indices and
   flow through the engine's existing colourmap/lighting pipeline.

3. **Doom palette extraction** (`d_main::load_voxels`): The first PLAYPAL
   lump is read from the WAD as 768 bytes of 8-bit RGB. Each PLAYPAL
   entry is stored in the WAD as a packed `u32` (`0x00RRGGBB`), so the
   loader unpacks R/G/B channels into a flat `Vec<u8>`.

4. **VOXELDEF parser** (`voxel/voxeldef.rs`): Maps sprite frame names to
   KVX files with optional spin/angle properties. AngleOffset receives a
   -90° adjustment (GZDoom convention: KVX front faces +X, Doom actors
   face +Y at angle 0).

5. **Slice generation** (`voxel/slices.rs`): For each of 6 face directions
   (±X, ±Y, ±Z), generates slice quads. Each slice at a given depth
   contains sparse column data encoding which voxel faces are exposed
   (adjacent to empty space) and their Doom palette indices.

   - `VoxelSliceQuad`: depth position, tight bounds, sparse columns
   - `VoxelColumn`: list of `VoxelSpan` (start row, pixel data)
   - `skip_cols`: precomputed distance to next non-empty column

6. **VoxelManager** (`voxel/mod.rs`): Stores all loaded models, maps
   `(sprite_index, frame_index)` to `VoxelSlices` for O(1) lookup.
   Models sharing the same KVX file are deduplicated.

### Render time

1. **Direction selection**: All 6 face directions are iterated. The depth
   buffer handles occlusion — back faces behind front faces are rejected.
   This avoids popping artifacts at axis-aligned viewing angles and
   correctly handles concave models.

2. **Texel projection** (`rasterizer/scanline.rs: rasterize_voxel_texels`):
   For each slice quad, walks the sparse column data directly:
   - Skip empty columns entirely
   - For each occupied texel, compute 4 world-space corners from the
     quad's origin + u_vec/v_vec basis vectors
   - Transform corners through view-projection matrix (4 mat4×vec4)
   - Rasterize the projected quad with 4-edge test and depth interpolation

   Cost is proportional to **occupied texels**, not bounding box area.
   The Spider Mastermind at 3% fill: 194 texels projected vs 12,462
   scanline pixels walked in the previous approach.

3. **Depth handling**: Per-pixel depth test with 1/w convention. Front-to-back
   sort by per-slice distance to camera. Hi-Z early rejection available.

4. **Colour pipeline at render time**: Grid values are Doom palette indices
   (post-remap). The rasterizer receives:
   - `colourmaps: &[&[usize]]` — 48-entry table mapping distance bands to
     colourmap rows (sector brightness + player extralight)
   - `palette: &[u32]` — Doom PLAYPAL as packed ARGB for final framebuffer write
   Per pixel: distance band from average inv_w → colourmap lookup →
   `palette[colourmap[doom_index]]` → framebuffer.

## File Layout

```
render/software3d/src/
  voxel/
    mod.rs          — VoxelManager, load_from_directory
    kvx.rs          — KVX binary parser
    slices.rs       — Slice-quad generation, VoxelSliceQuad/Column/Span
    voxeldef.rs     — VOXELDEF.txt parser

  rasterizer/
    scanline.rs     — rasterize_voxel_texels (shared by engine + viewer)

  scene/
    sprites.rs      — collect_voxel_slice_quads, render_voxel_slices

tools/voxel-viewer/ — Standalone viewer using same rasterizer
```

## Usage

```
room4doom -i doom.wad --voxels path/to/cheello_voxels/voxels/
```

The voxels directory contains `.kvx` files. `VOXELDEF.txt` is read from
the parent directory or the voxels directory itself.

## Viewer

```
cargo run -p voxel-viewer --release -- path/to/model.kvx [palette.pal] [--slices]
```

Controls: arrows = orbit, +/- = zoom, R = auto-rotate, S = toggle
direct/slice mode, Esc = quit.

Both direct mode (per-voxel point projection) and slice mode (texel
projection through shared rasterizer) are available for comparison.
