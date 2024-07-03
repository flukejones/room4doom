TODO: detail the portal/window handling

# Movement

+ p_xy_movement
  - `P_TryMove`
    + P_CheckPosition
      - R_PointInSubsector
      - P_BlockThingsIterator
      - PIT_CheckThing
      - P_BlockLinesIterator
    + P_UnsetThingPosition
    + P_SetThingPosition
      - R_PointInSubsector
    + P_PointOnLineSide (in another module)
    + P_CrossSpecialLine (in another module)
  - P_SlideMove
    + P_PathTraverse
      - P_BlockLinesIterator (in another module)
      - PIT_AddLineIntercepts (in another module)
      - PIT_AddThingIntercepts (in another module)
      - P_TraverseIntercepts (in another module)
    + PTR_SlideTraverse
    + `P_TryMove`
    + P_HitSlideLine

# Movement and collision handling

All `mobj_t` map objects, which are moveable entities spawned from map `Things` have movement and collision checks. When an entity moves it begins a chain of calls:

`P_XYMovement`, this preps a move by taking the object position and adding the object momentum, it then checks the move with `P_TryMove`.

If `P_XYMovement` fails, then if the mobj is a player a call to `P_SlideMove` is made to determine the direction to slide along the wall, and the best allowed slide. Both `P_XYMovement` and `P_SlideMove` call `P_TryMove`.

`P_TryMove` will call `P_CheckPosition` to see if a move is valid, and if so it will run special line/seg triggers if any were crossed, and check `Z` axis movement. The Z movement checks thing-to-ceiling height, and step height.

`P_CheckPosition` is the beginning of the collision detection. It sets things up to then call `PIT_CheckThing` and `PIT_CheckLine` - checking thing-to-thing collisions and thing-to-line collisions.

### P_CheckPosition

The first thing this function does is set up a `mobj` axis-aligned bounding box that is the x and y of the `mobj` radius. Note that all objects in Doom actually use an AABB for collision not a circle (AABB is much cheaper to do and enables various fast graph calculations). It then finds the subsector the `mobj` is in via descending the BSP with `R_PointInSubsector` only to set floor and ceiling Z, this is not used for collisions (the blockmap is. Later I will show how the BSP can be used for collisions).

Of note:
1. `validcount` is set to zero. Every line has its own `validcount` which is used to prevent checking the same line due to it being in multiple blockmap boxes.
2. `numspechit` is zeroed

Because the OG Doom collisions rely on the use of blockmaps to group lines the `mobj` AABB is extended by `MAXRADIUS` to ensure that blockmaps on the edge of the player are checked to enable items to be grabbed. For line checks the initial AABB is used.

Trying to use just the sector here in a modern take on this fails in some cases as groups of lines get missed, for example in E1M1 in the starting area the top left corner has a very small sector that gets missed. This can be fixed however.

### PIT_CheckLine

There are 3 fast checks made at the start of the function:

1. mobj AABB to line AABB
2. mobj AABB extents on line side (`P_BoxOnLineSide`)
3. is the line single sided?

The very first check is to see if any edge of the mobj AABB is within the lines own AABB. If false then return early (move is okay).

The second check is a fast `P_BoxOnLineSide` which checks if all points of the bbox are on the same side of the line. If true then return early (move is okay).

The rest of the function checks floor and ceiling heights, specials, and line flags, and sets the various bits for the mobj to check back in `P_CheckPosition`.

### P_BoxOnLineSide

In OG Doom this function is used to check if collided is `P_BoxOnLineSide` this does very fast checks using the line slope, for example a line that is horizontal or vertical checked against the top/bottom/left/right of AABB.

If the line is a slope then if it's positive or negative determines which box corners are used - Doom checks which side of the line each are on using `P_PointOnLineSide`. If both are same side then there is no intersection.

Doom code:

```rust
pub fn box_on_line_side(tmbox: &BBox, ld: &LineDef) -> i32 {
    let mut p1;
    let mut p2;

    match ld.slopetype {
        SlopeType::Horizontal => {
            p1 = (tmbox.top > ld.v1.y) as i32;
            p2 = (tmbox.bottom > ld.v1.y) as i32;
            if ld.delta.x < 0.0 {
                p1 ^= 1;
                p2 ^= 1;
            }
        }
        SlopeType::Vertical => {
            p1 = (tmbox.right > ld.v1.x) as i32;
            p2 = (tmbox.left > ld.v1.x) as i32;
            if ld.delta.y < 0.0 {
                p1 ^= 1;
                p2 ^= 1;
            }
        }
        SlopeType::Positive => {
            p1 = ld.point_on_side(&Vec2::new(tmbox.left, tmbox.top)) as i32;
            p2 = ld.point_on_side(&Vec2::new(tmbox.right, tmbox.bottom)) as i32;
        }
        SlopeType::Negative => {
            p1 = ld.point_on_side(&Vec2::new(tmbox.right, tmbox.top)) as i32;
            p2 = ld.point_on_side(&Vec2::new(tmbox.left, tmbox.bottom)) as i32;
        }
    }

    if p1 == p2 {
        return p1;
    }
    -1
}
```

