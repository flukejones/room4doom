# Room4Doom — Claude Context

## Build & Test
- `cargo build --release` — full release build (~20s)
- `cargo build` — dev build with opt-level=2 (~3s)
- `cargo test -p software3d` — voxel/render tests
- `cargo test -p gameplay` — gameplay tests (needs WAD files)
- `cargo build -p voxel-viewer` — standalone voxel viewer tool
- `cargo build --release --features render_stats` — enables per-frame stats printing (stdout)
- `cargo build --release --features hprof` — enables coarse-prof timing hierarchy
- Default display backend: `display-softbuffer` (set in game-exe/Cargo.toml default features)
- `render_stats` only prints in `software3d` renderer, not `software25d` — must use `-r software3d`
- Binary name: `room4doom` (package in `game-exe/` directory)

## Crate Structure

### Core
- `wad` — WAD file parsing, no internal deps
- `math` — fixed-point, trig (glam-based)
- `level` — BSP, PVS, map geometry, BSP3D triangulation. Depends on: math, wad. Key subdirs: `src/bsp3d/` (build, carve, movers, node), `src/pvs/`
- `pic-data` — visual asset data: textures, sprites, palettes, colourmaps, voxel models. Loaded from WAD/KVX/PK3
- `gameplay` — game logic, physics, thinkers. Re-exports `level::LevelState`. Depends on: level, math, wad, sound-common
- `game-config` — configuration management

### Rendering
- `render-common` — `GameRenderer`, `DrawBuffer`, `RenderView` traits. Depends on: level, pic-data (not gameplay)
- `software25d` / `software3d` — renderer implementations. Depend on: gameplay, level, render-common
- `backend` — bridges renderers + display backends (sdl2/softbuffer/pixels). Depends on both renderers

### UI
- `gamestate-traits` — `SubsystemTrait` for UI subsystems. Depends on gameplay, render-common
- `ui/menu` — menu system
- `ui/statusbar` — HUD statusbar
- `ui/intermission` — intermission screens
- `ui/hud-messages` — HUD message display
- `ui/hud-util` — shared HUD utilities
- `ui/finale` — end-of-episode/game screens

### Infrastructure
- `gamestate` — `Game` struct, game loop orchestration
- `game-exe` — entry point (binary name: `room4doom`), wires everything together
- `input` — keyboard/mouse handling. Depends on gameplay (for TicCmd/WeaponType)
- `sound/*` — trait (`sound/traits`) + backends (sdl2, rodio, nosnd, opl2_emulator)
- `tools/` — voxel-viewer, pvs-tool, multigen, test-utils

### Key dependency: gameplay → level
`gameplay` depends on `level` and re-exports only `LevelState`. Callers access level types directly via `level::`.

## Architecture
- View matrix uses eye-at-origin (`look_to_rh(ZERO, fwd, up)`); camera translation handled by subtracting `player_pos` in render code. Using `look_at_rh(pos, ...)` will double-apply the translation.
- Voxel pipeline: `VoxelManager` (load) → `collect_visible_slices` (cull/collect) → sort front-to-back → `rasterize_voxel_texels` (render). Shared between game engine and voxel-viewer via `software3d::voxel::collect`.
- Depth buffer uses 1/w (larger = closer). Hi-Z tiles are 8×8. Voxel writes must use `set_depth_update_hiz` (not `set_depth_unchecked`) to enable front-to-back voxel-to-voxel occlusion.
- PK3 files: ZIP archives with `filter/doom.id.doom1/` and `filter/doom.id.doom2/` for game-specific overrides. Loaded via `software3d::voxel::pk3`.

## Voxel Rendering
- Double-sided slice quads: 3 axis pairs (not 6 directions). Each quad stores neg_columns and pos_columns. Renderer picks visible side via dot product.
- Clip-space linear decomposition: `clip(u,v) = clip_origin + u*clip_du + v*clip_dv` — exact, replaces per-texel mat4 multiply with vec4 adds.
- Corner caching: within a span, bottom corners of current texel = top corners of next. Halves perspective divisions for interior texels.
- KVX pivot Z often = zsiz (pivot at ground/feet, not model center). zpivot is NOT the geometric center.
- Default spin (150 tics/rotation) applied only to PICKUP_SPRITES list, not all voxels.

## Gotchas
- `pixels` crate framebuffer is RGBA byte order = `0xAABBGGRR` on little-endian when cast to u32 (not `0xAARRGGBB`). The game engine's palette is `0xAARRGGBB`.
- Edge-on cull: compare `dist > CULL_DIST` (distance), not `dist > CULL_DIST_SQ` (distance²). The `cull_edge_on` flag uses `length_squared() > CULL_DIST_SQ` which is correct.
- OPL2 `update_frequency`: frequency multiplies can overflow u32 — use `wrapping_mul`.
- Whole-slice polygon rasterization (scanline UV lookup) is SLOWER than per-texel iteration for sparse voxel data. Most of the quad area is empty; the scanline walker wastes time discovering blank texels. Per-texel approach only visits occupied data.

## Voxel Viewer (tools/voxel-viewer)
- Testing ground for the engine render pipeline — must use the same collection/rasterization code paths.
- Controls: mouse drag=rotate, scroll=zoom (or slice step in single-slice mode), arrows=pan, s=toggle mode, w=wireframe cycle, d=single-slice mode, r=auto-rotate
- `--no-vsync` for benchmarking, `--slices` to start in slice mode
