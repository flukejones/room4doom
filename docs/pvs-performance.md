# PVS Performance

## Current Pipeline

Portal graph reduction stages, in order:

1. **Convex-pair merge** (`reduce_portal_graph`) — merge intra-sector subsector pairs that form convex shapes
2. **Pass-through merge** (Stage A) — merge tiny/sliver single-neighbor non-mover sectors entirely into neighbor (area < 1000 OR fill ratio < 0.05)
3. **Enclosed subsector merge** (Stage B) — merge individual subsectors of remaining single-neighbor sectors into sole adjacent neighbor subsector
4. **RDP boundary simplification** — Douglas-Peucker chain simplification of zigzag portal boundaries (epsilon 32)
5. **Trivial-height merge** (Stage C) — merge single-neighbor non-mover sectors with `|floor_delta| + |ceil_delta| <= 32` into neighbor proxy
6. **Internal fragmentation collapse** — iteratively merge internal-only subsectors of affected enclosing sectors into adjacent border subsectors

Stages 5-6 run last to avoid disrupting RDP chain structure.

## Current Results (all stages)

### Portal Reduction

| Map | Raw Portals | After All Stages | Reduction |
|---|---|---|---|
| E1M1 | 850 | 350 | 59% |
| E5M1 (Sigil) | 9,038 | 4,014 | 56% |
| E6M6 (Sigil 2) | 19,058 | 5,718 | 70% |

### Runtime — n-poly, PVS comparison (no sky polys, no solidsegs, spawn point)

Walls as quads, flats as n-gons, sky polygons skipped. MAP20 pvs2d and mightsee stats are distance-limited — level draw not complete at this view position.

| Metric | MAP03 no PVS | MAP03 mightsee | MAP03 full PVS | MAP20 no PVS | MAP20 mightsee | MAP20 full PVS |
|---|---|---|---|---|---|---|
| Submitted | 6,534 | 3,782 | 1,235 | 27,666 | 18,825 | 6,289 |
| Culled | 313 | 176 | 40 | 7,717 | 4,473 | 906 |
| Early-depth | 5,704 | 3,089 | 680 | 17,924 | 12,328 | 4,060 |
| No-draw | 210 | 210 | 208 | 1,807 | 1,806 | 1,145 |
| Rendered | 307 | 307 | 307 | 218 | 218 | 178 |
| Subsectors | 2,648/2,648 | 1,624/2,648 | 518/2,648 | 18,116/18,116 | 12,785/18,116 | 5,152/18,116 |
| FPS | ~377 | ~670 | ~1,024 | ~96 | ~123 | ~265 |

| Metric | E6M6 no PVS | E6M6 mightsee | E6M6 full PVS |
|---|---|---|---|
| Submitted | 2,432 | 2,182 | 1,694 |
| Culled | 48 | 48 | 34 |
| Early-depth | 582 | 560 | 471 |
| No-draw | 373 | 368 | 365 |
| Rendered | 578 | 578 | 578 |
| Subsectors | 1,692/1,692 | 1,547/1,692 | 1,197/1,692 |
| FPS | ~1,050 | ~953 | ~1,034 |

### Runtime — pvsmightsee triangulated (no solidsegs, spawn point, historical)

Triangle-fan tessellation with sky polys. BSP solidsegs disabled. PvsCluster excluded from MAP20: visibility gaps on large open maps.

| Metric | MAP03 pvsmightsee | E6M6 pvsmightsee | E6M6 PvsCluster | MAP20 pvsmightsee |
|---|---|---|---|---|
| Submitted | 4,558 | 4,311 | 3,610 | 16,130 |
| Frustum-clipped | 3 | 1 | 1 | 3 |
| Culled | 24 | 56 | 42 | 222 |
| Early-depth | 3,291 | 2,585 | 1,890 | 11,540 |
| No-draw | 569 | 588 | 615 | 3,367 |
| Rendered | 671 | 1,081 | 1,062 | 998 |
| FPS | ~840–843 | ~776–786 | ~842 | ~120–121 |

