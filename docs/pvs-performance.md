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

### Runtime (spawn point, E6M6)

| Metric | No PVS | Old PVS | Coarse Region PVS |
|---|---|---|---|
| Submitted | 4,279 | 3,580 | 2,910 |
| Frustum-clipped | 1 | 1 | 1 |
| Culled | 79 | 78 | 57 |
| Early-depth | 2,597 | 1,926 | 1,308 |
| No-draw | 597 | 576 | 573 |
| Rendered | 1,005 | 999 | 971 |
| FPS | ~754 | ~635 | ~916 |

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

---

## Key Insights

- Correct frustum clipping was the single biggest win — fixing the v3 bug outperformed all collapse/merge strategies
- Portal graph reduction helps build time and portal mightsee computation, not directly runtime FPS
- Smaller merge thresholds leave more subsectors for finer-grained later stages, often producing better total results
- Stages must run in correct order: merges that change proxy topology must not disrupt RDP chain structure
- Runtime culling is dominated by sub-pixel rejection and Hi-Z depth buffer, not PVS precision