A modern general purpose take on this function is to use a line-to-line intersection test:

```rust

pub fn line_line_intersection(
    mv1: Vec2, // bbox edge start
    mv2: Vec2, // bbox edge end
    lv1: Vec2, // line edge start
    lv2: Vec2, // line edge end
) -> bool {
    let denominator = ((mv2.x - mv1.x) * (lv2.y - lv1.y)) - ((mv2.y - mv1.y) * (lv2.x - lv1.x));
    let numerator1 = ((mv1.y - lv1.y) * (lv2.x - lv1.x)) - ((mv1.x - lv1.x) * (lv2.y - lv1.y));
    let numerator2 = ((mv1.y - lv1.y) * (mv2.x - mv1.x)) - ((mv1.x - lv1.x) * (mv2.y - mv1.y));

    if denominator == 0.0 {
        return numerator1 == 0.0 && numerator2 == 0.0
    }

    let r = numerator1 / denominator;
    let s = numerator2 / denominator;

    return (r >= 0.0 && r <= 1.0) && (s >= 0.0 && s <= 1.0);
}
```

the problem with the above is it's a lot of work, and you need to call it for each side of the bbox. So you can see how clever the use of stored line slopes really is here.

## Thing-to-line Collisions

The primary function for this is `PIT_CheckLine` which does a wide-phase check of AABB to line, then if the mobj is within the line bounding box it calls `P_BoxOnLineSide`.

Each line stores two (additional) items on level load: its AABB, and a line slope (2D XY). The slope is used to help very quickly check for AABB to line intersections as what it stores is whether the line is axis-aligned, or the slope is positive or negative.

If a line or tall step are encountered where the player is blocked, then the function `P_SlideMove` is called to check if the player can *slide* along the wall (in `P_XYMovement`).

## Movement

Movement for players is dictated but a `ticcmd`. This structure contains all the possible player actions and is also used for demo records and net-play.

As this is done at 35fps, it provides a consistent and (mostly) predictable world to play in, also meaning that the engine doesn't really need to do anything like collision penetration depth and subsequent movement of `Thing`s. It just
checks if a move can be done for the next frame, if not, it doesn't do the move *or* it does the player wall slide.

A second function is called if a player collision is detected; `P_SlideMove`. Comments above this particular function are `This is a kludgy mess`... Which is going to be the main driver of my updating this code to use a more modern approach.

The essence of sliding is to move the player along the wall, the line direction is gained from a call to `R_PointToAngle` which exploits the fact the top-down view of Doom is just a graph, then grabs a delta from the player momentum angle minus line angle, then sets the move momentum to the line direction + the initial velocity multiplied by the delta cosine to scale it.

Example code is:
```rust
pub fn line_slide_direction(
    origin: Vec2,
    momentum: Vec2,
    radius: f32,
    point1: Vec2,
    point2: Vec2,
) -> Option<Vec2> {
    let mxy = momentum.normalize() * radius;
    let move_to = origin + momentum + mxy;

    let lc = move_to - point1;
    let d = point2 - point1;
    let p = project_vec2(lc, d);

    let mxy_on_line = point1 + p;

    let lc = origin - point1;
    let p2 = project_vec2(lc, d);
    // point on line from starting point
    let origin_on_line = point1 + p2;

    if p.length() < d.length() && p.dot(d) > EPSILON {
        // line angle headng in direction we need to slide
        let mut slide_direction = (mxy_on_line - origin_on_line).normalize();
        if slide_direction.x.is_nan() || slide_direction.y.is_nan() {
            slide_direction = Vec2::default();
        }

        let mut vs_angle =
            mxy.angle_between(slide_direction).cos();
        if vs_angle.is_nan() {
            vs_angle = 0.0;
        }
        // the momentum is scaled by the angle between player direction and wall
        return Some(slide_direction * (vs_angle * momentum.length()));
    }
    None
}
```

### P_SlideMove

The actual inner workings of this are a little obtuse.

The first step is to find the leading and trailing edges of the mobj bbox. This is determined by the mobj x/y sign (direction in world).

```c
if (mo->momx > 0) {
    leadx = mo->x + mo->radius;
    trailx = mo->x - mo->radius;
} else {
    leadx = mo->x - mo->radius;
    trailx = mo->x + mo->radius;
}

if (mo->momy > 0) {
    leady = mo->y + mo->radius;
    traily = mo->y - mo->radius;
} else {
    leady = mo->y - mo->radius;
    traily = mo->y + mo->radius;
}
```

![](graphs/P_PathTraverse-trailing-leading.jpg)

Next the `bestslidefrac` is set, this is used later to scale the momentum. A little bit about the fixed-point math here:

`bestslidefrac = FRACUNIT+1;`

```c
#define FRACBITS        16
#define FRACUNIT        (1<<FRACBITS)
```

This is declaring a fixed point number. So `FRACUNIT` is 0.0. `bestslidefrac` is then initially `1.0`. Meaning that if no changes are made then out end momentum will be as it was.