---

## Stage Tuning Data

### Stages A+B: Pass-Through + Enclosed Subsector

Constants: `PASSTHROUGH_MAX_AREA`, `PASSTHROUGH_MAX_FILL_RATIO`. Sector qualifies if area < max OR fill ratio < max.

| Area / Fill | E1M1 Stage A | E1M1 Stage B | E1M1 Portals | E6M6 Stage A | E6M6 Stage B | E6M6 Portals |
|---|---|---|---|---|---|---|
| 500 / 0.03 | 2 | 8 | 384 | 43 | 29 | 5878 |
| **1000 / 0.05** | **2** | **8** | **384** | **50** | **25** | **5860** |
| 2000 / 0.08 | 2 | 8 | 384 | 60 | 20 | 5946 |
| 4000 / 0.15 | 3 | 7 | 384 | 60 | 20 | 5946 |
| 8000 / 0.30 | 4 | 7 | 376 | 56 | 23 | 6070 |

Portal counts are after A+B+RDP only (before Stage C). **1000/0.05 selected**: best E6M6, same E1M1 as all small-threshold variants. Smaller thresholds leave more subsectors for Stage B's finer-grained merging.

### Stage C: Trivial-Height Merge + Internal Collapse

Constant: `TRIVIAL_HEIGHT_DELTA` (combined `|floor_delta| + |ceil_delta|`). Runs after RDP.

| Delta | E1M1 Stage C | E1M1 Collapse | E1M1 Portals | E6M6 Stage C | E6M6 Collapse | E6M6 Portals | E5M1 Stage C | E5M1 Collapse | E5M1 Portals |
|---|---|---|---|---|---|---|---|---|---|
| 8 | 3 | 0 | 368 | 10 | 9 | 5738 | 2 | 1 | 4058 |
| 16 | 3 | 0 | 368 | 10 | 9 | 5738 | 2 | 1 | 4050 |
| 24 | 4 | 1 | 350 | 11 | 13 | 5718 | 3 | 1 | 4040 |
| **32** | **4** | **1** | **350** | **11** | **13** | **5718** | **5** | **2** | **4014** |

**32 selected**: matches 24 on E1M1/E6M6, best E5M1. One Doom step = 24 units, so 32 catches slightly more than one step of combined delta.

---

## Historical Performance Data

### Architecture

#### Tier 1: Sector-Level Portal Flow (parallel)

- Each source sector floods through portals, anti-penumbra frustum of 4 separating planes tightens at each hop
- Parallelized with rayon, one thread per source sector
- Subsector AABBs tested against accumulated frustum at each visited sector

#### Previous approaches (replaced)

- **v1 — 2D segment raycasts**: Prone to visibility errors
- **v2 — Portal flow + 3D raycasts**: Accurate but slow (Moller-Trumbore, 64 rays/pair)
- **v3 — Parallel + AABB-frustum (broken)**: Incorrect frustum clipping
- **v4 — Collapse/merge strategies**: Concentrated portals on representative nodes, creating worse hotspots
- **v5 — Correct frustum clipping**: Fixed v3's frustum accumulation bug, 2.8x faster than v4 on Sunder
- **v6 — Quad split + pass-through**: Replaced bounding quad with actual clipped polygon geometry

### Sunder MAP03

954 sectors, 4338 subsectors, 5280 raw portals.

