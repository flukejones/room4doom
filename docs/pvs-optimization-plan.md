# PVS Optimization Plan

## Frustum Correctness Criteria

The anti-penumbra frustum-based portal flow is the foundation of PVS. The key correctness requirement:

**The frustum must progressively shrink.** At each hop, the exit portal is clipped against the current frustum. The new frustum is then built from the source portal to the clipped exit portal, replacing the previous frustum. Because the clipped exit is always equal to or smaller than the unclipped portal, the frustum can only narrow — never widen. This ensures:
- Subsector AABB visibility marking uses a frustum that accurately reflects what can be seen through the chain of portals from the source
- No false positives from an inflated or stale frustum
- The frustum clips portals and subsectors consistently at every depth

All optimizations below must preserve this property.

---

## Current State

Two changes implemented and kept:
1. **Linedef vertices** — portals use full linedef v1/v2 instead of BSP segment vertices. Correct portal sizes.
2. **Pass-through sectors** — sectors fully enclosed by portals with matching neighbor heights skip frustum clip/rebuild. **Will be removed** once Idea 1 (region merging) is implemented, as it subsumes this.

**E6M6 baseline (current code):**
- Build: 3m21s, dominated by sector 46 (201s, 22M calls) and sector 322 (167s, 17M calls)
- 47150/232806 sector vis pairs (20.3%)
- 6014 submitted, 1088 rendered, ~630 FPS

**E5M1:** 26232 vis, ~18s build (slower than before due to wider linedef portals)

The exponential blowup comes from backtracking DFS: `visited[current_sector] = false` allows sectors to be re-explored via different frustum paths.

**Already tried and rejected:**
- Simply removing backtracking (persistent visited): fast but 53% sector visibility lost
- Extremal vertex search in build_anti_penumbra: caused E6M6 to balloon, wrong planes for E1M1
- Merging different linedefs' portals: changed anti-penumbra geometry, lost visibility
- Bounding quad is known to inflate clipped polygons but alternatives were too slow or incorrect

---

## Idea 1: Portal-Enclosed Region Merging

Find connected groups of portal-enclosed sectors (no solid walls), then expand to include their boundary sectors (non-enclosed sectors that border the group). The entire region — enclosed core + boundary sectors — becomes one merged area with the union of all boundary portals that lead outside the region.

**How it works:**
1. Flood-fill from each portal-enclosed sector to find connected components of enclosed sectors (the "core")
2. Find boundary sectors: non-enclosed sectors that neighbor the core
3. Merge core + boundary into one region
4. The region's portals = union of all portals from boundary sectors that lead to sectors OUTSIDE the region
5. All internal portals (between sectors within the region) are eliminated from traversal

**During traversal:** Entering any portal into the merged region means:
- Mark all region subsectors visible (AABB-tested against current frustum)
- Directly recurse to all other external portals of the region
- Frustum passes through unchanged (core has no occluders)
- Boundary sectors' walls still exist but are handled by the AABB test, not by frustum narrowing

**Replaces:** The per-sector pass-through check (currently implemented). Remove `build_pass_through_sectors` and the `is_pass_through` branch in `flood_portal_traverse` once region merging is in place.

**Expected impact:** Dramatically reduces portal count and traversal depth. A corridor of 10 enclosed sectors with boundary rooms at each end collapses to a single hop. Many more sectors participate than simple per-sector pass-through (8/483 on E6M6).

---

## Idea 2: Intra-Region Subsector Occlusion Test

Within merged regions (Idea 1), perform a finer-grained visibility test between subsector/leaf pairs. Instead of marking all region subsectors visible if they pass the AABB frustum test, check whether each pair can actually see each other without being fully occluded by intervening subsectors/leaves within the region.

**How it works:**
- For each pair of subsectors (source, target) within the same merged region:
  - Build a swept volume between the source and target AABBs
  - For each intervening subsector/leaf that intersects this swept volume:
    - Project the intervening subsector's open space onto the swept volume's plane
    - Clip the swept volume by this projection, keeping only the overlap
  - If the swept volume is fully clipped away, the pair cannot see each other — don't mark visible

**Key insight:** This handles columns, pillars, and internal walls within large open areas. A column subsector in the middle of a room would occlude subsectors directly behind it. The portal flow frustum handles visibility through connected portals to outer areas; this test handles intra-region occlusion only.

**Expected impact:** Reduces subsector visibility within large merged regions, leading to fewer submitted polys. Particularly effective in rooms with columns, pillars, or partial walls that are fully enclosed by portals.

---

## Step 1: Correct Oriented-Plane Frustum (Fix Bounding Quad)

**This is the first step — correctness before optimization.** Implement, test, and gather baseline metrics before proceeding to any other ideas.

The current frustum is degraded by the bounding quad reconstruction in `clip_portal_to_frustum`. After Sutherland-Hodgman clipping, the result is projected onto local H/Z axes and inflated to an axis-aligned rectangle. This destroys the actual clipped shape and widens the frustum.

**The fix:** One single correct frustum built from oriented planes using actual portal edges. The only hard requirement is that portals are 4-sided (quads), but their edges can be at arbitrary angles (sloped ceilings, angled walls, etc.). The frustum start is enclosed within the source portal edges, and oriented toward the exit portal edges.