```
0x800 as i32 = 2048
0x800 as f32 = 0.03125
```

The next step is the `P_PathTraverse` function. This is called with a pointer to the `PTR_SlideTraverse` function. `P_PathTraverse` itself is called 3 times with lines made from:
1. front-center leading + trailing vectors
2. left leading + trailing vectors
3. right leading + trailing vectors
Remember these were determined by the momentum direction, this diagram will better illustrate how the lines are made.

![](graphs/P_PathTraverse-call-vectors.jpg)

`P_PathTraverse` calls `PTR_SlideTraverse` with an `intercept_t` type. After the traversing finds the best (shortest) frac continue on to finding trying to move.

Next the momentum is scaled by the slide fraction.

The final part is to `P_HitSlideLine (bestslideline);` which adjust the momentum vector to travel along the best slide line.

#### P_PathTraverse (general purpose, for lines, aim, shoot, use)

At a glance, this function will walk through the blockmap on the way to the endpoint generating a list of all lines intercepted by the line between origin and end.

The highlevel overview of this function is that it calculates a y-intercept and x-intercept, it then uses these in a loop that increments each in a loop at the end of the function to find the next block in the blockmap to check. With each step in the loop `P_BlockLinesIterator` is called with `PIT_AddLineIntercepts` as the callback function.

When completed (function end) the `P_TraverseIntercepts` function is called with the function pointer from `P_SlideMove` (`PTR_SlideTraverse` for sliding) to iterate over the generated intercepts from `PIT_AddLineIntercepts`.

Returns true if the traverser function returns true for all lines.

A good exercise would be to walk the BSP from origin to endpoint, returning a list of all subsectors between, such as:

```rust
    pub use level::map_data::{MapData, IS_SSECTOR_MASK};
    pub fn find_ssect_intercepts<'a>(&mut self, map: &MapData) {
        if self.node_id & IS_SSECTOR_MASK == IS_SSECTOR_MASK {
            if !self.nodes.contains(&(self.node_id ^ IS_SSECTOR_MASK)) {
                self.nodes.push(self.node_id ^ IS_SSECTOR_MASK);
            }
            return;
        }
        let node = &map.get_nodes()[self.node_id as usize];

        // find which side the point is on
        let side1 = node.point_on_side(&self.origin);
        let side2 = node.point_on_side(&self.endpoint);
        if side1 != side2 {
            // On opposite sides of the splitting line, recurse down both sides
            // Traverse the side the origin is on first, then backside last. This
            // gives an ordered list of nodes from closest to furtherest.
            self.node_id = node.child_index[side1];
            self.find_ssect_intercepts(map);
            self.node_id = node.child_index[side2];
            self.find_ssect_intercepts(map);
        } else {
            self.node_id = node.child_index[side1];
            self.find_ssect_intercepts(map);
        }
    }
```

`P_PathTraverse` also sets up `divline_t trace` for later with:
```c
trace.x = x1;
trace.y = y1;
trace.dx = x2 - x1;
trace.dy = y2 - y1;
```

### P_BlockLinesIterator

This is a very short function that checks each line in the block that the mobj is in with a pointer to a function (for collision this is `PIT_AddLineIntercepts`).

### PIT_AddLineIntercepts

The function comment is probably the best description here.
```
// Looks for lines in the given block
// that intercept the given trace
// to add to the intercepts list.
//
// A line is crossed if its endpoints
// are on opposite sides of the trace.
// Returns true if earlyout and a solid line hit.
```

Interestingly it selects either `P_PointOnDivlineSide` or `P_PointOnLineSide` depending on the precision of the fixed-point uints.

Uses a ref to `trace` global. Checks the `trace` pos and pos+mov against one of the functions above.

Calculates the intercept fraction with `P_InterceptVector` if a line is hit then adds an intercept to the array of `intercept_t`. This struct is defined as:
```c
typedef struct
{
    fixed_t frac;       // along trace line
    boolean isaline;
    union {
    mobj_t* thing;
    line_t* line;
    }           d;
} intercept_t;
```

### PTR_SlideTraverse (check if line blocks, and if the slide frac is best)

This works on all the `intercept_t` generated above. The intercept links to the line which was contacted - from here it does a few checks to see if it blocks, if so then it calculates which intercept fraction is the best (shortest), which means that the line the intercept was generated from is the closest line.

Additionally if the frac/intercept is best, then the line is also stored as best for the slide.

### Modernising

The bulk of the slide stuff can be kept.

But, `P_PathTraverse` can have all the blockmap walking ripped out and replaced with a BSP point-to-point traversal to return a list of subsectors the point-to-point line passes through.

It is then easy enough to iterate over the above line list directly and call `PIT_AddLineIntercepts` (or `PIT_AddThingIntercepts`) to generate the intercept list. This means that both `P_BlockLinesIterator` and `P_BlockThingsIterator` can be removed, along with `validcount`.

## Updating

How I updated the code to use the BSP tree for collisions.
