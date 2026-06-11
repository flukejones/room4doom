# Full 3D BSP slice — investigation notes (not implemented)

Recorded during the bsp3d build/store/runtime split. The question: could the
engine move from "2D BSP with per-leaf 3D polygons" to a Quake-style true 3D
BSP (splitters chosen in 3D, leaves are convex *volumes*)?

## What it would buy

- Slopes and room-over-room become representable: nothing in a 3D leaf
  assumes "exactly one floor and one ceiling at constant z".
- Cleaner front-to-back traversal in 3D (the current tree partitions only in
  XY; vertical overdraw inside a leaf is resolved by the depth buffer).
- PVS over volume leaves (Quake's model) instead of 2D subsector heuristics.

## Constraints that keep the 2D BSP alive regardless

- **Gameplay must keep the 2D BSP + blockmap.** Demo sync, `P_CheckSight`,
  hitscan, and movement all walk OG-compatible structures with OG fixed-point
  arithmetic. A 3D render tree would be an *additional* structure, not a
  replacement — exactly how the current `BSP3D` already relates to the 2D
  tree.
- **Movers vs a static tree.** Quake splits world geometry once and handles
  doors/platforms as *brush models* (separate mini-BSPs transformed at
  runtime, clipped against the world per frame). Doom sectors mutate the
  world itself: any sector can move its floor/ceiling arbitrarily within
  neighbour bounds. Options:
  - Quake-style: every mover sector becomes a "brush model" leaf-set rendered
    separately. The static tree must then treat mover volumes as open space
    (carve at min/max travel), which reintroduces the AABB-expansion problem
    the current code solves with `expand_node_aabbs_for_movers`.
  - The current scheme (vertex-z-only movement of leaf polygons) already
    handles flat horizontal movers exactly, with no per-frame re-clipping.
    A 3D tree would have to keep splitter planes away from any plane a mover
    can sweep through, or accept per-frame leaf re-fitting.
- **WAD maps are 2.5D.** Until UDMF-style slopes are ingested, a 3D slice
  produces exactly the geometry the 2D slice already produces, at higher
  build cost.

## What the v2 lump format already accommodates

The disk format deliberately assumes nothing the 3D slice would break:

- A leaf is `(poly_start, poly_count)` — any number of polygons, any
  orientation. Nothing encodes "two horizontals + walls".
- Polygons are winding-defined (normals derived at parse), so sloped flats
  serialize unchanged; only the builder and the flat/wall classification in
  the engine's resolve step assume ±Z flats today.
- Nodes are *not* serialized — the engine derives its traversal tree at
  parse. Swapping the 2D-derived `Node3D` tree for true 3D nodes is a new
  parse-side derivation plus one new lump section; existing sections keep
  their meaning.

## If/when implemented

1. New `NODES3D` section: plane (Vec4) + children, leaf indices into the
   existing `LEAVES3D`.
2. Builder grows a 3D splitter chooser (axial-plane heuristic first: sector
   floor/ceiling planes are the only candidates in 2.5D maps).
3. Mover sectors excluded from splitter candidates (their travel range forms
   an open slab in the tree, as the AABB expansion does today).
4. Renderers swap `Node3D::point_on_side` (XY) for a plane dot — the leaf
   walk, surface cache, and event tables are unchanged.