**S-H clipping and the 4-vertex constraint:**
- When no corner is clipped, S-H produces the same 4 vertices — use directly as the clipped quad
- When a corner IS clipped (producing an L-shaped 5+ vertex polygon), split into two 4-sided quads and recurse with two separate frustums, each preserving the 4-edge constraint
- This avoids the need to reduce an arbitrary polygon back to a quad (which is what caused the bounding-quad inflation problem)

**What needs to change:**
- Remove the bounding quad reconstruction from `clip_portal_to_frustum`
- When S-H clipping preserves 4 vertices: return the clipped quad directly
- When S-H clipping clips a corner (5+ vertices): split the polygon into two quads and return both, each spawning its own frustum traversal branch
- `build_anti_penumbra` builds planes from actual (potentially angled) source edges to actual target vertices
- Both subsector AABB marking and portal traversal use this single correct frustum

**After implementing:** Run regression tests and E1M1/E6M6 release benchmarks to establish the corrected baseline. All subsequent optimizations are measured against this baseline.

---

## Idea 4: Runtime Early Discard

Reduce submitted-to-rendered ratio (currently 6014:1088 = 5.5:1) with cheap per-polygon rejection before full render pipeline.

**Options:**
- **Backface culling**: skip polygons facing away from camera. Cheap dot product test.
- **Bounding sphere reject**: per-subsector bounding sphere vs camera frustum, faster than per-poly.
- **Distance culling**: skip small polygons beyond a threshold distance.

**Expected impact:** Reduces rendered poly count or avoids submitting obviously-invisible polys. No effect on build time.

---

## Priority Order

1. **Step 1 (Correct frustum)** — fix bounding quad, establish correct baseline metrics
2. **Idea 1 (Region merging)** — reduces branching factor and depth, replaces pass-through
3. **Idea 2 (Intra-region occlusion)** — narrows subsector vis within merged regions
4. **Idea 4 (Runtime discard)** — rendering optimization, independent of PVS build

---

## Verification & Metrics Tracking

After each change, run E1M1 and E6M6 and record metrics in `docs/pvs-performance.md`, keyed by which PVS features are enabled.

**Key metrics to track:**
- Build time
- Sector visibility pairs (count and %)
- Subsector visible pairs (avg, min, max per subsector)
- Submitted polys / rendered polys / FPS
- Frustum correctness (no geometry holes)
- Frustum clipping effectiveness (submitted:rendered ratio)

**Test commands:**
```bash
# Unit tests
cargo test -p gameplay -- test_e1m1 test_e1m2 test_e5m1 --nocapture

# E1M1 release benchmark
cargo build --release
target/release/room4doom -n --iwad /Users/lukejones/DOOM/doom.wad -e 1 -m 1 --preprocess-pvs

# E6M6 release benchmark
target/release/room4doom -n --iwad /Users/lukejones/DOOM/doom.wad --pwad /Users/lukejones/DOOM/sigil.wad --pwad /Users/lukejones/DOOM/sigil2.wad -e 6 -m 6 --preprocess-pvs
```

**Existing data (from docs/pvs-performance.md):**

E6M6 (483 sectors, 3820 subsectors, 3387 portals):
| Method | Build Time | Submitted | Rendered | FPS |
|---|---|---|---|---|
| v1: 2D Raycasts | 57.3s | 5,310 | 1,082 | ~484 |
| v2: Portal + 3D Raycasts | 185.1s | 5,914 | 1,091 | ~450 |
| v5: Correct frustum clip + reduce | 2.53s | 5,580 | 1,083 | ~695 |
| bounding quad + linedef verts (current) | 3m21s | 6,014 | 1,088 | ~630 |

Sunder MAP03 (954 sectors, 4338 subsectors, 5280 portals):
| Method | Build Time | Submitted | Rendered | FPS |
|---|---|---|---|---|
| v5: Correct frustum clip + reduce | 6.58s | 4,872 | 665 | ~780 |

**Progress tracking:**
| Feature Set | Map | Build Time | Sector Vis | Submitted | Rendered | FPS |
|---|---|---|---|---|---|---|
| bounding quad (current) | E6M6 | 3m21s | 47150 (20.3%) | 6014 | 1088 | ~630 |
| correct frustum (Step 1) | E6M6 | 20.7s | 46758 (20.1%) | 5865 | 1088 | ~662 |
| + region merging | E6M6 | ? | ? | ? | ? | ? |
| + intra-region occlusion | E6M6 | ? | ? | ? | ? | ? |
| ... | ... | ... | ... | ... | ... | ... |

---

## Future Exploration: Sector-Only Visibility with Memoization

Exit-portal memoization (skipping re-recursion through already-explored portals) was considered but rejected for frustum-based subsector visibility — different frustum angles produce different subsector AABB results, so memoizing misses visibility from later visits.

A simpler variant: **sector-only visibility with no subsector marking.** Each exit portal is explored at most once, marking only which sectors can see which. No frustum-based subsector clipping at all — when a sector is visible from another, there's no easy way to know which of that sector's subsectors are actually visible without the frustum. Since memoization may mark entire downstream sector trees as visible from a position that can't actually see all the way through, subsector-level precision wouldn't improve the situation much anyway.

This trades subsector precision for fast, bounded build times. All subsectors in a visible sector would be considered visible, increasing submitted polys but eliminating the exponential backtracking entirely. May be viable for maps where build time is prohibitive and the submitted:rendered ratio is acceptable.