| Method | Portals | Build Time | Submitted | Rendered | FPS |
|---|---|---|---|---|---|
| No PVS | — | — | 17,241 | 679 | ~308 |
| v1: 2D Raycasts | 5280 | ~2 min | — | — | — |
| v2: Portal + 3D Raycasts | 5280 | 5m 20s | 4,010 | 664 | ~620 |
| v4: AABB merge | 817 | 18.3s | 7,077 | 669 | — |
| v5: Correct frustum | 5280 | 6.6s | 4,872 | 665 | ~780 |
| v6: Quad split (depth 10) | — | 7.1s | 4,547 | 665 | ~823 |
| Sub-pixel rejection | — | — | 7,312 | 679 | ~596 |
| **Coarse region PVS** | **—** | **—** | **2,207** | **617** | **~854–1044** |
| **PvsCluster** | **—** | **—** | **2,859** | **656** | **~1061** |
| **pvsmightsee** | **—** | **—** | **4,558** | **671** | **~840–843** |

> Note: PvsCluster on MAP03 has some missing polygons. pvsmightsee measured without BSP solidsegs (solidsegs causes visual errors with view tilt).

### E6M6 (Sigil 2)

483 sectors, 3820 subsectors, 19058 raw portals.

| Method | Portals Reduced | Submitted | Rendered | FPS |
|---|---|---|---|---|
| No PVS | — | 7,534 | 1,099 | ~400 |
| v1: 2D | — | 5,310 | 1,082 | ~484 |
| v5: Frustum clip | — | 5,580 | 1,083 | ~695 |
| v6: Quad split | — | 5,865 | 1,088 | ~653 |
| Sub-pixel rejection | — | 4,444 | 1,099 | ~755 |
| Old PVS (all stages) | 5,718 | 3,580 | 999 | ~635 |
| **Coarse region PVS** | **—** | **2,910** | **971** | **~916** |
| **PvsCluster** | **—** | **3,610** | **1,062** | **~842** |
| **pvsmightsee** | **—** | **4,311** | **1,081** | **~776–786** |
| **n-poly, no PVS** | **—** | **2,432** | **578** | **~1050** |
| n-poly + sky, no PVS | — | 2,960 | 582 | ~900 |

> Note: All entries above "n-poly" used triangle-fan tessellation and included sky polygons (floor/ceiling surfaces in sky sectors). The n-poly rows use walls as quads and flats as n-gons, without PVS. The first n-poly row skips sky polygons; the "+ sky" row includes them.

### E1M1

88 sectors, 239 subsectors, 850 raw portals.

| Method | Portals Reduced | Submitted | Rendered |
|---|---|---|---|
| v2: Portal + Raycasts | — | 501 | 128 |
| v6: Quad split | — | 305 | 128 |
| **Current (all stages)** | **350** | — | — |

---

## Runtime Optimization Notes

### Hi-Z Tiled Depth Buffer (8x8) — KEPT

Conservative hierarchical depth buffer. Polygon rejected only when ALL overlapping tiles fully covered AND polygon behind farthest first-write. Zero false rejections.

| Tile Size | Early-depth | No-draw | Rendered | FPS |
|---|---|---|---|---|
| No Hi-Z | 0 | 2593 | 1088 | ~678 |
| **8x8** | **2006** | **587** | **1088** | **~729** |
| 16x16 | 1797 | 796 | 1088 | ~704 |

### Sub-Pixel Rejection — KEPT

Pre-submission screen-space area estimate. Reject triangles under 1 pixel. Conservative (skipped when any vertex behind near plane).

- Sunder MAP20 (no PVS): 103,895 → 21,689 submitted (-79%), FPS ~47 → ~108
- E6M6 (with PVS): 6,876 → 4,444 submitted, FPS ~618 → ~755

### Coarse Depth Rejection (5-point) — REJECTED

Too aggressive. Visible polygons incorrectly rejected, causing holes.

### BSP Solidsegs Occlusion — DISABLED

2D horizontal occlusion buffer. Produces visual errors when the player tilts their view (pitch), as the 2D projected occlusion ranges no longer match the 3D geometry. Kept in codebase for reference; not used in current performance baseline.

