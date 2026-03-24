# Cool Things Tried

Archived rendering and visibility systems that were explored, benchmarked, and ultimately superseded by simpler approaches. Each was functional and correct but replaced by the unified `render_bsp` path with Hi-Z node culling.

See `docs/pvs-performance.md` for full benchmark history.

## seg_occluder.rs — BSP Solidsegs Occlusion

Port of Doom's 2D horizontal screen-column occlusion buffer to the 3D renderer. Solid wall segments insert opaque screen-X ranges during front-to-back BSP traversal; back-child subtrees are skipped when their AABB projects entirely within occluded ranges.

**Why it was removed**: Produces visual errors when the player tilts their view (pitch), because 2D projected occlusion ranges no longer match 3D geometry. The approach is fundamentally 2.5D — it assumes a fixed vertical FOV with no pitch, which the 3D renderer doesn't have.

**Performance**: E6M6 submitted 4,279→3,028 (-29%), FPS ~748→~760. Modest improvement but the pitch artifacts made it unsuitable for production.

## span_occluder.rs — Edge-Span Rasterization

Alternative rendering pipeline that emits polygon edges during BSP traversal into an edge-span system, then processes scanlines and draws spans in a second pass. Masked walls are collected separately for post-pass rasterization against the filled depth buffer.

**Why it was removed**: The deferred architecture prevents depth-buffer-driven early-out during traversal. The depth buffer is empty during the BSP walk (filled only during `process_and_draw_spans`), so Hi-Z node culling has nothing to reject against. This makes the path heavily dependent on PVS for culling, which is itself no longer needed.

**Performance**: Comparable to the collect-sort-render path when PVS was available, but no advantage over immediate rendering with Hi-Z node culling.

## pvs/ — Precomputed Visibility Sets

Portal-flow visibility computation between subsectors. Multiple implementations were explored:

- **PVS2D** (`pvs2d.rs`): Full portal-flood with anti-penumbra frustum tightening. Parallelized with rayon. Most accurate but expensive to build.
- **PvsCluster** (`pvs_cluster.rs`): Cluster-based conservative PVS. Faster build, looser visibility. Had visibility gaps on large open maps (MAP20).
- **Mightsee** (`mightsee.rs`): Coarse angular visibility as conservative superset for PVS2D. Uses angular bucket tracking for bounded re-exploration.
- **Portal graph** (`portal.rs`): Inter-subsector portal construction with reduction stages (convex-pair merge, pass-through merge, RDP simplification, trivial-height merge).
- **Cache format** (`traits.rs`): `PvsFile` for on-disk caching of computed PVS bitsets.

**Why it was removed**: Hi-Z node culling at BSP internal nodes achieves similar or better runtime culling without any precomputation. E6M6 no-PVS with Hi-Z node cull (1,095 submitted, ~961 fps) outperforms old full-PVS (1,694 submitted, ~1,034 fps). MAP20 spawn improved from ~96 fps (no PVS, old path) to ~300+ fps (no PVS, Hi-Z node cull). PVS added only marginal benefit on top of Hi-Z node culling (E6M6: 961→975 fps).

## pvs-tool/ — PVS Inspection Tool

Standalone CLI tool for building, inspecting, and visualizing PVS data. Supported building PVS for individual maps or entire WADs, viewing portal graphs, and comparing PVS variants.