Horizontal screen-column occlusion buffer (1D solidsegs) ported from 2.5D renderer. Solid wall segs insert opaque screen-X ranges during BSP front-to-back traversal; back-child subtrees are skipped when their AABB projects entirely within occluded ranges. Subsectors whose wall segs don't project on-screen fall back to AABB-based `is_bbox_visible` check (R_CheckBBox port).

Features: pitch pullback (`sin(pitch) * 128` units backward) for tilted views, 1-column float margin for precision.

Uses computed 3D AABBs (`bsp3d.get_node_aabb()`) for back-child frustum checks, not 2D WAD bboxes.

#### E6M6 (spawn point)

| Metric | No PVS, No Occlusion | BSP Solidsegs Occlusion |
|---|---|---|
| Submitted | 4,279 | 3,028 |
| Frustum-clipped | 1 | 1 |
| Culled | 79 | 62 |
| Early-depth | 2,602 | 1,446 |
| No-draw | 596 | 533 |
| Rendered | 1,001 | 986 |
| Fallback | — | 563 |
| FPS | ~748 | ~760 |

#### Sunder MAP03

| Metric | No Occlusion (PVS only) | BSP Solidsegs Occlusion |
|---|---|---|
| Submitted | 2,207 | 1,615 |
| Frustum-clipped | — | 1 |
| Culled | — | 43 |
| Early-depth | — | 704 |
| No-draw | — | 264 |
| Rendered | 617 | 603 |
| Fallback | — | 191 |
| FPS | ~854–1044 | ~845 |

#### Sunder MAP20

| Metric | No Occlusion (PVS only) | BSP Solidsegs Occlusion |
|---|---|---|
| Submitted | — | 5,194 |
| Frustum-clipped | — | 5 |
| Culled | — | 163 |
| Early-depth | — | 1,867 |
| No-draw | — | 2,205 |
| Rendered | — | 954 |
| Fallback | — | 2,723 |
| FPS | — | ~139 |

#### Sunder MAP20 — PvsCluster (no solidsegs occlusion)

| Metric | PvsCluster | pvsmightsee |
|---|---|---|
| Submitted | 7,205 | 16,130 |
| Frustum-clipped | — | 3 |
| Culled | — | 222 |
| Early-depth | 4,214 | 11,540 |
| No-draw | — | 3,367 |
| Rendered | 831 | 998 |
| FPS | ~237 | ~120–121 |

> PvsCluster excluded from MAP20 current baseline: visibility gaps on large open maps make it unsuitable despite better FPS.

### Allocation & Trig Optimization

Per-frame Vec pre-sizing (visible_polygons, visible_sectors, seen_sectors, sprite_quads), cached `fov_half_tan`, precomputed `seg_angle_rad` on OcclusionSeg, and `all_solid()` early-exit in mid-leaf seg loop.

#### E6M6 (spawn point, BSP solidsegs occlusion, no PVS)

| Metric | Before | After |
|---|---|---|
| Submitted | 3,028 | 3,028 |
| Rendered | 986 | 986 |
| FPS | ~584–647 | ~748–760 |

Poly counts unchanged — pure CPU-side optimization. ~18% FPS improvement.

**Note:** E6M6 submitted drops from 4,279 to 3,028 (-29%) with solidsegs occlusion. MAP03 drops from 2,207 to 1,615 (-27%). MAP20's high fallback count (2,723) indicates many subsectors require the AABB fallback path. BSP solidsegs occlusion works without PVS, making it viable for maps that can't be PVS'd (large open areas with detail).

---

## Key Insights

- Correct frustum clipping was the single biggest win — fixing the v3 bug outperformed all collapse/merge strategies
- Portal graph reduction helps build time and portal mightsee computation, not directly runtime FPS
- Smaller merge thresholds leave more subsectors for finer-grained later stages, often producing better total results
- Stages must run in correct order: merges that change proxy topology must not disrupt RDP chain structure
- Runtime culling is dominated by sub-pixel rejection and Hi-Z depth buffer, not PVS precision
